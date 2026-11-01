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
//! A force-terminated owner runs no `Drop`, so the binaries also call
//! [`bind_process_lifetime`] once at startup: the owner process joins one
//! kill-on-close Job Object and the OS ends every descendant when the owner
//! dies by any route (#9564).
//!
//! Ownership is never dropped without cleanup: [`Drop`] terminates the job
//! and reaps the direct child even on panic unwinding. Unix MCP attempts opt
//! into process-group containment through their captured cancellation token.
//! Other callers retain their prepared process-group configuration. Group
//! cleanup precedes the OS reap, which releases PID custody.

use std::process::{ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus};
use std::time::{Duration, Instant};

#[cfg(windows)]
use process_wrap::std::{ChildWrapper, CommandWrap, JobObject};

/// Bind the calling process's lifetime to its descendants (#9564, #9323 F2).
///
/// Windows: puts the current process in one Job Object created with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and keeps its only handle open for
/// the rest of the process. The OS closes that handle when the process ends
/// by *any* route, including `TerminateProcess`, a runner teardown or an
/// editor force-kill, which kills every process still in the job. That is the
/// one case [`OwnedProcess`]'s `Drop` cannot cover, because no user code runs
/// on a hard kill. The per-child jobs that `process-wrap` creates are
/// unchanged and nest inside this job (Windows 8 and later).
///
/// Why the owner is bound, not each child job: `process-wrap` 10 hard-codes
/// the per-child job without the kill-on-close flag and keeps its handle
/// private, so the flag cannot be set from outside without `unsafe` code or a
/// fork. Why this is safe for ripr: every shipped `OwnedProcess` caller is a
/// bounded command (git, the cargo metadata and Perl probes, xtask test and
/// verify runners) that must not outlive the process that asked for it, and no
/// ripr path intentionally leaves a detached child behind. Closing the job at
/// a normal exit therefore ends only processes that should not outlive the
/// owner; a descendant that outlives it by design (a build-cache daemon started
/// by a plain `cargo`, for example) is also ended and restarts on its next use.
///
/// Call this once, first thing in a binary's `main`. It is deliberately not
/// called from [`OwnedProcess::spawn`] so that a program embedding the `ripr`
/// library is never silently placed in a kill-on-close job. It is idempotent;
/// repeated calls return the first outcome.
///
/// Best effort: an `Err` (for example a host job that forbids nesting on a
/// pre-Windows-8 system) leaves the pre-existing behavior in place: `Drop`
/// still covers exit, timeout, error and unwinding. Other platforms: no-op,
/// `Ok`; Unix process-group containment is configured per spawn.
pub fn bind_process_lifetime() -> Result<(), String> {
    static OUTCOME: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    bind_once(&OUTCOME, platform_bind_process_lifetime)
}

/// Run `bind` at most once per `slot` and return the recorded outcome to every
/// caller, so a second call can never create a second job or re-attempt a
/// failed assignment.
fn bind_once(
    slot: &std::sync::OnceLock<Result<(), String>>,
    bind: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    slot.get_or_init(bind).clone()
}

#[cfg(windows)]
fn platform_bind_process_lifetime() -> Result<(), String> {
    let mut limits = win32job::ExtendedLimitInfo::new();
    limits.limit_kill_on_job_close();
    let job = win32job::Job::create_with_limit_info(&limits)
        .map_err(|err| format!("create kill-on-close job: {err}"))?;
    job.assign_current_process()
        .map_err(|err| format!("assign current process to kill-on-close job: {err}"))?;
    // The job's handle must stay open until the process ends: closing it is
    // what kills the descendants. `into_handle` releases it without closing.
    let _leaked_for_process_lifetime = job.into_handle();
    Ok(())
}

#[cfg(not(windows))]
fn platform_bind_process_lifetime() -> Result<(), String> {
    Ok(())
}

/// First sleep of a child-exit poll. Short-lived probes (`git rev-parse`,
/// `git diff` on a small repository) exit within a few milliseconds, so the
/// first re-check must come back on that scale (#5348).
const POLL_BACKOFF_FIRST: Duration = Duration::from_millis(1);

/// Ceiling of the child-exit poll backoff. A long-running child is still
/// re-checked (and cancellation/deadline observed) at least this often.
pub(crate) const POLL_BACKOFF_CEILING: Duration = Duration::from_millis(50);

/// Exponential sleep schedule for `try_wait` poll loops: 1 ms, doubling, up
/// to [`POLL_BACKOFF_CEILING`].
///
/// A fixed 50 ms interval made every short subprocess cost at least 50 ms of
/// wall time because the first `try_wait` right after spawn practically
/// always sees a running child; `ripr check` on a tiny crate spent ~0.35 s of
/// its ~0.4 s sleeping on eight `git` probes that had each exited in ~3 ms
/// (#5348). The ceiling keeps the long-running cadence (and the number of
/// wakeups for a slow child) where it was.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PollBackoff {
    next: Duration,
}

impl PollBackoff {
    pub(crate) const fn new() -> Self {
        Self {
            next: POLL_BACKOFF_FIRST,
        }
    }

    /// The delay to sleep before the next poll; advances the schedule.
    pub(crate) fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = self.next.saturating_mul(2).min(POLL_BACKOFF_CEILING);
        delay
    }

    /// Next poll delay after applying an optional deadline clamp at `now`.
    pub(crate) fn clamped_delay(&mut self, deadline: Option<Instant>, now: Instant) -> Duration {
        let mut delay = self.next_delay();
        if let Some(deadline) = deadline {
            delay = delay.min(deadline.saturating_duration_since(now));
        }
        delay
    }

    /// Sleep for the next delay, never past `deadline` when one is given.
    pub(crate) fn sleep(&mut self, deadline: Option<Instant>) {
        let delay = self.clamped_delay(deadline, Instant::now());
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
}

/// Budget for the best-effort primary reap after a failed tree-termination
/// request. Long enough to observe a dying primary on a loaded host, short
/// enough that the returned cleanup-failure error is never delayed by a
/// process that refuses to die.
const FALLBACK_REAP_BUDGET: Duration = Duration::from_secs(5);

