//! Behavioural controls for the `ripr-check` composite Action (#11659).
//!
//! The Action's `run:` blocks are extracted from the YAML and executed under
//! bash with stand-in `curl`, `ripr` and `git` remotes, so install routing and
//! base derivation are exercised without the network or a real release.
#![cfg(unix)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const ACTION: &str = include_str!("../../../.github/actions/ripr-check/action.yml");

/// The body of a step's `run: |` block (de-indented), found by step name.
fn run_block(step: &str) -> TestResult<String> {
    let lines: Vec<&str> = ACTION.lines().collect();
    let header = format!("- name: {step}");
    let start = lines
        .iter()
        .position(|line| line.trim() == header)
        .ok_or_else(|| format!("missing step {step}"))?;
    let run = lines[start..]
        .iter()
        .position(|line| line.trim() == "run: |")
        .ok_or_else(|| format!("step {step} has no run block"))?
        + start;
    let indent = lines[run + 1].len() - lines[run + 1].trim_start().len();
    let mut body = Vec::new();
    for line in &lines[run + 1..] {
        if line.trim().is_empty() {
            body.push("");
        } else if line.len() - line.trim_start().len() >= indent {
            body.push(&line[indent..]);
        } else {
            break;
        }
    }
    Ok(body.join("\n"))
}

fn scratch(label: &str) -> TestResult<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "ripr-check-action-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn stand_in(dir: &Path, name: &str, script: &str) -> TestResult<()> {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Passes the host tools the blocks need through, but never `cargo`.
fn tool_dir(root: &Path) -> TestResult<PathBuf> {
    let bin = root.join("bin");
    fs::create_dir_all(&bin)?;
    for tool in ["mkdir", "awk", "tar", "git", "sha256sum", "shasum"] {
        let real = ["/usr/bin", "/bin", "/usr/local/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(tool))
            .find(|path| path.exists());
        if let Some(real) = real {
            stand_in(&bin, tool, &format!("exec {} \"$@\"", real.display()))?;
        }
    }
    Ok(bin)
}

struct Run {
    output: Output,
}

impl Run {
    fn ok(&self) -> bool {
        self.output.status.success()
    }
    fn text(&self) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr)
        )
    }
}

fn bash(script: &str, dir: &Path, path: &Path, env: &[(&str, &str)]) -> TestResult<Run> {
    let mut command = Command::new("/bin/bash");
    command
        .args(["-e", "-c", script])
        .current_dir(dir)
        .env_clear()
        .env("PATH", path)
        .env("HOME", dir)
        .env("RUNNER_TEMP", dir.join("runner-temp"))
        .env("GITHUB_PATH", dir.join("github-path"));
    for (key, value) in env {
        command.env(key, value);
    }
    fs::create_dir_all(dir.join("runner-temp"))?;
    Ok(Run {
        output: command.output()?,
    })
}

fn install(version: &str, os: &str, arch: &str, root: &Path, bin: &Path) -> TestResult<Run> {
    bash(
        &run_block("Install ripr")?,
        root,
        bin,
        &[
            ("RIPR_INPUT_VERSION", version),
            ("RUNNER_OS", os),
            ("RUNNER_ARCH", arch),
        ],
    )
}

#[test]
fn malformed_versions_are_refused_before_any_download() -> TestResult {
    let root = scratch("version")?;
    let bin = tool_dir(&root)?;
    stand_in(
        &bin,
        "curl",
        &format!("echo called >> {}", root.join("curl.log").display()),
    )?;
    for bad in ["0.10.0; rm -rf /", "0.10", "latest", "", "1.2.3.4", "$(id)"] {
        let run = install(bad, "Linux", "X64", &root, &bin)?;
        assert!(!run.ok(), "{bad:?} must be refused: {}", run.text());
        assert!(
            run.text().contains("version must look like"),
            "{}",
            run.text()
        );
    }
    assert!(
        !root.join("curl.log").exists(),
        "a bad version reached curl"
    );
    Ok(())
}

#[test]
fn a_matching_ripr_on_path_skips_the_download() -> TestResult {
    let root = scratch("on-path")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo 'ripr 9.9.9 (abc)'")?;
    stand_in(
        &bin,
        "curl",
        &format!("echo called >> {}", root.join("curl.log").display()),
    )?;
    let run = install("9.9.9", "Linux", "X64", &root, &bin)?;
    assert!(run.ok(), "{}", run.text());
    assert!(run.text().contains("already on PATH"), "{}", run.text());
    assert!(!root.join("curl.log").exists());

    // A different installed version is not trusted.
    let run = install("9.9.8", "Linux", "X64", &root, &bin)?;
    assert!(root.join("curl.log").exists(), "{}", run.text());
    Ok(())
}

#[test]
fn a_checksum_mismatch_fails_before_extraction() -> TestResult {
    let root = scratch("checksum")?;
    let bin = tool_dir(&root)?;
    // Writes an archive and a .sha256 that does not match it.
    stand_in(
        &bin,
        "curl",
        "for last; do :; done; f=${last##*/}; case \"$last\" in *.sha256) echo 0000 > \"$f\";; *) echo not-a-tarball > \"$f\";; esac",
    )?;
    let run = install("0.10.0", "Linux", "X64", &root, &bin)?;
    assert!(!run.ok(), "{}", run.text());
    assert!(
        run.text().contains("does not match its published SHA-256"),
        "{}",
        run.text()
    );
    assert!(
        !root.join("github-path").exists(),
        "PATH must not be extended after a mismatch"
    );
    Ok(())
}

