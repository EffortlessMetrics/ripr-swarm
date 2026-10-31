//! RIPR-SPEC-0028: rebound generator counters and the changed boundary (#9733).
use super::owners_tests::{extract_owners, extract_tests};
use super::*;

fn classify_loop(
    owner: &str,
    line: usize,
    old: &str,
    input: &str,
    expected: &str,
) -> Result<Finding, String> {
    let file = Path::new("src/steps.py");
    let test = format!(
        "from src.steps import countdown\n\ndef test_steps():\n    assert list(countdown({input})) == {expected}\n"
    );
    let new = owner.lines().nth(line - 1).ok_or("missing changed line")?;
    classify_change_with_source(
        file,
        line,
        new,
        Some(old),
        &extract_owners(file, owner),
        &extract_tests(Path::new("tests/test_steps.py"), &test),
        Some(owner),
    )
    .ok_or_else(|| "missing finding".to_string())
}

const DOWN: &str =
    "def countdown(start):\n    while start >= 0:\n        yield start\n        start -= 1\n";

#[test]
fn unit_walk_rejects_past_bound_and_keeps_near_side_credit() -> Result<(), String> {
    for (input, expected, class) in [
        ("-2", "[]", ExposureClass::WeaklyExposed),
        ("3", "[3, 2, 1]", ExposureClass::Exposed),
        ("0", "[]", ExposureClass::Exposed),
    ] {
        let finding = classify_loop(DOWN, 2, "    while start > 0:", input, expected)?;
        assert_eq!(finding.class, class, "input {input}");
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line.starts_with("boundary_activation_unresolved: ")),
            "resolved input {input}: {:?}",
            finding.evidence
        );
        if class == ExposureClass::Exposed && input == "3" {
            assert!(
                finding
                    .activation
                    .observed_values
                    .iter()
                    .any(|fact| fact.text.contains("unit-step generator walk")),
                "{:?}",
                finding.activation
            );
        }
    }
    let up = DOWN.replace(">=", "<=").replace("-=", "+=");
    assert_eq!(
        classify_loop(&up, 2, "    while start < 0:", "2", "[]")?.class,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify_loop(&up, 2, "    while start < 0:", "-2", "[-2, -1]")?.class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn uncertain_walks_keep_the_named_unresolved_limit() -> Result<(), String> {
    for body in [
        "        yield start\n        start -= 2\n",
        "        yield start\n        if start > 0:\n            start -= 1\n",
        "        yield start\n        start -= 1\n        break\n",
        "        yield start\n        start -= 1\n        start -= 1\n",
    ] {
        let owner = format!("def countdown(start):\n    while start >= 0:\n{body}");
        let finding = classify_loop(&owner, 2, "    while start > 0:", "3", "[3, 1]")?;
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with("boundary_activation_unresolved: ")),
            "{body}: {:?}",
            finding.evidence
        );
    }
    let before = DOWN.replace("    while", "    start = 3\n    while");
    let finding = classify_loop(&before, 3, "    while start > 0:", "-2", "[3, 2, 1]")?;
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("boundary_activation_unresolved: "))
    );
    Ok(())
}

#[test]
fn reverse_walks_and_source_offsets_resolve_the_same_boundary() -> Result<(), String> {
    let down = DOWN.replace(">=", ">");
    assert_eq!(
        classify_loop(&down, 2, "    while start >= 0:", "3", "[3, 2, 1, 0]")?.class,
        ExposureClass::Exposed
    );
    assert_eq!(
        classify_loop(&down, 2, "    while start >= 0:", "-2", "[]")?.class,
        ExposureClass::WeaklyExposed
    );
    let up = DOWN.replace(">=", "<").replace("-=", "+=");
    assert_eq!(
        classify_loop(&up, 2, "    while start <= 0:", "-2", "[-2, -1, 0]")?.class,
        ExposureClass::Exposed
    );
    assert_eq!(
        classify_loop(&up, 2, "    while start <= 0:", "2", "[]")?.class,
        ExposureClass::WeaklyExposed
    );
    let prefixed = format!("# preceding source\nVALUE = 7\n\n{DOWN}");
    assert_eq!(
        classify_loop(&prefixed, 5, "    while start > 0:", "-2", "[]")?.class,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn a_readable_miss_cannot_hide_an_unreadable_call_row() -> Result<(), String> {
    for suffix in [
        "[]\n    args = (-2,)\n    assert list(countdown(*args)) == []",
        "[]\n    opaque = int(\"-2\")\n    assert list(countdown(opaque)) == []",
    ] {
        let finding = classify_loop(DOWN, 2, "    while start > 0:", "-2", suffix)?;
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with("boundary_activation_unresolved: ")),
            "{:?}",
            finding.evidence
        );
    }
    Ok(())
}

fn classify_test_source(owner: &str, test: &str) -> Result<Finding, String> {
    let file = Path::new("src/steps.py");
    classify_change_with_source(
        file,
        2,
        "    while start >= 0:",
        Some("    while start > 0:"),
        &extract_owners(file, owner),
        &extract_tests(Path::new("tests/test_steps.py"), test),
        Some(owner),
    )
    .ok_or_else(|| "missing finding".to_string())
}

