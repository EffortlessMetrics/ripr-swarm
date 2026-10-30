//! #10706: a trailing comment or inter-token whitespace added to a line is a
//! no-op. It yields no probe, so no gap id or discriminator can embed the
//! comment text. Anything that changes a token keeps its probe.

use super::no_behavior::is_comment_or_whitespace_only_change;
use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER: &str = "def discount(price):\n    return price\n";
const TEST: &str =
    "from src.pricing import discount\n\n\ndef test_discount():\n    assert discount(100) == 100\n";

fn classify_edit(old: &str, new: &str) -> Option<Finding> {
    let source = OWNER.replace("    return price", new);
    let owners = extract_owners(Path::new("src/pricing.py"), &source);
    let tests = extract_tests(Path::new("tests/test_pricing.py"), TEST);
    classify_change_with_old(
        Path::new("src/pricing.py"),
        2,
        new,
        Some(old),
        &owners,
        &tests,
    )
}

#[test]
fn a_trailing_comment_or_whitespace_edit_emits_no_probe() {
    let old = "    return price";
    for new in [
        "    return price  # fallback",
        "    return price # fallback",
        "    return price   ",
        "    return price\t",
    ] {
        assert!(classify_edit(old, new).is_none(), "probe for {new:?}");
    }
}

#[test]
fn a_changed_token_keeps_its_probe() {
    let old = "    return price";
    for new in [
        "    return price + 1  # fallback",
        "    return cost  # fallback",
        "    return 'price'",
    ] {
        assert!(classify_edit(old, new).is_some(), "no probe for {new:?}");
    }
}

#[test]
fn the_edit_predicate_is_token_exact_and_fails_closed() {
    let same = is_comment_or_whitespace_only_change;
    assert!(same("    x = f(a, b)", "    x = f(a, b)  # why"));
    assert!(same("    x = f(a, b)  # old", "    x = f(a, b)  # new"));
    assert!(same("    x = f(a,b)", "    x = f( a, b )"));
    // A `#` inside a string is data, not a comment.
    assert!(!same("    x = 'a'", "    x = 'a#b'"));
    assert!(!same("    x = 'a # one'", "    x = 'a # two'"));
    // A respelling is still an edit: only spacing and comments are ignored.
    assert!(!same("    x = 0x1", "    x = 1  # why"));
    assert!(!same("    x = \"a\"", "    x = 'a'"));
    // A changed token, a changed indentation and an identical line are edits.
    assert!(!same("    x = f(a)", "    x = f(b)  # why"));
    assert!(!same("    x = f(a)", "        x = f(a)  # why"));
    assert!(!same("    x = f(a)", "    x = f(a)"));
    // A comment-only line has no tokens to compare, so another guard owns it.
    assert!(!same("    # one", "    # two"));
    // A fragment of a multi-line construct does not lex, so it stays an edit.
    assert!(!same("    f(a,", "    f(a,  # why"));
}

#[test]
fn string_content_lines_are_found_from_the_whole_file() {
    use super::no_behavior::multi_line_string_line_ranges;
    let source = "def f():\n    sql = \"\"\"\n    SELECT a FROM t\n    \"\"\"\n    return sql\n";
    assert_eq!(multi_line_string_line_ranges(source), Some(vec![2..=4]));
    assert_eq!(
        multi_line_string_line_ranges("x = 'a'\ny = f'{x}'\n"),
        Some(Vec::new())
    );
    // A file that does not lex yields no answer, so the caller fails closed.
    assert_eq!(multi_line_string_line_ranges("x = '''open\n"), None);
}
