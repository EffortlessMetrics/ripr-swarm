//! #15110: an item moved within one file is not a behavior edit.
//!
//! Git shows a moved function as a removed run in one hunk and an added run
//! in another. Nothing the compiler sees for that item changed, yet every
//! probe-bearing line of it was seeded on both sides. A removed run and an
//! added run count as one moved block only when the diff and both files
//! prove all of this:
//!
//! - the two runs hold the same text, line for line, ignoring blank lines at
//!   their ends, and no other run in the file holds that text;
//! - the block reads as the same code in both files (same text with comment
//!   and string contents blanked), so it is not comment or string content on
//!   one side;
//! - the block is one or more complete items: it starts where an item can
//!   start, its delimiters balance, and it ends with the `}` or `;` that
//!   closes its last item;
//! - both copies sit in the same owner: the file's top level, or the same
//!   chain of `mod` blocks ending in a `mod`, `impl` or `trait` block, read
//!   from the block headers with their attributes. Item order means nothing
//!   there; it does inside a function body, an `enum` or a `struct`;
//! - neither owner block opens with an inner attribute (`#![cfg(...)]`
//!   gates the whole block, and the header alone does not show it);
//! - neither file can make item order or source position observable: no
//!   textual macro scope (`macro_rules!`, `#[macro_use]`), no
//!   `std::panic::Location`, no `line`/`column` imported under another
//!   name, and no macro invocation outside a short list of standard macros
//!   whose expansion does not depend on where it is written. A move shifts
//!   the lines of every item between its two positions, so a `line!()` or
//!   a logging macro in an unchanged item would change value with no line
//!   of that item in the diff.
//!
//! Anything else (a move with an edit, a fragment of a body, a move into
//! another `impl`) stays probed.

use crate::analysis::diff::ChangedFile;
use crate::analysis::extract::mask_comments_and_strings;
use std::collections::BTreeSet;

/// Spellings that make item order observable somewhere in a file.
const ORDER_OBSERVERS_IN_FILE: [&str; 2] = ["macro_rules", "macro_use"];

/// Words that make source position observable wherever they appear as a
/// whole word: the caller-location API.
const POSITION_OBSERVERS_IN_FILE: [&str; 1] = ["Location"];

/// Position macros; importing one under another name hides its spelling.
const POSITION_MACROS: [&str; 2] = ["line", "column"];

/// Standard macros whose expansion does not depend on the line or column it
/// is written at. Any other macro (`line!`, `column!`, `dbg!`, a logging
/// macro that records its call site, an imported alias) may, so its
/// presence keeps every move in the file probed.
const POSITION_FREE_MACROS: [&str; 31] = [
    "assert",
    "assert_eq",
    "assert_ne",
    "cfg",
    "compile_error",
    "concat",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "env",
    "eprint",
    "eprintln",
    "file",
    "format",
    "format_args",
    "include",
    "include_bytes",
    "include_str",
    "matches",
    "module_path",
    "option_env",
    "panic",
    "print",
    "println",
    "stringify",
    "todo",
    "unimplemented",
    "unreachable",
    "vec",
    "write",
    "writeln",
];

/// Block keywords under which item order carries no meaning.
const UNORDERED_OWNERS: [&str; 3] = ["mod", "impl", "trait"];

/// New-side and old-side line numbers (1-based) of blocks that only moved.
pub(super) struct MovedItemLines {
    pub(super) added: BTreeSet<usize>,
    pub(super) removed: BTreeSet<usize>,
}

/// One file as the move rule reads it.
struct Side<'a> {
    source: &'a str,
    masked: String,
    /// Byte range of each line, without its line ending.
    lines: Vec<(usize, usize)>,
}

impl<'a> Side<'a> {
    fn of(source: &'a str) -> Self {
        let mut lines = Vec::new();
        let mut start = 0;
        for piece in source.split_inclusive('\n') {
            let body = piece.strip_suffix('\n').unwrap_or(piece);
            let body = body.strip_suffix('\r').unwrap_or(body);
            lines.push((start, start + body.len()));
            start += piece.len();
        }
        Self {
            source,
            masked: mask_comments_and_strings(source),
            lines,
        }
    }

