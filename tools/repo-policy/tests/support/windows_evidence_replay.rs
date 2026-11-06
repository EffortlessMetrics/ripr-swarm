//! Execute only the literal summary and Cargo-evidence guard with synthetic
//! files. Linux Bash is not native Windows or actual product/capture evidence.
use super::*;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let temp = std::env::temp_dir();
        fs::create_dir_all(&temp)?;
        let root = temp.join(format!(
            "windows-evidence-replay-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root)?;
        let fixture = Self(root);
        fs::create_dir(fixture.0.join("target"))?;
        for producer in PRODUCERS {
            fixture.evidence(producer, Some(0), 1)?;
        }
        Ok(fixture)
    }

    fn evidence(&self, producer: Producer, status: Option<u8>, tests: u64) -> TestResult {
        let contract = producer.contract();
        let path = self.0.join(contract.status);
        if let Some(status) = status {
            fs::write(path, format!("{status}\n"))?;
        } else {
            fs::remove_file(path)?;
        }
        fs::write(
            self.0.join(contract.log),
            if contract.requires_nonzero_test_count {
                format!(
                    "test result: ok. {tests} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
                )
            } else {
                "synthetic non-test evidence\n".into()
            },
        )?;
        Ok(())
    }

    fn run(&self, source: &str, owner: &str) -> TestResult<Output> {
        let output = Command::new("/usr/bin/timeout")
            .args([
                "--kill-after=1s",
                "5s",
                "/bin/bash",
                "--noprofile",
                "--norc",
                "-eo",
                "pipefail",
                "-c",
                &bash_body(source, owner)?,
            ])
            .current_dir(&self.0)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.0)
            .env("LC_ALL", "C")
            .env("SELECTOR_CONSUMER_ENABLED", "true")
            .env("GITHUB_STEP_SUMMARY", self.0.join("step-summary.md"))
            .stdin(Stdio::null())
            .output()?;
        require(
            output
                .status
                .code()
                .is_some_and(|code| !matches!(code, 124 | 137)),
            "Bash replay timed out or lost its native status",
        )?;
        Ok(output)
    }
}

fn summary_mapping(source: &str) -> TestResult {
    for changed in PRODUCERS {
        for status in [Some(0), Some(101), None] {
            let fixture = Fixture::new()?;
            fixture.evidence(changed, status, 1)?;
            let output = fixture.run(source, SUMMARY_OWNER)?;
            require(
                output.status.success(),
                "summary must not gate advisory verdicts",
            )?;
            let stdout = String::from_utf8(output.stdout)?;
            require(
                stdout == fs::read_to_string(fixture.0.join(SUMMARY_PATH))?
                    && stdout == fs::read_to_string(fixture.0.join("step-summary.md"))?,
                "summary copies differ",
            )?;
            let actual: Vec<_> = stdout
                .lines()
                .filter(|line| line.starts_with("- "))
                .collect();
            let expected: Vec<_> = PRODUCERS
                .iter()
                .map(|producer| {
                    let verdict = if *producer != changed || status == Some(0) {
                        "PASS"
                    } else if status.is_some() {
                        "FAIL (exit 101)"
                    } else {
                        "not_run (no status file)"
                    };
                    format!("- {}: {verdict}", producer.contract().label)
                })
                .collect();
            if actual != expected {
                return Err(format!("summary mapping for {changed:?}/{status:?}: expected {expected:?}, observed {actual:?}").into());
            }
        }
    }
    Ok(())
}

#[test]
fn literal_summary_maps_all_five_producers_and_three_native_status_states() -> TestResult {
    summary_mapping(WORKFLOW)
}

#[test]
fn forced_pass_summary_control_changes_observed_behavior() -> TestResult {
    let error = summary_mapping(&forced_pass_summary()?)
        .err()
        .ok_or("forced-PASS replay unexpectedly met the summary contract")?;
    require(
        error
            .to_string()
            .starts_with("summary mapping for GapArtifacts/Some(101):"),
        "forced-PASS control failed before its intended behavioral witness",
    )?;
    eprintln!("forced-PASS behavior control: {error}");
    Ok(())
}

#[test]
fn omitted_cargo_home_control_drops_a_real_summary_row() -> TestResult {
    let error = summary_mapping(&omitted_cargo_home_summary()?)
        .err()
        .ok_or("missing-producer replay unexpectedly met the summary contract")?;
    require(
        error
            .to_string()
            .starts_with("summary mapping for GapArtifacts/Some(0):"),
        "missing-producer control failed before its intended behavioral witness",
    )?;
    eprintln!("omitted-producer behavior control: {error}");
    Ok(())
}

#[test]
fn literal_guard_rejects_zero_test_success_but_retains_advisory_failure() -> TestResult {
    let fixture = Fixture::new()?;
    require(
        fixture
            .run(WORKFLOW, CARGO_EVIDENCE_OWNER)?
            .status
            .success(),
        "nonempty test evidence must pass capture",
    )?;
    for producer in &PRODUCERS[..3] {
        let fixture = Fixture::new()?;
        fixture.evidence(*producer, Some(0), 0)?;
        let output = fixture.run(WORKFLOW, CARGO_EVIDENCE_OWNER)?;
        require(
            output.status.code() == Some(1),
            "zero-test success did not fail evidence capture",
        )?;
        require(
            String::from_utf8(output.stderr)?.contains(&format!(
                "{}: cargo exited 0 but no test executed",
                producer.contract().label
            )),
            "wrong refusal witness",
        )?;
        fixture.evidence(*producer, Some(101), 0)?;
        require(
            fixture
                .run(WORKFLOW, CARGO_EVIDENCE_OWNER)?
                .status
                .success(),
            "advisory failure must remain capturable",
        )?;
    }
    Ok(())
}

#[test]
fn noop_zero_test_guard_control_exits_success_despite_failure_diagnostic() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.evidence(Producer::CargoHome, Some(0), 0)?;
    let output = fixture.run(&disabled_zero_test_guard()?, CARGO_EVIDENCE_OWNER)?;
    let stderr = String::from_utf8(output.stderr)?;
    require(
        output.status.code() == Some(0),
        "mutation did not reproduce missing failure propagation",
    )?;
    require(
        stderr.contains("subset-cargo-home: cargo exited 0 but no test executed"),
        "mutation lost the intended failure witness",
    )?;
    eprintln!(
        "no-op zero-test behavior control: native exit {:?}; stderr: {stderr}",
        output.status.code()
    );
    Ok(())
}
