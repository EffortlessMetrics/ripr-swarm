//! A Python-only or TypeScript-only edit on a multi-file Rust fixture must
//! not read the workspace's Rust sources in the interactive modes (#10338
//! item 2). Wall time on a hosted runner cannot gate that on every runner, so
//! this test uses a sentinel instead: a Rust file outside the diff that ripr
//! cannot read (not UTF-8) is named in a limitation whenever its sources are
//! scanned.
//!
//! The docs-only case lives in `no_rust_diff_reads_no_rust.rs` (#10360). This
//! file pins the remaining no-Rust path: a preview adapter actually runs
//! (`complete_with_findings`, at least one probe) while Rust stays
//! `no_current_scope` and the sentinel stays unnamed. `summary.changed_rust_files`
//! being 0 is not enough: that field is changed-file identity, not parse work.
//!
//! Negative control: a Rust edit on the same mixed fixture still names the
//! sentinel, so adding Python/TypeScript files did not hide the probe.
//!
//! Boundary: the probe observes workspace Rust that reaches the index. A read
//! that never feeds the index (for example a bare open that is discarded)
//! would not name the sentinel; this test does not claim to catch that.
//!
//! This file is separate from `no_rust_diff_reads_no_rust.rs` because that
//! docs-only / ready-mode surface is in flight on #12556 / #10353.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
const LIB_RS: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n}\n";
const TEST_RS: &str =
    "use fx::discount;\n#[test]\nfn small() {\n    assert!(discount(5, 2) > 0);\n}\n";
const RUST_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 12 { price * qty * 9 / 10 } else { price * qty }\n }\n";
const PYTHON_SRC: &str = "def helper(value):\n    return value + 1\n";
#[cfg(feature = "lang-python")]
const PYTHON_DIFF: &str = "diff --git a/tools.py b/tools.py\n--- a/tools.py\n+++ b/tools.py\n@@ -1,2 +1,2 @@\n def helper(value):\n-    return value\n+    return value + 1\n";
const TYPESCRIPT_SRC: &str =
    "export function helper(value: number): number {\n  return value + 1;\n}\n";
#[cfg(feature = "lang-typescript")]
const TYPESCRIPT_DIFF: &str = "diff --git a/tools.ts b/tools.ts\n--- a/tools.ts\n+++ b/tools.ts\n@@ -1,3 +1,3 @@\n export function helper(value: number): number {\n-  return value;\n+  return value + 1;\n }\n";
/// Not UTF-8, so any scan of the workspace's Rust sources names it.
const SENTINEL_NAME: &str = "sentinel_not_utf8.rs";

struct Repo {
    base: PathBuf,
    root: PathBuf,
}

