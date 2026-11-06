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

#[cfg(unix)]
fn after_with_interposed_transition(
    root: &Path,
    attempt: &Attempt,
    result: &Path,
    mode: &str,
) -> Result<Output, String> {
    use std::os::unix::fs::PermissionsExt;
    let bin = root.join("target/git-shim");
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let shim = bin.join("git");
    fs::write(&shim, r##"#!/bin/sh
case "$*" in
  *"log -g --format="*)
    if [ "$RIPR_TEST_MODE" = file ] && [ ! -e "$RIPR_TEST_MARKER" ]; then
      cp "$RIPR_TEST_REPLACEMENT" "$RIPR_TEST_TARGET" || exit 90
      : > "$RIPR_TEST_MARKER"
    fi ;;
  *"cat-file -t "*)
    if [ "$RIPR_TEST_MODE" = head ]; then
      count=0; [ ! -f "$RIPR_TEST_COUNTER" ] || count=$(cat "$RIPR_TEST_COUNTER")
      count=$((count + 1)); printf '%s' "$count" > "$RIPR_TEST_COUNTER"
      if [ "$count" = 2 ]; then
        "$RIPR_TEST_REAL_GIT" add -- tests/discounted_total.rs || exit 91
        "$RIPR_TEST_REAL_GIT" -c user.name=Test -c user.email=test@example.invalid -c core.hooksPath=/dev/null commit -m concurrent-test >/dev/null || exit 92
        : > "$RIPR_TEST_MARKER"
      fi
    fi ;;
