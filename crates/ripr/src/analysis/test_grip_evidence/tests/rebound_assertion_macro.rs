//! #15183: an assertion the test spells as `assert_eq!` grips a seam only
//! when that name still means the standard macro at the assertion. A
//! test-local `macro_rules! assert_eq` that compares nothing must not read as
//! a strong oracle on any seam of the owner, in the full and the compact grip
//! paths alike; the same test with the standard macro keeps its credit.

use super::*;

const OWNER: &str = "pub enum Size {\n    Small,\n    Medium,\n    Large,\n}\n\n\
pub fn price_cents(size: Size) -> u32 {\n    match size {\n        Size::Small => 300,\n        \
Size::Medium => 450,\n        Size::Large => 500,\n    }\n}\n";

const REBOUND: &str = "    macro_rules! assert_eq {\n        \
($f:ident ( $($arg:tt)* ), $expected:expr) => {\n            \
let _ = $f(Size::Medium);\n        };\n    }\n\n";

const TEST: &str = "    #[test]\n    fn small_and_large_prices() {\n        \
assert_eq!(price_cents(Size::Small), 300);\n        \
assert_eq!(price_cents(Size::Large), 500);\n    }\n";

type SeamReading = (String, [TestGripEvidence; 2], [SeamGripClass; 2]);

fn match_seam_readings(rebound: bool) -> Result<Vec<SeamReading>, String> {
    let source = format!(
        "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{}{TEST}}}\n",
        if rebound { REBOUND } else { "" }
    );
    let file = PathBuf::from("src/lib.rs");
    let index = index_from_files(&[(file.clone(), source.as_str())])?;
    let seams = inventory_seams_from_index(std::slice::from_ref(&file), &index);
    let context = CompactGripContext::new(&index);
    let selected: Vec<SeamReading> = seams
        .iter()
        .filter(|seam| seam.kind() == SeamKind::MatchArm)
        .map(|seam| {
            let full = evidence_for_seam(seam, &index);
            let compact = compact_evidence_for_seam(seam, &context);
            let classes = [classify_seam(seam, &full), classify_seam(seam, &compact)];
            (seam.expression().to_string(), [full, compact], classes)
        })
        .collect();
    // The fixture builds the four seams the issue reports, each related to
    // the one test by a direct owner call, before any oracle claim is read.
    // Only the full path publishes the related-test records; the compact
    // path is read through its stages and class.
    assert_eq!(selected.len(), 4, "match seams: {seams:?}");
    for (expression, [full, _], _) in &selected {
        assert_eq!(full.related_tests.len(), 1, "{expression}");
        assert_eq!(
            full.related_tests[0].relation_reason,
            RelationReason::DirectOwnerCall,
            "{expression}"
        );
    }
    Ok(selected)
}

#[test]
fn a_rebound_assert_eq_grips_no_seam_of_the_owner() -> Result<(), String> {
    for (expression, evidence, classes) in match_seam_readings(true)? {
        assert_eq!(
            evidence[0].related_tests[0].oracle_strength,
            OracleStrength::Unknown,
            "{expression}: {:?}",
            evidence[0]
        );
        for evidence in &evidence {
            assert_ne!(
                evidence.discriminate.state,
                StageState::Yes,
                "{expression}: {evidence:?}"
            );
        }
        assert!(
            !classes.contains(&SeamGripClass::StronglyGripped),
            "{expression}: {classes:?}"
        );
    }
    Ok(())
}

#[test]
fn the_standard_assert_eq_keeps_its_strong_oracle() -> Result<(), String> {
    let selected = match_seam_readings(false)?;
    for (expression, [full, _], _) in &selected {
        assert_eq!(
            full.related_tests[0].oracle_strength,
            OracleStrength::Strong,
            "{expression}: {full:?}"
        );
    }
    let small = selected
        .iter()
        .find(|(expression, _, _)| expression.starts_with("Size::Small"))
        .ok_or("expected the `Size::Small` arm seam")?;
    assert_eq!(small.2, [SeamGripClass::StronglyGripped; 2]);
    for evidence in &small.1 {
        assert_eq!(evidence.discriminate.state, StageState::Yes);
    }
    Ok(())
}
