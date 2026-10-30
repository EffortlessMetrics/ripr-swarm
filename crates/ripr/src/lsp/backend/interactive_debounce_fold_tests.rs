//! Fold overlapping interactive refresh triggers into one debounce window (#8528).
//!
//! Counts scheduler requests, not wall-clock: a burst of `didOpen` handlers
//! that all slept 200ms used to issue one request each. Folded triggers
//! return before `refresh_scheduler.request`; the window owner reads the
//! latest workspace revision after waking.

use super::{Backend, INTERACTIVE_REFRESH_DEBOUNCE};
use crate::lsp::refresh_scheduler::{RefreshReason, RefreshScope};
use std::path::PathBuf;
use std::pin::pin;
use std::sync::atomic::{AtomicU64, Ordering};
use tower_lsp_server::LspService;

const OVERLAPPING_INTERACTIVE_TRIGGERS: u64 = 8;

async fn interactive_did_open(backend: &Backend, completed: &AtomicU64) {
    backend.advance_workspace_revision();
    backend
        .refresh_diagnostics(RefreshScope::Interactive, RefreshReason::DidOpen)
        .await;
    completed.fetch_add(1, Ordering::Release);
}

async fn overlapping_opens(backend: &Backend, completed: &AtomicU64) {
    tokio::join!(
        interactive_did_open(backend, completed),
        interactive_did_open(backend, completed),
        interactive_did_open(backend, completed),
        interactive_did_open(backend, completed),
        interactive_did_open(backend, completed),
        interactive_did_open(backend, completed),
        interactive_did_open(backend, completed),
        interactive_did_open(backend, completed),
    );
}

#[test]
fn overlapping_interactive_triggers_issue_one_scheduler_request_at_the_latest_revision()
-> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .map_err(|err| format!("failed to start test runtime: {err}"))?;
    runtime.block_on(async {
        let (service, socket) = LspService::new(|client| Backend::new(client, PathBuf::from(".")));
        drop(socket);
        let backend = service.inner();
        backend.initialize_test_workspace_root();

        let completed = AtomicU64::new(0);
        let mut burst = pin!(overlapping_opens(backend, &completed));
        let mut folded = pin!(async {
            for _ in 0..10_000 {
                if completed.load(Ordering::Acquire) >= OVERLAPPING_INTERACTIVE_TRIGGERS - 1 {
                    return Ok(());
                }
                tokio::task::yield_now().await;
            }
            Err(
                "folded didOpen handlers never returned while the debounce window was open"
                    .to_string(),
            )
        });
        tokio::select! {
            result = &mut folded => result?,
            () = &mut burst => {
                return Err(
                    "all interactive refreshes finished before the debounce window closed"
                        .to_string(),
                );
            }
        }

        let requests_before_wakeup = backend.refresh_telemetry_for_test().did_open_requests;
        if requests_before_wakeup != 0 {
            return Err(format!(
                "folded triggers must not reach the scheduler before the window owner wakes: did_open_requests={requests_before_wakeup}"
            ));
        }
        if backend.last_requested_workspace_revision_for_test() != 0 {
            return Err(
                "folded triggers must not sample a workspace revision before wakeup".to_string(),
            );
        }

        tokio::time::advance(INTERACTIVE_REFRESH_DEBOUNCE).await;

        let mut saw_request = pin!(async {
            for _ in 0..10_000 {
                if backend.refresh_telemetry_for_test().did_open_requests >= 1 {
                    return Ok(());
                }
                tokio::task::yield_now().await;
            }
            Err(format!(
                "window owner never reached the scheduler after wakeup; did_open_requests={}",
                backend.refresh_telemetry_for_test().did_open_requests
            ))
        });
        tokio::select! {
            result = &mut saw_request => result?,
            () = &mut burst => {}
        }

        let telemetry = backend.refresh_telemetry_for_test();
        if telemetry.did_open_requests != 1 {
            return Err(format!(
                "overlapping didOpen handlers must issue one scheduler request, got {}",
                telemetry.did_open_requests
            ));
        }
        let revision = backend.last_requested_workspace_revision_for_test();
        if revision != OVERLAPPING_INTERACTIVE_TRIGGERS {
            return Err(format!(
                "window owner must request the latest folded workspace revision {OVERLAPPING_INTERACTIVE_TRIGGERS}, got {revision}"
            ));
        }
        Ok(())
    })
}
