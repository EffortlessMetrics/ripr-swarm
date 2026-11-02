//! #11879: a first `ripr check` in a project whose git ignore rules do not
//! cover ripr's artifact root says so once on stderr, names the exact
//! `.gitignore` entry, and changes nothing on stdout. `--quiet` and a covering
//! entry silence it. This drives the real binary, so the `check` command's own
//! contract is exercised, not only the shared doctor verdict.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("git {args:?} failed"))
}

/// A Python project with one committed `>` to `>=` boundary edit on a feature
/// branch, in a repository whose global excludes are empty so the result does
/// not depend on the machine running the test.
fn project(label: &str) -> Result<Scratch, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-first-check-hint-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let scratch = Scratch(root.clone());
    let excludes = root.join("empty-excludes");
    std::fs::write(&excludes, "").map_err(|error| error.to_string())?;
    git(&root, &["init", "-q", "-b", "main"])?;
    git(&root, &["config", "user.email", "ripr@example.invalid"])?;
    git(&root, &["config", "user.name", "ripr tests"])?;
    git(
        &root,
        &[
            "config",
            "core.excludesFile",
            excludes.to_str().ok_or("non-utf8 excludes path")?,
        ],
    )?;
    std::fs::write(root.join("m.py"), "def f(a, t):\n    return a > t\n")
        .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("test_m.py"),
        "from m import f\n\n\ndef test_f():\n    assert f(1, 2) is False\n",
    )
    .map_err(|error| error.to_string())?;
    git(&root, &["add", "m.py", "test_m.py"])?;
    git(&root, &["commit", "-q", "-m", "base"])?;
    git(&root, &["checkout", "-q", "-b", "feat"])?;
    std::fs::write(root.join("m.py"), "def f(a, t):\n    return a >= t\n")
        .map_err(|error| error.to_string())?;
    git(&root, &["commit", "-q", "-am", "change"])?;
    Ok(scratch)
}

fn check(root: &Path, extra: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--base", "main", "--root"])
        .arg(root)
        .args(extra)
        .output()
        .map_err(|error| format!("spawn ripr check: {error}"))
}

const ENTRY: &str = "Add this line to .gitignore: /target/ripr/";

#[test]
fn unignored_root_gets_one_stderr_hint_and_stdout_is_unchanged() -> Result<(), String> {
    let scratch = project("hint")?;
    let out = check(&scratch.0, &[])?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stderr.matches(ENTRY).count(), 1, "{stderr}");
    assert!(!stdout.contains(".gitignore"), "{stdout}");
    let json = check(&scratch.0, &["--format", "json"])?;
    let json_stdout = String::from_utf8_lossy(&json.stdout);
    assert!(!json_stdout.contains(".gitignore"), "{json_stdout}");
    Ok(())
}

#[test]
fn quiet_and_a_covering_entry_silence_the_hint() -> Result<(), String> {
    let scratch = project("silent")?;
    let quiet = check(&scratch.0, &["--quiet"])?;
    assert!(
        !String::from_utf8_lossy(&quiet.stderr).contains(ENTRY),
        "--quiet must silence the hint"
    );
    std::fs::write(scratch.0.join(".gitignore"), "/target/ripr/\n")
        .map_err(|error| error.to_string())?;
    let covered = check(&scratch.0, &[])?;
    assert!(
        !String::from_utf8_lossy(&covered.stderr).contains(ENTRY),
        "a covering entry must silence the hint"
    );
    Ok(())
}
