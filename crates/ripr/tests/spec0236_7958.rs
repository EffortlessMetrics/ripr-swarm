//! RIPR-SPEC-0236 examples 17 and 23, as labeled in #7958.
//!
//! These cases keep their grip class; the seam evidence record must name
//! the spec's next step instead of a pre-spec record or category.

#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, OutputFormat, render_check};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create(label: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-7958-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_file(root: &Path, relative: &str, contents: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, contents).map_err(|error| error.to_string())
}

fn repo_exposure(root: PathBuf) -> Result<Value, String> {
    let output = ripr::app::repo_seam_inventory_input(CheckInput {
        root,
        ..CheckInput::default()
    });
    let rendered = render_check(&output, &OutputFormat::RepoExposureJson)?;
    serde_json::from_str(&rendered).map_err(|error| error.to_string())
}

fn seam_named<'a>(report: &'a Value, owner_suffix: &str) -> Result<&'a Value, String> {
    let seams = report["seams"]
        .as_array()
        .ok_or_else(|| format!("missing seams in {report}"))?;
    seams
        .iter()
        .find(|seam| {
            seam["owner"]
                .as_str()
                .is_some_and(|owner| owner.ends_with(owner_suffix))
                && seam["kind"] == "predicate_boundary"
        })
        .ok_or_else(|| format!("no predicate seam ending `{owner_suffix}`: {report}"))
}

fn limitation_categories(record: &Value) -> Vec<&str> {
    record["static_limitations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["category"].as_str())
        .collect()
}

/// RIPR-SPEC-0236 example 17 / corpus `spec0236-seam-boundary-direct-assert`.
#[test]
fn spec0236_example_17_direct_assert_is_actionable_related_test_extension() -> Result<(), String> {
    let scratch = Scratch::create("ex17")?;
    write_file(
        &scratch.0,
        "Cargo.toml",
        "[package]\nname = \"spec0236_17\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )?;
    write_file(
        &scratch.0,
        "src/lib.rs",
        "pub fn suffix(v: u64) -> &'static str {\n    if v > 1 { \"items\" } else { \"item\" }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn suffix_three_reads_items() {\n        assert_eq!(suffix(3), \"items\");\n    }\n}\n",
    )?;

    let report = repo_exposure(scratch.0.clone())?;
    assert_eq!(report["run_status"], "complete", "{report}");
    let seam = seam_named(&report, "::suffix")?;
    let record = &seam["evidence_record"];
    let evidence = &seam["evidence"];

    assert_eq!(
        seam["grip_class"], "weakly_gripped",
        "example 17 class: {seam}"
    );
    for stage in ["reach", "activate", "propagate", "observe", "discriminate"] {
        assert_eq!(
            evidence[stage], "yes",
            "example 17 stage `{stage}`: {evidence}"
        );
    }
    let missing = seam["missing_discriminators"]
        .as_array()
        .ok_or_else(|| format!("example 17 missing_discriminators: {seam}"))?;
    assert_eq!(
        missing.len(),
        1,
        "example 17 missing discriminator count: {missing:?}"
    );
    assert_eq!(
        record["actionability"]["class"], "actionable_related_test_extension",
        "example 17 record must not stay static_limitation: {record}"
    );
    Ok(())
}

/// RIPR-SPEC-0236 example 23 / corpus `spec0236-seam-private-tier-macro-witness`.
#[test]
fn spec0236_example_23_macro_witness_category_is_opaque_static_evidence_only() -> Result<(), String>
{
    let scratch = Scratch::create("ex23")?;
    write_file(
        &scratch.0,
        "Cargo.toml",
        "[package]\nname = \"macro_reach\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    )?;
    write_file(
        &scratch.0,
        "src/lib.rs",
        "fn tier(total: u64) -> u64 {\n    if total >= 100 { 20 } else { 0 }\n}\n\nmacro_rules! priced {\n    ($total:expr) => {\n        tier($total)\n    };\n}\n\npub fn run(total: u64) -> u64 {\n    priced!(total)\n}\n",
    )?;
    write_file(
        &scratch.0,
        "tests/run.rs",
        "#[test]\nfn runs_large_order() {\n    assert_eq!(macro_reach::run(150), 20);\n}\n",
    )?;

    let report = repo_exposure(scratch.0.clone())?;
    assert_eq!(report["run_status"], "complete", "{report}");
    let seam = seam_named(&report, "::tier")?;
    let record = &seam["evidence_record"];
    let categories = limitation_categories(record);

    assert_eq!(seam["grip_class"], "opaque", "example 23 class: {seam}");
    assert!(
        record["evidence_path"]["reach"]["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("rust_macro_reach_unresolved")),
        "example 23 must name rust_macro_reach_unresolved: {seam}"
    );
    assert!(
        categories.contains(&"opaque_static_evidence"),
        "example 23 category: {categories:?} record={record}"
    );
    assert!(
        !categories.contains(&"macro_generated_value"),
        "example 23 must not keep macro_generated_value: {categories:?} record={record}"
    );
    Ok(())
}
