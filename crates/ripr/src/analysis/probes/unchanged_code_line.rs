//! #14175: an edit that leaves a line's code unchanged and only adds, removes
//! or rewords its trailing line comment seeds no probe. The seeding loop in
//! `diff.rs` asks these helpers; they read comment starts from the shared
//! lexical scanner so a `//` inside a string or block comment stays code.
//!
//! Equal text is not equal meaning when another edited line moved a comment
//! or string boundary, so every neutral pair also needs [`same_code_context`]:
//! the two lines must read as the same code, and hold the same string
//! contents, in their own files.

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
/// before the comment, for a documentation comment (`///`, `//!`) on either
/// side, and for a line that is only a comment.
pub(super) fn is_trailing_line_comment_only_change(
    after: &str,
    after_comment: Option<usize>,
    before: &str,
    before_comment: Option<usize>,
) -> bool {
    if after == before || (after_comment.is_none() && before_comment.is_none()) {
        return false;
    }
    // `///` and `//!` are documentation attributes, tokens a macro can
    // match, so they are not comments here; `////` is an ordinary comment.
    let is_doc = |tail: &str| {
        tail.starts_with("//!") || (tail.starts_with("///") && !tail.starts_with("////"))
    };
    let code = |text: &'_ str, comment: Option<usize>| match comment {
        Some(at) => text
            .split_at_checked(at)
            .filter(|(_, tail)| tail.starts_with("//") && !is_doc(tail))
            .map(|(code, _)| code.trim_end().to_string()),
        None => Some(text.trim_end().to_string()),
    };
    match (code(after, after_comment), code(before, before_comment)) {
        (Some(after), Some(before)) => !after.trim().is_empty() && after == before,
        _ => false,
    }
}

/// One line read on its own: its masked code with whitespace runs collapsed,
/// and the raw text of each string and character literal in order. `None`
/// when the line ends inside a block comment or a string (it would not read
/// the same on its own as in its file), or when it holds a documentation
/// block comment (`/**`, `/*!`), which is an attribute a macro can match.
fn inline_code(line: &str) -> Option<(String, Vec<&str>)> {
    let probe = format!("{line}\nX");
    let masked = crate::analysis::extract::mask_comments_and_strings(&probe);
    if !masked.ends_with('X') {
        return None;
    }
    let ranges = crate::analysis::extract::rust_identity_spans(line).literal_ranges;
    let literals: Vec<&str> = ranges
        .iter()
        .filter_map(|&(open, close)| line.get(open..=close))
        .collect();
    let masked_line = masked.get(..line.len())?;
    let is_comment = |at: usize| {
        masked_line.as_bytes().get(at) == Some(&b' ')
            && !ranges
                .iter()
                .any(|&(open, close)| (open..=close).contains(&at))
    };
    let doc_comment = line.match_indices("/*").any(|(at, _)| {
        let tail = &line[at..];
        is_comment(at)
            && (tail.starts_with("/*!")
                || (tail.starts_with("/**")
                    && !tail.starts_with("/***")
                    && !tail.starts_with("/**/")))
    });
    if doc_comment {
        return None;
    }
    let code = masked_line.split_whitespace().collect::<Vec<_>>().join(" ");
    Some((code, literals))
}

/// Whether `after` and `before` hold the same code and differ only in block
/// comments inside the line (`if x > 1 /* old */ {` reworded, added or
/// removed) and the spacing around them. Each line is read on its own, so
/// the caller must also check that each reads the same in its own file
/// ([`reads_alone_as_in_file`]). Fails closed when the texts are equal, when
/// neither side holds a block comment, when either line ends inside a
/// comment or string, for a documentation block comment, and for a line
/// with no code. A comment that separated two tokens (`a/*x*/b` against
/// `ab`) changes the collapsed code and keeps its probe.
pub(super) fn is_inline_block_comment_only_change(after: &str, before: &str) -> bool {
    if after == before || !(after.contains("/*") || before.contains("/*")) {
        return false;
    }
    match (inline_code(after), inline_code(before)) {
        (Some(after), Some(before)) => !after.0.is_empty() && after == before,
        _ => false,
    }
}

