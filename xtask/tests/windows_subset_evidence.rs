//! Replay the production evidence guard and capture steps without product builds.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const WORKFLOW: &str = include_str!("../../.github/workflows/windows-subset-qualification.yml");
const LABELS: [&str; 4] = [
    "subset-gap-artifacts",
    "subset-cargo-home",
    "subset-cache-warning",
    "subset-clippy",
];
const PRODUCERS: [&str; 4] = [
    "Run lsp::gap_artifacts subset (advisory, #4921)",
    "Run Cargo-home identity subset (advisory, #7748)",
    "Run cache-warning smoke subset (advisory, #4918)",
    "Run clippy subset (advisory, #4337)",
];
static NEXT: AtomicU64 = AtomicU64::new(0);

fn body(name: &str) -> Result<String, String> {
    let (_, step) = WORKFLOW
        .split_once(&format!("      - name: {name}\n"))
        .ok_or("step missing")?;
    let (_, run) = step.split_once("        run: |\n").ok_or("run missing")?;
    Ok(run
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "ripr-subset-evidence-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(path.join("target")).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
    fn valid(&self) -> Result<(), String> {
        for label in LABELS {
            fs::write(self.0.join(format!("target/{label}.status")), "0\n")
                .map_err(|e| e.to_string())?;
            fs::write(
                self.0.join(format!("target/{label}.log")),
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn run(&self, script: &str) -> Result<bool, String> {
        let path = self.0.join("replay.sh");
        fs::write(&path, script).map_err(|e| e.to_string())?;
        let bash = std::env::var_os("RIPR_TEST_BASH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if cfg!(windows) {
                    PathBuf::from("C:/Program Files/Git/bin/bash.exe")
                } else {
                    PathBuf::from("bash")
                }
            });
        let output = Command::new(bash)
            .args(["--noprofile", "--norc", "-eo", "pipefail", "replay.sh"])
            .current_dir(&self.0)
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .output()
            .map_err(|e| e.to_string())?;
        Ok(output.status.success())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn guard_accepts_successful_executed_tests_and_native_advisory_failures() -> Result<(), String> {
    let scratch = Scratch::new()?;
    scratch.valid()?;
    let guard = body("Fail on missing or empty subset evidence")?;
    assert!(scratch.run(&guard)?, "valid success evidence rejected");
    for status in ["1\n", "101\n", "255\n"] {
        for label in LABELS {
            fs::write(scratch.0.join(format!("target/{label}.status")), status)
                .map_err(|e| e.to_string())?;
            fs::write(
                scratch.0.join(format!("target/{label}.log")),
                "native command failed; advisory verdict\n",
            )
            .map_err(|e| e.to_string())?;
        }
        assert!(
            scratch.run(&guard)?,
            "valid nonzero advisory status {status:?} rejected"
        );
    }
    Ok(())
}

#[test]
fn guard_rejects_all_empty_pairs() -> Result<(), String> {
    let scratch = Scratch::new()?;
    for label in LABELS {
        for suffix in ["status", "log"] {
            fs::write(scratch.0.join(format!("target/{label}.{suffix}")), "")
                .map_err(|e| e.to_string())?;
        }
    }
    assert!(
        !scratch.run(&body("Fail on missing or empty subset evidence")?)?,
        "four empty status/log pairs accepted"
    );
    Ok(())
}

#[test]
fn guard_rejects_each_absent_empty_or_nonfile_component() -> Result<(), String> {
    let guard = body("Fail on missing or empty subset evidence")?;
    for label in LABELS {
        for suffix in ["status", "log"] {
            for defect in ["absent", "empty", "directory"] {
                let scratch = Scratch::new()?;
                scratch.valid()?;
                // A nonzero verdict must not exempt its log from evidence checks.
                fs::write(scratch.0.join(format!("target/{label}.status")), "101\n")
                    .map_err(|e| e.to_string())?;
                let path = scratch.0.join(format!("target/{label}.{suffix}"));
                fs::remove_file(&path).map_err(|e| e.to_string())?;
                if defect == "empty" {
                    fs::write(&path, "").map_err(|e| e.to_string())?;
                }
                if defect == "directory" {
                    fs::create_dir(&path).map_err(|e| e.to_string())?;
                }
                assert!(!scratch.run(&guard)?, "accepted {label}.{suffix}: {defect}");
            }
        }
    }
    Ok(())
}

#[test]
fn guard_rejects_malformed_or_out_of_range_native_statuses() -> Result<(), String> {
    let guard = body("Fail on missing or empty subset evidence")?;
    for label in LABELS {
        for status in [
            "\n",
            " ",
            "oops\n",
            "0\n101\n",
            "-1\n",
            "256\n",
            "4294967295\n",
            "01\n",
            "0 \n",
            "0\0\n",
            "0\n\n",
        ] {
            let scratch = Scratch::new()?;
            scratch.valid()?;
            fs::write(scratch.0.join(format!("target/{label}.status")), status)
                .map_err(|e| e.to_string())?;
            assert!(!scratch.run(&guard)?, "accepted {label} status {status:?}");
        }
    }
    Ok(())
}

#[test]
fn guard_rejects_success_without_executed_tests_but_clippy_is_not_a_test() -> Result<(), String> {
    let guard = body("Fail on missing or empty subset evidence")?;
    for label in LABELS.into_iter().take(3) {
        let scratch = Scratch::new()?;
        scratch.valid()?;
        fs::write(
            scratch.0.join(format!("target/{label}.log")),
            "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 9 filtered out\n",
        )
        .map_err(|e| e.to_string())?;
        assert!(
            !scratch.run(&guard)?,
            "zero-execution success accepted: {label}"
        );
    }
    let scratch = Scratch::new()?;
    scratch.valid()?;
    fs::write(
        scratch.0.join("target/subset-clippy.log"),
        "Finished clippy successfully\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(
        scratch.run(&guard)?,
        "clippy incorrectly requires a test denominator"
    );
    Ok(())
}

#[test]
fn producers_preserve_native_verdict_but_fail_capture_write_errors() -> Result<(), String> {
    for (step, label) in PRODUCERS.into_iter().zip(LABELS) {
        let producer = body(step)?;
        let mock = "cargo() { printf 'native command output\\n'; return 101; }\n";
        let scratch = Scratch::new()?;
        assert!(
            scratch.run(&format!("{mock}{producer}"))?,
            "advisory native failure rejected: {label}"
        );
        assert_eq!(
            fs::read_to_string(scratch.0.join(format!("target/{label}.status")))
                .map_err(|e| e.to_string())?,
            "101\n"
        );
        for suffix in ["status", "log"] {
            let scratch = Scratch::new()?;
            fs::create_dir(scratch.0.join(format!("target/{label}.{suffix}")))
                .map_err(|e| e.to_string())?;
            assert!(
                !scratch.run(&format!("{mock}{producer}"))?,
                "capture write failure hidden: {label}.{suffix}"
            );
        }
        let scratch = Scratch::new()?;
        fs::remove_dir(scratch.0.join("target")).map_err(|e| e.to_string())?;
        fs::write(scratch.0.join("target"), "blocked").map_err(|e| e.to_string())?;
        assert!(
            !scratch.run(&format!("{mock}{producer}"))?,
            "target creation failure hidden: {label}"
        );
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn selector_capture_cmdlets_fail_writes_but_keep_native_verdict_advisory() -> Result<(), String> {
    let producer = body("Produce and replay exact-head selector recovery (advisory)")?;
    let start = producer
        .rfind("\ntry {")
        .ok_or("selector capture missing")?;
    let capture = &producer[start..];
    for defect in ["none", "status", "log"] {
        for status in [0, 101] {
            let scratch = Scratch::new()?;
            if defect != "none" {
                fs::create_dir(
                    scratch
                        .0
                        .join(format!("target/subset-selector-literal.{defect}")),
                )
                .map_err(|e| e.to_string())?;
            }
            let script = format!(
                "function python {{ Write-Output 'mock native consumer diagnostic'; $global:LASTEXITCODE = {status} }}\n$consumer = 'mock input'\n{capture}"
            );
            fs::write(scratch.0.join("capture.ps1"), script).map_err(|e| e.to_string())?;
            let output = Command::new("pwsh")
                .args(["-NoProfile", "-NonInteractive", "-File", "capture.ps1"])
                .current_dir(&scratch.0)
                .output()
                .map_err(|e| e.to_string())?;
            assert_eq!(
                output.status.success(),
                defect == "none",
                "selector capture {defect} / native {status}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if defect == "none" {
                assert_eq!(
                    fs::read_to_string(scratch.0.join("target/subset-selector-literal.status"))
                        .map_err(|e| e.to_string())?
                        .trim(),
                    status.to_string()
                );
            }
        }
    }
    Ok(())
}
