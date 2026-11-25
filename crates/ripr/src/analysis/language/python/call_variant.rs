//! A one-line edit whose only change picks another side of `str.strip`,
//! another digit count for `round`, or the other sort direction (#16203).
//!
//! `s.strip()` to `s.lstrip()` gives a different result only for a string
//! with trailing whitespace; `round(x, 2)` to `round(x, 3)` only for a
//! number the two counts round differently; `sorted(xs)` to
//! `sorted(xs, reverse=True)` only for a list holding two different
//! elements. A strong exact-value assertion whose
//! every call passes a value outside that set passes before and after the
//! edit.
//!
//! The rule reads the changed token in place: `<param>.strip()`,
//! `.lstrip()` or `.rstrip()` with no arguments, or `round(<param>, <int>)`
//! with a literal digit count, or one `sorted(<param>)` or `<param>.sort()`
//! whose literal `reverse` flag alone flips, where `<param>` is an owner parameter the body
//! never rebinds. Each strong related call (`boundary.rs`) must bind that
//! parameter to a literal the rule can evaluate both ways; any other call, or
//! no call at all, leaves the verdict to the other rules.

use super::PythonOwner;
use super::arm_activation::builtin_names_intact;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{PARAMETRIZE_ROW_MARKER, literals_equal, strong_owner_call_values};
use super::discriminators::python_string_literal_value;
use super::related_tests::PythonRelatedCandidate;
use super::term_activation::code_mention_count;
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
    /// `round(param, old)` to `round(param, new)`; a negative count rounds
    /// to tens, hundreds and coarser.
    Round { param: String, old: i32, new: i32 },
    /// `sorted(param)` or `param.sort()` with its `reverse=` flag flipped.
    Sort {
        param: String,
        old_call: String,
        new_call: String,
    },
}

type Token = (Tok, String);

/// The changed call when the only edit between the lines is a strip side or
/// a `round` digit count on a bare name; None for anything else.
pub(super) fn call_variant(old_line: &str, new_line: &str) -> Option<CallVariant> {
    // A line that also changes its block changes which code runs around it.
    let indent = |line: &str| line.len() - line.trim_start().len();
    if old_line.get(..indent(old_line)) != new_line.get(..indent(new_line)) {
        return None;
    }
    let old = spellings(old_line)?;
    let new = spellings(new_line)?;
    if let Some(sort) = sort_variant(&old, &new) {
        return Some(sort);
    }
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
            // A negative count lexes as a `-` before the digits; both sides
            // must spell the sign alike, or the edit changes more than the
            // count and the token counts already differed above.
            let negative = matches!(at(-1), Some((Tok::Minus, _)));
            let (round_at, lpar_at, param_at, comma_at) = if negative {
                (-5, -4, -3, -2)
            } else {
                (-4, -3, -2, -1)
            };
            let shape = matches!(at(round_at), Some((Tok::Name { .. }, name)) if name == "round")
                && not_attribute(round_at - 1)
                && matches!(at(lpar_at), Some((Tok::Lpar, _)))
                && matches!(at(param_at), Some((Tok::Name { .. }, _)))
                && matches!(at(comma_at), Some((Tok::Comma, _)))
                && matches!(at(1), Some((Tok::Rpar, _)));
            if !shape {
                return None;
            }
            let count = |token: &Token| -> Option<i32> {
                let count = token.1.parse::<i32>().ok()?;
                Some(if negative { -count } else { count })
            };
            Some(CallVariant::Round {
                param: at(param_at).map(|(_, text)| text.clone())?,
                old: count(&old[i])?,
                new: count(&new[i])?,
            })
        }
        _ => None,
    }
}

