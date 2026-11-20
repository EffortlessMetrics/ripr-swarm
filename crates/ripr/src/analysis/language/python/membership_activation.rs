//! Membership member activation evidence for the Python preview adapter
//! (#15632; extends RIPR-SPEC-0238).
//!
//! A changed predicate that edits the members of a literal collection
//! (`if val in (True, "yes", "y", "on"):` -> `if val in (True, "yes", "on"):`)
//! changes the branch only for an input equal to an altered member. A strong
//! exact-value oracle whose every related call passes other members, or values
//! outside both collections, still passes after the edit, so reach plus a
//! strong oracle does not discriminate it.
//!
//! The rule engages only when the old and the new line each read as
//! `<subject> in <display>` or `<subject> not in <display>` with the same
//! control keyword, the same subject and the same operator, where the display
//! is a tuple, list or set of scalar literals and at least one member is kept.
//! The subject must be a bare owner parameter that the owner rebinds only by a
//! case normalization (`val = val.lower()`) before the changed line, outside
//! any loop.
//!
//! Each altered member (removed, added, or both for a changed member) needs a
//! strong related call row whose literal input equals that member and whose
//! membership result differs between the old and the new collection. Python
//! equates `True` with `1` and `False` with `0`: removing `1` while `True`
//! stays changes nothing, so such a member is not altered, and a row is a hit
//! only when the two membership results are both readable and differ. A string
//! input under a case normalization counts only when the normalization leaves
//! it unchanged, and a non-string only when the normalization sits directly
//! under `if isinstance(<subject>, str):`. When some altered member has no hit, including when the
//! strong rows are empty or some strong call goes unread, the rule names that
//! member as the missing discriminator and the caller does not credit
//! `exposed`. Whole-collection replacement, a non-literal display, a subject
//! that is not a parameter, any other rebinding and an unreadable literal keep
//! the existing verdict.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    BoundaryActivation, CallRow, PythonBoundaryEvidence, literal_elements,
    strong_owner_call_rows_with_gaps, unrebound_constants,
};
use super::discriminators::python_string_literal_value;
use super::guard_activation::{leading_whitespace, owner_binding_lines};
use super::related_tests::PythonRelatedCandidate;
use super::truthiness_swap::strip_trailing_comment;
use crate::{domain::ValueContext, value_origins::LocatedValueFact as ValueFact};

/// A scalar literal as Python compares it inside a membership test.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Scalar {
    /// A number in canonical decimal text; `True`/`False` read as `1`/`0`.
    Number(String),
    /// A plain string's content without escapes.
    Text(String),
    None,
}

/// One collection member: its source text and its compared value.
#[derive(Clone, Debug)]
struct Member {
    text: String,
    value: Scalar,
}

/// One `<subject> [not] in <display>` predicate line.
struct Membership {
    keyword: &'static str,
    subject: String,
    negated: bool,
    members: Vec<Member>,
}

/// A case normalization the owner applies to the subject before the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Normalization {
    case: Case,
    /// Every normalization line sits directly under
    /// `if isinstance(<subject>, str):`, so a non-string keeps its value.
    str_guarded: bool,
}

