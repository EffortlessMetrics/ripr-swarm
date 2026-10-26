//! Exercise the check budget through argv, environment, analysis and process exit.
use super::*;

struct FixtureRoot(PathBuf);
impl Drop for FixtureRoot {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.0);
    }
}

#[test]
fn check_budget_flag_and_environment_reach_analysis_timeout() -> Result<(), String> {
    let root = FixtureRoot(init_check_refusal_repo("check-budget-public", 1)?);
    std::fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = \"budget_probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|err| err.to_string())?;
    std::fs::write(
        root.0.join("src/lib.rs"),
        "pub fn over(x: u32) -> bool { x > 10 }\n",
    )
    .map_err(|err| err.to_string())?;
    let original_path = std::env::var_os("PATH").ok_or("PATH is missing")?;
    let real_git = std::env::split_paths(&original_path)
        .map(|dir| dir.join("git"))
        .find(|path| path.is_file())
        .ok_or("git is missing")?;
    let real_git = std::fs::canonicalize(real_git).map_err(|err| err.to_string())?;
    let shim_dir = root.0.join("shim");
    std::fs::create_dir(&shim_dir).map_err(|err| err.to_string())?;
    let shim = shim_dir.join("git");
    // Metadata calls must succeed before the budget starts. Delay only the
    // actual diff, then delegate to the same real producer in every control.
    std::fs::write(&shim, "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = diff ]; then\n    printf 'entered\\n' > \"$RIPR_TEST_DIFF_MARKER\"\n    sleep 4\n    break\n  fi\ndone\nexec \"$RIPR_TEST_REAL_GIT\" \"$@\"\n")
        .map_err(|err| err.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))
        .map_err(|err| err.to_string())?;
    let search_path = std::env::join_paths(
        std::iter::once(shim_dir).chain(std::env::split_paths(&original_path)),
    )
    .map_err(|err| err.to_string())?;
    let marker = root.0.join("diff-entered");
    for (explicit, environment, times_out) in [
        (Some("0"), "bad", false),
        (None, "1", true),
        (Some("1"), "0", true),
    ] {
        if marker.exists() {
            std::fs::remove_file(&marker).map_err(|err| err.to_string())?;
        }
        let mut command = probe_command(env!("CARGO_BIN_EXE_ripr"));
        command
            .current_dir(&root.0)
            .args(["check", "--root"])
            .arg(&root.0)
            .args([
                "--worktree",
                "--base",
                "HEAD",
                "--json",
                "--git-timeout",
                "30",
            ])
            .env("PATH", &search_path)
            .env("RIPR_TEST_REAL_GIT", &real_git)
            .env("RIPR_TEST_DIFF_MARKER", &marker)
            .env("RIPR_CHECK_TIMEOUT", environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(timeout) = explicit {
            command.args(["--timeout", timeout]);
        }
        let output = run_owned_stdin_probe(command, &[], std::time::Duration::from_secs(30))
            .map_err(|err| format!("budget probe did not complete: {err}"))?;
        assert_eq!(
            std::fs::read_to_string(&marker)
                .map_err(|err| format!("analysis never reached git diff: {err}"))?,
            "entered\n"
        );
        if times_out {
            let value = assert_check_json_refusal(&output, "analysis_timeout", "analysis/timeout")?;
            assert!(
                value["run_limitations"][0]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("1s wall-clock budget")),
                "{value}"
            );
            assert!(String::from_utf8_lossy(&output.stderr).contains("analysis_timeout"));
        } else {
            assert_success(&output);
            let value: serde_json::Value =
                serde_json::from_slice(&output.stdout).map_err(|err| err.to_string())?;
            assert!(
                value["findings"]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty()),
                "disabled budget must analyze the nonempty diff: {value}"
            );
        }
    }
    Ok(())
}
