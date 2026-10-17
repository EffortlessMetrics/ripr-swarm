//! Removed-term activation evidence for the Python preview adapter (#6733).
//!
//! A change that removes (or adds) one additive call term
//! (`total + shipping_fee(subtotal_cents)` -> `total`) changes the owner's
//! result only for inputs where that term is nonzero. A strong exact-value
//! assertion whose every call gives the term the value `0` passes before and
//! after the change, so reach plus a strong oracle does not discriminate it.
//!
//! The rule reads one call deep, and only one callee shape: a same-file
//! module function whose body is a single guarded return,
//!
//! ```python
//! def shipping_fee(subtotal_cents):
//!     if subtotal_cents >= FREE_SHIPPING_CENTS:
//!         return 0
//!     return FLAT_SHIPPING_CENTS
//! ```
//!
//! where the guard compares one parameter with a literal or an unrebound
//! module constant and both returns are literals or such constants. The
//! callee's guarded parameter must take a value the boundary rule already
//! resolves (`boundary.rs`): a literal argument, or an owner parameter that a
//! strong related call binds to a literal. The term is inactive only when
//! every strong related call a strong assertion observes resolves and every
//! one evaluates the term to `0`. A discarded expression statement whose
//! owner does not store the changed expression is ignored (#7562). Any
//! unresolved call, unclear observation, or no observed call at all, leaves
//! the verdict to the other rules.

use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    PythonOwnerCallValues, literal_value, static_operand, strong_owner_call_values,
    unrebound_constants,
};
use super::effect_alignment::ChangedEffect;
use super::module_constants::{body_bound_names, mentions_identifier};
use super::no_behavior::split_top_level_args;
use super::related_tests::{PythonRelatedCandidate, python_text_hides_code};
use super::source_utils::{SourceText, line_for_range_end, line_for_range_start, text_for_range};
use super::static_limits::has_identifier_boundary;
use super::{PythonAssertion, PythonOwner, PythonParameter, PythonTest, expr_full_name};
use crate::domain::{OracleStrength, OwnerKind, ValueContext, ValueFact};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use rustpython_parser::text_size::TextSize;
use rustpython_parser::{Mode, parse};

/// A function body that is one guarded return of literals:
/// `if <parameter> <op> <operand>: return <then>` then `return <else>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonGuardedReturn {
    parameter: String,
    operator: &'static str,
    operand: String,
    then_value: String,
    else_value: String,
}

/// What a function owner's body lets rule 20 read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PythonTermFacts {
    /// As a callee: its one guarded return, when the function is undecorated
    /// and synchronous (a decorator or `async def` changes what a call returns).
    guarded_return: Option<PythonGuardedReturn>,
    /// As a caller: the parameters that hold their call-site value on every
    /// line of the body. Empty when the body has a nested scope, a lambda, a
    /// comprehension or a walrus, or binds names the walk cannot enumerate.
    stable_parameters: Vec<String>,
    /// As a caller: the names its body binds, so a callee name it shadows is
    /// not read as the module function.
    bound_names: Option<Vec<String>>,
    /// The parameter the function hands back: its one `return` is the last
    /// statement, `return <parameter>`, the parameter is never rebound, and
    /// the function is not a generator and has no decorator other than
    /// `@classmethod` or `@staticmethod`, which only bind the call. A caller's
    /// result is then the very object it passed (RIPR-SPEC-0238 rule 17,
    /// #7586, #7634).
    returned_parameter: Option<String>,
}

impl PythonTermFacts {
    pub(super) fn returned_parameter(&self) -> Option<&str> {
        self.returned_parameter.as_deref()
    }
}

