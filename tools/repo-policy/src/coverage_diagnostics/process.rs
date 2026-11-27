//! Private coverage command ownership and bounded record delivery (#4978).

use serde_json::{Value, json};
use std::io::{self, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

use super::Result;

pub(super) const CARGO_ARGS: &[&str] = &[
    "llvm-cov",
    "--workspace",
    "--all-features",
    "--lcov",
    "--output-path",
    "lcov.info",
];
// Linux PIPE_BUF: one bounded write cannot interleave with child pipe writes.
pub(super) const LINE_BYTES: usize = 4096;

pub(super) fn coverage_command() -> Command {
    let mut command = Command::new("cargo");
    command.args(CARGO_ARGS);
    // Inherit all three standard streams and every environment variable.
    command
}

pub(super) fn write_record(writer: &mut impl Write, line: &str) -> io::Result<()> {
    // Do not split a record across writes while Cargo shares the inherited pipe.
    if writer.write(line.as_bytes())? != line.len() {
        return Err(io::Error::new(io::ErrorKind::WriteZero, "partial receipt"));
    }
    writer.flush()
}

pub(super) struct CoverageChild {
    child: Child,
    pid: rustix::process::Pid,
    closed: bool,
}

impl CoverageChild {
    pub(super) fn spawn(mut command: Command) -> Result<Self> {
        command.process_group(0);
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let pid = match i32::try_from(child.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        {
            Some(pid) => pid,
            None => {
                let _ = child.kill();
                return Err("coverage child id is outside the positive PID range".into());
            }
        };
        Ok(Self {
            child,
            pid,
            closed: false,
        })
    }

    pub(super) fn exited(&mut self) -> io::Result<bool> {
        use rustix::process::{WaitId, WaitIdOptions, waitid};
        loop {
            match waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => return Ok(status.is_some()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => {
                    // An externally reaped child no longer supplies PID custody.
                    // Disarm all numeric signals instead of risking a reused PID.
                    self.closed = true;
                    return Err(error.into());
                }
            }
        }
    }

    pub(super) fn cleanup(&mut self) -> (Option<ExitStatus>, Option<&'static str>) {
        if self.closed || self.exited().is_err() {
            return (None, Some("owned_child_custody_unavailable"));
        }
        // Use the shared owner's non-reaping ordering: signal the owned group
        // while its primary PID is still reserved, then reap exactly once.
        let group_error =
            match rustix::process::kill_process_group(self.pid, rustix::process::Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => false,
                Err(_) => true,
            };
        if group_error {
            let _ = self.child.kill();
        }
        let started = Instant::now();
        let mut status = None;
        while started.elapsed() < Duration::from_secs(1) {
            match self.child.try_wait() {
                Ok(Some(observed)) => {
                    status = Some(observed);
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        self.closed = true;
        let error = if group_error || status.is_none() {
            Some("owned_group_cleanup_incomplete")
        } else {
            None
        };
        (status, error)
    }
}

impl Drop for CoverageChild {
    fn drop(&mut self) {
        if !self.closed
            && let (_, Some(error)) = self.cleanup()
        {
            let _ = writeln!(io::stderr().lock(), "coverage storage observer: {error}");
        }
    }
}

fn native_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

pub(super) fn run_observed(
    command: Command,
    interval: Duration,
    periodic_limit: usize,
    mut observe: impl FnMut(Value),
) -> Result<i32> {
    observe(json!({"phase": "before"}));
    let mut child = match CoverageChild::spawn(command) {
        Ok(child) => child,
        Err(error) => {
            observe(json!({"phase": "final", "error": "coverage_spawn_failed"}));
            return Err(error);
        }
    };
    let mut last_sample = Instant::now();
    let mut samples = 0;
    loop {
        match child.exited() {
            Ok(true) => {
                let (status, cleanup) = child.cleanup();
                let Some(status) = status else {
                    observe(json!({
                        "phase": "final",
                        "error": "native_status_unavailable",
                        "cleanup_error": cleanup,
                    }));
                    return Err("coverage child status unavailable after cleanup".into());
                };
                observe(json!({
                    "phase": "final",
                    "exit": status.code(),
                    "signal": status.signal(),
                    "shell_status": native_code(status),
                    "cleanup_error": cleanup,
                    "periodic_samples": samples,
                }));
                return Ok(native_code(status));
            }
            Ok(false) => {}
            Err(error) => {
                let (_, cleanup) = child.cleanup();
                observe(json!({
                    "phase": "final",
                    "error": "native_observation_failed",
                    "cleanup_error": cleanup,
                }));
                return Err(format!("observe coverage process: {error}"));
            }
        }
        if samples < periodic_limit && last_sample.elapsed() >= interval {
            samples += 1;
            observe(json!({"phase": "during", "sample": samples}));
            last_sample = Instant::now();
        }
        // Reaching the observation limit never terminates the coverage command.
        // The existing workflow timeout remains the command's execution bound.
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
