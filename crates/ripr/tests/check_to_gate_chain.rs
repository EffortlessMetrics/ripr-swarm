//! End-to-end producer chain for #1923.
//!
//! The generated `ripr init --ci github` workflow's analysis step is now
//! `ripr reports ci-packet --root .`. That command still runs this CLI
//! chain (`crates/ripr/src/cli/commands/ci_packet.rs`):
//!
//! 1. `check --format json` → `target/ripr/pr/check.json`
//! 2. `review-comments --check-output` → `target/ripr/review/comments.json`
//! 3. `gate evaluate --pr-guidance` (blocking when `RIPR_GATE_MODE` names
//!    `acknowledgeable`)
//!
//! Direct `check --json` → `gate evaluate` is not a supported gate input.
//! `--mode acknowledgeable` is the documented blocking mode from gate help,
//! the generated workflow, and `docs/EXIT_CODES.md`.

use serde_json::Value;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const CHECK_JSON: &str = "target/ripr/pr/check.json";
const COMMENTS_JSON: &str = "target/ripr/review/comments.json";
const GATE_JSON: &str = "target/ripr/reports/gate-decision.json";

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unique_root(label: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let parent = std::env::temp_dir();
    std::fs::create_dir_all(&parent)
        .map_err(|error| format!("create temp parent {}: {error}", parent.display()))?;
    Ok(parent.join(format!(
        "ripr-check-to-gate-{label}-{stamp}-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    )))
}

fn copy_fixture_source(root: &Path, relative: &str) -> Result<(), String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/boundary_gap/input")
        .join(relative);
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    std::fs::copy(&source, &destination).map_err(|error| {
        format!(
            "copy {} into {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn commit_fixture(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            message,
        ],
    )
    .map_err(|error| format!("fixture commit in {}: {error}", root.display()))
}

/// Temp git repo with the boundary-gap predicate changed on HEAD vs HEAD~1.
fn build_changed_repo(label: &str) -> Result<Fixture, String> {
    let root = unique_root(label)?;
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("create {}: {error}", root.display()))?;
    for relative in ["Cargo.toml", "src/lib.rs", "tests/pricing.rs"] {
        copy_fixture_source(&root, relative)?;
    }
    fixture_git_ok(&root, &["init"]).map_err(|error| format!("fixture git init: {error}"))?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])
        .map_err(|error| format!("fixture git config: {error}"))?;
    fixture_git_ok(
        &root,
        &["add", "Cargo.toml", "src/lib.rs", "tests/pricing.rs"],
    )
    .map_err(|error| format!("fixture git add: {error}"))?;
    commit_fixture(&root, "fixture")?;

    let source = root.join("src/lib.rs");
    let old = std::fs::read_to_string(&source)
        .map_err(|error| format!("read {}: {error}", source.display()))?;
    let changed = old.replace(
        "amount >= discount_threshold",
        "amount > discount_threshold",
    );
    if changed == old {
        return Err("fixture change did not alter its predicate".to_string());
    }
    std::fs::write(&source, changed)
        .map_err(|error| format!("write {}: {error}", source.display()))?;
    fixture_git_ok(&root, &["add", "src/lib.rs"])
        .map_err(|error| format!("fixture git add change: {error}"))?;
    commit_fixture(&root, "change the boundary predicate")?;
    Ok(Fixture(root))
}

fn diagnose(label: &str, output: &Output) -> String {
    format!(
        "{label} failed with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn run_ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?} in {}: {error}", root.display()))
}

/// Capture stdout into `dest`, matching `reports ci-packet`'s `ripr_to`.
fn run_ripr_to(root: &Path, args: &[&str], dest: &Path) -> Result<Output, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let file = File::create(dest).map_err(|error| format!("create {}: {error}", dest.display()))?;
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .stdout(Stdio::from(file))
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("spawn ripr {args:?} in {}: {error}", root.display()))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("parse {} as JSON: {error}\n{text}", path.display()))
}

fn mkdir_chain_dirs(root: &Path) -> Result<(), String> {
    for relative in [
        "target/ripr/pr",
        "target/ripr/review",
        "target/ripr/reports",
    ] {
        std::fs::create_dir_all(root.join(relative))
            .map_err(|error| format!("create {relative}: {error}"))?;
    }
    Ok(())
}

fn run_check_json(root: &Path, dest: &Path) -> Result<Value, String> {
    let output = run_ripr_to(
        root,
        &[
            "check", "--root", ".", "--base", "HEAD~1", "--format", "json",
        ],
        dest,
    )?;
    if !output.status.success() {
        return Err(diagnose("check --format json", &output));
    }
    let check = read_json(dest)?;
    if check.get("tool").and_then(Value::as_str) != Some("ripr") {
        return Err(format!("check JSON is not a ripr document: {check}"));
    }
    let findings = check
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("check JSON has no findings array: {check}"))?;
    if findings.is_empty() {
        return Err(format!(
            "check --format json produced no findings; the chain has no subject: {check}"
        ));
    }
    Ok(check)
}

