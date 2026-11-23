//! A condition whose only edit swaps one `and` for `or`, or back (#16203).
//!
//! `return a > 0 and b > 0` to `return a > 0 or b > 0` gives a different
//! result only for inputs where the two operands disagree. A strong
//! exact-value assertion whose every call makes both operands true, or both
//! false, passes before and after the edit, so reach plus a strong oracle does
//! not discriminate it.
//!
//! The rule reads one line: `return <A> <op> <B>` or `if`/`elif <A> <op> <B>:`,
//! where `<op>` is the only top-level `and`/`or` and each operand is a
//! parameter (in a condition only: `return a or b` returns an operand, not
//! its truth value), `not` a parameter, or one comparison between a
//! parameter, a literal or an unrebound module constant. Operand values come from the
//! strong related calls the boundary rule reads (`boundary.rs`). Any call that
//! does not resolve both operands, a parameter the body may rebind, or no call
//! at all, leaves the verdict to the other rules.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    PythonOwnerCallValues, literals_equal, static_operand, strong_owner_call_values,
    unrebound_constants,
};
use super::discriminators::python_string_literal_value;
use super::module_constants::PythonModuleConstant;
use super::related_tests::PythonRelatedCandidate;
use crate::domain::OwnerKind;
use rustpython_parser::{Mode, Tok, lexer::lex};

/// The swapped condition, and what the strong related calls make of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PythonConnectiveEvidence {
    /// The two operands, as spelled.
    pub(super) left: String,
    pub(super) right: String,
    /// The value both operands take in every strong related call.
    pub(super) agreed: Vec<bool>,
}

type Token = (Tok, String);

/// The two operands of a changed line whose only edit swaps its one
/// top-level `and`/`or`, as spelled; None for anything else.
pub(super) fn swapped_connective(old_line: &str, new_line: &str) -> Option<(String, String)> {
    // A line that also moves out of (or into) a block changes which code
    // runs around it, which operand agreement says nothing about.
    let indent = |line: &str| line.len() - line.trim_start().len();
    if old_line.get(..indent(old_line)) != new_line.get(..indent(new_line)) {
        return None;
    }
    let old = spellings(old_line)?;
    let new = spellings(new_line)?;
    if old.len() != new.len() {
        return None;
    }
    let differing: Vec<usize> = (0..new.len()).filter(|&i| old[i].0 != new[i].0).collect();
    let [index] = differing.as_slice() else {
        return None;
    };
    let swapped = matches!(
        (&old[*index].0, &new[*index].0),
        (Tok::And, Tok::Or) | (Tok::Or, Tok::And)
    );
    if !swapped || old.iter().zip(&new).any(|(a, b)| a.1 != b.1 && a.0 == b.0) {
        return None;
    }
    let (body, colon) = match new.first().map(|(tok, _)| tok) {
        Some(Tok::Return) => (&new[1..], false),
        Some(Tok::If | Tok::Elif) => (&new[1..], true),
        _ => return None,
    };
    let body = if colon {
        match body.split_last() {
            Some(((Tok::Colon, _), rest)) => rest,
            _ => return None,
        }
    } else {
        body
    };
    let split = index - 1;
    let (left, right) = (&body[..split], &body[split + 1..]);
    // `and`/`or` return one operand, not a bool: `return a and b` gives `b`
    // and `return a or b` gives `a` when both are truthy. A `return` swap
    // reads only operands that are always bool (`not name`, a comparison).
    let bare_name = |part: &[Token]| matches!(part, [(Tok::Name { .. }, _)]);
    if !colon && (bare_name(left) || bare_name(right)) {
        return None;
    }
    Some((simple_operand(left)?, simple_operand(right)?))
}

