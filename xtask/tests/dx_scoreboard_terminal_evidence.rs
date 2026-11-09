//! Execute the real scoreboard and its owned capture/cleanup route against
//! deterministic producer stubs. These are contract controls, not timings.
#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(program: &Path, args: &[&str], cwd: &Path) -> Result<Vec<u8>, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("launch {}: {error}", program.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed: {}",
            program.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

fn run_fixture(
    pilot: &str,
    checks: &str,
    self_delete: bool,
    timeout_ms: u64,
) -> Result<(Fixture, Value), String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "dx-terminal-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let fixture = Fixture(root);
    let source = fixture.0.join("corpus/fixture");
    fs::create_dir_all(source.join("src")).map_err(|error| error.to_string())?;
    let git = |args: &[&str]| command(Path::new("git"), args, &source);
    git(&["init", "--quiet"])?;
    git(&["config", "user.name", "fixture"])?;
    git(&["config", "user.email", "fixture@example.invalid"])?;
    git(&["config", "commit.gpgsign", "false"])?;
    fs::write(source.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n")
        .map_err(|error| error.to_string())?;
    git(&["add", "."])?;
    git(&["commit", "--quiet", "-m", "base"])?;
    fs::write(source.join("src/lib.rs"), "pub fn value() -> u8 { 2 }\n")
        .map_err(|error| error.to_string())?;
    git(&["add", "."])?;
    git(&["commit", "--quiet", "-m", "pin"])?;
    let pin = String::from_utf8(git(&["rev-parse", "HEAD"])?).map_err(|error| error.to_string())?;
    let config = format!(
        r#"schema_version = "ripr-dx-scoreboards-v1"
[[corpus]]
id = "fixture"
url = "unused"
sha = "{}"
[[metric]]
id = "speed.cold_pilot_ms"
board = "speed"
title = "Cold pilot"
unit = "ms"
direction = "lower_is_better"
target = 60000
regression_pct = 0
regression_floor = 0
runner_dependent = true
source = "measured"
per_repo = true
"#,
        pin.trim()
    );
    fs::write(fixture.0.join("scoreboards.toml"), config).map_err(|error| error.to_string())?;
    let script = format!(
        r#"#!/bin/sh
case "$1" in
--version) {} echo 'ripr fixture'; exit 0;;
pilot)
  shift
  while test "$#" -gt 0; do
    if test "$1" = --out; then shift; out=$1; fi
    shift
  done
  mkdir -p "$out"
  echo 'credential-sentinel-do-not-retain' >&2
  echo 'credential-sentinel-do-not-retain'
  echo ran > "$out/../../../pilot-ran"
  printf 'ripr_resource_cost_receipt token=%s {{"schema_version":"0.1","observer":"ripr_process_self","observer_pid":1,"host_os":"linux","host_arch":"x86_64","cpu":{{"state":"unavailable","reason":"platform_not_supported"}},"peak_resident_bytes":{{"state":"unavailable","reason":"platform_not_supported"}}}}\n' "$RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN" >&2
  echo 'ripr_repo_exposure_latency phase=diff_main_index status=built duration_ms=999999' >&2
  printf 'ripr_repo_exposure_latency token=%s phase=unknown-sensitive-phase status=built duration_ms=999998\n' "$RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN" >&2
  printf 'ripr_repo_exposure_latency token=%s phase=diff_main_index status=unknown-sensitive-status duration_ms=999997\n' "$RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN" >&2
  printf 'ripr_repo_exposure_latency token=%s phase=diff_admission_query status=core_files_3 duration_ms=1\n' "$RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN" >&2
  printf 'ripr_repo_exposure_latency token=%s phase=diff_admit_dependents status=main_files_5 duration_ms=1\n' "$RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN" >&2
  i=0
  while test "$i" -lt 20; do
    printf 'ripr_repo_exposure_latency token=%s phase=diff_main_index status=built duration_ms=1\n' "$RIPR_REPO_EXPOSURE_LATENCY_RECEIPT_TOKEN" >&2
    i=$((i+1))
  done
  {}
  ;;
