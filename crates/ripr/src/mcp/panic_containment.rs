//! Keeps one panicking MCP tool handler from ending the stdio session
//! (crash-containment, #11116; the MCP sibling of the LSP `ContainPanics`
//! layer from #11134).
//!
//! The SDK polls every `tools/call` handler future inside the session loop,
//! so an unwinding handler would end the session and, through the startup
//! panic boundary, the process: the agent client would lose its workspace
//! snapshot because of one bad call. [`ContainPanic`] converts a panic raised
//! while polling the handler future into a value the caller turns into a typed
//! tool failure that names the tool and the next step, and the session keeps
//! serving.
//!
//! `ripr_refresh` is wrapped for glue panics too (worker and language-adapter
//! panics are already typed `analysis_failed` failures at the analysis
//! boundary, #11132). `tools/list` and `resources/read` are contained by the
//! request-level wrapper in `server.rs` (`contain_request`). The `initialize`
//! handshake is not claimed: the SDK runs it before the service loop.
//!
//! Scope: unwinding panics only. A stack overflow or `abort` ends the process
//! and cannot be caught here. The panic payload is not echoed to the client:
//! it can quote repository text, and the process panic hook already reports
//! it on stderr.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

/// The handler future panicked while being polled.
pub(super) struct Panicked;

/// Future adapter: resolves to `Err(Panicked)` instead of unwinding.
pub(super) struct ContainPanic<F>(Pin<Box<F>>);

impl<F: Future> ContainPanic<F> {
    pub(super) fn new(inner: F) -> Self {
        Self(Box::pin(inner))
    }
}

/// `AssertUnwindSafe` invariant: after a panic this adapter resolves to
/// `Err(Panicked)` and the handler future is never polled again; the caller's
/// `.await` consumes the adapter and answers the typed failure. Tokio mutexes
/// do not poison and release their guard on unwind, so the shared session
/// stays usable; state the handler wrote before the panic persists, and a
/// later call proceeds on it, so a handler that mutates session state across a
/// possible panic point must order its writes so every intermediate state is
/// valid. The mutating tool handlers are `ripr_prepare_repair` and `ripr_refresh`;
/// `ripr_refresh` sets `in_flight` behind an `InFlightAttempt` guard that
/// clears it on unwind. `ripr_prepare_repair` records
/// the `superseded` tombstones before evicting stale transactions and inserts
/// the new transaction last, with a fault-injection control at each step.
impl<F: Future> Future for ContainPanic<F> {
    type Output = Result<F::Output, Panicked>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match catch_unwind(AssertUnwindSafe(|| self.0.as_mut().poll(cx))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(_payload) => Poll::Ready(Err(Panicked)),
        }
    }
}

/// Test-only fault injection keyed by workspace root and tool name, so
/// concurrent tests never see each other's arming (the #11132 precedent).
#[cfg(test)]
pub(super) mod injected_tool_panic {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    static ARMED: Mutex<BTreeSet<(PathBuf, String)>> = Mutex::new(BTreeSet::new());

    /// Disarms on drop.
    pub(in crate::mcp) struct Armed(PathBuf, String);

    impl Drop for Armed {
        fn drop(&mut self) {
            if let Ok(mut armed) = ARMED.lock() {
                armed.remove(&(self.0.clone(), self.1.clone()));
            }
        }
    }

    pub(in crate::mcp) fn arm(root: &Path, tool: &str) -> Armed {
        if let Ok(mut armed) = ARMED.lock() {
            armed.insert((root.to_path_buf(), tool.to_string()));
        }
        Armed(root.to_path_buf(), tool.to_string())
    }

    pub(in crate::mcp) fn fire(root: Option<&Path>, tool: &str) {
        let due = root.is_some_and(|root| {
            ARMED
                .lock()
                .map(|armed| armed.contains(&(root.to_path_buf(), tool.to_string())))
                .unwrap_or(false)
        });
        if due {
            std::panic::resume_unwind(Box::new("injected MCP tool handler panic (#11116)"));
        }
    }
}
