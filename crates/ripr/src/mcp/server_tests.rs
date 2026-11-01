use super::*;

/// These controls exercise a healthy root before its first analysis. Bind
/// both facts returned by the production root resolver, rather than pairing
/// a ready status with an artificially unavailable analysis root.
fn healthy_server() -> Result<McpServer, String> {
    let (status, root) = WorkspaceStatus::resolve_with_root(None);
    if root.is_none() {
        return Err("healthy MCP fixture did not resolve a usable root".to_string());
    }
    McpServer::new(status, root).map_err(|error| error.to_string())
}

#[tokio::test(flavor = "current_thread")]
async fn delayed_durable_read_leaves_the_async_executor_and_status_available() -> Result<(), String>
{
    let server = Arc::new(
        McpServer::new(
            WorkspaceStatus::resolve_with_root(None).0,
            Some(std::env::temp_dir()),
        )
        .map_err(|error| error.to_string())?,
    );
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    // Inject only the durable reader, through the production dispatcher and
    // its session-lock selection. Status uses this same server. This does not
    // claim a stock stdio cancellation journey.
    let reader_server = server.clone();
    let worker = tokio::spawn(async move {
        reader_server
            .repair_read_document_with("repair-attempt:delayed", false, move || {
                let _ = started_tx.send(());
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .map_err(|error| {
                        workspace::AttemptFailure::new(
                            workspace::CODE_ANALYSIS_FAILED,
                            format!("delayed reader was not released: {error}"),
                            "test reader release",
                        )
                    })?;
                Ok(serde_json::json!({ "read": "completed" }))
            })
            .await
    });
    let started = tokio::time::timeout(std::time::Duration::from_secs(1), started_rx).await;
    let live = tokio::time::timeout(
        std::time::Duration::from_millis(250),
        server.status_tool(super::super::MAX_RESPONSE_BYTES),
    )
    .await;
    let completed_early = worker.is_finished();
    let _ = release_tx.send(());
    let observed = worker.await.map_err(|error| error.to_string())?;
    assert!(
        !completed_early,
        "the delayed reader ran synchronously or timed out before release"
    );
    started
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    live.map_err(|error| format!("async status blocked behind durable read: {error}"))?
        .map_err(|error| error.to_string())?;
    let observed = observed.map_err(|failure| failure.detail)?;
    assert_eq!(observed["read"], "completed");
    Ok(())
}

#[test]
fn oversized_status_envelope_gets_typed_too_large_like_every_tool() -> Result<(), String> {
    // `ripr_workspace_status` goes through the shared response bound
    // (#5254 item 4): an over-cap envelope yields the typed
    // `result_too_large` failure, not the writer backstop.
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let envelope = serde_json::json!({
        "content": [{"type": "text", "text": "x".repeat(super::super::MAX_RESPONSE_BYTES + 1)}],
        "structuredContent": {},
        "isError": false,
    });
    let response = server
        .bounded_tool_envelope(
            envelope,
            workspace::SESSION_SCHEMA_VERSION,
            "serialize workspace status",
            workspace::RECOVERY_STATUS_TOO_LARGE,
            super::super::MAX_RESPONSE_BYTES,
        )
        .map_err(|error| error.to_string())?;
    let rendered = format!("{response:?}");
    if !rendered.contains(workspace::CODE_RESULT_TOO_LARGE) {
        return Err(format!(
            "oversized envelope missed typed refusal: {rendered}"
        ));
    }
    if rendered.contains("ripr_get_gap") {
        return Err(format!("status overflow named ripr_get_gap: {rendered}"));
    }
    Ok(())
}

/// #7768: the budget charges the request ID's actual JSON serialization.
/// Every escaping class is exact, and a saturated (oversized) legal id
/// leaves nothing: the semantic layers then refuse every response typed
/// instead of shipping one the wire must replace.
#[test]
fn outer_envelope_budget_reserves_the_escaped_request_id() -> Result<(), String> {
    let budget_for = |id: &str| {
        super::super::outer_envelope_budget(&rmcp::model::RequestId::String(id.to_owned().into()))
    };
    let bound = super::super::MAX_RESPONSE_BYTES;
    let reserve = super::super::OUTER_RESPONSE_OVERHEAD_BYTES;
    let cases = [
        ("plain", "req-1".to_owned(), "\"req-1\"".len()),
        ("quotes", "\"".repeat(32), 66),
        ("backslashes", "\\".repeat(32), 66),
        ("control", "\u{1}".repeat(32), 194),
        ("non-ascii", "é".repeat(32), 66),
        ("emoji", "\u{1F600}".repeat(32), 130),
    ];
    for (class, id, escaped) in cases {
        let budget = budget_for(&id);
        if budget != bound - escaped - reserve {
            return Err(format!(
                "{class} id budget drifted: {budget} != {bound} - {escaped} - {reserve}"
            ));
        }
    }
    let giant = "\u{1}".repeat(bound / 2);
    if budget_for(&giant) != 0 {
        return Err("a wire-overflowing id must saturate the budget to zero".into());
    }
    Ok(())
}

