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

/// #12281: an edit that only negates the guard's equality operator flips the
/// branch for every input; any other edit keeps the kind-only witness.
#[test]
fn equality_negation_is_only_a_single_operator_flip() {
    use super::typescript_changed_boundary_is_equality_negation as negation;
    use crate::domain::ProbeFamily;
    let file = std::path::Path::new("src/_utils.ts");
    let old = "  return typeof input === \"function\" ? (input as any)() : input;";
    for (old_line, new_line) in [
        (
            old,
            "  return typeof input !== \"function\" ? (input as any)() : input;",
        ),
        (
            "  if (typeof value != 'string') {",
            "  if (typeof value == 'string') {",
        ),
        ("  if (count === 0) {", "  if (count !== 0) {"),
    ] {
        assert!(
            negation(file, &ProbeFamily::Predicate, Some(old_line), new_line),
            "{old_line} -> {new_line}"
        );
    }
    for (old_line, new_line) in [
        // the kind changes: an input of another kind takes the same branch
        (
            old,
            "  return typeof input === \"object\" ? (input as any)() : input;",
        ),
        // operator and kind both change
        (
            old,
            "  return typeof input !== \"object\" ? (input as any)() : input;",
        ),
        // `===` to `==` is not a negation
        (
            old,
            "  return typeof input == \"function\" ? (input as any)() : input;",
        ),
        // a relational operator flip is not an equality negation
        ("  if (count < 0) {", "  if (count >= 0) {"),
        // the operator flip plus another edit elsewhere on the line
        (
            old,
            "  return typeof input !== \"function\" ? (input as any)(1) : input;",
        ),
        // an unchanged line has no edit to read
        (old, old),
    ] {
        assert!(
            !negation(file, &ProbeFamily::Predicate, Some(old_line), new_line),
            "{old_line} -> {new_line}"
        );
    }
    let flipped = "  return typeof input !== \"function\" ? (input as any)() : input;";
    // an added line has no prior operator, and a non-predicate family is not
    // a comparison edit
    assert!(!negation(file, &ProbeFamily::Predicate, None, flipped));
    assert!(!negation(
        file,
        &ProbeFamily::ReturnValue,
        Some(old),
        flipped
    ));
}

/// #12281 review: only a comparison that is the whole condition (with at most
/// `!` and parentheses around it) flips the outcome for every input. A clause
/// joined by `&&` / `||` or nested in another conditional keeps the exact-kind
/// rule, because another operand can decide the outcome on its own.
#[test]
fn equality_negation_requires_the_whole_condition() {
    use super::typescript_changed_boundary_is_equality_negation as negation;
    use crate::domain::ProbeFamily;
    let file = std::path::Path::new("src/f.ts");
    let flip = |line: &str| line.replacen("===", "!==", 1);
    for old_line in [
        "  if (typeof x === \"function\") return x();",
        "  if (!(typeof x === \"function\")) return x;",
        "  if ((typeof x === \"function\")) return x();",
        "  return !(typeof x === \"function\") ? x : x();",
        "  return (typeof x === \"function\") ? x() : x;",
    ] {
        let new_line = flip(old_line);
        assert_ne!(old_line, new_line, "fixture must flip an operator");
        assert!(
            negation(file, &ProbeFamily::Predicate, Some(old_line), &new_line),
            "{old_line} -> {new_line}"
        );
    }
    for old_line in [
        "  if (typeof x === \"function\" && flag) return x();",
        "  if (typeof x === \"function\" || flag) return x();",
        "  if (flag && typeof x === \"function\") return x();",
        "  if (!(typeof x === \"function\" && flag)) return x;",
        "  return (flag ? typeof x === \"function\" : false) ? x() : x;",
        "  return flag ? (typeof x === \"function\" ? x() : x) : x;",
    ] {
        let new_line = flip(old_line);
        assert_ne!(old_line, new_line, "fixture must flip an operator");
        assert!(
            !negation(file, &ProbeFamily::Predicate, Some(old_line), &new_line),
            "{old_line} -> {new_line}"
        );
    }
    // The `&&` / `||` subjects still name this comparison as their changed
    // boundary, so their `false` above is the whole-condition rule, not a
    // parse refusal.
    for old_line in [
        "  if (typeof x === \"function\" && flag) return x();",
        "  if (typeof x === \"function\" || flag) return x();",
        "  if (flag && typeof x === \"function\") return x();",
    ] {
        let new_line = flip(old_line);
        assert_eq!(
            super::typescript_changed_boundary_text(
                file,
                &ProbeFamily::Predicate,
                Some(old_line),
                &new_line
            )
            .as_ref(),
            "typeof x !== \"function\"",
            "{new_line}"
        );
    }
}