/// One `sorted(<name>[, reverse=<bool>])` or `<name>.sort([reverse=<bool>])`
/// in a line: its token span, the name, and the `reverse` flag.
fn sort_call(tokens: &[Token]) -> Option<(std::ops::Range<usize>, String, bool)> {
    let name = |index: usize, text: &str| matches!(tokens.get(index), Some((Tok::Name { .. }, spelled)) if spelled == text);
    let is = |index: usize, want: &Tok| tokens.get(index).is_some_and(|(tok, _)| tok == want);
    let reverse_flag = |at: usize| -> Option<(bool, usize)> {
        if !name(at, "reverse") || !is(at + 1, &Tok::Equal) {
            return None;
        }
        let flag = match tokens.get(at + 2) {
            Some((Tok::True, _)) => true,
            Some((Tok::False, _)) => false,
            _ => return None,
        };
        Some((flag, at + 3))
    };
    let mut found = None;
    for start in 0..tokens.len() {
        let preceded_by_dot = start > 0 && is(start - 1, &Tok::Dot);
        let call = if name(start, "sorted") && !preceded_by_dot && is(start + 1, &Tok::Lpar) {
            let param = match tokens.get(start + 2) {
                Some((Tok::Name { .. }, spelled)) => spelled.clone(),
                _ => continue,
            };
            let (flag, close) = if is(start + 3, &Tok::Comma) {
                reverse_flag(start + 4)?
            } else {
                (false, start + 3)
            };
            is(close, &Tok::Rpar).then_some((start..close + 1, param, flag))
        } else if matches!(tokens.get(start), Some((Tok::Name { .. }, _)))
            && !preceded_by_dot
            && is(start + 1, &Tok::Dot)
            && name(start + 2, "sort")
            && is(start + 3, &Tok::Lpar)
        {
            let param = tokens.get(start).map(|(_, spelled)| spelled.clone())?;
            let (flag, close) = if is(start + 4, &Tok::Rpar) {
                (false, start + 4)
            } else {
                reverse_flag(start + 4)?
            };
            is(close, &Tok::Rpar).then_some((start..close + 1, param, flag))
        } else {
            None
        };
        if let Some(call) = call {
            if found.is_some() {
                return None;
            }
            found = Some(call);
        }
    }
    found
}

/// A line pair whose only edit flips the `reverse` flag of one sort call.
fn sort_variant(old: &[Token], new: &[Token]) -> Option<CallVariant> {
    let (old_span, old_param, old_flag) = sort_call(old)?;
    let (new_span, new_param, new_flag) = sort_call(new)?;
    let same = |a: &[Token], b: &[Token]| {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0 == y.0 && x.1 == y.1)
    };
    let sorted_form = |tokens: &[Token], span: &std::ops::Range<usize>| {
        tokens
            .get(span.start)
            .is_some_and(|(_, text)| text == "sorted")
    };
    let spell = |tokens: &[Token], span: std::ops::Range<usize>| {
        tokens.get(span).map(|part| {
            part.iter()
                .map(|(_, text)| text.as_str())
                .collect::<String>()
        })
    };
    (old_param == new_param
        && old_flag != new_flag
        && sorted_form(old, &old_span) == sorted_form(new, &new_span)
        && same(old.get(..old_span.start)?, new.get(..new_span.start)?)
        && same(old.get(old_span.end..)?, new.get(new_span.end..)?))
    .then(|| {
        Some(CallVariant::Sort {
            param: old_param,
            old_call: spell(old, old_span)?.replace(',', ", "),
            new_call: spell(new, new_span)?.replace(',', ", "),
        })
    })
    .flatten()
}

