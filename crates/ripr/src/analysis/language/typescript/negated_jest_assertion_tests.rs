//! Negated Jest/Vitest assertions earn capped weak credit
//! (RIPR-SPEC-0243 rule 2, #8712).

use super::*;

const OWNER: &str = "export function price(amount: number): number {\n  if (amount > 100) {\n    return amount - 10;\n  }\n  return amount;\n}\n";

fn test_file(body: &str) -> String {
    format!("import {{ price }} from \"../src/lib\";\n\ntest(\"price\", () => {{\n{body}\n}});\n")
}

fn extracted_assertion(body: &str) -> TypeScriptAssertion {
    let source = test_file(body);
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    assert_eq!(tests.len(), 1, "fixture must register its test");
    assert_eq!(
        tests[0].assertions.len(),
        1,
        "fixture must hold one assertion"
    );
    tests[0].assertions[0].clone()
}

fn finding_for_body(body: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/lib.ts"), OWNER);
    assert_eq!(owners.len(), 1, "fixture must contain its exported owner");
    let source = test_file(body);
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    let line_text = OWNER
        .lines()
        .nth(2)
        .ok_or_else(|| "fixture line absent".to_string())?;
    classify_change(
        Path::new("src/lib.ts"),
        3,
        line_text,
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "fixture must produce a finding".to_string())
}

/// The issue repro (#8712): `expect(price(150)).not.toBe(150)` was dropped
/// from the assertion inventory (zero oracle credit), while the chai and AVA
/// negations in the same adapter earned `relational_check` / weak. Rule 2
/// caps the negated exact matcher at relational/weak and carries no expected
/// value.
#[test]
fn ts_negated_tobe_reads_relational_weak() -> Result<(), String> {
    let assertion = extracted_assertion("  expect(price(150)).not.toBe(150);");
    assert_eq!(
        assertion.oracle_kind,
        OracleKind::RelationalCheck,
        "negated toBe must read relational_check: {assertion:?}"
    );
    assert_eq!(
        assertion.oracle_strength,
        OracleStrength::Weak,
        "negated toBe must read weak: {assertion:?}"
    );
    assert!(
        assertion.expected_value_or_variant.is_none(),
        "a negated matcher operand is not an expected value: {assertion:?}"
    );
    assert!(
        !assertion.has_dynamic_matcher_arg,
        "a negated operand must not raise the dynamic-assertion limitation: {assertion:?}"
    );
    let got = finding_for_body("  expect(price(150)).not.toBe(150);")?;
    assert_eq!(
        got.class,
        ExposureClass::WeaklyExposed,
        "negated exact matcher cannot expose a ReturnValue change: {got:?}"
    );
    let all_text: String = got.missing.join("\n");
    assert!(
        !all_text.contains("typescript_oracle_helper_gated"),
        "a test whose only assertion is negated must not report helper_gated: {all_text:?}"
    );
    Ok(())
}

/// RIPR-SPEC-0243 acceptance example 6 (#8712): `not.toThrow()` reads
/// `broad_error` / weak, and the exact-payload override is disabled under
/// `.not`.
#[test]
fn ts_negated_tothrow_reads_broad_error_weak() {
    for call in [
        "expect(() => price(150)).not.toThrow();",
        "expect(() => price(150)).not.toThrow(Error);",
        "expect(() => price(150)).not.toThrow('bad');",
    ] {
        let assertion = extracted_assertion(&format!("  {call}"));
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::BroadError,
            "{call}: negated toThrow must read broad_error: {assertion:?}"
        );
        assert_eq!(
            assertion.oracle_strength,
            OracleStrength::Weak,
            "{call}: negated toThrow must read weak: {assertion:?}"
        );
        assert!(
            assertion.error_payload.is_none(),
            "{call}: the exact payload override never applies under .not: {assertion:?}"
        );
    }
}

/// The mock list under `.not` reads `mock_expectation` / weak with no payload
/// (#8712).
#[test]
fn ts_negated_mock_matchers_read_weak_without_payload() {
    for matcher in ["toHaveBeenCalled()", "toHaveBeenCalledWith(150)"] {
        let call = format!("expect(price).not.{matcher};");
        let assertion = extracted_assertion(&format!("  {call}"));
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::MockExpectation,
            "{call}: negated mock matcher keeps its kind: {assertion:?}"
        );
        assert_eq!(
            assertion.oracle_strength,
            OracleStrength::Weak,
            "{call}: negated mock matcher must read weak: {assertion:?}"
        );
        assert!(
            assertion.mock_payload.is_none(),
            "{call}: a negated operand is not a mock-call payload: {assertion:?}"
        );
    }
}

/// Relational and smoke matchers keep their kind under `.not`, capped at
/// weak / smoke (#8712).
#[test]
fn ts_negated_relational_and_smoke_matchers_cap_by_class() {
    for (matcher, kind, strength) in [
        (
            "toBeGreaterThan(100)",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        ),
        (
            "toContain(1)",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        ),
        ("toBeTruthy()", OracleKind::SmokeOnly, OracleStrength::Smoke),
    ] {
        let call = format!("expect(price(150)).not.{matcher};");
        let assertion = extracted_assertion(&format!("  {call}"));
        assert_eq!(
            assertion.oracle_kind, kind,
            "{call}: kind mismatch: {assertion:?}"
        );
        assert_eq!(
            assertion.oracle_strength, strength,
            "{call}: strength mismatch: {assertion:?}"
        );
    }
}

/// Snapshots and unknown matchers under `.not` read `unknown` / unknown —
/// never the medium snapshot grade (#8712).
#[test]
fn ts_negated_snapshot_and_unknown_read_unknown() {
    for matcher in ["toMatchSnapshot()", "toBeWithinRange(1, 2)"] {
        let call = format!("expect(price(150)).not.{matcher};");
        let assertion = extracted_assertion(&format!("  {call}"));
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::Unknown,
            "{call}: negated unrecognised matcher must read unknown: {assertion:?}"
        );
        assert_eq!(
            assertion.oracle_strength,
            OracleStrength::Unknown,
            "{call}: negated unrecognised matcher must read unknown strength: {assertion:?}"
        );
    }
}

/// The `.not` hop composes with `.resolves` in either order (#8712).
#[test]
fn ts_negation_composes_with_resolves_modifier() {
    for chain in ["resolves.not.toBe(150)", "not.resolves.toBe(150)"] {
        let source = format!(
            "import {{ price }} from \"../src/lib\";\n\ntest(\"price\", async () => {{\n  await expect(price(150)).{chain};\n}});\n"
        );
        let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
        assert_eq!(tests.len(), 1, "{chain}: fixture must register its test");
        assert_eq!(
            tests[0].assertions.len(),
            1,
            "{chain}: fixture must hold one assertion"
        );
        let assertion = &tests[0].assertions[0];
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::RelationalCheck,
            "{chain}: negated resolves.toBe must read relational_check: {assertion:?}"
        );
        assert_eq!(
            assertion.oracle_strength,
            OracleStrength::Weak,
            "{chain}: negated resolves.toBe must read weak: {assertion:?}"
        );
    }
}
