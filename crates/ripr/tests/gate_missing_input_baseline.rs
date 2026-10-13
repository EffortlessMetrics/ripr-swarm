//! Missing-input wording when `ripr gate evaluate --baseline` is passed
//! without a candidate source (#7654).
//!
//! `--baseline` is a comparison input. The usage error must acknowledge
//! that it was received so the operator is not told they passed nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_dir(label: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .map_err(|err| format!("clock: {err}"))?;
    let dir = std::env::temp_dir().join(format!(
        "ripr-gate-baseline-{label}-{stamp}-{}-{}",
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

fn assert_missing_input_exit(output: &Output, stderr: &str) -> Result<(), String> {
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
    Ok(())
}

#[test]
fn gate_evaluate_baseline_without_candidate_source_acknowledges_baseline() -> Result<(), String> {
    let dir = unique_dir("received")?;
    let baseline = dir.join("does-not-exist.json");
    let output = run_gate(
        &dir,
        &[
            "gate",
            "evaluate",
            "--baseline",
            &baseline.display().to_string(),
        ],
    )?;
    let stderr = stderr_text(&output);
    assert_missing_input_exit(&output, &stderr)?;
    if !stderr.contains("received --baseline") {
        return Err(format!(
            "missing-input stderr must acknowledge the passed --baseline: {stderr}"
        ));
    }
    if !stderr.contains("not a candidate source") {
        return Err(format!(
            "missing-input stderr must say --baseline is not a candidate source: {stderr}"
        ));
    }
    if !stderr.contains("requires at least one of --pr-guidance") {
        return Err(format!(
            "missing-input stderr must still name the candidate sources: {stderr}"
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn gate_evaluate_baseline_check_mode_without_baseline_does_not_claim_it_was_received()
-> Result<(), String> {
    let dir = unique_dir("no-baseline")?;
    let output = run_gate(&dir, &["gate", "evaluate", "--mode", "baseline-check"])?;
    let stderr = stderr_text(&output);
    assert_missing_input_exit(&output, &stderr)?;
    if stderr.contains("received --baseline") {
        return Err(format!(
            "absent --baseline must not be acknowledged as received: {stderr}"
        ));
    }
    if !stderr.contains("requires at least one of --pr-guidance") {
        return Err(format!(
            "missing-input stderr lost the usage error: {stderr}"
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
