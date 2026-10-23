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

/// Process-wide cooperative interrupt flags for a long-running server entry
/// (`ripr mcp --stdio`, #9895). Unlike the one-shot [`check`] registration,
/// a stdio server serves until its transport ends, so its registration and
/// flags live for the rest of the process and every dispatched analysis
/// observes the same pair. Unix-only: non-Unix installs are no-ops and no
/// consumer exists outside the Unix cancellation path.
#[cfg(unix)]
static SERVER_FLAGS: std::sync::OnceLock<InterruptFlags> = std::sync::OnceLock::new();

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
        if SERVER_FLAGS.get().is_some() {
            return Ok(());
        }
        let interrupted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let native = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let registration = SignalRegistration::create_for(
            &[signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM],
            (
                std::sync::Arc::clone(&interrupted),
                std::sync::Arc::clone(&native),
            ),
        )?;
        // The guard unregisters on drop; a stdio server runs until process
        // exit, so the registration is intentionally process-lifetime.
        std::mem::forget(registration);
        let _ = SERVER_FLAGS.set((interrupted, native));
        Ok(())
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

/// The installed server interrupt flags, when a server entry installed them.
/// Analysis entries consult these to build their cancellation token; absent
/// flags preserve the embedder-managed signal policy (plain `cli::run`).
/// Unix-only alongside [`SERVER_FLAGS`].
#[cfg(unix)]
pub(crate) fn server_interrupt_flags() -> Option<InterruptFlags> {
    SERVER_FLAGS.get().cloned()
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
    cancellation: Vec<signal_hook::SigId>,
    force: Vec<signal_hook::SigId>,
    native: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(unix)]
impl SignalRegistration {
    pub(super) fn create(flags: InterruptFlags) -> Result<Self, String> {
        Self::create_for(&[signal_hook::consts::SIGINT], flags)
    }

    /// Register the cooperative-cancel / conditional-native / forced-native
    /// triple for every signal in `signals`, sharing one flag pair: the
    /// first signal of any registered kind requests cooperative settlement,
    /// and a repeat of any registered kind restores native termination as
    /// the escape. `SIGINT` keeps its historical name so existing callers
    /// and tests stay unchanged; server entries add `SIGTERM` (#9895).
    fn create_for(signals: &[i32], (interrupted, native): InterruptFlags) -> Result<Self, String> {
        let mut cancellation_ids = Vec::new();
        let mut force_ids = Vec::new();
        for &signal in signals {
            // Order matters: publication's store/checkpoint handshake relies
            // on cancellation being stored before the conditional
            // native-action read.
            let cancellation = signal_hook::flag::register(signal, Arc::clone(&interrupted))
                .map_err(|error| format!("cannot observe signal {signal}: {error}"))?;
            cancellation_ids.push(cancellation);
            if let Err(error) =
                signal_hook::flag::register_conditional_default(signal, Arc::clone(&native))
            {
                for id in cancellation_ids.drain(..) {
                    signal_hook::low_level::unregister(id);
                }
                return Err(format!("cannot preserve native signal {signal}: {error}"));
            }
            // A second signal can escape blocked analysis I/O or stalled
            // cleanup. The first still requests cooperative settlement before
            // arming this.
            match signal_hook::flag::register(signal, Arc::clone(&native)) {
                Ok(force) => force_ids.push(force),
                Err(error) => {
                    native.store(true, std::sync::atomic::Ordering::SeqCst);
                    for id in cancellation_ids.drain(..) {
                        signal_hook::low_level::unregister(id);
                    }
                    for id in force_ids.drain(..) {
                        signal_hook::low_level::unregister(id);
                    }
                    return Err(format!("cannot arm forced signal {signal}: {error}"));
                }
            }
        }
        Ok(Self {
            cancellation: cancellation_ids,
            force: force_ids,
            native,
        })
    }
}

#[cfg(unix)]
impl Drop for SignalRegistration {
    fn drop(&mut self) {
        self.native.store(true, std::sync::atomic::Ordering::SeqCst);
        for id in self.cancellation.drain(..) {
            signal_hook::low_level::unregister(id);
        }
        for id in self.force.drain(..) {
            signal_hook::low_level::unregister(id);
        }
        // Keep conditional native termination for final diagnostics and until
        // process exit. This explicit standalone entry is called once; ordinary
        // cli::run never installs process-lifetime signal actions.
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{
        InterruptFlags, SignalRegistration, check, install_server_interrupts,
        server_interrupt_flags,
    };
    use crate::cli::CommandError;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard};

    // SIGINT handlers are process-wide: a signal raised by one test would also set the flags of
    // any registration alive in a parallel test, so tests that register hold this guard.
    static SIGNAL_TESTS: Mutex<()> = Mutex::new(());

    fn serialized() -> MutexGuard<'static, ()> {
        SIGNAL_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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

