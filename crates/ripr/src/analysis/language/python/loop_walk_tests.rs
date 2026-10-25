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
