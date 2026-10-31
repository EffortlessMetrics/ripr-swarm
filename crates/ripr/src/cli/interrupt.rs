//! First-signal bridge for standalone CLI invocations and long-running server
//! entries. Analysis cancellation sets a flag for existing checkpoints and
//! subprocess owners. Publication and repeated signals retain native
//! termination as an escape.

use super::{CommandError, commands};

#[cfg(unix)]
use std::sync::Arc;

pub(crate) type InterruptFlags = (
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
);

/// One retained registration owns the server's process-lifetime handlers.
/// Installation is serialized so a concurrent entry cannot leave a second,
/// untracked registration armed after the published flag was consumed.
#[cfg(unix)]
static SERVER_INTERRUPTS: std::sync::Mutex<Option<ServerInterrupts>> = std::sync::Mutex::new(None);

#[cfg(unix)]
struct ServerInterrupts {
    interrupted: Arc<std::sync::atomic::AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}

#[cfg(unix)]
impl ServerInterrupts {
    fn install() -> Result<Self, String> {
        let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut registration = Self {
            interrupted,
            registrations: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            // Unlike standalone CLI publication, the server has one state:
            // a pending signal both requests cancellation and arms the next
            // signal's native escape. Observe it BEFORE setting it, so the
            // first signal cooperates and a sequential repeat terminates.
            let native = signal_hook::flag::register_conditional_default(
                signal,
                Arc::clone(&registration.interrupted),
            )
            .map_err(|error| format!("cannot preserve native signal {signal}: {error}; check the process may install signal handlers, or report this as a ripr bug with the command you ran"))?;
            registration.registrations.push(native);
            let cancellation =
                signal_hook::flag::register(signal, Arc::clone(&registration.interrupted))
                    .map_err(|error| format!("cannot observe signal {signal}: {error}; check the process may install signal handlers, or report this as a ripr bug with the command you ran"))?;
            registration.registrations.push(cancellation);
        }
        Ok(registration)
    }
}

