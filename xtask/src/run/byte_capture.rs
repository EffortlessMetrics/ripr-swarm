//! One owned byte-capture implementation and its local execution capability.
//!
//! The capability is deliberately neither constructible by sibling modules nor
//! deserializable. Retained JSON and byte files are declarations; they cannot
//! recreate the fact that this process owner observed a particular invocation.
//! This boundary does not authenticate the executable, inherited configuration,
//! protected runner, repository currentness, or any formal proof claim.

use super::{
    ByteCaptureBudget, OwnedProcess, POST_KILL_DRAIN_GRACE, TimedBytesOutput, TimedCaptureError,
    TimedCaptureErrorKind, drain_byte_reader_bounded, end_tree_after_normal_primary_exit,
    spawn_byte_reader_channel, wait_for_child_with_deadline,
};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The request that configured the actual command, not a resolved tool identity.
/// Environment entries are the command's effective explicit overrides/removals.
/// Neither inherited variables nor file-based configuration are included.
/// No `Debug`/serialization implementation may accidentally publish their values.
pub(crate) struct RequestedInvocation {
    pub(crate) program: PathBuf,
    pub(crate) arguments: Vec<OsString>,
    pub(crate) cwd: PathBuf,
    pub(crate) explicit_environment: Vec<(OsString, Option<OsString>)>,
}

impl RequestedInvocation {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.arguments).current_dir(&self.cwd);
        for (name, value) in &self.explicit_environment {
            match value {
                Some(value) => command.env(name, value),
                None => command.env_remove(name),
            };
        }
        command
    }
}

/// Exact local cleanup evidence, restricted to the owner's containment scope.
/// A process that deliberately escaped that scope is not covered by these facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ByteCaptureCleanup {
    /// Prereap group settlement followed by a complete, read-only Linux scan.
    #[cfg(target_os = "linux")]
    LinuxGroupObservedEmpty,
    /// The retained Windows Job Object accepted termination and primary reaping.
    #[cfg(windows)]
    WindowsJobTerminated,
    /// Prereap settlement succeeded; this platform has no post-settlement census.
    #[cfg(all(unix, not(target_os = "linux")))]
    UnixGroupSettlementRequested,
}

/// A successful, bounded, complete byte capture from the actual owned process.
/// Fields and construction are private to this module; there is no DTO adapter
/// in this direction. Nonzero exits and enforced timeouts remain actual outcomes,
/// never proof success. Failed/over-budget drains do not produce this capability.
pub(crate) struct ObservedByteCapture {
    invocation: RequestedInvocation,
    output: TimedBytesOutput,
    budget: ByteCaptureBudget,
    cleanup: ByteCaptureCleanup,
    elapsed: Duration,
}

impl ObservedByteCapture {
    /// Consume the local observation exactly once. The callback receives the
    /// request, exact output/native status, enforced limits, cleanup disposition,
    /// and total local elapsed time (including drains/cleanup), respectively.
    /// Any persisted representation the caller creates is ordinary replay data.
    pub(crate) fn consume<R>(
        self,
        consumer: impl FnOnce(
            RequestedInvocation,
            TimedBytesOutput,
            ByteCaptureBudget,
            ByteCaptureCleanup,
            Duration,
        ) -> R,
    ) -> R {
        consumer(
            self.invocation,
            self.output,
            self.budget,
            self.cleanup,
            self.elapsed,
        )
    }
}

struct CompletedCapture {
    invocation: RequestedInvocation,
    output: TimedBytesOutput,
    cleanup: ByteCaptureCleanup,
    elapsed: Duration,
}

pub(super) fn capture_observed(
    program: &Path,
    args: &[String],
    cwd: &Path,
    envs: (&[(&str, &str)], &[&str]),
    budget: ByteCaptureBudget,
    error_context: &str,
) -> Result<ObservedByteCapture, TimedCaptureError> {
    let captured = capture(
        program,
        args,
        (cwd, None),
        envs,
        (budget.timeout, Some(budget)),
        error_context,
    )?;
    Ok(ObservedByteCapture {
        invocation: captured.invocation,
        output: captured.output,
        budget,
        cleanup: captured.cleanup,
        elapsed: captured.elapsed,
    })
}