/// Whether `text`, the changed text of 1-based `line` of `file`, reads as
/// the same code alone as it does in its file: the masked line in the file,
/// whitespace collapsed, equals the line masked on its own. A line inside a
/// multi-line comment or string in its file masks differently and fails.
pub(super) fn reads_alone_as_in_file(file: &LexicalLines, line: usize, text: &str) -> bool {
    let alone = crate::analysis::extract::mask_comments_and_strings(text);
    let collapse = |masked: &str| masked.split_whitespace().collect::<Vec<_>>().join(" ");
    file.code(line, None)
        .is_some_and(|(in_file, _)| collapse(in_file) == collapse(&alone))
}

/// One file read by the shared lexical scanner: its text with comment and
/// string contents blanked, each line's byte range, and each string
/// literal's delimiter offsets.
pub(super) struct LexicalLines {
    masked: String,
    lines: Vec<(usize, usize)>,
    strings: Vec<(usize, usize)>,
}

impl LexicalLines {
    pub(super) fn of(source: &str) -> Self {
        // Each line's byte range without its line ending, so a CRLF line
        // and its LF twin cover the same columns.
        let mut lines = Vec::new();
        let mut start = 0;
        for piece in source.split_inclusive('\n') {
            let body = piece.strip_suffix('\n').unwrap_or(piece);
            let body = body.strip_suffix('\r').unwrap_or(body);
            lines.push((start, start + body.len()));
            start += piece.len();
        }
        Self {
            masked: crate::analysis::extract::mask_comments_and_strings(source),
            lines,
            strings: crate::analysis::extract::string_literal_spans(source)
                .into_iter()
                .map(|(open, close, _)| (open, close))
                .collect(),
        }
    }

    /// The 1-based `line` up to byte column `cut` (the whole line when
    /// `None`): its masked text without trailing whitespace, and the parts of
    /// string literals that lie in it, as columns.
    fn code(&self, line: usize, cut: Option<usize>) -> Option<(&str, Vec<(usize, usize)>)> {
        let (start, line_end) = *self.lines.get(line.checked_sub(1)?)?;
        let end = match cut {
            Some(cut) => start.checked_add(cut).filter(|end| *end <= line_end)?,
            None => line_end,
        };
        let text = self.masked.get(start..end)?.trim_end();
        let strings = self
            .strings
            .iter()
            .filter(|(open, close)| *open < end && *close >= start)
            .map(|(open, close)| (open.max(&start) - start, close.min(&end) - start))
            .collect();
        Some((text, strings))
    }
}

