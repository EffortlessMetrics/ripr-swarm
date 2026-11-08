//! A top-level asymmetric matcher or a derived-property observation is a
//! partial check, not `exact_value` evidence (RIPR-SPEC-0243 oracle strength).

use super::*;

const OWNER: &str = "export function score(amount: number): boolean {\n  return amount > 100;\n}\n";

fn test_file(body: &str) -> String {
    format!("import {{ score }} from \"../src/lib\";\n\ntest(\"score\", () => {{\n{body}\n}});\n")
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
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &test_file(body));
    assert_eq!(tests.len(), 1, "fixture must register its test");
    let line_text = OWNER
        .lines()
        .nth(1)
        .ok_or_else(|| "fixture line absent".to_string())?;
    classify_change(
        Path::new("src/lib.ts"),
        2,
        line_text,
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "fixture must produce a finding".to_string())
}

#[test]
fn top_level_asymmetric_matchers_read_weak() -> Result<(), String> {
    for expected in [
        "expect.anything()",
        "expect.any(Boolean)",
        "expect.stringMatching(/t/)",
        "expect.not.stringContaining(\"zzz\")",
        "expect.objectContaining({ a: 1 })",
        "(expect.anything())",
    ] {
        for matcher in ["toBe", "toEqual", "toStrictEqual"] {
            let body = format!("  expect(score(100)).{matcher}({expected});");
            let assertion = extracted_assertion(&body);
            assert_eq!(
                assertion.oracle_kind,
                OracleKind::RelationalCheck,
                "{body} must not read exact_value: {assertion:?}"
            );
            assert_eq!(assertion.oracle_strength, OracleStrength::Weak, "{body}");
        }
        let got = finding_for_body(&format!("  expect(score(100)).toEqual({expected});"))?;
        assert_eq!(
            got.class,
            ExposureClass::WeaklyExposed,
            "{expected} matches any value and cannot expose the change: {got:?}"
        );
    }
    Ok(())
}

#[test]
fn derived_property_observations_read_weak() -> Result<(), String> {
    for observed in [
        "typeof score(100)",
        "[score(100)].length",
        "([score(100)]).length",
    ] {
        let body = format!("  expect({observed}).toBe(1);");
        let assertion = extracted_assertion(&body);
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::RelationalCheck,
            "{body} must not read exact_value: {assertion:?}"
        );
        let got = finding_for_body(&body)?;
        assert_eq!(
            got.class,
            ExposureClass::WeaklyExposed,
            "{body} cannot expose the change: {got:?}"
        );
    }
    Ok(())
}

/// Positive controls: a literal expectation and an observation of the value
/// itself keep their exact-value credit, and a data-dependent length is not a
/// derived constant.
#[test]
fn literal_and_value_expectations_stay_exact_value() -> Result<(), String> {
    for body in [
        "  expect(score(100)).toBe(false);",
        "  expect(score(100)).toEqual(false);",
        "  expect(score(100)).toStrictEqual(false);",
        "  expect([score(100)]).toEqual([false]);",
        "  expect([...score(100)].length).toBe(1);",
        "  expect(score(100).length).toBe(1);",
    ] {
        let assertion = extracted_assertion(body);
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::ExactValue,
            "{body} must keep exact_value: {assertion:?}"
        );
        assert_eq!(assertion.oracle_strength, OracleStrength::Strong, "{body}");
    }
    let got = finding_for_body("  expect(score(100)).toBe(false);")?;
    assert_eq!(got.class, ExposureClass::Exposed, "{got:?}");
    Ok(())
}