pub(super) fn capture_legacy(
    program: &Path,
    args: &[String],
    source: (&Path, Option<&[u8]>),
    envs: (&[(&str, &str)], &[&str]),
    deadline: (Duration, Option<ByteCaptureBudget>),
    error_context: &str,
) -> Result<TimedBytesOutput, TimedCaptureError> {
    // Legacy callers retain their data contract, including the uncapped drain
    // diagnostic. That diagnostic is never wrapped in ObservedByteCapture.
    capture(program, args, source, envs, deadline, error_context).map(|value| value.output)
}

fn capture(
    program: &Path,
    args: &[String],
    source: (&Path, Option<&[u8]>),
    envs: (&[(&str, &str)], &[&str]),
    deadline: (Duration, Option<ByteCaptureBudget>),
    error_context: &str,
) -> Result<CompletedCapture, TimedCaptureError> {
    let (cwd, input) = source;
    let (timeout, budget) = deadline;
    if timeout.is_zero() {
        return Err(failure(
            TimedCaptureErrorKind::ProcessControl,
            format!("{error_context}: byte capture requires a nonzero deadline"),
        ));
    }
    let started = Instant::now();
    let (overrides, removals) = envs;
    let mut invocation = RequestedInvocation {
        program: program.to_path_buf(),
        arguments: args.iter().map(OsString::from).collect(),
        cwd: cwd.to_path_buf(),
        explicit_environment: removals
            .iter()
            .map(|name| (OsString::from(name), None))
            .chain(
                overrides
                    .iter()
                    .map(|(name, value)| (OsString::from(name), Some(OsString::from(value)))),
            )
            .collect(),
    };
    let mut command = invocation.command();
    // Record the effective explicit entries after Command applies duplicate and
    // platform environment-key semantics; do not read or publish inherited env.
    invocation.explicit_environment = command
        .get_envs()
        .map(|(name, value)| (name.to_os_string(), value.map(OsString::from)))
        .collect();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    // A prepared process_group(0) alone does not establish prereap custody.
    // Explicit isolation uses the existing owner's one settlement mechanism.
    let mut child = OwnedProcess::spawn_isolated(command).map_err(|error| {
        failure(
            TimedCaptureErrorKind::Launch(error.kind()),
            format!("failed to run {error_context}: {error}"),
        )
    })?;
    let stdout = child.stdout_pipe().take().ok_or_else(|| {
        failure(
            TimedCaptureErrorKind::Capture,
            format!("failed to capture stdout for {error_context}"),
        )
    })?;
    let stderr = child.stderr_pipe().take().ok_or_else(|| {
        failure(
            TimedCaptureErrorKind::Capture,
            format!("failed to capture stderr for {error_context}"),
        )
    })?;
    let (stdout_handle, stdout_rx) =
        spawn_byte_reader_channel(stdout, budget.map(|value| ("stdout", value.stdout_bytes)));
    let (stderr_handle, stderr_rx) =
        spawn_byte_reader_channel(stderr, budget.map(|value| ("stderr", value.stderr_bytes)));
    let input_completion = start_input_writer(&mut child, input, error_context)?;
    let waited = wait_for_child_with_deadline(&mut child, started, Some(timeout), error_context)
        .map_err(|message| failure(TimedCaptureErrorKind::ProcessControl, message))?;
    let cleanup = observe_cleanup(&mut child, waited.timed_out, error_context);
    // Both drains run after containment settlement. Preserve cleanup failure
    // ahead of a simultaneous pipe failure, and never turn exit zero into EOF.
    let stdout = drain_byte_reader_bounded(
        stdout_rx,
        stdout_handle,
        POST_KILL_DRAIN_GRACE,
        "stdout",
        error_context,
        budget.is_some(),
    );
    let stderr = drain_byte_reader_bounded(
        stderr_rx,
        stderr_handle,
        POST_KILL_DRAIN_GRACE,
        "stderr",
        error_context,
        budget.is_some(),
    );
    let input_result = finish_input(input_completion, waited.timed_out, error_context);
    let cleanup = cleanup.map_err(|message| failure(TimedCaptureErrorKind::Cleanup, message))?;
    let stdout = stdout.map_err(|message| failure(TimedCaptureErrorKind::Capture, message))?;
    let stderr = stderr.map_err(|message| failure(TimedCaptureErrorKind::Capture, message))?;
    input_result?;
    Ok(CompletedCapture {
        invocation,
        output: TimedBytesOutput {
            status: Some(waited.status),
            stdout,
            stderr,
            duration: waited.duration,
            timed_out: waited.timed_out,
        },
        cleanup,
        elapsed: started.elapsed(),
    })
}