check) {};;
*) exit 0;;
esac
"#,
        if self_delete { "rm -- \"$0\";" } else { "" },
        pilot,
        checks
    );
    let binary = fixture.0.join("producer");
    fs::write(&binary, script).map_err(|error| error.to_string())?;
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let executable = std::env::var_os("RIPR_TEST_XTASK_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_xtask")));
    let timeout = timeout_ms.to_string();
    command(
        &executable,
        &[
            "dx-scoreboard",
            "--config",
            "scoreboards.toml",
            "--boards",
            "speed",
            "--corpus-dir",
            "corpus",
            "--samples",
            "3",
            "--timeout-ms",
            &timeout,
            "--ripr-bin",
            "producer",
        ],
        &fixture.0,
    )?;
    let report = fs::read(fixture.0.join("target/ripr/reports/dx-scoreboard.json"))
        .map_err(|error| error.to_string())?;
    let report: Value = serde_json::from_slice(&report).map_err(|error| error.to_string())?;
    let scratch = fixture.0.join("target/ripr/dx-scoreboard");
    let mut remaining = fs::read_dir(&scratch).map_err(|error| error.to_string())?;
    assert!(
        remaining.next().is_none(),
        "production scratch must be removed before report inspection"
    );
    assert!(
        fs::read_to_string(source.join("src/lib.rs"))
            .map_err(|error| error.to_string())?
            .contains("{ 2 }")
    );
    Ok((fixture, report))
}

fn invocations(report: &Value) -> Result<&Vec<Value>, String> {
    report["corpus"][0]["terminal_evidence"]["invocations"]
        .as_array()
        .ok_or_else(|| "missing ordered terminal evidence after production cleanup".to_string())
}

