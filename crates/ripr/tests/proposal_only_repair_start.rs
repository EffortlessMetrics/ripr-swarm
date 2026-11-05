//! #13203 A0: a seam whose only repair target is a producer-owned new-test
//! proposal (`Proposed`, InlineUnit or Integration) may start a repair, with the
//! card's workspace identity minted by the producer; every proposal the
//! producer does not admit still refuses with the no-target text.
//!
//! Real binary: `ripr agent repair --phase before` over small libraries. The
//! oracles are the command's exit and stderr and the retained edit-cage
//! baseline policy; nothing here reimplements admission or the cage.

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

/// A seam the producer admits no proposal for is refused before any attempt
/// exists, by the existing no-route refusal (the no-target text is for the
/// proposal-without-identity case, which no real producer output reaches).
const NOT_STARTED_TEXT: &str = "disposition=not_started";

const PRICE_FN: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";

/// A library with one public seam without a test and an unrelated integration test,
/// which establishes the `tests/` layout the integration proposal requires.
fn fixture(label: &str) -> Result<Fixture, String> {
    fixture_with(label, PRICE_FN, true)
}

/// `lib_source` is the library root; `with_tests_layout` adds an unrelated
/// `tests/smoke.rs` (the layout an integration proposal needs).
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
    id: String,
}

fn start(root: &Path) -> Result<Output, String> {
    let root_arg = root.display().to_string();
    let seam = seam_id(root, &root_arg)?;
    run_ripr(
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
    )
}

fn begin(root: &Path) -> Result<Attempt, String> {
    let output = start(root)?;
    if !output.status.success() {
        return Err(format!("before phase failed: {}", combined(&output)));
    }
    let id = stdout_json(&output)?
        .pointer("/repair_attempt/attempt_id")
        .and_then(Value::as_str)
        .ok_or("before phase named no attempt")?
        .to_owned();
    Ok(Attempt { id })
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

/// The cage the started attempt enforces, read from the retained baseline.
fn cage_policy(root: &Path, attempt: &Attempt) -> Result<Value, String> {
    let before = manifest(root, attempt)?;
    let baseline = retained(root, &before, "edit_cage_baseline")?;
    baseline
        .pointer("/policy")
        .cloned()
        .ok_or_else(|| format!("baseline has no policy:\n{baseline}"))
}

fn assert_refused_not_started(root: &Path, context: &str) -> Result<(), String> {
    let output = start(root)?;
    let text = combined(&output);
    if output.status.success() || !text.contains(NOT_STARTED_TEXT) {
        return Err(format!("{context}: expected a not-started refusal\n{text}"));
    }
    let attempts = root.join("target/ripr/repair-attempts");
    let published: Vec<String> = fs::read_dir(&attempts)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| !name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    if !published.is_empty() {
        return Err(format!(
            "{context}: a refused start published {published:?}"
        ));
    }
    Ok(())
}

#[test]
fn integration_proposal_only_seam_starts_with_the_proposed_file_as_its_cage() -> Result<(), String>
{
    let fixture = fixture("a0-integration")?;
    let attempt = begin(&fixture.root)?;
    let policy = cage_policy(&fixture.root, &attempt)?;
    assert_eq!(
        policy
            .pointer("/selected_target/path")
            .and_then(Value::as_str),
        Some(PROPOSED_FILE),
        "{policy}"
    );
    assert_eq!(
        policy
            .pointer("/allowed_edit_surface")
            .and_then(Value::as_array)
            .map(|surface| surface.iter().filter_map(|rule| rule.get("path")).count()),
        Some(1),
        "the cage must be exactly the proposed file: {policy}"
    );
    assert!(
        !fixture.root.join(PROPOSED_FILE).exists(),
        "starting must not create the proposed file"
    );
    Ok(())
}

#[test]
fn inline_proposal_only_seam_starts_confined_to_its_governed_module() -> Result<(), String> {
    // A private owner has no integration proposal, so the one governed inline
    // module is the only producer-owned target.
    let source = "fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n\npub fn unrelated() -> i32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn unrelated_is_one() {\n        assert_eq!(unrelated(), 1);\n    }\n}\n".to_string();
    let fixture = fixture_with("a0-inline", &source, false)?;
    let attempt = begin(&fixture.root)?;
    let policy = cage_policy(&fixture.root, &attempt)?;
    assert_eq!(
        policy
            .pointer("/selected_target/path")
            .and_then(Value::as_str),
        Some("src/lib.rs"),
        "{policy}"
    );
    assert_eq!(
        policy.pointer("/inline_test_module_target"),
        Some(&Value::Bool(true)),
        "an inline proposal confines the edit to the governed module: {policy}"
    );
    Ok(())
}

/// No producer-owned proposal exists (no tests/ layout, no inline module):
/// the seam stays unstartable.
#[test]
fn seam_without_any_proposal_still_refuses() -> Result<(), String> {
    let fixture = fixture_with("a0-missing", PRICE_FN, false)?;
    assert_refused_not_started(&fixture.root, "no proposal")
}

/// Two governed inline test modules: the insertion anchor is ambiguous, so the
/// producer admits nothing and the start still refuses.
#[test]
fn ambiguous_inline_modules_still_refuse() -> Result<(), String> {
    let source = "fn discounted_total(amount: i32) -> i32 {\n    if amount >= 10 {\n        amount - 1\n    } else {\n        amount\n    }\n}\n\n#[cfg(test)]\nmod first {\n    #[test]\n    fn a() {}\n}\n\n#[cfg(test)]\nmod second {\n    #[test]\n    fn b() {}\n}\n".to_string();
    let fixture = fixture_with("a0-ambiguous", &source, false)?;
    assert_refused_not_started(&fixture.root, "ambiguous modules")
}

/// The integration proposal's collision rule: when both candidate files
/// already exist the producer admits nothing.
#[test]
fn colliding_integration_files_still_refuse() -> Result<(), String> {
    let fixture = fixture("a0-collision")?;
    write(
        &fixture.root,
        "tests/discounted_total.rs",
        "#[test]\nfn other() {}\n",
    )?;
    write(
        &fixture.root,
        "tests/discounted_total_boundary.rs",
        "#[test]\nfn other() {}\n",
    )?;
    commit_all(&fixture.root, "both candidates exist")?;
    assert_refused_not_started(&fixture.root, "file collision")
}