/// Evidence that a changed literal-collection membership predicate is not
/// discriminated by any strong related call input. None when the line is not
/// that shape, when every altered member is hit, or when static evidence
/// cannot read the subject's value at the line.
pub(super) fn python_membership_member_gap(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
    line_text: &str,
    old_line_text: &str,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonBoundaryEvidence> {
    let new = parse_membership(line_text)?;
    let old = parse_membership(old_line_text)?;
    if new.keyword != old.keyword || new.subject != old.subject || new.negated != old.negated {
        return None;
    }
    let altered = altered_members(&old.members, &new.members)?;
    let subject = new.subject.as_str();
    if !owner
        .parameters
        .iter()
        .any(|parameter| parameter.name == subject)
    {
        return None;
    }
    let changed_index = changed_line.checked_sub(1)?;
    let normalization = subject_normalization(owner, owner_source, changed_index, subject)?;
    let constants = unrebound_constants(owner, related_candidates);
    let (rows, gaps) =
        strong_owner_call_rows_with_gaps(owner, &constants, related_candidates, focus);
    let unhit: Vec<&Member> = altered
        .iter()
        .filter(|member| {
            !rows
                .iter()
                .any(|row| row_hits(row, subject, member, &old, &new, normalization))
        })
        .collect();
    let first = unhit.first()?;
    let observed_values = observed_subject_values(subject, &rows);
    let shown: Vec<&str> = unhit.iter().map(|member| member.text.as_str()).collect();
    let observed: Vec<&str> = observed_values
        .iter()
        .filter_map(|fact| fact.value.split_once(" = ").map(|(_, value)| value))
        .collect();
    let mut reason = format!(
        "The edit changes `{subject} {}` for {} only, and no strong related test call passes `{subject}` such a value",
        if new.negated { "not in" } else { "in" },
        shown.join(", "),
    );
    if observed.is_empty() {
        reason.push_str("; no strong related call binds a literal for it");
    } else {
        reason.push_str(&format!(
            "; observed {subject} values: {}",
            observed.join(", ")
        ));
    }
    if gaps {
        reason.push_str("; some strong related call is not read");
    }
    reason.push_str(&format!(". Add a test case that passes {}", first.text));
    Some(PythonBoundaryEvidence {
        activation: BoundaryActivation::Missing,
        discriminator: Some(format!("{subject} == {}", first.text)),
        observed_values,
        reason,
    })
}

/// The line as one literal-collection membership predicate, or None.
fn parse_membership(line_text: &str) -> Option<Membership> {
    let text = strip_trailing_comment(line_text).trim();
    let text = text.strip_suffix(':')?.trim_end();
    let (keyword, condition) = ["if", "elif", "while"].into_iter().find_map(|keyword| {
        let rest = text.strip_prefix(keyword)?;
        rest.starts_with(char::is_whitespace)
            .then(|| (keyword, rest.trim()))
    })?;
    let subject_end = condition.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))?;
    let subject = &condition[..subject_end];
    if !is_plain_identifier(subject) {
        return None;
    }
    let rest = condition[subject_end..].trim_start();
    let (negated, display) = if let Some(display) = rest.strip_prefix("not in ") {
        (true, display)
    } else if let Some(display) = rest.strip_prefix("in ") {
        (false, display)
    } else {
        return None;
    };
    let display = display.trim();
    if !(display.starts_with('(') || display.starts_with('[') || display.starts_with('{')) {
        return None;
    }
    let members = literal_elements(display)?
        .into_iter()
        .map(|text| {
            Some(Member {
                value: scalar(&text)?,
                text,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Membership {
        keyword,
        subject: subject.to_string(),
        negated,
        members,
    })
}

fn is_plain_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && !matches!(
            text,
            "None" | "True" | "False" | "not" | "and" | "or" | "is" | "in" | "lambda"
        )
}

/// The compared value of one scalar literal, or None when static evidence
/// cannot read it (an escape inside a string).
fn scalar(text: &str) -> Option<Scalar> {
    match text {
        "True" => return Some(Scalar::Number("1".to_string())),
        "False" => return Some(Scalar::Number("0".to_string())),
        "None" => return Some(Scalar::None),
        _ => {}
    }
    if let Some(number) = super::boundary::canonical_decimal(text) {
        return Some(Scalar::Number(number));
    }
    let content = python_string_literal_value(text)?;
    (!content.contains('\\')).then_some(Scalar::Text(content))
}

/// Members present on one side only, after Python equality. None for a
/// whole-collection replacement (no member kept) or when nothing changed.
fn altered_members(old: &[Member], new: &[Member]) -> Option<Vec<Member>> {
    let contains =
        |side: &[Member], member: &Member| side.iter().any(|other| other.value == member.value);
    let removed: Vec<&Member> = old.iter().filter(|m| !contains(new, m)).collect();
    let added: Vec<&Member> = new.iter().filter(|m| !contains(old, m)).collect();
    let kept = old.iter().any(|m| contains(new, m));
    if !kept {
        return None;
    }
    let mut altered: Vec<Member> = Vec::new();
    for member in removed.into_iter().chain(added) {
        if !contains(&altered, member) {
            altered.push(member.clone());
        }
    }
    (!altered.is_empty()).then_some(altered)
}

/// How the owner normalizes the subject before the changed line:
/// `Some(None)` when it never rebinds it, `Some(Some(_))` for one case
/// normalization (`val = val.lower()`), and None for any other rebinding or a
/// changed line inside a loop.
fn subject_normalization(
    owner: &PythonOwner,
    owner_source: &str,
    changed_index: usize,
    subject: &str,
) -> Option<Option<Normalization>> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed_indent = leading_whitespace(lines.get(changed_index)?);
    let loop_enclosed = lines
        .get(owner.start_line..changed_index)?
        .iter()
        .any(|raw| {
            let trimmed = raw.trim_start();
            (trimmed.starts_with("for ") || trimmed.starts_with("while "))
                && leading_whitespace(raw) < changed_indent
        });
    if loop_enclosed {
        return None;
    }
    let mut normalization: Option<Normalization> = None;
    for (index, line) in owner_binding_lines(owner_source, owner, subject) {
        if index >= changed_index {
            return None;
        }
        let case = normalization_case(strip_trailing_comment(line), subject)?;
        let str_guarded = str_guarded(&lines, index, subject);
        match &mut normalization {
            Some(seen) if seen.case != case => return None,
            Some(seen) => seen.str_guarded &= str_guarded,
            None => normalization = Some(Normalization { case, str_guarded }),
        }
    }
    Some(normalization)
}

