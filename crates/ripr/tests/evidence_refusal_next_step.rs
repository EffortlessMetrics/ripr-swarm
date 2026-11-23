//! `ripr pr-evidence --check` and `ripr impacted-evidence --check` refusals
//! for a missing or broken packet name the command that writes it, not just
//! the read error. Each case runs the real binary in a scratch directory and
//! pins the next-step clause, so a regression that drops it fails here rather
//! than only in the looser action-word floor (`refusal_next_step.rs`).

#[path = "common/mod.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

fn scratch() -> Result<ScratchDir, String> {
    let dir = std::env::temp_dir().join(format!(
        "ripr-evidence-refusal-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|error| format!("create scratch dir: {error}"))?;
    Ok(ScratchDir(dir))
}

/// Fixture git goes through the shared helper, which drops inherited `GIT_*`
/// selectors (for example a hook's `GIT_INDEX_FILE`) so setup cannot touch
/// the caller's repository.
fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let mut full = vec![
        "-c",
        "user.name=ripr",
        "-c",
        "user.email=ripr@example.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    full.extend_from_slice(args);
    common::fixture_git::fixture_git_ok(dir, &full)
}

/// `ripr` in `dir` with inherited `GIT_*` selectors removed, so the binary
/// answers for the scratch directory rather than the caller's repository.
fn ripr(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(dir).args(args);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
}

fn committed_repo() -> Result<ScratchDir, String> {
    let dir = scratch()?;
    git(&dir.0, &["init", "-q"])?;
    std::fs::write(dir.0.join("README.md"), "fixture\n")
        .map_err(|error| format!("write README.md: {error}"))?;
    git(&dir.0, &["add", "README.md"])?;
    git(&dir.0, &["commit", "-q", "-m", "fixture"])?;
    Ok(dir)
}

/// Runs `ripr` in `dir` and returns stderr when the exit code is 2.
fn refusal(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = ripr(dir, args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.code() == Some(2) {
        Ok(stderr)
    } else {
        Err(format!(
            "ripr {args:?}: expected exit 2, got {:?}:\n{stderr}",
            output.status.code()
        ))
    }
}

fn stderr_has_all(stderr: &str, wanted: &[&str]) -> Result<(), String> {
    match wanted.iter().find(|clause| !stderr.contains(*clause)) {
        None => Ok(()),
        Some(missing) => Err(format!("stderr lacks `{missing}`:\n{stderr}")),
    }
}

const CHECK: &[&str] = &["pr-evidence", "--base", "HEAD", "--head", "HEAD", "--check"];

#[test]
fn pr_evidence_check_without_a_packet_names_the_command_that_writes_it() -> Result<(), String> {
    let repo = committed_repo()?;
    let stderr = refusal(&repo.0, CHECK)?;
    stderr_has_all(
        &stderr,
        &[
            "missing or unreadable target/ripr/pr/repo-exposure.json: ",
            "; run `ripr pr-evidence` with the same --base, --head and --root to write it, then rerun with --check",
        ],
    )
}

#[test]
fn pr_evidence_check_with_a_broken_packet_names_the_regeneration() -> Result<(), String> {
    let repo = committed_repo()?;
    let packet = repo.0.join("target/ripr/pr");
    std::fs::create_dir_all(&packet).map_err(|error| format!("create packet dir: {error}"))?;
    std::fs::write(packet.join("repo-exposure.json"), "{ not json")
        .map_err(|error| format!("write packet: {error}"))?;
    let stderr = refusal(&repo.0, CHECK)?;
    stderr_has_all(
        &stderr,
        &[
            "target/ripr/pr/repo-exposure.json is not valid JSON: ",
            "; regenerate it with `ripr pr-evidence` (same --base, --head and --root), then rerun with --check",
        ],
    )
}

/// The advice keeps the caller's own options: following it with a
/// non-default `--pr-evidence` path writes outputs the original check accepts.
#[test]
fn impacted_evidence_check_without_outputs_names_the_command_that_writes_them() -> Result<(), String>
{
    let dir = scratch()?;
    std::fs::write(
        dir.0.join("review.json"),
        r#"{"summary":{"ripr_severe_gap":false,"requires_targeted_mutation":false}}"#,
    )
    .map_err(|error| format!("write packet: {error}"))?;
    let check = [
        "impacted-evidence",
        "--pr-evidence",
        "review.json",
        "--check",
    ];
    let stderr = refusal(&dir.0, &check)?;
    stderr_has_all(
        &stderr,
        &[
            "missing or unreadable target/xtask/impacted-evidence/latest.json: ",
            "; rerun the same `ripr impacted-evidence` command without --check to write it",
        ],
    )?;
    success(&dir.0, &check[..3])?;
    success(&dir.0, &check)
}

fn success(dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = ripr(dir, args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ripr {args:?}: expected success, got {:?}:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
