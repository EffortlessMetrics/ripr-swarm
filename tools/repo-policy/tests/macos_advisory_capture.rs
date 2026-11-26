//! Replay the advisory producers: Cargo outcomes are advisory, capture errors are not.
//! Linux fixtures exercise Bash semantics; they do not qualify native macOS execution.
#[path = "../test_support.rs"]
mod test_support;

#[cfg(target_os = "linux")]
mod shell {
    use super::test_support;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::PathBuf;
    use std::process::{Command, Output};

    const PRODUCERS: [(&str, &str, &str); 2] = [
        (
            "Clippy (advisory outcome)",
            "clippy",
            "clippy -p ripr -p xtask --all-targets -- -D warnings",
        ),
        (
            "Library tests (advisory outcome)",
            "lib-tests",
            "test -p ripr --lib --no-fail-fast",
        ),
    ];
    const CARGO: &str = r#"#!/bin/bash
printf '%s\n' "$*" >> "$RUNNER_TEMP/cargo-calls"
printf '%s\n' 'fixture diagnostic' 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'
exit "$CARGO_FIXTURE_STATUS"
"#;
    // Leave a nonempty, apparently complete log before failing. Existence-only
    // summary checks cannot detect this instrument failure.
    const TEE: &str = r#"#!/bin/bash
/usr/bin/tee "$@" || exit "$?"
exit "$TEE_FIXTURE_STATUS"
"#;