esac
exec "$RIPR_TEST_REAL_GIT" "$@"
"##).map_err(|e| e.to_string())?;
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    let marker = root.join("target/race-marker");
    let replacement = root.join("target/replacement.rs");
    fs::write(
        &replacement,
        NEW_TEST.replace("equality_boundary_discounts", "different_test"),
    )
    .map_err(|e| e.to_string())?;
    let original_path = std::env::var_os("PATH").ok_or("PATH is missing")?;
    let real_git = std::env::split_paths(&original_path)
        .map(|directory| directory.join("git"))
        .find(|path| {
            fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .ok_or("git is missing from PATH")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&original_path)))
            .map_err(|e| e.to_string())?;
    let output = ripr_command(root)
        .env("PATH", path)
        .env("RIPR_TEST_REAL_GIT", real_git)
        .env("RIPR_TEST_MODE", mode)
        .env("RIPR_TEST_MARKER", &marker)
        .env("RIPR_TEST_COUNTER", root.join("target/race-count"))
        .env("RIPR_TEST_REPLACEMENT", replacement)
        .env("RIPR_TEST_TARGET", root.join(PROPOSED_FILE))
        .args([
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
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !marker.exists() {
        return Err(format!(
            "race transition did not execute: {}",
            combined(&output)
        ));
    }
    Ok(output)
}

// The tracked-surface inventory follows receipt delta recomputation. Change a
// tracked production file exactly as that inventory starts, so the decision
// refusal is real rather than synthesized from an error string.
#[cfg(unix)]
fn after_with_receipt_surface_transition(
    root: &Path,
    attempt: &Attempt,
    result: Option<&Path>,
    stop: bool,
) -> Result<Output, String> {
    use std::os::unix::fs::PermissionsExt;
    let bin = root.join(".git/receipt-surface-shim");
    fs::create_dir_all(&bin).map_err(|error| error.to_string())?;
    let shim = bin.join("git");
    fs::write(
        &shim,
        r##"#!/bin/sh
case "$*" in
  *"diff --no-renames --no-ext-diff --name-only -z "*)
    if [ ! -e "$RIPR_TEST_MARKER" ]; then
      printf '\n// receipt surface transition\n' >> "$RIPR_TEST_TARGET" || exit 90
      : > "$RIPR_TEST_MARKER"
    fi ;;
esac
exec "$RIPR_TEST_REAL_GIT" "$@"
"##,
    )
    .map_err(|error| error.to_string())?;
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())?;
    let original_path = std::env::var_os("PATH").ok_or("PATH is missing")?;
    let real_git = std::env::split_paths(&original_path)
        .map(|directory| directory.join("git"))
        .find(|path| {
            fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .ok_or("git is missing from PATH")?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let path = std::env::join_paths(
        std::iter::once(bin.clone()).chain(std::env::split_paths(&original_path)),
    )
    .map_err(|error| error.to_string())?;
    let marker = bin.join("marker");
    let mut command = ripr_command(root);
    command
        .env("PATH", path)
        .env("RIPR_TEST_REAL_GIT", real_git)
        .env("RIPR_TEST_MARKER", &marker)
        .env("RIPR_TEST_TARGET", root.join("src/lib.rs"))
        .args([
            "agent",
            "repair",
            "--json",
            "--root",
            &attempt.root_arg,
            "--attempt",
            &attempt.id,
            "--phase",
            "after",
        ]);
    if let Some(result) = result {
        command.arg("--test-result").arg(result);
    }
    if stop {
        let source = bin.join("stop.c");
        let library = bin.join("stop.so");
        fs::write(
            &source,
            r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>
#include <unistd.h>
ssize_t write(int fd, const void *buf, size_t count) {
    ssize_t (*next)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    ssize_t result = next(fd, buf, count);
    const char *needle = "outside trusted edit surface";
    if (fd == 2 && memmem(buf, count, needle, strlen(needle))) _exit(77);
    return result;
}
"#,
        )
        .map_err(|error| error.to_string())?;
        let compiled = Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(&library)
            .arg(&source)
            .arg("-ldl")
            .output()
            .map_err(|error| error.to_string())?;
        if !compiled.status.success() {
            return Err(combined(&compiled));
        }
        command.env("LD_PRELOAD", library);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !marker.exists() {
        return Err(format!(
            "receipt surface transition did not execute: {}",
            combined(&output)
        ));
    }
    Ok(output)
}

#[cfg(unix)]
#[test]
fn receipt_surface_decision_is_not_mislabeled_as_publication_failure() -> Result<(), String> {
    for has_result in [false, true] {
        let (fixture, attempt) = edited_attempt(if has_result {
            "surface-focused"
        } else {
            "surface-ordinary"
        })?;
        let root = &fixture.root;
        let input = result_document(
            &attempt.id,
            &head_sha(root)?,
            PROPOSED_FILE,
            serde_json::json!([ok_event("equality_boundary_discounts")]),
        );
        let result = write_result(root, "surface", &input)?;
        let output = after_with_receipt_surface_transition(
            root,
            &attempt,
            has_result.then_some(result.as_path()),
            false,
        )?;
        let text = combined(&output);
        assert_eq!(
            output.status.code(),
            Some(3),
            "focused={has_result}: {text}"
        );
        assert!(text.contains("outside trusted edit surface"), "{text}");
        assert!(
            text.contains("git diff -- <path>") && text.contains("git stash"),
            "{text}"
        );
        assert!(!text.contains("fix the publication error"), "{text}");
        assert!(
            !terminal_roles(&manifest(root, &attempt)?)
                .iter()
                .any(|role| role == RESULT_ROLE)
        );
        if has_result {
            let refused = manifest(root, &attempt)?;
            write(root, "src/lib.rs", PRICE_FN)?;
            let retry = after_with(root, &attempt, &result)?;
            assert!(
                !retry.status.success(),
                "terminal refusal was retried: {}",
                combined(&retry)
            );
            assert_eq!(manifest(root, &attempt)?, refused);
        }
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn interrupted_surface_refusal_cannot_be_retried() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("surface-interrupted-refusal")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "surface-interrupted", &input)?;
    let stopped = after_with_receipt_surface_transition(root, &attempt, Some(&result), true)?;
    assert_eq!(stopped.status.code(), Some(77), "{}", combined(&stopped));
    let state = manifest(root, &attempt)?;
    write(root, "src/lib.rs", PRICE_FN)?;
    let retry = after_with(root, &attempt, &result)?;
    assert!(
        !retry.status.success(),
        "terminal decision was retried: {}",
        combined(&retry)
    );
    assert_eq!(manifest(root, &attempt)?, state);
    Ok(())
}

#[cfg(unix)]
#[test]
fn focused_result_does_not_bind_a_replaced_test_file() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-file-race")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "file-race", &input)?;
    let output = after_with_interposed_transition(root, &attempt, &result, "file")?;
    assert_eq!(
        output.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_refused_and_unrecorded(root, &attempt, &output, "focused")?;
    write(root, PROPOSED_FILE, NEW_TEST)?;
    let retry = after_with(root, &attempt, &result)?;
    assert!(retry.status.success(), "{}", combined(&retry));
    Ok(())
}

#[cfg(unix)]
#[test]
fn focused_result_does_not_bind_an_intervening_descendant_commit() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-head-race")?;
    let root = &fixture.root;
    let old_head = head_sha(root)?;
    let input = result_document(
        &attempt.id,
        &old_head,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "head-race", &input)?;
    let output = after_with_interposed_transition(root, &attempt, &result, "head")?;
    let new_head = head_sha(root)?;
    assert_ne!(old_head, new_head, "transition did not advance HEAD");
    assert_eq!(
        output.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_refused_and_unrecorded(root, &attempt, &output, "focused")?;
    let updated = result_document(
        &attempt.id,
        &new_head,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "head-retry", &updated)?;
    let retry = after_with(root, &attempt, &result)?;
    assert!(retry.status.success(), "{}", combined(&retry));
    Ok(())
}

#[test]
fn focused_result_accepts_a_descendant_committed_before_validation() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-committed-before")?;
    let root = &fixture.root;
    commit_all(root, "commit the selected test")?;
    let head = head_sha(root)?;
    let input = result_document(
        &attempt.id,
        &head,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "committed-before", &input)?;
    let output = after_with(root, &attempt, &result)?;
    assert!(output.status.success(), "{}", combined(&output));
    let record = retained_terminal(root, &manifest(root, &attempt)?, RESULT_ROLE)?;
    assert_eq!(record["head"], head);
    Ok(())
}

#[test]
fn finish_without_a_focused_result_remains_supported() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-optional")?;
    let output = run_ripr(
        &fixture.root,
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
    assert!(output.status.success(), "{}", combined(&output));
    assert!(
        !terminal_roles(&manifest(&fixture.root, &attempt)?)
            .iter()
            .any(|role| role == RESULT_ROLE)
    );
    Ok(())
}

#[test]
fn noncompliant_finish_does_not_retain_the_supplied_focused_result() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-noncompliant")?;
    let root = &fixture.root;
    write(root, "tests/outside.rs", "#[test] fn outside() {}\n")?;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "noncompliant", &input)?;
    let output = after_with(root, &attempt, &result)?;
    assert!(!output.status.success(), "{}", combined(&output));
    let state = manifest(root, &attempt)?;
    assert_eq!(state["after"]["verdict"]["status"], "violated", "{state}");
    assert!(state["after"].get("focused_result").is_none(), "{state}");
    assert!(
        !terminal_roles(&state)
            .iter()
            .any(|role| role == RESULT_ROLE)
    );
    Ok(())
}

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

