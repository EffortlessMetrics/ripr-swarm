//! Hardened git helper for unit-test fixture setup (#3742, Slice 1).
//!
//! Fixture repositories are built with short-lived `git` child processes.
//! Host slowness (Defender scans of fresh `.git` directories on Windows,
//! parallel-build CPU starvation) routinely pushes a single `git add` or
//! `commit` past short windows, so every fixture invocation runs under a
//! generous deadline, idempotent commands are retried once after a timeout,
//! and a timed-out `commit` is reconciled against HEAD instead of re-run
//! (a re-run would fail with "nothing to commit" and mask the real outcome).
//! Extracted verbatim from the `edit_cage` test helper born in the #3597
//! review; the retry set and reconcile rule below are the contract that the
//! harness-local `tests/common` mirror keeps in sync.

use std::path::Path;
use std::time::Duration;

/// Per-invocation fixture deadline. Host git slowness (Defender scans
/// of fresh `.git` directories on Windows) routinely pushes a single
/// `git add`/`commit` past shorter windows, so the deadline is generous
/// and idempotent commands are retried once on a timeout.
pub(crate) const FIXTURE_GIT_DEADLINE: Duration = Duration::from_secs(30);

/// Per-stream capture cap for one fixture invocation.
const FIXTURE_GIT_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Fixture git commands whose re-execution cannot change fixture state.
/// `commit` is deliberately absent: a timed-out commit may still have
/// landed, so it is reconciled by checking whether HEAD exists instead
/// of blindly re-running (a re-run would fail with "nothing to
/// commit"), (#3597 review).
const RETRYABLE_FIXTURE_GIT: &[&str] = &[
    "init",
    "config",
    "add",
    "checkout",
    "update-ref",
    "rev-parse",
    "status",
];

/// Run one fixture `git` invocation under the shared deadline, with one
/// idempotent retry and commit reconcile. Returns `Ok(())` only when git
/// itself reports success (or a timed-out commit provably landed).
pub(crate) fn fixture_git_ok(root: &Path, args: &[&str]) -> Result<(), String> {
    fixture_git_ok_with_deadline(root, args, FIXTURE_GIT_DEADLINE)
}