#[test]
fn partial_or_lossy_consumers_do_not_claim_a_walked_boundary() -> Result<(), String> {
    let mut failures = Vec::new();
    for assertion in [
        "assert next(countdown(3)) == 3",
        "assert sum(countdown(3)) == 6",
        "assert sum(countdown(0)) == 0",
        "assert max(countdown(3)) == 3",
        "assert list(countdown(3))[:1] == [3]",
        "assert list(countdown(3))[0] == 3",
    ] {
        let test =
            format!("from src.steps import countdown\n\ndef test_steps():\n    {assertion}\n");
        let finding = classify_test_source(DOWN, &test)?;
        if !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("boundary_activation_unresolved: "))
            || finding
                .activation
                .observed_values
                .iter()
                .any(|fact| fact.text.contains("unit-step generator walk"))
        {
            failures.push(format!(
                "{assertion}: {:?}; {:?}",
                finding.evidence, finding.activation
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[test]
fn an_unobserved_generator_call_cannot_supply_the_boundary() -> Result<(), String> {
    for (owner, body) in [
        (
            DOWN.to_string(),
            "    countdown(3)\n    assert list(countdown(-2)) == []\n",
        ),
        (
            DOWN.to_string(),
            "    assert list(countdown(-2)) == [], list(countdown(3))\n",
        ),
        (
            DOWN.replace("start):", "start, unused=None):"),
            "    assert list(countdown(-2, unused=countdown(3))) == []\n",
        ),
    ] {
        let test = format!("from src.steps import countdown\n\ndef test_steps():\n{body}");
        let finding = classify_test_source(&owner, &test)?;
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with("boundary_activation_unresolved: ")),
            "{body}: {:?}",
            finding.evidence
        );
        assert!(
            !finding
                .activation
                .observed_values
                .iter()
                .any(|fact| fact.text.contains("unit-step generator walk")),
            "{body}: {:?}",
            finding.activation
        );
    }
    Ok(())
}

#[test]
fn shadowed_sequence_constructors_cannot_supply_the_boundary() -> Result<(), String> {
    for (setup, body) in [
        (
            "list = lambda value: []\n",
            "    assert list(countdown(3)) == []\n",
        ),
        (
            "",
            "    list = lambda value: []\n    assert list(countdown(3)) == []\n",
        ),
        (
            "import builtins\nbuiltins.list = lambda value: []\n",
            "    assert list(countdown(3)) == []\n",
        ),
    ] {
        let test = format!("from src.steps import countdown\n{setup}\ndef test_steps():\n{body}");
        let finding = classify_test_source(DOWN, &test)?;
        assert!(
            !finding
                .activation
                .observed_values
                .iter()
                .any(|fact| fact.text.contains("unit-step generator walk")),
            "{setup}{body}: {:?}",
            finding.activation
        );
    }
    Ok(())
}

#[test]
fn unit_walk_keeps_integer_boundaries_exact() -> Result<(), String> {
    for (bound, near, past) in [
        ("9007199254740993", "9007199254740994", "9007199254740992"),
        (
            "170141183460469231731687303715884105726",
            "170141183460469231731687303715884105727",
            "170141183460469231731687303715884105725",
        ),
        (
            "-170141183460469231731687303715884105727",
            "-170141183460469231731687303715884105726",
            "-170141183460469231731687303715884105728",
        ),
    ] {
        let owner = DOWN.replace(">= 0", &format!(">= {bound}"));
        let old = format!("    while start > {bound}:");
        assert_eq!(
            classify_loop(&owner, 2, &old, past, "[]")?.class,
            ExposureClass::WeaklyExposed,
            "past {bound}"
        );
        assert_eq!(
            classify_loop(&owner, 2, &old, near, &format!("[{near}]"))?.class,
            ExposureClass::Exposed,
            "near {bound}"
        );
        assert_eq!(
            classify_loop(&owner, 2, &old, bound, "[]")?.class,
            ExposureClass::Exposed,
            "equal {bound}"
        );
    }
    Ok(())
}

#[test]
fn whole_sequence_oracles_keep_owner_and_constructor_identity() -> Result<(), String> {
    for test in [
        "from src.steps import countdown\n\ndef test_steps():\n    assert (3, 2, 1) == tuple(countdown(3))\n",
        "from src.steps import countdown as walk\n\ndef test_steps():\n    assert list(walk(3)) == [3, 2, 1]\n",
        "import src.steps as steps\n\ndef test_steps():\n    assert list(steps.countdown(3)) == [3, 2, 1]\n",
    ] {
        let finding = classify_test_source(DOWN, test)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{test}");
        assert!(
            finding
                .activation
                .observed_values
                .iter()
                .any(|fact| fact.text.contains("unit-step generator walk")),
            "{test}: {:?}",
            finding.activation
        );
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line.starts_with("boundary_activation_unresolved: ")),
            "{test}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}