fn failure(kind: TimedCaptureErrorKind, message: String) -> TimedCaptureError {
    TimedCaptureError::new(kind, message)
}

fn observe_cleanup(
    child: &mut OwnedProcess,
    timed_out: bool,
    error_context: &str,
) -> Result<ByteCaptureCleanup, String> {
    end_tree_after_normal_primary_exit(child, timed_out)?;
    #[cfg(target_os = "linux")]
    {
        super::confirm_timed_process_group_gone(child.id(), error_context)?;
        Ok(ByteCaptureCleanup::LinuxGroupObservedEmpty)
    }
    #[cfg(windows)]
    {
        let _ = error_context;
        Ok(ByteCaptureCleanup::WindowsJobTerminated)
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        let _ = error_context;
        Ok(ByteCaptureCleanup::UnixGroupSettlementRequested)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(format!("{error_context}: owned containment is unavailable"))
    }
}

fn start_input_writer(
    child: &mut OwnedProcess,
    input: Option<&[u8]>,
    error_context: &str,
) -> Result<Option<mpsc::Receiver<std::io::Result<()>>>, TimedCaptureError> {
    let Some(bytes) = input else {
        return Ok(None);
    };
    let mut stdin = child.stdin_pipe().take().ok_or_else(|| {
        failure(
            TimedCaptureErrorKind::Capture,
            format!("failed to capture stdin for {error_context}"),
        )
    })?;
    let bytes = bytes.to_vec();
    let (sender, receiver) = mpsc::channel();
    let _input_writer = thread::Builder::new()
        .name("bounded-process-stdin".to_string())
        .spawn(move || {
            let result = stdin.write_all(&bytes);
            drop(stdin);
            let _ = sender.send(result);
        })
        .map_err(|error| {
            failure(
                TimedCaptureErrorKind::Capture,
                format!("start stdin writer for {error_context}: {error}"),
            )
        })?;
    Ok(Some(receiver))
}

fn finish_input(
    completion: Option<mpsc::Receiver<std::io::Result<()>>>,
    timed_out: bool,
    error_context: &str,
) -> Result<(), TimedCaptureError> {
    let Some(receiver) = completion else {
        return Ok(());
    };
    let result = receiver
        .recv_timeout(POST_KILL_DRAIN_GRACE)
        .map_err(|error| {
            failure(
                TimedCaptureErrorKind::Capture,
                format!("stdin completion for {error_context}: {error}"),
            )
        })?;
    if let Err(error) = result {
        // An enforced timeout can close the input pipe during a blocked write.
        // Early exit and every other writer failure still refuse the capture.
        if !timed_out || error.kind() != std::io::ErrorKind::BrokenPipe {
            return Err(failure(
                TimedCaptureErrorKind::Capture,
                format!("write stdin for {error_context}: {error}"),
            ));
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
#[path = "byte_capture/tests.rs"]
mod tests;
