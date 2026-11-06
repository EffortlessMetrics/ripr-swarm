//! Issue #7483: a division after a postfix `!` or `++` is not a regex.

use super::classifier::ts_mask_literals_and_comments;

#[test]
fn division_after_non_null_or_postfix_keeps_the_rest_of_the_line_visible() {
    for (label, source) in [
        (
            "non-null assertion",
            "const h = n! / 2; const Forbidden = NotFound; const q = n / 3;",
        ),
        (
            "postfix increment",
            "const h = i++ / 2; const Forbidden = NotFound; const q = n / 3;",
        ),
        (
            "non-null call result",
            "const h = f()! / 2; const Forbidden = NotFound; const q = n / 3;",
        ),
    ] {
        let masked = ts_mask_literals_and_comments(source);
        assert!(
            masked.contains("Forbidden = NotFound"),
            "{label}: {masked:?}"
        );
    }
}

#[test]
fn prefix_not_comparisons_and_double_assertions_keep_their_meaning() {
    for (label, source, visible) in [
        (
            "double non-null",
            "const h = n!! / 2; const Forbidden = NotFound; const q = n / 3;",
            true,
        ),
        (
            "strict inequality then division",
            "const h = a !== b / 2; const Forbidden = NotFound; const q = n / 3;",
            true,
        ),
        (
            "prefix not after return",
            "function f() { return !/NotFound/.test(x); }",
            false,
        ),
        (
            "prefix not in a condition",
            "if (!/NotFound/.test(x)) { go(); }",
            false,
        ),
    ] {
        let masked = ts_mask_literals_and_comments(source);
        assert_eq!(masked.contains("NotFound"), visible, "{label}: {masked:?}");
    }
}

#[test]
fn prefix_not_before_a_regex_still_masks_the_regex_body() {
    let masked = ts_mask_literals_and_comments("ok = !/NotFound/.test(x); y = 1;");
    assert!(!masked.contains("NotFound"), "{masked:?}");
}
