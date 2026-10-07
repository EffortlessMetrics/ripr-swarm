//! Cooperative cancellation for synchronous analysis work.
//!
//! LSP refreshes run on a dedicated analysis thread, so dropping the async
//! future that awaits a refresh cannot stop the analysis closure. This small,
//! dependency-free context lets long-running analysis loops observe that
//! their desired request has been superseded or cancelled and return before
//! publishing a partial result.

use std::cell::RefCell;
use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::time::{Duration, Instant};

const ACTIVE: u8 = 0;
const SUPERSEDED: u8 = 1;
const CANCELLED: u8 = 2;
const DEADLINE_EXCEEDED: u8 = 3;

/// What the most recent error-path event of the attempt was (#4860, #6721).
const OUTCOME_NONE: u8 = 0;
const OUTCOME_ABORT_OBSERVED: u8 = 1;
const OUTCOME_FAILURE_PROPAGATED: u8 = 2;

pub(crate) type AnalysisClock = Arc<dyn Fn() -> Instant + Send + Sync>;

struct AnalysisBudget {
    started: Instant,
    limit: Duration,
    now: AnalysisClock,
}

struct CancellationState {
    reason: AtomicU8,
    /// The latest error-path event: a checkpoint returning the abort to
    /// running work (#4860), or a parallel batch propagating an ordinary
    /// worker failure in preference to a sibling's abort (#6721). A recorded
    /// reason alone does not mean the work stopped because of it: work that
    /// failed or finished before its next checkpoint never saw it.
    outcome: AtomicU8,
    budget: Option<AnalysisBudget>,
}

#[derive(Clone)]
pub(crate) struct AnalysisCancellationToken(Arc<CancellationState>);

impl fmt::Debug for AnalysisCancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalysisCancellationToken")
            .field("reason", &self.abort_kind())
            .field("has_budget", &self.0.budget.is_some())
            .finish()
    }
}

impl PartialEq for AnalysisCancellationToken {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for AnalysisCancellationToken {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AnalysisAbortKind {
    Superseded,
    Cancelled,
    DeadlineExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AnalysisCancellation {
    pub(crate) kind: AnalysisAbortKind,
}

impl fmt::Display for AnalysisCancellation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "analysis cancelled: {:?}", self.kind)
    }
}

impl AnalysisCancellationToken {
    pub(crate) fn new() -> Self {
        Self(Arc::new(CancellationState {
            reason: AtomicU8::new(ACTIVE),
            outcome: AtomicU8::new(OUTCOME_NONE),
            budget: None,
        }))
    }

    /// One owned monotonic clock and origin; no ambient test clock or worker
    /// preemption. A budget is observed only at cooperative checkpoints.
    pub(crate) fn with_budget(started: Instant, limit: Duration, now: AnalysisClock) -> Self {
        Self(Arc::new(CancellationState {
            reason: AtomicU8::new(ACTIVE),
            outcome: AtomicU8::new(OUTCOME_NONE),
            budget: Some(AnalysisBudget {
                started,
                limit,
                now,
            }),
        }))
    }

    pub(crate) fn remaining_budget(&self) -> Option<Duration> {
        self.0.budget.as_ref().map(|budget| {
            budget
                .limit
                .saturating_sub((budget.now)().saturating_duration_since(budget.started))
        })
    }

    /// Pure query: classifying an ordinary source failure must not advance
    /// the clock and replace it with a later deadline failure.
    pub(crate) fn abort_kind(&self) -> Option<AnalysisAbortKind> {
        match self.0.reason.load(Ordering::Acquire) {
            SUPERSEDED => Some(AnalysisAbortKind::Superseded),
            CANCELLED => Some(AnalysisAbortKind::Cancelled),
            DEADLINE_EXCEEDED => Some(AnalysisAbortKind::DeadlineExceeded),
            _ => None,
        }
    }

