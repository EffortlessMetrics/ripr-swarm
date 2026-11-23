//! A one-line edit whose only change picks another side of `str.strip` or
//! another digit count for `round` (#16203).
//!
//! `s.strip()` to `s.lstrip()` gives a different result only for a string
//! with trailing whitespace; `round(x, 2)` to `round(x, 3)` only for a number
//! with more than two decimal places. A strong exact-value assertion whose
//! every call passes a value outside that set passes before and after the
//! edit.
//!
//! The rule reads the changed token in place: `<param>.strip()`,
//! `.lstrip()` or `.rstrip()` with no arguments, or `round(<param>, <int>)`
//! with a literal digit count, where `<param>` is an owner parameter the body
//! never rebinds. Each strong related call (`boundary.rs`) must bind that
//! parameter to a literal the rule can evaluate both ways; any other call, or
//! no call at all, leaves the verdict to the other rules.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::strong_owner_call_values;
use super::discriminators::python_string_literal_value;
use super::related_tests::PythonRelatedCandidate;
use crate::domain::OwnerKind;
use rustpython_parser::{Mode, Tok, lexer::lex};

/// The changed call, and the input that would tell its versions apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PythonCallVariantEvidence {
    /// The call before and after the edit, as spelled (`s.strip()`).
    pub(super) old_call: String,
    pub(super) new_call: String,
    /// The input that separates them (`` `s` with trailing whitespace ``).
    pub(super) discriminator: String,
}

/// The one changed call of a line pair the rule reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CallVariant {
    /// `param.<old>()` to `param.<new>()` within the strip family.
    Strip {
        param: String,
        old: String,
        new: String,
    },
    /// `round(param, old)` to `round(param, new)`.
    Round { param: String, old: u32, new: u32 },
}

type Token = (Tok, String);

/// The changed call when the only edit between the lines is a strip side or
/// a `round` digit count on a bare name; None for anything else.
pub(super) fn call_variant(old_line: &str, new_line: &str) -> Option<CallVariant> {
    let old = spellings(old_line)?;
    let new = spellings(new_line)?;
    if old.len() != new.len() {
        return None;
    }
    let differing: Vec<usize> = (0..new.len())
        .filter(|&i| old[i].0 != new[i].0 || old[i].1 != new[i].1)
        .collect();
    let [i] = differing.as_slice() else {
        return None;
    };
    let i = *i;
    let at = |offset: isize| -> Option<&Token> {
        i.checked_add_signed(offset)
            .and_then(|index| new.get(index))
    };
    let not_attribute = |offset: isize| !matches!(at(offset), Some((Tok::Dot, _)));
    match (&old[i].0, &new[i].0) {
        (Tok::Name { .. }, Tok::Name { .. }) => {
            let strip = |text: &str| matches!(text, "strip" | "lstrip" | "rstrip");
            let shape = matches!(at(-1), Some((Tok::Dot, _)))
                && matches!(at(-2), Some((Tok::Name { .. }, _)))
                && not_attribute(-3)
                && matches!(at(1), Some((Tok::Lpar, _)))
                && matches!(at(2), Some((Tok::Rpar, _)));
            (shape && strip(&old[i].1) && strip(&new[i].1)).then(|| CallVariant::Strip {
                param: at(-2).map(|(_, text)| text.clone()).unwrap_or_default(),
                old: old[i].1.clone(),
                new: new[i].1.clone(),
            })
        }
        (Tok::Int { .. }, Tok::Int { .. }) => {
            let shape = matches!(at(-4), Some((Tok::Name { .. }, name)) if name == "round")
                && not_attribute(-5)
                && matches!(at(-3), Some((Tok::Lpar, _)))
                && matches!(at(-2), Some((Tok::Name { .. }, _)))
                && matches!(at(-1), Some((Tok::Comma, _)))
                && matches!(at(1), Some((Tok::Rpar, _)));
            if !shape {
                return None;
            }
            Some(CallVariant::Round {
                param: at(-2).map(|(_, text)| text.clone())?,
                old: old[i].1.parse().ok()?,
                new: new[i].1.parse().ok()?,
            })
        }
        _ => None,
    }
}

/// Evidence that every strong related call binds the changed call's
/// argument to a value both versions treat alike, or None.
pub(super) fn python_call_variant_evidence(
    old_line: &str,
    new_line: &str,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonCallVariantEvidence> {
    if !matches!(
        owner.owner_kind,
        Some(OwnerKind::Function | OwnerKind::Method | OwnerKind::ClassMethod)
    ) {
        return None;
    }
    let variant = call_variant(old_line, new_line)?;
    let param = match &variant {
        CallVariant::Strip { param, .. } | CallVariant::Round { param, .. } => param,
    };
    if !owner
        .parameters
        .iter()
        .any(|parameter| &parameter.name == param)
        || !owner.term_facts.is_stable_parameter(param)
    {
        return None;
    }
    let rows = strong_owner_call_values(owner, related_candidates, focus)?;
    if rows.is_empty() {
        return None;
    }
    for row in &rows {
        let value = row.values.get(param)?;
        if !variant_agrees(&variant, value)? {
            return None;
        }
    }
    Some(match variant {
        CallVariant::Strip { param, old, new } => PythonCallVariantEvidence {
            old_call: format!("{param}.{old}()"),
            new_call: format!("{param}.{new}()"),
            discriminator: format!("`{param}` with {}", strip_discriminator(&old, &new)),
        },
        CallVariant::Round { param, old, new } => PythonCallVariantEvidence {
            old_call: format!("round({param}, {old})"),
            new_call: format!("round({param}, {new})"),
            discriminator: format!(
                "`{param}` with more than {} decimal place{}",
                old.min(new),
                if old.min(new) == 1 { "" } else { "s" }
            ),
        },
    })
}

/// Whether both versions of the call give the same result for one literal
/// argument; None when the rule cannot evaluate it.
pub(super) fn variant_agrees(variant: &CallVariant, value: &str) -> Option<bool> {
    match variant {
        CallVariant::Strip { old, new, .. } => {
            let text = python_string_literal_value(value)?;
            if !value.trim_start().starts_with(['"', '\''])
                || text
                    .chars()
                    .any(|ch| ch == '\\' || !(ch == '\t' || (' '..='~').contains(&ch)))
            {
                return None;
            }
            Some(strip(&text, old)? == strip(&text, new)?)
        }
        CallVariant::Round { old, new, .. } => {
            let digits = decimal_places(value)?;
            Some(digits <= (*old).min(*new))
        }
    }
}

fn strip<'a>(text: &'a str, side: &str) -> Option<&'a str> {
    let space = |ch: char| ch == ' ' || ch == '\t';
    Some(match side {
        "strip" => text.trim_matches(space),
        "lstrip" => text.trim_start_matches(space),
        "rstrip" => text.trim_end_matches(space),
        _ => return None,
    })
}

fn strip_discriminator(old: &str, new: &str) -> &'static str {
    match (old, new) {
        ("strip", "lstrip") | ("lstrip", "strip") => "trailing whitespace",
        ("strip", "rstrip") | ("rstrip", "strip") => "leading whitespace",
        _ => "leading or trailing whitespace",
    }
}

/// Decimal places of a plain int or float literal, trailing zeros dropped.
fn decimal_places(value: &str) -> Option<u32> {
    let text = value.trim().replace('_', "");
    let unsigned = text.strip_prefix('-').unwrap_or(&text);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    u32::try_from(fraction.trim_end_matches('0').len()).ok()
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
