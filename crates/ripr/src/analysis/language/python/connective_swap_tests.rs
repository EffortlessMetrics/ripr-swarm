//! Tests for the connective-swap gate (#16203): a condition whose only edit
//! swaps `and` for `or` is discriminated only by a call where its operands
//! disagree.

use super::connective_swap::swapped_connective;
use super::owners_tests::{extract_owners, extract_tests};
use super::*;

const SOURCE: &str = "LIMIT = 10\n\n\ndef ok(a, b):\n    return a > 0 and b > 0\n\n\ndef capped(n, strict):\n    if n > LIMIT and strict:\n        return 0\n    return n\n";

fn classify(line: usize, old: &str, new: &str, test_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/rules.py"), SOURCE);
    let tests = extract_tests(Path::new("tests/test_rules.py"), test_source);
    classify_change_with_old(
        Path::new("src/rules.py"),
        line,
        new,
        Some(old),
        &owners,
        &tests,
    )
    .ok_or_else(|| "a condition change should classify".to_string())
}

fn classify_ok(test_source: &str) -> Result<Finding, String> {
    classify(
        5,
        "    return a > 0 and b > 0",
        "    return a > 0 or b > 0",
        test_source,
    )
}

#[test]
fn agreeing_operands_do_not_expose_an_and_or_swap() -> Result<(), String> {
    let finding = classify_ok(
        "from src.rules import ok\n\n\ndef test_ok():\n    assert ok(1, 2) is True\n    assert ok(-1, -2) is False\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|entry| entry.contains("where `a > 0` and `b > 0` differ")),
        "{:?}",
        finding.missing
    );
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `ok` with inputs where `a > 0` and `b > 0` differ"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    Ok(())
}

#[test]
fn a_call_with_disagreeing_operands_keeps_exposed() -> Result<(), String> {
    let finding = classify_ok(
        "from src.rules import ok\n\n\ndef test_ok():\n    assert ok(1, 2) is True\n    assert ok(1, -2) is False\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_unresolved_call_keeps_exposed() -> Result<(), String> {
    let finding = classify_ok(
        "from src.rules import ok\n\nVALUES = (1, -2)\n\n\ndef test_ok():\n    assert ok(1, 2) is True\n    assert ok(*VALUES) is False\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_if_condition_with_a_module_constant_reads_agreeing_calls() -> Result<(), String> {
    let old = "    if n > LIMIT and strict:";
    let new = "    if n > LIMIT or strict:";
    let agreeing = classify(
        9,
        old,
        new,
        "from src.rules import capped\n\n\ndef test_capped():\n    assert capped(20, True) == 0\n    assert capped(3, False) == 3\n",
    )?;
    assert_eq!(agreeing.class, ExposureClass::WeaklyExposed);
    let disagreeing = classify(
        9,
        old,
        new,
        "from src.rules import capped\n\n\ndef test_capped():\n    assert capped(20, False) == 20\n",
    )?;
    assert_eq!(
        disagreeing.class,
        ExposureClass::Exposed,
        "{:?}",
        disagreeing.missing
    );
    Ok(())
}

#[test]
fn only_a_lone_connective_swap_between_simple_operands_is_read() {
    assert_eq!(
        swapped_connective("return a > 0 and b > 0", "return a > 0 or b > 0"),
        Some(("a > 0".to_string(), "b > 0".to_string()))
    );
    assert_eq!(
        swapped_connective("elif not done or ready:", "elif not done and ready:"),
        Some(("not done".to_string(), "ready".to_string()))
    );
    // Another token changed too.
    assert_eq!(
        swapped_connective("return a > 0 and b > 0", "return a > 1 or b > 0"),
        None
    );
    // Three operands: the swapped one is not the only top-level connective.
    assert_eq!(
        swapped_connective("return a and b and c", "return a or b and c"),
        None
    );
    // An operand the rule cannot evaluate.
    assert_eq!(
        swapped_connective("return f(a) and b", "return f(a) or b"),
        None
    );
    // Not a condition statement.
    assert_eq!(swapped_connective("x = a and b", "x = a or b"), None);
    // A comment on the line.
    assert_eq!(
        swapped_connective("return a and b  # both", "return a or b  # both"),
        None
    );
}
