//! A changed range header needs a strong input on which its returned sums differ.
//! This is a withholding gate, not a general loop evaluator or a repair producer.
use super::arm_activation::builtin_names_intact;
use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::boundary::{
    CallRow, resolve_operand, strong_owner_call_rows_with_gaps, unrebound_constants,
};
use super::guard_activation::name_rebound_in_owner;
use super::module_constants::mentions_identifier;
use super::related_tests::PythonRelatedCandidate;
use super::static_limits::PythonStaticLimit;
use super::{PythonOwner, PythonTest};
use crate::domain::{OracleStrength, StaticLimitKind};

pub(super) fn changed_range_limit(
    new_line: &str,
    old_line: Option<&str>,
    owner: &PythonOwner,
    source: &str,
    candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonStaticLimit> {
    let new = range_header(new_line);
    let old = old_line.and_then(range_header);
    if new.is_none() && old.is_none() {
        return None;
    }
    let constants = unrebound_constants(owner, candidates);
    let (rows, gaps) = strong_owner_call_rows_with_gaps(owner, &constants, candidates, focus);
    let witnessed = !gaps
        && !source.is_empty()
        && builtin_names_intact(owner, source, &["range"])
        && sum_accumulator(owner, source, new_line)
        && new.as_ref().zip(old.as_ref()).is_some_and(|(new, old)| {
            new.target == old.target
                && new.readable
                && old.readable
                && owner.parameters.iter().all(|parameter| {
                    !new.args
                        .iter()
                        .chain(&old.args)
                        .any(|arg| mentions_identifier(arg, &parameter.name))
                        || !name_rebound_in_owner(source, owner, &parameter.name)
                })
                && rows.iter().any(|row| {
                    let Some((expected, test)) =
                        asserted_expectation(owner, row, candidates, focus)
                    else {
                        return false;
                    };
                    let read = |header: &RangeHeader, row: &CallRow| {
                        // The comparison resolver canonicalizes integral floats
                        // while shifting bounds. Python range rejects those
                        // values, so establish integer inputs before shifting.
                        if !header.argument_exprs.iter().all(|expr| {
                            integer_range_input(expr, &|name| {
                                resolve_operand(name, row, &constants)
                                    .is_some_and(|value| value.parse::<i128>().is_ok())
                            })
                        }) {
                            return None;
                        }
                        let values: Option<Vec<i128>> = header
                            .args
                            .iter()
                            .map(|arg| resolve_operand(arg, row, &constants)?.parse().ok())
                            .collect();
                        range_sum(&values?)
                    };
                    let modeled = |row: &CallRow| {
                        let skip = receiver_skip(owner)?;
                        if !owner
                            .parameters
                            .get(skip..)?
                            .iter()
                            .all(|parameter| row.bindings.contains_key(&parameter.name))
                        {
                            return None;
                        }
                        read(new, row).zip(read(old, row))
                    };
                    closed_test_body(owner, test, row, &rows, |row| modeled(row).is_some())
                        && matches!(modeled(row), Some((new, old))
                            if new != old && (expected == new || expected == old))
                })
        });
    (!witnessed).then(|| PythonStaticLimit {
        kind: StaticLimitKind::UnsupportedSyntax,
        evidence: "static_limit unsupported_syntax: changed range has no established differing accumulator result".to_string(),
        missing: "Static limit `unsupported_syntax`: no strong related Python call establishes different old and new returned range sums. Both ranges may be empty or sum to the same value for the pinned input; other loop bodies, unreadable bounds, rebound names and unavailable source remain unresolved. Check an input that changes the iterations with a runtime test.".to_string(),
    })
}

/// The shared row reader inventories every call in a strong test. A range
/// witness must instead be the call observed by its selected assertion.
/// Parse the complete, bounded row excerpt: partial/multiline rows refuse.
fn asserted_expectation<'a>(
    owner: &PythonOwner,
    row: &CallRow,
    candidates: &'a [PythonRelatedCandidate<'a>],
    focus: &PythonAssertionFocus,
) -> Option<(i128, &'a PythonTest)> {
    use rustpython_parser::ast::{self, Expr, Mod, Stmt};
    let selected = candidates.iter().find(|candidate| {
        candidate.test.file == row.file
            && select_relevant_assertion(&candidate.test.assertions, Some(focus))
                .assertion()
                .is_some_and(|assertion| {
                    assertion.line == row.line
                        && assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
                })
    });
    let candidate = selected?;
    if candidate.test.constant_rebinding.assigns_attribute("range") {
        return None;
    }
    let Ok(Mod::Module(module)) = super::source_facts::parse_module_result(
        std::path::Path::new("<range-assertion>"),
        &row.text,
    ) else {
        return None;
    };
    let pair = match module.body.as_slice() {
        [Stmt::Assert(assertion)] if assertion.msg.is_none() => {
            let Expr::Compare(compare) = assertion.test.as_ref() else {
                return None;
            };
            let ([ast::CmpOp::Eq], [right]) =
                (compare.ops.as_slice(), compare.comparators.as_slice())
            else {
                return None;
            };
            (compare.left.as_ref(), right)
        }
        [Stmt::Expr(statement)] if candidate.test.framework == super::PythonFramework::Unittest => {
            let Expr::Call(assertion) = statement.value.as_ref() else {
                return None;
            };
            if !assertion.keywords.is_empty()
                || !super::expr_full_name(&assertion.func).is_some_and(|name| {
                    matches!(
                        name.rsplit('.').next(),
                        Some("assertEqual" | "assertEquals")
                    )
                })
            {
                return None;
            }
            let [left, right] = assertion.args.as_slice() else {
                return None;
            };
            (left, right)
        }
        _ => return None,
    };
    direct_integer_pin(owner, pair.0, pair.1)
        .or_else(|| direct_integer_pin(owner, pair.1, pair.0))
        .map(|expected| (expected, candidate.test))
}