    pub(crate) fn cancel(&self, kind: AnalysisAbortKind) -> bool {
        let value = match kind {
            AnalysisAbortKind::Superseded => SUPERSEDED,
            AnalysisAbortKind::Cancelled => CANCELLED,
            AnalysisAbortKind::DeadlineExceeded => DEADLINE_EXCEEDED,
        };
        self.0
            .reason
            .compare_exchange(ACTIVE, value, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn checkpoint(&self) -> Result<(), AnalysisCancellation> {
        if self.abort_kind().is_none() && self.remaining_budget().is_some_and(|left| left.is_zero())
        {
            self.cancel(AnalysisAbortKind::DeadlineExceeded);
        }
        // Re-read the winner after CAS, including a concurrent earlier reason.
        match self.abort_kind() {
            None => Ok(()),
            Some(kind) => {
                self.0
                    .outcome
                    .store(OUTCOME_ABORT_OBSERVED, Ordering::Release);
                Err(AnalysisCancellation { kind })
            }
        }
    }

    /// The typed outcome of an attempt that ended in an error (#4860): the
    /// recorded abort, but only once a checkpoint handed it to the work.
    /// Consumers decide cancellation here instead of parsing the error's
    /// rendered text, so a wrapped cancellation stays a cancellation and an
    /// ordinary failure that merely reads like one stays a failure. A few
    /// best-effort walks swallow a checkpoint error and stop early; that still
    /// counts as observed, so a later failure in the same attempt is named by
    /// the abort that truncated it. A parallel batch that propagates an
    /// ordinary worker failure over a sibling's abort records that it did
    /// (#6721), so the attempt is that failure until a later checkpoint hands
    /// the abort to work again. Pure: it never expires a budget.
    pub(crate) fn observed_abort(&self) -> Option<AnalysisAbortKind> {
        if self.0.outcome.load(Ordering::Acquire) == OUTCOME_ABORT_OBSERVED {
            self.abort_kind()
        } else {
            None
        }
    }

    /// Record that the error now propagating is an ordinary failure chosen
    /// over an abort a sibling worker observed (#6721).
    fn record_propagated_failure(&self) {
        self.0
            .outcome
            .store(OUTCOME_FAILURE_PROPAGATED, Ordering::Release);
    }
}

/// One parallel worker's error, typed at the worker so the batch never has to
/// read a rendered message to tell an abort from an ordinary failure (#6721).
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum WorkerError {
    Aborted(AnalysisCancellation),
    Failed(String),
}

impl WorkerError {
    /// The error's existing wording, for the String-typed caller.
    pub(crate) fn into_message(self) -> String {
        match self {
            Self::Aborted(cancellation) => cancellation.to_string(),
            Self::Failed(message) => message,
        }
    }
}

impl From<AnalysisCancellation> for WorkerError {
    fn from(cancellation: AnalysisCancellation) -> Self {
        Self::Aborted(cancellation)
    }
}

/// The error a parallel batch propagates, or `None` when every worker
/// succeeded. The first ordinary failure in input order wins over any abort,
/// so a real source failure is never hidden by whichever sibling happened to
/// reach a checkpoint after the abort; it is recorded on `token` so the
/// attempt is classified as that failure. With no ordinary failure, the first
/// abort in input order is returned with its existing wording.
pub(crate) fn select_batch_error<'a>(
    token: Option<&AnalysisCancellationToken>,
    errors: impl IntoIterator<Item = &'a WorkerError>,
) -> Option<String> {
    let mut first_abort = None;
    for error in errors {
        match error {
            WorkerError::Failed(message) => {
                if let Some(token) = token {
                    token.record_propagated_failure();
                }
                return Some(message.clone());
            }
            WorkerError::Aborted(cancellation) => {
                first_abort.get_or_insert(*cancellation);
            }
        }
    }
    first_abort.map(|cancellation| cancellation.to_string())
}

thread_local! {
    static CURRENT_TOKEN: RefCell<Option<AnalysisCancellationToken>> = const { RefCell::new(None) };
}

struct ContextGuard(Option<AnalysisCancellationToken>);

impl Drop for ContextGuard {
    fn drop(&mut self) {
        let previous = self.0.take();
        CURRENT_TOKEN.with(|slot| {
            *slot.borrow_mut() = previous;
        });
    }
}

pub(crate) fn with_token<T>(token: &AnalysisCancellationToken, work: impl FnOnce() -> T) -> T {
    with_optional_token(Some(token), work)
}

/// Capture the owning request before dispatching work to a different thread.
pub(crate) fn current_token() -> Option<AnalysisCancellationToken> {
    CURRENT_TOKEN.with(|slot| slot.borrow().clone())
}

/// Install exactly the captured context, including no token. Restoring the
/// previous context prevents cancellation leaking between reused pool jobs.
pub(crate) fn with_optional_token<T>(
    token: Option<&AnalysisCancellationToken>,
    work: impl FnOnce() -> T,
) -> T {
    let previous = CURRENT_TOKEN.with(|slot| slot.replace(token.cloned()));
    let _guard = ContextGuard(previous);
    work()
}

pub(crate) fn checkpoint() -> Result<(), String> {
    checkpoint_typed().map_err(|error| error.to_string())
}

/// [`checkpoint`] for callers that carry [`crate::core_error::CoreError`]:
/// the abort stays typed instead of becoming rendered text.
pub(crate) fn checkpoint_typed() -> Result<(), AnalysisCancellation> {
    CURRENT_TOKEN.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(Ok(()), AnalysisCancellationToken::checkpoint)
    })
}

