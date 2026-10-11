//! Execute the advisory lane's production script, including failed instruments.
use std::path::Path;

fn workflow() -> Result<String, String> {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(".github/workflows/future-clippy.yml"),
    )
    .map_err(|e| e.to_string())
}

fn step(name: &str) -> Result<String, String> {
    let text = workflow()?;
    let (_, tail) = text
        .split_once(&format!("- name: {name}\n"))
        .ok_or_else(|| format!("missing workflow step: {name}"))?;
    repo_policy::extract_workflow_run_blocks(tail)
        .first()
        .map(|block| block.text.clone())
        .ok_or_else(|| format!("missing run block: {name}"))
}

#[test]
fn pinned_component_and_failure_evidence_are_wired() -> Result<(), String> {
    let text = workflow()?;
    assert!(text.contains("uses: dtolnay/rust-toolchain@1.95.0\n"));
    assert!(text.contains("components: clippy\n"));
    assert!(step("Run future Clippy (advisory)")?.contains("cargo +1.95.0 clippy "));
    let (_, summary) = text
        .split_once("- name: Write step summary\n")
        .ok_or("missing summary")?;
    assert!(summary.starts_with("        if: always()\n"));
    assert!(summary.contains("SCAN_OUTCOME: ${{ steps.scan.outcome }}"));
    assert!(text.contains("id: scan\n"));
    let (_, upload) = text
        .split_once("- name: Upload future-clippy log\n")
        .ok_or("missing upload")?;
    assert!(upload.starts_with("        if: always()\n"));
    assert!(upload.contains("target/future-clippy.log"));
    assert!(upload.contains("target/future-clippy-status.txt"));
    assert!(upload.contains("if-no-files-found: error"));
    assert!(!text.contains("lane never fails"));
    Ok(())
}

#[cfg(target_os = "linux")]
mod behavior {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::{Command, Output};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::step;

    const CARGO: &str = r#"#!/bin/bash
printf '%s\n' "$@" > "$ARG_LOG"
case "$SCAN_CASE" in
  missing) echo "error: the 'cargo-clippy' binary is not installed for toolchain '1.95-x86_64-unknown-linux-gnu'" >&2; exit 1 ;;
  compile) echo 'error[E0308]: mismatched types' >&2; echo '{"reason":"build-finished","success":false}'; exit 101 ;;
  empty) exit 0 ;;
  completion-only) echo '{"reason":"build-finished","success":true}'; exit 0 ;;
  warning) echo 'warning: clippy::duration_suboptimal_units' >&2 ;;
esac
echo '{"reason":"compiler-artifact","fresh":true}'
if [[ "$SCAN_CASE" != truncated ]]; then
  echo '{"reason":"build-finished","success":true}'
fi
if [[ "$SCAN_CASE" == late-failed ]]; then
  echo '{"reason":"build-finished","success":false}'
