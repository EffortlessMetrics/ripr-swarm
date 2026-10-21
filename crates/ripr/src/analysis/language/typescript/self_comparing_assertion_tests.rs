//! Self-comparing exact matchers are tautologies, not exact-value evidence
//! (RIPR-SPEC-0243 rule 9, #8713).

use super::*;

const OWNER: &str = "export function price(amount: number): number {\n  if (amount > 100) {\n    return amount - 10;\n  }\n  return amount;\n}\n";

fn test_file(body: &str) -> String {
    format!("import {{ price }} from \"../src/lib\";\n\ntest(\"price\", () => {{\n{body}\n}});\n")
}

fn finding_for_body(body: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/lib.ts"), OWNER);
    assert_eq!(owners.len(), 1, "fixture must contain its exported owner");
    let source = test_file(body);
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    assert_eq!(tests.len(), 1, "fixture must register its test");
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

/// Direct spelling of the tautology (#8713): the expected side re-invokes the
/// owner with the same arguments, so the assertion passes under both sides of
/// the diff. Rule 9 reads it `relational_check` / weak, and the ReturnValue
/// finding downgrades from the former false `exposed`.
#[test]
fn ts_self_comparing_owner_recall_downgrades() -> Result<(), String> {
    let assertion = extracted_assertion("  expect(price(150)).toBe(price(150));");
    assert_eq!(
        assertion.oracle_kind,
        OracleKind::RelationalCheck,
        "self-comparing toBe must not read exact_value: {assertion:?}"
    );
    assert_eq!(
        assertion.oracle_strength,
        OracleStrength::Weak,
        "self-comparing toBe must read weak: {assertion:?}"
    );
    let got = finding_for_body("  expect(price(150)).toBe(price(150));")?;
    assert_eq!(
        got.class,
        ExposureClass::WeaklyExposed,
        "self-comparing assertion cannot expose a ReturnValue change: {got:?}"
    );
    Ok(())
}

/// One-hop-local spelling of the tautology (#8713): `const a = price(150);
/// expect(a).toBe(a)` re-reads the same local on both sides. The one-hop
/// alias credit must not turn the self-comparison into an observation.
#[test]
fn ts_self_comparing_one_hop_local_downgrades() -> Result<(), String> {
    let body = "  const a = price(150);\n  expect(a).toBe(a);";
    let assertion = extracted_assertion(body);
    assert_eq!(
        assertion.oracle_kind,
        OracleKind::RelationalCheck,
        "self-comparing one-hop local must not read exact_value: {assertion:?}"
    );
    let got = finding_for_body(body)?;
    assert_eq!(
        got.class,
        ExposureClass::WeaklyExposed,
        "self-comparing one-hop local cannot expose a ReturnValue change: {got:?}"
    );
    Ok(())
}

/// Whitespace outside string literals cannot hide the tautology (#8713).
#[test]
fn ts_self_comparing_ignores_whitespace_outside_strings() {
    let assertion = extracted_assertion("  expect(price( 150 )).toBe( price(150) );");
    assert_eq!(
        assertion.oracle_kind,
        OracleKind::RelationalCheck,
        "formatting alone must not escape the rule 9 demotion: {assertion:?}"
    );
}

/// Whitespace INSIDE a string literal is significant (#8713): `"ab"` and
/// `"a b"` are different expected values, so the assertion is not
/// self-comparing and keeps its exact-value credit.
#[test]
fn ts_distinct_string_literals_stay_exact_value() {
    let source = test_file("  expect(\"ab\").toBe(\"a b\");");
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    let assertion = &tests[0].assertions[0];
    assert_eq!(
        assertion.oracle_kind,
        OracleKind::ExactValue,
        "distinct string literals must keep exact_value: {assertion:?}"
    );
    assert_eq!(
        assertion.oracle_strength,
        OracleStrength::Strong,
        "distinct string literals must keep strong strength: {assertion:?}"
    );
}

/// The adjacent `toEqual` / `toStrictEqual` spellings of the tautology read
/// relational/weak as well (#8713).
#[test]
fn ts_self_comparing_to_equal_family_downgrades() {
    for matcher in ["toEqual", "toStrictEqual"] {
        let body = format!("  expect(price(150)).{matcher}(price(150));");
        let assertion = extracted_assertion(&body);
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::RelationalCheck,
            "self-comparing {matcher} must not read exact_value: {assertion:?}"
        );
        assert_eq!(
            assertion.oracle_strength,
            OracleStrength::Weak,
            "self-comparing {matcher} must read weak: {assertion:?}"
        );
    }
}

/// Control: an unrelated expected value is a real discriminator and keeps the
/// `exposed` class on the same change (#8713).
#[test]
fn ts_distinct_expected_stays_exposed() -> Result<(), String> {
    let got = finding_for_body("  expect(price(150)).toBe(130);")?;
    assert_eq!(
        got.class,
        ExposureClass::Exposed,
        "distinct expected value must keep the owner-call observation: {got:?}"
    );
    Ok(())
}