fn direct_integer_pin(
    owner: &PythonOwner,
    value: &rustpython_parser::ast::Expr,
    expected: &rustpython_parser::ast::Expr,
) -> Option<i128> {
    use rustpython_parser::ast::{self, Expr};
    let mut sign = 1_i128;
    let expected = match expected {
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
            if unary.op == ast::UnaryOp::USub {
                sign = -1;
            }
            unary.operand.as_ref()
        }
        other => other,
    };
    let Expr::Constant(constant) = expected else {
        return None;
    };
    let ast::Constant::Int(integer) = &constant.value else {
        return None;
    };
    let value_expected = integer
        .to_string()
        .parse::<i128>()
        .ok()?
        .checked_mul(sign)?;
    let Expr::Call(call) = value else {
        return None;
    };
    (valid_owner_call(owner, call)
        && super::expr_full_name(&call.func).is_some()
        && call.args.iter().all(plain_argument)
        && call
            .keywords
            .iter()
            .all(|keyword| keyword.arg.is_some() && plain_argument(&keyword.value)))
    .then_some(value_expected)
}

/// The shared binding reader inventories values, but deliberately tolerates
/// unmatched arguments. The range model needs a call that actually binds.
fn valid_owner_call(owner: &PythonOwner, call: &rustpython_parser::ast::ExprCall) -> bool {
    let Some(skip) = receiver_skip(owner) else {
        return false;
    };
    let Some(parameters) = owner.parameters.get(skip..) else {
        return false;
    };
    let names: std::collections::BTreeSet<_> = parameters.iter().map(|p| p.name.as_str()).collect();
    if names.len() != parameters.len() {
        return false;
    }
    let positional: Vec<_> = parameters.iter().filter(|p| !p.keyword_only).collect();
    if call.args.len() > positional.len() {
        return false;
    }
    let mut bound = std::collections::BTreeSet::new();
    for (argument, parameter) in call.args.iter().zip(positional) {
        if !plain_argument(argument) || !bound.insert(parameter.name.as_str()) {
            return false;
        }
    }
    for keyword in &call.keywords {
        let Some(name) = keyword.arg.as_ref().map(|name| name.as_str()) else {
            return false;
        };
        if !parameters
            .iter()
            .any(|p| p.name == name && !p.positional_only)
            || !bound.insert(name)
            || !plain_argument(&keyword.value)
        {
            return false;
        }
    }
    parameters
        .iter()
        .all(|p| bound.contains(p.name.as_str()) || p.default.is_some())
}

