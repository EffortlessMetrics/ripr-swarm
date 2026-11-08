//! A non-empty unparseable `--diff` maps to `unsupported_input`, so the agent
//! and github formats take the incomplete-analysis path and never report a
//! clean result (#13658). The empty-diff `no_scope` case is pinned elsewhere.
use super::{run_command, unique_external_workspace};
use std::fs;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn unparseable_diff_routes_to_incomplete_analysis_not_a_clean_result() -> TestResult {
    let root = unique_external_workspace("diff-unparseable-routing")?;
    fs::create_dir_all(&root)?;
    let diff = root.join("garbage.diff");
    fs::write(&diff, "this is not a diff\n")?;
    let diff = diff.to_string_lossy().into_owned();
    let agent = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["check", "--diff", &diff, "--format", "agent", "--quiet"],
    )?;
    let agent = String::from_utf8_lossy(&agent.stdout).into_owned();
    assert!(agent.contains("outcome=unsupported_input"), "{agent}");
    assert!(agent.contains("\nstop=analysis incomplete"), "{agent}");
    assert!(!agent.contains("done="), "{agent}");
    assert!(!agent.contains("verify=complete"), "{agent}");
    let github = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["check", "--diff", &diff, "--format", "github", "--quiet"],
    )?;
    let github = String::from_utf8_lossy(&github.stdout).into_owned();
    assert!(
        github.contains("::warning title=ripr analysis incomplete::"),
        "{github}"
    );
    assert!(
        !github.contains("No static exposure findings found"),
        "{github}"
    );
    Ok(())
}
