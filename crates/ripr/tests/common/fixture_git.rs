//! Hardened git helper for integration-test fixture setup (#3742, Slice 1).
//!
//! Contract mirror of `crate::testing::fixture_git`: every fixture
//! invocation runs under a generous deadline, idempotent commands are
//! retried once after a timeout, and a timed-out `commit` is reconciled
//! against HEAD instead of re-run. Integration-test targets cannot see
//! `pub(crate)` library items, so this harness-local copy carries the same
//! behavior on `std` only (spawn + `try_wait` polling + kill on deadline).
//! Keep the deadline, the retry set, and the commit-reconcile rule in sync
//! with the canonical in-crate module; only the error prefix differs
//! (`fixture_git_timeout` here, the shared `git_invocation_timeout` there).

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Per-invocation fixture deadline, mirroring the canonical helper.
pub const FIXTURE_GIT_DEADLINE: Duration = Duration::from_secs(30);

/// Fixture git commands whose re-execution cannot change fixture state.
/// `commit` is deliberately absent: a timed-out commit may still have
/// landed, so it is reconciled instead of re-run.
const RETRYABLE_FIXTURE_GIT: &[&str] = &[
    "init",
    "config",
    "add",
    "checkout",
    "update-ref",
    "rev-parse",
    "status",
];

/// Poll interval while waiting out a fixture invocation.
const FIXTURE_GIT_POLL: Duration = Duration::from_millis(10);

/// Prefix identifying a deadline expiry (never a git failure).
const FIXTURE_GIT_TIMEOUT_PREFIX: &str = "fixture_git_timeout";

/// Run one fixture `git` invocation under the shared deadline, with one
/// idempotent retry and commit reconcile. Returns `Ok(())` only when git
/// itself reports success (or a timed-out commit provably landed).
pub fn fixture_git_ok(root: &Path, args: &[&str]) -> Result<(), String> {
    // A `commit` is reconciled against the revision it started from: a
    // timed-out commit that moved HEAD landed; one that left HEAD
    // unchanged did not and must not be re-run blindly.
    let head_before = if args.first() == Some(&"commit") {
        fixture_head(root)
    } else {
        None
    };

    let first = match run_git_deadline(root, args, FIXTURE_GIT_DEADLINE) {
        Ok(FixtureGitOutcome::Finished(output)) if output.status.success() => return Ok(()),
        Ok(FixtureGitOutcome::Finished(output)) => {
            return Err(format!(
                "fixture git {args:?} failed in {}: {}",
                root.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        // The timeout is the retryable/reconcilable outcome; a spawn or
        // wait failure propagates untouched.
        Ok(FixtureGitOutcome::TimedOut(error)) => error,
        Err(error) => return Err(error),
    };

    // Reconcile a timed-out commit: HEAD moved past the pre-commit
    // revision, so the commit landed despite the deadline.
    if args.first() == Some(&"commit") {
        let head_after = fixture_head(root);
        let landed = head_after.is_some() && head_after != head_before;
        if landed {
            return Ok(());
        }
        return Err(first);
    }

    // Only provably idempotent commands are re-executed.
    if !args
        .first()
        .is_some_and(|command| RETRYABLE_FIXTURE_GIT.contains(command))
    {
        return Err(first);
    }
    match run_git_deadline(root, args, FIXTURE_GIT_DEADLINE) {
        Ok(FixtureGitOutcome::Finished(output)) if output.status.success() => Ok(()),
        Ok(FixtureGitOutcome::Finished(output)) => Err(format!(
            "fixture git {args:?} failed again in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Ok(FixtureGitOutcome::TimedOut(error)) => Err(format!("retry timed out: {error}")),
        Err(error) => Err(error),
    }
}

/// Current HEAD revision of the fixture repository, or `None` when the
/// repository has no commits (fresh `git init`).
fn fixture_head(root: &Path) -> Option<String> {
    let outcome = run_git_deadline(
        root,
        &["rev-parse", "-q", "--verify", "HEAD"],
        FIXTURE_GIT_DEADLINE,
    )
    .ok()?;
    let output = match outcome {
        FixtureGitOutcome::Finished(output) => output,
        FixtureGitOutcome::TimedOut(_) => return None,
    };
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

enum FixtureGitOutcome {
    Finished(std::process::Output),
    TimedOut(String),
}

fn run_git_deadline(
    root: &Path,
    args: &[&str],
    deadline: Duration,
) -> Result<FixtureGitOutcome, String> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn fixture git {args:?} in {}: {error}", root.display()))?;
    let started = Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|error| format!("wait fixture git {args:?} in {}: {error}", root.display()))?
        {
            Some(_) => {
                let output = child.wait_with_output().map_err(|error| {
                    format!(
                        "collect fixture git {args:?} in {}: {error}",
                        root.display()
                    )
                })?;
                return Ok(FixtureGitOutcome::Finished(output));
            }
            None if started.elapsed() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(FixtureGitOutcome::TimedOut(format!(
                    "{FIXTURE_GIT_TIMEOUT_PREFIX}: fixture git {args:?} exceeded {deadline:?} in {}",
                    root.display()
                )));
            }
            None => std::thread::sleep(FIXTURE_GIT_POLL),
        }
    }
}