/// #7768: a long legal request id narrows the shared bound end to end.
/// An envelope exactly at the id-aware budget ships structured; one step
/// past it — including the window where the compact text would still fit —
/// returns the typed `result_too_large` refusal with structured failure
/// content, and the refusal detail names the narrowed budget so a host can
/// see the request id, not an instrument error, caused it. (The
/// double-carry envelope moves in 2-byte steps, so the exact one-byte flip
/// is pinned in
/// `protocol.rs::tests::tool_result_fits_and_refuses_at_the_id_aware_envelope_budget`.)
#[tokio::test(flavor = "current_thread")]
async fn long_request_id_refuses_typed_at_the_id_aware_boundary() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let id = rmcp::model::RequestId::String("\"".repeat(60_000).into());
    let budget = super::super::outer_envelope_budget(&id);
    if budget == 0 || budget >= super::super::MAX_RESPONSE_BYTES {
        return Err(format!("long-id budget drifted out of range: {budget}"));
    }
    // The envelope grows exactly 2 bytes per `x` (raw plus escaped text
    // copy), so sizes sit on even steps from the measured base.
    let document = |pad: usize| {
        serde_json::json!({
            "schema_version": "ripr-mcp-workspace-status-v1",
            "note": "x".repeat(pad),
        })
    };
    let envelope_of = |pad: usize| -> Result<usize, String> {
        crate::mcp::protocol::structured_envelope_bytes(&document(pad))
            .map_err(|error| error.to_string())
    };
    let base = envelope_of(0)?;
    let delta = budget - base;
    if !delta.is_multiple_of(2) {
        return Err(format!(
            "boundary fixture parity drifted: base={base}, budget={budget}"
        ));
    }
    let at = delta / 2;
    if envelope_of(at)? != budget {
        return Err("boundary fixture missed its exact bytes".into());
    }
    // The refusal fixture sits far past the budget in both shapes: its
    // compact-text copy alone renders more than the budget, so the
    // envelope builder fails closed instead of shipping text-only.
    let shipping = server
        .bounded_tool_result(
            document(at),
            workspace::SESSION_SCHEMA_VERSION,
            "serialize boundary fixture",
            workspace::RECOVERY_STATUS_TOO_LARGE,
            budget,
        )
        .map_err(|error| error.to_string())?;
    // Assert on the wire shape, not the Debug rendering: the Debug output
    // names the `structured_content` field even when it is None, so only
    // the serialized value distinguishes a structured result from the
    // text-only fallback.
    let rmcp::model::CallToolResponse::Complete(shipped_result) = shipping else {
        return Err("boundary fixture did not complete".into());
    };
    let shipped = serde_json::to_value(&shipped_result).map_err(|error| error.to_string())?;
    if shipped.get("structuredContent").is_none() || shipped["isError"] == serde_json::json!(true) {
        return Err(format!(
            "an envelope exactly at the id-aware budget must ship structured: {shipped}"
        ));
    }
    // The window fixture sits one step past the budget: the structured
    // envelope overflows while the compact text still fits — the exact
    // interval where the removed text-only fallback shipped a success
    // without `structuredContent` (#7768). It must refuse typed with
    // structured failure content, asserted on the serialized wire shape.
    let window = at + 32;
    if envelope_of(window)? <= budget {
        return Err("window fixture must overflow the structured envelope".into());
    }
    let text_only = serde_json::json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&document(window)).map_err(|error| error.to_string())? }],
        "isError": false,
    });
    let text_only_bytes = serde_json::to_vec(&text_only)
        .map_err(|error| error.to_string())?
        .len();
    if text_only_bytes > budget {
        return Err(format!(
            "window fixture must fit as compact text: {text_only_bytes} against {budget}"
        ));
    }
    let windowed = server
        .bounded_tool_result(
            document(window),
            workspace::SESSION_SCHEMA_VERSION,
            "serialize boundary fixture",
            workspace::RECOVERY_STATUS_TOO_LARGE,
            budget,
        )
        .map_err(|error| error.to_string())?;
    let rmcp::model::CallToolResponse::Complete(window_result) = windowed else {
        return Err("window fixture did not complete".into());
    };
    let window_wire = serde_json::to_value(&window_result).map_err(|error| error.to_string())?;
    if window_wire.pointer("/structuredContent/failure/code")
        != Some(&serde_json::json!("result_too_large"))
        || window_wire.get("isError") != Some(&serde_json::json!(true))
    {
        return Err(format!(
            "a window document must refuse typed with structured failure content, not drop structuredContent silently: {window_wire}"
        ));
    }
    // The refusal fixture sits far past the budget in both shapes: its
    // compact-text copy alone renders more than the budget, so the
    // envelope builder fails closed there too.
    let refused = server
        .bounded_tool_result(
            document(budget),
            workspace::SESSION_SCHEMA_VERSION,
            "serialize boundary fixture",
            workspace::RECOVERY_STATUS_TOO_LARGE,
            budget,
        )
        .map_err(|error| error.to_string())?;
    let refused = format!("{refused:?}");
    if !refused.contains(workspace::CODE_RESULT_TOO_LARGE) || !refused.contains(&budget.to_string())
    {
        return Err(format!(
            "a document past the id-aware budget in both shapes must refuse typed with the narrowed budget named: {refused}"
        ));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn status_tool_still_serves_through_the_shared_bound() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    server
        .status_tool(super::super::MAX_RESPONSE_BYTES)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// #7462 review: the recovery wording is wired into the real routes, not
/// only the shared constant. One canonical item whose serialized document
/// exceeds the MCP response bound is bound into a completed snapshot and
/// read through the actual `ripr_get_gap` tool dispatch; the typed failure
/// must leave MCP through the committed CLI read (disclosed as a bounded
/// summary) and must not send the caller back through `ripr_get_gap`. The
/// gap-resource failure rendering and its URI route carry the same
/// recovery, so a miswired constant or a reintroduced circular recovery
/// fails here even though the constant-level control in `workspace.rs`
/// still passes.
#[tokio::test(flavor = "current_thread")]
async fn oversized_gap_item_leaves_mcp_through_the_real_routes() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let outcome = crate::analysis_outcome::AnalysisOutcome::new(
        crate::analysis_outcome::AnalysisOutcomeKind::CompleteWithFindings,
        Default::default(),
        crate::analysis_outcome::AnalysisOutcomeCounts {
            changed_file_count: 1,
            changed_line_count: 2,
            candidate_line_count: 2,
            probe_count: 2,
            finding_count: 1,
        },
        Vec::new(),
    )
    .map_err(|error| format!("fixture outcome refused: {error}"))?;
    let mut finding = gaps::test_finding()?;
    finding.evidence = vec!["x".repeat(super::super::MAX_RESPONSE_BYTES)];
    let output = crate::app::CheckOutput {
        value_origins: Default::default(),
        schema_version: crate::app::CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
        harness_projections: Vec::new(),
        tool: "ripr".to_string(),
        mode: crate::app::Mode::Draft,
        root: PathBuf::from("."),
        base: None,
        analysis_outcome: Some(outcome),
        summary: crate::domain::Summary::default(),
        findings: vec![finding],
        preview_language_advisories: Vec::new(),
        language_runs: Vec::new(),
        no_scope_provided: false,
        unanalyzed_working_tree: false,
        untracked_working_tree_source_paths: Vec::new(),
        ignored_working_tree_source_paths: Vec::new(),
        unlinked_python_tests: None,
        suppression: None,
        partial_scope: None,
        analyzed_revisions: None,
    };
    let snapshot = workspace::Snapshot::from_output(&output, Some("root:sha256:test"))
        .map_err(|failure| failure.detail)?;
    *server.session.lock().await = workspace::WorkspaceSession {
        in_flight: false,
        last_good: Some(Arc::new(snapshot)),
        ..Default::default()
    };

    // The tool route: the real `ripr_get_gap` dispatch.
    let mut arguments = serde_json::Map::new();
    arguments.insert("canonical_id".to_string(), Value::from("gap:test:1"));
    let response = server
        .get_gap_tool(Some(arguments), super::super::MAX_RESPONSE_BYTES)
        .await
        .map_err(|error| format!("an oversized item must fail typed, not error: {error}"))?;
    let rendered = format!("{response:?}");
    if !rendered.contains(workspace::CODE_RESULT_TOO_LARGE) {
        return Err(format!(
            "routed tool response missed typed refusal: {rendered}"
        ));
    }
    if !rendered.contains("--committed --json")
        || !rendered.contains("related tests are capped per finding")
    {
        return Err(format!(
            "routed tool recovery lost the committed CLI route: {rendered}"
        ));
    }
    if rendered.contains("one item through ripr_get_gap") {
        return Err(format!(
            "routed tool recovery went circular again: {rendered}"
        ));
    }

    // The gap-resource route: the same session read rendered through the
    // production resource-failure path, behind the URI route.
    if gaps::gap_resource_id("ripr://gap/gap:test:1") != Some("gap:test:1") {
        return Err("the gap resource URI stopped routing this item".to_string());
    }
    let failure = server
        .session
        .lock()
        .await
        .get_gap("gap:test:1", None)
        .err()
        .ok_or_else(|| "the oversized item must fail the resource read too".to_string())?;
    if failure.code != workspace::CODE_RESULT_TOO_LARGE {
        return Err(format!("unexpected resource failure code {}", failure.code));
    }
    let error = resource_failure("gap", &failure, None);
    let data = error
        .data
        .ok_or_else(|| "the gap resource failure carries typed data".to_string())?;
    let recovery = data["recovery"]
        .as_str()
        .ok_or("gap resource lost recovery")?;
    if !recovery.contains("--committed --json")
        || !recovery.contains("related tests are capped per finding")
    {
        return Err(format!("gap resource recovery drifted: {data}"));
    }
    Ok(())
}

