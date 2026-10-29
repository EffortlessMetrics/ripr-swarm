use super::infection_shape::ts_unmodeled_infection;

fn shape(old: &str, new: &str) -> Option<&'static str> {
    ts_unmodeled_infection(Some(old), new).map(|found| found.shape)
}

#[test]
fn names_each_unmodeled_shape() {
    assert_eq!(
        shape("  return price;", "  return price; // fallback"),
        Some("comment_only")
    );
    assert_eq!(
        shape("  return a / b;", "  return a / b + 0;"),
        Some("arithmetic_noop")
    );
    assert_eq!(
        shape("  return opts?.max ?? 10;", "  return opts?.max ?? 20;"),
        Some("fallback_literal")
    );
    assert_eq!(
        shape("  return x || \"a\";", "  return x || \"b\";"),
        Some("fallback_literal")
    );
    assert_eq!(
        shape(
            "export function say(msg: string, loud = false): string {",
            "export function say(msg: string, loud = true): string {"
        ),
        Some("default_parameter")
    );
    assert_eq!(
        shape("    return 5;", "    return 6;"),
        Some("literal_return")
    );
}

#[test]
fn behavior_neutral_only_for_comment_and_identity_edits() {
    let neutral = |old, new| ts_unmodeled_infection(Some(old), new).map(|s| s.behavior_neutral);
    assert_eq!(neutral("return p;", "return p; // c"), Some(true));
    assert_eq!(neutral("return a;", "return a * 1;"), Some(true));
    assert_eq!(neutral("return 1;", "return 2;"), Some(false));
}

#[test]
fn modeled_or_ambiguous_edits_stay_unnamed() {
    // A relational predicate is modeled elsewhere.
    assert_eq!(shape("if (qty >= 10) {", "if (qty > 10) {"), None);
    // A non-identity operand is a real value change.
    assert_eq!(shape("return a / b;", "return a / b + 1;"), None);
    // A `//` inside a string is code, not a comment.
    assert_eq!(
        shape("return \"a//b\";", "return \"a//c\";"),
        Some("literal_return")
    );
    // A non-literal fallback or return is not this shape.
    assert_eq!(shape("return x ?? y;", "return x ?? z;"), None);
    assert_eq!(shape("return a;", "return b;"), None);
    // A changed head next to the same fallback is not a fallback-only edit.
    assert_eq!(shape("return a ?? 1;", "return b ?? 2;"), None);
    // An added line has no old side to compare.
    assert_eq!(ts_unmodeled_infection(None, "return 6;"), None);
    // Unchanged text.
    assert_eq!(shape("return 6;", "return 6;"), None);
}

#[test]
fn names_a_single_numeric_operand_edit() {
    assert_eq!(
        shape(
            "return Math.floor(price * 0.9);",
            "return Math.floor(price * 0.8);"
        ),
        Some("numeric_operand")
    );
    assert_eq!(
        shape("return x * 10;", "return x * 12;"),
        Some("numeric_operand")
    );
    // Two literals changed, or an identifier suffix, is not one operand edit.
    assert_eq!(shape("return 1 + 2;", "return 3 + 4;"), None);
    assert_eq!(shape("return v1 * 2;", "return v2 * 2;"), None);
}
