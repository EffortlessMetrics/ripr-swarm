use super::{run_command, unique_temp_workspace};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn init_ci_followup_explains_adoption_for_created_and_refreshed_workflows() -> TestResult {
    let scratch = unique_temp_workspace("init-ci-guidance");
    let result: TestResult = (|| {
        for refresh in [false, true] {
            let root = scratch.join(if refresh {
                "refresh répo it's"
            } else {
                "new répo it's"
            });
            std::fs::create_dir_all(&root)?;
            let config = "[analysis]\nmode = \"instant\"\n";
            std::fs::write(root.join("ripr.toml"), config)?;
            let workflow = root.join(".github/workflows/ripr.yml");
            if refresh {
                std::fs::create_dir_all(workflow.parent().ok_or("workflow parent")?)?;
                std::fs::write(&workflow, "name: old workflow\n")?;
            }
            let root_arg = root.to_str().ok_or("fixture root must be UTF-8")?;
            let mut args = vec!["init", "--root", root_arg, "--ci", "github"];
            if refresh {
                args.push("--force");
            }
            let output = run_command(env!("CARGO_BIN_EXE_ripr"), None, &args)?;
            assert!(output.status.success(), "{output:?}");
            assert!(workflow.is_file());
            assert_eq!(std::fs::read_to_string(root.join("ripr.toml"))?, config);
            let stdout = String::from_utf8(output.stdout)?;
            let guidance = stdout.lines().find(|line| line.starts_with("CI next:"));
            let guidance =
                guidance.ok_or_else(|| format!("missing workflow adoption guidance: {stdout}"))?;
            let shown = workflow
                .to_str()
                .ok_or("workflow must be UTF-8")?
                .replace('\\', "/");
            assert!(guidance.contains(&format!("review `{shown}`")), "{stdout}");
            for expected in [
                "advisory by default",
                "RIPR_GATE_MODE",
                "opt-in",
                "commit",
                "pull request",
                "job summary",
            ] {
                assert!(guidance.contains(expected), "missing {expected}: {stdout}");
            }
            assert!(stdout.contains("ripr doctor --root"), "{stdout}");
            assert!(stdout.contains("ripr check --root"), "{stdout}");
            assert!(
                stdout.contains("(PowerShell) ripr doctor --root"),
                "{stdout}"
            );
            assert!(
                stdout.contains("(PowerShell) ripr check --root"),
                "{stdout}"
            );
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

#[test]
fn init_ci_followup_does_not_claim_adoption_for_config_only_or_preview() -> TestResult {
    let root = unique_temp_workspace("init-ci-guidance-controls");
    std::fs::create_dir_all(&root)?;
    let result: TestResult = (|| {
        let root_arg = root.to_str().ok_or("fixture root must be UTF-8")?;
        for args in [
            vec!["init", "--root", root_arg],
            vec!["init", "--root", root_arg, "--ci", "github", "--dry-run"],
        ] {
            let output = run_command(env!("CARGO_BIN_EXE_ripr"), None, &args)?;
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout)?;
            assert!(!stdout.contains("CI next:"), "{stdout}");
            assert!(!root.join(".github/workflows/ripr.yml").exists());
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&root)?;
    result
}
