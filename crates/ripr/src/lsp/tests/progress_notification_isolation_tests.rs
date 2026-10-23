use super::*;
use crate::lsp::progress::{BoxFuture, ProgressNotificationValue, ProgressSink, RecordingSink};
use std::sync::Arc;

/// Only queued B's Begin is suspended. A's sends remain immediately ready,
/// so waiting for B cannot be attributed to shared transport backpressure.
struct SelectiveBeginSink {
    recorder: Arc<RecordingSink>,
    held_token: String,
    healthy_token: String,
    healthy_end: tokio::sync::Notify,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

impl ProgressSink for SelectiveBeginSink {
    fn create(&self, token: String) -> BoxFuture<'_, Result<(), String>> {
        self.recorder.create(token)
    }

    fn notify(&self, token: String, value: ProgressNotificationValue) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            if token == self.held_token && matches!(value, ProgressNotificationValue::Begin { .. })
            {
                self.entered.notify_one();
                self.release.notified().await;
            }
            let healthy_end = token == self.healthy_token
                && matches!(value, ProgressNotificationValue::End { .. });
            self.recorder.notify(token, value).await;
            if healthy_end {
                self.healthy_end.notify_one();
            }
        })
    }
}

fn terminal_count(sink: &RecordingSink, token: &str) -> usize {
    sink.events()
        .iter()
        .filter(|event| matches!(event, ProgressEvent::End { token: ended, .. } if ended == token))
        .count()
}

fn active_completion_with_held_begin(report_while_held: bool) -> Result<(), String> {
    // The enclosing runtime owns all spawned tasks and cancels them on return,
    // including when setup, announcement joining or cleanup exceeds this budget.
    work_done_progress_runtime()?.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (service, socket) = LspService::new(|client| Backend::new(client, PathBuf::from(".")));
            drop(socket);
            let backend = service.inner();
            let recorder = backend.install_progress_recorder();
            let first = work_done_progress_request(backend, 1)?;
            let active = started_request(&first)?;
            backend.emit_recorded_progress_for_decision(&first).await?;
            if !report_while_held {
                // A has passed the publishing progress report; isolate its End.
                backend.progress.report_publishing(active.generation).await;
            }
            let second = work_done_progress_request(backend, 2)?;
            let RefreshDecision::Queued { generation, .. } = second else {
                return Err(format!("SETUP: expected queued B, got {second:?}"));
            };
            let active_token = format!("ripr-analysis-{}", active.generation);
            let queued_token = format!("ripr-analysis-{generation}");
            if generation == active.generation {
                return Err("SETUP: the control requires two distinct generations".into());
            }
            let sink = Arc::new(SelectiveBeginSink {
                recorder: Arc::clone(&recorder),
                held_token: queued_token.clone(),
                healthy_token: active_token.clone(),
                healthy_end: tokio::sync::Notify::new(),
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            });
            backend.progress.install_test_sink(sink.clone());
            let announcement = backend.emit_progress_for_decision(&second).await
                .ok_or_else(|| "SETUP: queued B was not registered".to_string())?;
            sink.entered.notified().await;

            // Exercise the production progress-report, terminal outcome and
            // scheduler-finish owners; no diagnostics or snapshot is published here.
            // Do not include B's later transition: same-token operations must still wait for its Begin.
            let completion = tokio::time::timeout(Duration::from_millis(200), async {
                if report_while_held {
                    backend.progress.report_publishing(active.generation).await;
                }
                backend.record_health_outcome(&active, RefreshAttemptOutcome::Published);
                backend.end_progress_for_attempt(&active, RefreshAttemptOutcome::Published).await;
                let promoted = backend.refresh_scheduler_for_test().finish(&active, true)
                    .ok_or_else(|| "active completion did not promote queued B".to_string())?;
                if promoted.generation != generation {
                    return Err(format!("wrong promoted generation: {}", promoted.generation));
                }
                if terminal_count(&recorder, &active_token) != 1 {
                    return Err(format!("active A did not end exactly once: {:?}", recorder.events()));
                }
                Ok::<(), String>(())
            }).await;
            let before_release = recorder.events();
            sink.release.notify_one();
            announcement.await.map_err(|error| format!("queued announcement failed: {error}"))?;
            backend.progress.end_all(crate::lsp::progress::AnalysisProgressEnd::Cancelled).await;
            completion.map_err(|_elapsed| format!(
                "active A completion/promotion waited for queued B's suspended Begin: {before_release:?}"
            ))??;
            if report_while_held && !before_release.iter().any(|event| matches!(event,
                ProgressEvent::Report { token, message }
                    if token == &active_token && message == "publishing diagnostics"
            )) {
                return Err(format!("active A publishing report missing before B release: {before_release:?}"));
            }
            if before_release.iter().any(|event| matches!(event,
                ProgressEvent::Begin { token, .. } if token == &queued_token
            )) {
                return Err("SETUP: queued B's Begin was not actually suspended".into());
            }
            if terminal_count(&recorder, &active_token) != 1
                || terminal_count(&recorder, &queued_token) != 1
            {
                return Err(format!("two-token terminal lifecycle drifted: {:?}", recorder.events()));
            }
            Ok(())
        }).await.map_err(|_elapsed| "two-token exchange exceeded its 5-second budget".to_string())?
    })
}

