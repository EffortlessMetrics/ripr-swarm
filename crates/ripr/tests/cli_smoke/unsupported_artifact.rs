//! Unsupported input must not publish reusable findings (#8212).
use super::{DeclaredHunkScratch, run_command, unique_temp_workspace};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Output;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const VALID: &str = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn value() -> u32 { 1 }\n+pub fn value() -> u32 { 2 }\n";
const TRUNCATED: &str = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n-pub fn value() -> u32 { 1 }\n+pub fn value() -> u32 { 2 }\n";

fn fixture(label: &str) -> Result<DeclaredHunkScratch, Box<dyn std::error::Error>> {
    let root = unique_temp_workspace(label);
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"artifact-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(root.join("src/lib.rs"), "pub fn value() -> u32 { 2 }\n")?;
    fs::write(root.join("valid.patch"), VALID)?;
    Ok(DeclaredHunkScratch(root))
}

fn check(root: &Path, patch: &str, artifact: Option<&str>) -> std::io::Result<Output> {
    let mut args = vec!["check", "--root", ".", "--diff", patch, "--json", "--quiet"];
    if let Some(path) = artifact {
        args.extend(["--write-artifact", path]);
    }
    run_command(env!("CARGO_BIN_EXE_ripr"), Some(root), &args)
}

fn context(root: &Path, artifact: &str) -> std::io::Result<Output> {
    run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        &[
            "context",
            "--root",
            ".",
            "--from",
            artifact,
            "--at",
            "src/lib.rs:1",
        ],
    )
}

fn assert_unsupported(output: &Output, findings: usize) -> TestResult {
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(report["schema_version"], "0.2");
    assert_eq!(report["analysis_outcome"]["analysis_complete"], false);
    assert_eq!(
        report["analysis_outcome"]["outcome"]["kind"],
        "unsupported_input"
    );
    assert_eq!(report["findings"].as_array().map(Vec::len), Some(findings));
    let limitations = report["analysis_outcome"]["outcome"]["limitations"]
        .as_array()
        .ok_or("limitations must be an array")?;
    assert!(
        limitations
            .iter()
            .any(|item| item["kind"] == "malformed_diff")
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported_input"));
    Ok(())
}

#[test]
fn unsupported_artifact_preserves_destinations_and_incomplete_report() -> TestResult {
    let scratch = fixture("unsupported-artifact-destinations")?;
    let root = &scratch.0;
    let mixed = format!(
        "{VALID}diff --git a/src/other.rs b/src/other.rs\n--- a/src/other.rs\n+++ b/src/other.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n"
    );
    for (label, patch, findings) in [
        ("garbage", "not a unified diff\n", 0),
        ("truncated", TRUNCATED, 1),
        ("mixed", mixed.as_str(), 1),
    ] {
        let patch_name = format!("{label}.patch");
        fs::write(root.join(&patch_name), patch)?;
        let ordinary = check(root, &patch_name, None)?;
        assert_unsupported(&ordinary, findings)?;
        for destination in ["new.json", "sentinel.json", "absent-parent/new.json"] {
            let sentinel = b"PREVIOUS_ARTIFACT_SENTINEL";
            if destination == "sentinel.json" {
                fs::write(root.join(destination), sentinel)?;
            }
            let refused = check(root, &patch_name, Some(destination))?;
            assert_unsupported(&refused, findings)?;
            assert_eq!(
                refused.stdout, ordinary.stdout,
                "{label}/{destination}: keep the original report and direct-input navigation"
            );
            assert_eq!(refused.stderr, ordinary.stderr, "{label}/{destination}");
            if destination == "sentinel.json" {
                assert_eq!(fs::read(root.join(destination))?, sentinel, "{label}");
            } else {
                assert!(
                    !root.join(destination).exists(),
                    "{label}: must not publish {destination}"
                );
            }
        }
        assert!(!root.join("absent-parent").exists());
        let reuse = context(root, "new.json")?;
        assert_eq!(reuse.status.code(), Some(2));
        let refusal: Value = serde_json::from_slice(&reuse.stdout)?;
        assert_eq!(
            refusal["analysis_scope"]["run_status"],
            "check_artifact_unusable"
        );
        assert_eq!(refusal["analysis_scope"]["downstream_consumable"], false);
    }
    Ok(())
}

#[test]
fn unsupported_artifact_preserves_valid_artifact_and_its_context() -> TestResult {
    let scratch = fixture("unsupported-artifact-valid-preserved")?;
    let root = &scratch.0;
    let accepted = check(root, "valid.patch", Some("accepted.json"))?;
    assert_eq!(accepted.status.code(), Some(0));
    let report: Value = serde_json::from_slice(&accepted.stdout)?;
    assert_eq!(
        report["analysis_outcome"]["outcome"]["kind"],
        "complete_with_findings"
    );
    assert_eq!(report["findings"].as_array().map(Vec::len), Some(1));
    let bytes = fs::read(root.join("accepted.json"))?;
    let artifact: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(artifact["schema_version"], "ripr-check-artifact-v1");
    let before = context(root, "accepted.json")?;
    assert_eq!(before.status.code(), Some(0));
    let packet: Value = serde_json::from_slice(&before.stdout)?;
    assert_eq!(packet["source_currentness"], "candidate_current");
    assert!(before.stderr.is_empty());
    fs::write(root.join("bad.patch"), TRUNCATED)?;
    let refused = check(root, "bad.patch", Some("accepted.json"))?;
    assert_unsupported(&refused, 1)?;
    assert_eq!(fs::read(root.join("accepted.json"))?, bytes);
    let after = context(root, "accepted.json")?;
    assert_eq!(after.status.code(), Some(0));
    assert_eq!(after.stdout, before.stdout);
    assert_eq!(after.stderr, before.stderr);
    Ok(())
}

#[test]
fn unsupported_artifact_keeps_advisory_partial_write_and_reuse() -> TestResult {
    let scratch = fixture("unsupported-artifact-partial")?;
    let root = &scratch.0;
    fs::write(
        root.join("partial.patch"),
        format!(
            "{VALID}diff --git a/src/absent.rs b/src/absent.rs\n--- a/src/absent.rs\n+++ b/src/absent.rs\n@@ -0,0 +1 @@\n+pub fn absent() -> u32 {{ 1 }}\n"
        ),
    )?;
    let partial = check(root, "partial.patch", Some("partial.json"))?;
    assert_eq!(partial.status.code(), Some(0));
    let report: Value = serde_json::from_slice(&partial.stdout)?;
    assert_eq!(report["analysis_outcome"]["analysis_complete"], false);
    assert_eq!(
        report["analysis_outcome"]["outcome"]["kind"],
        "partial_with_limitations"
    );
    assert_eq!(report["findings"].as_array().map(Vec::len), Some(1));
    let artifact: Value = serde_json::from_slice(&fs::read(root.join("partial.json"))?)?;
    assert_eq!(artifact["schema_version"], "ripr-check-artifact-v1");
    assert_eq!(artifact["findings"].as_array().map(Vec::len), Some(1));
    let reused = context(root, "partial.json")?;
    assert_eq!(reused.status.code(), Some(0));
    let packet: Value = serde_json::from_slice(&reused.stdout)?;
    assert_eq!(packet["source_currentness"], "candidate_current");
    Ok(())
}