/// The rule 20 facts of a function owner.
pub(super) fn term_facts(
    source: &str,
    body: &[Stmt],
    parameters: &[PythonParameter],
    owner_text: &str,
    wrapped: bool,
    // Synchronous, with every decorator `@classmethod` or `@staticmethod`.
    binding_only: bool,
) -> PythonTermFacts {
    let bound_names = body_bound_names(body);
    let opaque_scope = ["lambda", "for", "async", "await", ":="]
        .into_iter()
        .any(|keyword| code_mentions(owner_text, keyword));
    let stable_parameters = match (&bound_names, opaque_scope) {
        (Some(bound), false) => parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .filter(|name| !bound.contains(name))
            .collect(),
        _ => Vec::new(),
    };
    let returned_parameter = match body.last() {
        Some(Stmt::Return(ast::StmtReturn {
            value: Some(value), ..
        })) if binding_only
            && code_mention_count(owner_text, "return") == 1
            && code_mention_count(owner_text, "yield") == 0 =>
        {
            match value.as_ref() {
                Expr::Name(name) if stable_parameters.iter().any(|p| p == name.id.as_str()) => {
                    Some(name.id.to_string())
                }
                _ => None,
            }
        }
        _ => None,
    };
    PythonTermFacts {
        guarded_return: (!wrapped)
            .then(|| guarded_return(source, body, parameters))
            .flatten(),
        stable_parameters,
        bound_names,
        returned_parameter,
    }
}

/// Whether `text` holds `token` as code, not in a string, docstring or
/// comment; a word token must stand as a whole identifier.
fn code_mentions(text: &str, token: &str) -> bool {
    code_mention_count(text, token) > 0
}

/// How many times `text` holds `token` as code (see `code_mentions`).
fn code_mention_count(text: &str, token: &str) -> usize {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let word = token.chars().all(is_word);
    text.match_indices(token)
        .filter(|(start, _)| {
            let (start, end) = (*start, start + token.len());
            let whole = !word
                || (!text[..start].chars().next_back().is_some_and(is_word)
                    && !text[end..].chars().next().is_some_and(is_word));
            whole && !python_text_hides_code(text, start)
        })
        .count()
}

/// The guarded-return shape of a function body, when it has exactly that
/// shape (a leading docstring is allowed).
fn guarded_return(
    source: &str,
    body: &[Stmt],
    parameters: &[PythonParameter],
) -> Option<PythonGuardedReturn> {
    let body = match body.first() {
        Some(Stmt::Expr(expr))
            if matches!(
                expr.value.as_ref(),
                Expr::Constant(ast::ExprConstant {
                    value: ast::Constant::Str(_),
                    ..
                })
            ) =>
        {
            &body[1..]
        }
        _ => body,
    };
    let [Stmt::If(guard), Stmt::Return(fallback)] = body else {
        return None;
    };
    let [Stmt::Return(early)] = guard.body.as_slice() else {
        return None;
    };
    if !guard.orelse.is_empty() {
        return None;
    }
    let Expr::Compare(compare) = guard.test.as_ref() else {
        return None;
    };
    let ([operator], [operand]) = (compare.ops.as_slice(), compare.comparators.as_slice()) else {
        return None;
    };
    let Expr::Name(parameter) = compare.left.as_ref() else {
        return None;
    };
    let parameter = parameter.id.to_string();
    if !parameters
        .iter()
        .any(|declared| declared.name == parameter && !declared.keyword_only)
    {
        return None;
    }
    let operator = match operator {
        ast::CmpOp::Lt => "<",
        ast::CmpOp::LtE => "<=",
        ast::CmpOp::Gt => ">",
        ast::CmpOp::GtE => ">=",
        ast::CmpOp::Eq => "==",
        ast::CmpOp::NotEq => "!=",
        _ => return None,
    };
    let value_text = |value: &Option<Box<Expr>>| {
        value
            .as_deref()
            .map(|value| text_for_range(source, value.range()).trim().to_string())
    };
    Some(PythonGuardedReturn {
        parameter,
        operator,
        operand: text_for_range(source, operand.range()).trim().to_string(),
        then_value: value_text(&early.value)?,
        else_value: value_text(&fallback.value)?,
    })
}

/// The one additive call term a change removes or adds.
#[derive(Debug, PartialEq, Eq)]
struct CallTerm<'a> {
    callee: &'a str,
    arguments: Vec<&'a str>,
    text: &'a str,
}

