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
fn name_or_location_only_rows_never_get_no_call_path() -> Result<(), String> {
    // Referenced but never called: related by file stem only. The test body
    // cannot prove it never reaches the owner (callbacks, aliases, parametrize
    // and fixture `params`, `usefixtures`, `pytestmark`, conftest, plugins), so
    // the claim is withheld (#6657-class blind spots).
    let finding = score_case("def test_score_is_importable():\n    assert callable(score)\n")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].miss, None);
    Ok(())
}

#[test]
fn mixed_rows_keep_their_own_reasons() -> Result<(), String> {
    let finding = score_case(
        "def test_score_is_importable():\n    assert callable(score)\n\ndef test_weak():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(miss_of(&finding, "test_score_is_importable"), None);
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
fn several_strong_rows_are_aligned_independently() -> Result<(), String> {
    // Two strong rows asserting on another function, plus a weak row: each row
    // is aligned on its own from the shared prepared state.
    let finding = score_case(
        "from src.subject import other\n\ndef test_b():\n    score(0)\n    assert other(1) == 2\n\ndef test_c():\n    score(0)\n    assert other(2) == 3\n\ndef test_d():\n    assert score(1)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        miss_of(&finding, "test_b"),
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    assert_eq!(
        miss_of(&finding, "test_c"),
        Some(RelatedTestMiss::ObservationUnconfirmed)
    );
    assert_eq!(
        miss_of(&finding, "test_d"),
        Some(RelatedTestMiss::WeakAssertion)
    );
    Ok(())
}

#[test]
fn hidden_call_shapes_never_get_no_call_path() -> Result<(), String> {
    for body in [
        "def test_score_map():\n    assert list(map(score, [0, 1])) == [8, 8]\n",
        "def test_score_alias():\n    f = score\n    assert [f(0)] == [8]\n",
        "from src.subject import score as s\n\ndef test_alias_map():\n    assert list(map(s, [0, 1])) == [8, 8]\n",
        "import pytest\n\n@pytest.mark.parametrize('cb', [score])\ndef test_score(cb):\n    assert callable(score)\n    assert cb(0) == 8\n",
        "import pytest\n\n@pytest.fixture(params=[score])\ndef cb(request):\n    return request.param\n\ndef test_score_param(cb):\n    assert callable(score)\n    assert cb(0) == 8\n",
        "import pytest\n\n@pytest.fixture\ndef setup():\n    return score(0)\n\n@pytest.mark.usefixtures('setup')\ndef test_score_setup():\n    assert callable(score)\n",
        "import pytest\n\npytestmark = pytest.mark.parametrize('cb', [score])\n\ndef test_score(cb):\n    assert callable(score)\n    assert cb(0) == 8\n",
        "import pytest\n\npytestmark = [pytest.mark.parametrize('cb', [score])]\n\ndef test_score(cb):\n    assert callable(score)\n    assert cb(0) == 8\n",
        "def test_nested_call():\n    class Box:\n        __call__ = score\n    assert Box()(0) == 8\n",
        "def test_kw():\n    assert run(\n        cb = score\n    ) == 8\n",
        "def test_unused_alias():\n    cb = score\n    assert 8 == 8\n",
    ] {
        let finding = score_case(body)?;
        assert!(!finding.related_tests.is_empty(), "{body}");
        assert!(
            finding
                .related_tests
                .iter()
                .all(|row| row.miss != Some(RelatedTestMiss::NoCallPath)),
            "{body}: {:?}",
            misses(&finding)
        );
    }
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
    use super::super::boundary::python_row_boundary_omission;
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
                focus: &focus,
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
    fn dunder_protocol_and_fixture_name_rows_get_no_no_call_path() -> Result<(), String> {
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
        assert_eq!(rows[0].1, None);
        Ok(())
    }

    #[test]
    fn module_constant_owner_row_gets_no_no_call_path() -> Result<(), String> {
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
        Ok(())
    }

    fn omission(case: &Case, index: usize) -> bool {
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
        let candidates = related_test_candidates(&case.owner, &case.tests);
        python_row_boundary_omission(
            &case.after,
            Some(&case.before),
            &case.owner,
            &candidates[index],
            &candidates,
            &focus,
        )
    }

    #[test]
    fn same_name_function_from_another_module_is_not_an_owner_call() -> Result<(), String> {
        let source =
            "def bulk(quantity):\n    if quantity > 100:\n        return 1\n    return 0\n";
        let before = "    if quantity >= 100:";
        let after = "    if quantity > 100:";
        // Positive control: the import resolves to the owner's module.
        let resolved = case_in(
            "src/pricing/legacy.py",
            "tests/test_bulk.py",
            source,
            "from pricing.legacy import bulk\n\ndef test_bulk():\n    assert bulk(101) == 1\n",
            2,
            before,
            after,
        )?;
        assert!(
            omission(&resolved, 0),
            "import-resolved call is an owner call"
        );
        // Same name, different module: not the owner's call.
        let other = case_in(
            "src/pricing/legacy.py",
            "tests/test_bulk.py",
            source,
            "from pricing.discounts import bulk\n\ndef test_bulk():\n    assert bulk(101) == 1\n",
            2,
            before,
            after,
        )?;
        for index in 0..related_test_candidates(&other.owner, &other.tests).len() {
            assert!(!omission(&other, index), "row {index}");
        }
        Ok(())
    }

    #[test]
    fn non_predicate_family_never_gets_missing_input() -> Result<(), String> {
        let case = case(
            "def score(value):\n    return 8\n",
            "from src.subject import score\n\ndef test_score():\n    assert score(0) == 9\n",
            2,
            "    return 7",
            "    return 8",
        )?;
        let rows = run(&case, &ExposureClass::WeaklyExposed, false);
        assert!(!rows.is_empty());
        assert!(
            rows.iter()
                .all(|(_, miss)| *miss != Some(RelatedTestMiss::MissingInput)),
            "{rows:?}"
        );
        Ok(())
    }

    #[test]
    fn single_row_unresolved_or_computed_operand_gets_no_missing_input() -> Result<(), String> {
        let source = "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n";
        for body in [
            "def test_unresolved(n):\n    assert can_reserve(n, 3) == True\n",
            "def test_computed():\n    assert can_reserve(2 + 3, 3) == True\n",
        ] {
            let case = case(
                source,
                &format!("from src.subject import can_reserve\n{body}"),
                2,
                "    if qty > on_hand:",
                "    if qty >= on_hand:",
            )?;
            let rows = run(&case, &ExposureClass::WeaklyExposed, false);
            assert_eq!(rows.len(), 1, "{body}");
            assert_ne!(rows[0].1, Some(RelatedTestMiss::MissingInput), "{body}");
        }
        Ok(())
    }

    #[test]
    fn old_equality_operands_count_as_the_boundary() -> Result<(), String> {
        // `level == 2` -> `level == 3`: a call at 2 or 3 separates old from new.
        let case = case(
            "def is_pro(level):\n    if level == 3:\n        return True\n    return False\n",
            "from src.subject import is_pro\n\ndef test_a_off_both():\n    assert is_pro(1) == False\n\ndef test_b_on_old():\n    assert is_pro(2) == False\n\ndef test_c_on_new():\n    assert is_pro(3) == True\n",
            2,
            "    if level == 2:",
            "    if level == 3:",
        )?;
        let candidates = related_test_candidates(&case.owner, &case.tests);
        let mut names: Vec<_> = candidates.iter().map(|c| c.test.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["test_a_off_both", "test_b_on_old", "test_c_on_new"],
            "fixture rows must parse"
        );
        let index = |name: &str| candidates.iter().position(|c| c.test.name == name);
        let at = |name: &str| index(name).ok_or_else(|| format!("missing {name}"));
        assert!(omission(&case, at("test_a_off_both")?), "off both pairs");
        assert!(
            !omission(&case, at("test_b_on_old")?),
            "a call on the OLD pair separates old from new"
        );
        assert!(
            !omission(&case, at("test_c_on_new")?),
            "a call on the NEW pair"
        );
        Ok(())
    }

    const RES: &str = "def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n";
    const RES_IMPORT: &str = "from src.subject import can_reserve\n";

    fn reserve(tests: &str) -> Result<Case, String> {
        case(
            RES,
            &format!("{RES_IMPORT}{tests}"),
            2,
            "    if qty > on_hand:",
            "    if qty >= on_hand:",
        )
    }

    /// No row of `case` may claim `missing_input`, directly or via the pass.
    fn assert_no_missing_input(case: &Case, label: &str) {
        let count = related_test_candidates(&case.owner, &case.tests).len();
        assert!(count > 0, "{label}: fixture rows must parse");
        for index in 0..count {
            assert!(!omission(case, index), "{label}: row {index} omission");
        }
        let rows = run(case, &ExposureClass::WeaklyExposed, false);
        assert!(
            rows.iter()
                .all(|(_, miss)| *miss != Some(RelatedTestMiss::MissingInput)),
            "{label}: {rows:?}"
        );
    }

    #[test]
    fn positive_control_off_boundary_direct_call_is_an_omission() -> Result<(), String> {
        let case = reserve("def test_a():\n    assert can_reserve(5, 3) == True\n")?;
        assert!(omission(&case, 0));
        Ok(())
    }

    #[test]
    fn unreadable_old_equality_operand_is_not_an_omission() -> Result<(), String> {
        let source =
            "def is_pro(level):\n    if level == 3:\n        return True\n    return False\n";
        let tests =
            "from src.subject import is_pro\n\ndef test_a():\n    assert is_pro(2) == False\n";
        let readable = case(source, tests, 2, "    if level == 2:", "    if level == 3:")?;
        // Control: with a readable old pair, 2 sits on the OLD boundary.
        assert!(!omission(&readable, 0));
        let unreadable = case(
            source,
            tests,
            2,
            "    if level == 2 * 1:",
            "    if level == 3:",
        )?;
        assert_no_missing_input(&unreadable, "old operand `2 * 1`");
        Ok(())
    }

    #[test]
    fn indirect_or_aliased_owner_reach_is_not_an_omission() -> Result<(), String> {
        for (label, tests) in [
            (
                "helper in the test file",
                "def helper():\n    return can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n    assert helper() == True\n",
            ),
            (
                "map reference",
                "def test_a():\n    assert can_reserve(5, 3) == True\n    assert list(map(can_reserve, [4], [4])) == [True]\n",
            ),
            (
                "functools.partial",
                "import functools\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n    g = functools.partial(can_reserve, 4)\n    assert g(4) == True\n",
            ),
            (
                "alias",
                "def test_a():\n    f = can_reserve\n    assert can_reserve(5, 3) == True\n    assert f(4, 4) == True\n",
            ),
            (
                "fixture parameter",
                "import pytest\n\n@pytest.fixture\ndef reserver():\n    return can_reserve\n\ndef test_a(reserver):\n    assert can_reserve(5, 3) == True\n    assert reserver(4, 4) == True\n",
            ),
        ] {
            assert_no_missing_input(&reserve(tests)?, label);
        }
        Ok(())
    }

    #[test]
    fn owner_that_rebinds_a_compared_parameter_is_not_an_omission() -> Result<(), String> {
        let source = "def can_reserve(qty, on_hand):\n    qty = qty - 2\n    if qty >= on_hand:\n        return True\n    return False\n";
        let tests = "from src.subject import can_reserve\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n";
        let rebound = case(
            source,
            tests,
            3,
            "    if qty > on_hand:",
            "    if qty >= on_hand:",
        )?;
        assert_no_missing_input(&rebound, "rebound qty");
        // Control: the same call against a body that never rebinds is an omission.
        assert!(omission(
            &reserve("def test_a():\n    assert can_reserve(5, 3) == True\n")?,
            0
        ));
        Ok(())
    }

    #[test]
    fn mixed_kind_or_escaped_literals_are_unknown_not_unequal() -> Result<(), String> {
        let flag = "def f(flag):\n    if flag == 1:\n        return True\n    return False\n";
        for (label, tests) in [
            (
                "True vs 1",
                "from src.subject import f\n\ndef test_a():\n    assert f(True) == True\n",
            ),
            (
                "False vs 0",
                "from src.subject import f\n\ndef test_a():\n    assert f(False) == False\n",
            ),
        ] {
            let case = case(flag, tests, 2, "    if flag == 0:", "    if flag == 1:")?;
            assert_no_missing_input(&case, label);
        }
        let name = "def f(name):\n    if name == 'a':\n        return True\n    return False\n";
        let escaped = case(
            name,
            "from src.subject import f\n\ndef test_a():\n    assert f('\\x61') == True\n",
            2,
            "    if name == 'b':",
            "    if name == 'a':",
        )?;
        assert_no_missing_input(&escaped, "escaped string");
        // Control: two plain strings that differ are an omission.
        let plain = case(
            name,
            "from src.subject import f\n\ndef test_a():\n    assert f('z') == False\n",
            2,
            "    if name == 'a':",
            "    if name == 'a':",
        )?;
        assert!(omission(&plain, 0));
        Ok(())
    }

    #[test]
    fn method_call_on_another_receiver_is_not_an_owner_call() -> Result<(), String> {
        let source = "class Cart:\n    def can_reserve(self, qty, on_hand):\n        if qty >= on_hand:\n            return True\n        return False\n\nclass Other:\n    def can_reserve(self, qty, on_hand):\n        return True\n";
        let build = |body: &str| {
            case(
                source,
                &format!("from src.subject import Cart, Other\n\n{body}"),
                3,
                "        if qty > on_hand:",
                "        if qty >= on_hand:",
            )
        };
        for (label, body) in [
            (
                "other class",
                "def test_a():\n    assert Other().can_reserve(5, 3) == True\n",
            ),
            (
                "unknown receiver",
                "def test_a(cart):\n    assert cart.can_reserve(5, 3) == True\n",
            ),
            (
                "reassigned receiver",
                "def test_a():\n    c = Cart()\n    c = Other()\n    assert c.can_reserve(5, 3) == True\n",
            ),
        ] {
            assert_no_missing_input(&build(body)?, label);
        }
        // Control: a receiver provably the owner class is an owner call.
        let owned = build("def test_a():\n    assert Cart().can_reserve(5, 3) == True\n")?;
        assert!(omission(&owned, 0), "Cart().can_reserve is the owner");
        Ok(())
    }

    #[test]
    fn relational_change_that_moves_the_operand_is_not_an_omission() -> Result<(), String> {
        let source = "def big(x):\n    if x > 20:\n        return True\n    return False\n";
        let tests = "from src.subject import big\n\ndef test_a():\n    assert big(5) == False\n";
        let moved = case(source, tests, 2, "    if x >= 10:", "    if x > 20:")?;
        assert_no_missing_input(&moved, "x >= 10 -> x > 20");
        // Control: same operands, operator change only, is an omission.
        let same = case(source, tests, 2, "    if x >= 20:", "    if x > 20:")?;
        assert!(omission(&same, 0));
        Ok(())
    }

    #[test]
    fn implicit_reach_outside_the_test_function_is_not_an_omission() -> Result<(), String> {
        let direct = "def test_a():\n    assert can_reserve(5, 3) == True\n";
        for (label, tests) in [
            (
                "module autouse fixture",
                "import pytest\n\n@pytest.fixture(autouse=True)\ndef boot():\n    can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "in-class autouse fixture",
                "import pytest\n\nclass TestReserve:\n    @pytest.fixture(autouse=True)\n    def boot(self):\n        can_reserve(4, 4)\n\n    def test_a(self):\n        assert can_reserve(5, 3) == True\n",
            ),
            (
                "pytestmark usefixtures",
                "import pytest\n\n@pytest.fixture\ndef boot():\n    can_reserve(4, 4)\n\npytestmark = pytest.mark.usefixtures('boot')\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "pytestmark parametrize",
                "import pytest\n\npytestmark = pytest.mark.parametrize('n', [4])\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "setUp hook",
                "import unittest\n\nclass T(unittest.TestCase):\n    def setUp(self):\n        can_reserve(4, 4)\n\n    def test_a(self):\n        self.assertEqual(can_reserve(5, 3), True)\n",
            ),
            (
                "setup_module hook",
                "def setup_module():\n    can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "module-level call",
                "RESULT = can_reserve(4, 4)\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "test module redefines the owner name",
                "def can_reserve(qty, on_hand):\n    return True\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "test module rebinds the owner name",
                "can_reserve = lambda qty, on_hand: True\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n",
            ),
            (
                "test-local shadow of the owner name",
                "def test_a():\n    def can_reserve(qty, on_hand):\n        return True\n    assert can_reserve(5, 3) == True\n",
            ),
        ] {
            assert_no_missing_input(&reserve(tests)?, label);
        }
        // Controls: the same direct call with none of those shapes is an
        // omission, and a module of only imports, definitions and constants
        // stays one.
        assert!(omission(&reserve(direct)?, 0));
        assert!(omission(&reserve(&format!("LIMIT = 3\n\n{direct}"))?, 0));
        Ok(())
    }

    #[test]
    fn decorated_owner_is_not_an_omission() -> Result<(), String> {
        let tests = "from src.subject import can_reserve\n\ndef test_a():\n    assert can_reserve(5, 3) == True\n";
        let build = |decorator: &str| {
            case(
                &format!(
                    "{decorator}def can_reserve(qty, on_hand):\n    if qty >= on_hand:\n        return True\n    return False\n"
                ),
                tests,
                if decorator.is_empty() { 2 } else { 3 },
                "    if qty > on_hand:",
                "    if qty >= on_hand:",
            )
        };
        assert_no_missing_input(&build("@always_true\n")?, "decorated owner");
        assert!(omission(&build("")?, 0), "undecorated control");
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
                    focus: &focus,
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
        let family = classify_probe_shape(&case.after).0;
        let focus = PythonAssertionFocus::for_change(family, &case.after, Some(&case.before));
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
                focus: &focus,
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

    fn two_oracle_case() -> Result<Case, String> {
        case(
            "def score(value):\n    return 8\n",
            "from src.subject import score\n\ndef test_a():\n    assert score(0)\n\ndef test_b():\n    assert score(1)\n",
            2,
            "    return 7",
            "    return 8",
        )
    }

    #[test]
    fn reordered_rows_that_both_have_oracles_annotate_nothing() -> Result<(), String> {
        let case = two_oracle_case()?;
        let mut aligned = base_rows(&case);
        assert_eq!(aligned.len(), 2);
        assert!(aligned.iter().all(|row| row.oracle.is_some()));
        annotate(&case, &mut aligned);
        // Control: aligned rows each gain their own reason.
        assert!(aligned.iter().all(|row| row.miss.is_some()), "control");

        let mut reordered = base_rows(&case);
        reordered.reverse();
        annotate(&case, &mut reordered);
        assert!(reordered.iter().all(|row| row.miss.is_none()));
        Ok(())
    }

    #[test]
    fn weak_strength_row_gets_weak_assertion_like_smoke() -> Result<(), String> {
        let case = two_oracle_case()?;
        for strength in [
            crate::domain::OracleStrength::Weak,
            crate::domain::OracleStrength::Smoke,
        ] {
            let mut rows = base_rows(&case);
            for row in &mut rows {
                row.oracle_strength = strength.clone();
            }
            annotate(&case, &mut rows);
            assert!(
                rows.iter()
                    .all(|row| row.miss == Some(RelatedTestMiss::WeakAssertion)),
                "{strength:?}"
            );
        }
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
