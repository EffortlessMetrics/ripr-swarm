//! Keeps one panicking request handler from taking down the LSP sidecar
//! (crash-containment, #10724 broader ask).
//!
//! tower-lsp-server polls every handler future inside the transport's serve
//! loop, so an unwinding handler would unwind `Server::serve` itself, end the
//! session and (through the startup panic boundary) exit the process: every
//! open document would lose diagnostics because of one bad request. This
//! layer sits around the service, converts a panic raised while building or
//! polling the handler future into a typed JSON-RPC `InternalError` response
//! that names the method and the next step, and keeps serving. A panicking
//! notification has no response to carry the error, so it is dropped and the
//! session continues.
//!
//! Scope: unwinding panics only. A stack overflow or `abort` ends the process
//! and cannot be caught here; those stay behind the per-language parse
//! budgets and worker-thread stack sizes.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

use tower::Service;
use tower_lsp_server::jsonrpc::{Error, ErrorCode, Id, Request, Response};

pub(super) struct ContainPanics<S>(pub(super) S);

/// Stable, user-facing text for a contained handler panic. The panic payload
/// itself is already reported on stderr by the process panic hook and can
/// quote repository text, so it is not echoed into the protocol response.
fn contained_error(method: &str) -> Error {
    Error {
        code: ErrorCode::InternalError,
        message: format!(
            "ripr hit an internal error while handling `{method}` and contained it; the server is still running. Retry the request; if it repeats, save the file again or restart the ripr server and report the stderr panic message as a bug."
        )
        .into(),
        data: None,
    }
}

/// Response for a contained panic: an error for a request, nothing for a
/// notification.
fn contained_response(id: Option<Id>, method: &str) -> Option<Response> {
    id.map(|id| Response::from_error(id, contained_error(method)))
}

impl<S> Service<Request> for ContainPanics<S>
where
    S: Service<Request, Response = Option<Response>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Option<Response>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Option<Response>, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let method = request.method().to_owned();
        let id = request.id().cloned();
        let inner = match catch_unwind(AssertUnwindSafe(|| self.0.call(request))) {
            Ok(future) => future,
            Err(_payload) => {
                let response = contained_response(id, &method);
                return Box::pin(std::future::ready(Ok(response)));
            }
        };
        Box::pin(ContainedFuture {
            inner: Box::pin(inner),
            method,
            id,
        })
    }
}

struct ContainedFuture<F> {
    inner: Pin<Box<F>>,
    method: String,
    id: Option<Id>,
}

impl<F, E> Future for ContainedFuture<F>
where
    F: Future<Output = Result<Option<Response>, E>>,
{
    type Output = Result<Option<Response>, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        match catch_unwind(AssertUnwindSafe(|| this.inner.as_mut().poll(cx))) {
            Ok(poll) => poll,
            Err(_payload) => Poll::Ready(Ok(contained_response(this.id.take(), &this.method))),
        }
    }
}
