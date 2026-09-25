//! Shared owned-subprocess authority for bounded commands (#3803).
//!
//! Every shipped bounded subprocess path — the git invocation helpers, the
//! Perl facts exporter probe, the cargo metadata probe, and the `xtask`
//! timed-command runners — spawns its child through [`OwnedProcess`] so one
//! typed owner holds the process, its termination, wait and cleanup. On
//! Windows the owner guarantees Job Object containment with
//! assign-before-execution:
//!
//! ```text
//! create primary process suspended (CREATE_SUSPENDED)
//! → create Job Object + assign the suspended process
//! → resume execution
//! → bounded observation by the caller
//! → on timeout/cancellation/error terminate the owned job
//! → wait/reap the primary process
//! → only then release ownership
//! ```
//!
//! Because the assignment happens before any child user code can run, every
//! descendant the child later spawns joins the job, and terminating the job
//! (or ending ownership) kills the whole tree. No orphaned descendant can
//! outlive the owner to hold Windows file or working-directory locks. This
//! replaces the previous `taskkill /PID <pid> /T /F` shell dependency, whose
//! snapshot-based tree walk was racy (#3096, #1022) and which silently
//! degraded to a direct-child kill whenever `taskkill` was missing or
//! reported success before the parent had exited.
//!
//! Ownership is never dropped without cleanup: [`Drop`] terminates the job
//! and reaps the direct child even on panic unwinding. Non-Windows targets
//! keep their existing behavior: a transparent `std::process::Child`
//! passthrough whose termination kills and reaps the direct child, with the
//! Unix process-group authority unchanged in its callers.

use std::process::{ChildStderr, ChildStdout, Command, ExitStatus};
use std::time::Duration;

#[cfg(windows)]
use process_wrap::std::{ChildWrapper, CommandWrap, JobObject};

/// Poll interval for the owner's bounded reap loop. Matches the shared
/// deadline poll in [`crate::git`] so a terminating tree is reaped on the
/// same cadence the callers already budget for.
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// One owned child process for a bounded command path.
///
/// Windows: the child is created suspended, assigned to a Job Object before
/// any child user code executes, and only then resumed. Any descendant it
/// spawns joins the job, so [`OwnedProcess::terminate_tree`] (or ending
/// ownership) kills the entire assigned tree. Other platforms: a transparent
/// wrapper over `std::process::Child`; termination kills and reaps the
/// direct child exactly as the pre-#3803 helpers did.
#[derive(Debug)]
pub struct OwnedProcess {
    #[cfg(windows)]
    child: Box<dyn ChildWrapper>,
    #[cfg(not(windows))]
    child: std::process::Child,
}

impl OwnedProcess {
    /// Spawn the fully prepared `command` under owned containment.
    ///
    /// Stdio redirection, environment, working directory, arguments and any
    /// platform process-group configuration must be applied to `command`
    /// before this call; they are preserved exactly. On Windows the process
    /// is created with `CREATE_SUSPENDED`, assigned to a fresh Job Object,
    /// and resumed inside this call. A setup failure (job creation,
    /// assignment, or resume) is returned as a typed `std::io::Error` and
    /// the suspended or partially set-up child is terminated and reaped
    /// before the error surfaces, so no half-owned process escapes.
    pub fn spawn(command: Command) -> std::io::Result<Self> {
        #[cfg(windows)]
        {
            let mut wrapped = CommandWrap::from(command);
            wrapped.wrap(JobObject);
            let child = wrapped.spawn()?;
            Ok(Self { child })
        }
        #[cfg(not(windows))]
        {
            // The passthrough spawn configures the prepared command in
            // place; the owned binding keeps the call site identical to
            // the Windows arm's by-value consumption.
            let mut command = command;
            let child = command.spawn()?;
            Ok(Self { child })
        }
    }

    /// The child's process id.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Take the piped stdout declared on the command before spawning.
    pub fn stdout_pipe(&mut self) -> &mut Option<ChildStdout> {
        #[cfg(windows)]
        {
            self.child.stdout()
        }
        #[cfg(not(windows))]
        {
            &mut self.child.stdout
        }
    }

    /// Take the piped stderr declared on the command before spawning.
    pub fn stderr_pipe(&mut self) -> &mut Option<ChildStderr> {
        #[cfg(windows)]
        {
            self.child.stderr()
        }
        #[cfg(not(windows))]
        {
            &mut self.child.stderr
        }
    }

