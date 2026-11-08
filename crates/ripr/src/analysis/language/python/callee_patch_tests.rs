//! A non-mock related test is credited independently of mock-patched siblings
//! (`callee_patch.rs`, #13633); the mock-only set keeps its limit.

use super::classify::classify_change_with_source;
use super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

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
