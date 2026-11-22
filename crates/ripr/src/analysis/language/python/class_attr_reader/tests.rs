//! #16009: a changed non-string class attribute is exposed only when a
//! related test reads the name or calls a class method that reads it.

use super::super::classify::classify_change_with_source;
use super::super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

const CLOCK: &str = "class Clock:\n    _PER_YEAR = 13\n    _PER_DAY = 24\n\n    def __init__(self, hours):\n        self.hours = hours\n\n    def days(self):\n        return self.hours // self._PER_DAY\n\n    def years(self, months):\n        return months // self._PER_YEAR\n\n    def total(self, months):\n        return self.years(months) + 1\n";
const OLD_LINE: &str = "    _PER_YEAR = 12";
const NEW_LINE: &str = "    _PER_YEAR = 13";
const DAYS_TEST: &str =
    "from clock import Clock\n\n\ndef test_days():\n    assert Clock(48).days() == 2\n";

fn classify(source: &str, line: usize, tests: &str) -> Result<Finding, String> {
    let file = Path::new("clock.py");
    let owners = extract_owners(file, source);
    let tests = extract_tests(Path::new("tests/test_clock.py"), tests);
    classify_change_with_source(
        file,
        line,
        NEW_LINE,
        Some(OLD_LINE),
        &owners,
        &tests,
        Some(source),
    )
    .ok_or_else(|| "class attribute change must produce a finding".to_string())
}

#[test]
fn class_attribute_value_unread_by_related_tests_is_weakly_exposed() -> Result<(), String> {
    let finding = classify(CLOCK, 2, DAYS_TEST)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    let missing = finding
        .activation
        .missing_discriminators
        .iter()
        .find(|missing| missing.value == "class attribute `_PER_YEAR` value `13`")
        .ok_or_else(|| format!("missing discriminator expected: {finding:?}"))?;
    assert!(
        missing.reason.contains("(`total`, `years`)") && missing.reason.contains("`12` -> `13`"),
        "{}",
        missing.reason
    );
    Ok(())
}

#[test]
fn class_attribute_value_stays_exposed_when_a_test_reads_it() -> Result<(), String> {
    for tests in [
        // A direct reader.
        "from clock import Clock\n\n\ndef test_years():\n    assert Clock(1).years(24) == 2\n",
        // A same-class caller of a reader.
        "from clock import Clock\n\n\ndef test_total():\n    assert Clock(1).total(24) == 3\n",
        // The name itself, from a test that never builds the class.
        "from clock import Clock\n\n\ndef test_days():\n    assert Clock(48).days() == 2\n\n\ndef test_attr():\n    assert Clock._PER_YEAR == 12\n",
    ] {
        let finding = classify(CLOCK, 2, tests)?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{tests}: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn class_attribute_value_fails_open_when_readers_are_not_all_in_the_class() -> Result<(), String> {
    let cases = [
        // Read on construction.
        CLOCK.replace("self.hours = hours", "self.hours = hours * self._PER_YEAR"),
        // A derived class constant reads it outside any method.
        CLOCK.replace(
            "    _PER_DAY = 24\n",
            "    _PER_DAY = 24\n    _TWICE = _PER_YEAR * 2\n",
        ),
        // A dunder calls a reader.
        format!("{CLOCK}\n    def __str__(self):\n        return str(self.years(12))\n"),
        // A decorated class (dataclass-like fields).
        format!("@decorate\n{CLOCK}"),
        // A base class can hold more readers.
        CLOCK.replace("class Clock:", "class Clock(Base):"),
        // Same-file code calls a reader on an instance the test never names.
        format!("{CLOCK}\n\ndef yearly(clock):\n    return clock.years(36)\n"),
    ];
    for source in &cases {
        let line = if source.starts_with('@') { 3 } else { 2 };
        let finding = classify(source, line, DAYS_TEST)?;
        // Fail open: the earlier verdict stands, and this gate adds nothing.
        assert_ne!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{source}: {finding:?}"
        );
        assert!(
            !finding
                .activation
                .missing_discriminators
                .iter()
                .any(|missing| missing.value.starts_with("class attribute `_PER_YEAR`")),
            "{source}: {finding:?}"
        );
    }
    Ok(())
}
