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

#[test]
fn owner_used_as_callback_or_alias_is_not_a_no_call_path() -> Result<(), String> {
    for body in [
        "def test_score_map():\n    assert list(map(score, [0, 1])) == [8, 8]\n",
        "def test_score_alias():\n    f = score\n    assert [f(0)] == [8]\n",
    ] {
        let finding = score_case(body)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        assert_ne!(
            finding.related_tests[0].miss,
            Some(RelatedTestMiss::NoCallPath),
            "{body}"
        );
    }
    // Positive control: an alias that is never used again is still no call.
    let finding = score_case("def test_score_alias_unused():\n    f = score\n    assert 8 == 8\n")?;
    assert_eq!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::NoCallPath)
    );
    Ok(())
}

#[test]
fn off_boundary_strong_predicate_row_is_never_missing_input_here() -> Result<(), String> {
    // `missing_input` is pending follow-up (#5491): the row keeps only the
    // weaker `observation_unconfirmed`, or no reason, never a boundary claim.
    let finding = classify(
        "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n",
        "from src.subject import can_reserve\n\ndef test_off_boundary():\n    assert can_reserve(5, 3) == True\n",
        2,
        "    if qty > on_hand:",
        "    if qty >= on_hand:",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_ne!(
        finding.related_tests[0].miss,
        Some(RelatedTestMiss::MissingInput)
    );
    Ok(())
}

#[test]
fn medium_strength_mock_row_keeps_no_miss_in_a_weak_finding() -> Result<(), String> {
    let finding = score_case(
        "def test_mock():\n    cb = Mock()\n    score(cb)\n    cb.assert_called_once_with(1)\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(miss_of(&finding, "test_mock"), None);
    // Discriminating control: the weak sibling still carries its own reason.
    assert_eq!(
        miss_of(&finding, "test_weak"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

#[test]
fn credited_strong_row_keeps_no_miss_beside_a_weak_row() -> Result<(), String> {
    let finding = score_case(
        "def test_exact():\n    assert score(0) == 8\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    // A credited strong row exposes the finding; no row then carries a miss,
    // including the weak sibling (the class gate, not the row, decides).
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(miss_of(&finding, "test_exact"), None);
    assert_eq!(miss_of(&finding, "test_weak"), None);
    Ok(())
}

#[test]
fn helper_owner_call_row_keeps_no_miss_inside_a_weak_finding() -> Result<(), String> {
    // The test reaches the helper through its same-module caller: a path
    // exists, and the row has no oracle, so no reason is established.
    let finding = classify(
        "def _validate_epoch(epoch):\n    if epoch >= 0:\n        return epoch\n    raise ValueError('bad')\n\n\nclass Version:\n    def __init__(self, text):\n        self.epoch = _validate_epoch(int(text))\n",
        "from src.subject import Version\n\n\ndef test_epoch_zero():\n    assert Version('0').epoch == 0\n",
        2,
        "    if epoch > 0:",
        "    if epoch >= 0:",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].miss, None);
    Ok(())
}

// Direct reads of the pass, so a gate is exercised independently of the class
// the classifier would have chosen.
mod direct {
    use super::super::assertion_selection::PythonAssertionFocus;
    use super::super::owners_tests::{extract_owners, extract_tests};
    use super::super::probe_shape::classify_probe_shape;
    use super::super::related_tests::{
        PythonRelationKind, related_test_candidates, related_tests_for_candidates,
    };
    use super::super::row_miss::{RowMissChange, annotate_row_misses};
    use super::super::{PythonOwner, PythonTest, owner_for_changed_line};
    use crate::domain::{ExposureClass, RelatedTestMiss};
    use std::path::Path;

    struct Case {
        owner: PythonOwner,
        tests: Vec<PythonTest>,
        before: String,
        after: String,
    }

    fn case(
        source: &str,
        tests: &str,
        line: usize,
        before: &str,
        after: &str,
    ) -> Result<Case, String> {
        case_in(
            "src/subject.py",
            "tests/test_subject.py",
            source,
            tests,
            line,
            before,
            after,
        )
    }

    fn case_in(
        owner_file: &str,
        test_file: &str,
        source: &str,
        tests: &str,
        line: usize,
        before: &str,
        after: &str,
    ) -> Result<Case, String> {
        let file = Path::new(owner_file);
        let owners = extract_owners(file, source);
        let owner = owner_for_changed_line(file, line, &owners)
            .cloned()
            .ok_or_else(|| "changed line must have an owner".to_string())?;
        Ok(Case {
            owner,
            tests: extract_tests(Path::new(test_file), tests),
            before: before.to_string(),
            after: after.to_string(),
        })
    }

    /// The relation of each candidate and the miss the pass assigns under
    /// `class` and `has_static_limit`.
    fn run(
        case: &Case,
        class: &ExposureClass,
        has_static_limit: bool,
    ) -> Vec<(PythonRelationKind, Option<RelatedTestMiss>)> {
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
        let candidates = related_test_candidates(&case.owner, &case.tests);
        let mut rows = related_tests_for_candidates(&candidates, Some(&focus));
        annotate_row_misses(
            &mut rows,
            &candidates,
            class,
            has_static_limit,
            &RowMissChange {
                owner: &case.owner,
                line_text: &case.after,
                old_line_text: Some(&case.before),
            },
            &case.tests,
        );
        candidates
            .iter()
            .zip(&rows)
            .map(|(candidate, row)| (candidate.relation, row.miss))
            .collect()
    }

    fn score_case() -> Result<Case, String> {
        case(
            "def score(value):\n    return 8\n",
            "from src.subject import score\n\ndef test_score():\n    assert score(0)\n",
            2,
            "    return 7",
            "    return 8",
        )
    }

    #[test]
    fn a_static_limit_keeps_every_row_none_even_when_the_class_says_weak() -> Result<(), String> {
        let case = score_case()?;
        // Positive control: without a static limit the same row has its miss.
        assert_eq!(
            run(&case, &ExposureClass::WeaklyExposed, false),
            vec![(
                PythonRelationKind::SyntacticCall,
                Some(RelatedTestMiss::WeakAssertion)
            )]
        );
        let rows = run(&case, &ExposureClass::WeaklyExposed, true);
        assert_eq!(rows.len(), 1);
        assert!(rows.iter().all(|(_, miss)| miss.is_none()), "{rows:?}");
        Ok(())
    }

    #[test]
    fn dunder_protocol_row_is_not_a_no_call_path_and_fixture_name_row_is() -> Result<(), String> {
        let dunder = case(
            "class Box:\n    def __init__(self, v):\n        self.v = v\n    def __eq__(self, other):\n        return self.v == other.v\n",
            "from src.subject import Box\n\ndef test_box():\n    assert Box(1) == Box(1)\n",
            5,
            "        return self.v != other.v",
            "        return self.v == other.v",
        )?;
        let rows = run(&dunder, &ExposureClass::WeaklyExposed, false);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].0, PythonRelationKind::DunderProtocol);
        assert_eq!(rows[0].1, None);

        let fixture = case_in(
            "src/pricing.py",
            "tests/test_checkout.py",
            "def calculate_fee(amount):\n    return amount + 2\n",
            "from src.pricing import calculate_fee\n\n\ndef test_checkout_total(pricing):\n    fee = calculate_fee\n    assert 102 == 102\n",
            2,
            "    return amount + 1",
            "    return amount + 2",
        )?;
        let rows = run(&fixture, &ExposureClass::WeaklyExposed, false);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].0, PythonRelationKind::FixtureName);
        assert_eq!(rows[0].1, Some(RelatedTestMiss::NoCallPath));
        Ok(())
    }

    #[test]
    fn module_constant_owner_row_is_not_a_no_call_path() -> Result<(), String> {
        let constant = case(
            "BASE_DISCOUNT = 8\n",
            "from src.subject import BASE_DISCOUNT\n\ndef test_subject_base_discount():\n    assert BASE_DISCOUNT > 0\n",
            1,
            "BASE_DISCOUNT = 7",
            "BASE_DISCOUNT = 8",
        )?;
        let rows = run(&constant, &ExposureClass::WeaklyExposed, false);
        assert!(!rows.is_empty(), "fixture rows must parse");
        assert!(
            rows.iter()
                .all(|(_, miss)| *miss != Some(RelatedTestMiss::NoCallPath)),
            "{rows:?}"
        );
        // Positive control: a callable owner with the same name-only relation.
        let callable = case(
            "def subject_base_discount():\n    return 8\n",
            "from src.subject import subject_base_discount\n\ndef test_subject_base_discount_exists():\n    assert callable(subject_base_discount)\n",
            2,
            "    return 7",
            "    return 8",
        )?;
        let rows = run(&callable, &ExposureClass::WeaklyExposed, false);
        assert!(
            rows.iter()
                .any(|(_, miss)| *miss == Some(RelatedTestMiss::NoCallPath)),
            "{rows:?}"
        );
        Ok(())
    }

    #[test]
    fn miss_annotation_changes_nothing_but_the_miss_field() -> Result<(), String> {
        let cases = [
            case(
                "def score(value):\n    return 8\n",
                "from src.subject import score\n\ndef test_a():\n    assert score(0)\n\ndef test_score_is_importable():\n    assert callable(score)\n\ndef test_b():\n    assert score(1) == 8\n",
                2,
                "    return 7",
                "    return 8",
            )?,
            case(
                "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n",
                "from src.subject import can_reserve\n\ndef test_off():\n    assert can_reserve(5, 3) == True\n\ndef test_weak():\n    assert can_reserve(4, 4)\n",
                2,
                "    if qty > on_hand:",
                "    if qty >= on_hand:",
            )?,
        ];
        let mut annotated_any = false;
        for case in &cases {
            let family = classify_probe_shape(&case.after).0;
            let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
            let candidates = related_test_candidates(&case.owner, &case.tests);
            let stripped = related_tests_for_candidates(&candidates, Some(&focus));
            assert!(stripped.iter().all(|row| row.miss.is_none()));
            let mut rows = stripped.clone();
            annotate_row_misses(
                &mut rows,
                &candidates,
                &ExposureClass::WeaklyExposed,
                false,
                &RowMissChange {
                    owner: &case.owner,
                    line_text: &case.after,
                    old_line_text: Some(&case.before),
                },
                &case.tests,
            );
            assert_eq!(rows.len(), stripped.len());
            for (row, base) in rows.iter().zip(&stripped) {
                annotated_any |= row.miss.is_some();
                // Order, identity, oracle, relation and confidence are the
                // un-annotated row's; `is_unmatched` stays false.
                let mut reset = row.clone();
                reset.miss = None;
                assert_eq!(&reset, base);
                assert!(!row.is_unmatched());
            }
        }
        assert!(annotated_any, "positive control: some row gains a miss");
        Ok(())
    }

    fn two_row_case() -> Result<Case, String> {
        case(
            "def score(value):\n    return 8\n",
            "from src.subject import score\n\ndef test_a():\n    assert score(0)\n\ndef test_score_is_importable():\n    assert callable(score)\n",
            2,
            "    return 7",
            "    return 8",
        )
    }

    fn base_rows(case: &Case) -> Vec<crate::domain::RelatedTest> {
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
        let candidates = related_test_candidates(&case.owner, &case.tests);
        related_tests_for_candidates(&candidates, Some(&focus))
    }

    fn annotate(case: &Case, rows: &mut [crate::domain::RelatedTest]) {
        let candidates = related_test_candidates(&case.owner, &case.tests);
        annotate_row_misses(
            rows,
            &candidates,
            &ExposureClass::WeaklyExposed,
            false,
            &RowMissChange {
                owner: &case.owner,
                line_text: &case.after,
                old_line_text: Some(&case.before),
            },
            &case.tests,
        );
    }

    #[test]
    fn mismatched_length_or_reordered_rows_annotate_nothing() -> Result<(), String> {
        let case = two_row_case()?;
        // Positive control: aligned rows gain misses.
        let mut aligned = base_rows(&case);
        assert_eq!(aligned.len(), 2);
        annotate(&case, &mut aligned);
        assert!(aligned.iter().any(|row| row.miss.is_some()), "control");

        let mut truncated = base_rows(&case);
        truncated.truncate(1);
        annotate(&case, &mut truncated);
        assert!(truncated.iter().all(|row| row.miss.is_none()));

        let mut reordered = base_rows(&case);
        reordered.reverse();
        annotate(&case, &mut reordered);
        assert!(reordered.iter().all(|row| row.miss.is_none()));
        Ok(())
    }

    #[test]
    fn recognized_oracle_with_unknown_strength_gets_no_miss() -> Result<(), String> {
        let case = two_row_case()?;
        let mut rows = base_rows(&case);
        let row = rows
            .iter_mut()
            .find(|row| row.name == "test_a")
            .ok_or("test_a row")?;
        assert!(row.oracle.is_some());
        row.oracle_strength = crate::domain::OracleStrength::Unknown;
        annotate(&case, &mut rows);
        let row = rows
            .iter()
            .find(|row| row.name == "test_a")
            .ok_or("test_a row")?;
        assert_eq!(row.miss, None);
        Ok(())
    }
}
