//! #14175: an edit that leaves a line's code unchanged and only adds, removes
//! or rewords its trailing line comment seeds no probe. The seeding loop in
//! `diff.rs` asks these helpers; they read comment starts from the shared
//! lexical scanner so a `//` inside a string or block comment stays code.

/// 1-based line of `source` to the byte column where that line's actual line
/// comment starts. A line comment runs to the end of its line, so each line
/// holds at most one.
pub(super) fn line_comment_columns(source: &str) -> std::collections::BTreeMap<usize, usize> {
    let mut columns = std::collections::BTreeMap::new();
    if !source.contains("//") {
        return columns;
    }
    let mut starts = crate::analysis::extract::line_comment_starts(source)
        .into_iter()
        .peekable();
    let (mut line, mut line_start) = (1usize, 0usize);
    for (offset, byte) in source.bytes().enumerate() {
        if starts.peek().is_none() {
            break;
        }
        if starts.next_if_eq(&offset).is_some() {
            columns.insert(line, offset - line_start);
        }
        if byte == b'\n' {
            line += 1;
            line_start = offset + 1;
        }
    }
    columns
}

/// Whether `after` and `before` hold the same code and differ only in a
/// trailing line comment that was added, removed or reworded (#14175). Each
/// `*_comment` is the byte column where that side's file reads an actual line
/// comment on the line (see [`line_comment_columns`]), so a `//` inside a
/// string or block comment is code or data here and keeps its probe. Fails
/// closed when neither side holds a comment, when a column does not land on
/// `//` in the changed text (a line-1 byte-order mark, a stale index), when
/// the code before the comment differs in any byte other than whitespace
/// before the comment, and for a line that is only a comment.
pub(super) fn is_trailing_line_comment_only_change(
    after: &str,
    after_comment: Option<usize>,
    before: &str,
    before_comment: Option<usize>,
) -> bool {
    if after == before || (after_comment.is_none() && before_comment.is_none()) {
        return false;
    }
    let code = |text: &'_ str, comment: Option<usize>| match comment {
        Some(at) => text
            .split_at_checked(at)
            .filter(|(_, tail)| tail.starts_with("//"))
            .map(|(code, _)| code.trim_end().to_string()),
        None => Some(text.trim_end().to_string()),
    };
    match (code(after, after_comment), code(before, before_comment)) {
        (Some(after), Some(before)) => !after.trim().is_empty() && after == before,
        _ => false,
    }
}