/// `fixture_git_ok` with an explicit deadline. Production fixture setup
/// always passes `FIXTURE_GIT_DEADLINE`; tests pass a zero deadline to
/// observe the timeout path without waiting out the production window.
pub(crate) fn fixture_git_ok_with_deadline(
    root: &Path,
    args: &[&str],
    deadline: Duration,
) -> Result<(), String> {
    // A `commit` is reconciled against the revision it started from: a
    // timed-out commit that moved HEAD landed; one that left HEAD
    // unchanged did not and must not be re-run blindly ("nothing to
    // commit" would mask the real failure).
    let head_before = if args.first() == Some(&"commit") {
        fixture_head(root, deadline)
    } else {
        None
    };

    let first = match crate::git::run_git_output_with_deadline_and_limit_isolated(
        root,
        args,
        deadline,
        FIXTURE_GIT_OUTPUT_LIMIT,
    ) {
        Ok(output) if output.status.success() => return Ok(()),
        Ok(output) => {
            return Err(format!(
                "isolated fixture git {args:?} failed in {}: {}",
                root.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        // The timeout is the retryable/reconcilable outcome; every other
        // runner error propagates untouched.
        Err(error) if crate::git::is_git_invocation_timeout(&error) => error,
        Err(error) => return Err(error),
    };

    // Reconcile a timed-out commit: HEAD moved past the pre-commit
    // revision, so the commit landed despite the deadline.
    if args.first() == Some(&"commit") {
        let head_after = fixture_head(root, deadline);
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
    match crate::git::run_git_output_with_deadline_and_limit_isolated(
        root,
        args,
        deadline,
        FIXTURE_GIT_OUTPUT_LIMIT,
    ) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!(
            "isolated fixture git {args:?} failed again in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) if crate::git::is_git_invocation_timeout(&error) => {
            Err(format!("retry timed out: {error}"))
        }
        Err(error) => Err(error),
    }
}

/// Current HEAD revision of the fixture repository, or `None` when the
/// repository has no commits (fresh `git init`).
fn fixture_head(root: &Path, deadline: Duration) -> Option<String> {
    let output = crate::git::run_git_output_with_deadline_and_limit_isolated(
        root,
        &["rev-parse", "-q", "--verify", "HEAD"],
        deadline,
        FIXTURE_GIT_OUTPUT_LIMIT,
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static FIXTURE_GIT_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_root(label: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("clock before UNIX_EPOCH: {err}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-fixture-git-{label}-{}-{}-{stamp}",
            std::process::id(),
            FIXTURE_GIT_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root)
            .map_err(|err| format!("create {} failed: {err}", root.display()))?;
        Ok(root)
    }

    #[test]
    fn builds_a_committed_fixture_repo() -> Result<(), String> {
        let root = unique_root("committed")?;
        fixture_git_ok(&root, &["-c", "init.templateDir=", "init", "-q"])?;
        fixture_git_ok(&root, &["config", "user.email", "ripr@example.invalid"])?;
        fixture_git_ok(&root, &["config", "user.name", "RIPR Test"])?;
        fixture_git_ok(&root, &["config", "commit.gpgSign", "false"])?;
        std::fs::write(root.join("marker.txt"), "fixture\n")
            .map_err(|err| format!("write marker failed: {err}"))?;
        fixture_git_ok(&root, &["add", "."])?;
        fixture_git_ok(&root, &["commit", "-qm", "baseline"])?;
        fixture_git_ok(&root, &["rev-parse", "--verify", "HEAD"])?;
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn reports_a_failing_git_command() -> Result<(), String> {
        let root = unique_root("failing")?;
        match fixture_git_ok(&root, &["no-such-command-xyz"]) {
            Ok(()) => Err("unknown git command unexpectedly succeeded".to_string()),
            Err(error) if error.contains("failed") => {
                let _ = std::fs::remove_dir_all(&root);
                Ok(())
            }
            Err(error) => Err(format!("unexpected error shape: {error}")),
        }
    }

    #[test]
    fn zero_deadline_retries_then_reports_a_timeout() -> Result<(), String> {
        let root = unique_root("timeout-retry")?;
        match fixture_git_ok_with_deadline(&root, &["init"], Duration::ZERO) {
            Ok(()) => Err("zero-deadline init unexpectedly succeeded".to_string()),
            Err(error) if error.contains("retry timed out") => {
                let _ = std::fs::remove_dir_all(&root);
                Ok(())
            }
            Err(error) => Err(format!("unexpected error shape: {error}")),
        }
    }

    #[test]
    fn zero_deadline_never_retries_a_non_idempotent_command() -> Result<(), String> {
        let root = unique_root("timeout-no-retry")?;
        match fixture_git_ok_with_deadline(&root, &["reset"], Duration::ZERO) {
            Ok(()) => Err("zero-deadline reset unexpectedly succeeded".to_string()),
            Err(error)
                if error.contains(crate::git::GIT_INVOCATION_TIMEOUT_PREFIX)
                    && !error.contains("retry timed out") =>
            {
                let _ = std::fs::remove_dir_all(&root);
                Ok(())
            }
            Err(error) => Err(format!("unexpected error shape: {error}")),
        }
    }

    #[test]
    fn zero_deadline_commit_reconciles_to_a_timeout_without_landing() -> Result<(), String> {
        let root = unique_root("timeout-commit")?;
        match fixture_git_ok_with_deadline(&root, &["commit", "-m", "x"], Duration::ZERO) {
            Ok(()) => Err("zero-deadline commit unexpectedly succeeded".to_string()),
            Err(error)
                if error.contains(crate::git::GIT_INVOCATION_TIMEOUT_PREFIX)
                    && !error.contains("retry timed out") =>
            {
                let _ = std::fs::remove_dir_all(&root);
                Ok(())
            }
            Err(error) => Err(format!("unexpected error shape: {error}")),
        }
    }
}
