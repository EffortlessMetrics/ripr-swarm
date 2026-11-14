//! #11879: a first `ripr check` in a project whose git ignore rules do not
//! cover ripr's artifact root says so once on stderr, names the exact
//! `.gitignore` entry, and changes nothing on stdout. `--quiet` and a covering
//! entry silence it. The default `target/ripr` tree ignores itself, so in
//! practice the hint is left for a tree ripr cannot mark (a user-owned ignore
//! file there) and for relocated cache bases. This drives the real binary, so the `check` command's own
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

fn git_stdout(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
    if !out.status.success() {
        return Err(format!("git {args:?} failed"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

const SELF_IGNORE: &str =
    "# Created by ripr: everything under target/ripr is ripr's own disposable output.\n*\n";

/// Zero setup: a Python project that does not ignore Cargo's `target/` gets
/// no `.gitignore` chore from its first `check` or `pilot`. ripr's own output tree
/// ignores itself, so `git status` is exactly as clean afterwards as before,
/// and nothing on stdout changes.
#[test]
fn first_check_leaves_git_status_clean_without_a_gitignore_chore() -> Result<(), String> {
    let scratch = project("hint")?;
    // Fixture: the project really is unignored and clean before ripr runs.
    assert!(!scratch.0.join(".gitignore").exists());
    assert!(!scratch.0.join("target").exists());
    let status_before = git_stdout(&scratch.0, &["status", "--porcelain"])?;
    assert!(!status_before.contains("target"), "{status_before}");
    let out = check(&scratch.0, &[])?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stderr.contains(".gitignore"), "{stderr}");
    assert!(!stdout.contains(".gitignore"), "{stdout}");
    assert_eq!(
        git_stdout(&scratch.0, &["status", "--porcelain"])?,
        status_before
    );
    // `pilot` writes its packet under target/ripr/pilot; the first write
    // into the guarded tree gives it the self-ignore file.
    let pilot = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["pilot", "--root"])
        .arg(&scratch.0)
        .output()
        .map_err(|error| format!("spawn ripr pilot: {error}"))?;
    let pilot_stderr = String::from_utf8_lossy(&pilot.stderr);
    assert!(
        scratch
            .0
            .join("target/ripr/pilot/pilot-summary.json")
            .is_file(),
        "{pilot_stderr}"
    );
    assert!(!pilot_stderr.contains(".gitignore"), "{pilot_stderr}");
    assert_eq!(
        std::fs::read_to_string(scratch.0.join("target/ripr/.gitignore"))
            .map_err(|error| error.to_string())?,
        SELF_IGNORE
    );
    assert_eq!(
        git_stdout(&scratch.0, &["status", "--porcelain"])?,
        status_before
    );
    let json = check(&scratch.0, &["--format", "json"])?;
    let json_stdout = String::from_utf8_lossy(&json.stdout);
    assert!(!json_stdout.contains(".gitignore"), "{json_stdout}");
    // The repository's own `.gitignore` is never created or edited.
    assert!(!scratch.0.join(".gitignore").exists());
    Ok(())
}

/// A `target/ripr` tree an older ripr left unignored is adopted on the next
/// `check`: it gains the self-ignore file before the verdict, so the run
/// neither prints the hint nor leaves `?? target/` behind.
#[test]
fn existing_unignored_tree_is_adopted_before_the_verdict() -> Result<(), String> {
    let scratch = project("adopt")?;
    std::fs::create_dir_all(scratch.0.join("target/ripr/cache"))
        .map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("target/ripr/legend-seen"), "1\n")
        .map_err(|error| error.to_string())?;
    // Fixture: git really reports the old tree as untracked content.
    let status_before = git_stdout(&scratch.0, &["status", "--porcelain"])?;
    assert!(status_before.contains("?? target/\n"), "{status_before}");
    let out = check(&scratch.0, &[])?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains(ENTRY), "{stderr}");
    assert_eq!(
        git_stdout(&scratch.0, &["status", "--porcelain"])?,
        status_before.replace("?? target/\n", "")
    );
    Ok(())
}

/// A `target/ripr/.gitignore` the user wrote is theirs: ripr never rewrites
/// it, and when it does not cover ripr's output the hint still names the
/// exact entry, once, on stderr only.
#[test]
fn a_user_owned_ignore_file_is_kept_and_an_uncovered_tree_still_gets_the_hint() -> Result<(), String>
{
    let scratch = project("user-owned")?;
    std::fs::create_dir_all(scratch.0.join("target/ripr")).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("target/ripr/.gitignore"), "# mine\n")
        .map_err(|error| error.to_string())?;
    let out = check(&scratch.0, &[])?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stderr.matches(ENTRY).count(), 1, "{stderr}");
    assert!(!stdout.contains(".gitignore"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(scratch.0.join("target/ripr/.gitignore"))
            .map_err(|error| error.to_string())?,
        "# mine\n"
    );
    Ok(())
}

