use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use ripr::process_owner::OwnedProcess;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[path = "run/byte_capture.rs"]
pub(crate) mod byte_capture;
pub(crate) use byte_capture::ObservedByteCapture;
#[path = "run/command_diagnostic.rs"]
mod command_diagnostic;
#[cfg(any(target_os = "linux", test))]
#[path = "run/proc_stat.rs"]
mod proc_stat;
#[cfg(test)]
use proc_stat::PROC_STAT_ESRCH;
#[cfg(any(target_os = "linux", test))]
use proc_stat::proc_stat_error_is_gone;
#[cfg(all(test, target_os = "linux"))]
#[path = "run/test_cleanup.rs"]
mod test_cleanup;
#[cfg(all(test, target_os = "linux"))]
#[path = "run/test_session.rs"]
mod test_session;
pub(crate) use command_diagnostic::{
    CommandDiagnostic, DISCLOSURE as COMMAND_DIAGNOSTIC_DISCLOSURE,
};
#[cfg(all(test, target_os = "linux", not(feature = "automation")))]
pub(crate) use test_session::custody;

/// Grace period for draining stdout/stderr pipes after a process-group kill.
///
/// After `terminate_timed_process_tree` fires, any descendant that escaped the
/// group-kill may still hold the inherited pipe write-end open, keeping
/// `read_to_end` blocked. This constant caps how long we wait for the pipes to
/// drain before giving up and returning whatever was captured so far, plus a
/// diagnostic note in the output.  Five seconds is generous relative to the
/// typical `~100 ms` group-kill propagation delay while still being a hard
/// upper bound.
const POST_KILL_DRAIN_GRACE: Duration = Duration::from_secs(5);

/// Budget for confirming that a timed-out Unix process group has no live
/// members (#5382).
///
/// A recorded `timed_out` result is not proof that descendants died: the first
/// `kill -KILL -- -<pgid>` can race a fork (GNU `time` starting `ripr` is the
/// motivating case). After reaping the direct child, the timeout path polls
/// until a read-only observation finds the group empty, and refuses to return
/// `timed_out: true` if the observation is incomplete or still populated.
/// Reaping releases numeric identity custody: confirmation must never signal
/// that number again. Two seconds bounds observation, not another kill attempt.
#[cfg(unix)]
const POST_KILL_GROUP_CONFIRM_GRACE: Duration = Duration::from_secs(2);

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub(crate) struct CapturedOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ProcessErrorKind {
    Launch,
    Exit,
}

#[derive(Debug)]
pub(crate) struct ProcessError {
    pub(crate) kind: ProcessErrorKind,
    pub(crate) message: String,
}

pub(crate) fn capture_process_output(
    program: &str,
    args: &[String],
    envs: &[(&str, &str)],
) -> Result<Vec<u8>, ProcessError> {
    capture_process_output_isolated(program, args, &[], &[], envs)
}

pub(crate) fn capture_process_output_isolated(
    program: &str,
    args: &[String],
    inherited_envs: &[(&str, &str)],
    removed_envs: &[&str],
    envs: &[(&str, &str)],
) -> Result<Vec<u8>, ProcessError> {
    capture_process_output_in(program, args, None, inherited_envs, removed_envs, envs)
}

/// `capture_process_output_isolated` with an explicit working directory for the
/// child.
///
/// A caller that needs the child to resolve relative paths from somewhere other
/// than the xtask process's own directory uses this. It sets the directory on
/// the child only: `std::env::set_current_dir` is process-wide and the fixture
/// runs are rayon-parallel, so mutating the xtask process's directory would
/// race every sibling run.
pub(crate) fn capture_process_output_in(
    program: &str,
    args: &[String],
    current_dir: Option<&Path>,
    inherited_envs: &[(&str, &str)],
    removed_envs: &[&str],
    envs: &[(&str, &str)],
) -> Result<Vec<u8>, ProcessError> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    for (name, value) in inherited_envs {
        command.env(name, value);
    }
    for name in removed_envs {
        command.env_remove(name);
    }
    for (name, value) in envs {
        command.env(name, value);
    }
    let output = command.output().map_err(|error| ProcessError {
        kind: ProcessErrorKind::Launch,
        message: format!("failed to launch {program}: {error}"),
    })?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(ProcessError {
            kind: ProcessErrorKind::Exit,
            message: format!(
                "{program} {} failed with {}; stdout: {}; stderr: {}",
                args.join(" "),
                output.status,
                stdout,
                stderr
            ),
        })
    }
}

pub(crate) fn run_process_status(program: &str, args: &[String]) -> Result<(), ProcessError> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|error| ProcessError {
            kind: ProcessErrorKind::Launch,
            message: format!("failed to launch {program}: {error}"),
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(ProcessError {
            kind: ProcessErrorKind::Exit,
            message: format!("{program} {} failed with {status}", args.join(" ")),
        })
    }
}

pub(crate) struct TimedOutput {
    pub(crate) status: Option<ExitStatus>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) duration: Duration,
    pub(crate) timed_out: bool,
}

/// Preserve the failing stage without requiring report callers to parse or
/// publish an OS diagnostic (which can include arbitrary child output).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimedCaptureErrorKind {
    Launch(std::io::ErrorKind),
    ProcessControl,
    Capture,
    Cleanup,
}

#[derive(Debug)]
pub(crate) struct TimedCaptureError {
    pub(crate) kind: TimedCaptureErrorKind,
    message: String,
}

impl TimedCaptureError {
    fn new(kind: TimedCaptureErrorKind, message: String) -> Self {
        Self { kind, message }
    }

    fn into_message(self) -> String {
        self.message
    }
}

fn settle_timed_capture_streams(
    stdout: Result<String, String>,
    stderr: Result<String, String>,
    tree_cleanup: Result<(), String>,
    require_complete_output: bool,
    error_context: &str,
) -> Result<(String, String), TimedCaptureError> {
    // Cleanup has already run. A descendant that remained can also make
    // the drain fail, so typed callers must retain that observed cause.
    if require_complete_output && let Err(cleanup) = &tree_cleanup {
        return Err(TimedCaptureError::new(
            TimedCaptureErrorKind::Cleanup,
            format!(
                "{error_context}: incomplete tree cleanup after primary exit: {cleanup}; stdout={stdout:?}; stderr={stderr:?}"
            ),
        ));
    }
    let stdout = stdout
        .map_err(|message| TimedCaptureError::new(TimedCaptureErrorKind::Capture, message))?;
    let stderr = stderr
        .map_err(|message| TimedCaptureError::new(TimedCaptureErrorKind::Capture, message))?;
    if let Err(cleanup) = tree_cleanup {
        return Err(TimedCaptureError::new(
            TimedCaptureErrorKind::Cleanup,
            format!(
                "{error_context}: incomplete tree cleanup after primary exit: {cleanup}; stdout={stdout}; stderr={stderr}"
            ),
        ));
    }
    Ok((stdout, stderr))
}

pub(crate) struct TimedBytesOutput {
    pub(crate) status: Option<ExitStatus>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) duration: Duration,
    pub(crate) timed_out: bool,
}

pub(crate) struct TimedFileOutput {
    pub(crate) status: Option<ExitStatus>,
    pub(crate) stderr: String,
    pub(crate) duration: Duration,
    pub(crate) timed_out: bool,
    pub(crate) stdout_bytes: usize,
}

struct WaitOutcome {
    status: ExitStatus,
    duration: Duration,
    timed_out: bool,
    /// Highest `VmHWM` observed while polling, when sampling was requested
    /// and the platform exposes it (Linux `/proc`).
    peak_rss_bytes: Option<u64>,
}

/// A timed child run with its sampled peak resident memory.
pub(crate) struct MeasuredOutput {
    pub(crate) output: TimedOutput,
    pub(crate) peak_rss_bytes: Option<u64>,
}

pub(crate) fn run(program: &str, args: &[&str]) -> Result<ExitStatus, String> {
    run_with_envs(program, args, &[])
}

pub(crate) fn run_with_envs(
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<ExitStatus, String> {
    run_with_envs_recorded(program, args, envs, None)
}

pub(crate) fn run_diagnostic(
    program: &str,
    args: &[&str],
    diagnostics: &mut Vec<CommandDiagnostic>,
) -> Result<ExitStatus, String> {
    run_with_envs_recorded(program, args, &[], Some(diagnostics))
}

fn run_with_envs_recorded(
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
    diagnostics: Option<&mut Vec<CommandDiagnostic>>,
) -> Result<ExitStatus, String> {
    let env_text = envs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(" ");
    let env_prefix = if env_text.is_empty() {
        String::new()
    } else {
        format!("{env_text} ")
    };
    eprintln!("$ {env_prefix}{} {}", program, args.join(" "));
    let started = Instant::now();
    let mut command = Command::new(program);
    command.args(args);
    for (name, value) in envs {
        command.env(name, value);
    }
    let native = command.status();
    if let Some(diagnostics) = diagnostics {
        diagnostics.push(CommandDiagnostic::inherited(
            program,
            args,
            &native,
            started.elapsed(),
        ));
    }
    let status = native.map_err(|err| format!("failed to run {program}: {err}"))?;
    if status.success() {
        Ok(status)
    } else {
        Err(format!("{program} {} failed with {status}", args.join(" ")))
    }
}

pub(crate) fn command_success_owned(program: &str, args: &[String]) -> Result<bool, String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|err| format!("failed to run {program}: {err}"))?;
    Ok(status.success())
}

pub(crate) fn run_owned(program: &str, args: &[String]) -> Result<(), String> {
    run_process_status(program, args).map_err(|error| error.message)
}

pub(crate) fn run_in_dir(program: &Path, args: &[&str], cwd: &Path) -> Result<ExitStatus, String> {
    run_in_dir_with_envs(program, args, cwd, &[])
}

pub(crate) fn run_in_dir_with_envs(
    program: &Path,
    args: &[&str],
    cwd: &Path,
    envs: &[(&str, &str)],
) -> Result<ExitStatus, String> {
    let env_text = envs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(" ");
    let env_prefix = if env_text.is_empty() {
        String::new()
    } else {
        format!("{env_text} ")
    };
    eprintln!(
        "$ (cd {} && {}{} {})",
        cwd.display(),
        env_prefix,
        program.display(),
        args.join(" ")
    );
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    for (name, value) in envs {
        command.env(name, value);
    }
    let status = command.status().map_err(|err| {
        format!(
            "failed to run {} in {}: {err}",
            program.display(),
            cwd.display()
        )
    })?;
    if status.success() {
        Ok(status)
    } else {
        Err(format!(
            "{} {} failed with {status} in {}",
            program.display(),
            args.join(" "),
            cwd.display()
        ))
    }
}

pub(crate) fn run_output(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {program}: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} failed with {}",
            args.join(" "),
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `run_output` returning raw stdout bytes (#4006). Path inventories decode
/// through the shared NUL authority at the call site; `String::from_utf8_lossy`
/// here would collapse non-UTF-8 names before identity comparison.
/// Delegates to the shared capture spawn site, so this adds no new process
/// spawn for the process-policy gate; failure messages carry the shared
/// runner's stdout/stderr context instead of `run_output`'s terse form.
pub(crate) fn run_output_bytes(program: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    capture_process_output_in(program, &owned, None, &[], &[], &[]).map_err(|error| error.message)
}

/// `run_output_optional` returning raw stdout bytes (#4006): success yields
/// stdout, exit failure yields empty output, launch failure still errors —
/// the exact `run_output_optional` contract, without lossy decoding and
/// without a new spawn site (see `run_output_bytes`).
pub(crate) fn run_output_optional_bytes(program: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    match capture_process_output_in(program, &owned, None, &[], &[], &[]) {
        Ok(stdout) => Ok(stdout),
        Err(error) if matches!(error.kind, ProcessErrorKind::Exit) => Ok(Vec::new()),
        Err(error) => Err(error.message),
    }
}

pub(crate) fn run_output_owned(program: &str, args: &[String]) -> Result<String, String> {
    run_output_owned_with_envs(program, args, &[])
}

/// `run_output_owned` with an explicit working directory for the child.
///
/// A fixture render that must resolve relative paths from inside a packet tree
/// runs here rather than mutating the xtask process directory, which the
/// rayon-parallel fixture runs share.
pub(crate) fn run_output_owned_in(
    program: &str,
    args: &[String],
    current_dir: &Path,
) -> Result<String, String> {
    let output = capture_process_output_in(program, args, Some(current_dir), &[], &[], &[])
        .map_err(|error| error.message)?;
    Ok(String::from_utf8_lossy(&output).into_owned())
}

/// `run_output_owned` with an explicit child environment overlay.
///
/// Callers that must pin a child's environment — for example a fixture run that
/// pins `RIPR_CACHE_DIR` so the cache it writes is the cache the caller cleared
/// (#3054) — use this instead of mutating the xtask process environment, which
/// the rayon-parallel fixture runs share.
pub(crate) fn run_output_owned_with_envs(
    program: &str,
    args: &[String],
    envs: &[(&str, &str)],
) -> Result<String, String> {
    let output = capture_process_output(program, args, envs).map_err(|error| error.message)?;
    Ok(String::from_utf8_lossy(&output).into_owned())
}

/// Default deadline for building the ripr binary inside PR-evidence commands.
///
/// The required Rust gates build the workspace before PR evidence runs, so
/// this build is incremental in CI; fifteen minutes stays generous for a cold
/// local build while still bounding a wedged toolchain (issue #2230).
pub(crate) const DEFAULT_TOOL_BUILD_TIMEOUT_SECS: u64 = 900;

/// Env override (seconds, positive integer) for the tool-build deadline.
pub(crate) const TOOL_BUILD_TIMEOUT_ENV: &str = "RIPR_TOOL_BUILD_TIMEOUT_SECS";

pub(crate) fn tool_build_timeout() -> Result<Duration, String> {
    env_timeout_secs(TOOL_BUILD_TIMEOUT_ENV, DEFAULT_TOOL_BUILD_TIMEOUT_SECS)
}

pub(crate) fn env_timeout_secs(name: &str, default_secs: u64) -> Result<Duration, String> {
    // A malformed override must fail fast, not silently fall back to the
    // default: NotPresent means "no override", anything else is validated.
    match std::env::var(name) {
        Ok(value) => {
            parse_env_timeout_secs(name, Some(&value), default_secs).map(Duration::from_secs)
        }
        Err(std::env::VarError::NotPresent) => Ok(Duration::from_secs(default_secs)),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "{name} must be valid UTF-8 text naming a positive integer"
        )),
    }
}

