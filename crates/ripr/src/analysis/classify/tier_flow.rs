//! A bounded selected literal, local binding and direct-return witness.
//!
//! This is deliberately limited to primitive, literal-input paths. Unknown
//! syntax, unresolved names, outputs transformed by anything other than the
//! one admitted `binding * <u32 literal>` return shape and ambiguous test
//! calls produce no witness.

use super::const_flow::expr::{
    Assertion, BinOp, Expr, Machine, Reader, Ty, Value, parse_assertion, parse_const_decl,
    parse_expression, parse_fn_header,
};
use super::owner_pin::{
    OwnerPinSyntax, builtin_type_unshadowed, declared_return_type, targeted_free_call_reaches_owner,
};
use crate::analysis::diff::ChangedFile;
use crate::analysis::facts::{FunctionImplContext, FunctionSummary, OracleFact, TestSummary};
use crate::analysis::rust_index::{RustIndex, find_file_facts};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasName},
};
use std::collections::{BTreeMap, BTreeSet};

// These limits precede the recursive expression parser and bound both the
// tier-by-sink value expansion and the per-test assertion replay.
const MAX_OWNER_BYTES: usize = 4096;
const MAX_TEST_BYTES: usize = 4096;
const MAX_FILE_BYTES: usize = 131072;
const MAX_SYNTAX_DEPTH: usize = 24;
const MAX_UNARY_TOKENS: usize = 32;
const MAX_OPERATOR_TOKENS: usize = 128;
const MAX_TOKENS: usize = 256;
const MAX_IF_KEYWORDS: usize = 24;
const MAX_RECURSIVE_KEYWORDS: usize = 64;
const MAX_FILE_TOKENS: usize = 1024;
const MAX_FILE_OPERATORS: usize = 256;
const MAX_FILE_UNARY_TOKENS: usize = 128;
const MAX_FILE_IF_KEYWORDS: usize = 64;
const MAX_FILE_RECURSIVE_KEYWORDS: usize = 128;
const MAX_TESTS: usize = 8;
const MAX_ASSERTIONS: usize = 16;
const MAX_CHANGED_LINES: usize = 256;
const MAX_TIER_NODES: usize = 32;
const MAX_SINK_NODES: usize = 16;

