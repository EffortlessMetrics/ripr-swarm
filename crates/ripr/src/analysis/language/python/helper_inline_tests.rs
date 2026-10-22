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
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "an input away from the boundary reads weakly exposed, never exposed: {:?}",
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

fn body_of(source: &str) -> String {
    extract_tests(Path::new("tests/test_core.py"), source)
        .iter()
        .find(|test| test.name == "test_pass_mark")
        .map(|test| test.body_text.clone())
        .unwrap_or_default()
}

#[test]
fn given_a_conditional_or_nested_redefinition_then_it_stays_unknown() {
    let plain = "def check_pass(score, want):\n    assert passes(score) == want\n\n\n";
    let conditional = format!(
        "{plain}if False:\n    def check_pass(score, want):\n        assert passes(score) != want\n\n\n"
    );
    assert_eq!(
        assertion_count(&test_source(&conditional, "    check_pass(50, True)\n")),
        0
    );
    assert_eq!(
        assertion_count(&test_source(plain, "    check_pass(50, True)\n")),
        1
    );
}

#[test]
fn given_a_negative_literal_then_it_is_parenthesised() {
    let helper = "def check_pass(score, want):\n    assert passes(score ** 2) == want\n\n\n";
    let source = test_source(helper, "    check_pass(-2, True)\n");
    assert_eq!(assertion_count(&source), 1);
    assert!(body_of(&source).contains("passes((-2) ** 2)"), "{source}");
}

#[test]
fn given_a_non_ascii_literal_then_it_is_copied_whole() {
    let helper = "def check_pass(score, want):\n    assert passes(score) == want, 'caf\u{e9} \u{2713}'\n\n\n";
    let source = test_source(helper, "    check_pass(50, True)\n");
    assert_eq!(assertion_count(&source), 1);
    assert!(
        body_of(&source).contains("'caf\u{e9} \u{2713}'"),
        "{source}"
    );
}

#[test]
fn given_a_non_ascii_parameter_name_then_it_is_substituted_whole() {
    let helper = "def check_pass(scor\u{e9}, want):\n    assert passes(scor\u{e9}) == want\n\n\n";
    let source = test_source(helper, "    check_pass(50, True)\n");
    assert!(body_of(&source).contains("passes(50)"), "{source}");
}

#[test]
fn given_a_numeric_argument_used_as_a_member_object_then_it_stays_unknown() {
    let helper =
        "def check_pass(score, want):\n    assert passes(score.bit_length()) == want\n\n\n";
    let source = test_source(helper, "    check_pass(5, True)\n");
    assert_eq!(assertion_count(&source), 0);
    assert!(!body_of(&source).contains("5.bit_length"), "{source}");
}

#[test]
fn given_a_walrus_in_the_test_body_then_it_stays_unknown() {
    let source = test_source(
        HELPER,
        "    (passes := lambda s: True)\n    check_pass(50, True)\n",
    );
    assert_eq!(assertion_count(&source), 0);
}

#[test]
fn given_the_literals_then_the_rewrite_is_exactly_the_substituted_asserts() {
    let source = test_source(
        HELPER,
        "    check_pass(50, True)\n    check_pass(49, False)\n",
    );
    let body = body_of(&source);
    assert!(
        body.ends_with("    assert passes(50) == True, 50\n    assert passes(49) == False, 49"),
        "{body}"
    );
}

#[test]
fn given_dynamic_name_binding_in_the_file_then_nothing_is_inlined() {
    for line in [
        "NAMES = globals()\n",
        "setattr(object(), 'x', 1)\n",
        "exec('x = 1')\n",
        "VALUE = eval('1')\n",
        "from os.path import *\n",
    ] {
        let source = format!(
            "{line}{}",
            test_source(HELPER, "    check_pass(50, True)\n")
        );
        assert_eq!(assertion_count(&source), 0, "{line}");
    }
}

#[test]
fn given_a_helper_that_is_not_a_plain_function_then_it_stays_unknown() {
    let cases = [
        "@decorate\ndef check_pass(score, want):\n    assert passes(score) == want\n\n\n",
        "async def check_pass(score, want):\n    assert passes(score) == want\n\n\n",
        "def check_pass(*scores):\n    assert passes(1) == scores\n\n\n",
        "def check_pass(score, **extra):\n    assert passes(score) == extra\n\n\n",
        "def check_pass(score, *, want):\n    assert passes(score) == want\n\n\n",
        "def check_pass(score, want):\n    \"doc\"\n    print(score)\n\n\n",
    ];
    for helper in cases {
        let source = test_source(helper, "    check_pass(50, True)\n");
        assert_eq!(assertion_count(&source), 0, "{helper}");
    }
}

#[test]
fn given_a_call_that_does_not_own_its_lines_then_it_stays_unknown() {
    for body in [
        "    check_pass(50, True); check_pass(49, False)\n",
        "    check_pass(50, True)  # pin\n",
        "    check_pass(\n        50, True)\n",
    ] {
        let source = test_source(HELPER, body);
        assert_eq!(assertion_count(&source), 0, "{body}");
    }
}

#[test]
fn given_an_assert_the_substitution_cannot_prove_then_it_stays_unknown() {
    for assertion in [
        "assert (lambda s: s)(score) == want",
        "assert passes(score) == [want]",
        "assert passes(score) == {want}",
        "assert passes(score) == want, 'a\\nb'",
        "assert (flag := passes(score)) == want",
        "assert all(passes(score) for _ in want)",
    ] {
        let helper = format!("def check_pass(score, want):\n    {assertion}\n\n\n");
        let source = test_source(&helper, "    check_pass(50, True)\n");
        assert_eq!(assertion_count(&source), 0, "{assertion}");
    }
}

#[test]
fn given_a_test_with_a_non_expression_statement_then_it_stays_unknown() {
    for first in [
        "x = 1",
        "import os",
        "with open('f'):\n        pass",
        "del x",
    ] {
        let source = test_source(HELPER, &format!("    {first}\n    check_pass(50, True)\n"));
        assert_eq!(assertion_count(&source), 0, "{first}");
    }
}
