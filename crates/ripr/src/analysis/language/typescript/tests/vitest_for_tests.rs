//! #10760: Vitest `test.for(rows)(title, fn)` passes each row WHOLE as the
//! callback's first argument and the running `TestContext` as its second.
//! A resolved Vitest `test` / `it` import over an inline literal table reads
//! one concrete case per row, like an admitted `.each`. Every other shape
//! stays unindexed and keeps the `test.for registration` partial disclosure.
//! Each test pins the parsed per-row assertions before any class.

use super::*;

const TIER: &str = "export function loyaltyTier(points: number): string {\n  if (1000 <= points) {\n    return \"gold\";\n  }\n  return \"silver\";\n}\n";
const TIER_LINE: (usize, &str) = (2, "  if (1000 <= points) {");
const FILE: &str = "tests/tier.test.ts";

fn vitest_for(import: &str, params: &str, body: &str) -> String {
    format!(
        "import {{ {import} }} from \"vitest\";\nimport {{ loyaltyTier }} from \"../src/tier\";\n\ntest.for([\n  [999, \"silver\"],\n  [1000, \"gold\"],\n])(\"%i points earn %s\", ({params}) => {{\n  {body}\n}});\n"
    )
}

fn observed_and_expected(source: &str) -> Vec<(Option<String>, Option<String>, bool, usize)> {
    extract_tests(Path::new(FILE), source)
        .iter()
        .flat_map(|test| test.assertions.iter())
        .map(|assertion| {
            (
                assertion.observed_expression.clone(),
                assertion.expected_value_or_variant.clone(),
                assertion.has_dynamic_matcher_arg,
                assertion.line,
            )
        })
        .collect()
}

fn boundary_rows(line: usize) -> Vec<(Option<String>, Option<String>, bool, usize)> {
    vec![
        (
            Some("loyaltyTier(999)".to_string()),
            Some("\"silver\"".to_string()),
            false,
            line,
        ),
        (
            Some("loyaltyTier(1000)".to_string()),
            Some("\"gold\"".to_string()),
            false,
            line,
        ),
    ]
}

/// The registration is not indexed and the partial-extraction disclosure
/// names it, so a `no_static_path` reader knows the index is incomplete.
fn assert_disclosed_not_read(label: &str, source: &str) {
    let tests = extract_tests(Path::new(FILE), source);
    assert!(tests.is_empty(), "{label}: must not be indexed: {tests:?}");
    let gap = detect_partial_test_extraction(Path::new(FILE), source, &tests);
    assert!(
        gap.as_ref()
            .is_some_and(|gap| gap.shape == "test.for registration"),
        "{label}: must stay disclosed, got {gap:?}"
    );
}