fn parse_env_timeout_secs(
    name: &str,
    value: Option<&str>,
    default_secs: u64,
) -> Result<u64, String> {
    let Some(value) = value else {
        return Ok(default_secs);
    };
    let parsed = value
        .trim()
        .parse::<u64>()
        .map_err(|err| format!("{name} must be a positive integer: {err}"))?;
    if parsed > 0 {
        Ok(parsed)
    } else {
        Err(format!("{name} must be a positive integer"))
    }
}

/// Cargo output that says it could not replace the debug binary because a
/// running process holds it. Unix unlinks a running executable; Windows answers
/// `Access is denied (os error 5)`.
pub(crate) fn binary_replacement_blocked(output: &str) -> bool {
    output.contains("failed to remove file") && output.contains("Access is denied")
}

/// Repeat a nested `cargo build -p ripr` that Windows refused because a test
/// running in parallel holds `target/debug/ripr.exe` (#9272). The compile has
/// finished when the link step fails, so a repeat only redoes that step. The
/// retry is Windows-only and bounded (about a minute); `blocked` decides from
/// the error text whether a failure is this one, so a real compile error
/// fails at once. Pass `|_| true` where the caller cannot see Cargo's output.
pub(crate) fn retry_while_binary_busy<T>(
    blocked: impl Fn(&str) -> bool,
    mut attempt: impl FnMut() -> Result<T, String>,
) -> Result<T, String> {
    const ATTEMPTS: u32 = 12;
    let mut tries = 1;
    loop {
        match attempt() {
            Err(error) if cfg!(windows) && tries < ATTEMPTS && blocked(&error) => {
                tries += 1;
                thread::sleep(Duration::from_secs(5));
            }
            other => return other,
        }
    }
}

/// Like `run_output_owned`, but bounds the child with the shared
/// timed-process machinery so a wedged build cannot hang the caller. A
/// timeout is a named result: the child process tree is terminated and the
/// error names the deadline and the calling context.
pub(crate) fn run_output_owned_with_timeout(
    program: &str,
    args: &[String],
    timeout: Duration,
    error_context: &str,
) -> Result<String, String> {
    let output = capture_output_with_timeout(program, args, &[], timeout, error_context)?;
    if output.timed_out {
        let seconds = timeout.as_secs();
        return Err(format!(
            "{error_context} timed out after {seconds} {}; the child process tree was terminated",
            if seconds == 1 { "second" } else { "seconds" }
        ));
    }
    let Some(status) = output.status else {
        return Err(format!("{error_context} did not report a process status"));
    };
    if status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "{program} {} failed with {status}\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            output.stdout.trim(),
            output.stderr.trim()
        ))
    }
}

pub(crate) fn run_output_optional(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {program}: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Ok(String::new())
    }
}

pub(crate) fn capture_output(
    program: &str,
    args: &[&str],
    error_context: &str,
) -> Result<CapturedOutput, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {error_context}: {err}"))?;
    Ok(CapturedOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub(crate) fn capture_output_in_dir(
    program: &str,
    args: &[String],
    cwd: &Path,
    error_context: &str,
) -> Result<CapturedOutput, String> {
    capture_output_in_dir_with_envs(program, args, cwd, error_context, &[], &[])
}

pub(crate) fn capture_output_in_dir_with_envs(
    program: &str,
    args: &[String],
    cwd: &Path,
    error_context: &str,
    envs: &[(&str, &str)],
    env_remove: &[&str],
) -> Result<CapturedOutput, String> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    for (name, value) in envs {
        command.env(name, value);
    }
    for name in env_remove {
        command.env_remove(name);
    }
    let output = command
        .output()
        .map_err(|err| format!("failed to run {error_context} in {}: {err}", cwd.display()))?;
    Ok(CapturedOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub(crate) fn capture_output_with_timeout(
    program: &str,
    args: &[String],
    envs: &[(&str, &str)],
    timeout: Duration,
    error_context: &str,
) -> Result<TimedOutput, String> {
    capture_output_with_deadline(program, args, envs, Some(timeout), error_context)
}

/// The same process owner and deadline, with typed failure stages and a
/// required complete pipe drain. Existing string-error callers retain their
/// historical diagnostic/truncated-output contract through the adapter below.
pub(crate) fn capture_output_with_timeout_typed(
    program: &str,
    args: &[String],
    envs: &[(&str, &str)],
    timeout: Duration,
    error_context: &str,
) -> Result<TimedOutput, TimedCaptureError> {
    capture_output_sampled_owned(
        program,
        args,
        None,
        envs,
        &mut CaptureDeadline::FromStart(&Some(timeout)),
        CaptureMode::Platform {
            sample_rss: false,
            require_complete_output: true,
        },
        error_context,
    )
    .map(|(output, _)| output)
}

/// `capture_output_with_timeout` in an explicit working directory that also
/// samples the child's peak resident memory (Linux `VmHWM`, polled with the
/// deadline loop, so the value is a lower bound). Other platforms report
/// `None`. Used by the developer-experience scoreboard.
pub(crate) fn capture_output_measured(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
    timeout: Duration,
    error_context: &str,
) -> Result<MeasuredOutput, String> {
    let deadline = Some(timeout);
    let (output, peak_rss_bytes) = capture_output_sampled(
        program,
        args,
        cwd,
        envs,
        &mut CaptureDeadline::FromStart(&deadline),
        true,
        error_context,
    )?;
    Ok(MeasuredOutput {
        output,
        peak_rss_bytes,
    })
}

/// Test-only phase budget. Readiness failures still use the capture owner's
/// termination, reap, process-group confirmation and bounded pipe drains.
#[cfg(all(test, target_os = "linux"))]
pub(crate) struct ReadinessDeadline<'a> {
    startup: Duration,
    after_ready: Duration,
    probe: &'a mut dyn FnMut() -> Result<bool, String>,
    armed: Option<Instant>,
    elapsed_at_abort: Option<Duration>,
    failure: Option<String>,
}

#[cfg(all(test, target_os = "linux"))]
impl<'a> ReadinessDeadline<'a> {
    pub(crate) fn new(
        startup: Duration,
        after_ready: Duration,
        probe: &'a mut dyn FnMut() -> Result<bool, String>,
    ) -> Self {
        Self {
            startup,
            after_ready,
            probe,
            armed: None,
            elapsed_at_abort: None,
            failure: None,
        }
    }
}

enum CaptureDeadline<'a> {
    FromStart(&'a Option<Duration>),
    #[cfg(all(test, target_os = "linux"))]
    AfterReadiness(ReadinessDeadline<'a>),
}

impl CaptureDeadline<'_> {
    fn expired(&mut self, started: Instant) -> bool {
        match self {
            Self::FromStart(timeout) => timeout.is_some_and(|timeout| started.elapsed() >= timeout),
            #[cfg(all(test, target_os = "linux"))]
            Self::AfterReadiness(phase) => {
                if let Some(armed) = phase.armed {
                    let elapsed = armed.elapsed();
                    if elapsed >= phase.after_ready {
                        phase.elapsed_at_abort = Some(elapsed);
                        return true;
                    }
                    return false;
                }
                if started.elapsed() >= phase.startup {
                    phase.failure = Some("readiness not observed within startup budget".into());
                    return true;
                }
                let readiness = (phase.probe)();
                let observed_at = Instant::now();
                if observed_at.duration_since(started) >= phase.startup {
                    phase.failure = Some("readiness not observed within startup budget".into());
                    return true;
                }
                match readiness {
                    Ok(true) => phase.armed = Some(observed_at),
                    Ok(false) => {}
                    Err(error) => {
                        phase.failure = Some(format!("readiness observation failed: {error}"));
                        return true;
                    }
                }
                false
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
pub(crate) fn capture_output_measured_after_readiness(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
    deadline: ReadinessDeadline<'_>,
    error_context: &str,
) -> Result<(MeasuredOutput, Option<Duration>), String> {
    let mut deadline = CaptureDeadline::AfterReadiness(deadline);
    let (output, peak_rss_bytes) =
        capture_output_sampled(program, args, cwd, envs, &mut deadline, true, error_context)?;
    let elapsed_at_abort = match deadline {
        CaptureDeadline::AfterReadiness(phase) => phase.elapsed_at_abort,
        CaptureDeadline::FromStart(_) => None,
    };
    Ok((
        MeasuredOutput {
            output,
            peak_rss_bytes,
        },
        elapsed_at_abort,
    ))
}

/// `capture_output_with_timeout` with no per-step wall-clock cap.
///
/// For callers whose child may legitimately run longer than any fixed
/// host-dependent cutoff, such as a cold Cargo/Clippy build in precommit
/// (#4150).  The child is still spawned through the shared owned-subprocess
/// authority. After a normal primary exit the remaining owned tree is ended
/// before the pipe drain (#9323 F3), matching `run_doctor_tool`, so a
/// Windows Job Object descendant cannot hold the capture for the drain
/// grace. The post-exit drain remains bounded by `POST_KILL_DRAIN_GRACE`:
/// Unix `terminate_tree` after a post-reap primary is a no-op without
/// group ownership, so a pipe-inheriting descendant there is still truncated
/// with a named diagnostic instead of blocking for its lifetime. A
/// genuinely hung child still waits until an operator cancels the run; the
/// hosted job bounds are the only automated stop.
pub(crate) fn capture_output_without_timeout(
    program: &str,
    args: &[String],
    envs: &[(&str, &str)],
    error_context: &str,
) -> Result<TimedOutput, String> {
    capture_output_with_deadline(program, args, envs, None, error_context)
}

fn capture_output_with_deadline(
    program: &str,
    args: &[String],
    envs: &[(&str, &str)],
    deadline: Option<Duration>,
    error_context: &str,
) -> Result<TimedOutput, String> {
    capture_output_sampled(
        program,
        args,
        None,
        envs,
        &mut CaptureDeadline::FromStart(&deadline),
        false,
        error_context,
    )
    .map(|(output, _)| output)
}

fn capture_output_sampled(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
    deadline: &mut CaptureDeadline<'_>,
    sample_rss: bool,
    error_context: &str,
) -> Result<(TimedOutput, Option<u64>), String> {
    capture_output_sampled_owned(
        program,
        args,
        cwd,
        envs,
        deadline,
        CaptureMode::Platform {
            sample_rss,
            require_complete_output: false,
        },
        error_context,
    )
    .map_err(TimedCaptureError::into_message)
}

/// The reexecuted test must call `enter_test_session` before launching work.
/// This is a cooperative test boundary, not containment of hostile programs
/// that deliberately create further sessions.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn capture_isolated_test(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
    timeout: Duration,
    error_context: &str,
) -> Result<MeasuredOutput, String> {
    let (output, peak_rss_bytes) = capture_output_sampled_owned(
        program,
        args,
        cwd,
        envs,
        &mut CaptureDeadline::FromStart(&Some(timeout)),
        CaptureMode::TestSession,
        error_context,
    )
    .map_err(TimedCaptureError::into_message)?;
    Ok(MeasuredOutput {
        output,
        peak_rss_bytes,
    })
}

#[derive(Clone, Copy)]
enum CaptureMode {
    Platform {
        sample_rss: bool,
        require_complete_output: bool,
    },
    #[cfg(all(test, target_os = "linux"))]
    TestSession,
}

fn capture_output_sampled_owned(
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
    deadline: &mut CaptureDeadline<'_>,
    mode: CaptureMode,
    error_context: &str,
) -> Result<(TimedOutput, Option<u64>), TimedCaptureError> {
    let started = Instant::now();
    let (sample_rss, isolated_test, require_complete_output) = match mode {
        CaptureMode::Platform {
            sample_rss,
            require_complete_output,
        } => (sample_rss, false, require_complete_output),
        #[cfg(all(test, target_os = "linux"))]
        CaptureMode::TestSession => (true, true, false),
    };
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    if !isolated_test {
        configure_timed_child_command(&mut command);
    }
    for (name, value) in envs {
        command.env(name, value);
    }
    // The timed-child spawn goes through the shared owned-subprocess
    // authority (#3803): on Windows the child is assigned to a
    // kill-on-termination Job Object before its user code runs, so the
    // timeout path below owns the whole tree instead of shelling out to
    // `taskkill /T /F`.
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = OwnedProcess::spawn(command).map_err(|err| {
        TimedCaptureError::new(
            TimedCaptureErrorKind::Launch(err.kind()),
            format!("failed to run {error_context}: {err}"),
        )
    })?;
    #[cfg(all(test, target_os = "linux"))]
    let mut session = isolated_test.then(|| test_session::TestSession::new(child.id()));
    let stdout = child.stdout_pipe().take().ok_or_else(|| {
        TimedCaptureError::new(
            TimedCaptureErrorKind::Capture,
            format!("failed to capture stdout for {error_context}"),
        )
    })?;
    let stderr = child.stderr_pipe().take().ok_or_else(|| {
        TimedCaptureError::new(
            TimedCaptureErrorKind::Capture,
            format!("failed to capture stderr for {error_context}"),
        )
    })?;
    let echo_latency_trace = envs
        .iter()
        .any(|(name, _)| *name == "RIPR_REPO_EXPOSURE_LATENCY_TRACE");

    // Always use channel-based readers so the post-kill drain path can apply a
    // bounded wait via `recv_timeout` (see `drain_stream_reader_bounded`).
    let (stdout_handle, stdout_rx) = spawn_stream_reader_channel(stdout);
    let (stderr_handle, stderr_rx) = if echo_latency_trace {
        spawn_latency_stream_reader_channel(stderr)
    } else {
        spawn_stream_reader_channel(stderr)
    };

    #[cfg(all(test, target_os = "linux"))]
    let wait_outcome = if let Some(session) = session.as_mut() {
        session.wait(&mut child, started, deadline, sample_rss, error_context)
    } else {
        wait_for_child_sampled(&mut child, started, deadline, sample_rss, error_context)
    }
    .map_err(|message| TimedCaptureError::new(TimedCaptureErrorKind::ProcessControl, message))?;
    #[cfg(not(all(test, target_os = "linux")))]
    let wait_outcome =
        wait_for_child_sampled(&mut child, started, deadline, sample_rss, error_context).map_err(
            |message| TimedCaptureError::new(TimedCaptureErrorKind::ProcessControl, message),
        )?;

    let tree_cleanup = end_tree_after_normal_primary_exit(&mut child, wait_outcome.timed_out);

    // Always use the bounded drain. Ending the remaining tree first closes
    // leftover Windows job members so the readers reach EOF promptly. On a
    // timed-out kill the grace timeout still caps the drain if a descendant
    // escaped the process-group kill and holds the pipe open. The same grace
    // is the Unix #4150 bound when the direct child exits but a pipe-inheriting
    // descendant survives a post-reap no-op `terminate_tree`.
    let stdout = drain_stream_reader_bounded(
        stdout_rx,
        stdout_handle,
        POST_KILL_DRAIN_GRACE,
        "stdout",
        error_context,
        require_complete_output,
    );
    if !require_complete_output && let Err(message) = &stdout {
        // Preserve legacy callers' early stdout-error path and exact message.
        return Err(TimedCaptureError::new(
            TimedCaptureErrorKind::Capture,
            message.clone(),
        ));
    }
    let stderr = drain_stream_reader_bounded(
        stderr_rx,
        stderr_handle,
        POST_KILL_DRAIN_GRACE,
        "stderr",
        error_context,
        require_complete_output,
    );
    let (stdout, stderr) = settle_timed_capture_streams(
        stdout,
        stderr,
        tree_cleanup,
        require_complete_output,
        error_context,
    )?;

    #[cfg(all(test, target_os = "linux"))]
    if let CaptureDeadline::AfterReadiness(phase) = &*deadline
        && let Some(error) = phase.failure.as_deref().or_else(|| {
            phase
                .armed
                .is_none()
                .then_some("child exited before readiness was observed")
        })
    {
        return Err(TimedCaptureError::new(
            TimedCaptureErrorKind::ProcessControl,
            format!(
                "{error_context}: {error}; reaped status={}, timed_out={}; stdout={stdout}; stderr={stderr}",
                wait_outcome.status, wait_outcome.timed_out
            ),
        ));
    }

    Ok((
        TimedOutput {
            status: Some(wait_outcome.status),
            stdout,
            stderr,
            duration: wait_outcome.duration,
            timed_out: wait_outcome.timed_out,
        },
        wait_outcome.peak_rss_bytes,
    ))
}

pub(crate) fn capture_bytes_in_dir_with_timeout(
    program: &Path,
    args: &[String],
    cwd: &Path,
    envs: &[(&str, &str)],
    env_remove: &[&str],
    timeout: Duration,
    error_context: &str,
) -> Result<TimedBytesOutput, String> {
    capture_bytes_with_input(
        program,
        args,
        (cwd, None),
        envs,
        env_remove,
        (timeout, None),
        error_context,
    )
}

/// Bounded binary input/output for Git's batch protocol. The existing process
/// owner retains child/descendant custody while stdin is written concurrently.
#[cfg(all(test, unix))]
pub(crate) fn capture_bytes_in_dir_with_input_timeout(
    program: &Path,
    args: &[String],
    cwd: &Path,
    input: &[u8],
    env_remove: &[&str],
    timeout: Duration,
    error_context: &str,
) -> Result<TimedBytesOutput, String> {
    capture_bytes_with_input(
        program,
        args,
        (cwd, Some(input)),
        &[],
        env_remove,
        (timeout, None),
        error_context,
    )
}

/// Explicit resource limits for selected-source Git capture. Other callers
/// retain their existing behavior; the owned process lifecycle is shared.
#[derive(Clone, Copy)]
pub(crate) struct ByteCaptureBudget {
    pub(crate) timeout: Duration,
    pub(crate) stdout_bytes: usize,
    pub(crate) stderr_bytes: usize,
}

pub(crate) fn capture_bytes_in_dir_with_budget(
    program: &Path,
    args: &[String],
    source: (&Path, Option<&[u8]>),
    env_remove: &[&str],
    budget: ByteCaptureBudget,
    error_context: &str,
) -> Result<TimedBytesOutput, String> {
    capture_bytes_with_input(
        program,
        args,
        source,
        &[],
        env_remove,
        (budget.timeout, Some(budget)),
        error_context,
    )
}

/// `capture_bytes_in_dir_with_budget` for a caller that owns the child's
/// environment. Overflow on either stream is a refusal, not truncated output.
pub(crate) fn capture_bytes_in_dir_with_env_budget(
    program: &Path,
    args: &[String],
    cwd: &Path,
    envs: (&[(&str, &str)], &[&str]),
    budget: ByteCaptureBudget,
    error_context: &str,
) -> Result<TimedBytesOutput, String> {
    capture_bytes_in_dir_with_env_budget_observed(program, args, cwd, envs, budget, error_context)
        // Ordinary callers keep their data result and deliberately discard the
        // non-deserializable local-execution observation.
        .map(|observed| observed.consume(|_, output, _, _, _| output))
        .map_err(TimedCaptureError::into_message)
}

/// Observe one actual bounded invocation. The returned capability can only be
/// produced by the owned capture path, never by deserializing retained files.
/// It records requested identity and local outcomes, not protected provenance.
pub(crate) fn capture_bytes_in_dir_with_env_budget_observed(
    program: &Path,
    args: &[String],
    cwd: &Path,
    envs: (&[(&str, &str)], &[&str]),
    budget: ByteCaptureBudget,
    error_context: &str,
) -> Result<ObservedByteCapture, TimedCaptureError> {
    byte_capture::capture_observed(program, args, cwd, envs, budget, error_context)
}

/// `capture_bytes_in_dir_with_env_budget` that also writes `input` to the
/// child's stdin (an MCP stdio session with a configured environment).
pub(crate) fn capture_bytes_in_dir_with_input_env_budget(
    program: &Path,
    args: &[String],
    source: (&Path, Option<&[u8]>),
    envs: (&[(&str, &str)], &[&str]),
    budget: ByteCaptureBudget,
    error_context: &str,
) -> Result<TimedBytesOutput, String> {
    let (envs, env_remove) = envs;
    capture_bytes_with_input(
        program,
        args,
        source,
        envs,
        env_remove,
        (budget.timeout, Some(budget)),
        error_context,
    )
}

fn capture_bytes_with_input(
    program: &Path,
    args: &[String],
    source: (&Path, Option<&[u8]>),
    envs: &[(&str, &str)],
    env_remove: &[&str],
    deadline: (Duration, Option<ByteCaptureBudget>),
    error_context: &str,
) -> Result<TimedBytesOutput, String> {
    byte_capture::capture_legacy(
        program,
        args,
        source,
        (envs, env_remove),
        deadline,
        error_context,
    )
    .map_err(TimedCaptureError::into_message)
}

pub(crate) fn capture_stdout_to_file_with_timeout(
    program: &str,
    args: &[String],
    envs: &[(&str, &str)],
    stdout_path: &Path,
    timeout: Duration,
    error_context: &str,
) -> Result<TimedFileOutput, String> {
    let started = Instant::now();
    let stdout_tmp_path = stdout_capture_temp_path(stdout_path);
    let stdout_file = fs::File::create(&stdout_tmp_path).map_err(|err| {
        format!(
            "failed to create stdout file {} for {error_context}: {err}",
            stdout_tmp_path.display()
        )
    })?;
    let mut command = Command::new(program);
    command.args(args).stdout(Stdio::piped());
    configure_timed_child_command(&mut command);
    for (name, value) in envs {
        command.env(name, value);
    }
    // File capture needs explicit prereap containment even outside a scoped
    // analysis attempt. A prepared numeric process group alone is not custody.
    command.stderr(Stdio::piped());
    let mut child = match OwnedProcess::spawn_isolated(command) {
        Ok(child) => child,
        Err(err) => {
            drop(stdout_file);
            let mut error = format!("failed to run {error_context}: {err}");
            if let Err(cleanup) = remove_stdout_capture_temp(&stdout_tmp_path) {
                error.push_str(&format!("; {cleanup}"));
            }
            return Err(error);
        }
    };
    let stdout = match child.stdout_pipe().take() {
        Some(stdout) => stdout,
        None => {
            drop(stdout_file);
            return Err(discard_failed_stdout_capture(
                &mut child,
                &stdout_tmp_path,
                [None, None],
                error_context,
                format!("failed to capture stdout for {error_context}"),
            ));
        }
    };
    let stderr = match child.stderr_pipe().take() {
        Some(stderr) => stderr,
        None => {
            drop(stdout_file);
            return Err(discard_failed_stdout_capture(
                &mut child,
                &stdout_tmp_path,
                [None, None],
                error_context,
                format!("failed to capture stderr for {error_context}"),
            ));
        }
    };
    let echo_latency_trace = envs
        .iter()
        .any(|(name, _)| *name == "RIPR_REPO_EXPOSURE_LATENCY_TRACE");
    let (stdout_writer_handle, stdout_writer_rx) =
        spawn_stream_file_writer_channel(stdout, stdout_file);
    let (stderr_handle, stderr_rx) = if echo_latency_trace {
        spawn_latency_stream_reader_channel(stderr)
    } else {
        spawn_stream_reader_channel(stderr)
    };

    let wait_outcome =
        match wait_for_child_with_deadline(&mut child, started, Some(timeout), error_context) {
            Ok(wait_outcome) => wait_outcome,
            Err(err) => {
                return Err(discard_failed_stdout_capture(
                    &mut child,
                    &stdout_tmp_path,
                    [Some(stdout_writer_handle), Some(stderr_handle)],
                    error_context,
                    err,
                ));
            }
        };

    let tree_cleanup = end_tree_after_normal_primary_exit(&mut child, wait_outcome.timed_out);
    // A zero primary exit is not a complete capture: a descendant can retain
    // either pipe. Both workers must finish before the staging file is published.
    // Unlike diagnostic string capture, file capture cannot replace missing
    // completion evidence with an empty byte count or a truncation note.
    let captured = (|| {
        tree_cleanup.map_err(|error| {
            format!("{error_context}: incomplete tree cleanup after primary exit: {error}")
        })?;
        let stdout_bytes = drain_file_capture_bounded(
            &stdout_writer_rx,
            POST_KILL_DRAIN_GRACE,
            "stdout",
            error_context,
        )?;
        let stderr =
            drain_file_capture_bounded(&stderr_rx, POST_KILL_DRAIN_GRACE, "stderr", error_context)?;
        #[cfg(target_os = "linux")]
        confirm_timed_process_group_gone(child.id(), error_context)?;
        publish_stdout_capture(&stdout_tmp_path, stdout_path, error_context)?;
        Ok(TimedFileOutput {
            status: Some(wait_outcome.status),
            stderr,
            duration: wait_outcome.duration,
            timed_out: wait_outcome.timed_out,
            stdout_bytes,
        })
    })();
    captured.map_err(|error| {
        discard_failed_stdout_capture(
            &mut child,
            &stdout_tmp_path,
            [Some(stdout_writer_handle), Some(stderr_handle)],
            error_context,
            error,
        )
    })
}

fn stdout_capture_temp_path(stdout_path: &Path) -> std::path::PathBuf {
    let file_name = stdout_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("stdout");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    stdout_path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        unique
    ))
}

fn remove_stdout_capture_temp(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to remove stdout staging file {}: {error}",
            path.display()
        )),
    }
}

