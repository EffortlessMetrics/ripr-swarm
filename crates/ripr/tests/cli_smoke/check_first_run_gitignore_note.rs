//! #11879: a human `check` in a non-Cargo project tells the user, on stderr and
//! until the rule exists, which `.gitignore` line covers the roots ripr keeps its
//! reports in (`target/ripr`, or a relocated `RIPR_CACHE_DIR` base).
use super::{run_command, run_command_with_env, run_git, unique_external_workspace};
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const NOTE: &str = "ripr note: ripr keeps its reports and caches under target/ripr";

/// A bare source tree with no `Cargo.toml`, so the Cargo `target/` convention
/// does not apply, inside a git repository whose ignore rules are empty.
fn project(label: &str) -> TestResult<std::path::PathBuf> {
    let project = unique_external_workspace(label)?;
    fs::create_dir_all(project.join("src"))?;
    fs::create_dir_all(project.join("tests"))?;
    fs::write(project.join("src/lib.rs"), "pub fn price() -> u32 { 2 }\n")?;
    fs::write(
        project.join("tests/price.rs"),
        "use pricing::price;\n#[test]\nfn exact_price() { assert_eq!(price(), 2); }\n",
    )?;
    fs::write(
        project.join("change.diff"),
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn price() -> u32 { 1 }\n+pub fn price() -> u32 { 2 }\n",
    )?;
    fs::write(project.join("empty-excludes"), "")?;
    run_git(&project, &["init", "-q"])?;
    run_git(&project, &["config", "core.excludesFile", "empty-excludes"])?;
    Ok(project)
}

fn stderr_of(project: &Path, extra: &[&str]) -> TestResult<String> {
    let mut args = vec!["check", "--diff", "change.diff"];
    args.extend_from_slice(extra);
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(project), &args)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stderr)?)
}

#[test]
fn first_check_names_the_gitignore_line_until_the_rule_exists() -> TestResult {
    let project = project("check-gitignore-note")?;

    let first = stderr_of(&project, &[])?;
    assert_eq!(first.matches(NOTE).count(), 1, "{first}");
    assert!(
        first.contains("add `/target/ripr/` to .gitignore"),
        "{first}"
    );

    // `human-full` is a separate gate arm and carries the same single note.
    let full = stderr_of(&project, &["--format", "human-full"])?;
    assert_eq!(full.matches(NOTE).count(), 1, "{full}");

    // Discriminating controls: machine formats and `--quiet` stay silent.
    let json = stderr_of(&project, &["--format", "json"])?;
    assert!(!json.contains(NOTE), "{json}");
    let quiet = stderr_of(&project, &["--quiet"])?;
    assert!(!quiet.contains(NOTE), "{quiet}");

    let env_quiet = run_command_with_env(
        env!("CARGO_BIN_EXE_ripr"),
        &project,
        &["check", "--diff", "change.diff"],
        &[("RIPR_QUIET", "1")],
    )?;
    let env_quiet = String::from_utf8(env_quiet.stderr)?;
    assert!(!env_quiet.contains(NOTE), "{env_quiet}");

    // Once a covering rule exists the note never repeats, and ripr did not
    // write the rule itself.
    fs::write(project.join(".gitignore"), "/target/ripr/\n")?;
    let covered = stderr_of(&project, &[])?;
    assert!(!covered.contains(NOTE), "{covered}");
    assert_eq!(
        fs::read_to_string(project.join(".gitignore"))?,
        "/target/ripr/\n"
    );
    Ok(())
}

#[test]
fn a_cargo_project_gets_no_gitignore_note() -> TestResult {
    let project = project("check-gitignore-cargo")?;
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    let stderr = stderr_of(&project, &[])?;
    assert!(!stderr.contains(NOTE), "{stderr}");
    Ok(())
}

#[test]
fn a_relocated_cache_base_is_probed_and_named() -> TestResult {
    let project = project("check-gitignore-relocated")?;
    fs::write(project.join(".gitignore"), "/target/ripr/\n")?;
    let output = run_command_with_env(
        env!("CARGO_BIN_EXE_ripr"),
        &project,
        &["check", "--diff", "change.diff"],
        &[("RIPR_CACHE_DIR", ".ripr-cache")],
    )?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(stderr.matches("ripr note:").count(), 1, "{stderr}");
    assert!(stderr.contains(".ripr-cache"), "{stderr}");
    assert!(!stderr.contains("target/ripr"), "{stderr}");

    // Covering the relocated base silences the note.
    fs::write(project.join(".gitignore"), "/target/ripr/\n/.ripr-cache/\n")?;
    let output = run_command_with_env(
        env!("CARGO_BIN_EXE_ripr"),
        &project,
        &["check", "--diff", "change.diff"],
        &[("RIPR_CACHE_DIR", ".ripr-cache")],
    )?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(!stderr.contains("ripr note:"), "{stderr}");
    Ok(())
}
