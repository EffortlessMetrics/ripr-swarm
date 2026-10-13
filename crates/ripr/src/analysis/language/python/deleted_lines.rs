//! Removed-only Python probes (#7587, RIPR-SPEC-0156).
//!
//! A deleted behavior line that no added line pairs with left no record at
//! all: a pure deletion, or a code line whose comment pairs by content with a
//! re-added comment, read as "no behavioral candidates". The Rust adapter keeps
//! such a line as a removed-only probe whose `source_currentness` says it is
//! base-side evidence (`currentness_matrix_tail`); this module does the same
//! for Python. A base-side record is never a candidate edit target, so every
//! counted, gated, annotated and diagnostic surface skips it (#3281).

use super::classify::is_python_declaration_line;
use super::{
    ChangedFile, Finding, PythonNoBehaviorContext, PythonOwner, PythonTest,
    classify_change_with_context, is_python_import_line, is_python_no_behavior_line,
    is_python_structural_line, line_is_in_ranges, normalized_path, owner_for_changed_line,
};
use crate::analysis::diff::ChangedLine;
use crate::domain::SourceCurrentness;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

/// What the old side of one changed file says about its removed lines.
pub(super) struct OldSideRanges<'a> {
    pub(super) docstrings: &'a [RangeInclusive<usize>],
    pub(super) imports: &'a [RangeInclusive<usize>],
}

/// The candidate source of the changed file, when it was read.
pub(super) type NewSource<'a> = Option<&'a str>;

/// One base-side finding per removed behavior line that no added line pairs
/// with. `counterparts` is the file's one pairing
/// (`ChangedFile::aligned_counterparts`); `findings` holds the file's
/// candidate-side findings so a repeated probe id gets an ordinal suffix.
pub(super) fn deleted_line_findings(
    changed: &ChangedFile,
    counterparts: &BTreeMap<usize, &ChangedLine>,
    old_side: &OldSideRanges<'_>,
    new_source: NewSource<'_>,
    owners: &[PythonOwner],
    tests: &[PythonTest],
    findings: &[Finding],
) -> Vec<Finding> {
    let paired: BTreeSet<usize> = counterparts.values().map(|removed| removed.line).collect();
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    for finding in findings {
        *ids.entry(finding.probe.id.0.clone()).or_default() += 1;
    }
    let mut out = Vec::new();
    for removed in &changed.removed_lines {
        if paired.contains(&removed.line) || !is_removed_behavior_line(removed, old_side) {
            continue;
        }
        let Some(owner_line) = deleted_line_owner_line(changed, removed, new_source, owners) else {
            continue;
        };
        let Some(mut finding) = classify_change_with_context(
            &changed.path,
            owner_line,
            &removed.text,
            None,
            owners,
            tests,
            PythonNoBehaviorContext::default(),
        ) else {
            continue;
        };
        let expression = removed.text.trim();
        // Same-text evidence is the only movement evidence; without it the
        // record stays `base_deleted` (RIPR-SPEC-0156).
        finding.source_currentness = if changed
            .added_lines
            .iter()
            .any(|added| added.text.trim() == expression)
        {
            SourceCurrentness::MovedOrRenamed
        } else {
            SourceCurrentness::BaseDeleted
        };
        finding.probe.before = Some(removed.text.clone());
        finding.probe.after = None;
        // The projected new-side coordinate, as the Rust adapter records it.
        finding.probe.location.line = removed.new_side_line;
        let count = ids.entry(finding.probe.id.0.clone()).or_default();
        *count += 1;
        if *count > 1 {
            finding.probe.id.0 = format!("{}.{}", finding.probe.id.0, count);
            finding.id = finding.probe.id.0.clone();
        }
        out.push(finding);
    }
    out
}

/// A removed line that ran behavior: not a comment, blank line, docstring,
/// import, lone bracket or declaration. A removed `def`, `class` or decorator
/// deletes or re-shapes an owner, which this record does not describe.
fn is_removed_behavior_line(removed: &ChangedLine, old_side: &OldSideRanges<'_>) -> bool {
    !line_is_in_ranges(removed.line, old_side.docstrings)
        && !line_is_in_ranges(removed.line, old_side.imports)
        && !is_python_import_line(&removed.text)
        && !is_python_no_behavior_line(&removed.text)
        && !is_python_structural_line(&removed.text)
        && !is_python_declaration_line(&removed.text)
}