fn discard_failed_stdout_capture(
    child: &mut OwnedProcess,
    path: &Path,
    workers: [Option<thread::JoinHandle<()>>; 2],
    error_context: &str,
    mut error: String,
) -> String {
    // Termination is delegated to the retained owner. After reap, the numeric
    // group is only an observation target: it must never be signaled again.
    if let Err(cleanup) = child.terminate_tree() {
        error.push_str(&format!("; primary/tree cleanup failed: {cleanup}"));
    }
    #[cfg(unix)]
    if let Err(cleanup) = confirm_timed_process_group_gone(child.id(), error_context) {
        error.push_str(&format!("; process-group cleanup failed: {cleanup}"));
    }
    let [stdout_worker, stderr_worker] = workers;
    let stdout_closed = settle_file_capture_worker(stdout_worker, "stdout", error_context);
    if let Err(cleanup) = &stdout_closed {
        error.push_str(&format!(
            "; {cleanup}; stdout staging retained at {}",
            path.display()
        ));
    }
    if let Err(cleanup) = settle_file_capture_worker(stderr_worker, "stderr", error_context) {
        error.push_str(&format!("; {cleanup}"));
    }
    // An unsettled writer still owns the file. Retain and name it instead of
    // pretending removal succeeded or deleting through a live handle.
    if stdout_closed.is_ok()
        && let Err(cleanup) = remove_stdout_capture_temp(path)
    {
        error.push_str(&format!("; {cleanup}"));
    }
    error
}

