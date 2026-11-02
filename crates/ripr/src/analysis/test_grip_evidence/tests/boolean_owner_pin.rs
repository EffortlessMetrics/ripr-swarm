//! #8610: a natural bool assertion must carry the same owner-bound credit
//! into the repair evidence as its explicit `assert_eq!(.., true)` spelling.

use super::*;
use crate::analysis::test_grip_evidence::value_contradiction::boolean_assertion_verdict_for_test;

fn boundary_fixture(
    owner_tail: &str,
    predicate: &str,
    test_body: &str,
) -> Result<(FixtureIndex, RepoSeam), String> {
    seam_fixture(
        SeamKind::PredicateBoundary,
        owner_tail,
        predicate,
        test_body,
    )
}

fn seam_fixture(
    kind: SeamKind,
    owner_tail: &str,
    predicate: &str,
    test_body: &str,
) -> Result<(FixtureIndex, RepoSeam), String> {
    let file = PathBuf::from("src/lib.rs");
    let source = format!(
        "pub fn is_adult(age: u32) -> bool {{\n    {owner_tail}\n}}\n\n\
         pub fn other(_: u32) -> bool {{ true }}\n\n\
         #[cfg(test)]\nmod tests {{\n    use super::*;\n\
         #[test]\nfn adult_boundary() {{\n{test_body}\n}}\n}}\n"
    );
    let index = index_from_files(&[(file.clone(), &source)])?;
    let owner = index
        .functions()
        .iter()
        .find(|owner| owner.file == file && owner.name == "is_adult")
        .ok_or("fixture lost its boolean owner")?;
    let owner_id = owner.id.0.replace('\\', "/");
    let mut selected = inventory_seams_from_index(std::slice::from_ref(&file), &index)
        .into_iter()
        .filter(|seam| {
            seam.kind() == kind && seam.owner() == owner_id && seam.expression().trim() == predicate
        })
        .collect::<Vec<_>>();
    if selected.len() != 1 {
        return Err(format!(
            "expected exactly one owner-bound `{predicate}` seam, got {selected:?}"
        ));
    }
    let seam = selected.pop().ok_or("fixture lost its selected seam")?;
    Ok((index, seam))
}

#[test]
fn boolean_boundary_assertions_credit_the_same_owner_result() -> Result<(), String> {
    for (predicate, assertion, raw_kind, raw_strength) in [
        (
            "age >= 18",
            "assert_eq!(is_adult(18), true);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        ),
        (
            "age >= 18",
            "assert!(is_adult(18));",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        ),
        (
            "age > 18",
            "assert!(!is_adult(18), \"eighteen is below the strict boundary\");",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        ),
    ] {
        let (index, seam) = boundary_fixture(predicate, predicate, assertion)?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "adult_boundary")
            .ok_or("fixture lost its parsed test")?;
        assert_eq!(test.assertions.len(), 1, "one actual asserted result");
        assert_eq!(test.assertions[0].kind, raw_kind, "extraction is unchanged");
        assert_eq!(test.assertions[0].strength, raw_strength);

        let full = evidence_for_seam(&seam, &index);
        assert_eq!(full.related_tests.len(), 1, "{assertion}: {full:?}");
        assert_eq!(full.reach.state, StageState::Yes, "{assertion}: {full:?}");
        assert_eq!(
            full.activate.state,
            StageState::Yes,
            "{assertion}: {full:?}"
        );
        assert!(
            full.missing_discriminators.is_empty(),
            "{assertion}: {full:?}"
        );
        assert_eq!(
            full.discriminate.state,
            StageState::Yes,
            "{assertion}: {full:?}"
        );
        assert_eq!(
            full.related_tests[0].oracle_strength,
            OracleStrength::Strong,
            "repair receipt must carry the admitted owner-relative strength: {assertion}"
        );
        assert_eq!(full.related_tests[0].oracle_kind, raw_kind);
        assert_eq!(
            crate::analysis::seam_classification::classify_seam(&seam, &full),
            SeamGripClass::StronglyGripped,
            "{assertion}: {full:?}"
        );

        let context = CompactGripContext::new(&index);
        let compact = compact_evidence_for_seam(&seam, &context);
        assert_eq!(compact.discriminate.state, full.discriminate.state);
        assert_eq!(
            crate::analysis::seam_classification::classify_seam(&seam, &compact),
            SeamGripClass::StronglyGripped,
            "compact evidence must agree: {assertion}: {compact:?}"
        );
    }
    Ok(())
}