fn receiver_skip(owner: &PythonOwner) -> Option<usize> {
    use crate::domain::OwnerKind;
    match owner.owner_kind {
        Some(OwnerKind::Method) => Some(1),
        Some(OwnerKind::ClassMethod) => Some(usize::from(
            !owner.decorators.iter().any(|d| d.ends_with("staticmethod")),
        )),
        Some(OwnerKind::Function) => Some(0),
        _ => None,
    }
}

fn plain_argument(value: &rustpython_parser::ast::Expr) -> bool {
    use rustpython_parser::ast::{self, Expr};
    match value {
        Expr::Constant(_) | Expr::Name(_) => true,
        Expr::Attribute(_) => super::expr_full_name(value).is_some(),
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
            plain_argument(&unary.operand)
        }
        _ => false,
    }
}

/// No unrelated assertion, branch, exceptional expression or opaque call may
/// prevent the selected assertion, or force the test to fail on both versions.
fn closed_test_body(
    owner: &PythonOwner,
    test: &PythonTest,
    row: &CallRow,
    rows: &[CallRow],
    modeled: impl Fn(&CallRow) -> bool,
) -> bool {
    use rustpython_parser::ast::{Expr, Mod, Ranged, Stmt};
    let source = dedent(&test.body_text);
    let Ok(Mod::Module(module)) =
        super::source_facts::parse_module_result(std::path::Path::new("<range-test>"), &source)
    else {
        return false;
    };
    let [Stmt::FunctionDef(function)] = module.body.as_slice() else {
        return false;
    };
    let parsed = super::source_utils::SourceText::new(&source);
    let names = super::boundary::owner_call_names(
        owner,
        test,
        matches!(
            owner.owner_kind,
            Some(crate::domain::OwnerKind::Method | crate::domain::OwnerKind::ClassMethod)
        ),
    );
    let mut selected = 0;
    for statement in &function.body {
        let local = super::source_utils::line_for_range_start(&parsed, statement.range());
        let Some(line) = test
            .line
            .checked_add(local)
            .and_then(|line| line.checked_sub(1))
        else {
            return false;
        };
        if line == row.line {
            selected += 1;
            continue;
        }
        match statement {
            Stmt::Assign(assign) => {
                let [Expr::Name(name)] = assign.targets.as_slice() else {
                    return false;
                };
                if !literal(&assign.value)
                    || names
                        .iter()
                        .any(|callee| callee.split('.').next() == Some(name.id.as_str()))
                {
                    return false;
                }
            }
            Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::Constant(_)) => {}
            Stmt::Expr(expression) => {
                let Expr::Call(call) = expression.value.as_ref() else {
                    return false;
                };
                if !valid_owner_call(owner, call)
                    || !super::expr_full_name(&call.func).is_some_and(|name| names.contains(&name))
                    || !call.args.iter().all(plain_argument)
                    || !call
                        .keywords
                        .iter()
                        .all(|keyword| keyword.arg.is_some() && plain_argument(&keyword.value))
                {
                    return false;
                }
                let calls: Vec<_> = rows
                    .iter()
                    .filter(|call| call.file == test.file && call.line == line)
                    .collect();
                if calls.is_empty() || !calls.into_iter().all(&modeled) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    selected == 1
}

fn literal(value: &rustpython_parser::ast::Expr) -> bool {
    use rustpython_parser::ast::{self, Expr};
    matches!(value, Expr::Constant(_))
        || matches!(value, Expr::UnaryOp(unary)
        if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub)
            && matches!(unary.operand.as_ref(), Expr::Constant(c) if matches!(c.value, ast::Constant::Int(_))))
}

fn dedent(source: &str) -> String {
    let indent = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    source
        .lines()
        .map(|line| line.get(indent..).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn owner_source_slice(owner: &PythonOwner, source: &str) -> Option<String> {
    if owner.start_line == 0 || owner.end_line < owner.start_line {
        return None;
    }
    let text = source
        .lines()
        .skip(owner.start_line.saturating_sub(1))
        .take(
            owner
                .end_line
                .saturating_sub(owner.start_line)
                .saturating_add(1),
        )
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then(|| dedent(&text))
}

/// Header applicability comes from exact AST line coordinates, not a
/// line-only bare-range recognizer. Unsupported iterator forms fail closed.
pub(super) fn loop_context_limit(
    owner: &PythonOwner,
    source: &str,
    line: usize,
    new_line: &str,
    old_line: Option<&str>,
    sole_line_replacement: bool,
    compound_loop_edit: bool,
) -> Option<PythonStaticLimit> {
    if compound_loop_edit {
        return Some(PythonStaticLimit {
            kind: StaticLimitKind::UnsupportedSyntax,
            evidence: "static_limit unsupported_syntax: combined loop-header and owner edits lack an old-body witness".to_string(),
            missing: "Static limit `unsupported_syntax`: a changed loop header shares this owner with an unresolved multi-line diff. Sibling body or return edits may cancel the apparent range difference; an owner-name assertion alone does not establish different old and new results.".to_string(),
        });
    }
    let old_header = sole_line_replacement
        && old_line
            .and_then(|old| replaced_line(source, line, new_line, old))
            .is_some_and(|old_source| owner_header_at_line(owner, &old_source, line));
    let applies = starts_loop_header(new_line)
        || old_line.is_some_and(starts_loop_header)
        || owner_header_at_line(owner, source, line)
        || old_header;
    if !applies
        || (sole_line_replacement
            && range_header(new_line).is_some()
            && old_line.and_then(range_header).is_some())
    {
        return None;
    }
    Some(PythonStaticLimit {
        kind: StaticLimitKind::UnsupportedSyntax,
        evidence: "static_limit unsupported_syntax: changed loop header lacks a supported isolated range replacement".to_string(),
        missing: "Static limit `unsupported_syntax`: multiline or wrapped iterators and changes without an unchanged old-body witness remain unresolved. The bounded range model requires a single-line header replacement with no other changes in the production file.".to_string(),
    })
}

pub(super) type CompoundLoopOwners = std::collections::BTreeSet<crate::domain::SymbolId>;

/// Bind header edits to the narrowest source owner before quiet-line filtering.
/// Old-side coordinates are read only against reconstructed old facts; a
/// deletion's insertion point must never taint the next independent owner.
pub(super) fn compound_loop_owners(
    changed: &crate::analysis::diff::ChangedFile,
    source: &str,
    owners: &[PythonOwner],
    old_source: Option<&str>,
    old_owners: &[PythonOwner],
) -> CompoundLoopOwners {
    let mut affected = std::collections::BTreeSet::new();
    if matches!((changed.added_lines.as_slice(), changed.removed_lines.as_slice()),
        ([added], [removed]) if added.line == removed.line)
    {
        return affected;
    }
    for added in &changed.added_lines {
        if let Some(owner) = super::owner_for_changed_line(&changed.path, added.line, owners)
            && (starts_loop_header(&added.text) || owner_header_at_line(owner, source, added.line))
        {
            affected.insert(owner.symbol_id());
        }
    }
    if let Some(old_source) = old_source {
        for removed in &changed.removed_lines {
            let Some(old_owner) =
                super::owner_for_changed_line(&changed.path, removed.line, old_owners)
            else {
                continue;
            };
            if !starts_loop_header(&removed.text)
                && !owner_header_at_line(old_owner, old_source, removed.line)
            {
                continue;
            }
            // A retained qualified symbol supplies the association; moving or
            // deleting another function cannot transfer its loop to a neighbor.
            for owner in owners.iter().filter(|owner| {
                owner.file == changed.path
                    && owner.qualified_name == old_owner.qualified_name
                    && owner.owner_kind == old_owner.owner_kind
            }) {
                affected.insert(owner.symbol_id());
            }
        }
    } else {
        // Missing old source permits only the diff's explicit replacement
        // association. Never infer ownership from a deletion insertion point.
        for (line, old) in changed.aligned_counterparts() {
            if starts_loop_header(&old.text)
                && let Some(owner) = super::owner_for_changed_line(&changed.path, line, owners)
            {
                affected.insert(owner.symbol_id());
            }
        }
    }
    affected
}

fn starts_loop_header(line: &str) -> bool {
    use rustpython_parser::{Mode, Tok, lexer::lex};
    let mut tokens = lex(line.trim_start(), Mode::Module)
        .filter_map(Result::ok)
        .map(|(token, _)| token);
    match tokens.next() {
        Some(Tok::For) => true,
        Some(Tok::Async) => matches!(tokens.next(), Some(Tok::For)),
        _ => false,
    }
}

fn replaced_line(source: &str, line: usize, new: &str, old: &str) -> Option<String> {
    if old.contains('\n') {
        return None;
    }
    let mut lines: Vec<_> = source.lines().collect();
    let current = lines.get_mut(line.checked_sub(1)?)?;
    if *current != new.trim_end_matches('\r') {
        return None;
    }
    *current = old;
    Some(lines.join("\n"))
}

fn owner_header_at_line(owner: &PythonOwner, source: &str, line: usize) -> bool {
    use rustpython_parser::ast::Fold;
    let Some(text) = owner_source_slice(owner, source) else {
        return false;
    };
    if !rustpython_parser::lexer::lex(&text, rustpython_parser::Mode::Module)
        .any(|token| matches!(token, Err(_) | Ok((rustpython_parser::Tok::For, _))))
    {
        return false;
    }
    let Some(local_line) = line
        .checked_sub(owner.start_line)
        .and_then(|line| line.checked_add(1))
    else {
        return false;
    };
    let Ok(module) =
        super::source_facts::parse_module_result(std::path::Path::new("<range-loop-span>"), &text)
    else {
        return text.contains("for");
    };
    HeaderLine {
        source: super::source_utils::SourceText::new(&text),
        line: local_line,
    }
    .fold_mod(module)
    .is_err()
}

struct HeaderLine<'a> {
    source: super::source_utils::SourceText<'a>,
    line: usize,
}

impl rustpython_parser::ast::Fold<rustpython_parser::text_size::TextRange> for HeaderLine<'_> {
    type TargetU = rustpython_parser::text_size::TextRange;
    type Error = ();
    type UserContext = ();
    fn will_map_user(&mut self, _: &rustpython_parser::text_size::TextRange) {}
    fn map_user(
        &mut self,
        range: rustpython_parser::text_size::TextRange,
        _: (),
    ) -> Result<Self::TargetU, ()> {
        Ok(range)
    }
    fn fold_stmt(
        &mut self,
        node: rustpython_parser::ast::Stmt,
    ) -> Result<rustpython_parser::ast::Stmt, ()> {
        use rustpython_parser::ast::{Ranged, Stmt};
        let iterator = match &node {
            Stmt::For(header) => Some(header.iter.as_ref()),
            Stmt::AsyncFor(header) => Some(header.iter.as_ref()),
            _ => None,
        };
        if let Some(iterator) = iterator {
            let start = super::source_utils::line_for_range_start(&self.source, node.range());
            let end = super::source_utils::line_for_range_end(&self.source, iterator.range());
            if (start..=end).contains(&self.line) {
                return Err(());
            }
        }
        rustpython_parser::ast::fold::fold_stmt(self, node)
    }
}

struct RangeHeader {
    target: String,
    args: Vec<String>,
    argument_exprs: Vec<rustpython_parser::ast::Expr>,
    readable: bool,
}

/// Keep the native integer type of every input, before the shared resolver's
/// decimal canonicalization. The final existing resolver still restricts which
/// expressions are supported; this check only withholds credit.
fn integer_range_input(
    expression: &rustpython_parser::ast::Expr,
    integer_name: &impl Fn(&str) -> bool,
) -> bool {
    use rustpython_parser::ast::{self, Expr};
    match expression {
        Expr::Constant(value) => matches!(value.value, ast::Constant::Int(_)),
        Expr::Name(name) => integer_name(name.id.as_str()),
        Expr::Attribute(_) => {
            super::expr_full_name(expression).is_some_and(|name| integer_name(&name))
        }
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
            integer_range_input(&unary.operand, integer_name)
        }
        Expr::BinOp(binary) if matches!(binary.op, ast::Operator::Add | ast::Operator::Sub) => {
            integer_range_input(&binary.left, integer_name)
                && integer_range_input(&binary.right, integer_name)
        }
        _ => false,
    }
}

