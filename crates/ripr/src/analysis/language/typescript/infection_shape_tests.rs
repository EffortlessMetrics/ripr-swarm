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
fn a_fallback_literal_stops_where_its_quote_closes() {
    // Positive: adjacent quoted operands are each a literal token.
    assert_eq!(
        shape("return x ?? 'a'+tail;", "return x ?? 'b'+tail;"),
        Some("fallback_literal")
    );
    // Negative: a changed tail after the closing quote is not a literal-only edit.
    assert_eq!(shape("return x ?? 'a'+t1;", "return x ?? 'b'+t2;"), None);
}

#[test]
fn operators_and_parens_inside_strings_are_not_structure() {
    // Negative: `??` and `(` inside a string do not make a fallback or default edit.
    assert_eq!(
        shape("return f(\"a ?? 1\" + x);", "return f(\"a ?? 2\" + x);"),
        None
    );
    assert_eq!(shape("log(\"(a = 1)\", z);", "log(\"(a = 2)\", z);"), None);
    // Positive: the real fallback after a string that holds `??` is found.
    assert_eq!(
        shape("return \"a ?? b\" ?? 1;", "return \"a ?? b\" ?? 2;"),
        Some("fallback_literal")
    );
}

#[test]
fn an_escaped_quote_does_not_end_a_string_literal() {
    // Positive: `'it\'s'` is one literal.
    assert_eq!(
        shape("return 'it\\'s';", "return 'it\\'S';"),
        Some("literal_return")
    );
    // Negative: an unescaped inner quote is not one literal.
    assert_eq!(shape("return 'a'b';", "return 'a'c';"), None);
}
