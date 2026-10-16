//! Public repository-renderer controls for #8120/#8121's bounded admission guard.
//! Snapshot syntax does not establish expected bytes or the returned field's
//! receiver. Native Insta execution is retained separately in the issue packet.

#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, OutputFormat, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub struct Record {\n    pub items: usize,\n}\n\npub fn make_record(count: usize) -> Record {\n    Record { items: count + 0 }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    use expect_test::expect;\n    #[test]\n    fn snapshot_field() {\n        let record = make_record(2);\n        __RIPR_FIELD_SNAPSHOT_ASSERTION__\n    }\n}\n";
const ASSERTION_MARKER: &str = "__RIPR_FIELD_SNAPSHOT_ASSERTION__";
const SNAPSHOT: &str = "src/snapshots/field_snapshot_admission__tests__snapshot_field.snap";

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(assertion: &str, expected: Option<&str>) -> Result<Self, String> {
        assert_eq!(SOURCE.matches(ASSERTION_MARKER).count(), 1);
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-field-snapshot-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self { root };
        std::fs::create_dir_all(fixture.root.join("src/snapshots"))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.root.join("Cargo.toml"),
            "[package]\nname = \"field_snapshot_admission\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[dev-dependencies]\ninsta = { version = \"=1.49.0\", features = [\"json\"] }\nexpect-test = \"1\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.root.join("src/lib.rs"),
            SOURCE.replace(ASSERTION_MARKER, assertion),
        )
        .map_err(|error| error.to_string())?;
        fixture.expected(expected)?;
        Ok(fixture)
    }

    fn expected(&self, value: Option<&str>) -> Result<(), String> {
        let path = self.root.join(SNAPSHOT);
        match value {
            Some(value) => std::fs::write(
                path,
                format!("---\nsource: src/lib.rs\nexpression: record.items\n---\n{value}\n"),
            ),
            None if path.exists() => std::fs::remove_file(path),
            None => Ok(()),
        }
        .map_err(|error| error.to_string())
    }

    fn field(&self) -> Result<Value, String> {
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
                seam["kind"] == "field_construction"
                    && seam["owner"] == "src/lib.rs::make_record"
                    && seam["expression"]
                        .as_str()
                        .is_some_and(|expression| expression.starts_with("items:"))
            })
            .collect();
        assert_eq!(matching.len(), 1, "must inspect one actual owner field");
        Ok(matching[0].clone())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn assert_weak(field: &Value) {
    assert_eq!(field["evidence"]["reach"], "yes");
    assert_eq!(field["evidence"]["activate"], "yes");
    assert_eq!(field["grip_class"], "weakly_gripped");
    assert_eq!(field["evidence"]["discriminate"], "weak");
    assert_ne!(
        field["evidence_record"]["canonical_item"]["gap_state"],
        "already_observed"
    );
    assert_ne!(
        field["evidence_record"]["canonical_item"]["recommended_repair"],
        "No new RIPR action; current static evidence already observes this seam."
    );
}

#[test]
fn snapshot_of_another_receiver_cannot_close_the_owner_field() -> Result<(), String> {
    let fixture = Fixture::new(
        "let other = Record { items: 2 };\n        insta::assert_json_snapshot!(other.items);",
        Some("2"),
    )?;
    assert_weak(&fixture.field()?);
    Ok(())
}

#[test]
fn snapshot_without_an_expected_file_cannot_close_the_owner_field() -> Result<(), String> {
    let fixture = Fixture::new("insta::assert_json_snapshot!(record.items);", None)?;
    assert!(!fixture.root.join(SNAPSHOT).exists());
    assert_weak(&fixture.field()?);
    Ok(())
}

#[test]
fn snapshot_only_expected_edits_do_not_invent_field_discrimination() -> Result<(), String> {
    let fixture = Fixture::new("insta::assert_json_snapshot!(record.items);", Some("2"))?;
    let source =
        std::fs::read(fixture.root.join("src/lib.rs")).map_err(|error| error.to_string())?;
    for expected in [Some("2"), Some("99"), None, Some("2")] {
        fixture.expected(expected)?;
        assert_weak(&fixture.field()?);
        assert_eq!(
            std::fs::read(fixture.root.join("src/lib.rs")).map_err(|error| error.to_string())?,
            source
        );
    }
    let cold = Fixture::new("insta::assert_json_snapshot!(record.items);", None)?;
    assert_weak(&cold.field()?);
    Ok(())
}

#[test]
fn independent_literal_field_assertion_remains_strong_beside_a_snapshot() -> Result<(), String> {
    let fixture = Fixture::new(
        "let other = Record { items: 2 };\n        insta::assert_json_snapshot!(other.items);\n        assert_eq!(record.items, 2);",
        Some("2"),
    )?;
    let field = fixture.field()?;
    assert_eq!(field["grip_class"], "strongly_gripped");
    assert_eq!(field["evidence"]["discriminate"], "yes");
    Ok(())
}

#[test]
fn inline_expect_test_field_comparison_keeps_its_existing_grade() -> Result<(), String> {
    let fixture = Fixture::new(
        "expect![[\"2\"]].assert_eq(&record.items.to_string());",
        None,
    )?;
    let field = fixture.field()?;
    assert_eq!(field["grip_class"], "strongly_gripped");
    assert_eq!(field["evidence"]["discriminate"], "yes");
    Ok(())
}

#[test]
fn inline_snapshot_expected_text_cannot_borrow_another_receiver_field() -> Result<(), String> {
    let fixture = Fixture::new(
        "let other = Record { items: 2 };\n        insta::assert_json_snapshot!(other.items, @\"2\");",
        None,
    )?;
    assert_weak(&fixture.field()?);
    Ok(())
}