fn tier_finding(label: &str, source: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/tier.ts"), TIER)?;
    ts_write_file(&root.join(FILE), source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/tier.ts", &[TIER_LINE])],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == TIER_LINE.0)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

#[test]
fn destructured_rows_read_one_boundary_case_per_row() -> Result<(), String> {
    let source = vitest_for(
        "expect, test",
        "[points, tier]",
        "expect(loyaltyTier(points)).toBe(tier);",
    );
    assert_eq!(observed_and_expected(&source), boundary_rows(8));
    let tests = extract_tests(Path::new(FILE), &source);
    assert!(
        detect_partial_test_extraction(Path::new(FILE), &source, &tests).is_none(),
        "an indexed .for registration is no longer a partial-extraction gap"
    );
    let finding = tier_finding("vitest-for-destructured", &source)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_evidence_lacks(&finding, "typescript_table_case_unresolved");
    Ok(())
}

/// The same changed behavior reaches the same evidence through a direct
/// literal test, an admitted `.each` and an admitted `.for`.
#[test]
fn direct_each_and_for_tests_earn_equivalent_evidence() -> Result<(), String> {
    let direct = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest(\"999\", () => {\n  expect(loyaltyTier(999)).toBe(\"silver\");\n});\ntest(\"1000\", () => {\n  expect(loyaltyTier(1000)).toBe(\"gold\");\n});\n";
    let each = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest.each([\n  [999, \"silver\"],\n  [1000, \"gold\"],\n])(\"%i points earn %s\", (points, tier) => {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    let for_rows = vitest_for(
        "expect, test",
        "[points, tier]",
        "expect(loyaltyTier(points)).toBe(tier);",
    );
    assert_eq!(observed_and_expected(each), boundary_rows(8));
    let mut classes = Vec::new();
    for (label, source) in [
        ("equivalent-direct", direct.to_string()),
        ("equivalent-each", each.to_string()),
        ("equivalent-for", for_rows),
    ] {
        classes.push(tier_finding(label, &source)?.class);
    }
    assert_eq!(
        classes,
        vec![
            ExposureClass::Exposed,
            ExposureClass::Exposed,
            ExposureClass::Exposed
        ]
    );
    Ok(())
}

#[test]
fn rows_missing_the_boundary_stay_a_gap() -> Result<(), String> {
    let source = "import { expect, it } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\nit.for([\n  [500, \"silver\"],\n  [2000, \"gold\"],\n])(\"%i\", ([points, tier]) => {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_eq!(
        observed_and_expected(source),
        vec![
            (
                Some("loyaltyTier(500)".to_string()),
                Some("\"silver\"".to_string()),
                false,
                8
            ),
            (
                Some("loyaltyTier(2000)".to_string()),
                Some("\"gold\"".to_string()),
                false,
                8
            ),
        ]
    );
    let finding = tier_finding("vitest-for-missed", source)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn whole_row_indexed_reads_preserve_row_correlation() {
    let source = vitest_for(
        "expect, test",
        "row",
        "expect(loyaltyTier(row[0])).toBe(row[1]);",
    );
    assert_eq!(observed_and_expected(&source), boundary_rows(8));
}

#[test]
fn scalar_rows_bind_the_whole_row() {
    let source = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest.for([999, 1000])(\"%i\", (points) => {\n  expect(loyaltyTier(points)).toBe(\"gold\");\n});\n";
    assert_eq!(
        observed_and_expected(source),
        vec![
            (
                Some("loyaltyTier(999)".to_string()),
                Some("\"gold\"".to_string()),
                false,
                5
            ),
            (
                Some("loyaltyTier(1000)".to_string()),
                Some("\"gold\"".to_string()),
                false,
                5
            ),
        ]
    );
}

#[test]
fn second_argument_context_expect_supplies_the_runner_expect() {
    let source = vitest_for(
        "test",
        "[points, tier], { expect }",
        "expect(loyaltyTier(points)).toBe(tier);",
    );
    assert_eq!(observed_and_expected(&source), boundary_rows(8));
}

#[test]
fn rebound_context_expect_earns_no_credit() {
    let source = vitest_for(
        "test",
        "[points, tier], { expect }",
        "const expect = (value: unknown) => ({ toBe: (_: unknown) => value });\n  expect(loyaltyTier(points)).toBe(tier);",
    );
    assert_disclosed_not_read("rebound expect", &source);
}

/// An array row is never misbound as several callback arguments: the
/// `.each`-style `(points, tier)` reads `tier` as the `TestContext`.
#[test]
fn each_style_parameters_are_not_spread_rows() {
    let source = vitest_for(
        "expect, test",
        "points, tier",
        "expect(loyaltyTier(points)).toBe(tier);",
    );
    assert_disclosed_not_read("spread misbinding", &source);
}

#[test]
fn context_skip_withdraws_the_registration() {
    let source = vitest_for(
        "expect, test",
        "[points, tier], ctx",
        "ctx.skip();\n  expect(loyaltyTier(points)).toBe(tier);",
    );
    assert_disclosed_not_read("ctx.skip", &source);
    let aliased = vitest_for(
        "expect, test",
        "[points, tier], ctx",
        "const { skip } = ctx;\n  skip();\n  expect(loyaltyTier(points)).toBe(tier);",
    );
    assert_disclosed_not_read("aliased ctx.skip", &aliased);
    // An unused named context is the plain row form.
    let unused = vitest_for(
        "expect, test",
        "[points, tier], ctx",
        "expect(loyaltyTier(points)).toBe(tier);",
    );
    assert_eq!(observed_and_expected(&unused), boundary_rows(8));
}

#[test]
fn unsupported_row_bindings_stay_disclosed() {
    for (label, params, body) in [
        (
            "whole array row passed on",
            "row",
            "expect(loyaltyTier(row as unknown as number)).toBe(\"gold\");",
        ),
        (
            "index past the row",
            "row",
            "expect(loyaltyTier(row[0])).toBe(row[2]);",
        ),
        (
            "spread whole row",
            "row",
            "expect(loyaltyTier(...(row as [number]))).toBe(row[1]);",
        ),
        (
            "bare spread whole row",
            "row",
            "expect(String(...row)).toBe(row[1]);",
        ),
        (
            "computed index",
            "row",
            "const i = 0;\n  expect(loyaltyTier(row[i])).toBe(row[1]);",
        ),
        (
            "missing cells",
            "[points]",
            "expect(loyaltyTier(points)).toBe(\"gold\");",
        ),
        (
            "defaulted element",
            "[points, tier = \"x\"]",
            "expect(loyaltyTier(points)).toBe(tier);",
        ),
        (
            "rest element",
            "[points, ...rest]",
            "expect(loyaltyTier(points)).toBe(rest[0]);",
        ),
        (
            "mutated cell",
            "[points, tier]",
            "points = 1;\n  expect(loyaltyTier(points)).toBe(tier);",
        ),
        (
            "other context member",
            "[points, tier], { expect, skip }",
            "expect(loyaltyTier(points)).toBe(tier);",
        ),
    ] {
        assert_disclosed_not_read(label, &vitest_for("expect, test", params, body));
    }
}

#[test]
fn non_literal_or_object_tables_stay_disclosed() {
    let variable = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\nconst rows = [\n  [999, \"silver\"],\n];\ntest.for(rows)(\"%i\", ([points, tier]) => {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_disclosed_not_read("variable table", variable);
    let objects = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest.for([\n  { points: 999, tier: \"silver\" },\n])(\"%i\", ({ points, tier }) => {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_disclosed_not_read("object rows", objects);
}

/// `.for` is read only through a resolved Vitest import: a global `test`
/// (Jest has no `.for`) keeps the disclosure.
#[test]
fn global_test_for_without_a_vitest_import_stays_disclosed() {
    let source = "import { loyaltyTier } from \"../src/tier\";\n\ntest.for([\n  [999, \"silver\"],\n])(\"%i\", ([points, tier]) => {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_disclosed_not_read("global test.for", source);
}

/// `[[]]` registers one test with an empty row; `[]` registers none.
#[test]
fn one_empty_row_is_one_test_and_an_empty_table_is_none() {
    let one = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest.for([[]])(\"once\", () => {\n  expect(loyaltyTier(1000)).toBe(\"gold\");\n});\n";
    assert_eq!(
        observed_and_expected(one),
        vec![(
            Some("loyaltyTier(1000)".to_string()),
            Some("\"gold\"".to_string()),
            false,
            5
        )]
    );
    let none = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest.for([])(\"never\", () => {\n  expect(loyaltyTier(1000)).toBe(\"gold\");\n});\n";
    assert!(extract_tests(Path::new(FILE), none).is_empty());
}

#[test]
fn skipped_for_registrations_activate_no_test() {
    for callee in ["test.skip.for", "test.todo.for"] {
        let source = format!(
            "import {{ expect, test }} from \"vitest\";\nimport {{ loyaltyTier }} from \"../src/tier\";\n\n{callee}([\n  [999, \"silver\"],\n])(\"%i\", ([points, tier]) => {{\n  expect(loyaltyTier(points)).toBe(tier);\n}});\n"
        );
        assert!(
            extract_tests(Path::new(FILE), &source).is_empty(),
            "{callee} must not activate a test"
        );
    }
}

/// A member named like the row parameter is not the row: only the bare
/// `row[<i>]` reads are rewritten.
#[test]
fn member_named_like_the_row_is_not_rewritten() {
    let source = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\nconst box = { row: [5] };\ntest.for([\n  [999, \"silver\"],\n  [1000, \"gold\"],\n])(\"%i\", (row) => {\n  expect(loyaltyTier(row[0] + box.row[0])).toBe(row[1]);\n});\n";
    let observed: Vec<_> = observed_and_expected(source)
        .into_iter()
        .map(|(observed, expected, _, _)| (observed, expected))
        .collect();
    assert_eq!(
        observed,
        vec![
            (
                Some("loyaltyTier(999 + box.row[0])".to_string()),
                Some("\"silver\"".to_string())
            ),
            (
                Some("loyaltyTier(1000 + box.row[0])".to_string()),
                Some("\"gold\"".to_string())
            ),
        ]
    );
}

/// An options object between the title and the callback is honored: a
/// running one keeps the rows, a skip option activates no test.
#[test]
fn options_object_on_the_for_registration_is_honored() {
    let running = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest.for([\n  [999, \"silver\"],\n  [1000, \"gold\"],\n])(\"%i\", { timeout: 50 }, ([points, tier]) => {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_eq!(observed_and_expected(running), boundary_rows(8));
    let skipped = running.replace("{ timeout: 50 }", "{ skip: true }");
    assert!(extract_tests(Path::new(FILE), &skipped).is_empty());
}

/// A `test` shadowed inside a suite is not the Vitest import, and a renamed
/// import (`test as t`) is not read through `t.for`; both stay disclosed.
#[test]
fn shadowed_or_renamed_roots_are_not_read() {
    let shadowed = "import { describe, expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ndescribe(\"tiers\", () => {\n  const test = { for: (_rows: unknown) => (_t: string, fn: (row: [number, string]) => void) => fn([1, \"x\"]) };\n  test.for([\n    [999, \"silver\"],\n  ])(\"%i\", ([points, tier]) => {\n    expect(loyaltyTier(points)).toBe(tier);\n  });\n});\n";
    assert!(
        extract_tests(Path::new(FILE), shadowed).is_empty(),
        "a suite-local test object is not a runner"
    );
    let renamed = "import { expect, test as t } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\nt.for([\n  [999, \"silver\"],\n])(\"%i\", ([points, tier]) => {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert!(
        extract_tests(Path::new(FILE), renamed).is_empty(),
        "renamed .for roots are not read yet"
    );
}

/// A generator callback never runs its body, and a `delete` of a row cell
/// changes what later reads see; neither is read as concrete rows.
#[test]
fn dormant_or_deleting_callbacks_stay_disclosed() {
    let generator = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest.for([\n  [999, \"silver\"],\n])(\"%i\", function* ([points, tier]) {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_disclosed_not_read("generator", generator);
    let async_generator = generator.replace("function*", "async function*");
    assert_disclosed_not_read("async generator", &async_generator);
    let zero_parameter = "import { expect, test } from \"vitest\";\nimport { loyaltyTier } from \"../src/tier\";\n\ntest.for([[]])(\"once\", function* () {\n  expect(loyaltyTier(1000)).toBe(\"gold\");\n});\n";
    assert_disclosed_not_read("zero-parameter generator", zero_parameter);
    for body in [
        "(delete row[0]);\n  expect(loyaltyTier(row[0])).toBe(row[1]);",
        "delete row[0];\n  expect(loyaltyTier(row[0])).toBe(row[1]);",
    ] {
        assert_disclosed_not_read("deleted cell", &vitest_for("expect, test", "row", body));
    }
    // A plain function expression still reads its rows.
    let plain = generator.replace("function* ", "function ");
    assert_eq!(
        observed_and_expected(&plain),
        vec![(
            Some("loyaltyTier(999)".to_string()),
            Some("\"silver\"".to_string()),
            false,
            7
        )]
    );
}
