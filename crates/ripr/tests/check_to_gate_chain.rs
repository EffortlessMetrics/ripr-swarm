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
    run_ripr_env(root, args, &[])
}

/// `run_ripr` with explicit environment overrides. The GitHub Actions run
/// variables `reports ci-packet` reads are removed first, so a hosted test
/// run cannot leak its own event into the packet under test.
fn run_ripr_env(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(root).args(args);
    if !env.is_empty() {
        for name in [
            "GITHUB_EVENT_NAME",
            "GITHUB_EVENT_PATH",
            "GITHUB_BASE_REF",
            "GITHUB_REPOSITORY",
            "GITHUB_ACTOR",
            "RIPR_GATE_BASELINE",
            "RIPR_COMMENT_MODE",
            "RIPR_UPLOAD_SARIF",
            "RIPR_PILOT_TIMEOUT_MS",
        ] {
            command.env_remove(name);
        }
        for (name, value) in env {
            command.env(name, value);
        }
    }
    command
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

    // #14841: the CI packet runs this same gate as a required step and must
    // pass the blocked decision through as exit 3, not flatten it to the
    // could-not-complete exit 2. This drives the binary's real dispatch.
    let packet_output = run_ripr_env(
        &fixture.0,
        &[
            "reports",
            "ci-packet",
            "--root",
            ".",
            "--step",
            "Evaluate RIPR gate decision",
        ],
        &[("RIPR_GATE_MODE", "acknowledgeable")],
    )?;
    let packet_gate = read_json(&gate_path)?;
    if packet_gate.get("status").and_then(Value::as_str) != Some("blocked") {
        return Err(format!(
            "ci-packet gate step should have written a blocked decision: {packet_gate}\n{}",
            diagnose("reports ci-packet --step", &packet_output)
        ));
    }
    if packet_output.status.code() != Some(3) {
        return Err(format!(
            "ci-packet with a blocked gate must exit 3 (docs/EXIT_CODES.md), got {:?}\n{}",
            packet_output.status.code(),
            diagnose("reports ci-packet --step", &packet_output)
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

// ---- #13895: one suppression decision reaches review-comments and the gate ----

const RETRY_BASE: &str = "pub fn retry_allowed(attempts: u32) -> bool {\n    attempts < 3\n}\n\npub fn backoff_ms(attempts: u32) -> u32 {\n    if attempts > 2 { 500 } else { 100 }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() {\n        assert!(retry_allowed(0));\n    }\n}\n";

/// Line 2 is `retry_allowed`, line 6 is `backoff_ms`.
const RETRY_LINE: u64 = 2;
const BACKOFF_LINE: u64 = 6;

fn retry_source(retry_expression: &str, backoff_expression: &str) -> String {
    RETRY_BASE
        .replace("attempts < 3", retry_expression)
        .replace("attempts > 2", backoff_expression)
}

fn write_retry_source(root: &Path, source: &str) -> Result<(), String> {
    let path = root.join("src/retry.rs");
    std::fs::write(&path, source).map_err(|error| format!("write {}: {error}", path.display()))
}

/// Three commits on `src/retry.rs`: the base, then two successive edits of both
/// functions. Diffing `HEAD~1..HEAD` always changes both, so the suppressed
/// function and the unsuppressed control stay in the diff.
fn build_retry_repo(label: &str) -> Result<Fixture, String> {
    let root = unique_root(label)?;
    std::fs::create_dir_all(root.join("src"))
        .map_err(|error| format!("create {}: {error}", root.display()))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"retry_gate_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write Cargo.toml: {error}"))?;
    std::fs::write(root.join("src/lib.rs"), "pub mod retry;\n")
        .map_err(|error| format!("write lib.rs: {error}"))?;
    write_retry_source(&root, RETRY_BASE)?;
    fixture_git_ok(&root, &["init"]).map_err(|error| format!("fixture git init: {error}"))?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])
        .map_err(|error| format!("fixture git config: {error}"))?;
    fixture_git_ok(&root, &["add", "-A"]).map_err(|error| format!("fixture git add: {error}"))?;
    commit_fixture(&root, "base")?;
    write_retry_source(&root, &retry_source("attempts <= 3", "attempts > 3"))?;
    fixture_git_ok(&root, &["add", "-A"]).map_err(|error| format!("fixture git add: {error}"))?;
    commit_fixture(&root, "first edit")?;
    Ok(Fixture(root))
}

/// A second edit of the suppressed function's expression and of the control's.
fn edit_retry_expressions(root: &Path) -> Result<(), String> {
    write_retry_source(root, &retry_source("attempts <= 4", "attempts > 4"))?;
    fixture_git_ok(root, &["add", "-A"]).map_err(|error| format!("fixture git add: {error}"))?;
    commit_fixture(root, "second edit")
}

fn write_policy(root: &Path, relative: &str, entries: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    std::fs::write(&path, format!("schema_version = 1\n{entries}"))
        .map_err(|error| format!("write {}: {error}", path.display()))
}

