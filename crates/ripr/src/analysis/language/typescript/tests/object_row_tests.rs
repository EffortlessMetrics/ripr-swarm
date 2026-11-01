//! #11196: a table row's `row.key` expected value resolves to a concrete
//! literal only when the table is a never-touched `const` array of object
//! literals, EVERY row holds a one-line literal at each key the callback
//! reads, and the row parameter is only ever a plain `row.key` read. Every
//! negative below keeps today's dynamic expected value.

use super::*;

const PREAMBLE: &str =
    "import { describe, expect, test } from \"vitest\";\nimport { encode } from \"../src\";\n\n";

/// The assertions of the only test registered in `source`.
fn assertions_of(source: &str) -> Result<Vec<TypeScriptAssertion>, String> {
    let tests = extract_tests(Path::new("tests/encode.test.ts"), source);
    match tests.as_slice() {
        [test] => Ok(test.assertions.clone()),
        other => Err(format!("expected one test, got {}", other.len())),
    }
}

fn describe_source(table: &str, extra: &str, loop_head: &str, body: &str) -> String {
    format!(
        "{PREAMBLE}describe(\"encode\", () => {{\n  const tests = {table};\n{extra}  {loop_head} {{\n    {body}\n  }}\n}});\n"
    )
}

const TABLE: &str =
    "[\n    { input: \"a b\", out: \"a%20b\" },\n    { input: \"c d\", out: \"c%20d\" },\n  ]";
const LOOP: &str = "for (const t of tests)";
const REGISTRATION: &str =
    "test(t.input, () => { expect(encode(t.input)).toStrictEqual(t.out); });";

fn expected_values(assertions: &[TypeScriptAssertion]) -> Vec<Option<&str>> {
    assertions
        .iter()
        .map(|assertion| assertion.expected_value_or_variant.as_deref())
        .collect()
}

/// No row literal reached any assertion: the expected value stays dynamic
/// (or the registration carries no credit at all).
fn all_dynamic(assertions: &[TypeScriptAssertion]) -> bool {
    assertions
        .iter()
        .all(|assertion| assertion.expected_value_or_variant.is_none())
}

#[test]
fn loop_over_a_const_object_table_reads_one_literal_assertion_per_row() -> Result<(), String> {
    let assertions = assertions_of(&describe_source(TABLE, "", LOOP, REGISTRATION))?;
    assert_eq!(
        expected_values(&assertions),
        vec![Some("\"a%20b\""), Some("\"c%20d\"")]
    );
    assert!(assertions.iter().all(|a| !a.has_dynamic_matcher_arg));
    // The row literal reaches the observed expression too (input witness).
    assert_eq!(
        assertions[1].observed_expression.as_deref(),
        Some("encode(\"c d\")")
    );
    Ok(())
}

#[test]
fn container_cells_resolve_only_as_bare_values() -> Result<(), String> {
    let table =
        "[\n    { input: [\"a\", \"b\"], out: { k: [1, 2] } },\n    { input: [], out: {} },\n  ]";
    let ok = describe_source(
        table,
        "",
        LOOP,
        "test(\"x\", () => { expect(encode(...t.input)).toEqual(t.out); });",
    );
    let assertions = assertions_of(&ok)?;
    assert_eq!(
        expected_values(&assertions),
        vec![Some("{ k: [1, 2] }"), Some("{}")]
    );
    // A container read through a further member could be mutated through the
    // shared row: refused.
    let mutated = describe_source(
        table,
        "",
        LOOP,
        "test(\"x\", () => { t.input.push(\"z\"); expect(encode(t.input)).toEqual(t.out); });",
    );
    assert!(all_dynamic(&assertions_of(&mutated)?));
    // A container cell may be read once, directly, in a lone expect statement.
    for (label, body) in [
        (
            "Object.assign on the shared row",
            "test(\"x\", () => { Object.assign(t.input, [9]); expect(encode(...t.input)).toEqual(t.out); });",
        ),
        (
            "a mutating call on the row",
            "test(\"x\", () => { fix(t.input); expect(encode(...t.input)).toEqual(t.out); });",
        ),
        (
            "a parenthesized member call",
            "test(\"x\", () => { expect(encode((t.input).pop())).toEqual(t.out); });",
        ),
        (
            "two reads of the same cell",
            "test(\"x\", () => { expect(encode(...t.input)).toEqual(t.input); });",
        ),
        (
            "an argument of a call nested inside the owner call",
            "test(\"x\", () => { expect(encode(fix(t.input))).toEqual(t.out); });",
        ),
        (
            "a type assertion wrapper",
            "test(\"x\", () => { expect(encode(...(t.input as string[]))).toEqual(t.out); });",
        ),
    ] {
        let assertions = assertions_of(&describe_source(table, "", LOOP, body))?;
        if !all_dynamic(&assertions) {
            return Err(format!("{label} resolved: {assertions:?}"));
        }
    }
    let member = describe_source(
        table,
        "",
        LOOP,
        "test(\"x\", () => { expect(encode(t.input)).toEqual(t.out.k); });",
    );
    assert!(all_dynamic(&assertions_of(&member)?));
    Ok(())
}