    /// Signal handlers and raised signals are process-wide, and a dropped
    /// registration intentionally leaves a conditional-default handler armed
    /// — so any test that raises a termination signal runs its scenario in a
    /// re-invoked copy of this test binary (one exact filter plus a child
    /// marker env var) instead of the shared lib-test runner. A retained
    /// handler can otherwise take the whole runner down with the next
    /// signal, which hosted CI observed as the lib test executable dying
    /// with SIGTERM right after these tests (#9895 review).
    fn in_isolated_child() -> bool {
        std::env::var_os("RIPR_INTERRUPT_ISOLATED_CHILD").is_some()
    }

    /// libtest reports test paths without the crate-name prefix that
    /// `module_path!()` carries, so an exact filter built from
    /// `module_path!()` verbatim would silently select zero tests and the
    /// child would exit 0 without running anything. Drop the first segment.
    fn exact_test_filter(test_name: &str) -> String {
        let module = module_path!();
        let module = module.split_once("::").map_or(module, |(_, rest)| rest);
        format!("{module}::{test_name}")
    }

    /// Every isolated child prints this marker — carrying its own test name
    /// — before touching anything signal-related, so the wrapper can prove
    /// the exact filter actually selected a subject. A zero-test selection
    /// can never print the marker, so it can never pass vacuously.
    fn child_ack() {
        let name = std::env::var("RIPR_INTERRUPT_ISOLATED_CHILD").unwrap_or_default();
        eprintln!("[interrupt-isolated-child] ack {name}");
    }

    fn combined_output(output: &std::process::Output) -> String {
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        text
    }

