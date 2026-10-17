//! Public controls for #3161's return-snapshot subject admission.
//! Native Insta oracle outcomes remain in the independently retained audit.

#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, OutputFormat, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub fn make_count(count: usize) -> usize {\n    count + 0\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    use expect_test::expect;\n    #[test]\n    fn snapshot_count() {\n        __BODY__\n    }\n}\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new(body: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-return-snapshot-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        std::fs::create_dir_all(fixture.0.join("src/snapshots"))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname=\"return_snapshot_admission\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[dev-dependencies]\ninsta={version=\"=1.49.0\",features=[\"json\"]}\nexpect-test=\"1\"\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(SOURCE.matches("__BODY__").count(), 1);
        std::fs::write(
            fixture.0.join("src/lib.rs"),
            SOURCE.replace("__BODY__", body),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture
                .0
                .join("src/snapshots/return_snapshot_admission__tests__snapshot_count.snap"),
            "---\nsource: src/lib.rs\nexpression: actual\n---\n2\n",
        )
        .map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn seam(&self) -> Result<Value, String> {
        let output = ripr::app::repo_seam_inventory_input(CheckInput {
            root: self.0.clone(),
            ..CheckInput::default()
        });
        let report: Value =
            serde_json::from_str(&render_check(&output, &OutputFormat::RepoExposureJson)?)
                .map_err(|error| error.to_string())?;
        assert_eq!(report["run_status"], "complete");
        let matching: Vec<_> = report["seams"]
            .as_array()
            .ok_or("missing seams")?
            .iter()
            .filter(|seam| {
                seam["kind"] == "return_value" && seam["owner"] == "src/lib.rs::make_count"
            })
            .collect();
        assert_eq!(matching.len(), 1, "inspect the actual owner's return");
        Ok(matching[0].clone())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn weak(body: &str) -> Result<(), String> {
    let seam = Fixture::new(body)?.seam()?;
    assert_eq!(seam["evidence"]["reach"], "yes");
    assert_eq!(seam["evidence"]["activate"], "yes");
    assert_eq!(
        seam["evidence"]["observe"], "yes",
        "retain raw snapshot evidence"
    );
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
    Ok(())
}

fn strong(body: &str) -> Result<(), String> {
    let seam = Fixture::new(body)?.seam()?;
    assert_eq!(seam["grip_class"], "strongly_gripped");
    assert_eq!(seam["evidence"]["discriminate"], "yes");
    Ok(())
}

#[test]
fn unrelated_snapshot_cannot_close_the_owner_return() -> Result<(), String> {
    weak(
        "let actual = make_count(2);\n        let other = 2usize;\n        insta::assert_json_snapshot!(other);",
    )
}

#[test]
fn direct_owner_result_snapshots_keep_their_grade() -> Result<(), String> {
    for assertion in [
        "insta::assert_json_snapshot!(actual);",
        "insta::assert_snapshot!(actual, @\"2\");",
        "insta::assert_debug_snapshot!(\"named\", &actual);",
        "insta::assert_json_snapshot!(make_count(2));",
        "insta::assert_json_snapshot!(crate::make_count(2));",
    ] {
        strong(&format!("let actual = make_count(2);\n        {assertion}"))?;
    }
    strong("let actual = make_count(2); insta::assert_json_snapshot!(actual);")?;
    strong("let actual = crate::make_count(2);\n        insta::assert_json_snapshot!(actual);")
}

#[test]
fn independent_literal_and_expect_oracles_keep_their_grade() -> Result<(), String> {
    for assertion in [
        "assert_eq!(actual, 2);",
        "expect![[\"2\"]].assert_eq(&actual.to_string());",
    ] {
        strong(&format!(
            "let actual = make_count(2);\n        let other = 2usize;\n        insta::assert_json_snapshot!(other);\n        {assertion}"
        ))?;
    }
    Ok(())
}

#[test]
fn snapshot_names_and_expected_text_cannot_supply_the_subject() -> Result<(), String> {
    for assertion in [
        "insta::assert_json_snapshot!(\"actual\", other);",
        "insta::assert_snapshot!(other, @\"actual make_count(2)\");",
        "insta::assert_json_snapshot!((actual, other).1);",
        "insta::assert_json_snapshot!({ let actual = other; actual });",
    ] {
        weak(&format!(
            "let actual = make_count(2);\n        let other = 2usize;\n        {assertion}"
        ))?;
    }
    Ok(())
}

#[test]
fn changed_or_escaped_result_bindings_cannot_supply_snapshot_credit() -> Result<(), String> {
    for body in [
        "let actual = make_count(2);\n        let actual = 2usize;\n        insta::assert_json_snapshot!(actual);",
        "let mut actual = make_count(2);\n        actual = 2;\n        insta::assert_json_snapshot!(actual);",
        "let mut actual = make_count(2);\n        let _ = &mut actual;\n        insta::assert_json_snapshot!(actual);",
        "let actual = make_count(2);\n        inspect(&actual);\n        insta::assert_json_snapshot!(actual);",
        "let actual = make_count(2);\n        rewrite!();\n        insta::assert_json_snapshot!(actual);",
    ] {
        weak(body)?;
    }
    Ok(())
}