    fn text(&self, line: usize) -> Option<&'a str> {
        let (start, end) = *self.lines.get(line.checked_sub(1)?)?;
        self.source.get(start..end)
    }

    fn masked_text(&self, line: usize) -> Option<&str> {
        let (start, end) = *self.lines.get(line.checked_sub(1)?)?;
        self.masked.get(start..end)
    }

    /// `first..=last` without the blank lines at its ends.
    fn trimmed(&self, first: usize, last: usize) -> Option<(usize, usize)> {
        let blank = |line: usize| self.text(line).is_none_or(|text| text.trim().is_empty());
        let first = (first..=last).find(|line| !blank(*line))?;
        let last = (first..=last).rev().find(|line| !blank(*line))?;
        Some((first, last))
    }

    /// The headers of the blocks enclosing the complete items on
    /// `first..=last`, outermost first, or `None` when the lines are not
    /// complete items in an owner where item order means nothing.
    fn unordered_owner(&self, first: usize, last: usize) -> Option<Vec<String>> {
        let block_start = self.lines.get(first.checked_sub(1)?)?.0;
        let block_end = self.lines.get(last.checked_sub(1)?)?.1;
        let bytes = self.masked.as_bytes();
        // Where the current item began, and the open blocks around it as
        // (their own item start, their `{`).
        let mut item_start = 0;
        let mut open_blocks: Vec<(usize, usize)> = Vec::new();
        let mut grouping = 0usize;
        for (offset, byte) in bytes.get(..block_start)?.iter().enumerate() {
            match byte {
                b'(' | b'[' => grouping += 1,
                b')' | b']' => grouping = grouping.checked_sub(1)?,
                b'{' if grouping == 0 => {
                    open_blocks.push((item_start, offset));
                    item_start = offset + 1;
                }
                b'}' if grouping == 0 => {
                    open_blocks.pop()?;
                    item_start = offset + 1;
                }
                b';' if grouping == 0 => item_start = offset + 1,
                _ => {}
            }
        }
        // The block starts where an item can start: nothing but blanks and
        // comments since the last item ended or the owner opened.
        let prefix = without_inner_attributes(self.masked.get(item_start..block_start)?)?;
        if grouping != 0 || !prefix.trim().is_empty() {
            return None;
        }
        // The block is balanced, never closes its owner, and ends with the
        // delimiter that closes its last item.
        let (mut braces, mut grouping) = (0usize, 0usize);
        let block = bytes.get(block_start..block_end)?;
        for byte in block {
            match byte {
                b'(' | b'[' => grouping += 1,
                b')' | b']' => grouping = grouping.checked_sub(1)?,
                b'{' => braces += 1,
                b'}' => braces = braces.checked_sub(1)?,
                _ => {}
            }
        }
        let closing = block
            .iter()
            .rev()
            .find(|byte| !byte.is_ascii_whitespace())?;
        if braces != 0 || grouping != 0 || !matches!(closing, b'}' | b';') {
            return None;
        }
        let mut headers = Vec::new();
        for (position, (header_start, open)) in open_blocks.iter().enumerate() {
            let keyword = owner_keyword(self.masked.get(*header_start..*open)?)?;
            let innermost = position + 1 == open_blocks.len();
            if keyword != "mod" && !(innermost && UNORDERED_OWNERS.contains(&keyword)) {
                return None;
            }
            // An inner attribute gates the whole block without showing in
            // its header, so two blocks with the same header may differ.
            if self.masked.get(open + 1..)?.trim_start().starts_with("#!") {
                return None;
            }
            // The header as written, attributes and string contents included
            // (`#[cfg(feature = "a")]` differs from `"b"`).
            let header = self.source.get(*header_start..*open)?;
            headers.push(header.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        Some(headers)
    }
}

/// The keyword that opens the block whose header is `masked_header`, after
/// its attributes and qualifiers; `None` when it is not a plain item block.
fn owner_keyword(masked_header: &str) -> Option<&str> {
    let mut rest = masked_header.trim_start();
    while let Some(attribute) = rest.strip_prefix("#[") {
        let mut depth = 1usize;
        let end = attribute.char_indices().find_map(|(at, character)| {
            match character {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(at)
        })?;
        rest = attribute.get(end + 1..)?.trim_start();
    }
    // Qualifiers: `pub`, `pub(crate)`, `pub(in some::path)`, `unsafe`.
    loop {
        rest = rest.trim_start();
        if let Some(after) = strip_word(rest, "pub") {
            let after = after.trim_start();
            rest = match after.strip_prefix('(') {
                Some(inside) => inside.get(inside.find(')')? + 1..)?,
                None => after,
            };
        } else if let Some(after) = strip_word(rest, "unsafe") {
            rest = after;
        } else {
            break;
        }
    }
    let word = rest.split_whitespace().next()?;
    Some(word.split(['<', '(']).next().unwrap_or(word))
}

/// `text` after `word` when `text` starts with `word` as a whole word.
fn strip_word<'t>(text: &'t str, word: &str) -> Option<&'t str> {
    let after = text.strip_prefix(word)?;
    let continues = after
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    (!continues).then_some(after)
}