fn function_entry(function: &str) -> String {
    format!(
        "\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"src/retry.rs\"\nfunction = \"{function}\"\nreason = \"accepted retry debt\"\nowner = \"team-retry\"\n"
    )
}

fn run_ok(root: &Path, label: &str, args: &[&str]) -> Result<Output, String> {
    let output = run_ripr(root, args)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(diagnose(label, &output))
    }
}

fn review_comments_args<'a>(out: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "review-comments",
        "--root",
        ".",
        "--base",
        "HEAD~1",
        "--head",
        "HEAD",
        "--out",
        out,
    ];
    args.extend_from_slice(extra);
    args
}

/// `(key, line)` for every seam item the review-comments document lists.
fn listed_seam_lines(doc: &Value, key: &str) -> Vec<u64> {
    doc.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.pointer("/seam/line")
                .or_else(|| item.get("line"))
                .and_then(Value::as_u64)
        })
        .collect()
}

fn evaluate_gate(root: &Path, comments: &str) -> Result<(Value, Option<i32>), String> {
    let output = run_ripr(
        root,
        &[
            "gate",
            "evaluate",
            "--root",
            ".",
            "--pr-guidance",
            comments,
            "--mode",
            "acknowledgeable",
            "--out",
            GATE_JSON,
        ],
    )?;
    let gate = read_json(&root.join(GATE_JSON))
        .map_err(|error| format!("{error}\n{}", diagnose("gate evaluate", &output)))?;
    Ok((gate, output.status.code()))
}

fn gate_count(gate: &Value, field: &str) -> Result<u64, String> {
    gate.pointer(&format!("/summary/{field}"))
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("gate summary has no {field}: {gate}"))
}

