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
    new_line_number: usize,
    multiline: &[(usize, usize)],
) -> bool {
    if old == new
        || multiline
            .iter()
            .any(|(start, end)| (*start..=*end).contains(&new_line_number))
    {
        return false;
    }
    let indent = |text: &str| text.len() - text.trim_start().len();
    if old.get(..indent(old)) != new.get(..indent(new)) {
        return false;
    }
    super::infection_shape::same_executable_text(old, new)
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