/// One owned child process for a bounded command path.
///
/// Windows: the child is created suspended, assigned to a Job Object before
/// any child user code executes, and only then resumed. Any descendant it
/// spawns joins the job, so [`OwnedProcess::terminate_tree`] (or ending
/// ownership) kills the entire assigned tree. Unix MCP attempts give each
/// child a fresh process group, including pipe-inheriting descendants;
/// other Unix callers preserve their existing process-group configuration. Other
/// platforms: a transparent wrapper over `std::process::Child`;
/// termination kills and reaps the direct child exactly as the
/// pre-#3803 helpers did.
#[derive(Debug)]
pub struct OwnedProcess {
    #[cfg(windows)]
    child: Box<dyn ChildWrapper>,
    #[cfg(not(windows))]
    child: std::process::Child,
    #[cfg(unix)]
    group_owned: bool,
    #[cfg(unix)]
    custody_lost: bool,
    /// Unix PID custody: the group signal has been issued and the primary
    /// reaped. Once the leader is reaped its pgid becomes recyclable by an
    /// unrelated process group, so every later release path — `Drop`, a
    /// repeated `terminate_tree` — must stay disarmed rather than risk
    /// signaling a group that reused the number (#9895 review).
    #[cfg(all(unix, not(windows)))]
    group_settled: bool,
    /// Unix test oracle: the custody contract allows exactly one group
    /// signal per owner lifetime; each `request_kill` on unix increments
    /// this counter so a witness can pin it.
    #[cfg(all(test, unix, not(windows)))]
    test_group_kills: usize,
    #[cfg(all(test, unix))]
    test_direct_kills: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Test-only failure injection (#9323 controls 8 and 9): when set, the
    /// tree-termination request reports this error instead of being issued,
    /// so the typed cleanup-failure path can be exercised on every platform.
    #[cfg(test)]
    injected_kill_failure: Option<std::io::ErrorKind>,
}

impl OwnedProcess {
    /// Spawn the fully prepared `command` under owned containment.
    ///
    /// Stdio redirection, environment, working directory, arguments and any
    /// platform process-group configuration must be applied to `command`
    /// before this call. Outside MCP attempt scopes they are preserved exactly;
    /// an MCP attempt deliberately creates a fresh Unix process group. On Windows the process
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
            Ok(Self::from_child(child))
        }
        #[cfg(all(unix, not(windows)))]
        {
            let group_owned = crate::analysis::cancellation::owns_process_groups();
            Self::spawn_with_group(command, group_owned)
        }
        #[cfg(not(any(windows, unix)))]
        {
            // The passthrough spawn configures the prepared command in
            // place; the owned binding keeps the call site identical to
            // the Windows arm's by-value consumption.
            let mut command = command;
            let child = command.spawn()?;
            Ok(Self::from_child(child))
        }
    }

    /// Spawn with explicit descendant containment for an owned execution.
    ///
    /// The installed ordinary-test journey uses this entry point outside an
    /// analysis cancellation scope. On supported Unix platforms it creates a
    /// fresh process group and retains the existing prereap PID custody. This
    /// replaces any caller-prepared group setting. Windows uses the same Job
    /// Object setup as [`Self::spawn`]. Platforms without those containment
    /// primitives refuse before spawning. The default [`Self::spawn`] behavior
    /// and its caller-prepared group settings remain unchanged.
    pub fn spawn_isolated(command: Command) -> std::io::Result<Self> {
        #[cfg(all(unix, not(windows)))]
        {
            Self::spawn_with_group(command, true)
        }
        #[cfg(windows)]
        {
            Self::spawn(command)
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = command;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "isolated process execution requires descendant containment",
            ))
        }
    }

    #[cfg(all(unix, not(windows)))]
    fn spawn_with_group(mut command: Command, group_owned: bool) -> std::io::Result<Self> {
        use std::os::unix::process::CommandExt as _;
        if group_owned {
            ensure_group_observation_supported()?;
            command.process_group(0);
        }
        let child = command.spawn()?;
        Ok(Self {
            child,
            group_owned,
            custody_lost: false,
            group_settled: false,
            #[cfg(test)]
            test_group_kills: 0,
            #[cfg(test)]
            test_direct_kills: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            injected_kill_failure: None,
        })
    }

    #[cfg(windows)]
    fn from_child(child: Box<dyn ChildWrapper>) -> Self {
        Self {
            child,
            #[cfg(test)]
            injected_kill_failure: None,
        }
    }

    #[cfg(not(any(windows, unix)))]
    fn from_child(child: std::process::Child) -> Self {
        Self {
            child,
            #[cfg(test)]
            injected_kill_failure: None,
        }
    }

    /// The child's process id.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Take the piped stdin declared on the command before spawning.
    pub fn stdin_pipe(&mut self) -> &mut Option<ChildStdin> {
        #[cfg(windows)]
        {
            self.child.stdin()
        }
        #[cfg(not(windows))]
        {
            &mut self.child.stdin
        }
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

    /// Non-blocking exit check. Contained Unix groups settle while the leader
    /// is still waitable: WNOWAIT observes exit without releasing its identity.
    /// The group is disarmed before the subsequent standard-library reap.
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        #[cfg(unix)]
        if self.custody_lost {
            return Err(std::io::Error::other(
                "owned child custody was lost; no wait or signal is permitted",
            ));
        }
        #[cfg(unix)]
        if self.group_owned && !self.group_settled {
            if !self.observe_owned_exit()? {
                return Ok(None);
            }
            self.request_kill()?;
        }
        self.child.try_wait()
    }

    /// Reap the direct child, preserving the same prereap settlement ordering
    /// as `try_wait`. Callers that need a deadline use their bounded poll loop.
    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let mut backoff = PollBackoff::new();
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            backoff.sleep(None);
        }
    }

    #[cfg(unix)]
    fn observe_owned_exit(&mut self) -> std::io::Result<bool> {
        if self.custody_lost {
            return Err(std::io::Error::other(
                "owned child custody was lost; signalling is disarmed",
            ));
        }
        match observe_unreaped_exit(&self.child) {
            Ok(exited) => Ok(exited),
            Err(error) => {
                self.custody_lost = true;
                self.group_settled = true;
                Err(error)
            }
        }
    }

    /// Request termination while identity custody is retained. An OS
    /// non-reaping observation at the signal boundary rejects lost custody;
    /// no numeric group signal is issued after an external/erroneous reap.
    fn request_kill(&mut self) -> std::io::Result<()> {
        #[cfg(test)]
        if let Some(kind) = self.injected_kill_failure.take() {
            return Err(std::io::Error::new(kind, "injected termination failure"));
        }
        #[cfg(windows)]
        {
            self.child.start_kill()
        }
        #[cfg(not(windows))]
        {
            #[cfg(unix)]
            if self.group_owned {
                if self.custody_lost {
                    return Err(std::io::Error::other(
                        "owned child custody was lost; signalling is disarmed",
                    ));
                }
                if self.group_settled {
                    return Ok(());
                }
                self.observe_owned_exit()?;
                let pid = checked_child_pid(&self.child)?;
                #[cfg(test)]
                {
                    self.test_group_kills += 1;
                }
                match rustix::process::kill_process_group(pid, rustix::process::Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {
                        self.group_settled = true;
                        return Ok(());
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            self.child.kill()
        }
    }

    /// Terminate the whole owned tree and reap the direct child before
    /// returning. Windows: one termination request kills every process in
    /// the job. Unix: one `SIGKILL` to the child's process group reaches
    /// every descendant, including pipe-inheriting grandchildren, and — per
    /// the PID-custody contract — that group signal is always issued BEFORE
    /// the primary is reaped: an unreaped leader pins its pgid, so the
    /// signal can only reach this owner's tree; after the reap the number
    /// becomes recyclable and later release paths stay disarmed. Other
    /// platforms: the direct child kill and reap.
    ///
    /// An already-exited child is reaped and — on Windows and Unix — its
    /// job or process group is still terminated, because the per-child job
    /// is not kill-on-close (the owner-wide job from
    /// [`bind_process_lifetime`] only fires when the owner process ends) and
    /// descendants the child spawned may outlive it in the job or group. A
    /// FAILED termination request falls back to a bounded best-effort
    /// direct-child kill and reap and is returned as an error that reports
    /// the incomplete tree cleanup: waiting on a process that refused to
    /// die would hang the caller on the unbounded poll, and the fallback
    /// can reap the direct child without certifying that every descendant
    /// stopped. The reap error is reported only when termination
    /// succeeded (a successful kill makes a failed reap a real zombie).
    pub fn terminate_tree(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        if self.group_owned {
            if let Err(error) = self.request_kill() {
                self.group_settled = true;
                if self.custody_lost {
                    return Err(format!(
                        "incomplete tree cleanup: child custody unavailable: {error}; no fallback signal sent"
                    ));
                }
                return Err(self.failed_termination_fallback(error.to_string()));
            }
            return if self.reap_within(FALLBACK_REAP_BUDGET) {
                Ok(())
            } else {
                Err("owned group was signalled but primary reap exceeded its budget".to_string())
            };
        }
        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|err| format!("failed to probe owned process: {err}"))?
        {
            let _ = status;
            // The primary exited on its own, but the job it was assigned
            // does not kill on close: any descendant still in the job would
            // survive ownership unless it is terminated here. The request
            // is a no-op for the exited primary and kills the remaining
            // tree; its wait returns immediately on the reaped primary.
            #[cfg(windows)]
            {
                self.request_kill().map_err(|err| {
                    format!(
                        "incomplete tree cleanup: failed to terminate the owned job after the \
                         primary exited: {err}"
                    )
                })?;
                self.wait()
                    .map_err(|err| format!("failed to reap owned process: {err}"))?;
            }
            return Ok(());
        }
        let termination = self.request_kill().map_err(|err| err.to_string());
        if let Err(termination_err) = termination {
            return Err(self.failed_termination_fallback(termination_err));
        }
        let reap = self
            .wait()
            .map_err(|err| format!("failed to reap owned process: {err}"));
        reap.map(|_| ())
    }

    /// Best-effort cleanup after the tree-termination request was refused:
    /// re-issue the kill through the direct-child fallback path and reap
    /// the primary within a fixed budget. The fallback reaps the direct
    /// child but cannot certify that every descendant stopped, so the
    /// caller always receives the `incomplete tree cleanup` error carrying
    /// the fallback evidence — never a plain termination failure and never
    /// an `Ok` that would imply confirmed containment.
    fn failed_termination_fallback(&mut self, termination_err: String) -> String {
        let fallback_kill = self.kill().map_err(|err| err.to_string());
        let reaped = match &fallback_kill {
            // The fallback kill requests direct-child termination; the bounded
            // poll performs and confirms the reap. Without a successful kill request only
            // a natural exit counts, so a single probe avoids stalling the
            // caller on a process nothing has asked to die.
            Ok(()) => self.reap_within(FALLBACK_REAP_BUDGET),
            Err(_) => matches!(self.try_wait(), Ok(Some(_))),
        };
        let fallback = match fallback_kill {
            Ok(()) => "succeeded".to_string(),
            Err(err) => format!("failed: {err}"),
        };
        format!(
            "incomplete tree cleanup: failed to terminate owned process: {termination_err}; \
             direct-child fallback kill {fallback}; primary reaped within budget: {reaped}"
        )
    }

    /// Bounded reap: poll the direct child until it exits or the budget
    /// expires. Never blocks past the budget, so a process that ignores
    /// the termination cannot hang the cleanup reporter.
    fn reap_within(&mut self, budget: Duration) -> bool {
        #[cfg(unix)]
        if self.custody_lost {
            return false;
        }
        let deadline = Instant::now() + budget;
        let mut backoff = PollBackoff::new();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    backoff.sleep(Some(deadline));
                }
                Err(_) => return false,
            }
        }
    }

    /// Direct-child kill request without tree scope or reaping.
    ///
    /// This is only the narrow fallback for a failed tree-termination
    /// request; ownership paths must prefer [`OwnedProcess::terminate_tree`].
    /// On Windows the wrapper stack re-issues the Job Object termination
    /// request and then waits the tree out; on other platforms this requests
    /// direct-child termination without reaping.
    pub fn kill(&mut self) -> std::io::Result<()> {
        #[cfg(unix)]
        if self.group_owned {
            self.observe_owned_exit()?;
        }
        #[cfg(all(test, unix))]
        self.test_direct_kills
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.child.kill()
    }
}

