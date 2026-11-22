//! Tests for an added keyword argument that restates the callee's default
//! (#16011 shape 2).

use super::default_equal_keyword::default_equal_keyword;
use super::owners_tests::{extract_owners, extract_tests};
use super::*;

const CLOCK_SOURCE: &str = "class Clock:\n    def __init__(self, t):\n        self.t = t\n\n    def shift(self, check_imaginary=True, **kwargs):\n        return Clock(self.t + kwargs.get(\"hours\", 0))\n\n\ndef dehumanize(current_time, **time_changes):\n    return current_time.shift(check_imaginary=True, **time_changes)\n";
const OLD_LINE: &str = "    return current_time.shift(**time_changes)";
const NEW_LINE: &str = "    return current_time.shift(check_imaginary=True, **time_changes)";
const CLOCK_TEST: &str = "from src.clock import Clock, dehumanize\n\n\ndef test_dehumanize():\n    assert dehumanize(Clock(1), hours=2).t == 3\n";

fn clock_file() -> &'static Path {
    Path::new("src/clock.py")
}

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
    let found = default_equal_keyword(OLD_LINE, NEW_LINE, clock_file(), &owners);
    assert_eq!(
        found.map(|keyword| (keyword.callee, keyword.argument)),
        Some(("shift".to_string(), "check_imaginary=True".to_string()))
    );
    // The keyword may come last, with or without spaces.
    assert!(
        default_equal_keyword(
            "    return current_time.shift(**time_changes)",
            "    return current_time.shift(**time_changes, check_imaginary = True)",
            clock_file(),
            &owners,
        )
        .is_some()
    );
    // A second definition with another default makes the callee ambiguous.
    let rival = format!(
        "{CLOCK_SOURCE}\n\nclass Other:\n    def shift(self, check_imaginary=False):\n        return self\n"
    );
    assert_eq!(
        default_equal_keyword(OLD_LINE, NEW_LINE, clock_file(), &clock_owners(&rival)),
        None
    );
    // A class of the callee's name is a constructor, not a definition read here.
    let class_named = format!("{CLOCK_SOURCE}\n\nclass shift:\n    pass\n");
    assert_eq!(
        default_equal_keyword(
            OLD_LINE,
            NEW_LINE,
            clock_file(),
            &clock_owners(&class_named)
        ),
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
        assert_eq!(
            default_equal_keyword(old, new, clock_file(), &owners),
            None,
            "{new}"
        );
    }
}

#[test]
fn review_controls_keep_their_reading() {
    // The only argument of an empty call.
    let source = "def label(x, sep=\" \"):\n    return x + sep + x\n\n\ndef shown():\n    return label(\"x\")\n";
    let owners = clock_owners(source);
    let file = clock_file();
    let empty = "def tick(count=1):\n    return count\n\n\ndef run():\n    return tick()\n";
    assert!(
        default_equal_keyword(
            "    return tick()",
            "    return tick(count=1)",
            file,
            &clock_owners(empty),
        )
        .is_some()
    );
    // Spaces inside a string are part of its value.
    assert!(
        default_equal_keyword(
            "    return label(\"x\")",
            "    return label(\"x\", sep=\" \")",
            file,
            &owners,
        )
        .is_some()
    );
    assert_eq!(
        default_equal_keyword(
            "    return label(\"x\")",
            "    return label(\"x\", sep=\"  \")",
            file,
            &owners,
        ),
        None
    );
    assert_eq!(
        default_equal_keyword(
            "    return label(\"x \")",
            "    return label(\"x\", sep=\" \")",
            file,
            &owners,
        ),
        None
    );
    // A call spelled inside a string is text, not a call.
    assert_eq!(
        default_equal_keyword(
            "    return \"label(\\\"x\\\")\"",
            "    return \"label(\\\"x\\\", sep=\\\" \\\")\"",
            file,
            &owners,
        ),
        None
    );
    // A built-in method name may reach the built-in, not a workspace `split`.
    let split = "def split(sep, maxsplit=1):\n    return sep\n\n\ndef last(text):\n    return text.split(\",\", maxsplit=1)[-1]\n";
    assert_eq!(
        default_equal_keyword(
            "    return text.split(\",\")[-1]",
            "    return text.split(\",\", maxsplit=1)[-1]",
            file,
            &clock_owners(split),
        ),
        None
    );
    // A positional argument already binds the parameter.
    let send =
        "def send(user, tag=1):\n    return user\n\n\ndef go():\n    return send(3, 0, tag=1)\n";
    assert_eq!(
        default_equal_keyword(
            "    return send(3, 0)",
            "    return send(3, 0, tag=1)",
            file,
            &clock_owners(send),
        ),
        None
    );
    // A bare call to a function defined only in another file is not resolved.
    assert_eq!(
        default_equal_keyword(
            "    return tick()",
            "    return tick(count=1)",
            Path::new("src/other.py"),
            &clock_owners(empty),
        ),
        None
    );
}
