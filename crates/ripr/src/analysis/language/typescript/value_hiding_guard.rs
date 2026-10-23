//! RIPR-SPEC-0248 rules 2 and 3: an exact matcher that cannot tell the
//! changed return value apart.
//!
//! Rule 2: `return unit * qty` is checked by `scaled(1, q)` against `q`. With
//! `qty` fixed at the multiplicative identity, `unit / qty` returns the same
//! number, so the exact matcher passes on the operator and on its inverse.
//! Rule 3: `normalize(normalize(text))` against `normalize(text)` holds for
//! any constant the owner returns, so an assertion built from owner calls
//! alone cannot notice a changed return value.

use super::*;

/// The right operand of the changed line's one arithmetic operator, and the
/// value that operand takes when the operator's inverse gives the same result.
pub(crate) struct NeutralOperandChange {
    /// The owner parameter the right operand reads.
    pub(crate) parameter: String,
    /// `"1"`, the identity of `*` and `/`.
    pub(crate) neutral: &'static str,
    /// The operator as written.
    pub(crate) operator: char,
}

/// `return <operand> <op> <param>;` with `op` one of `*` or `/`, a plain
/// identifier or number on the left and an identifier on the right. Anything
/// else (a longer expression, a compound operator, a literal right operand)
/// is `None`, so the rule never reads a shape it cannot bound.
pub(crate) fn ts_neutral_operand_change(line_text: &str) -> Option<NeutralOperandChange> {
    let masked = ts_mask_literals_and_comments(line_text.trim());
    let expression = return_expression(&masked)?;
    let operator_at = expression.find(['*', '/'])?;
    let operator = expression[operator_at..].chars().next()?;
    let (left, right) = (
        expression[..operator_at].trim(),
        expression[operator_at + 1..].trim(),
    );
    if right.contains(['*', '/', '+', '-', '=']) || !is_plain_identifier(right) {
        return None;
    }
    if !(is_plain_identifier(left) || numeric_literal_value(left).is_some()) {
        return None;
    }
    Some(NeutralOperandChange {
        parameter: right.to_string(),
        neutral: "1",
        operator,
    })
}

/// `true` when the identity input for the new line's right operand also
/// hides the edit from `old`, so the edit changes the value only where that
/// operand is not the identity:
/// - the same commutative operator (`*`) over the same two operands,
///   in either order;
/// - the inverse operator over the same operands in the same order
///   (`a / b` against `a * b`);
/// - the old line is the new line's left operand alone (`return a;` against
///   `return a * b;`), an added factor or term.
///
/// Any other edit (a changed operand, a dropped term, an unrelated operator)
/// can be pinned by an identity input, so the credit stays.
pub(crate) fn ts_identity_hides_edit(old: &str, new: &str) -> bool {
    let Some((new_ops, new_op)) = operands(new) else {
        return false;
    };
    if let Some((old_ops, old_op)) = operands(old) {
        let mut swapped = old_ops.clone();
        swapped.reverse();
        let commutative =
            old_op == new_op && new_op == '*' && (new_ops == old_ops || new_ops == swapped);
        let inverse = new_ops == old_ops && matches!((old_op, new_op), ('/', '*') | ('*', '/'));
        return commutative || inverse;
    }
    single_operand(old).is_some_and(|operand| new_ops.first() == Some(&operand))
}

/// `return <operand>;` with a plain identifier or number.
fn single_operand(line: &str) -> Option<String> {
    let masked = ts_mask_literals_and_comments(line.trim());
    let operand = return_expression(&masked)?;
    (is_plain_identifier(operand) || numeric_literal_value(operand).is_some())
        .then(|| operand.to_string())
}

/// The expression of `return <expression>[;]`, whether or not the statement
/// ends in a semicolon.
fn return_expression(masked: &str) -> Option<&str> {
    let rest = masked
        .trim()
        .strip_prefix("return")
        .filter(|rest| rest.starts_with(char::is_whitespace))?
        .trim();
    Some(rest.strip_suffix(';').unwrap_or(rest).trim())
}

fn operands(line: &str) -> Option<(Vec<String>, char)> {
    let masked = ts_mask_literals_and_comments(line.trim());
    let expression = return_expression(&masked)?;
    let at = expression.find(['*', '/', '+', '-'])?;
    let operator = expression[at..].chars().next()?;
    let (left, right) = (expression[..at].trim(), expression[at + 1..].trim());
    if [left, right].iter().any(|side| {
        side.contains(['*', '/', '+', '-', '='])
            || !(is_plain_identifier(side) || numeric_literal_value(side).is_some())
    }) {
        return None;
    }
    Some((vec![left.to_string(), right.to_string()], operator))
}

pub(crate) fn is_plain_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}