#[cfg(unix)]
fn checked_child_pid(child: &std::process::Child) -> std::io::Result<rustix::process::Pid> {
    i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| std::io::Error::other("owned child id is outside the positive PID range"))
}

#[cfg(unix)]
fn ensure_group_observation_supported() -> std::io::Result<()> {
    if cfg!(any(
        target_os = "cygwin",
        target_os = "horizon",
        target_os = "openbsd",
        target_os = "redox"
    )) {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "MCP child containment requires a non-reaping process observation",
        ))
    } else {
        Ok(())
    }
}

#[cfg(all(
    unix,
    not(any(
        target_os = "cygwin",
        target_os = "horizon",
        target_os = "openbsd",
        target_os = "redox"
    ))
))]
fn observe_unreaped_exit(child: &std::process::Child) -> std::io::Result<bool> {
    use rustix::process::{WaitId, WaitIdOptions, waitid};
    let pid = checked_child_pid(child)?;
    loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => return Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(all(
    unix,
    any(
        target_os = "cygwin",
        target_os = "horizon",
        target_os = "openbsd",
        target_os = "redox"
    )
))]
fn observe_unreaped_exit(_child: &std::process::Child) -> std::io::Result<bool> {
    ensure_group_observation_supported()?;
    Ok(false)
}