/// The additive call term (`+ f(args)` or `- f(args)`) that is the whole
/// difference between the old and the new line, removed or added. The term
/// must sit where dropping it leaves the rest of the expression intact: after
/// an operand and before a closing bracket, a comma, another additive
/// operator, a comment or the end of the line (`x - f(y) * 2` is not one).
fn changed_call_term<'a>(old_line: &'a str, new_line: &'a str) -> Option<CallTerm<'a>> {
    let prefix: usize = old_line
        .chars()
        .zip(new_line.chars())
        .take_while(|(old, new)| old == new)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let (old_rest, new_rest) = (old_line.get(prefix..)?, new_line.get(prefix..)?);
    let suffix: usize = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(old, new)| old == new)
        .map(|(ch, _)| ch.len_utf8())
        .sum();
    let old_middle = old_rest.get(..old_rest.len().checked_sub(suffix)?)?;
    let new_middle = new_rest.get(..new_rest.len().checked_sub(suffix)?)?;
    let (longer, middle) = match (old_middle.trim().is_empty(), new_middle.trim().is_empty()) {
        (false, true) => (old_line, old_middle),
        (true, false) => (new_line, new_middle),
        _ => return None,
    };
    // A term inside a string or a comment is not live code, and a term in a
    // `def` header sits in a default value, read in the enclosing scope.
    let sign = prefix + (middle.len() - middle.trim_start().len());
    if python_text_hides_code(longer, sign) || longer.trim_start().starts_with("def ") {
        return None;
    }
    let before = longer.get(..prefix)?.trim_end();
    let after = longer.get(prefix + middle.len()..)?.trim_start();
    let last_word = before
        .rsplit(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .next()
        .unwrap_or_default();
    let operand_ends = before
        .chars()
        .last()
        .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | ')' | ']' | '"' | '\''))
        && !matches!(
            last_word,
            "return"
                | "yield"
                | "in"
                | "is"
                | "and"
                | "or"
                | "not"
                | "if"
                | "else"
                | "lambda"
                | "await"
                | "raise"
                | "assert"
                | "del"
                | "except"
        );
    // At the end of the line the expression must end too: an open bracket or
    // a `\` continuation can carry a `* 2` on the next line.
    let line_ends_expression =
        bracket_depth(longer) == Some(0) && !longer.trim_end().ends_with('\\');
    let rest_intact = ((after.is_empty() || after.starts_with('#')) && line_ends_expression)
        || after.starts_with([')', ']', '}', ',', '+'])
        || (after.starts_with('-') && !after.starts_with("->"));
    if !operand_ends || !rest_intact {
        return None;
    }
    let term = middle.trim();
    let call = term.strip_prefix(['+', '-'])?.trim_start();
    let open = call.find('(')?;
    let callee = &call[..open];
    if callee.is_empty() || !callee.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
        return None;
    }
    let arguments = call[open + 1..].strip_suffix(')')?;
    // The closing parenthesis must be the call's own.
    let mut depth = 0usize;
    for ch in arguments.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let arguments = split_top_level_args(arguments)
        .into_iter()
        .map(str::trim)
        .filter(|argument| !argument.is_empty())
        .collect();
    Some(CallTerm {
        callee,
        arguments,
        text: call,
    })
}

/// The bracket depth at the end of a line, outside string literals and a
/// trailing comment, or None when it closes more than it opens or ends
/// inside a string.
fn bracket_depth(line: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in line.chars() {
        match quote {
            Some(_) if escaped => escaped = false,
            Some(_) if ch == '\\' => escaped = true,
            Some(open) if ch == open => quote = None,
            Some(_) => {}
            None => match ch {
                '#' => break,
                '"' | '\'' => quote = Some(ch),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.checked_sub(1)?,
                _ => {}
            },
        }
    }
    quote.is_none().then_some(depth)
}

/// Evidence that no strong related call activates the changed term.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PythonTermEvidence {
    /// The equality that would activate the term (`shipping_fee(subtotal_cents) != 0`).
    pub(super) discriminator: String,
    pub(super) observed_values: Vec<ValueFact>,
    pub(super) reason: String,
}

