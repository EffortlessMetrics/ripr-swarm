//! A one-line edit whose only change picks another side of `str.strip`,
//! another digit count for `round`, or the other sort direction (#16203).
//!
//! `s.strip()` to `s.lstrip()` gives a different result only for a string
//! with trailing whitespace; `round(x, 2)` to `round(x, 3)` only for a number
//! with more than two decimal places; `sorted(xs)` to
//! `sorted(xs, reverse=True)` only for a list holding two different
//! elements; `d.get("k", 0)` to `d.get("k", 5)` only for a dict without the
//! key. A strong exact-value assertion whose
//! every call passes a value outside that set passes before and after the
//! edit.
//!
//! The rule reads the changed token in place: `<param>.strip()`,
//! `.lstrip()` or `.rstrip()` with no arguments, or `round(<param>, <int>)`
//! with a literal digit count, or one `sorted(<param>)` or `<param>.sort()`
//! whose literal `reverse` flag alone flips, or one `<param>.get(<key>, <lit>)`
//! whose literal fallback alone changes, where `<param>` is an owner parameter the body
//! never rebinds. Each strong related call (`boundary.rs`) must bind that
//! parameter to a literal the rule can evaluate both ways; any other call, or
//! no call at all, leaves the verdict to the other rules.

use super::PythonOwner;
use super::arm_activation::builtin_names_intact;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    PARAMETRIZE_ROW_MARKER, literal_value, literals_equal, strong_owner_call_values,
};
use super::discriminators::python_string_literal_value;
use super::related_tests::PythonRelatedCandidate;
use super::term_activation::code_mention_count;
use crate::domain::OwnerKind;
use rustpython_parser::ast::{Expr, Mod, Ranged};
use rustpython_parser::{Mode, Tok, lexer::lex, parse};

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
    /// `sorted(param)` or `param.sort()` with its `reverse=` flag flipped.
    Sort {
        param: String,
        old_call: String,
        new_call: String,
    },
    /// `param.get(key, old)` to `param.get(key, new)`: another literal
    /// fallback for one literal key.
    GetFallback {
        param: String,
        key: String,
        old: String,
        new: String,
    },
}

type Token = (Tok, String);

/// The changed call when the only edit between the lines is a strip side, a
/// `round` digit count, a sort direction or a `get` fallback on a bare name;
/// None for anything else.
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
    let scalar = |tok: &Tok| {
        matches!(
            tok,
            Tok::Int { .. }
                | Tok::Float { .. }
                | Tok::String { .. }
                | Tok::None
                | Tok::True
                | Tok::False
        )
    };
    let get_shape = matches!(at(-6), Some((Tok::Name { .. }, _)))
        && not_attribute(-7)
        && matches!(at(-5), Some((Tok::Dot, _)))
        && matches!(at(-4), Some((Tok::Name { .. }, text)) if text == "get")
        && matches!(at(-3), Some((Tok::Lpar, _)))
        && matches!(at(-2), Some((Tok::String { .. } | Tok::Int { .. }, _)))
        && matches!(at(-1), Some((Tok::Comma, _)))
        && matches!(at(1), Some((Tok::Rpar, _)));
    if get_shape && scalar(&old[i].0) && scalar(&new[i].0) {
        return Some(CallVariant::GetFallback {
            param: at(-6).map(|(_, text)| text.clone())?,
            key: at(-2).map(|(_, text)| text.clone())?,
            old: old[i].1.clone(),
            new: new[i].1.clone(),
        });
    }
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
        | CallVariant::Sort { param, .. }
        | CallVariant::GetFallback { param, .. } => param,
    };
    if !owner
        .parameters
        .iter()
        .any(|parameter| &parameter.name == param)
        || !owner.term_facts.is_stable_parameter(param)
    {
        return None;
    }
    // A list or dict is mutable: the body must not reach it before the
    // changed line, and the line must name it once, so the call sees the
    // collection the test passed.
    if matches!(
        variant,
        CallVariant::Sort { .. } | CallVariant::GetFallback { .. }
    ) && (owner.term_facts.first_mention_line(param) != Some(line)
        || code_mention_count(new_line, param) != 1)
    {
        return None;
    }
    let rows = strong_owner_call_values(owner, related_candidates, focus)?;
    if rows.is_empty() {
        return None;
    }
    for row in &rows {
        let agrees = match &variant {
            // A parametrized list or dict may be changed in place by the test
            // body before the call, so only a display the call spells is read.
            CallVariant::Sort { .. } | CallVariant::GetFallback { .. }
                if row.text.contains(PARAMETRIZE_ROW_MARKER) =>
            {
                return None;
            }
            CallVariant::Sort { .. } => elements_agree(row.elements.get(param)?)?,
            // A present key never reaches the fallback.
            CallVariant::GetFallback { key, .. } => key_present(row.dict_keys.get(param)?, key)?,
            _ => variant_agrees(&variant, row.values.get(param)?)?,
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
            discriminator: format!(
                "`{param}` with more than {} decimal place{}",
                old.min(new),
                if old.min(new) == 1 { "" } else { "s" }
            ),
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
        CallVariant::GetFallback {
            param,
            key,
            old,
            new,
        } => PythonCallVariantEvidence {
            old_call: format!("{param}.get({key}, {old})"),
            new_call: format!("{param}.get({key}, {new})"),
            discriminator: format!("`{param}` without the key {key}"),
        },
    })
}

