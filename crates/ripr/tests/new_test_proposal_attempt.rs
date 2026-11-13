//! #13203 slice A: a repair attempt binds the producer-owned new-test proposal
//! it follows. Real binary: `ripr agent repair --phase before/after` over a
//! library whose only seam has no related test, so the producer proposes one
//! new integration test file.
//!
//! The oracles are the retained packet, the retained edit-cage baseline policy
//! and the attempt manifest the commands already wrote; nothing here
//! reimplements the cage.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

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

fn assert_no_published_attempt(root: &Path, diagnostic: &str) -> Result<(), String> {
    let attempts = root.join("target/ripr/repair-attempts");
    let entries = match fs::read_dir(&attempts) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("read {}: {error}", attempts.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("repair-attempt-")
        {
            continue;
        }
        let path = entry.path();
        match fs::symlink_metadata(path.join("attempt.json")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(format!(
                    "refusal published a manifest at {}",
                    path.display()
                ));
            }
            Err(error) => return Err(format!("inspect {}: {error}", path.display())),
        }
        if !diagnostic.contains(&path.display().to_string())
            || !diagnostic.contains("inspect it before any manual cleanup")
        {
            return Err(format!(
                "refusal did not identify retained reservation {}: {diagnostic}",
                path.display()
            ));
        }
    }
    Ok(())
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
    write(
        &root,
        "src/lib.rs",
        "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
    )?;
    write(&root, "tests/smoke.rs", "#[test]\nfn smoke() {}\n")?;
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

fn after(root: &Path, attempt: &Attempt) -> Result<Output, String> {
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
        ],
    )
}

fn manifest(root: &Path, attempt: &Attempt) -> Result<Value, String> {
    let path = root
        .join("target/ripr/repair-attempts")
        .join(&attempt.id)
        .join("attempt.json");
    let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse manifest: {error}"))
}

fn retained(root: &Path, manifest: &Value, role: &str) -> Result<Value, String> {
    let relative = manifest
        .pointer("/artifacts")
        .and_then(Value::as_array)
        .and_then(|artifacts| {
            artifacts
                .iter()
                .find(|artifact| artifact.get("role").and_then(Value::as_str) == Some(role))
        })
        .and_then(|artifact| artifact.get("path").and_then(Value::as_str))
        .ok_or_else(|| format!("manifest has no {role} artifact"))?;
    let bytes = fs::read(root.join(relative)).map_err(|error| format!("read {role}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {role}: {error}"))
}

