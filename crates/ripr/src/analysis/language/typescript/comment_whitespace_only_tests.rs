use super::comment_whitespace_only::{
    is_trailing_comment_or_whitespace_only_change, multiline_construct_lines,
    reconstruct_old_source,
};
use crate::analysis::diff::ChangedLine;
use std::path::Path;

fn noop(old: &str, new: &str) -> bool {
    is_trailing_comment_or_whitespace_only_change(old, new, (1, &[]), (1, &[]))
}

#[test]
fn a_trailing_comment_or_trailing_whitespace_edit_is_a_no_op() {
    assert!(noop("  return price;", "  return price; // fallback"));
    assert!(noop("  return price; // a", "  return price; // b"));
    assert!(noop("  return `n=${n}`;", "  return `n=${n}`;   "));
    assert!(noop("  return price;   ", "  return price;"));
}

#[test]
fn a_changed_token_semicolon_or_indent_is_still_an_edit() {
    assert!(!noop("  return price;", "  return price;"));
    assert!(!noop("  return price;", "  return cost; // fallback"));
    // Dropping the semicolon before a comment can change ASI.
    assert!(!noop("  return f;", "  return f // c"));
    assert!(!noop("  return price;", "    return price;"));
    // `//` inside a string or URL is code, not a comment.
    assert!(!noop("  return 'a';", "  return 'a//b';"));
    assert!(!noop("  return x;", "  return x; /* c */"));
    // A comment-only line has no executable text.
    assert!(!noop("  // a", "  // b"));
    // An unterminated quote cannot be split reliably.
    assert!(!noop("  return `a", "  return `a   "));
}

#[test]
fn a_line_inside_a_multiline_construct_on_either_side_is_never_a_no_op() {
    let check =
        |new_line, new_ranges: &[(usize, usize)], old_line, old_ranges: &[(usize, usize)]| {
            is_trailing_comment_or_whitespace_only_change(
                "  return x;",
                "  return x;   ",
                (new_line, new_ranges),
                (old_line, old_ranges),
            )
        };
    assert!(!check(3, &[(2, 4)], 9, &[]));
    assert!(!check(9, &[], 3, &[(2, 4)]));
    assert!(check(5, &[(2, 4)], 5, &[(2, 4)]));
}

#[test]
fn comment_shapes_that_ride_along_stay_edits() {
    // `//` inside a string, a regex or after a block comment is not a plain
    // trailing comment edit.
    // Text after the first `//` is comment, whatever it contains.
    assert!(noop("  return 'a';", "  return 'a'; // x // y'"));
    assert!(!noop("  return /a/;", "  return /a/; // c"));
    // `//` inside a regex character class is not a comment start.
    assert!(!noop("  return /[ //a]/;", "  return /[ //b]/;"));
    assert!(!noop("  return x;", "  return x; /* c */ // d"));
    assert!(!noop("  return 'a//b';", "  return 'a//c';"));
}

fn line(number: usize, text: &str) -> ChangedLine {
    ChangedLine {
        line: number,
        text: text.to_string(),
        new_side_line: number.min(2),
    }
}

#[test]
fn the_old_source_is_rebuilt_from_the_diff_and_fails_closed_when_it_cannot_be() {
    let new_source = "a\n  const s = `old`;\n  return 7;   \n  // rest\n}\n";
    let added = [
        line(2, "  const s = `old`;"),
        line(3, "  return 7;   "),
        line(4, "  // rest"),
    ];
    let removed = [
        line(2, "  const s = `old"),
        line(3, "  return 7;"),
        line(4, "`; return 0;"),
    ];
    assert_eq!(
        reconstruct_old_source(new_source, &added, &removed).as_deref(),
        Some("a\n  const s = `old\n  return 7;\n`; return 0;\n}\n")
    );
    // A diff that does not line up with the source gets no old source.
    assert_eq!(reconstruct_old_source("a\n", &added, &removed), None);
}

#[test]
fn multiline_constructs_are_found_from_the_whole_file() {
    let source =
        "const a = 1;\nconst b = `x\ny\nz`;\n/* c\nd */\nconst e = 'q\\\nr';\nconst f = 2;\n";
    assert_eq!(
        multiline_construct_lines(Path::new("a.ts"), source),
        Some(vec![(2, 4), (5, 6), (7, 8)])
    );
    assert_eq!(
        multiline_construct_lines(Path::new("a.ts"), "const = ;"),
        None
    );
}

#[test]
fn a_stale_source_or_a_multi_hunk_diff_is_handled_when_rebuilding() {
    // Two hunks: old lines 2 and 5 are replaced, lines between are unchanged.
    let new_source = "a\nB\nc\nd\nE\nf\n";
    let added = [line(2, "B"), line(5, "E")];
    let removed = [line(2, "b"), line(5, "e")];
    assert_eq!(
        reconstruct_old_source(new_source, &added, &removed).as_deref(),
        Some("a\nb\nc\nd\ne\nf\n")
    );
    // An added line that is not the on-disk text means a stale source.
    assert_eq!(
        reconstruct_old_source("a\nX\nc\nd\nE\nf\n", &added, &removed),
        None
    );
}

#[test]
fn a_division_word_inside_a_trailing_comment_is_not_a_regex() {
    assert!(noop("  return a;", "  return a; // in / out"));
}
