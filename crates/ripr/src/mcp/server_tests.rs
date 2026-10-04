use super::*;

#[test]
fn sdk_server_metadata_preserves_bounded_status_instructions() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let config = server.get_info();
    let instructions = config
        .instructions
        .ok_or_else(|| "SDK metadata omitted instructions".to_string())?;
    if config.server_info.name != "ripr"
        || !instructions.contains("ripr check --format json")
        || !instructions.contains("never edits source")
        || !instructions.contains("ripr_list_gaps")
    {
        return Err("SDK metadata changed application identity or analysis authority".into());
    }
    let tools = &server.tools;
    if tools.tools.len() != 8 {
        return Err(format!(
            "SDK descriptor must expose the eight-tool slice surface, got {} tools",
            tools.tools.len()
        ));
    }
    let tool = tools
        .tools
        .iter()
        .find(|tool| tool.name == protocol::STATUS_TOOL_NAME)
        .ok_or_else(|| "status tool missing".to_string())?;
    let document = serde_json::to_value(tool).map_err(|error| error.to_string())?;
    if document
        .pointer("/annotations/readOnlyHint")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err("SDK status tool lost read-only annotation".into());
    }
    Ok(())
}

#[test]
fn sdk_server_declares_snapshot_and_gap_resource_templates() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let templates =
        serde_json::to_value(&server.resource_templates).map_err(|error| error.to_string())?;
    let templates = templates
        .pointer("/resourceTemplates")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "SDK resource templates missing".to_string())?;
    let uris = templates
        .iter()
        .filter_map(|template| {
            template
                .pointer("/uriTemplate")
                .and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>();
    for expected in [
        protocol::SNAPSHOT_RESOURCE_TEMPLATE,
        protocol::GAP_RESOURCE_TEMPLATE,
        repair::REPAIR_ATTEMPT_TEMPLATE,
        repair::RECEIPT_TEMPLATE,
        repair_card::REPAIR_CARD_TEMPLATE,
    ] {
        if !uris.contains(&expected) {
            return Err(format!("SDK resource templates lost {expected}: {uris:?}"));
        }
    }
    Ok(())
}

#[tokio::test]
async fn list_gaps_before_any_refresh_is_a_typed_no_snapshot_failure() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let response = server
        .list_gaps_tool(None)
        .await
        .map_err(|error| error.to_string())?;
    let result = match response {
        rmcp::model::CallToolResponse::Complete(result) => result,
        other => {
            return Err(format!(
                "pre-refresh list_gaps must complete with a typed failure, got {other:?}"
            ));
        }
    };
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    if value
        .pointer("/isError")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(format!(
            "pre-refresh list_gaps must be a typed failure: {value}"
        ));
    }
    if value
        .pointer("/structuredContent/failure/code")
        .and_then(serde_json::Value::as_str)
        != Some(workspace::CODE_NO_SNAPSHOT)
    {
        return Err(format!("pre-refresh list_gaps lost no_snapshot: {value}"));
    }
    Ok(())
}