/// A user-owned `target/ripr/.gitignore` that ignores most of the tree but
/// unignores `reports/` still leaves reports addable, so the hint must keep
/// firing: only ripr's own exact `*` file counts as covering the tree.
#[test]
fn a_user_negation_rule_keeps_the_hint() -> Result<(), String> {
    let scratch = project("negation")?;
    std::fs::create_dir_all(scratch.0.join("target/ripr/reports"))
        .map_err(|error| error.to_string())?;
    std::fs::write(
        scratch.0.join("target/ripr/.gitignore"),
        "*\n!reports/\n!reports/**\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("target/ripr/reports/result.json"), "{}\n")
        .map_err(|error| error.to_string())?;
    // Fixture: git really would stage the report.
    let status = git_stdout(
        &scratch.0,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    assert!(
        status.contains("target/ripr/reports/result.json"),
        "{status}"
    );
    let out = check(&scratch.0, &[])?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stderr.matches(ENTRY).count(), 1, "{stderr}");
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

/// A Cargo project with ripr's cache committed into the index — the exact
/// `git add -A` trap of #12432: the ignore advice alone cannot cure it, and
/// the tracked-cache advisory repeats once per index build. One boundary
/// edit on a feature branch drives the multi-build diff flow.
fn cargo_project_with_committed_cache(label: &str) -> Result<Scratch, String> {
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
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"witness\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn clamp(a: u32, hi: u32) -> u32 {\n    if a > hi { hi } else { a }\n}\n",
    )
    .map_err(|error| error.to_string())?;
    // The trap: cache contents committed into the index by a routine
    // `git add -A` (force-add is the deliberate author state; a later
    // `.gitignore` cannot untrack what the index already holds). The entry
    // sits in a registered cache layer so the advisory's exact untrack
    // command owns every indexed path it names.
    std::fs::create_dir_all(root.join("target/ripr/cache/repo-file-facts/v1"))
        .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("target/ripr/cache/repo-file-facts/v1/committed-entry.json"),
        "{}",
    )
    .map_err(|error| error.to_string())?;
    git(&root, &["add", "Cargo.toml", "src/lib.rs"])?;
    git(&root, &["add", "-f", "--", "target"])?;
    git(&root, &["commit", "-q", "-m", "base"])?;
    git(&root, &["checkout", "-q", "-b", "feat"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn clamp(a: u32, hi: u32) -> u32 {\n    if a >= hi { hi } else { a }\n}\n",
    )
    .map_err(|error| error.to_string())?;
    git(&root, &["commit", "-q", "-am", "change"])?;
    Ok(scratch)
}

/// #12432 smoke: the committed-cache trap gets the ignore note WITH the
/// exact untrack command, and the tracked-cache advisory reports once per
/// run — not once per index build. stdout stays free of both, and every run
/// is a fresh `ripr` process, so a per-run guard is what keeps the count at
/// one across repeated runs.
#[test]
fn committed_cache_trap_names_the_untrack_cure_and_advises_once_per_run() -> Result<(), String> {
    let scratch = cargo_project_with_committed_cache("cargo-trap")?;
    for run in 1..=2 {
        let out = check(&scratch.0, &[])?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stderr.contains("Add this line to .gitignore: /target/"),
            "run {run} must keep the entry advice: {stderr}"
        );
        assert!(
            stderr.contains("--literal-pathspecs rm --cached -r -- target"),
            "run {run} must name the untrack cure: {stderr}"
        );
        let advisories = stderr.matches("repo file fact cache entr").count();
        assert_eq!(
            advisories, 1,
            "run {run} must advise once per run, got {advisories}: {stderr}"
        );
        assert!(
            stderr.contains("exact untrack command: git -C"),
            "run {run} advisory must carry the bound cure: {stderr}"
        );
        assert!(!stdout.contains(".gitignore"), "run {run}: {stdout}");
        assert!(
            !stdout.contains("rm --cached"),
            "stdout must stay free of the cure, run {run}: {stdout}"
        );
        assert!(
            !stdout.contains("repo file fact cache entr"),
            "stdout must stay free of the advisory, run {run}: {stdout}"
        );
    }
    Ok(())
}

/// A `RIPR_CACHE_DIR` base relocated inside the repository is an artifact
/// root ripr's writers use, so an uncovered relocated base is named, and
/// covering it silences the hint even though the default root is covered.
#[test]
fn relocated_cache_base_is_probed_and_named() -> Result<(), String> {
    let scratch = project("relocated")?;
    std::fs::write(scratch.0.join(".gitignore"), "/target/ripr/\n")
        .map_err(|error| error.to_string())?;
    let cache = scratch.0.join(".ripr-cache");
    let run = |scratch: &Scratch| -> Result<String, String> {
        let out = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--base", "main", "--root"])
            .arg(&scratch.0)
            .env("RIPR_CACHE_DIR", &cache)
            .output()
            .map_err(|error| format!("spawn ripr check: {error}"))?;
        Ok(String::from_utf8_lossy(&out.stderr).into_owned())
    };
    let stderr = run(&scratch)?;
    assert!(
        stderr.contains("Add this line to .gitignore: /.ripr-cache/"),
        "{stderr}"
    );
    assert!(!stderr.contains("/target/ripr/"), "{stderr}");
    std::fs::write(
        scratch.0.join(".gitignore"),
        "/target/ripr/\n/.ripr-cache/\n",
    )
    .map_err(|error| error.to_string())?;
    let covered = run(&scratch)?;
    assert!(
        !covered.contains("Add this line to .gitignore"),
        "{covered}"
    );
    Ok(())
}
