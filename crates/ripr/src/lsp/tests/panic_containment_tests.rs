//! A panicking request handler must become a typed JSON-RPC error instead of
//! ending the LSP session. The injected panic unwinds out of the real
//! `textDocument/hover` handler and is served through the same
//! `ContainPanics` layer `serve_streams` composes.

use super::*;
use crate::lsp::panic_containment::ContainPanics;
use std::task::{Context, Poll};
use tower_lsp_server::jsonrpc::{Request, Response};

const HOVER_BUDGET: Duration = Duration::from_secs(30);

fn hover_request(id: u64, uri: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "textDocument/hover",
        "params": {
            "textDocument": {"uri": uri},
            "position": {"line": 0, "character": 0},
        }
    })
}

struct Wire {
    reader: tokio::io::ReadHalf<tokio::io::DuplexStream>,
    writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
}

impl Wire {
    async fn initialize(&mut self, root_uri: &str) -> Result<(), String> {
        write_lsp_message(
            &mut self.writer,
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"processId": null, "rootUri": root_uri, "capabilities": {}}
            }),
        )
        .await?;
        let response = read_lsp_response(&mut self.reader, 1).await?;
        if response.get("error").is_some() {
            return Err(format!("initialize failed: {response}"));
        }
        write_lsp_message(
            &mut self.writer,
            serde_json::json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
        )
        .await
    }

    async fn hover(&mut self, id: u64, uri: &str) -> Result<serde_json::Value, String> {
        write_lsp_message(&mut self.writer, hover_request(id, uri)).await?;
        tokio::time::timeout(HOVER_BUDGET, read_lsp_response(&mut self.reader, id))
            .await
            .map_err(|elapsed| format!("hover {id} was never answered: {elapsed}"))?
    }

    /// Like `hover`, but also returns every message read before the answer so a
    /// test can assert on stray traffic that `read_lsp_response` would discard.
    async fn hover_with_skipped(
        &mut self,
        id: u64,
        uri: &str,
    ) -> Result<(serde_json::Value, Vec<serde_json::Value>), String> {
        write_lsp_message(&mut self.writer, hover_request(id, uri)).await?;
        tokio::time::timeout(
            HOVER_BUDGET,
            read_lsp_response_with_notifications(&mut self.reader, id),
        )
        .await
        .map_err(|elapsed| format!("hover {id} was never answered: {elapsed}"))?
    }
}

#[derive(Clone, Copy)]
enum Fault {
    Hover,
    DidSave,
}

fn start(root: &Path, contain: bool, fault: Fault) -> (Wire, tokio::task::JoinHandle<()>) {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(client_io);
    let (server_read, server_write) = tokio::io::split(server_io);
    let backend_root = root.to_path_buf();
    let (service, socket) =
        LspService::new(move |client| Backend::new(client, backend_root.clone()));
    // The next call of the chosen handler unwinds.
    match fault {
        Fault::Hover => service.inner().panic_in_next_hover_for_test(),
        Fault::DidSave => service.inner().panic_in_next_did_save_for_test(),
    }
    let server = Server::new(server_read, server_write, socket);
    let task = if contain {
        tokio::spawn(async move { server.serve(ContainPanics(service)).await })
    } else {
        tokio::spawn(async move { server.serve(service).await })
    };
    (Wire { reader, writer }, task)
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to start test runtime: {err}"))
}

#[test]
fn panicking_hover_handler_becomes_a_typed_error_and_the_session_keeps_serving()
-> Result<(), String> {
    runtime()?.block_on(async {
        let root = unique_lsp_test_root("panic-containment-contained")?;
        let root_uri = file_uri_for_path(root.path())?;
        let uri = root_uri.to_string();
        let (mut wire, task) = start(root.path(), true, Fault::Hover);
        wire.initialize(&uri).await?;

        let failed = wire.hover(10, &uri).await?;
        let error = failed
            .get("error")
            .ok_or_else(|| format!("panicking hover must answer with an error: {failed}"))?;
        // InternalError, with the method and the next step named.
        if error.get("code").and_then(serde_json::Value::as_i64) != Some(-32603) {
            return Err(format!("expected JSON-RPC InternalError: {error}"));
        }
        let message = error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if !message.contains("textDocument/hover") || !message.contains("Retry the request") {
            return Err(format!(
                "error must name the method and a next step: {message}"
            ));
        }
        if message.contains("injected hover handler panic") {
            return Err(format!(
                "panic payload must not reach the client: {message}"
            ));
        }

        // The session is still alive: the same handler now answers normally.
        let recovered = wire.hover(11, &uri).await?;
        if recovered.get("error").is_some() || recovered.get("result").is_none() {
            return Err(format!(
                "hover after a contained panic must succeed: {recovered}"
            ));
        }
        if task.is_finished() {
            return Err("server task ended after a contained panic".to_string());
        }
        Ok(())
    })
}

