//! Literal rejection equality observes an error reason, not a resolved value (#7315).

use super::*;

const OWNER: &str = include_str!(
    "../../../../../../fixtures/typescript_rejects_literal_equality/input/src/load.ts"
);
const TEST: &str = include_str!(
    "../../../../../../fixtures/typescript_rejects_literal_equality/input/tests/load.test.ts"
);

const REEXPORT_SHADOW: &str = include_str!(
    "../../../../../../fixtures/typescript_rejects_reexport_shadow/input/tests/load.test.ts"
);
const UNEXERCISED_ERROR: &str = include_str!(
    "../../../../../../fixtures/typescript_rejects_literal_equality/counterexamples/unexercised-error/load.ts"
);

fn finding(test_source: &str, line: usize) -> Result<Finding, String> {
    finding_for_owner(OWNER, test_source, line)
}

fn finding_for_owner(
    owner_source: &str,
    test_source: &str,
    line: usize,
) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/load.ts"), owner_source);
    assert_eq!(
        owners.len(),
        1,
        "fixture must contain its exported async owner"
    );
    let tests = extract_tests(Path::new("tests/load.test.ts"), test_source);
    assert_eq!(tests.len(), 1, "fixture must register its async test");
    assert_eq!(
        tests[0].assertions.len(),
        1,
        "fixture must contain its assertion"
    );
    let line_text = owner_source
        .lines()
        .nth(line - 1)
        .ok_or("fixture line absent")?;
    classify_change(
        Path::new("src/load.ts"),
        line,
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
fn literal_rejection_equality_observes_error_path() -> Result<(), String> {
    for (module, matcher) in [
        ("vitest", "toBe"),
        ("vitest", "toEqual"),
        ("vitest", "toStrictEqual"),
        ("@jest/globals", "toBe"),
    ] {
        let test = TEST.replace("vitest", module).replace("toBe", matcher);
        let got = finding(&test, 3)?;
        assert_eq!(got.probe.family, ProbeFamily::ErrorPath);
        assert_eq!(got.class, ExposureClass::Exposed, "{matcher}: {got:?}");
        assert_eq!(got.related_tests.len(), 1);
        assert_eq!(
            got.related_tests[0].oracle_kind,
            OracleKind::ExactErrorVariant
        );
        assert_eq!(got.related_tests[0].oracle_strength, OracleStrength::Strong);
        assert_eq!(
            got.related_tests[0].oracle.as_deref(),
            Some(format!("expect(...).rejects.{matcher}(...)").as_str())
        );
    }
    Ok(())
}

#[test]
fn unknown_or_shadowed_expect_does_not_gain_rejection_credit() -> Result<(), String> {
    let shadow = "const expect = (_value: unknown) => ({ rejects: { toBe() {} } });\n";
    for test in [
        TEST.replace("{ expect, test }", "{ test }"),
        TEST.replace("import { expect, test } from \"vitest\";", "import { test } from \"vitest\";\nconst expect = (_value: unknown) => ({ rejects: { toBe() {} } });"),
        TEST.replace("  await expect", &format!("  {shadow}  await expect")),
        TEST.replace("async () =>", "async (expect) =>"),
        format!("import {{ expect, test, describe }} from 'vitest';\nimport {{ load }} from '../src/load';\ndescribe('nested', () => {{\n{shadow}test('missing token', async () => {{ await expect(load()).rejects.toBe('TOKEN_REQUIRED'); }});\n}});"),
    ] {
        let got = finding(&test, 3)?;
        assert_eq!(got.class, ExposureClass::WeaklyExposed, "{got:?}");
        assert_eq!(got.related_tests[0].oracle_kind, OracleKind::Unknown);
    }
    Ok(())
}

#[test]
fn rejection_equality_cannot_expose_resolved_return() -> Result<(), String> {
    let got = finding(TEST, 5)?;
    assert_eq!(got.probe.family, ProbeFamily::ReturnValue);
    assert_eq!(got.class, ExposureClass::WeaklyExposed, "{got:?}");
    assert_eq!(got.related_tests[0].oracle_kind, OracleKind::Unknown);
    Ok(())
}

#[test]
fn resolved_equality_cannot_expose_rejection() -> Result<(), String> {
    let test = TEST
        .replace("load()).rejects", "load({ token: 'ok' })).resolves")
        .replace("TOKEN_REQUIRED", "ready");
    let got = finding(&test, 3)?;
    assert_eq!(got.probe.family, ProbeFamily::ErrorPath);
    assert_eq!(got.class, ExposureClass::WeaklyExposed, "{got:?}");
    let got = finding(&test, 5)?;
    assert_eq!(got.probe.family, ProbeFamily::ReturnValue);
    assert_eq!(got.class, ExposureClass::Exposed, "{got:?}");
    assert_eq!(got.related_tests[0].oracle_kind, OracleKind::ExactValue);
    Ok(())
}

#[test]
fn unresolved_rejection_checks_do_not_gain_exact_error_credit() {
    for (call, kind, strength) in [
        ("toBeTruthy()", OracleKind::SmokeOnly, OracleStrength::Smoke),
        (
            "toBe(expected)",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        ),
        (
            "toCustomReason('TOKEN_REQUIRED')",
            OracleKind::Unknown,
            OracleStrength::Unknown,
        ),
    ] {
        let source = TEST.replace("toBe(\"TOKEN_REQUIRED\")", call);
        let tests = extract_tests(Path::new("tests/load.test.ts"), &source);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].assertions.len(), 1);
        let assertion = &tests[0].assertions[0];
        assert_eq!(assertion.oracle_kind, kind, "{call}: {assertion:?}");
        assert_eq!(assertion.oracle_strength, strength, "{call}: {assertion:?}");
        if call == "toBe(expected)" {
            assert!(assertion.has_dynamic_matcher_arg);
            assert!(assertion.expected_value_or_variant.is_none());
        }
    }
    let negated = TEST.replace(".rejects.toBe", ".rejects.not.toBe");
    let tests = extract_tests(Path::new("tests/load.test.ts"), &negated);
    assert_eq!(tests.len(), 1);
    assert!(tests[0].assertions.is_empty());
}

#[test]
fn reexport_does_not_bind_the_local_rejection_assertion() -> Result<(), String> {
    let got = finding(REEXPORT_SHADOW, 3)?;
    assert_eq!(got.class, ExposureClass::WeaklyExposed, "{got:?}");
    assert_eq!(got.related_tests[0].oracle_kind, OracleKind::Unknown);
    Ok(())
}

#[test]
fn rejection_literal_cannot_expose_an_unexercised_error_branch() -> Result<(), String> {
    // Known integration blocker: #6798 owns expected-value error liveness.
    // Keep this regression live rather than hiding the unsafe new promotion.
    let got = finding_for_owner(UNEXERCISED_ERROR, TEST, 6)?;
    assert_eq!(got.probe.family, ProbeFamily::ErrorPath);
    assert_eq!(got.class, ExposureClass::WeaklyExposed, "{got:?}");
    Ok(())
}
