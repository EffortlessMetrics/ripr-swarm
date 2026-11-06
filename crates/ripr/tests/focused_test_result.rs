//! #13203 slice B: a caller-supplied focused `cargo test` result is validated
//! and recorded by the after phase. Real binary over a library whose only seam
//! has no related test (one proposed integration file). ripr never runs the
//! tests: the result is a libtest JSON claim, accepted only when it is bound
//! to this attempt and the evaluated HEAD, names the selected file, and
//! contains at least one test that ran in that file.
//!
//! Oracles: the exit status, the attempt manifest (`state`, `after`,
//! `terminal_artifacts`) and the retained record the commands wrote.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok, fixture_git_output};

static COUNTER: AtomicU64 = AtomicU64::new(0);

const PROPOSED_FILE: &str = "tests/discounted_total.rs";

const NEW_TEST: &str = r#"use proposal_fixture::discounted_total;

#[test]
fn equality_boundary_discounts() {
    assert_eq!(discounted_total(100, 100), 90);
}
"#;

struct Fixture {
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run_ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn combined(output: &Output) -> String {
    format!(
        "exit {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn write(root: &Path, relative: &str, text: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

fn commit_all(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "-m",
            message,
        ],
    )
}

/// A library with one public seam without a test and an unrelated integration test,
/// which establishes the `tests/` layout the integration proposal requires.
fn fixture(label: &str) -> Result<Fixture, String> {
    fixture_with(label, PRICE_FN, true)
}

const PRICE_FN: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";

fn fixture_with(label: &str, lib_source: &str, with_tests_layout: bool) -> Result<Fixture, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-{label}-{stamp}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).map_err(|error| format!("create {}: {error}", root.display()))?;
    let owned = Fixture { root: root.clone() };
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"proposal_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"proposal_fixture\"\npath = \"src/lib.rs\"\n",
    )?;
    write(&root, "src/lib.rs", lib_source)?;
    if with_tests_layout {
        write(&root, "tests/smoke.rs", "#[test]\nfn smoke() {}\n")?;
    }
    write(&root, ".gitignore", "/target/\n")?;
    fixture_git_ok(&root, &["init"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    commit_all(&root, "fixture")?;
    Ok(owned)
}

fn stdout_json(output: &Output) -> Result<Value, String> {
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("stdout is not JSON: {error}\n{}", combined(output)))
}

fn seam_id(root: &Path, root_arg: &str) -> Result<String, String> {
    let output = run_ripr(
        root,
        &[
            "check",
            "--root",
            root_arg,
            "--mode",
            "draft",
            "--format",
            "repo-exposure-json",
        ],
    )?;
    if !output.status.success() {
        return Err(format!("repo-exposure failed: {}", combined(&output)));
    }
    stdout_json(&output)?
        .pointer("/seams/0/seam_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "fixture produced no seam".to_string())
}

struct Attempt {
    root_arg: String,
    id: String,
}

fn begin(root: &Path) -> Result<Attempt, String> {
    let root_arg = root.display().to_string();
    let seam = seam_id(root, &root_arg)?;
    let output = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--json",
            "--root",
            &root_arg,
            "--seam-id",
            &seam,
            "--phase",
            "before",
        ],
    )?;
    if !output.status.success() {
        return Err(format!("before phase failed: {}", combined(&output)));
    }
    let id = stdout_json(&output)?
        .pointer("/repair_attempt/attempt_id")
        .and_then(Value::as_str)
        .ok_or("before phase named no attempt")?
        .to_owned();
    Ok(Attempt { root_arg, id })
}

fn manifest(root: &Path, attempt: &Attempt) -> Result<Value, String> {
    let path = root
        .join("target/ripr/repair-attempts")
        .join(&attempt.id)
        .join("attempt.json");
    let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse manifest: {error}"))
}

const RESULT_ROLE: &str = "focused_test_result";

fn head_sha(root: &Path) -> Result<String, String> {
    Ok(fixture_git_output(root, &["rev-parse", "HEAD"])?
        .trim()
        .to_string())
}

