//! Tests for crediting a bare boolean assertion on a `bool` owner (#9542).

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const RULES: &str = "src/rules.py";
const TESTS_PATH: &str = "tests/test_rules.py";

/// Run the pass with each file's own source, as the workspace pass does.
fn credit(
    tests: &mut [PythonTest],
    owners: &[PythonOwner],
    owner_source: &str,
    tests_source: &str,
) {
    bool_oracle::credit_bare_bool_asserts(tests, owners, |path| {
        Some(if path == Path::new(TESTS_PATH) {
            tests_source
        } else {
            owner_source
        })
    });
}

fn credited_strengths(owner_source: &str, tests_source: &str) -> Vec<OracleStrength> {
    let owners = extract_owners(Path::new(RULES), owner_source);
    let mut tests = extract_tests(Path::new("tests/test_rules.py"), tests_source);
    credit(&mut tests, &owners, owner_source, tests_source);
    tests
        .iter()
        .flat_map(|test| test.assertions.iter())
        .map(|assertion| assertion.oracle_strength.clone())
        .collect()
}

fn is_adult_finding(tests_source: &str) -> Result<Finding, String> {
    let owner_source = "def is_adult(age):\n    return 18 <= age\n";
    let owners = extract_owners(Path::new(RULES), owner_source);
    let mut tests = extract_tests(Path::new("tests/test_rules.py"), tests_source);
    credit(&mut tests, &owners, owner_source, tests_source);
    classify_change(Path::new(RULES), 2, "    return 18 <= age", &owners, &tests)
        .ok_or_else(|| "expected a finding for the changed predicate".to_string())
}

const ADULT_TEST: &str = "from src.rules import is_adult\n\n\ndef test_adult():\n    assert is_adult(18)\n    assert not is_adult(17)\n";

#[test]
fn given_literal_bare_asserts_on_a_comparison_owner_then_both_are_exact_pins() {
    let strengths = credited_strengths("def is_adult(age):\n    return age >= 18\n", ADULT_TEST);

    assert_eq!(strengths, vec![OracleStrength::Strong; 2]);
}

#[test]
fn given_a_credited_bare_assert_then_the_gate_reads_the_equivalent_comparison() -> Result<(), String>
{
    let owner_source = "def is_adult(age):\n    return age >= 18\n";
    let owners = extract_owners(Path::new(RULES), owner_source);
    let mut tests = extract_tests(Path::new("tests/test_rules.py"), ADULT_TEST);
    credit(&mut tests, &owners, owner_source, ADULT_TEST);

    let gates: Vec<_> = tests[0]
        .assertions
        .iter()
        .map(|assertion| assertion.gate_text.as_str())
        .collect();
    assert_eq!(
        gates,
        vec![
            "assert is_adult(18) == True",
            "assert is_adult(17) == False"
        ]
    );
    assert_eq!(tests[0].assertions[0].text, "assert is_adult(18)");
    Ok(())
}

