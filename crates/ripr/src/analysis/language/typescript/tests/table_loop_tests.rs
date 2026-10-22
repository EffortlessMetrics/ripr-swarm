//! A `for (const [a, b] of rows)` loop over a test-local literal table
//! reads one concrete case per row (RIPR-SPEC-0243 rule 11, #9533). Each
//! test pins the parsed per-row assertions before the class.

use super::*;

const TIER: &str = "export function loyaltyTier(points: number): string {\n  if (1000 <= points) {\n    return \"gold\";\n  }\n  return \"silver\";\n}\n";
const TIER_LINE: (usize, &str) = (2, "  if (1000 <= points) {");

fn loop_test(rows: &str, body: &str) -> String {
    format!(
        "import {{ loyaltyTier }} from \"../src/tier\";\n\ntest(\"tiers\", () => {{\n  const rows = [\n{rows}\n  ];\n  for (const [points, tier] of rows) {{\n{body}\n  }}\n}});\n"
    )
}

fn observed(source: &str) -> Vec<(Option<String>, Option<String>, usize)> {
    extract_tests(Path::new("tests/tier.test.ts"), source)
        .iter()
        .flat_map(|test| test.assertions.iter())
        .map(|assertion| {
            (
                assertion.observed_expression.clone(),
                assertion.expected_value_or_variant.clone(),
                assertion.line,
            )
        })
        .collect()
}

fn tier_class(label: &str, source: &str) -> Result<ExposureClass, String> {
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
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

const ASSERT: &str = "    expect(loyaltyTier(points)).toBe(tier);";

#[test]
fn loop_rows_pinning_the_boundary_are_exposed() -> Result<(), String> {
    let source = loop_test("    [999, \"silver\"],\n    [1000, \"gold\"],", ASSERT);
    assert_eq!(
        observed(&source),
        vec![
            (
                Some("loyaltyTier(999)".to_string()),
                Some("\"silver\"".to_string()),
                9
            ),
            (
                Some("loyaltyTier(1000)".to_string()),
                Some("\"gold\"".to_string()),
                9
            ),
        ]
    );
    assert_eq!(tier_class("loop-pinned", &source)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn loop_rows_missing_the_boundary_stay_weak() -> Result<(), String> {
    let source = loop_test("    [500, \"silver\"],\n    [2000, \"gold\"],", ASSERT);
    assert_eq!(observed(&source).len(), 2);
    assert_eq!(
        tier_class("loop-missed", &source)?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

/// Every shape the reader refuses keeps the loop's own assertion, whose
/// observed call still names the loop variable.
#[test]
fn unreadable_loops_keep_the_loop_variable_assertion() -> Result<(), String> {
    let rows = "    [999, \"silver\"],\n    [1000, \"gold\"],";

    // A non-literal cell.
    let computed = loop_test("    [999, \"silver\"],\n    [base + 1, \"gold\"],", ASSERT);
    assert_eq!(observed(&computed).len(), 1);
    assert_eq!(
        tier_class("loop-computed", &computed)?,
        ExposureClass::WeaklyExposed
    );

    // The table is read again, so the loop may not see what was declared.
    let mutated =
        loop_test(rows, ASSERT).replace("  for (const", "  rows.push([5, \"x\"]);\n  for (const");
    assert!(
        observed(&mutated)
            .iter()
            .all(|(call, _, _)| call.as_deref() == Some("loyaltyTier(points)"))
    );

    // An exit that can end the loop before a row runs.
    let breaking = loop_test(rows, &format!("    if (points > 1) {{ break; }}\n{ASSERT}"));
    assert!(
        observed(&breaking)
            .iter()
            .all(|(call, _, _)| call.as_deref() == Some("loyaltyTier(points)"))
    );
    assert_eq!(
        tier_class("loop-break", &breaking)?,
        ExposureClass::WeaklyExposed
    );

    // A body that is not a block.
    let bare = "import { loyaltyTier } from \"../src/tier\";\n\ntest(\"tiers\", () => {\n  const rows = [\n    [999, \"silver\"],\n    [1000, \"gold\"],\n  ];\n  for (const [points, tier] of rows) expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_eq!(
        observed(bare),
        vec![(Some("loyaltyTier(points)".to_string()), None, 8)]
    );
    Ok(())
}