fn ok_event(name: &str) -> Value {
    serde_json::json!({"type": "test", "event": "ok", "name": name})
}

fn result_document(attempt: &str, head: &str, file: &str, events: Value) -> Value {
    serde_json::json!({
        "schema_version": "0.1",
        "kind": "focused_test_result",
        "attempt_id": attempt,
        "head": head,
        "test_file": file,
        "libtest_events": events,
    })
}

/// A begun attempt whose proposed file has been written but not yet finished.
fn edited_attempt(label: &str) -> Result<(Fixture, Attempt), String> {
    let fixture = fixture(label)?;
    let attempt = begin(&fixture.root)?;
    write(&fixture.root, PROPOSED_FILE, NEW_TEST)?;
    Ok((fixture, attempt))
}

fn after_with(root: &Path, attempt: &Attempt, result: &Path) -> Result<Output, String> {
    run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--json",
            "--root",
            &attempt.root_arg,
            "--attempt",
            &attempt.id,
            "--phase",
            "after",
            "--test-result",
            &result.display().to_string(),
        ],
    )
}

fn terminal_roles(manifest: &Value) -> Vec<String> {
    manifest
        .pointer("/terminal_artifacts")
        .and_then(Value::as_array)
        .map(|artifacts| {
            artifacts
                .iter()
                .filter_map(|artifact| artifact.get("role").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// A refused result leaves the attempt exactly as the edit left it: still
/// awaiting the after phase, no verdict, no recorded focused run.
fn assert_refused_and_unrecorded(
    root: &Path,
    attempt: &Attempt,
    output: &Output,
    reason: &str,
) -> Result<(), String> {
    let text = combined(output);
    if output.status.code() != Some(3) || !text.contains(reason) {
        return Err(format!("expected a refusal naming `{reason}`:\n{text}"));
    }
    let manifest = manifest(root, attempt)?;
    if manifest.pointer("/state").and_then(Value::as_str) != Some("awaiting_edit")
        || !manifest.pointer("/after").is_none_or(Value::is_null)
        || terminal_roles(&manifest)
            .iter()
            .any(|role| role == RESULT_ROLE)
    {
        return Err(format!("a refused result changed the attempt:\n{manifest}"));
    }
    Ok(())
}

fn write_result(root: &Path, name: &str, document: &Value) -> Result<PathBuf, String> {
    // Outside the repository so the result is not an edit.
    let path = root.with_file_name(format!(
        "{}-{name}.json",
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("fixture")
    ));
    fs::write(&path, document.to_string())
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    Ok(path)
}

#[test]
fn a_bound_result_with_a_ran_test_is_validated_and_recorded() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-bound")?;
    let root = &fixture.root;
    let document = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([
            {"type": "suite", "event": "started", "test_count": 1},
            {"type": "test", "event": "started", "name": "equality_boundary_discounts"},
            ok_event("equality_boundary_discounts"),
            ok_event("not_in_the_changed_file"),
        ]),
    );
    let result = write_result(root, "bound", &document)?;
    let output = after_with(root, &attempt, &result)?;
    let finished = manifest(root, &attempt)?;
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(
        finished
            .pointer("/after/verdict/status")
            .and_then(Value::as_str),
        Some("compliant"),
        "{finished}"
    );
    assert!(
        terminal_roles(&finished)
            .iter()
            .any(|role| role == RESULT_ROLE),
        "{finished}"
    );
    let record = retained_terminal(root, &finished, RESULT_ROLE)?;
    assert_eq!(record["attempt_id"], Value::String(attempt.id.clone()));
    assert_eq!(record["head"], Value::String(head_sha(root)?));
    assert_eq!(record["file"], Value::String(PROPOSED_FILE.to_string()));
    assert_eq!(
        record["ran_tests"],
        serde_json::json!(["equality_boundary_discounts"])
    );
    assert_eq!(record["passed"], serde_json::json!(1));
    assert_eq!(record["uncounted_ran"], serde_json::json!(1));
    Ok(())
}

fn retained_terminal(root: &Path, manifest: &Value, role: &str) -> Result<Value, String> {
    let relative = manifest
        .pointer("/terminal_artifacts")
        .and_then(Value::as_array)
        .and_then(|artifacts| {
            artifacts
                .iter()
                .find(|artifact| artifact.get("role").and_then(Value::as_str) == Some(role))
        })
        .and_then(|artifact| artifact.get("path").and_then(Value::as_str))
        .ok_or_else(|| format!("manifest has no terminal {role} artifact"))?;
    let bytes = fs::read(root.join(relative)).map_err(|error| format!("read {role}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {role}: {error}"))
}

#[test]
fn a_result_where_no_test_ran_in_the_changed_file_is_not_counted() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-zero")?;
    let root = &fixture.root;
    let document = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([
            {"type": "test", "event": "started", "name": "equality_boundary_discounts"},
            {"type": "test", "event": "ignored", "name": "equality_boundary_discounts"},
            ok_event("smoke"),
        ]),
    );
    let result = write_result(root, "zero", &document)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "no test ran in")?;
    // The refusal is recoverable: the same attempt accepts a real run.
    let good = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "zero-good", &good)?;
    let output = after_with(root, &attempt, &result)?;
    assert!(output.status.success(), "{}", combined(&output));
    Ok(())
}

