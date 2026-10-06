//! #6824: a syntax error in the changed file must not collapse diff-mode
//! `ripr check` into a complete, limitations-empty `no_behavioral_candidates`
//! zero. A present-but-unparseable changed file — the normal mid-edit state
//! for a coding agent — must yield the typed `producer_failure` limitation
//! (the same shape the Rust twin emits) and a partial outcome.
//!
//! Python-only: a build without `lang-python` refuses a Python project
//! before any parse runs, so the failure shape is not observable there.

#![cfg(feature = "lang-python")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static WORKSPACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Best-effort fixture custody: the repo's `[env] TEMP=target` redirect puts
/// test fixtures inside the outer git worktree, so leaked fixtures slow
/// every later git invocation in the tree. The guard removes the root on
/// scope exit, success or failure.
struct FixtureGuard {
    root: PathBuf,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run_ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?} failed: {error}"))
}

/// The issue's scenario shape: the changed file carries the predicate
/// mutation *plus* an appended invalid line, so the file the diff touches
/// cannot parse at all. With `broken = false` the file parses: the control.
fn write_workspace(broken: bool) -> Result<PathBuf, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "ripr-py-diff-parse-failure-{}-{call}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).map_err(|error| format!("mkdir: {error}"))?;
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"pricing\"\n",
    )
    .map_err(|error| format!("write marker: {error}"))?;
    let tail = if broken { "\ndef broken(:\n" } else { "" };
    fs::write(
        root.join("src/pricing.py"),
        format!(
            "def apply_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 1\n    return amount\n{tail}"
        ),
    )
    .map_err(|error| format!("write broken source: {error}"))?;
    fs::write(
        root.join("change.diff"),
        "\
diff --git a/src/pricing.py b/src/pricing.py
index 0000000..1111111 100644
--- a/src/pricing.py
+++ b/src/pricing.py
@@ -1,4 +1,5 @@
 def apply_discount(amount, threshold):
-    if amount > threshold:
+    if amount >= threshold:
         return amount - 1
     return amount
+
",
    )
    .map_err(|error| format!("write diff: {error}"))?;
    Ok(root)
}

#[test]
fn changed_file_syntax_error_is_a_named_partial_run_not_a_complete_zero() -> Result<(), String> {
    let root = write_workspace(true)?;
    let _guard = FixtureGuard { root: root.clone() };
    let output = run_ripr(
        root.join(".").as_path(),
        &[
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--format",
            "json",
        ],
    )?;
    let _ = fs::remove_dir_all(&root);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code().is_none() {
        return Err(format!(
            "ripr aborted instead of reporting the parse failure\nstderr: {stderr}\nstdout: {stdout}"
        ));
    }
    // The old collapse: a complete-looking zero with no parse disclosure.
    // The fixed run must instead name the producer failure and go partial,
    // asserted structurally against the typed outcome.
    if stdout.contains("no_behavioral_candidates") {
        return Err(format!(
            "an unparseable changed file must not end as no_behavioral_candidates\nstdout: {stdout}"
        ));
    }
    let document: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("check output is not JSON: {error}\nstdout: {stdout}"))?;
    if document
        .pointer("/analysis_outcome/outcome/kind")
        .and_then(|kind| kind.as_str())
        != Some("partial_with_limitations")
    {
        return Err(format!(
            "expected the partial outcome kind in the typed outcome\nstdout: {stdout}"
        ));
    }
    let limitations = document
        .pointer("/analysis_outcome/outcome/limitations")
        .and_then(|limitations| limitations.as_array())
        .ok_or_else(|| format!("the outcome lost its limitations\nstdout: {stdout}"))?;
    let [limitation] = limitations.as_slice() else {
        return Err(format!(
            "exactly one typed limitation expected, got {limitations:?}"
        ));
    };
    for (field, expected) in [
        ("kind", "producer_failure"),
        ("producer_stage", "language_adapter"),
        ("path", "src/pricing.py"),
    ] {
        if limitation.get(field).and_then(|value| value.as_str()) != Some(expected) {
            return Err(format!(
                "limitation {field} must be {expected:?}: {limitation}"
            ));
        }
    }
    if limitation
        .pointer("/recovery/kind")
        .and_then(|value| value.as_str())
        != Some("inspect_failure")
        || !limitation
            .pointer("/recovery/detail")
            .and_then(|value| value.as_str())
            .is_some_and(|detail| detail.contains("Fix the file so it parses as Python"))
    {
        return Err(format!(
            "the limitation must carry the typed fix-then-rerun recovery: {limitation}"
        ));
    }
    Ok(())
}

/// Control: a changed file that parses keeps the run complete with no
/// producer failure, so the disclosure tracks actual parse failures.
#[test]
fn parseable_changed_file_stays_a_complete_run() -> Result<(), String> {
    let root = write_workspace(false)?;
    let _guard = FixtureGuard { root: root.clone() };
    let output = run_ripr(
        root.join(".").as_path(),
        &[
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--format",
            "json",
        ],
    )?;
    let _ = fs::remove_dir_all(&root);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code().is_none() {
        return Err(format!(
            "ripr aborted on a parseable workspace\nstderr: {stderr}\nstdout: {stdout}"
        ));
    }
    if stdout.contains("producer_failure") {
        return Err(format!(
            "a parseable changed file must not disclose a producer failure\nstdout: {stdout}"
        ));
    }
    if !stdout.contains("\"complete_with_findings\"")
        && !stdout.contains("\"complete_no_findings\"")
    {
        return Err(format!(
            "a parseable changed file must complete the run\nstdout: {stdout}"
        ));
    }
    Ok(())
}