/// Inactive-term evidence for a changed line, or None when the change is not
/// one additive call term of the supported callee shape, or when any strong
/// related call leaves the term's value unresolved or nonzero.
pub(super) fn python_inactive_term_evidence(
    old_line: &str,
    new_line: &str,
    owner: &PythonOwner,
    owners: &[PythonOwner],
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonTermEvidence> {
    if owner.owner_kind != Some(OwnerKind::Function) {
        return None;
    }
    let term = changed_call_term(old_line, new_line)?;
    // The callee is the file's one definition of that name, the owner does
    // not rebind the name, and no related test may replace it.
    let mut definitions = owners.iter().filter(|candidate| {
        candidate.file == owner.file
            && candidate.name == term.callee
            && candidate.qualified_name == term.callee
    });
    let callee = definitions.next()?;
    if definitions.next().is_some()
        || callee.owner_kind != Some(OwnerKind::Function)
        || owner
            .parameters
            .iter()
            .any(|parameter| parameter.name == term.callee)
        || owner
            .term_facts
            .bound_names
            .as_ref()
            .is_none_or(|bound| bound.iter().any(|name| name == term.callee))
        || related_candidates
            .iter()
            .any(|candidate| test_may_replace(candidate.test, term.callee))
    {
        return None;
    }
    let guard = callee.term_facts.guarded_return.as_ref()?;
    let position = callee
        .parameters
        .iter()
        .filter(|parameter| !parameter.keyword_only)
        .position(|parameter| parameter.name == guard.parameter)?;
    if term.arguments.len() != callee.parameters.len()
        || term
            .arguments
            .iter()
            .any(|argument| argument.starts_with('*') || argument.contains('='))
    {
        return None;
    }
    let argument = *term.arguments.get(position)?;
    let callee_constants = unrebound_constants(callee, related_candidates);
    let owner_constants = unrebound_constants(owner, related_candidates);
    let operand = static_operand(&guard.operand, &callee_constants)?;
    let then_value = static_operand(&guard.then_value, &callee_constants)?;
    let else_value = static_operand(&guard.else_value, &callee_constants)?;
    let rows = strong_owner_call_values(owner, related_candidates, focus)?;
    let rows = observed_term_call_rows(owner, related_candidates, focus, old_line, new_line, rows)?;
    if rows.is_empty() {
        return None;
    }
    let mut observed_values = Vec::new();
    let mut branches = std::collections::BTreeSet::new();
    for row in &rows {
        let value = argument_value(
            argument,
            row,
            &owner.term_facts.stable_parameters,
            &owner_constants,
        )?;
        let taken = compare(&value, guard.operator, &operand)?;
        let result = if taken { &then_value } else { &else_value };
        branches.insert(taken);
        if !is_zero(result) {
            return None;
        }
        observed_values.push(ValueFact {
            line: row.line,
            text: row.text.clone(),
            value: format!("{argument} = {value}"),
            context: ValueContext::FunctionArgument,
        });
    }
    observed_values.sort_by(|a, b| a.line.cmp(&b.line).then(a.value.cmp(&b.value)));
    observed_values.dedup();
    let values: Vec<&str> = observed_values
        .iter()
        .map(|fact| fact.value.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let condition = format!("{} {} {}", guard.parameter, guard.operator, guard.operand);
    let branch = branches
        .iter()
        .map(|taken| {
            if *taken {
                format!("returns {} when `{condition}`", guard.then_value)
            } else {
                format!("returns {} when not `{condition}`", guard.else_value)
            }
        })
        .collect::<Vec<_>>()
        .join(" and ");
    Some(PythonTermEvidence {
        discriminator: format!("{} != 0", term.text),
        reason: format!(
            "static evidence gives the changed term `{}` the value 0 in every strong related test call a strong assertion observes ({}; `{}` {branch}), so those calls read the same asserted result with or without it",
            term.text,
            values.join(", "),
            callee.name,
        ),
        observed_values,
    })
}

/// Keep only owner-call rows a strong assertion observes (#7562).
///
/// A discarded expression statement whose owner has no side-effecting store
/// of the changed expression is ignored. Any row whose observation is unclear
/// blocks the downgrade, as does an empty observed set.
fn observed_term_call_rows(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
    old_line: &str,
    new_line: &str,
    rows: Vec<PythonOwnerCallValues>,
) -> Option<Vec<PythonOwnerCallValues>> {
    let no_store = return_change_has_no_effect_store(old_line, new_line, owner);
    let mut observed = Vec::new();
    for row in rows {
        match call_observation(owner, related_candidates, focus, &row, no_store) {
            CallObservation::Observed => observed.push(row),
            CallObservation::Discarded => {}
            CallObservation::Unclear => return None,
        }
    }
    (!observed.is_empty()).then_some(observed)
}

/// How a strong related owner-call row is observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallObservation {
    /// The call sits in a strong assertion, or its result local is read by one.
    Observed,
    /// A top-level expression statement whose result is discarded, and the
    /// owner does not store the changed expression.
    Discarded,
    /// Nested, assigned-but-unread, passed on, or otherwise not established unused.
    Unclear,
}

fn call_observation(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
    row: &PythonOwnerCallValues,
    no_store: bool,
) -> CallObservation {
    let Some(candidate) = candidate_for_row(related_candidates, row.line) else {
        return CallObservation::Unclear;
    };
    let strong = strong_admitted_assertions(candidate.test, focus);
    if strong.iter().any(|assertion| assertion.line == row.line) {
        return CallObservation::Observed;
    }
    classify_body_call(owner, candidate.test, &strong, row, no_store)
        .unwrap_or(CallObservation::Unclear)
}

fn candidate_for_row<'a>(
    related_candidates: &'a [PythonRelatedCandidate<'a>],
    line: usize,
) -> Option<&'a PythonRelatedCandidate<'a>> {
    related_candidates.iter().find(|candidate| {
        let start = candidate.test.line;
        let end = start + candidate.test.body_text.lines().count().saturating_sub(1);
        line >= start && line <= end
    })
}

