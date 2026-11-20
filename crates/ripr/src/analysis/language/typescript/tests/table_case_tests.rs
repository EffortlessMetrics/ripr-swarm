//! Inline literal `test.each` / `it.each` tables read one concrete case per
//! row: the row's input literals reach the predicate boundary witness and
//! its expected literal is the assertion's expected value. Each test pins
//! the parsed per-row assertions before the class.

use super::*;

const TIER: &str = "export function loyaltyTier(points: number): string {\n  if (1000 <= points) {\n    return \"gold\";\n  }\n  return \"silver\";\n}\n";
const TIER_LINE: (usize, &str) = (2, "  if (1000 <= points) {");

fn table_test(rows: &str) -> String {
    format!(
        "import {{ loyaltyTier }} from \"../src/tier\";\n\ntest.each([\n{rows}\n])(\"%i points earn %s\", (points, tier) => {{\n  expect(loyaltyTier(points)).toBe(tier);\n}});\n"
    )
}

fn observed_and_expected(source: &str) -> Vec<(Option<String>, Option<String>, bool, usize)> {
    extract_tests(Path::new("tests/tier.test.ts"), source)
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

fn tier_finding(label: &str, source: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/tier.ts"), TIER)?;
    ts_write_file(&root.join("tests/tier.test.ts"), source)?;
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
fn literal_table_rows_pinning_the_boundary_are_exposed() -> Result<(), String> {
    let source = table_test("  [999, \"silver\"],\n  [1000, \"gold\"],");
    assert_eq!(
        observed_and_expected(&source),
        vec![
            (
                Some("loyaltyTier(999)".to_string()),
                Some("\"silver\"".to_string()),
                false,
                7
            ),
            (
                Some("loyaltyTier(1000)".to_string()),
                Some("\"gold\"".to_string()),
                false,
                7
            ),
        ]
    );
    let finding = tier_finding("table-boundary-pinned", &source)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_evidence_lacks(&finding, "typescript_table_case_unresolved");
    Ok(())
}

#[test]
fn literal_table_rows_missing_the_boundary_stay_a_gap() -> Result<(), String> {
    let source = table_test("  [500, \"silver\"],\n  [2000, \"gold\"],");
    assert_eq!(
        observed_and_expected(&source),
        vec![
            (
                Some("loyaltyTier(500)".to_string()),
                Some("\"silver\"".to_string()),
                false,
                7
            ),
            (
                Some("loyaltyTier(2000)".to_string()),
                Some("\"gold\"".to_string()),
                false,
                7
            ),
        ]
    );
    let finding = tier_finding("table-boundary-missed", &source)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

/// A table that is not an inline array of literal rows keeps the row
/// parameter, so the expected value stays dynamic and unresolved.
#[test]
fn non_literal_table_stays_unresolved() -> Result<(), String> {
    let source = "import { loyaltyTier } from \"../src/tier\";\n\nconst rows = [\n  [999, \"silver\"],\n  [1000, \"gold\"],\n];\ntest.each(rows)(\"%i points earn %s\", (points, tier) => {\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_eq!(
        observed_and_expected(source),
        vec![(Some("loyaltyTier(points)".to_string()), None, true, 8)]
    );
    let finding = tier_finding("table-variable", source)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_evidence_contains(&finding, "typescript_table_case_unresolved");

    let computed = table_test("  [999, \"silver\"],\n  [base + 1, \"gold\"],");
    assert_eq!(
        observed_and_expected(&computed),
        vec![(Some("loyaltyTier(points)".to_string()), None, true, 7)]
    );
    Ok(())
}

/// A non-ASCII parameter or body fails closed: the row substitution scans
/// ASCII identifiers only.
#[test]
fn non_ascii_table_stays_unresolved() {
    let parameter = "import { loyaltyTier } from \"../src/tier\";\n\ntest.each([\n  [999, \"silver\"],\n])(\"%i\", (pointsé, tier) => {\n  expect(loyaltyTier(pointsé)).toBe(tier);\n});\n";
    assert_eq!(
        observed_and_expected(parameter),
        vec![(Some("loyaltyTier(pointsé)".to_string()), None, true, 6)]
    );
    let body = "import { loyaltyTier } from \"../src/tier\";\n\ntest.each([\n  [999, \"silver\"],\n])(\"%i\", (points, tier) => {\n  const pointsé = 1;\n  expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_eq!(
        observed_and_expected(body),
        vec![(Some("loyaltyTier(points)".to_string()), None, true, 7)]
    );
}

// Independent literal-oracle additions for tests/table_case_tests.rs.
// Reuses its retained public analyze_diff helper `tier_finding`.
#[test]
fn empty_parameter_tables_register_no_test_evidence() -> Result<(), String> {
    for registration in [
        r#"test.each([])("dead", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"it.each([])("dead", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"test.only.each([])("dead", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"test.concurrent.each([])("dead", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"test.each(([] as const))("dead", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"describe.each([])("dead suite", () => { test("dead child", () => { expect(loyaltyTier(1000)).toBe("gold"); }); });"#,
        r#"suite.each([])("dead suite", () => { test("dead child", () => { expect(loyaltyTier(1000)).toBe("gold"); }); });"#,
        r#"describe.each(([] satisfies readonly unknown[]))("dead suite", () => { test("dead child", () => { expect(loyaltyTier(1000)).toBe("gold"); }); });"#,
        r#"test.skip("dead", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"test.todo("dead", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
    ] {
        let source = format!("import {{ loyaltyTier }} from \"../src/tier\";\n{registration}\n");
        let file = Path::new("tests/tier.test.ts");
        let tests = extract_tests(file, &source);
        assert_eq!(tests.len(), 0, "{registration}");
        assert_eq!(observed_and_expected(&source), Vec::new(), "{registration}");
        assert!(
            detect_partial_test_extraction(file, &source, &tests).is_none(),
            "{registration}"
        );
        let finding = tier_finding("empty-parameter-table", &source)?;
        assert_eq!(finding.class, ExposureClass::NoStaticPath, "{registration}");
        assert_eq!(finding.ripr.reach.state, StageState::No, "{registration}");
        assert_eq!(finding.related_tests.len(), 0, "{registration}");
        assert_evidence_lacks(&finding, "typescript_test_extraction_partial");
    }
    Ok(())
}

#[test]
fn nonempty_zero_parameter_cases_and_ordinary_siblings_retain_exact_credit() -> Result<(), String> {
    for registration in [
        r#"test.each([[]])("live", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"test.only.each([[]])("live", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"test.each([1000])("live", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
        r#"describe("live suite", () => { test("live", () => { expect(loyaltyTier(1000)).toBe("gold"); }); });"#,
        r#"describe.each([])("dead suite", () => { test("dead child", () => { expect(loyaltyTier(1000)).toBe("gold"); }); });
           test("live", () => { expect(loyaltyTier(1000)).toBe("gold"); });"#,
    ] {
        let source = format!("import {{ loyaltyTier }} from \"../src/tier\";\n{registration}\n");
        let file = Path::new("tests/tier.test.ts");
        let tests = extract_tests(file, &source);
        assert_eq!(tests.len(), 1, "{registration}");
        assert_eq!(tests[0].local_name, "live", "{registration}");
        assert_eq!(tests[0].assertions.len(), 1, "{registration}");
        assert_eq!(
            tests[0].assertions[0].observed_expression.as_deref(),
            Some("loyaltyTier(1000)")
        );
        assert_eq!(
            tests[0].assertions[0].expected_value_or_variant.as_deref(),
            Some("\"gold\"")
        );
        assert!(!tests[0].assertions[0].has_dynamic_matcher_arg);
        let finding = tier_finding("nonempty-or-ordinary-parameter-table", &source)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{registration}");
        assert_eq!(finding.ripr.reach.state, StageState::Yes, "{registration}");
        assert_eq!(finding.related_tests.len(), 1, "{registration}");
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
        assert_evidence_lacks(&finding, "typescript_test_extraction_partial");
    }
    Ok(())
}

#[test]
fn empty_tables_preserve_partial_disclosure_for_evaluated_title_expressions() -> Result<(), String>
{
    let file = Path::new("tests/tier.test.ts");
    let source = r#"import { loyaltyTier } from "../src/tier";
describe.each([])(class EvaluatedTitle {
    static { test("evaluated live test", () => { expect(loyaltyTier(1000)).toBe("gold"); }); }
}, () => { test("dead child", () => { expect(loyaltyTier(1000)).toBe("gold"); }); });
"#;
    let tests = extract_tests(file, source);
    assert_eq!(tests.len(), 0);
    let gap = detect_partial_test_extraction(file, source, &tests);
    assert!(
        gap.is_some(),
        "evaluated title expressions can register a live test"
    );
    if let Some(gap) = gap {
        assert_eq!(gap.sample_line, 3);
        assert_eq!(gap.shape, "test/it call in loop/callback/nested body");
        assert!(gap.snippet.contains("evaluated live test"));
        assert!(!gap.snippet.contains("dead child"));
    }
    let root = ts_unique_tempdir("empty-table-evaluated-title")?;
    ts_write_file(&root.join("src/tier.ts"), TIER)?;
    ts_write_file(&root.join("tests/tier.test.ts"), source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/tier.ts", &[TIER_LINE])],
    );
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(result.limitations.iter().any(|limitation| {
        limitation
            .bounded_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("typescript_test_extraction_partial"))
    }));
    let finding = result
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == TIER_LINE.0)
        .ok_or_else(|| "missing evaluated-title finding".to_string())?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert_eq!(finding.ripr.reach.state, StageState::No);
    assert_eq!(finding.related_tests.len(), 0);
    Ok(())
}

