//! Exercise the collect-all command through the actual xtask driver and leaves.
use std::path::PathBuf;
use std::process::Command;

#[path = "../../tools/repo-policy/tests/common/git_environment.rs"]
mod git_environment;

const CHECKS: &[(&str, &str)] = &[
    ("check-static-language", "static-language.md"),
    ("check-no-panic-family", "no-panic-family.md"),
    ("check-allow-attributes", "allow-attributes.md"),
    ("check-local-context", "local-context.md"),
    ("check-covered-by", "covered-by.md"),
    ("check-executable-files", "executable-files.md"),
    ("check-process-policy", "process-policy.md"),
    ("check-network-policy", "network-policy.md"),
    ("check-lint-policy", "lint-policy.md"),
];

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "xtask-policy-all-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        let fixture = Self(root);
        for dir in [".ripr", "policy", "crates/sample/src", "target"] {
            std::fs::create_dir_all(fixture.0.join(dir)).map_err(|error| error.to_string())?;
        }
        fixture.write("Cargo.toml", "[workspace]\nmembers = [\"crates/sample\"]\n")?;
        fixture.write("crates/sample/Cargo.toml", "[package]\nname = \"sample\"\n")?;
        fixture.write("crates/sample/src/lib.rs", "#[test]\nfn sample_test() {}\n")?;
        fixture.write("README.md", "Draft exposure evidence.\n")?;
        fixture.write("ordinary", "ordinary file\n")?;
        fixture.write(
            ".ripr/static-language-allowlist.toml",
            "schema_version = 1\n",
        )?;
        for path in [
            ".ripr/allow-attributes.txt",
            "policy/local_context_allowlist.txt",
            "policy/clippy-exceptions.toml",
            "policy/process_allowlist.txt",
            "policy/network_allowlist.txt",
            "policy/clippy-lints.toml",
            "policy/clippy-debt.toml",
        ] {
            fixture.write(path, "")?;
        }
        fixture.git(&["init", "--quiet"])?;
        fixture.git(&["-c", "core.filemode=false", "add", "."])?;
        fixture.git(&["update-index", "--chmod=-x", "ordinary"])?;
        // Prove setup and preserve the exact existing leaf behavior as our oracle.
        for (check, _) in CHECKS {
            let output = fixture.run(check)?;
            assert!(output.status.success(), "healthy leaf {check}: {output:?}");
        }
        Ok(fixture)
    }

    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(path), text).map_err(|error| error.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let mut command = Command::new("git");
        command.args(args).current_dir(&self.0);
        git_environment::isolate_git_environment(&mut command);
        let output = command.output().map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!("fixture Git {args:?}: {output:?}"));
        }
        Ok(())
    }

    fn run(&self, check: &str) -> Result<std::process::Output, String> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command.arg(check).current_dir(&self.0);
        git_environment::isolate_git_environment(&mut command);
        command.output().map_err(|error| error.to_string())
    }

    fn reports(&self) -> PathBuf {
        self.0.join("target/ripr/reports")
    }

    fn compare_leaves(
        &self,
        aggregate: &std::process::Output,
        failed: &[&str],
    ) -> Result<(), String> {
        // Snapshot all nine before rerunning any: stale/omitted leaves cannot pass.
        let snapshots = CHECKS
            .iter()
            .map(|(_, file)| std::fs::read(self.reports().join(file)).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        for ((check, file), snapshot) in CHECKS.iter().zip(snapshots) {
            let output = self.run(check)?;
            assert_eq!(
                output.status.success(),
                !failed.contains(check),
                "{check}: {output:?}"
            );
            if output.status.success() {
                assert!(
                    String::from_utf8_lossy(&aggregate.stdout)
                        .contains(String::from_utf8_lossy(&output.stdout).as_ref()),
                    "leaf stdout changed: {check}"
                );
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                let error = error
                    .trim()
                    .strip_prefix("xtask: ")
                    .ok_or("leaf driver error prefix missing")?;
                assert!(
                    String::from_utf8_lossy(&aggregate.stderr).contains(error),
                    "leaf error changed: {check}"
                );
            }
            assert_eq!(
                std::fs::read(self.reports().join(file)).map_err(|e| e.to_string())?,
                snapshot,
                "leaf output changed: {check}"
            );
        }
        Ok(())
    }

    fn clear_reports(&self) -> Result<(), String> {
        std::fs::remove_dir_all(self.reports()).map_err(|error| error.to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!("retain fixture {}: {error}", self.0.display());
        }
    }
}

