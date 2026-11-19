//! #14487: `ripr check` and `ripr gate check` agree on a boundary that a
//! test covers with two owner calls on one line.
//!
//! The probe path reads every same-line free-owner call as its own input row
//! (#13373); the seam path that feeds review-comments and the gate used to
//! read only the first, so `check` said `exposed` while the gate carried a
//! weak-grip decision naming the second input as a missing equality
//! boundary. The negative control keeps the gap when no call on the line
//! hits the boundary.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn commit(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "-am",
            message,
        ],
    )
}

/// A one-crate repo whose HEAD changes `ok` from `n > 10` to `n > 11`, with
/// `test_line` as the single-line body of the only integration test.
fn changed_repo(label: &str, test_line: &str) -> Result<Fixture, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-check-gate-same-line-{label}-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let fixture = Fixture(root.clone());
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"ctl\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", "pub fn ok(n: u32) -> bool { n > 10 }\n")?;
    write(
        "tests/t.rs",
        &format!("use ctl::ok;\n#[test]\nfn b() {{ {test_line} }}\n"),
    )?;
    fixture_git_ok(&root, &["init"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    fixture_git_ok(&root, &["add", "-A"])?;
    commit(&root, "base")?;
    write("src/lib.rs", "pub fn ok(n: u32) -> bool { n > 11 }\n")?;
    commit(&root, "move the boundary")?;
    Ok(fixture)
}

fn run_ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn diagnose(label: &str, output: &Output) -> String {
    format!(
        "{label} exited {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The classification of every `ripr check --format json` finding.
fn check_classes(root: &Path) -> Result<Vec<String>, String> {
    let output = run_ripr(root, &["check", "--base", "HEAD~1", "--format", "json"])?;
    if !output.status.success() {
        return Err(diagnose("check --format json", &output));
    }
    let check: Value =
        serde_json::from_slice(&output.stdout).map_err(|error| format!("check JSON: {error}"))?;
    let findings = check
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("check JSON has no findings: {check}"))?;
    if findings.is_empty() {
        return Err(format!("check produced no findings, no subject: {check}"));
    }
    findings
        .iter()
        .map(|finding| {
            finding
                .get("classification")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("finding without classification: {finding}"))
        })
        .collect()
}

/// `ripr gate check` in visible-only mode, so the report is written and the
/// process exits 0 whatever the decision; returns the decision document.
fn gate_decision(root: &Path) -> Result<Value, String> {
    let output = run_ripr(
        root,
        &[
            "gate",
            "check",
            "--base",
            "HEAD~1",
            "--mode",
            "visible-only",
        ],
    )?;
    if !output.status.success() {
        return Err(diagnose("gate check", &output));
    }
    let path = root.join("target/ripr/reports/gate-decision.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("gate JSON: {error}\n{text}"))
}

fn decision_classes(gate: &Value) -> Result<Vec<String>, String> {
    gate.get("decisions")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("gate report has no decisions array: {gate}"))?
        .iter()
        .map(|decision| {
            Ok(decision
                .get("static_class")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .to_string())
        })
        .collect()
}

#[test]
fn same_line_boundary_call_reads_exposed_through_check_and_the_gate() -> Result<(), String> {
    let fixture = changed_repo(
        "covered",
        "assert_eq!(ok(12), true); assert_eq!(ok(11), false);",
    )?;
    let classes = check_classes(&fixture.0)?;
    if classes.iter().any(|class| class != "exposed") {
        return Err(format!(
            "check must read every finding exposed: {classes:?}"
        ));
    }
    let gate = gate_decision(&fixture.0)?;
    let decisions = decision_classes(&gate)?;
    if !decisions.is_empty() {
        return Err(format!(
            "the gate must carry no gap for a change check calls exposed, got {decisions:?}: {gate}"
        ));
    }
    let unknown = gate
        .pointer("/summary/unknown_confidence")
        .and_then(Value::as_u64);
    if unknown != Some(0) {
        return Err(format!("unknown_confidence must be 0, got {unknown:?}"));
    }
    Ok(())
}

#[test]
fn same_line_calls_off_the_boundary_keep_the_gap_in_both_views() -> Result<(), String> {
    let fixture = changed_repo(
        "uncovered",
        "assert_eq!(ok(12), true); assert_eq!(ok(13), true);",
    )?;
    let classes = check_classes(&fixture.0)?;
    if !classes.iter().any(|class| class == "weakly_exposed") {
        return Err(format!(
            "check must keep the missing boundary weakly exposed: {classes:?}"
        ));
    }
    let gate = gate_decision(&fixture.0)?;
    let decisions = decision_classes(&gate)?;
    if !decisions.iter().any(|class| class == "weakly_gripped") {
        return Err(format!(
            "the gate must keep a weak-grip gap for the untested boundary, got {decisions:?}: {gate}"
        ));
    }
    Ok(())
}
