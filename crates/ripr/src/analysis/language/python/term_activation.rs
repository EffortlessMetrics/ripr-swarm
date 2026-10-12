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
//! every such call resolves and every one evaluates the term to `0`; any
//! unresolved call, or no call at all, leaves the verdict to the other rules.

use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    PythonOwnerCallValues, literal_value, static_operand, strong_owner_call_values,
    unrebound_constants,
};
use super::no_behavior::split_top_level_args;
use super::related_tests::PythonRelatedCandidate;
use super::source_utils::text_for_range;
use super::{PythonOwner, PythonParameter};
use crate::domain::{OwnerKind, ValueContext, ValueFact};
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

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

/// The guarded-return shape of a function body, when it has exactly that
/// shape (a leading docstring is allowed).
pub(super) fn guarded_return(
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
        );
    let rest_intact = after.is_empty()
        || after.starts_with(['#', ')', ']', '}', ',', '+'])
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
    let callee = owners.iter().find(|candidate| {
        candidate.file == owner.file
            && candidate.owner_kind == Some(OwnerKind::Function)
            && candidate.name == term.callee
            && candidate.qualified_name == term.callee
    })?;
    let guard = callee.guarded_return.as_ref()?;
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
    let rows = strong_owner_call_values(owner, related_candidates, focus);
    if rows.is_empty() {
        return None;
    }
    let mut observed_values = Vec::new();
    for row in &rows {
        let value = argument_value(argument, row, &owner_constants)?;
        let taken = compare(&value, guard.operator, &operand)?;
        let result = if taken { &then_value } else { &else_value };
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
    let mut values: Vec<&str> = observed_values
        .iter()
        .map(|fact| fact.value.as_str())
        .collect();
    values.dedup();
    Some(PythonTermEvidence {
        discriminator: format!("{} != 0", term.text),
        reason: format!(
            "every strong related test call gives the changed term `{}` the value 0 ({}; `{}` returns {} when `{} {} {}`), so the asserted result is the same with or without it",
            term.text,
            values.join(", "),
            callee.name,
            guard.then_value,
            guard.parameter,
            guard.operator,
            guard.operand,
        ),
        observed_values,
    })
}

/// The literal a call argument takes in one owner call: a literal, an owner
/// parameter the call binds, or an unrebound owner-module constant.
fn argument_value(
    argument: &str,
    row: &PythonOwnerCallValues,
    constants: &[super::module_constants::PythonModuleConstant],
) -> Option<String> {
    literal_value(argument)
        .or_else(|| row.values.get(argument).cloned())
        .or_else(|| static_operand(argument, constants))
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

fn is_zero(value: &str) -> bool {
    integer(value) == Some(0)
        || value
            .parse::<f64>()
            .is_ok_and(|value| value == 0.0 && !value.is_sign_negative())
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
    }
}