/// Evidence that every strong related call gives both operands the same
/// truth value, or None when any call disagrees or does not resolve.
pub(super) fn python_connective_evidence(
    old_line: &str,
    new_line: &str,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonConnectiveEvidence> {
    if !matches!(
        owner.owner_kind,
        Some(OwnerKind::Function | OwnerKind::Method | OwnerKind::ClassMethod)
    ) {
        return None;
    }
    let (left, right) = swapped_connective(old_line, new_line)?;
    let rows = strong_owner_call_values(owner, related_candidates, focus)?;
    if rows.is_empty() {
        return None;
    }
    let constants = unrebound_constants(owner, related_candidates);
    let mut agreed = Vec::new();
    for row in &rows {
        let a = operand_truth(&left, row, owner, &constants)?;
        let b = operand_truth(&right, row, owner, &constants)?;
        if a != b {
            return None;
        }
        agreed.push(a);
    }
    Some(PythonConnectiveEvidence {
        left,
        right,
        agreed,
    })
}

/// Each token of one trimmed line with its source spelling; None when the
/// line does not lex on its own or holds a comment.
fn spellings(line: &str) -> Option<Vec<Token>> {
    let text = line.trim();
    let mut tokens = Vec::new();
    let mut code_end = 0usize;
    for result in lex(text, Mode::Module) {
        let (tok, range) = result.ok()?;
        if tok == Tok::Newline {
            continue;
        }
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        code_end = code_end.max(end);
        tokens.push((tok, text.get(start..end)?.to_string()));
    }
    text.get(code_end..)
        .is_some_and(|rest| rest.trim().is_empty())
        .then_some(tokens)
}

/// One operand the rule can evaluate, as spelled: `name`, `not name`, or
/// `x <cmp> y` with each side a name or a scalar literal (a sign allowed).
fn simple_operand(tokens: &[Token]) -> Option<String> {
    let side = |part: &[Token]| -> bool {
        matches!(
            part,
            [(Tok::Name { .. }, _)]
                | [(
                    Tok::Int { .. }
                        | Tok::Float { .. }
                        | Tok::String { .. }
                        | Tok::True
                        | Tok::False
                        | Tok::None,
                    _
                )]
                | [(Tok::Minus, _), (Tok::Int { .. } | Tok::Float { .. }, _)]
        )
    };
    let ok = match tokens {
        [(Tok::Name { .. }, _)] | [(Tok::Not, _), (Tok::Name { .. }, _)] => true,
        _ => tokens
            .iter()
            .position(|(tok, _)| comparison(tok).is_some())
            .is_some_and(|at| side(&tokens[..at]) && side(&tokens[at + 1..])),
    };
    ok.then(|| {
        tokens
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn comparison(tok: &Tok) -> Option<&'static str> {
    Some(match tok {
        Tok::Less => "<",
        Tok::LessEqual => "<=",
        Tok::Greater => ">",
        Tok::GreaterEqual => ">=",
        Tok::EqEqual => "==",
        Tok::NotEqual => "!=",
        _ => return None,
    })
}

/// The truth value one operand takes in one call row.
fn operand_truth(
    operand: &str,
    row: &PythonOwnerCallValues,
    owner: &PythonOwner,
    constants: &[PythonModuleConstant],
) -> Option<bool> {
    let tokens = spellings(operand)?;
    match tokens.as_slice() {
        [(Tok::Name { .. }, name)] => truthy(&value_of(name, row, owner, constants)?),
        [(Tok::Not, _), (Tok::Name { .. }, name)] => {
            truthy(&value_of(name, row, owner, constants)?).map(|value| !value)
        }
        _ => {
            let at = tokens
                .iter()
                .position(|(tok, _)| comparison(tok).is_some())?;
            let operator = comparison(&tokens[at].0)?;
            let side = |part: &[Token]| -> Option<String> {
                let text = part
                    .iter()
                    .map(|(_, text)| text.as_str())
                    .collect::<String>();
                match part {
                    [(Tok::Name { .. }, name)] => value_of(name, row, owner, constants),
                    _ => static_operand(&text, constants),
                }
            };
            let left = side(&tokens[..at])?;
            let right = side(&tokens[at + 1..])?;
            compare(&left, operator, &right)
        }
    }
}

/// A name's literal value in one call row: a module constant, or an owner
/// parameter the body never rebinds.
fn value_of(
    name: &str,
    row: &PythonOwnerCallValues,
    owner: &PythonOwner,
    constants: &[PythonModuleConstant],
) -> Option<String> {
    if owner
        .parameters
        .iter()
        .any(|parameter| parameter.name == name)
    {
        if !owner.term_facts.is_stable_parameter(name) {
            return None;
        }
        return row.values.get(name).cloned();
    }
    static_operand(name, constants)
}

/// Python truthiness of a scalar literal the rule can read.
pub(super) fn truthy(value: &str) -> Option<bool> {
    match value {
        "True" => Some(true),
        "False" | "None" => Some(false),
        _ => {
            if decimal(value).is_some() {
                return Some(value.chars().any(|ch| ('1'..='9').contains(&ch)));
            }
            plain_string(value).map(|text| !text.is_empty())
        }
    }
}

/// The content of a quoted string literal with no prefix and no escapes;
/// None for anything else, since an escape can spell one character several
/// ways (`"\n"`, `"\x0a"`).
fn plain_string(value: &str) -> Option<String> {
    if !value.trim_start().starts_with(['"', '\'']) {
        return None;
    }
    python_string_literal_value(value).filter(|text| !text.contains('\\'))
}

/// An int literal Python compares exactly; None for a float or other text.
fn integer(value: &str) -> Option<i128> {
    let text = value.replace('_', "");
    let digits = text.strip_prefix('-').unwrap_or(&text);
    if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn decimal(value: &str) -> Option<f64> {
    let text = value.replace('_', "");
    if text.is_empty()
        || !text
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == '-')
    {
        return None;
    }
    text.parse().ok()
}

pub(super) fn compare(left: &str, operator: &str, right: &str) -> Option<bool> {
    let quoted = |value: &str| value.trim_start().starts_with(['"', '\'']);
    if (quoted(left) && plain_string(left).is_none())
        || (quoted(right) && plain_string(right).is_none())
    {
        return None;
    }
    if let (Some(left), Some(right)) = (integer(left), integer(right)) {
        return Some(match operator {
            "==" => left == right,
            "!=" => left != right,
            "<" => left < right,
            "<=" => left <= right,
            ">" => left > right,
            ">=" => left >= right,
            _ => return None,
        });
    }
    // An int beyond 2**53 has no exact float, so a mixed comparison with it
    // is not read.
    let exact = |value: &str| integer(value).is_none_or(|number| number.unsigned_abs() <= 1 << 53);
    if !exact(left) || !exact(right) {
        return None;
    }
    match operator {
        "==" => literals_equal(left, right),
        "!=" => literals_equal(left, right).map(|equal| !equal),
        _ => {
            let (left, right) = (decimal(left)?, decimal(right)?);
            Some(match operator {
                "<" => left < right,
                "<=" => left <= right,
                ">" => left > right,
                ">=" => left >= right,
                _ => return None,
            })
        }
    }
}
