//! An empty or unparseable `--diff` examined nothing, so the agent and github
//! formats must say so on stdout instead of reporting `done=` or the clean
//! notice (#13658).
use super::{run_command, unique_external_workspace};
use std::fs;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn empty_and_unparseable_diffs_are_not_reported_clean_on_stdout() -> TestResult {
    let root = unique_external_workspace("diff-nothing-analyzed")?;
    fs::create_dir_all(&root)?;
    // An empty diff maps to `no_scope` (this fix); a non-empty unparseable one
    // already maps to `unsupported_input` and was never reported clean.
    for (name, body, stop, warning) in [
        (
            "empty.diff",
            "",
            "\nstop=nothing was analyzed",
            "::warning title=ripr nothing analyzed::",
        ),
        (
            "garbage.diff",
            "this is not a diff\n",
            "\nstop=analysis incomplete",
            "::warning title=ripr analysis incomplete::",
        ),
    ] {
        let diff = root.join(name);
        fs::write(&diff, body)?;
        let diff = diff.to_string_lossy().into_owned();
        let agent = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["check", "--diff", &diff, "--format", "agent", "--quiet"],
        )?;
        let agent = String::from_utf8_lossy(&agent.stdout).into_owned();
        assert!(agent.contains(stop), "{name}: {agent}");
        assert!(!agent.contains("done="), "{name}: {agent}");
        let github = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["check", "--diff", &diff, "--format", "github", "--quiet"],
        )?;
        let github = String::from_utf8_lossy(&github.stdout).into_owned();
        assert!(github.contains(warning), "{name}: {github}");
        assert!(
            !github.contains("No static exposure findings found"),
            "{name}: {github}"
        );
    }
    Ok(())
}
