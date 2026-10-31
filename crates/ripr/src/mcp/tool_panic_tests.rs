//! #11116: panic containment for non-refresh MCP tool handlers. The fault is
//! injected inside the real `ripr_list_gaps` handler. The contained call
//! answers a typed `analysis_failed` tool failure naming the tool and a next
//! step, never echoes the panic payload, and the same server answers the next
//! call. The uncontained control proves the injection fires: the same call
//! through the unwrapped dispatcher unwinds its task.
use super::super::panic_containment::injected_tool_panic;
use super::*;

/// Temp root removed on every exit path, including early `Err` returns.
struct TempRoot(std::path::PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture_root(label: &str) -> Result<TempRoot, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-mcp-tool-panic-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(TempRoot(root))
}

fn server_at(root: &std::path::Path) -> Result<McpServer, String> {
    McpServer::new(
        WorkspaceStatus::resolve_with_root(Some(root.to_path_buf())).0,
        Some(root.to_path_buf()),
    )
    .map_err(|error| error.to_string())
}

#[tokio::test(flavor = "current_thread")]
async fn panicking_tool_handler_becomes_a_typed_failure_and_the_server_keeps_serving()
-> Result<(), String> {
    let guard = fixture_root("contained")?;
    let root = guard.0.clone();
    let server = server_at(&root)?;
    let armed = injected_tool_panic::arm(&root, protocol::LIST_GAPS_TOOL_NAME);
    let contained = server
        .call_tool_contained(
            rmcp::model::CallToolRequestParams::new(protocol::LIST_GAPS_TOOL_NAME),
            super::super::MAX_RESPONSE_BYTES,
        )
        .await
        .map_err(|error| format!("a contained panic must answer a tool failure: {error:?}"))?;
    drop(armed);
    let rmcp::model::CallToolResponse::Complete(result) = contained else {
        return Err("contained panic did not complete".to_string());
    };
    let wire = serde_json::to_value(&result).map_err(|error| error.to_string())?;
    if wire.get("isError") != Some(&serde_json::json!(true))
        || wire.pointer("/structuredContent/failure/code")
            != Some(&serde_json::json!(workspace::CODE_ANALYSIS_FAILED))
    {
        return Err(format!("contained panic is not a typed failure: {wire}"));
    }
    let text = wire.to_string();
    if !text.contains(protocol::LIST_GAPS_TOOL_NAME)
        || !text.contains("retry")
        || text.contains("injected MCP tool handler panic")
    {
        return Err(format!(
            "failure must name the tool and a next step without echoing the payload: {wire}"
        ));
    }
    // Same server, next call: the disarmed tool answers normally (a typed
    // no_snapshot refusal, not a panic failure), and another tool serves.
    let after = server
        .call_tool_contained(
            rmcp::model::CallToolRequestParams::new(protocol::LIST_GAPS_TOOL_NAME),
            super::super::MAX_RESPONSE_BYTES,
        )
        .await
        .map_err(|error| format!("list_gaps after contained panic: {error:?}"))?;
    let rmcp::model::CallToolResponse::Complete(after) = after else {
        return Err("disarmed list_gaps did not complete".to_string());
    };
    let after = serde_json::to_value(&after).map_err(|error| error.to_string())?;
    if after
        .pointer("/structuredContent/failure/code")
        .and_then(serde_json::Value::as_str)
        != Some(workspace::CODE_NO_SNAPSHOT)
    {
        return Err(format!("disarmed handler must answer normally: {after}"));
    }
    server
        .call_tool_contained(
            rmcp::model::CallToolRequestParams::new(protocol::STATUS_TOOL_NAME),
            super::super::MAX_RESPONSE_BYTES,
        )
        .await
        .map_err(|error| format!("status after contained panic: {error:?}"))?;
    Ok(())
}

fn tool_arguments(value: serde_json::Value) -> Option<serde_json::Map<String, serde_json::Value>> {
    value.as_object().cloned()
}