fn strong_admitted_assertions<'a>(
    test: &'a PythonTest,
    focus: &PythonAssertionFocus,
) -> Vec<&'a PythonAssertion> {
    test.assertions
        .iter()
        .filter(|assertion| {
            focus.admits(assertion)
                && assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
        })
        .collect()
}

fn classify_body_call(
    owner: &PythonOwner,
    test: &PythonTest,
    strong: &[&PythonAssertion],
    row: &PythonOwnerCallValues,
    no_store: bool,
) -> Option<CallObservation> {
    let source = dedent_python(&test.body_text);
    let Mod::Module(module) = parse(&source, Mode::Module, "<python-term-call-body>").ok()? else {
        return None;
    };
    let body = test_function_body(module.body)?;
    let parsed = SourceText::new(&source);
    let source_line = row.line.checked_sub(test.line)? + 1;
    let offset = offset_of_line_code(&source, source_line)?;
    let covering = body
        .iter()
        .find(|stmt| range_contains(stmt.range(), offset))?;
    if strong_assertion_covers_stmt(covering, strong, test, &parsed) {
        return Some(CallObservation::Observed);
    }
    Some(match covering {
        Stmt::Expr(expr) if is_owner_call_expr(expr.value.as_ref(), owner, test) => {
            if no_store {
                CallObservation::Discarded
            } else {
                CallObservation::Unclear
            }
        }
        Stmt::Assign(assign) => bound_owner_result(assign, owner, test, strong, row.line),
        Stmt::AnnAssign(assign) => match (assign.target.as_ref(), assign.value.as_deref()) {
            (Expr::Name(name), Some(value)) if is_owner_call_expr(value, owner, test) => {
                later_strong_assertion_reads(strong, row.line, name.id.as_str())
            }
            _ => CallObservation::Unclear,
        },
        _ => CallObservation::Unclear,
    })
}

fn test_function_body(body: Vec<Stmt>) -> Option<Vec<Stmt>> {
    match body.len() {
        1 => match body.into_iter().next()? {
            Stmt::FunctionDef(function) => Some(function.body),
            Stmt::AsyncFunctionDef(function) => Some(function.body),
            other => Some(vec![other]),
        },
        _ => Some(body),
    }
}