fi
echo '    Finished dev profile' >&2
"#;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(case: &str) -> Result<Self, String> {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .join(format!(
                    "future-clippy-{case}-{}-{}",
                    std::process::id(),
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_nanos()
                ));
            fs::create_dir_all(root.join("bin")).map_err(|e| e.to_string())?;
            let fixture = Self(root);
            fixture.executable("cargo", CARGO)?;
            Ok(fixture)
        }

        fn executable(&self, name: &str, source: &str) -> Result<(), String> {
            let path = self.0.join("bin").join(name);
            fs::write(&path, source).map_err(|e| e.to_string())?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
        }

        fn read(&self, path: &str) -> Result<String, String> {
            fs::read_to_string(self.0.join(path)).map_err(|e| e.to_string())
        }

        fn run(&self, name: &str, case: &str, outcome: &str) -> Result<Output, String> {
            let script = self.0.join("step.sh");
            fs::write(&script, step(name)?).map_err(|e| e.to_string())?;
            let mut paths = vec![self.0.join("bin")];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").ok_or("missing PATH")?,
            ));
            Command::new("timeout")
                .args(["20s", "bash", "--noprofile", "--norc", "-eo", "pipefail"])
                .arg(script)
                .current_dir(&self.0)
                .env(
                    "PATH",
                    std::env::join_paths(paths).map_err(|e| e.to_string())?,
                )
                .env("ARG_LOG", self.0.join("args"))
                .env("SCAN_CASE", case)
                .env("SCAN_OUTCOME", outcome)
                .env("GITHUB_SHA", "candidate-sha")
                .env("GITHUB_RUN_ID", "7307")
                .env("GITHUB_RUN_ATTEMPT", "2")
                .env("GITHUB_STEP_SUMMARY", self.0.join("summary"))
                .output()
                .map_err(|e| e.to_string())
        }

        fn summary(&self, outcome: &str) -> Result<String, String> {
            let output = self.run("Write step summary", "empty", outcome)?;
            assert!(output.status.success(), "{output:?}");
            self.read("summary")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(e) = fs::remove_dir_all(&self.0) {
                eprintln!("retain fixture {}: {e}", self.0.display());
            }
        }
    }

    #[test]
    fn missing_component_is_not_a_success() -> Result<(), String> {
        let fixture = Fixture::new("missing")?;
        let output = fixture.run("Run future Clippy (advisory)", "missing", "failure")?;
        assert!(
            !output.status.success(),
            "missing Clippy was green: {output:?}"
        );
        assert!(
            fixture
                .read("target/future-clippy.log")?
                .contains("not installed")
        );
        let summary = fixture.summary("failure")?;
        assert!(summary.contains("cargo_exit=1"));
        assert!(!summary.contains("scan_completed=true"));
        Ok(())
    }

    #[test]
    fn warnings_and_cached_artifacts_remain_advisory() -> Result<(), String> {
        let fixture = Fixture::new("warning")?;
        let output = fixture.run("Run future Clippy (advisory)", "warning", "success")?;
        assert!(output.status.success(), "{output:?}");
        let args = fixture.read("args")?;
        assert!(args.starts_with("+1.95.0\nclippy\n--workspace\n--all-targets\n"));
        assert!(args.contains("--message-format=json-render-diagnostics\n"));
        assert_eq!(args.lines().filter(|arg| *arg == "-W").count(), 10);
        assert!(args.contains("clippy::same_length_and_capacity\n"));
        assert!(
            fixture
                .read("target/future-clippy.log")?
                .contains("warning:")
        );
        let summary = fixture.summary("success")?;
        for evidence in [
            "source_sha=candidate-sha",
            "run_id=7307",
            "run_attempt=2",
            "toolchain=1.95.0",
            "cargo_exit=0",
            "tee_exit=0",
            "scan_completed=true",
        ] {
            assert!(summary.contains(evidence), "missing {evidence}: {summary}");
        }
        Ok(())
    }

    #[test]
    fn failed_compile_or_incomplete_current_scan_cannot_reuse_old_success() -> Result<(), String> {
        for case in [
            "compile",
            "empty",
            "truncated",
            "completion-only",
            "late-failed",
        ] {
            let fixture = Fixture::new(case)?;
            fs::create_dir_all(fixture.0.join("target")).map_err(|e| e.to_string())?;
            fs::write(fixture.0.join("target/future-clippy.log"), "{\"reason\":\"compiler-artifact\"}\n{\"reason\":\"build-finished\",\"success\":true}\n")
                .map_err(|e| e.to_string())?;
            fs::write(
                fixture.0.join("target/future-clippy-status.txt"),
                "scan_completed=true\nsource_sha=old-sha\n",
            )
            .map_err(|e| e.to_string())?;
            let output = fixture.run("Run future Clippy (advisory)", case, "failure")?;
            assert!(!output.status.success(), "{case} was green: {output:?}");
            let summary = fixture.summary("failure")?;
            assert!(
                !summary.contains("scan_completed=true"),
                "{case}: {summary}"
            );
            assert!(!summary.contains("old-sha"), "{case}: {summary}");
            if case == "compile" {
                assert!(summary.contains("cargo_exit=101"), "{summary}");
            }
        }
        Ok(())
    }

    #[test]
    fn failed_log_capture_and_absent_scan_are_visible() -> Result<(), String> {
        let fixture = Fixture::new("tee")?;
        fixture.executable("tee", "#!/bin/bash\n/bin/cat\nexit 74\n")?;
        let output = fixture.run("Run future Clippy (advisory)", "warning", "failure")?;
        assert!(!output.status.success(), "failed tee was green: {output:?}");
        let summary = fixture.summary("failure")?;
        assert!(summary.contains("cargo_exit=0"), "{summary}");
        assert!(summary.contains("tee_exit=74"), "{summary}");
        assert!(!summary.contains("scan_completed=true"), "{summary}");
        let absent = Fixture::new("not-run")?;
        assert!(
            absent
                .summary("skipped")?
                .contains("Scan evidence unavailable")
        );
        fs::create_dir_all(absent.0.join("target")).map_err(|e| e.to_string())?;
        for identity in [
            "source_sha=old-sha\nrun_id=7307\nrun_attempt=2",
            "source_sha=candidate-sha\nrun_id=old-run\nrun_attempt=2",
            "source_sha=candidate-sha\nrun_id=7307\nrun_attempt=1",
        ] {
            fs::write(absent.0.join("summary"), "").map_err(|e| e.to_string())?;
            fs::write(
                absent.0.join("target/future-clippy-status.txt"),
                format!("{identity}\nscan_completed=true\n"),
            )
            .map_err(|e| e.to_string())?;
            let summary = absent.summary("skipped")?;
            assert!(summary.contains("Scan evidence unavailable"), "{summary}");
            assert!(!summary.contains("scan_completed=true"), "{summary}");
        }
        Ok(())
    }
}
