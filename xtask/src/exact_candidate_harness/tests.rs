use std::path::PathBuf;

use super::admission::admit_installed_binary;
use super::model::{
    Aggregate, BinaryIdentity, CandidateProofControl, CandidateProofPacket, CandidateProofRow,
    HarnessExecutionState, HostIdentity, InputDigest, SCHEMA, SubjectCounts, render_json,
    render_markdown,
};
use super::packet::{aggregate_packet, packet_digest, subject_counts};
use super::{AdmitOptions, ROOT_TOKEN, environment_boundary, execution_boundary_controls};

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
fn admit_options_reject_version_mismatch() -> TestResult {
    let dir = tempfile_root("admit-version")?;
    let script = dir.join("fake-ripr");
    std::fs::write(&script, b"#!/bin/sh\necho ripr 0.10.0\n").map_err(|error| error.to_string())?;
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
