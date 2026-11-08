//! Patches around the changed owner (`callee_patch.rs`, #13632): a literal
//! stand-in for a callee the edit neither names nor feeds, and a patched owner
//! that no test runs. Also (`callee_patch.rs`, #13633): a non-mock related test
//! is credited independently of mock-patched siblings; the mock-only set keeps
//! its limit.

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

const OWNER: &str = "def tax_for(amount, region):\n    return amount * 3\n";
const MOCK_TEST: &str = "def test_tax_for_with_mock(monkeypatch):\n    monkeypatch.setattr(\"src.subject.rate_for\", lambda r: other(r))\n    assert tax_for(10, \"x\") == 30\n";
const PLAIN_TEST: &str = "def test_tax_for_plain():\n    assert tax_for(10, \"x\") == 30\n";

fn sibling_class(tests: &[&str]) -> Result<ExposureClass, String> {
    let joined = format!(
        "from unittest import mock\n\nfrom src.subject import tax_for\n\n\n{}",
        tests.join("\n\n")
    );
    classify(
        OWNER,
        &joined,
        2,
        "    return amount * 2",
        "    return amount * 3",
    )
    .map(|finding| finding.class)
}

/// Issue #13633: a non-mock exact test is credited independently of a
/// mock-patched sibling; the mock-only set keeps its limit.
#[test]
fn a_non_mock_test_is_credited_beside_a_mock_patched_sibling() -> Result<(), String> {
    assert_eq!(
        sibling_class(&[MOCK_TEST, PLAIN_TEST])?,
        ExposureClass::Exposed
    );
    assert_eq!(
        sibling_class(&[PLAIN_TEST, MOCK_TEST])?,
        ExposureClass::Exposed
    );
    // `mock.patch` as a context manager, around the owner itself.
    let patched_owner = "def test_tax_for_with_patch():\n    with mock.patch(\"src.subject.tax_for\", return_value=30):\n        assert tax_for(10, \"x\") == 30\n";
    assert_eq!(
        sibling_class(&[patched_owner, PLAIN_TEST])?,
        ExposureClass::Exposed
    );
    Ok(())
}

/// Negative controls (#13633): the mock tests alone, a sibling whose patch may
/// outlive it, and an independent test that does not itself discriminate all
/// keep the limit the mock-patched sibling earned.
#[test]
fn mock_siblings_keep_their_limit_without_an_independent_discriminator() -> Result<(), String> {
    assert_eq!(sibling_class(&[MOCK_TEST])?, ExposureClass::StaticUnknown);
    let second_mock = "def test_tax_for_again(monkeypatch):\n    monkeypatch.setattr(\"src.subject.rate_for\", lambda r: other(r))\n    assert tax_for(10, \"x\") == 30\n";
    assert_eq!(
        sibling_class(&[MOCK_TEST, second_mock])?,
        ExposureClass::StaticUnknown
    );
    // The patch is started and never undone by its test.
    let leaking = "def test_tax_for_leaks():\n    patcher = mock.patch(\"src.subject.rate_for\")\n    patcher.start()\n    assert tax_for(10, \"x\") == 30\n";
    assert_eq!(
        sibling_class(&[leaking, PLAIN_TEST])?,
        ExposureClass::StaticUnknown
    );
    // The independent test calls the owner but pins nothing about the edit.
    let weak = "def test_tax_for_runs():\n    tax_for(10, \"x\")\n";
    assert_ne!(sibling_class(&[MOCK_TEST, weak])?, ExposureClass::Exposed);
    Ok(())
}

/// Negative controls (#13632): the edit's value reaches the patched callee
/// through more than one binding hop, a loop, an unpack, a nested scope or a
/// call spread over lines. Each keeps `mocked_module`.
#[test]
fn an_edit_that_reaches_the_patched_callee_transitively_keeps_the_limit() -> Result<(), String> {
    let bodies: [&str; 8] = [
        // Alias chain.
        "    base = amount * 3\n    key = base + 1\n    return rate_for(key)\n",
        "    base = amount * 3\n    first = base\n    second = first\n    return rate_for(second)\n",
        // Loop variable.
        "    base = amount * 3\n    total = 0\n    for step in [base, 1]:\n        total = step\n    return rate_for(total)\n",
        // Tuple unpack.
        "    base = amount * 3\n    first, second = base, 1\n    return rate_for(second)\n",
        // Nested def default argument.
        "    base = amount * 3\n\n    def inner(a=rate_for(base)):\n        return a\n\n    return inner()\n",
        // Nested def whose default reads the edit, called with the default.
        "    base = amount * 3\n\n    def inner(a=base):\n        return rate_for(a)\n\n    return inner()\n",
        // Nested def body that reads the tainted name.
        "    base = amount * 3\n\n    def inner():\n        return rate_for(base)\n\n    return inner()\n",
        // Call spread over lines.
        "    base = amount * 3\n    key = base + 1\n    return rate_for(\n        key,\n    )\n",
    ];
    for body in bodies {
        let source = format!(
            "def rate_for(region):\n    return 1\n\n\ndef tax_for(amount, region):\n{body}"
        );
        let line = source
            .lines()
            .position(|text| text == BASE_NEW)
            .map(|index| index + 1)
            .ok_or_else(|| format!("edited line in {source}"))?;
        let finding = classify(
            &source,
            &caller_test(PATCH_RATE, "60"),
            line,
            BASE_OLD,
            BASE_NEW,
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::StaticUnknown,
            "{source}\n{finding:?}"
        );
    }
    Ok(())
}