#[tokio::test]
async fn get_gap_rejects_bad_arguments_at_the_dispatch_edge() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    for arguments in [
        serde_json::json!({}),
        serde_json::json!({"canonical_id": ""}),
        serde_json::json!({"canonical_id": 7}),
        serde_json::json!({"canonical_id": "gap:x", "verbose": true}),
    ] {
        let arguments = serde_json::from_value(arguments).map_err(|error| error.to_string())?;
        match server.get_gap_tool(arguments).await {
            Err(error) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS => {}
            Ok(response) => {
                return Err(format!(
                    "get_gap must reject malformed arguments with invalid params: {response:?}"
                ));
            }
            Err(error) => {
                return Err(format!(
                    "get_gap must reject malformed arguments with invalid params, got {error:?}"
                ));
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn get_gap_accepts_canonical_id_at_the_dispatch_edge() -> Result<(), String> {
    // #5209: the tool input spells the identity the outputs emit
    // (`canonical_id`), not a second `canonical_id` name. A pre-refresh server
    // answers past-dispatch calls with typed `no_snapshot`, which proves
    // the dispatch edge accepted the spelling.
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let arguments = serde_json::from_value(serde_json::json!({"canonical_id": "gap:x"}))
        .map_err(|error| error.to_string())?;
    let response = server
        .get_gap_tool(arguments)
        .await
        .map_err(|error| format!("dispatch must accept canonical_id: {error}"))?;
    let result = match response {
        rmcp::model::CallToolResponse::Complete(result) => result,
        other => {
            return Err(format!(
                "canonical_id call must reach the typed failure, got {other:?}"
            ));
        }
    };
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    if value
        .pointer("/structuredContent/failure/code")
        .and_then(serde_json::Value::as_str)
        != Some(workspace::CODE_NO_SNAPSHOT)
    {
        return Err(format!("canonical_id call lost no_snapshot: {value}"));
    }
    Ok(())
}

#[test]
fn gap_resource_templates_spell_the_identity_canonical_id() -> Result<(), String> {
    // #5209: one identity, one name — the resource templates use the same
    // `canonical_id` spelling the evidence documents emit.
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let templates =
        serde_json::to_value(&server.resource_templates).map_err(|error| error.to_string())?;
    let templates = templates
        .pointer("/resourceTemplates")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "SDK resource templates missing".to_string())?;
    let uris = templates
        .iter()
        .filter_map(|template| {
            template
                .pointer("/uriTemplate")
                .and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>();
    for expected in [
        "ripr://gap/{canonical_id}",
        "ripr://repair-card/{canonical_id}",
    ] {
        if !uris.contains(&expected) {
            return Err(format!("resource templates lost {expected}: {uris:?}"));
        }
    }
    for uri in &uris {
        if uri.contains("canonical_item_id") {
            return Err(format!(
                "retired template variable still advertised: {uris:?}"
            ));
        }
    }
    Ok(())
}

#[tokio::test]
async fn prepare_repair_fails_closed_before_refresh() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let arguments = serde_json::from_value(serde_json::json!({"canonical_id": "gap:any"}))
        .map_err(|error| error.to_string())?;
    let response = server
        .prepare_repair_tool(arguments)
        .await
        .map_err(|error| error.to_string())?;
    let result = match response {
        rmcp::model::CallToolResponse::Complete(result) => result,
        other => {
            return Err(format!(
                "pre-refresh prepare_repair must complete with a typed failure, got {other:?}"
            ));
        }
    };
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    if value
        .pointer("/isError")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(format!(
            "pre-refresh prepare_repair must be a typed failure: {value}"
        ));
    }
    if value
        .pointer("/structuredContent/failure/code")
        .and_then(serde_json::Value::as_str)
        != Some(workspace::CODE_NO_SNAPSHOT)
    {
        return Err(format!(
            "pre-refresh prepare_repair lost no_snapshot: {value}"
        ));
    }
    Ok(())
}

#[tokio::test]
async fn repair_attempt_lookup_fails_closed_before_refresh() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let arguments =
        serde_json::from_value(serde_json::json!({"attempt_id": "repair-attempt:absent"}))
            .map_err(|error| error.to_string())?;
    let response = server
        .get_repair_attempt_tool(arguments)
        .await
        .map_err(|error| error.to_string())?;
    let result = match response {
        rmcp::model::CallToolResponse::Complete(result) => result,
        other => {
            return Err(format!(
                "pre-refresh get_repair_attempt must complete with a typed failure, got {other:?}"
            ));
        }
    };
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    if value
        .pointer("/isError")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(format!(
            "pre-refresh get_repair_attempt must be a typed failure: {value}"
        ));
    }
    if value
        .pointer("/structuredContent/failure/code")
        .and_then(serde_json::Value::as_str)
        != Some("attempt_not_found")
    {
        return Err(format!(
            "pre-refresh get_repair_attempt lost attempt_not_found: {value}"
        ));
    }
    Ok(())
}

#[tokio::test]
async fn receipt_status_fails_closed_before_refresh() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let arguments = serde_json::from_value(serde_json::json!({"receipt_id": "receipt:absent"}))
        .map_err(|error| error.to_string())?;
    let response = server
        .get_receipt_status_tool(arguments)
        .await
        .map_err(|error| error.to_string())?;
    let result = match response {
        rmcp::model::CallToolResponse::Complete(result) => result,
        other => {
            return Err(format!(
                "pre-refresh get_receipt_status must complete with a typed failure, got {other:?}"
            ));
        }
    };
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    if value
        .pointer("/isError")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(format!(
            "pre-refresh get_receipt_status must be a typed failure: {value}"
        ));
    }
    if value
        .pointer("/structuredContent/failure/code")
        .and_then(serde_json::Value::as_str)
        != Some("attempt_not_found")
    {
        return Err(format!(
            "pre-refresh get_receipt_status lost attempt_not_found: {value}"
        ));
    }
    Ok(())
}

