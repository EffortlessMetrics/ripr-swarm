//! RIPR-SPEC-0048: decide a changed `const`/`static` by evaluating it.
//!
//! `const_readers` relates the tests that call a same-file reader of the
//! constant; that only corrects reach. This module takes the next step for the
//! one shape it can decide exactly: a reader that is one expression over its
//! parameters and the constant, called by tests whose assertions compare the
//! call to literals. It evaluates each assertion with the constant's old
//! initializer and again with the new one. An assertion that holds before and
//! fails after notices the change; when every related assertion holds both
//! ways, no related test can.
//!
//! Nothing here approximates. A control-flow reader, a lookup, a binding,
//! a non-literal expected value, an unreadable assertion or an assertion that
//! does not hold on the old value (the test is stale, or misread) ends in
//! `None`, and the caller keeps `static_unknown`.

mod assoc;
mod expr;

pub(in crate::analysis) use self::assoc::associated_const_binding;
use std::collections::BTreeMap;

use self::expr::{
    Assertion, Machine, Reader, Value, parse_assertion, parse_const_decl, parse_reader,
    parse_trait_reader,
};
use super::related_tests::normalize_path;
use crate::analysis::diff::ChangedFile;
use crate::analysis::facts::{FunctionContainer, FunctionSummary, TestSummary};

/// Related tests evaluated per constant. Past it the flow is unread rather
/// than sampled.
const TEST_LIMIT: usize = 8;

/// The old and new declaration lines of one changed constant, by file and
/// new-side line.
#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct ChangedConsts {
    by_line: BTreeMap<(String, usize), ConstChange>,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct ConstChange {
    old_declaration: String,
    new_declaration: String,
}

impl ChangedConsts {
    /// Pair each added single-line constant declaration with the removed
    /// declaration of the same name in the same file. A name that is removed
    /// or added more than once in a file is ambiguous and pairs with nothing.
    pub(in crate::analysis) fn from_changed_files<'a>(
        files: impl IntoIterator<Item = &'a ChangedFile>,
    ) -> Self {
        let mut by_line = BTreeMap::new();
        for file in files {
            let declared = |lines: &[crate::analysis::diff::ChangedLine]| {
                let mut seen: BTreeMap<String, Vec<usize>> = BTreeMap::new();
                for (at, line) in lines.iter().enumerate() {
                    if let Some(decl) = parse_const_decl(&line.text) {
                        seen.entry(decl.name).or_default().push(at);
                    }
                }
                seen
            };
            let removed = declared(&file.removed_lines);
            let added = declared(&file.added_lines);
            for (name, added_at) in &added {
                let (&[added_at], Some(&[removed_at])) =
                    (added_at.as_slice(), removed.get(name).map(Vec::as_slice))
                else {
                    continue;
                };
                let (Some(new), Some(old)) = (
                    file.added_lines.get(added_at),
                    file.removed_lines.get(removed_at),
                ) else {
                    continue;
                };
                by_line.insert(
                    (normalize_path(&file.path), new.line),
                    ConstChange {
                        old_declaration: old.text.clone(),
                        new_declaration: new.text.clone(),
                    },
                );
            }
        }
        Self { by_line }
    }

    pub(in crate::analysis) fn lookup(
        &self,
        file: &std::path::Path,
        line: usize,
    ) -> Option<&ConstChange> {
        self.by_line.get(&(normalize_path(file), line))
    }
}

/// #11400: whether the paired declarations name one constant of one type
/// whose evaluated value differs. A visibility, spelling or type-only edit
/// (`10` to `5 + 5`, `const` to `pub const`) or a value the evaluator cannot
/// read is not a changed value.
pub(in crate::analysis) fn const_value_changes(change: &ConstChange) -> bool {
    let (Some(old), Some(new)) = (
        parse_const_decl(change.old_declaration.trim()),
        parse_const_decl(change.new_declaration.trim()),
    ) else {
        return false;
    };
    if old.name != new.name || old.ty != new.ty {
        return false;
    }
    match (
        expr::evaluate_initializer(&old.init, old.ty),
        expr::evaluate_initializer(&new.init, new.ty),
    ) {
        (Some(before), Some(after)) => before != after,
        _ => false,
    }
}

