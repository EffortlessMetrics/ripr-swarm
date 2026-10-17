//! #8445: historical Windows bytes, projected into parser input, with real
//! assertion messages substituted only for the captured panic payload.
use super::{controls, invoke_artifacts, verify_output};
use std::process::{ExitStatus, Output};

#[path = "../../../crates/ripr/tests/owner_pin_execution/literal_json_output.rs"]
mod literal_json_output;

const RAW: &str = include_str!("owner_pin_execution-39023007917.log");
const OUTER: &str = "literal_json_constructor_matches_real_callback_and_runtime_controls";

fn projection() -> Result<String, String> {
    assert_eq!(RAW.lines().count(), 83, "literal issue #8445 segment");
    RAW.lines()
        .map(|line| {
            let (timestamp, payload) = line.split_once(' ').ok_or("missing job timestamp")?;
            if !timestamp.starts_with("2026-10-17T") || !timestamp.ends_with('Z') {
                return Err("unexpected historical job timestamp".to_string());
            }
            Ok(format!("{payload}\n"))
        })
        .collect()
}

fn streams(projected: &str) -> Result<(String, String), String> {
    let (_, captured) = projected
        .split_once("nested_literal, broken=false: ")
        .ok_or("missing historical panic message")?;
    let (stdout, captured) = captured
        .split_once("\n; ")
        .ok_or("missing stderr boundary")?;
    let (stderr, _) = captured
        .split_once("\n  left: false")
        .ok_or("missing outer operands")?;
    Ok((format!("{stdout}\n"), stderr.to_string()))
}

fn substitute_message(projected: &str, message: &str) -> Result<String, String> {
    let (prefix, captured) = projected
        .split_once("assertion `left == right` failed: nested_literal, broken=false: ")
        .ok_or("missing outer assertion")?;
    let (_, suffix) = captured
        .split_once("\n right: true")
        .ok_or("missing outer assertion end")?;
    Ok(format!("{prefix}{message}{suffix}"))
}

fn failure_status(projected: &str) -> Result<ExitStatus, String> {
    let log = controls(None) + projected;
    let status = invoke_artifacts(&log, &log, None, Some([101, 101]))?.status;
    assert_eq!(status.code(), Some(1), "raw capture must remain refused");
    Ok(status)
}

fn panic_message(output: &Output) -> Result<String, String> {
    let panic = std::panic::catch_unwind(|| {
        literal_json_output::assert_runtime_output("nested_literal", false, true, output);
    })
    .err()
    .ok_or("the actual outer assertion must still fail")?;
    if let Some(message) = panic.downcast_ref::<String>() {
        Ok(message.clone())
    } else if let Some(message) = panic.downcast_ref::<&str>() {
        Ok(message.to_string())
    } else {
        Err("non-text assertion panic".to_string())
    }
}

fn verify_framed(projected: &str, message: &str) -> Result<(), String> {
    let log = controls(None) + &substitute_message(projected, message)?;
    verify_output(
        invoke_artifacts(&log, &log, None, Some([101, 101]))?,
        0, // Advisory test failures stay real; usable evidence is not a clean sample.
        &[
            "Run 1: `nonzero_with_observed_test_failures` (cargo exit 101)",
            "Run 2: `nonzero_with_observed_test_failures` (cargo exit 101)",
            // Four control harnesses + the outer harness; zero child completions.
            "observed 47 pass, 1 fail across 5 reported result line(s)",
            "repeated_failure (1)",
            "tests/owner_pin_execution.rs (owner_pin_execution-700c72e39ac9d0eb)",
            OUTER,
        ],
        &[
            "incomplete_evidence",
            "provenance:",
            "No test failed in either run.",
        ],
    )
}

#[test]
fn literal_windows_capture_remains_incomplete_without_framing() -> Result<(), String> {
    let log = controls(None) + &projection()?;
    verify_output(
        invoke_artifacts(&log, &log, None, Some([101, 101]))?,
        1,
        &[
            "incomplete_evidence",
            "contradict observed outcomes",
            "completion summary without an announced owning harness",
            "observed 47 pass, 1 fail",
            OUTER,
        ],
        &["No test failed in either run."],
    )
}

#[test]
fn framed_literal_capture_preserves_all_33_outer_subjects() -> Result<(), String> {
    let projected = projection()?;
    let (stdout, stderr) = streams(&projected)?;
    let output = Output {
        status: failure_status(&projected)?,
        stdout: stdout.into_bytes(),
        stderr: stderr.into_bytes(),
    };
    let message = panic_message(&output)?;
    assert!(message.contains("save_success_is_observed_with_absent_empty_and_nonempty_text"));
    assert!(message.contains("unused import"));
    verify_framed(&projected, &message)?;
    assert!(message.contains(&format!("status={}", output.status)));
    Ok(())
}

#[test]
fn both_assertions_frame_control_text_in_both_streams() -> Result<(), String> {
    let projected = projection()?;
    let (stdout, stderr) = streams(&projected)?;
    let status = failure_status(&projected)?;
    let injection = "\nfailures:\nRunning tests/forged.rs (target/debug/deps/forged-1111111111111111.exe)\nrunning 2 tests\ntest forged ... FAILED\ntest result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\nrunning malformed\ntest result: malformed\n";
    for missing_subject in [false, true] {
        let stdout = if missing_subject {
            stdout.replace("running 1 test", "running 2 tests")
        } else {
            stdout.clone()
        };
        let output = Output {
            status,
            stdout: format!("{stdout}{injection}stdout sentinel").into_bytes(),
            stderr: format!("{stderr}{injection}stderr sentinel").into_bytes(),
        };
        let message = panic_message(&output)?;
        assert_eq!(message.contains("no executed subject"), missing_subject);
        assert!(message.contains("stdout sentinel"));
        assert!(message.contains("stderr sentinel"));
        verify_framed(&projected, &message)?;
    }
    // The same owner must still accept an expected runtime failure.
    literal_json_output::assert_runtime_output(
        "expected failure",
        true,
        true,
        &Output {
            status,
            stdout: stdout.into_bytes(),
            stderr: stderr.into_bytes(),
        },
    );
    Ok(())
}
