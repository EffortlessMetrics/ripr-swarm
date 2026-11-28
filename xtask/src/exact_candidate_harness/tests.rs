use std::path::PathBuf;

use super::admission::{admit_installed_binary, version_commit};
#[cfg(unix)]
use super::execution_boundary_controls;
use super::model::{
    Aggregate, BinaryIdentity, CandidateProofControl, CandidateProofPacket, CandidateProofRow,
    HarnessExecutionState, HostIdentity, InputDigest, SCHEMA, SubjectCounts, render_json,
    render_markdown,
};
use super::packet::{aggregate_packet, packet_digest, subject_counts};
use super::{AdmitOptions, ROOT_TOKEN, environment_boundary};

type TestResult = Result<(), String>;

#[test]
fn human_markdown_cannot_strengthen_a_failed_machine_state() -> TestResult {
    let packet = CandidateProofPacket {
        schema: SCHEMA,
        candidate: BinaryIdentity {
            path: "/tmp/ripr".to_string(),
            bytes: 1,
            sha256: "a".repeat(64),
            reported_version: "ripr 0.11.0".to_string(),
            path_resolution: "absolute".to_string(),
        },
        host: HostIdentity {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            family: "unix".to_string(),
        },
        environment_boundary: vec!["scrubbed PATH".to_string()],
        counts: SubjectCounts {
            selected: 1,
            executed: 1,
            passed: 0,
            failed: 1,
            skipped: 0,
        },
        rows: vec![CandidateProofRow {
            id: "row".to_string(),
            state: HarnessExecutionState::SemanticFailure,
            detail: "expected failure".to_string(),
            argv: vec!["check".to_string()],
            cwd: ROOT_TOKEN.to_string(),
            inputs: Vec::new(),
            stdout_sha256: String::new(),
            stderr_sha256: String::new(),
            artifact_sha256: None,
            cleanup: "removed".to_string(),
        }],
        controls: Vec::new(),
        aggregate: Aggregate::Failed,
        aggregate_reasons: vec!["row `row` is semantic_failure".to_string()],
        non_claims: vec!["fixture infrastructure only".to_string()],
        digest: String::new(),
    };
    let md = render_markdown(&packet);
    if md.contains("**passed**") {
        return Err("markdown must not claim pass when aggregate failed".to_string());
    }
    if !md.contains("semantic_failure") {
        return Err("markdown must retain the machine state".to_string());
    }
    Ok(())
}

#[test]
fn json_and_markdown_share_subject_counts() -> TestResult {
    let mut packet = sample_packet();
    packet.digest = packet_digest(&packet)?;
    let json = render_json(&packet)?;
    let md = render_markdown(&packet);
    for token in ["selected", "executed", "passed"] {
        if !json.contains(token) {
            return Err(format!("json missing count token {token}"));
        }
        if !md.contains(token) {
            return Err(format!("markdown missing count token {token}"));
        }
    }
    Ok(())
}

#[test]
fn zero_selected_rows_cannot_aggregate_to_pass() -> TestResult {
    let counts = SubjectCounts {
        selected: 0,
        executed: 0,
        passed: 0,
        failed: 0,
        skipped: 0,
    };
    let (aggregate, reasons) = aggregate_packet(&counts, &[], &[], "removed");
    if aggregate == Aggregate::Passed {
        return Err("zero selected must not pass".to_string());
    }
    if !reasons
        .iter()
        .any(|reason| reason.contains("zero selected"))
    {
        return Err(format!("unexpected reasons: {reasons:?}"));
    }
    Ok(())
}

#[test]
fn zero_executed_rows_cannot_aggregate_to_pass() -> TestResult {
    let rows = vec![CandidateProofRow {
        id: "skipped".to_string(),
        state: HarnessExecutionState::NotRun,
        detail: String::new(),
        argv: Vec::new(),
        cwd: ROOT_TOKEN.to_string(),
        inputs: Vec::new(),
        stdout_sha256: String::new(),
        stderr_sha256: String::new(),
        artifact_sha256: None,
        cleanup: "not_created".to_string(),
    }];
    let counts = subject_counts(&rows);
    let (aggregate, _) = aggregate_packet(&counts, &rows, &[], "removed");
    if aggregate == Aggregate::Passed {
        return Err("zero executed must not pass".to_string());
    }
    Ok(())
}