#[test]
fn scalar_cells_resolve_only_as_direct_reads_in_a_lone_expect() -> Result<(), String> {
    // A wrapped or sequenced read can see a value the table literal does not
    // hold (`delete t.out` makes the runtime expected value `undefined`).
    for (label, body) in [
        (
            "delete in a void array",
            "test(\"x\", () => { void [delete t.out]; expect(encode(t.input)).toStrictEqual(t.out); });",
        ),
        (
            "delete in a comma list",
            "test(\"x\", () => { expect((delete t.out, encode(t.input))).toStrictEqual(t.out); });",
        ),
        (
            "delete as the observed argument",
            "test(\"x\", () => { expect(encode(delete t.out, t.input)).toStrictEqual(t.out); });",
        ),
        (
            "a second statement",
            "test(\"x\", () => { const n = 1; expect(encode(t.input)).toStrictEqual(t.out); });",
        ),
        (
            "a member read",
            "test(\"x\", () => { expect(encode(t.input)).toStrictEqual(t.out.toString()); });",
        ),
        (
            "an operator around the read",
            "test(\"x\", () => { expect(encode(t.input + \"\")).toStrictEqual(t.out); });",
        ),
    ] {
        let assertions = assertions_of(&describe_source(TABLE, "", LOOP, body))?;
        if !all_dynamic(&assertions) {
            return Err(format!("{label} resolved: {assertions:?}"));
        }
    }
    // A negative number followed by a member would re-lex as `-1.5.toString()`.
    let negative = "[\n    { input: \"a\", out: -1.5 },\n    { input: \"b\", out: -2.5 },\n  ]";
    let member = describe_source(
        negative,
        "",
        LOOP,
        "test(\"x\", () => { expect(encode(t.input)).toStrictEqual(t.out.toString()); });",
    );
    assert!(all_dynamic(&assertions_of(&member)?));
    Ok(())
}

