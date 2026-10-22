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
    /// `"1"` for `*` and `/`, `"0"` for `+` and `-`.
    pub(crate) neutral: &'static str,
    /// The operator as written.
    pub(crate) operator: char,
}

/// `return <operand> <op> <param>;` with `op` one of `* / + -`, a plain
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
    let operator_at = expression.find(['*', '/', '+', '-'])?;
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
        neutral: if matches!(operator, '*' | '/') {
            "1"
        } else {
            "0"
        },
        operator,
    })
}

fn is_plain_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}