fn ripr_command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(root);
    command
}

fn run_ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    ripr_command(root)
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
    use sha2::{Digest as _, Sha256};
    let artifact = manifest
        .pointer("/terminal_artifacts")
        .and_then(Value::as_array)
        .and_then(|artifacts| {
            artifacts
                .iter()
                .find(|artifact| artifact.get("role").and_then(Value::as_str) == Some(role))
        })
        .ok_or_else(|| format!("manifest has no terminal {role} artifact"))?;
    let relative = artifact
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("terminal {role} has no path"))?;
    let bytes = fs::read(root.join(relative)).map_err(|error| format!("read {role}: {error}"))?;
    let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    if artifact.get("sha256").and_then(Value::as_str) != Some(digest.as_str())
        || artifact.get("bytes").and_then(Value::as_u64) != Some(bytes.len() as u64)
    {
        return Err(format!(
            "terminal {role} bytes do not match its manifest binding"
        ));
    }
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

    let unknown = result_document(
        &attempt.id,
        &head,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts"), {"type": "other"}]),
    );
    let result = write_result(root, "unknown-type", &unknown)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "unknown type")?;

    let missing = root.with_file_name("focused-missing.json");
    let output = after_with(root, &attempt, &missing)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "read the focused test result")
}

#[test]
fn malformed_suite_event_is_refused_even_with_a_counted_test() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-malformed-suite")?;
    let root = &fixture.root;
    let head = head_sha(root)?;
    let invalid = result_document(
        &attempt.id,
        &head,
        PROPOSED_FILE,
        serde_json::json!([{"type": "suite"}, ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "invalid-suite", &invalid)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "invalid suite event")?;
    let valid = result_document(
        &attempt.id,
        &head,
        PROPOSED_FILE,
        serde_json::json!([{"type": "suite", "event": "started"}, ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "valid-suite", &valid)?;
    let output = after_with(root, &attempt, &result)?;
    assert!(output.status.success(), "{}", combined(&output));
    Ok(())
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
    write(
        root,
        "src/lib.rs",
        &format!("{edited}\n#[test] fn outside() {{}}\n"),
    )?;
    let head = head_sha(root)?;

    let outside = result_document(
        &attempt.id,
        &head,
        "src/lib.rs",
        serde_json::json!([
            ok_event("other::equality_boundary_discounts"),
            ok_event("bar::tests::equality_boundary_discounts"),
            ok_event("tests::outside"),
            ok_event("smoke")
        ]),
    );
    let result = write_result(root, "inline-outside", &outside)?;
    let output = after_with(root, &attempt, &result)?;
    assert_refused_and_unrecorded(root, &attempt, &output, "no test ran in the `tests` module")?;
    write(root, "src/lib.rs", &edited)?;

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

#[test]
fn failed_focused_result_publication_restores_the_attempt_for_retry() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-publication-failure")?;
    let root = &fixture.root;
    let document = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "publication-failure", &document)?;
    let blocked = root
        .join("target/ripr/workflow/focused-results")
        .join(&attempt.id)
        .join("focused-test-result.json");
    fs::create_dir_all(&blocked).map_err(|error| error.to_string())?;
    let output = after_with(root, &attempt, &result)?;
    assert_eq!(output.status.code(), Some(2), "{}", combined(&output));
    assert!(combined(&output).contains("fix the publication error"));
    assert!(!combined(&output).contains("focused test result recorded"));
    let state = manifest(root, &attempt)?;
    assert_eq!(state["state"], "awaiting_edit", "{state}");
    assert!(
        state.pointer("/after").is_none_or(Value::is_null),
        "{state}"
    );
    assert!(
        !terminal_roles(&state)
            .iter()
            .any(|role| role == RESULT_ROLE)
    );
    fs::remove_dir(&blocked).map_err(|error| error.to_string())?;
    let retry = after_with(root, &attempt, &result)?;
    assert!(retry.status.success(), "{}", combined(&retry));
    assert!(
        terminal_roles(&manifest(root, &attempt)?)
            .iter()
            .any(|role| role == RESULT_ROLE)
    );
    Ok(())
}

#[test]
fn failed_receipt_publication_restores_focused_finish_for_retry() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-receipt-failure")?;
    let root = &fixture.root;
    let document = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "receipt-failure", &document)?;
    let blocked = root.join("target/ripr/reports/agent-receipt.json");
    fs::create_dir_all(&blocked).map_err(|error| error.to_string())?;
    let output = after_with(root, &attempt, &result)?;
    assert_eq!(output.status.code(), Some(2), "{}", combined(&output));
    assert!(combined(&output).contains("fix the publication error"));
    let state = manifest(root, &attempt)?;
    assert_eq!(
        state["state"],
        "awaiting_edit",
        "{state}\n{}",
        combined(&output)
    );
    fs::remove_dir(&blocked).map_err(|error| error.to_string())?;
    let retry = after_with(root, &attempt, &result)?;
    assert!(retry.status.success(), "{}", combined(&retry));
    assert!(
        terminal_roles(&manifest(root, &attempt)?)
            .iter()
            .any(|role| role == RESULT_ROLE)
    );
    Ok(())
}

