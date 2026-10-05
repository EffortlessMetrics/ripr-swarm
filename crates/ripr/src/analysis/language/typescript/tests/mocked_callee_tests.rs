//! RIPR-SPEC-0234 rule 8: a mocked dependency on the changed line.
//!
//! When every test file whose assertion would credit `exposed` mocks a module
//! the changed line calls, the mock can replace the changed value, so the
//! finding reads `static_unknown`. Any other static limit is advisory: an
//! `exposed` finding keeps the limit but not `typescript_mock_only_observer`.
//! Each test first pins the parsed subject (owner imports, extracted test,
//! collected mocks, assertion) and only then the class.

use super::*;

/// `price` calls `fee` from `./other` on the changed line (line 5).
const FEE_OWNER: &str = "import { fee } from \"./other\";\n\nexport function price(amount: number): number {\n  if (amount > 100) {\n    return fee(amount - 20);\n  }\n  return amount;\n}\n";
const FEE_LINE: (usize, &str) = (5, "    return fee(amount - 20);");
/// The base owner imports nothing; the changed line is line 3.
const BASE_OWNER: &str = "export function price(amount: number): number {\n  if (amount > 100) {\n    return amount - 20;\n  }\n  return amount;\n}\n";
const BASE_LINE: (usize, &str) = (3, "    return amount - 20;");
const OTHER: &str = "export function fee(v: number): number {\n  return v;\n}\n";

fn test_file(mock: &str, assertion: &str) -> String {
    format!(
        "import {{ price }} from \"../src/lib\";\n\n{mock}\n\ntest(\"price\", () => {{\n  {assertion}\n}});\n"
    )
}

struct Subject<'a> {
    label: &'a str,
    owner: &'a str,
    line: (usize, &'a str),
    tests: &'a [(&'a str, String)],
}

/// Writes the subject, checks the parsed owner and tests, and returns the
/// finding on the changed line.
fn finding_for(subject: &Subject<'_>) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/lib.ts"), subject.owner);
    let [owner] = owners.as_slice() else {
        return Err(format!(
            "{}: expected one owner, got {owners:?}",
            subject.label
        ));
    };
    if owner.name != "price" {
        return Err(format!("{}: owner is {}", subject.label, owner.name));
    }
    for (path, source) in subject.tests {
        let tests = extract_tests(Path::new(path), source);
        let [test] = tests.as_slice() else {
            return Err(format!(
                "{}: {path} must hold one test, got {tests:?}",
                subject.label
            ));
        };
        if test.assertions.len() != 1 {
            return Err(format!(
                "{}: {path} must hold one assertion, got {:?}",
                subject.label, test.assertions
            ));
        }
    }
    let root = ts_unique_tempdir(subject.label)?;
    ts_write_file(&root.join("src/lib.ts"), subject.owner)?;
    ts_write_file(&root.join("src/other.ts"), OTHER)?;
    ts_write_file(&root.join("src/unrelated.ts"), OTHER)?;
    for (path, source) in subject.tests {
        ts_write_file(&root.join(path), source)?;
    }
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/lib.ts", &[subject.line])],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == subject.line.0)
        .ok_or_else(|| format!("{}: expected a finding on the changed line", subject.label))
}

fn assert_mocked_static_unknown(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::MockedModule)
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes);
    assert_eq!(finding.ripr.reveal.observe.state, StageState::Unknown);
    assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
    assert_eq!(finding.stop_reasons, vec![StopReason::StaticProbeUnknown]);
    assert_evidence_contains(finding, "gap_state: static_limitation");
    assert_evidence_contains(
        finding,
        "typescript_limitation: typescript_mock_only_observer",
    );
    assert_evidence_contains(
        finding,
        "static_limit mocked_module: changed line calls `fee` from `./other`",
    );
}