/// #12281 re-review: the changed line alone is parsed, so a line of a
/// multi-line condition must never read as the whole condition. Only a
/// self-contained `if` / `while` header or a complete `return ...;` does.
#[test]
fn equality_negation_refuses_lines_of_a_multi_line_condition() {
    use super::typescript_changed_boundary_is_equality_negation as negation;
    use crate::domain::ProbeFamily;
    let file = std::path::Path::new("src/f.ts");
    let flip = |line: &str| line.replacen("===", "!==", 1);
    for old_line in [
        // continuation line of `if (\n flag &&\n typeof input ... \n) {`
        "    typeof input === \"function\"",
        // continuation line of `return flag &&\n typeof input ...;`
        "    typeof input === \"function\";",
        // first line of `if (typeof input ... &&\n flag) {`
        "  if (typeof input === \"function\" &&",
        // first line of `return typeof input ... &&\n flag;`
        "  return typeof input === \"function\" &&",
        // a `return` without its terminating `;` may continue below
        "  return typeof input === \"function\"",
        // a header whose parenthesis closes on a later line
        "  if (typeof input === \"function\"",
        // a leading continuation operator
        "    && typeof input === \"function\") {",
        // a declaration is not a condition header
        "  const ok = typeof input === \"function\";",
    ] {
        let new_line = flip(old_line);
        assert_ne!(old_line, new_line, "fixture must flip an operator");
        assert!(
            !negation(file, &ProbeFamily::Predicate, Some(old_line), &new_line),
            "{old_line} -> {new_line}"
        );
    }
    for old_line in [
        "  } else if (typeof input === \"function\") {",
        "  while (typeof input === \"function\") input = input();",
        "  return typeof input === \"function\";",
    ] {
        let new_line = flip(old_line);
        assert!(
            negation(file, &ProbeFamily::Predicate, Some(old_line), &new_line),
            "{old_line} -> {new_line}"
        );
    }
}

/// #12281 re-review: the negation credit needs every call to evaluate the
/// guard, so the changed line must be where the owner's first body statement
/// starts. Each case names the owner and the 1-based line of the guard.
#[test]
fn negation_guard_must_be_the_first_body_statement() -> Result<(), String> {
    use super::typescript_changed_line_is_first_body_statement as first;
    let file = std::path::Path::new("src/f.ts");
    let guard = "typeof input !== \"function\"";
    let case = |source: &str, owner: &str| -> Result<(super::TypeScriptOwner, usize), String> {
        let owner = super::extract_owners(file, source)
            .into_iter()
            .find(|candidate| candidate.name == owner)
            .ok_or_else(|| format!("owner `{owner}` not extracted from:\n{source}"))?;
        let line = source
            .lines()
            .position(|text| text.contains(guard))
            .map(|index| index + 1)
            .ok_or_else(|| format!("no guard line in:\n{source}"))?;
        Ok((owner, line))
    };
    for (owner, source) in [
        (
            "resolveValue",
            "export function resolveValue(input: unknown): unknown {\n  return typeof input !== \"function\" ? (input as any)() : input;\n}\n",
        ),
        (
            "f",
            "export function f(input: unknown): unknown {\n  if (typeof input !== \"function\") return (input as any)();\n  return input;\n}\n",
        ),
        (
            "f",
            "export const f = (input: unknown): unknown => {\n  if (typeof input !== \"function\") return (input as any)();\n  return input;\n};\n",
        ),
        (
            "f",
            "export class C {\n  f(input: unknown): unknown {\n    return typeof input !== \"function\" ? (input as any)() : input;\n  }\n}\n",
        ),
    ] {
        let (owner, line) = case(source, owner)?;
        assert!(
            first(&owner, line),
            "first statement must credit:\n{source}"
        );
    }
    for source in [
        // an earlier `if` returns first
        "export function f(input: unknown): unknown {\n  if (typeof input === \"number\") return input;\n  if (typeof input !== \"function\") return (input as any)();\n  return input;\n}\n",
        // the guard is an `else if` of an earlier branch
        "export function f(input: unknown): unknown {\n  if (typeof input === \"number\") {\n    return input;\n  } else if (typeof input !== \"function\") {\n    return (input as any)();\n  }\n  return input;\n}\n",
        // an earlier `switch` returns first
        "export function f(input: unknown): unknown {\n  switch (typeof input) {\n    case \"number\":\n      return input;\n  }\n  if (typeof input !== \"function\") return (input as any)();\n  return input;\n}\n",
        // an earlier `throw`
        "export function f(input: unknown): unknown {\n  if (input === 42) throw new Error(\"x\");\n  if (typeof input !== \"function\") return (input as any)();\n  return input;\n}\n",
        // an earlier loop returns first
        "export function f(input: unknown): unknown {\n  for (const k of [1]) {\n    return input;\n  }\n  if (typeof input !== \"function\") return (input as any)();\n  return input;\n}\n",
        // an earlier plain declaration (refused: in doubt, refuse)
        "export function f(input: unknown): unknown {\n  const seen = 1;\n  if (typeof input !== \"function\") return (input as any)();\n  return input;\n}\n",
        // nested in an `if` block
        "export function f(input: unknown, flag: boolean): unknown {\n  if (flag) {\n    if (typeof input !== \"function\") return (input as any)();\n  }\n  return input;\n}\n",
        // nested in a loop
        "export function f(input: unknown, n: number): unknown {\n  for (let i = 0; i < n; i++) {\n    if (typeof input !== \"function\") return (input as any)();\n  }\n  return input;\n}\n",
        // nested in a callback the owner never calls
        "export function f(input: unknown): unknown {\n  [].forEach(() => {\n    if (typeof input !== \"function\") return (input as any)();\n  });\n  return input;\n}\n",
        // a directive precedes the guard
        "export function f(input: unknown): unknown {\n  \"use strict\";\n  if (typeof input !== \"function\") return (input as any)();\n  return input;\n}\n",
    ] {
        let (owner, line) = case(source, "f")?;
        assert!(!first(&owner, line), "must not credit:\n{source}");
    }
    Ok(())
}