/// `masked` without the complete inner attributes (`#![...]`) at its start;
/// `None` when one does not close.
fn without_inner_attributes(masked: &str) -> Option<&str> {
    let mut rest = masked.trim_start();
    while let Some(attribute) = rest.strip_prefix("#!") {
        let attribute = attribute.trim_start().strip_prefix('[')?;
        let mut depth = 1usize;
        let end = attribute.char_indices().find_map(|(at, character)| {
            match character {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(at)
        })?;
        rest = attribute.get(end + 1..)?.trim_start();
    }
    Some(rest)
}

/// The source lines `first..=last` of `side`.
fn block_text<'a>(side: &Side<'a>, (first, last): (usize, usize)) -> Option<Vec<&'a str>> {
    (first..=last).map(|line| side.text(line)).collect()
}

/// The same lines with comment and string contents blanked.
fn masked_text<'s>(side: &'s Side<'_>, (first, last): (usize, usize)) -> Option<Vec<&'s str>> {
    (first..=last).map(|line| side.masked_text(line)).collect()
}

/// Whether `masked` code can observe where its items are written: it names
/// the caller-location API, imports a position macro under another name, or
/// invokes a macro outside [`POSITION_FREE_MACROS`]. Rust allows blanks and
/// comments (blanked here) between a macro's name and its `!`, also across
/// lines.
fn observes_source_position(masked: &str) -> bool {
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let bytes = masked.as_bytes();
    let whole_word = |at: usize, len: usize| {
        !at.checked_sub(1)
            .and_then(|before| bytes.get(before))
            .is_some_and(|byte| is_word(*byte))
            && !bytes.get(at + len).is_some_and(|byte| is_word(*byte))
    };
    let named = POSITION_OBSERVERS_IN_FILE.iter().any(|name| {
        masked
            .match_indices(name)
            .any(|(at, _)| whole_word(at, name.len()))
    });
    let aliased = POSITION_MACROS.iter().any(|name| {
        masked.match_indices(name).any(|(at, _)| {
            whole_word(at, name.len())
                && strip_word(masked[at + name.len()..].trim_start(), "as").is_some()
        })
    });
    // Every `!` that ends a macro name and opens its arguments: not `!=`,
    // not a negation (`return !done`), not a negative impl, not an inner
    // attribute.
    let unvetted_macro = masked.match_indices('!').any(|(at, _)| {
        if !masked[at + 1..].trim_start().starts_with(['(', '[', '{']) {
            return false;
        }
        let name_end = masked[..at].trim_end().len();
        // Past the whole character before the name, which may be multibyte
        // (`\u{85}` is trimmed as a blank but `e\u{301}` is not).
        let name_start = masked[..name_end]
            .char_indices()
            .rfind(|(_, character)| !(character.is_ascii_alphanumeric() || *character == '_'))
            .map_or(0, |(before, character)| before + character.len_utf8());
        let name = &masked[name_start..name_end];
        !name.is_empty()
            && !name.starts_with(|character: char| character.is_ascii_digit())
            && !POSITION_FREE_MACROS.contains(&name)
            && name != "macro_rules"
    });
    named || aliased || unvetted_macro
}

