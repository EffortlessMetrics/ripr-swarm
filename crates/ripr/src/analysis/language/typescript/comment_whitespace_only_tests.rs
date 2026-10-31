use super::comment_whitespace_only::{
    is_trailing_comment_or_whitespace_only_change, multiline_construct_lines,
};
use std::path::Path;

fn noop(old: &str, new: &str) -> bool {
    is_trailing_comment_or_whitespace_only_change(old, new, 1, &[])
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
fn a_line_inside_a_multiline_construct_is_never_a_no_op() {
    assert!(!is_trailing_comment_or_whitespace_only_change(
        "  return x;",
        "  return x;   ",
        3,
        &[(2, 4)]
    ));
    assert!(is_trailing_comment_or_whitespace_only_change(
        "  return x;",
        "  return x;   ",
        5,
        &[(2, 4)]
    ));
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