fn assert_exposed_with_advisory_mock(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::MockedModule)
    );
    assert!(finding.stop_reasons.is_empty());
    assert_evidence_contains(finding, "gap_state: static_limitation");
    assert_evidence_lacks(finding, "typescript_mock_only_observer");
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("Static limit `mocked_module`")),
        "the limit's missing text stays: {:?}",
        finding.missing
    );
}

/// Example 24: the test mocks a module the owner does not import.
#[test]
fn spec0234_ex24_mock_of_unimported_module_stays_exposed_without_observer_limitation()
-> Result<(), String> {
    let source = test_file(
        "jest.mock(\"../src/other\");",
        "expect(price(150)).toBe(130);",
    );
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    assert_eq!(
        tests.first().map(|test| test.mocks_in_file.clone()),
        Some(vec!["../src/other".to_string()])
    );
    let owners = extract_owners(Path::new("src/lib.ts"), BASE_OWNER);
    assert!(owners.iter().all(|owner| owner.imports.is_empty()));
    let finding = finding_for(&Subject {
        label: "spec0234-ex24",
        owner: BASE_OWNER,
        line: BASE_LINE,
        tests: &[("tests/lib.test.ts", source)],
    })?;
    assert_exposed_with_advisory_mock(&finding);
    Ok(())
}

/// Example 31: the only exposing test file mocks the module whose `fee` the
/// changed line calls.
#[test]
fn spec0234_ex31_mocked_changed_line_callee_reads_static_unknown() -> Result<(), String> {
    let source = test_file(
        "jest.mock(\"../src/other\", () => ({ fee: () => 5 }));",
        "expect(price(150)).toBe(5);",
    );
    let owners = extract_owners(Path::new("src/lib.ts"), FEE_OWNER);
    let imports: Vec<(String, Option<String>)> = owners
        .iter()
        .flat_map(|owner| owner.imports.iter())
        .map(|import| (import.source.clone(), import.imported.clone()))
        .collect();
    assert_eq!(
        imports,
        vec![("./other".to_string(), Some("fee".to_string()))]
    );
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    assert_eq!(
        tests.first().map(|test| test.mocks_in_file.clone()),
        Some(vec!["../src/other".to_string()])
    );
    assert_eq!(
        tests
            .first()
            .and_then(|test| test.assertions.first())
            .map(|assertion| (
                assertion.oracle_kind.clone(),
                assertion.oracle_strength.clone()
            )),
        Some((OracleKind::ExactValue, OracleStrength::Strong))
    );
    let finding = finding_for(&Subject {
        label: "spec0234-ex31",
        owner: FEE_OWNER,
        line: FEE_LINE,
        tests: &[("tests/lib.test.ts", source)],
    })?;
    assert_mocked_static_unknown(&finding);
    Ok(())
}

/// Example 32: the test mocks an unrelated module while the real `fee` runs.
#[test]
fn spec0234_ex32_mock_of_unrelated_module_stays_exposed_without_observer_limitation()
-> Result<(), String> {
    let source = test_file(
        "jest.mock(\"../src/unrelated\");",
        "expect(price(150)).toBe(130);",
    );
    let finding = finding_for(&Subject {
        label: "spec0234-ex32",
        owner: FEE_OWNER,
        line: FEE_LINE,
        tests: &[("tests/lib.test.ts", source)],
    })?;
    assert_exposed_with_advisory_mock(&finding);
    Ok(())
}