fn did_save(uri: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didSave",
        "params": {"textDocument": {"uri": uri}, "text": "fn main() {}\n"}
    })
}

/// A panicking notification has no response to carry an error: it is dropped,
/// nothing is written for it, and the session keeps serving.
#[test]
fn panicking_notification_is_dropped_and_the_session_keeps_serving() -> Result<(), String> {
    runtime()?.block_on(async {
        let root = unique_lsp_test_root("panic-containment-notification")?;
        let uri = file_uri_for_path(root.path())?.to_string();
        let (mut wire, task) = start(root.path(), true, Fault::DidSave);
        wire.initialize(&uri).await?;

        write_lsp_message(&mut wire.writer, did_save(&uri)).await?;
        // The hover is answered with its own success response, not an error
        // for the dropped notification.
        let (recovered, skipped) = wire.hover_with_skipped(11, &uri).await?;
        // Server-initiated requests and notifications carry a `method`; a
        // message without one is a response, which nothing was owed.
        if let Some(stray) = skipped.iter().find(|m| m.get("method").is_none()) {
            return Err(format!(
                "a response was written for the dropped notification: {stray}"
            ));
        }
        if recovered.get("error").is_some() || recovered.get("result").is_none() {
            return Err(format!(
                "request after a dropped notification must succeed: {recovered}"
            ));
        }
        if task.is_finished() {
            return Err("server task ended after a contained notification panic".to_string());
        }
        Ok(())
    })
}

/// Control for the notification path: the same didSave fault without the layer
/// unwinds the serve task, proving the injection fires on this path.
#[test]
fn panicking_notification_without_containment_ends_the_session() -> Result<(), String> {
    runtime()?.block_on(async {
        let root = unique_lsp_test_root("panic-containment-notification-control")?;
        let uri = file_uri_for_path(root.path())?.to_string();
        let (mut wire, task) = start(root.path(), false, Fault::DidSave);
        wire.initialize(&uri).await?;
        write_lsp_message(&mut wire.writer, did_save(&uri)).await?;
        let joined = tokio::time::timeout(HOVER_BUDGET, task)
            .await
            .map_err(|elapsed| format!("uncontained server task never ended: {elapsed}"))?;
        match joined {
            Err(error) if error.is_panic() => Ok(()),
            other => Err(format!("expected the serve task to unwind: {other:?}")),
        }
    })
}

/// Inner service whose `call` panics synchronously, before any future exists.
struct PanicsWhileConstructing;

impl tower::Service<Request> for PanicsWhileConstructing {
    type Response = Option<Response>;
    type Error = std::convert::Infallible;
    type Future = std::future::Ready<Result<Option<Response>, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _request: Request) -> Self::Future {
        std::panic::resume_unwind(Box::new("injected call-construction panic"))
    }
}

#[test]
fn panic_while_constructing_the_handler_future_is_contained() -> Result<(), String> {
    use tower::Service;
    runtime()?.block_on(async {
        let mut layer = ContainPanics(PanicsWhileConstructing);

        let request = Request::build("textDocument/hover").id(7).finish();
        let response = layer
            .call(request)
            .await
            .unwrap_or_else(|never| match never {})
            .ok_or("a request must be answered with an error response")?;
        let value = serde_json::to_value(&response).map_err(|err| err.to_string())?;
        let error = value
            .get("error")
            .ok_or_else(|| format!("expected an error response: {value}"))?;
        let message = error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if error.get("code").and_then(serde_json::Value::as_i64) != Some(-32603)
            || value.get("id") != Some(&serde_json::json!(7))
            || !message.contains("textDocument/hover")
            || message.contains("injected call-construction panic")
        {
            return Err(format!("unexpected contained response: {value}"));
        }

        let notification = Request::build("textDocument/didSave").finish();
        let dropped = layer
            .call(notification)
            .await
            .unwrap_or_else(|never| match never {});
        if dropped.is_some() {
            return Err(format!("a notification must get no response: {dropped:?}"));
        }
        Ok(())
    })
}

/// Control: the same injection without the layer ends the serve loop, so the
/// test above fails for the right reason if the layer is removed.
#[test]
fn panicking_hover_handler_without_containment_ends_the_session() -> Result<(), String> {
    runtime()?.block_on(async {
        let root = unique_lsp_test_root("panic-containment-control")?;
        let uri = file_uri_for_path(root.path())?.to_string();
        let (mut wire, task) = start(root.path(), false, Fault::Hover);
        wire.initialize(&uri).await?;
        let outcome = wire.hover(10, &uri).await;
        if outcome.is_ok() {
            return Err(format!(
                "uncontained panic must not be answered: {outcome:?}"
            ));
        }
        let joined = tokio::time::timeout(HOVER_BUDGET, task)
            .await
            .map_err(|elapsed| format!("uncontained server task never ended: {elapsed}"))?;
        match joined {
            Err(error) if error.is_panic() => Ok(()),
            other => Err(format!("expected the serve task to unwind: {other:?}")),
        }
    })
}
