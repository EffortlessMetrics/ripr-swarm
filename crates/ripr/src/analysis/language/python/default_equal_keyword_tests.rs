//! Tests for an added keyword argument that restates the callee's default
//! (#16011 shape 2).

use super::default_equal_keyword::default_equal_keyword;
use super::owners_tests::{extract_owners, extract_tests};
use super::*;

const CLOCK_SOURCE: &str = "class Clock:\n    def __init__(self, t):\n        self.t = t\n\n    def shift(self, check_imaginary=True, **kwargs):\n        return Clock(self.t + kwargs.get(\"hours\", 0))\n\n\ndef dehumanize(current_time, **time_changes):\n    return current_time.shift(check_imaginary=True, **time_changes)\n";
const OLD_LINE: &str = "    return current_time.shift(**time_changes)";
const NEW_LINE: &str = "    return current_time.shift(check_imaginary=True, **time_changes)";
const CLOCK_TEST: &str = "from src.clock import Clock, dehumanize\n\n\ndef test_dehumanize():\n    assert dehumanize(Clock(1), hours=2).t == 3\n";

fn clock_owners(source: &str) -> Vec<PythonOwner> {
    extract_owners(Path::new("src/clock.py"), source)
}

#[test]
fn a_restated_default_reads_weakly_exposed() -> Result<(), String> {
    let owners = clock_owners(CLOCK_SOURCE);
    let tests = extract_tests(Path::new("tests/test_clock.py"), CLOCK_TEST);
    let finding = classify_change_with_old(
        Path::new("src/clock.py"),
        10,
        NEW_LINE,
        Some(OLD_LINE),
        &owners,
        &tests,
    )
    .ok_or_else(|| "the call change should classify".to_string())?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|entry| entry.contains("`check_imaginary=True` equals the declared default")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_different_value_keeps_its_reading() -> Result<(), String> {
    let source = CLOCK_SOURCE.replace(
        "shift(check_imaginary=True, **time_changes)",
        "shift(check_imaginary=False, **time_changes)",
    );
    let owners = clock_owners(&source);
    let tests = extract_tests(Path::new("tests/test_clock.py"), CLOCK_TEST);
    let finding = classify_change_with_old(
        Path::new("src/clock.py"),
        10,
        "    return current_time.shift(check_imaginary=False, **time_changes)",
        Some(OLD_LINE),
        &owners,
        &tests,
    )
    .ok_or_else(|| "the call change should classify".to_string())?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn only_a_restated_default_of_every_definition_matches() {
    let owners = clock_owners(CLOCK_SOURCE);
    let found = default_equal_keyword(OLD_LINE, NEW_LINE, &owners);
    assert_eq!(
        found.map(|keyword| (keyword.callee, keyword.argument)),
        Some(("shift".to_string(), "check_imaginary=True".to_string()))
    );
    // The keyword may come last, with or without spaces.
    assert!(
        default_equal_keyword(
            "    return current_time.shift(**time_changes)",
            "    return current_time.shift(**time_changes, check_imaginary = True)",
            &owners,
        )
        .is_some()
    );
    // A second definition with another default makes the callee ambiguous.
    let rival = format!(
        "{CLOCK_SOURCE}\n\nclass Other:\n    def shift(self, check_imaginary=False):\n        return self\n"
    );
    assert_eq!(
        default_equal_keyword(OLD_LINE, NEW_LINE, &clock_owners(&rival)),
        None
    );
    // A class of the callee's name is a constructor, not a definition read here.
    let class_named = format!("{CLOCK_SOURCE}\n\nclass shift:\n    pass\n");
    assert_eq!(
        default_equal_keyword(OLD_LINE, NEW_LINE, &clock_owners(&class_named)),
        None
    );
    for (old, new) in [
        // Another value, a name value, an unknown callee, another edit.
        (
            OLD_LINE,
            "    return current_time.shift(check_imaginary=False, **time_changes)",
        ),
        (
            OLD_LINE,
            "    return current_time.shift(check_imaginary=flag, **time_changes)",
        ),
        (
            "    return current_time.move(**time_changes)",
            "    return current_time.move(check_imaginary=True, **time_changes)",
        ),
        (
            OLD_LINE,
            "    return current_time.shift(check_imaginary=True, **other)",
        ),
        (
            OLD_LINE,
            "    return current_time.shift(check_imaginary=True, **time_changes)  # x",
        ),
    ] {
        assert_eq!(default_equal_keyword(old, new, &owners), None, "{new}");
    }
}
