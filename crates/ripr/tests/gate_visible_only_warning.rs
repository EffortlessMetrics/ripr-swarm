//! Placement of the default visible-only gate warning (#7447).
//!
//! The warning is mode-disclosure for a completed evaluation. A missing-input
//! usage error never used that mode, so stderr must not lead with it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const VISIBLE_ONLY_WARNING_NEEDLE: &str = "using default mode visible-only";

fn unique_dir(label: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .map_err(|err| format!("clock: {err}"))?;
    let dir = std::env::temp_dir().join(format!(
        "ripr-gate-warning-{label}-{stamp}-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|err| format!("mkdir {}: {err}", dir.display()))?;
    Ok(dir)
}

fn run_gate(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|err| format!("run ripr {args:?}: {err}"))
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn gate_evaluate_missing_input_does_not_emit_visible_only_warning() -> Result<(), String> {
    let dir = unique_dir("missing-input")?;
    let output = run_gate(&dir, &["gate", "evaluate"])?;
    let stderr = stderr_text(&output);
    if output.status.success() {
        return Err(format!(
            "expected missing-input to fail; stdout={:?} stderr={stderr}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    if output.status.code() != Some(2) {
        return Err(format!(
            "expected exit 2 for missing input, got {:?}; stderr={stderr}",
            output.status.code()
        ));
    }
    if !stderr.contains("gate evaluate requires at least one of --pr-guidance") {
        return Err(format!(
            "missing-input stderr lost the usage error: {stderr}"
        ));
    }
    if stderr.contains(VISIBLE_ONLY_WARNING_NEEDLE) {
        return Err(format!(
            "missing-input exit 2 must not emit the unused-mode warning: {stderr}"
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

const CLEAN_PR_GUIDANCE_JSON: &str = r#"{
  "schema_version": "0.1",
  "tool": "ripr",
  "status": "advisory",
  "summary": {"unchanged_tests": true},
  "comments": [],
  "summary_only": [],
  "suppressed": []
}"#;

#[test]
fn gate_evaluate_config_error_does_not_emit_visible_only_warning() -> Result<(), String> {
    let dir = unique_dir("config-error")?;
    let guidance = dir.join("bad.json");
    std::fs::write(&guidance, b"[")
        .map_err(|err| format!("write {}: {err}", guidance.display()))?;
    let out = dir.join("gate-decision.json");
    let output = run_gate(
        &dir,
        &[
            "gate",
            "evaluate",
            "--pr-guidance",
            &guidance.display().to_string(),
            "--out",
            &out.display().to_string(),
        ],
    )?;
    let stderr = stderr_text(&output);
    if output.status.code() != Some(2) {
        return Err(format!(
            "expected exit 2 for config_error, got {:?}; stderr={stderr}",
            output.status.code()
        ));
    }
    if !stderr.contains("config_error") {
        return Err(format!("config_error stderr lost the decision: {stderr}"));
    }
    if stderr.contains(VISIBLE_ONLY_WARNING_NEEDLE) {
        return Err(format!(
            "config_error exit 2 must not emit the unused-mode warning: {stderr}"
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn gate_evaluate_completed_visible_only_run_still_emits_default_mode_warning() -> Result<(), String>
{
    let dir = unique_dir("completed-advisory")?;
    let guidance = dir.join("comments.json");
    std::fs::write(&guidance, CLEAN_PR_GUIDANCE_JSON)
        .map_err(|err| format!("write {}: {err}", guidance.display()))?;
    let out = dir.join("gate-decision.json");
    let output = run_gate(
        &dir,
        &[
            "gate",
            "evaluate",
            "--pr-guidance",
            &guidance.display().to_string(),
            "--out",
            &out.display().to_string(),
        ],
    )?;
    let stderr = stderr_text(&output);
    if !output.status.success() {
        return Err(format!(
            "expected a completed visible-only run to succeed; stderr={stderr}"
        ));
    }
    if !stderr.contains(VISIBLE_ONLY_WARNING_NEEDLE) {
        return Err(format!(
            "completed default-mode evaluation must still disclose visible-only: {stderr}"
        ));
    }
    if !stderr.contains("never blocks on findings") {
        return Err(format!(
            "visible-only warning must qualify that findings do not block: {stderr}"
        ));
    }
    if !stderr.contains("exception-policy violations still can") {
        return Err(format!(
            "visible-only warning must disclose that exception-policy can still block: {stderr}"
        ));
    }
    if stderr.contains("and never blocks.") {
        return Err(format!(
            "unqualified 'never blocks' contradicts exception-policy blocking in visible-only: {stderr}"
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
