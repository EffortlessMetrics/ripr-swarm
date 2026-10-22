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
    let expression = masked
        .trim()
        .strip_prefix("return")
        .filter(|rest| rest.starts_with(char::is_whitespace))?
        .trim()
        .strip_suffix(';')?
        .trim();
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

/// `true` when `old` and `new` are both `return <operand> <op> <operand>;`
/// with the same commutative operator (`*` or `+`) over the same two
/// operands, in either order. Any other edit (a changed operator, a dropped
/// or replaced term) changes the value, so a test that passes the identity
/// can still pin it.
pub(crate) fn ts_operands_rewritten_in_place(old: &str, new: &str) -> bool {
    let (Some((old_ops, old_op)), Some((new_ops, new_op))) = (operands(old), operands(new)) else {
        return false;
    };
    let mut swapped = old_ops.clone();
    swapped.reverse();
    old_op == new_op && new_op == '*' && (new_ops == old_ops || new_ops == swapped)
}

fn operands(line: &str) -> Option<(Vec<String>, char)> {
    let masked = ts_mask_literals_and_comments(line.trim());
    let expression = masked
        .trim()
        .strip_prefix("return")
        .filter(|rest| rest.starts_with(char::is_whitespace))?
        .trim()
        .strip_suffix(';')?
        .trim();
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

fn is_plain_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}