#[cfg(unix)]
impl Drop for ServerInterrupts {
    fn drop(&mut self) {
        // Also rolls back every already-installed handler if construction
        // fails. A successful registration stays in the static owner.
        for id in self.registrations.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}

/// Install process-lifetime SIGINT+SIGTERM cooperative interrupts for a
/// server entry (Unix). Idempotent: a repeated install is a no-op so nested
/// or retried entry points cannot stack handlers. Without this registration
/// an external SIGTERM takes the default action — immediate process death
/// mid-analysis — and any in-flight git child outlives the server as an
/// orphan (#9895). With it, the first signal requests cooperative
/// cancellation: bounded child waits observe the flag at their poll cadence,
/// terminate the owned process tree, and unwind, so no git child is left
/// behind; a repeated signal restores native termination as the escape.
/// Non-Unix targets have no signal-hook surface; containment there is the
/// job-object ownership already in `process_owner`, and this is a no-op.
pub(crate) fn install_server_interrupts() -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut registration = SERVER_INTERRUPTS
            .lock()
            .map_err(|error| format!("server interrupt installation lock was poisoned: {error}"))?;
        if registration.is_none() {
            *registration = Some(ServerInterrupts::install()?);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

/// The installed server interrupt flag, when a server entry installed it.
/// Analysis entries consult these to build their cancellation token; absent
/// flags preserve the embedder-managed signal policy (plain `cli::run`).
#[cfg(unix)]
pub(crate) fn server_interrupt_flag() -> Option<Arc<std::sync::atomic::AtomicBool>> {
    SERVER_INTERRUPTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .map(|registration| Arc::clone(&registration.interrupted))
}

/// Consume the signal at the terminal boundary, after owned work has settled.
/// One atomic exchange both resets the repeated-signal escape and captures a
/// late signal that arrived after the last analysis checkpoint. That signal
/// refuses an otherwise successful result; an existing failure keeps its
/// diagnostic. A signal stored after this exchange belongs to the next attempt.
#[cfg(unix)]
pub(crate) fn settle_server_attempt<T, E>(
    interrupted: &std::sync::atomic::AtomicBool,
    result: Result<T, E>,
    cancelled: impl FnOnce() -> E,
) -> Result<T, E> {
    let pending = interrupted.swap(false, std::sync::atomic::Ordering::SeqCst);
    match result {
        Ok(_) if pending => Err(cancelled()),
        other => other,
    }
}

pub(super) fn check(args: &[String]) -> Result<(), CommandError> {
    #[cfg(unix)]
    {
        use crate::analysis::cancellation::{self, AnalysisCancellationToken};
        use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};

        let interrupted = Arc::new(AtomicBool::new(false));
        let native = Arc::new(AtomicBool::new(false));
        let token = AnalysisCancellationToken::with_interrupt_flags(
            Arc::clone(&interrupted),
            Arc::clone(&native),
        );
        let result = cancellation::with_token(&token, || {
            commands::check_with_interrupt_flags(
                args,
                (Arc::clone(&interrupted), Arc::clone(&native)),
            )
        });
        if interrupted.load(Ordering::SeqCst) {
            return Err(CommandError::Interrupted(
                "check interrupted; rerun the same command when ready. Any retained unpublished exporter staging is not reused."
                    .to_string(),
            ));
        }
        result.map_err(CommandError::from)
    }
    #[cfg(not(unix))]
    commands::check(args).map_err(CommandError::from)
}

#[cfg(unix)]
pub(super) struct SignalRegistration {
    cancellation: signal_hook::SigId,
    force: signal_hook::SigId,
    native: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(unix)]
impl SignalRegistration {
    pub(super) fn create((interrupted, native): InterruptFlags) -> Result<Self, String> {
        let signal = signal_hook::consts::SIGINT;
        // Order matters: publication's store/checkpoint handshake relies on
        // cancellation being stored before the conditional native-action read.
        let cancellation = signal_hook::flag::register(signal, interrupted)
            .map_err(|error| format!("check: cannot observe SIGINT: {error}; check the process may install signal handlers, or report this as a ripr bug with the command you ran"))?;
        if let Err(error) = signal_hook::flag::register_conditional_default(signal, native.clone())
        {
            signal_hook::low_level::unregister(cancellation);
            return Err(format!(
                "check: cannot preserve native SIGINT: {error}; check the process may install signal handlers, or report this as a ripr bug with the command you ran"
            ));
        }
        // A second signal can escape blocked analysis I/O or stalled cleanup.
        // The first still requests cooperative settlement before arming this.
        match signal_hook::flag::register(signal, native.clone()) {
            Ok(force) => Ok(Self {
                cancellation,
                force,
                native,
            }),
            Err(error) => {
                native.store(true, std::sync::atomic::Ordering::SeqCst);
                signal_hook::low_level::unregister(cancellation);
                Err(format!(
                    "check: cannot arm forced SIGINT: {error}; check the process may install signal handlers, or report this as a ripr bug with the command you ran"
                ))
            }
        }
    }
}

#[cfg(unix)]
impl Drop for SignalRegistration {
    fn drop(&mut self) {
        self.native.store(true, std::sync::atomic::Ordering::SeqCst);
        signal_hook::low_level::unregister(self.cancellation);
        signal_hook::low_level::unregister(self.force);
        // Keep conditional native termination for final diagnostics and until
        // process exit. This explicit standalone entry is called once; ordinary
        // cli::run never installs process-lifetime signal actions.
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::{
        InterruptFlags, SignalRegistration, check, install_server_interrupts,
        server_interrupt_flag, settle_server_attempt,
    };
    use crate::cli::CommandError;
    use std::io::{Read, Seek};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Child, ExitStatus, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    const CHILD_CASE: &str = "RIPR_SIGINT_TEST_CHILD";
    const CHILD_TIMEOUT: Duration = Duration::from_secs(30);
    const OUTPUT_LIMIT: u64 = 32 * 1024;

    // Drop intentionally retains an armed process-wide handler. A mutex cannot
    // protect a later signal from it: register/drop/raise only in fresh children.
    fn in_child(case: &str) -> Result<bool, String> {
        match std::env::var_os(CHILD_CASE) {
            None => Ok(false),
            Some(value) if value == case => Ok(true),
            Some(value) => Err(format!(
                "signal child identity mismatch: {value:?} != {case}"
            )),
        }
    }

    fn mark(case: &str, stage: &str) {
        // Identity comes from the executing scenario, never the requested env.
        println!("\n[interrupt-child] {case} {stage}");
    }

    struct ChildResult {
        status: ExitStatus,
        timed_out: bool,
        output: String,
    }

    impl ChildResult {
        fn require(&self, case: &str, stage: &str, signal: Option<i32>) -> Result<(), String> {
            let marker = format!("[interrupt-child] {case} {stage}");
            if self.timed_out
                || !self.output.lines().any(|line| line == marker)
                || self.status.signal() != signal
                || (signal.is_none() && !self.status.success())
            {
                return Err(format!(
                    "signal child {case}: status={}, timed_out={}, missing/wrong stage {stage}: {}",
                    self.status, self.timed_out, self.output
                ));
            }
            Ok(())
        }
    }

    fn poll_child(child: &mut Child, timeout: Duration) -> Result<Option<ExitStatus>, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                return Ok(Some(status));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn stop_child(child: &mut Child) -> Result<ExitStatus, String> {
        let kill_error = child.kill().err();
        let status = poll_child(child, Duration::from_secs(5)).map_err(|error| {
            format!(
                "child {} reap poll failed: {error}; kill error: {kill_error:?}",
                child.id()
            )
        })?;
        let Some(status) = status else {
            return Err(format!(
                "child {} not reaped after bounded kill; kill error: {kill_error:?}",
                child.id()
            ));
        };
        if let Some(error) = kill_error {
            // Exit may race kill. Record the error and actual reaping outcome.
            eprintln!(
                "child {} kill returned {error}; reaped {status}",
                child.id()
            );
        }
        Ok(status)
    }

    fn run_isolated(case: &str, filter: &str, timeout: Duration) -> Result<ChildResult, String> {
        use std::sync::atomic::AtomicU64;
        static NEXT_LOG: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir();
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path = directory.join(format!(
            "ripr-sigint-{}-{epoch}-{}.log",
            std::process::id(),
            NEXT_LOG.fetch_add(1, Ordering::Relaxed)
        ));
        let mut log = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        // Regular-file capture avoids pipe-drain waits, even after an adverse
        // child exit. Reads below have a byte cap. Retain the log for diagnosis.
        let stdout = log.try_clone().map_err(|error| error.to_string())?;
        let stderr = log.try_clone().map_err(|error| error.to_string())?;
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        // libtest paths omit the crate-name segment of module_path!().
        let module = module_path!();
        let module = module.split_once("::").map_or(module, |(_, rest)| rest);
        let mut child = std::process::Command::new(executable)
            .arg(format!("{module}::{filter}"))
            .args(["--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD_CASE, case)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .map_err(|error| error.to_string())?;
        // Once spawned, every return first establishes terminal/reaped state or
        // reports the exact unsettled PID. Neither polling nor cleanup blocks.
        let (status, timed_out) = match poll_child(&mut child, timeout) {
            Ok(Some(status)) => (status, false),
            Ok(None) => (stop_child(&mut child)?, true),
            Err(error) => {
                let cleanup = stop_child(&mut child);
                return Err(format!(
                    "child {} poll failed: {error}; cleanup: {cleanup:?}; log {}",
                    child.id(),
                    path.display()
                ));
            }
        };
        log.rewind().map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        log.take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            return Err(format!(
                "signal child output exceeds {OUTPUT_LIMIT} bytes: {}",
                path.display()
            ));
        }
        Ok(ChildResult {
            status,
            timed_out,
            output: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }

    fn flags() -> (InterruptFlags, Arc<AtomicBool>, Arc<AtomicBool>) {
        let interrupted = Arc::new(AtomicBool::new(false));
        let native = Arc::new(AtomicBool::new(false));
        (
            (Arc::clone(&interrupted), Arc::clone(&native)),
            interrupted,
            native,
        )
    }

    #[test]
    fn check_help_completes_without_interruption() {
        let result = check(&["--help".to_string()]);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn check_unknown_flag_is_failure_not_interruption() -> Result<(), String> {
        // Positive control: the same entry accepts a valid request.
        assert_eq!(check(&["--help".to_string()]), Ok(()));
        let Err(error) = check(&["--definitely-not-a-flag".to_string()]) else {
            return Err("unknown flag must fail closed".to_string());
        };
        assert!(matches!(error, CommandError::Failure(_)), "{error:?}");
        assert_eq!(error.exit_code(), 2);
        assert!(
            error.message().contains("--definitely-not-a-flag"),
            "{error:?}"
        );
        Ok(())
    }

    #[test]
    fn registration_leaves_flags_clear_until_drop_arms_native_termination() -> Result<(), String> {
        const CASE: &str = "registration_leaves_flags_clear_until_drop_arms_native_termination";
        if !in_child(CASE)? {
            return run_isolated(CASE, CASE, CHILD_TIMEOUT)?.require(CASE, "drop-observed", None);
        }
        let (pair, interrupted, native) = flags();
        let registration = SignalRegistration::create(pair)?;
        assert!(!interrupted.load(Ordering::SeqCst));
        assert!(
            !native.load(Ordering::SeqCst),
            "native termination must stay disarmed while analysis settles"
        );
        drop(registration);
        assert!(
            native.load(Ordering::SeqCst),
            "dropping the registration must arm native termination"
        );
        assert!(!interrupted.load(Ordering::SeqCst));
        mark(CASE, "drop-observed");
        Ok(())
    }

    #[test]
    fn first_sigint_requests_cancellation_then_arms_native_termination() -> Result<(), String> {
        const CASE: &str = "first_sigint_requests_cancellation_then_arms_native_termination";
        if !in_child(CASE)? {
            return run_isolated(CASE, CASE, CHILD_TIMEOUT)?.require(
                CASE,
                "first-signal-observed",
                None,
            );
        }
        let (pair, interrupted, native) = flags();
        let registration = SignalRegistration::create(pair)?;
        // Construction: nothing is requested or armed before the signal.
        assert!(!interrupted.load(Ordering::SeqCst));
        assert!(!native.load(Ordering::SeqCst));
        signal_hook::low_level::raise(signal_hook::consts::SIGINT)
            .map_err(|error| error.to_string())?;
        assert!(
            interrupted.load(Ordering::SeqCst),
            "first SIGINT must request cooperative cancellation"
        );
        assert!(
            native.load(Ordering::SeqCst),
            "first SIGINT must arm native termination for a repeated signal"
        );
        mark(CASE, "first-signal-observed");
        drop(registration);
        Ok(())
    }

    #[test]
    fn repeated_sigint_after_arm_takes_native_death() -> Result<(), String> {
        const CASE: &str = "repeated_sigint_after_arm_takes_native_death";
        if !in_child(CASE)? {
            return run_isolated(CASE, CASE, CHILD_TIMEOUT)?.require(
                CASE,
                "first-signal-observed",
                Some(signal_hook::consts::SIGINT),
            );
        }
        let (pair, interrupted, native) = flags();
        let _registration = SignalRegistration::create(pair)?;
        assert!(!interrupted.load(Ordering::SeqCst));
        assert!(!native.load(Ordering::SeqCst));
        signal_hook::low_level::raise(signal_hook::consts::SIGINT)
            .map_err(|error| error.to_string())?;
        assert!(interrupted.load(Ordering::SeqCst));
        assert!(native.load(Ordering::SeqCst));
        mark(CASE, "first-signal-observed");
        signal_hook::low_level::raise(signal_hook::consts::SIGINT)
            .map_err(|error| error.to_string())?;
        Err("repeated SIGINT unexpectedly returned".to_string())
    }

    #[test]
    fn sigint_after_drop_takes_native_death() -> Result<(), String> {
        const CASE: &str = "sigint_after_drop_takes_native_death";
        if !in_child(CASE)? {
            return run_isolated(CASE, CASE, CHILD_TIMEOUT)?.require(
                CASE,
                "drop-observed",
                Some(signal_hook::consts::SIGINT),
            );
        }
        let (pair, interrupted, native) = flags();
        let registration = SignalRegistration::create(pair)?;
        assert!(!interrupted.load(Ordering::SeqCst));
        assert!(!native.load(Ordering::SeqCst));
        drop(registration);
        assert!(!interrupted.load(Ordering::SeqCst));
        assert!(native.load(Ordering::SeqCst));
        mark(CASE, "drop-observed");
        signal_hook::low_level::raise(signal_hook::consts::SIGINT)
            .map_err(|error| error.to_string())?;
        Err("SIGINT after Drop unexpectedly returned".to_string())
    }

    #[test]
    fn isolated_children_reject_wrong_or_empty_selection() -> Result<(), String> {
        const CASE: &str = "first_sigint_requests_cancellation_then_arms_native_termination";
        let empty = run_isolated(CASE, "no_such_signal_test", CHILD_TIMEOUT)?;
        assert!(empty.status.success());
        assert!(empty.require(CASE, "first-signal-observed", None).is_err());
        let wrong = run_isolated(
            CASE,
            "registration_leaves_flags_clear_until_drop_arms_native_termination",
            CHILD_TIMEOUT,
        )?;
        assert!(!wrong.status.success());
        assert!(wrong.output.contains("signal child identity mismatch"));
        assert!(wrong.require(CASE, "first-signal-observed", None).is_err());
        Ok(())
    }

    #[test]
    fn isolated_child_failure_is_reported() -> Result<(), String> {
        const CASE: &str = "isolated_child_failure_is_reported";
        if in_child(CASE)? {
            mark(CASE, "started");
            return Err("controlled child failure".to_string());
        }
        let child = run_isolated(CASE, CASE, CHILD_TIMEOUT)?;
        assert!(!child.status.success());
        assert!(child.output.contains("controlled child failure"));
        assert!(child.require(CASE, "started", None).is_err());
        Ok(())
    }

    #[test]
    fn isolated_child_signal_before_checkpoint_is_rejected() -> Result<(), String> {
        const CASE: &str = "isolated_child_signal_before_checkpoint_is_rejected";
        if in_child(CASE)? {
            mark(CASE, "started");
            signal_hook::low_level::raise(signal_hook::consts::SIGINT)
                .map_err(|error| error.to_string())?;
            return Err("unregistered SIGINT unexpectedly returned".to_string());
        }
        let child = run_isolated(CASE, CASE, CHILD_TIMEOUT)?;
        assert_eq!(child.status.signal(), Some(signal_hook::consts::SIGINT));
        child.require(CASE, "started", Some(signal_hook::consts::SIGINT))?;
        assert!(
            child
                .require(
                    CASE,
                    "first-signal-observed",
                    Some(signal_hook::consts::SIGINT)
                )
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn isolated_child_timeout_is_killed_and_reaped() -> Result<(), String> {
        const CASE: &str = "isolated_child_timeout_is_killed_and_reaped";
        if in_child(CASE)? {
            mark(CASE, "started");
            loop {
                std::thread::park();
            }
        }
        let child = run_isolated(CASE, CASE, Duration::from_secs(2))?;
        assert!(child.timed_out);
        assert_eq!(child.status.signal(), Some(signal_hook::consts::SIGKILL));
        assert!(
            child
                .output
                .contains(&format!("[interrupt-child] {CASE} started"))
        );
        assert!(child.require(CASE, "started", None).is_err());
        Ok(())
    }

    #[test]
    fn isolated_child_output_is_capped() -> Result<(), String> {
        const CASE: &str = "isolated_child_output_is_capped";
        if in_child(CASE)? {
            println!("{}", "x".repeat(OUTPUT_LIMIT as usize + 1));
            return Ok(());
        }
        let error = match run_isolated(CASE, CASE, CHILD_TIMEOUT) {
            Ok(_) => return Err("oversized child output was accepted".to_string()),
            Err(error) => error,
        };
        assert!(error.contains("signal child output exceeds"), "{error}");
        Ok(())
    }

    #[test]
    fn first_sigterm_requests_cancellation_then_arms_native_termination() -> Result<(), String> {
        const CASE: &str = "first_sigterm_requests_cancellation_then_arms_native_termination";
        if !in_child(CASE)? {
            return run_isolated(CASE, CASE, CHILD_TIMEOUT)?.require(
                CASE,
                "first-signal-observed",
                None,
            );
        }
        install_server_interrupts()?;
        let flag = server_interrupt_flag().ok_or_else(|| "server flag missing".to_string())?;
        assert!(!flag.load(Ordering::SeqCst));
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM)
            .map_err(|error| error.to_string())?;
        assert!(flag.load(Ordering::SeqCst));
        mark(CASE, "first-signal-observed");
        Ok(())
    }

    #[test]
    fn sigint_and_sigterm_share_server_interrupt_state() -> Result<(), String> {
        const CASE: &str = "sigint_and_sigterm_share_server_interrupt_state";
        if !in_child(CASE)? {
            return run_isolated(CASE, CASE, CHILD_TIMEOUT)?.require(
                CASE,
                "first-signal-observed",
                Some(signal_hook::consts::SIGINT),
            );
        }
        install_server_interrupts()?;
        let flag = server_interrupt_flag().ok_or_else(|| "server flag missing".to_string())?;
        assert!(!flag.load(Ordering::SeqCst));
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM)
            .map_err(|error| error.to_string())?;
        assert!(flag.load(Ordering::SeqCst));
        mark(CASE, "first-signal-observed");
        signal_hook::low_level::raise(signal_hook::consts::SIGINT)
            .map_err(|error| error.to_string())?;
        Err("mixed server signal repeat unexpectedly returned".to_string())
    }

    #[test]
    fn repeated_server_sigterm_takes_native_death() -> Result<(), String> {
        const CASE: &str = "repeated_server_sigterm_takes_native_death";
        if !in_child(CASE)? {
            return run_isolated(CASE, CASE, CHILD_TIMEOUT)?.require(
                CASE,
                "first-signal-observed",
                Some(signal_hook::consts::SIGTERM),
            );
        }
        install_server_interrupts()?;
        let flag = server_interrupt_flag().ok_or_else(|| "server flag missing".to_string())?;
        assert!(!flag.load(Ordering::SeqCst));
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM)
            .map_err(|error| error.to_string())?;
        assert!(flag.load(Ordering::SeqCst));
        mark(CASE, "first-signal-observed");
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM)
            .map_err(|error| error.to_string())?;
        Err("server SIGTERM repeat unexpectedly returned".to_string())
    }

    #[test]
    fn server_interrupt_install_is_idempotent_and_exposes_flags() -> Result<(), String> {
        const CASE: &str = "server_interrupt_install_is_idempotent_and_exposes_flags";
        if !in_child(CASE)? {
            return run_isolated(CASE, CASE, CHILD_TIMEOUT)?.require(
                CASE,
                "reset-cycles-observed",
                None,
            );
        }
        let barrier = std::sync::Barrier::new(8);
        let installed = std::thread::scope(|scope| {
            let workers = (0..8)
                .map(|_| {
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        install_server_interrupts()?;
                        server_interrupt_flag().ok_or_else(|| "server flag missing".to_string())
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_join| "server installer failed".to_string())?
                })
                .collect::<Result<Vec<_>, String>>()
        })?;
        let flag = server_interrupt_flag().ok_or_else(|| "server flag missing".to_string())?;
        assert!(
            installed
                .iter()
                .all(|observed| Arc::ptr_eq(observed, &flag))
        );
        for signal in [
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
        ] {
            assert!(!flag.load(Ordering::SeqCst));
            signal_hook::low_level::raise(signal).map_err(|error| error.to_string())?;
            assert!(flag.load(Ordering::SeqCst));
            assert_eq!(
                settle_server_attempt(&flag, Ok(7), || "cancelled"),
                Err("cancelled")
            );
            assert!(!flag.load(Ordering::SeqCst));
            assert_eq!(settle_server_attempt(&flag, Ok(7), || "cancelled"), Ok(7));
        }
        mark(CASE, "reset-cycles-observed");
        Ok(())
    }

    #[test]
    fn terminal_signal_exchange_preserves_late_and_next_attempts() {
        let flag = AtomicBool::new(false);
        // The producer already returned success when a late signal arrives.
        let result = Ok(7);
        flag.store(true, Ordering::SeqCst);
        let settled = settle_server_attempt(&flag, result, || {
            // A signal after the exchange belongs to the next attempt. No
            // second reset may erase it while this failure is constructed.
            flag.store(true, Ordering::SeqCst);
            "cancelled"
        });
        assert_eq!(settled, Err("cancelled"));
        assert!(flag.load(Ordering::SeqCst));
        assert_eq!(
            settle_server_attempt(&flag, Ok(9), || "cancelled"),
            Err("cancelled")
        );
        assert!(!flag.load(Ordering::SeqCst));
        assert_eq!(settle_server_attempt(&flag, Ok(11), || "cancelled"), Ok(11));
        flag.store(true, Ordering::SeqCst);
        assert_eq!(
            settle_server_attempt::<(), _>(&flag, Err("config_invalid"), || "cancelled"),
            Err("config_invalid")
        );
        assert!(!flag.load(Ordering::SeqCst));
    }
}