/// `check --format json` → `review-comments --check-output` → `gate evaluate
/// --pr-guidance`, asserting both the blocking decision and exit 3.
#[test]
fn check_json_review_comments_gate_chain_blocks_and_exits_3() -> Result<(), String> {
    let fixture = build_changed_repo("happy")?;
    mkdir_chain_dirs(&fixture.0)?;
    let check_path = fixture.0.join(CHECK_JSON);
    let comments_path = fixture.0.join(COMMENTS_JSON);
    let gate_path = fixture.0.join(GATE_JSON);

    run_check_json(&fixture.0, &check_path)?;

    let comments_output = run_ripr(
        &fixture.0,
        &[
            "review-comments",
            "--root",
            ".",
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
            "--check-output",
            CHECK_JSON,
            "--out",
            COMMENTS_JSON,
        ],
    )?;
    if !comments_output.status.success() {
        return Err(diagnose("review-comments --check-output", &comments_output));
    }
    let comments = read_json(&comments_path)?;
    if comments.get("tool").and_then(Value::as_str) != Some("ripr") {
        return Err(format!(
            "review-comments did not write a ripr guidance document: {comments}"
        ));
    }
    let actionable = comments
        .get("comments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|card| card.get("gap_state").and_then(Value::as_str) == Some("actionable"));
    if !actionable {
        return Err(format!(
            "review-comments produced no actionable card; gate has no candidate: {comments}"
        ));
    }

    let gate_output = run_ripr(
        &fixture.0,
        &[
            "gate",
            "evaluate",
            "--root",
            ".",
            "--pr-guidance",
            COMMENTS_JSON,
            "--mode",
            "acknowledgeable",
            "--out",
            GATE_JSON,
        ],
    )?;
    let gate = read_json(&gate_path).map_err(|error| {
        format!(
            "{error}\n{}",
            diagnose("gate evaluate --pr-guidance", &gate_output)
        )
    })?;
    let status = gate
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("gate-decision.json has no status: {gate}"))?;
    let exit = gate_output.status.code();
    if status != "blocked" {
        return Err(format!(
            "acknowledgeable gate on an actionable review-comments document should be blocked, got {status:?} exit {exit:?}\ngate={gate}\n{}",
            diagnose("gate evaluate --pr-guidance", &gate_output)
        ));
    }
    if exit != Some(3) {
        return Err(format!(
            "blocked gate decision must exit 3 (docs/EXIT_CODES.md), got {exit:?}\ngate={gate}\n{}",
            diagnose("gate evaluate --pr-guidance", &gate_output)
        ));
    }
    Ok(())
}

/// Malformed or incomplete `check --format json` output must be refused by
/// `review-comments --check-output`. If that flag were ignored, review-comments
/// would re-analyze the repo and exit 0.
#[test]
fn malformed_or_incomplete_check_output_is_refused() -> Result<(), String> {
    let fixture = build_changed_repo("fail-closed")?;
    mkdir_chain_dirs(&fixture.0)?;
    let check_path = fixture.0.join(CHECK_JSON);
    let mut check = run_check_json(&fixture.0, &check_path)?;

    let malformed_path = fixture.0.join("target/ripr/pr/malformed-check.json");
    std::fs::write(&malformed_path, "{")
        .map_err(|error| format!("write malformed check: {error}"))?;
    assert_check_output_refused(
        &fixture.0,
        "target/ripr/pr/malformed-check.json",
        "target/ripr/review/malformed-comments.json",
        "JSON parse failed",
    )?;

    check["analysis_outcome"] = Value::Null;
    let incomplete_path = fixture.0.join("target/ripr/pr/incomplete-check.json");
    let incomplete = serde_json::to_vec(&check)
        .map_err(|error| format!("serialize incomplete check: {error}"))?;
    std::fs::write(&incomplete_path, incomplete)
        .map_err(|error| format!("write incomplete check: {error}"))?;
    assert_check_output_refused(
        &fixture.0,
        "target/ripr/pr/incomplete-check.json",
        "target/ripr/review/incomplete-comments.json",
        "analysis_outcome is null",
    )?;
    Ok(())
}

fn assert_check_output_refused(
    root: &Path,
    check_output: &str,
    comments_out: &str,
    expected_needle: &str,
) -> Result<(), String> {
    let comments_path = root.join(comments_out);
    let output = run_ripr(
        root,
        &[
            "review-comments",
            "--root",
            ".",
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
            "--check-output",
            check_output,
            "--out",
            comments_out,
        ],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        return Err(format!(
            "review-comments --check-output {check_output} should refuse; if --check-output is ignored the command re-analyzes and exits 0\n{}",
            diagnose("review-comments fail-closed", &output)
        ));
    }
    if output.status.code() != Some(2) {
        return Err(format!(
            "invalid --check-output should exit 2, got {:?}\n{}",
            output.status.code(),
            diagnose("review-comments fail-closed", &output)
        ));
    }
    if !stderr.contains("review-comments --check-output") || !stderr.contains("invalid") {
        return Err(format!(
            "fail-closed stderr lost the --check-output invalid diagnostic: {stderr}"
        ));
    }
    if !stderr.contains(expected_needle) {
        return Err(format!(
            "fail-closed stderr should name {expected_needle:?}: {stderr}"
        ));
    }
    if comments_path.exists() {
        return Err(format!(
            "refused --check-output must not publish {}: {}",
            comments_path.display(),
            std::fs::read_to_string(&comments_path).unwrap_or_default()
        ));
    }
    Ok(())
}