/// Stop a rustup proxy (`cargo`, `rustc`) from installing a toolchain while
/// ripr probes it (#4734). rustup 1.28.1+ auto-installs the toolchain a
/// `rust-toolchain.toml` pins when it is missing, so a bare `cargo --version`
/// or `cargo metadata --offline` in such a checkout downloads a whole
/// toolchain. With auto-install off, rustup fails fast and names the missing
/// toolchain; the probe then reports that instead of installing software.
/// Older rustup releases ignore the variable.
pub(crate) fn forbid_rustup_auto_install(command: &mut Command) {
    command.env("RUSTUP_AUTO_INSTALL", "0");
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.group_owned {
            if self.request_kill().is_err() {
                self.group_settled = true;
                if self.custody_lost {
                    return;
                }
                let _ = self.kill();
            }
            let _ = self.reap_within(FALLBACK_REAP_BUDGET);
            return;
        }
        // Kill-on-close semantics live at this boundary: whichever path
        // releases the owner — normal completion, timeout, cancellation,
        // wait failure, or panic unwinding — terminates the owned tree and
        // reaps the direct child first. Both steps are best-effort here;
        // typed evidence belongs to `terminate_tree`, which every explicit
        // termination path uses. An already-reaped child is left alone on
        // platforms without tree containment, and a failed kill is not
        // followed by an unbounded wait on a process that refused to die.
        if self.child.try_wait().is_ok_and(|status| status.is_some()) {
            // Even though the primary exited, the containment boundary does
            // not kill on close: descendants it spawned may still hold the
            // job, so ownership release still issues the termination
            // request and reaps the (already exited) primary before
            // dropping the owner.
            #[cfg(windows)]
            {
                if self.request_kill().is_ok() {
                    let _ = self.wait();
                }
            }
            return;
        }
        if self.request_kill().is_ok() {
            let _ = self.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::process::Stdio;

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

    /// Force-stop test-owned processes by PID. An already-exited PID is a
    /// silent no-op (`-ErrorAction SilentlyContinue`), so one gone survivor
    /// does not keep the others running.
    #[cfg(windows)]
    fn stop_processes_on_windows(pids: &[u32]) {
        if pids.is_empty() {
            return;
        }
        let ids = pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Stop-Process -Id {ids} -Force -ErrorAction SilentlyContinue"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
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

    /// #9564: the bind runs its action once per slot and every caller sees
    /// that first outcome, so a failed assignment is neither retried nor
    /// replaced by a second job. Negative experiment: calling `bind` on every
    /// invocation (no `OnceLock`) makes the counter 2 and fails the assertion.
    #[test]
    fn bind_once_runs_the_action_once_and_replays_its_outcome() {
        use std::sync::OnceLock;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let slot: OnceLock<Result<(), String>> = OnceLock::new();
        let first = bind_once(&slot, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Err("nested job refused".to_string())
        });
        let second = bind_once(&slot, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        assert_eq!(first, Err("nested job refused".to_string()));
        assert_eq!(second, first, "a later call must replay the first outcome");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// Off Windows there is nothing to bind: the call is an `Ok` no-op, and
    /// repeated calls stay `Ok` (Unix containment is configured per spawn).
    #[cfg(not(windows))]
    #[test]
    fn bind_process_lifetime_is_an_idempotent_no_op_off_windows() {
        assert_eq!(bind_process_lifetime(), Ok(()));
        assert_eq!(bind_process_lifetime(), Ok(()));
    }

    /// Environment names of the re-exec'd hard-kill helper (#9564).
    #[cfg(windows)]
    const KILL_HELPER_MODE_ENV: &str = "RIPR_OWNER_KILL_HELPER_MODE";
    #[cfg(windows)]
    const KILL_HELPER_MARKER_ENV: &str = "RIPR_OWNER_KILL_HELPER_MARKER";

    /// Helper entry, executed only in a process re-exec'd by
    /// `force_terminated_owner_ends_descendants_only_when_bound`; a plain test run
    /// returns immediately. It plays the owner: optionally binds its lifetime,
    /// spawns an `OwnedProcess` whose primary starts a long-lived descendant
    /// and records the descendant PID, then idles until the test hard-kills it.
    #[cfg(windows)]
    #[test]
    fn hard_kill_helper_owner_entry() -> Result<(), String> {
        let Ok(mode) = std::env::var(KILL_HELPER_MODE_ENV) else {
            return Ok(());
        };
        let marker = std::env::var(KILL_HELPER_MARKER_ENV)
            .map_err(|err| format!("helper marker path: {err}"))?;
        if mode == "bind" {
            bind_process_lifetime()?;
        }
        let marker = marker.replace('\'', "''");
        let mut command = Command::new("powershell");
        // The primary records its own PID first (`{marker}.primary`) so the
        // unbound control can end it too; the descendant marker is the
        // readiness signal and is written last.
        command.args([
            "-NoProfile",
            "-Command",
            &format!(
                "Set-Content -LiteralPath '{marker}.primary' -Value $PID; \
                 $p = Start-Process -FilePath powershell -ArgumentList @('-NoProfile','-Command','Start-Sleep -Seconds 40') -NoNewWindow -PassThru; \
                 Set-Content -LiteralPath '{marker}' -Value $p.Id; Start-Sleep -Seconds 40"
            ),
        ]);
        command.stdin(Stdio::null()).stdout(Stdio::null());
        let _owned = OwnedProcess::spawn(command).map_err(|err| format!("helper spawn: {err}"))?;
        std::thread::sleep(Duration::from_mins(2));
        Ok(())
    }

    /// Re-exec this test binary as the helper owner in `mode`, wait until its
    /// descendant is demonstrably alive, then hard-kill the helper with
    /// `TerminateProcess` (std `Child::kill`), so no `Drop` can run.
    /// Returns the descendant PID and the owned primary's PID, so the unbound
    /// control can end both survivors.
    #[cfg(windows)]
    fn hard_kill_helper_owner(mode: &str) -> Result<(u32, u32), String> {
        /// End the helper and any survivor PIDs already learned, so a setup
        /// failure leaves nothing holding the harness's inherited pipe (#12223).
        fn abandon(helper: &mut std::process::Child, survivors: &[u32]) {
            let _ = helper.kill();
            let _ = helper.wait();
            stop_processes_on_windows(survivors);
        }
        let marker_path = std::env::temp_dir().join(format!(
            "ripr-owner-hardkill-{mode}-{}.pid",
            std::process::id()
        ));
        let primary_path = std::path::PathBuf::from(format!("{}.primary", marker_path.display()));
        let _ = std::fs::remove_file(&marker_path);
        let _ = std::fs::remove_file(&primary_path);
        let exe = std::env::current_exe().map_err(|err| format!("current_exe: {err}"))?;
        let mut helper = Command::new(exe)
            .args([
                "--exact",
                "process_owner::tests::hard_kill_helper_owner_entry",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(KILL_HELPER_MODE_ENV, mode)
            .env(KILL_HELPER_MARKER_ENV, &marker_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("spawn helper owner: {err}"))?;
        let descendant_pid = match wait_for_descendant_marker(&marker_path) {
            Ok(pid) => pid,
            Err(err) => {
                // No descendant PID was learned; the primary marker is written
                // first, so it may still name a survivor.
                let primary = std::fs::read_to_string(&primary_path)
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok());
                abandon(&mut helper, primary.as_slice());
                let _ = std::fs::remove_file(&primary_path);
                return Err(err);
            }
        };
        let _ = std::fs::remove_file(&marker_path);
        let primary_pid = std::fs::read_to_string(&primary_path)
            .map_err(|err| format!("read primary PID marker: {err}"))
            .and_then(|text| {
                text.trim()
                    .parse::<u32>()
                    .map_err(|err| format!("primary PID marker {:?}: {err}", text.trim()))
            });
        let _ = std::fs::remove_file(&primary_path);
        let primary_pid = match primary_pid {
            Ok(pid) => pid,
            Err(err) => {
                abandon(&mut helper, &[descendant_pid]);
                return Err(err);
            }
        };
        if !alive_on_windows(descendant_pid) {
            abandon(&mut helper, &[descendant_pid, primary_pid]);
            return Err(format!(
                "setup failure: descendant {descendant_pid} was not alive before the owner was terminated"
            ));
        }
        helper
            .kill()
            .map_err(|err| format!("hard-kill helper owner: {err}"))?;
        helper
            .wait()
            .map_err(|err| format!("reap helper owner: {err}"))?;
        Ok((descendant_pid, primary_pid))
    }

    /// #9564 / #9323 F2: a force-terminated owner must not leak descendants once it
    /// has called `bind_process_lifetime`, and the same owner without the bind
    /// does leak, which proves the helper really bypasses `Drop` and the
    /// assertion can fail. Native Windows only; proved by the advisory lane's
    /// `process_owner::tests::force_terminated_owner_ends_descendants_only_when_bound`.
    #[cfg(windows)]
    #[test]
    fn force_terminated_owner_ends_descendants_only_when_bound() -> Result<(), String> {
        // Control: an unbound owner's descendant survives the hard kill.
        let (leaked_pid, leaked_primary_pid) = hard_kill_helper_owner("nobind")?;
        std::thread::sleep(Duration::from_secs(3));
        let leaked = alive_on_windows(leaked_pid);
        // Clean both of the control's survivors before asserting so a failure
        // leaks nothing. The unbound owned primary outlives the hard kill too,
        // and left running it holds the harness's inherited output pipe for
        // its remaining 40 s, which nextest reports as LEAK.
        stop_processes_on_windows(&[leaked_pid, leaked_primary_pid]);
        if !leaked {
            return Err(format!(
                "control invalid: descendant {leaked_pid} of an unbound force-terminated owner did not survive, so the bound cell proves nothing"
            ));
        }
        // Subject: a bound owner's descendant ends with it.
        let (bound_pid, _bound_primary_pid) = hard_kill_helper_owner("bind")?;
        assert_descendant_terminated(bound_pid, "hard kill of a kill-on-close-bound owner")
    }

    /// #5348: the child-exit poll starts at 1 ms and doubles to the 50 ms
    /// ceiling. Negative experiment: a constant 50 ms schedule (the
    /// pre-#5348 `WAIT_POLL_INTERVAL`) fails the first-delay assertion.
    #[test]
    fn poll_backoff_starts_at_one_millisecond_and_caps_at_the_ceiling() {
        let mut backoff = PollBackoff::new();
        let delays: Vec<Duration> = (0..10).map(|_| backoff.next_delay()).collect();
        assert_eq!(delays[0], Duration::from_millis(1));
        assert_eq!(
            &delays[..6],
            &[1, 2, 4, 8, 16, 32].map(Duration::from_millis),
            "schedule must double from 1 ms"
        );
        assert!(
            delays[6..]
                .iter()
                .all(|delay| *delay == POLL_BACKOFF_CEILING)
        );
    }

    /// Deadline clamping is asserted on the computed delay, not wall-clock sleep
    /// (see #10592).
    #[test]
    fn poll_backoff_clamped_delay_respects_deadline() {
        let now = Instant::now();

        let mut past_deadline = PollBackoff::new();
        for _ in 0..8 {
            let _ = past_deadline.next_delay();
        }
        assert_eq!(
            past_deadline.clamped_delay(Some(now), now),
            Duration::ZERO,
            "a deadline already in the past must clamp to zero"
        );

        let mut no_deadline = PollBackoff::new();
        assert_eq!(
            no_deadline.clamped_delay(None, now),
            POLL_BACKOFF_FIRST,
            "without a deadline the scheduled delay is returned"
        );

        let mut near_deadline = PollBackoff::new();
        for _ in 0..8 {
            let _ = near_deadline.next_delay();
        }
        let remaining = Duration::from_millis(10);
        assert_eq!(
            near_deadline.clamped_delay(Some(now + remaining), now),
            remaining,
            "a near deadline must clamp the scheduled ceiling delay"
        );
    }

    /// Mutant guard: returning the scheduled delay while ignoring the deadline
    /// would fail the past-deadline case above.
    #[test]
    fn poll_backoff_clamped_delay_deadline_clamp_is_load_bearing() {
        let now = Instant::now();
        let mut backoff = PollBackoff::new();
        for _ in 0..8 {
            let _ = backoff.next_delay();
        }
        let clamped = backoff.clamped_delay(Some(now), now);
        assert_eq!(clamped, Duration::ZERO);
        assert_eq!(POLL_BACKOFF_CEILING, Duration::from_millis(50));
        assert_ne!(
            POLL_BACKOFF_CEILING, clamped,
            "ignoring the deadline would return the ceiling delay"
        );
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
        let descendant_pid = wait_for_descendant_marker(&marker_path)?;
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
        // The unrelated process must outlive the whole observation window,
        // not merely start alive: under full-suite parallelism the cold
        // PowerShell starts, the bounded termination and the PowerShell
        // alive-check itself can legitimately exceed a short sleep, and an
        // expired unrelated process misreads as "terminated" (#7937). The
        // window matches the sibling containment tests; the test kills the
        // unrelated process itself on every exit path, so nothing leaks.
        let mut unrelated_command = platform_sleep_command(60);
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

    /// A program that cannot start surfaces as the typed spawn error from
    /// `OwnedProcess::spawn` itself, before any ownership exists (#3803
    /// native control 7, spawn arm; the assign/resume arms need an injection
    /// seam and stay tracked on #9323).
    #[test]
    fn spawning_a_missing_program_returns_a_typed_not_found_error() -> Result<(), String> {
        let mut command = Command::new("ripr-owner-no-such-program-9323");
        command.stdin(Stdio::null());
        match OwnedProcess::spawn(command) {
            Ok(_) => Err("a missing program must not produce an owned process".to_string()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!("expected NotFound, got {:?}: {err}", err.kind())),
        }
    }

    /// Arguments containing spaces, embedded quotes, a trailing backslash and
    /// non-ASCII text reach the child unchanged through the suspended-create
    /// path (#3803 native control 11). The child is this test binary running
    /// [`argument_recorder_child`], which records its own `env::args` as UTF-8
    /// to a file, so only the Rust command-line round trip is observed, with
    /// no shell parser or console code page in between.
    #[test]
    fn arguments_round_trip_through_owned_spawn() -> Result<(), String> {
        let out = std::env::temp_dir().join(format!("ripr-owner-args-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let expected = [
            "two words",
            "say \"hi\"",
            "naïve ✓ ключ",
            "trailing slash\\",
        ];
        let exe = std::env::current_exe().map_err(|err| format!("current_exe: {err}"))?;
        let mut command = Command::new(exe);
        command
            .args([
                "--exact",
                "--nocapture",
                "process_owner::tests::argument_recorder_child",
                "--",
            ])
            .args(expected)
            .env("RIPR_OWNER_ARGV_OUT", &out)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut owned = OwnedProcess::spawn(command).map_err(|err| format!("spawn: {err}"))?;
        let status = owned.wait().map_err(|err| format!("wait: {err}"))?;
        let observed = std::fs::read_to_string(&out);
        let _ = std::fs::remove_file(&out);
        if !status.success() {
            return Err(format!("recorder child failed: {status}"));
        }
        let observed = observed.map_err(|err| format!("read recorded args: {err}"))?;
        let lines: Vec<&str> = observed.lines().collect();
        let tail = lines
            .iter()
            .position(|line| *line == "--")
            .map(|at| lines.get(at + 1..).unwrap_or_default().to_vec())
            .ok_or_else(|| format!("no argument separator recorded: {lines:?}"))?;
        if tail != expected {
            return Err(format!("arguments changed in transit: {tail:?}"));
        }
        Ok(())
    }

    /// Child half of [`arguments_round_trip_through_owned_spawn`]: a no-op
    /// unless the parent set the output variable.
    #[test]
    fn argument_recorder_child() -> Result<(), String> {
        let Some(path) = std::env::var_os("RIPR_OWNER_ARGV_OUT") else {
            return Ok(());
        };
        let mut text = String::new();
        for arg in std::env::args() {
            text.push_str(&arg);
            text.push('\n');
        }
        std::fs::write(path, text).map_err(|err| format!("record args: {err}"))
    }

    /// A failed tree-termination request never reads as a clean end: the
    /// owner reports the incomplete cleanup as a typed error, still reaps the
    /// direct child through the bounded fallback, and leaves nothing running
    /// (#3803 native control 9, via the test-only injection).
    #[test]
    fn injected_termination_failure_surfaces_as_incomplete_tree_cleanup() -> Result<(), String> {
        let mut owned = OwnedProcess::spawn(platform_sleep_command(60))
            .map_err(|err| format!("spawn: {err}"))?;
        owned.injected_kill_failure = Some(std::io::ErrorKind::PermissionDenied);
        let error = match owned.terminate_tree() {
            Ok(()) => return Err("an injected failure must not report a clean end".to_string()),
            Err(error) => error,
        };
        if !error.contains("incomplete tree cleanup") {
            return Err(format!("cleanup failure lost its typed marker: {error}"));
        }
        // The fallback ended the direct child, so ownership is not leaked.
        match owned.try_wait() {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(
                "the direct child remained running after the failed-cleanup fallback".to_string(),
            ),
            Err(err) => Err(format!("probe after fallback: {err}")),
        }
    }

    /// Cancellation lands at each stage of the child's life without error:
    /// while running, again after it was already ended, and after it exited
    /// on its own (#3803 native control 8).
    #[test]
    fn terminate_tree_is_safe_at_every_stage() -> Result<(), String> {
        let mut running = OwnedProcess::spawn(platform_sleep_command(60))
            .map_err(|err| format!("spawn running: {err}"))?;
        running.terminate_tree()?;
        if !matches!(running.try_wait(), Ok(Some(_))) {
            return Err("running child was not reaped by terminate_tree".to_string());
        }
        // Idempotent: a second request after the end is still a clean end.
        running.terminate_tree()?;

        let mut finished = OwnedProcess::spawn(platform_sleep_command(0))
            .map_err(|err| format!("spawn finished: {err}"))?;
        finished.wait().map_err(|err| format!("wait: {err}"))?;
        finished.terminate_tree()
    }

    /// Poll until the marker holds a whole PID. `Set-Content` creates the
    /// file before writing it, so an early read can see it empty or partial;
    /// that is not yet a marker, and stopping on it failed the setup rather
    /// than the containment under test. The window covers two cold
    /// PowerShell starts on a loaded runner; the last observation is kept so
    /// a setup failure still says what was seen.
    #[cfg(windows)]
    fn wait_for_descendant_marker(marker_path: &std::path::Path) -> Result<u32, String> {
        let mut last = "marker not written".to_string();
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            match std::fs::read_to_string(marker_path) {
                Ok(text) => match text.trim().parse::<u32>() {
                    Ok(pid) => return Ok(pid),
                    Err(err) => last = format!("marker {:?} not a PID yet: {err}", text.trim()),
                },
                Err(err) => last = format!("marker not written: {err}"),
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(format!("descendant PID marker after 30s: {last}"))
    }

    /// Spawn an owned primary that starts a long-lived descendant, records
    /// the descendant PID, and exits immediately on its own. The returned
    /// descendant is demonstrably alive while the owned primary has already
    /// exited — the early-exit containment setup under review.
    #[cfg(windows)]
    fn spawn_exited_primary_with_descendant(
        marker_path: &std::path::Path,
    ) -> Result<(OwnedProcess, u32), String> {
        let marker_path_text = marker_path.display().to_string().replace('\'', "''");
        let mut command = Command::new("powershell");
        command.args([
            "-NoProfile",
            "-Command",
            &format!(
                "$p = Start-Process -FilePath powershell -ArgumentList @('-NoProfile','-Command','Start-Sleep -Seconds 60') -NoNewWindow -PassThru; \
                 Set-Content -LiteralPath '{marker_path_text}' -Value $p.Id; exit 0"
            ),
        ]);
        command.stdin(Stdio::null()).stdout(Stdio::null());
        let mut owned = OwnedProcess::spawn(command).map_err(|err| format!("spawn: {err}"))?;
        let descendant_pid = wait_for_descendant_marker(marker_path)?;
        // Observe the primary's own exit before the containment claim: the
        // descendant must outlive a primary that already exited.
        let mut primary_exited = false;
        for _ in 0..100 {
            if matches!(owned.try_wait(), Ok(Some(_))) {
                primary_exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if !primary_exited {
            return Err("primary did not exit after writing the descendant marker".to_string());
        }
        if !alive_on_windows(descendant_pid) {
            return Err(format!(
                "setup failure: descendant {descendant_pid} was not alive after the primary exited"
            ));
        }
        Ok((owned, descendant_pid))
    }

    /// Bounded liveness confirmation: the descendant must disappear within
    /// the window after ownership release; a survivor past it is the
    /// containment failure under review, not an observation lag.
    #[cfg(windows)]
    fn assert_descendant_terminated(descendant_pid: u32, boundary: &str) -> Result<(), String> {
        for _ in 0..30 {
            if !alive_on_windows(descendant_pid) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(format!(
            "descendant {descendant_pid} persisted past {boundary}; early-exit tree containment not established"
        ))
    }

    /// The primary exits successfully on its own while a descendant keeps
    /// running: ending ownership must still terminate the descendant even
    /// though no explicit termination was requested — the early-exit
    /// `Drop` arm (the per-child job is not kill-on-close; an owner that
    /// stays alive must end the tree itself).
    #[cfg(windows)]
    #[test]
    fn owner_drop_kills_descendants_after_the_primary_exits() -> Result<(), String> {
        let marker_path =
            std::env::temp_dir().join(format!("ripr-owner-exited-drop-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&marker_path);
        let (owned, descendant_pid) = spawn_exited_primary_with_descendant(&marker_path)?;
        let _ = std::fs::remove_file(&marker_path);
        drop(owned);
        assert_descendant_terminated(descendant_pid, "owner drop after the primary exited")
    }

    /// `terminate_tree` on a tree whose primary already exited still
    /// terminates the surviving descendant and reports success — the
    /// early-exit `terminate_tree` arm.
    #[cfg(windows)]
    #[test]
    fn terminate_tree_after_primary_exit_kills_descendants() -> Result<(), String> {
        let marker_path =
            std::env::temp_dir().join(format!("ripr-owner-exited-term-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&marker_path);
        let (mut owned, descendant_pid) = spawn_exited_primary_with_descendant(&marker_path)?;
        let _ = std::fs::remove_file(&marker_path);
        owned.terminate_tree()?;
        assert_descendant_terminated(descendant_pid, "terminate_tree after the primary exited")
    }

    #[cfg(unix)]
    fn contained_command(command: Command) -> Result<OwnedProcess, String> {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let token =
            crate::analysis::cancellation::AnalysisCancellationToken::with_server_interrupt(flag);
        crate::analysis::cancellation::with_token(&token, || OwnedProcess::spawn(command))
            .map_err(|error| error.to_string())
    }

    #[cfg(unix)]
    fn successful_group_command() -> Result<OwnedProcess, String> {
        let mut command = Command::new("sh");
        command
            .args(["-c", "printf owned-output; exit 7"])
            .stdout(Stdio::piped());
        contained_command(command)
    }

    #[cfg(unix)]
    #[test]
    fn unix_wait_settles_before_reap_preserving_output_and_status() -> Result<(), String> {
        let mut owned = successful_group_command()?;
        let status = owned.wait().map_err(|error| error.to_string())?;
        assert_eq!(status.code(), Some(7));
        let mut output = String::new();
        owned
            .stdout_pipe()
            .take()
            .ok_or("missing stdout")?
            .read_to_string(&mut output)
            .map_err(|error| error.to_string())?;
        assert_eq!(output, "owned-output");
        assert_eq!(owned.test_group_kills, 1);
        assert!(
            observe_unreaped_exit(&owned.child).is_err(),
            "wait must actually reap"
        );
        owned.terminate_tree()?;
        owned.terminate_tree()?;
        assert_eq!(owned.test_group_kills, 1, "later releases stay disarmed");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unix_try_wait_settles_before_reap_without_descendants() -> Result<(), String> {
        let mut owned = successful_group_command()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = owned.try_wait().map_err(|error| error.to_string())? {
                assert_eq!(status.code(), Some(7));
                break;
            }
            if Instant::now() >= deadline {
                return Err("owned fixture did not exit".to_string());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(owned.test_group_kills, 1);
        owned.terminate_tree()?;
        assert_eq!(owned.test_group_kills, 1);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unix_reap_before_signal_is_refused_without_signalling() -> Result<(), String> {
        let mut owned = successful_group_command()?;
        // Deliberate wrong ordering, but the production signal boundary must
        // refuse it before killpg. No PID reuse or unrelated process is needed.
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = owned.child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err("owned fixture did not exit before deliberate reap".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(status.code(), Some(7));
        assert!(owned.request_kill().is_err());
        assert_eq!(owned.test_group_kills, 0);
        assert!(owned.group_settled);
        Ok(())
    }

    #[cfg(all(
        unix,
        not(any(
            target_os = "cygwin",
            target_os = "horizon",
            target_os = "openbsd",
            target_os = "redox"
        ))
    ))]
    #[test]
    fn unix_external_reap_disarms_direct_fallback_and_drop() -> Result<(), String> {
        use rustix::process::{WaitId, WaitIdOptions, waitid};
        let mut owned = successful_group_command()?;
        let pid = checked_child_pid(&owned.child).map_err(|error| error.to_string())?;
        // An OS reap deliberately bypasses std Child's cached exit status.
        // This is a disposable child; no PID reuse is attempted or required.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG,
            )
            .map_err(|error| error.to_string())?
            .is_some()
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err("owned fixture did not exit before external reap".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let direct = owned.test_direct_kills.clone();
        assert!(owned.terminate_tree().is_err());
        assert!(owned.kill().is_err());
        // The refusal must name the lost custody, not an arbitrary wait error.
        let Err(refusal) = owned.try_wait() else {
            return Err("try_wait after external reap unexpectedly succeeded".into());
        };
        assert!(
            refusal.to_string().contains("custody was lost"),
            "{refusal}"
        );
        assert!(owned.custody_lost);
        assert_eq!(owned.test_group_kills, 0);
        drop(owned);
        assert_eq!(direct.load(std::sync::atomic::Ordering::SeqCst), 0);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unix_repeated_terminate_tree_stays_disarmed() -> Result<(), String> {
        let mut command = Command::new("sleep");
        command.arg("60");
        let mut owned = contained_command(command)?;
        owned.terminate_tree()?;
        owned.terminate_tree()?;
        assert_eq!(owned.test_group_kills, 1);
        assert!(
            owned
                .child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unix_public_spawn_preserves_the_callers_group() -> Result<(), String> {
        use std::os::unix::process::CommandExt;
        let group = rustix::process::getpgrp();
        let mut command = Command::new("sleep");
        command.arg("60").process_group(group.as_raw_pid());
        let mut owned = OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
        let pid = checked_child_pid(&owned.child).map_err(|error| error.to_string())?;
        let observed = rustix::process::getpgid(Some(pid)).map_err(|error| error.to_string())?;
        assert_eq!(observed, group);
        assert!(!owned.group_owned);
        owned.terminate_tree()?;
        assert_eq!(owned.test_group_kills, 0);
        Ok(())
    }
}
