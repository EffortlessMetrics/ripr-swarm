use super::*;
use std::sync::atomic::AtomicUsize;

#[cfg(unix)]
#[test]
fn producer_sigint_death_is_cancellation_once_the_cli_flag_arrives() -> Result<(), String> {
    let status = std::process::Command::new("sh")
        .args(["-c", "kill -INT $$"])
        .status()
        .map_err(|e| e.to_string())?;
    assert!(
        exited_by_sigint(&status),
        "fixture must die by SIGINT: {status:?}"
    );
    let flag = Arc::new(AtomicBool::new(false));
    let token = AnalysisCancellationToken::with_interrupt_flags(
        flag.clone(),
        Arc::new(AtomicBool::new(false)),
    );
    // The flag lands after the producer's exit is already observed.
    let late = std::thread::spawn({
        let flag = flag.clone();
        move || {
            std::thread::sleep(Duration::from_millis(50));
            flag.store(true, Ordering::SeqCst);
        }
    });
    let interrupted = with_token(&token, || cli_interrupted_after_exit(&status));
    late.join()
        .map_err(|_panic| "flag thread failed".to_string())?;
    assert!(
        interrupted,
        "SIGINT death with a late CLI flag is an interrupt"
    );

    // Negative: no flag ever arrives, so it stays a producer failure.
    let silent = AnalysisCancellationToken::with_interrupt_flags(
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let failed = with_token(&silent, || {
        cli_interrupted_after_exit_within(&status, Duration::from_millis(20))
    });
    assert!(!failed, "an unannounced SIGINT death must stay a failure");
    // Negative: an ordinary token owns no flag to await.
    let ordinary = AnalysisCancellationToken::new();
    assert!(!with_token(&ordinary, || cli_interrupted_after_exit(
        &status
    )));
    Ok(())
}

#[cfg(unix)]
#[test]
fn server_group_authority_follows_captured_token_only() -> Result<(), String> {
    assert!(!owns_process_groups());
    let flag = Arc::new(AtomicBool::new(false));
    let server = AnalysisCancellationToken::with_server_interrupt(flag.clone());
    let ordinary = AnalysisCancellationToken::new();
    let cli = AnalysisCancellationToken::with_interrupt_flags(
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    for token in [&ordinary, &cli] {
        with_token(token, || assert!(!owns_process_groups()));
    }
    let captured = with_token(&server, || {
        assert!(owns_process_groups());
        begin_cli_publication()?;
        assert!(!flag.load(Ordering::SeqCst));
        current_token().ok_or_else(|| "server context missing".to_string())
    })?;
    let worker = captured.for_check_budget_worker();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                assert!(!owns_process_groups());
                with_optional_token(Some(&worker), || {
                    assert!(owns_process_groups());
                    flag.store(true, Ordering::SeqCst);
                    assert_eq!(
                        checkpoint_typed(),
                        Err(AnalysisCancellation {
                            kind: AnalysisAbortKind::Cancelled
                        }),
                    );
                });
                assert!(!owns_process_groups());
            })
            .join()
            .map_err(|_join| "captured server worker failed".to_string())
    })?;
    assert!(!owns_process_groups());
    Ok(())
}

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
    let token = AnalysisCancellationToken::with_budget(started, Duration::from_millis(10), clock);
    token.checkpoint().map_err(|error| error.to_string())?;
    elapsed.store(10, Ordering::SeqCst);
    if token.checkpoint().err().map(|error| error.kind) != Some(AnalysisAbortKind::DeadlineExceeded)
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
        || token.checkpoint().err().map(|error| error.kind) != Some(AnalysisAbortKind::Superseded)
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
    let token =
        AnalysisCancellationToken::with_budget(started, Duration::ZERO, Arc::new(move || started));
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