fn bounded_masked(
    text: &str,
    max_bytes: usize,
    max_unary: usize,
    whole_file: bool,
) -> Option<String> {
    if text.len() > max_bytes {
        return None;
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(text);
    let (max_tokens, max_operators, max_ifs, max_keywords) = if whole_file {
        (
            MAX_FILE_TOKENS,
            MAX_FILE_OPERATORS,
            MAX_FILE_IF_KEYWORDS,
            MAX_FILE_RECURSIVE_KEYWORDS,
        )
    } else {
        (
            MAX_TOKENS,
            MAX_OPERATOR_TOKENS,
            MAX_IF_KEYWORDS,
            MAX_RECURSIVE_KEYWORDS,
        )
    };
    let words = || masked.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'));
    if words()
        .filter(|word| *word == "if")
        .take(max_ifs + 1)
        .count()
        > max_ifs
        || words()
            .filter(|word| {
                matches!(
                    *word,
                    "if" | "return"
                        | "break"
                        | "yield"
                        | "async"
                        | "move"
                        | "while"
                        | "for"
                        | "match"
                        | "let"
                        | "loop"
                )
            })
            .take(max_keywords + 1)
            .count()
            > max_keywords
    {
        return None;
    }
    let mut depth = 0usize;
    let mut unary = 0usize;
    let mut operators = 0usize;
    let mut tokens = 0usize;
    let mut in_word = false;
    for byte in masked.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii() {
            if !in_word {
                tokens += 1;
            }
            in_word = true;
        } else {
            in_word = false;
            if !byte.is_ascii_whitespace() {
                tokens += 1;
            }
        }
        if tokens > max_tokens {
            return None;
        }
        if matches!(
            byte,
            b'!' | b'-'
                | b'*'
                | b'&'
                | b'+'
                | b'/'
                | b'%'
                | b'|'
                | b'^'
                | b'<'
                | b'>'
                | b'='
                | b'.'
                | b'@'
        ) {
            operators += 1;
            if operators > max_operators {
                return None;
            }
        }
        match byte {
            b'(' | b'[' | b'{' => {
                depth += 1;
                if depth > MAX_SYNTAX_DEPTH {
                    return None;
                }
            }
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'!' | b'-' | b'*' | b'&' => {
                unary += 1;
                if unary > max_unary {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(masked)
}

fn bounded_syntax(text: &str, max_bytes: usize, max_unary: usize, whole_file: bool) -> bool {
    bounded_masked(text, max_bytes, max_unary, whole_file).is_some()
}

fn bounded_expression(text: &str) -> Option<Expr> {
    bounded_syntax(text, 256, MAX_UNARY_TOKENS, false)
        .then(|| parse_expression(text))
        .flatten()
}

fn bounded_assertion(text: &str) -> Option<Assertion> {
    bounded_syntax(text, 256, MAX_UNARY_TOKENS, false)
        .then(|| parse_assertion(text))
        .flatten()
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct TierFlow {
    pub owner: String,
    pub literal: u32,
    pub test: String,
    pub file: std::path::PathBuf,
    pub line: usize,
    pub assertion: String,
    pub sink: String,
    pub sink_line: usize,
}

fn number(text: &str) -> Option<u32> {
    let text = text.trim();
    (!text.is_empty() && text.bytes().all(|c| c.is_ascii_digit() || c == b'_'))
        .then(|| text.replace('_', "").parse().ok())?
}

/// Read only boolean combinations of primitive values and comparisons.
fn condition(text: &str, names: &mut BTreeSet<String>) -> Option<Expr> {
    fn admit(expr: &Expr, names: &mut BTreeSet<String>) -> bool {
        match expr {
            Expr::Literal(Value::Int(_) | Value::Bool(_)) => true,
            Expr::Var(name) => {
                names.insert(name.clone());
                true
            }
            Expr::Not(inner) => admit(inner, names),
            Expr::Bin(
                BinOp::And
                | BinOp::Or
                | BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge,
                left,
                right,
            ) => admit(left, names) && admit(right, names),
            _ => false,
        }
    }
    let parsed = bounded_expression(text)?;
    admit(&parsed, names).then_some(parsed)
}

// Reject malformed or ambiguous expressions instead of assigning them Rust types.
// An integer comparison needs a declared u32 operand; two unsuffixed literals
// could otherwise use a different default type.
fn typed_trace(expr: &Expr, params: &[(String, Ty)], constant: &str) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Bool,
        U32,
        UntypedInt,
    }
    fn kind(expr: &Expr, params: &[(String, Ty)], constant: &str) -> Option<Kind> {
        match expr {
            Expr::Literal(Value::Bool(_)) => Some(Kind::Bool),
            Expr::Literal(Value::Int(value)) if (0..=u32::MAX as i64).contains(value) => {
                Some(Kind::UntypedInt)
            }
            Expr::Var(name) if !constant.is_empty() && name == constant => Some(Kind::U32),
            Expr::Var(name) => match params.iter().find(|(param, _)| param == name)?.1 {
                Ty::Bool => Some(Kind::Bool),
                Ty::Int {
                    min: 0,
                    max: 4294967295,
                } => Some(Kind::U32),
                _ => None,
            },
            Expr::Not(inner) if kind(inner, params, constant)? == Kind::Bool => Some(Kind::Bool),
            Expr::Bin(BinOp::And | BinOp::Or, left, right)
                if kind(left, params, constant)? == Kind::Bool
                    && kind(right, params, constant)? == Kind::Bool =>
            {
                Some(Kind::Bool)
            }
            Expr::Bin(op, left, right)
                if matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                ) =>
            {
                let l = kind(left, params, constant)?;
                let r = kind(right, params, constant)?;
                ((l == Kind::U32 && matches!(r, Kind::U32 | Kind::UntypedInt))
                    || (r == Kind::U32 && matches!(l, Kind::U32 | Kind::UntypedInt))
                    || (l == Kind::Bool && r == Kind::Bool && matches!(op, BinOp::Eq | BinOp::Ne)))
                .then_some(Kind::Bool)
            }
            Expr::If(cond, yes, no)
                if kind(cond, params, constant)? == Kind::Bool
                    && kind(yes, params, constant)? == Kind::Bool
                    && kind(no, params, constant)? == Kind::Bool =>
            {
                Some(Kind::Bool)
            }
            _ => None,
        }
    }
    kind(expr, params, constant) == Some(Kind::Bool)
}

fn boolean(value: bool) -> Expr {
    Expr::Literal(Value::Bool(value))
}
fn choose(condition: Expr, yes: Expr, no: Expr) -> Expr {
    Expr::If(Box::new(condition), Box::new(yes), Box::new(no))
}
fn only_tail(block: &ast::BlockExpr) -> Option<ast::Expr> {
    let list = block.stmt_list()?;
    list.statements()
        .next()
        .is_none()
        .then(|| list.tail_expr())
        .flatten()
}
fn line_of(node: &ra_ap_syntax::SyntaxNode, owner: &FunctionSummary) -> Option<usize> {
    let offset = usize::from(node.text_range().start());
    Some(
        owner.start_line
            + owner
                .body
                .get(..offset)?
                .bytes()
                .filter(|c| *c == b'\n')
                .count(),
    )
}

/// Map literal leaves of a complete if/else initializer to whether this is
/// the unique changed leaf. Conditions stay as executable bounded expressions.
fn tier_trace(
    expr: ast::Expr,
    owner: &FunctionSummary,
    probe: &Probe,
    names: &mut BTreeSet<String>,
    hits: &mut usize,
    remaining: &mut usize,
) -> Option<Expr> {
    if *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    match expr {
        ast::Expr::IfExpr(branch) => {
            let cond = condition(&branch.condition()?.syntax().text().to_string(), names)?;
            let yes = tier_trace(
                only_tail(&branch.then_branch()?)?,
                owner,
                probe,
                names,
                hits,
                remaining,
            )?;
            let no = match branch.else_branch()? {
                ast::ElseBranch::Block(block) => {
                    tier_trace(only_tail(&block)?, owner, probe, names, hits, remaining)?
                }
                ast::ElseBranch::IfExpr(other) => tier_trace(
                    ast::Expr::IfExpr(other),
                    owner,
                    probe,
                    names,
                    hits,
                    remaining,
                )?,
            };
            Some(choose(cond, yes, no))
        }
        ast::Expr::Literal(literal) => {
            let value = number(&literal.syntax().text().to_string())?;
            let selected = line_of(literal.syntax(), owner)? == probe.location.line;
            if selected {
                if value != number(probe.after.as_deref()?)? {
                    return None;
                }
                *hits += 1;
            }
            Some(boolean(selected))
        }
        _ => None,
    }
}

/// A sink leaf reaches the return when it is the binding itself or the one
/// admitted computed shape `binding * <u32 literal>`; a different branch is
/// represented as false. Which value the assertion must pin is decided by
/// `plain_assertion` and the whole-test distinguishing replay, not here.
fn sink_trace(
    expr: ast::Expr,
    binding: &str,
    names: &mut BTreeSet<String>,
    remaining: &mut usize,
) -> Option<Expr> {
    if *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    match expr {
        ast::Expr::IfExpr(branch) => {
            let cond = condition(&branch.condition()?.syntax().text().to_string(), names)?;
            let yes = sink_trace(
                only_tail(&branch.then_branch()?)?,
                binding,
                names,
                remaining,
            )?;
            let no = match branch.else_branch()? {
                ast::ElseBranch::Block(block) => {
                    sink_trace(only_tail(&block)?, binding, names, remaining)?
                }
                ast::ElseBranch::IfExpr(other) => {
                    sink_trace(ast::Expr::IfExpr(other), binding, names, remaining)?
                }
            };
            Some(choose(cond, yes, no))
        }
        ast::Expr::PathExpr(path) => (path.syntax().text() == binding).then(|| boolean(true)),
        // The one admitted computed shape: the binding multiplied by a u32
        // literal. It reaches the return exactly like the binding itself;
        // the pin and replay gates decide whether the asserted product
        // actually distinguishes the changed tier.
        ast::Expr::BinExpr(binary) => {
            let parsed = bounded_expression(&binary.syntax().text().to_string())?;
            match parsed {
                Expr::Bin(BinOp::Mul, left, right)
                    if matches!(*left, Expr::Var(ref name) if name == binding)
                        && matches!(*right, Expr::Literal(Value::Int(value)) if (0..=u32::MAX as i64).contains(&value)) =>
                {
                    Some(boolean(true))
                }
                _ => None,
            }
        }
        ast::Expr::Literal(literal) => {
            number(&literal.syntax().text().to_string()).map(|_| boolean(false))
        }
        _ => None,
    }
}

fn prefix_guard(statement: &ast::Stmt, names: &mut BTreeSet<String>) -> Option<(Expr, u32)> {
    let ast::Stmt::ExprStmt(statement) = statement else {
        return None;
    };
    let ast::Expr::IfExpr(branch) = statement.expr()? else {
        return None;
    };
    if branch.else_branch().is_some() {
        return None;
    }
    let list = branch.then_branch()?.stmt_list()?;
    let statements: Vec<_> = list.statements().collect();
    let [ast::Stmt::ExprStmt(exit)] = statements.as_slice() else {
        return None;
    };
    if list.tail_expr().is_some() {
        return None;
    }
    let ast::Expr::ReturnExpr(returned) = exit.expr()? else {
        return None;
    };
    let value = number(&returned.expr()?.syntax().text().to_string())?;
    Some((
        condition(&branch.condition()?.syntax().text().to_string(), names)?,
        value,
    ))
}

// The execution prefix needs the actual return value, not just the selected
// branch bit. This reader handles exactly the same bounded expression shapes.
fn tier_values(
    expr: ast::Expr,
    owner: &FunctionSummary,
    probe: &Probe,
    old: Option<u32>,
) -> Option<(Expr, Expr)> {
    match expr {
        ast::Expr::IfExpr(branch) => {
            let condition = condition(
                &branch.condition()?.syntax().text().to_string(),
                &mut BTreeSet::new(),
            )?;
            let (new_yes, old_yes) =
                tier_values(only_tail(&branch.then_branch()?)?, owner, probe, old)?;
            let (new_no, old_no) = match branch.else_branch()? {
                ast::ElseBranch::Block(block) => {
                    tier_values(only_tail(&block)?, owner, probe, old)?
                }
                ast::ElseBranch::IfExpr(other) => {
                    tier_values(ast::Expr::IfExpr(other), owner, probe, old)?
                }
            };
            Some((
                choose(condition.clone(), new_yes, new_no),
                choose(condition, old_yes, old_no),
            ))
        }
        ast::Expr::Literal(literal) => {
            let value = number(&literal.syntax().text().to_string())?;
            let previous = if line_of(literal.syntax(), owner)? == probe.location.line {
                old.unwrap_or(value)
            } else {
                value
            };
            Some((
                Expr::Literal(Value::Int(i64::from(value))),
                Expr::Literal(Value::Int(i64::from(previous))),
            ))
        }
        _ => None,
    }
}

fn returned_value(expr: ast::Expr, binding: &str, value: &Expr) -> Option<Expr> {
    match expr {
        ast::Expr::IfExpr(branch) => {
            let condition = condition(
                &branch.condition()?.syntax().text().to_string(),
                &mut BTreeSet::new(),
            )?;
            let yes = returned_value(only_tail(&branch.then_branch()?)?, binding, value)?;
            let no = match branch.else_branch()? {
                ast::ElseBranch::Block(block) => {
                    returned_value(only_tail(&block)?, binding, value)?
                }
                ast::ElseBranch::IfExpr(other) => {
                    returned_value(ast::Expr::IfExpr(other), binding, value)?
                }
            };
            Some(choose(condition, yes, no))
        }
        ast::Expr::PathExpr(path) if path.syntax().text() == binding => Some(value.clone()),
        ast::Expr::BinExpr(binary) => {
            match bounded_expression(&binary.syntax().text().to_string())? {
                Expr::Bin(BinOp::Mul, left, right)
                    if matches!(*left, Expr::Var(ref name) if name == binding)
                        && matches!(*right, Expr::Literal(Value::Int(number)) if (0..=u32::MAX as i64).contains(&number)) =>
                {
                    Some(Expr::Bin(BinOp::Mul, Box::new(value.clone()), right))
                }
                _ => None,
            }
        }
        ast::Expr::Literal(literal) => Some(Expr::Literal(Value::Int(i64::from(number(
            &literal.syntax().text().to_string(),
        )?)))),
        _ => None,
    }
}

fn changed_literal(
    probe: &Probe,
    owner: &FunctionSummary,
    changed: &ChangedFile,
) -> Option<Option<u32>> {
    if probe.family != ProbeFamily::StaticUnknown
        || changed.added_lines.len() > MAX_CHANGED_LINES
        || changed.removed_lines.len() > 1
    {
        return None;
    }
    let now = number(probe.after.as_deref()?)?;
    if !changed
        .added_lines
        .iter()
        .any(|line| line.new_side_line == probe.location.line && number(&line.text) == Some(now))
    {
        return None;
    }
    if changed.removed_lines.is_empty() {
        let lines: BTreeSet<_> = changed
            .added_lines
            .iter()
            .map(|line| line.new_side_line)
            .collect();
        return (owner.start_line..=owner.end_line)
            .all(|line| lines.contains(&line))
            .then_some(None);
    }
    if changed.added_lines.len() != 1 || changed.removed_lines.len() != 1 {
        return None;
    }
    let paired = changed.replaced_line_counterpart(probe.location.line)?;
    let old = number(&paired.text)?;
    if number(probe.before.as_deref()?) != Some(old) {
        return None;
    }
    (old != now).then_some(Some(old))
}

/// The literal multipliers of every admitted `binding * <u32 literal>` leaf
/// in the return tail. Any other leaf shape withholds the witness, matching
/// `returned_value`.
fn sink_multipliers(expr: ast::Expr, binding: &str) -> Option<BTreeSet<u32>> {
    match expr {
        ast::Expr::IfExpr(branch) => {
            let mut found = sink_multipliers(only_tail(&branch.then_branch()?)?, binding)?;
            let rest = match branch.else_branch()? {
                ast::ElseBranch::Block(block) => sink_multipliers(only_tail(&block)?, binding)?,
                ast::ElseBranch::IfExpr(other) => {
                    sink_multipliers(ast::Expr::IfExpr(other), binding)?
                }
            };
            found.extend(rest);
            Some(found)
        }
        ast::Expr::PathExpr(path) if path.syntax().text() == binding => Some(BTreeSet::new()),
        ast::Expr::BinExpr(binary) => {
            match bounded_expression(&binary.syntax().text().to_string())? {
                Expr::Bin(BinOp::Mul, left, right) if matches!(*left, Expr::Var(ref name) if name == binding) =>
                {
                    let Expr::Literal(Value::Int(value)) = *right else {
                        return None;
                    };
                    let value = u32::try_from(value).ok()?;
                    Some(BTreeSet::from([value]))
                }
                _ => None,
            }
        }
        ast::Expr::Literal(literal) => {
            number(&literal.syntax().text().to_string()).map(|_| BTreeSet::new())
        }
        _ => None,
    }
}

fn plain_assertion(
    assertion: &Assertion,
    name: &str,
    now: u32,
    old: Option<u32>,
    multipliers: &BTreeSet<u32>,
) -> bool {
    match assertion {
        Assertion::Eq(Expr::Call(callee, args), Expr::Literal(Value::Int(expected)))
            if callee == name
                && args
                    .iter()
                    .all(|arg| matches!(arg, Expr::Literal(Value::Int(_) | Value::Bool(_)))) =>
        {
            if *expected == i64::from(now) || old.is_some_and(|old| *expected == i64::from(old)) {
                return true;
            }
            // A computed pin must name the current or previous tier times the
            // admitted multiplier, with the product representable as u32.
            let Ok(expected) = u32::try_from(*expected) else {
                return false;
            };
            multipliers.iter().any(|factor| {
                now.checked_mul(*factor) == Some(expected)
                    || old.is_some_and(|old| old.checked_mul(*factor) == Some(expected))
            })
        }
        _ => false,
    }
}

/// The existing occurrence reader does not associate multiple owner calls on
/// one line with individual assertions (#13373). Count those calls once for
/// every oracle line so each later read refuses ambiguous lines without
/// repeatedly masking or rescanning the test file.
fn owner_call_counts(masked: &str, test: &TestSummary, name: &str) -> BTreeMap<usize, usize> {
    let wanted: BTreeSet<usize> = test.assertions.iter().map(|oracle| oracle.line).collect();
    let mut counts = BTreeMap::new();
    for (at, text) in masked.lines().enumerate() {
        let line = at + 1;
        if !wanted.contains(&line) {
            continue;
        }
        let mut count = 0usize;
        for (offset, _) in text.match_indices(name) {
            let before = text[..offset].chars().next_back();
            let after = &text[offset + name.len()..];
            if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                || after
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }
            if after.trim_start().starts_with('(') {
                count += 1;
            }
        }
        counts.insert(line, count);
    }
    counts
}

// A failed or opaque statement before or after the pin can make both test
// versions fail. Establish the prefix on both versions and the suffix on the
// version where the pin holds, using only bounded assertions of this owner.
fn assertion_test_distinguishes(
    test: &TestSummary,
    target: &OracleFact,
    name: &str,
    readers: (&Reader, &Reader, bool),
    constant: (&str, &Value),
    site: (&RustIndex, &OwnerPinSyntax, &BTreeMap<usize, usize>),
) -> bool {
    let Some(parse) = parse_clean_source_file(&test.body) else {
        return false;
    };
    let Some(function) = parse
        .tree()
        .syntax()
        .children()
        .filter_map(ast::Fn::cast)
        .next()
    else {
        return false;
    };
    if function.name().is_none_or(|item| item.text() != test.name) {
        return false;
    }
    let Some(list) = function.body().and_then(|body| body.stmt_list()) else {
        return false;
    };
    let new_readers = [readers.0.clone()];
    let old_readers = [readers.1.clone()];
    let machine = |readers| Machine {
        readers,
        constant: constant.0,
        constant_ty: Ty::Int {
            min: 0,
            max: 4294967295,
        },
        constant_value: constant.1,
    };
    let new_machine = machine(&new_readers);
    let old_machine = machine(&old_readers);
    let Some(target_assertion) = bounded_assertion(&target.text) else {
        return false;
    };
    let new_holds = new_machine.holds(&target_assertion);
    let old_holds = old_machine.holds(&target_assertion);
    let passing_new = match (new_holds, old_holds, readers.2) {
        (Some(true), Some(false), true) | (Some(true), _, false) => true,
        (Some(false), Some(true), true) => false,
        _ => return false,
    };
    let same = |left: &str, right: &str| {
        left.trim().trim_end_matches(';').trim() == right.trim().trim_end_matches(';').trim()
    };
    let nodes = list
        .statements()
        .map(|stmt| stmt.syntax().clone())
        .chain(list.tail_expr().map(|tail| tail.syntax().clone()));
    let mut seen_target = false;
    for node in nodes {
        let offset = usize::from(node.text_range().start());
        let Some(prefix) = test.body.get(..offset) else {
            return false;
        };
        let line = test.start_line + prefix.bytes().filter(|byte| *byte == b'\n').count();
        let text = node.text().to_string();
        if line == target.line && same(&text, &target.text) && !seen_target {
            seen_target = true;
            continue;
        }
        let Some(oracle) = test
            .assertions
            .iter()
            .find(|oracle| oracle.line == line && same(&text, &oracle.text))
        else {
            return false;
        };
        if site.2.get(&line) != Some(&1)
            || !site.1.admits_tier_return_assertion(test, oracle, site.0)
        {
            return false;
        }
        let Some(assertion) = bounded_assertion(&oracle.text) else {
            return false;
        };
        let Assertion::Eq(Expr::Call(ref callee, ref args), Expr::Literal(Value::Int(expected))) =
            assertion
        else {
            return false;
        };
        if callee != name
            || !(0..=u32::MAX as i64).contains(&expected)
            || !args
                .iter()
                .all(|arg| matches!(arg, Expr::Literal(Value::Int(_) | Value::Bool(_))))
        {
            return false;
        }
        let new_holds = new_machine.holds(&assertion);
        let old_holds = old_machine.holds(&assertion);
        if (!seen_target && (new_holds != Some(true) || old_holds != Some(true)))
            || (seen_target && if passing_new { new_holds } else { old_holds } != Some(true))
        {
            return false;
        }
    }
    seen_target
}

pub(in crate::analysis) fn evaluate_tier_flow(
    probe: &Probe,
    owner: &FunctionSummary,
    changed: &ChangedFile,
    tests: &[&TestSummary],
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
    workspace_complete: bool,
) -> Option<TierFlow> {
    if !workspace_complete
        || !matches!(owner.impl_context, FunctionImplContext::Free)
        || !owner.attrs.is_empty()
        || tests.is_empty()
        || tests.len() > MAX_TESTS
        || !bounded_syntax(&owner.body, MAX_OWNER_BYTES, MAX_UNARY_TOKENS, false)
        || declared_return_type(&owner.body).as_deref() != Some("u32")
    {
        return None;
    }
    let old = changed_literal(probe, owner, changed)?;
    let now = number(probe.after.as_deref()?)?;
    let facts = find_file_facts(index, &owner.file)?;
    if !bounded_syntax(&facts.source, MAX_FILE_BYTES, MAX_FILE_UNARY_TOKENS, true)
        || !builtin_type_unshadowed("u32", index, syntax)
        || !builtin_type_unshadowed("bool", index, syntax)
    {
        return None;
    }
    let (name, params) = parse_fn_header(&owner.body)?;
    if name != owner.name
        || params.iter().any(|(_, ty)| {
            !matches!(
                ty,
                Ty::Bool
                    | Ty::Int {
                        min: 0,
                        max: 4294967295
                    }
            )
        })
    {
        return None;
    }
    let param_names: BTreeSet<String> = params.iter().map(|(name, _)| name.clone()).collect();
    if param_names.len() != params.len() {
        return None;
    }
    let parse = parse_clean_source_file(&owner.body)?;
    let mut items = parse.tree().syntax().children().filter_map(ast::Item::cast);
    let ast::Item::Fn(function) = items.next()? else {
        return None;
    };
    if items.next().is_some()
        || function.attrs().next().is_some()
        || function.name()?.text() != owner.name
    {
        return None;
    }
    if function.syntax().descendants().skip(1).any(|node| {
        ast::MacroCall::can_cast(node.kind())
            || ast::ClosureExpr::can_cast(node.kind())
            || ast::TryExpr::can_cast(node.kind())
            || ast::Item::can_cast(node.kind())
    }) {
        return None;
    }
    let body = function.body()?.stmt_list()?;
    let statements: Vec<_> = body.statements().collect();
    let (guard, binding) = match statements.as_slice() {
        [ast::Stmt::LetStmt(binding)] => (None, binding),
        [guard, ast::Stmt::LetStmt(binding)] => (Some(guard), binding),
        _ => return None,
    };
    if binding.attrs().next().is_some() || binding.ty().is_some() || binding.let_else().is_some() {
        return None;
    }
    let ast::Pat::IdentPat(local) = binding.pat()? else {
        return None;
    };
    if local.mut_token().is_some() || local.ref_token().is_some() || local.at_token().is_some() {
        return None;
    }
    let binding = local.name()?.text().to_string();
    if param_names.contains(&binding) {
        return None;
    }
    let mut names = BTreeSet::new();
    let mut hits = 0;
    let mut tier_budget = MAX_TIER_NODES;
    let tier = tier_trace(
        binding_initializer(&statements)?,
        owner,
        probe,
        &mut names,
        &mut hits,
        &mut tier_budget,
    )?;
    if hits != 1 {
        return None;
    }
    let tail = body.tail_expr()?;
    let sink_line = line_of(tail.syntax(), owner)?;
    let sink_text = tail.syntax().text().to_string();
    let mut sink_budget = MAX_SINK_NODES;
    let sink = sink_trace(tail, &binding, &mut names, &mut sink_budget)?;
    let mut trace = Expr::Bin(BinOp::And, Box::new(tier), Box::new(sink));
    let guard = match guard {
        Some(guard) => Some(prefix_guard(guard, &mut names)?),
        None => None,
    };
    if let Some((condition, _)) = &guard {
        trace = choose(condition.clone(), boolean(false), trace);
    }
    let free: Vec<_> = names.difference(&param_names).collect();
    let (constant_name, constant_value) = match free.as_slice() {
        [] => (String::new(), Value::Int(0)),
        [name] => {
            if syntax.macro_may_declare(name, index) {
                return None;
            }
            let mut declarations = facts.source.lines().enumerate().filter_map(|(at, line)| {
                if !bounded_syntax(line, 256, MAX_UNARY_TOKENS, false) {
                    return None;
                }
                let decl = parse_const_decl(line)?;
                (decl.name == **name).then_some((at + 1, decl))
            });
            let (line, decl) = declarations.next()?;
            if declarations.next().is_some()
                || decl.ty
                    != (Ty::Int {
                        min: 0,
                        max: 4294967295,
                    })
                || !super::const_flow::shares_module_scope(&facts.source, line, &[owner.start_line])
            {
                return None;
            }
            let parsed_file = parse_clean_source_file(&facts.source)?;
            let constants: Vec<_> = parsed_file
                .tree()
                .syntax()
                .children()
                .filter_map(ast::Const::cast)
                .filter(|item| {
                    item.name()
                        .is_some_and(|item_name| item_name.text() == name.as_str())
                })
                .collect();
            if constants.len() != 1 || constants[0].attrs().next().is_some() {
                return None;
            }
            let Expr::Literal(Value::Int(value)) = decl.init else {
                return None;
            };
            if !(0..=u32::MAX as i64).contains(&value) {
                return None;
            }
            (name.to_string(), Value::Int(value))
        }
        _ => return None,
    };
    if !typed_trace(&trace, &params, &constant_name) {
        return None;
    }
    let (new_tier, old_tier) = tier_values(binding_initializer(&statements)?, owner, probe, old)?;
    let tail = body.tail_expr()?;
    let multipliers = sink_multipliers(tail.clone(), &binding)?;
    let mut new_value = returned_value(tail.clone(), &binding, &new_tier)?;
    let mut old_value = returned_value(tail, &binding, &old_tier)?;
    if let Some((condition, value)) = guard {
        let exit = Expr::Literal(Value::Int(i64::from(value)));
        new_value = choose(condition.clone(), exit.clone(), new_value);
        old_value = choose(condition, exit, old_value);
    }
    let new_reader = Reader::from_branch_parts(name.clone(), params.clone(), new_value)?;
    let old_reader = Reader::from_branch_parts(name.clone(), params.clone(), old_value)?;
    let reader = Reader::from_branch_parts(name.clone(), params, trace)?;
    let readers = [reader];
    let machine = Machine {
        readers: &readers,
        constant: &constant_name,
        constant_ty: Ty::Int {
            min: 0,
            max: 4294967295,
        },
        constant_value: &constant_value,
    };
    for test in tests {
        if test.generated.is_some()
            || test.assertions.len() > MAX_ASSERTIONS
            || !bounded_syntax(&test.body, MAX_TEST_BYTES, MAX_UNARY_TOKENS, false)
            || test.attrs.iter().any(|attr| !attr.trim().eq("#[test]"))
        {
            continue;
        }
        let Some(test_facts) = find_file_facts(index, &test.file) else {
            continue;
        };
        let Some(masked) = bounded_masked(
            &test_facts.source,
            MAX_FILE_BYTES,
            MAX_FILE_UNARY_TOKENS,
            true,
        ) else {
            continue;
        };
        if !targeted_free_call_reaches_owner(test, owner, None, index, syntax) {
            continue;
        }
        let call_counts = owner_call_counts(&masked, test, &name);
        for oracle in &test.assertions {
            if !syntax.admits_tier_return_assertion(test, oracle, index) {
                continue;
            }
            if call_counts.get(&oracle.line) != Some(&1) {
                continue;
            }
            let Some(assertion) = bounded_assertion(&oracle.text) else {
                continue;
            };
            if !plain_assertion(&assertion, &name, now, old, &multipliers) {
                continue;
            }
            if machine.call_result(&assertion) != Some(Value::Bool(true)) {
                continue;
            }
            if !assertion_test_distinguishes(
                test,
                oracle,
                &name,
                (&new_reader, &old_reader, old.is_some()),
                (&constant_name, &constant_value),
                (index, syntax, &call_counts),
            ) {
                continue;
            }
            return Some(TierFlow {
                owner: name,
                literal: now,
                test: test.name.clone(),
                file: test.file.clone(),
                line: test.start_line,
                assertion: oracle.text.clone(),
                sink: sink_text.clone(),
                sink_line,
            });
        }
    }
    None
}

fn binding_initializer(statements: &[ast::Stmt]) -> Option<ast::Expr> {
    let binding = statements.iter().find_map(|statement| match statement {
        ast::Stmt::LetStmt(binding) => Some(binding),
        _ => None,
    })?;
    binding.initializer()
}