#[test]
fn hidden_mutation_routes_are_refused_on_every_entry_path() -> Result<(), String> {
    // Each body would resolve "A%20B"-style literals without the shared guard.
    let each_table = "[{ input: \"a b\", out: \"a%20b\" }]";
    let guarded = [
        (
            "arguments",
            "function (t) { expect(encode(t.input, arguments[0].out = encode(\"a\"))).toBe(t.out); }",
        ),
        (
            "eval",
            "(t) => { expect(encode(t.input, eval(\"t.out = encode('a')\"))).toBe(t.out); }",
        ),
        (
            "indirect eval",
            "(t) => { expect(encode(t.input, (0, eval)(\"1\"))).toBe(t.out); }",
        ),
        (
            "Function constructor",
            "(t) => { expect(encode(t.input, new Function(\"1\")())).toBe(t.out); }",
        ),
        (
            "escaped identifier",
            "(t) => { expect(encode(t.input, t.\\u006fut = encode(\"a\"))).toBe(t.out); }",
        ),
        (
            "this",
            "function (t) { expect(encode(t.input, this.x)).toBe(t.out); }",
        ),
        (
            "regex literal hiding eval",
            "(t) => { expect(encode(t.input, eval(\"t.out = encode('a')\"), \"it's\")).toBe(t.out); }, typeof /\"a/",
        ),
    ];
    // The same callback without a hidden route resolves, so the guard is the
    // only reason the cases below stay dynamic.
    let control = format!(
        "{PREAMBLE}test.each({each_table})(\"x\", (t) => {{ expect(encode(t.input)).toBe(t.out); }});\n"
    );
    let resolved = assertions_of(&control)?;
    assert_eq!(expected_values(&resolved), vec![Some("\"a%20b\"")]);
    for (label, callback) in guarded {
        let source = format!("{PREAMBLE}test.each({each_table})(\"x\", {callback});\n");
        let assertions = assertions_of(&source)?;
        if !all_dynamic(&assertions) {
            return Err(format!("test.each {label} resolved: {assertions:?}"));
        }
        // The for...of path shares the guard.
        let looped = describe_source(
            TABLE,
            "",
            LOOP,
            &format!(
                "test(\"x\", {});",
                callback
                    .replace("(t) =>", "() =>")
                    .replace("function (t)", "function ()")
            ),
        );
        let assertions = assertions_of(&looped)?;
        if !all_dynamic(&assertions) {
            return Err(format!("for...of {label} resolved: {assertions:?}"));
        }
    }
    Ok(())
}

/// The tuple-row `test.each([[cell, want], ..])` route shares
/// `substitute_row`. Container cells never resolve there (pre-#11196
/// behavior): a bare read, a direct spread, a spread after a mutation and a
/// spread through an alias all stay dynamic, while a scalar control resolves.
#[test]
fn tuple_row_container_cells_never_resolve() -> Result<(), String> {
    let source = |rows: &str, callback: &str| {
        format!("{PREAMBLE}test.each({rows})(\"x\", (xs, want) => {{ {callback} }});\n")
    };
    let control = assertions_of(&source(
        "[[1, \"A\"], [2, \"B\"]]",
        "expect(encode(xs)).toBe(want);",
    ))?;
    assert_eq!(
        expected_values(&control),
        vec![Some("\"A\""), Some("\"B\"")]
    );
    let rows = "[[[\"a\"], \"A\"], [[\"b\"], \"B\"]]";
    for (label, callback) in [
        ("a bare read", "expect(encode(xs)).toBe(want);"),
        ("a direct spread", "expect(encode(...xs)).toBe(want);"),
        (
            "a spread after a mutation",
            "xs.push(\"z\"); expect(encode(...xs)).toBe(want);",
        ),
        (
            "a spread through an alias",
            "const ys = xs; ys.push(\"z\"); expect(encode(...ys)).toBe(want);",
        ),
    ] {
        let assertions = assertions_of(&source(rows, callback))?;
        if assertions
            .iter()
            .any(|a| a.expected_value_or_variant.as_deref() == Some("\"A\""))
        {
            return Err(format!("{label} resolved: {assertions:?}"));
        }
    }
    Ok(())
}

#[test]
fn every_row_must_hold_a_literal_at_every_key_read() -> Result<(), String> {
    for (label, table) in [
        (
            "a row missing the key",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\" }]",
        ),
        (
            "an identifier cell",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\", out: other }]",
        ),
        (
            "a call cell",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\", out: make() }]",
        ),
        (
            "a shorthand cell",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\", out }]",
        ),
        (
            "a spread cell in a container",
            "[{ input: \"a\", out: [\"A\"] }, { input: \"b\", out: [...rest] }]",
        ),
        (
            "a template cell",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\", out: `B` }]",
        ),
        (
            "Number.POSITIVE_INFINITY, outside the literal vocabulary",
            "[{ input: \"a\", out: 1 }, { input: \"b\", out: Number.POSITIVE_INFINITY }]",
        ),
        (
            "a spread row",
            "[{ input: \"a\", out: \"A\" }, { ...base, input: \"b\", out: \"B\" }]",
        ),
        (
            "a computed key",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\", [key]: \"B\" }]",
        ),
        (
            "a duplicate key",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\", out: \"B\", out: \"C\" }]",
        ),
        (
            "a method cell",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\", out() { return 1; } }]",
        ),
        (
            "a multi-line cell",
            "[{ input: \"a\", out: \"A\" }, { input: \"b\", out: [\n 1,\n 2] }]",
        ),
        (
            "a non-object row",
            "[{ input: \"a\", out: \"A\" }, [\"b\", \"B\"]]",
        ),
        ("an empty table", "[]"),
    ] {
        let assertions = assertions_of(&describe_source(table, "", LOOP, REGISTRATION))?;
        if !all_dynamic(&assertions) {
            return Err(format!("{label} resolved: {assertions:?}"));
        }
    }
    Ok(())
}

