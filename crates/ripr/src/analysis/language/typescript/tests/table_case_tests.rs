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