fn settle_file_capture_worker(
    worker: Option<thread::JoinHandle<()>>,
    stream: &str,
    error_context: &str,
) -> Result<(), String> {
    let Some(worker) = worker else {
        return Ok(());
    };
    let deadline = Instant::now() + POST_KILL_DRAIN_GRACE;
    while !worker.is_finished() {
        if Instant::now() >= deadline {
            return Err(format!(
                "{stream} capture worker did not settle after cleanup for {error_context}"
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    worker
        .join()
        .map_err(|_panic| format!("{stream} capture worker panicked for {error_context}"))
}

fn publish_stdout_capture(
    tmp_path: &Path,
    stdout_path: &Path,
    error_context: &str,
) -> Result<(), String> {
    match fs::remove_file(stdout_path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(format!(
                "failed to remove stale stdout file {} for {error_context}: {err}",
                stdout_path.display()
            ));
        }
    }
    fs::rename(tmp_path, stdout_path).map_err(|err| {
        format!(
            "failed to publish stdout file {} for {error_context}: {err}",
            stdout_path.display()
        )
    })
}

/// End leftover descendants after the primary has already been reaped without
/// a timeout, before joining pipe readers. Matches `run_doctor_tool`.
///
/// Timeout paths already terminated the tree in `wait_for_child_sampled`.
/// After a normal Unix reap, `terminate_tree` is a no-op unless the group was
/// owned before reap, so the #4150 truncation diagnostic remains the bound
/// for a pipe-holding descendant. On Windows the still-open Job Object is
/// terminated here so those descendants cannot hold the drain for the grace
/// period (#9323 F3).
fn end_tree_after_normal_primary_exit(
    child: &mut OwnedProcess,
    timed_out: bool,
) -> Result<(), String> {
    if timed_out {
        Ok(())
    } else {
        child.terminate_tree()
    }
}

fn wait_for_child_with_deadline(
    child: &mut OwnedProcess,
    started: Instant,
    deadline: Option<Duration>,
    error_context: &str,
) -> Result<WaitOutcome, String> {
    wait_for_child_sampled(
        child,
        started,
        &mut CaptureDeadline::FromStart(&deadline),
        false,
        error_context,
    )
}

fn wait_for_child_sampled(
    child: &mut OwnedProcess,
    started: Instant,
    deadline: &mut CaptureDeadline<'_>,
    sample_rss: bool,
    error_context: &str,
) -> Result<WaitOutcome, String> {
    let mut peak_rss_bytes = None;
    loop {
        // Sample before polling exit: once the child is reaped its /proc
        // entry is gone. VmHWM is the kernel's own high-water mark, so the
        // last sample before exit already covers earlier peaks.
        if sample_rss && let Some(bytes) = proc_peak_rss_bytes(child.id()) {
            peak_rss_bytes = Some(peak_rss_bytes.map_or(bytes, |seen: u64| seen.max(bytes)));
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|err| format!("failed to poll {error_context}: {err}"))?
        {
            return Ok(WaitOutcome {
                status,
                duration: started.elapsed(),
                timed_out: false,
                peak_rss_bytes,
            });
        }

        if deadline.expired(started) {
            #[cfg(unix)]
            let pgid = child.id();
            let termination_requested = terminate_after_timeout(child, error_context)?;
            let status = child
                .wait()
                .map_err(|err| format!("failed to finish timed-out {error_context}: {err}"))?;
            #[cfg(unix)]
            if termination_requested {
                // A timeout is only recorded after the Unix process group is
                // confirmed empty (or this call fails closed that it could
                // not). Windows Job Object termination already owns the tree.
                confirm_timed_process_group_gone(pgid, error_context)?;
            }
            return Ok(WaitOutcome {
                status,
                duration: started.elapsed(),
                timed_out: timeout_was_enforced(termination_requested, &status),
                peak_rss_bytes,
            });
        }

        thread::sleep(poll_interval(sample_rss));
    }
}

/// Measured runs poll finer so a sub-second command's wall time is not
/// rounded up to the next 100 ms tick; scoreboard timings depend on it.
pub(crate) fn poll_interval(sample_rss: bool) -> Duration {
    Duration::from_millis(if sample_rss { 10 } else { 100 })
}

/// Peak resident set size of a live process from `/proc/<pid>/status`
/// (`VmHWM`, reported in kB). `None` off Linux or once the process is gone.
pub(crate) fn proc_peak_rss_bytes(pid: u32) -> Option<u64> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    parse_vm_hwm_bytes(&status)
}

pub(crate) fn parse_vm_hwm_bytes(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kb = line
        .trim_start_matches("VmHWM:")
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    kb.checked_mul(1024)
}

fn configure_timed_child_command(command: &mut Command) {
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

fn timeout_was_enforced(termination_requested: bool, _status: &ExitStatus) -> bool {
    termination_requested
}

fn terminate_after_timeout(child: &mut OwnedProcess, error_context: &str) -> Result<bool, String> {
    if child
        .try_wait()
        .map_err(|err| format!("failed to poll {error_context}: {err}"))?
        .is_some()
    {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        // Owned Job Object termination (#3803): one typed request kills the
        // whole assigned tree, and the direct child cannot linger past it
        // the way the `taskkill /T /F` report-before-exit race allowed, so
        // no exit-grace confirmation is needed before the caller's reap.
        match child.terminate_tree() {
            Ok(()) => Ok(true),
            Err(err) => {
                if child
                    .try_wait()
                    .map_err(|err| format!("failed to poll {error_context}: {err}"))?
                    .is_some()
                {
                    // The tree died in the race between the termination
                    // request and its failure report; still an enforced
                    // timeout.
                    Ok(true)
                } else {
                    Err(format!(
                        "failed to terminate timed-out {error_context}: {err}"
                    ))
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        let tree_terminated = terminate_timed_process_tree(child);
        if tree_terminated {
            return Ok(true);
        }
        match child.kill() {
            Ok(()) => Ok(true),
            Err(kill_err) => {
                if child
                    .try_wait()
                    .map_err(|err| format!("failed to poll {error_context}: {err}"))?
                    .is_some()
                {
                    Ok(tree_terminated)
                } else {
                    Err(format!(
                        "failed to terminate timed-out {error_context}: {kill_err}"
                    ))
                }
            }
        }
    }
}

#[cfg(not(windows))]
fn terminate_timed_process_tree(child: &OwnedProcess) -> bool {
    signal_process_group(child.id(), "-KILL").is_ok_and(|status| status.success())
}

#[cfg(not(windows))]
fn signal_process_group(pgid: u32, signal: &str) -> std::io::Result<ExitStatus> {
    run_kill_on_group(pgid, signal).map(|output| output.status)
}

#[cfg(not(windows))]
fn run_kill_on_group(pgid: u32, signal: &str) -> std::io::Result<std::process::Output> {
    let group = format!("-{pgid}");
    Command::new("kill")
        .args([signal, "--", group.as_str()])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
}

/// After a timeout kill, confirm the child's Unix process group has no live
/// members, or fail closed that confirmation was impossible.
#[cfg(unix)]
fn confirm_timed_process_group_gone(pgid: u32, error_context: &str) -> Result<(), String> {
    confirm_process_group_gone(pgid, POST_KILL_GROUP_CONFIRM_GRACE, error_context)
}

#[cfg(unix)]
fn confirm_process_group_gone(
    pgid: u32,
    budget: Duration,
    error_context: &str,
) -> Result<(), String> {
    let deadline = Instant::now() + budget;
    loop {
        let members = process_group_live_members(pgid)
            .map_err(|error| format!("{error_context}: {error}"))?;
        if members.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{error_context}: could not confirm process group {pgid} is gone during cleanup; still running: {members:?}"
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Read-only live-group observation after settlement. A complete Linux `/proc`
/// scan omits zombies and includes readable other-uid members. An incomplete or
/// unavailable scan cannot establish absence. Numeric signaling, including
/// `kill -0`, must not be revived after the primary's identity was released.
/// This observes group membership at a point in time, not escaped descendants.
#[cfg(unix)]
fn process_group_live_members(pgid: u32) -> Result<Vec<u32>, String> {
    #[cfg(target_os = "linux")]
    {
        members_from_group_scan(pgid, scan_proc_pgrp(pgid))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(format!(
            "read-only cleanup observation for process group {pgid} is unavailable on this platform"
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(any(target_os = "linux", test))]
enum ProcGroupScan {
    Complete { live: Vec<u32> },
    Incomplete,
}

#[cfg(any(target_os = "linux", test))]
fn members_from_group_scan(pgid: u32, scan: Option<ProcGroupScan>) -> Result<Vec<u32>, String> {
    match scan {
        Some(ProcGroupScan::Complete { live }) => Ok(live),
        Some(ProcGroupScan::Incomplete) => Err(format!(
            "read-only cleanup observation for process group {pgid} is incomplete"
        )),
        None => Err(format!(
            "read-only cleanup observation for process group {pgid} is unavailable"
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(target_os = "linux", test))]
enum ProcStatContribution {
    Live,
    Ignore,
    Incomplete,
}

/// One `/proc/<pid>/stat` outcome for a group scan. Matching live members
/// contribute their pid; vanished members are ignored; unreadable or
/// unparseable members make the scan incomplete.
#[cfg(any(target_os = "linux", test))]
fn contribute_proc_stat(pgid: u32, stat: Result<&str, &std::io::Error>) -> ProcStatContribution {
    match stat {
        Ok(text) => match parse_stat_pgrp(text) {
            Some(ProcStatPgrp::Live { pgrp, .. }) if pgrp == pgid => ProcStatContribution::Live,
            Some(_) => ProcStatContribution::Ignore,
            None => ProcStatContribution::Incomplete,
        },
        Err(err) if proc_stat_error_is_gone(err) => ProcStatContribution::Ignore,
        Err(_) => ProcStatContribution::Incomplete,
    }
}

#[cfg(any(target_os = "linux", test))]
fn apply_proc_stat_contribution(
    live: &mut Vec<u32>,
    complete: &mut bool,
    pid: u32,
    contribution: ProcStatContribution,
) {
    match contribution {
        ProcStatContribution::Live => live.push(pid),
        ProcStatContribution::Ignore => {}
        ProcStatContribution::Incomplete => *complete = false,
    }
}

#[cfg(any(target_os = "linux", test))]
fn finish_proc_group_scan(live: Vec<u32>, complete: bool) -> ProcGroupScan {
    if complete {
        ProcGroupScan::Complete { live }
    } else {
        ProcGroupScan::Incomplete
    }
}

#[cfg(test)]
fn scan_from_stat_reads<'a, I>(pgid: u32, reads: I) -> ProcGroupScan
where
    I: IntoIterator<Item = (u32, Result<&'a str, &'a std::io::Error>)>,
{
    let mut live = Vec::new();
    let mut complete = true;
    for (pid, stat) in reads {
        apply_proc_stat_contribution(
            &mut live,
            &mut complete,
            pid,
            contribute_proc_stat(pgid, stat),
        );
    }
    finish_proc_group_scan(live, complete)
}

#[cfg(target_os = "linux")]
fn parse_proc_dir_pid(name: &std::ffi::OsStr) -> Option<u32> {
    name.to_str()?.parse().ok()
}

/// Non-zombie PIDs whose `/proc/<pid>/stat` pgrp matches `pgid`, regardless
/// of uid. `None` if `/proc` cannot be listed. `Incomplete` if a numeric
/// entry exists but its `stat` is unreadable for a reason other than a
/// vanished process (`NotFound` / ESRCH).
#[cfg(target_os = "linux")]
fn scan_proc_pgrp(pgid: u32) -> Option<ProcGroupScan> {
    let entries = fs::read_dir("/proc").ok()?;
    let mut live = Vec::new();
    let mut complete = true;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        let Some(pid) = parse_proc_dir_pid(&entry.file_name()) else {
            continue;
        };
        let contribution = match fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => contribute_proc_stat(pgid, Ok(stat.as_str())),
            Err(err) => contribute_proc_stat(pgid, Err(&err)),
        };
        apply_proc_stat_contribution(&mut live, &mut complete, pid, contribution);
    }
    Some(finish_proc_group_scan(live, complete))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(target_os = "linux", test))]
enum ProcStatPgrp {
    Live { state: char, pgrp: u32 },
    NotLive,
}

/// `/proc/<pid>/stat` state and pgrp. `comm` may contain spaces and
/// parentheses, so this splits only after the last `)`. Zombies and dead
/// tasks are `NotLive`. Unparseable text is `None` so the scan stays
/// incomplete instead of pretending the member was gone.
#[cfg(any(target_os = "linux", test))]
fn parse_stat_pgrp(stat: &str) -> Option<ProcStatPgrp> {
    let close = stat.rfind(')')?;
    let rest = stat.get(close.saturating_add(1)..)?;
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    if matches!(state, 'Z' | 'X') {
        return Some(ProcStatPgrp::NotLive);
    }
    let _ppid = fields.next()?;
    let pgrp = fields.next()?.parse().ok()?;
    Some(ProcStatPgrp::Live { state, pgrp })
}

/// Ceiling on a drain whose caller set no budget. Far above any real tool
/// output (a full `cargo test` log is a few megabytes); it bounds memory when a
/// child floods a pipe, and overflow is an error rather than a truncation.
pub(crate) const UNBUDGETED_OUTPUT_BYTES: usize = 256 * 1024 * 1024;

fn read_stream<T: Read>(stream: T) -> Result<String, String> {
    let bytes = read_stream_bytes(stream)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_stream_bytes<T: Read>(stream: T) -> Result<Vec<u8>, String> {
    read_stream_bytes_limited(stream, "process output", UNBUDGETED_OUTPUT_BYTES)
}

fn read_stream_bytes_limited(
    stream: impl Read,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    stream
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read bounded {name}: {error}"))?;
    if bytes.len() > limit {
        return Err(format!("{name} exceeds its {limit}-byte output budget"));
    }
    Ok(bytes)
}

fn stream_to_file<T: Read>(mut stream: T, mut file: fs::File) -> Result<usize, String> {
    let mut total = 0usize;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let bytes = stream
            .read(&mut buf)
            .map_err(|err| format!("failed to read process stdout: {err}"))?;
        if bytes == 0 {
            break;
        }
        file.write_all(&buf[..bytes])
            .map_err(|err| format!("failed to write process stdout: {err}"))?;
        total = total.saturating_add(bytes);
    }
    file.flush()
        .map_err(|err| format!("failed to flush process stdout: {err}"))?;
    Ok(total)
}

fn read_stream_with_latency_progress<T: Read>(stream: T) -> Result<String, String> {
    read_latency_stream_limited(stream, UNBUDGETED_OUTPUT_BYTES)
}

fn read_latency_stream_limited<T: Read>(stream: T, limit: usize) -> Result<String, String> {
    let mut reader = BufReader::new(stream);
    let mut out = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|err| format!("failed to read process output: {err}"))?;
        if bytes == 0 {
            break;
        }
        if line.starts_with("ripr_repo_exposure_latency ") {
            eprint!("{line}");
        }
        if out.len().saturating_add(line.len()) > limit {
            return Err(format!(
                "process output exceeds its {limit}-byte output budget"
            ));
        }
        out.push_str(&line);
    }
    Ok(out)
}

/// Spawn a stream reader that delivers its result over a channel rather than
/// only through `JoinHandle::join`.  Returns `(handle, receiver)`.  The handle
/// is kept so the OS can reap the thread; the receiver is used to impose a
/// deadline on the drain via `recv_timeout`.
fn spawn_stream_reader_channel<T: Read + Send + 'static>(
    stream: T,
) -> (
    thread::JoinHandle<()>,
    mpsc::Receiver<Result<String, String>>,
) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = read_stream(stream);
        // Ignore send errors: the receiver may have been abandoned on grace
        // expiry, which is expected.
        let _ = tx.send(result);
    });
    (handle, rx)
}

fn spawn_byte_reader_channel<T: Read + Send + 'static>(
    stream: T,
    limit: Option<(&'static str, usize)>,
) -> (
    thread::JoinHandle<()>,
    mpsc::Receiver<Result<Vec<u8>, String>>,
) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = match limit {
            Some((name, bytes)) => read_stream_bytes_limited(stream, name, bytes),
            None => read_stream_bytes(stream),
        };
        let _ = tx.send(result);
    });
    (handle, rx)
}

/// Spawn a latency-progress stream reader that delivers over a channel.
fn spawn_latency_stream_reader_channel<T: Read + Send + 'static>(
    stream: T,
) -> (
    thread::JoinHandle<()>,
    mpsc::Receiver<Result<String, String>>,
) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = read_stream_with_latency_progress(stream);
        let _ = tx.send(result);
    });
    (handle, rx)
}