#[test]
fn wrong_sha256_subject_is_rejected_before_execution() -> TestResult {
    if reject_wrong_sha256("aa".repeat(64).as_str(), "bb".repeat(64).as_str()).is_err() {
        return Err("mismatched digests should pass the guard".to_string());
    }
    if reject_wrong_sha256("cc".repeat(64).as_str(), "cc".repeat(64).as_str()).is_ok() {
        return Err("matching digests must be rejected by the control helper".to_string());
    }
    Ok(())
}

#[test]
fn portable_root_token_preserves_semantic_cwd() -> TestResult {
    let left = CandidateProofRow {
        id: "a".to_string(),
        state: HarnessExecutionState::Passed,
        detail: String::new(),
        argv: Vec::new(),
        cwd: ROOT_TOKEN.to_string(),
        inputs: vec![InputDigest {
            path: "input.txt".to_string(),
            bytes: 3,
            sha256: "ab".repeat(32),
        }],
        stdout_sha256: String::new(),
        stderr_sha256: String::new(),
        artifact_sha256: None,
        cleanup: "removed".to_string(),
    };
    let right = left.clone();
    if left.cwd != right.cwd || left.inputs != right.inputs {
        return Err("portable identities must not depend on temp root spelling".to_string());
    }
    Ok(())
}

#[test]
fn version_commit_parses_parenthesized_version_line() -> TestResult {
    let sha = "c7d6b397c5ea11d121800e32af890856f252d097";
    let line = format!("ripr 0.11.0 ({sha})");
    if version_commit(&line).as_deref() != Some(sha) {
        return Err("expected commit from parenthesized version line".to_string());
    }
    Ok(())
}

#[test]
fn instrument_failure_rows_do_not_count_as_executed() -> TestResult {
    let rows = vec![CandidateProofRow {
        id: "broken".to_string(),
        state: HarnessExecutionState::InstrumentFailure,
        detail: "fixture setup failed".to_string(),
        argv: Vec::new(),
        cwd: ROOT_TOKEN.to_string(),
        inputs: Vec::new(),
        stdout_sha256: String::new(),
        stderr_sha256: String::new(),
        artifact_sha256: None,
        cleanup: "not_created".to_string(),
    }];
    let counts = subject_counts(&rows);
    if counts.executed != 0 {
        return Err(format!(
            "instrument failure must not increment executed; got {:?}",
            counts
        ));
    }
    Ok(())
}

/// A Windows child cannot run a `#!/bin/sh` file, so the fake candidate there is a
/// `.cmd` batch file that prints the same version line.
fn fake_ripr_name() -> &'static str {
    if cfg!(windows) {
        "fake-ripr.cmd"
    } else {
        "fake-ripr"
    }
}

fn fake_ripr_script(version_line: &str) -> Vec<u8> {
    if cfg!(windows) {
        format!("@echo off\r\necho {version_line}\r\n").into_bytes()
    } else {
        format!("#!/bin/sh\necho \"{version_line}\"\n").into_bytes()
    }
}

