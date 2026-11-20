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
//! The rule reads the parsed owner module on the new side and on the old
//! side (the new source with the changed line restored). It engages on the
//! one `<subject> in <display>` or `<subject> not in <display>` comparison
//! whose lines include the changed line and whose members differ, anywhere in
//! the changed statement: an `if`/`elif`/`while` condition, an `and`/`or`
//! compound, `not (...)`, a parenthesized test, a `return`, an assignment, or
//! a display that spans several lines. The display is a tuple, list or set of
//! scalar literals, at least one member is kept, and the subject and
//! operator are the same on both sides. A second edit on the line outside the
//! display, or a member static evidence cannot read, withholds credit without
//! reading rows. A removed-only line inside a multi-line display pairs the
//! old comparison with the new one that lacks the line's elements. The
//! subject is a bare owner parameter, or `p.lower()`, `p.casefold()`,
//! `p.upper()`, `p.strip()` or `str(p)` of one.
//!
//! Each altered member (removed, added, or both for a changed member) needs a
//! strong related call row whose literal input equals that member and whose
//! membership result differs between the old and the new collection. Python
//! equates `True` with `1` and `False` with `0`: removing `1` while `True`
//! stays changes nothing, so such a member is not altered. A member hit
//! flips only the comparison; a compound condition may still not flip, which
//! this rule accepts, since it never credits on its own. A string input under
//! a case normalization (`val = val.lower()`) or a transforming subject counts
//! only when the transform leaves it unchanged; a non-string counts under a
//! normalization only when it sits directly under
//! `if isinstance(<subject>, str):`, and never under a transforming subject.
//! Any other rebinding of the parameter before the comparison, or any
//! rebinding inside a loop around it, places no input. When some altered
//! member has no hit, including when the strong rows are empty or some strong
//! call goes unread, the rule names that member as the missing discriminator
//! and the caller does not credit `exposed`. A changed line of literal
//! elements inside a display an `in` comparison owns whose old side cannot be
//! paired withholds credit the same way.
//!
//! Not engaged, so the existing verdict stands: a whole-collection
//! replacement with nothing else changed, a non-literal or computed display,
//! a subject that is not a parameter or one of the transforms above (an
//! alias, `self.attr`, `cfg["key"]`), a source that does not parse, and a
//! `match` arm.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    BoundaryActivation, CallRow, PythonBoundaryEvidence, literal_elements,
    strong_owner_call_rows_with_gaps, unrebound_constants,
};
use super::discriminators::python_string_literal_value;
use super::guard_activation::{leading_whitespace, owner_binding_lines};
use super::module_constants::{mentions_identifier, visit_exprs_in_statements};
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, text_for_range};
use super::truthiness_swap::strip_trailing_comment;
use crate::{domain::ValueContext, value_origins::LocatedValueFact as ValueFact};
use rustpython_parser::ast::{self, Expr, Mod, Ranged};
use std::path::Path;

/// A scalar literal as Python compares it inside a membership test.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Scalar {
    /// A number in canonical decimal text; `True`/`False` read as `1`/`0`.
    Number(String),
    /// A plain string's content without escapes.
    Text(String),
    None,
}

/// One collection member: its source text and its compared value, or None
/// when static evidence cannot read it (an escape, `1e0`, `0x1`, a name,
/// `b"y"`).
#[derive(Clone, Debug)]
struct Member {
    text: String,
    value: Option<Scalar>,
}

impl Member {
    /// Python equality for readable members; source text otherwise.
    fn same(&self, other: &Member) -> bool {
        match (&self.value, &other.value) {
            (Some(left), Some(right)) => left == right,
            (None, None) => self.text == other.text,
            _ => false,
        }
    }
}

/// A transform the membership test applies to the parameter before
/// comparing it (`val.lower() in (...)`, `str(val) in (...)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transform {
    Identity,
    Lower,
    Upper,
    Strip,
    Str,
}

