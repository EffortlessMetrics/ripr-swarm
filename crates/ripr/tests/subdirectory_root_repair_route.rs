//! A repository whose Python package lives below the Git top-level: `ripr agent
//! repair --root <subdir>` refuses ("attempt root ... is not the exact Git
//! top-level") before any file is written under the subdirectory (#15363).
//! Withholding the start that `ripr check --root <subdir>` still prints is #15488.

#![cfg(feature = "lang-python")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| format!("create {}: {error}", to.display()))?;
    for entry in
        std::fs::read_dir(from).map_err(|error| format!("read {}: {error}", from.display()))?
    {
        let entry = entry.map_err(|error| format!("entry: {error}"))?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .map_err(|error| format!("copy {}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Git top-level `repo/` holding the Python fixture package under `repo/backend`.
/// Returns the guard, the top-level and the package root.
fn monorepo(label: &str) -> Result<(TempDirGuard, PathBuf, PathBuf), String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/blind_journey_installed_python/repository");
    let launch = std::env::temp_dir().join(format!(
        "ripr-subdir-repair-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let guard = TempDirGuard(launch.clone());
    let top = launch.join("repo");
    let backend = top.join("backend");
    copy_tree(&source.join("base"), &backend)?;
    fixture_git_ok(&top, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(&top, &["config", "core.autocrlf", "false"])?;
    fixture_git_ok(&top, &["config", "commit.gpgsign", "false"])?;
    fixture_git_ok(&top, &["config", "user.name", "fixture"])?;
    fixture_git_ok(&top, &["config", "user.email", "fixture@example.invalid"])?;
    fixture_git_ok(&top, &["add", "-A"])?;
    fixture_git_ok(&top, &["commit", "-q", "-m", "base"])?;
    copy_tree(&source.join("head"), &backend)?;
    fixture_git_ok(&top, &["add", "-A"])?;
    fixture_git_ok(&top, &["commit", "-q", "-m", "head"])?;
    Ok((guard, top, backend))
}

fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr: {error}"))
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Positive control: the same package checked from the Git top-level offers
/// the start, so the refusal below is about the subdirectory, not the fixture.
#[test]
fn check_at_the_git_top_level_still_offers_the_repair_start() -> Result<(), String> {
    let (_guard, top, _backend) = monorepo("top")?;
    std::fs::write(
        top.join("ripr.toml"),
        "[languages]\nenabled = [\"python\"]\n",
    )
    .map_err(|error| format!("ripr.toml: {error}"))?;
    let output = ripr(&top, &["check", "--quiet", "--base", "HEAD~1"])?;
    let text = stdout_of(&output);
    if !text.contains("agent repair --gap-id") {
        return Err(format!("top-level check lost its repair start:\n{text}"));
    }
    Ok(())
}

#[test]
fn agent_repair_refuses_a_subdirectory_root_before_writing_anything() -> Result<(), String> {
    let (_guard, _top, backend) = monorepo("refuse")?;
    let output = ripr(
        &backend,
        &[
            "agent",
            "repair",
            "--gap-id",
            "gap:python:pricing/core.py:price:predicate_boundary:predicate:qty>10",
            "--base",
            "HEAD~1",
            "--root",
            &backend.to_string_lossy(),
            "--phase",
            "before",
        ],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.code() != Some(2) || !stderr.contains("is not the exact Git top-level") {
        return Err(format!(
            "expected the top-level refusal with exit 2, got {:?}:\n{stderr}",
            output.status.code()
        ));
    }
    if backend.join("target").exists() {
        return Err(format!(
            "the refusal wrote under the subdirectory root:\n{stderr}"
        ));
    }
    Ok(())
}