#[test]
fn empty_tables_preserve_partial_disclosure_for_evaluated_sequence_titles() -> Result<(), String> {
    let file = Path::new("tests/tier.test.ts");
    let source = r#"import { test, describe, expect } from "vitest";
import { loyaltyTier } from "../src/tier";
describe.each([])((test("evaluated live test", () => { expect(loyaltyTier(1000)).toBe("gold"); }), "dead suite"), () => { test("dead child", () => { expect(loyaltyTier(1000)).toBe("gold"); }); });
"#;
    let tests = extract_tests(file, source);
    assert_eq!(tests.len(), 0);
    let gap = detect_partial_test_extraction(file, source, &tests);
    assert!(
        gap.is_some(),
        "evaluated sequence titles register a live test"
    );
    if let Some(gap) = gap {
        assert_eq!(gap.sample_line, 3);
        assert_eq!(gap.shape, "test/it call in loop/callback/nested body");
        assert!(gap.snippet.contains("evaluated live test"));
        assert!(!gap.snippet.contains("dead child"));
    }
    let root = ts_unique_tempdir("empty-table-evaluated-sequence-title")?;
    ts_write_file(&root.join("src/tier.ts"), TIER)?;
    ts_write_file(&root.join("tests/tier.test.ts"), source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/tier.ts", &[TIER_LINE])],
    );
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(result.limitations.iter().any(|limitation| {
        limitation
            .bounded_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("typescript_test_extraction_partial"))
    }));
    let finding = result
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == TIER_LINE.0)
        .ok_or_else(|| "missing evaluated-sequence-title finding".to_string())?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert_eq!(finding.ripr.reach.state, StageState::No);
    assert_eq!(finding.related_tests.len(), 0);
    Ok(())
}