/// Read per test file: when a second exposing test file runs the real `fee`,
/// not every exposing file mocks the callee, so the class stays `exposed`.
/// When both files mock it, the finding reads `static_unknown`.
#[test]
fn spec0234_rule8_reads_every_exposing_test_file() -> Result<(), String> {
    let mocked = test_file(
        "jest.mock(\"../src/other\");",
        "expect(price(150)).toBe(5);",
    );
    let real = test_file("", "expect(price(150)).toBe(130);");
    let finding = finding_for(&Subject {
        label: "spec0234-rule8-one-real",
        owner: FEE_OWNER,
        line: FEE_LINE,
        tests: &[
            ("tests/mocked.test.ts", mocked.clone()),
            ("tests/real.test.ts", real),
        ],
    })?;
    assert_exposed_with_advisory_mock(&finding);

    let also_mocked = test_file(
        "vi.mock(\"../src/other.ts\");",
        "expect(price(150)).toBe(130);",
    );
    let finding = finding_for(&Subject {
        label: "spec0234-rule8-both-mocked",
        owner: FEE_OWNER,
        line: FEE_LINE,
        tests: &[
            ("tests/mocked.test.ts", mocked),
            ("tests/also.test.ts", also_mocked),
        ],
    })?;
    assert_mocked_static_unknown(&finding);
    Ok(())
}

/// Rule 8 moves only an `exposed` ladder result. An unresolved mock
/// specifier already withholds the owner relation (it may mock the owner's
/// own module, #4294), so the ladder never reaches `exposed` and rule 8 does
/// not apply; its match is pinned in
/// `mock_names_import_module_resolves_each_specifier_from_its_own_file`. A
/// weak ladder result is unchanged and keeps `typescript_mock_only_observer`.
#[test]
fn spec0234_rule8_leaves_non_exposed_ladder_results_unchanged() -> Result<(), String> {
    let unresolved = test_file(
        "const target = \"../src/other\";\njest.mock(target);",
        "expect(price(150)).toBe(5);",
    );
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &unresolved);
    assert_eq!(
        tests.first().map(|test| test.mocks_in_file.clone()),
        Some(vec![UNRESOLVED_MOCK_SPECIFIER.to_string()])
    );
    let finding = finding_for(&Subject {
        label: "spec0234-rule8-unresolved",
        owner: FEE_OWNER,
        line: FEE_LINE,
        tests: &[("tests/lib.test.ts", unresolved)],
    })?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(finding.stop_reasons.is_empty());
    assert_evidence_contains(
        &finding,
        "typescript_limitation: typescript_mock_only_observer",
    );

    let weak = test_file(
        "jest.mock(\"../src/other\");",
        "expect(price(150)).toBeGreaterThan(1);",
    );
    let finding = finding_for(&Subject {
        label: "spec0234-rule8-weak",
        owner: FEE_OWNER,
        line: FEE_LINE,
        tests: &[("tests/lib.test.ts", weak)],
    })?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(finding.stop_reasons.is_empty());
    assert_evidence_contains(
        &finding,
        "typescript_limitation: typescript_mock_only_observer",
    );
    Ok(())
}

/// Mock and import specifiers resolve from their own files: the test's
/// `../src/other` and the owner's `./other` name one module; a sibling
/// module, a package of the same name and a directory `index` are decided by
/// module identity, not text.
#[test]
fn mock_names_import_module_resolves_each_specifier_from_its_own_file() {
    let import = |source: &str| TypeScriptImport {
        source: source.to_string(),
        imported: Some("fee".to_string()),
        local: "fee".to_string(),
        namespace: false,
    };
    let test = Path::new("tests/lib.test.ts");
    let owner = Path::new("src/lib.ts");
    for (mock, source, expected) in [
        ("../src/other", "./other", true),
        ("../src/other.js", "./other", true),
        ("../src/other", "./other/index", true),
        ("../src/unrelated", "./other", false),
        ("../other", "./other", false),
        ("axios", "axios", true),
        ("axios", "./axios", true),
        ("../src/axios", "axios", false),
        ("lodash", "axios", false),
        ("/src/other", "./other", true),
        ("/lib/other", "./other", false),
        (UNRESOLVED_MOCK_SPECIFIER, "./anything", true),
    ] {
        assert_eq!(
            mock_names_import_module(mock, test, &import(source), owner, None, None),
            expected,
            "mock {mock} against owner import {source}"
        );
    }
}
