//! `check --json` projects the shared language completeness view (#11272), so
//! a completed zero-result run is distinguishable from a run with nothing in
//! scope or a language that was never dispatched.
use super::{DeclaredHunkScratch, run_ripr, unique_temp_workspace, workspace_root};
use serde_json::Value;
use std::fs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const RUST_BOUNDARY_PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n-    if amount > discount_threshold {\n+    if amount >= discount_threshold {\n         amount - 10\n     } else {\n         amount\n";
const RUST_COMMENT_PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,4 @@\n+// a comment only\n pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n     if amount >= discount_threshold {\n         amount - 10\n";
const MARKDOWN_ONLY_PATCH: &str = "diff --git a/README.md b/README.md\nindex 0000000..1111111 100644\n--- a/README.md\n+++ b/README.md\n@@ -1,1 +1,2 @@\n old\n+new\n";

/// Runs `check --format json` over the copied Rust fixture with `patch`.
fn check_with_patch(label: &str, patch: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let root = unique_temp_workspace(label);
    let _cleanup = DeclaredHunkScratch(root.clone());
    let fixture = workspace_root().join("fixtures/boundary_gap/input");
    for directory in ["src", "tests"] {
        fs::create_dir_all(root.join(directory))?;
    }
    for relative in ["Cargo.toml", "src/lib.rs", "tests/pricing.rs"] {
        fs::copy(fixture.join(relative), root.join(relative))?;
    }
    fs::write(root.join("change.patch"), patch)?;
    let root_arg = root.to_string_lossy().into_owned();
    let diff_arg = root.join("change.patch").to_string_lossy().into_owned();
    let output = run_ripr(&[
        "check", "--root", &root_arg, "--diff", &diff_arg, "--mode", "draft", "--format", "json",
        "--quiet",
    ]);
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn rust_row(report: &Value) -> Result<&Value, Box<dyn std::error::Error>> {
    let projection = &report["language_completeness"];
    let rows = projection["languages"]
        .as_array()
        .ok_or("language_completeness.languages must be an array")?;
    rows.iter()
        .find(|row| row["language"] == "rust")
        .ok_or_else(|| "no rust row in language_completeness".into())
}

#[test]
fn a_findings_run_names_the_completed_language_and_its_count() -> TestResult {
    let report = check_with_patch("lang-completeness-findings", RUST_BOUNDARY_PATCH)?;
    let projection = &report["language_completeness"];
    assert_eq!(projection["mode"], "diff");
    assert_eq!(projection["scope"], "diff");
    assert_eq!(projection["completeness"], "complete");
    assert_eq!(projection["total_findings"], 1);
    let row = rust_row(&report)?;
    assert_eq!(row["status"], "complete");
    assert_eq!(row["findings"], 1);
    // The projection is additive: the compatibility rows stay silent for a
    // successful run.
    assert!(report.get("language_runs").is_none(), "{report}");
    Ok(())
}

#[test]
fn a_complete_zero_run_is_distinct_from_nothing_in_scope() -> TestResult {
    let zero = check_with_patch("lang-completeness-zero", RUST_COMMENT_PATCH)?;
    let nothing = check_with_patch("lang-completeness-no-scope", MARKDOWN_ONLY_PATCH)?;
    // Both report an empty findings array and no `language_runs`.
    for report in [&zero, &nothing] {
        assert_eq!(report["findings"].as_array().map(Vec::len), Some(0));
        assert!(report.get("language_runs").is_none(), "{report}");
    }
    let zero_row = rust_row(&zero)?;
    assert_eq!(zero_row["status"], "complete_zero");
    assert_eq!(zero_row["findings"], 0);
    assert_eq!(zero["language_completeness"]["completeness"], "complete");
    let nothing_row = rust_row(&nothing)?;
    assert_eq!(nothing_row["status"], "no_current_scope");
    assert_ne!(zero_row["status"], nothing_row["status"]);
    Ok(())
}
