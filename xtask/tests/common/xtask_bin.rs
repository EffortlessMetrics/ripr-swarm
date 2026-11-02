//! Spawn the workspace `xtask` binary from integration tests.
//!
//! On Windows, nested workspace builds can briefly remove `target/debug/xtask.exe`
//! while the driver bootstrap restores it (#3699, #4293). A concurrent
//! `CARGO_BIN_EXE_xtask` spawn then surfaces as `NotFound (os error 2)`. The
//! same race can affect `ripr.exe` (#9272); retry only that launch error with
//! a bounded backoff.

use std::process::Command;
use std::process::Output;
use std::thread;
use std::time::Duration;

const MAX_NOT_FOUND_ATTEMPTS: u32 = 20;
const NOT_FOUND_BACKOFF: Duration = Duration::from_millis(500);

/// Run a configured `xtask` command, retrying a bounded number of times when
/// the canonical binary path is briefly missing at launch.
pub fn output(configure: impl Fn(&mut Command)) -> Result<Output, String> {
    let mut attempt = 0;
    loop {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        configure(&mut command);
        match command.output() {
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && attempt < MAX_NOT_FOUND_ATTEMPTS =>
            {
                attempt += 1;
                thread::sleep(NOT_FOUND_BACKOFF);
            }
            other => return other.map_err(|error| format!("failed to run xtask: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_not_found_retry_constants_match_windows_advisory_launch() {
        assert_eq!(MAX_NOT_FOUND_ATTEMPTS, 20);
        assert_eq!(NOT_FOUND_BACKOFF, Duration::from_millis(500));
    }

    #[test]
    fn canonical_xtask_path_launches() -> Result<(), String> {
        let output = output(|command| {
            command.arg("help");
        })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "xtask help failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
}
