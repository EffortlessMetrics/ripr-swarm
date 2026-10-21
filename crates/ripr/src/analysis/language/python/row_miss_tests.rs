//! Row-local typed misses for Python related tests (#5491, RIPR-SPEC-0224).
//!
//! Every positive control has a discriminating negative: the same row shape
//! outside a concrete gap, or with the row's own fact unresolved, stays
//! `miss: None`. `miss` is explanation only, so each case also pins that the
//! class is the one the classifier chose without it.

use super::classify::classify_change_with_old;
use super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, Finding, RelatedTestMiss};
use std::path::Path;

fn classify(
    source: &str,
    tests: &str,
    line: usize,
    before: &str,
    after: &str,
) -> Result<Finding, String> {
    let file = Path::new("src/subject.py");
    let owners = extract_owners(file, source);
    let tests = extract_tests(Path::new("tests/test_subject.py"), tests);
    classify_change_with_old(file, line, after, Some(before), &owners, &tests)
        .ok_or_else(|| "behavioral fixture must produce a finding".to_string())
}

fn misses(finding: &Finding) -> Vec<(String, Option<RelatedTestMiss>)> {
    finding
        .related_tests
        .iter()
        .map(|row| (row.name.clone(), row.miss))
        .collect()
}

fn miss_of(finding: &Finding, name: &str) -> Option<RelatedTestMiss> {
    let rows: Vec<_> = finding
        .related_tests
        .iter()
        .filter(|row| row.name == name)
        .collect();
    assert_eq!(rows.len(), 1, "one row named {name}: {:?}", misses(finding));
    rows[0].miss
}

const SCORE: &str = "def score(value):\n    return 8\n";
const SCORE_IMPORT: &str = "from src.subject import score\n";

fn score_case(tests: &str) -> Result<Finding, String> {
    classify(
        SCORE,
        &format!("{SCORE_IMPORT}{tests}"),
        2,
        "    return 7",
        "    return 8",
    )
}

#[test]
fn weak_family_selected_assertion_is_weak_assertion() -> Result<(), String> {
    let finding = score_case("def test_score():\n    assert score(0)\n")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_score"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

#[test]
fn exposed_finding_keeps_every_row_miss_none_even_beside_a_weak_row() -> Result<(), String> {
    let finding = score_case(
        "def test_exact():\n    assert score(0) == 8\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.related_tests.len(), 2, "{:?}", misses(&finding));
    assert!(
        finding.related_tests.iter().all(|row| row.miss.is_none()),
        "{:?}",
        misses(&finding)
    );
    Ok(())
}

#[test]
fn heuristic_only_rows_are_no_call_path_in_a_concrete_gap() -> Result<(), String> {
    // Referenced but never called: related by file stem only.
    let finding = score_case("def test_score_is_importable():\n    assert callable(score)\n")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::NoCallPath)
    );
    Ok(())
}

#[test]
fn mixed_rows_keep_their_own_reasons() -> Result<(), String> {
    let finding = score_case(
        "def test_score_is_importable():\n    assert callable(score)\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_score_is_importable"),
        Some(RelatedTestMiss::NoCallPath)
    );
    assert_eq!(
        miss_of(&finding, "test_weak"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

#[test]
fn no_assertion_is_not_emitted_while_cross_file_blind_spots_remain() -> Result<(), String> {
    // #6657: a conftest autouse fixture or plugin may still assert around
    // this test, so `extraction_complete_no_assertion` is not yet a proof.
    for tests in [
        "def test_score():\n    score(0)\n",
        "import pytest\n\n@pytest.mark.parametrize('value', [0, 1])\ndef test_score(value):\n    score(value)\n",
    ] {
        let finding = score_case(tests)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{tests}");
        assert_eq!(finding.related_tests.len(), 1, "{tests}");
        let row = &finding.related_tests[0];
        assert_eq!(row.miss, None, "{tests}");
        assert_eq!(row.oracle, None, "{tests}");
    }
    Ok(())
}

#[test]
fn unsupported_custom_assertion_form_gets_no_fabricated_miss() -> Result<(), String> {
    let finding = score_case(
        "def check(result):\n    assert result == 8\n\ndef test_score():\n    check(score(0))\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_score"),
        None,
        "{:?}",
        misses(&finding)
    );
    Ok(())
}

#[test]
fn skipped_and_xfail_rows_gain_no_miss() -> Result<(), String> {
    for decorator in ["pytest.mark.skip(reason='off')", "pytest.mark.xfail"] {
        let finding = score_case(&format!(
            "import pytest\n\n@{decorator}\ndef test_score():\n    assert score(0) == 8\n"
        ))?;
        // #5389: the class is the classifier's, and no row is credited a miss.
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{decorator}");
        assert!(
            finding.related_tests.iter().all(|row| row.miss.is_none()),
            "{decorator}: {:?}",
            misses(&finding)
        );
    }
    Ok(())
}

#[test]
fn strong_assertion_that_this_row_does_not_credit_is_observation_unconfirmed() -> Result<(), String>
{
    let finding = score_case(
        "from src.subject import other\n\ndef test_score():\n    score(0)\n    assert other(1) == 2\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // Never an affirmative mismatch: Python alignment is credit-only.
    assert_eq!(
        miss_of(&finding, "test_score"),
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    Ok(())
}

const RESERVE: &str = "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n";

fn reserve_case(tests: &str) -> Result<Finding, String> {
    classify(
        RESERVE,
        &format!("from src.subject import can_reserve\n{tests}"),
        2,
        "    if qty > on_hand:",
        "    if qty >= on_hand:",
    )
}

#[test]
fn off_boundary_strong_row_is_missing_input_and_a_weak_sibling_is_not() -> Result<(), String> {
    let finding = reserve_case(
        "def test_off_boundary():\n    assert can_reserve(5, 3) == True\n\ndef test_weak_on_boundary():\n    assert can_reserve(4, 4)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_off_boundary"),
        Some(RelatedTestMiss::MissingInput)
    );
    // The weak sibling sits on the boundary but earns only its own reason; it
    // neither lends its input to the strong row nor takes missing_input.
    assert_eq!(
        miss_of(&finding, "test_weak_on_boundary"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

#[test]
fn unresolved_row_inside_a_boundary_gap_does_not_borrow_a_sibling_fact() -> Result<(), String> {
    let finding = reserve_case(
        "def test_off_boundary():\n    assert can_reserve(5, 3) == True\n\ndef test_unresolved(n):\n    assert can_reserve(n, 3) == True\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_off_boundary"),
        Some(RelatedTestMiss::MissingInput)
    );
    assert_eq!(
        miss_of(&finding, "test_unresolved"),
        None,
        "{:?}",
        misses(&finding)
    );
    Ok(())
}

#[test]
fn boundary_input_row_makes_the_finding_exposed_with_no_row_miss() -> Result<(), String> {
    let finding = reserve_case(
        "def test_off_boundary():\n    assert can_reserve(5, 3) == True\n\ndef test_on_boundary():\n    assert can_reserve(4, 4) == True\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert!(
        finding.related_tests.iter().all(|row| row.miss.is_none()),
        "{:?}",
        misses(&finding)
    );
    Ok(())
}
