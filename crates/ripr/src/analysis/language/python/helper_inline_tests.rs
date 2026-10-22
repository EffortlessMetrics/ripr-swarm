//! A plain test's call to a same-file assertion helper reads as the helper's
//! asserts, credit-or-unknown only (#9636).

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_PY: &str = "def passes(score):\n    return 50 <= score\n";
const IMPORT: &str = "from src.core import passes\n\n\n";
const HELPER: &str = "def check_pass(score, want):\n    assert passes(score) == want, score\n\n\n";

fn finding_for(tests_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/core.py"), OWNER_PY);
    let tests = extract_tests(Path::new("tests/test_core.py"), tests_source);
    classify_change(
        Path::new("src/core.py"),
        2,
        "    return 50 <= score",
        &owners,
        &tests,
    )
    .ok_or_else(|| "expected a finding for the changed line".to_string())
}

fn test_source(helper: &str, body: &str) -> String {
    format!("{IMPORT}{helper}def test_pass_mark():\n{body}")
}

fn assertion_count(source: &str) -> usize {
    extract_tests(Path::new("tests/test_core.py"), source)
        .iter()
        .find(|test| test.name == "test_pass_mark")
        .map_or(0, |test| test.assertions.len())
}

#[test]
fn given_a_same_file_helper_called_with_literals_then_the_test_is_credited() -> Result<(), String> {
    let source = test_source(
        HELPER,
        "    check_pass(50, True)\n    check_pass(49, False)\n",
    );
    assert_eq!(assertion_count(&source), 2);
    let finding = finding_for(&source)?;
    assert!(
        matches!(
            finding.class,
            ExposureClass::Exposed | ExposureClass::WeaklyExposed
        ),
        "{:?} {:?}",
        finding.class,
        finding.evidence
    );
    Ok(())
}

#[test]
fn given_inlining_then_every_line_number_is_unchanged() -> Result<(), String> {
    let source = test_source(
        HELPER,
        "    check_pass(50, True)\n    check_pass(49, False)\n",
    );
    let lines: Vec<&str> = source.lines().collect();
    let tests = extract_tests(Path::new("tests/test_core.py"), &source);
    let test = tests
        .iter()
        .find(|test| test.name == "test_pass_mark")
        .ok_or_else(|| "test_pass_mark is extracted".to_string())?;
    assert_eq!(lines.get(test.line - 1), Some(&"def test_pass_mark():"));
    assert_eq!(test.body_text.lines().count(), 3);
    Ok(())
}

/// Negative controls: each shape leaves the helper call unread, so the test
/// keeps its pre-change reading (no assertion, no related test).
#[test]
fn given_a_shape_that_is_not_provably_straight_line_then_it_stays_unknown() {
    let plain_helper = "def check_pass(score, want):\n    assert passes(score) == want\n\n\n";
    let cases: [(&str, String, &str); 12] = [
        (
            "conditional call",
            HELPER.to_string(),
            "    if True:\n        check_pass(50, True)\n",
        ),
        (
            "call inside a loop",
            HELPER.to_string(),
            "    for _ in range(1):\n        check_pass(50, True)\n",
        ),
        (
            "non-literal argument",
            HELPER.to_string(),
            "    n = 50\n    check_pass(n, True)\n",
        ),
        (
            "name argument",
            HELPER.to_string(),
            "    check_pass(passes, True)\n",
        ),
        (
            "keyword argument",
            HELPER.to_string(),
            "    check_pass(score=50, want=True)\n",
        ),
        (
            "argument count mismatch",
            HELPER.to_string(),
            "    check_pass(50)\n",
        ),
        (
            "helper with a branch",
            "def check_pass(score, want):\n    if score:\n        assert passes(score) == want\n\n\n"
                .to_string(),
            "    check_pass(50, True)\n",
        ),
        (
            "helper with a default",
            "def check_pass(score, want=True):\n    assert passes(score) == want\n\n\n".to_string(),
            "    check_pass(50, True)\n",
        ),
        (
            "helper passed as a value",
            format!("{plain_helper}alias = check_pass\n\n\n"),
            "    check_pass(50, True)\n",
        ),
        (
            "helper defined twice",
            format!(
                "{plain_helper}def check_pass(score, want):\n    assert passes(score) != want\n\n\n"
            ),
            "    check_pass(50, True)\n",
        ),
        (
            "parameter used as an attribute",
            "def check_pass(score, want):\n    assert passes(1) == want.score\n\n\n".to_string(),
            "    check_pass(50, True)\n",
        ),
        (
            "parameter named inside a string",
            "def check_pass(score, want):\n    assert passes(50) == want, 'score'\n\n\n"
                .to_string(),
            "    check_pass(50, True)\n",
        ),
    ];
    for (label, helper, body) in cases {
        let source = test_source(&helper, body);
        assert_eq!(assertion_count(&source), 0, "{label}: {source}");
    }
}

#[test]
fn given_a_helper_in_another_file_then_nothing_is_inlined() -> Result<(), String> {
    let source = "from src.core import passes\nfrom helpers import check_pass\n\n\ndef test_pass_mark():\n    check_pass(50, True)\n";
    assert_eq!(assertion_count(source), 0);
    assert_eq!(finding_for(source)?.class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn given_a_test_with_a_local_or_parameter_then_no_shadowing_is_risked() {
    let with_local = test_source(
        HELPER,
        "    passes = lambda s: True\n    check_pass(50, True)\n",
    );
    assert_eq!(assertion_count(&with_local), 0);
    let with_parameter =
        format!("{IMPORT}{HELPER}def test_pass_mark(passes):\n    check_pass(50, True)\n");
    assert_eq!(assertion_count(&with_parameter), 0);
}

#[test]
fn given_a_helper_called_away_from_the_boundary_then_it_is_not_exposed() -> Result<(), String> {
    let source = test_source(HELPER, "    check_pass(60, True)\n");
    let finding = finding_for(&source)?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "an input away from the boundary must not read exposed: {:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn given_a_file_that_is_not_a_test_file_then_nothing_is_inlined() {
    let source = test_source(HELPER, "    check_pass(50, True)\n");
    let tests = extract_tests(Path::new("src/helpers.py"), &source);
    let count = tests
        .iter()
        .find(|test| test.name == "test_pass_mark")
        .map_or(0, |test| test.assertions.len());
    assert_eq!(count, 0);
}