/// Maximal runs of consecutive line numbers, as `(first, last)`.
fn runs(lines: impl IntoIterator<Item = usize>) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for line in lines.into_iter().collect::<BTreeSet<_>>() {
        match runs.last_mut() {
            Some((_, last)) if *last + 1 == line => *last = line,
            _ => runs.push((line, line)),
        }
    }
    runs
}

/// Lines of `changed` that belong to an item block moved within the file
/// with no other change (see the module documentation for the proof each
/// block needs).
pub(super) fn moved_item_lines(
    changed: &ChangedFile,
    new_source: &str,
    old_source: &str,
) -> MovedItemLines {
    let mut moved = MovedItemLines {
        added: BTreeSet::new(),
        removed: BTreeSet::new(),
    };
    if changed.added_lines.is_empty() || changed.removed_lines.is_empty() {
        return moved;
    }
    let (new, old) = (Side::of(new_source), Side::of(old_source));
    if [&new, &old].iter().any(|side| {
        ORDER_OBSERVERS_IN_FILE
            .iter()
            .any(|observer| side.masked.contains(observer))
            || observes_source_position(&side.masked)
    }) {
        return moved;
    }
    let added_runs = runs(changed.added_lines.iter().map(|line| line.new_side_line))
        .into_iter()
        .filter_map(|(first, last)| new.trimmed(first, last))
        .collect::<Vec<_>>();
    let removed_runs = runs(changed.removed_lines.iter().map(|line| line.line))
        .into_iter()
        .filter_map(|(first, last)| old.trimmed(first, last))
        .collect::<Vec<_>>();
    for &removed_run in &removed_runs {
        let Some(text) = block_text(&old, removed_run) else {
            continue;
        };
        let same_text = |run: &(usize, usize)| block_text(&new, *run).as_ref() == Some(&text);
        let mut twins = added_runs.iter().filter(|run| same_text(run));
        let (Some(&added_run), None) = (twins.next(), twins.next()) else {
            continue;
        };
        let unique_on_old_side = removed_runs
            .iter()
            .filter(|run| block_text(&old, **run).as_ref() == Some(&text))
            .count()
            == 1;
        let masked = masked_text(&old, removed_run);
        let same_code = masked.is_some() && masked == masked_text(&new, added_run);
        if !unique_on_old_side || !same_code {
            continue;
        }
        let old_owner = old.unordered_owner(removed_run.0, removed_run.1);
        if old_owner.is_none() || old_owner != new.unordered_owner(added_run.0, added_run.1) {
            continue;
        }
        moved.removed.extend(removed_run.0..=removed_run.1);
        moved.added.extend(added_run.0..=added_run.1);
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::moved_item_lines;
    use crate::analysis::diff::{ChangedFile, ChangedLine};

    /// `old` with the lines `first..=last` (1-based) moved to just before
    /// old line `before`, and the diff git would write for that move.
    fn moved(old: &str, first: usize, last: usize, before: usize) -> (String, ChangedFile) {
        let lines: Vec<&str> = old.lines().collect();
        let block = &lines[first - 1..last];
        let mut new_lines = Vec::new();
        let mut changed = ChangedFile::default();
        for (at, line) in lines.iter().enumerate() {
            let number = at + 1;
            if number == before {
                for text in block {
                    new_lines.push(*text);
                    changed.added_lines.push(ChangedLine {
                        line: new_lines.len(),
                        new_side_line: new_lines.len(),
                        text: (*text).to_string(),
                    });
                }
            }
            if (first..=last).contains(&number) {
                changed.removed_lines.push(ChangedLine {
                    line: number,
                    new_side_line: new_lines.len() + 1,
                    text: (*line).to_string(),
                });
            } else {
                new_lines.push(*line);
            }
        }
        (format!("{}\n", new_lines.join("\n")), changed)
    }

    fn counts(old: &str, new: &str, changed: &ChangedFile) -> (usize, usize) {
        let lines = moved_item_lines(changed, new, old);
        (lines.added.len(), lines.removed.len())
    }

    const TOP_LEVEL: &str = "pub fn discount(total: u32) -> u32 {\n    if total > 100 {\n        total - 10\n    } else {\n        total\n    }\n}\n\npub fn tax(total: u32) -> u32 {\n    total / 10\n}\n\n#[cfg(test)]\nmod tests {\n    fn helper() {}\n}\n";

    #[test]
    fn a_complete_item_moved_in_its_owner_is_neutral() {
        // `discount` plus its blank line moves below `tax`.
        let (new, changed) = moved(TOP_LEVEL, 1, 8, 13);
        assert_eq!(counts(TOP_LEVEL, &new, &changed), (7, 7));

        // A method moved inside one `impl`, with its attribute.
        let old = "struct S;\n\nimpl S {\n    #[inline]\n    fn a(&self) -> u8 {\n        1\n    }\n\n    fn b(&self) -> u8 {\n        2\n    }\n\n    fn c(&self) {}\n}\n";
        let (new, changed) = moved(old, 4, 8, 13);
        assert_eq!(counts(old, &new, &changed), (4, 4));
    }

    #[test]
    fn a_move_that_can_change_meaning_stays_probed() {
        // Statements inside a function body: order is behavior.
        let old = "fn f() {\n    first();\n    second();\n    third();\n}\n";
        let (new, changed) = moved(old, 2, 2, 4);
        assert_eq!(counts(old, &new, &changed), (0, 0));

        // An `enum` variant: order sets discriminants.
        let old = "enum E {\n    A,\n    B,\n    C,\n}\n";
        let (new, changed) = moved(old, 2, 2, 4);
        assert_eq!(counts(old, &new, &changed), (0, 0));

        // Out of one `impl` into another.
        let old = "impl A {\n    fn one(&self) {}\n\n    fn two(&self) {}\n}\n\nimpl B {\n    fn three(&self) {}\n}\n";
        let (new, changed) = moved(old, 2, 3, 8);
        assert_eq!(counts(old, &new, &changed), (0, 0));

        // Between two blocks whose headers differ only in a string.
        let old = "#[cfg(feature = \"a\")]\nmod m {\n    fn one() {}\n\n    fn two() {}\n}\n\n#[cfg(feature = \"b\")]\nmod m {\n    fn three() {}\n}\n";
        let (new, changed) = moved(old, 3, 4, 10);
        assert_eq!(counts(old, &new, &changed), (0, 0));

        // A fragment: the body without its signature.
        let (new, changed) = moved(TOP_LEVEL, 2, 6, 10);
        assert_eq!(counts(TOP_LEVEL, &new, &changed), (0, 0));

        // An item that leaves its attribute behind.
        let old = "#[inline]\nfn a() {}\n\nfn b() {}\n\nfn c() {}\n";
        let (new, changed) = moved(old, 2, 3, 6);
        assert_eq!(counts(old, &new, &changed), (0, 0));

        // Textual macro scope anywhere in the file.
        let old = format!("macro_rules! m {{ () => {{}} }}\n\n{TOP_LEVEL}");
        let (new, changed) = moved(&old, 3, 10, 15);
        assert_eq!(counts(&old, &new, &changed), (0, 0));

        // A source position macro in an unchanged item the move passes: its
        // line number shifts with no line of it in the diff.
        let old = "fn a() {}\n\nfn where_am_i() -> u32 {\n    line!()\n}\n\nfn last() {}\n";
        let (new, changed) = moved(old, 1, 2, 7);
        assert_eq!(counts(old, &new, &changed), (0, 0));

        // A source position macro inside the block.
        for invocation in [
            "line!()",
            "line !()",
            "line /* here */ !()",
            "column\n    !()",
        ] {
            let old = format!(
                "fn where_am_i() -> u32 {{\n    {invocation}\n}}\n\nfn other() {{}}\n\nfn last() {{}}\n"
            );
            let last = old.lines().count();
            let (new, changed) = moved(&old, 1, last - 3, last);
            assert_eq!(counts(&old, &new, &changed), (0, 0), "{invocation}");
        }
        // A name that only ends in `line`, and a negated `line`, are not it.
        let old = "fn f(line: bool, outline: bool) -> bool {\n    line != outline\n}\n\nfn other() {}\n\nfn last() {}\n";
        let (new, changed) = moved(old, 1, 4, 7);
        assert_eq!(counts(old, &new, &changed), (3, 3));

        // A position macro imported under another name, the caller-location
        // API, and a macro that may record its call site.
        for observer in [
            "use std::line as source_line;\n\nfn early() -> u32 {\n    source_line!()\n}\n",
            "use std::{column as here};\n\nfn early() {}\n",
            "fn early() -> u32 {\n    std::panic::Location::caller().line()\n}\n",
            "fn early() {\n    tracing::info!(\"start\");\n}\n",
            "fn early() {\n    dbg!(1);\n}\n",
        ] {
            let old = format!("{observer}\nfn a() {{}}\n\nfn b() {{}}\n\nfn c() {{}}\n");
            let first = observer.lines().count() + 2;
            let (new, changed) = moved(&old, first, first + 1, first + 4);
            assert_eq!(counts(&old, &new, &changed), (0, 0), "{observer}");
        }
        // Standard position-free macros, negation and a negative impl do not
        // block a neutral move.
        let old = "fn early(done: bool) -> bool {\n    assert!(true);\n    println!(\"{}\", vec![1].len());\n    return !done;\n}\n\nfn a() {}\n\nfn b() {}\n\nfn c() {}\n";
        let (new, changed) = moved(old, 7, 8, 11);
        assert_eq!(counts(old, &new, &changed), (1, 1));
    }

    #[test]
    fn owner_qualifiers_and_inner_attributes() {
        // Two `impl S` blocks gated by different inner attributes.
        let old = "struct S;\n\nimpl S {\n    #![cfg(feature = \"a\")]\n    fn moved(&self) {}\n\n    fn anchor_a(&self) {}\n}\n\nimpl S {\n    #![cfg(feature = \"b\")]\n    fn anchor_b(&self) {}\n}\n";
        let (new, changed) = moved(old, 5, 6, 13);
        assert_eq!(counts(old, &new, &changed), (0, 0));

        // A crate-level inner attribute before the first item is not part
        // of that item.
        let old = "#![forbid(unsafe_code)]\npub fn a(n: u32) -> bool {\n    n > 10\n}\n\npub fn b() {}\n\npub fn c() {}\n";
        let (new, changed) = moved(old, 2, 5, 8);
        assert_eq!(counts(old, &new, &changed), (3, 3));

        // A module with restricted visibility is still a module.
        let old = "mod outer {\n    pub(in crate::outer) mod m {\n        fn a() {}\n\n        fn b() {}\n\n        fn c() {}\n    }\n}\n";
        let (new, changed) = moved(old, 3, 4, 7);
        assert_eq!(counts(old, &new, &changed), (1, 1));
    }

    #[test]
    fn a_multibyte_character_before_a_macro_name_is_read_whole() {
        // `\u{85}` is a blank to `trim_end`; `\u{301}` is a combining mark.
        // Both are two bytes, so the name starts past the whole character.
        for (source, observes) in [
            ("let s = \u{85}stringify!(x);", false),
            ("let s = stringify\u{85}!(x);", false),
            ("e\u{301}assert!(x);", false),
            ("let s = \u{85}tracing_info!(x);", true),
        ] {
            assert_eq!(
                super::observes_source_position(source),
                observes,
                "{source:?}"
            );
        }
    }

    #[test]
    fn a_move_with_an_edit_or_into_a_comment_stays_probed() {
        // The moved copy changes one comparison.
        let (new, mut changed) = moved(TOP_LEVEL, 1, 8, 13);
        let edited = new.replacen("total > 100", "total >= 100", 1);
        for line in &mut changed.added_lines {
            line.text = line.text.replace("total > 100", "total >= 100");
        }
        assert_eq!(counts(TOP_LEVEL, &edited, &changed), (0, 0));

        // The moved copy lands inside a block comment.
        let old = "fn a() {}\n\nfn b() {}\n\n/*\n*/\n";
        let (new, changed) = moved(old, 1, 2, 6);
        assert_eq!(counts(old, &new, &changed), (0, 0));
    }
}
