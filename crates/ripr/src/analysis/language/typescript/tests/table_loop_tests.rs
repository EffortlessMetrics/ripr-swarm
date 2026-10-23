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

/// The refused loop keeps its own assertion, whose observed call still
/// names the loop variable rather than a row literal.
fn keeps_loop_variable(source: &str) {
    let calls = observed(source);
    assert!(!calls.is_empty(), "no assertion parsed from {source}");
    assert!(
        calls
            .iter()
            .all(|(call, _, _)| call.as_deref() == Some("loyaltyTier(points)")),
        "{calls:?}"
    );
}

/// No observed call names a row literal: a refused loop either keeps the
/// loop-variable assertion or, for an expression statement the collector
/// does not read, collects nothing.
fn names_no_row_literal(source: &str) {
    assert!(
        observed(source)
            .iter()
            .all(|(call, _, _)| call.as_deref() == Some("loyaltyTier(points)"))
    );
}

const TWO_ROWS: &str = "    [999, \"silver\"],\n    [1000, \"gold\"],";

#[test]
fn a_non_literal_cell_keeps_the_loop_assertion() -> Result<(), String> {
    let source = loop_test("    [999, \"silver\"],\n    [base + 1, \"gold\"],", ASSERT);
    keeps_loop_variable(&source);
    assert_eq!(
        tier_class("loop-computed", &source)?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn a_table_read_elsewhere_keeps_the_loop_assertion() {
    let pushed = loop_test(TWO_ROWS, ASSERT)
        .replace("  for (const", "  rows.push([5, \"x\"]);\n  for (const");
    keeps_loop_variable(&pushed);
}

#[test]
fn a_let_table_or_a_table_declared_after_the_loop_keeps_the_loop_assertion() {
    keeps_loop_variable(&loop_test(TWO_ROWS, ASSERT).replace("const rows", "let rows"));
    let after = format!(
        "import {{ loyaltyTier }} from \"../src/tier\";\n\ntest(\"tiers\", () => {{\n  for (const [points, tier] of rows) {{\n{ASSERT}\n  }}\n  const rows = [\n{TWO_ROWS}\n  ];\n}});\n"
    );
    keeps_loop_variable(&after);
}

#[test]
fn a_row_longer_than_the_pattern_keeps_the_loop_assertion() {
    keeps_loop_variable(&loop_test(
        "    [999, \"silver\", 1],\n    [1000, \"gold\", 2],",
        ASSERT,
    ));
}

/// A body that can end the loop early runs fewer rows than the table holds.
#[test]
fn an_early_exit_keeps_the_loop_assertion() -> Result<(), String> {
    for (label, exit) in [
        ("break", "break;"),
        ("continue", "continue;"),
        ("return", "return;"),
        ("throw", "throw new Error(\"stop\");"),
    ] {
        let source = loop_test(
            TWO_ROWS,
            &format!("    if (points > 1) {{ {exit} }}\n{ASSERT}"),
        );
        keeps_loop_variable(&source);
        assert_eq!(
            tier_class(&format!("loop-{label}"), &source)?,
            ExposureClass::WeaklyExposed
        );
    }
    Ok(())
}

/// A branch or inner loop in the body decides per row whether the assertion
/// runs, and a substituted literal would credit rows it never reaches.
#[test]
fn a_branch_in_the_body_keeps_the_loop_assertion() -> Result<(), String> {
    for (label, body) in [
        (
            "if",
            format!("    if (points >= 1000) {{\n  {ASSERT}\n    }}"),
        ),
        (
            "switch",
            format!("    switch (points) {{\n      case 1000:\n  {ASSERT}\n    }}"),
        ),
        (
            "while",
            format!("    while (points > 1000) {{\n  {ASSERT}\n    }}"),
        ),
        (
            "and",
            "    points >= 1000 && expect(loyaltyTier(points)).toBe(tier);".to_string(),
        ),
        (
            "ternary",
            "    points >= 1000 ? expect(loyaltyTier(points)).toBe(tier) : 0;".to_string(),
        ),
    ] {
        let source = loop_test(TWO_ROWS, &body);
        names_no_row_literal(&source);
        assert_eq!(
            tier_class(&format!("loop-{label}"), &source)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn a_non_block_body_or_a_redeclared_name_keeps_the_loop_assertion() {
    let bare = "import { loyaltyTier } from \"../src/tier\";\n\ntest(\"tiers\", () => {\n  const rows = [\n    [999, \"silver\"],\n    [1000, \"gold\"],\n  ];\n  for (const [points, tier] of rows) expect(loyaltyTier(points)).toBe(tier);\n});\n";
    assert_eq!(
        observed(bare),
        vec![(Some("loyaltyTier(points)".to_string()), None, 8)]
    );
    keeps_loop_variable(&loop_test(
        TWO_ROWS,
        &format!("    const tier = \"x\";\n{ASSERT}"),
    ));
}

#[test]
fn a_loop_over_an_empty_const_table_holds_no_assertions() -> Result<(), String> {
    let empty = loop_test("", ASSERT);
    assert_eq!(observed(&empty), Vec::new());
    assert_ne!(tier_class("loop-empty", &empty)?, ExposureClass::Exposed);
    // A table the test can still change is not known to be empty.
    let reassignable = empty.replace("const rows", "let rows");
    assert_eq!(
        observed(&reassignable),
        vec![(Some("loyaltyTier(points)".to_string()), None, 8)]
    );
    Ok(())
}