/// Whether the declaration at `declaration_line` and every function starting
/// at one of `function_lines` sit directly in the same module of `source`
/// (no enclosing block but `mod name { .. }`, the same chain of modules).
/// Spelling cannot tell an imported or sibling-module constant from the
/// edited one, so a reader in another module, or a declaration inside a
/// block, is not evaluated.
pub(in crate::analysis) fn shares_module_scope(
    source: &str,
    declaration_line: usize,
    function_lines: &[usize],
) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(source);
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut scopes: BTreeMap<usize, Vec<Option<String>>> = BTreeMap::new();
    for (at, line) in masked.lines().enumerate() {
        let number = at + 1;
        scopes.insert(number, stack.clone());
        let mut words = line.split_whitespace().peekable();
        let mut module: Option<String> = None;
        while let Some(word) = words.next() {
            if word == "mod"
                && let Some(name) = words.peek()
            {
                module = Some(name.trim_end_matches('{').to_string());
            }
        }
        for ch in line.chars() {
            match ch {
                '{' => stack.push(module.take()),
                '}' => {
                    stack.pop();
                }
                _ => {}
            }
        }
    }
    let Some(declared) = scopes.get(&declaration_line) else {
        return false;
    };
    if declared.iter().any(Option::is_none) {
        return false;
    }
    function_lines
        .iter()
        .all(|line| scopes.get(line) == Some(declared))
}

/// What evaluating the related tests under the old and new value showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum ConstFlow {
    /// A related assertion holds on the old value and fails on the new.
    Discriminated(ConstFlowDetail),
    /// Every related assertion holds on both values.
    Indistinguishable(ConstFlowDetail),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct ConstFlowDetail {
    pub constant: String,
    pub old_value: String,
    pub new_value: String,
    pub reader: String,
    /// The test and assertion the summary cites: the first to fail on the
    /// new value, else the first evaluated.
    pub test: String,
    pub assertion: String,
    pub old_result: String,
    /// The reader's return under `new_result_at`.
    pub new_result: String,
    /// The constant value that produced `new_result`.
    pub new_result_at: String,
    pub tests_evaluated: usize,
    /// The edit kept the value; `new_value` lists the neighbours tried.
    pub rewrite: bool,
    /// For a rewrite: each neighbour with the assertion that first broke on
    /// it. The credit needs one per neighbour, so all are cited.
    pub neighbour_breaks: Vec<String>,
}