pub(crate) fn remaining_budget() -> Option<Duration> {
    CURRENT_TOKEN.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|token| token.remaining_budget())
    })
}

/// Pure query for the active token's recorded abort. Unlike [`checkpoint`],
/// this never expires a budget or writes a deadline reason.
pub(crate) fn current_abort_kind() -> Option<AnalysisAbortKind> {
    CURRENT_TOKEN.with(|slot| slot.borrow().as_ref().and_then(|token| token.abort_kind()))
}

/// Rendered-wording assertion for tests only (#4860). Production decides
/// cancellation from [`AnalysisCancellationToken::observed_abort`] or
/// [`crate::core_error::CoreError::is_analysis_cancelled`], never from text.
#[cfg(test)]
pub(crate) fn is_cancellation_error(error: &str) -> bool {
    error.starts_with("analysis cancelled:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn optional_worker_context_restores_after_unwind() -> Result<(), String> {
        let outer = AnalysisCancellationToken::new();
        outer.cancel(AnalysisAbortKind::Cancelled);
        let inner = AnalysisCancellationToken::new();
        let result = with_token(&outer, || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_optional_token(Some(&inner), || {
                    assert_eq!(current_token(), Some(inner.clone()));
                    std::panic::resume_unwind(Box::new("intentional context restoration control"));
                });
            }))
        });
        assert_eq!(
            result
                .err()
                .and_then(|payload| payload.downcast_ref::<&str>().copied()),
            Some("intentional context restoration control")
        );
        assert_eq!(current_token(), None);
        // Also verify restoration while the outer request is still installed.
        with_token(&outer, || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_optional_token(None, || std::panic::resume_unwind(Box::new(7usize)))
            }));
            assert_eq!(
                result
                    .err()
                    .and_then(|payload| payload.downcast_ref::<usize>().copied()),
                Some(7)
            );
            assert_eq!(
                checkpoint().err().as_deref(),
                Some("analysis cancelled: Cancelled")
            );
        });
        checkpoint()
    }

    #[test]
    fn optional_worker_context_clears_and_restores_an_outer_request() -> Result<(), String> {
        let token = AnalysisCancellationToken::new();
        token.cancel(AnalysisAbortKind::Cancelled);
        with_token(&token, || {
            assert_eq!(current_token(), Some(token.clone()));
            with_optional_token(None, || {
                assert_eq!(current_token(), None);
                checkpoint()
            })?;
            assert_eq!(
                checkpoint().err().as_deref(),
                Some("analysis cancelled: Cancelled")
            );
            Ok::<(), String>(())
        })?;
        assert_eq!(current_token(), None);
        checkpoint()
    }

    #[test]
    fn owned_budget_expires_and_default_token_has_no_deadline() -> Result<(), String> {
        let elapsed = Arc::new(AtomicUsize::new(0));
        let clock_elapsed = Arc::clone(&elapsed);
        let started = Instant::now();
        let clock: AnalysisClock = Arc::new(move || {
            started + Duration::from_millis(clock_elapsed.load(Ordering::SeqCst) as u64)
        });
        let token =
            AnalysisCancellationToken::with_budget(started, Duration::from_millis(10), clock);
        token.checkpoint().map_err(|error| error.to_string())?;
        elapsed.store(10, Ordering::SeqCst);
        if token.checkpoint().err().map(|error| error.kind)
            != Some(AnalysisAbortKind::DeadlineExceeded)
        {
            return Err("owned budget did not expire at equality".to_string());
        }
        let default = AnalysisCancellationToken::new();
        if default.remaining_budget().is_some() || default.abort_kind().is_some() {
            return Err("default LSP token acquired an automatic deadline".to_string());
        }
        default.checkpoint().map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn expired_clock_preserves_first_reason_and_pure_error_query() -> Result<(), String> {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock_calls = Arc::clone(&calls);
        let started = Instant::now();
        let token = AnalysisCancellationToken::with_budget(
            started,
            Duration::ZERO,
            Arc::new(move || {
                clock_calls.fetch_add(1, Ordering::SeqCst);
                started
            }),
        );
        if token.abort_kind().is_some() || calls.load(Ordering::SeqCst) != 0 {
            return Err("error classification advanced the deadline clock".to_string());
        }
        if !token.cancel(AnalysisAbortKind::Superseded)
            || token.checkpoint().err().map(|error| error.kind)
                != Some(AnalysisAbortKind::Superseded)
            || calls.load(Ordering::SeqCst) != 0
        {
            return Err("expired deadline replaced or polled an earlier cancellation".to_string());
        }
        Ok(())
    }

    #[test]
    fn cancellation_is_visible_inside_scoped_context() -> Result<(), String> {
        let token = AnalysisCancellationToken::new();
        with_token(&token, checkpoint)?;
        if !token.cancel(AnalysisAbortKind::Superseded) {
            return Err("first cancellation should win".to_string());
        }
        let result = with_token(&token, checkpoint);
        if !result
            .as_ref()
            .is_err_and(|error| error.contains("Superseded"))
        {
            return Err(format!("expected superseded cancellation, got {result:?}"));
        }
        Ok(())
    }

    #[test]
    fn nested_active_token_restores_cancelled_outer_context() -> Result<(), String> {
        let outer = AnalysisCancellationToken::new();
        let inner = AnalysisCancellationToken::new();
        if outer != outer.clone() || outer == inner {
            return Err("token identity must follow its owned shared state".to_string());
        }
        if !outer.cancel(AnalysisAbortKind::Superseded) {
            return Err("fixture could not cancel its outer token".to_string());
        }

        with_token(&outer, || {
            with_token(&inner, checkpoint)?;
            if !checkpoint().is_err_and(|error| error.contains("Superseded")) {
                return Err("nested token did not restore the cancelled outer context".to_string());
            }
            Ok(())
        })?;
        // The active token's constructor must remain deadline-free for LSP
        // callers, and an outer cancellation must not leak beyond its scope.
        inner.checkpoint().map_err(|error| error.to_string())?;
        checkpoint()?;
        Ok(())
    }

    #[test]
    fn deadline_exceeded_is_reported_by_checkpoint() -> Result<(), String> {
        let token = AnalysisCancellationToken::new();
        if !token.cancel(AnalysisAbortKind::DeadlineExceeded) {
            return Err("deadline cancellation should win on an active token".to_string());
        }
        let result = with_token(&token, checkpoint);
        if !result
            .as_ref()
            .is_err_and(|error| error.contains("DeadlineExceeded"))
        {
            return Err(format!(
                "expected deadline-exceeded cancellation, got {result:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn deadline_cancel_loses_to_an_earlier_superseded() -> Result<(), String> {
        let token = AnalysisCancellationToken::new();
        if !token.cancel(AnalysisAbortKind::Superseded) {
            return Err("first cancellation should win".to_string());
        }
        if token.cancel(AnalysisAbortKind::DeadlineExceeded) {
            return Err("deadline cancel must lose to an earlier superseded".to_string());
        }
        let result = with_token(&token, checkpoint);
        if !result
            .as_ref()
            .is_err_and(|error| error.contains("Superseded"))
        {
            return Err(format!(
                "expected the earlier superseded outcome to be preserved, got {result:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn a_batch_propagates_its_first_ordinary_failure_over_any_abort() {
        // #6721: in either input order, an ordinary worker failure wins over a
        // sibling's abort, and the token then names the attempt as that
        // failure. A later checkpoint that hands the abort to work again makes
        // the abort the attempt's outcome once more.
        let failed = || WorkerError::Failed("failed to read src/a.rs".to_string());
        let aborted = || {
            WorkerError::Aborted(AnalysisCancellation {
                kind: AnalysisAbortKind::DeadlineExceeded,
            })
        };
        for errors in [vec![aborted(), failed()], vec![failed(), aborted()]] {
            let token = AnalysisCancellationToken::new();
            assert!(token.cancel(AnalysisAbortKind::DeadlineExceeded));
            assert!(token.checkpoint().is_err());
            assert_eq!(
                token.observed_abort(),
                Some(AnalysisAbortKind::DeadlineExceeded)
            );
            assert_eq!(
                select_batch_error(Some(&token), &errors),
                Some("failed to read src/a.rs".to_string())
            );
            assert_eq!(token.observed_abort(), None, "{errors:?}");
            assert!(token.checkpoint().is_err());
            assert_eq!(
                token.observed_abort(),
                Some(AnalysisAbortKind::DeadlineExceeded)
            );
        }

        // Two ordinary failures: the first in input order wins.
        let two = [
            WorkerError::Failed("first".to_string()),
            WorkerError::Failed("second".to_string()),
        ];
        assert_eq!(select_batch_error(None, &two), Some("first".to_string()));

        // Only aborts: the first abort's existing wording, still observed.
        let token = AnalysisCancellationToken::new();
        assert!(token.cancel(AnalysisAbortKind::Superseded));
        assert!(token.checkpoint().is_err());
        let aborts = [
            WorkerError::Aborted(AnalysisCancellation {
                kind: AnalysisAbortKind::Superseded,
            }),
            aborted(),
        ];
        assert_eq!(
            select_batch_error(Some(&token), &aborts),
            Some("analysis cancelled: Superseded".to_string())
        );
        assert_eq!(token.observed_abort(), Some(AnalysisAbortKind::Superseded));

        // No errors: nothing to propagate, and the token is untouched.
        let clean = AnalysisCancellationToken::new();
        assert_eq!(select_batch_error(Some(&clean), std::iter::empty()), None);
        assert_eq!(clean.observed_abort(), None);
    }

    #[test]
    fn observed_abort_requires_a_checkpoint_to_hand_the_abort_to_work() {
        // #4860: a recorded reason alone does not name the attempt's outcome.
        let token = AnalysisCancellationToken::new();
        assert_eq!(token.observed_abort(), None);
        assert!(token.cancel(AnalysisAbortKind::Superseded));
        assert_eq!(token.abort_kind(), Some(AnalysisAbortKind::Superseded));
        assert_eq!(
            token.observed_abort(),
            None,
            "work that never reached a checkpoint was not stopped by the abort"
        );
        assert_eq!(
            token.checkpoint(),
            Err(AnalysisCancellation {
                kind: AnalysisAbortKind::Superseded
            })
        );
        assert_eq!(token.observed_abort(), Some(AnalysisAbortKind::Superseded));
        // First-cancel-wins: a later deadline cannot relabel the outcome.
        assert!(!token.cancel(AnalysisAbortKind::DeadlineExceeded));
        assert_eq!(token.observed_abort(), Some(AnalysisAbortKind::Superseded));
    }

    #[test]
    fn budget_expiry_is_observed_as_the_deadline_kind_through_the_free_checkpoint() {
        let started = Instant::now();
        let token = AnalysisCancellationToken::with_budget(
            started,
            Duration::ZERO,
            Arc::new(move || started),
        );
        assert_eq!(token.observed_abort(), None);
        let typed = with_token(&token, checkpoint_typed);
        assert_eq!(
            typed,
            Err(AnalysisCancellation {
                kind: AnalysisAbortKind::DeadlineExceeded
            })
        );
        assert_eq!(
            token.observed_abort(),
            Some(AnalysisAbortKind::DeadlineExceeded)
        );
        // The String checkpoint keeps the public wording.
        assert_eq!(
            with_token(&token, checkpoint).err().as_deref(),
            Some("analysis cancelled: DeadlineExceeded")
        );
    }
}