#[test]
fn given_a_credited_boundary_pin_then_the_changed_predicate_is_exposed() -> Result<(), String> {
    let finding = is_adult_finding(ADULT_TEST)?;

    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn given_a_bare_assert_pinning_only_one_side_of_the_boundary_then_it_is_still_a_pin()
-> Result<(), String> {
    let finding = is_adult_finding(
        "from src.rules import is_adult\n\n\ndef test_adult():\n    assert is_adult(18)\n",
    )?;

    assert_eq!(
        finding.related_tests[0].oracle_strength,
        OracleStrength::Strong
    );
    Ok(())
}

#[test]
fn given_a_non_literal_argument_then_the_assertion_stays_smoke() {
    let strengths = credited_strengths(
        "def is_adult(age):\n    return age >= 18\n",
        "from src.rules import is_adult\n\n\ndef test_monotonic():\n    for age in range(0, 120):\n        assert is_adult(age + 1), age\n    assert not is_adult(age)\n",
    );

    assert_eq!(strengths, vec![OracleStrength::Smoke; 2]);
}

#[test]
fn given_an_owner_that_may_not_return_a_bool_then_the_assertion_stays_smoke() {
    for owner_source in [
        "def is_adult(age):\n    return age * 2\n",
        "def is_adult(age):\n    return age and 18\n",
        "def is_adult(age):\n    if age >= 18:\n        return True\n    return False\n",
        "def is_adult(age):\n    return lookup(age)\n",
    ] {
        let strengths = credited_strengths(owner_source, ADULT_TEST);
        assert_eq!(strengths, vec![OracleStrength::Smoke; 2], "{owner_source}");
    }
}

#[test]
fn given_a_bool_annotation_then_the_owner_is_a_bool_owner() {
    let strengths = credited_strengths(
        "def is_adult(age) -> bool:\n    return check(age)\n",
        ADULT_TEST,
    );

    assert_eq!(strengths, vec![OracleStrength::Strong; 2]);
}

#[test]
fn given_a_boolean_operator_then_only_bool_literal_operands_are_credited() {
    let owner_source = "def can_ship(paid, in_stock):\n    return paid and in_stock\n";
    let credited = credited_strengths(
        owner_source,
        "from src.rules import can_ship\n\n\ndef test_ship():\n    assert can_ship(True, True)\n    assert not can_ship(False, True)\n",
    );
    let truthy_argument = credited_strengths(
        owner_source,
        "from src.rules import can_ship\n\n\ndef test_ship():\n    assert can_ship(1, True)\n    assert not can_ship(0, True)\n",
    );

    assert_eq!(credited, vec![OracleStrength::Strong; 2]);
    assert_eq!(truthy_argument, vec![OracleStrength::Smoke; 2]);
}

#[test]
fn given_an_owner_that_cannot_be_resolved_uniquely_then_the_assertion_stays_smoke() {
    for owner_source in [
        "def is_adult(age):\n    return age >= 18\n\n\ndef is_adult(age):\n    return age >= 21\n",
        "@cache\ndef is_adult(age):\n    return age >= 18\n",
        "class Person:\n    def is_adult(self, age):\n        return age >= 18\n",
        "def is_adult(age):\n    return age >= 18\n\n\nis_adult = wrap(is_adult)\n",
        "async def is_adult(age):\n    return age >= 18\n",
        "def is_adult(*ages):\n    return ages[0] >= 18\n",
    ] {
        let strengths = credited_strengths(owner_source, ADULT_TEST);
        assert_eq!(strengths, vec![OracleStrength::Smoke; 2], "{owner_source}");
    }
}

#[test]
fn given_an_unrelated_callee_then_the_assertion_stays_smoke() {
    let strengths = credited_strengths(
        "def is_adult(age):\n    return age >= 18\n",
        "from other import is_adult\n\n\ndef test_adult():\n    assert is_adult(18)\n",
    );

    assert_eq!(strengths, vec![OracleStrength::Smoke]);
}

const BOOL_OWNER: &str = "def is_adult(age):\n    return age >= 18\n";

fn strengths_with_owner(owner_source: &str) -> Vec<OracleStrength> {
    credited_strengths(owner_source, ADULT_TEST)
}

#[test]
fn given_another_binding_of_the_owner_name_anywhere_in_its_module_then_the_assertion_stays_smoke() {
    for rebinding in [
        "if True:\n    def is_adult(age):\n        return 1\n",
        "try:\n    from fast import is_adult\nexcept ImportError:\n    pass\n",
        "from legacy import is_adult\n",
        "is_adult += 1\n",
        "del is_adult\n",
        "for is_adult in range(2):\n    pass\n",
        "with open('x') as is_adult:\n    pass\n",
    ] {
        let strengths = strengths_with_owner(&format!("{BOOL_OWNER}{rebinding}"));
        assert_eq!(strengths, vec![OracleStrength::Smoke; 2], "{rebinding}");
    }
}

#[test]
fn given_a_generator_annotated_bool_then_the_assertion_stays_smoke() {
    let strengths = strengths_with_owner("def is_adult(age) -> bool:\n    yield age >= 18\n");

    assert_eq!(strengths, vec![OracleStrength::Smoke; 2]);
}

#[test]
fn given_a_test_that_patches_or_rebinds_the_callee_then_the_assertion_stays_smoke() {
    for tests in [
        "from unittest import mock\nfrom src.rules import is_adult\n\n\n@mock.patch('src.rules.is_adult')\ndef test_adult(m):\n    assert is_adult(18)\n",
        "from src.rules import is_adult\n\n\ndef test_adult(monkeypatch):\n    monkeypatch.setattr('src.rules.is_adult', None)\n    assert is_adult(18)\n",
        "from src.rules import is_adult\n\n\ndef test_adult():\n    is_adult = lambda a: 1\n    assert is_adult(18)\n",
    ] {
        let strengths = credited_strengths(BOOL_OWNER, tests);
        assert_eq!(strengths, vec![OracleStrength::Smoke], "{tests}");
    }
}

#[test]
fn given_keyword_dotted_or_nested_arguments_then_the_assertion_stays_smoke() {
    for call in [
        "is_adult(age=18)",
        "rules.is_adult(18)",
        "is_adult(int('18'))",
    ] {
        let tests = format!(
            "from src import rules\nfrom src.rules import is_adult\n\n\ndef test_adult():\n    assert {call}\n"
        );
        let strengths = credited_strengths(BOOL_OWNER, &tests);
        assert_eq!(strengths, vec![OracleStrength::Smoke], "{call}");
    }
}

#[test]
fn given_an_off_boundary_literal_then_the_credited_pin_does_not_expose_the_boundary_change()
-> Result<(), String> {
    let finding = is_adult_finding(
        "from src.rules import is_adult\n\n\ndef test_adult():\n    assert is_adult(100)\n",
    )?;

    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn given_not_and_literal_returns_and_a_docstring_then_the_owner_is_a_bool_owner() {
    for owner in [
        "def is_adult(age):\n    return not age < 18\n",
        "def is_adult(age):\n    return True\n",
        "def is_adult(age):\n    \"\"\"Adults.\"\"\"\n    return age >= 18\n",
        "def is_adult(age):\n    return age >= 18 or age == 17\n",
    ] {
        assert_eq!(
            strengths_with_owner(owner),
            vec![OracleStrength::Strong; 2],
            "{owner}"
        );
    }
}