impl Repo {
    fn new(label: &str) -> Result<Self, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "ripr-preview-no-rust-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("fx");
        fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        write(&root.join("Cargo.toml"), MANIFEST.as_bytes())?;
        write(&root.join("src/lib.rs"), LIB_RS.as_bytes())?;
        write(&root.join("tests/discount.rs"), TEST_RS.as_bytes())?;
        write(
            &root.join("src").join(SENTINEL_NAME),
            b"\xff\xfe pub fn x() {}\n",
        )?;
        // Only list languages this binary was built with; ripr refuses a
        // config naming a language whose Cargo feature is off.
        let mut languages = vec!["\"rust\""];
        if cfg!(feature = "lang-python") {
            languages.push("\"python\"");
        }
        if cfg!(feature = "lang-typescript") {
            languages.push("\"typescript\"");
        }
        write(
            &root.join("ripr.toml"),
            format!("[languages]\nenabled = [{}]\n", languages.join(", ")).as_bytes(),
        )?;
        write(&root.join("tools.py"), PYTHON_SRC.as_bytes())?;
        write(&root.join("tools.ts"), TYPESCRIPT_SRC.as_bytes())?;
        Ok(Self { base, root })
    }

    /// Runs `ripr check --mode <mode> --format json` for `diff` and returns
    /// the full report and the `analysis_outcome` object.
    fn check_report(
        &self,
        mode: &str,
        diff: &str,
    ) -> Result<(serde_json::Value, serde_json::Value), String> {
        let diff_path = self.base.join("change.diff");
        write(&diff_path, diff.as_bytes())?;
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--mode", mode, "--root"])
            .arg(&self.root)
            .arg("--diff")
            .arg(&diff_path)
            .args(["--format", "json"])
            .env("RIPR_CACHE_DIR", self.base.join("cache"))
            .output()
            .map_err(|e| format!("run ripr check: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr check failed: {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let report: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| format!("report is not JSON: {e}\n{stdout}"))?;
        let outcome = report
            .get("analysis_outcome")
            .cloned()
            .ok_or_else(|| format!("report has no analysis_outcome:\n{stdout}"))?;
        Ok((report, outcome))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
}

fn names_sentinel(outcome: &serde_json::Value) -> bool {
    outcome["outcome"]["limitations"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|limitation| {
            limitation["bounded_detail"]
                .as_str()
                .is_some_and(|detail| detail.contains(SENTINEL_NAME))
        })
}

fn changed_files_for_language(report: &serde_json::Value, language: &str) -> Option<u64> {
    report["summary"]["changed_files_by_language"]
        .as_array()?
        .iter()
        .find(|entry| entry["language"] == language)?["files"]
        .as_u64()
}

fn rust_completeness_status(report: &serde_json::Value) -> Option<&str> {
    report["language_completeness"]["languages"]
        .as_array()?
        .iter()
        .find(|row| row["language"] == "rust")
        .and_then(|row| row["status"].as_str())
}

fn rust_completeness_findings(report: &serde_json::Value) -> Option<u64> {
    report["language_completeness"]["languages"]
        .as_array()?
        .iter()
        .find(|row| row["language"] == "rust")
        .and_then(|row| row["findings"].as_u64())
}

/// #10338 item 2: a Python-only or TypeScript-only edit on a multi-file Rust
/// fixture must not index workspace Rust. The preview adapter must actually
/// run so a silent sentinel is not a skipped analysis.
#[cfg(any(feature = "lang-python", feature = "lang-typescript"))]
#[test]
fn preview_language_only_diff_does_not_scan_workspace_rust() -> Result<(), String> {
    let cases: &[(&str, &str, &str)] = &[
        #[cfg(feature = "lang-python")]
        ("python", PYTHON_DIFF, "python"),
        #[cfg(feature = "lang-typescript")]
        ("typescript", TYPESCRIPT_DIFF, "typescript"),
    ];
    assert!(
        !cases.is_empty(),
        "at least one preview-language feature must be enabled"
    );
    for mode in ["instant", "draft", "fast"] {
        for (label, diff, language) in cases {
            let repo = Repo::new(&format!("{label}-{mode}"))?;
            let (report, outcome) = repo.check_report(mode, diff)?;
            assert_eq!(
                report["summary"]["changed_rust_files"], 0,
                "{label}/{mode}: preview-language diff counted as changed Rust: {report}"
            );
            assert_eq!(
                changed_files_for_language(&report, language),
                Some(1),
                "{label}/{mode}: adapter did not admit the changed {language} file, so a silent sentinel would not prove a no-Rust parse: {report}"
            );
            assert!(
                outcome["outcome"]["counts"]["probe_count"]
                    .as_u64()
                    .is_some_and(|count| count >= 1),
                "{label}/{mode}: preview adapter produced no probes, so the run did not analyze the {language} change: {outcome}"
            );
            assert_eq!(
                rust_completeness_status(&report),
                Some("no_current_scope"),
                "{label}/{mode}: rust completeness must be no_current_scope on a no-Rust diff: {report}"
            );
            assert_eq!(
                rust_completeness_findings(&report),
                Some(0),
                "{label}/{mode}: rust findings on a no-Rust diff: {report}"
            );
            assert!(
                !names_sentinel(&outcome),
                "{label}/{mode}: a {language}-only diff scanned the workspace's Rust sources: {outcome}"
            );
        }
    }
    Ok(())
}

/// Negative control for the mixed fixture: adding Python/TypeScript files
/// must not hide the sentinel. A Rust edit on the same tree still names it,
/// so the preview-language silence above is a real skip, not a dead probe.
#[test]
fn rust_diff_on_mixed_preview_fixture_still_reaches_the_sentinel() -> Result<(), String> {
    let repo = Repo::new("mixed-control")?;
    let (_, outcome) = repo.check_report("draft", RUST_DIFF)?;
    assert!(
        names_sentinel(&outcome),
        "the sentinel is invisible on the mixed fixture, so the preview-language test proves nothing: {outcome}"
    );
    Ok(())
}
