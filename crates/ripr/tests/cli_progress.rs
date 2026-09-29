//! Built-binary stdout/stderr discriminator for CLI analysis progress (#4810).

use std::path::PathBuf;
use std::process::{Command, Output};

fn ripr() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sample_diff() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/sample/example.diff")
}

fn sample_root() -> String {
    workspace_root().display().to_string()
}

fn sample_diff_arg() -> String {
    sample_diff().display().to_string()
}

fn run_check(extra: &[&str]) -> Result<Output, String> {
    let root = sample_root();
    let diff = sample_diff_arg();
    let mut args = vec!["check", "--root", root.as_str(), "--diff", diff.as_str()];
    args.extend_from_slice(extra);
    ripr()
        .args(&args)
        .output()
        .map_err(|error| format!("run ripr {args:?}: {error}"))
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn check_json_stdout_parses_while_progress_stays_on_stderr() -> Result<(), String> {
    let output = run_check(&["--format", "json"])?;
    assert!(
        output.status.success(),
        "check json failed: {}",
        stderr_text(&output)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("stdout is not JSON: {error}"))?;
    assert_eq!(parsed["schema_version"], "0.2");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = stderr_text(&output);
    assert!(
        !stdout.contains("ripr progress:"),
        "progress leaked onto stdout"
    );
    assert!(
        stderr.contains("ripr progress: loading_input [diff]"),
        "missing loading_input on stderr: {stderr}"
    );
    assert!(
        stderr.contains("ripr progress: analyzing [diff]"),
        "missing analyzing on stderr: {stderr}"
    );
    assert!(
        stderr.contains("ripr progress: completed [diff]"),
        "missing completed on stderr: {stderr}"
    );
    assert!(
        !stderr.contains('\u{1b}'),
        "non-TTY stderr has ANSI: {stderr}"
    );
    assert!(!stderr.contains('\r'), "non-TTY stderr has CR: {stderr}");
    assert!(
        !stderr.contains('%'),
        "unknown totals leaked a percent: {stderr}"
    );
    assert!(
        !stderr.to_ascii_lowercase().contains("eta"),
        "unknown totals leaked an ETA: {stderr}"
    );
    Ok(())
}

#[test]
fn check_quiet_keeps_json_stdout_byte_identical_and_drops_progress() -> Result<(), String> {
    let loud = run_check(&["--format", "json"])?;
    let quiet = run_check(&["--format", "json", "--quiet"])?;
    assert!(loud.status.success(), "{}", stderr_text(&loud));
    assert!(quiet.status.success(), "{}", stderr_text(&quiet));
    assert_eq!(
        loud.stdout, quiet.stdout,
        "quiet must not change machine stdout"
    );
    let quiet_err = stderr_text(&quiet);
    assert!(
        !quiet_err.contains("ripr progress:"),
        "--quiet must suppress progress: {quiet_err}"
    );
    assert!(
        stderr_text(&loud).contains("ripr progress:"),
        "control run must have emitted progress"
    );
    Ok(())
}

#[test]
fn check_sarif_stdout_is_unchanged_by_progress() -> Result<(), String> {
    let loud = run_check(&["--format", "sarif"])?;
    let quiet = run_check(&["--format", "sarif", "--quiet"])?;
    assert!(loud.status.success(), "{}", stderr_text(&loud));
    assert!(quiet.status.success(), "{}", stderr_text(&quiet));
    assert_eq!(loud.stdout, quiet.stdout);
    let stdout = String::from_utf8_lossy(&loud.stdout);
    assert!(stdout.contains("\"version\": \"2.1.0\"") || stdout.contains("sarif"));
    assert!(!stdout.contains("ripr progress:"));
    assert!(stderr_text(&loud).contains("ripr progress:"));
    Ok(())
}

#[test]
fn check_progress_failure_emits_failed_not_completed() -> Result<(), String> {
    let root = sample_root();
    let missing = workspace_root().join("target/ripr/absent-progress.diff");
    let missing_arg = missing.display().to_string();
    let output = ripr()
        .args([
            "check",
            "--root",
            root.as_str(),
            "--diff",
            missing_arg.as_str(),
            "--format",
            "json",
        ])
        .output()
        .map_err(|error| format!("run failing check: {error}"))?;
    assert!(
        !output.status.success(),
        "missing diff must fail the command"
    );
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("ripr progress: failed [diff]"),
        "failure must project failed: {stderr}"
    );
    assert!(
        !stderr.contains("ripr progress: completed"),
        "failure must not project completed: {stderr}"
    );
    Ok(())
}

#[test]
fn check_help_does_not_spray_progress() -> Result<(), String> {
    let output = ripr()
        .args(["check", "--help"])
        .output()
        .map_err(|error| format!("run check help: {error}"))?;
    assert!(output.status.success());
    let stderr = stderr_text(&output);
    assert!(
        !stderr.contains("ripr progress:"),
        "help is a short command: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--quiet"));
    assert!(stdout.contains("percentage or ETA"));
    Ok(())
}

#[test]
fn check_github_stdout_is_unchanged_by_progress() -> Result<(), String> {
    let loud = run_check(&["--format", "github"])?;
    let quiet = run_check(&["--format", "github", "--quiet"])?;
    assert!(loud.status.success(), "{}", stderr_text(&loud));
    assert!(quiet.status.success(), "{}", stderr_text(&quiet));
    assert_eq!(loud.stdout, quiet.stdout);
    let stdout = String::from_utf8_lossy(&loud.stdout);
    assert!(!stdout.contains("ripr progress:"));
    assert!(stderr_text(&loud).contains("ripr progress: loading_input [diff]"));
    Ok(())
}

#[test]
fn check_worktree_projects_worktree_scope_on_stderr() -> Result<(), String> {
    let root = sample_root();
    let output = ripr()
        .args([
            "check",
            "--root",
            root.as_str(),
            "--worktree",
            "--format",
            "json",
        ])
        .output()
        .map_err(|error| format!("run worktree check: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = stderr_text(&output);
    assert!(!stdout.contains("ripr progress:"));
    assert!(
        stderr.contains("ripr progress: loading_input [worktree]"),
        "missing worktree scope on stderr: {stderr}"
    );
    assert!(
        !stderr.contains("[diff]"),
        "worktree run must not project the diff scope: {stderr}"
    );
    assert!(
        !stderr.contains("ripr progress: completed [diff]"),
        "worktree failure must not leak a diff completed token: {stderr}"
    );
    Ok(())
}

#[test]
fn check_quiet_failure_keeps_errors_and_drops_progress() -> Result<(), String> {
    let root = sample_root();
    let missing = workspace_root().join("target/ripr/absent-progress.diff");
    let missing_arg = missing.display().to_string();
    let output = ripr()
        .args([
            "check",
            "--root",
            root.as_str(),
            "--diff",
            missing_arg.as_str(),
            "--format",
            "json",
            "--quiet",
        ])
        .output()
        .map_err(|error| format!("run quiet failing check: {error}"))?;
    assert!(
        !output.status.success(),
        "missing diff must fail under --quiet"
    );
    let stderr = stderr_text(&output);
    assert!(
        !stderr.contains("ripr progress:"),
        "--quiet must suppress progress on failure: {stderr}"
    );
    assert!(
        !stderr.trim().is_empty(),
        "--quiet must still report the command error"
    );
    Ok(())
}