#[test]
fn a_touched_table_is_never_resolved() -> Result<(), String> {
    for (label, extra, loop_head) in [
        (
            "push",
            "  tests.push({ input: \"z\", out: \"Z\" });\n",
            LOOP,
        ),
        (
            "index assignment",
            "  tests[0] = { input: \"z\", out: \"Z\" };\n",
            LOOP,
        ),
        ("row mutation", "  tests[0].out = \"Z\";\n", LOOP),
        ("an alias", "  const alias = tests;\n", LOOP),
        ("a spread copy", "  const copy = [...tests];\n", LOOP),
        ("concat", "  const more = tests.concat([]);\n", LOOP),
        ("an export", "  export { tests };\n", LOOP),
        ("a second read", "  const n = tests.length;\n", LOOP),
        (
            "a derived .map table",
            "",
            "for (const t of tests.map((row) => row))",
        ),
        ("a different table", "", "for (const t of other)"),
        (
            "a destructured loop variable",
            "",
            "for (const { input, out } of tests)",
        ),
        ("an awaited loop", "", "for await (const t of tests)"),
    ] {
        let assertions = assertions_of(&describe_source(TABLE, extra, loop_head, REGISTRATION))?;
        if !all_dynamic(&assertions) {
            return Err(format!("{label} resolved: {assertions:?}"));
        }
    }
    Ok(())
}

#[test]
fn a_reassignable_or_outer_or_exported_table_is_never_resolved() -> Result<(), String> {
    let loop_and_registration = format!("  {LOOP} {{\n    {REGISTRATION}\n  }}\n");
    for (label, source) in [
        (
            "let table",
            format!(
                "{PREAMBLE}describe(\"e\", () => {{\n  let tests = {TABLE};\n{loop_and_registration}}});\n"
            ),
        ),
        (
            "var table",
            format!(
                "{PREAMBLE}describe(\"e\", () => {{\n  var tests = {TABLE};\n{loop_and_registration}}});\n"
            ),
        ),
        (
            "an exported table",
            format!(
                "{PREAMBLE}export const tests = {TABLE};\ndescribe(\"e\", () => {{\n{loop_and_registration}}});\n"
            ),
        ),
        (
            "an outer-scope table named again in a nested scope",
            format!(
                "{PREAMBLE}const tests = {TABLE};\ndescribe(\"e\", () => {{\n  const tests = [];\n{loop_and_registration}}});\n"
            ),
        ),
        (
            "a table declared after the loop",
            format!(
                "{PREAMBLE}describe(\"e\", () => {{\n{loop_and_registration}  const tests = {TABLE};\n}});\n"
            ),
        ),
        (
            "an eval in scope",
            format!(
                "{PREAMBLE}describe(\"e\", () => {{\n  const tests = {TABLE};\n  eval(\"tests.pop()\");\n{loop_and_registration}}});\n"
            ),
        ),
        (
            "an eval with a split table name",
            format!(
                "{PREAMBLE}describe(\"e\", () => {{\n  const tests = {TABLE};\n  eval(\"te\" + \"sts[0].out = 'X'\");\n{loop_and_registration}}});\n"
            ),
        ),
        (
            "a with statement in a sibling statement",
            format!(
                "{PREAMBLE}describe(\"e\", () => {{\n  const tests = {TABLE};\n  with ({{}}) {{ }}\n{loop_and_registration}}});\n"
            ),
        ),
        (
            "a unicode escape in a sibling statement",
            format!(
                "{PREAMBLE}describe(\"e\", () => {{\n  const tests = {TABLE};\n  const \\u0078 = 1;\n{loop_and_registration}}});\n"
            ),
        ),
    ] {
        let assertions = assertions_of(&source)?;
        if !all_dynamic(&assertions) {
            return Err(format!("{label} resolved: {assertions:?}"));
        }
    }
    Ok(())
}