#[test]
fn empty_tables_ignore_discarded_function_values_in_sequence_titles() -> Result<(), String> {
    let file = Path::new("tests/tier.test.ts");
    let source = r#"import { test, describe, expect } from "vitest";
import { loyaltyTier } from "../src/tier";
describe.each([])((() => { test("dead title callback", () => { expect(loyaltyTier(1000)).toBe("gold"); }); }, "dead suite"), () => { test("dead child", () => { expect(loyaltyTier(1000)).toBe("gold"); }); });
"#;
    let tests = extract_tests(file, source);
    assert_eq!(tests.len(), 0);
    let gap = detect_partial_test_extraction(file, source, &tests);
    assert!(
        gap.is_none(),
        "discarded function values cannot register tests"
    );
    let root = ts_unique_tempdir("empty-table-discarded-sequence-function")?;
    ts_write_file(&root.join("src/tier.ts"), TIER)?;
    ts_write_file(&root.join("tests/tier.test.ts"), source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/tier.ts", &[TIER_LINE])],
    );
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert!(result.limitations.iter().all(|limitation| {
        limitation
            .bounded_detail
            .as_deref()
            .is_none_or(|detail| !detail.contains("typescript_test_extraction_partial"))
    }));
    let finding = result
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == TIER_LINE.0)
        .ok_or_else(|| "missing discarded-sequence-function finding".to_string())?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert_eq!(finding.ripr.reach.state, StageState::No);
    assert_eq!(finding.related_tests.len(), 0);
    Ok(())
}