async fn call_value(
    server: &McpServer,
    tool: &'static str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut request = rmcp::model::CallToolRequestParams::new(tool);
    request.arguments = tool_arguments(arguments);
    let response = server
        .call_tool_contained(request, super::super::MAX_RESPONSE_BYTES)
        .await
        .map_err(|error| format!("{tool}: {error:?}"))?;
    let rmcp::model::CallToolResponse::Complete(result) = response else {
        return Err(format!("{tool} did not complete"));
    };
    serde_json::to_value(&result).map_err(|error| error.to_string())
}

fn failure_code(value: &serde_json::Value) -> Option<&str> {
    value
        .pointer("/structuredContent/failure/code")
        .and_then(serde_json::Value::as_str)
}

/// #11116 hazard: `ripr_prepare_repair` evicts transactions of an older
/// snapshot and records their `superseded` tombstones. A panic between those
/// steps is contained and the session outlives it, so the split write must be
/// unwind-safe: after a panic at either point the old attempt still reads as
/// `superseded` (never `attempt_not_found`) and a retry of the tool works.
#[tokio::test(flavor = "current_thread")]
async fn prepare_repair_panic_after_eviction_keeps_old_attempt_superseded() -> Result<(), String> {
    for stage in ["after_tombstones", "after_eviction"] {
        let guard = fixture_root(stage)?;
        let mut server = server_at(&guard.0)?;
        let identity = format!("root:sha256:panic-{stage}-{}", std::process::id());
        server.root_identity = Some(identity.clone());
        *server.session.lock().await =
            repair::tests::session_with(&[gaps::test_finding()?], &identity)?;
        let prepared = call_value(
            &server,
            protocol::PREPARE_REPAIR_TOOL_NAME,
            serde_json::json!({ "canonical_id": "gap:test:1" }),
        )
        .await?;
        let old_id = prepared
            .pointer("/structuredContent/attempt/attempt_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{stage}: first prepare lost its attempt id: {prepared}"))?
            .to_string();

        // A refresh moved the session to a snapshot with different evidence.
        let mut changed = gaps::test_finding()?;
        changed
            .evidence
            .push("a later analysis pass adds one more evidence line".to_string());
        let refreshed = repair::tests::session_with(&[changed], &identity)?;
        server.session.lock().await.last_good = refreshed.last_good;

        let armed = repair::injected_prepare_panic::arm(&identity, stage);
        let panicked = call_value(
            &server,
            protocol::PREPARE_REPAIR_TOOL_NAME,
            serde_json::json!({ "canonical_id": "gap:test:1" }),
        )
        .await?;
        if failure_code(&panicked) != Some(workspace::CODE_ANALYSIS_FAILED)
            || !panicked
                .to_string()
                .contains(protocol::PREPARE_REPAIR_TOOL_NAME)
        {
            return Err(format!(
                "{stage}: panic not contained as typed failure: {panicked}"
            ));
        }
        // (b) the old attempt is still superseded, not attempt_not_found.
        let old = call_value(
            &server,
            protocol::GET_REPAIR_ATTEMPT_TOOL_NAME,
            serde_json::json!({ "attempt_id": old_id }),
        )
        .await?;
        if failure_code(&old) != Some("superseded") {
            return Err(format!(
                "{stage}: old attempt must stay superseded after the panic: {old}"
            ));
        }
        // (a) the retry works once the fault is gone, and the old attempt is
        // still superseded afterwards.
        drop(armed);
        let retry = call_value(
            &server,
            protocol::PREPARE_REPAIR_TOOL_NAME,
            serde_json::json!({ "canonical_id": "gap:test:1" }),
        )
        .await?;
        let new_id = retry
            .pointer("/structuredContent/attempt/attempt_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{stage}: retry did not prepare: {retry}"))?;
        if new_id == old_id {
            return Err(format!("{stage}: retry reused the superseded attempt id"));
        }
        let old_after = call_value(
            &server,
            protocol::GET_REPAIR_ATTEMPT_TOOL_NAME,
            serde_json::json!({ "attempt_id": old_id }),
        )
        .await?;
        if failure_code(&old_after) != Some("superseded") {
            return Err(format!(
                "{stage}: old attempt lost after retry: {old_after}"
            ));
        }
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn prepare_repair_fault_hook_fires_without_containment() -> Result<(), String> {
    // Control: the same armed stage unwinds the uncontained dispatcher, so the
    // hook provably fires inside `prepare_repair` (the test above would
    // otherwise pass vacuously).
    let guard = fixture_root("prepare-control")?;
    let mut server = server_at(&guard.0)?;
    let identity = format!("root:sha256:panic-control-{}", std::process::id());
    server.root_identity = Some(identity.clone());
    *server.session.lock().await =
        repair::tests::session_with(&[gaps::test_finding()?], &identity)?;
    let _armed = repair::injected_prepare_panic::arm(&identity, "after_eviction");
    let server = std::sync::Arc::new(server);
    let uncontained = tokio::spawn(async move {
        let mut request =
            rmcp::model::CallToolRequestParams::new(protocol::PREPARE_REPAIR_TOOL_NAME);
        request.arguments = tool_arguments(serde_json::json!({ "canonical_id": "gap:test:1" }));
        server
            .dispatch_tool(request, super::super::MAX_RESPONSE_BYTES)
            .await
            .is_ok()
    })
    .await;
    match uncontained {
        Err(error) if error.is_panic() => Ok(()),
        other => Err(format!(
            "armed prepare_repair must unwind uncontained: {other:?}"
        )),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn panicking_tool_handler_without_containment_unwinds() -> Result<(), String> {
    let guard = fixture_root("control")?;
    let root = guard.0.clone();
    let server = std::sync::Arc::new(server_at(&root)?);
    let _armed = injected_tool_panic::arm(&root, protocol::LIST_GAPS_TOOL_NAME);
    let uncontained = tokio::spawn(async move {
        server
            .dispatch_tool(
                rmcp::model::CallToolRequestParams::new(protocol::LIST_GAPS_TOOL_NAME),
                super::super::MAX_RESPONSE_BYTES,
            )
            .await
            .is_ok()
    })
    .await;
    drop(guard);
    match uncontained {
        Err(error) if error.is_panic() => Ok(()),
        other => Err(format!(
            "the injected panic must unwind the uncontained handler, got {other:?}"
        )),
    }
}

#[test]
fn every_tool_is_panic_contained() -> Result<(), String> {
    let (status, root) = WorkspaceStatus::resolve_with_root(None);
    let server = McpServer::new(status, root).map_err(|error| error.to_string())?;
    if server.tools.tools.is_empty() {
        return Err("tool catalog is empty".to_string());
    }
    for tool in &server.tools.tools {
        let name = tool.name.as_ref();
        let contained = contained_tool_schema_version(name).is_some();
        if !contained {
            return Err(format!("{name}: glue panics are not contained"));
        }
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn refresh_glue_panic_is_typed_and_clears_in_flight() -> Result<(), String> {
    let guard = fixture_root("refresh")?;
    let server = server_at(&guard.0)?;
    let armed = injected_tool_panic::arm(&guard.0, protocol::REFRESH_TOOL_NAME);
    let panicked = call_value(&server, protocol::REFRESH_TOOL_NAME, serde_json::json!({})).await?;
    if failure_code(&panicked) != Some(workspace::CODE_ANALYSIS_FAILED)
        || panicked
            .to_string()
            .contains("injected MCP tool handler panic")
    {
        return Err(format!("refresh panic not contained: {panicked}"));
    }
    if server.session.lock().await.in_flight {
        return Err("a contained refresh panic left in_flight set".to_string());
    }
    drop(armed);
    let after = call_value(&server, protocol::REFRESH_TOOL_NAME, serde_json::json!({})).await?;
    if failure_code(&after) == Some(workspace::CODE_ANALYSIS_IN_FLIGHT) {
        return Err(format!("later refresh refused as in flight: {after}"));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn refresh_glue_panic_without_containment_unwinds() -> Result<(), String> {
    let guard = fixture_root("refresh-control")?;
    let server = std::sync::Arc::new(server_at(&guard.0)?);
    let _armed = injected_tool_panic::arm(&guard.0, protocol::REFRESH_TOOL_NAME);
    let joined = tokio::spawn(async move {
        server
            .dispatch_tool(
                rmcp::model::CallToolRequestParams::new(protocol::REFRESH_TOOL_NAME),
                super::super::MAX_RESPONSE_BYTES,
            )
            .await
            .is_ok()
    })
    .await;
    match joined {
        Err(error) if error.is_panic() => Ok(()),
        other => Err(format!("armed refresh must unwind uncontained: {other:?}")),
    }
}

/// Drives the real stdio `serve` wiring (`InitializeSessionService`) so that
/// removing `contain_request` fails this test. `tools/list` and
/// `resources/read` are armed in the real handlers; the control session is
/// identical but unarmed and must answer both normally, proving the fault
/// (not the request shape) produces the typed error. Later requests on the
/// same connection keep being served.
#[tokio::test(flavor = "multi_thread")]
async fn real_session_contains_tools_list_and_resources_read_glue_panics() -> Result<(), String> {
    let guard = fixture_root("session")?;
    let root = guard.0.clone();
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"control","version":"1"}}}"#;
    let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let read = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{{"uri":"{}"}}}}"#,
        protocol::STATUS_RESOURCE_URI
    );
    let ping = r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#;
    let list_again = r#"{"jsonrpc":"2.0","id":5,"method":"tools/list"}"#;
    let input = format!("{initialize}\n{initialized}\n{list}\n{read}\n{ping}\n{list_again}\n");
    let run = |armed: bool| {
        let root = root.clone();
        let input = input.clone();
        async move {
            let _armed = armed.then(|| {
                (
                    injected_tool_panic::arm(&root, "tools/list"),
                    injected_tool_panic::arm(&root, "resources/read"),
                )
            });
            let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
            let sink = SharedSink(output.clone());
            super::super::transport::serve(
                std::io::Cursor::new(input.into_bytes()),
                sink,
                WorkspaceStatus::resolve_with_root(Some(root.clone())).0,
                Some(root.clone()),
            )
            .await?;
            let bytes = output.lock().map_err(|e| e.to_string())?.clone();
            bytes
                .split(|byte| *byte == b'\n')
                .filter(|frame| !frame.is_empty())
                .map(serde_json::from_slice::<serde_json::Value>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())
        }
    };
    let control = run(false).await?;
    for id in [2, 3, 5] {
        let frame = control
            .iter()
            .find(|f| f.get("id") == Some(&serde_json::json!(id)))
            .ok_or_else(|| format!("control lost id {id}: {control:?}"))?;
        if frame.get("error").is_some() {
            return Err(format!("unarmed control must succeed: {frame}"));
        }
    }
    let armed = run(true).await?;
    for id in [2, 3] {
        let frame = armed
            .iter()
            .find(|f| f.get("id") == Some(&serde_json::json!(id)))
            .ok_or_else(|| format!("armed session lost id {id}: {armed:?}"))?;
        if frame.pointer("/error/code") != Some(&serde_json::json!(-32603))
            || frame.pointer("/error/data/code")
                != Some(&serde_json::json!(workspace::CODE_ANALYSIS_FAILED))
            || frame
                .to_string()
                .contains("injected MCP tool handler panic")
        {
            return Err(format!("id {id}: not a typed analysis_failed: {frame}"));
        }
    }
    // The session kept serving after both panics: ping answers, and the next armed
    // tools/list is again a typed error rather than a dead connection.
    let ping_ok = armed
        .iter()
        .any(|f| f.get("id") == Some(&serde_json::json!(4)) && f.get("error").is_none());
    let again = armed
        .iter()
        .any(|f| f.get("id") == Some(&serde_json::json!(5)) && f.get("error").is_some());
    if !ping_ok || !again {
        return Err(format!("session stopped serving after a panic: {armed:?}"));
    }
    Ok(())
}

/// In-memory `AsyncWrite` that keeps the wire bytes readable afterwards.
struct SharedSink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl tokio::io::AsyncWrite for SharedSink {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.0.lock() {
            Ok(mut out) => {
                out.extend_from_slice(buf);
                std::task::Poll::Ready(Ok(buf.len()))
            }
            Err(_) => std::task::Poll::Ready(Err(std::io::Error::other("sink unavailable"))),
        }
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
