//! Trailing-comment / trailing-whitespace no-op detection for the TypeScript
//! preview adapter (#10706).
//!
//! A changed line whose old and new text differ only in a trailing `//`
//! comment or in leading/trailing whitespace runs identically, so it has no
//! behavior for a test to discriminate: no probe, and no gap id or
//! discriminator that could embed the comment text. This is the TypeScript
//! counterpart of the Python adapter's `is_comment_or_whitespace_only_change`.
//!
//! The check fails closed. The executable text of both lines must be equal
//! (including the semicolon), both lines must split into code and comment
//! reliably, indentation must be unchanged, and the line must not sit inside
//! a construct that crosses a line break (template literal, block comment,
//! line-continued string, JSX text), where whitespace and `//` are content.
//! A file that does not parse gets no guard.

use super::*;
use oxc_ast::ast::{JSXText, StringLiteral, TemplateLiteral};
use oxc_ast_visit::{Visit, walk};

/// Whether `new` differs from `old` only in a trailing `//` comment or in
/// trailing whitespace. `multiline` holds the 1-based inclusive line ranges
/// of constructs spanning several lines (see [`multiline_construct_lines`]);
/// a `new_line_number` inside one is content, never a no-op.
pub(crate) fn is_trailing_comment_or_whitespace_only_change(
    old: &str,
    new: &str,
    (new_line_number, multiline): (usize, &[(usize, usize)]),
    (old_line_number, old_multiline): (usize, &[(usize, usize)]),
) -> bool {
    let inside = |ranges: &[(usize, usize)], line: usize| {
        ranges
            .iter()
            .any(|(start, end)| (*start..=*end).contains(&line))
    };
    // Both sides must sit outside such a construct: a paired old line that was
    // template content while the new line is code (or the reverse) is a real
    // change even when the trimmed text is equal.
    if old == new || inside(multiline, new_line_number) || inside(old_multiline, old_line_number) {
        return false;
    }
    let indent = |text: &str| text.len() - text.trim_start().len();
    if old.get(..indent(old)) != new.get(..indent(new)) {
        return false;
    }
    super::infection_shape::same_executable_text(old, new)
}

/// The pre-change source, rebuilt from the post-change `new_source` and the
/// diff: removed lines sit at their old-side line numbers and the unchanged
/// lines fill the other slots in order. `None` (fail closed) when the diff and
/// the source do not line up, so no guard applies.
pub(crate) fn reconstruct_old_source(
    new_source: &str,
    added: &[crate::analysis::diff::ChangedLine],
    removed: &[crate::analysis::diff::ChangedLine],
) -> Option<String> {
    let added_lines: std::collections::BTreeSet<usize> = added.iter().map(|l| l.line).collect();
    let removed_at: std::collections::BTreeMap<usize, &str> =
        removed.iter().map(|l| (l.line, l.text.as_str())).collect();
    if removed_at.len() != removed.len() || removed_at.contains_key(&0) {
        return None;
    }
    // Stale or quarantined source: an added line must be the text on disk.
    let on_disk: Vec<&str> = new_source.lines().collect();
    if added.iter().any(|line| {
        line.line
            .checked_sub(1)
            .and_then(|index| on_disk.get(index))
            .is_none_or(|text| text.trim_end_matches('\r') != line.text.trim_end_matches('\r'))
    }) {
        return None;
    }
    let mut unchanged = new_source
        .lines()
        .enumerate()
        .filter(|(index, _)| !added_lines.contains(&(index + 1)))
        .map(|(_, text)| text);
    let total = new_source.lines().count().checked_sub(added_lines.len())? + removed_at.len();
    let mut out = String::new();
    for old_line in 1..=total {
        let text = match removed_at.get(&old_line) {
            Some(text) => *text,
            None => unchanged.next()?,
        };
        out.push_str(text);
        out.push('\n');
    }
    unchanged.next().is_none().then_some(out)
}

/// 1-based inclusive line ranges of every template literal, block comment,
/// string literal and JSX text that crosses a line break, read from the
/// syntax tree. `None` when the file does not parse (the caller then applies
/// no guard) or the worker refuses it.
pub(crate) fn multiline_construct_lines(file: &Path, source: &str) -> Option<Vec<(usize, usize)>> {
    parse_on_worker(file, source, |file, source, allocator| {
        let ret = oxc_parser::Parser::new(allocator, source, source_type_for(file)).parse();
        if !ret.errors.is_empty() || ret.panicked {
            return None;
        }
        let mut spans: Vec<(u32, u32)> = ret
            .program
            .comments
            .iter()
            .filter(|comment| comment.is_block())
            .map(|comment| (comment.span.start, comment.span.end))
            .collect();
        let mut collector = Collector { spans: Vec::new() };
        collector.visit_program(&ret.program);
        spans.extend(collector.spans);
        Some(lines_of_spans(source, &spans))
    })
    .ok()
    .flatten()
}

struct Collector {
    spans: Vec<(u32, u32)>,
}

impl<'a> Visit<'a> for Collector {
    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        self.spans.push((it.span.start, it.span.end));
        walk::walk_template_literal(self, it);
    }
    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        self.spans.push((it.span.start, it.span.end));
    }
    fn visit_jsx_text(&mut self, it: &JSXText<'a>) {
        self.spans.push((it.span.start, it.span.end));
    }
}

fn lines_of_spans(source: &str, spans: &[(u32, u32)]) -> Vec<(usize, usize)> {
    let mut sorted: Vec<(usize, usize)> = spans
        .iter()
        .map(|(start, end)| (*start as usize, *end as usize))
        .collect();
    sorted.sort_unstable();
    let (mut line, mut scanned) = (1usize, 0usize);
    let mut line_at = |offset: usize| {
        line += source
            .get(scanned..offset)
            .map_or(0, |gap| gap.matches('\n').count());
        scanned = scanned.max(offset);
        line
    };
    let mut ranges = Vec::new();
    for (start, end) in sorted {
        let first = line_at(start);
        let last = line_at(end);
        if first < last {
            ranges.push((first, last));
        }
    }
    ranges
}