/// Whether the nearest non-blank line above `index` is a less indented
/// `if isinstance(<subject>, str):` header.
fn str_guarded(lines: &[&str], index: usize, subject: &str) -> bool {
    let Some(raw) = lines.get(index) else {
        return false;
    };
    let indent = leading_whitespace(raw);
    let Some(header) = lines
        .get(..index)
        .and_then(|before| before.iter().rev().find(|line| !line.trim().is_empty()))
    else {
        return false;
    };
    let compact: String = strip_trailing_comment(header)
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    leading_whitespace(header) < indent && compact == format!("ifisinstance({subject},str):")
}

fn normalization_case(line: &str, subject: &str) -> Option<Case> {
    let compact: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    let call = compact.strip_prefix(&format!("{subject}={subject}."))?;
    match call {
        "lower()" | "casefold()" => Some(Case::Lower),
        "upper()" => Some(Case::Upper),
        _ => None,
    }
}

/// Whether the row passes `member` and the old and new membership results
/// for that input are both readable and differ.
fn row_hits(
    row: &CallRow,
    subject: &str,
    member: &Member,
    old: &Membership,
    new: &Membership,
    normalization: Option<Normalization>,
) -> bool {
    let Some(binding) = row.bindings.get(subject) else {
        return false;
    };
    let Some(value) = scalar(&binding.value) else {
        return false;
    };
    if let (Some(normalization), Scalar::Text(content)) = (normalization, &value) {
        // A normalized string keeps its value only when the normalization
        // leaves it unchanged; otherwise static evidence does not place it.
        let normalized = match normalization.case {
            Case::Lower => content.to_ascii_lowercase(),
            Case::Upper => content.to_ascii_uppercase(),
        };
        if !content.is_ascii() || normalized != *content {
            return false;
        }
    }
    if normalization.is_some_and(|normalization| !normalization.str_guarded)
        && !matches!(value, Scalar::Text(_))
    {
        // A non-string under an unguarded `val.lower()` raises before the
        // membership test runs.
        return false;
    }
    let holds = |side: &Membership| side.members.iter().any(|m| m.value == value);
    value == member.value && holds(old) != holds(new)
}

fn observed_subject_values(subject: &str, rows: &[CallRow]) -> Vec<ValueFact> {
    let mut facts: Vec<ValueFact> = rows
        .iter()
        .filter_map(|row| {
            let binding = row.bindings.get(subject)?;
            (!binding.from_default).then(|| ValueFact {
                file: Some(row.file.clone()),
                line: row.line,
                text: row.text.clone(),
                value: format!("{subject} = {}", binding.value),
                context: ValueContext::FunctionArgument,
            })
        })
        .collect();
    crate::analysis::classify::sort_value_facts(&mut facts);
    facts.dedup();
    facts
}