/// #7768: the original silent drop at the real `ripr_get_gap` dispatch.
/// One canonical item whose serialized document the document guard approves
/// (under the raw response bound) but whose complete double-carry envelope
/// cannot fit the bound used to ship as a success envelope missing
/// `structuredContent` — no error, no explanation — although the tool
/// advertises an `outputSchema`. It must refuse typed with structured
/// failure content, asserted on the serialized wire shape, and its
/// recovery must send the caller to the committed CLI read rather than
/// back through `ripr_get_gap`.
#[tokio::test(flavor = "current_thread")]
async fn window_sized_gap_item_refuses_typed_instead_of_dropping_structured_content()
-> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let outcome = crate::analysis_outcome::AnalysisOutcome::new(
        crate::analysis_outcome::AnalysisOutcomeKind::CompleteWithFindings,
        Default::default(),
        crate::analysis_outcome::AnalysisOutcomeCounts {
            changed_file_count: 1,
            changed_line_count: 2,
            candidate_line_count: 2,
            probe_count: 2,
            finding_count: 1,
        },
        Vec::new(),
    )
    .map_err(|error| format!("fixture outcome refused: {error}"))?;
    let mut finding = gaps::test_finding()?;
    finding.evidence = vec!["x".repeat(70_000)];
    let output = crate::app::CheckOutput {
        value_origins: Default::default(),
        schema_version: crate::app::CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
        harness_projections: Vec::new(),
        tool: "ripr".to_string(),
        mode: crate::app::Mode::Draft,
        root: PathBuf::from("."),
        base: None,
        analysis_outcome: Some(outcome),
        summary: crate::domain::Summary::default(),
        findings: vec![finding],
        preview_language_advisories: Vec::new(),
        language_runs: Vec::new(),
        no_scope_provided: false,
        unanalyzed_working_tree: false,
        untracked_working_tree_source_paths: Vec::new(),
        ignored_working_tree_source_paths: Vec::new(),
        unlinked_python_tests: None,
        suppression: None,
        partial_scope: None,
        analyzed_revisions: None,
    };
    let snapshot = workspace::Snapshot::from_output(&output, Some("root:sha256:test"))
        .map_err(|failure| failure.detail)?;
    *server.session.lock().await = workspace::WorkspaceSession {
        in_flight: false,
        last_good: Some(Arc::new(snapshot)),
        ..Default::default()
    };

    // Fixture guard: the window document passes the raw-cap document guard
    // while its complete double-carry envelope overflows the bound.
    let document = server
        .session
        .lock()
        .await
        .get_gap("gap:test:1", None)
        .map_err(|failure| failure.detail)?;
    let document_bytes = serde_json::to_vec(&document)
        .map_err(|error| error.to_string())?
        .len();
    let envelope_bytes = crate::mcp::protocol::structured_envelope_bytes(&document)
        .map_err(|error| error.to_string())?;
    if document_bytes >= super::super::MAX_RESPONSE_BYTES
        || envelope_bytes <= super::super::MAX_RESPONSE_BYTES
    {
        return Err(format!(
            "window fixture missed its interval: document={document_bytes}, envelope={envelope_bytes}, bound={}",
            super::super::MAX_RESPONSE_BYTES
        ));
    }

    // The real `ripr_get_gap` dispatch.
    let mut arguments = serde_json::Map::new();
    arguments.insert("canonical_id".to_string(), Value::from("gap:test:1"));
    let response = server
        .get_gap_tool(Some(arguments), super::super::MAX_RESPONSE_BYTES)
        .await
        .map_err(|error| format!("a window item must fail typed, not error: {error}"))?;
    let rmcp::model::CallToolResponse::Complete(result) = response else {
        return Err("window gap item did not complete".into());
    };
    let wire = serde_json::to_value(&result).map_err(|error| error.to_string())?;
    if wire.get("isError") != Some(&serde_json::json!(true))
        || wire.pointer("/structuredContent/failure/code")
            != Some(&serde_json::json!("result_too_large"))
    {
        return Err(format!(
            "a window gap item must refuse typed with structured failure content, not ship a success without structuredContent: {wire}"
        ));
    }
    let recovery = wire
        .pointer("/structuredContent/failure/recovery")
        .and_then(Value::as_str)
        .ok_or_else(|| "window gap refusal omitted recovery".to_string())?;
    if !recovery.contains("--committed --json")
        || !recovery.contains("related tests are capped per finding")
    {
        return Err(format!("window gap refusal recovery drifted: {recovery}"));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn oversized_status_tool_response_returns_typed_too_large() -> Result<(), String> {
    // The envelope-level test pins the shared bound; this one pins that
    // `status_tool` itself routes through it (#6291): restoring the
    // previous unbounded conversion must fail here.
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    server.session.lock().await.last_failure = Some(
        workspace::AttemptFailure::new(workspace::CODE_ANALYSIS_FAILED, "test failure", "retry")
            .with_data(serde_json::json!({
                "payload": "x".repeat(super::super::MAX_RESPONSE_BYTES + 1),
            })),
    );
    let response = server
        .status_tool(super::super::MAX_RESPONSE_BYTES)
        .await
        .map_err(|error| error.to_string())?;
    let rendered = format!("{response:?}");
    if !rendered.contains(workspace::CODE_RESULT_TOO_LARGE) {
        return Err(format!("oversized status missed typed refusal: {rendered}"));
    }
    Ok(())
}

#[test]
fn sdk_server_metadata_preserves_bounded_status_instructions() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let config = server.get_info();
    let instructions = config
        .instructions
        .ok_or_else(|| "SDK metadata omitted instructions".to_string())?;
    if config.server_info.name != "ripr"
        || !instructions.contains("ripr check --worktree --format json")
        || !instructions.contains("never edits source")
        || !instructions.contains("ripr_list_gaps")
    {
        return Err("SDK metadata changed application identity or analysis authority".into());
    }
    let tools = &server.tools;
    if tools.tools.len() != 10 {
        return Err(format!(
            "SDK descriptor must expose the ten-tool slice surface, got {} tools",
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
fn sdk_session_mutating_tools_pin_user_world_readonly_scope() -> Result<(), String> {
    // #5192 decision: readOnlyHint covers the user's world (no source
    // edits, no launched processes, no verification/mutation execution).
    // Session-scoped in-memory state (the committed snapshot, the
    // deterministic replayed transaction) does not revoke it. Refresh
    // additionally pins idempotentHint:false because repeat calls advance
    // the committed snapshot; prepare_repair pins idempotentHint:true
    // because replays return the identical document.
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    for (name, idempotent) in [
        (protocol::REFRESH_TOOL_NAME, false),
        (protocol::PREPARE_REPAIR_TOOL_NAME, true),
    ] {
        let tool = server
            .tools
            .tools
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| format!("{name} tool missing"))?;
        let document = serde_json::to_value(tool).map_err(|error| error.to_string())?;
        let annotations = document
            .pointer("/annotations")
            .ok_or_else(|| format!("{name} tool lost annotations"))?;
        if annotations
            .pointer("/readOnlyHint")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            return Err(format!("{name} tool lost user-world read-only annotation"));
        }
        if annotations
            .pointer("/idempotentHint")
            .and_then(serde_json::Value::as_bool)
            != Some(idempotent)
        {
            return Err(format!("{name} tool changed idempotency disclosure"));
        }
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
    let server = healthy_server()?;
    let response = server
        .list_gaps_tool(None, super::super::MAX_RESPONSE_BYTES)
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
async fn list_gaps_accepts_offset_and_limit_at_the_dispatch_edge() -> Result<(), String> {
    // #6021: paging arguments are part of the input contract. A pre-refresh
    // server answers past dispatch with typed `no_snapshot` — proving the
    // arguments were accepted — while malformed values stay invalid params.
    let server = healthy_server()?;
    for arguments in [
        serde_json::json!({}),
        serde_json::json!({"offset": 0}),
        serde_json::json!({"offset": 3, "limit": 10}),
        serde_json::json!({"snapshot_id": "snapshot:sha256:x", "offset": 0, "limit": 1}),
    ] {
        let arguments = serde_json::from_value(arguments).map_err(|error| error.to_string())?;
        let response = server
            .list_gaps_tool(arguments, super::super::MAX_RESPONSE_BYTES)
            .await
            .map_err(|error| format!("dispatch must accept paging arguments: {error:?}"))?;
        let result = match response {
            rmcp::model::CallToolResponse::Complete(result) => result,
            other => {
                return Err(format!(
                    "accepted paging arguments must reach the typed failure, got {other:?}"
                ));
            }
        };
        let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
        if value
            .pointer("/structuredContent/failure/code")
            .and_then(serde_json::Value::as_str)
            != Some(workspace::CODE_NO_SNAPSHOT)
        {
            return Err(format!("paging call lost no_snapshot: {value}"));
        }
    }
    for arguments in [
        serde_json::json!({"offset": -1}),
        serde_json::json!({"offset": 1.5}),
        serde_json::json!({"offset": "3"}),
        serde_json::json!({"limit": 0}),
        serde_json::json!({"limit": true}),
        serde_json::json!({"verbose": true}),
    ] {
        let arguments = serde_json::from_value(arguments).map_err(|error| error.to_string())?;
        match server
            .list_gaps_tool(arguments, super::super::MAX_RESPONSE_BYTES)
            .await
        {
            Err(error) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS => {}
            Ok(response) => {
                return Err(format!(
                    "list_gaps must reject malformed paging arguments with invalid params: {response:?}"
                ));
            }
            Err(error) => {
                return Err(format!(
                    "list_gaps must reject malformed paging arguments with invalid params, got {error:?}"
                ));
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn list_gaps_accepts_filters_at_the_dispatch_edge() -> Result<(), String> {
    // #7464: filter arguments are part of the input contract. A pre-refresh
    // server answers past dispatch with typed `no_snapshot` — proving the
    // arguments were accepted — while malformed values stay invalid params.
    let server = healthy_server()?;
    for arguments in [
        serde_json::json!({"file": "src/checkout.rs"}),
        serde_json::json!({"class": "weakly_exposed"}),
        serde_json::json!({"language": "rust"}),
        serde_json::json!({"repair_ready": true}),
        serde_json::json!({"repair_ready": false}),
        serde_json::json!({
            "file": "src/checkout.rs",
            "class": "weakly_exposed",
            "language": "rust",
            "repair_ready": true,
            "offset": 0,
            "limit": 10,
        }),
    ] {
        let arguments = serde_json::from_value(arguments).map_err(|error| error.to_string())?;
        let response = server
            .list_gaps_tool(arguments, super::super::MAX_RESPONSE_BYTES)
            .await
            .map_err(|error| format!("dispatch must accept filter arguments: {error:?}"))?;
        let result = match response {
            rmcp::model::CallToolResponse::Complete(result) => result,
            other => {
                return Err(format!(
                    "accepted filter arguments must reach the typed failure, got {other:?}"
                ));
            }
        };
        let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
        if value
            .pointer("/structuredContent/failure/code")
            .and_then(serde_json::Value::as_str)
            != Some(workspace::CODE_NO_SNAPSHOT)
        {
            return Err(format!("filtered call lost no_snapshot: {value}"));
        }
    }
    for arguments in [
        serde_json::json!({"file": ""}),
        serde_json::json!({"file": 7}),
        serde_json::json!({"file": null}),
        serde_json::json!({"class": ""}),
        serde_json::json!({"class": null}),
        serde_json::json!({"language": 7}),
        serde_json::json!({"repair_ready": "true"}),
        serde_json::json!({"repair_ready": 1}),
        serde_json::json!({"repair_ready": null}),
        serde_json::json!({"repairReady": true}),
    ] {
        let arguments = serde_json::from_value(arguments).map_err(|error| error.to_string())?;
        match server
            .list_gaps_tool(arguments, super::super::MAX_RESPONSE_BYTES)
            .await
        {
            Err(error) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS => {}
            Ok(response) => {
                return Err(format!(
                    "list_gaps must reject malformed filter arguments with invalid params: {response:?}"
                ));
            }
            Err(error) => {
                return Err(format!(
                    "list_gaps must reject malformed filter arguments with invalid params, got {error:?}"
                ));
            }
        }
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
        match server
            .get_gap_tool(arguments, super::super::MAX_RESPONSE_BYTES)
            .await
        {
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
    // (`canonical_id`), not the old `gap_id` name. A pre-refresh server
    // answers past-dispatch calls with typed `no_snapshot`, which proves
    // the dispatch edge accepted the spelling.
    let server = healthy_server()?;
    let arguments = serde_json::from_value(serde_json::json!({"canonical_id": "gap:x"}))
        .map_err(|error| error.to_string())?;
    let response = server
        .get_gap_tool(arguments, super::super::MAX_RESPONSE_BYTES)
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

#[tokio::test]
async fn get_gap_rejects_the_retired_gap_id_name() -> Result<(), String> {
    // #5209 removal side: guessing the old `gap_id` spelling fails at the
    // dispatch edge with invalid params, the same commit that accepts
    // `canonical_id` above.
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let arguments = serde_json::from_value(serde_json::json!({"gap_id": "gap:x"}))
        .map_err(|error| error.to_string())?;
    let result = server
        .get_gap_tool(arguments, super::super::MAX_RESPONSE_BYTES)
        .await;
    if !matches!(
        result,
        Err(error) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS
    ) {
        return Err("get_gap must reject gap_id with invalid params".to_string());
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
    let server = healthy_server()?;
    let arguments = serde_json::from_value(serde_json::json!({"canonical_id": "gap:any"}))
        .map_err(|error| error.to_string())?;
    let response = server
        .prepare_repair_tool(arguments, super::super::MAX_RESPONSE_BYTES)
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
        .get_repair_attempt_tool(arguments, super::super::MAX_RESPONSE_BYTES)
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
        .get_receipt_status_tool(arguments, super::super::MAX_RESPONSE_BYTES)
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
    let server = healthy_server()?;
    let arguments = serde_json::from_value(serde_json::json!({"canonical_id": "gap:test:1"}))
        .map_err(|error| error.to_string())?;
    let response = server
        .get_repair_card_tool(arguments, super::super::MAX_RESPONSE_BYTES)
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
            "prepare",
            serde_json::json!({"gap_id": "gap:x"}),
            "prepare_repair rejects the retired gap_id name",
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
        (
            "card",
            serde_json::json!({"gap_id": "gap:x"}),
            "get_repair_card rejects the retired gap_id name",
        ),
    ] {
        let arguments: Option<serde_json::Map<String, Value>> =
            serde_json::from_value(arguments).map_err(|error| error.to_string())?;
        // Each case binds to the handler it names: rejection by a sibling
        // handler must not satisfy the assertion.
        let result = match tool {
            "prepare" => {
                server
                    .prepare_repair_tool(arguments, super::super::MAX_RESPONSE_BYTES)
                    .await
            }
            "attempt" => {
                server
                    .get_repair_attempt_tool(arguments, super::super::MAX_RESPONSE_BYTES)
                    .await
            }
            "card" => {
                server
                    .get_repair_card_tool(arguments, super::super::MAX_RESPONSE_BYTES)
                    .await
            }
            _ => {
                server
                    .get_receipt_status_tool(arguments, super::super::MAX_RESPONSE_BYTES)
                    .await
            }
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

/// #8736 review follow-up: one captured resource-id segment must resolve
/// deterministically. Raw-first keeps legacy raw URIs working; the decoded
/// retry serves URI-emitting clients; and when both the raw spelling and
/// its decoded form name two distinct existing items, the route fails
/// closed with `id_ambiguous` instead of silently picking one.
#[test]
fn resource_id_document_resolves_raw_decoded_and_ambiguous_spellings() -> Result<(), String> {
    let item_not_found = |_id: &str| {
        Err(workspace::AttemptFailure::new(
            workspace::CODE_ITEM_NOT_FOUND,
            "no such item",
            "list the current canonical ids, then retry",
        ))
    };

    // Raw hit with nothing to decode: returned as captured.
    match resource_id_document("gap:rust:src/lib.rs:owner", |id| {
        if id == "gap:rust:src/lib.rs:owner" {
            return Ok(serde_json::json!({"id": id}));
        }
        item_not_found(id)
    }) {
        Ok(document) => assert_eq!(
            document["id"],
            serde_json::json!("gap:rust:src/lib.rs:owner")
        ),
        Err(_failure) => return Err("raw id must resolve as captured".into()),
    }

    // Raw hit whose decoded form misses: a legacy raw URI for a literal-`%`
    // file stays unambiguous.
    match resource_id_document("gap:python:src/a%20b.py:owner", |id| {
        if id == "gap:python:src/a%20b.py:owner" {
            return Ok(serde_json::json!({"id": id}));
        }
        item_not_found(id)
    }) {
        Ok(document) => assert_eq!(
            document["id"],
            serde_json::json!("gap:python:src/a%20b.py:owner")
        ),
        Err(_failure) => return Err("raw literal-percent id must resolve as captured".into()),
    }

    // Raw miss, decoded hit: the URI-emitting client is served.
    match resource_id_document("gap:python:src/a%2520b.py:owner", |id| {
        if id == "gap:python:src/a%20b.py:owner" {
            return Ok(serde_json::json!({"id": id}));
        }
        item_not_found(id)
    }) {
        Ok(document) => assert_eq!(
            document["id"],
            serde_json::json!("gap:python:src/a%20b.py:owner")
        ),
        Err(_failure) => return Err("encoded id must resolve through the decoded retry".into()),
    }

    // Both spellings name two distinct items: fail closed, never pick one.
    match resource_id_document("gap:python:src/a%20b.py:owner", |id| {
        if id == "gap:python:src/a%20b.py:owner" || id == "gap:python:src/a b.py:owner" {
            return Ok(serde_json::json!({"id": id}));
        }
        item_not_found(id)
    }) {
        Ok(document) => {
            return Err(format!(
                "ambiguous id must not silently resolve: {document}"
            ));
        }
        Err(failure) => {
            if failure.code != workspace::CODE_ID_AMBIGUOUS {
                return Err(format!(
                    "ambiguous id must fail closed as id_ambiguous: {failure:?}"
                ));
            }
            if !failure.recovery.contains("plain JSON string") {
                return Err(format!(
                    "ambiguous recovery must name the JSON-string tool route: {failure:?}"
                ));
            }
        }
    }

    // Raw miss with no decodable escape keeps the ordinary miss mapping.
    match resource_id_document("gap:rust:src/missing.rs:owner", item_not_found) {
        Ok(document) => return Err(format!("missing id must not resolve: {document}")),
        Err(failure) => {
            if failure.code != workspace::CODE_ITEM_NOT_FOUND {
                return Err(format!("plain miss must keep item_not_found: {failure:?}"));
            }
        }
    }

    // Non-miss failures pass through untouched.
    match resource_id_document::<Value>("gap:rust:src/lib.rs:owner", |_id| {
        Err(workspace::AttemptFailure::new(
            workspace::CODE_ANALYSIS_FAILED,
            "instrument failure",
            "retry",
        ))
    }) {
        Ok(document) => {
            return Err(format!("instrument failure must not resolve: {document}"));
        }
        Err(failure) => {
            if failure.code != workspace::CODE_ANALYSIS_FAILED {
                return Err(format!("instrument failure must pass through: {failure:?}"));
            }
        }
    }
    Ok(())
}

fn argument_map(value: Value) -> Option<serde_json::Map<String, Value>> {
    value.as_object().cloned()
}

#[test]
fn paging_argument_parsers_pin_type_range_and_absence() -> Result<(), String> {
    let none: Option<serde_json::Map<String, Value>> = None;
    assert_eq!(optional_nonnegative_argument(&none, "offset"), Ok(None));
    assert_eq!(optional_positive_argument(&none, "limit"), Ok(None));
    let empty = argument_map(serde_json::json!({}));
    assert_eq!(optional_nonnegative_argument(&empty, "offset"), Ok(None));

    // Zero is a valid offset but never a valid limit.
    let zero = argument_map(serde_json::json!({"offset": 0, "limit": 0}));
    assert_eq!(optional_nonnegative_argument(&zero, "offset"), Ok(Some(0)));
    let refused = optional_positive_argument(&zero, "limit")
        .err()
        .ok_or("limit 0 must be refused")?;
    assert_eq!(refused.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert_eq!(refused.message, "limit must be at least 1");
    let one = argument_map(serde_json::json!({"limit": 1}));
    assert_eq!(optional_positive_argument(&one, "limit"), Ok(Some(1)));

    // Negatives, floats, strings, booleans and null never coerce.
    for bad in [
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!("2"),
        serde_json::json!(true),
        Value::Null,
    ] {
        let arguments = argument_map(serde_json::json!({"offset": bad}));
        let error = optional_nonnegative_argument(&arguments, "offset")
            .err()
            .ok_or_else(|| format!("offset {bad} must be refused"))?;
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(
            error.message.starts_with(&format!(
                "offset must be a whole JSON integer from 0 to {}",
                usize::MAX
            )) || error.message == "offset must be at least 0; -1 is negative",
            "{}",
            error.message
        );
    }
    // The cap and the u64 bound are named for what they are (#9895).
    let huge = argument_map(serde_json::json!({"limit": 1e30}));
    let error = optional_positive_argument(&huge, "limit")
        .err()
        .ok_or("limit 1e30 must be refused")?;
    assert!(
        error.message.contains(&format!(
            "larger than the largest accepted integer {}",
            usize::MAX
        )),
        "{}",
        error.message
    );
    let capped = optional_capped_page_limit(&huge, "limit", 256)
        .err()
        .ok_or("capped limit 1e30 must be refused")?;
    assert!(
        capped
            .message
            .contains("larger than the largest accepted integer 256"),
        "{}",
        capped.message
    );
    Ok(())
}

/// #9895 review: the declared maximum is capped at the platform's `usize`
/// width, and `data.maximum`, the message and the recovery all name that one
/// effective bound. Driven with a 32-bit `platform_max` so a 64-bit host
/// proves the 32-bit behavior.
#[test]
fn integer_argument_effective_bound_is_the_platform_cap_everywhere() -> Result<(), String> {
    const PLATFORM_MAX_32: u64 = u32::MAX as u64; // widening, never lossy
    assert_eq!(
        effective_integer_maximum(u64::MAX, PLATFORM_MAX_32),
        PLATFORM_MAX_32
    );
    assert_eq!(effective_integer_maximum(256, PLATFORM_MAX_32), 256);
    assert_eq!(effective_integer_maximum(u64::MAX, u64::MAX), u64::MAX);

    let at_bound = argument_map(serde_json::json!({"limit": PLATFORM_MAX_32}));
    assert_eq!(
        optional_integer_argument_for_platform(&at_bound, "limit", 1, u64::MAX, PLATFORM_MAX_32),
        Ok(Some(u32::MAX as usize))
    );
    for over in [
        serde_json::json!(PLATFORM_MAX_32 + 1),
        serde_json::json!(u64::MAX),
        serde_json::json!(1e30),
    ] {
        let arguments = argument_map(serde_json::json!({"limit": over}));
        let error = optional_integer_argument_for_platform(
            &arguments,
            "limit",
            1,
            u64::MAX,
            PLATFORM_MAX_32,
        )
        .err()
        .ok_or_else(|| format!("limit {over} must be refused"))?;
        let data = error.data.clone().ok_or("refusal lost structured data")?;
        let bound = PLATFORM_MAX_32.to_string();
        assert_eq!(data["maximum"], serde_json::json!(PLATFORM_MAX_32));
        assert!(error.message.contains(&bound), "{}", error.message);
        assert!(
            !error.message.contains("18446744073709551615"),
            "{}",
            error.message
        );
        let recovery = data["recovery"].as_str().ok_or("recovery missing")?;
        assert!(
            recovery.contains(&format!("from 1 to {bound}")),
            "{recovery}"
        );
        assert_eq!(
            data["expected"],
            serde_json::json!(format!("integer at most {bound}"))
        );
    }
    Ok(())
}

#[test]
fn unknown_arguments_are_refused_by_name_and_allowed_ones_pass() -> Result<(), String> {
    let allowed = ["inventory_id", "offset", "limit"];
    assert_eq!(reject_unknown_arguments(&None, &allowed), Ok(()));
    let known = argument_map(serde_json::json!({"offset": 1, "limit": 2}));
    assert_eq!(reject_unknown_arguments(&known, &allowed), Ok(()));
    let unknown = argument_map(serde_json::json!({"offset": 1, "verbose": true}));
    let error = reject_unknown_arguments(&unknown, &allowed)
        .err()
        .ok_or("unknown argument must be refused")?;
    assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(error.message.contains("\"verbose\""), "{}", error.message);
    // #9853: the refusal carries the offending name and the allowed list as
    // structured data, not only as message text.
    let data = error
        .data
        .clone()
        .ok_or_else(|| format!("unknown-argument refusal lost structured data: {error:?}"))?;
    assert_eq!(data["argument"], serde_json::json!("verbose"));
    assert_eq!(data["allowed"], serde_json::json!(allowed));
    Ok(())
}

#[tokio::test]
async fn argument_validation_errors_carry_structured_recovery_data() -> Result<(), String> {
    // #9853: the dispatch-edge -32602 sites carry machine-readable recovery
    // data (`argument`/`expected`, `argument`/`allowed`) so a client can
    // branch on the payload instead of parsing message prose.
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;

    // A required argument is missing: `argument` names it, `expected` names
    // the shape that would pass.
    let missing = server
        .get_gap_tool(
            argument_map(serde_json::json!({})),
            super::super::MAX_RESPONSE_BYTES,
        )
        .await
        .err()
        .ok_or("missing canonical_id must be refused")?;
    assert_eq!(missing.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    let data = missing
        .data
        .as_ref()
        .ok_or_else(|| format!("missing-argument refusal lost structured data: {missing:?}"))?;
    assert_eq!(data["argument"], serde_json::json!("canonical_id"));
    assert_eq!(data["expected"], serde_json::json!("non-empty string"));

    // An argument has the wrong JSON type: the typed validator echoes the
    // same `argument`/`expected` shape.
    let wrong_type = server
        .list_repair_attempts_tool(
            argument_map(serde_json::json!({"offset": "1"})),
            super::super::MAX_RESPONSE_BYTES,
        )
        .await
        .err()
        .ok_or("a string offset must be refused")?;
    assert_eq!(wrong_type.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    let data = wrong_type
        .data
        .as_ref()
        .ok_or_else(|| format!("wrong-type refusal lost structured data: {wrong_type:?}"))?;
    assert_eq!(data["argument"], serde_json::json!("offset"));
    assert_eq!(data["expected"], serde_json::json!("non-negative integer"));
    Ok(())
}

#[tokio::test]
async fn list_repair_attempts_rejects_bad_arguments_at_the_dispatch_edge() -> Result<(), String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    for arguments in [
        serde_json::json!({"offset": -1}),
        serde_json::json!({"offset": "1"}),
        serde_json::json!({"limit": 0}),
        serde_json::json!({"limit": 1.5}),
        serde_json::json!({"inventory_id": 7}),
        serde_json::json!({"inventory_id": null}),
        serde_json::json!({"cursor": "x"}),
    ] {
        match server
            .list_repair_attempts_tool(
                argument_map(arguments.clone()),
                super::super::MAX_RESPONSE_BYTES,
            )
            .await
        {
            Err(error) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS => {}
            other => {
                return Err(format!(
                    "list_repair_attempts must refuse {arguments} as invalid params: {other:?}"
                ));
            }
        }
    }
    // Valid arguments with an unavailable root are a typed failure, not a protocol error.
    let response = server
        .list_repair_attempts_tool(
            argument_map(serde_json::json!({"offset": 0, "limit": 1})),
            super::super::MAX_RESPONSE_BYTES,
        )
        .await
        .map_err(|error| format!("unavailable root must be a typed result: {error:?}"))?;
    let result = match response {
        rmcp::model::CallToolResponse::Complete(result) => result,
        other => return Err(format!("expected a complete typed failure, got {other:?}")),
    };
    let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    assert_eq!(value["isError"], serde_json::json!(true));
    assert_eq!(
        value["structuredContent"]["failure"]["code"],
        serde_json::json!("workspace_unavailable")
    );
    Ok(())
}

#[test]
fn resource_failures_carry_the_typed_code_as_data_and_the_current_snapshot() -> Result<(), String> {
    let failure = workspace::AttemptFailure::new("item_not_found", "no such gap", "refresh");
    let error = resource_failure("gap", &failure, Some("snapshot:abc"));
    assert_eq!(error.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
    assert_eq!(error.message, "unavailable gap resource: item_not_found");
    let data = error.data.ok_or("typed failure data")?;
    assert_eq!(data["code"], serde_json::json!("item_not_found"));
    assert_eq!(data["recovery"], serde_json::json!("refresh"));
    assert_eq!(
        data["current_snapshot_id"],
        serde_json::json!("snapshot:abc")
    );
    assert_eq!(data["failure"]["detail"], serde_json::json!("no such gap"));

    // Without a current snapshot the key is absent rather than null.
    let bare = resource_failure("gap", &failure, None)
        .data
        .ok_or("typed failure data")?;
    assert!(bare.get("current_snapshot_id").is_none(), "{bare}");
    Ok(())
}

/// #9854: every resource-read failure kind answers its own protocol-honest
/// JSON-RPC code instead of the blanket ResourceNotFound. Identity misses
/// keep `-32002`; session-state and bound kinds answer InternalError
/// (`-32603`). The human sentence and `data.code` stay stable for existing
/// clients on both paths, and the snapshot route keeps disclosing
/// `current_snapshot_id`.
#[test]
fn resource_failure_maps_each_kind_to_a_protocol_honest_code() -> Result<(), String> {
    let not_found = ErrorCode::RESOURCE_NOT_FOUND;
    let internal = ErrorCode::INTERNAL_ERROR;
    for (code, expected) in [
        ("no_snapshot", internal),
        ("stale_snapshot", internal),
        ("analysis_in_flight", internal),
        ("workspace_unavailable", internal),
        (workspace::CODE_RESULT_TOO_LARGE, internal),
        ("attempt_invalid", internal),
        ("superseded", internal),
        ("seam_not_found", internal),
        (workspace::CODE_ITEM_NOT_FOUND, not_found),
        (repair::CODE_ATTEMPT_NOT_FOUND, not_found),
    ] {
        let failure = workspace::AttemptFailure::new(code, "typed detail", "typed recovery");
        let error = resource_failure("snapshot", &failure, Some("snapshot:current"));
        if error.code != expected {
            return Err(format!(
                "resource failure kind {code} must answer {expected:?}, got {:?}",
                error.code
            ));
        }
        if error.message != format!("unavailable snapshot resource: {code}") {
            return Err(format!("human sentence drifted for {code}: {error:?}"));
        }
        let data = error
            .data
            .ok_or_else(|| format!("{code} lost typed data"))?;
        if data["code"] != serde_json::json!(code)
            || data["recovery"] != serde_json::json!("typed recovery")
            || data["failure"]["detail"] != serde_json::json!("typed detail")
            || data["current_snapshot_id"] != serde_json::json!("snapshot:current")
        {
            return Err(format!("typed data payload drifted for {code}: {data}"));
        }
    }
    // The snapshot route's not-found disclosure stays on the identity path:
    // a stale-snapshot miss still names the current snapshot in data.
    let stale = workspace::AttemptFailure::new("stale_snapshot", "stale", "refresh");
    let error = resource_failure("snapshot", &stale, Some("snapshot:now"));
    if error.code != internal {
        return Err(format!(
            "stale_snapshot must not read as a resource miss: {error:?}"
        ));
    }
    let data = error.data.ok_or("stale_snapshot lost typed data")?;
    if data["current_snapshot_id"] != serde_json::json!("snapshot:now") {
        return Err(format!("stale_snapshot lost current identity: {data}"));
    }
    Ok(())
}

#[test]
fn resource_budget_failure_reports_bytes_budget_and_recovery() -> Result<(), String> {
    let error = resource_budget_failure(9_001, 4_000, "narrow the read");
    assert_eq!(
        error.message,
        format!(
            "unavailable bounded resource: {}",
            workspace::CODE_RESULT_TOO_LARGE
        )
    );
    let data = error.data.ok_or("typed failure data")?;
    assert_eq!(data["recovery"], serde_json::json!("narrow the read"));
    let detail = data["detail"].as_str().ok_or("detail")?;
    assert!(detail.contains("9001 bytes"), "{detail}");
    assert!(detail.contains("budget is 4000"), "{detail}");
    let envelope = json_envelope("ripr://x", "{}".to_string());
    assert_eq!(
        envelope["contents"][0]["uri"],
        serde_json::json!("ripr://x")
    );
    assert_eq!(
        envelope["contents"][0]["mimeType"],
        serde_json::json!("application/json")
    );
    Ok(())
}

/// #10518/#10539: the JSON-RPC code is coarse by contract and consumers branch
/// on `error.data.code`. `docs/interop/mcp.md` tabulates every kind; this test
/// keeps that table equal to the shared vocabulary (`failure_vocabulary`, the
/// owner of the enumerated kinds) and to what `resource_failure` answers.
#[test]
fn mcp_doc_error_table_matches_vocabulary_and_resource_failure_mapping() -> Result<(), String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/interop/mcp.md");
    let doc =
        std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let mut rows: Vec<(String, String)> = Vec::new();
    for line in doc.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // `| code | jsonrpc | class | recovery |` splits into 6 cells.
        if cells.len() != 6 || !cells[1].starts_with('`') || !cells[2].starts_with('`') {
            continue;
        }
        let code = cells[1].trim_matches('`').to_string();
        let jsonrpc = cells[2].trim_matches('`').to_string();
        if !jsonrpc.starts_with("-32") {
            continue;
        }
        rows.push((code, jsonrpc));
    }
    if rows.is_empty() {
        return Err("docs/interop/mcp.md lost its error-code table".to_string());
    }
    let mut documented: Vec<&str> = rows.iter().map(|(code, _)| code.as_str()).collect();
    documented.sort_unstable();
    let before = documented.len();
    documented.dedup();
    if documented.len() != before {
        return Err("docs error table lists a data.code twice".to_string());
    }
    // `id_ambiguous` is emitted only by the resource-id route, so it rides
    // `resource_failure` without being in the tool failure-schema vocabulary.
    let mut vocabulary = protocol::failure_vocabulary();
    vocabulary.push(workspace::CODE_ID_AMBIGUOUS);
    vocabulary.sort_unstable();
    if documented != vocabulary {
        return Err(format!(
            "docs/interop/mcp.md error table {documented:?} drifted from the failure vocabulary {vocabulary:?}"
        ));
    }
    for code in vocabulary {
        let jsonrpc = rows
            .iter()
            .find(|(documented, _)| documented == code)
            .map(|(_, jsonrpc)| jsonrpc.as_str())
            .ok_or_else(|| format!("{code} missing from docs table"))?;
        let failure = workspace::AttemptFailure::new(code, "d", "r");
        let error = resource_failure("gap", &failure, None);
        if error.code.0.to_string() != jsonrpc {
            return Err(format!(
                "docs say {code} answers {jsonrpc}, resource_failure answers {}",
                error.code.0
            ));
        }
    }
    Ok(())
}

/// #11132: a panic inside a language adapter during `ripr_refresh` is a typed
/// `analysis_failed` tool failure. The server answers it, keeps serving the
/// next request, and the refresh after the failure analyzes normally, so a
/// contained failure never ends the MCP process or leaves a half-committed
/// snapshot behind.
async fn worktree_refresh(
    server: &McpServer,
) -> Result<rmcp::model::CallToolResponse, rmcp::model::ErrorData> {
    let mut arguments = serde_json::Map::new();
    arguments.insert("source".to_string(), Value::String("worktree".to_string()));
    server
        .refresh_tool(Some(arguments), super::super::MAX_RESPONSE_BYTES)
        .await
}

#[tokio::test(flavor = "current_thread")]
async fn refresh_contains_a_language_adapter_panic() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-mcp-adapter-panic-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"mcp-panic\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn gate_state(flag: bool) -> bool { flag }\n",
    )
    .map_err(|error| error.to_string())?;
    let run_git = |args: &[&str]| -> Result<(), String> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .map_err(|error| format!("git {args:?}: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    };
    run_git(&["init"])?;
    run_git(&["config", "user.email", "ripr@example.invalid"])?;
    run_git(&["config", "user.name", "RIPR Test"])?;
    run_git(&["add", "Cargo.toml", "src/lib.rs"])?;
    run_git(&["commit", "-m", "base"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn gate_state(flag: bool) -> bool {\n    if flag { true } else { false }\n}\n",
    )
    .map_err(|error| error.to_string())?;

    let server = McpServer::new(
        WorkspaceStatus::resolve_with_root(Some(root.clone())).0,
        Some(root.clone()),
    )
    .map_err(|error| error.to_string())?;
    let text = |response: CallToolResponse| -> Result<String, String> {
        let rmcp::model::CallToolResponse::Complete(result) = response else {
            return Err("tool call did not complete".to_string());
        };
        serde_json::to_string(&result).map_err(|error| error.to_string())
    };

    {
        let _armed = crate::analysis::injected_adapter_panic::arm(
            &root,
            crate::analysis::language::LanguageId::Rust,
        );
        let failed = text(
            worktree_refresh(&server)
                .await
                .map_err(|error| format!("refresh must answer, not error: {error:?}"))?,
        )?;
        assert!(
            failed.contains("analysis_failed"),
            "typed analysis_failed failure expected, got {failed}"
        );
    }

    // The server is still alive and the same request now succeeds.
    server
        .status_tool(super::super::MAX_RESPONSE_BYTES)
        .await
        .map_err(|error| format!("status after contained failure: {error:?}"))?;
    let recovered = text(
        worktree_refresh(&server)
            .await
            .map_err(|error| format!("recovered refresh: {error:?}"))?,
    )?;
    assert!(
        !recovered.contains("analysis_failed"),
        "the refresh after a contained panic must analyze normally, got {recovered}"
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// #9360: a typed failure is never digested. `RIPR_MCP_TEXT_CONTENT=summary`
/// only changes the text companion of a success; a refusal keeps its
/// `isError: true` shape with the code and recovery in text.
#[test]
fn summary_mode_never_digests_a_typed_failure() -> Result<(), String> {
    let mut server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    server.text_mode = protocol::TextContentMode::Summary;
    let oversized = serde_json::json!({ "rows": "x".repeat(super::super::MAX_RESPONSE_BYTES) });
    let response = server
        .bounded_tool_result(
            oversized,
            workspace::SESSION_SCHEMA_VERSION,
            "serialize refusal fixture",
            workspace::RECOVERY_STATUS_TOO_LARGE,
            super::super::MAX_RESPONSE_BYTES,
        )
        .map_err(|error| error.to_string())?;
    let rmcp::model::CallToolResponse::Complete(result) = response else {
        return Err("refusal did not complete".into());
    };
    let wire = serde_json::to_value(&result).map_err(|error| error.to_string())?;
    let text = wire
        .pointer("/content/0/text")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("refusal lost its text: {wire}"))?;
    if wire["isError"] != serde_json::json!(true)
        || !text.starts_with(workspace::CODE_RESULT_TOO_LARGE)
        || text.contains("ripr summary")
        || wire.pointer("/structuredContent/failure/code")
            != Some(&serde_json::json!(workspace::CODE_RESULT_TOO_LARGE))
    {
        return Err(format!("a typed failure must not be digested: {wire}"));
    }
    Ok(())
}

async fn contained_call(
    server: &McpServer,
    tool: &str,
    arguments: serde_json::Value,
) -> Result<Result<serde_json::Value, ErrorData>, String> {
    let mut request = rmcp::model::CallToolRequestParams::new(tool.to_string());
    if let serde_json::Value::Object(map) = arguments {
        request = request.with_arguments(map);
    }
    match server
        .call_tool_contained(request, super::super::MAX_RESPONSE_BYTES)
        .await
    {
        Ok(rmcp::model::CallToolResponse::Complete(result)) => serde_json::to_value(&result)
            .map(Ok)
            .map_err(|error| error.to_string()),
        Ok(_other) => Err("tool call did not complete".to_string()),
        Err(error) => Ok(Err(error)),
    }
}

/// #11621: a model-fixable argument problem on a known tool is an
/// `isError: true` `invalid_argument` tool result for every tool, carrying
/// `data` and a recovery; an unknown tool stays a protocol -32602 (no tool ran).
#[tokio::test(flavor = "current_thread")]
async fn argument_errors_are_typed_tool_failures_and_unknown_tools_stay_protocol_errors()
-> Result<(), String> {
    let server = healthy_server()?;
    let cases = [
        (
            protocol::STATUS_TOOL_NAME,
            serde_json::json!({"verbose": true}),
        ),
        (protocol::REFRESH_TOOL_NAME, serde_json::json!({"other": 1})),
        (
            protocol::LIST_GAPS_TOOL_NAME,
            serde_json::json!({"class": "bogus"}),
        ),
        (
            protocol::LIST_GAPS_TOOL_NAME,
            serde_json::json!({"limit": 0}),
        ),
        (
            protocol::LIST_GAPS_TOOL_NAME,
            serde_json::json!({"limit": 501}),
        ),
        (protocol::GET_GAP_TOOL_NAME, serde_json::json!({})),
        (
            protocol::PREPARE_REPAIR_TOOL_NAME,
            serde_json::json!({"canonical_id": 7}),
        ),
        (
            protocol::GET_REPAIR_ATTEMPT_TOOL_NAME,
            serde_json::json!({}),
        ),
        (
            protocol::GET_RECEIPT_STATUS_TOOL_NAME,
            serde_json::json!({}),
        ),
        (
            repair_card::GET_REPAIR_CARD_TOOL_NAME,
            serde_json::json!({}),
        ),
        (
            super::super::attempt_inventory::TOOL_NAME,
            serde_json::json!({"limit": 257}),
        ),
    ];
    for (tool, arguments) in cases {
        let wire = contained_call(&server, tool, arguments.clone())
            .await?
            .map_err(|error| format!("{tool} {arguments} leaked a protocol error: {error:?}"))?;
        let failure = wire.pointer("/structuredContent/failure");
        let text = |key: &str| {
            failure
                .and_then(|failure| failure.get(key))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
        };
        if wire["isError"] != serde_json::json!(true)
            || text("code") != "invalid_argument"
            || text("recovery").is_empty()
            || text("detail").is_empty()
            || !failure
                .and_then(|failure| failure.get("data"))
                .is_some_and(serde_json::Value::is_object)
        {
            return Err(format!(
                "{tool} {arguments} is not an invalid_argument failure: {wire}"
            ));
        }
    }
    // The limit data names the working-set bound, not usize::MAX.
    let wire = contained_call(
        &server,
        protocol::LIST_GAPS_TOOL_NAME,
        serde_json::json!({"limit": 501}),
    )
    .await?
    .map_err(|error| format!("{error:?}"))?;
    if wire.pointer("/structuredContent/failure/data/maximum") != Some(&serde_json::json!(500)) {
        return Err(format!("limit must be capped at 500: {wire}"));
    }
    // Negative: an unknown tool has no tool result; it stays -32602.
    match contained_call(&server, "ripr_everything", serde_json::json!({})).await? {
        Err(error) if error.code == rmcp::model::ErrorCode::INVALID_PARAMS => {}
        other => {
            return Err(format!(
                "an unknown tool must stay a protocol error: {other:?}"
            ));
        }
    }
    // Negative: a valid call on the same server is not a failure.
    let ok = contained_call(&server, protocol::STATUS_TOOL_NAME, serde_json::json!({}))
        .await?
        .map_err(|error| format!("{error:?}"))?;
    if ok["isError"] != serde_json::json!(false) {
        return Err(format!("a valid call must succeed: {ok}"));
    }
    Ok(())
}

#[test]
fn typed_failure_carries_the_remedy_its_recovery_prose_states() {
    // Positive: the recovery names a tool and a retry.
    let stated = workspace::AttemptFailure::new(
        "item_not_found",
        "no canonical item gap:x exists",
        "list the current canonical ids with ripr_list_gaps, then retry",
    )
    .value();
    assert_eq!(stated["remedy"]["kind"], "mcp_tool", "{stated}");
    assert_eq!(stated["remedy"]["tool"], "ripr_list_gaps", "{stated}");
    assert_eq!(stated["remedy"]["retry_safe"], true, "{stated}");
    // Negative: a recovery that names no route is an explicit none, and a
    // recovery that says repeating changes nothing is not retry-safe.
    let silent =
        workspace::AttemptFailure::new("item_not_found", "gone", "read the detail").value();
    assert_eq!(silent["remedy"]["kind"], "none", "{silent}");
    assert_eq!(
        silent["remedy"]["tool"],
        serde_json::Value::Null,
        "{silent}"
    );
    let pointless = workspace::AttemptFailure::new(
        "item_not_found",
        "gone",
        "calling it again changes nothing; read next.static_limit",
    )
    .value();
    assert_eq!(pointless["remedy"]["retry_safe"], false, "{pointless}");
}
