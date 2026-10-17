//! Public repository-renderer controls for #8121's return-value residual.
//! A snapshot observes only the value it names: an unrelated committed
//! snapshot cannot close a changed return seam, while a snapshot of the
//! owner's own result keeps its existing grade.

#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, OutputFormat, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub fn make_count(count: usize) -> usize {\n    count + 1\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn snapshot_count() {\n        __RIPR_RETURN_SNAPSHOT_BODY__\n    }\n}\n";
const BODY_MARKER: &str = "__RIPR_RETURN_SNAPSHOT_BODY__";
const SNAPSHOT: &str = "src/snapshots/return_snapshot_admission__tests__snapshot_count.snap";

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(body: &str) -> Result<Self, String> {
        assert_eq!(SOURCE.matches(BODY_MARKER).count(), 1);
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-return-snapshot-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src/snapshots")).map_err(|error| error.to_string())?;
        let fixture = Self { root };
        std::fs::write(
            fixture.root.join("Cargo.toml"),
            "[package]\nname = \"return_snapshot_admission\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[dev-dependencies]\ninsta = { version = \"=1.49.0\", features = [\"json\"] }\nexpect-test = \"1\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.root.join("src/lib.rs"),
            SOURCE.replace(BODY_MARKER, body),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.root.join(SNAPSHOT),
            "---\nsource: src/lib.rs\nexpression: count\n---\n3\n",
        )
        .map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn seam(&self) -> Result<Value, String> {
        let output = ripr::app::repo_seam_inventory_input(CheckInput {
            root: self.root.clone(),
            ..CheckInput::default()
        });
        let rendered = render_check(&output, &OutputFormat::RepoExposureJson)?;
        let report: Value = serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
        assert_eq!(report["run_status"], "complete");
        let seams = report["seams"].as_array().ok_or("missing seams")?;
        let matching: Vec<_> = seams
            .iter()
            .filter(|seam| {
                seam["kind"] == "return_value" && seam["owner"] == "src/lib.rs::make_count"
            })
            .collect();
        assert_eq!(matching.len(), 1, "must inspect one actual owner return");
        Ok(matching[0].clone())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn assert_weak(seam: &Value) {
    assert_eq!(seam["evidence"]["reach"], "yes");
    assert_eq!(seam["evidence"]["activate"], "yes");
    assert_eq!(seam["grip_class"], "weakly_gripped");
    assert_eq!(seam["evidence"]["discriminate"], "weak");
    assert_ne!(
        seam["evidence_record"]["canonical_item"]["gap_state"],
        "already_observed"
    );
    assert_ne!(
        seam["evidence_record"]["canonical_item"]["recommended_repair"],
        "No new RIPR action; current static evidence already observes this seam."
    );
}

fn assert_strong(seam: &Value) {
    assert_eq!(seam["grip_class"], "strongly_gripped");
    assert_eq!(seam["evidence"]["discriminate"], "yes");
}

#[test]
fn snapshot_of_an_unrelated_value_cannot_close_the_owner_return() -> Result<(), String> {
    let fixture = Fixture::new(
        "let _count = make_count(2);\n        let other = 2usize;\n        insta::assert_json_snapshot!(other);",
    )?;
    assert_weak(&fixture.seam()?);
    Ok(())
}

#[test]
fn snapshot_of_the_owner_result_binding_keeps_its_grade() -> Result<(), String> {
    let fixture =
        Fixture::new("let count = make_count(2);\n        insta::assert_json_snapshot!(count);")?;
    assert_strong(&fixture.seam()?);
    Ok(())
}

#[test]
fn snapshot_of_the_direct_owner_call_keeps_its_grade() -> Result<(), String> {
    let fixture = Fixture::new("insta::assert_json_snapshot!(make_count(2));")?;
    assert_strong(&fixture.seam()?);
    Ok(())
}

#[test]
fn rebound_owner_result_name_cannot_carry_the_snapshot() -> Result<(), String> {
    let fixture = Fixture::new(
        "let count = make_count(2);\n        let count = 2usize;\n        insta::assert_json_snapshot!(count);",
    )?;
    assert_weak(&fixture.seam()?);
    Ok(())
}

#[test]
fn independent_literal_assertion_remains_strong_beside_an_unrelated_snapshot() -> Result<(), String>
{
    let fixture = Fixture::new(
        "let count = make_count(2);\n        let other = 2usize;\n        insta::assert_json_snapshot!(other);\n        assert_eq!(count, 3);",
    )?;
    assert_strong(&fixture.seam()?);
    Ok(())
}

#[test]
fn read_only_comparisons_do_not_rebind_the_owner_result() -> Result<(), String> {
    let fixture = Fixture::new(
        "let count = make_count(2);\n        if count == 0 {\n            return;\n        }\n        debug_assert!(count >= 1);\n        insta::assert_json_snapshot!(count);",
    )?;
    assert_strong(&fixture.seam()?);
    Ok(())
}

#[test]
fn inner_block_owner_binding_cannot_carry_an_outer_literal_snapshot() -> Result<(), String> {
    let fixture = Fixture::new(
        "let count = 2usize;\n        {\n            let count = make_count(2);\n            let _ = count;\n        }\n        insta::assert_json_snapshot!(count);",
    )?;
    assert_weak(&fixture.seam()?);
    Ok(())
}

#[test]
fn pattern_binder_shadow_cannot_carry_the_owner_result_snapshot() -> Result<(), String> {
    let fixture = Fixture::new(
        "let count = make_count(2);\n        if let Some(count) = Some(2usize) {\n            insta::assert_json_snapshot!(count);\n        }",
    )?;
    assert_weak(&fixture.seam()?);
    Ok(())
}

#[test]
fn transformed_owner_call_is_not_the_owner_result() -> Result<(), String> {
    let fixture = Fixture::new(
        "let count = make_count(2).to_string();\n        insta::assert_json_snapshot!(count);",
    )?;
    assert_weak(&fixture.seam()?);
    Ok(())
}