fn strong_assertion_covers_stmt(
    stmt: &Stmt,
    strong: &[&PythonAssertion],
    test: &PythonTest,
    parsed: &SourceText<'_>,
) -> bool {
    let start = test.line + line_for_range_start(parsed, stmt.range()) - 1;
    let end = test.line + line_for_range_end(parsed, stmt.range()) - 1;
    strong
        .iter()
        .any(|assertion| assertion.line >= start && assertion.line <= end)
}

fn bound_owner_result(
    assign: &ast::StmtAssign,
    owner: &PythonOwner,
    test: &PythonTest,
    strong: &[&PythonAssertion],
    row_line: usize,
) -> CallObservation {
    let [target] = assign.targets.as_slice() else {
        return CallObservation::Unclear;
    };
    let Expr::Name(name) = target else {
        return CallObservation::Unclear;
    };
    if !is_owner_call_expr(assign.value.as_ref(), owner, test) {
        return CallObservation::Unclear;
    }
    later_strong_assertion_reads(strong, row_line, name.id.as_str())
}

fn later_strong_assertion_reads(
    strong: &[&PythonAssertion],
    row_line: usize,
    local: &str,
) -> CallObservation {
    if strong.iter().any(|assertion| {
        assertion.line > row_line && assertion_text_observes_token(&assertion.gate_text, local)
    }) {
        CallObservation::Observed
    } else {
        CallObservation::Unclear
    }
}

fn is_owner_call_expr(expr: &Expr, owner: &PythonOwner, test: &PythonTest) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };
    let Some(name) = expr_full_name(call.func.as_ref()) else {
        return false;
    };
    name == owner.name
        || test
            .imports
            .iter()
            .any(|import| import.imported == owner.name && import.alias == name)
        || name
            .rsplit_once('.')
            .is_some_and(|(_, last)| last == owner.name)
}

fn assertion_text_observes_token(text: &str, token: &str) -> bool {
    !token.is_empty()
        && text
            .match_indices(token)
            .any(|(idx, _)| has_identifier_boundary(text, idx, token.len()))
}

fn return_change_has_no_effect_store(old_line: &str, new_line: &str, owner: &PythonOwner) -> bool {
    ChangedEffect::of(old_line, Some(new_line), owner).is_none()
        && ChangedEffect::of(new_line, Some(old_line), owner).is_none()
        && old_line.trim().starts_with("return")
        && new_line.trim().starts_with("return")
}

fn offset_of_line_code(source: &str, line: usize) -> Option<TextSize> {
    if line == 0 {
        return None;
    }
    let start = if line == 1 {
        0
    } else {
        source
            .match_indices('\n')
            .nth(line - 2)
            .map(|(idx, _)| idx + 1)?
    };
    let rest = source.get(start..)?;
    let indent = rest
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .map(char::len_utf8)
        .sum::<usize>();
    u32::try_from(start.checked_add(indent)?)
        .ok()
        .map(TextSize::from)
}

fn range_contains(range: rustpython_parser::text_size::TextRange, offset: TextSize) -> bool {
    range.start() <= offset && offset < range.end()
}