/// Whether line `new_line` of the new file and `old_line` of the old file,
/// each up to its own `*_cut` column, read as the same code in their own
/// files (#14175). A line that is live code on one side and comment text on
/// the other differs in its masked form; a line that is string content on one
/// side only differs in its string columns. Fails closed for a line or column
/// outside its file.
pub(super) fn same_code_context(
    new: &LexicalLines,
    new_line: usize,
    new_cut: Option<usize>,
    old: &LexicalLines,
    old_line: usize,
    old_cut: Option<usize>,
) -> bool {
    match (new.code(new_line, new_cut), old.code(old_line, old_cut)) {
        (Some(new), Some(old)) => new == old,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LexicalLines, is_inline_block_comment_only_change, is_trailing_line_comment_only_change,
        reads_alone_as_in_file, same_code_context,
    };

    #[test]
    fn a_block_comment_inside_the_line_is_not_code() {
        for (after, before) in [
            (
                "    if total > 100 /* the threshold for big orders */ {",
                "    if total > 100 /* threshold */ {",
            ),
            (
                "    if total > 100 {",
                "    if total > 100 /* threshold */ {",
            ),
            ("    f(a, /* b */ c);", "    f(a, c);"),
            (
                "    let s = \"/* x */\"; /* c */",
                "    let s = \"/* x */\";",
            ),
            ("    if x /* \u{e9}t\u{e9} */ > 1 {", "    if x > 1 {"),
        ] {
            assert!(
                is_inline_block_comment_only_change(after, before),
                "{after:?} vs {before:?}"
            );
        }
        for (after, before) in [
            // Code changed beside the comment.
            (
                "    if total >= 100 /* c */ {",
                "    if total > 100 /* c */ {",
            ),
            // The comment separated two tokens.
            ("    let v = ab;", "    let v = a/* x */b;"),
            // A changed string literal, comment or not.
            ("    let s = \"a  b\"; /* c */", "    let s = \"a b\";"),
            // Documentation block comments are attributes.
            ("    f(/** doc */ x);", "    f(x);"),
            ("    f(/*! doc */ x);", "    f(x);"),
            // The line ends inside a block comment or a string.
            ("    f(x); /* open", "    f(x); /* other"),
            ("    let s = \"open", "    let s = \"open /* c */"),
            // No code, no comment, or no change.
            ("    /* a */", "    /* b */"),
            ("    f(x);", "    f( x );"),
            ("    f(x); /* c */", "    f(x); /* c */"),
        ] {
            assert!(
                !is_inline_block_comment_only_change(after, before),
                "{after:?} vs {before:?}"
            );
        }
    }

    #[test]
    fn a_line_inside_a_multi_line_comment_or_string_does_not_read_alone() {
        let file = "fn f() {\n    /*\n    g(); /* c */\n    */\n    h(); /* c */\n    let s = \"\n    k(); /* c */\n    \";\n}\n";
        let lines = LexicalLines::of(file);
        assert!(reads_alone_as_in_file(&lines, 5, "    h(); /* c */"));
        assert!(!reads_alone_as_in_file(&lines, 7, "    k(); /* c */"));
        assert!(!reads_alone_as_in_file(&lines, 99, "    h();"));
    }

    #[test]
    fn documentation_comments_are_tokens_not_comments() {
        for (after, before) in [
            ("flag!(0 /// note", "flag!(0 // note"),
            ("flag!(0 // note", "flag!(0 /// note"),
            ("flag!(0 //! note", "flag!(0 // note"),
            ("flag!(0 /// b", "flag!(0 /// a"),
        ] {
            assert!(
                !is_trailing_line_comment_only_change(after, Some(8), before, Some(8)),
                "{after} vs {before}"
            );
        }
        // Four slashes are an ordinary comment.
        assert!(is_trailing_line_comment_only_change(
            "flag!(0 //// note",
            Some(8),
            "flag!(0 // note",
            Some(8)
        ));
    }

    #[test]
    fn equal_text_under_a_moved_comment_or_string_boundary_is_not_the_same_code() {
        let same = |new: &str, new_line, old: &str, old_line| {
            same_code_context(
                &LexicalLines::of(new),
                new_line,
                None,
                &LexicalLines::of(old),
                old_line,
                None,
            )
        };
        let live = "fn f() {\r\n    // disabled\r\n    g();\r\n    //\r\n}\r\n";
        let disabled = "fn f() {\n    /* disabled\n    g();\n    */\n}\n";
        // The statement is live on one side and comment text on the other.
        assert!(!same(live, 3, disabled, 3));
        assert!(!same(disabled, 3, live, 3));
        // Lines outside the moved boundary, and a pure line-ending rewrite.
        assert!(same(live, 1, disabled, 1));
        assert!(same(live, 5, disabled, 5));
        let lf = live.replace("\r\n", "\n");
        for line in 1..=5 {
            assert!(same(live, line, &lf, line), "line {line}");
        }
        // String content on one side, comment text on the other: both mask
        // to blanks, the string columns tell them apart.
        let string = "fn f() {\n    let _s = \"\n    g();\n    \";\n}\n";
        assert!(!same(string, 3, disabled, 3));
        assert!(!same(string, 3, &lf, 3));
        // The same multi-line string on both sides is the same content.
        assert!(same(string, 3, &string.replace('\n', "\r\n"), 3));
        // A line or column outside the file.
        assert!(!same(live, 9, disabled, 3));
        assert!(!same_code_context(
            &LexicalLines::of(live),
            3,
            Some(400),
            &LexicalLines::of(disabled),
            3,
            None
        ));
    }
}
