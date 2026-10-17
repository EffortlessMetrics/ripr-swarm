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

use crate::analysis::cancellation::{AnalysisAbortKind, AnalysisCancellationToken, with_token};
use crate::core_error::CoreError;
use std::sync::mpsc;
use std::time::Duration;

/// Default `ripr check` wall-clock budget in seconds. The slowest measured
/// run that finished (a 22-file diff on rust-lang/rust, #5369) took 273 s.
pub(super) const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 600;

/// How long a cancelled worker gets to reach a checkpoint and close its
/// progress run before the refusal is reported.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

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
pub(super) fn run_with_budget<T, F>(budget: Option<Duration>, work: F) -> Result<T, CoreError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CoreError> + Send + 'static,
{
    let Some(budget) = budget else {
        return work();
    };
    let token = AnalysisCancellationToken::new();
    let worker_token = token.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = with_token(&worker_token, work);
        let _ignored = tx.send(result);
    });
    match rx.recv_timeout(budget) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            token.cancel(AnalysisAbortKind::DeadlineExceeded);
            let _ignored = rx.recv_timeout(CANCEL_GRACE);
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

    #[test]
    fn work_inside_the_budget_returns_its_result() {
        let result = run_with_budget(Some(Duration::from_secs(30)), || Ok(7_u32));
        assert_eq!(result, Ok(7));
    }

    #[test]
    fn work_error_inside_the_budget_is_not_a_timeout() {
        let result: Result<u32, _> = run_with_budget(Some(Duration::from_secs(30)), || {
            Err(CoreError::message("boom"))
        });
        let error = result.expect_err("worker failure must propagate");
        assert_eq!(error.check_failure_kind(), None);
        assert_eq!(error.to_string(), "boom");
    }

    #[test]
    fn disabled_budget_runs_inline() {
        let caller = std::thread::current().id();
        let result = run_with_budget(None, move || Ok(std::thread::current().id() == caller));
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn exhausted_budget_refuses_with_the_typed_timeout_and_no_result() {
        let started = std::time::Instant::now();
        let result: Result<u32, _> = run_with_budget(Some(Duration::from_millis(50)), || {
            std::thread::sleep(Duration::from_secs(60));
            Ok(1)
        });
        let error = result.expect_err("a stuck worker must hit the budget");
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
    }

    #[test]
    fn exhausted_budget_cancels_the_worker_token() {
        let (seen_tx, seen_rx) = mpsc::channel();
        let result: Result<u32, _> = run_with_budget(Some(Duration::from_millis(50)), move || {
            let token = crate::analysis::cancellation::current_token();
            while token.as_ref().is_some_and(|t| t.abort_kind().is_none()) {
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ignored = seen_tx.send(token.and_then(|t| t.abort_kind()));
            Err(CoreError::message("cancelled"))
        });
        assert!(result.is_err());
        assert_eq!(
            seen_rx.recv_timeout(Duration::from_secs(5)),
            Ok(Some(AnalysisAbortKind::DeadlineExceeded))
        );
    }
}