/// Evidence that every strong related call binds the changed call's
/// argument to a value both versions treat alike, or None.
pub(super) fn python_call_variant_evidence(
    old_line: &str,
    new_line: &str,
    line: usize,
    owner: &PythonOwner,
    owner_source: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonCallVariantEvidence> {
    if !matches!(
        owner.owner_kind,
        Some(OwnerKind::Function | OwnerKind::Method | OwnerKind::ClassMethod)
    ) || owner.term_facts.calls_own_name()
    {
        // A recursive call can pass the changed call a value no test call
        // spells.
        return None;
    }
    let variant = call_variant(old_line, new_line)?;
    // A bare `round` or `sorted` is the builtin only when nothing in the
    // owner's file rebinds the name.
    let builtin = match &variant {
        CallVariant::Round { .. } => Some("round"),
        CallVariant::Sort { old_call, .. } if old_call.starts_with("sorted(") => Some("sorted"),
        _ => None,
    };
    if let Some(name) = builtin
        && !owner_source.is_some_and(|source| builtin_names_intact(owner, source, &[name]))
    {
        return None;
    }
    let param = match &variant {
        CallVariant::Strip { param, .. }
        | CallVariant::Round { param, .. }
        | CallVariant::Sort { param, .. } => param,
    };
    if !owner
        .parameters
        .iter()
        .any(|parameter| &parameter.name == param)
        || !owner.term_facts.is_stable_parameter(param)
    {
        return None;
    }
    // A list is mutable: the body must not reach it before the changed line,
    // and the line must name it once, so the sort sees the list the test
    // passed.
    if matches!(variant, CallVariant::Sort { .. })
        && (owner.term_facts.first_mention_line(param) != Some(line)
            || code_mention_count(new_line, param) != 1)
    {
        return None;
    }
    let rows = strong_owner_call_values(owner, related_candidates, focus)?;
    if rows.is_empty() {
        return None;
    }
    for row in &rows {
        let agrees = if matches!(variant, CallVariant::Sort { .. }) {
            // A parametrized list may be changed in place by the test body
            // before the call, so only a list the call spells is read.
            if row.text.contains(PARAMETRIZE_ROW_MARKER) {
                return None;
            }
            elements_agree(row.elements.get(param)?)?
        } else {
            variant_agrees(&variant, row.values.get(param)?)?
        };
        if !agrees {
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
            discriminator: if old.min(new) < 0 {
                // A negative count rounds to a multiple of `10^-count`;
                // a value already on that grid reads the same both ways.
                let grid_exponent = u32::try_from(-old.min(new)).unwrap_or(u32::MAX).min(18);
                let grid = 10i64
                    .checked_pow(grid_exponent)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| format!("10^{}", -old.min(new)));
                format!("`{param}` with a value that is not a multiple of {grid}")
            } else {
                format!(
                    "`{param}` with more than {} decimal place{}",
                    old.min(new),
                    if old.min(new) == 1 { "" } else { "s" }
                )
            },
        },
        CallVariant::Sort {
            param,
            old_call,
            new_call,
        } => PythonCallVariantEvidence {
            old_call,
            new_call,
            discriminator: format!("`{param}` holding two or more different elements"),
        },
    })
}

/// Whether sorting the elements either way gives the same list: every
/// element equals the first, so no order exists to reverse. None when an
/// equality cannot be read.
pub(super) fn elements_agree(elements: &[String]) -> Option<bool> {
    // An int spelling may group its digits with underscores (`1_000`).
    let ungrouped = |text: &str| -> String {
        let grouped = !text.starts_with('_')
            && !text.ends_with('_')
            && text.chars().all(|ch| ch.is_ascii_digit() || ch == '_');
        if grouped {
            text.replace('_', "")
        } else {
            text.to_string()
        }
    };
    let Some(first) = elements.first() else {
        return Some(true);
    };
    let first = ungrouped(first);
    for element in elements {
        if !literals_equal(&first, &ungrouped(element))? {
            return Some(false);
        }
    }
    Some(true)
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
        CallVariant::Round { old, new, .. } => round_agrees(value, *old, *new),
        // A sort reads a collection, never a scalar binding.
        CallVariant::Sort { .. } => None,
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
        ("strip", "lstrip") | ("lstrip", "strip") => "text followed by trailing whitespace",
        ("strip", "rstrip") | ("rstrip", "strip") => "leading whitespace before text",
        _ => "leading or trailing whitespace around text",
    }
}

/// Longest literal, and coarsest count magnitude, the reader evaluates. A
/// float's stored binary value can round unlike its decimal spelling near a
/// half-way point, and the farther the spelling goes past `f64`'s roughly 17
/// significant digits the wider that gap grows; beyond these bounds the
/// reader returns None rather than claim agreement.
const MAX_ROUND_DIGITS: usize = 15;
const MAX_ROUND_COUNT: i32 = 20;
const MAX_ROUND_SPELLING: usize = 48;

