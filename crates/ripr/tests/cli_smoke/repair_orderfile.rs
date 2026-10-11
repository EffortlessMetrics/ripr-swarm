//! #6837: ambient diff ordering cannot abort or bypass repair admission.
use super::{
    REPAIR_ROUTE_BOUNDARY_TEST, REPAIR_ROUTE_SEAM, REPAIR_ROUTE_WEAK_TEST, assert_success,
    ignore_remove_dir_all, repair_route_after, repair_route_attempt_id, repair_route_before,
    repair_route_commit, repair_route_manifest, repair_route_workspace, run_command, run_git,
    run_ripr,
};
use std::path::PathBuf;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.0);
    }
}

fn fixture(mode: &str) -> TestResult<Fixture> {
    let fixture = Fixture(repair_route_workspace(&format!("repair orderfile {mode}"))?);
    let orderfile = fixture.0.join(".git/repair orderfile");
    match mode {
        "default" => {}
        "dangling" => {
            assert!(!orderfile.exists(), "the dangling input must be absent");
            run_git(
                &fixture.0,
                &[
                    "config",
                    "diff.orderFile",
                    orderfile.to_str().ok_or("non-UTF-8 orderfile")?,
                ],
            )?;
        }
        "live" => {
            std::fs::write(&orderfile, "tests/pricing.rs\nsrc/lib.rs\n")?;
            run_git(
                &fixture.0,
                &[
                    "config",
                    "diff.orderFile",
                    orderfile.to_str().ok_or("non-UTF-8 orderfile")?,
                ],
            )?;
        }
        _ => return Err(format!("unknown fixture mode {mode}").into()),
    }
    Ok(fixture)
}

#[test]
fn repair_completes_despite_user_orderfile() -> TestResult {
    for mode in ["default", "dangling", "live"] {
        for committed in [false, true] {
            let fixture = fixture(mode)?;
            let root = &fixture.0;
            let attempt = repair_route_attempt_id(&repair_route_before(root)?)?;
            let before = repair_route_manifest(root, &attempt)?;
            assert_eq!(before["state"], "awaiting_edit", "{mode}: {before:#}");
            assert_eq!(before["seam_id"], REPAIR_ROUTE_SEAM);
            std::fs::write(
                root.join("tests/pricing.rs"),
                format!("{REPAIR_ROUTE_WEAK_TEST}{REPAIR_ROUTE_BOUNDARY_TEST}"),
            )?;
            if committed {
                run_git(root, &["add", "tests/pricing.rs"])?;
                repair_route_commit(root, "focused boundary test")?;
            }
            let after = repair_route_after(root, &attempt);
            assert_success(&after);
            let document: serde_json::Value = serde_json::from_slice(&after.stdout)?;
            assert_eq!(
                document["kind"], "repair_after_result",
                "{mode}: {document:#}"
            );
            let manifest = repair_route_manifest(root, &attempt)?;
            assert_eq!(manifest["state"], "ready_to_finish", "{mode}: {manifest:#}");
            let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(
                root.join("target/ripr/reports/agent-receipt.json"),
            )?)?;
            // Literal contract oracles: completing the static loop does not run
            // the test or turn advisory evidence into runtime verification.
            assert_eq!(receipt["status"], "advisory", "{mode}: {receipt:#}");
            assert_eq!(receipt["analysis_outcome_status"], "complete");
            assert_eq!(receipt["provenance"]["movement"], "improved");
            assert_eq!(receipt["verification"]["status"], "verification_not_run");
            assert_eq!(receipt["repair_attempt"]["attempt_id"], attempt);
            assert_eq!(
                receipt["repair_attempt"]["edit_cage_verdict"]["status"],
                "compliant"
            );
        }
    }
    Ok(())
}

#[test]
fn dirty_production_requires_recovery_despite_user_orderfile() -> TestResult {
    for mode in ["default", "dangling", "live"] {
        let fixture = fixture(mode)?;
        let root = &fixture.0;
        let production = std::fs::read_to_string(root.join("src/lib.rs"))?;
        std::fs::write(
            root.join("src/lib.rs"),
            format!("{production}\n// dirty production premise\n"),
        )?;
        std::fs::write(
            root.join("tests/pricing.rs"),
            format!("{REPAIR_ROUTE_WEAK_TEST}\n// dirty focused test\n"),
        )?;

        // Independently observe the stimulus with ordinary Git: the live
        // file reverses path order, and the dangling file really aborts it.
        let git = run_command(
            "git",
            Some(root),
            &[
                "diff",
                "--no-renames",
                "--no-ext-diff",
                "--name-only",
                "-z",
                "HEAD",
                "--",
            ],
        )?;
        if mode == "dangling" {
            assert_eq!(git.status.code(), Some(128), "{git:?}");
            assert!(String::from_utf8_lossy(&git.stderr).contains("failed to read orderfile"));
        } else {
            assert_success(&git);
            let expected = if mode == "live" {
                b"tests/pricing.rs\0src/lib.rs\0".as_slice()
            } else {
                b"src/lib.rs\0tests/pricing.rs\0".as_slice()
            };
            assert_eq!(
                git.stdout, expected,
                "{mode}: fixture must dirty both tracked files"
            );
        }
        let root_arg = root.to_str().ok_or("non-UTF-8 fixture root")?;
        let refused = run_ripr(&[
            "agent",
            "repair",
            "--json",
            "--root",
            root_arg,
            "--seam-id",
            REPAIR_ROUTE_SEAM,
            "--phase",
            "before",
        ]);
        assert_eq!(refused.status.code(), Some(2), "{mode}: {refused:?}");
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(
            stderr
                .contains("outside the allowed test surface already differ from HEAD: src/lib.rs"),
            "{mode}: {stderr}"
        );
        assert!(
            stderr.contains("to recover: commit the listed file(s)"),
            "{mode}: {stderr}"
        );
        assert!(
            stderr.contains("No workflow was prepared and no repair attempt was started"),
            "{mode}: {stderr}"
        );
        assert!(
            !stderr.contains("failed to read orderfile"),
            "{mode}: {stderr}"
        );
        // Admission allocates an operational .before.lock, even on refusal;
        // that store directory is not a published repair attempt.
        for entry in std::fs::read_dir(root.join("target/ripr/repair-attempts"))? {
            assert!(
                !entry?
                    .file_name()
                    .to_string_lossy()
                    .starts_with("repair-attempt-"),
                "{mode}: refused premise published an attempt"
            );
        }
        assert!(!root.join("target/ripr/workflow").exists());
    }
    Ok(())
}