#[test]
fn only_a_plain_row_key_read_in_a_lone_registration_resolves() -> Result<(), String> {
    for (label, body) in [
        (
            "a bare row",
            "test(\"x\", () => { expect(encode(t)).toStrictEqual(t.out); });",
        ),
        (
            "a computed key",
            "test(\"x\", () => { expect(encode(t.input)).toStrictEqual(t[\"out\"]); });",
        ),
        (
            "an optional chain",
            "test(\"x\", () => { expect(encode(t.input)).toStrictEqual(t?.out); });",
        ),
        (
            "an assignment to the cell",
            "test(\"x\", () => { t.out = \"Z\"; expect(encode(t.input)).toStrictEqual(t.out); });",
        ),
        (
            "a shadowing arrow parameter",
            "test(\"x\", () => { [1].map((t) => t); expect(encode(t.input)).toStrictEqual(t.out); });",
        ),
        (
            "a shadowing declaration",
            "test(\"x\", () => { const t = { out: 1, input: 1 }; expect(encode(t.input)).toStrictEqual(t.out); });",
        ),
        (
            "a callback with its own parameter",
            "test(\"x\", (ctx) => { expect(encode(t.input)).toStrictEqual(t.out); });",
        ),
        (
            "a key the rows lack",
            "test(\"x\", () => { expect(encode(t.input)).toStrictEqual(t.want); });",
        ),
        (
            "a callback that never reads the row",
            "test(\"x\", () => { expect(encode(\"a\")).toStrictEqual(\"b\"); });",
        ),
    ] {
        let assertions = assertions_of(&describe_source(TABLE, "", LOOP, body))?;
        let resolved = assertions
            .iter()
            .any(|a| a.expected_value_or_variant.as_deref() == Some("\"a%20b\""));
        if resolved {
            return Err(format!("{label} resolved: {assertions:?}"));
        }
        // The credit is never duplicated per row for a body that reads no row.
        if label == "a callback that never reads the row" && assertions.len() != 1 {
            return Err(format!("{label} repeated per row: {assertions:?}"));
        }
    }
    // A sibling statement beside the registration (it could `continue`) keeps
    // the loop unresolved.
    let sibling = describe_source(
        TABLE,
        "",
        LOOP,
        &format!("if (!t.input) continue;\n    {REGISTRATION}"),
    );
    let tests = extract_tests(Path::new("tests/encode.test.ts"), &sibling);
    assert!(
        tests
            .iter()
            .flat_map(|test| &test.assertions)
            .all(|a| a.expected_value_or_variant.is_none()),
        "a sibling statement resolved the rows"
    );
    Ok(())
}

/// R1: a pattern that repeats a row key binds two names to one cell.
#[test]
fn a_destructuring_pattern_repeating_a_key_is_never_resolved() -> Result<(), String> {
    let source = format!(
        "{PREAMBLE}describe(\"e\", () => {{\n  test.each([{{ input: [3, 1, 2] }}, {{ input: [2, 1] }}])(\"x\", ({{ input: a, input: b }}) => {{ expect(encode(a)).toEqual(b); }});\n}});\n"
    );
    assert!(all_dynamic(&assertions_of(&source)?));
    // Control: distinct keys over distinct cells resolve.
    let ok = format!(
        "{PREAMBLE}describe(\"e\", () => {{\n  test.each([{{ input: [3, 1], out: [1, 3] }}])(\"x\", ({{ input: a, out: b }}) => {{ expect(encode(a)).toEqual(b); }});\n}});\n"
    );
    assert!(!all_dynamic(&assertions_of(&ok)?));
    Ok(())
}