#[test]
fn policy_all_reports_two_independent_violations_and_runs_later_leaves() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.write(
        "README.md",
        &format!("{} claim.\n", "prov".to_string() + "en"),
    )?;
    fixture.git(&["update-index", "--chmod=+x", "ordinary"])?;
    fixture.clear_reports()?;
    let output = fixture.run("check-policy-all")?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "check-policy-all: 2 gate(s) failed: check-static-language, check-executable-files"
        ),
        "{stderr}"
    );
    assert!(stderr.contains("README.md:1"), "{stderr}");
    assert!(stderr.contains("not allowlisted: ordinary"), "{stderr}");
    let report = std::fs::read_to_string(fixture.reports().join("check-policy-all.md"))
        .map_err(|e| e.to_string())?;
    let commands = report
        .lines()
        .filter_map(|line| {
            line.strip_prefix("- `cargo xtask ")
                .and_then(|line| line.split_once('`').map(|(command, _)| command))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        CHECKS.iter().map(|(check, _)| *check).collect::<Vec<_>>()
    );
    assert!(report.contains("Status: fail"));
    assert!(report.contains("cargo xtask check-static-language"));
    assert!(report.contains("cargo xtask check-executable-files"));
    fixture.compare_leaves(
        &output,
        &["check-static-language", "check-executable-files"],
    )?;
    assert!(!fixture.reports().join("file-policy.md").exists());
    Ok(())
}

#[test]
fn policy_all_healthy_fixture_passes_with_original_leaf_reports() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.clear_reports()?;
    let output = fixture.run("check-policy-all")?;
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let pass_order = stdout
        .lines()
        .filter_map(|line| line.split_once(": pass ").map(|(check, _)| check))
        .filter(|check| CHECKS.iter().any(|(selected, _)| check == selected))
        .collect::<Vec<_>>();
    assert_eq!(
        pass_order,
        CHECKS.iter().map(|(check, _)| *check).collect::<Vec<_>>()
    );
    let report = std::fs::read_to_string(fixture.reports().join("check-policy-all.md"))
        .map_err(|e| e.to_string())?;
    assert!(report.contains("Status: pass"));
    fixture.compare_leaves(&output, &[])?;
    assert!(!fixture.reports().join("file-policy.md").exists());
    Ok(())
}

#[test]
fn policy_all_report_write_error_preserves_leaf_failures() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.git(&["update-index", "--chmod=+x", "ordinary"])?;
    std::fs::create_dir(fixture.reports().join("check-policy-all.md"))
        .map_err(|e| e.to_string())?;
    let output = fixture.run("check-policy-all")?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("check-policy-all: 1 gate(s) failed: check-executable-files"),
        "{stderr}"
    );
    assert!(stderr.contains("not allowlisted: ordinary"), "{stderr}");
    assert!(stderr.contains("failed to write"), "{stderr}");
    assert!(stderr.contains("check-policy-all.md"), "{stderr}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("check-lint-policy: pass"));
    Ok(())
}

#[test]
fn policy_all_healthy_leaves_still_fail_when_aggregate_report_cannot_be_written()
-> Result<(), String> {
    let fixture = Fixture::new()?;
    std::fs::create_dir(fixture.reports().join("check-policy-all.md"))
        .map_err(|error| error.to_string())?;
    let output = fixture.run("check-policy-all")?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to write"), "{stderr}");
    assert!(stderr.contains("check-policy-all.md"), "{stderr}");
    fixture.compare_leaves(&output, &[])?;
    Ok(())
}

#[test]
fn policy_all_keeps_early_io_error_and_does_not_trust_stale_leaf_report() -> Result<(), String> {
    let fixture = Fixture::new()?;
    std::fs::remove_file(fixture.0.join(".ripr/allow-attributes.txt"))
        .map_err(|e| e.to_string())?;
    fixture.git(&["update-index", "--chmod=+x", "ordinary"])?;
    let output = fixture.run("check-policy-all")?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "check-policy-all: 2 gate(s) failed: check-allow-attributes, check-executable-files"
        ),
        "{stderr}"
    );
    assert!(stderr.contains("allow-attributes.txt"), "{stderr}");
    let report = std::fs::read_to_string(fixture.reports().join("check-policy-all.md"))
        .map_err(|e| e.to_string())?;
    assert!(report.contains("check-allow-attributes`: fail"));
    assert!(report.contains("allow-attributes.txt"));
    assert!(report.contains("check-lint-policy`: pass"));
    Ok(())
}

#[test]
fn ci_policy_collects_real_cheap_failures_before_expensive_work() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.git(&[
        "-c",
        "user.name=fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-qm",
        "base",
    ])?;
    fixture.git(&["branch", "-f", "main"])?;
    fixture.write(
        "README.md",
        &format!("{} claim.\n", "prov".to_string() + "en"),
    )?;
    fixture.git(&["update-index", "--chmod=+x", "ordinary"])?;
    // A stale pass and a local receipt cannot supply a CI result or be overwritten.
    std::fs::write(
        fixture.reports().join("ci-policy.json"),
        r#"{"status":"pass"}"#,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(fixture.reports().join("precommit-v2.json"), "local witness")
        .map_err(|e| e.to_string())?;
    let output = fixture.run("ci-policy")?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "check-policy-all: 2 gate(s) failed: check-static-language, check-executable-files"
        ),
        "{stderr}"
    );
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixture.reports().join("ci-policy.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(report["status"], "source_or_policy_failure");
    assert_eq!(report["composition"], "ci_policy");
    assert_eq!(
        std::fs::read_to_string(fixture.reports().join("precommit-v2.json"))
            .map_err(|e| e.to_string())?,
        "local witness"
    );
    assert!(!fixture.reports().join("file-policy.md").exists());
    assert!(!fixture.reports().join("precommit.md").exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("check-lint-policy: pass"));
    Ok(())
}
