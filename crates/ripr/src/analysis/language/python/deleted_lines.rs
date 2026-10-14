//! Removed-only Python probes (#7587, RIPR-SPEC-0156).
//!
//! A deleted behavior line that no added line pairs with left no record at
//! all: a pure deletion, or a code line whose comment pairs by content with a
//! re-added comment, read as "no behavioral candidates". The Rust adapter keeps
//! such a line as a removed-only probe whose `source_currentness` says it is
//! base-side evidence (`currentness_matrix_tail`); this module does the same
//! for Python. A base-side record is never a candidate edit target, so every
//! counted, gated, annotated and diagnostic surface skips it (#3281).

use super::classify::{ChangeSite, classify_change_for_owner, is_python_declaration_line};
use super::{
    ChangedFile, Finding, PythonNoBehaviorContext, PythonOwner, PythonTest, is_python_import_line,
    is_python_no_behavior_line, is_python_structural_line, line_is_in_ranges, normalized_path,
};
use crate::analysis::diff::ChangedLine;
use crate::domain::SourceCurrentness;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

/// What the old side of one changed file says about its removed lines.
pub(super) struct OldSideRanges<'a> {
    pub(super) docstrings: &'a [RangeInclusive<usize>],
    pub(super) imports: &'a [RangeInclusive<usize>],
    /// The old file, rebuilt from the diff; `None` when the diff and the
    /// candidate file disagree.
    pub(super) source: Option<&'a str>,
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
    // Removed headers an added header replaced: the owner they open lives on.
    let re_headed: BTreeSet<usize> = counterparts
        .iter()
        .filter(|(added_line, _)| {
            changed
                .added_lines
                .iter()
                .any(|added| added.line == **added_line && is_python_declaration_line(&added.text))
        })
        .map(|(_, removed)| removed.line)
        .collect();
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    for finding in findings {
        *ids.entry(finding.probe.id.0.clone()).or_default() += 1;
    }
    // A paired line is the candidate side's record, not this deletion's, so
    // it does not use up the one record a run inside a kept statement gets.
    let statement_lines = continuation_lines(changed, old_side.source, |removed| {
        !paired.contains(&removed.line)
            && is_removed_behavior_line(removed, old_side)
            && !opens_only_brackets(&removed.text)
    });
    let mut out = Vec::new();
    for removed in &changed.removed_lines {
        if paired.contains(&removed.line)
            || statement_lines.fragments.contains(&removed.line)
            || !is_removed_behavior_line(removed, old_side)
            || opens_only_brackets(&removed.text)
        {
            continue;
        }
        let Some(owner) = deleted_line_owner(
            changed,
            removed,
            &re_headed,
            &statement_lines.statement_indent,
            new_source,
            owners,
        ) else {
            continue;
        };
        // Evidence names the projected new-side coordinate, as the Rust
        // adapter records it; only the owner comes from a live neighbour.
        let site = ChangeSite {
            file: &changed.path,
            line: removed.new_side_line,
            owner,
        };
        let Some(mut finding) = classify_change_for_owner(
            site,
            &removed.text,
            None,
            owners,
            tests,
            PythonNoBehaviorContext::default(),
            // A deleted line is base-side evidence: the guarded-arm gate
            // (#7392) reads new-side guards around a live line, so it stays
            // off here.
            None,
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

/// The live owner that held the deleted line. The diff parser
/// records a removed line at the new-side line that follows it, so a line
/// deleted from the middle of a body sits inside its owner. A line deleted
/// from the end of a body sits after it, so the line before is the owner's
/// last line. Either owner must have its header indented less than the
/// deleted line, and when both qualify the narrower one held it: a method's
/// last line deleted where its class goes on is the method's, not the
/// class's. A line outside every `def` and `class` belongs to the module. A
/// line whose own
/// `def` or `class` was deleted with it belonged to an owner that no longer
/// exists, and resolves to none.
fn deleted_line_owner<'a>(
    changed: &ChangedFile,
    removed: &ChangedLine,
    re_headed: &BTreeSet<usize>,
    statement_indent: &BTreeMap<usize, usize>,
    new_source: NewSource<'_>,
    owners: &'a [PythonOwner],
) -> Option<&'a PythonOwner> {
    let indent = statement_indent
        .get(&removed.line)
        .copied()
        .unwrap_or_else(|| indent_width(&removed.text));
    if lost_its_owner(changed, removed, indent, re_headed, statement_indent) {
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
    let around_body =
        scope_owner(changed, at, owners).filter(|owner| owner.start_line < at && holds_it(owner));
    // The owner's last line is the last code line above the deletion: blank
    // and comment lines kept between the body and the deleted tail are not
    // part of any owner's span.
    let tail_line = last_code_line_before(new_source, at);
    let after_tail = tail_line.and_then(|tail| {
        scope_owner(changed, tail, owners)
            .filter(|owner| owner.end_line >= tail && owner.end_line < at && holds_it(owner))
    });
    if let Some(owner) = around_body
        .into_iter()
        .chain(after_tail)
        .min_by_key(|owner| (owner.span_width(), owner.specificity_rank()))
    {
        return Some(owner);
    }
    let inside_a_scope = |line: usize| scope_owner(changed, line, owners).is_some();
    if indent > 0
        && (inside_a_scope(at)
            || before.is_some_and(inside_a_scope)
            || tail_line.is_some_and(inside_a_scope))
    {
        return None;
    }
    // Held by no `def` or `class`: the module's, even where the deletion
    // projects past the module's last surviving line.
    let file = normalized_path(&changed.path);
    owners
        .iter()
        .find(|owner| owner.name == MODULE_OWNER && normalized_path(&owner.file) == file)
}

/// Whether the `def` or `class` that enclosed `removed` was deleted with it.
/// The walk goes back through the removed lines in old-file order while they
/// stay contiguous there (an added line interrupting the block takes no old
/// line, and the diff parser gives the lines after it a new coordinate), and
/// follows each shallower code line outward: an enclosing `if` leads on to the
/// `def` around it. A kept line breaks contiguity, and its owner is live; so
/// is a removed header an added header pairs with, as when only its
/// annotations changed. Continuation lines of a statement say nothing about
/// nesting and are passed over.
fn lost_its_owner(
    changed: &ChangedFile,
    removed: &ChangedLine,
    indent: usize,
    re_headed: &BTreeSet<usize>,
    statement_indent: &BTreeMap<usize, usize>,
) -> bool {
    let by_old_line: BTreeMap<usize, &ChangedLine> = changed
        .removed_lines
        .iter()
        .map(|line| (line.line, line))
        .collect();
    let mut threshold = indent;
    let mut old_line = removed.line;
    while threshold > 0 {
        let Some(previous) = old_line
            .checked_sub(1)
            .and_then(|line| by_old_line.get(&line))
        else {
            return false;
        };
        old_line = previous.line;
        if statement_indent.contains_key(&previous.line) {
            continue;
        }
        let trimmed = previous.text.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || continues_a_statement(&previous.text) {
            continue;
        }
        let previous_indent = indent_width(&previous.text);
        if previous_indent < threshold {
            if is_python_declaration_line(&previous.text) {
                return !re_headed.contains(&previous.line);
            }
            threshold = previous_indent;
        }
    }
    false
}

/// A line that closes a bracket opened above it, so its indent says nothing
/// about nesting: the `):` closing a multi-line `def` signature. A bare
/// `try:`, `else:` or `finally:` is a block header and still counts.
fn continues_a_statement(text: &str) -> bool {
    text.trim_start().starts_with([')', ']', '}'])
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

/// Removed lines that are fragments of a statement rather than records of
/// their own: inside an open bracket, inside a triple-quoted string, or after
/// a trailing backslash. A deleted statement's record is the line that starts
/// it; the others would each attach a fragment (`p,`, `) -> int:`, a string
/// body at column 0) to whatever owner their own indent suggests (#7690).
/// The state runs across each contiguous old-side run and restarts at a gap
/// from the state the old file's kept lines above it leave there. When that
/// state continues, the run deletes from inside a statement a kept line
/// starts (an argument of a kept call, a line of a kept string), and a
/// deleted statement whose first line is a lone opening bracket has no
/// behavior on that line: either way its first unpaired behavior line is
/// the deletion's record and the rest of that statement are fragments.
fn continuation_lines(
    changed: &ChangedFile,
    old_source: Option<&str>,
    is_behavior: impl Fn(&ChangedLine) -> bool,
) -> StatementLines {
    let mut lines = StatementLines::default();
    if changed.removed_lines.is_empty() {
        return lines;
    }
    let mut by_old_line: Vec<&ChangedLine> = changed.removed_lines.iter().collect();
    by_old_line.sort_by_key(|line| line.line);
    let old_states = states_before_each_line(old_source);
    let old_lines: Vec<&str> = old_source
        .map(|source| source.lines().collect())
        .unwrap_or_default();
    let mut start_indent = 0;
    let mut state = StatementState::default();
    let mut previous_line: Option<usize> = None;
    // The current statement has no record yet: it began on a kept line, or
    // on a removed line holding only an opening bracket.
    let mut record_pending = false;
    for line in by_old_line {
        if previous_line.is_none_or(|previous| previous + 1 != line.line) {
            // The kept old lines above the run decide its start: a string
            // they open is one the run closes.
            state = old_states
                .get(line.line.saturating_sub(1))
                .copied()
                .unwrap_or_default();
            record_pending = state.continues();
            if state.continues() {
                // The statement starts on a kept line above the run.
                let mut start = line.line.saturating_sub(1);
                while start > 1
                    && old_states
                        .get(start - 1)
                        .is_some_and(|before| before.continues())
                {
                    start -= 1;
                }
                start_indent = old_lines
                    .get(start.saturating_sub(1))
                    .map_or(0, |text| indent_width(text));
            }
        }
        if !state.continues() {
            record_pending = false;
        }
        let starts_a_statement = !state.continues();
        if starts_a_statement {
            start_indent = indent_width(&line.text);
        } else {
            lines.statement_indent.insert(line.line, start_indent);
        }
        if record_pending && is_behavior(line) {
            record_pending = false;
        } else if state.continues() {
            lines.fragments.insert(line.line);
        }
        state = state.after(&line.text);
        // A statement that opens with a lone bracket (`(` on its own line)
        // has no behavior on its first line, so its first behavior line
        // carries the record instead.
        if starts_a_statement && state.continues() && opens_only_brackets(&line.text) {
            record_pending = true;
        }
        previous_line = Some(line.line);
    }
    lines
}

/// Removed lines placed by the statement each sits in.
#[derive(Default)]
struct StatementLines {
    /// Lines that are not records of their own.
    fragments: BTreeSet<usize>,
    /// Lines that start inside an open statement, record lines included,
    /// with the indent of the line that starts it: their own indent says
    /// nothing about which block holds them.
    statement_indent: BTreeMap<usize, usize>,
}

/// The statement state at the start of each old-file line, by 0-based
/// index: entry `i` is the state after old lines `1..=i`. Without the old
/// file every run starts outside any statement, as a run of its own.
fn states_before_each_line(old_source: Option<&str>) -> Vec<StatementState> {
    let Some(source) = old_source else {
        return Vec::new();
    };
    let mut states = vec![StatementState::default()];
    let mut state = StatementState::default();
    for text in source.lines() {
        state = state.after(text);
        states.push(state);
    }
    states
}

/// A line holding nothing but opening brackets.
fn opens_only_brackets(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| matches!(ch, '(' | '[' | '{'))
}

/// Where a statement stands at the end of a line.
#[derive(Clone, Copy, Default)]
struct StatementState {
    /// Open `(`, `[` and `{` outside strings and comments.
    depth: usize,
    /// The quote character of an open triple-quoted string.
    triple: Option<char>,
    /// The line ended with a backslash outside any string or comment.
    backslash: bool,
}

impl StatementState {
    fn continues(self) -> bool {
        self.depth > 0 || self.triple.is_some() || self.backslash
    }

    fn after(mut self, text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let tripled = |at: usize, quote: char| {
            chars.get(at + 1) == Some(&quote) && chars.get(at + 2) == Some(&quote)
        };
        let mut quote: Option<char> = None;
        let mut commented = false;
        let mut at = 0;
        while let Some(&ch) = chars.get(at) {
            if let Some(open) = self.triple {
                if ch == '\\' {
                    at += 2;
                    continue;
                }
                if ch == open && tripled(at, open) {
                    self.triple = None;
                    at += 3;
                    continue;
                }
            } else if let Some(open) = quote {
                if ch == '\\' {
                    at += 2;
                    continue;
                }
                if ch == open {
                    quote = None;
                }
            } else {
                match ch {
                    '#' => {
                        commented = true;
                        break;
                    }
                    '"' | '\'' if tripled(at, ch) => {
                        self.triple = Some(ch);
                        at += 3;
                        continue;
                    }
                    '"' | '\'' => quote = Some(ch),
                    '(' | '[' | '{' => self.depth += 1,
                    ')' | ']' | '}' => self.depth = self.depth.saturating_sub(1),
                    _ => {}
                }
            }
            at += 1;
        }
        // A backslash ending a comment is part of the comment, not a join.
        self.backslash = self.triple.is_none()
            && quote.is_none()
            && !commented
            && text.trim_end().ends_with('\\');
        self
    }
}
