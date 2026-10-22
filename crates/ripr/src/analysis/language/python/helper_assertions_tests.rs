//! Bounded helper-assertion resolution (#9419): a single-assert helper reads
//! as the inline assertion; every opaque shape stays unresolved.

use super::helper_assertions::apply_helper_assertions;
use super::source_facts::extract_source_facts;
use super::*;
use std::collections::BTreeMap;

const TEST_FILE: &str = "tests/test_sq.py";
const HELPER_FILE: &str = "tests/helpers.py";

fn test_source(call: &str) -> String {
    format!(
        "from pkg.sq import sq\r\nfrom tests.helpers import check_equal\r\n\r\n\r\ndef test_sq():\r\n    {call}\r\n"
    )
}

/// The only test of `test_src`, after helper resolution against `helpers`.
fn resolved(test_src: &str, helpers: &str) -> PythonTest {
    let mut modules = BTreeMap::new();
    let mut tests = Vec::new();
    for (path, source) in [(TEST_FILE, test_src), (HELPER_FILE, helpers)] {
        let facts = extract_source_facts(Path::new(path), source);
        tests.extend(facts.tests);
        modules.insert(PathBuf::from(path), facts.helper_module);
    }
    apply_helper_assertions(&mut tests, &modules);
    assert_eq!(tests.len(), 1, "fixture must parse to one test");
    tests.remove(0)
}

fn only_assertion(test: &PythonTest) -> Result<&PythonAssertion, String> {
    match test.assertions.as_slice() {
        [assertion] => Ok(assertion),
        other => Err(format!("expected one assertion, got {}", other.len())),
    }
}

const EQ_HELPER: &str = "def check_equal(actual, expected):\r\n    assert actual == expected\r\n";

#[test]
fn given_a_single_assert_equality_helper_when_called_then_it_reads_as_the_exact_assertion()
-> Result<(), String> {
    let test = resolved(&test_source("check_equal(sq(3), 9)"), EQ_HELPER);

    let assertion = only_assertion(&test)?;
    assert_eq!(assertion.oracle_shape, PythonOracleShape::ExactAssertion);
    assert_eq!(assertion.oracle_strength, OracleStrength::Strong);
    assert_eq!(assertion.gate_text, "assert sq(3) == 9");
    assert_eq!(assertion.text, "check_equal(sq(3), 9)");
    assert_eq!(assertion.line, 6);
    assert_eq!(
        test.assertion_admission,
        admission::PythonAssertionAdmission::Recognized
    );
    Ok(())
}

#[test]
fn given_a_helper_named_assert_equal_when_called_then_the_body_replaces_the_unknown_reading()
-> Result<(), String> {
    let helper = "def assert_equal(actual, expected):\n    assert actual == expected\n";
    let source = "from pkg.sq import sq\nfrom tests.helpers import assert_equal\n\n\ndef test_sq():\n    assert_equal(sq(3), 9)\n";
    let test = resolved(source, helper);

    let assertion = only_assertion(&test)?;
    assert_eq!(assertion.oracle_shape, PythonOracleShape::ExactAssertion);
    assert_eq!(assertion.gate_text, "assert sq(3) == 9");
    Ok(())
}

#[test]
fn given_a_weak_helper_comparison_when_called_then_the_substituted_form_stays_weak()
-> Result<(), String> {
    let helper = "def check_positive(actual, floor):\n    assert actual > floor\n";
    let source = "from pkg.sq import sq\nfrom tests.helpers import check_positive\n\n\ndef test_sq():\n    check_positive(sq(3), 0)\n";
    let test = resolved(source, helper);

    let assertion = only_assertion(&test)?;
    assert_eq!(assertion.oracle_strength, OracleStrength::Weak);
    assert_eq!(assertion.oracle_shape, PythonOracleShape::BoundaryAssertion);
    Ok(())
}

#[test]
fn given_a_compound_argument_when_substituted_then_it_is_parenthesized() -> Result<(), String> {
    let test = resolved(&test_source("check_equal(sq(3) + 1, 10)"), EQ_HELPER);

    assert_eq!(only_assertion(&test)?.gate_text, "assert (sq(3) + 1) == 10");
    Ok(())
}

#[test]
fn given_opaque_helpers_when_called_then_the_call_stays_unresolved() {
    let opaque = [
        // Extra statement.
        "def check_equal(actual, expected):\n    print(actual)\n    assert actual == expected\n",
        // Branching.
        "def check_equal(actual, expected):\n    if expected:\n        assert actual == expected\n",
        // A free name could be any module global.
        "def check_equal(actual, expected):\n    assert actual == LIMIT\n",
        // A call in the asserted expression.
        "def check_equal(actual, expected):\n    assert abs(actual) == expected\n",
        // Not a comparison.
        "def check_equal(actual, expected):\n    assert actual\n",
        // A default value changes what a short call means.
        "def check_equal(actual, expected=9):\n    assert actual == expected\n",
        // Variadic.
        "def check_equal(*values):\n    assert values\n",
        // Decorated.
        "@wrap\ndef check_equal(actual, expected):\n    assert actual == expected\n",
        // Defined twice.
        "def check_equal(actual, expected):\n    assert actual == expected\n\n\ndef check_equal(actual, expected):\n    pass\n",
        // Rebound.
        "def check_equal(actual, expected):\n    assert actual == expected\n\n\ncheck_equal = print\n",
    ];
    for helper in opaque {
        let test = resolved(&test_source("check_equal(sq(3), 9)"), helper);
        assert!(
            test.assertions.is_empty(),
            "opaque helper must not resolve: {helper}"
        );
        assert_ne!(
            test.assertion_admission,
            admission::PythonAssertionAdmission::Recognized,
            "opaque helper must not admit an assertion: {helper}"
        );
    }
}

#[test]
fn given_a_mismatched_or_shadowed_call_when_resolving_then_it_stays_unresolved() {
    for call in [
        "check_equal(sq(3))",
        "check_equal(sq(3), 9, 1)",
        "check_equal(sq(3), expected=9)",
        "check_equal(*sq(3))",
    ] {
        let test = resolved(&test_source(call), EQ_HELPER);
        assert!(test.assertions.is_empty(), "{call} must not resolve");
    }
    let shadowed = test_source("check_equal = print\r\n    check_equal(sq(3), 9)");
    let test = resolved(&shadowed, EQ_HELPER);
    assert!(
        test.assertions.is_empty(),
        "a locally rebound name is not the helper"
    );
    let imported_twice = test_source("check_equal(sq(3), 9)").replace(
        "\r\n\r\n\r\ndef",
        "\r\nfrom tests.other import check_equal\r\n\r\n\r\ndef",
    );
    let test = resolved(&imported_twice, EQ_HELPER);
    assert!(
        test.assertions.is_empty(),
        "two imports of one alias leave the bound target ambiguous"
    );
    let redefined = format!(
        "{}\r\n\r\ndef check_equal(a, b):\r\n    pass\r\n",
        test_source("check_equal(sq(3), 9)")
    );
    let test = resolved(&redefined, EQ_HELPER);
    assert!(
        test.assertions.is_empty(),
        "a test module that also defines the name is ambiguous"
    );
}

#[test]
fn given_a_helper_in_the_test_module_when_called_then_it_resolves() -> Result<(), String> {
    let source = "from pkg.sq import sq\n\n\ndef check_equal(actual, expected):\n    assert actual == expected\n\n\ndef test_sq():\n    check_equal(sq(3), 9)\n";
    let test = resolved(source, "");

    assert_eq!(only_assertion(&test)?.gate_text, "assert sq(3) == 9");
    Ok(())
}