/// The new-side line whose owner held the deleted line. The diff parser
/// records a removed line at the new-side line that follows it, so a line
/// deleted from the middle of a body sits inside its owner. A line deleted
/// from the end of a body sits after it, so the line before is the owner's
/// last line. Either owner must have its header indented less than the
/// deleted line, and when both qualify the narrower one held it: a method's
/// last line deleted where its class goes on is the method's, not the
/// class's. A line outside every `def` and `class` belongs to the module,
/// resolved at a neighbouring line no narrower owner claims. A line whose own
/// `def` or `class` was deleted with it belonged to an owner that no longer
/// exists, and resolves to none.
fn deleted_line_owner_line(
    changed: &ChangedFile,
    removed: &ChangedLine,
    new_source: NewSource<'_>,
    owners: &[PythonOwner],
) -> Option<usize> {
    let indent = indent_width(&removed.text);
    if lost_its_owner(changed, removed) {
        return None;
    }
    let at = removed.new_side_line;
    let before = at.checked_sub(1).filter(|&line| line > 0);
    let header_indent = |owner: &PythonOwner| {
        new_source
            .and_then(|source| source.lines().nth(owner.start_line.checked_sub(1)?))
            .map(indent_width)
    };
    let holds_it = |owner: &PythonOwner| header_indent(owner).is_some_and(|header| header < indent);
    let around_body = scope_owner(changed, at, owners)
        .filter(|owner| owner.start_line < at && holds_it(owner))
        .map(|owner| (owner, at));
    // The owner's last line is the last code line above the deletion: blank
    // and comment lines kept between the body and the deleted tail are not
    // part of any owner's span.
    let tail_line = last_code_line_before(new_source, at);
    let after_tail = tail_line.and_then(|tail| {
        scope_owner(changed, tail, owners)
            .filter(|owner| owner.end_line >= tail && owner.end_line < at && holds_it(owner))
            .map(|owner| (owner, tail))
    });
    if let Some((_, line)) = around_body
        .into_iter()
        .chain(after_tail)
        .min_by_key(|(owner, _)| (owner.span_width(), owner.specificity_rank()))
    {
        return Some(line);
    }
    let inside_a_scope = |line: usize| scope_owner(changed, line, owners).is_some();
    if indent > 0
        && (inside_a_scope(at)
            || before.is_some_and(inside_a_scope)
            || tail_line.is_some_and(inside_a_scope))
    {
        return None;
    }
    // `classify_change_with_context` resolves the narrowest owner itself, so
    // a module line is classified at a line only the module holds.
    before.into_iter().chain([at]).find(|&line| {
        owner_for_changed_line(&changed.path, line, owners)
            .is_some_and(|owner| owner.name == MODULE_OWNER)
    })
}

/// Whether the `def` or `class` that enclosed `removed` was deleted with it.
/// The walk goes back through the removed lines in old-file order while they
/// stay contiguous there (an added line interrupting the block takes no old
/// line, and the diff parser gives the lines after it a new coordinate), and
/// follows each shallower code line outward: an enclosing `if` leads on to the
/// `def` around it. A kept line breaks contiguity, and its owner is live.
fn lost_its_owner(changed: &ChangedFile, removed: &ChangedLine) -> bool {
    let by_old_line: BTreeMap<usize, &ChangedLine> = changed
        .removed_lines
        .iter()
        .map(|line| (line.line, line))
        .collect();
    let mut threshold = indent_width(&removed.text);
    let mut old_line = removed.line;
    // Walking backward, a line opening with triple quotes closes a string
    // whose body runs up to the line that opened it; none of it is nesting.
    let mut in_string: Option<&str> = None;
    while threshold > 0 {
        let Some(previous) = old_line
            .checked_sub(1)
            .and_then(|line| by_old_line.get(&line))
        else {
            return false;
        };
        old_line = previous.line;
        let trimmed = previous.text.trim();
        if let Some(quotes) = in_string {
            if trimmed.contains(quotes) {
                in_string = None;
            }
            continue;
        }
        if let Some(quotes) = ["\"\"\"", "'''"]
            .into_iter()
            .find(|quotes| trimmed.starts_with(quotes) && trimmed.matches(quotes).count() == 1)
        {
            in_string = Some(quotes);
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') || continues_a_statement(&previous.text) {
            continue;
        }
        let previous_indent = indent_width(&previous.text);
        if previous_indent < threshold {
            if is_python_declaration_line(&previous.text) {
                return true;
            }
            threshold = previous_indent;
        }
    }
    false
}

/// A line that closes or continues a statement begun above it, so its indent
/// says nothing about nesting: the `):` closing a multi-line `def` signature,
/// or a line opening with a closing bracket.
fn continues_a_statement(text: &str) -> bool {
    is_python_structural_line(text) || text.trim_start().starts_with([')', ']', '}'])
}

const MODULE_OWNER: &str = "<module>";

/// The narrowest `def` or `class` owner holding `line`; the module owner
/// spans the whole file, so it holds every line and is left out.
fn scope_owner<'a>(
    changed: &ChangedFile,
    line: usize,
    owners: &'a [PythonOwner],
) -> Option<&'a PythonOwner> {
    let file = normalized_path(&changed.path);
    owners
        .iter()
        .filter(|owner| owner.name != MODULE_OWNER)
        .filter(|owner| normalized_path(&owner.file) == file)
        .filter(|owner| line >= owner.start_line && line <= owner.end_line)
        .min_by_key(|owner| (owner.span_width(), owner.specificity_rank()))
}

/// The nearest line above `at` that holds code, skipping blank and
/// comment-only lines. Without the candidate source it is the line above.
fn last_code_line_before(new_source: NewSource<'_>, at: usize) -> Option<usize> {
    let Some(source) = new_source else {
        return at.checked_sub(1).filter(|&line| line > 0);
    };
    let lines: Vec<&str> = source.lines().collect();
    (1..at).rev().find(|&line| {
        lines.get(line - 1).is_some_and(|text| {
            let trimmed = text.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
    })
}

fn indent_width(text: &str) -> usize {
    text.len() - text.trim_start().len()
}