/// One `<subject> in|not in <literal display>` comparison in a parsed source.
#[derive(Clone, Debug)]
struct Site {
    parameter: String,
    /// The subject as written, whitespace removed (`val.lower()`).
    subject: String,
    transform: Transform,
    negated: bool,
    members: Vec<Member>,
    /// 1-based first and last line of the whole comparison.
    lines: (usize, usize),
    /// Byte span of the display in its source.
    display: (usize, usize),
    /// 1-based first and last line of the display.
    display_lines: (usize, usize),
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

/// How the owner binds the parameter before the comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rebinding {
    Never,
    Normalized(Normalization),
    /// Any other rebinding: no call input is placed on a member.
    Unmodelled,
}

/// Evidence that a changed literal-collection membership comparison is not
/// discriminated by any strong related call input. The rule only withholds
/// `exposed`; it never credits. None when the change is not that shape or
/// when every altered member is hit.
pub(super) fn python_membership_member_gap(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
    line_text: &str,
    old_line_text: &str,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonBoundaryEvidence> {
    if line_text.trim() == old_line_text.trim()
        || !(may_hold_membership(line_text) || may_hold_membership(old_line_text))
    {
        return None;
    }
    let new_sites = sites_on_line(owner_source, changed_line)?;
    let old_source = replace_line(owner_source, changed_line, old_line_text)?;
    let old_sites = sites_on_line(&old_source, changed_line);
    let paired = old_sites
        .as_ref()
        .and_then(|old_sites| changed_pair(old_sites, &new_sites));
    let Some((old, new)) = paired else {
        // The old side cannot be paired with the new one. A changed line
        // that holds only literal elements of a display an `in` comparison
        // owns still edits members: withhold credit without naming one.
        return element_line_fallback(owner, &new_sites, line_text, old_line_text);
    };
    let extra_edit = outside_display(&old_source, changed_line, old.display)?
        != outside_display(owner_source, changed_line, new.display)?;
    member_gap(
        owner,
        owner_source,
        (old, new),
        extra_edit,
        related_candidates,
        focus,
    )
}

/// Evidence for a removed-only line (no new-side counterpart) that sat
/// inside a multi-line literal membership display: the old file holds the
/// display with the line, the new file the same comparison without it.
#[allow(
    clippy::too_many_arguments,
    reason = "old and new sources, both coordinates and the oracle inputs"
)]
pub(super) fn python_membership_deleted_gap(
    owner: &PythonOwner,
    old_source: &str,
    old_line: usize,
    new_source: &str,
    new_line: usize,
    removed_text: &str,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonBoundaryEvidence> {
    let removed = element_line(removed_text)?;
    let old_sites = sites_on_line(old_source, old_line)?;
    let old = old_sites.iter().find(|site| {
        site.display_lines.0 < old_line
            && old_line < site.display_lines.1
            && owner
                .parameters
                .iter()
                .any(|parameter| parameter.name == site.parameter)
    })?;
    let removed: Vec<Member> = removed
        .into_iter()
        .map(|text| Member {
            value: scalar(&text),
            text,
        })
        .collect();
    // The old members without the removed line's elements, in order.
    let mut expected = old.members.clone();
    for member in &removed {
        let index = expected.iter().position(|kept| kept.same(member))?;
        expected.remove(index);
    }
    let mut candidates: Vec<Site> = sites_on_line(new_source, new_line)
        .into_iter()
        .chain(
            new_line
                .checked_sub(1)
                .and_then(|line| sites_on_line(new_source, line)),
        )
        .flatten()
        .filter(|site| {
            site.subject == old.subject
                && site.negated == old.negated
                && site.members.len() == expected.len()
                && site
                    .members
                    .iter()
                    .zip(&expected)
                    .all(|(left, right)| left.same(right))
        })
        .collect();
    candidates.sort_by_key(|site| site.display);
    candidates.dedup_by_key(|site| site.display);
    match candidates.as_slice() {
        [new] => member_gap(
            owner,
            new_source,
            (old, new),
            false,
            related_candidates,
            focus,
        ),
        // The new comparison cannot be located: withhold credit.
        _ => Some(forced_gap(
            old,
            removed.first(),
            "static evidence cannot locate the comparison after the deleted member line",
        )),
    }
}

/// The verdict for one paired old/new comparison whose members differ.
fn member_gap(
    owner: &PythonOwner,
    owner_source: &str,
    (old, new): (&Site, &Site),
    extra_edit: bool,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonBoundaryEvidence> {
    let subject = new.parameter.as_str();
    if !owner
        .parameters
        .iter()
        .any(|parameter| parameter.name == subject)
    {
        return None;
    }
    let unreadable = old
        .members
        .iter()
        .chain(&new.members)
        .any(|member| member.value.is_none());
    if unreadable || extra_edit {
        let readable = readable_altered(&old.members, &new.members);
        return Some(forced_gap(
            new,
            readable.first(),
            if extra_edit {
                "the changed line also edits code outside the collection, so no input reading can establish the edit is observed"
            } else {
                "a member of the collection is not a scalar literal static evidence reads, so no test input is placed on the altered members"
            },
        ));
    }
    let altered = altered_members(&old.members, &new.members)?;
    let rebinding = subject_rebinding(owner, owner_source, new.lines.0 - 1, subject)?;
    let constants = unrebound_constants(owner, related_candidates);
    let (rows, gaps) =
        strong_owner_call_rows_with_gaps(owner, &constants, related_candidates, focus);
    let unhit: Vec<&Member> = altered
        .iter()
        .filter(|member| {
            !rows
                .iter()
                .any(|row| row_hits(row, member, old, new, rebinding))
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
        "The edit changes `{} {}` for {} only, and no strong related test call passes `{subject}` such a value",
        subject_text(new),
        if new.negated { "not in" } else { "in" },
        shown.join(", "),
    );
    if rebinding == Rebinding::Unmodelled {
        reason.push_str(&format!(
            "; `{subject}` is rebound in `{}` before the comparison in a way static evidence does not model",
            owner.name
        ));
    } else if observed.is_empty() {
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
        discriminator: Some(format!("{} == {}", subject_text(new), first.text)),
        observed_values,
        reason,
    })
}

/// A gap that withholds `exposed` without reading rows, naming `first` as
/// the discriminator when one is readable.
fn forced_gap(site: &Site, first: Option<&Member>, why: &str) -> PythonBoundaryEvidence {
    let subject = subject_text(site);
    let operator = if site.negated { "not in" } else { "in" };
    PythonBoundaryEvidence {
        activation: BoundaryActivation::Missing,
        discriminator: first.map(|member| format!("{subject} == {}", member.text)),
        observed_values: Vec::new(),
        reason: format!(
            "The edit changes the members of a literal `{subject} {operator}` collection, and {why}{}",
            first
                .map(|member| format!(". Add a test case that passes {}", member.text))
                .unwrap_or_default()
        ),
    }
}

/// Readable members present on one side only, without the kept-member
/// requirement.
fn readable_altered(old: &[Member], new: &[Member]) -> Vec<Member> {
    let mut altered: Vec<Member> = Vec::new();
    let one_side = |from: &[Member], other: &[Member]| -> Vec<Member> {
        from.iter()
            .filter(|member| member.value.is_some() && !other.iter().any(|o| o.same(member)))
            .cloned()
            .collect()
    };
    for member in one_side(old, new).into_iter().chain(one_side(new, old)) {
        if !altered.iter().any(|seen| seen.same(&member)) {
            altered.push(member);
        }
    }
    altered
}

fn subject_text(site: &Site) -> String {
    site.subject.clone()
}

/// A cheap pre-check before parsing: the line names `in`, or reads as
/// literal display elements (a line inside a multi-line display).
fn may_hold_membership(line: &str) -> bool {
    let line = strip_trailing_comment(line);
    mentions_identifier(line, "in") || element_line(line).is_some()
}

/// The literal elements of a line that holds only display elements, with
/// surrounding brackets, commas and a trailing `:` removed.
fn element_line(line: &str) -> Option<Vec<String>> {
    let text = strip_trailing_comment(line)
        .trim()
        .trim_start_matches(['(', '[', '{'])
        .trim_end_matches([')', ']', '}', ':', ','])
        .trim();
    if text.is_empty() {
        return None;
    }
    literal_elements(&format!("[{text}]")).filter(|elements| !elements.is_empty())
}

fn replace_line(source: &str, line: usize, text: &str) -> Option<String> {
    let mut lines: Vec<&str> = source.split('\n').collect();
    let slot = lines.get_mut(line.checked_sub(1)?)?;
    let carriage = slot.ends_with('\r') && !text.ends_with('\r');
    let replacement = if carriage {
        format!("{text}\r")
    } else {
        text.to_string()
    };
    *slot = "";
    let mut joined = String::with_capacity(source.len() + text.len());
    for (index, part) in lines.iter().enumerate() {
        if index > 0 {
            joined.push('\n');
        }
        if index == line - 1 {
            joined.push_str(&replacement);
        } else {
            joined.push_str(part);
        }
    }
    Some(joined)
}

/// The membership comparisons whose lines include `line`, in source order.
fn sites_on_line(source: &str, line: usize) -> Option<Vec<Site>> {
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("owner.py"), source) else {
        return None;
    };
    let text = SourceText::new(source);
    let mut sites = Vec::new();
    visit_exprs_in_statements(&module.body, &mut |expr| {
        if let Expr::Compare(compare) = expr
            && let Some(site) = site(&text, compare)
            && site.lines.0 <= line
            && line <= site.lines.1
        {
            sites.push(site);
        }
    });
    sites.sort_by_key(|site| site.display);
    sites.dedup_by_key(|site| site.display);
    Some(sites)
}

fn site(text: &SourceText<'_>, compare: &ast::ExprCompare) -> Option<Site> {
    let ([op], [display]) = (compare.ops.as_slice(), compare.comparators.as_slice()) else {
        return None;
    };
    let negated = match op {
        ast::CmpOp::In => false,
        ast::CmpOp::NotIn => true,
        _ => return None,
    };
    let elements = match display {
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::List(list) => &list.elts,
        Expr::Set(set) => &set.elts,
        _ => return None,
    };
    let members = elements
        .iter()
        .map(|element| {
            let source = text_for_range(text, element.range());
            Member {
                value: scalar(source.trim()),
                text: source.trim().to_string(),
            }
        })
        .collect::<Vec<_>>();
    let (parameter, transform) = subject(&compare.left)?;
    let range = compare.range();
    let subject_source: String = text_for_range(text, compare.left.range())
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    Some(Site {
        parameter,
        subject: subject_source,
        transform,
        negated,
        members,
        lines: (
            text.line_for_offset(usize::from(range.start())),
            text.line_for_offset(usize::from(range.end())),
        ),
        display: (
            usize::from(display.range().start()),
            usize::from(display.range().end()),
        ),
        display_lines: (
            text.line_for_offset(usize::from(display.range().start())),
            text.line_for_offset(usize::from(display.range().end())),
        ),
    })
}

/// A bare name, `name.lower()` / `.casefold()` / `.upper()` / `.strip()`, or
/// `str(name)`.
fn subject(expr: &Expr) -> Option<(String, Transform)> {
    match expr {
        Expr::Name(name) => Some((name.id.to_string(), Transform::Identity)),
        Expr::Call(call) if call.keywords.is_empty() => {
            match (call.func.as_ref(), call.args.as_slice()) {
                (Expr::Attribute(attribute), []) => {
                    let Expr::Name(name) = attribute.value.as_ref() else {
                        return None;
                    };
                    let transform = match attribute.attr.as_str() {
                        "lower" | "casefold" => Transform::Lower,
                        "upper" => Transform::Upper,
                        "strip" => Transform::Strip,
                        _ => return None,
                    };
                    Some((name.id.to_string(), transform))
                }
                (Expr::Name(function), [Expr::Name(name)]) if function.id.as_str() == "str" => {
                    Some((name.id.to_string(), Transform::Str))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The one old/new comparison pair whose members differ, with every other
/// comparison on the line unchanged and the same subject and operator.
fn changed_pair<'a>(old: &'a [Site], new: &'a [Site]) -> Option<(&'a Site, &'a Site)> {
    if old.len() != new.len() || new.is_empty() {
        return None;
    }
    let mut changed = old.iter().zip(new).filter(|(old, new)| {
        old.members.len() != new.members.len()
            || old
                .members
                .iter()
                .zip(&new.members)
                .any(|(left, right)| !left.same(right))
    });
    let (old_site, new_site) = changed.next()?;
    if changed.next().is_some()
        || old_site.parameter != new_site.parameter
        || old_site.subject != new_site.subject
        || old_site.negated != new_site.negated
    {
        return None;
    }
    Some((old_site, new_site))
}

/// The text of `line` outside the display span, whitespace-normalized.
fn outside_display(source: &str, line: usize, display: (usize, usize)) -> Option<String> {
    let mut start = 0usize;
    for _ in 1..line {
        start += source.get(start..)?.find('\n')? + 1;
    }
    let end = source
        .get(start..)?
        .find('\n')
        .map_or(source.len(), |offset| start + offset);
    let before = source.get(start..display.0.clamp(start, end))?;
    let after = source.get(display.1.clamp(start, end)..end)?;
    let joined = format!(
        "{}\u{0}{}",
        strip_trailing_comment(before),
        strip_trailing_comment(after)
    );
    Some(joined.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// A line of literal elements inside a display an `in` comparison owns,
/// whose old side could not be paired: withhold `exposed`.
fn element_line_fallback(
    owner: &PythonOwner,
    new_sites: &[Site],
    line_text: &str,
    old_line_text: &str,
) -> Option<PythonBoundaryEvidence> {
    let new_elements = element_line(line_text)?;
    let old_elements = element_line(old_line_text)?;
    let site = new_sites.iter().find(|site| {
        owner
            .parameters
            .iter()
            .any(|parameter| parameter.name == site.parameter)
    })?;
    let altered: Vec<&String> = old_elements
        .iter()
        .filter(|element| !new_elements.contains(element))
        .chain(
            new_elements
                .iter()
                .filter(|element| !old_elements.contains(element)),
        )
        .collect();
    let first = altered.first()?;
    Some(PythonBoundaryEvidence {
        activation: BoundaryActivation::Missing,
        discriminator: Some(format!("{} == {first}", subject_text(site))),
        observed_values: Vec::new(),
        reason: format!(
            "The edit changes the members of a literal `{} {}` collection, and static evidence cannot pair the old comparison to place a test input on them. Add a test case that passes {first}",
            subject_text(site),
            if site.negated { "not in" } else { "in" },
        ),
    })
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
    let contains = |side: &[Member], member: &Member| side.iter().any(|other| other.same(member));
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

/// How the owner binds the parameter before the comparison that starts at
/// 0-based line `compare_index`: never, by one case normalization
/// (`val = val.lower()`) outside any loop, or in a way this rule does not
/// model (any other binding, or any binding inside a loop around the
/// comparison).
fn subject_rebinding(
    owner: &PythonOwner,
    owner_source: &str,
    compare_index: usize,
    subject: &str,
) -> Option<Rebinding> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let compare_indent = leading_whitespace(lines.get(compare_index)?);
    let loop_enclosed = lines
        .get(owner.start_line.min(compare_index)..compare_index)?
        .iter()
        .any(|raw| {
            let trimmed = raw.trim_start();
            (trimmed.starts_with("for ") || trimmed.starts_with("while "))
                && leading_whitespace(raw) < compare_indent
        });
    // A `while` comparison re-reads the subject after its own body runs.
    let compare_is_loop = lines.get(compare_index)?.trim_start().starts_with("while ");
    let mut normalization: Option<Normalization> = None;
    for (index, line) in owner_binding_lines(owner_source, owner, subject) {
        if loop_enclosed || (index == compare_index && line.contains(":=")) {
            // Inside a loop, or a walrus on the comparison line itself, which
            // binds before `in` reads the subject. A plain `val = val in (...)`
            // assigns only after the comparison has read the call input.
            return Some(Rebinding::Unmodelled);
        }
        if index > compare_index && compare_is_loop {
            return Some(Rebinding::Unmodelled);
        }
        if index >= compare_index {
            // Runs after the comparison, outside any loop around it.
            continue;
        }
        let Some(case) = normalization_case(strip_trailing_comment(line), subject) else {
            return Some(Rebinding::Unmodelled);
        };
        let str_guarded = str_guarded(&lines, index, subject);
        match &mut normalization {
            Some(seen) if seen.case != case => return Some(Rebinding::Unmodelled),
            Some(seen) => seen.str_guarded &= str_guarded,
            None => normalization = Some(Normalization { case, str_guarded }),
        }
    }
    Some(normalization.map_or(Rebinding::Never, Rebinding::Normalized))
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
fn row_hits(row: &CallRow, member: &Member, old: &Site, new: &Site, rebinding: Rebinding) -> bool {
    let Some(binding) = row.bindings.get(&new.parameter) else {
        return false;
    };
    let Some(value) = scalar(&binding.value) else {
        return false;
    };
    match rebinding {
        Rebinding::Never => {}
        Rebinding::Unmodelled => return false,
        Rebinding::Normalized(normalization) => match &value {
            // A normalized string keeps its value only when the
            // normalization leaves it unchanged; otherwise static evidence
            // does not place it.
            Scalar::Text(content) => {
                let transform = match normalization.case {
                    Case::Lower => Transform::Lower,
                    Case::Upper => Transform::Upper,
                };
                if !leaves_unchanged(transform, content) {
                    return false;
                }
            }
            // A non-string under an unguarded `val.lower()` raises before
            // the membership test runs.
            _ if !normalization.str_guarded => return false,
            _ => {}
        },
    }
    if new.transform != Transform::Identity {
        // The comparison transforms the parameter itself: only a string the
        // transform leaves unchanged is placed.
        let Scalar::Text(content) = &value else {
            return false;
        };
        if !leaves_unchanged(new.transform, content) {
            return false;
        }
    }
    let holds = |side: &Site| {
        side.members
            .iter()
            .any(|m| m.value.as_ref() == Some(&value))
    };
    member.value.as_ref() == Some(&value) && holds(old) != holds(new)
}

/// Whether `transform` maps the ASCII string `content` to itself.
fn leaves_unchanged(transform: Transform, content: &str) -> bool {
    if !content.is_ascii() {
        return false;
    }
    match transform {
        Transform::Identity | Transform::Str => true,
        Transform::Lower => content.to_ascii_lowercase() == content,
        Transform::Upper => content.to_ascii_uppercase() == content,
        Transform::Strip => {
            // Python strips every `str.isspace` character, which in ASCII
            // is `\t\n\x0b\x0c\r`, `\x1c`-`\x1f` and the space.
            let space = |ch: char| {
                ch.is_ascii_whitespace() || ch == '\x0b' || ('\x1c'..='\x1f').contains(&ch)
            };
            !content.starts_with(space) && !content.ends_with(space)
        }
    }
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
