//! #15338: a repository whose default branch is neither `main` nor `master`
//! and whose `origin/HEAD` was never set cannot run a bare `ripr check`, so
//! `ripr init` does not promise a default-branch comparison there, and a bad
//! explicit `--base` is not answered with the same flag again.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SET_HEAD_AUTO: &str = "git remote set-head origin --auto";

fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|err| format!("spawn git {args:?}: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn temp_dir(label: &str) -> Result<PathBuf, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ripr-default-base-advice-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    Ok(dir)
}

/// A work clone on a `work` branch whose remote `origin` has `default_branch`
/// pushed and `origin/HEAD` unset (what `git remote add` plus `git push`
/// leaves). With `with_origin` false the repository has no remote at all.
fn repo(
    label: &str,
    default_branch: &str,
    with_origin: bool,
) -> Result<(PathBuf, PathBuf), String> {
    let remote = temp_dir(&format!("{label}-origin"))?;
    let work = temp_dir(label)?;
    std::fs::create_dir_all(work.join("src")).map_err(|err| err.to_string())?;
    git(&remote, &["init", "-q", "--bare", "-b", default_branch])?;
    git(&work, &["init", "-q", "-b", default_branch])?;
    git(&work, &["config", "user.email", "t@example.com"])?;
    git(&work, &["config", "user.name", "t"])?;
    git(&work, &["config", "commit.gpgSign", "false"])?;
    std::fs::write(
        work.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|err| err.to_string())?;
    std::fs::write(work.join(".gitignore"), "/target/\n").map_err(|err| err.to_string())?;
    std::fs::write(
        work.join("src/lib.rs"),
        "pub fn f(x: u32) -> bool {\n    x >= 10\n}\n",
    )
    .map_err(|err| err.to_string())?;
    git(&work, &["add", "-A"])?;
    git(&work, &["commit", "-q", "-m", "base"])?;
    if with_origin {
        let url = remote.to_string_lossy().into_owned();
        git(&work, &["remote", "add", "origin", &url])?;
        // `git push` updates the `origin/<branch>` tracking ref but never
        // `origin/HEAD`. No `git fetch` here: since Git 2.48 a fetch creates
        // `origin/HEAD` by default (`remote.origin.followRemoteHEAD`).
        git(&work, &["push", "-q", "origin", default_branch])?;
        // Belt and braces for any Git that sets it anyway: drop the symref
        // itself (`--no-deref`), never the branch it points at.
        git(
            &work,
            &["update-ref", "-d", "--no-deref", "refs/remotes/origin/HEAD"],
        )?;
        // Fixture precondition: the tracking ref exists, `origin/HEAD` does not.
        let tracking = format!("refs/remotes/origin/{default_branch}");
        git(&work, &["rev-parse", "-q", "--verify", &tracking])?;
        if git(
            &work,
            &["rev-parse", "-q", "--verify", "refs/remotes/origin/HEAD"],
        )
        .is_ok()
        {
            return Err("fixture: origin/HEAD must be unset".to_string());
        }
    }
    git(&work, &["checkout", "-q", "-b", "work"])?;
    std::fs::write(
        work.join("src/lib.rs"),
        "pub fn f(x: u32) -> bool {\n    x > 10\n}\n",
    )
    .map_err(|err| err.to_string())?;
    git(&work, &["commit", "-q", "-am", "change"])?;
    Ok((work, remote))
}

fn ripr(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|err| format!("spawn ripr {args:?}: {err}"))
}

fn cleanup(paths: &[&Path]) {
    for path in paths {
        let _ = std::fs::remove_dir_all(path);
    }
}

#[test]
fn init_does_not_promise_a_default_branch_check_that_will_refuse() -> Result<(), String> {
    let (work, remote) = repo("init", "trunk", true)?;
    let output = ripr(&work, &["init"])?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    cleanup(&[&work, &remote]);
    if stdout.contains("against its default branch") || !stdout.contains(SET_HEAD_AUTO) {
        return Err(format!(
            "init closing line must not promise the refused check:\n{stdout}"
        ));
    }
    Ok(())
}

#[test]
fn init_keeps_the_default_branch_promise_when_a_base_resolves() -> Result<(), String> {
    let (work, remote) = repo("init-main", "main", true)?;
    let output = ripr(&work, &["init"])?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    cleanup(&[&work, &remote]);
    if !stdout.contains("against its default branch") {
        return Err(format!(
            "control: a resolvable base keeps the promise:\n{stdout}"
        ));
    }
    Ok(())
}

#[test]
fn a_bad_explicit_base_is_not_answered_with_the_same_flag() -> Result<(), String> {
    let (work, remote) = repo("bad-base", "main", true)?;
    let output = ripr(&work, &["check", "--base", "nope-ref"])?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    cleanup(&[&work, &remote]);
    if !stderr.contains("does not resolve to a commit") || stderr.contains("pass `--base <ref>`") {
        return Err(format!("circular --base advice:\n{stderr}"));
    }
    Ok(())
}