/// R2: the loop parameter may not reach the registration's callee or any
/// argument outside the callback, except a pure scalar title.
#[test]
fn loop_parameter_in_registration_arguments_is_never_resolved() -> Result<(), String> {
    let table = "[\n    { input: [\"a\"], out: \"A\", name: \"n1\" },\n    { input: [\"b\"], out: \"B\", name: \"n2\" },\n  ]";
    let callback = "() => { expect(encode(...t.input)).toBe(t.out); }";
    for (label, registration, resolves) in [
        (
            "a scalar key title",
            format!("test(t.name, {callback});"),
            true,
        ),
        (
            "a toString title",
            format!("test(t.name.toString(), {callback});"),
            true,
        ),
        ("no use", format!("test(\"x\", {callback});"), true),
        (
            "a template title of scalar key reads",
            format!("test(`${{t.name}} -> ${{t.out}}`, {callback});"),
            true,
        ),
        (
            "a template title calling a helper",
            format!("test(`${{fix(t)}}`, {callback});"),
            false,
        ),
        (
            "a template title with a container key",
            format!("test(`${{t.input}}`, {callback});"),
            false,
        ),
        (
            "Object.assign in the title",
            format!("test(String(Object.assign(t.input, [9])), {callback});"),
            false,
        ),
        (
            "a helper over the row",
            format!("test(fix(t), {callback});"),
            false,
        ),
        (
            "a container key as the title",
            format!("test(t.input, {callback});"),
            false,
        ),
        (
            "a key the rows lack",
            format!("test(t.title, {callback});"),
            false,
        ),
        (
            "the row in the options argument",
            format!("test(\"x\", {{ timeout: t.out }}, {callback});"),
            false,
        ),
        (
            "a row method callee",
            format!("t.run(\"x\", {callback});"),
            false,
        ),
    ] {
        let tests = extract_tests(
            Path::new("tests/encode.test.ts"),
            &describe_source(table, "", LOOP, &registration),
        );
        // A registration the extractor does not index at all credits nothing.
        let assertions: Vec<_> = tests.iter().flat_map(|t| t.assertions.clone()).collect();
        let resolved = assertions
            .iter()
            .any(|a| a.expected_value_or_variant.as_deref() == Some("\"A\""));
        if resolved != resolves {
            return Err(format!("{label}: resolved={resolved}: {assertions:?}"));
        }
    }
    Ok(())
}

/// A row title that throws or is rejected at registration means the test is
/// never registered, so nothing may be credited (reviewer finding on #11420).
#[test]
fn a_null_or_undefined_row_title_never_credits_unregistered_tests() -> Result<(), String> {
    let callback = "() => { expect(encode(t.input)).toBe(t.out); }";
    // Positive control: the same table with valid titles is credited, so the
    // negatives below cannot pass vacuously.
    for title in ["t.name", "t.name.toString()"] {
        let table = "[\n    { input: \"a\", out: \"A\", name: \"first\" },\n    { input: \"b\", out: \"B\", name: 2 },\n  ]";
        let tests = extract_tests(
            Path::new("tests/encode.test.ts"),
            &describe_source(table, "", LOOP, &format!("test({title}, {callback});")),
        );
        let values: Vec<_> = tests
            .iter()
            .flat_map(|t| t.assertions.iter())
            .map(|a| a.expected_value_or_variant.as_deref())
            .collect();
        if values != vec![Some("\"A\""), Some("\"B\"")] {
            return Err(format!("control {title}: {values:?}"));
        }
    }
    for (label, name_cell, title) in [
        ("null toString title", "null", "t.name.toString()"),
        ("undefined toString title", "undefined", "t.name.toString()"),
        ("null bare title", "null", "t.name"),
        ("boolean bare title", "true", "t.name"),
        ("false bare title", "false", "t.name"),
        ("bigint bare title", "1n", "t.name"),
        (
            "null template toString title",
            "null",
            "`${t.name.toString()}`",
        ),
    ] {
        let table = format!(
            "[\n    {{ input: \"a\", out: \"A\", name: \"first\" }},\n    {{ input: \"b\", out: \"B\", name: {name_cell} }},\n  ]"
        );
        let tests = extract_tests(
            Path::new("tests/encode.test.ts"),
            &describe_source(&table, "", LOOP, &format!("test({title}, {callback});")),
        );
        let credited = tests.iter().flat_map(|t| t.assertions.iter()).any(|a| {
            matches!(
                a.expected_value_or_variant.as_deref(),
                Some("\"A\"") | Some("\"B\"")
            )
        });
        if credited {
            return Err(format!("{label}: credited: {tests:?}"));
        }
    }
    Ok(())
}

