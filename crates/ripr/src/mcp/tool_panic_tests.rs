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
fn every_tool_except_refresh_is_panic_contained() -> Result<(), String> {
    let (status, root) = WorkspaceStatus::resolve_with_root(None);
    let server = McpServer::new(status, root).map_err(|error| error.to_string())?;
    if server.tools.tools.is_empty() {
        return Err("tool catalog is empty".to_string());
    }
    for tool in &server.tools.tools {
        let name = tool.name.as_ref();
        let contained = contained_tool_schema_version(name).is_some();
        if contained == (name == protocol::REFRESH_TOOL_NAME) {
            return Err(format!(
                "{name}: contained={contained}; only ripr_refresh is excluded (adapter panics are contained at the analysis boundary)"
            ));
        }
    }
    Ok(())
}
