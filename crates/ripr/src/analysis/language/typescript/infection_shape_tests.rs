use super::infection_shape::ts_unmodeled_infection;

fn shape(old: &str, new: &str) -> Option<&'static str> {
    ts_unmodeled_infection(Some(old), new, false).map(|found| found.shape)
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
    let neutral =
        |old, new| ts_unmodeled_infection(Some(old), new, false).map(|s| s.behavior_neutral);
    assert_eq!(neutral("return p;", "return p; // c"), Some(true));
    // An identity operand is never claimed neutral: `s + 0` and BigInt differ.
    assert_eq!(neutral("return a;", "return a * 1;"), Some(false));
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
    assert_eq!(ts_unmodeled_infection(None, "return 6;", false), None);
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

#[test]
fn a_double_slash_that_is_not_a_comment_is_not_comment_only() {
    // Negative: URL text, a regex literal and a template-literal middle line
    // are not trailing comments, so none is a behavior-neutral comment edit.
    assert_eq!(
        shape("<a href=\"http://x\">y</a>", "<a href=\"http://z\">y</a>"),
        None
    );
    assert_eq!(
        shape(
            "return s.replace(/\\//g, \"-\");",
            "return s.replace(/\\//g, \"+\");"
        ),
        None
    );
    assert_eq!(
        shape("  see http://a.example/x", "  see http://b.example/x"),
        None
    );
    // Positive: a whitespace-led `//` is a comment.
    assert_eq!(
        shape("  return price;", "  return price; // note http://x"),
        Some("comment_only")
    );
}

#[test]
fn a_predicate_seam_has_no_fallback_shape() {
    let found = |predicate| {
        ts_unmodeled_infection(
            Some("if (!ok || 10 <= n) {"),
            "if (!ok || 11 <= n) {",
            predicate,
        )
        .map(|s| s.shape)
    };
    assert_eq!(found(true), None);
    assert_eq!(found(false), Some("fallback_literal"));
}

#[test]
fn a_call_argument_assignment_is_not_a_default_parameter() {
    // Negative: `f(a = 1)` is a call, not a declaration.
    assert_eq!(shape("  f(a = 1);", "  f(a = 2);"), None);
    // Positive: a declaration, an arrow and a method are.
    assert_eq!(
        shape("const g = (a = 1) => a;", "const g = (a = 2) => a;"),
        Some("default_parameter")
    );
    assert_eq!(
        shape("  total(a = 1) {", "  total(a = 2) {"),
        Some("default_parameter")
    );
}

#[test]
fn dropping_a_semicolon_before_a_comment_is_not_comment_only() {
    // Negative: `return f;` then `(1);` -- without `;` the next line joins the
    // return, so the edit changes behavior.
    assert_eq!(shape("  return f;", "  return f // note"), None);
    // Positive control: the semicolon is kept.
    assert_eq!(
        shape("  return f;", "  return f; // note"),
        Some("comment_only")
    );
}

#[test]
fn only_a_real_declaration_head_makes_a_default_parameter_edit() {
    // Negative: a substring match on `function` in a call, a member call and
    // a control statement are not declarations.
    assert_eq!(
        shape("myfunctionCall(a = 1);", "myfunctionCall(a = 2);"),
        None
    );
    assert_eq!(shape("  wrap.call(a = 1);", "  wrap.call(a = 2);"), None);
    assert_eq!(shape("if (a = 1) {", "if (a = 2) {"), None);
    // Positive: a function declaration, an anonymous function, an async
    // generator and a typed arrow.
    for (old, new) in [
        ("export function f(a = 1) {", "export function f(a = 2) {"),
        (
            "const g = function (a = 1) {",
            "const g = function (a = 2) {",
        ),
        (
            "export async function* h(a = 1) {",
            "export async function* h(a = 2) {",
        ),
        (
            "const k = (a = 1): number => a;",
            "const k = (a = 2): number => a;",
        ),
    ] {
        assert_eq!(shape(old, new), Some("default_parameter"), "{old}");
    }
}

#[test]
fn a_block_comment_hiding_a_double_slash_is_undecidable_not_comment_only() {
    // #11723: the line-comment cut stops at the `//` inside `/* a // b */`, so
    // both lines would compare equal and hide a real edit. Fail closed.
    assert_eq!(
        shape("x /* a // b */ y", "x /* a // b */ z"),
        None,
        "a block comment line is not a comment-only no-op"
    );
    // Positive control: the same edit without a block comment is still read.
    assert_eq!(
        shape("  return price;", "  return price; // fallback"),
        Some("comment_only")
    );
}

#[test]
fn a_block_comment_opener_inside_a_string_does_not_decline_the_line() {
    // `/*` inside a literal is text, so the trailing `//` comment is still read.
    assert_eq!(
        shape("  return \"/*\";", "  return \"/*\"; // note"),
        Some("comment_only")
    );
}

#[test]
fn any_block_comment_line_is_declined_even_without_a_double_slash() {
    // The reader does not track `/* ... */` extents, so every block-comment
    // line fails closed, not only one that hides a `//`.
    assert_eq!(shape("x /* a */ y", "x /* a */ z"), None);
}
