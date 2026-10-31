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
    .map(|finding| finding.class)
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
fn a_comparison_condition_is_read_like_a_guard() -> Result<(), String> {
    let small = "def test_cap():\n    assert cap(1) == 1\n    assert cap(2) == 2\n";
    // The false arm changed and every input is small: the arm runs, credit stays.
    let owner = "def cap(n):\n    return n * 2 if n > 10 else n * 3";
    let old = "    return n * 2 if n > 10 else n";
    assert_eq!(classify(owner, old, small)?, ExposureClass::Exposed);
    // The true arm changed and every input is small: unreached.
    let owner = "def cap(n):\n    return n * 3 if n > 10 else n";
    let old = "    return n * 2 if n > 10 else n";
    assert_eq!(classify(owner, old, small)?, ExposureClass::WeaklyExposed);
    let big = "def test_cap():\n    assert cap(11) == 33\n";
    assert_eq!(classify(owner, old, big)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn unreadable_shapes_fail_open() -> Result<(), String> {
    let omitted = "def test_greet():\n    assert greet(\"a\") == \"hi a\"\n";
    // The condition changed too: a predicate-boundary question, not this rule.
    let cond_changed = "def greet(name, loud=False):\n    msg = \"hi \" + name\n    return msg.lower() if not loud else msg";
    assert_ne!(
        classify(cond_changed, TRUE_ARM_OLD, omitted)?,
        ExposureClass::WeaklyExposed
    );
    // The owner rebinds the condition parameter before the expression.
    let rebound = "def greet(name, loud=False):\n    loud = name == \"a\"\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg";
    assert_eq!(
        classify(rebound, TRUE_ARM_OLD, omitted)?,
        ExposureClass::Exposed
    );
    // A comment on the line is not read.
    let commented = "def greet(name, loud=False):\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg  # quiet";
    assert_eq!(
        classify(commented, TRUE_ARM_OLD, omitted)?,
        ExposureClass::Exposed
    );
    // A nested conditional expression is not read.
    let nested = "def greet(name, loud=False):\n    msg = \"hi \" + name\n    return msg.lower() if loud else msg if name else \"\"";
    assert_eq!(
        classify(nested, TRUE_ARM_OLD, omitted)?,
        ExposureClass::Exposed
    );
    // A non-literal argument leaves the row unreadable.
    let dynamic = "def test_greet(flag):\n    assert greet(\"a\", loud=flag) == \"hi a\"\n";
    assert_eq!(
        classify(TRUE_ARM, TRUE_ARM_OLD, dynamic)?,
        ExposureClass::Exposed
    );
    Ok(())
}