    fn run_isolated(test_name: &str) -> Result<std::process::Output, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        std::process::Command::new(executable)
            .arg(exact_test_filter(test_name))
            .args(["--exact", "--nocapture"])
            .env("RIPR_INTERRUPT_ISOLATED_CHILD", test_name)
            .output()
            .map_err(|error| error.to_string())
    }

    /// Assert the isolated child acknowledged its selection, ran exactly
    /// one test to completion, and exited successfully.
    fn assert_child_success(test_name: &str) -> Result<(), String> {
        let output = run_isolated(test_name)?;
        let text = combined_output(&output);
        let ack = format!("[interrupt-isolated-child] ack {test_name}");
        if !text.contains(&ack) {
            return Err(format!(
                "isolated signal child {test_name} never acknowledged; the exact filter selected no test: {text}"
            ));
        }
        if !text.contains("1 passed") {
            return Err(format!(
                "isolated signal child {test_name} must run exactly one test to completion: {text}"
            ));
        }
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "isolated signal child {test_name} failed: {}",
            output.status
        ))
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
        // Drop arms the conditional-default handler process-wide, so — like
        // every handler-mutating scenario — the assertion runs in an isolated
        // re-invoked child and the retained handler dies with it.
        if in_isolated_child() {
            return child_registration_leaves_flags_clear_until_drop_arms_native_termination();
        }
        assert_child_success("registration_leaves_flags_clear_until_drop_arms_native_termination")
    }

    fn child_registration_leaves_flags_clear_until_drop_arms_native_termination()
    -> Result<(), String> {
        child_ack();
        let _serial = serialized();
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
        Ok(())
    }

    #[test]
    fn first_sigint_requests_cancellation_then_arms_native_termination() -> Result<(), String> {
        if in_isolated_child() {
            return child_first_sigint_requests_cancellation_then_arms_native_termination();
        }
        assert_child_success("first_sigint_requests_cancellation_then_arms_native_termination")
    }

    fn child_first_sigint_requests_cancellation_then_arms_native_termination() -> Result<(), String>
    {
        child_ack();
        let _serial = serialized();
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
        drop(registration);
        Ok(())
    }

    #[test]
    fn first_sigterm_requests_cancellation_then_arms_native_termination() -> Result<(), String> {
        // #9895: SIGTERM gets the same cooperative-first contract as SIGINT,
        // so an external termination request settles the in-flight attempt
        // (bounded child waits terminate the owned tree) instead of orphaning
        // it; only a repeated signal takes native death.
        if in_isolated_child() {
            return child_first_sigterm_requests_cancellation_then_arms_native_termination();
        }
        assert_child_success("first_sigterm_requests_cancellation_then_arms_native_termination")
    }

    fn child_first_sigterm_requests_cancellation_then_arms_native_termination() -> Result<(), String>
    {
        child_ack();
        let _serial = serialized();
        let (pair, interrupted, native) = flags();
        let registration = SignalRegistration::create_for(&[signal_hook::consts::SIGTERM], pair)?;
        assert!(!interrupted.load(Ordering::SeqCst));
        assert!(!native.load(Ordering::SeqCst));
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM)
            .map_err(|error| error.to_string())?;
        assert!(
            interrupted.load(Ordering::SeqCst),
            "first SIGTERM must request cooperative cancellation"
        );
        assert!(
            native.load(Ordering::SeqCst),
            "first SIGTERM must arm native termination for a repeated signal"
        );
        drop(registration);
        Ok(())
    }

    #[test]
    fn sigint_and_sigterm_share_one_flag_pair_in_server_registrations() -> Result<(), String> {
        // #9895: the server registration covers both signals on one pair, so
        // whichever arrives first requests settlement and a repeat of either
        // arms native termination — the two signals cannot disagree about the
        // attempt's state.
        if in_isolated_child() {
            return child_sigint_and_sigterm_share_one_flag_pair_in_server_registrations();
        }
        assert_child_success("sigint_and_sigterm_share_one_flag_pair_in_server_registrations")
    }

    fn child_sigint_and_sigterm_share_one_flag_pair_in_server_registrations() -> Result<(), String>
    {
        child_ack();
        let _serial = serialized();
        let (pair, interrupted, native) = flags();
        let registration = SignalRegistration::create_for(
            &[signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM],
            pair,
        )?;
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM)
            .map_err(|error| error.to_string())?;
        assert!(interrupted.load(Ordering::SeqCst));
        assert!(native.load(Ordering::SeqCst));
        // A repeat of the OTHER registered signal is the force escape too:
        // the pair is shared, so the conditional default reads the same arm.
        signal_hook::low_level::raise(signal_hook::consts::SIGINT)
            .map_err(|error| error.to_string())?;
        assert!(interrupted.load(Ordering::SeqCst));
        assert!(native.load(Ordering::SeqCst));
        drop(registration);
        Ok(())
    }

    /// The escape hatch is real: once the first signal armed native
    /// termination, a repeated signal takes native death. The expected
    /// outcome is the scenario process dying by SIGINT, so — like every
    /// raising scenario — it runs in an isolated re-invoked child and the
    /// outer test only inspects the exit signal.
    #[test]
    fn repeated_sigint_after_arm_takes_native_death() -> Result<(), String> {
        if in_isolated_child() {
            return child_repeated_sigint_after_arm_takes_native_death();
        }
        let output = run_isolated("repeated_sigint_after_arm_takes_native_death")?;
        let text = combined_output(&output);
        if !text
            .contains("[interrupt-isolated-child] ack repeated_sigint_after_arm_takes_native_death")
        {
            return Err(format!(
                "isolated signal child never acknowledged; the exact filter selected no test: {text}"
            ));
        }
        use std::os::unix::process::ExitStatusExt as _;
        match output.status.signal() {
            Some(signal) if signal == signal_hook::consts::SIGINT => Ok(()),
            other => Err(format!(
                "the repeated SIGINT must take native death by SIGINT, got signal {other:?} ({})",
                output.status
            )),
        }
    }

    fn child_repeated_sigint_after_arm_takes_native_death() -> Result<(), String> {
        child_ack();
        let _serial = serialized();
        let (pair, interrupted, _native) = flags();
        let registration = SignalRegistration::create(pair)?;
        signal_hook::low_level::raise(signal_hook::consts::SIGINT)
            .map_err(|error| error.to_string())?;
        assert!(
            interrupted.load(Ordering::SeqCst),
            "the first SIGINT must still request cooperative cancellation"
        );
        // If the contract holds this raise never returns: the registration
        // is still alive, native is armed, and the repeated signal takes the
        // default action.
        signal_hook::low_level::raise(signal_hook::consts::SIGINT)
            .map_err(|error| error.to_string())?;
        drop(registration);
        Err("the repeated SIGINT must not return once native termination is armed".to_string())
    }

    #[test]
    fn server_interrupt_install_is_idempotent_and_exposes_flags() -> Result<(), String> {
        // #9895: installing twice must not stack handlers (signal-hook would
        // otherwise hold duplicate registrations for the server process), and
        // the installed pair is what run_check binds its cancellation token to.
        // The install is process-lifetime (`mem::forget`), so the isolated
        // child owns the armed handlers; the runner never keeps them.
        if in_isolated_child() {
            return child_server_interrupt_install_is_idempotent_and_exposes_flags();
        }
        assert_child_success("server_interrupt_install_is_idempotent_and_exposes_flags")
    }

    fn child_server_interrupt_install_is_idempotent_and_exposes_flags() -> Result<(), String> {
        child_ack();
        let _serial = serialized();
        install_server_interrupts().map_err(|error| error.to_string())?;
        install_server_interrupts().map_err(|error| error.to_string())?;
        let (interrupted, _native) = server_interrupt_flags()
            .ok_or_else(|| "install_server_interrupts must expose the flag pair".to_string())?;
        assert!(
            !interrupted.load(Ordering::SeqCst),
            "installation alone must not request cancellation"
        );
        Ok(())
    }
}
