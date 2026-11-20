//! Placement of the default visible-only gate warning (#7447).
//!
//! The warning is mode-disclosure for a completed evaluation. A missing-input
//! usage error never used that mode, so stderr must not lead with it.
//!
//! Also pins the Markdown posture note (#7445): a visible-only report that
//! recorded nothing still reads `advisory`, and says the token reflects the
//! mode rather than a finding.

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

/// Needle from the Markdown posture note (#7445). The note is rendered only
/// when a visible-only report reads `advisory` with nothing recorded.
const POSTURE_NOTE_NEEDLE: &str = "so the status reflects the mode, not a finding";

struct GateRun {
    status: String,
    mode: String,
    advisory: u64,
    markdown: String,
    json: String,
}

fn evaluate_guidance(label: &str, guidance: &Path, mode: &str) -> Result<GateRun, String> {
    let dir = unique_dir(label)?;
    let out = dir.join("gate-decision.json");
    let out_md = dir.join("gate-decision.md");
    let output = run_gate(
        &dir,
        &[
            "gate",
            "evaluate",
            "--pr-guidance",
            &guidance.display().to_string(),
            "--mode",
            mode,
            "--out",
            &out.display().to_string(),
            "--out-md",
            &out_md.display().to_string(),
        ],
    )?;
    if !output.status.success() {
        return Err(format!(
            "{label}: expected exit 0, got {:?}; stderr={}",
            output.status.code(),
            stderr_text(&output)
        ));
    }
    let json = std::fs::read_to_string(&out).map_err(|err| format!("read {label} json: {err}"))?;
    let markdown =
        std::fs::read_to_string(&out_md).map_err(|err| format!("read {label} md: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|err| format!("parse {label} json: {err}"))?;
    let run = GateRun {
        status: value["status"].as_str().unwrap_or("").to_string(),
        mode: value["mode"].as_str().unwrap_or("").to_string(),
        advisory: value["summary"]["advisory"].as_u64().unwrap_or(u64::MAX),
        markdown,
        json,
    };
    let _ = std::fs::remove_dir_all(&dir);
    Ok(run)
}

fn write_clean_guidance(label: &str) -> Result<(PathBuf, PathBuf), String> {
    let dir = unique_dir(label)?;
    let guidance = dir.join("comments.json");
    std::fs::write(&guidance, CLEAN_PR_GUIDANCE_JSON)
        .map_err(|err| format!("write {}: {err}", guidance.display()))?;
    Ok((dir, guidance))
}

/// #7445: the status vocabulary is unchanged (a clean visible-only run still
/// reads `advisory`), and the Markdown report now says the token reflects the
/// mode and names the enforcing modes.
#[test]
fn clean_visible_only_report_stays_advisory_and_markdown_discloses_mode_posture()
-> Result<(), String> {
    let (dir, guidance) = write_clean_guidance("posture-clean")?;
    let run = evaluate_guidance("posture-clean-run", &guidance, "visible-only")?;
    if run.status != "advisory" || run.mode != "visible-only" || run.advisory != 0 {
        return Err(format!(
            "clean visible-only run must stay advisory with zero advisory candidates: {}",
            run.json
        ));
    }
    if !run
        .markdown
        .starts_with("# RIPR Gate Decision\n\nDecision: advisory\nMode: visible-only\n")
    {
        return Err(format!("heading changed: {}", run.markdown));
    }
    if run.markdown.matches(POSTURE_NOTE_NEEDLE).count() != 1 {
        return Err(format!(
            "clean visible-only Markdown must carry the posture note once: {}",
            run.markdown
        ));
    }
    for needle in [
        "Advisory: 0\n\nVisible-only mode never blocks on findings",
        "`--mode acknowledgeable`",
        "`--mode baseline-check`",
        "`--mode calibrated-gate`",
    ] {
        if !run.markdown.contains(needle) {
            return Err(format!("posture note lost {needle:?}: {}", run.markdown));
        }
    }
    // The JSON contract is untouched: the note is Markdown-only.
    if run.json.contains(POSTURE_NOTE_NEEDLE) {
        return Err(format!("posture note leaked into JSON: {}", run.json));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// #7445 negatives: an enforcing clean run reads `pass` and a visible-only run
/// with an advisory candidate reads `advisory` for a finding; neither carries
/// the posture note.
#[test]
fn posture_note_is_withheld_for_enforcing_pass_and_for_recorded_findings() -> Result<(), String> {
    let (dir, guidance) = write_clean_guidance("posture-enforcing")?;
    let enforcing = evaluate_guidance("posture-enforcing-run", &guidance, "acknowledgeable")?;
    if enforcing.status != "pass" {
        return Err(format!(
            "clean acknowledgeable run must read pass: {}",
            enforcing.json
        ));
    }
    if enforcing.markdown.contains(POSTURE_NOTE_NEEDLE) {
        return Err(format!(
            "pass report must not carry the visible-only note: {}",
            enforcing.markdown
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);

    let finding_guidance = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/boundary_gap/expected/pr-guidance/exact-line/comments.json");
    let with_finding = evaluate_guidance("posture-finding-run", &finding_guidance, "visible-only")?;
    if with_finding.status != "advisory" || with_finding.advisory != 1 {
        return Err(format!(
            "fixture guidance must yield one advisory candidate in visible-only: {}",
            with_finding.json
        ));
    }
    if with_finding.markdown.contains(POSTURE_NOTE_NEEDLE) {
        return Err(format!(
            "a report with a recorded finding must not carry the posture note: {}",
            with_finding.markdown
        ));
    }
    Ok(())
}
