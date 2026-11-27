//! Tests for the connective-swap gate (#16203): a condition whose only edit
//! swaps `and` for `or` is discriminated only by a call where its operands
//! disagree.

use super::connective_swap::{compare, swapped_connective, truthy};
use super::owners_tests::{extract_owners, extract_tests};
use super::*;

const SOURCE: &str = "LIMIT = 10\n\n\ndef ok(a, b):\n    return a > 0 and b > 0\n\n\ndef capped(n, strict):\n    if n > LIMIT and strict:\n        return 0\n    return n\n\n\ndef near(a, b):\n    return a == 0.1 and b == 0\n";

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
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    assert!(
        finding.ripr.infect.summary.contains("same truth value"),
        "{}",
        finding.ripr.infect.summary
    );
    Ok(())
}

#[test]
fn a_call_with_disagreeing_operands_keeps_exposed() -> Result<(), String> {
    let finding = classify_ok(
        "from src.rules import ok\n\n\ndef test_ok():\n    assert ok(1, 2) is True\n    assert ok(1, -2) is False\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    Ok(())
}

fn classify_near(test_source: &str) -> Result<Finding, String> {
    classify(
        15,
        "    return a == 0.1 and b == 0",
        "    return a == 0.1 or b == 0",
        test_source,
    )
}

#[test]
fn a_float_operand_is_read_by_its_represented_value() -> Result<(), String> {
    // `0.10000000000000001 == 0.1` holds in Python, so both operands are
    // true here and `and`/`or` agree.
    let agreeing = classify_near(
        "from src.rules import near\n\n\ndef test_near():\n    assert near(0.10000000000000001, 0) is True\n",
    )?;
    assert_eq!(agreeing.class, ExposureClass::WeaklyExposed);
    // With `b == 0` false, the operands disagree and the call tells the
    // versions apart.
    let disagreeing = classify_near(
        "from src.rules import near\n\n\ndef test_near():\n    assert near(0.10000000000000001, 1) is False\n",
    )?;
    assert_eq!(disagreeing.class, ExposureClass::Exposed);
    assert_eq!(disagreeing.ripr.infect.state, StageState::Yes);
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
    // A `return` of a bare operand gives that operand's value, not its
    // truth value: `1 and 2` is 2, `1 or 2` is 1.
    assert_eq!(swapped_connective("return a and b", "return a or b"), None);
    assert_eq!(
        swapped_connective("return a > 0 and b", "return a > 0 or b"),
        None
    );
    assert_eq!(
        swapped_connective("if a and b:", "if a or b:"),
        Some(("a".to_string(), "b".to_string()))
    );
    // A line that also changes its block.
    assert_eq!(swapped_connective("    if a and b:", "if a or b:"), None);
    // Not a condition statement.
    assert_eq!(swapped_connective("x = a and b", "x = a or b"), None);
    // A comment on the line.
    assert_eq!(
        swapped_connective("return a and b  # both", "return a or b  # both"),
        None
    );
}

#[test]
fn a_static_method_swap_reads_its_calls() -> Result<(), String> {
    let source =
        "class Rules:\n    @staticmethod\n    def ok(a, b):\n        return a > 0 and b > 0\n";
    let owners = extract_owners(Path::new("src/rules.py"), source);
    let finding = |test_source: &str| {
        let tests = extract_tests(Path::new("tests/test_rules.py"), test_source);
        classify_change_with_old(
            Path::new("src/rules.py"),
            4,
            "        return a > 0 or b > 0",
            Some("        return a > 0 and b > 0"),
            &owners,
            &tests,
        )
        .ok_or_else(|| "a condition change should classify".to_string())
    };
    let agreeing = finding(
        "from src.rules import Rules\n\n\ndef test_ok():\n    assert Rules.ok(1, 2) is True\n",
    )?;
    assert_eq!(agreeing.class, ExposureClass::WeaklyExposed);
    let disagreeing = finding(
        "from src.rules import Rules\n\n\ndef test_ok():\n    assert Rules.ok(1, -2) is False\n",
    )?;
    assert_eq!(disagreeing.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn operand_values_follow_python_comparison_and_truth() {
    // Ints compare exactly, past the last exact float.
    assert_eq!(
        compare("1700000000000000001", ">", "1700000000000000000"),
        Some(true)
    );
    assert_eq!(
        compare("9007199254740993", "==", "9007199254740992.0"),
        None
    );
    assert_eq!(compare("2", ">=", "1.5"), Some(true));
    assert_eq!(
        compare(
            "170141183460469231731687303715884105729",
            ">",
            "170141183460469231731687303715884105728"
        ),
        None
    );
    // Floats compare by the value each spelling denotes.
    assert_eq!(compare("0.10000000000000001", "==", "0.1"), Some(true));
    assert_eq!(compare("1.0", "!=", "1"), Some(false));
    // Strings with escapes or prefixes are not read.
    assert_eq!(compare("\"\\n\"", "==", "\"\\x0a\""), None);
    assert_eq!(compare("\"a\"", "==", "'a'"), Some(true));
    assert_eq!(truthy("\"\"\"\"\"\""), Some(false));
    assert_eq!(truthy("''"), Some(false));
    assert_eq!(truthy("' '"), Some(true));
    assert_eq!(truthy("b''"), None);
    assert_eq!(truthy("0.000"), Some(false));
    assert_eq!(truthy("-0.5"), Some(true));
}

#[test]
fn a_return_of_bare_operands_keeps_its_reading() -> Result<(), String> {
    // `1 and 2` is 2 and `1 or 2` is 1, so an exact assertion on a truthy
    // pair still tells the swap apart.
    let source = "def pick(a, b):\n    return a and b\n";
    let owners = extract_owners(Path::new("src/pick.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_pick.py"),
        "from src.pick import pick\n\n\ndef test_pick():\n    assert pick(1, 2) == 2\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/pick.py"),
        2,
        "    return a or b",
        Some("    return a and b"),
        &owners,
        &tests,
    )
    .ok_or_else(|| "a return change should classify".to_string())?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_ne!(finding.ripr.infect.state, StageState::Weak);
    Ok(())
}
