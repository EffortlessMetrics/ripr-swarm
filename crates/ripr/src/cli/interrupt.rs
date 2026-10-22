//! First-SIGINT bridge for one standalone CLI check invocation. Analysis
//! cancellation sets a flag for existing checkpoints and subprocess owners.
//! Publication and repeated signals retain native termination as an escape.

use super::{CommandError, commands};

pub(super) type InterruptFlags = (
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
);

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
            .map_err(|error| format!("check: cannot observe SIGINT: {error}"))?;
        if let Err(error) = signal_hook::flag::register_conditional_default(signal, native.clone())
        {
            signal_hook::low_level::unregister(cancellation);
            return Err(format!("check: cannot preserve native SIGINT: {error}"));
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
                Err(format!("check: cannot arm forced SIGINT: {error}"))
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

#[cfg(all(test, unix))]
mod tests {
    use super::{InterruptFlags, SignalRegistration, check};
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
}
