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

mod expr;

use std::collections::BTreeMap;

use self::expr::{
    Assertion, Machine, Reader, Value, parse_assertion, parse_const_decl, parse_reader,
};
use super::related_tests::normalize_path;
use crate::analysis::diff::ChangedFile;
use crate::analysis::facts::{FunctionSummary, TestSummary};

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
    pub new_result: String,
    pub tests_evaluated: usize,
    /// The edit kept the value; `new_value` lists the neighbours tried.
    pub rewrite: bool,
}

/// Evaluate the flow, or `None` when any part of it is unreadable.
///
/// `functions_named` counts the functions of the constant's file with a
/// name: a reader whose name another function of the file shares, or that
/// sits in an `impl`, may not be the function the test's call reaches.
pub(in crate::analysis) fn evaluate_const_flow(
    change: &ConstChange,
    readers: &[&FunctionSummary],
    tests: &[&TestSummary],
    functions_named: &dyn Fn(&str) -> usize,
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
        if functions_named(&reader.name) != 1
            || !matches!(
                reader.impl_context,
                crate::analysis::facts::FunctionImplContext::Free
            )
        {
            return None;
        }
        parsed.push(parse_reader(&reader.body)?);
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
        after: first_after,
    };
    let reader_names: Vec<&str> = parsed.iter().map(|reader| reader.name.as_str()).collect();

    let mut first_seen: Option<ConstFlowDetail> = None;
    let mut failing: Option<ConstFlowDetail> = None;
    let mut flipped = vec![false; afters.len()];
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
            let names = assertion.call_names();
            if names.is_empty()
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
            let detail = explain.detail(test, &oracle.text, &assertion);
            if first_seen.is_none() {
                first_seen = Some(detail.clone());
            }
            for (after, flip) in afters.iter().zip(flipped.iter_mut()) {
                if !after.holds(&assertion)? {
                    *flip = true;
                    if failing.is_none() {
                        failing = Some(detail.clone());
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
        // Credit only when every neighbour is noticed; a rewrite that some
        // tests catch and others do not is not decided here.
        return match (flipped.iter().all(|flip| *flip), failing) {
            (true, Some(failing)) => Some(ConstFlow::Discriminated(detail(failing))),
            _ => None,
        };
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
    after: &'a Machine<'a>,
}

impl Explain<'_> {
    fn detail(&self, test: &TestSummary, text: &str, assertion: &Assertion) -> ConstFlowDetail {
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
            new_result: render(self.after),
            tests_evaluated: 0,
            rewrite: false,
        }
    }
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