/// Drain a stream reader within a bounded grace period.
///
/// Used after a process-group kill to guard against escaped descendants that
/// still hold the pipe write-end open.  If `grace` elapses before the reader
/// finishes, the `JoinHandle` is leaked (the thread will unblock when the
/// process eventually exits or the OS reclaims the fd) and the function returns
/// whatever was captured up to the kill, prefixed with a diagnostic message.
/// The caller should check `timed_out` and propagate the diagnostic
/// appropriately. Callers that require complete output reject abandonment.
fn drain_stream_reader_bounded(
    rx: mpsc::Receiver<Result<String, String>>,
    // The handle is intentionally held until after recv_timeout so the thread
    // stays alive while we wait.  On grace expiry we drop it; the thread
    // continues in the background until the fd closes.
    _handle: thread::JoinHandle<()>,
    grace: Duration,
    stream_name: &str,
    error_context: &str,
    require_complete: bool,
) -> Result<String, String> {
    match rx.recv_timeout(grace) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) if require_complete => Err(format!(
            "{stream_name} drain exceeded post-kill grace ({}s) for {error_context}; complete output is not established",
            grace.as_secs()
        )),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // A descendant escaped the process-group kill and still holds the
            // pipe open.  Return an empty string with a diagnostic; the caller
            // already knows the process timed out.
            Ok(format!(
                "[ripr-xtask: {stream_name} drain exceeded post-kill grace \
                 ({grace_secs}s) for {error_context}; a descendant process \
                 escaped group-kill and kept the pipe open — output truncated]",
                grace_secs = grace.as_secs(),
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(format!(
            "{stream_name} reader thread disconnected while running {error_context}"
        )),
    }
}

fn drain_byte_reader_bounded(
    rx: mpsc::Receiver<Result<Vec<u8>, String>>,
    _handle: thread::JoinHandle<()>,
    grace: Duration,
    stream_name: &str,
    error_context: &str,
    require_complete: bool,
) -> Result<Vec<u8>, String> {
    match rx.recv_timeout(grace) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) if require_complete => Err(format!(
            "{stream_name} drain exceeded post-kill grace ({}s) for {error_context}; byte output is not established",
            grace.as_secs()
        )),
        // Preserve established uncapped callers' timeout/reporting contract.
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(format!(
            "[ripr-xtask: {stream_name} drain exceeded post-kill grace ({}s) for {error_context}; output truncated]",
            grace.as_secs()
        ).into_bytes()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(format!(
            "{stream_name} reader thread disconnected while running {error_context}"
        )),
    }
}

/// A file capture may publish only after each worker reports completion.
/// Keep its receiver and handle owned by the caller so failure cleanup can
/// terminate the process group/job and observe worker closure before unlinking.
fn drain_file_capture_bounded<T>(
    rx: &mpsc::Receiver<Result<T, String>>,
    grace: Duration,
    stream_name: &str,
    error_context: &str,
) -> Result<T, String> {
    match rx.recv_timeout(grace) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "{stream_name} file capture incomplete: drain exceeded post-exit grace ({}s) for {error_context}; artifact not published",
            grace.as_secs()
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(format!(
            "{stream_name} file capture worker disconnected while running {error_context}"
        )),
    }
}

/// Spawn a stream-to-file writer that delivers its byte count over a channel.
fn spawn_stream_file_writer_channel<T: Read + Send + 'static>(
    stream: T,
    file: fs::File,
) -> (
    thread::JoinHandle<()>,
    mpsc::Receiver<Result<usize, String>>,
) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = stream_to_file(stream, file);
        let _ = tx.send(result);
    });
    (handle, rx)
}

#[cfg(test)]
mod tests {
    #[test]
    fn measured_runs_poll_ten_times_finer_than_plain_timed_runs() {
        assert_eq!(super::poll_interval(true), super::Duration::from_millis(10));
        assert_eq!(
            super::poll_interval(false),
            super::Duration::from_millis(100)
        );
    }

    #[test]
    fn a_drain_refuses_output_past_its_budget_instead_of_truncating() {
        let flood = std::io::repeat(b'x').take(1_000);
        assert_eq!(
            super::read_stream_bytes_limited(flood, "stdout", 100),
            Err("stdout exceeds its 100-byte output budget".to_string())
        );

        let within =
            super::read_stream_bytes_limited(std::io::repeat(b'x').take(100), "stdout", 100);
        assert_eq!(within.map(|bytes| bytes.len()), Ok(100));
    }

    #[test]
    fn the_latency_drain_refuses_output_past_its_budget() {
        let lines = "ripr_repo_exposure_latency a=1\nplain\n".repeat(10);
        assert_eq!(
            super::read_latency_stream_limited(Cursor::new(lines.as_bytes()), 40),
            Err("process output exceeds its 40-byte output budget".to_string())
        );
        assert_eq!(
            super::read_latency_stream_limited(Cursor::new(lines.as_bytes()), lines.len()),
            Ok(lines.clone())
        );
    }

    #[cfg(unix)]
    use super::POST_KILL_GROUP_CONFIRM_GRACE;
    use super::{
        CapturedOutput, POST_KILL_DRAIN_GRACE, capture_output, capture_output_with_timeout,
        capture_output_without_timeout, capture_stdout_to_file_with_timeout, command_success_owned,
        drain_stream_reader_bounded, parse_env_timeout_secs, parse_stat_pgrp,
        read_stream_with_latency_progress, remove_stdout_capture_temp, run, run_in_dir, run_output,
        run_output_optional, run_output_owned, run_output_owned_with_envs,
        run_output_owned_with_timeout, run_owned, spawn_stream_reader_channel,
        terminate_after_timeout, timeout_was_enforced,
    };
    use crate::acquire_test_cwd_read_guard;
    use ripr::process_owner::OwnedProcess;
    use std::fs;
    use std::io::{Cursor, Read};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    #[cfg(unix)]
    use std::time::Instant;

    #[cfg(unix)]
    use std::os::unix::process::CommandExt;

    type TestCommand = (String, Vec<String>, Vec<(String, String)>);

    #[test]
    fn run_reports_success_and_failure_status() -> Result<(), String> {
        // Hold the shared cwd lock so concurrent cwd-manipulating tests in
        // main.rs cannot race this subprocess spawn (issue #2044).
        let _cwd_guard = acquire_test_cwd_read_guard();
        let status = run("rustc", &["--version"])?;
        if !status.success() {
            return Err("rustc --version should succeed".to_string());
        }

        let Err(err) = run("rustc", &["--ripr-invalid-test-flag"]) else {
            return Err("invalid rustc flag should fail".to_string());
        };
        if !err.contains("failed with") {
            return Err(format!("failure message should include status: {err}"));
        }
        Ok(())
    }

    #[test]
    fn owned_run_helpers_report_success_and_failure_status() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let version_args = vec!["--version".to_string()];
        if !command_success_owned("rustc", &version_args)? {
            return Err("rustc --version should report success".to_string());
        }
        run_owned("rustc", &version_args)?;