#[test]
fn admit_options_reject_version_mismatch() -> TestResult {
    let dir = tempfile_root("admit-version")?;
    let script = dir.join(fake_ripr_name());
    std::fs::write(&script, fake_ripr_script("ripr 0.10.0")).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    let options = AdmitOptions {
        expected_version: Some("0.11.0".to_string()),
        ..AdmitOptions::default()
    };
    if admit_installed_binary(&script, &options).is_ok() {
        return Err("wrong version string must fail admission".to_string());
    }
    let commit = "c7d6b397c5ea11d121800e32af890856f252d097";
    std::fs::write(
        &script,
        fake_ripr_script(&format!("ripr 0.11.0 ({commit})")),
    )
    .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    let options = AdmitOptions {
        expected_commit: Some(commit.to_string()),
        ..AdmitOptions::default()
    };
    if admit_installed_binary(&script, &options).is_err() {
        return Err(
            "parenthesized commit in version line must satisfy expected_commit".to_string(),
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_boundary_controls_discriminate_on_unix() -> TestResult {
    let work = tempfile_root("execution-controls")?;
    let controls = execution_boundary_controls(&work);
    let required = [
        "execution-scrubbed-path",
        "execution-owned-cwd",
        "execution-decoy-repository",
        "execution-snapshot-isolation",
    ];
    for id in required {
        let Some(control) = controls.iter().find(|control| control.id == id) else {
            return Err(format!("missing control {id}"));
        };
        if !control.discriminated {
            return Err(format!(
                "control {id} did not discriminate: {:?}",
                control.detail
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_controls_retain_owned_root_setup_error() -> TestResult {
    let work = tempfile_root("control-setup-error")?;
    std::fs::create_dir_all(work.join("path")).map_err(|error| error.to_string())?;
    let controls = execution_boundary_controls(&work);
    let item = controls
        .iter()
        .find(|item| item.id == "execution-scrubbed-path")
        .ok_or("missing PATH control")?;
    assert!(!item.discriminated);
    assert!(
        item.detail.contains("owned fixture root"),
        "{}",
        item.detail
    );
    assert!(item.detail.contains("already exists"), "{}", item.detail);
    assert!(item.detail.contains("path"), "{}", item.detail);
    Ok(())
}

fn reject_wrong_sha256(admitted: &str, supplied: &str) -> Result<(), String> {
    if admitted.eq_ignore_ascii_case(supplied) {
        Err("expected digest mismatch for wrong-subject control".to_string())
    } else {
        Ok(())
    }
}

fn sample_packet() -> CandidateProofPacket {
    CandidateProofPacket {
        schema: SCHEMA,
        candidate: BinaryIdentity {
            path: "/tmp/ripr".to_string(),
            bytes: 10,
            sha256: "d".repeat(64),
            reported_version: "ripr 0.11.0".to_string(),
            path_resolution: "absolute".to_string(),
        },
        host: HostIdentity {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            family: std::env::consts::FAMILY.to_string(),
        },
        environment_boundary: environment_boundary(),
        counts: SubjectCounts {
            selected: 2,
            executed: 2,
            passed: 2,
            failed: 0,
            skipped: 0,
        },
        rows: vec![
            CandidateProofRow {
                id: "one".to_string(),
                state: HarnessExecutionState::Passed,
                detail: String::new(),
                argv: Vec::new(),
                cwd: ROOT_TOKEN.to_string(),
                inputs: Vec::new(),
                stdout_sha256: String::new(),
                stderr_sha256: String::new(),
                artifact_sha256: None,
                cleanup: "removed".to_string(),
            },
            CandidateProofRow {
                id: "two".to_string(),
                state: HarnessExecutionState::Passed,
                detail: String::new(),
                argv: Vec::new(),
                cwd: ROOT_TOKEN.to_string(),
                inputs: Vec::new(),
                stdout_sha256: String::new(),
                stderr_sha256: String::new(),
                artifact_sha256: None,
                cleanup: "removed".to_string(),
            },
        ],
        controls: vec![CandidateProofControl {
            id: "execution-scrubbed-path".to_string(),
            expected: "scrubbed".to_string(),
            observed: Some("scrubbed".to_string()),
            discriminated: true,
            detail: String::new(),
        }],
        aggregate: Aggregate::Passed,
        aggregate_reasons: Vec::new(),
        non_claims: vec!["does not qualify release".to_string()],
        digest: String::new(),
    }
}

fn tempfile_root(label: &str) -> Result<PathBuf, String> {
    let base = std::env::temp_dir().join(format!("ripr-4510-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&base).map_err(|error| error.to_string())?;
    Ok(base)
}
#[cfg(unix)]
fn diagnostic_probe(label: &str, script: Option<&str>) -> Result<CandidateProofControl, String> {
    use std::os::unix::fs::PermissionsExt;
    let work = tempfile_root(label)?;
    let binary = work.join("diagnostic-probe");
    if let Some(script) = script {
        std::fs::write(&binary, script).map_err(|error| error.to_string())?;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    let mut item = CandidateProofControl {
        id: "execution-diagnostic-test".to_string(),
        expected: "expected stdout and a clean child exit".to_string(),
        observed: None,
        discriminated: false,
        detail: String::new(),
    };
    super::controls::probe_control(
        &mut item,
        &work,
        Ok(binary),
        "probe",
        |stdout| stdout == "expected\n",
        ("matched", "mismatched"),
    );
    Ok(item)
}

#[cfg(unix)]
fn assert_control_keeps_packet_failed(item: CandidateProofControl) {
    assert!(!item.discriminated, "failed probe became green: {item:?}");
    let packet = sample_packet();
    let (aggregate, reasons) = aggregate_packet(&packet.counts, &packet.rows, &[item], "removed");
    assert_eq!(aggregate, Aggregate::Failed);
    assert!(
        reasons
            .iter()
            .any(|reason| reason.contains("did not discriminate"))
    );
}

#[cfg(unix)]
#[test]
fn execution_controls_retain_launch_error_without_child_observations() -> TestResult {
    let item = diagnostic_probe("control-launch-error", None)?;
    assert!(item.observed.is_none());
    assert!(
        item.detail.contains("launch/capture probe"),
        "{}",
        item.detail
    );
    assert!(item.detail.contains("diagnostic-probe"), "{}", item.detail);
    assert!(
        item.detail.contains("No such file or directory"),
        "{}",
        item.detail
    );
    assert!(!item.detail.contains("exit_code="), "{}", item.detail);
    assert_control_keeps_packet_failed(item);
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_controls_retain_empty_stdout_and_child_failure() -> TestResult {
    let item = diagnostic_probe(
        "control-child-error",
        Some("#!/bin/sh\nprintf 'diagnostic: child failed\\n' >&2\nexit 17\n"),
    )?;
    for required in [
        "state=passed; exit_code=Some(17)",
        "timed_out=false; aborted=false",
        "termination=\"exit_code 17\"",
        "stdout=\"\"",
        "stderr=\"diagnostic: child failed\\n\"",
    ] {
        assert!(
            item.detail.contains(required),
            "missing {required}: {}",
            item.detail
        );
    }
    assert_eq!(item.observed.as_deref(), Some("process_failed"));
    assert_control_keeps_packet_failed(item);
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_controls_retain_bounded_escaped_mismatch_streams() -> TestResult {
    let payload = "\u{1b}\u{202e}".repeat(3000);
    let script = format!("#!/bin/sh\nprintf '%s' '{payload}'\nprintf '%s' '{payload}' >&2\n");
    let item = diagnostic_probe("control-mismatch", Some(&script))?;
    assert_eq!(item.observed.as_deref(), Some("mismatched"));
    assert!(item.detail.contains("exit_code=Some(0)"), "{}", item.detail);
    assert!(
        item.detail.contains("stdout=\"\\u{1b}\\u{202e}"),
        "{}",
        item.detail
    );
    assert!(
        item.detail
            .contains("...[truncated]\"; stderr=\"\\u{1b}\\u{202e}"),
        "{}",
        item.detail
    );
    assert_eq!(item.detail.matches("...[truncated]").count(), 2);
    assert!(item.detail.is_ascii());
    assert!(!item.detail.chars().any(char::is_control));
    assert!(
        item.detail.len() < 4600,
        "unbounded diagnostic: {} bytes",
        item.detail.len()
    );
    assert_control_keeps_packet_failed(item);
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_controls_require_clean_exit_even_when_stdout_matches() -> TestResult {
    let failure = diagnostic_probe(
        "control-matching-failure",
        Some("#!/bin/sh\nprintf 'expected\\n'\nexit 17\n"),
    )?;
    assert_eq!(failure.observed.as_deref(), Some("process_failed"));
    assert!(
        failure.detail.contains("stdout=\"expected\\n\""),
        "{}",
        failure.detail
    );
    assert_control_keeps_packet_failed(failure);
    let success = diagnostic_probe(
        "control-matching-success",
        Some("#!/bin/sh\nprintf 'expected\\n'\n"),
    )?;
    assert!(success.discriminated, "{}", success.detail);
    assert_eq!(success.observed.as_deref(), Some("matched"));
    assert!(success.detail.is_empty());
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_controls_retain_signal_termination() -> TestResult {
    let item = diagnostic_probe(
        "control-signal",
        Some("#!/bin/sh\nprintf 'before signal\\n' >&2\nkill -TERM $$\n"),
    )?;
    assert!(
        item.detail.contains("state=process_abort; exit_code=None"),
        "{}",
        item.detail
    );
    assert!(item.detail.contains("aborted=true"), "{}", item.detail);
    assert!(item.detail.contains("signal 15"), "{}", item.detail);
    assert!(
        item.detail.contains("stderr=\"before signal\\n\""),
        "{}",
        item.detail
    );
    assert_control_keeps_packet_failed(item);
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_controls_retain_bounded_prepare_error() -> TestResult {
    let work = tempfile_root("control-prepare-error")?;
    std::fs::write(work.join("stubs"), b"not a directory").map_err(|error| error.to_string())?;
    for item in execution_boundary_controls(&work) {
        assert!(
            item.detail
                .contains("prepare probe: create probe directory"),
            "{}",
            item.detail
        );
        assert!(item.detail.contains("stubs"), "{}", item.detail);
        assert!(item.detail.len() <= 2048);
        assert_control_keeps_packet_failed(item);
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_controls_tokenize_both_failure_streams() -> TestResult {
    let item = diagnostic_probe(
        "control-root-diagnostic",
        Some("#!/bin/sh\npwd -P\npwd -P >&2\n"),
    )?;
    assert!(
        item.detail.contains("stdout=\"<case-root>\\n\""),
        "{}",
        item.detail
    );
    assert!(
        item.detail.contains("stderr=\"<case-root>\\n\""),
        "{}",
        item.detail
    );
    assert_control_keeps_packet_failed(item);
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_controls_scrub_unusual_and_long_setup_paths_before_escaping() -> TestResult {
    let base = tempfile_root("control-root-diagnostics")?;
    let unusual = base.join("quoted-\"-backslash-\\-unicode-é");
    std::fs::create_dir_all(unusual.join("path")).map_err(|error| error.to_string())?;
    let controls = execution_boundary_controls(&unusual);
    let item = controls
        .iter()
        .find(|item| item.id == "execution-scrubbed-path")
        .ok_or("missing PATH control")?;
    assert_eq!(
        item.detail,
        "prepare owned root: owned fixture root <work-root>/path already exists; refusing to reuse it"
    );
    assert!(!item.discriminated);

    // Exercise the actual prepare-error consumer without assuming the host's
    // filesystem supports paths longer than the diagnostic budget.
    let long = (0..12).fold(base.join("long"), |root, _| root.join("x".repeat(200)));
    let mut failed = sample_packet()
        .controls
        .pop()
        .ok_or("missing sample control")?;
    super::controls::probe_control(
        &mut failed,
        &long,
        Err(format!(
            "create probe directory {}/stubs: original OS cause",
            long.display()
        )),
        "probe",
        |_| true,
        ("matched", "mismatched"),
    );
    assert_eq!(
        failed.detail,
        "prepare probe: create probe directory <work-root>/stubs: original OS cause"
    );
    assert_control_keeps_packet_failed(failed);
    Ok(())
}

#[cfg(unix)]
#[test]
fn execution_control_diagnostics_keep_elapsed_time_out_of_semantic_digest() -> TestResult {
    let immediate = diagnostic_probe(
        "control-digest-immediate",
        Some("#!/bin/sh\nprintf 'diagnostic\\n' >&2\nexit 17\n"),
    )?;
    let delayed = diagnostic_probe(
        "control-digest-delayed",
        Some("#!/bin/sh\nsleep 0.2\nprintf 'diagnostic\\n' >&2\nexit 17\n"),
    )?;
    assert!(!immediate.discriminated && !delayed.discriminated);
    assert_eq!(immediate.detail, delayed.detail);
    let mut packet = sample_packet();
    packet.controls = vec![immediate];
    let first = packet_digest(&packet)?;
    packet.controls = vec![delayed];
    assert_eq!(first, packet_digest(&packet)?);
    Ok(())
}
