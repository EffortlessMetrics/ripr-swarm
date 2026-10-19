//! Wall-clock budget for `ripr check` analysis (#5369).
//!
//! `ripr pilot` has always bounded its analysis and written a disclosed
//! partial summary. `check` had no equivalent: one hard owner could run for
//! fifteen minutes with only heartbeat lines on stderr. The budget runs the
//! analysis on a worker thread and, when the deadline passes, cancels the
//! cooperative token and refuses with the typed `analysis_timeout` identity.
//! The refusal carries no findings and is never a clean pass.
//!
//! Cancellation is cooperative, so a worker stuck between checkpoints keeps
//! running until the process exits; the thread boundary, not the token, is
//! what makes the bound hard.

use crate::analysis::cancellation::{
    AnalysisAbortKind, AnalysisCancellationToken, current_token, with_token,
};
use crate::core_error::CoreError;
use std::sync::mpsc;
use std::time::Duration;

/// Default `ripr check` wall-clock budget in seconds. The slowest measured
/// run that finished (a 22-file diff on rust-lang/rust, #5369) took 273 s.
pub(super) const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 600;

/// How long a cancelled worker gets to reach a checkpoint and close its
/// progress run before the refusal is reported.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

/// Stack for the analysis worker thread.
const ANALYSIS_STACK_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn default_check_timeout() -> Option<Duration> {
    Some(Duration::from_secs(DEFAULT_CHECK_TIMEOUT_SECS))
}

/// The refusal diagnostic, also printed on stderr.
fn timeout_detail(budget: Duration) -> String {
    format!(
        "analysis_timeout: ripr check exceeded its {}s wall-clock budget and was stopped. \
         No findings were produced, so this is not a clean result. \
         Repair route: raise or disable the budget (0 disables it) with --timeout SECS or \
         RIPR_CHECK_TIMEOUT=<seconds>, or narrow the diff, then re-run.",
        budget.as_secs()
    )
}

/// Run `work` under `budget`. `None` runs it inline with no thread.
///
/// `on_timeout` runs once when the budget fires and the worker has not
/// finished, before the refusal is returned, so the caller can close any
/// progress output the abandoned worker still holds.
pub(super) fn run_with_budget<T, F>(
    budget: Option<Duration>,
    on_timeout: impl FnOnce(),
    work: F,
) -> Result<T, CoreError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CoreError> + Send + 'static,
{
    let Some(budget) = budget else {
        return work();
    };
    // A standalone check's first-SIGINT token is already active on the
    // caller. Carry it into the budget worker so exporter waits observe the
    // same cancellation request before the outer CLI renders status 130.
    let token = current_token().unwrap_or_else(AnalysisCancellationToken::new);
    let worker_token = token.clone();
    let (tx, rx) = mpsc::channel();
    // The worker replaces the main thread's analysis stack, so it gets at
    // least the main thread's usual 8 MiB; the 2 MiB default would let a
    // deeply nested source abort the process instead of refusing.
    let spawned = std::thread::Builder::new()
        .name("ripr-check-analysis".to_string())
        .stack_size(ANALYSIS_STACK_BYTES)
        .spawn(move || {
            let result = with_token(&worker_token, work);
            let _ignored = tx.send(result);
        });
    if let Err(error) = spawned {
        return Err(CoreError::message(format!(
            "start check analysis thread failed: {error}"
        )));
    }
    match rx.recv_timeout(budget) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            token.cancel(AnalysisAbortKind::DeadlineExceeded);
            let finished = rx.recv_timeout(CANCEL_GRACE).is_ok();
            if !finished {
                on_timeout();
            }
            Err(CoreError::analysis_timeout(timeout_detail(budget)))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(CoreError::message(
            "check analysis stopped before producing a result",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_error::CheckFailureKind;

    #[cfg(unix)]
    #[test]
    fn budget_worker_observes_caller_sigint_token() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };

        let interrupted = Arc::new(AtomicBool::new(true));
        let token = AnalysisCancellationToken::with_interrupt_flags(
            Arc::clone(&interrupted),
            Arc::new(AtomicBool::new(false)),
        );
        let result = with_token(&token, || {
            run_with_budget(
                Some(Duration::from_secs(2)),
                || {},
                || {
                    Ok(crate::analysis::cancellation::checkpoint_typed()
                        .err()
                        .map(|e| e.kind))
                },
            )
        });
        assert!(interrupted.load(Ordering::SeqCst));
        assert_eq!(result, Ok(Some(AnalysisAbortKind::Cancelled)));
    }

    #[test]
    fn work_inside_the_budget_returns_its_result() {
        let result = run_with_budget(Some(Duration::from_secs(30)), || {}, || Ok(7_u32));
        assert_eq!(result, Ok(7));
    }

    #[test]
    fn work_error_inside_the_budget_is_not_a_timeout() -> Result<(), String> {
        let result: Result<u32, _> = run_with_budget(
            Some(Duration::from_secs(30)),
            || {},
            || Err(CoreError::message("boom")),
        );
        let Err(error) = result else {
            return Err("worker failure must propagate".to_string());
        };
        assert_eq!(error.check_failure_kind(), None);
        assert_eq!(error.to_string(), "boom");
        Ok(())
    }

    #[test]
    fn disabled_budget_runs_inline() {
        let caller = std::thread::current().id();
        let result = run_with_budget(
            None,
            || {},
            move || Ok(std::thread::current().id() == caller),
        );
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn exhausted_budget_refuses_with_the_typed_timeout_and_no_result() -> Result<(), String> {
        let started = std::time::Instant::now();
        let result: Result<u32, _> = run_with_budget(
            Some(Duration::from_millis(50)),
            || {},
            || {
                std::thread::sleep(Duration::from_mins(1));
                Ok(1)
            },
        );
        let Err(error) = result else {
            return Err("a stuck worker must hit the budget".to_string());
        };
        assert_eq!(
            error.check_failure_kind(),
            Some(CheckFailureKind::AnalysisTimeout)
        );
        let refusal = error.check_refusal();
        assert_eq!(refusal.identity, "analysis_timeout");
        assert_eq!(refusal.repair_route, "analysis/timeout");
        assert!(error.to_string().contains("not a clean result"));
        // The bound is the budget plus the cancel grace, not the worker's 60 s.
        assert!(started.elapsed() < Duration::from_secs(10));
        Ok(())
    }

    #[test]
    fn exhausted_budget_cancels_the_worker_token() {
        let (seen_tx, seen_rx) = mpsc::channel();
        let result: Result<u32, _> = run_with_budget(
            Some(Duration::from_millis(50)),
            || {},
            move || {
                let token = crate::analysis::cancellation::current_token();
                while token.as_ref().is_some_and(|t| t.abort_kind().is_none()) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let _ignored = seen_tx.send(token.and_then(|t| t.abort_kind()));
                Err(CoreError::message("cancelled"))
            },
        );
        assert_eq!(
            result,
            Err(CoreError::analysis_timeout(timeout_detail(
                Duration::from_millis(50)
            )))
        );
        assert_eq!(
            seen_rx.recv_timeout(Duration::from_secs(5)),
            Ok(Some(AnalysisAbortKind::DeadlineExceeded))
        );
    }
}