/// Whether sorting the elements either way gives the same list: every
/// element equals the first, so no order exists to reverse. None when an
/// equality cannot be read.
pub(super) fn elements_agree(elements: &[String]) -> Option<bool> {
    let Some(first) = elements.first() else {
        return Some(true);
    };
    for element in elements {
        if !same_element(first, element)? {
            return Some(false);
        }
    }
    Some(true)
}

/// Whether two element literals hold the same value: the same spelling, two
/// plain strings with the same text, or two ints with the same value. Mixed
/// or float spellings are not read: an int and a float that print alike can
/// differ once the float is rounded to binary (`2**53 + 1` and its float).
fn same_element(left: &str, right: &str) -> Option<bool> {
    let (left, right) = (left.trim(), right.trim());
    if left == right {
        return Some(true);
    }
    let plain = |text: &str| text.starts_with(['"', '\'']) && !text.contains('\\');
    if plain(left) && plain(right) {
        return Some(python_string_literal_value(left)? == python_string_literal_value(right)?);
    }
    let int = |text: &str| text.replace('_', "").parse::<i128>().ok();
    Some(int(left)? == int(right)?)
}

/// Whether a dict with these literal keys holds `key`; None when an equality
/// cannot be read.
pub(super) fn key_present(keys: &[String], key: &str) -> Option<bool> {
    // A number key is read only as an int: an int and a float that print
    // alike can be different keys once the float is rounded to binary.
    let float = |text: &str| {
        let text = text.trim();
        text.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-' || ch == '.')
            && text.replace('_', "").parse::<i128>().is_err()
    };
    if float(key) {
        return None;
    }
    for candidate in keys {
        if float(candidate) {
            return None;
        }
        // Digit separators are spelling only (`1_000 == 1000`).
        let unseparated = |text: &str| literal_value(text).unwrap_or_else(|| text.to_string());
        if literals_equal(&unseparated(candidate), &unseparated(key))? {
            return Some(true);
        }
    }
    Some(false)
}

/// The key literals of a dict display whose every key is a scalar literal
/// (`{"k": 2, 3: x}`); None for anything else, including a `**` unpack or a
/// computed key, whose keys are not all known.
pub(super) fn literal_dict_keys(text: &str) -> Option<Vec<String>> {
    let text = text.trim();
    if !text.starts_with('{') {
        return None;
    }
    let Mod::Expression(expression) = parse(text, Mode::Expression, "<argument>").ok()? else {
        return None;
    };
    let Expr::Dict(dict) = expression.body.as_ref() else {
        return None;
    };
    dict.keys
        .iter()
        .map(|key| {
            let key = key.as_ref()?;
            if !matches!(key, Expr::Constant(_)) {
                return None;
            }
            let start = usize::from(key.range().start());
            let end = usize::from(key.range().end());
            literal_value(text.get(start..end)?)
        })
        .collect()
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
        // A sort or a lookup reads a collection, never a scalar binding.
        CallVariant::Sort { .. } | CallVariant::GetFallback { .. } => None,
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
