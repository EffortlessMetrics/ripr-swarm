//! Operator and `with` entries of dunder owners (#9421): the syntax binds to
//! the owner only where the receiver's class and the operator are certain.

use super::classify::classify_change_with_old;
use super::owners_tests::{extract_owners, extract_tests};
use super::related_tests::{apply_class_facts, related_test_candidates};
use super::{PythonOwner, PythonTest};
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

type Check = Result<(), String>;

const MONEY: &str = "class Money:\n    def __init__(self, cents):\n        self.cents = cents\n\n    def __add__(self, other):\n        return Money(self.cents + other.cents)\n\n    def __eq__(self, other):\n        return isinstance(other, Money) and self.cents == other.cents\n\n    def __lt__(self, other):\n        return self.cents < other.cents\n";

const RECORDER: &str = "class Recorder:\n    def __init__(self):\n        self.events = []\n\n    def __enter__(self):\n        self.events.append(\"enter\")\n        return self\n\n    def __exit__(self, *exc):\n        self.events.append(\"exit\")\n        return False\n";

const MONEY_IMPORT: &str = "from shapes.money import Money\n\n\n";

fn owners_of(src: &str, file: &str) -> Vec<PythonOwner> {
    let mut owners = extract_owners(Path::new(file), src);
    apply_class_facts(&mut owners);
    owners
}

fn owner<'a>(owners: &'a [PythonOwner], name: &str) -> Result<&'a PythonOwner, String> {
    owners
        .iter()
        .find(|owner| owner.name == name)
        .ok_or_else(|| format!("owner {name} must be extracted"))
}

fn tests_of(src: &str) -> Result<Vec<PythonTest>, String> {
    let tests = extract_tests(Path::new("tests/test_money.py"), src);
    if tests.len() == 1 {
        Ok(tests)
    } else {
        Err("fixture must parse exactly one test".to_string())
    }
}

fn relations(
    owners: &[PythonOwner],
    name: &str,
    test_src: &str,
) -> Result<Vec<&'static str>, String> {
    let tests = tests_of(test_src)?;
    Ok(related_test_candidates(owner(owners, name)?, &tests)
        .iter()
        .map(|candidate| candidate.relation.as_str())
        .collect())
}

fn relation(
    owners: &[PythonOwner],
    name: &str,
    test_src: &str,
) -> Result<Option<&'static str>, String> {
    Ok(relations(owners, name, test_src)?.first().copied())
}

fn classify(
    src: &str,
    file: &str,
    name: &str,
    line_text: &str,
    old: &str,
    test_src: &str,
) -> Result<Finding, String> {
    let owners = owners_of(src, file);
    let line = owner(&owners, name)?.start_line + 1;
    let tests = tests_of(test_src)?;
    classify_change_with_old(Path::new(file), line, line_text, Some(old), &owners, &tests)
        .ok_or_else(|| "the change must yield a finding".to_string())
}

fn classify_add(test_src: &str) -> Result<Finding, String> {
    classify(
        MONEY,
        "shapes/money.py",
        "__add__",
        "        return Money(self.cents - other.cents)",
        "        return Money(self.cents + other.cents)",
        test_src,
    )
}

fn money_test(body: &str) -> String {
    format!("{MONEY_IMPORT}def test_it():\n{body}\n")
}