fn loop_header(line: &str) -> Option<(String, rustpython_parser::ast::Stmt)> {
    use rustpython_parser::ast::{Mod, Stmt};
    let source = format!("{}\n    pass\n", line.trim());
    let Mod::Module(module) =
        super::source_facts::parse_module_result(std::path::Path::new("<range-header>"), &source)
            .or_else(|_| {
                super::source_facts::parse_module_result(
                    std::path::Path::new("<inline-range-header>"),
                    line.trim(),
                )
            })
            .ok()?
    else {
        return None;
    };
    let mut body = module.body.into_iter();
    let header = body.next()?;
    if body.next().is_some() || !matches!(header, Stmt::For(_) | Stmt::AsyncFor(_)) {
        return None;
    }
    Some((source, header))
}

fn range_header(line: &str) -> Option<RangeHeader> {
    use rustpython_parser::ast::{Expr, Ranged, Stmt};
    let (source, header) = loop_header(line)?;
    let Stmt::For(header) = header else {
        return None;
    };
    let Expr::Call(call) = header.iter.as_ref() else {
        return None;
    };
    if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "range") {
        return None;
    }
    Some(RangeHeader {
        target: super::source_utils::text_for_range(&source, header.target.range()).to_string(),
        args: call
            .args
            .iter()
            .map(|arg| super::source_utils::text_for_range(&source, arg.range()).to_string())
            .collect(),
        argument_exprs: call.args.clone(),
        readable: call.keywords.is_empty()
            && (1..=3).contains(&call.args.len())
            && matches!(header.body.as_slice(), [Stmt::Pass(_)]),
    })
}

