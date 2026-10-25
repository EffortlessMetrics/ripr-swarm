//! A diff with no analyzable source file must not read the workspace's Rust
//! sources in the interactive modes (#10338). Wall time on a hosted runner
//! cannot gate that on every runner, so this test uses a sentinel instead:
//! a Rust file outside the diff that ripr cannot read (not UTF-8) is named in
//! a limitation whenever its sources are scanned. A docs-only diff must not
//! mention it; the same sentinel must appear once the diff touches Rust, so
//! the silence is not an insensitive probe.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
const LIB_RS: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n}\n";
const TEST_RS: &str =
    "use fx::discount;\n#[test]\nfn small() {\n    assert!(discount(5, 2) > 0);\n}\n";
const README_DIFF: &str = "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1,3 +1,3 @@\n # fx\n \n-old\n+new\n";
const RUST_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 12 { price * qty * 9 / 10 } else { price * qty }\n }\n";
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
            "ripr-no-rust-diff-{label}-{}-{id}",
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
        write(&root.join("README.md"), b"# fx\n\nold\n")?;
        Ok(Self { base, root })
    }

    /// Runs `ripr check --mode <mode> --format json` for `diff` and returns
    /// the `analysis_outcome` object plus stderr.
    fn check(&self, mode: &str, diff: &str) -> Result<(serde_json::Value, String), String> {
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
        Ok((
            outcome,
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
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

#[test]
fn docs_only_diff_does_not_scan_workspace_rust_in_interactive_modes() -> Result<(), String> {
    for mode in ["instant", "draft", "fast"] {
        let repo = Repo::new(mode)?;
        let (outcome, _) = repo.check(mode, README_DIFF)?;
        assert_eq!(outcome["analysis_complete"], true, "{mode}: {outcome}");
        assert_eq!(
            outcome["outcome"]["kind"], "no_behavioral_candidates",
            "{mode}: {outcome}"
        );
        assert!(
            !names_sentinel(&outcome),
            "{mode}: a docs-only diff scanned the workspace's Rust sources: {outcome}"
        );
    }
    Ok(())
}

/// Positive control for the sentinel: once the diff touches Rust, the
/// interactive scan does reach the unreadable file outside the diff, so its
/// absence above is evidence and not an insensitive probe.
#[test]
fn rust_diff_does_reach_the_sentinel_in_draft_mode() -> Result<(), String> {
    let repo = Repo::new("control")?;
    let (outcome, _) = repo.check("draft", RUST_DIFF)?;
    assert!(
        names_sentinel(&outcome),
        "the sentinel is invisible to a Rust diff, so the docs-only test proves nothing: {outcome}"
    );
    Ok(())
}