#[test]
fn a_result_bound_to_another_attempt_or_head_is_refused() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-binding")?;
    let root = &fixture.root;
    let events = serde_json::json!([ok_event("equality_boundary_discounts")]);
    let head = head_sha(root)?;
    let other_attempt = result_document(
        "repair-attempt-000000000000000000000000",
        &head,
        PROPOSED_FILE,
        events.clone(),
    );
    let result = write_result(root, "other-attempt", &other_attempt)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "bound to attempt")?;

    let other_head = result_document(
        &attempt.id,
        "0000000000000000000000000000000000000000",
        PROPOSED_FILE,
        events,
    );
    let result = write_result(root, "other-head", &other_head)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "bound to HEAD")
}

#[test]
fn a_result_for_another_file_or_a_malformed_result_is_refused() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-malformed")?;
    let root = &fixture.root;
    let head = head_sha(root)?;
    let events = serde_json::json!([ok_event("equality_boundary_discounts")]);

    let other_file = result_document(&attempt.id, &head, "tests/smoke.rs", events.clone());
    let result = write_result(root, "other-file", &other_file)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "selected test file")?;

    let not_json = root.with_file_name("focused-not-json.json");
    fs::write(&not_json, "this is not json").map_err(|error| error.to_string())?;
    let output = after_with(root, &attempt, &not_json)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "not a valid")?;

    let bad_event = result_document(&attempt.id, &head, PROPOSED_FILE, serde_json::json!([42]));
    let result = write_result(root, "bad-event", &bad_event)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "not a JSON object")?;

    let missing = root.with_file_name("focused-missing.json");
    let output = after_with(root, &attempt, &missing)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "read the focused test result")
}

#[test]
fn test_result_is_only_valid_with_the_after_phase() -> Result<(), String> {
    let fixture = fixture("focused-phase")?;
    let output = run_ripr(
        &fixture.root,
        &[
            "agent",
            "repair",
            "--root",
            &fixture.root.display().to_string(),
            "--seam-id",
            "x",
            "--phase",
            "before",
            "--test-result",
            "result.json",
        ],
    )?;
    let text = combined(&output);
    assert!(
        !output.status.success() && text.contains("only valid with --phase after"),
        "{text}"
    );
    Ok(())
}