        let bad_args = vec!["--ripr-invalid-test-flag".to_string()];
        if command_success_owned("rustc", &bad_args)? {
            return Err("invalid rustc flag should report failure".to_string());
        }
        let Err(err) = run_owned("rustc", &bad_args) else {
            return Err("invalid rustc flag should fail".to_string());
        };
        if !err.contains("failed with") {
            return Err(format!("failure message should include status: {err}"));
        }
        Ok(())
    }

    #[test]
    fn run_in_dir_reports_success_and_failure_with_cwd() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        let status = run_in_dir(Path::new("rustc"), &["--version"], cwd)?;
        if !status.success() {
            return Err("rustc --version should succeed".to_string());
        }

        let Err(err) = run_in_dir(Path::new("rustc"), &["--ripr-invalid-test-flag"], cwd) else {
            return Err("invalid rustc flag should fail".to_string());
        };
        if !err.contains("failed with") || !err.contains(&cwd.display().to_string()) {
            return Err(format!(
                "failure message should include status and cwd: {err}"
            ));
        }
        Ok(())
    }

    #[test]
    fn run_output_reports_stdout_and_failure() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let stdout = run_output("rustc", &["--version"])?;
        if !stdout.contains("rustc") {
            return Err(format!("rustc version output should name rustc: {stdout}"));
        }

        let Err(err) = run_output("rustc", &["--ripr-invalid-test-flag"]) else {
            return Err("invalid rustc flag should fail".to_string());
        };
        if !err.contains("failed with") {
            return Err(format!("failure message should include status: {err}"));
        }
        Ok(())
    }

    /// Fixture runs pin `RIPR_CACHE_DIR` through this overlay so the cache the
    /// child writes is the cache the runner cleared (#3054). `git var
    /// GIT_AUTHOR_IDENT` echoes `GIT_AUTHOR_NAME` back, which makes the child's
    /// view of the environment observable without a shell.
    #[test]
    fn run_output_owned_with_envs_overlays_the_child_environment() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        // Echo the variable through the platform shell rather than through
        // `git`. Both earlier attempts used `git var`, and both failed on a CI
        // runner for reasons that had nothing to do with env overlaying:
        // `GIT_AUTHOR_IDENT` needs `user.name`/`user.email` from ambient config
        // (exit 128), and `GIT_EDITOR` exits 1 with empty output when no editor
        // resolves. A shell `echo` always exits 0 and prints exactly what the
        // child environment holds, so this asserts the overlay and nothing else.
        #[cfg(windows)]
        let (program, args) = (
            "cmd",
            vec!["/C".to_string(), "echo %RIPR_ENV_PROBE%".to_string()],
        );
        #[cfg(not(windows))]
        let (program, args) = (
            "sh",
            vec![
                "-c".to_string(),
                "printf %s \"$RIPR_ENV_PROBE\"".to_string(),
            ],
        );

        let overlaid =
            run_output_owned_with_envs(program, &args, &[("RIPR_ENV_PROBE", "ripr-env-overlay")])?;
        if !overlaid.contains("ripr-env-overlay") {
            return Err(format!(
                "the overlaid value should reach the child: {overlaid}"
            ));
        }

        let plain = run_output_owned(program, &args)?;
        if plain.contains("ripr-env-overlay") {
            return Err(format!(
                "an empty overlay must not set the variable: {plain}"
            ));
        }
        Ok(())
    }

    #[test]
    fn run_output_owned_includes_stderr_on_failure() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let args = vec!["--version".to_string()];
        let stdout = run_output_owned("rustc", &args)?;
        if !stdout.contains("rustc") {
            return Err(format!("rustc version output should name rustc: {stdout}"));
        }

        let bad_args = vec!["--ripr-invalid-test-flag".to_string()];
        let Err(err) = run_output_owned("rustc", &bad_args) else {
            return Err("invalid rustc flag should fail".to_string());
        };
        for expected in ["stdout:", "stderr:", "failed with"] {
            if !err.contains(expected) {
                return Err(format!("failure message should include {expected}: {err}"));
            }
        }
        Ok(())
    }

    #[test]
    fn run_output_optional_returns_empty_for_failure() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let stdout = run_output_optional("rustc", &["--version"])?;
        if !stdout.contains("rustc") {
            return Err(format!("rustc version output should name rustc: {stdout}"));
        }

        let empty = run_output_optional("rustc", &["--ripr-invalid-test-flag"])?;
        if !empty.is_empty() {
            return Err(format!("failed optional output should be empty: {empty}"));
        }
        Ok(())
    }

    #[test]
    fn run_output_owned_with_timeout_returns_stdout_on_success() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let args = vec!["--version".to_string()];
        let stdout = run_output_owned_with_timeout(
            "rustc",
            &args,
            Duration::from_secs(30),
            "rustc version",
        )?;
        if !stdout.contains("rustc") {
            return Err(format!("rustc version output should name rustc: {stdout}"));
        }
        Ok(())
    }

    #[test]
    fn run_output_owned_with_timeout_reports_named_timeout() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let (program, args, _envs) = long_running_command()?;
        let err = match run_output_owned_with_timeout(
            &program,
            &args,
            Duration::from_secs(1),
            "bounded build probe",
        ) {
            Ok(stdout) => {
                return Err(format!(
                    "long-running command should time out, got stdout: {stdout}"
                ));
            }
            Err(err) => err,
        };
        #[cfg(not(all(unix, not(target_os = "linux"))))]
        let expected_parts = [
            "bounded build probe",
            "timed out after 1 second;",
            "process tree was terminated",
        ];
        #[cfg(all(unix, not(target_os = "linux")))]
        let expected_parts = [
            "bounded build probe",
            "read-only cleanup observation",
            "unavailable on this platform",
        ];
        for expected in expected_parts {
            if !err.contains(expected) {
                return Err(format!("timeout message should name {expected:?}: {err}"));
            }
        }
        Ok(())
    }

    #[test]
    fn parse_env_timeout_secs_validates_values() -> Result<(), String> {
        assert_eq!(
            parse_env_timeout_secs("RIPR_TEST_TIMEOUT", None, 900),
            Ok(900)
        );
        assert_eq!(
            parse_env_timeout_secs("RIPR_TEST_TIMEOUT", Some("120"), 900),
            Ok(120)
        );
        assert_eq!(
            parse_env_timeout_secs("RIPR_TEST_TIMEOUT", Some(" 45 "), 900),
            Ok(45)
        );
        assert_eq!(
            parse_env_timeout_secs("RIPR_TEST_TIMEOUT", Some("0"), 900),
            Err("RIPR_TEST_TIMEOUT must be a positive integer".to_string())
        );
        let err = match parse_env_timeout_secs("RIPR_TEST_TIMEOUT", Some("abc"), 900) {
            Ok(value) => return Err(format!("invalid timeout should fail, got {value}")),
            Err(err) => err,
        };
        assert!(err.contains("RIPR_TEST_TIMEOUT"));
        assert!(err.contains("positive integer"));
        Ok(())
    }

    #[test]
    fn capture_output_returns_status_stdout_and_stderr() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let CapturedOutput {
            status,
            stdout,
            stderr,
        } = capture_output("rustc", &["--version"], "rustc version")?;

        if !status.success() {
            return Err("rustc --version should succeed".to_string());
        }
        if !stdout.contains("rustc") {
            return Err(format!("captured stdout should name rustc: {stdout}"));
        }
        if !stderr.is_empty() {
            return Err(format!("captured stderr should be empty: {stderr}"));
        }
        Ok(())
    }

    #[test]
    fn capture_output_with_timeout_reports_completed_process() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let args = vec!["--version".to_string()];
        let output = capture_output_with_timeout(
            "rustc",
            &args,
            &[],
            Duration::from_secs(30),
            "rustc version",
        )?;

        if output.timed_out {
            return Err("rustc --version should not time out".to_string());
        }
        if !output.status.is_some_and(|status| status.success()) {
            return Err("rustc --version should succeed".to_string());
        }
        if !output.stdout.contains("rustc") {
            return Err(format!(
                "captured stdout should name rustc: {}",
                output.stdout
            ));
        }
        Ok(())
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn capture_output_with_timeout_reports_timed_out_process() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let (program, args, envs) = long_running_command()?;
        let env_refs = envs
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let output = capture_output_with_timeout(
            &program,
            &args,
            &env_refs,
            Duration::from_millis(100),
            "long-running command",
        )?;

        assert!(output.timed_out, "long-running command should time out");
        let diagnostic = super::CommandDiagnostic::captured(
            &program,
            &args,
            &Ok(super::TimedOutput {
                status: output.status,
                stdout: output.stdout.clone(),
                stderr: output.stderr.clone(),
                duration: output.duration,
                timed_out: output.timed_out,
            }),
        );
        #[cfg(unix)]
        assert_eq!(
            diagnostic.elapsed_scope,
            "runner_start_through_timeout_termination_and_group_confirmation_excluding_pipe_drain"
        );
        #[cfg(not(unix))]
        assert_eq!(
            diagnostic.elapsed_scope,
            "runner_start_through_timeout_termination_excluding_pipe_drain"
        );
        #[cfg(unix)]
        assert!(
            output.status.is_some(),
            "timed-out long-running command should report a process status"
        );
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    #[test]
    fn timeout_without_a_read_only_group_observer_refuses_confirmation() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let result = capture_output_with_timeout(
            "sh",
            &["-c".into(), "sleep 30".into()],
            &[],
            Duration::from_millis(100),
            "unobserved cleanup fixture",
        );
        match result {
            Err(error)
                if error.contains("unobserved cleanup fixture")
                    && error.contains("read-only cleanup observation")
                    && error.contains("unavailable on this platform") =>
            {
                Ok(())
            }
            Err(error) => Err(format!("wrong unobserved-cleanup refusal: {error}")),
            Ok(_) => Err("timeout invented process-group cleanup evidence".into()),
        }
    }

    #[cfg(unix)]
    fn long_running_command() -> Result<TestCommand, String> {
        Ok((
            "sh".to_string(),
            vec!["-c".to_string(), "sleep 30".to_string()],
            Vec::new(),
        ))
    }

    #[cfg(windows)]
    fn long_running_command() -> Result<TestCommand, String> {
        Ok((
            "powershell".to_string(),
            vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Start-Sleep -Seconds 30".to_string(),
            ],
            Vec::new(),
        ))
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn capture_output_with_timeout_terminates_pipe_inheriting_descendants() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let marker = unique_pgid_marker("pipe-descendant");
        let args = vec![
            "-c".to_string(),
            r#"printf %s "$$" > "$RIPR_XTASK_PGID_MARKER"; sleep 30 & wait"#.to_string(),
        ];
        let marker_env = marker.to_string_lossy().into_owned();
        let envs = [("RIPR_XTASK_PGID_MARKER", marker_env.as_str())];
        // The timeout must comfortably exceed the time for `sh` to fork
        // `sleep 30` INTO its process group, otherwise the group-kill on
        // timeout can race a not-yet-grouped descendant: the descendant
        // survives, keeps the inherited stdout pipe open, and `read_to_end`
        // hangs. Under parallel test load process startup can take hundreds of
        // ms, so a tight (100 ms) timeout flaked (#1022). Five seconds is a
        // generous, deterministic margin — do not tighten without re-checking
        // this race.
        let output = capture_output_with_timeout(
            "sh",
            &args,
            &envs,
            Duration::from_secs(5),
            "pipe-inheriting descendant",
        )?;

        assert!(
            output.timed_out,
            "pipe-inheriting descendant should time out"
        );
        assert_recorded_timeout_left_process_group_gone(&marker)?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn capture_output_with_timeout_confirms_gnu_time_descendant_group_gone() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        if !Path::new("/usr/bin/time").exists() {
            return Ok(());
        }
        let marker = unique_pgid_marker("gnu-time");
        // `exec` keeps the process-group leader pid while GNU `time` forks
        // `sleep`, which is the scale-cliff wrapper shape that motivated #5382.
        let args = vec![
            "-c".to_string(),
            r#"printf %s "$$" > "$RIPR_XTASK_PGID_MARKER"; exec /usr/bin/time -v sleep 30"#
                .to_string(),
        ];
        let marker_env = marker.to_string_lossy().into_owned();
        let envs = [("RIPR_XTASK_PGID_MARKER", marker_env.as_str())];
        let output = capture_output_with_timeout(
            "sh",
            &args,
            &envs,
            Duration::from_secs(5),
            "gnu-time descendant",
        )?;

        if !output.timed_out {
            return Err("gnu-time wrapped sleep should time out".to_string());
        }
        assert_recorded_timeout_left_process_group_gone(&marker)?;
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn capture_output_with_timeout_terminates_pipe_inheriting_descendants() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let marker = std::env::temp_dir().join(format!(
            "ripr-xtask-pipe-descendant-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let script = [
            "$p = Start-Process -FilePath powershell -ArgumentList @('-NoProfile','-Command','Start-Sleep -Seconds 30') -NoNewWindow -PassThru",
            "Wait-Process -Id $p.Id",
            "Set-Content -LiteralPath $env:RIPR_XTASK_DESCENDANT_MARKER -Value alive",
        ]
        .join("; ");
        let args = vec!["-NoProfile".to_string(), "-Command".to_string(), script];
        let marker_env = marker.to_string_lossy().into_owned();
        let envs = [("RIPR_XTASK_DESCENDANT_MARKER", marker_env.as_str())];
        // The owned Job Object (#3803) contains descendants from process
        // creation, so even a descendant started late in the run dies with
        // the tree: assignment precedes any child user code and there is no
        // snapshot window to escape through (#1022's race is structural now).
        let output = capture_output_with_timeout(
            "powershell",
            &args,
            &envs,
            Duration::from_secs(5),
            "pipe-inheriting descendant",
        )?;

        assert!(
            output.timed_out,
            "pipe-inheriting descendant should time out"
        );
        if marker.exists() {
            let _ = fs::remove_file(&marker);
            return Err("timed-out process tree should not run its continuation".to_string());
        }
        // The marker is written by the parent, so a direct-child-only kill
        // also leaves it absent. The descendant is what holds the captured
        // pipes: when the job ends it the readers reach EOF, and when it
        // survives the bounded drain gives up with a truncation diagnostic.
        for (stream, text) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
            assert!(
                !text.contains("output truncated"),
                "{stream} drain was abandoned, so the pipe-holding descendant outlived the timeout: {text:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn sampled_capture_ends_tree_after_normal_primary_exit_before_drain() -> Result<(), String> {
        let src = include_str!("run.rs");
        let body = src
            .split("fn capture_output_sampled_owned(")
            .nth(1)
            .and_then(|rest| {
                rest.split("\npub(crate) fn capture_bytes_in_dir_with_timeout")
                    .next()
            })
            .ok_or_else(|| {
                "capture_output_sampled_owned must sit immediately before capture_bytes_in_dir_with_timeout"
                    .to_string()
            })?;
        let wait = body
            .find("wait_for_child_sampled(")
            .ok_or_else(|| "capture_output_sampled_owned must wait for the child".to_string())?;
        let end_tree = body
            .find("let tree_cleanup = end_tree_after_normal_primary_exit(")
            .ok_or_else(|| {
                "capture_output_sampled_owned must end the owned tree after a normal primary exit"
                    .to_string()
            })?;
        let drain = body
            .find("let stdout = drain_stream_reader_bounded(")
            .ok_or_else(|| "capture_output_sampled_owned must drain after the wait".to_string())?;
        if wait >= end_tree || end_tree >= drain {
            return Err(
                "capture_output_sampled_owned must terminate leftover descendants after the primary wait and before draining"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn tree_end_after_normal_exit_skips_when_timeout_already_ended_the_tree() -> Result<(), String>
    {
        let src = include_str!("run.rs");
        let body = src
            .split("fn end_tree_after_normal_primary_exit(")
            .nth(1)
            .and_then(|rest| rest.split("\nfn wait_for_child_with_deadline(").next())
            .ok_or_else(|| {
                "end_tree_after_normal_primary_exit must sit immediately before wait_for_child_with_deadline"
                    .to_string()
            })?;
        let timeout_guard = body
            .find("if timed_out")
            .ok_or_else(|| "timeout path must skip a second tree end".to_string())?;
        let terminate = body
            .find("terminate_tree")
            .ok_or_else(|| "normal-exit path must terminate the remaining tree".to_string())?;
        if timeout_guard >= terminate {
            return Err(
                "a recorded timeout must not issue a second terminate_tree before the drain"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn capture_output_without_timeout_returns_after_direct_child_exit_with_lingering_pipe_descendant()
    -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        // The direct `sh` child exits 0 immediately; the backgrounded `sleep`
        // inherits the captured stdout/stderr pipes and outlives it. Post-reap
        // `terminate_tree` cannot signal that group (#9895 recyclable pgid),
        // so the no-deadline runner must still return bounded by the post-exit
        // drain grace instead of waiting for the descendant (#4150 review
        // control), and it must not report a step timeout.
        #[cfg(target_os = "linux")]
        let marker = unique_pgid_marker("legacy-lingering-pipe");
        #[cfg(target_os = "linux")]
        let args = vec![
            "-c".to_string(),
            r#"sh -c 'cat /proc/$$/stat > "$1"; exec sleep 60' descendant "$1" & exit 0"#
                .to_string(),
            "legacy-capture".to_string(),
            marker.display().to_string(),
        ];
        #[cfg(not(target_os = "linux"))]
        let args = vec!["-c".to_string(), "sleep 60 & exit 0".to_string()];
        let captured =
            capture_output_without_timeout("sh", &args, &[], "post-exit descendant drain");
        #[cfg(target_os = "linux")]
        {
            // Preserve the capture result before fixture cleanup. The legacy
            // truncation oracle must not leave a running child behind the test.
            let recorded = fs::read_to_string(&marker)
                .map_err(|error| format!("missing legacy descendant identity: {error}"))?;
            super::test_cleanup::finish_recorded_member(&recorded)?;
            fs::remove_file(&marker).map_err(|error| error.to_string())?;
        }
        let output = captured?;

        assert!(
            output.status.is_some_and(|status| status.success()),
            "direct child that exited 0 must not be reported as failed: {:?}",
            output.status
        );
        assert!(
            !output.timed_out,
            "the no-deadline path must never report a step timeout"
        );
        // The truncation diagnostic is only written when the drain grace gave
        // up on the 60s pipe-holding descendant, which is the bounded return.
        assert!(
            output.stdout.contains("output truncated"),
            "truncated-capture diagnostic missing from stdout: {:?}",
            output.stdout
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn capture_output_without_timeout_returns_after_direct_child_exit_with_lingering_pipe_descendant()
    -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        // Same setup as the Unix #4150 truncation control: the direct `cmd`
        // child exits 0 immediately after `start /b`-ing a longer-lived ping
        // descendant that inherits the captured stdout/stderr pipes. After that
        // normal primary exit the owned Job Object is ended before the drain
        // (#9323 F3), so the descendant cannot hold the pipes for the grace
        // period. `cmd`/`ping` are used instead of PowerShell so the control
        // does not depend on a heavyweight shell under a loaded host.
        let script = "start /b ping -n 60 127.0.0.1 >NUL & exit /b 0";
        let args = vec![
            "/d".to_string(),
            "/s".to_string(),
            "/c".to_string(),
            script.to_string(),
        ];
        let output =
            capture_output_without_timeout("cmd", &args, &[], "post-exit descendant drain")?;

        assert!(
            output.status.is_some_and(|status| status.success()),
            "direct child that exited 0 must not be reported as failed: {:?}",
            output.status
        );
        assert!(
            !output.timed_out,
            "the no-deadline path must never report a step timeout"
        );
        for (stream, text) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
            assert!(
                !text.contains("output truncated"),
                "{stream} drain was abandoned after a normal primary exit, so the pipe-holding descendant outlived the job: {text:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn capture_stdout_to_file_with_timeout_streams_stdout_to_file() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let path = std::env::temp_dir().join(format!(
            "ripr-xtask-stdout-file-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::write(&path, "stale output")
            .map_err(|err| format!("failed to write stale stdout file: {err}"))?;
        let args = vec!["--version".to_string()];
        let output = capture_stdout_to_file_with_timeout(
            "rustc",
            &args,
            &[],
            &path,
            Duration::from_secs(30),
            "rustc version",
        )?;

        if output.timed_out {
            return Err("rustc --version should not time out".to_string());
        }
        if !output.status.is_some_and(|status| status.success()) {
            return Err("rustc --version should succeed".to_string());
        }
        if output.stdout_bytes == 0 {
            return Err("streamed stdout should report bytes".to_string());
        }
        let captured = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read streamed stdout file: {err}"))?;
        let parent = path
            .parent()
            .ok_or_else(|| "streamed stdout path should have a parent".to_string())?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "streamed stdout path should have a UTF-8 file name".to_string())?;
        let temp_prefix = format!(".{file_name}.");
        let leaked_temp = fs::read_dir(parent)
            .map_err(|err| format!("failed to inspect streamed stdout parent: {err}"))?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.starts_with(&temp_prefix) && name.ends_with(".tmp"));
        fs::remove_file(&path)
            .map_err(|err| format!("failed to remove streamed stdout file: {err}"))?;
        if let Some(leaked_temp) = leaked_temp {
            return Err(format!(
                "streamed stdout should publish through temp file without leaving {leaked_temp}"
            ));
        }
        if captured.contains("stale output") {
            return Err(format!(
                "captured stdout should overwrite stale file contents: {captured}"
            ));
        }
        if !captured.contains("rustc") {
            return Err(format!("captured stdout should name rustc: {captured}"));
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stdout_file_capture_incomplete_stdout_cleans_owned_descendant() -> Result<(), String> {
        owned_stdout_file_capture_fixture("stdout", false)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stdout_file_capture_incomplete_stderr_cleans_owned_descendant() -> Result<(), String> {
        owned_stdout_file_capture_fixture("stderr", false)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stdout_file_capture_timeout_retains_actual_observation() -> Result<(), String> {
        owned_stdout_file_capture_fixture("stdout", true)
    }

    #[cfg(target_os = "linux")]
    fn owned_stdout_file_capture_fixture(stream: &str, wait_primary: bool) -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let root = unique_pgid_marker("stdout-file").with_extension("dir");
        fs::create_dir(&root).map_err(|error| format!("create owned fixture {root:?}: {error}"))?;
        let destination = root.join("stdout.json");
        let sibling = root.join(".stdout.json.other-owner.tmp");
        fs::write(&destination, b"prior-good-artifact").map_err(|error| error.to_string())?;
        fs::write(&sibling, b"other-owner").map_err(|error| error.to_string())?;
        let script = r#"
printf '%s' "$$" > "$RIPR_CAPTURE_FIXTURE/primary"
if [ "$RIPR_CAPTURE_STREAM" = stdout ]; then
    sh -c 'trap "" TERM; cat /proc/$$/stat > "$RIPR_CAPTURE_FIXTURE/ready.stat"; printf "%s" "$$" > "$RIPR_CAPTURE_FIXTURE/ready"; exec sleep 30' 2>/dev/null &
else
    sh -c 'trap "" TERM; cat /proc/$$/stat > "$RIPR_CAPTURE_FIXTURE/ready.stat"; printf "%s" "$$" > "$RIPR_CAPTURE_FIXTURE/ready"; exec sleep 30' >/dev/null &
fi
attempt=0
while [ ! -s "$RIPR_CAPTURE_FIXTURE/ready" ]; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 300 ]; then exit 99; fi
    sleep 0.01
done
printf '{"complete":true}\n'
if [ "$RIPR_CAPTURE_WAIT" = yes ]; then wait; fi
"#;
        let root_text = root.to_string_lossy().into_owned();
        let envs = [
            ("RIPR_CAPTURE_FIXTURE", root_text.as_str()),
            ("RIPR_CAPTURE_STREAM", stream),
            ("RIPR_CAPTURE_WAIT", if wait_primary { "yes" } else { "no" }),
        ];
        let started = Instant::now();
        let capture = capture_stdout_to_file_with_timeout(
            "sh",
            &["-c".to_string(), script.to_string()],
            &envs,
            &destination,
            Duration::from_secs(if wait_primary { 5 } else { 10 }),
            "owned stdout-file fixture",
        );
        let wall = started.elapsed();
        let primary = fs::read_to_string(root.join("primary"))
            .map_err(|error| format!("fixture did not start in {root:?}: {error}"))?
            .parse::<u32>()
            .map_err(|error| format!("invalid primary marker: {error}"))?;
        let descendant = fs::read_to_string(root.join("ready"))
            .map_err(|error| format!("resistant descendant was not ready in {root:?}: {error}"))?
            .parse::<u32>()
            .map_err(|error| format!("invalid readiness marker: {error}"))?;
        // Snapshot cleanup evidence BEFORE the test's fallback termination.
        // Only the runner's own cleanup can satisfy these expectations.
        let primary_absent = !Path::new(&format!("/proc/{primary}")).exists();
        let members_at_return = super::process_group_live_members(primary);
        let descendant_at_return = fs::read_to_string(format!("/proc/{descendant}/stat"));
        let artifact = fs::read(&destination);
        let sibling_bytes = fs::read(&sibling);
        let staging = fs::read_dir(&root)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.starts_with(".stdout.json.")
                    && name.ends_with(".tmp")
                    && entry.path() != sibling
            })
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        // Snapshot first, then clean a red witness through a revalidated pidfd.
        // The primary has been reaped, so the numeric group has no signal authority.
        let recorded = fs::read_to_string(root.join("ready.stat"))
            .map_err(|error| format!("read fixture descendant identity: {error}"))?;
        let fallback_cleanup = super::test_cleanup::finish_recorded_member(&recorded);
        let result = (|| {
            fallback_cleanup?;
            let members = members_at_return?;
            if !primary_absent || !members.is_empty() {
                return Err(format!(
                    "capture returned before owned cleanup: primary_absent={primary_absent}, live_group={members:?}, descendant={descendant_at_return:?}"
                ));
            }
            if let Ok(stat) = descendant_at_return
                && !matches!(
                    super::parse_stat_pgrp(&stat),
                    Some(super::ProcStatPgrp::NotLive)
                )
            {
                return Err(format!("descendant was still executing at return: {stat}"));
            }
            if wall >= Duration::from_secs(20) {
                return Err(format!(
                    "capture waited for the 30-second descendant: {wall:?}"
                ));
            }
            if sibling_bytes.map_err(|error| error.to_string())? != b"other-owner"
                || !staging.is_empty()
            {
                return Err(format!(
                    "attempt cleanup damaged sibling or left staging: {staging:?}"
                ));
            }
            let output = capture?;
            if output.timed_out != wait_primary
                || output
                    .status
                    .is_none_or(|status| status.success() == wait_primary)
            {
                return Err("capture lost its actual primary exit/timeout disposition".into());
            }
            if output.stdout_bytes != 18
                || artifact.map_err(|error| error.to_string())? != b"{\"complete\":true}\n"
            {
                return Err("settled capture must retain the 18 literal fixture bytes".into());
            }
            Ok(())
        })();
        fs::remove_dir_all(&root)
            .map_err(|error| format!("remove owned fixture {root:?}: {error}"))?;
        result
    }

    #[cfg(unix)]
    #[test]
    fn stdout_file_capture_stderr_error_preserves_prior_artifact() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let root = unique_pgid_marker("stderr-error").with_extension("dir");
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let destination = root.join("stdout.json");
        fs::write(&destination, b"prior-good-artifact").map_err(|error| error.to_string())?;
        let capture = capture_stdout_to_file_with_timeout(
            "sh",
            &[
                "-c".to_string(),
                "printf complete; printf '\\377' >&2".to_string(),
            ],
            &[("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1")],
            &destination,
            Duration::from_secs(10),
            "stderr read-error fixture",
        );
        let bytes = fs::read(&destination).map_err(|error| error.to_string())?;
        let entries = fs::read_dir(&root)
            .map_err(|error| error.to_string())?
            .count();
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        let error = match capture {
            Ok(_) => return Err("invalid UTF-8 in the real latency reader must refuse".into()),
            Err(error) => error,
        };
        assert!(error.contains("failed to read process output"), "{error}");
        assert_eq!(bytes, b"prior-good-artifact");
        assert_eq!(entries, 1, "failed capture left attempt staging");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn stdout_file_capture_complete_empty_and_nonzero_are_distinct() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        for (code, expected) in [(0, ""), (7, "nonzero-output")] {
            let destination = unique_pgid_marker("complete-file");
            let script = format!("printf '{expected}'; exit {code}");
            let output = capture_stdout_to_file_with_timeout(
                "sh",
                &["-c".to_string(), script],
                &[],
                &destination,
                Duration::from_secs(10),
                "complete stdout-file fixture",
            )?;
            let bytes = fs::read(&destination).map_err(|error| error.to_string())?;
            fs::remove_file(&destination).map_err(|error| error.to_string())?;
            assert_eq!(output.status.and_then(|status| status.code()), Some(code));
            assert!(!output.timed_out);
            assert_eq!(output.stdout_bytes, expected.len());
            assert_eq!(bytes, expected.as_bytes());
        }
        Ok(())
    }

    #[test]
    fn stdout_file_capture_empty_completion_is_not_a_drain_deadline() -> Result<(), String> {
        let (sender, receiver) = mpsc::channel::<Result<usize, String>>();
        sender.send(Ok(0)).map_err(|error| error.to_string())?;
        assert_eq!(
            super::drain_file_capture_bounded(&receiver, Duration::ZERO, "stdout", "empty file")?,
            0,
        );
        // Keep the sender alive: lack of completion must hit the deadline,
        // rather than testing the disconnected-worker branch.
        let error = match super::drain_file_capture_bounded(
            &receiver,
            Duration::from_millis(20),
            "stdout",
            "unfinished file",
        ) {
            Ok(_) => return Err("missing completion must not become zero-byte success".into()),
            Err(error) => error,
        };
        assert!(error.contains("stdout file capture incomplete"), "{error}");
        assert!(error.contains("artifact not published"), "{error}");
        drop(sender);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn stdout_file_capture_publication_failure_cleans_staging() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let root = unique_pgid_marker("publication-error").with_extension("dir");
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let destination = root.join("stdout.json");
        fs::create_dir(&destination).map_err(|error| error.to_string())?;
        fs::write(destination.join("blocker"), b"preserve").map_err(|error| error.to_string())?;
        let capture = capture_stdout_to_file_with_timeout(
            "sh",
            &["-c".to_string(), "printf complete".to_string()],
            &[],
            &destination,
            Duration::from_secs(10),
            "publication-error fixture",
        );
        let blocker = fs::read(destination.join("blocker")).map_err(|error| error.to_string())?;
        let entries = fs::read_dir(&root)
            .map_err(|error| error.to_string())?
            .count();
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        let error = match capture {
            Ok(_) => return Err("directory publication blocker must refuse".into()),
            Err(error) => error,
        };
        assert!(
            error.contains("failed to remove stale stdout file"),
            "{error}"
        );
        assert_eq!(blocker, b"preserve");
        assert_eq!(entries, 1, "failed publication left attempt staging");
        Ok(())
    }

    #[test]
    fn latency_progress_reader_preserves_captured_stderr() -> Result<(), String> {
        let stderr = "first\nripr_repo_exposure_latency phase=evidence_for_seams status=start duration_ms=0\nlast\n";
        let captured = read_stream_with_latency_progress(Cursor::new(stderr.as_bytes()))?;
        assert_eq!(captured, stderr);
        Ok(())
    }

    #[test]
    fn terminate_after_timeout_returns_false_for_already_finished_child() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let mut command = Command::new("rustc");
        command.arg("--version");
        command.stdout(Stdio::null()).stderr(Stdio::null());
        let mut child =
            OwnedProcess::spawn(command).map_err(|err| format!("spawn rustc version: {err}"))?;

        loop {
            if child
                .try_wait()
                .map_err(|err| format!("poll rustc version: {err}"))?
                .is_some()
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let termination_requested = terminate_after_timeout(&mut child, "rustc version")?;
        let status = child
            .wait()
            .map_err(|err| format!("wait for rustc version: {err}"))?;
        let timed_out = timeout_was_enforced(termination_requested, &status);
        if timed_out {
            return Err("finished process should not be reported as timed out".to_string());
        }
        Ok(())
    }

    #[test]
    fn timeout_was_enforced_reports_requested_termination() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let success = capture_output("rustc", &["--version"], "rustc version")?.status;
        let failure =
            capture_output("rustc", &["--ripr-invalid-test-flag"], "rustc invalid flag")?.status;

        if !timeout_was_enforced(true, &success) {
            return Err("requested termination should be reported as timeout".to_string());
        }
        if timeout_was_enforced(false, &failure) {
            return Err("failure without termination should not be a timeout".to_string());
        }
        if !timeout_was_enforced(true, &failure) {
            return Err("terminated failure should be treated as timeout".to_string());
        }
        Ok(())
    }

    #[test]
    fn parse_stat_pgrp_reads_the_field_after_comm() -> Result<(), String> {
        if parse_stat_pgrp("42 (sleep) S 1 99 0 -1")
            != Some(super::ProcStatPgrp::Live {
                state: 'S',
                pgrp: 99,
            })
        {
            return Err("pgrp is the third field after the closing comm parenthesis".to_string());
        }
        if parse_stat_pgrp("7 (name with spaces) R 3 88 0")
            != Some(super::ProcStatPgrp::Live {
                state: 'R',
                pgrp: 88,
            })
        {
            return Err("comm may contain spaces; parse after the last ')'".to_string());
        }
        if parse_stat_pgrp("10 (weird)name) S 1 3 0")
            != Some(super::ProcStatPgrp::Live {
                state: 'S',
                pgrp: 3,
            })
        {
            return Err("comm may contain ')'; parse after the last ')'".to_string());
        }
        if parse_stat_pgrp("9 (sleep) Z 1 9 9 0") != Some(super::ProcStatPgrp::NotLive) {
            return Err("zombie group members are not still-running".to_string());
        }
        if parse_stat_pgrp("no-paren-line").is_some() {
            return Err("malformed stat should not invent a pgrp".to_string());
        }
        if parse_stat_pgrp("9 (sleep) S").is_some() {
            return Err("truncated non-zombie stat must stay unparseable".to_string());
        }
        Ok(())
    }

    #[test]
    fn members_from_group_scan_requires_complete_read_only_observation() -> Result<(), String> {
        for live in [Vec::new(), vec![9, 12]] {
            let found = super::members_from_group_scan(
                1234,
                Some(super::ProcGroupScan::Complete { live: live.clone() }),
            )?;
            assert_eq!(found, live);
        }
        for (scan, expected) in [
            (Some(super::ProcGroupScan::Incomplete), "incomplete"),
            (None, "unavailable"),
        ] {
            match super::members_from_group_scan(1234, scan) {
                Err(error) if error.contains(expected) && error.contains("1234") => {}
                Err(error) => return Err(format!("wrong cleanup refusal: {error}")),
                Ok(members) => return Err(format!("unobserved absence was accepted: {members:?}")),
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn scan_proc_pgrp_includes_readable_other_uid_members() -> Result<(), String> {
        let Ok(stat) = fs::read_to_string("/proc/1/stat") else {
            return Ok(());
        };
        let Some(super::ProcStatPgrp::Live { pgrp, .. }) = parse_stat_pgrp(&stat) else {
            return Ok(());
        };
        match super::scan_proc_pgrp(pgrp) {
            Some(super::ProcGroupScan::Complete { live }) if live.contains(&1) => Ok(()),
            other => Err(format!(
                "readable pid 1 in group {pgrp} must appear in the scan, got {other:?}"
            )),
        }
    }

    #[test]
    fn proc_stat_error_is_gone_treats_vanished_tasks_not_permission() -> Result<(), String> {
        let not_found = std::io::Error::from(std::io::ErrorKind::NotFound);
        if !super::proc_stat_error_is_gone(&not_found) {
            return Err("ENOENT / NotFound must be a vanished member".to_string());
        }
        let permission = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        if super::proc_stat_error_is_gone(&permission) {
            return Err("PermissionDenied must stay unreadable, not gone".to_string());
        }
        let interrupted = std::io::Error::from(std::io::ErrorKind::Interrupted);
        if super::proc_stat_error_is_gone(&interrupted) {
            return Err("Interrupted must stay unreadable, not gone".to_string());
        }
        let eacces = std::io::Error::from_raw_os_error(13);
        if super::proc_stat_error_is_gone(&eacces) {
            return Err("EACCES must stay unreadable, not gone".to_string());
        }
        let eperm = std::io::Error::from_raw_os_error(1);
        if super::proc_stat_error_is_gone(&eperm) {
            return Err("EPERM must stay unreadable, not gone".to_string());
        }
        let esrch = std::io::Error::from_raw_os_error(super::PROC_STAT_ESRCH);
        if !super::proc_stat_error_is_gone(&esrch) {
            return Err(format!(
                "mid-scan ESRCH must be gone even when ErrorKind is {:?}, raw {:?}",
                esrch.kind(),
                esrch.raw_os_error()
            ));
        }
        let io_error = std::io::Error::from_raw_os_error(5);
        if super::proc_stat_error_is_gone(&io_error) {
            return Err("EIO / other OS errors must stay unreadable, not gone".to_string());
        }
        Ok(())
    }

    #[test]
    fn scan_from_stat_reads_keeps_live_members_when_unrelated_pids_exit() -> Result<(), String> {
        let live_stat = "1 (init) S 0 1 1 0";
        let other_group = "9 (sleep) S 1 99 0 -1";
        let esrch = std::io::Error::from_raw_os_error(super::PROC_STAT_ESRCH);
        let gone = std::io::Error::from(std::io::ErrorKind::NotFound);
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let eio = std::io::Error::from_raw_os_error(5);

        match super::scan_from_stat_reads(
            1,
            [
                (1, Ok(live_stat)),
                (4242, Err(&esrch)),
                (7, Ok(other_group)),
            ],
        ) {
            super::ProcGroupScan::Complete { live } if live == vec![1] => {}
            other => {
                return Err(format!(
                    "ESRCH on an unrelated pid must not drop a readable live member, got {other:?}"
                ));
            }
        }

        match super::scan_from_stat_reads(1, [(1, Ok(live_stat)), (8, Err(&gone))]) {
            super::ProcGroupScan::Complete { live } if live == vec![1] => {}
            other => {
                return Err(format!(
                    "NotFound on another pid must leave a complete live scan, got {other:?}"
                ));
            }
        }

        if super::scan_from_stat_reads(1, [(1, Ok(live_stat)), (3, Err(&denied))])
            != super::ProcGroupScan::Incomplete
        {
            return Err(
                "PermissionDenied must keep the scan Incomplete even when pid 1 is readable"
                    .to_string(),
            );
        }
        if super::scan_from_stat_reads(1, [(1, Ok(live_stat)), (4, Err(&eio))])
            != super::ProcGroupScan::Incomplete
        {
            return Err("non-ESRCH OS errors must keep the scan Incomplete".to_string());
        }
        if super::scan_from_stat_reads(1, [(1, Ok(live_stat)), (5, Ok("no-paren-line"))])
            != super::ProcGroupScan::Incomplete
        {
            return Err("unparseable stat must keep the scan Incomplete".to_string());
        }
        match super::scan_from_stat_reads(1, [(9, Err(&esrch))]) {
            super::ProcGroupScan::Complete { live } if live.is_empty() => {}
            other => {
                return Err(format!(
                    "a matching member that exits mid-scan must be gone, not Incomplete, got {other:?}"
                ));
            }
        }
        if super::contribute_proc_stat(1, Ok("9 (sleep) Z 1 1 1 0"))
            != super::ProcStatContribution::Ignore
        {
            return Err("zombies must not count as live members".to_string());
        }
        Ok(())
    }

    #[test]
    fn remove_stdout_capture_temp_deletes_the_file() -> Result<(), String> {
        let path = std::env::temp_dir().join(format!(
            "ripr-xtask-stdout-tmp-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::write(&path, "capture").map_err(|err| format!("write temp capture: {err}"))?;
        remove_stdout_capture_temp(&path)?;
        if path.exists() {
            let _ = fs::remove_file(&path);
            return Err("wait-error cleanup must remove the stdout temp file".to_string());
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn post_kill_group_confirm_grace_is_two_seconds() -> Result<(), String> {
        if POST_KILL_GROUP_CONFIRM_GRACE != Duration::from_secs(2) {
            return Err(format!(
                "POST_KILL_GROUP_CONFIRM_GRACE should be 2 s; got {:?}",
                POST_KILL_GROUP_CONFIRM_GRACE
            ));
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn confirm_process_group_gone_never_signals_a_live_observation_target() -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let (mut child, pgid) = spawn_sleep_in_own_group()?;
        wait_until_process_group_has_members(pgid)?;
        let result = super::confirm_process_group_gone(
            pgid,
            Duration::from_millis(100),
            "read-only confirmation",
        );
        let survived = child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none();
        // This handle still owns the waitable primary. Confirmation itself has
        // no termination authority, even though the test owns this child.
        child.terminate_tree()?;
        if !survived {
            return Err("read-only confirmation terminated its observation target".into());
        }
        match result {
            Err(error)
                if error.contains("could not confirm") && error.contains("still running") =>
            {
                Ok(())
            }
            Err(error) => Err(format!("wrong live-group refusal: {error}")),
            Ok(()) => Err("read-only confirmation claimed a live group was absent".into()),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn confirm_process_group_gone_reports_when_a_live_group_outlives_zero_budget()
    -> Result<(), String> {
        let _cwd_guard = acquire_test_cwd_read_guard();
        let (mut child, pgid) = spawn_sleep_in_own_group()?;
        wait_until_process_group_has_members(pgid)?;
        let result = super::confirm_process_group_gone(pgid, Duration::ZERO, "zero-budget");
        let still_live = super::process_group_live_members(pgid);
        child.terminate_tree()?;
        let error = match result {
            Err(error) => error,
            Ok(()) => return Err("zero-budget confirmation accepted a live group".into()),
        };
        for expected in ["could not confirm", "zero-budget", &pgid.to_string()] {
            if !error.contains(expected) {
                return Err(format!(
                    "unconfirmed cleanup must name {expected:?}: {error}"
                ));
            }
        }
        if still_live?.is_empty() {
            return Err("zero-budget confirmation terminated its observation target".into());
        }
        Ok(())
    }

    #[cfg(unix)]
    fn unique_pgid_marker(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ripr-xtask-pgid-{label}-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ))
    }

    #[cfg(target_os = "linux")]
    fn assert_recorded_timeout_left_process_group_gone(marker: &Path) -> Result<(), String> {
        let pgid_text = fs::read_to_string(marker).map_err(|err| {
            format!(
                "timed-out child should have recorded its pgid at {}: {err}",
                marker.display()
            )
        })?;
        let _ = fs::remove_file(marker);
        let pgid = pgid_text
            .trim()
            .parse::<u32>()
            .map_err(|err| format!("pgid marker should contain a pid, got {pgid_text:?}: {err}"))?;
        let members = super::process_group_live_members(pgid)?;
        if !members.is_empty() {
            return Err(format!(
                "recorded timeout coexisted with live process group {pgid} members {members:?}"
            ));
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn spawn_sleep_in_own_group() -> Result<(super::OwnedProcess, u32), String> {
        let mut command = Command::new("sleep");
        command.arg("30");
        command.stdout(Stdio::null()).stderr(Stdio::null());
        command.process_group(0);
        let child = super::OwnedProcess::spawn_isolated(command)
            .map_err(|err| format!("spawn sleep in its own process group: {err}"))?;
        let pgid = child.id();
        Ok((child, pgid))
    }

    #[cfg(target_os = "linux")]
    fn wait_until_process_group_has_members(pgid: u32) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match super::process_group_live_members(pgid) {
                Ok(members) if !members.is_empty() => return Ok(()),
                Ok(_) => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "sleep process group {pgid} did not appear in /proc"
                        ));
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// A `Read` implementation that blocks indefinitely on every `read` call.
    ///
    /// This simulates a pipe whose write-end is held open by a descendant that
    /// escaped the process-group kill.  It is used to exercise the
    /// `drain_stream_reader_bounded` grace-timeout path at the unit level
    /// without spawning a real process tree.
    ///
    /// Note on Windows/platform portability: the owned Job Object tree now
    /// terminates descendants deterministically (see #3803), but this
    /// unit-level seam test remains the authoritative check for the
    /// bounded-drain guarantee on all platforms without a real process tree.
    struct BlockingRead {
        /// Receiving on this channel blocks until the sender half is dropped,
        /// i.e. forever from the `Read` side.
        _park: mpsc::Receiver<()>,
    }

    impl BlockingRead {
        fn new() -> (Self, mpsc::SyncSender<()>) {
            let (tx, rx) = mpsc::sync_channel(0);
            (BlockingRead { _park: rx }, tx)
        }
    }

    impl Read for BlockingRead {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            // Block until the sender half (held by the test to keep the "pipe"
            // open) is dropped.  A real escaped descendant behaves identically:
            // it holds the write-end of the inherited pipe, and `read_to_end`
            // on the read-end never returns until that fd is closed.
            match self._park.recv() {
                Ok(()) | Err(mpsc::RecvError) => Ok(0),
            }
        }
    }

    /// The bounded-drain path must return within `grace + slack` even when the
    /// reader thread is permanently blocked (simulating an escaped descendant
    /// holding the pipe write-end open after a process-group kill).
    ///
    /// Uses a short synthetic grace to keep the test fast.  The production
    /// `POST_KILL_DRAIN_GRACE` constant is also verified to equal 5 s so the
    /// actual timeout is deterministic in CI.
    #[test]
    fn drain_stream_reader_bounded_returns_within_grace_when_pipe_stays_open() -> Result<(), String>
    {
        // Sanity-check the production constant so reviewers know what the real
        // bound is.
        if POST_KILL_DRAIN_GRACE != Duration::from_secs(5) {
            return Err(format!(
                "POST_KILL_DRAIN_GRACE should be 5 s; got {:?}",
                POST_KILL_DRAIN_GRACE
            ));
        }

        // Use a short synthetic grace (200 ms) to keep the test fast while
        // still proving the timeout fires.
        let test_grace = Duration::from_millis(200);
        let (blocking, _keeper) = BlockingRead::new();
        // _keeper is kept alive so the BlockingRead::read never returns EOF —
        // it stays blocked for the entire test, simulating the escaped
        // descendant scenario.

        let (handle, rx) = spawn_stream_reader_channel(blocking);

        // The reader never unblocks, so returning at all proves the grace fired.
        let result =
            drain_stream_reader_bounded(rx, handle, test_grace, "stdout", "test-context", false)?;

        // The result must contain the diagnostic message indicating output was
        // truncated due to the escaped-descendant scenario.
        if !result.contains("drain exceeded post-kill grace") {
            return Err(format!(
                "truncated drain should include diagnostic message; got: {result:?}"
            ));
        }
        if !result.contains("test-context") {
            return Err(format!(
                "diagnostic message should name the error context; got: {result:?}"
            ));
        }

        Ok(())
    }

    #[test]
    fn complete_capture_rejects_a_pipe_that_outlives_the_drain_grace() -> Result<(), String> {
        let (blocking, _keeper) = BlockingRead::new();
        let (handle, rx) = spawn_stream_reader_channel(blocking);
        let result = drain_stream_reader_bounded(
            rx,
            handle,
            Duration::from_millis(50),
            "stdout",
            "complete capture control",
            true,
        );
        match result {
            Err(error) if error.contains("complete output is not established") => Ok(()),
            other => Err(format!(
                "an unfinished reader must fail complete capture, got {other:?}"
            )),
        }
    }

    #[test]
    fn typed_capture_retains_cleanup_failure_when_both_drains_also_fail() -> Result<(), String> {
        let error = super::settle_timed_capture_streams(
            Err("stdout drain failed".to_string()),
            Err("stderr drain failed".to_string()),
            Err("owned descendant remained live".to_string()),
            true,
            "mixed failure control",
        )
        .err()
        .ok_or("mixed cleanup/capture failure was accepted")?;
        assert_eq!(error.kind, super::TimedCaptureErrorKind::Cleanup);
        assert!(error.message.contains("owned descendant remained live"));
        assert!(error.message.contains("stdout drain failed"));
        assert!(error.message.contains("stderr drain failed"));

        let legacy = super::settle_timed_capture_streams(
            Ok("version text".to_string()),
            Err("stderr drain failed".to_string()),
            Err("owned descendant remained live".to_string()),
            false,
            "legacy failure control",
        )
        .err()
        .ok_or("legacy capture failure was accepted")?;
        assert_eq!(legacy.kind, super::TimedCaptureErrorKind::Capture);
        assert_eq!(legacy.into_message(), "stderr drain failed");
        Ok(())
    }
}

#[cfg(all(test, unix))]
#[path = "run/input_tests.rs"]
mod input_tests;
