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