#[test]
fn boolean_boundary_assertions_do_not_borrow_unobserved_or_wrong_results() -> Result<(), String> {
    for (owner_tail, body) in [
        ("age >= 18", "assert!(is_adult(30));"),
        (
            "age >= 18",
            "let _unobserved = is_adult(18);\nassert!(is_adult(30));",
        ),
        (
            "age >= 18",
            "let _unobserved = is_adult(18);\nassert!(other(18));",
        ),
        (
            "age >= 18",
            "let _unobserved = is_adult(18);\nlet is_adult = |_: u32| true;\nassert!(is_adult(18));",
        ),
        ("age >= 18", "assert!(is_adult(18) || true);"),
        ("age >= 18 && age < 99", "assert!(is_adult(18));"),
        (
            "if age == 18 { return true; }\n    age >= 18",
            "assert!(is_adult(18));",
        ),
        ("age >= 18", "assert!(!is_adult(18));"),
    ] {
        let (index, seam) = boundary_fixture(owner_tail, "age >= 18", body)?;
        let full = evidence_for_seam(&seam, &index);
        assert!(
            !full.related_tests.is_empty(),
            "nonempty control: {body}: {full:?}"
        );
        assert_ne!(
            full.discriminate.state,
            StageState::Yes,
            "no new boolean credit for an unpaired, masked, shadowed, partial or wrong result: {body}: {full:?}"
        );
        assert_ne!(
            crate::analysis::seam_classification::classify_seam(&seam, &full),
            SeamGripClass::StronglyGripped,
            "{body}: {full:?}"
        );
        let context = CompactGripContext::new(&index);
        let compact = compact_evidence_for_seam(&seam, &context);
        assert_eq!(compact.discriminate.state, full.discriminate.state);
        assert_ne!(
            crate::analysis::seam_classification::classify_seam(&seam, &compact),
            SeamGripClass::StronglyGripped,
            "compact evidence must keep the same refusal: {body}: {compact:?}"
        );
    }
    Ok(())
}

