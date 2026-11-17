//! #12078: a strong oracle from a test whose input selects another arm must
//! not grip a `match_arm` seam. The repo seam reads `ripr check`'s own arm
//! selection (RIPR-SPEC-0229), in the full and the compact grip paths alike.

use super::*;

const PRICES: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n";

fn arm_evidence(
    test_body: &str,
    arm: &str,
) -> Result<(TestGripEvidence, TestGripEvidence, [SeamGripClass; 2]), String> {
    let tests = format!("use prices::*;\n\n#[test]\nfn prices() {{\n{test_body}\n}}\n");
    let files: Vec<(PathBuf, &str)> = vec![
        (PathBuf::from("src/lib.rs"), PRICES),
        (PathBuf::from("tests/prices.rs"), tests.as_str()),
    ];
    let index = index_from_files(&files)?;
    let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
    let seam = seams
        .iter()
        .find(|seam| seam.kind() == SeamKind::MatchArm && seam.expression().starts_with(arm))
        .ok_or_else(|| format!("expected the `{arm}` match-arm seam: {seams:?}"))?;
    let full = evidence_for_seam(seam, &index);
    let context = CompactGripContext::new(&index);
    let compact = compact_evidence_for_seam(seam, &context);
    let classes = [classify_seam(seam, &full), classify_seam(seam, &compact)];
    Ok((full, compact, classes))
}

fn missing_values(evidence: &TestGripEvidence) -> Vec<&str> {
    evidence
        .missing_discriminators
        .iter()
        .map(|fact| fact.value.as_str())
        .collect()
}

#[test]
fn an_arm_no_test_input_selects_is_weakly_gripped_with_its_pattern_missing() -> Result<(), String> {
    let (full, compact, class) = arm_evidence(
        "    assert_eq!(price_cents(Size::Small), 300);\n    assert_eq!(price_cents(Size::Large), 500);",
        "Size::Medium",
    )?;
    assert_eq!(class, [SeamGripClass::WeaklyGripped; 2]);
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Weak);
        assert_eq!(missing_values(evidence), ["Size::Medium"]);
        assert!(
            evidence
                .activate
                .summary
                .starts_with(crate::domain::ARM_UNSELECTED_REASON_PREFIX),
            "{}",
            evidence.activate.summary
        );
    }
    Ok(())
}

#[test]
fn an_arm_a_test_input_selects_keeps_its_strong_grip() -> Result<(), String> {
    let (full, compact, class) = arm_evidence(
        "    assert_eq!(price_cents(Size::Small), 300);\n    assert_eq!(price_cents(Size::Medium), 450);",
        "Size::Medium",
    )?;
    assert_eq!(class, [SeamGripClass::StronglyGripped; 2]);
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Yes);
        assert_eq!(missing_values(evidence), Vec::<&str>::new());
    }
    Ok(())
}

#[test]
fn an_input_the_reading_cannot_resolve_leaves_the_arm_unnamed() -> Result<(), String> {
    // A computed input may select the arm, so the shared reading refuses and
    // the seam keeps the stage it had before #12078.
    let (full, compact, class) = arm_evidence(
        "    let size = if std::env::args().count() > 99 { Size::Small } else { Size::Medium };\n    \
         assert_eq!(price_cents(size), 450);\n    \
         assert_eq!(price_cents(Size::Large), 500);",
        "Size::Medium",
    )?;
    assert_eq!(class, [SeamGripClass::StronglyGripped; 2]);
    for evidence in [&full, &compact] {
        assert_eq!(evidence.activate.state, StageState::Yes);
        assert!(
            !missing_values(evidence).contains(&"Size::Medium"),
            "{:?}",
            evidence.missing_discriminators
        );
    }
    Ok(())
}