/// Whitespace inside `expect( ... )` around the sole observed call is still a
/// direct read (reviewer finding on #11420).
#[test]
fn whitespace_inside_expect_keeps_the_direct_read_resolving() -> Result<(), String> {
    for body in [
        "test(t.input, () => { expect( encode(t.input) ).toBe(t.out); });",
        "test(t.input, () => { expect(\n encode(t.input)\n ).toBe(t.out); });",
    ] {
        let assertions = assertions_of(&describe_source(TABLE, "", LOOP, body))?;
        assert_eq!(
            expected_values(&assertions),
            vec![Some("\"a%20b\""), Some("\"c%20d\"")],
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn inline_each_object_rows_resolve_for_one_parameter_or_a_plain_pattern() -> Result<(), String> {
    let each = |table: &str, callback: &str| {
        format!(
            "{PREAMBLE}describe(\"e\", () => {{\n  test.each({table})(\"%s\", {callback});\n}});\n"
        )
    };
    let rows = "[{ input: [\"a\", \"b\"], out: \"a/b\" }, { input: [], out: \"\" }]";
    for callback in [
        "(t) => { expect(encode(...t.input)).toBe(t.out); }",
        "({ input, out }) => { expect(encode(...input)).toBe(out); }",
        "({ input: i, out: want }) => { expect(encode(...i)).toBe(want); }",
    ] {
        let assertions = assertions_of(&each(rows, callback))?;
        if expected_values(&assertions) != vec![Some("\"a/b\""), Some("\"\"")] {
            return Err(format!("{callback} did not resolve: {assertions:?}"));
        }
    }
    for (label, table, callback) in [
        (
            "a non-literal table",
            "makeRows()",
            "(t) => { expect(encode(t.input)).toBe(t.out); }",
        ),
        (
            "a row with an identifier cell",
            "[{ input: [], out: \"\" }, { input: [], out: other }]",
            "(t) => { expect(encode(t.input)).toBe(t.out); }",
        ),
        (
            "a row missing the key",
            "[{ input: [], out: \"\" }, { input: [] }]",
            "(t) => { expect(encode(t.input)).toBe(t.out); }",
        ),
        (
            "a spread row",
            "[{ input: [], out: \"\" }, { ...base }]",
            "(t) => { expect(encode(t.input)).toBe(t.out); }",
        ),
        (
            "a defaulted pattern property",
            rows,
            "({ input, out = \"x\" }) => { expect(encode(...input)).toBe(out); }",
        ),
        (
            "a rest pattern",
            rows,
            "({ input, ...rest }) => { expect(encode(...input)).toBe(rest.out); }",
        ),
        (
            "an extra callback parameter",
            rows,
            "(t, extra) => { expect(encode(...t.input)).toBe(t.out); }",
        ),
        (
            "a bare row use",
            rows,
            "(t) => { expect(encode(t)).toBe(t.out); }",
        ),
    ] {
        let assertions = assertions_of(&each(table, callback))?;
        if assertions
            .iter()
            .any(|a| a.expected_value_or_variant.is_some())
        {
            return Err(format!("{label} resolved: {assertions:?}"));
        }
    }
    Ok(())
}
