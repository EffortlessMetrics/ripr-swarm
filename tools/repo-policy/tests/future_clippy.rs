//! Execute the advisory lane's production script, including failed instruments.
#[path = "../test_support.rs"]
mod test_support;

fn workflow() -> Result<String, String> {
    std::fs::read_to_string(
        test_support::workspace_root()?.join(".github/workflows/future-clippy.yml"),
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
    assert!(text.contains(
        "uses: dtolnay/rust-toolchain@035fceb3c12fb0c6a9461b52775e16473133e591 # 1.95.0\n"
    ));
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

#[test]
fn stable_compatibility_requires_matching_tools_and_focused_commands() -> Result<(), String> {
    const SETUP: &str = "      - uses: dtolnay/rust-toolchain@b725eda525362e7cf34af9a7243cac1e3f01a7ec # stable\n        with:\n          toolchain: stable\n          components: clippy, llvm-tools-preview\n";
    const TESTS: &str =
        "cargo +stable test --locked -p repo-policy --lib coverage_diagnostics -- --nocapture";
    const CLIPPY: &str =
        "cargo +stable clippy --locked -p repo-policy --all-targets -- -D warnings";
    let job = |text: &str, name: &str| {
        text.split_once(&format!("\n  {name}:\n")).map(|(_, tail)| {
            tail.lines()
                .take_while(|line| {
                    line.is_empty()
                        || line.starts_with("    ")
                        || line.trim_start().starts_with('#')
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
    };
    let prepared = |text: &str| {
        let (Some(stable), Some(pinned)) =
            (job(text, "stable-repo-policy"), job(text, "future-clippy"))
        else {
            return false;
        };
        let pinned_guard = pinned.lines().find(|line| line.starts_with("    if:"));
        if pinned_guard.is_none()
            || stable.lines().find(|line| line.starts_with("    if:")) != pinned_guard
            || !stable.contains("    env:\n      RUSTUP_TOOLCHAIN: stable\n")
            || stable
                .lines()
                .filter(|line| line.trim_start().starts_with("RUSTUP_TOOLCHAIN:"))
                .count()
                != 1
            || !stable.lines().any(|line| line == "    runs-on: ubuntu-latest")
            || !stable.lines().any(|line| line == "    timeout-minutes: 15")
            || stable.lines().any(|line| {
                line.starts_with("        if:")
                    || line.starts_with("      - if:")
                    || line.trim_start().starts_with("continue-on-error:")
            })
        {
            return false;
        }
        let Some((before, after)) = stable.split_once(SETUP) else {
            return false;
        };
        if !repo_policy::extract_workflow_run_blocks(before).is_empty() {
            return false;
        }
        let blocks = repo_policy::extract_workflow_run_blocks(after);
        matches!(blocks.as_slice(), [identity, tests, clippy]
            if identity.text.contains("compiler=$(rustc -vV)")
                && identity.text.contains("\"$profdata\" --version")
                && tests.text.trim() == TESTS
                && clippy.text.trim() == CLIPPY)
    };
    let text = workflow()?;
    assert!(prepared(&text), "stable compatibility proof must be wired");
    for (label, from, to) in [
        (
            "missing LLVM component",
            "components: clippy, llvm-tools-preview",
            "components: clippy",
        ),
        ("wrong toolchain", "toolchain: stable", "toolchain: 1.95.0"),
        (
            "wrong fixture activation",
            "RUSTUP_TOOLCHAIN: stable",
            "RUSTUP_TOOLCHAIN: 1.95.0",
        ),
        (
            "conditional tool preparation",
            "          components: clippy, llvm-tools-preview\n",
            "          components: clippy, llvm-tools-preview\n        if: false\n",
        ),
        (
            "ignored failures",
            "    name: stable-repo-policy\n",
            "    name: stable-repo-policy\n    continue-on-error: true\n",
        ),
        ("omitted diagnostics", TESTS, "echo diagnostics omitted"),
        ("omitted Clippy", CLIPPY, "echo Clippy omitted"),
    ] {
        let changed = text.replace(from, to);
        assert_ne!(changed, text, "{label} must change the real workflow");
        assert!(!prepared(&changed), "{label} must reject");
    }
    let late_setup = text.replacen(SETUP, "", 1) + SETUP;
    assert!(!prepared(&late_setup), "tools must precede proof commands");
    Ok(())
}

#[cfg(target_os = "linux")]
mod behavior {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::{Command, Output};

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
            let root = super::test_support::scratch(&format!("future-clippy-{case}"))?;
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
    fn stable_identity_uses_matching_llvm_and_preserves_tool_failures() -> Result<(), String> {
        const RUSTC: &str = r#"#!/bin/bash
case "$*" in
  -vV)
    [[ "$SCAN_CASE" != compiler ]] || exit 65
    echo 'rustc fixture identity'
    [[ "$SCAN_CASE" == host ]] || echo 'host: fixture-host'
    ;;
  '--print sysroot')
    [[ "$SCAN_CASE" != sysroot ]] || exit 67
    printf '%s/selected toolchain\n' "$PWD"
    ;;
  *) exit 68 ;;
esac
"#;
        const LLVM: &str = r#"#!/bin/bash
[[ "$*" == --version ]] || exit 69
[[ "$SCAN_CASE" != llvm ]] || exit 70
echo 'LLVM fixture identity'
"#;
        for (case, status) in [
            ("success", 0),
            ("compiler", 65),
            ("sysroot", 67),
            ("host", 1),
            ("llvm", 70),
            ("missing-llvm", 127),
        ] {
            let fixture = Fixture::new(case)?;
            fixture.executable("rustc", RUSTC)?;
            fixture.executable(
                "llvm-profdata",
                "#!/bin/bash\necho 'wrong LLVM from PATH'\nexit 77\n",
            )?;
            let bin = fixture
                .0
                .join("selected toolchain/lib/rustlib/fixture-host/bin");
            fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
            let profdata = bin.join("llvm-profdata");
            if case != "missing-llvm" {
                fs::write(&profdata, LLVM).map_err(|e| e.to_string())?;
                fs::set_permissions(&profdata, fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
            }
            let output = fixture.run("Record stable compiler and LLVM identity", case, "unused")?;
            assert_eq!(output.status.code(), Some(status), "{case}: {output:?}");
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(!stdout.contains("wrong LLVM from PATH"), "{stdout}");
            if status == 0 {
                for evidence in [
                    "rustc fixture identity".to_string(),
                    format!("llvm-profdata={}", profdata.display()),
                    "LLVM fixture identity".to_string(),
                ] {
                    assert!(stdout.contains(&evidence), "missing {evidence}: {stdout}");
                }
            }
        }
        Ok(())
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