/// Evaluate the flow, or `None` when any part of it is unreadable.
///
/// `functions_named` counts the functions of the constant's file with a
/// name: a reader whose name another function of the file shares, or that
/// sits in an `impl`, may not be the function the test's call reaches.
///
/// `assoc_type` is the self type of the `impl Trait for Type` that declares an
/// associated constant (see `associated_const_binding`): its readers are trait
/// default methods, and every call a test makes to one must name that type.
pub(in crate::analysis) fn evaluate_const_flow(
    change: &ConstChange,
    readers: &[&FunctionSummary],
    tests: &[&TestSummary],
    functions_named: &dyn Fn(&str) -> usize,
    assoc_type: Option<&str>,
) -> Option<ConstFlow> {
    let old = parse_const_decl(&change.old_declaration)?;
    let new = parse_const_decl(&change.new_declaration)?;
    if old.name != new.name || old.ty != new.ty {
        return None;
    }
    let old_value = expr::evaluate_initializer(&old.init, old.ty)?;
    let new_value = expr::evaluate_initializer(&new.init, new.ty)?;
    if readers.is_empty() || tests.is_empty() || tests.len() > TEST_LIMIT {
        return None;
    }
    // A changed value is compared with the old one. An edit that keeps the
    // value (`10` to `5 + 5`) has no old/new pair, so it is asked the
    // mutation question instead: would a test notice the value being one
    // off, either way?
    let rewrite = old_value == new_value;
    let alternatives = if rewrite {
        neighbours(&old_value, old.ty)?
    } else {
        vec![new_value.clone()]
    };
    let mut parsed: Vec<Reader> = Vec::new();
    for reader in readers {
        if functions_named(&reader.name) != 1 {
            return None;
        }
        if assoc_type.is_some() {
            if !matches!(reader.item.container, FunctionContainer::Trait { .. })
                || !reader.item.has_body
                || reader.item.has_self_param
            {
                return None;
            }
            parsed.push(parse_trait_reader(&reader.body)?);
        } else {
            if !matches!(
                reader.impl_context,
                crate::analysis::facts::FunctionImplContext::Free
            ) {
                return None;
            }
            parsed.push(parse_reader(&reader.body)?);
        }
    }
    let machine = |constant_value| Machine {
        readers: &parsed,
        constant: &old.name,
        constant_ty: old.ty,
        constant_value,
    };
    let before = machine(&old_value);
    let afters: Vec<Machine<'_>> = alternatives.iter().map(machine).collect();
    let first_after = afters.first()?;
    let explain = Explain {
        constant: &old.name,
        old_value: &old_value,
        new_value: alternatives
            .iter()
            .map(Value::render)
            .collect::<Vec<_>>()
            .join(" and "),
        before: &before,
    };
    let reader_names: Vec<&str> = parsed.iter().map(|reader| reader.name.as_str()).collect();

    let mut first_seen: Option<ConstFlowDetail> = None;
    let mut failing: Option<ConstFlowDetail> = None;
    let mut broke: Vec<Option<ConstFlowDetail>> = vec![None; afters.len()];
    for test in tests {
        let mut evaluated = 0usize;
        let mut calls_evaluated = 0usize;
        for oracle in &test.assertions {
            if !reader_names
                .iter()
                .any(|name| mentions_call(&oracle.text, name))
            {
                continue;
            }
            let assertion = parse_assertion(&oracle.text)?;
            if let Some(self_type) = assoc_type
                && !reader_names
                    .iter()
                    .all(|name| calls_name_the_type(&oracle.text, name, self_type))
            {
                return None;
            }
            let names = assertion.call_names();
            if !assertion.has_literal_expectation()
                || names.is_empty()
                || !names
                    .iter()
                    .all(|name| reader_names.contains(&name.as_str()))
            {
                return None;
            }
            // The test must pass today; a stale or misread assertion decides
            // nothing.
            if !before.holds(&assertion)? {
                return None;
            }
            evaluated += 1;
            calls_evaluated += names.len();
            if first_seen.is_none() {
                first_seen = Some(explain.detail(
                    test,
                    &oracle.text,
                    &assertion,
                    (first_after, &alternatives[0]),
                ));
            }
            for ((after, value), first_break) in
                afters.iter().zip(&alternatives).zip(broke.iter_mut())
            {
                if !after.holds(&assertion)? {
                    // The cited result is the one that broke the assertion,
                    // not the first value tried.
                    let detail = explain.detail(test, &oracle.text, &assertion, (after, value));
                    if failing.is_none() {
                        failing = Some(detail.clone());
                    }
                    if first_break.is_none() {
                        *first_break = Some(detail);
                    }
                }
            }
        }
        // Every call of a reader in the test body must sit in an evaluated
        // assertion; a call feeding a `let` or a helper is not read.
        let body_calls = test
            .body_calls()
            .filter(|call| reader_names.contains(&call.name.as_str()))
            .count();
        if evaluated == 0 || body_calls != calls_evaluated {
            return None;
        }
    }
    let detail = |detail: ConstFlowDetail| ConstFlowDetail {
        tests_evaluated: tests.len(),
        rewrite,
        ..detail
    };
    if rewrite {
        // Credit only when every neighbour breaks some related assertion,
        // from any of the tests; one unnoticed neighbour leaves the edit
        // undecided. Each neighbour's breaking assertion is cited.
        let breaks = broke.into_iter().collect::<Option<Vec<_>>>()?;
        let neighbour_breaks = breaks
            .iter()
            .map(|b| {
                format!(
                    "{} breaks `{}` in `{}` (`{}` returns {})",
                    b.new_result_at, b.assertion, b.test, b.reader, b.new_result
                )
            })
            .collect();
        let first = breaks.into_iter().next()?;
        return Some(ConstFlow::Discriminated(ConstFlowDetail {
            neighbour_breaks,
            ..detail(first)
        }));
    }
    match (failing, first_seen) {
        (Some(failing), _) => Some(ConstFlow::Discriminated(detail(failing))),
        (None, Some(first)) => Some(ConstFlow::Indistinguishable(detail(first))),
        (None, None) => None,
    }
}