#[test]
fn exit_zero_partial_reason_survives_real_measurement_cleanup() -> Result<(), String> {
    let (_fixture, report) = run_fixture(
        "printf '%s' '{\"status\":\"partial\",\"reason\":\"timeout\",\"timeout_ms\":270000}' > \"$out/pilot-summary.json\"; exit 0",
        "printf '{}'; exit 0",
        false,
        2_000,
    )?;
    let rows = invocations(&report)?;
    assert_eq!(rows.len(), 5);
    assert_eq!(
        report["corpus"][0]["terminal_evidence"]["binary_sha256_before_pilot"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert_eq!(
        report["corpus"][0]["terminal_evidence"]["root_identity_sha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert_eq!(report["corpus"][0]["terminal_evidence"]["base"], "HEAD~1");

    assert_eq!(rows[0]["capture"], "completed");
    assert_eq!(rows[0]["analysis"], "partial");
    assert_eq!(rows[0]["pilot_summary"]["reason"], "timeout");
    assert_eq!(rows[0]["pilot_summary"]["timeout_ms"], 270000);
    assert_eq!(rows[1]["kind"], "check_warmup");
    assert!(rows[2..].iter().all(|row| row["kind"] == "check_measured"));
    assert_eq!(report["metrics"][0]["samples"][0]["status"], "incomplete");
    assert!(
        report["metrics"][0]["samples"][0]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("reason `timeout`"))
    );
    assert_eq!(report["gate"]["status"], "not_run");
    assert_eq!(rows[0]["streams"]["retention"], "digests_only");
    assert_eq!(
        rows[0]["producer_resource_cost"]["cpu"]["state"],
        "unavailable"
    );
    assert!(
        rows[0]["producer_resource_cost"]["cpu"]
            .get("user_ms")
            .is_none()
    );
    assert_eq!(
        rows[0]["authenticated_phases"].as_array().map(Vec::len),
        Some(14)
    );
    assert_eq!(rows[0]["phase_limit_reached"], true);
    assert_eq!(rows[0]["phase_lines_considered"], 16);
    assert_eq!(rows[0]["phase_rows_withheld"], 2);
    assert_eq!(
        rows[0]["authenticated_phases"][0]["phase"],
        "diff_admission_query"
    );
    assert_eq!(rows[0]["authenticated_phases"][0]["status"], "core_files_3");
    assert_eq!(
        rows[0]["authenticated_phases"][1]["phase"],
        "diff_admit_dependents"
    );
    assert_eq!(rows[0]["authenticated_phases"][1]["status"], "main_files_5");
    assert!(!report.to_string().contains("unknown-sensitive"));
    assert!(
        rows[0]["authenticated_phases"]
            .as_array()
            .is_some_and(|phases| phases.iter().all(|phase| phase["duration_ms"] == 1))
    );

    assert_eq!(
        rows[0]["streams"]["stdout"]["sha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert!(
        !report
            .to_string()
            .contains("credential-sentinel-do-not-retain")
    );
    Ok(())
}

#[test]
fn invalid_utf8_stream_identity_is_labelled_as_the_decoded_text() -> Result<(), String> {
    let (_fixture, report) = run_fixture(
        "printf '\\377' >&2; printf '%s' '{\"status\":\"partial\"}' > \"$out/pilot-summary.json\"; exit 0",
        "printf '{}'; exit 0",
        false,
        2_000,
    )?;
    let streams = &invocations(&report)?[0]["streams"];
    assert_eq!(streams["identity"], "lossy_utf8_decoded");
    // One raw 0xff byte decodes to a three-byte replacement character; the
    // report must not present that count as the emitted byte count.
    assert_eq!(streams["stderr"]["bytes"], 3);
    Ok(())
}

#[test]
fn complete_pilot_retains_existing_admission() -> Result<(), String> {
    let (_fixture, report) = run_fixture(
        "printf '%s' '{\"status\":\"complete\"}' > \"$out/pilot-summary.json\"; exit 0",
        "printf '{}'; exit 0",
        false,
        2_000,
    )?;
    assert_eq!(invocations(&report)?[0]["analysis"], "complete");
    assert_eq!(report["metrics"][0]["samples"][0]["status"], "meets_target");
    Ok(())
}

#[test]
fn failed_exit_preserves_order_and_unstarted_measured_slots() -> Result<(), String> {
    let (_fixture, report) = run_fixture("exit 7", "exit 7", false, 2_000)?;
    let rows = invocations(&report)?;
    assert_eq!(rows.len(), 5);
    for row in &rows[..3] {
        assert_eq!(row["capture"], "failed_exit");
        assert_eq!(row["exit_code"], 7);
    }
    for row in &rows[3..] {
        assert_eq!(row["capture"], "not_run");
        assert!(row["streams"].is_null());
    }
    assert_eq!(report["metrics"][0]["samples"][0]["status"], "incomplete");
    Ok(())
}

#[test]
fn deadline_and_launch_failure_do_not_invent_evidence() -> Result<(), String> {
    let (fixture, timed) = run_fixture("exec sleep 2", "printf '{}'; exit 0", false, 200)?;
    assert!(
        fixture.0.join("target/ripr/pilot-ran").is_file(),
        "producer must run before timeout"
    );
    let row = &invocations(&timed)?[0];
    assert_eq!(row["capture"], "timed_out");
    assert_eq!(row["timed_out"], true);
    assert_ne!(row["analysis"], "complete");
    let (_fixture, failed) = run_fixture("exit 0", "exit 0", true, 2_000)?;
    let rows = invocations(&failed)?;
    assert_eq!(rows[0]["capture"], "failed");
    assert!(rows[0]["streams"].is_null());
    assert_eq!(rows[1]["capture"], "failed");
    assert!(rows[2..].iter().all(|row| row["capture"] == "not_run"));
    Ok(())
}

#[test]
fn unrecognized_summary_reason_is_not_an_output_or_environment_dump() -> Result<(), String> {
    let (_fixture, report) = run_fixture(
        "printf '%s' '{\"status\":\"partial\",\"reason\":\"credential-sentinel-do-not-retain\"}' > \"$out/pilot-summary.json\"; exit 0",
        "printf '{}'; exit 0",
        false,
        2_000,
    )?;
    assert_eq!(
        invocations(&report)?[0]["pilot_summary"]["reason"],
        "unrecognized_reason"
    );
    assert!(
        !report
            .to_string()
            .contains("credential-sentinel-do-not-retain")
    );
    Ok(())
}

#[test]
fn signal_termination_is_not_a_completed_or_failed_exit_capture() -> Result<(), String> {
    let (_fixture, report) = run_fixture("kill -TERM $$", "printf '{}'; exit 0", false, 2_000)?;
    let rows = invocations(&report)?;
    assert_eq!(rows[0]["capture"], "terminated");
    assert!(rows[0]["exit_code"].is_null());
    assert_eq!(rows[0]["timed_out"], false);
    assert_eq!(rows[0]["analysis"], "incomplete");
    Ok(())
}

#[test]
fn complete_nonzero_check_population_is_retained_per_invocation() -> Result<(), String> {
    let check = r#"shift
while test "$#" -gt 0; do
  if test "$1" = --root; then shift; root=$1; fi
  shift
done
printf '{"schema_version":"0.2","tool":"ripr","mode":"draft","root":"%s","base":"HEAD~1","summary":{"findings":1},"findings":[{"id":"p0","classification":"weakly_exposed","related_tests_total":1,"evidence":[]}],"analysis_outcome":{"analysis_complete":true,"outcome":{"schema_version":"0.1","kind":"complete_with_findings","identity":{"base_revision":"HEAD~1"},"counts":{"changed_file_count":1,"changed_line_count":1,"candidate_line_count":1,"probe_count":1,"finding_count":1},"limitations":[],"claim_boundary":"Static analysis outcome only; no correctness, test-adequacy, runtime-execution, or merge-readiness claim."}}}' "$root"
exit 0"#;
    let (_fixture, report) = run_fixture(
        "printf '%s' '{\"status\":\"complete\"}' > \"$out/pilot-summary.json\"; exit 0",
        check,
        false,
        2_000,
    )?;
    let rows = invocations(&report)?;
    assert_eq!(rows.len(), 5);
    for row in &rows[1..] {
        assert_eq!(row["analysis"], "complete");
        assert_eq!(row["check_finding_count"], 1);
    }
    Ok(())
}
