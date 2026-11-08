//! Bounded process observation for one harness row (#4510).

use std::path::Path;
use std::time::Duration;

use super::launch::{CHILD_OUTPUT_BYTES, child_env_for_root, removed_env};
use super::model::HarnessExecutionState;
use crate::run;

pub(crate) struct BoundedProcessObservation {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) duration: Duration,
    pub(crate) timed_out: bool,
    pub(crate) aborted: bool,
    pub(crate) termination: String,
    pub(crate) exit_code: Option<i32>,
}

pub(crate) fn observe_process(
    binary: &Path,
    argv: &[String],
    cwd: &Path,
    deadline: Duration,
    label: &str,
) -> Result<BoundedProcessObservation, String> {
    let envs = child_env_for_root(cwd)?;
    let env_refs: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let removed = removed_env();
    let removed_refs: Vec<&str> = removed.iter().map(String::as_str).collect();
    let timed = run::capture_bytes_in_dir_with_env_budget(
        binary,
        argv,
        cwd,
        (&env_refs, &removed_refs),
        run::ByteCaptureBudget {
            timeout: deadline,
            stdout_bytes: CHILD_OUTPUT_BYTES,
            stderr_bytes: CHILD_OUTPUT_BYTES,
        },
        label,
    )?;
    let (aborted, termination) = match timed.status {
        Some(status) if timed.timed_out => (false, format!("deadline_kill ({status})")),
        Some(status) => match status.code() {
            Some(code) if is_abort_code(code) => (true, format!("exit_code {code} (abort)")),
            Some(code) => (false, format!("exit_code {code}")),
            None => (true, signal_description(&status)),
        },
        None => (false, "unknown".to_string()),
    };
    Ok(BoundedProcessObservation {
        stdout: timed.stdout,
        stderr: timed.stderr,
        duration: timed.duration,
        timed_out: timed.timed_out,
        aborted,
        termination,
        exit_code: timed.status.and_then(|status| status.code()),
    })
}

pub(crate) fn classify_observation(
    observation: &BoundedProcessObservation,
    deadline: Duration,
) -> (HarnessExecutionState, String) {
    if observation.timed_out {
        return (
            HarnessExecutionState::NonterminalDeadline,
            format!("outer deadline of {}s reached", deadline.as_secs()),
        );
    }
    if observation.aborted {
        return (
            HarnessExecutionState::ProcessAbort,
            format!(
                "process ended without a clean exit code ({})",
                observation.termination
            ),
        );
    }
    (
        HarnessExecutionState::Passed,
        format!(
            "process completed within the deadline (exit {:?}, stderr {} bytes, {} ms)",
            observation.exit_code,
            observation.stderr.len(),
            observation.duration.as_millis()
        ),
    )
}

fn is_abort_code(code: i32) -> bool {
    let unsigned = code.cast_unsigned();
    unsigned == 0xC000_00FD || unsigned == 0xC000_0409 || unsigned == 0xC000_0005
}

#[cfg(unix)]
fn signal_description(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.signal() {
        Some(signal) => format!("signal {signal}"),
        None => "no exit code".to_string(),
    }
}

#[cfg(not(unix))]
fn signal_description(_status: &std::process::ExitStatus) -> String {
    "no exit code".to_string()
}
