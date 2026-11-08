//! Patches around the changed owner (`callee_patch.rs`, #13632): a literal
//! stand-in for a callee the edit neither names nor feeds, and a patched owner
//! that no test runs.

use super::classify::classify_change_with_source;
use super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

const SOURCE: &str = "def rate_for(region):\n    return 1\n\n\ndef tax_for(amount, region):\n    base = amount * 2\n    return base * rate_for(region)\n";

const BASE_OLD: &str = "    base = amount * 2";
const BASE_NEW: &str = "    base = amount * 3";

/// The finding for the change of `line` (1-based) from `before` to `after`
/// in `SOURCE`-shaped `source`, under `tests`.
fn classify(
    source: &str,
    tests: &str,
    line: usize,
    before: &str,
    after: &str,
) -> Result<Finding, String> {
    let file = Path::new("src/subject.py");
    let owners = extract_owners(file, source);
    let extracted = extract_tests(Path::new("tests/test_subject.py"), tests);
    assert!(!extracted.is_empty(), "tests must parse: {tests}");
    classify_change_with_source(
        file,
        line,
        after,
        Some(before),
        &owners,
        &extracted,
        Some(source),
    )
    .ok_or_else(|| "behavioral fixture must produce a finding".to_string())
}

fn caller_test(patch: &str, expected: &str) -> String {
    format!(
        "from src.subject import tax_for\n\n\ndef test_tax_for(monkeypatch):\n    {patch}\n    assert tax_for(10, \"x\") == {expected}\n"
    )
}

const PATCH_RATE: &str = "monkeypatch.setattr(\"src.subject.rate_for\", lambda r: 2)";

/// Positive control (#13632): the callee is replaced by a nonzero literal, the
/// edit sits on a line that neither names nor feeds it, and the exact
/// assertion reads the edit.
#[test]
fn a_literal_stand_in_for_a_callee_the_edit_does_not_feed_keeps_the_credit() -> Result<(), String> {
    let finding = classify(
        &SOURCE.replace(BASE_OLD, BASE_NEW),
        &caller_test(PATCH_RATE, "60"),
        6,
        BASE_OLD,
        BASE_NEW,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// Negative controls: every way the edit can hide behind the stand-in, or the
/// stand-in can stop being a plain literal, keeps `mocked_module`.
#[test]
fn an_edit_that_can_hide_behind_the_stand_in_keeps_the_limit() -> Result<(), String> {
    let source = SOURCE.replace(BASE_OLD, BASE_NEW);
    let cases: Vec<(String, String, &str, &str)> = vec![
        // The edit is the patched call's argument.
        (
            SOURCE.replace(
                "    return base * rate_for(region)",
                "    return base * rate_for(region.upper())",
            ),
            caller_test(PATCH_RATE, "40"),
            "    return base * rate_for(region)",
            "    return base * rate_for(region.upper())",
        ),
        // The edit binds a name the patched call receives.
        (
            "def rate_for(region):\n    return 1\n\n\ndef tax_for(amount, region):\n    key = region + \"b\"\n    return amount * rate_for(key)\n".to_string(),
            caller_test(PATCH_RATE, "20"),
            "    key = region + \"a\"",
            "    key = region + \"b\"",
        ),
        // The stand-in is zero, which absorbs a product.
        (
            source.clone(),
            caller_test("monkeypatch.setattr(\"src.subject.rate_for\", lambda r: 0)", "0"),
            BASE_OLD,
            BASE_NEW,
        ),
        // The patch targets another module, or a non-literal stand-in.
        (
            source.clone(),
            caller_test("monkeypatch.setattr(\"other.rate_for\", lambda r: 2)", "60"),
            BASE_OLD,
            BASE_NEW,
        ),
        (
            source.clone(),
            caller_test("monkeypatch.setattr(\"src.subject.rate_for\", lambda r: other(r))", "60"),
            BASE_OLD,
            BASE_NEW,
        ),
        // The patch replaces the owner itself.
        (
            source.clone(),
            caller_test("monkeypatch.setattr(\"src.subject.tax_for\", lambda a, r: 60)", "60"),
            BASE_OLD,
            BASE_NEW,
        ),
        // A second patch is not a literal stand-in.
        (
            source.clone(),
            caller_test(
                &format!("{PATCH_RATE}\n    monkeypatch.setattr(\"src.subject.rate_for\", print)"),
                "60",
            ),
            BASE_OLD,
            BASE_NEW,
        ),
        // The edit calls something that may reach the patched name.
        (
            source.replace(BASE_NEW, "    base = helper(amount)"),
            caller_test(PATCH_RATE, "60"),
            BASE_OLD,
            "    base = helper(amount)",
        ),
        // The pytest fixture is rebound before it patches.
        (
            source.clone(),
            caller_test(&format!("monkeypatch = Fake()\n    {PATCH_RATE}"), "60"),
            BASE_OLD,
            BASE_NEW,
        ),
    ];
    for (changed_source, tests, before, after) in cases {
        let line = changed_source
            .lines()
            .position(|text| text == after)
            .map(|index| index + 1)
            .ok_or_else(|| format!("changed line {after} in {changed_source}"))?;
        let finding = classify(&changed_source, &tests, line, before, after)?;
        assert_eq!(
            finding.class,
            ExposureClass::StaticUnknown,
            "{tests}\n{finding:?}"
        );
    }
    Ok(())
}

/// The change inside the patched function (#13632): every related test
/// replaces it, so none runs the changed code. Reached, unrevealed, and never
/// credited from the stand-in.
#[test]
fn a_change_inside_the_patched_callee_reads_reached_but_unrevealed() -> Result<(), String> {
    let source = "def rate_for(region):\n    return 3\n\n\ndef tax_for(amount, region):\n    return amount * rate_for(region)\n";
    let finding = classify(
        source,
        &caller_test(PATCH_RATE, "20"),
        2,
        "    return 1",
        "    return 3",
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{finding:?}"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|text| text.contains("replaces `rate_for`")),
        "{finding:?}"
    );
    // A patch of some other name still reads as an ordinary heuristic link.
    let other = caller_test(
        "monkeypatch.setattr(\"src.subject.unrelated\", lambda r: 2)",
        "20",
    );
    let finding = classify(source, &other, 2, "    return 1", "    return 3")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    // A patch under another module does not replace this owner.
    let foreign = caller_test("monkeypatch.setattr(\"other.rate_for\", lambda r: 2)", "20");
    let finding = classify(source, &foreign, 2, "    return 1", "    return 3")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// Negative control (#13632): a test that patches the owner and asserts the
/// stand-in's value is never a discriminator for the owner's change.
#[test]
fn a_patched_owner_asserted_through_its_stand_in_is_not_credited() -> Result<(), String> {
    let source = "def rate_for(region):\n    return 3\n";
    let tests = "from src.subject import rate_for\n\n\ndef test_rate_for(monkeypatch):\n    monkeypatch.setattr(\"src.subject.rate_for\", lambda r: 2)\n    assert rate_for(\"x\") == 2\n";
    let finding = classify(source, tests, 2, "    return 1", "    return 3")?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    Ok(())
}