/// The values one below and one above `value` that its type admits. `None`
/// when `value` is not an integer or neither neighbour exists.
fn neighbours(value: &Value, ty: expr::Ty) -> Option<Vec<Value>> {
    let (Value::Int(v), expr::Ty::Int { min, max }) = (value, ty) else {
        return None;
    };
    let found: Vec<Value> = [v.checked_sub(1), v.checked_add(1)]
        .into_iter()
        .flatten()
        .filter(|n| (min..=max).contains(n))
        .map(Value::Int)
        .collect();
    (!found.is_empty()).then_some(found)
}

/// What one evaluation explains: the constant's two values and the machines
/// that hold them.
struct Explain<'a> {
    constant: &'a str,
    old_value: &'a Value,
    new_value: String,
    before: &'a Machine<'a>,
}

impl Explain<'_> {
    fn detail(
        &self,
        test: &TestSummary,
        text: &str,
        assertion: &Assertion,
        (after, at): (&Machine<'_>, &Value),
    ) -> ConstFlowDetail {
        let render = |machine: &Machine<'_>| {
            machine
                .call_result(assertion)
                .map(|value| value.render())
                .unwrap_or_default()
        };
        ConstFlowDetail {
            constant: self.constant.to_string(),
            old_value: self.old_value.render(),
            new_value: self.new_value.clone(),
            reader: assertion
                .call_names()
                .into_iter()
                .next()
                .unwrap_or_default(),
            test: test.name.clone(),
            assertion: text.split_whitespace().collect::<Vec<_>>().join(" "),
            old_result: render(self.before),
            new_result: render(after),
            new_result_at: at.render(),
            tests_evaluated: 0,
            rewrite: false,
            neighbour_breaks: Vec::new(),
        }
    }
}

/// Whether every call of `name(` in `text` is spelled `self_type::name(`.
/// A trait default method reached through another type, a generic parameter
/// or an unqualified path is not the one the edited `impl` binds.
fn calls_name_the_type(text: &str, name: &str, self_type: &str) -> bool {
    let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    let qualifier = format!("{self_type}::");
    text.match_indices(name)
        .filter(|(at, _)| {
            text[..*at].chars().next_back().is_none_or(|ch| !word(ch))
                && text[at + name.len()..].trim_start().starts_with('(')
        })
        .all(|(at, _)| {
            let before = &text[..at];
            before.ends_with(&qualifier)
                && before[..before.len() - qualifier.len()]
                    .chars()
                    .next_back()
                    .is_none_or(|ch| !word(ch) && ch != ':')
        })
}

/// Whether `text` calls `name(`: a whole-word occurrence followed by `(`.
fn mentions_call(text: &str, name: &str) -> bool {
    let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    text.match_indices(name).any(|(at, _)| {
        text[..at].chars().next_back().is_none_or(|ch| !word(ch))
            && text[at + name.len()..].trim_start().starts_with('(')
    })
}

#[cfg(test)]
mod tests;