#[test]
fn empty_tables_disclose_only_executed_inline_title_functions() -> Result<(), String> {
    let file = Path::new("tests/tier.test.ts");
    let cases = [
        (
            "arrow-block",
            r#"(() => { test("live arrow title", () => { expect(loyaltyTier(1000)).toBe("gold"); }); return "dead suite"; })()"#,
            true,
        ),
        (
            "ordinary-function",
            r#"(function () { test("live function title", () => { expect(loyaltyTier(1000)).toBe("gold"); }); return "dead suite"; })()"#,
            true,
        ),
        (
            "expression-arrow",
            r#"(() => (test("live concise title", () => { expect(loyaltyTier(1000)).toBe("gold"); }), "dead suite"))()"#,
            true,
        ),
        (
            "generator-not-advanced",
            r#"(function* () { test("dead generator title", () => { expect(loyaltyTier(1000)).toBe("gold"); }); return "dead suite"; })()"#,
            false,
        ),
        (
            "returned-dormant-function",
            r#"(() => (() => { test("dead returned function title", () => { expect(loyaltyTier(1000)).toBe("gold"); }); }))()"#,
            false,
        ),
        (
            "uninvoked-dormant-function",
            r#"(() => { test("dead uninvoked function title", () => { expect(loyaltyTier(1000)).toBe("gold"); }); return "dead suite"; })"#,
            false,
        ),
    ];
    for (label, title, partial_warning) in cases {
        let source = format!(
            "import {{ test, describe, expect }} from \"vitest\";\n\
             import {{ loyaltyTier }} from \"../src/tier\";\n\
             describe.each([])({title}, () => {{ test(\"dead empty callback\", () => {{ expect(loyaltyTier(1000)).toBe(\"gold\"); }}); }});\n"
        );
        let tests = extract_tests(file, &source);
        assert_eq!(tests.len(), 0, "{label}");
        let gap = detect_partial_test_extraction(file, &source, &tests);
        assert_eq!(gap.is_some(), partial_warning, "{label}");
        if let Some(gap) = gap {
            assert!(gap.snippet.contains("live"), "{label}: {}", gap.snippet);
            assert!(!gap.snippet.contains("dead empty callback"), "{label}");
        }
        let root = ts_unique_tempdir(&format!("empty-table-iife-{label}"))?;
        ts_write_file(&root.join("src/tier.ts"), TIER)?;
        ts_write_file(&root.join("tests/tier.test.ts"), &source)?;
        let result = TypeScriptAdapter.analyze_diff(
            &ts_analysis_options(root.clone()),
            &OraclePolicy::default(),
            &[changed_with_lines("src/tier.ts", &[TIER_LINE])],
        );
        let _ = std::fs::remove_dir_all(&root);
        let result = result?;
        let actual_partial = result.limitations.iter().any(|limitation| {
            limitation
                .bounded_detail
                .as_deref()
                .is_some_and(|detail| detail.contains("typescript_test_extraction_partial"))
        });
        assert_eq!(actual_partial, partial_warning, "{label}");
        assert_eq!(result.findings.len(), 1, "{label}");
        let finding = result
            .findings
            .into_iter()
            .find(|finding| finding.probe.location.line == TIER_LINE.0)
            .ok_or_else(|| format!("missing IIFE finding: {label}"))?;
        assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
        assert_eq!(finding.ripr.reach.state, StageState::No, "{label}");
        assert_eq!(finding.related_tests.len(), 0, "{label}");
    }
    Ok(())
}

#[test]
fn row_writes_withhold_each_literals_and_disclose_for_registrations() {
    fn assert_for_disclosed(source: &str, statement: &str) {
        let file = Path::new("tests/tier.test.ts");
        let tests = extract_tests(file, source);
        assert!(tests.is_empty(), "{statement}: {tests:?}");
        let gap = detect_partial_test_extraction(file, source, &tests);
        assert!(
            gap.is_some_and(|gap| gap.shape == "test.for registration"),
            "{statement}"
        );
    }

    for write in [
        "[p] = [5]",
        "({ p } = value)",
        "[a, p] = value",
        "({ key: [p] } = value)",
        "delete (p)[0]",
        "(p = 5)",
        "(p++)",
    ] {
        for (method, binding) in [("each", "p, tier"), ("for", "[p, tier]")] {
            let source = format!(
                "import {{ test, expect }} from 'vitest';\ntest.{method}([[999, 'silver']])('tier', ({binding}) => {{\n  {write};\n  expect(loyaltyTier(p)).toBe(tier);\n}});"
            );
            if method == "each" {
                assert_eq!(
                    observed_and_expected(&source),
                    vec![(Some("loyaltyTier(p)".into()), None, true, 4)],
                    "{write}"
                );
            } else {
                assert_for_disclosed(&source, write);
            }
        }
    }

    for (statement, concrete) in [
        ("[row[0]] = [5]", false),
        ("delete row[0]", false),
        ("[other] = [row[0]]", true),
        ("[other[row[0]]] = [5]", true),
        ("delete other[row[0]]", true),
        ("[other = f(row[0])] = []", true),
    ] {
        let source = format!(
            "import {{ test, expect }} from 'vitest';\ntest.for([[999, 'silver']])('tier', (row) => {{\n  {statement};\n  expect(loyaltyTier(row[0])).toBe(row[1]);\n}});"
        );
        if concrete {
            assert_eq!(
                observed_and_expected(&source),
                vec![(
                    Some("loyaltyTier(999)".into()),
                    Some("'silver'".into()),
                    false,
                    4
                )],
                "{statement}"
            );
        } else {
            assert_for_disclosed(&source, statement);
        }
    }
}

#[test]
fn object_each_row_writes_keep_dynamic_expected_values() {
    for (binding, value, write) in [
        ("row", "row.out", "[row.out] = [5]"),
        ("row", "row.out", "delete row.out"),
        ("{out}", "out", "[out] = [5]"),
    ] {
        let source = format!(
            "test.each([{{out: 17}}])('result', ({binding}) => {{\n  {write};\n  expect(f()).toBe({value});\n}});"
        );
        assert_eq!(
            observed_and_expected(&source),
            vec![(Some("f()".into()), None, true, 3)],
            "{write}"
        );
    }
}