/// Whether `round(value, old)` and `round(value, new)` return the same
/// number, read as exact decimal half-to-even rounding of a plain decimal
/// spelling, within [`MAX_ROUND_DIGITS`], a spelling of
/// [`MAX_ROUND_SPELLING`] digits and counts within [`MAX_ROUND_COUNT`].
/// Python rounds a float by the double it names. A spelling that is not on
/// an exact decimal tie rounds alike under both, inside those digits; on a
/// tie the double may sit on either side (`round(1.05, 1)` is `1.1`, where
/// half-to-even on the spelling gives `1.0`), so a float spelling that sits
/// on an exact tie at either count is not read. An int spelling is exact
/// and Python rounds it as this read does. Anything beyond the bounds is
/// refused. A scientific spelling is not a plain decimal, so it is refused;
/// the argument reader passes plain decimals only.
fn round_agrees(value: &str, old: i32, new: i32) -> Option<bool> {
    if old.unsigned_abs() > MAX_ROUND_COUNT as u32 || new.unsigned_abs() > MAX_ROUND_COUNT as u32 {
        return None;
    }
    let text = value.trim().replace('_', "");
    let unsigned = text.strip_prefix('-').unwrap_or(&text);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty() && fraction.is_empty()
        || whole.len() + fraction.len() > MAX_ROUND_SPELLING
        || !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    let mut digits: Vec<u8> = whole
        .chars()
        .chain(fraction.chars())
        .map(|ch| ch as u8 - b'0')
        .collect();
    let leading = digits.iter().position(|&digit| digit != 0);
    let Some(leading) = leading else {
        // Zero agrees under any two counts (and `-0.0 == 0.0`).
        return Some(true);
    };
    let digits = digits.split_off(leading);
    if digits.len() > MAX_ROUND_DIGITS {
        return None;
    }
    if !fraction.is_empty()
        && [old, new]
            .iter()
            .any(|&keep| rounds_an_exact_tie(&digits, fraction.len(), keep))
    {
        return None;
    }
    // `round` treats both signs alike, so the magnitudes decide.
    Some(
        round_half_even(&digits, fraction.len(), old)
            == round_half_even(&digits, fraction.len(), new),
    )
}

/// Whether `round(<digits> * 10^-scale, keep)` drops an exact tie: the
/// first dropped digit is 5 and nothing but zeros follows it.
fn rounds_an_exact_tie(digits: &[u8], scale: usize, keep: i32) -> bool {
    let cut = digits.len() as i64 - scale as i64 + keep as i64;
    if cut < 0 || cut >= digits.len() as i64 {
        return false;
    }
    let cut = usize::try_from(cut).unwrap_or(0);
    digits[cut] == 5 && digits[cut + 1..].iter().all(|&digit| digit == 0)
}

/// One exact `round(<digits> * 10^-scale, keep)` as `(digits, exponent)`:
/// the value is `digits * 10^exponent`, rounded half to even. A `keep`
/// below zero rounds to a multiple of `10^-keep`. Read by the sibling test
/// module for the tie rule alone.
pub(super) fn round_half_even(digits: &[u8], scale: usize, keep: i32) -> (Vec<u8>, i64) {
    let int_len = digits.len() as i64 - scale as i64;
    // The kept digits end at the `10^keep` place, index `int_len + keep`.
    let cut = int_len + keep as i64;
    if cut >= digits.len() as i64 {
        return normalize_rounded(digits.to_vec(), -(scale as i64));
    }
    if cut < 0 {
        // Less than one digit of the quantum (`round(60, -3)`): zero.
        return (vec![0], 0);
    }
    let cut = usize::try_from(cut).unwrap_or(0);
    let mut kept = digits[..cut].to_vec();
    let dropped = digits.get(cut).copied().unwrap_or(0);
    // The digits after the dropped one decide a tie that is not exact.
    let rest_nonzero = digits[cut + 1..].iter().any(|&digit| digit != 0);
    let round_up = match dropped.cmp(&5) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            if rest_nonzero {
                true
            } else {
                // An exact tie rounds to the even last digit.
                kept.last().is_some_and(|&digit| digit % 2 == 1)
            }
        }
    };
    if round_up {
        let mut index = kept.len();
        while index > 0 {
            index -= 1;
            if kept[index] == 9 {
                kept[index] = 0;
            } else {
                kept[index] += 1;
                break;
            }
        }
        if index == 0 && (kept.is_empty() || kept.first() == Some(&0)) {
            kept.insert(0, 1);
        }
    }
    if kept.is_empty() {
        // Nothing kept and no rounding up (`round(500, -3)`): zero.
        return (vec![0], 0);
    }
    normalize_rounded(kept, int_len - cut as i64)
}

/// Trim trailing zeros so equal values render as one `(digits, exponent)`.
fn normalize_rounded(mut digits: Vec<u8>, mut exponent: i64) -> (Vec<u8>, i64) {
    while digits.last() == Some(&0) && digits.len() > 1 {
        digits.pop();
        exponent += 1;
    }
    (digits, exponent)
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