    fn step(name: &str) -> Result<String, String> {
        let source = fs::read_to_string(
            test_support::workspace_root()?.join(".github/workflows/macos-advisory.yml"),
        )
        .map_err(|e| e.to_string())?;
        let marker = format!("      - name: {name}\n");
        let mut sections = source.split(&marker);
        sections.next();
        let section = sections
            .next()
            .ok_or_else(|| format!("missing step {name}"))?;
        assert!(sections.next().is_none(), "duplicate step {name}");
        let end = section.find("\n      - ").unwrap_or(section.len());
        let blocks = repo_policy::extract_workflow_run_blocks(&section[..end]);
        match blocks.as_slice() {
            [block] => Ok(block.text.clone()),
            _ => Err(format!("expected one run block for {name}")),
        }
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Result<Self, String> {
            let fixture = Self(test_support::scratch("macos-capture")?);
            fs::create_dir_all(fixture.0.join("bin")).map_err(|e| e.to_string())?;
            fs::create_dir_all(fixture.0.join("macos-advisory")).map_err(|e| e.to_string())?;
            for (name, source) in [("cargo", CARGO), ("tee", TEE)] {
                let path = fixture.0.join("bin").join(name);
                fs::write(&path, source).map_err(|e| e.to_string())?;
                fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
            }
            Ok(fixture)
        }

        fn evidence(&self, stem: &str, suffix: &str) -> PathBuf {
            self.0
                .join("macos-advisory")
                .join(format!("{stem}.{suffix}"))
        }

        fn run(&self, name: &str, cargo: i32, tee: i32) -> Result<Output, String> {
            let script = self.0.join("step.sh");
            fs::write(&script, step(name)?).map_err(|e| e.to_string())?;
            let mut paths = vec![self.0.join("bin")];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").ok_or("missing PATH")?,
            ));
            Command::new("timeout")
                .args([
                    "5s",
                    "bash",
                    "--noprofile",
                    "--norc",
                    "-e",
                    "-o",
                    "pipefail",
                ])
                .arg(script)
                .current_dir(&self.0)
                .env(
                    "PATH",
                    std::env::join_paths(paths).map_err(|e| e.to_string())?,
                )
                .env("RUNNER_TEMP", &self.0)
                .env("GITHUB_STEP_SUMMARY", self.0.join("summary.md"))
                .env("CARGO_FIXTURE_STATUS", cargo.to_string())
                .env("TEE_FIXTURE_STATUS", tee.to_string())
                .env_remove("BASH_ENV")
                .output()
                .map_err(|e| e.to_string())
        }

        fn check_producer(
            &self,
            stem: &str,
            argv: &str,
            status: Option<i32>,
        ) -> Result<(), String> {
            let calls =
                fs::read_to_string(self.0.join("cargo-calls")).map_err(|e| e.to_string())?;
            assert_eq!(
                calls,
                format!("{argv}\n"),
                "exactly one intended Cargo invocation"
            );
            let log = fs::read_to_string(self.evidence(stem, "log")).map_err(|e| e.to_string())?;
            assert!(log.contains("fixture diagnostic"));
            assert!(log.contains("test result: ok. 1 passed;"));
            if let Some(status) = status {
                assert_eq!(
                    fs::read_to_string(self.evidence(stem, "status")).map_err(|e| e.to_string())?,
                    format!("{status}\n")
                );
            }
            Ok(())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0) {
                eprintln!("retaining fixture {}: {error}", self.0.display());
            }
        }
    }

    fn advisory(index: usize) -> Result<(), String> {
        let (name, stem, argv) = PRODUCERS[index];
        for cargo in [0, 41] {
            let fixture = Fixture::new()?;
            let output = fixture.run(name, cargo, 0)?;
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            fixture.check_producer(stem, argv, Some(cargo))?;
        }
        Ok(())
    }
    fn failed_tee(index: usize) -> Result<(), String> {
        let (name, stem, argv) = PRODUCERS[index];
        for cargo in [0, 41] {
            let fixture = Fixture::new()?;
            let output = fixture.run(name, cargo, 73)?;
            fixture.check_producer(stem, argv, Some(cargo))?;
            assert_eq!(
                output.status.code(),
                Some(1),
                "tee failure must fail the step: {output:?}"
            );
            assert!(String::from_utf8_lossy(&output.stderr).contains("evidence capture failed"));
        }
        Ok(())
    }
    fn failed_status(index: usize, full: bool) -> Result<(), String> {
        let (name, stem, argv) = PRODUCERS[index];
        let fixture = Fixture::new()?;
        let status = fixture.evidence(stem, "status");
        if full {
            symlink("/dev/full", status)
        } else {
            fs::create_dir(status)
        }
        .map_err(|e| e.to_string())?;
        let output = fixture.run(name, 0, 0)?;
        fixture.check_producer(stem, argv, None)?;
        assert_eq!(
            output.status.code(),
            Some(1),
            "status write failure must fail the step: {output:?}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("evidence status write failed"));
        Ok(())
    }

    #[test]
    fn clippy_outcomes_stay_advisory() -> Result<(), String> {
        advisory(0)
    }
    #[test]
    fn library_outcomes_stay_advisory() -> Result<(), String> {
        advisory(1)
    }
    #[test]
    fn clippy_tee_failure_refuses_nonempty_log() -> Result<(), String> {
        failed_tee(0)
    }
    #[test]
    fn library_tee_failure_refuses_nonempty_log() -> Result<(), String> {
        failed_tee(1)
    }
    #[test]
    fn clippy_status_open_failure_refuses() -> Result<(), String> {
        failed_status(0, false)
    }
    #[test]
    fn library_status_open_failure_refuses() -> Result<(), String> {
        failed_status(1, false)
    }
    #[test]
    fn clippy_status_write_failure_refuses() -> Result<(), String> {
        failed_status(0, true)
    }
    #[test]
    fn library_status_write_failure_refuses() -> Result<(), String> {
        failed_status(1, true)
    }

    #[test]
    fn captured_advisory_failures_reach_summary() -> Result<(), String> {
        let fixture = Fixture::new()?;
        for (name, _, _) in PRODUCERS {
            let output = fixture.run(name, 41, 0)?;
            assert_eq!(output.status.code(), Some(0), "{output:?}");
        }
        let output = fixture.run("Summarize and enforce evidence", 0, 0)?;
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let summary =
            fs::read_to_string(fixture.0.join("summary.md")).map_err(|e| e.to_string())?;
        assert!(summary.contains("| clippy -p ripr -p xtask --all-targets | 41 | advisory |"));
        assert!(summary.contains("| test -p ripr --lib | 41 | test result: ok. 1 passed;"));
        Ok(())
    }
}
