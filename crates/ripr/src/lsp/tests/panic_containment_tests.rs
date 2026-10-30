//! A panicking request handler must become a typed JSON-RPC error instead of
//! ending the LSP session. The injected panic unwinds out of the real
//! `textDocument/hover` handler and is served through the same
//! `ContainPanics` layer `serve_streams` composes.

use super::*;
use crate::lsp::panic_containment::ContainPanics;

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
}

fn start(root: &Path, contain: bool) -> (Wire, tokio::task::JoinHandle<()>) {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(client_io);
    let (server_read, server_write) = tokio::io::split(server_io);
    let backend_root = root.to_path_buf();
    let (service, socket) =
        LspService::new(move |client| Backend::new(client, backend_root.clone()));
    // The next hover handler call unwinds.
    service.inner().panic_in_next_hover_for_test();
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
        let (mut wire, task) = start(root.path(), true);
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

/// Control: the same injection without the layer ends the serve loop, so the
/// test above fails for the right reason if the layer is removed.
#[test]
fn panicking_hover_handler_without_containment_ends_the_session() -> Result<(), String> {
    runtime()?.block_on(async {
        let root = unique_lsp_test_root("panic-containment-control")?;
        let uri = file_uri_for_path(root.path())?.to_string();
        let (mut wire, task) = start(root.path(), false);
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