#[test]
fn queued_begin_cannot_hold_active_attempt_completion_and_promotion() -> Result<(), String> {
    active_completion_with_held_begin(false)
}

#[test]
fn queued_begin_cannot_hold_active_publishing_report() -> Result<(), String> {
    active_completion_with_held_begin(true)
}

// This boundary control observes the awaited drain. Per-token isolation
// does not add an end_all or shutdown latency budget.
#[test]
fn end_all_preserves_order_but_still_awaits_its_suspended_token() -> Result<(), String> {
    // The enclosing runtime owns all spawned tasks and cancels them on return,
    // including when setup, announcement joining or cleanup exceeds this budget.
    work_done_progress_runtime()?.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (service, socket) =
                LspService::new(|client| Backend::new(client, PathBuf::from(".")));
            drop(socket);
            let backend = service.inner();
            let recorder = backend.install_progress_recorder();
            let first = work_done_progress_request(backend, 1)?;
            let active = started_request(&first)?;
            backend.emit_recorded_progress_for_decision(&first).await?;
            let second = work_done_progress_request(backend, 2)?;
            let RefreshDecision::Queued { generation, .. } = second else {
                return Err(format!("SETUP: expected queued B, got {second:?}"));
            };
            let active_token = format!("ripr-analysis-{}", active.generation);
            let queued_token = format!("ripr-analysis-{generation}");
            let sink = Arc::new(SelectiveBeginSink {
                recorder: Arc::clone(&recorder),
                held_token: queued_token.clone(),
                healthy_token: active_token.clone(),
                healthy_end: tokio::sync::Notify::new(),
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            });
            backend.progress.install_test_sink(sink.clone());
            let announcement = backend
                .emit_progress_for_decision(&second)
                .await
                .ok_or_else(|| "SETUP: queued B was not registered".to_string())?;
            sink.entered.notified().await;
            let tracker = Arc::clone(&backend.progress);
            let drain = tokio::spawn(async move {
                tracker
                    .end_all(crate::lsp::progress::AnalysisProgressEnd::Cancelled)
                    .await;
            });
            let healthy_end =
                tokio::time::timeout(Duration::from_millis(200), sink.healthy_end.notified()).await;
            let pending = !drain.is_finished();
            let active_ended = terminal_count(&recorder, &active_token);
            sink.release.notify_one();
            announcement
                .await
                .map_err(|error| format!("queued announcement failed: {error}"))?;
            drain
                .await
                .map_err(|error| format!("drain task failed: {error}"))?;
            healthy_end.map_err(|_elapsed| {
                "end_all held healthy active A behind queued B's Begin".to_string()
            })?;
            if !pending {
                return Err("end_all returned before the held same-token Begin completed".into());
            }
            if active_ended != 1 {
                return Err("end_all held healthy active A behind queued B's Begin".into());
            }
            let queued_events: Vec<_> = recorder
                .events()
                .into_iter()
                .filter(|event| match event {
                    ProgressEvent::Create { token }
                    | ProgressEvent::Begin { token, .. }
                    | ProgressEvent::Report { token, .. }
                    | ProgressEvent::End { token, .. } => token == &queued_token,
                })
                .collect();
            if !matches!(
                queued_events.as_slice(),
                [
                    ProgressEvent::Create { .. },
                    ProgressEvent::Begin { .. },
                    ProgressEvent::End { .. }
                ]
            ) || terminal_count(&recorder, &active_token) != 1
            {
                return Err(format!(
                    "drain lost same-token order/exactly-once end: {:?}",
                    recorder.events()
                ));
            }
            Ok(())
        })
        .await
        .map_err(|_elapsed| "two-token exchange exceeded its 5-second budget".to_string())?
    })
}