#[tokio::test]
async fn repair_card_fails_closed_before_refresh() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let arguments = serde_json::from_value(serde_json::json!({"canonical_id": "gap:test:1"}))
        .map_err(|error| error.to_string())?;
    let response = server
        .get_repair_card_tool(arguments)
        .await
        .map_err(|error| error.to_string())?;
    let result = match response {
        rmcp::model::CallToolResponse::Complete(result) => result,
        other => {
            return Err(format!(
                "pre-refresh get_repair_card must complete with a typed failure, got {other:?}"
            ));
        }
    };
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    if value
        .pointer("/isError")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(format!(
            "pre-refresh get_repair_card must be a typed failure: {value}"
        ));
    }
    if value
        .pointer("/structuredContent/failure/code")
        .and_then(serde_json::Value::as_str)
        != Some(workspace::CODE_NO_SNAPSHOT)
    {
        return Err(format!(
            "pre-refresh get_repair_card lost no_snapshot: {value}"
        ));
    }
    Ok(())
}

#[tokio::test]
async fn repair_tools_reject_bad_arguments_at_the_dispatch_edge() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    for (tool, arguments, call) in [
        (
            "prepare",
            serde_json::json!({}),
            "prepare_repair rejects a missing canonical_id",
        ),
        (
            "prepare",
            serde_json::json!({"canonical_id": ""}),
            "prepare_repair rejects an empty canonical_id",
        ),
        (
            "prepare",
            serde_json::json!({"canonical_id": 7}),
            "prepare_repair rejects a non-string canonical_id",
        ),
        (
            "prepare",
            serde_json::json!({"canonical_id": "gap:x", "verbose": true}),
            "prepare_repair rejects unknown arguments",
        ),
        (
            "attempt",
            serde_json::json!({}),
            "get_repair_attempt rejects a missing attempt_id",
        ),
        (
            "attempt",
            serde_json::json!({"attempt_id": ""}),
            "get_repair_attempt rejects an empty attempt_id",
        ),
        (
            "attempt",
            serde_json::json!({"attempt_id": 7}),
            "get_repair_attempt rejects a non-string attempt_id",
        ),
        (
            "attempt",
            serde_json::json!({"attempt_id": "repair-attempt:x", "verbose": true}),
            "get_repair_attempt rejects unknown arguments",
        ),
        (
            "receipt",
            serde_json::json!({}),
            "get_receipt_status rejects a missing receipt_id",
        ),
        (
            "receipt",
            serde_json::json!({"receipt_id": ""}),
            "get_receipt_status rejects an empty receipt_id",
        ),
        (
            "receipt",
            serde_json::json!({"receipt_id": 7}),
            "get_receipt_status rejects a non-string receipt_id",
        ),
        (
            "receipt",
            serde_json::json!({"receipt_id": "receipt:x", "verbose": true}),
            "get_receipt_status rejects unknown arguments",
        ),
        (
            "card",
            serde_json::json!({}),
            "get_repair_card rejects a missing canonical_id",
        ),
        (
            "card",
            serde_json::json!({"canonical_id": ""}),
            "get_repair_card rejects an empty canonical_id",
        ),
        (
            "card",
            serde_json::json!({"canonical_id": 7}),
            "get_repair_card rejects a non-string canonical_id",
        ),
        (
            "card",
            serde_json::json!({"canonical_id": "gap:x", "verbose": true}),
            "get_repair_card rejects unknown arguments",
        ),
    ] {
        let arguments: Option<serde_json::Map<String, Value>> =
            serde_json::from_value(arguments).map_err(|error| error.to_string())?;
        // Each case binds to the handler it names: rejection by a sibling
        // handler must not satisfy the assertion.
        let result = match tool {
            "prepare" => server.prepare_repair_tool(arguments).await,
            "attempt" => server.get_repair_attempt_tool(arguments).await,
            "card" => server.get_repair_card_tool(arguments).await,
            _ => server.get_receipt_status_tool(arguments).await,
        };
        let rejected = matches!(
            result,
            Err(error) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS
        );
        if !rejected {
            return Err(format!("{call} with invalid params"));
        }
    }
    Ok(())
}