#[test]
fn boolean_consistency_distinguishes_the_current_true_and_false_result() -> Result<(), String> {
    for (predicate, assertion, consistent) in [
        ("age >= 18", "assert!(is_adult(18));", true),
        ("age >= 18", "assert!(!is_adult(18));", false),
        ("age > 18", "assert!(!is_adult(18));", true),
        ("age > 18", "assert!(is_adult(18));", false),
    ] {
        let (index, _) = boundary_fixture(predicate, predicate, assertion)?;
        let owner = index
            .functions()
            .iter()
            .find(|owner| owner.name == "is_adult")
            .ok_or("fixture lost its owner")?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "adult_boundary")
            .ok_or("fixture lost its test")?;
        let verdict = boolean_assertion_verdict_for_test(owner, assertion, test);
        if consistent {
            assert_eq!(
                verdict,
                ExactValueVerdict::Consistent,
                "{predicate}: {assertion}"
            );
        } else {
            assert!(
                matches!(verdict, ExactValueVerdict::Contradicted { .. }),
                "wrong boolean expectation must be contradicted: {predicate}: {assertion}: {verdict:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn boolean_consistency_requires_representable_matching_integer_literals() -> Result<(), String> {
    for (argument, expected) in [
        ("18", ExactValueVerdict::Consistent),
        ("18u32", ExactValueVerdict::Consistent),
        ("-1", ExactValueVerdict::NotEvaluable),
        ("4294967296", ExactValueVerdict::NotEvaluable),
        ("18u64", ExactValueVerdict::NotEvaluable),
        ("18usize", ExactValueVerdict::NotEvaluable),
    ] {
        let assertion = format!("assert!(is_adult({argument}));");
        let (index, _) = boundary_fixture("age >= 18", "age >= 18", &assertion)?;
        let owner = index
            .functions()
            .iter()
            .find(|owner| owner.name == "is_adult")
            .ok_or("fixture lost its owner")?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "adult_boundary")
            .ok_or("fixture lost its test")?;
        assert_eq!(
            boolean_assertion_verdict_for_test(owner, &assertion, test),
            expected,
            "the u32 parameter cannot receive an incompatible literal: {argument}"
        );
    }
    Ok(())
}

#[test]
fn a_contradicted_boolean_assertion_withholds_the_whole_tests_promotion() -> Result<(), String> {
    // #8610 review: `assert!(is_adult(18)); assert!(!is_adult(18));` fails on
    // its second line, so this baseline-failing test must carry no activation
    // credit and cannot lift the seam to a strong grip through its consistent
    // first line — the same whole-test refusal an equality contradiction
    // already receives (#6026, #7007).
    let body = "assert!(is_adult(18));\nassert!(!is_adult(18));";
    let (index, seam) = boundary_fixture("age >= 18", "age >= 18", body)?;
    let owner = index
        .functions()
        .iter()
        .find(|owner| owner.name == "is_adult")
        .ok_or("fixture lost its owner")?;
    let test = index
        .tests()
        .iter()
        .find(|test| test.name == "adult_boundary")
        .ok_or("fixture lost its paired test")?;
    assert_eq!(test.assertions.len(), 2, "extraction keeps both assertions");
    assert!(
        test_has_contradicted_assertion(test, Some(owner), &index),
        "the contradictory boolean assertion must count beside an equality one: {test:?}"
    );

    let full = evidence_for_seam(&seam, &index);
    assert!(!full.related_tests.is_empty(), "nonempty control: {full:?}");
    assert_ne!(
        full.activate.state,
        StageState::Yes,
        "the baseline-failing test keeps no activation credit: {full:?}"
    );
    // Should-stay control (docs/LEARNINGS.md): the repo surface's verdict
    // for this contradictory sibling is exactly `activation_unknown` — the
    // typed withholding #10980 makes the diff surface match. If a change
    // moves this class, it must move both surfaces together.
    assert_eq!(
        crate::analysis::seam_classification::classify_seam(&seam, &full),
        SeamGripClass::ActivationUnknown,
        "the repo verdict for the contradictory sibling is pinned: {full:?}"
    );
    let context = CompactGripContext::new(&index);
    let compact = compact_evidence_for_seam(&seam, &context);
    assert_eq!(
        crate::analysis::seam_classification::classify_seam(&seam, &compact),
        SeamGripClass::ActivationUnknown,
        "compact evidence must keep the same refusal: {compact:?}"
    );
    Ok(())
}

/// #11111: the return-value seam of a bool owner whose whole tail is the
/// comparison reads the same verdict as its predicate seam, as `ripr check`
/// does for the return-value finding; a wrong, unpaired or partial result
/// still earns no credit on either seam.
#[test]
fn boolean_tail_return_value_seam_reads_the_predicate_seams_verdict() -> Result<(), String> {
    for (body, gripped) in [
        ("assert!(is_adult(18));\nassert!(!is_adult(17));", true),
        ("assert!(is_adult(18));", true),
        ("assert!(is_adult(30));", false),
        ("assert!(!is_adult(18));", false),
        ("assert!(is_adult(18) || true);", false),
    ] {
        for kind in [SeamKind::PredicateBoundary, SeamKind::ReturnValue] {
            let (index, seam) = seam_fixture(kind, "age >= 18", "age >= 18", body)?;
            let full = evidence_for_seam(&seam, &index);
            assert!(
                !full.related_tests.is_empty(),
                "nonempty control: {kind:?} {body}: {full:?}"
            );
            let class = crate::analysis::seam_classification::classify_seam(&seam, &full);
            assert_eq!(
                class == SeamGripClass::StronglyGripped,
                gripped,
                "{kind:?} {body}: {class:?} {full:?}"
            );
            let context = CompactGripContext::new(&index);
            let compact = compact_evidence_for_seam(&seam, &context);
            assert_eq!(
                crate::analysis::seam_classification::classify_seam(&seam, &compact),
                class,
                "compact evidence must agree: {kind:?} {body}: {compact:?}"
            );
        }
    }
    Ok(())
}
