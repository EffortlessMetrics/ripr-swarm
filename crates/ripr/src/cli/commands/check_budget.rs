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
/// `input_facts`, when given, is the capture slot a deadline refusal reads:
/// the run's deadline is stamped here, when the budget starts, and the worker
/// publishes into the slot with that stamp, so a producer finishing inside
/// the cancel grace is recorded post-deadline instead of as available
/// context (#10796 review).
///
/// `on_timeout` runs once when the budget fires and the worker has not
/// finished, before the refusal is returned, so the caller can close any
/// progress output the abandoned worker still holds.
pub(super) fn run_with_budget<T, F>(
    budget: Option<Duration>,
    input_facts: Option<&crate::analysis::InputFactsSlot>,
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
    // The deadline stamp is taken when the budget starts — the same instant
    // `rx.recv_timeout(budget)` measures against — never from wall-clock
    // observation inside a producer.
    let deadline = std::time::Instant::now() + budget;
    // The standalone caller owns first-SIGINT flags, but a budget expiry
    // must cancel only this worker. Copy the flags into a separate token so
    // exporter waits can settle before the caller renders status 130.
    let token = current_token().map_or_else(AnalysisCancellationToken::new, |caller| {
        caller.for_check_budget_worker()
    });
    let worker_token = token.clone();
    let worker_slot = input_facts.map(std::sync::Arc::clone);
    let (tx, rx) = mpsc::channel();
    #[cfg(test)]
    let rx = ObservedReceiver(rx);
    // The worker replaces the main thread's analysis stack, so it gets at
    // least the main thread's usual 8 MiB; the 2 MiB default would let a
    // deeply nested source abort the process instead of refusing.
    let spawned = std::thread::Builder::new()
        .name("ripr-check-analysis".to_string())
        .stack_size(ANALYSIS_STACK_BYTES)
        .spawn(move || {
            // `with_input_facts_capture` restores the previous registration
            // only on normal return; that is safe here because this worker
            // thread terminates as soon as `work` returns — a panic unwinds
            // out of the closure and the thread-local dies with the thread —
            // and the inline `None`-budget path above exits before any
            // sibling run can reuse this thread.
            let result = match worker_slot {
                Some(slot) => {
                    crate::analysis::with_input_facts_capture(slot, Some(deadline), move || {
                        with_token(&worker_token, work)
                    })
                }
                None => with_token(&worker_token, work),
            };
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
        // The worker dropped its sender without sending: it panicked (the
        // panic hook already printed the message on stderr). No result exists,
        // so this is a typed refusal, never a clean or partial result.
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(CoreError::analysis_worker_stopped(
            worker_stopped_detail("ripr check", "narrow the diff or base range"),
        )),
    }
}

/// The refusal diagnostic for an analysis worker that stopped without a
/// result, shared by `check` and `pilot`. `narrow` names the command's own
/// way to reduce scope.
pub(super) fn worker_stopped_detail(command: &str, narrow: &str) -> String {
    format!(
        "analysis_worker_stopped: the {command} analysis worker stopped before producing a \
         result, most likely because it panicked (the panic message is printed above on stderr). \
         No findings were produced, so this is not a clean result. \
         Repair route: re-run; if it repeats, report it with the panic hook output \
         (RUST_BACKTRACE=1 adds a backtrace) and {narrow} to isolate the input."
    )
}

// Production calls the standard receiver directly. This test-only observer
// captures the argument at that method boundary, so there is no production
// forwarding adapter below the trace whose argument could diverge from it.
#[cfg(test)]
struct ObservedReceiver<T>(mpsc::Receiver<T>);

#[cfg(test)]
impl<T> ObservedReceiver<T> {
    fn recv_timeout(&self, timeout: Duration) -> Result<T, mpsc::RecvTimeoutError> {
        WAIT_TRACE.with(|trace| {
            if let Some(waits) = trace.borrow_mut().as_mut() {
                waits.push(timeout);
            }
        });
        self.0.recv_timeout(timeout)
    }
}

#[cfg(test)]
impl<T> std::ops::Deref for ObservedReceiver<T> {
    type Target = mpsc::Receiver<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
thread_local! {
    static WAIT_TRACE: std::cell::RefCell<Option<Vec<Duration>>> = const {
        std::cell::RefCell::new(None)
    };
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
                None,
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
    fn budget_timeout_does_not_cancel_caller_token() {
        let caller = AnalysisCancellationToken::new();
        let result: Result<(), _> = with_token(&caller, || {
            run_with_budget(
                Some(Duration::from_millis(30)),
                None,
                || {},
                || {
                    while crate::analysis::cancellation::checkpoint_typed().is_ok() {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok(())
                },
            )
        });
        assert!(result.is_err(), "worker deadline must remain a refusal");
        assert_eq!(
            caller.abort_kind(),
            None,
            "caller must still publish refusal"
        );
    }

    #[test]
    fn work_inside_the_budget_returns_its_result() {
        let result = run_with_budget(Some(Duration::from_secs(30)), None, || {}, || Ok(7_u32));
        assert_eq!(result, Ok(7));
    }

    #[test]
    fn work_error_inside_the_budget_is_not_a_timeout() -> Result<(), String> {
        let result: Result<u32, _> = run_with_budget(
            Some(Duration::from_secs(30)),
            None,
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
            None,
            || {},
            move || Ok(std::thread::current().id() == caller),
        );
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn exhausted_budget_refuses_with_the_typed_timeout_and_no_result() -> Result<(), String> {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        // Outer containment: if the grace wait is broken into an unbounded
        // wait for the worker, this releases the worker after a finite time
        // so the run ends with a failed assertion instead of hanging.
        const CONTAINMENT: Duration = Duration::from_secs(20);
        // The worker parks until released, so it cannot finish inside the
        // budget or the grace on its own. `on_timeout` runs only when the
        // grace elapsed with the worker still parked. The wait trace checks
        // the durations actually passed by the production entry point; it
        // does not assert the scheduler or standard library's elapsed time.
        let (release, parked) = mpsc::channel::<()>();
        let (done, done_rx) = mpsc::channel::<()>();
        let (worker_done, worker_done_rx) = mpsc::channel::<()>();
        let watchdog_release = release.clone();
        let watchdog = std::thread::spawn(move || {
            if matches!(
                done_rx.recv_timeout(CONTAINMENT),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                let _ = watchdog_release.send(());
            }
        });
        let grace_elapsed_with_worker_parked = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&grace_elapsed_with_worker_parked);
        WAIT_TRACE.with(|trace| *trace.borrow_mut() = Some(Vec::new()));
        let result: Result<u32, _> = run_with_budget(
            Some(Duration::from_millis(50)),
            None,
            move || observed.store(true, Ordering::SeqCst),
            move || {
                let _ = parked.recv();
                let _ = worker_done.send(());
                Ok(1)
            },
        );
        let waits = WAIT_TRACE.with(|trace| trace.borrow_mut().take().unwrap_or_default());
        let _ = done.send(());
        let _ = release.send(());
        watchdog
            .join()
            .map_err(|_panic_payload| "watchdog panicked".to_string())?;
        assert_eq!(worker_done_rx.recv_timeout(CONTAINMENT), Ok(()));
        assert_eq!(
            waits,
            vec![Duration::from_millis(50), Duration::from_secs(2)]
        );
        assert!(
            grace_elapsed_with_worker_parked.load(Ordering::SeqCst),
            "the grace wait must expire with the worker still parked, not wait for it"
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
        Ok(())
    }

    #[test]
    fn panicking_worker_refuses_with_the_typed_worker_stopped_failure() -> Result<(), String> {
        // The worker unwinds before sending, so the channel disconnects (the
        // production hook would print the panic message on stderr); the
        // refusal must be typed and carry no result.
        let result: Result<u32, _> = run_with_budget(
            Some(Duration::from_secs(30)),
            None,
            || {},
            || -> Result<u32, CoreError> {
                std::panic::resume_unwind(Box::new("injected check worker fault (#11116)"))
            },
        );
        let Err(error) = result else {
            return Err("a dead worker must not yield a result".to_string());
        };
        assert_eq!(
            error.check_failure_kind(),
            Some(CheckFailureKind::AnalysisWorkerStopped)
        );
        let refusal = error.check_refusal();
        assert_eq!(refusal.identity, "analysis_worker_stopped");
        assert_eq!(refusal.repair_route, "analysis/worker-stopped");
        let text = error.to_string();
        for needle in ["panicked", "not a clean result", "re-run", "panic hook"] {
            assert!(text.contains(needle), "missing {needle:?}: {text}");
        }
        assert!(!text.contains("injected check worker fault"));
        Ok(())
    }

    #[test]
    fn exhausted_budget_cancels_the_worker_token() {
        let (seen_tx, seen_rx) = mpsc::channel();
        let result: Result<u32, _> = run_with_budget(
            Some(Duration::from_millis(50)),
            None,
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

    // Exercise the real budget worker and preserve each test's published facts.
    fn timed_capture(
        budget: Duration,
        publish: impl FnOnce() + Send + 'static,
    ) -> Result<crate::analysis::InputFactsSlot, String> {
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let result: Result<u32, _> = run_with_budget(
            Some(budget),
            Some(&slot),
            || {},
            move || {
                publish();
                while crate::analysis::cancellation::checkpoint_typed().is_ok() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(1)
            },
        );
        let Err(error) = result else {
            return Err("the stalled worker must hit the budget".to_string());
        };
        assert_eq!(
            error.check_failure_kind(),
            Some(CheckFailureKind::AnalysisTimeout)
        );
        Ok(slot)
    }

    #[test]
    fn budget_timeout_before_discovery_retains_no_input_facts() -> Result<(), String> {
        // #10796 decisive control, early timeout: the worker is stuck before
        // any producer publishes. The deadline is injected (50 ms), never
        // derived from wall-clock observation, and the refusal must read as
        // "no input facts" — an empty capture slot, not fabricated zeroes.
        let slot = timed_capture(Duration::from_millis(50), || {})?;
        if crate::analysis::published_input_facts(&slot).is_some() {
            return Err("no fact may be retained before its producer completed".to_string());
        }
        Ok(())
    }

    #[test]
    fn budget_timeout_after_published_input_facts_retains_them() -> Result<(), String> {
        // Retain both producers' facts without a post-deadline mask. The 500ms
        // budget allows for a slow worker start after the deadline is stamped;
        // the worker exits shortly after cancellation (#10796).
        let slot = timed_capture(Duration::from_millis(500), || {
            crate::analysis::publish_loaded_input_facts(
                crate::analysis::InputSource::WorkingTree,
                &crate::analysis::LoadedDiff {
                    text: String::new(),
                    effective_base: Some("origin/main".to_string()),
                    resolved_base_commit: Some("06e6aea".to_string()),
                    merge_base_origin: Some("4d3c2b1".to_string()),
                },
            );
            crate::analysis::publish_scoped_intake_facts(6, 2);
        })?;
        let captured = crate::analysis::published_input_facts(&slot)
            .ok_or("published facts must be retained for the refusal")?;
        let facts = &captured.facts;
        assert_eq!(
            facts.source,
            Some(crate::analysis::InputSource::WorkingTree)
        );
        assert_eq!(facts.effective_base.as_deref(), Some("origin/main"));
        assert_eq!(facts.resolved_base_commit.as_deref(), Some("06e6aea"));
        assert_eq!(facts.merge_base_origin.as_deref(), Some("4d3c2b1"));
        assert_eq!(facts.scoped_changed_file_count, Some(6));
        assert_eq!(facts.out_of_scope_file_count, Some(2));
        assert_eq!(
            captured.post_deadline,
            crate::analysis::PublishedInputFacts::default(),
            "pre-deadline publishes must stay unmasked"
        );
        assert!(
            captured.loader_published && captured.intake_published,
            "both producing stages completed before the deadline"
        );
        Ok(())
    }

    #[test]
    fn budget_timeout_loader_published_but_intake_interrupted_keeps_the_deadline_reason()
    -> Result<(), String> {
        // #10796 review: the loader completing must not speak for the intake.
        // When only the load stage published, the renderer must keep
        // `not_published_before_deadline` for the counts instead of the false
        // `not_computed_for_source`.
        let slot = timed_capture(Duration::from_millis(500), || {
            crate::analysis::publish_loaded_input_facts(
                crate::analysis::InputSource::WorkingTree,
                &crate::analysis::LoadedDiff {
                    text: String::new(),
                    effective_base: Some("origin/main".to_string()),
                    resolved_base_commit: Some("06e6aea".to_string()),
                    merge_base_origin: None,
                },
            );
            // The intake never runs: the budget fires inside it.
        })?;
        let captured = crate::analysis::published_input_facts(&slot)
            .ok_or("loader facts must be retained for the refusal")?;
        assert!(
            captured.loader_published,
            "the load stage completed before the deadline"
        );
        assert!(
            !captured.intake_published,
            "the intake must not read as completed when the budget fired inside it"
        );
        assert_eq!(
            captured.facts.scoped_changed_file_count, None,
            "no intake count may exist without an intake completion"
        );
        Ok(())
    }

    #[test]
    fn budget_timeout_facts_cover_committed_and_supplied_diff_modes() -> Result<(), String> {
        // #10796 decisive controls, source modes: the loader publishes the
        // source label with its own revision facts; a supplied diff publishes
        // no base facts because its producer never computes them.
        for (source, expect_base) in [
            (crate::analysis::InputSource::Committed, true),
            (crate::analysis::InputSource::SuppliedDiff, false),
        ] {
            let slot: crate::analysis::InputFactsSlot =
                std::sync::Arc::new(std::sync::Mutex::new(None));
            // Mirror the real loader: a supplied diff leaves the base unset
            // because its producer never computes one.
            let loaded = crate::analysis::LoadedDiff {
                text: String::new(),
                effective_base: if expect_base {
                    Some("origin/main".to_string())
                } else {
                    None
                },
                resolved_base_commit: Some("06e6aea".to_string()),
                merge_base_origin: None,
            };
            crate::analysis::with_input_facts_capture(std::sync::Arc::clone(&slot), None, || {
                crate::analysis::publish_loaded_input_facts(source, &loaded);
            });
            let captured = crate::analysis::published_input_facts(&slot)
                .ok_or("loaded facts must publish for every source mode")?;
            let facts = &captured.facts;
            assert_eq!(facts.source, Some(source));
            if expect_base {
                assert_eq!(facts.effective_base.as_deref(), Some("origin/main"));
            } else {
                assert!(
                    facts.effective_base.is_none(),
                    "a supplied diff never computes a base"
                );
            }
        }
        Ok(())
    }
}