fn dedent_python(source: &str) -> String {
    let min_indent = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    if min_indent == 0 {
        return source.to_string();
    }
    source
        .lines()
        .map(|line| line.get(min_indent..).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The literal a call argument takes in one owner call: a literal, an owner
/// parameter the call binds and the body never rebinds, or an unrebound
/// owner-module constant.
fn argument_value(
    argument: &str,
    row: &PythonOwnerCallValues,
    stable_parameters: &[String],
    constants: &[super::module_constants::PythonModuleConstant],
) -> Option<String> {
    if stable_parameters.iter().any(|name| name == argument) {
        return row.values.get(argument).cloned();
    }
    literal_value(argument).or_else(|| static_operand(argument, constants))
}

/// Whether a related test may replace the callee at runtime: it patches,
/// `setattr`s or assigns an attribute of that name, or its file writes a
/// dynamic namespace.
fn test_may_replace(test: &super::PythonTest, callee: &str) -> bool {
    test.constant_rebinding.assigns_attribute(callee)
        || test
            .decorators
            .iter()
            .any(|decorator| decorator.contains("patch"))
        || (mentions_identifier(&test.body_text, callee)
            && ["patch", "setattr", "monkeypatch"]
                .into_iter()
                .any(|writer| mentions_identifier(&test.body_text, writer)))
}

fn integer(value: &str) -> Option<i128> {
    value.replace('_', "").parse().ok()
}

fn compare(left: &str, operator: &str, right: &str) -> Option<bool> {
    let (left, right) = (integer(left)?, integer(right)?);
    Some(match operator {
        "<" => left < right,
        "<=" => left <= right,
        ">" => left > right,
        ">=" => left >= right,
        "==" => left == right,
        "!=" => left != right,
        _ => return None,
    })
}

/// Integer zero only: adding `0.0` to an integer changes its type.
fn is_zero(value: &str) -> bool {
    integer(value) == Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removed_or_added_additive_call_term_is_read() {
        let old = "    return {\"total_cents\": total + shipping_fee(subtotal_cents)}";
        let new = "    return {\"total_cents\": total}";
        let term = changed_call_term(old, new);
        assert_eq!(
            term,
            Some(CallTerm {
                callee: "shipping_fee",
                arguments: vec!["subtotal_cents"],
                text: "shipping_fee(subtotal_cents)",
            })
        );
        assert_eq!(changed_call_term(new, old), term);
        assert!(changed_call_term("    return a - fee(x, 2)", "    return a").is_some());
    }

    #[test]
    fn a_term_whose_removal_changes_the_rest_of_the_expression_is_not_read() {
        // Precedence: `x - f(y) * 2` without `- f(y)` is `x * 2`.
        assert_eq!(
            changed_call_term("    return x - f(y) * 2", "    return x * 2"),
            None
        );
        // A unary sign is not an additive term.
        assert_eq!(changed_call_term("    return -f(y)", "    return "), None);
        // A changed argument is not a removed term.
        assert_eq!(
            changed_call_term("    return x + f(y)", "    return x + f(z)"),
            None
        );
        // A method call or a nested call chain is not a bare callee.
        assert_eq!(
            changed_call_term("    return x + a.f(y)", "    return x"),
            None
        );
        assert_eq!(
            changed_call_term("    return x + f(y)(z)", "    return x"),
            None
        );
        // The next line may hold a higher-precedence operator.
        assert_eq!(
            changed_call_term("    return (x + f(y)", "    return (x"),
            None
        );
        assert_eq!(
            changed_call_term("    return x + f(y) \\", "    return x \\"),
            None
        );
        // Inside a string or a comment the term is not code.
        assert_eq!(
            changed_call_term("    return \"x + f(y), ok\"", "    return \"x, ok\""),
            None
        );
        assert_eq!(
            changed_call_term("    return x  # x + f(y)", "    return x  # x"),
            None
        );
        // A default value is read in the enclosing scope.
        assert_eq!(
            changed_call_term("def g(x, y=B + f(x)):", "def g(x, y=B):"),
            None
        );
        assert_eq!(changed_call_term("    raise -f(y)", "    raise "), None);
    }

    #[test]
    fn a_keyword_in_a_docstring_or_comment_is_not_code() {
        let text = "def quote(s):\n    \"\"\"Total for an order.\"\"\"\n    # lambda for later\n    return s\n";
        assert!(!code_mentions(text, "for"));
        assert!(!code_mentions(text, "lambda"));
        assert!(code_mentions("    return [s for s in x]\n", "for"));
        assert!(code_mentions("    return (y := s)\n", ":="));
        assert!(!code_mentions("    return before\n", "for"));
    }

    #[test]
    fn integer_zero_only() {
        assert!(is_zero("0"));
        assert!(!is_zero("0.0"));
        assert!(!is_zero("499"));
    }
}