#[test]
fn a_runner_without_an_archive_or_cargo_says_what_to_do() -> TestResult {
    let root = scratch("no-cargo")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "curl", "exit 1")?;
    let run = install("0.10.0", "Windows", "X64", &root, &bin)?;
    assert!(!run.ok(), "{}", run.text());
    assert!(
        run.text()
            .contains("no prebuilt ripr 0.10.0 for Windows-X64"),
        "{}",
        run.text()
    );
    assert!(run.text().contains("no cargo"), "{}", run.text());
    Ok(())
}

#[test]
fn an_unreachable_archive_falls_back_to_a_pinned_cargo_install() -> TestResult {
    let root = scratch("fallback")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "curl", "exit 1")?;
    stand_in(
        &bin,
        "cargo",
        &format!("echo \"$@\" > {}", root.join("cargo.args").display()),
    )?;
    let run = install("0.10.0", "Linux", "X64", &root, &bin)?;
    assert!(run.ok(), "{}", run.text());
    assert_eq!(
        fs::read_to_string(root.join("cargo.args"))?.trim(),
        "install ripr --version 0.10.0 --locked"
    );
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> TestResult {
    let out = Command::new("git")
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(args)
        .current_dir(dir)
        .output()?;
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

/// A clone of a two-branch remote that has only the feature branch fetched,
/// like a single-branch `actions/checkout`.
fn single_branch_clone(root: &Path) -> TestResult<PathBuf> {
    let origin = root.join("origin");
    fs::create_dir_all(&origin)?;
    git(&origin, &["init", "-q", "-b", "main"])?;
    fs::write(origin.join("a.txt"), "1")?;
    git(&origin, &["add", "."])?;
    git(&origin, &["commit", "-qm", "base"])?;
    git(&origin, &["checkout", "-qb", "feature"])?;
    fs::write(origin.join("a.txt"), "2")?;
    git(&origin, &["commit", "-qam", "change"])?;
    let work = root.join("work");
    git(
        root,
        &[
            "clone",
            "-q",
            "--single-branch",
            "--branch",
            "feature",
            origin.to_str().ok_or("path")?,
            work.to_str().ok_or("path")?,
        ],
    )?;
    Ok(work)
}

fn check(work: &Path, bin: &Path, base: &str, base_ref: Option<&str>) -> TestResult<Run> {
    let mut env = vec![
        ("RIPR_INPUT_BASE", base),
        ("RIPR_INPUT_MODE", "draft"),
        ("RIPR_INPUT_FORMAT", "human"),
    ];
    if let Some(base_ref) = base_ref {
        env.push(("GITHUB_BASE_REF", base_ref));
    }
    bash(&run_block("Run ripr check")?, work, bin, &env)
}

fn recorded_argv(work: &Path) -> TestResult<String> {
    Ok(fs::read_to_string(work.join("ripr.argv"))?
        .trim()
        .to_string())
}

#[test]
fn a_pull_request_base_is_derived_and_fetched_when_the_checkout_lacks_it() -> TestResult {
    let root = scratch("base")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo \"$@\" > \"$PWD/ripr.argv\"")?;
    let work = single_branch_clone(&root)?;
    let missing = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "origin/main^{commit}"])
        .current_dir(&work)
        .status()?;
    assert!(!missing.success(), "fixture must start without origin/main");

    let run = check(&work, &bin, "", Some("main"))?;
    assert!(run.ok(), "{}", run.text());
    assert_eq!(
        recorded_argv(&work)?,
        "check --root . --mode draft --format human --base origin/main"
    );
    let fetched = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "origin/main^{commit}"])
        .current_dir(&work)
        .status()?;
    assert!(fetched.success(), "the base branch was not fetched");
    Ok(())
}

#[test]
fn outside_a_pull_request_the_default_base_is_left_to_ripr() -> TestResult {
    let root = scratch("no-base")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo \"$@\" > \"$PWD/ripr.argv\"")?;
    let work = single_branch_clone(&root)?;
    let run = check(&work, &bin, "", None)?;
    assert!(run.ok(), "{}", run.text());
    assert_eq!(
        recorded_argv(&work)?,
        "check --root . --mode draft --format human"
    );
    Ok(())
}

#[test]
fn an_explicit_base_wins_over_the_pull_request_base() -> TestResult {
    let root = scratch("explicit")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo \"$@\" > \"$PWD/ripr.argv\"")?;
    let work = single_branch_clone(&root)?;
    let run = check(&work, &bin, "origin/feature~1", Some("main"))?;
    assert!(run.ok(), "{}", run.text());
    assert_eq!(
        recorded_argv(&work)?,
        "check --root . --mode draft --format human --base origin/feature~1"
    );
    Ok(())
}

#[test]
fn an_unfetchable_base_warns_and_leaves_the_message_to_ripr() -> TestResult {
    let root = scratch("unfetchable")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo \"$@\" > \"$PWD/ripr.argv\"")?;
    let work = single_branch_clone(&root)?;
    let run = check(&work, &bin, "", Some("no-such-branch"))?;
    assert!(run.ok(), "{}", run.text());
    assert!(
        run.text()
            .contains("could not fetch the pull request base branch"),
        "{}",
        run.text()
    );
    assert!(recorded_argv(&work)?.ends_with("--base origin/no-such-branch"));
    Ok(())
}