/// Control: with no policy both functions are comment-eligible and the gate
/// blocks. With a `path` + `function` policy the suppressed function moves to
/// `suppressed` (reason `suppressed_by_policy`) and stops blocking while the
/// other function in the same file still blocks; covering both makes the gate
/// pass with exit 0.
#[test]
fn review_comments_suppression_policy_keeps_the_gate_from_blocking_on_suppressed_function()
-> Result<(), String> {
    let fixture = build_retry_repo("policy-gate")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;

    run_ok(
        root,
        "review-comments (control)",
        &review_comments_args(COMMENTS_JSON, &[]),
    )?;
    let control = read_json(&root.join(COMMENTS_JSON))?;
    let control_listed: Vec<u64> = ["comments", "summary_only"]
        .iter()
        .flat_map(|key| listed_seam_lines(&control, key))
        .collect();
    assert!(
        control_listed.contains(&RETRY_LINE) && control_listed.contains(&BACKOFF_LINE),
        "control run must list both functions, got {control_listed:?}: {control}"
    );
    let (control_gate, control_exit) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(control_exit, Some(3), "{control_gate}");
    assert_eq!(gate_count(&control_gate, "suppressed")?, 0);
    let control_blocking = gate_count(&control_gate, "blocking")?;
    assert!(control_blocking >= 2, "{control_gate}");

    write_policy(root, "policy.toml", &function_entry("retry_allowed"))?;
    run_ok(
        root,
        "review-comments --suppression-policy",
        &review_comments_args(COMMENTS_JSON, &["--suppression-policy", "policy.toml"]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    let suppressed_lines = listed_seam_lines(&doc, "suppressed");
    assert!(
        !suppressed_lines.is_empty() && suppressed_lines.iter().all(|line| *line == RETRY_LINE),
        "only retry_allowed seams may be suppressed, got {suppressed_lines:?}: {doc}"
    );
    for key in ["comments", "summary_only"] {
        assert!(
            !listed_seam_lines(&doc, key).contains(&RETRY_LINE),
            "suppressed seam still listed in {key}: {doc}"
        );
    }
    assert!(
        listed_seam_lines(&doc, "comments")
            .iter()
            .chain(listed_seam_lines(&doc, "summary_only").iter())
            .any(|line| *line == BACKOFF_LINE),
        "negative control: the other function in the same file stays listed: {doc}"
    );
    for item in doc["suppressed"].as_array().into_iter().flatten() {
        assert_eq!(item["reason"], "suppressed_by_policy", "{item}");
        assert!(
            item["canonical_gap_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "a policy-suppressed seam keeps its canonical gap identity: {item}"
        );
        let message = item["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("policy.toml") && message.contains("src/retry.rs::retry_allowed"),
            "message must name the policy entry: {item}"
        );
    }
    let (gate, exit) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(
        exit,
        Some(3),
        "the unsuppressed control still blocks: {gate}"
    );
    assert_eq!(
        gate_count(&gate, "suppressed")?,
        suppressed_lines.len() as u64
    );
    assert!(
        gate_count(&gate, "blocking")? < control_blocking,
        "suppressed seams must leave the blocking count: {gate}"
    );
    let blocking_lines: Vec<u64> = gate["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|decision| decision["decision"] == "blocking")
        .filter_map(|decision| decision.pointer("/placement/line").and_then(Value::as_u64))
        .collect();
    assert_eq!(
        blocking_lines,
        vec![BACKOFF_LINE],
        "only the unsuppressed control function may still block: {gate}"
    );

    let both = format!(
        "{}{}",
        function_entry("retry_allowed"),
        function_entry("backoff_ms")
    );
    write_policy(root, "policy-both.toml", &both)?;
    run_ok(
        root,
        "review-comments --suppression-policy (both)",
        &review_comments_args(COMMENTS_JSON, &["--suppression-policy", "policy-both.toml"]),
    )?;
    let (gate, exit) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(
        exit,
        Some(0),
        "all findings suppressed must not block: {gate}"
    );
    assert_ne!(gate["status"], "blocked", "{gate}");
    assert_eq!(gate_count(&gate, "blocking")?, 0, "{gate}");
    Ok(())
}

/// Editing the suppressed expression changes its finding id, so an id-keyed
/// entry stops applying while the owner-scoped entry keeps suppressing both
/// in `ripr check` and in review-comments.
#[test]
fn function_selector_survives_an_expression_edit_but_a_finding_id_does_not() -> Result<(), String> {
    let fixture = build_retry_repo("policy-edit")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;
    let check_path = root.join(CHECK_JSON);

    let before = run_check_json(root, &check_path)?;
    let retry_ids = |check: &Value| -> Vec<String> {
        check["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|finding| {
                finding.pointer("/probe/line").and_then(Value::as_u64) == Some(RETRY_LINE)
            })
            .filter_map(|finding| finding["id"].as_str().map(str::to_string))
            .collect()
    };
    let ids_before = retry_ids(&before);
    assert!(!ids_before.is_empty(), "{before}");

    edit_retry_expressions(root)?;
    let after = run_check_json(root, &check_path)?;
    let ids_after = retry_ids(&after);
    assert!(!ids_after.is_empty(), "{after}");
    assert!(
        ids_before.iter().all(|id| !ids_after.contains(id)),
        "the edit must change the finding ids (the premise): {ids_before:?} vs {ids_after:?}"
    );

    // An id-keyed entry written before the edit no longer matches.
    let id_policy = format!(
        "\n[[suppressions]]\nkind = \"exposure_gap\"\nfinding_id = \"{}\"\nreason = \"r\"\nowner = \"o\"\n",
        ids_before[0]
    );
    write_policy(root, "policy-id.toml", &id_policy)?;
    write_policy(root, "policy-fn.toml", &function_entry("retry_allowed"))?;
    for (policy, expect_suppressed) in [("policy-id.toml", false), ("policy-fn.toml", true)] {
        let dest = root.join("target/ripr/pr/check-policy.json");
        let output = run_ripr_to(
            root,
            &[
                "check",
                "--root",
                ".",
                "--base",
                "HEAD~1",
                "--format",
                "json",
                "--suppression-policy",
                policy,
            ],
            &dest,
        )?;
        if !output.status.success() {
            return Err(diagnose("check --suppression-policy", &output));
        }
        let check = read_json(&dest)?;
        let suppressed: Vec<u64> = check["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|finding| finding["suppressed"] == true)
            .filter_map(|finding| finding.pointer("/probe/line").and_then(Value::as_u64))
            .collect();
        if expect_suppressed {
            assert!(
                !suppressed.is_empty() && suppressed.iter().all(|line| *line == RETRY_LINE),
                "{policy}: owner-scoped entry must keep suppressing retry_allowed only: {suppressed:?}"
            );
        } else {
            assert!(
                suppressed.is_empty(),
                "{policy}: stale id must not match: {suppressed:?}"
            );
        }
    }

    // And the same after-edit state in review-comments + the gate.
    run_ok(
        root,
        "review-comments after edit",
        &review_comments_args(COMMENTS_JSON, &["--suppression-policy", "policy-fn.toml"]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    let suppressed_lines = listed_seam_lines(&doc, "suppressed");
    assert!(
        !suppressed_lines.is_empty() && suppressed_lines.iter().all(|line| *line == RETRY_LINE),
        "{doc}"
    );
    let (gate, exit) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(exit, Some(3), "backoff_ms still blocks: {gate}");
    assert_eq!(gate_count(&gate, "blocking")?, 1, "{gate}");
    Ok(())
}

/// `--check-output` honors what the check run suppressed, with or without a
/// policy of its own, and the union applies when both are given.
#[test]
fn check_output_suppression_reaches_review_comments_and_the_gate() -> Result<(), String> {
    let fixture = build_retry_repo("policy-check-output")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;
    write_policy(root, "policy-fn.toml", &function_entry("retry_allowed"))?;
    let check_path = root.join(CHECK_JSON);
    let output = run_ripr_to(
        root,
        &[
            "check",
            "--root",
            ".",
            "--base",
            "HEAD~1",
            "--format",
            "json",
            "--suppression-policy",
            "policy-fn.toml",
        ],
        &check_path,
    )?;
    if !output.status.success() {
        return Err(diagnose("check --suppression-policy", &output));
    }

    run_ok(
        root,
        "review-comments --check-output",
        &review_comments_args(COMMENTS_JSON, &["--check-output", CHECK_JSON]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    let suppressed_lines = listed_seam_lines(&doc, "suppressed");
    assert!(
        !suppressed_lines.is_empty() && suppressed_lines.iter().all(|line| *line == RETRY_LINE),
        "{doc}"
    );
    assert!(
        !listed_seam_lines(&doc, "comments").contains(&RETRY_LINE)
            && !listed_seam_lines(&doc, "summary_only").contains(&RETRY_LINE),
        "{doc}"
    );
    let (gate, exit) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(exit, Some(3), "backoff_ms still blocks: {gate}");
    assert_eq!(gate_count(&gate, "blocking")?, 1, "{gate}");

    // Union: the check run suppressed retry_allowed; the policy adds backoff_ms.
    write_policy(root, "policy-backoff.toml", &function_entry("backoff_ms"))?;
    run_ok(
        root,
        "review-comments --check-output --suppression-policy",
        &review_comments_args(
            COMMENTS_JSON,
            &[
                "--check-output",
                CHECK_JSON,
                "--suppression-policy",
                "policy-backoff.toml",
            ],
        ),
    )?;
    let (gate, exit) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(exit, Some(0), "the union suppresses every finding: {gate}");
    assert_eq!(gate_count(&gate, "blocking")?, 0, "{gate}");
    Ok(())
}

/// The id of the predicate finding on `line` of a `ripr check --json` document.
fn predicate_finding_id(check: &Value, line: u64) -> Result<String, String> {
    check["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|finding| finding.pointer("/probe/line").and_then(Value::as_u64) == Some(line))
        .filter_map(|finding| finding["id"].as_str())
        .find(|id| id.contains(":predicate:"))
        .map(str::to_string)
        .ok_or_else(|| format!("no predicate finding at line {line}: {check}"))
}

fn id_entry(id: &str) -> String {
    format!(
        "\n[[suppressions]]\nkind = \"exposure_gap\"\nfinding_id = \"{id}\"\nreason = \"accepted\"\nowner = \"team-retry\"\n"
    )
}

/// A check run that suppressed ONE finding of a function holding several
/// seams must suppress only that finding's seam in review-comments: the check
/// run's `suppressed_by` is an identity, never widened to its owner function.
#[test]
fn check_output_suppresses_only_the_named_finding_of_a_multi_seam_function() -> Result<(), String> {
    let fixture = build_retry_repo("policy-one-of-two")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;
    let check_path = root.join(CHECK_JSON);
    let control = run_check_json(root, &check_path)?;
    let id = predicate_finding_id(&control, RETRY_LINE)?;
    let retry_findings = control["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|finding| {
            finding.pointer("/probe/line").and_then(Value::as_u64) == Some(RETRY_LINE)
        })
        .count();
    assert!(
        retry_findings >= 2,
        "premise: retry_allowed holds several findings: {control}"
    );

    write_policy(root, "policy-one.toml", &id_entry(&id))?;
    let output = run_ripr_to(
        root,
        &[
            "check",
            "--root",
            ".",
            "--base",
            "HEAD~1",
            "--format",
            "json",
            "--suppression-policy",
            "policy-one.toml",
        ],
        &check_path,
    )?;
    if !output.status.success() {
        return Err(diagnose("check --suppression-policy", &output));
    }
    let check = read_json(&check_path)?;
    let check_suppressed = check["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|finding| finding["suppressed"] == true)
        .count();
    assert_eq!(check_suppressed, 1, "{check}");

    run_ok(
        root,
        "review-comments --check-output",
        &review_comments_args(COMMENTS_JSON, &["--check-output", CHECK_JSON]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    assert_eq!(
        doc["suppressed"].as_array().map(Vec::len),
        Some(1),
        "only the named finding's seam is suppressed: {doc}"
    );
    let still_listed = listed_seam_lines(&doc, "comments")
        .into_iter()
        .chain(listed_seam_lines(&doc, "summary_only"))
        .any(|line| line == RETRY_LINE);
    assert!(
        still_listed,
        "the sibling seam in retry_allowed must stay listed: {doc}"
    );
    let (gate, exit) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(exit, Some(3), "backoff_ms still blocks: {gate}");
    // Exactly one seam is suppressed at the gate; retry_allowed's sibling seam
    // stays evaluated (advisory in this fixture, never suppressed).
    assert_eq!(gate_count(&gate, "suppressed")?, 1, "{gate}");
    assert_eq!(gate_count(&gate, "blocking")?, 1, "{gate}");
    assert_eq!(gate_count(&gate, "advisory")?, 1, "{gate}");
    Ok(())
}

/// A `finding_id` entry applied with `--suppression-policy` reaches the
/// review-comments seam it uniquely names, and nothing else.
#[test]
fn finding_id_entry_applied_through_review_comments_policy_suppresses_one_seam()
-> Result<(), String> {
    let fixture = build_retry_repo("policy-finding-id")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;
    let check = run_check_json(root, &root.join(CHECK_JSON))?;
    let id = predicate_finding_id(&check, RETRY_LINE)?;
    write_policy(root, "policy-id.toml", &id_entry(&id))?;

    run_ok(
        root,
        "review-comments --suppression-policy (finding id)",
        &review_comments_args(COMMENTS_JSON, &["--suppression-policy", "policy-id.toml"]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    let suppressed = doc["suppressed"].as_array().cloned().unwrap_or_default();
    assert_eq!(suppressed.len(), 1, "{doc}");
    assert_eq!(suppressed[0]["reason"], "suppressed_by_policy", "{doc}");
    assert_eq!(suppressed[0]["line"].as_u64(), Some(RETRY_LINE), "{doc}");
    assert!(
        doc["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|warning| warning["message"].as_str().unwrap_or_default())
            .all(|warning| !warning.contains("not applied")),
        "a unique id must not warn: {doc}"
    );
    // The control function and retry_allowed's other seam are untouched.
    assert!(
        listed_seam_lines(&doc, "comments")
            .into_iter()
            .chain(listed_seam_lines(&doc, "summary_only"))
            .any(|line| line == BACKOFF_LINE),
        "{doc}"
    );
    Ok(())
}

/// The default `.ripr/suppressions.toml` stays opt-in: it is disclosed, never
/// applied; an unreadable explicit policy and `--gap-ledger` are refused.
#[test]
fn review_comments_never_applies_the_default_policy_implicitly_and_fails_closed()
-> Result<(), String> {
    let fixture = build_retry_repo("policy-optin")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;
    write_policy(
        root,
        ".ripr/suppressions.toml",
        &function_entry("retry_allowed"),
    )?;

    let output = run_ok(
        root,
        "review-comments without the flag",
        &review_comments_args(COMMENTS_JSON, &[]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    assert!(listed_seam_lines(&doc, "suppressed").is_empty(), "{doc}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("detected .ripr/suppressions.toml") && stderr.contains("not applied"),
        "the unused default must be disclosed: {stderr}"
    );

    run_ok(
        root,
        "review-comments with the flag",
        &review_comments_args(
            COMMENTS_JSON,
            &["--suppression-policy", ".ripr/suppressions.toml"],
        ),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    assert!(!listed_seam_lines(&doc, "suppressed").is_empty(), "{doc}");

    for (extra, needle) in [
        (
            &["--suppression-policy", "missing.toml"][..],
            "failed to read suppression policy",
        ),
        (
            &[
                "--suppression-policy",
                "p.toml",
                "--gap-ledger",
                "ledger.json",
            ][..],
            "not --gap-ledger",
        ),
    ] {
        let refused = run_ripr(
            root,
            &review_comments_args("target/ripr/review/refused.json", extra),
        )?;
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert_eq!(refused.status.code(), Some(2), "{stderr}");
        assert!(stderr.contains(needle), "{stderr}");
    }
    Ok(())
}

/// An expired entry is reported and never applied, so the gate keeps blocking
/// on the seam it named; a plain `path` glob suppresses every seam of the
/// files it matches and no others.
#[test]
fn review_comments_skips_expired_entries_and_applies_path_globs() -> Result<(), String> {
    let fixture = build_retry_repo("policy-expired-glob")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;

    let expired = function_entry("retry_allowed") + "expires = \"2000-01-01\"\n";
    write_policy(root, "expired.toml", &expired)?;
    run_ok(
        root,
        "review-comments with an expired entry",
        &review_comments_args(COMMENTS_JSON, &["--suppression-policy", "expired.toml"]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    assert!(
        listed_seam_lines(&doc, "suppressed").is_empty(),
        "an expired entry must not suppress anything: {doc}"
    );
    assert!(
        listed_seam_lines(&doc, "comments")
            .into_iter()
            .chain(listed_seam_lines(&doc, "summary_only"))
            .any(|line| line == RETRY_LINE),
        "the seam named by the expired entry stays listed: {doc}"
    );
    let warnings = doc["warnings"].to_string();
    assert!(
        warnings.contains("expired"),
        "expiry must be reported: {doc}"
    );
    let (gate, exit) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(exit, Some(3), "{gate}");
    assert_eq!(gate_count(&gate, "suppressed")?, 0, "{gate}");

    let glob_entry = |pattern: &str| {
        format!(
            "\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"{pattern}\"\nreason = \"generated retry table\"\nowner = \"team-retry\"\n"
        )
    };
    write_policy(root, "miss.toml", &glob_entry("src/other*.rs"))?;
    run_ok(
        root,
        "review-comments with a non-matching glob",
        &review_comments_args(COMMENTS_JSON, &["--suppression-policy", "miss.toml"]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    assert!(
        listed_seam_lines(&doc, "suppressed").is_empty(),
        "a glob naming another file suppresses nothing: {doc}"
    );

    write_policy(root, "glob.toml", &glob_entry("src/retry*.rs"))?;
    run_ok(
        root,
        "review-comments with a matching glob",
        &review_comments_args(COMMENTS_JSON, &["--suppression-policy", "glob.toml"]),
    )?;
    let doc = read_json(&root.join(COMMENTS_JSON))?;
    let suppressed = listed_seam_lines(&doc, "suppressed");
    assert!(
        suppressed.contains(&RETRY_LINE) && suppressed.contains(&BACKOFF_LINE),
        "a glob over src/retry.rs suppresses both functions, got {suppressed:?}: {doc}"
    );
    for key in ["comments", "summary_only"] {
        assert!(listed_seam_lines(&doc, key).is_empty(), "{key}: {doc}");
    }
    let (gate, _) = evaluate_gate(root, COMMENTS_JSON)?;
    assert_eq!(gate_count(&gate, "blocking")?, 0, "{gate}");
    assert_eq!(
        gate_count(&gate, "suppressed")?,
        suppressed.len() as u64,
        "{gate}"
    );
    Ok(())
}

/// SARIF results honor a configured `path` + `function` entry the way check
/// JSON does: the named owner's results carry a `suppressions` array and the
/// sibling function's results do not.
#[test]
fn sarif_marks_function_selector_results_suppressed_and_spares_the_sibling() -> Result<(), String> {
    let fixture = build_retry_repo("policy-sarif")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;
    write_policy(
        root,
        ".ripr/suppressions.toml",
        &function_entry("retry_allowed"),
    )?;
    for format in ["sarif", "sarif-compact"] {
        let dest = root.join(format!("target/ripr/pr/check.{format}.json"));
        let output = run_ripr_to(
            root,
            &[
                "check", "--root", ".", "--base", "HEAD~1", "--format", format,
            ],
            &dest,
        )?;
        if !output.status.success() {
            return Err(diagnose(format, &output));
        }
        let sarif = read_json(&dest)?;
        let results = sarif["runs"][0]["results"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let at_line = |line: u64| {
            results
                .iter()
                .filter(|result| {
                    result["locations"][0]["physicalLocation"]["region"]["startLine"].as_u64()
                        == Some(line)
                })
                .collect::<Vec<_>>()
        };
        let retry = at_line(RETRY_LINE);
        let backoff = at_line(BACKOFF_LINE);
        assert!(
            !retry.is_empty() && !backoff.is_empty(),
            "{format}: both functions need results: {sarif}"
        );
        assert!(
            retry
                .iter()
                .all(|result| result["suppressions"][0]["justification"] == "accepted retry debt"),
            "{format}: retry_allowed results must be suppressed: {sarif}"
        );
        assert!(
            backoff
                .iter()
                .all(|result| result.get("suppressions").is_none()),
            "{format}: the sibling function stays unsuppressed: {sarif}"
        );
    }
    Ok(())
}

// ---- #14129: owner selector negatives, the stale-id warning, suppression-health ----

/// `src/retry.rs` declares `retry_allowed` at the top level (line 6) and again
/// inside `mod inner` (line 11), next to a `const` and a `static` no function
/// owns (lines 1 and 3). `src/other.rs` declares a third `retry_allowed`.
fn same_name_retry_source(limit: u32) -> String {
    format!(
        "pub const LIMIT: u32 = if cfg!(test) {{ {limit} }} else {{ 9 }};\n\npub static FLOOR: bool = LIMIT > {limit};\n\npub fn retry_allowed(attempts: u32) -> bool {{\n    attempts < {limit}\n}}\n\npub mod inner {{\n    pub fn retry_allowed(attempts: u32) -> bool {{\n        attempts <= {limit}\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn t() {{\n        assert!(retry_allowed(0));\n        assert!(inner::retry_allowed(0));\n    }}\n}}\n"
    )
}

fn same_name_other_source(limit: u32) -> String {
    format!(
        "pub fn retry_allowed(attempts: u32) -> bool {{\n    attempts > {limit}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn t() {{\n        assert!(!retry_allowed(0));\n    }}\n}}\n"
    )
}

const OWNERLESS_LINES: [u64; 2] = [1, 3];
const TOP_LEVEL_RETRY_LINE: u64 = 6;
const INNER_RETRY_LINE: u64 = 11;
const OTHER_FILE_RETRY_LINE: u64 = 2;

fn build_same_name_repo(label: &str) -> Result<Fixture, String> {
    let root = unique_root(label)?;
    std::fs::create_dir_all(root.join("src"))
        .map_err(|error| format!("create {}: {error}", root.display()))?;
    let fixture = Fixture(root);
    let root = &fixture.0;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"same_name_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write Cargo.toml: {error}"))?;
    std::fs::write(root.join("src/lib.rs"), "pub mod retry;\npub mod other;\n")
        .map_err(|error| format!("write lib.rs: {error}"))?;
    fixture_git_ok(root, &["init"]).map_err(|error| format!("fixture git init: {error}"))?;
    fixture_git_ok(root, &["config", "core.autocrlf", "false"])
        .map_err(|error| format!("fixture git config: {error}"))?;
    for (limit, message) in [(3, "base"), (4, "edit")] {
        write_retry_source(root, &same_name_retry_source(limit))?;
        std::fs::write(root.join("src/other.rs"), same_name_other_source(limit))
            .map_err(|error| format!("write other.rs: {error}"))?;
        fixture_git_ok(root, &["add", "-A"])
            .map_err(|error| format!("fixture git add: {error}"))?;
        commit_fixture(root, message)?;
    }
    Ok(fixture)
}

/// `(file suffix, line)` of every finding, split by its `suppressed` flag,
/// plus the policy warnings, for one `check --suppression-policy` run.
struct PolicyRun {
    suppressed: Vec<(String, u64)>,
    reported: Vec<(String, u64)>,
    warnings: Vec<String>,
}

fn run_check_with_policy(root: &Path, policy: &str) -> Result<PolicyRun, String> {
    let dest = root.join("target/ripr/pr/check-policy.json");
    let output = run_ripr_to(
        root,
        &[
            "check",
            "--root",
            ".",
            "--base",
            "HEAD~1",
            "--format",
            "json",
            "--suppression-policy",
            policy,
        ],
        &dest,
    )?;
    if !output.status.success() {
        return Err(diagnose("check --suppression-policy", &output));
    }
    let check = read_json(&dest)?;
    let mut run = PolicyRun {
        suppressed: Vec::new(),
        reported: Vec::new(),
        warnings: check
            .pointer("/suppression_policy/warnings")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("check JSON lost suppression_policy.warnings: {check}"))?
            .iter()
            .filter_map(|warning| warning.as_str().map(str::to_string))
            .collect(),
    };
    let findings = check["findings"]
        .as_array()
        .ok_or_else(|| format!("check JSON lost findings: {check}"))?;
    for finding in findings {
        let file = finding
            .pointer("/probe/file")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("finding without probe.file: {finding}"))?
            .replace('\\', "/");
        let file = file.trim_start_matches("./").to_string();
        let line = finding
            .pointer("/probe/line")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("finding without probe.line: {finding}"))?;
        if finding["suppressed"] == true {
            run.suppressed.push((file, line));
        } else {
            run.reported.push((file, line));
        }
    }
    Ok(run)
}

fn has_finding(findings: &[(String, u64)], file: &str, line: u64) -> bool {
    findings
        .iter()
        .any(|(found_file, found_line)| found_file == file && *found_line == line)
}

/// An owner selector names one function in one file: the same leaf name in
/// another file or in a nested module, and a changed item no function owns,
/// all stay reported.
#[test]
fn function_selector_spares_same_named_owners_and_ownerless_findings() -> Result<(), String> {
    let fixture = build_same_name_repo("policy-same-name")?;
    let root = &fixture.0;

    write_policy(root, "policy-fn.toml", &function_entry("retry_allowed"))?;
    let run = run_check_with_policy(root, "policy-fn.toml")?;
    assert!(
        !run.suppressed.is_empty()
            && run
                .suppressed
                .iter()
                .all(|(file, line)| file == "src/retry.rs" && *line == TOP_LEVEL_RETRY_LINE),
        "only the top-level src/retry.rs retry_allowed is suppressed: {:?}",
        run.suppressed
    );
    assert!(
        !has_finding(&run.reported, "src/retry.rs", TOP_LEVEL_RETRY_LINE),
        "every finding of the named owner is suppressed: {:?}",
        run.reported
    );
    // The negatives must be real findings of this run, not absent ones.
    assert!(
        has_finding(&run.reported, "src/other.rs", OTHER_FILE_RETRY_LINE),
        "retry_allowed in another file stays reported: {:?}",
        run.reported
    );
    assert!(
        has_finding(&run.reported, "src/retry.rs", INNER_RETRY_LINE),
        "inner::retry_allowed in the same file stays reported: {:?}",
        run.reported
    );
    for line in OWNERLESS_LINES {
        assert!(
            has_finding(&run.reported, "src/retry.rs", line),
            "the ownerless item on line {line} stays reported: {:?}",
            run.reported
        );
    }
    assert!(run.warnings.is_empty(), "{:?}", run.warnings);

    // Control: the nested function is selectable by its own owner path, so the
    // spared finding above is a different identity, not an unselectable one.
    write_policy(
        root,
        "policy-inner.toml",
        &function_entry("inner::retry_allowed"),
    )?;
    let inner = run_check_with_policy(root, "policy-inner.toml")?;
    assert!(
        !inner.suppressed.is_empty()
            && inner
                .suppressed
                .iter()
                .all(|(file, line)| file == "src/retry.rs" && *line == INNER_RETRY_LINE),
        "{:?}",
        inner.suppressed
    );

    // Naming the ownerless items themselves matches nothing and says so.
    for item in ["LIMIT", "FLOOR"] {
        write_policy(root, "policy-item.toml", &function_entry(item))?;
        let run = run_check_with_policy(root, "policy-item.toml")?;
        assert!(
            run.suppressed.is_empty(),
            "{item}: an owner selector never matches a finding without a function owner: {:?}",
            run.suppressed
        );
        assert_eq!(
            run.warnings,
            vec![format!(
                "exposure_gap suppression for `src/retry.rs::{item}` did not match any current finding"
            )],
        );
    }
    Ok(())
}

/// An id-keyed entry that no longer matches says which selector survives the
/// edit; an unmatched owner selector does not get that advice.
#[test]
fn stale_finding_id_warning_names_the_owner_selector() -> Result<(), String> {
    let fixture = build_retry_repo("policy-stale-warning")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;
    let before = run_check_json(root, &root.join(CHECK_JSON))?;
    let stale_id = predicate_finding_id(&before, RETRY_LINE)?;
    edit_retry_expressions(root)?;

    write_policy(root, "policy-id.toml", &id_entry(&stale_id))?;
    let run = run_check_with_policy(root, "policy-id.toml")?;
    assert!(run.suppressed.is_empty(), "{:?}", run.suppressed);
    assert_eq!(run.warnings.len(), 1, "{:?}", run.warnings);
    let warning = &run.warnings[0];
    assert!(
        warning.contains(&format!(
            "exposure_gap suppression for `{stale_id}` did not match any current finding"
        )) && warning.contains("select it with `path` + `function` instead of `finding_id`"),
        "the stale-id warning must name the owner selector: {warning}"
    );

    write_policy(root, "policy-gone.toml", &function_entry("removed_fn"))?;
    let run = run_check_with_policy(root, "policy-gone.toml")?;
    assert_eq!(
        run.warnings,
        vec![
            "exposure_gap suppression for `src/retry.rs::removed_fn` did not match any current finding"
                .to_string()
        ],
    );
    Ok(())
}

fn run_suppression_health(root: &Path, manifest: &str) -> Result<Value, String> {
    let out = "target/ripr/reports/suppression-health.json";
    run_ok(
        root,
        "policy suppression-health",
        &[
            "policy",
            "suppression-health",
            "--root",
            ".",
            "--manifest",
            manifest,
            "--out",
            out,
            "--out-md",
            "target/ripr/reports/suppression-health.md",
        ],
    )?;
    read_json(&root.join(out))
}

/// `policy suppression-health` reads a `path` + `function` entry as a valid
/// selector named by its owner label, and reports a malformed one.
#[test]
fn suppression_health_reads_a_function_selector_and_reports_a_malformed_one() -> Result<(), String>
{
    let fixture = build_retry_repo("policy-health")?;
    let root = &fixture.0;
    mkdir_chain_dirs(root)?;
    let metadata = "scope = \"function:retry_allowed\"\ncreated_at = \"2026-01-01\"\nlast_seen = \"2026-05-01\"\nreview_by = \"2999-12-01\"\nexpected_visibility = \"suppressed_visible\"\nstatic_class = \"weakly_exposed\"\nlanguage = \"rust\"\n";

    write_policy(
        root,
        "health-fn.toml",
        &format!("{}{metadata}", function_entry("retry_allowed")),
    )?;
    let report = run_suppression_health(root, "health-fn.toml")?;
    assert_eq!(report["status"], "healthy", "{report}");
    assert_eq!(report["summary"]["suppressions"], 1, "{report}");
    assert_eq!(report["summary"]["config_errors"], 0, "{report}");
    assert_eq!(report["summary"]["unknown_selector"], 0, "{report}");
    assert_eq!(
        report["records"][0]["identity"], "src/retry.rs::retry_allowed",
        "{report}"
    );
    assert_eq!(report["records"][0]["health"], "healthy", "{report}");

    for (label, entry, needle) in [
        (
            "glob path",
            function_entry("retry_allowed").replace("src/retry.rs", "src/*.rs"),
            "`function` selects by exact file",
        ),
        (
            "ambiguous owner",
            function_entry("retry_allowed#L12"),
            "names a line-disambiguated duplicate owner",
        ),
        (
            "function without path",
            function_entry("retry_allowed").replace("path = \"src/retry.rs\"\n", ""),
            "requires `finding_id`, a `path` glob, or `path` + `function`",
        ),
    ] {
        write_policy(root, "health-bad.toml", &format!("{entry}{metadata}"))?;
        let report = run_suppression_health(root, "health-bad.toml")?;
        assert_eq!(report["status"], "config_error", "{label}: {report}");
        assert_eq!(report["summary"]["suppressions"], 0, "{label}: {report}");
        assert_eq!(report["summary"]["config_errors"], 1, "{label}: {report}");
        let message = report["findings"][0]["message"]
            .as_str()
            .unwrap_or_default();
        assert!(message.contains(needle), "{label}: {report}");
    }
    Ok(())
}
