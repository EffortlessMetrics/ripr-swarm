//! Argument values that were misparsed or accepted unvalidated (#15259).

use super::{ignore_remove_dir_all, probe_command, unique_temp_workspace};
use std::path::Path;
use std::process::Output;

fn run(root: &Path, args: &[&str]) -> Result<Output, String> {
    probe_command(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("run ripr: {error}"))
}

fn refused(root: &Path, args: &[&str], fragments: &[&str]) -> Result<(), String> {
    let output = run(root, args)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() != Some(2) {
        return Err(format!("{args:?}: expected exit 2; {output:?}"));
    }
    for fragment in fragments {
        if !stderr.contains(fragment) {
            return Err(format!("{args:?}: missing {fragment:?} in {stderr}"));
        }
    }
    Ok(())
}

fn workspace(name: &str) -> Result<std::path::PathBuf, String> {
    let root = unique_temp_workspace(name);
    std::fs::create_dir_all(&root).map_err(|error| format!("create fixture: {error}"))?;
    Ok(root)
}

/// A `--json` request gets the typed `cli_refusal` document on stdout:
/// `(code, token, next command)`.
fn refused_json(root: &Path, args: &[&str], expected: (&str, &str, &str)) -> Result<(), String> {
    let output = run(root, args)?;
    if output.status.code() != Some(2) {
        return Err(format!("{args:?}: expected exit 2; {output:?}"));
    }
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("{args:?}: stdout must be one JSON document: {err}"))?;
    let (code, token, next) = expected;
    let actual = (
        document["status"].as_str(),
        document["error"]["code"].as_str(),
        document["error"]["token"].as_str(),
        document["error"]["next"]["command"].as_str(),
    );
    if actual != (Some("refused"), Some(code), Some(token), Some(next)) {
        return Err(format!("{args:?}: unexpected refusal document {document}"));
    }
    Ok(())
}

#[test]
fn init_value_flags_followed_by_a_flag_report_a_missing_value() -> Result<(), String> {
    let root = workspace("argval-init")?;
    for flag in ["--ci", "--agents"] {
        refused(
            &root,
            &["init", flag, "--root", "."],
            &[&format!("missing value for {flag}")],
        )?;
    }
    refused(
        &root,
        &["init", "--root", "--ci", "github"],
        &["missing value for --root"],
    )?;
    refused(
        &root,
        &["init", "--ci", "github", "--ci", "gitlab"],
        &[
            "init --ci takes one provider per run",
            "--ci github or --ci gitlab",
        ],
    )?;
    refused_json(
        &root,
        &["init", "--ci", "github", "--ci", "gitlab", "--json"],
        ("conflicting_flags", "--ci", "ripr init --help"),
    )?;
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn init_refuses_a_directory_at_ripr_toml_without_suggesting_force() -> Result<(), String> {
    let root = workspace("argval-init-dir")?;
    std::fs::create_dir_all(root.join("ripr.toml")).map_err(|error| format!("mkdir: {error}"))?;
    for args in [&["init"][..], &["init", "--force"][..]] {
        let output = run(&root, args)?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(stderr.contains("is a directory, not a file"), "{stderr}");
        assert!(!stderr.contains("rerun `ripr init --force"), "{stderr}");
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn pr_comments_plan_validates_pull_request_and_conflicting_toggles() -> Result<(), String> {
    let root = workspace("argval-prc")?;
    refused(
        &root,
        &["pr-comments", "plan", "--pull-request", "abc"],
        &["--pull-request requires a positive integer", "\"abc\""],
    )?;
    refused(
        &root,
        &["pr-comments", "plan", "--pull-request", "0"],
        &["--pull-request requires a positive integer", "\"0\""],
    )?;
    refused(
        &root,
        &["pr-comments", "plan", "--token-available", "--no-token"],
        &["either --token-available or --no-token"],
    )?;
    refused(
        &root,
        &[
            "pr-comments",
            "plan",
            "--write-permission",
            "--no-write-permission",
        ],
        &["either --write-permission or --no-write-permission"],
    )?;
    for (first, second) in [
        ("--token-available", "--no-token"),
        ("--no-token", "--token-available"),
        ("--write-permission", "--no-write-permission"),
        ("--no-write-permission", "--write-permission"),
    ] {
        refused_json(
            &root,
            &["pr-comments", "plan", first, second, "--json"],
            ("conflicting_flags", second, "ripr pr-comments plan --help"),
        )?;
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn policy_history_pr_number_must_be_a_positive_integer() -> Result<(), String> {
    let root = workspace("argval-policy")?;
    for value in ["abc", "0", "-1", "99999999999999999999"] {
        refused(
            &root,
            &["policy", "history", "--pr-number", value],
            &[
                "--pr-number requires a positive integer",
                &format!("{value:?}"),
            ],
        )?;
    }
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn completions_suggest_a_shell_only_when_genuinely_close() -> Result<(), String> {
    let root = workspace("argval-completions")?;
    refused(
        &root,
        &["completions", "pwsh"],
        &["Did you mean `powershell`?"],
    )?;
    refused(&root, &["completions", "bsh"], &["Did you mean `bash`?"])?;
    let output = run(&root, &["completions", "sh"])?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(!stderr.contains("Did you mean"), "{stderr}");
    refused(
        &root,
        &["completions", "--shell", "bash"],
        &["positional argument", "ripr completions bash"],
    )?;
    refused(
        &root,
        &["completions", "--shell=bash"],
        &["positional argument", "ripr completions bash"],
    )?;
    // A JSON request still gets the typed refusal document.
    refused_json(
        &root,
        &["completions", "--shell", "bash", "--json"],
        ("unknown_argument", "--shell", "ripr completions --help"),
    )?;
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn agent_stub_at_with_two_line_suffixes_names_the_expected_form() -> Result<(), String> {
    let root = workspace("argval-stub")?;
    refused(
        &root,
        &["agent", "stub", "--at", "src/lib.rs:1:2"],
        &["--at expects FILE:LINE (for example src/lib.rs:12)"],
    )?;
    ignore_remove_dir_all(&root);
    Ok(())
}
