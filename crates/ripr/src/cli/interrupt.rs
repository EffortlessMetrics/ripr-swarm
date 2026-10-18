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