// Interpose exactly after an atomic publication, rather than racing a polling
// thread. The hook is GNU/Linux-only; ordinary tests remain platform independent.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn after_with_publication_hook(
    root: &Path,
    attempt: &Attempt,
    result: &Path,
    mode: &str,
) -> Result<Output, String> {
    let dir = root.join(".git/ripr-publication-hook");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let source = dir.join("hook.c");
    let library = dir.join("hook.so");
    fs::write(
        &source,
        r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int rename(const char *old, const char *new) {
    int (*next)(const char *, const char *) = dlsym(RTLD_NEXT, "rename");
    int result = next(old, new);
    const char *target = getenv("RIPR_HOOK_TARGET");
    if (result || !target || strcmp(new, target)) return result;
    const char *mode = getenv("RIPR_HOOK_MODE");
    const char *marker = getenv("RIPR_HOOK_MARKER");
    FILE *mark = fopen(marker, "w"); if (mark) fclose(mark);
    static int matches = 0; matches++;
    if (!strcmp(mode, "stop") || !strcmp(mode, "stop-finish") || !strcmp(mode, "stop-retention") || (!strcmp(mode, "stop-start") && matches == 2) || (!strcmp(mode, "stop-accepted") && matches == 3)) _exit(77);
    if (!strcmp(mode, "replace")) {
        FILE *file = fopen(new, "w");
        if (!file) _exit(91);
        fputs("foreign bytes\n", file); fclose(file);
    } else if (!strcmp(mode, "replace-receipt")) {
        FILE *file = fopen(getenv("RIPR_HOOK_RECEIPT"), "a");
        if (!file) _exit(93);
        fputs(" ", file); fclose(file);
    } else if (!strcmp(mode, "interpose")) {
        unsetenv("LD_PRELOAD");
        if (system("sh \"$RIPR_HOOK_SCRIPT\"") == -1) _exit(92);
    }
    return result;
}
"#,
    )
    .map_err(|error| error.to_string())?;
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(&source)
        .arg("-ldl")
        .output()
        .map_err(|error| error.to_string())?;
    if !compiled.status.success() {
        return Err(format!("compile test hook: {}", combined(&compiled)));
    }
    let script = dir.join("second-after.sh");
    fs::write(&script, "\"$RIPR_HOOK_BIN\" agent repair --json --root \"$RIPR_HOOK_ROOT\" --attempt \"$RIPR_HOOK_ATTEMPT\" --phase after >\"$RIPR_HOOK_SECOND_OUT\" 2>&1\nprintf '%s' \"$?\" >\"$RIPR_HOOK_SECOND_STATUS\"\n").map_err(|error| error.to_string())?;
    let target = if mode == "stop-finish"
        || mode == "stop-retention"
        || mode == "stop-start"
        || mode == "stop-accepted"
    {
        root.join("target/ripr/repair-attempts")
            .join(&attempt.id)
            .join("attempt.json")
    } else if mode == "replace" || mode == "replace-receipt" {
        root.join("target/ripr/workflow/focused-results")
            .join(&attempt.id)
            .join("focused-test-result.json")
    } else {
        root.join("target/ripr/reports/agent-receipt.json")
    };
    let marker = dir.join("marker");
    if marker.exists() {
        fs::remove_file(&marker).map_err(|error| error.to_string())?;
    }
    let output = ripr_command(root)
        .env("LD_PRELOAD", &library)
        .env("RIPR_HOOK_TARGET", target)
        .env(
            "RIPR_HOOK_RECEIPT",
            root.join("target/ripr/reports/agent-receipt.json"),
        )
        .env("RIPR_HOOK_MODE", mode)
        .env("RIPR_HOOK_MARKER", &marker)
        .env("RIPR_HOOK_SCRIPT", script)
        .env("RIPR_HOOK_BIN", env!("CARGO_BIN_EXE_ripr"))
        .env("RIPR_HOOK_ROOT", root)
        .env("RIPR_HOOK_ATTEMPT", &attempt.id)
        .env("RIPR_HOOK_SECOND_OUT", dir.join("second.out"))
        .env("RIPR_HOOK_SECOND_STATUS", dir.join("second.status"))
        .args([
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
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !marker.exists() {
        return Err(format!(
            "publication hook did not run: {}",
            combined(&output)
        ));
    }
    Ok(output)
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn pending_promotion_rejects_an_interposed_receipt_replacement() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-promotion-swap")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "promotion-swap", &input)?;
    let stopped = after_with_publication_hook(root, &attempt, &result, "stop")?;
    assert_eq!(stopped.status.code(), Some(77), "{}", combined(&stopped));
    let receipt_path = root.join("target/ripr/reports/agent-receipt.json");
    let original = fs::read(&receipt_path).map_err(|error| error.to_string())?;
    let before = manifest(root, &attempt)?;
    let interposed = after_with_publication_hook(root, &attempt, &result, "replace-receipt")?;
    assert!(!interposed.status.success(), "{}", combined(&interposed));
    let mut foreign = original.clone();
    foreign.push(b' ');
    assert_eq!(
        fs::read(&receipt_path).map_err(|error| error.to_string())?,
        foreign
    );
    assert_eq!(manifest(root, &attempt)?, before);
    fs::write(&receipt_path, original).map_err(|error| error.to_string())?;
    let recovery = after_with(root, &attempt, &result)?;
    assert!(recovery.status.success(), "{}", combined(&recovery));
    let retained = manifest(root, &attempt)?;
    for role in ["agent_receipt", "agent_verify", RESULT_ROLE] {
        retained_terminal(root, &retained, role)?;
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn interrupted_receipt_evaluation_fails_closed() -> Result<(), String> {
    for mode in ["stop-start", "stop-accepted"] {
        let (fixture, attempt) = edited_attempt(mode)?;
        let root = &fixture.root;
        let input = result_document(
            &attempt.id,
            &head_sha(root)?,
            PROPOSED_FILE,
            serde_json::json!([ok_event("equality_boundary_discounts")]),
        );
        let result = write_result(root, mode, &input)?;
        let stopped = after_with_publication_hook(root, &attempt, &result, mode)?;
        assert_eq!(stopped.status.code(), Some(77), "{}", combined(&stopped));
        let state = manifest(root, &attempt)?;
        assert_eq!(
            state["focused_receipt_evaluation"]["state"],
            if mode == "stop-start" {
                "started"
            } else {
                "accepted"
            }
        );
        assert!(terminal_roles(&state).is_empty(), "{state}");
        let retry = after_with(root, &attempt, &result)?;
        assert_eq!(retry.status.code(), Some(3), "{}", combined(&retry));
        assert_eq!(manifest(root, &attempt)?, state);
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn interrupted_after_retention_can_recover_success() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-post-retention")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "post-retention", &input)?;
    let first = after_with_publication_hook(root, &attempt, &result, "stop")?;
    assert_eq!(first.status.code(), Some(77), "{}", combined(&first));
    let second = after_with_publication_hook(root, &attempt, &result, "stop-retention")?;
    assert_eq!(second.status.code(), Some(77), "{}", combined(&second));
    let retained = manifest(root, &attempt)?;
    for role in ["agent_receipt", "agent_verify", RESULT_ROLE] {
        retained_terminal(root, &retained, role)?;
    }
    let changed_input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([{"type":"test", "event":"failed", "name":"equality_boundary_discounts"}]),
    );
    let changed = write_result(root, "post-retention-changed", &changed_input)?;
    let wrong = after_with(root, &attempt, &changed)?;
    assert_eq!(wrong.status.code(), Some(3), "{}", combined(&wrong));
    let no_result = run_ripr(
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
    assert!(!no_result.status.success(), "{}", combined(&no_result));
    write(root, PROPOSED_FILE, &format!("{NEW_TEST}\n// drift\n"))?;
    let drift = after_with(root, &attempt, &result)?;
    assert!(!drift.status.success(), "{}", combined(&drift));
    write(root, PROPOSED_FILE, NEW_TEST)?;
    assert_eq!(manifest(root, &attempt)?, retained);
    let retry = after_with(root, &attempt, &result)?;
    assert!(retry.status.success(), "{}", combined(&retry));
    assert_eq!(stdout_json(&retry)?["kind"], "repair_after_result");
    assert_eq!(manifest(root, &attempt)?, retained);
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn interrupted_focused_finish_recovers_the_full_evidence_set() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-interrupted")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "interrupted", &input)?;
    let first = after_with_publication_hook(root, &attempt, &result, "stop")?;
    assert_eq!(first.status.code(), Some(77), "{}", combined(&first));
    let recovery = after_with(root, &attempt, &result)?;
    assert!(recovery.status.success(), "{}", combined(&recovery));
    assert_eq!(stdout_json(&recovery)?["kind"], "repair_after_result");
    let state = manifest(root, &attempt)?;
    let roles = terminal_roles(&state);
    for role in ["agent_receipt", "agent_verify", RESULT_ROLE] {
        assert!(
            roles.iter().any(|found| found == role),
            "missing {role}: {state}"
        );
    }
    let receipt = retained_terminal(root, &state, "agent_receipt")?;
    let verify = retained_terminal(root, &state, "agent_verify")?;
    assert_eq!(receipt["repair_attempt"]["attempt_id"], attempt.id);
    assert_eq!(verify["tool"], "ripr");
    let record = retained_terminal(root, &state, RESULT_ROLE)?;
    assert_eq!(
        record["ran_tests"],
        serde_json::json!(["equality_boundary_discounts"])
    );
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn interrupted_focused_recovery_refuses_changed_claim_and_partial_publication() -> Result<(), String>
{
    let (fixture, attempt) = edited_attempt("focused-interrupted-partial")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "interrupted-original", &input)?;
    let stopped = after_with_publication_hook(root, &attempt, &result, "stop")?;
    assert_eq!(stopped.status.code(), Some(77), "{}", combined(&stopped));
    let pending = manifest(root, &attempt)?;
    assert!(terminal_roles(&pending).is_empty(), "{pending}");
    let changed_input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([{"type": "test", "event": "failed", "name": "equality_boundary_discounts"}]),
    );
    let changed = write_result(root, "interrupted-changed", &changed_input)?;
    let refused = after_with(root, &attempt, &changed)?;
    assert_eq!(refused.status.code(), Some(3), "{}", combined(&refused));
    assert_eq!(stdout_json(&refused)?["kind"], "repair_after_refusal");
    assert_eq!(manifest(root, &attempt)?, pending);
    let blocked = root
        .join("target/ripr/workflow/focused-results")
        .join(&attempt.id)
        .join("focused-test-result.json");
    fs::create_dir_all(&blocked).map_err(|error| error.to_string())?;
    let failed = after_with(root, &attempt, &result)?;
    assert_eq!(failed.status.code(), Some(2), "{}", combined(&failed));
    assert!(terminal_roles(&manifest(root, &attempt)?).is_empty());
    fs::remove_dir(&blocked).map_err(|error| error.to_string())?;
    let retry = after_with(root, &attempt, &result)?;
    assert!(retry.status.success(), "{}", combined(&retry));
    let state = manifest(root, &attempt)?;
    for role in ["agent_receipt", "agent_verify", RESULT_ROLE] {
        retained_terminal(root, &state, role)?;
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn interrupted_before_receipt_requires_and_retries_the_same_claim() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-before-receipt")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "before-receipt", &input)?;
    let first = after_with_publication_hook(root, &attempt, &result, "stop-finish")?;
    assert_eq!(first.status.code(), Some(77), "{}", combined(&first));
    let pending = manifest(root, &attempt)?;
    assert_eq!(pending["state"], "ready_to_finish", "{pending}");
    let without = run_ripr(
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
    assert_eq!(without.status.code(), Some(3), "{}", combined(&without));
    assert!(
        combined(&without).contains("same --test-result"),
        "{}",
        combined(&without)
    );
    assert_eq!(manifest(root, &attempt)?, pending);
    let changed_input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([{ "type": "test", "event": "failed", "name": "equality_boundary_discounts" }]),
    );
    let changed = write_result(root, "changed-claim", &changed_input)?;
    let refused = after_with(root, &attempt, &changed)?;
    assert_eq!(refused.status.code(), Some(3), "{}", combined(&refused));
    assert!(
        combined(&refused).contains("differs from the interrupted finish"),
        "{}",
        combined(&refused)
    );
    assert_eq!(manifest(root, &attempt)?, pending);
    let retry = after_with(root, &attempt, &result)?;
    assert!(retry.status.success(), "{}", combined(&retry));
    let state = manifest(root, &attempt)?;
    let record = retained_terminal(root, &state, RESULT_ROLE)?;
    assert_eq!(
        record["ran_tests"],
        serde_json::json!(["equality_boundary_discounts"])
    );
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn another_after_cannot_drop_or_undo_a_focused_result() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-second-after")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "second-after", &input)?;
    let first = after_with_publication_hook(root, &attempt, &result, "interpose")?;
    let second = fs::read_to_string(root.join(".git/ripr-publication-hook/second.out"))
        .map_err(|error| error.to_string())?;
    assert!(
        second.contains("another repair phase is using the shared workflow artifacts"),
        "{second}"
    );
    assert!(first.status.success(), "{}", combined(&first));
    let state = manifest(root, &attempt)?;
    assert_eq!(state["state"], "ready_to_finish", "{state}");
    let record = retained_terminal(root, &state, RESULT_ROLE)?;
    assert_eq!(
        record["ran_tests"],
        serde_json::json!(["equality_boundary_discounts"])
    );
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn foreign_focused_bytes_are_not_retained() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-foreign-bytes")?;
    let root = &fixture.root;
    let input = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([ok_event("equality_boundary_discounts")]),
    );
    let result = write_result(root, "foreign", &input)?;
    let output = after_with_publication_hook(root, &attempt, &result, "replace")?;
    assert!(!output.status.success(), "{}", combined(&output));
    assert!(
        combined(&output).contains("foreign bytes"),
        "{}",
        combined(&output)
    );
    let state = manifest(root, &attempt)?;
    assert_eq!(state["state"], "awaiting_edit", "{state}");
    assert!(
        !terminal_roles(&state)
            .iter()
            .any(|role| role == RESULT_ROLE)
    );
    let retry = after_with(root, &attempt, &result)?;
    assert!(retry.status.success(), "{}", combined(&retry));
    let state = manifest(root, &attempt)?;
    let record = retained_terminal(root, &state, RESULT_ROLE)?;
    assert_eq!(
        record["ran_tests"],
        serde_json::json!(["equality_boundary_discounts"])
    );
    Ok(())
}

/// A failed test ran, so it counts as ran; the record keeps the failure
/// visible so a later gate can require `failed == 0`.
#[test]
fn a_failed_only_result_is_recorded_as_ran_with_the_failure_visible() -> Result<(), String> {
    let (fixture, attempt) = edited_attempt("focused-failed")?;
    let root = &fixture.root;
    let document = result_document(
        &attempt.id,
        &head_sha(root)?,
        PROPOSED_FILE,
        serde_json::json!([
            {"type": "test", "event": "failed", "name": "equality_boundary_discounts"}
        ]),
    );
    let result = write_result(root, "failed-only", &document)?;
    let output = after_with(root, &attempt, &result)?;
    assert!(output.status.success(), "{}", combined(&output));
    let record = retained_terminal(root, &manifest(root, &attempt)?, RESULT_ROLE)?;
    assert_eq!(record["failed"], serde_json::json!(1));
    assert_eq!(record["passed"], serde_json::json!(0));
    assert_eq!(
        record["ran_tests"],
        serde_json::json!(["equality_boundary_discounts"])
    );
    Ok(())
}
