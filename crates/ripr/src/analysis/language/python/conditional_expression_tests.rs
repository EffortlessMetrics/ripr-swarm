//! Pins for the unreached conditional-expression arm rule (#9184).
//!
//! Each shape that must stop crediting `exposed` has a control that keeps the
//! credit when an observed input evaluates the changed arm, and fail-open
//! probes for every shape the reader refuses.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_FILE: &str = "src/greet.py";
const TEST_FILE: &str = "tests/test_greet.py";
const IMPORT: &str = "from src.greet import greet";

/// Classify the last line of `owner` against `test_body`.
fn classify(owner: &str, old_line: &str, test_body: &str) -> Result<ExposureClass, String> {
    finding(owner, old_line, test_body).map(|finding| finding.class)
}

/// Whether this rule (and not the boundary or oracle logic) produced the
/// verdict: its reason names the conditional-expression arm.
fn rule_fired(owner: &str, old_line: &str, test_body: &str) -> Result<bool, String> {
    finding(owner, old_line, test_body)
        .map(|finding| format!("{finding:?}").contains("arm of a conditional expression"))
}

fn finding(owner: &str, old_line: &str, test_body: &str) -> Result<Finding, String> {
    let line = owner.lines().count();
    let new_line = owner
        .lines()
        .last()
        .ok_or_else(|| "owner is empty".to_string())?;
    let owners = extract_owners(Path::new(OWNER_FILE), owner);
    let tests = extract_tests(Path::new(TEST_FILE), &format!("{IMPORT}\n\n\n{test_body}"));
    classify_change_with_context(
        Path::new(OWNER_FILE),
        line,
        new_line,
        Some(old_line),
        &owners,
        &tests,
        PythonNoBehaviorContext::default(),
        Some(owner),
    )
    .ok_or_else(|| "changed line should classify".to_string())
}

const TRUE_ARM: &str = "def greet(name, loud=False):\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg";
const TRUE_ARM_OLD: &str = "    return msg.upper() if loud else msg";

#[test]
fn a_true_arm_no_observed_input_takes_does_not_credit() -> Result<(), String> {
    // The default `loud=False` and an explicit `False` never take the arm.
    let omitted = "def test_greet():\n    assert greet(\"a\") == \"hi a\"\n";
    assert_eq!(
        classify(TRUE_ARM, TRUE_ARM_OLD, omitted)?,
        ExposureClass::WeaklyExposed
    );
    let explicit = "def test_greet():\n    assert greet(\"a\", loud=False) == \"hi a\"\n    assert greet(\"b\", False) == \"hi b\"\n";
    assert_eq!(
        classify(TRUE_ARM, TRUE_ARM_OLD, explicit)?,
        ExposureClass::WeaklyExposed
    );
    // A single loud call evaluates the changed arm: the credit stays.
    let reached = "def test_greet():\n    assert greet(\"a\") == \"hi a\"\n    assert greet(\"a\", loud=True) == \"hi a\"\n";
    assert_eq!(
        classify(TRUE_ARM, TRUE_ARM_OLD, reached)?,
        ExposureClass::Exposed
    );
    // A truthy non-bool literal takes the arm as well.
    let truthy = "def test_greet():\n    assert greet(\"a\", 1) == \"hi a\"\n";
    assert_eq!(
        classify(TRUE_ARM, TRUE_ARM_OLD, truthy)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_false_arm_every_observed_input_skips_does_not_credit() -> Result<(), String> {
    let owner = "def greet(name, loud=False):\n    msg = \"hi \" + name\n    return msg.upper() if loud else msg + \"!\"";
    let old = "    return msg.upper() if loud else msg";
    let always_loud = "def test_greet():\n    assert greet(\"a\", loud=True) == \"HI A\"\n";
    assert_eq!(
        classify(owner, old, always_loud)?,
        ExposureClass::WeaklyExposed
    );
    let quiet_too = "def test_greet():\n    assert greet(\"a\", loud=True) == \"HI A\"\n    assert greet(\"a\") == \"hi a!\"\n";
    assert_eq!(classify(owner, old, quiet_too)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_comparison_condition_never_reads_exposed_when_the_arm_is_unreached() -> Result<(), String> {
    // Numeric thresholds are claimed first by the boundary reader, so this
    // pins the verdict (not which reader produced it): every input is below
    // the threshold, the changed true arm never runs, and no credit is given.
    let small = "def test_cap():\n    assert cap(1) == 1\n    assert cap(2) == 2\n";
    let owner = "def cap(n):\n    return n * 3 if n > 10 else n";
    let old = "    return n * 2 if n > 10 else n";
    assert_eq!(classify(owner, old, small)?, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn unreadable_shapes_fail_open() -> Result<(), String> {
    let omitted = "def test_greet():\n    assert greet(\"a\") == \"hi a\"\n";
    // The condition changed too: a predicate-boundary question, not this rule.
    let cond_changed = "def greet(name, loud=False):\n    msg = \"hi \" + name\n    return msg.lower() if not loud else msg";
    assert!(!rule_fired(cond_changed, TRUE_ARM_OLD, omitted)?);
    // The owner rebinds the condition parameter before the expression.
    let rebound = "def greet(name, loud=False):\n    loud = name == \"a\"\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg";
    assert!(!rule_fired(rebound, TRUE_ARM_OLD, omitted)?);
    // A comment on the line is not read.
    let commented = "def greet(name, loud=False):\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg  # quiet";
    assert!(!rule_fired(commented, TRUE_ARM_OLD, omitted)?);
    // A nested conditional expression is not read.
    let nested = "def greet(name, loud=False):\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg if name else \"\"";
    assert!(!rule_fired(nested, TRUE_ARM_OLD, omitted)?);
    // A non-literal argument leaves the row unreadable.
    let dynamic = "def test_greet(flag):\n    assert greet(\"a\", loud=flag) == \"hi a\"\n";
    assert!(!rule_fired(TRUE_ARM, TRUE_ARM_OLD, dynamic)?);
    Ok(())
}

#[test]
fn a_rebound_default_parameter_is_not_credited_as_truthiness_discriminated() -> Result<(), String> {
    // The omitted `loud` takes its `False` default, but the owner rebinds the
    // name through a statement form that is not an assignment, so the call's
    // default no longer predicts what the expression reads.
    let omitted = "def test_greet():\n    assert greet(\"a\") == \"hi a\"\n";
    for rebinding in [
        "    import settings as loud",
        "    from settings import loud",
        "    def loud():\n        return True",
        "    class loud:\n        pass",
        "    del loud",
        "    global loud",
    ] {
        let owner = format!(
            "def greet(name, loud=False):\n{rebinding}\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg"
        );
        assert!(
            !rule_fired(&owner, TRUE_ARM_OLD, omitted)?,
            "rebinding `{rebinding}` must fail open"
        );
    }
    // Discriminating control: a body that never binds `loud` keeps the
    // unreached-arm downgrade for the very same omitted default.
    let untouched = "def greet(name, loud=False):\n    import settings\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg";
    assert_eq!(
        classify(untouched, TRUE_ARM_OLD, omitted)?,
        ExposureClass::WeaklyExposed
    );
    // A same-prefixed name is a different binding and must not over-refuse.
    let sibling = "def greet(name, loud=False):\n    import settings as loudness\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg";
    assert_eq!(
        classify(sibling, TRUE_ARM_OLD, omitted)?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}