/// An inline proposal: a test counts only when its path runs through the
/// governed module.
#[test]
fn an_inline_result_counts_only_tests_in_the_governed_module() -> Result<(), String> {
    let source = format!(
        "{}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n}}\n",
        PRICE_FN.trim_start_matches("pub ")
    );
    let fixture = fixture_with("focused-inline", &source, false)?;
    let root = &fixture.root;
    let attempt = begin(root)?;
    let edited = format!(
        "{}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn equality_boundary_discounts() {{\n        assert_eq!(discounted_total(100, 100), 90);\n    }}\n}}\n",
        PRICE_FN.trim_start_matches("pub ")
    );
    write(root, "src/lib.rs", &edited)?;
    let head = head_sha(root)?;

    let outside = result_document(
        &attempt.id,
        &head,
        "src/lib.rs",
        serde_json::json!([
            ok_event("other::equality_boundary_discounts"),
            ok_event("bar::tests::equality_boundary_discounts"),
            ok_event("smoke")
        ]),
    );
    let result = write_result(root, "inline-outside", &outside)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "no test ran in the `tests` module")?;

    let inside = result_document(
        &attempt.id,
        &head,
        "src/lib.rs",
        serde_json::json!([ok_event("tests::equality_boundary_discounts")]),
    );
    let result = write_result(root, "inline-inside", &inside)?;
    let output = after_with(root, &attempt, &result)?;
    assert!(output.status.success(), "{}", combined(&output));
    let record = retained_terminal(root, &manifest(root, &attempt)?, RESULT_ROLE)?;
    assert_eq!(record["module"], serde_json::json!("tests"));
    assert_eq!(
        record["ran_tests"],
        serde_json::json!(["tests::equality_boundary_discounts"])
    );
    Ok(())
}

/// A failed test ran, so it counts as ran, but the full-proof finish gate
/// (slice C) refuses it: the new test must pass. The refusal leaves the attempt
/// retryable, and the same attempt then finishes on a passing run.
#[test]
fn a_failed_result_is_refused_by_the_finish_gate_and_recoverable() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-failed")?;
    let root = &fixture.root;
    let head = head_sha(root)?;
    let failing = result_document(
        &attempt.id,
        &head,
        PROPOSED_FILE,
        serde_json::json!([
            {"type": "test", "event": "failed", "name": "equality_boundary_discounts"}
        ]),
    );
    let result = write_result(root, "failed-only", &failing)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "1 failed test(s)")?;
    let passing = result_document(
        &attempt.id,
        &head,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "failed-then-passing", &passing)?;
    let output = after_with(root, &attempt, &result)?;
    assert!(output.status.success(), "{}", combined(&output));
    let record = retained_terminal(root, &manifest(root, &attempt)?, RESULT_ROLE)?;
    assert_eq!(record["failed"], serde_json::json!(0));
    Ok(())
}

/// Finishing an attempt that follows a new-test proposal needs a recorded
/// focused result; without one the attempt is refused and stays retryable.
#[test]
fn a_proposal_attempt_cannot_finish_without_a_focused_result() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-required")?;
    let root = &fixture.root;
    let output = run_ripr(
        root,
        &[
            "agent",
            "repair",
            "--json",
            "--root",
            &attempt.root_arg,
            "--attempt",
            &attempt.id,
            "--phase",
            "after",
        ],
    )?;
    assert_refused_and_unrecorded(root, &attempt, &output, "needs a focused test result")?;
    Ok(())
}

/// A result saved inside the checkout would be an untracked edit the cage
/// judges (exit 2 and a violated attempt before this was refused up front), so
/// it is refused with a next step and the attempt stays retryable.
#[test]
fn a_result_saved_inside_the_checkout_is_refused_up_front() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-in-checkout")?;
    let root = &fixture.root;
    let document = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let inside = root.join("focused-result.json");
    fs::write(&inside, document.to_string())
        .map_err(|error| format!("write {}: {error}", inside.display()))?;
    let output = after_with(root, &attempt, &inside)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "is inside the checkout")?;
    // The same result saved outside is accepted on the same attempt.
    fs::remove_file(&inside).map_err(|error| format!("remove: {error}"))?;
    let outside = write_result(root, "outside", &document)?;
    let output = after_with(root, &attempt, &outside)?;
    assert!(output.status.success(), "{}", combined(&output));
    Ok(())
}
