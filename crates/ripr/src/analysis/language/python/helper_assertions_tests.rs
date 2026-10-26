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

const FIELD_OWNER: &str = "class Stats:\n    def __init__(self):\n        self.writes = 0\n        self.hits = 0\n\n    def record_write(self):\n        self.writes += 1\n";
const FIELD_HELPER: &str = "def check_writes(obj, want):\n    assert obj.writes == want\n";

fn classified_helper_field(
    owner_source: &str,
    test_source: &str,
    helper_source: &str,
) -> Result<crate::domain::Finding, String> {
    classified_helper_change(
        owner_source,
        test_source,
        helper_source,
        "        self.writes += 1",
        "        self.writes = self.writes + 1",
    )
}

fn classified_helper_change(
    owner_source: &str,
    test_source: &str,
    helper_source: &str,
    new_line: &str,
    old_line: &str,
) -> Result<crate::domain::Finding, String> {
    let facts = extract_source_facts(Path::new("src/stats.py"), owner_source);
    assert!(
        facts
            .owners
            .iter()
            .any(|owner| owner.name == "record_write")
    );
    assert_eq!(owner_source.lines().nth(6), Some(new_line));
    // Resolution must run at its workspace owner before classification. The
    // extract_tests-only wrapper used before #9711 would miss that pass.
    let test = resolved(test_source, helper_source);
    assert!(
        test.assertions
            .iter()
            .any(|assertion| { assertion.oracle_strength == OracleStrength::Strong }),
        "the control must reach strong helper-derived alignment"
    );
    super::classify::classify_change_with_old(
        Path::new("src/stats.py"),
        7,
        new_line,
        Some(old_line),
        &facts.owners,
        &[test],
    )
    .ok_or_else(|| "the changed owner write must produce a finding".to_string())
}

fn field_test_source(helper: &str, statements: &str) -> String {
    format!(
        "from src.stats import Stats\n\n{helper}\ndef drop(pending):\n    pending.close()\n\ndef test_writes():\n    stats = Stats()\n    {statements}\n"
    )
}

#[test]
fn given_a_helper_when_owner_body_is_deferred_then_no_changed_effect_is_credited()
-> Result<(), String> {
    let bodies = [
        FIELD_OWNER.replace("def record_write(", "async def record_write("),
        format!("{FIELD_OWNER}        yield None\n"),
        format!("{FIELD_OWNER}        f\"{{(yield None)}}\"\n"),
    ];
    for owner in bodies {
        for (in_test, in_module) in [
            (FIELD_HELPER, ""),
            ("from tests.helpers import check_writes\n", FIELD_HELPER),
        ] {
            let source = field_test_source(
                in_test,
                "drop(stats.record_write())\n    check_writes(stats, 0)",
            );
            assert_eq!(
                classified_helper_field(&owner, &source, in_module)?.class,
                crate::domain::ExposureClass::WeaklyExposed,
                "closing a deferred object must not credit its body: {owner}"
            );
        }
    }
    Ok(())
}

#[test]
fn given_a_helper_when_deferred_owner_is_only_a_method_token_then_no_branch_restores_credit()
-> Result<(), String> {
    let helper = "def check_writes(obj, want):\n    assert obj.record_write == want\n";
    for owner in [
        FIELD_OWNER.replace("def record_write(", "async def record_write("),
        format!("{FIELD_OWNER}        yield None\n"),
        format!("{FIELD_OWNER}        f\"{{(yield None)}}\"\n"),
    ] {
        for (in_test, in_module) in [
            (helper, ""),
            ("from tests.helpers import check_writes\n", helper),
        ] {
            let source = field_test_source(
                in_test,
                "drop(stats.record_write())\n    method = stats.record_write\n    check_writes(stats, method)",
            );
            assert_eq!(
                classified_helper_field(&owner, &source, in_module)?.class,
                crate::domain::ExposureClass::WeaklyExposed,
                "a method-name credit branch must not bypass refusal: {owner}"
            );
        }
    }
    Ok(())
}

#[test]
fn given_a_synchronous_field_helper_when_body_runs_then_changed_effect_stays_credited()
-> Result<(), String> {
    for (in_test, in_module) in [
        (FIELD_HELPER, ""),
        ("from tests.helpers import check_writes\n", FIELD_HELPER),
    ] {
        let source = field_test_source(
            in_test,
            "stats.record_write()\n    stats.record_write()\n    check_writes(stats, 2)",
        );
        assert_eq!(
            classified_helper_field(FIELD_OWNER, &source, in_module)?.class,
            crate::domain::ExposureClass::Exposed
        );
    }
    Ok(())
}

#[test]
fn given_a_deferred_helper_when_a_write_is_unmodeled_then_method_credit_still_refuses()
-> Result<(), String> {
    let helper = "def check_writes(obj, want):\n    assert obj.record_write == want\n";
    for (new_line, old_line) in [
        (
            "        self.writes: int = 1",
            "        self.writes: int = 0",
        ),
        (
            "        self.writes[0] += 1",
            "        self.writes[0] = self.writes[0] + 1",
        ),
    ] {
        let base = FIELD_OWNER
            .replace("        self.writes += 1", new_line)
            .replace("self.writes = 0", "self.writes = [0]");
        for owner in [
            base.replace("def record_write(", "async def record_write("),
            format!("{base}        yield None\n"),
            format!("{base}        f\"{{(yield None)}}\"\n"),
        ] {
            let source = field_test_source(
                helper,
                "drop(stats.record_write())\n    method = stats.record_write\n    check_writes(stats, method)",
            );
            assert_eq!(
                classified_helper_change(&owner, &source, "", new_line, old_line)?.class,
                crate::domain::ExposureClass::WeaklyExposed,
                "unmodeled write must not bypass the helper refusal: {owner}"
            );
        }
    }
    Ok(())
}

#[test]
fn given_a_function_result_helper_when_substituted_then_normal_owner_credit_is_preserved()
-> Result<(), String> {
    let source = "def sq(x):\n    return x * x\n";
    let facts = extract_source_facts(Path::new("pkg/sq.py"), source);
    assert!(facts.owners.iter().any(|owner| owner.name == "sq"));
    let test = resolved(&test_source("check_equal(sq(3), 9)"), EQ_HELPER);
    let finding = super::classify::classify_change_with_old(
        Path::new("pkg/sq.py"),
        2,
        "    return x * x",
        Some("    return x + x"),
        &facts.owners,
        &[test],
    )
    .ok_or("the changed function result must produce a finding")?;
    assert_eq!(finding.class, crate::domain::ExposureClass::Exposed);
    Ok(())
}