fn violation_kinds(manifest: &Value) -> Vec<String> {
    manifest
        .pointer("/after/verdict/violations")
        .and_then(Value::as_array)
        .map(|violations| {
            violations
                .iter()
                .filter_map(|violation| violation.get("kind").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Proposal -> edit inside the cage -> bound: the attempt records the proposal
/// it followed and credits only the created proposed file.
#[test]
fn created_proposed_file_binds_the_attempt_to_its_proposal() -> Result<(), String> {
    let fixture = fixture("proposal-bound")?;
    let root = &fixture.root;
    let attempt = begin(root)?;
    let before = manifest(root, &attempt)?;

    let packet = retained(root, &before, "agent_packet")?;
    let packet_target = packet
        .pointer("/packets/0/recommended_test")
        .or_else(|| packet.pointer("/recommended_test"))
        .ok_or_else(|| format!("packet has no recommended_test:\n{packet}"))?;
    assert_eq!(
        packet_target
            .pointer("/target_kind")
            .and_then(Value::as_str),
        Some("new_integration_test"),
        "the packet must name the proposal kind: {packet_target}"
    );
    assert_eq!(
        packet_target.pointer("/file").and_then(Value::as_str),
        Some(PROPOSED_FILE)
    );

    let baseline = retained(root, &before, "edit_cage_baseline")?;
    assert_eq!(
        baseline
            .pointer("/policy/new_test_proposal")
            .cloned()
            .unwrap_or(Value::Null),
        serde_json::json!({"kind": "integration", "file": PROPOSED_FILE}),
        "the attempt must record the proposal it follows: {baseline}"
    );

    write(root, PROPOSED_FILE, NEW_TEST)?;
    let output = after(root, &attempt)?;
    let finished = manifest(root, &attempt)?;
    assert_eq!(
        finished
            .pointer("/after/verdict/status")
            .and_then(Value::as_str),
        Some("compliant"),
        "{}\n{finished}",
        combined(&output)
    );
    // The cage also lists ripr's own operational writes under `target/ripr`;
    // the attempt's credited edit is the one non-operational path.
    let edited: Vec<String> = finished
        .pointer("/after/verdict/changed_paths")
        .and_then(Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .filter(|path| !path.starts_with("target/ripr/"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(edited, [PROPOSED_FILE]);
    Ok(())
}

/// An edit outside the proposal's cage is refused even when it is a test file.
#[test]
fn edit_outside_the_proposals_cage_is_refused() -> Result<(), String> {
    let fixture = fixture("proposal-out-of-cage")?;
    let root = &fixture.root;
    let attempt = begin(root)?;
    write(root, "tests/elsewhere.rs", NEW_TEST)?;
    let output = after(root, &attempt)?;
    let finished = manifest(root, &attempt)?;
    assert_eq!(
        finished
            .pointer("/after/verdict/status")
            .and_then(Value::as_str),
        Some("violated"),
        "{}\n{finished}",
        combined(&output)
    );
    let kinds = violation_kinds(&finished);
    assert!(
        kinds.iter().any(|kind| kind == "outside_allowed_surface")
            && kinds
                .iter()
                .any(|kind| kind == "selected_target_not_changed"),
        "unexpected violations {kinds:?}"
    );
    assert!(
        !output.status.success(),
        "an out-of-cage edit must not exit zero: {}",
        combined(&output)
    );
    Ok(())
}

fn limitations(manifest: &Value) -> Vec<String> {
    manifest
        .pointer("/limitations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// #13205: a followed new-test proposal is preflighted read-only and the
/// attempt says plainly that no test exists yet, naming the owning package;
/// it never reads as an empty test result.
#[test]
fn a_proposal_attempt_says_no_test_exists_yet() -> Result<(), String> {
    let fixture = fixture("proposal-preflight")?;
    let root = &fixture.root;
    let attempt = begin(root)?;
    let before = manifest(root, &attempt)?;
    let notes: Vec<String> = limitations(&before)
        .into_iter()
        .filter(|note| note.starts_with("target preflight:"))
        .collect();
    assert_eq!(notes.len(), 1, "{before}");
    assert!(
        notes[0].contains(PROPOSED_FILE)
            && notes[0].contains("at the before phase")
            && notes[0].contains("had not yet been written")
            && notes[0].contains("package `.`")
            && notes[0].contains("no test result was available"),
        "{}",
        notes[0]
    );
    assert!(
        !root.join(PROPOSED_FILE).exists(),
        "the preflight must not create the file"
    );

    // #13205: the disclosure is historical wording, so it must stay accurate
    // once the proposed test exists and the after phase has run.
    write(root, PROPOSED_FILE, NEW_TEST)?;
    let output = after(root, &attempt)?;
    let finished = manifest(root, &attempt)?;
    assert_eq!(
        finished
            .pointer("/after/verdict/status")
            .and_then(Value::as_str),
        Some("compliant"),
        "{}\n{finished}",
        combined(&output)
    );
    let retained: Vec<String> = limitations(&finished)
        .into_iter()
        .filter(|note| note.starts_with("target preflight:"))
        .collect();
    assert_eq!(retained.len(), 1, "{finished}");
    assert!(
        retained[0].contains(PROPOSED_FILE)
            && retained[0].contains("at the before phase")
            && retained[0].contains("had not yet been written"),
        "{}",
        retained[0]
    );
    Ok(())
}

/// The negative: when the seam already has a related test, the target is an
/// existing one and the attempt carries no not-yet disclosure.
#[test]
fn an_existing_test_target_attempt_has_no_not_yet_disclosure() -> Result<(), String> {
    let fixture = fixture("existing-preflight")?;
    let root = &fixture.root;
    write(
        root,
        "tests/existing.rs",
        "use proposal_fixture::discounted_total;\n\n#[test]\nfn below_threshold_keeps_amount() {\n    assert_eq!(discounted_total(5, 100), 5);\n}\n",
    )?;
    commit_all(root, "existing test")?;
    let attempt = begin(root)?;
    let before = manifest(root, &attempt)?;
    assert!(
        limitations(&before)
            .iter()
            .all(|note| !note.starts_with("target preflight:")),
        "{before}"
    );
    Ok(())
}

/// The preflight refuses a defective target at start: a symlinked `tests`
/// directory (the proposal's parent) publishes no attempt manifest; any
/// retained reservation is disclosed, and the fixture without the defect starts.
#[cfg(unix)]
#[test]
fn a_symlinked_tests_directory_fails_the_preflight_and_leaves_no_attempt() -> Result<(), String> {
    let fixture = fixture("preflight-symlink")?;
    let root = &fixture.root;
    // The fixture's tests/ directory is replaced by a link to a real one.
    let real = root.join("real-tests");
    fs::rename(root.join("tests"), &real).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(&real, root.join("tests")).map_err(|error| error.to_string())?;
    commit_all(root, "symlinked tests")?;
    let root_arg = root.display().to_string();
    let seam = seam_id(root, &root_arg)?;
    let args = [
        "agent",
        "repair",
        "--json",
        "--root",
        &root_arg,
        "--seam-id",
        &seam,
        "--phase",
        "before",
    ];
    let output = run_ripr(root, &args)?;
    let text = combined(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("symlinked directory"), "{text}");
    assert_no_published_attempt(root, &text)?;
    // Without the defect the same fixture starts.
    fs::remove_file(root.join("tests")).map_err(|error| error.to_string())?;
    fs::rename(&real, root.join("tests")).map_err(|error| error.to_string())?;
    commit_all(root, "real tests")?;
    let started = begin(root)?;
    assert!(!started.id.is_empty());
    Ok(())
}
