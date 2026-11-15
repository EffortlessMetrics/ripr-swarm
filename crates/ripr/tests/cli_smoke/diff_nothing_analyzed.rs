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

/// A real docs-only git-style diff parses to one changed file and no analyzable
/// source, so the producer reports a complete-but-empty outcome. The agent and
/// github formats must say nothing was analyzed instead of reporting a clean
/// result (#13658).
#[test]
fn docs_only_diff_is_not_reported_clean_on_stdout() -> TestResult {
    let root = unique_external_workspace("diff-docs-only")?;
    fs::create_dir_all(&root)?;
    let diff = root.join("docs.diff");
    fs::write(
        &diff,
        "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\n # Title\n+More words.\n",
    )?;
    let diff = diff.to_string_lossy().into_owned();
    let run = |format: &str| -> TestResult<String> {
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["check", "--diff", &diff, "--format", format, "--quiet"],
        )?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };

    // Pin the stimulus first: one parsed changed file, no analyzable source.
    let json: serde_json::Value = serde_json::from_str(&run("json")?)?;
    let outcome = &json["analysis_outcome"]["outcome"];
    assert!(
        matches!(
            outcome["kind"].as_str(),
            Some("no_changed_lines" | "no_behavioral_candidates")
        ),
        "{outcome}"
    );
    assert_eq!(outcome["counts"]["changed_file_count"], 1, "{outcome}");
    assert_eq!(json["summary"]["changed_rust_files"], 0, "{json}");

    let agent = run("agent")?;
    assert!(
        agent.contains("\nstop=nothing was analyzed (no changed source in scope)"),
        "{agent}"
    );
    assert!(!agent.contains("done="), "{agent}");
    assert!(!agent.contains("verify=complete"), "{agent}");
    let github = run("github")?;
    assert!(
        github.contains("::warning title=ripr nothing analyzed::"),
        "{github}"
    );
    assert!(
        !github.contains("No static exposure findings found"),
        "{github}"
    );
    Ok(())
}