#[test]
fn operator_on_constructed_operands_credits_the_exact_comparison() -> Check {
    let finding = classify_add(&money_test("    assert Money(5) + Money(7) == Money(12)"))?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn operator_on_locals_bound_once_to_the_class_credits_the_attribute_read() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    let test_src = money_test("    a = Money(5)\n    b = Money(7)\n    assert (a + b).cents == 12");
    assert_eq!(
        relation(&owners, "__add__", &test_src)?,
        Some("protocol_entry_call")
    );
    assert_eq!(classify_add(&test_src)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn operator_result_masked_by_a_weak_assertion_is_not_credited() -> Check {
    for body in [
        "    assert (Money(5) + Money(7)).cents >= 0",
        "    assert (Money(5) + Money(7)).cents",
        "    assert Money(5) + Money(7)",
    ] {
        let finding = classify_add(&money_test(body))?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn operator_that_is_not_certain_keeps_the_dunder_protocol_relation() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    let uncertain = [
        // The receiver is the right operand: Python runs `__radd__`.
        "    assert 5 + Money(7) == Money(12)",
        // The other operand's type is unknown and may be a subclass.
        "    assert Money(5) + other() == Money(12)",
        // A parameter-like name is not a bound instance.
        "    assert a + b == Money(12)",
        // The operator is part of a larger expression.
        "    assert Money(1) * 2 + Money(1) == Money(3)",
        "    assert Money(1) + Money(1) + Money(1) == Money(3)",
        // A wrapper may ignore the result.
        "    assert always_true(Money(5) + Money(7))",
        // The class is not the imported one.
        "    assert Other(5) + Other(7) == Money(12)",
        // Attribute access on a call result is not a grouping read.
        "    assert (Money(5) + Money(7)).scaled(2) == 24",
    ];
    for body in uncertain {
        assert_eq!(
            relation(&owners, "__add__", &money_test(body))?,
            Some("dunder_protocol"),
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn operator_on_a_rebound_or_multiply_bound_local_is_not_bound() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    for body in [
        "    a = Money(5)\n    a = other\n    assert a + Money(7) == Money(12)",
        "    a = Money(5)\n    b = Money(7)\n    b = Money(8)\n    assert a + b == Money(13)",
        "    a, b = Money(5), Money(7)\n    assert a + b == Money(12)",
        "    for a in [Money(5)]:\n        assert a + Money(7) == Money(12)",
    ] {
        assert_eq!(
            relation(&owners, "__add__", &money_test(body))?,
            Some("dunder_protocol"),
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn subclass_operand_is_not_bound() -> Check {
    let owners = owners_of(MONEY, "shapes/money.py");
    let test_src = format!(
        "{MONEY_IMPORT}class Sub(Money):\n    def __radd__(self, other):\n        return 0\n\n\ndef test_it():\n    assert Money(5) + Sub(7) == Money(12)\n"
    );
    let tests = extract_tests(Path::new("tests/test_money.py"), &test_src);
    let found: Vec<_> = related_test_candidates(owner(&owners, "__add__")?, &tests)
        .iter()
        .map(|candidate| candidate.relation.as_str())
        .collect();
    assert!(
        found.iter().all(|kind| *kind != "protocol_entry_call"),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn not_equal_binds_the_eq_owner_only_when_the_class_defines_no_ne() -> Check {
    let with_ne =
        format!("{MONEY}\n    def __ne__(self, other):\n        return not self == other\n");
    let test_src = money_test("    assert Money(1) != Money(2)");
    let plain = owners_of(MONEY, "shapes/money.py");
    assert_eq!(
        relation(&plain, "__eq__", &test_src)?,
        Some("protocol_entry_call")
    );
    let overridden = owners_of(&with_ne, "shapes/money.py");
    assert_eq!(
        relation(&overridden, "__eq__", &test_src)?,
        Some("dunder_protocol")
    );
    assert_eq!(
        relation(&overridden, "__ne__", &test_src)?,
        Some("protocol_entry_call")
    );
    // Without the workspace pass the class's methods are unknown: fail closed.
    let unfilled = extract_owners(Path::new("shapes/money.py"), MONEY);
    assert_eq!(
        relation(&unfilled, "__eq__", &test_src)?,
        Some("dunder_protocol")
    );
    Ok(())
}

#[test]
fn weak_equality_read_of_an_eq_owner_abstains_instead_of_grading_a_gap() -> Check {
    // `Money(1) != 1` cannot tell a removed `isinstance` guard from the
    // original through the weak oracle, yet the guard's removal raises.
    let finding = classify(
        MONEY,
        "shapes/money.py",
        "__eq__",
        "        return self.cents == other.cents",
        "        return isinstance(other, Money) and self.cents == other.cents",
        &money_test("    assert Money(1) != 1"),
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    Ok(())
}

#[test]
fn ordering_dunder_with_a_relational_assertion_is_a_gap_not_an_abstention() -> Check {
    let finding = classify(
        MONEY,
        "shapes/money.py",
        "__lt__",
        "        return self.cents <= other.cents",
        "        return self.cents < other.cents",
        &money_test("    assert Money(1) < Money(2)"),
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

fn classify_exit(test_src: &str) -> Result<Finding, String> {
    classify(
        RECORDER,
        "shapes/ctx.py",
        "__exit__",
        "        self.events.append(\"leave\")",
        "        self.events.append(\"exit\")",
        test_src,
    )
}

fn recorder_test(body: &str) -> String {
    format!("from shapes.ctx import Recorder\n\n\ndef test_it():\n{body}\n")
}

#[test]
fn with_block_state_read_after_the_block_credits_the_exit_effect() -> Check {
    let finding = classify_exit(&recorder_test(
        "    with Recorder() as rec:\n        pass\n    assert rec.events == [\"enter\", \"exit\"]",
    ))?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    let bound = classify_exit(&recorder_test(
        "    rec = Recorder()\n    with rec:\n        pass\n    assert rec.events == [\"enter\", \"exit\"]",
    ))?;
    assert_eq!(bound.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn with_block_read_inside_the_block_does_not_credit_the_exit_effect() -> Check {
    let finding = classify_exit(&recorder_test(
        "    with Recorder() as rec:\n        assert rec.events == [\"enter\"]",
    ))?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{:#?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn with_block_unrelated_assertion_is_a_gap_for_the_suppress_edit() -> Check {
    let finding = classify(
        RECORDER,
        "shapes/ctx.py",
        "__exit__",
        "        return True",
        "        return False",
        &recorder_test(
            "    with Recorder() as rec:\n        pass\n    assert rec.events == [\"enter\", \"exit\"]",
        ),
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn with_entry_whose_target_may_not_be_the_manager_is_not_bound() -> Check {
    let other_target = RECORDER.replace("        return self\n", "        return []\n");
    let owners = owners_of(&other_target, "shapes/ctx.py");
    assert_eq!(
        relation(
            &owners,
            "__exit__",
            &recorder_test(
                "    with Recorder() as rec:\n        pass\n    assert rec == [\"enter\", \"exit\"]"
            )
        )?,
        Some("dunder_protocol")
    );
    // Entering twice cannot be attributed to one statement.
    let owners = owners_of(RECORDER, "shapes/ctx.py");
    assert_eq!(
        relation(
            &owners,
            "__exit__",
            &recorder_test(
                "    with Recorder() as rec:\n        pass\n    with Recorder() as rec:\n        pass\n    assert rec.events == [\"exit\"]"
            )
        )?,
        Some("dunder_protocol")
    );
    // A multi-item `with` may enter another object first.
    assert_eq!(
        relation(
            &owners,
            "__exit__",
            &recorder_test(
                "    with open_thing(), Recorder() as rec:\n        pass\n    assert rec.events == [\"exit\"]"
            )
        )?,
        Some("dunder_protocol")
    );
    Ok(())
}