/// Canonical sequence identity without materializing or walking the range.
/// Empty sequences agree, and the step is irrelevant for a singleton.
fn range_signature(args: &[i128]) -> Option<(i128, i128, i128)> {
    let (start, stop, step) = match *args {
        [stop] => (0, stop, 1),
        [start, stop] => (start, stop, 1),
        [start, stop, step] if step != 0 => (start, stop, step),
        _ => return None,
    };
    let (distance, stride) = if step > 0 {
        (stop.checked_sub(start)?, step)
    } else {
        (start.checked_sub(stop)?, step.checked_neg()?)
    };
    if distance <= 0 {
        return Some((0, 0, 0));
    }
    let count = distance
        .checked_sub(1)?
        .checked_div(stride)?
        .checked_add(1)?;
    Some((count, start, if count == 1 { 0 } else { step }))
}

/// Only the complete zero-initialized integer sum shape returns the edited
/// iterations without another branch, early exit, transformation or effect.
fn sum_accumulator(owner: &PythonOwner, source: &str, changed_line: &str) -> bool {
    use rustpython_parser::ast::{self, Expr, Mod, Stmt};
    let Some(source) = owner_source_slice(owner, source) else {
        return false;
    };
    let Ok(Mod::Module(module)) =
        super::source_facts::parse_module_result(std::path::Path::new("<range-sum>"), &source)
    else {
        return false;
    };
    let [Stmt::FunctionDef(function)] = module.body.as_slice() else {
        return false;
    };
    if function
        .args
        .posonlyargs
        .iter()
        .chain(&function.args.args)
        .chain(&function.args.kwonlyargs)
        .any(|argument| {
            argument
                .default
                .as_deref()
                .is_some_and(|expr| !literal(expr))
                || argument
                    .def
                    .annotation
                    .as_deref()
                    .is_some_and(|expr| !literal(expr))
        })
        || function
            .args
            .vararg
            .iter()
            .chain(function.args.kwarg.iter())
            .any(|argument| {
                argument
                    .annotation
                    .as_deref()
                    .is_some_and(|expr| !literal(expr))
            })
        || function
            .returns
            .as_deref()
            .is_some_and(|expr| !literal(expr))
    {
        return false;
    }
    let [
        Stmt::Assign(initial),
        Stmt::For(walk),
        Stmt::Return(returned),
    ] = function.body.as_slice()
    else {
        return false;
    };
    let [Expr::Name(acc)] = initial.targets.as_slice() else {
        return false;
    };
    let Expr::Constant(zero) = initial.value.as_ref() else {
        return false;
    };
    if !matches!(&zero.value, ast::Constant::Int(value) if value.to_string() == "0")
        || !walk.orelse.is_empty()
    {
        return false;
    }
    let Expr::Name(item) = walk.target.as_ref() else {
        return false;
    };
    let [Stmt::AugAssign(update)] = walk.body.as_slice() else {
        return false;
    };
    let (Expr::Name(target), Expr::Name(value), Some(Expr::Name(result))) = (
        update.target.as_ref(),
        update.value.as_ref(),
        returned.value.as_deref(),
    ) else {
        return false;
    };
    use rustpython_parser::ast::Ranged;
    let start = usize::from(walk.range().start());
    let header = source.get(start..).and_then(|rest| rest.lines().next());
    acc.id != item.id
        && target.id == acc.id
        && value.id == item.id
        && result.id == acc.id
        && matches!(update.op, ast::Operator::Add)
        && header.is_some_and(|header| header.trim() == changed_line.trim())
}

fn range_sum(args: &[i128]) -> Option<i128> {
    let (count, start, step) = range_signature(args)?;
    if count == 0 {
        return Some(0);
    }
    let last = start.checked_add(count.checked_sub(1)?.checked_mul(step)?)?;
    let pair = start.checked_add(last)?;
    if count % 2 == 0 {
        (count / 2).checked_mul(pair)
    } else {
        count.checked_mul(pair / 2)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn changed_range_owner_line_count_bounds() -> Result<(), String> {
        let source = "def sum_to(n):\n    return n\n";
        let mut owner = super::super::owners_tests::extract_owners(
            std::path::Path::new("src/subject.py"),
            source,
        )
        .into_iter()
        .find(|owner| owner.name == "sum_to")
        .ok_or("owner missing")?;
        assert_eq!(
            super::owner_source_slice(&owner, source).as_deref(),
            Some(source.trim_end())
        );
        for (start, end) in [(0, usize::MAX), (usize::MAX, usize::MAX), (4, 3)] {
            owner.start_line = start;
            owner.end_line = end;
            assert!(
                super::owner_source_slice(&owner, source).is_none(),
                "{start}..{end}"
            );
        }
        Ok(())
    }
}
