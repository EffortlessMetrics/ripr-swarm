//! `typeof` guards name the kind test, not the type-name string (#11196).

use super::probe_shape::typescript_boundary_discriminator;
use super::typeof_kind::literal_argument_typeof_kind;

#[test]
fn typeof_guard_names_the_kind_test_not_the_type_name_string() {
    for (line, expected) in [
        (
            "  return typeof input !== \"function\" ? (input as any)() : input;",
            "typeof input == \"function\"",
        ),
        (
            "  return typeof input === \"function\" ? (input as any)() : input;",
            "typeof input == \"function\"",
        ),
        (
            "  if (typeof value === 'string') {",
            "typeof value == \"string\"",
        ),
        (
            "  if (\"number\" != typeof count) {",
            "typeof count == \"number\"",
        ),
        (
            "  if (ready && typeof opts.cb === \"function\") {",
            "typeof opts.cb == \"function\"",
        ),
    ] {
        assert_eq!(
            typescript_boundary_discriminator(line).as_deref(),
            Some(expected),
            "{line}"
        );
    }
}

#[test]
fn typeof_guard_never_names_the_bare_operand_as_the_literal() {
    for line in [
        // two typeof sides, a non-literal kind, a relational operator and a
        // kind that is not a plain word name no checkable kind
        "  if (typeof a === typeof b) {",
        "  if (typeof a === kind) {",
        "  if (typeof a > \"function\") {",
        "  if (typeof a === \"two words\") {",
        "  if (typeof a === \"\") {",
    ] {
        let found = typescript_boundary_discriminator(line);
        assert_eq!(found, None, "{line}");
    }
}

#[test]
fn non_typeof_comparisons_keep_their_discriminator() {
    assert_eq!(
        typescript_boundary_discriminator("  if (kind === \"function\") {").as_deref(),
        Some("kind == \"function\"")
    );
    assert_eq!(
        typescript_boundary_discriminator("  if (total >= 100) {").as_deref(),
        Some("total == 100")
    );
}

#[test]
fn literal_argument_kind_reads_only_what_the_text_fixes() {
    for (argument, kind) in [
        ("() => 42", "function"),
        ("(a, b) => a + b", "function"),
        ("x => x", "function"),
        ("async () => 1", "function"),
        ("function () { return 1; }", "function"),
        ("(() => 42) as any", "function"),
        ("\"function\"", "string"),
        ("\"function\" as any", "string"),
        ("'a'", "string"),
        ("`plain`", "string"),
        ("42", "number"),
        ("-1.5", "number"),
        ("10n", "bigint"),
        ("true", "boolean"),
        ("null", "object"),
        ("undefined", "undefined"),
        ("{ a: 1 }", "object"),
        ("[1, 2]", "object"),
        ("42 as unknown as string", "number"),
    ] {
        assert_eq!(
            literal_argument_typeof_kind(argument),
            Some(kind),
            "{argument}"
        );
    }
    for argument in [
        "fn",
        "make()",
        "obj.fn",
        "a + b",
        "`x${y}`",
        "[1][0]",
        "{ a: 1 }.a",
        "(x): number => x",
        "",
    ] {
        assert_eq!(literal_argument_typeof_kind(argument), None, "{argument}");
    }
}