    /// Non-blocking exit check for the direct child.
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Reap the direct child to completion.
    ///
    /// This polls the child instead of blocking on a job completion port so
    /// the wait stays bounded by the caller's termination decisions and can
    /// never hang on queued job notifications.
    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(WAIT_POLL_INTERVAL);
        }
    }

    /// Request termination of the owned primary process without waiting.
    ///
    /// Windows: the Job Object kill request. Other platforms:
    /// `std::process::Child::kill`, which — like the Job Object request —
    /// returns `Ok(())` for an already-exited child and never waits, so the
    /// owner's terminate-then-reap sequence stays the single reaping
    /// authority.
    fn request_kill(&mut self) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            self.child.start_kill()
        }
        #[cfg(not(windows))]
        {
            self.child.kill()
        }
    }

    /// Terminate the whole owned tree and reap the direct child before
    /// returning. Windows: one termination request kills every process in
    /// the job. Other platforms: the direct child kill and reap.
    ///
    /// The `Err` arm reports the typed termination or reap failure; the
    /// reap is still attempted in that case so no zombie remains.
    pub fn terminate_tree(&mut self) -> Result<(), String> {
        let termination = self.request_kill().map_err(|err| err.to_string());
        let reap = self.wait();
        match (termination, reap) {
            (Err(err), _) => Err(format!("failed to terminate owned process: {err}")),
            (_, Err(err)) => Err(format!("failed to reap owned process: {err}")),
            (Ok(()), Ok(_)) => Ok(()),
        }
    }

    /// Direct-child kill request without tree scope or reaping.
    ///
    /// This is only the narrow fallback for a failed tree-termination
    /// request; ownership paths must prefer [`OwnedProcess::terminate_tree`].
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        // Kill-on-close semantics live at this boundary: whichever path
        // releases the owner — normal completion, timeout, cancellation,
        // wait failure, or panic unwinding — terminates the owned tree and
        // reaps the direct child first. Both steps are best-effort here;
        // typed evidence belongs to `terminate_tree`, which every explicit
        // termination path uses.
        let _ = self.request_kill();
        let _ = self.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::process::Stdio;

    #[cfg(windows)]
    fn platform_sleep_command(seconds: u32) -> Command {
        #[cfg(windows)]
        {
            let mut command = Command::new("powershell");
            command.args([
                "-NoProfile",
                "-Command",
                &format!("Start-Sleep -Seconds {seconds}"),
            ]);
            command
        }
        #[cfg(not(windows))]
        {
            let mut command = Command::new("sleep");
            command.arg(seconds.to_string());
            command
        }
    }

    #[cfg(windows)]
    fn alive_on_windows(pid: u32) -> bool {
        #[cfg(windows)]
        {
            Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
                    ),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            false
        }
    }

    /// Normal completion, captured output and pipe teardown through the
    /// owner (native control 1: normal exit and complete output).
    #[test]
    fn owned_process_completes_and_captures_output() -> Result<(), String> {
        let mut command = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            command.args(["/C", "echo ripr-owned-ok"]);
        } else {
            command.args(["-c", "echo ripr-owned-ok"]);
        }
        command.stdin(Stdio::null()).stdout(Stdio::piped());
        let mut owned = OwnedProcess::spawn(command).map_err(|err| format!("spawn: {err}"))?;
        let mut stdout = owned
            .stdout_pipe()
            .take()
            .ok_or_else(|| "stdout pipe missing".to_string())?;
        let status = owned.wait().map_err(|err| format!("wait: {err}"))?;
        let mut captured = String::new();
        stdout
            .read_to_string(&mut captured)
            .map_err(|err| format!("read: {err}"))?;
        drop(owned);
        if !status.success() {
            return Err(format!("child should exit successfully, got {status}"));
        }
        if !captured.contains("ripr-owned-ok") {
            return Err(format!(
                "captured stdout should carry the marker: {captured}"
            ));
        }
        Ok(())
    }

    /// Environment and working-directory configuration survive the owned
    /// spawn (native control 11: arguments/environment fidelity).
    #[test]
    fn owned_process_preserves_environment_and_directory() -> Result<(), String> {
        let mut command = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            command.args(["/C", "echo %RIPR_OWNER_TEST_MARKER%"]);
        } else {
            command.args(["-c", "echo $RIPR_OWNER_TEST_MARKER"]);
        }
        command
            .env("RIPR_OWNER_TEST_MARKER", "env-fidelity-ok")
            .stdin(Stdio::null())
            .stdout(Stdio::piped());
        let mut owned = OwnedProcess::spawn(command).map_err(|err| format!("spawn: {err}"))?;
        let mut stdout = owned
            .stdout_pipe()
            .take()
            .ok_or_else(|| "stdout pipe missing".to_string())?;
        let status = owned.wait().map_err(|err| format!("wait: {err}"))?;
        let mut captured = String::new();
        stdout
            .read_to_string(&mut captured)
            .map_err(|err| format!("read: {err}"))?;
        drop(owned);
        if !status.success() || !captured.contains("env-fidelity-ok") {
            return Err(format!(
                "owned spawn should preserve child environment (status {status}): {captured}"
            ));
        }
        Ok(())
    }

    /// A still-running owned child is terminated and reaped when the owner
    /// is dropped without an explicit wait — the kill-on-close owner
    /// boundary (Windows host-verified containment).
    #[cfg(windows)]
    #[test]
    fn owner_drop_terminates_a_still_running_child() -> Result<(), String> {
        let command = platform_sleep_command(60);
        let pid;
        {
            let owned = OwnedProcess::spawn(command).map_err(|err| format!("spawn: {err}"))?;
            pid = owned.id();
            // Give the child one poll window to reach Start-Sleep so the
            // later liveness check observes the terminated process, not a
            // process that had not fully started.
            std::thread::sleep(Duration::from_millis(500));
            drop(owned);
        }
        if alive_on_windows(pid) {
            return Err(format!("owned child {pid} persisted past owner drop"));
        }
        Ok(())
    }

    /// A descendant created by the owned child is terminated by
    /// `terminate_tree` even when it holds inherited pipe handles, and the
    /// call returns after the direct child is reaped (native controls 3-5).
    #[cfg(windows)]
    #[test]
    fn terminate_tree_kills_pipe_inheriting_descendants() -> Result<(), String> {
        let marker_path =
            std::env::temp_dir().join(format!("ripr-owner-descendant-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&marker_path);
        let marker_path_text = marker_path.display().to_string().replace('\'', "''");
        let mut command = Command::new("powershell");
        command.args([
            "-NoProfile",
            "-Command",
            &format!(
                "$p = Start-Process -FilePath powershell -ArgumentList @('-NoProfile','-Command','Start-Sleep -Seconds 60') -NoNewWindow -PassThru; Set-Content -LiteralPath '{marker_path_text}' -Value $p.Id; Wait-Process -Id $p.Id"
            ),
        ]);
        command.stdin(Stdio::null()).stdout(Stdio::piped());
        let mut owned = OwnedProcess::spawn(command).map_err(|err| format!("spawn: {err}"))?;
        // Wait for the marker so the descendant demonstrably exists before
        // termination; a missing marker after a generous window is a setup
        // failure, not a containment success.
        let mut marker = Err("marker not written".to_string());
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(&marker_path) {
                marker = text.trim().parse::<u32>().map_err(|err| err.to_string());
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let descendant_pid = marker.map_err(|err| format!("descendant PID marker: {err}"))?;
        let termination = owned.terminate_tree();
        let _ = std::fs::remove_file(&marker_path);
        termination?;
        if alive_on_windows(descendant_pid) {
            return Err(format!(
                "descendant {descendant_pid} persisted past terminate_tree; tree containment not established"
            ));
        }
        Ok(())
    }

    /// Termination of an owned tree leaves unrelated same-name processes
    /// alone (native control 10).
    #[cfg(windows)]
    #[test]
    fn terminate_tree_leaves_unrelated_processes_alive() -> Result<(), String> {
        let mut unrelated_command = platform_sleep_command(5);
        let mut unrelated = unrelated_command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("spawn unrelated: {err}"))?;
        let unrelated_pid = unrelated.id();

        let owned_result = OwnedProcess::spawn(platform_sleep_command(60));
        let mut owned = match owned_result {
            Ok(owned) => owned,
            Err(err) => {
                let _ = unrelated.kill();
                let _ = unrelated.wait();
                return Err(format!("spawn owned: {err}"));
            }
        };
        std::thread::sleep(Duration::from_millis(500));
        let termination = owned.terminate_tree();

        let unrelated_alive = alive_on_windows(unrelated_pid);
        let _ = unrelated.kill();
        let _ = unrelated.wait();
        termination?;
        if !unrelated_alive {
            return Err("terminate_tree terminated an unrelated same-name process".to_string());
        }
        Ok(())
    }
}
