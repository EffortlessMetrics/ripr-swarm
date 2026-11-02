//! #7894: adjacent expected bindings must strongly grip repo field seams.

use super::*;
use crate::analysis::seam_classification::classify_seam;
use crate::analysis::seams::SeamGripClass;
use std::path::PathBuf;

const CONFIG_LIB: &str = r#"#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub retries: u32,
    pub name: String,
}

pub fn build(n: u32) -> Config {
    Config {
        retries: 1 + n,
        name: "x".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_config() {
        let c = build(3);
        let e = Config { retries: 4, name: "x".into() };
        assert_eq!(c, e);
    }
}
"#;

fn spec0225_seam(index: &FixtureIndex) -> Result<RepoSeam, String> {
    let file = PathBuf::from("src/lib.rs");
    let owner = index
        .functions()
        .iter()
        .find(|owner| owner.file == file && owner.name == "build")
        .ok_or("missing build owner")?;
    let owner_id = owner.id.0.replace('\\', "/");
    let mut seams = inventory_seams_from_index(std::slice::from_ref(&file), index)
        .into_iter()
        .filter(|seam| {
            seam.kind() == SeamKind::FieldConstruction
                && seam.owner() == owner_id
                && seam.expression().contains("retries:")
        })
        .collect::<Vec<_>>();
    if seams.len() != 1 {
        return Err(format!("expected one retries field seam, got {seams:?}"));
    }
    Ok(seams.pop().ok_or("missing seam")?)
}

#[test]
fn adjacent_expected_binding_strongly_grips_the_changed_field_seam() -> Result<(), String> {
    let file = PathBuf::from("src/lib.rs");
    let index = index_from_files(&[(file, CONFIG_LIB)])?;
    let seam = spec0225_seam(&index)?;
    let full = evidence_for_seam(&seam, &index);
    assert_eq!(full.discriminate.state, StageState::Yes, "{full:?}");
    assert!(full.missing_discriminators.is_empty(), "{full:?}");
    assert_eq!(
        classify_seam(&seam, &full),
        SeamGripClass::StronglyGripped,
        "{full:?}"
    );
    let context = CompactGripContext::new(&index);
    let compact = compact_evidence_for_seam(&seam, &context);
    assert_eq!(compact.discriminate.state, full.discriminate.state);
    assert_eq!(
        classify_seam(&seam, &compact),
        SeamGripClass::StronglyGripped,
        "{compact:?}"
    );
    Ok(())
}

#[test]
fn self_derived_expected_binding_stays_weak_on_the_field_seam() -> Result<(), String> {
    let file = PathBuf::from("src/lib.rs");
    let dependent = CONFIG_LIB.replace(
        "let e = Config { retries: 4, name: \"x\".into() };",
        "let e = Config { retries: c.retries, name: \"x\".into() };",
    );
    let index = index_from_files(&[(file, &dependent)])?;
    let seam = spec0225_seam(&index)?;
    let full = evidence_for_seam(&seam, &index);
    assert_eq!(full.discriminate.state, StageState::Weak, "{full:?}");
    assert_eq!(
        classify_seam(&seam, &full),
        SeamGripClass::WeaklyGripped,
        "{full:?}"
    );
    Ok(())
}
