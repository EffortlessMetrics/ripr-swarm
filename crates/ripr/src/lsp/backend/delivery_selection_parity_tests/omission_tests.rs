use super::*;
use crate::lsp::diagnostic_budget::{DiagnosticBudget, DiagnosticDeliverySelection};
use tower_lsp_server::ls_types::PreviousResultId;

fn drained_parity_backend() -> Result<ParityHarness, String> {
    let mut harness = parity_backend()?;
    let (idle_service, idle_socket) =
        LspService::new(|client| Backend::new(client, PathBuf::from(".")));
    let socket = std::mem::replace(&mut harness._socket, idle_socket);
    let (input, peer) = tokio::io::duplex(1);
    // Warning logs use the client channel even before initialization. Drive
    // it so repeated partial pulls cannot block on test-only backpressure.
    // Dropping the owned runtime aborts/reaps this in-process server task.
    harness.runtime.spawn(async move {
        let _peer = peer;
        tower_lsp_server::Server::new(input, tokio::io::sink(), socket)
            .serve(idle_service)
            .await;
    });
    Ok(harness)
}

fn document_report(
    harness: &ParityHarness,
    uri: &Uri,
    previous: Option<String>,
) -> Result<serde_json::Value, String> {
    let report = harness
        .runtime
        .block_on(
            harness
                .service
                .inner()
                .diagnostic(DocumentDiagnosticParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    identifier: None,
                    previous_result_id: previous,
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                }),
        )
        .map_err(|error| error.to_string())?;
    serde_json::to_value(report).map_err(|error| error.to_string())
}

fn workspace_report(
    harness: &ParityHarness,
    previous: Vec<PreviousResultId>,
) -> Result<serde_json::Value, String> {
    let report = harness
        .runtime
        .block_on(
            harness
                .service
                .inner()
                .workspace_diagnostic(WorkspaceDiagnosticParams {
                    identifier: None,
                    previous_result_ids: previous,
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                }),
        )
        .map_err(|error| error.to_string())?;
    serde_json::to_value(report).map_err(|error| error.to_string())
}

fn result_id(report: &serde_json::Value) -> Result<String, String> {
    report["resultId"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("missing result identity: {report}"))
}

fn bounded_workspace(uri: &Uri, count: usize, budget: &DiagnosticBudget) -> WorkspaceDiagnostics {
    let mut workspace = parity_workspace_diagnostics(vec![(
        uri.clone(),
        (0..count)
            .map(|index| parity_diagnostic(&format!("gap:omission-{index:02}"), true))
            .collect(),
    )]);
    // Fixed evidence/profile identities deliberately isolate omission payload
    // changes from unrelated identity changes. Retaining this selection also
    // rejects a transport that reevaluates the default budget.
    workspace.snapshot.delivery_selection = Some(Arc::new(DiagnosticDeliverySelection::evaluate(
        &workspace.snapshot.diagnostics_by_uri,
        budget,
        "fixed-profile",
        "fixed-evidence",
    )));
    workspace
}

#[test]
fn omission_payload_change_invalidates_unchanged_on_both_pull_paths() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    let uri = parity_uri("identity.rs")?;
    let budget = DiagnosticBudget {
        max_items_per_document: 1,
        ..Default::default()
    };
    let first = commit(harness.service.inner(), bounded_workspace(&uri, 2, &budget))?;
    let first_report = document_report(&harness, &uri, None)?;
    let previous = result_id(&first_report)?;
    assert_eq!(
        document_report(&harness, &uri, Some(previous.clone()))?["kind"],
        "unchanged"
    );
    assert_eq!(
        workspace_report(
            &harness,
            vec![PreviousResultId {
                uri: uri.clone(),
                value: previous.clone()
            }]
        )?["items"][0]["kind"],
        "unchanged"
    );

    let second = commit(harness.service.inner(), bounded_workspace(&uri, 3, &budget))?;
    assert_eq!(
        first.served_diagnostics_for_uri(&uri),
        second.served_diagnostics_for_uri(&uri)
    );
    assert_eq!(
        applied_result(stored_selection(&first)?)?.snapshot_profile_budget_identity,
        applied_result(stored_selection(&second)?)?.snapshot_profile_budget_identity
    );
    let changed = document_report(&harness, &uri, Some(previous.clone()))?;
    assert_eq!(changed["kind"], "full");
    assert_ne!(result_id(&changed)?, previous);
    assert_eq!(first_report["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(first_report["items"][1]["data"]["omitted_count"], 1);
    assert_eq!(changed["items"][1]["data"]["omitted_count"], 2);
    let workspace = workspace_report(
        &harness,
        vec![PreviousResultId {
            uri,
            value: previous,
        }],
    )?;
    assert_eq!(workspace["items"][0]["kind"], "full");
    assert_eq!(workspace["items"][0]["items"], changed["items"]);
    Ok(())
}

#[test]
fn omission_retracts_under_budget_and_quarantine_and_clears_previous_uri() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    let uri = parity_uri("controls.rs")?;
    let budget = DiagnosticBudget {
        max_items_per_document: 1,
        ..Default::default()
    };
    commit(harness.service.inner(), bounded_workspace(&uri, 2, &budget))?;
    let omitted = document_report(&harness, &uri, None)?;
    assert_eq!(omitted["items"].as_array().map(Vec::len), Some(2));
    let previous = result_id(&omitted)?;
    commit(harness.service.inner(), bounded_workspace(&uri, 1, &budget))?;
    let complete = document_report(&harness, &uri, Some(previous.clone()))?;
    assert_eq!(complete["kind"], "full");
    assert_eq!(complete["items"].as_array().map(Vec::len), Some(1));
    assert!(complete["items"][0]["data"]["diagnostic_id"].is_string());

    // The budgeted snapshot still has an omission; dirty-buffer quarantine
    // must withdraw the marker along with all saved-state diagnostics.
    commit(harness.service.inner(), bounded_workspace(&uri, 2, &budget))?;
    harness
        .service
        .inner()
        .documents
        .lock()
        .map_err(|error| error.to_string())?
        .open(tower_lsp_server::ls_types::DidOpenTextDocumentParams {
            text_document: tower_lsp_server::ls_types::TextDocumentItem {
                uri: uri.clone(),
                language_id: "rust".to_string(),
                version: 7,
                text: "dirty".to_string(),
            },
        });
    let quarantined = document_report(&harness, &uri, Some(previous.clone()))?;
    assert_eq!(quarantined["kind"], "full");
    assert_eq!(quarantined["items"], serde_json::json!([]));
    let quarantine_id = result_id(&quarantined)?;
    assert_ne!(quarantine_id, previous);
    assert_eq!(
        document_report(&harness, &uri, Some(quarantine_id.clone()))?["kind"],
        "unchanged"
    );
    let workspace = workspace_report(&harness, Vec::new())?;
    assert_eq!(workspace["items"][0]["items"], serde_json::json!([]));
    assert_eq!(workspace["items"][0]["version"], 7);
    assert_eq!(
        workspace_report(
            &harness,
            vec![PreviousResultId {
                uri: uri.clone(),
                value: quarantine_id
            }]
        )?["items"][0]["kind"],
        "unchanged"
    );

    let missing = parity_uri("no-longer-current.rs")?;
    let cleared = workspace_report(
        &harness,
        vec![PreviousResultId {
            uri: missing.clone(),
            value: previous,
        }],
    )?;
    let items = cleared["items"]
        .as_array()
        .ok_or("workspace items absent")?;
    let missing_report = items
        .iter()
        .find(|item| item["uri"] == missing.as_str())
        .ok_or("previous URI not cleared")?;
    assert_eq!(missing_report["kind"], "full");
    assert_eq!(missing_report["items"], serde_json::json!([]));
    Ok(())
}

#[test]
fn omission_payload_uses_stored_uri_for_alias_requests() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    let uri = parity_uri("alias.rs")?;
    let alias: Uri = "file:///workspace/src/%61lias.rs"
        .parse()
        .map_err(|error| format!("alias URI: {error}"))?;
    commit(
        harness.service.inner(),
        bounded_workspace(
            &uri,
            2,
            &DiagnosticBudget {
                max_items_per_document: 1,
                ..Default::default()
            },
        ),
    )?;
    let ordinary = document_report(&harness, &uri, None)?;
    let aliased = document_report(&harness, &alias, None)?;
    assert_eq!(ordinary["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(aliased["items"], ordinary["items"]);
    assert_eq!(aliased["resultId"], ordinary["resultId"]);
    Ok(())
}

#[test]
fn omission_profile_only_and_unavailable_controls_have_no_marker() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    let uri = parity_uri("fallback.rs")?;
    let workspace = parity_workspace_diagnostics(vec![(
        uri.clone(),
        vec![parity_diagnostic("gap:withheld", false)],
    )]);
    let snapshot = commit(harness.service.inner(), workspace)?;
    assert!(stored_selection(&snapshot)?.document_omissions().is_empty());
    assert_eq!(
        document_report(&harness, &uri, None)?["items"],
        serde_json::json!([])
    );
    assert_eq!(
        workspace_report(&harness, Vec::new())?["items"][0]["items"],
        serde_json::json!([])
    );

    let snapshot = commit(
        harness.service.inner(),
        bounded_workspace(
            &uri,
            2,
            &DiagnosticBudget {
                max_items_per_document: 0,
                ..Default::default()
            },
        ),
    )?;
    assert!(matches!(
        stored_selection(&snapshot)?.outcome,
        DiagnosticDeliveryOutcome::Unavailable { .. }
    ));
    let document = document_report(&harness, &uri, None)?;
    let items = document["items"]
        .as_array()
        .ok_or("fallback items missing")?;
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|item| item["data"]["diagnostic_id"].is_string())
    );
    assert_eq!(
        workspace_report(&harness, Vec::new())?["items"][0]["items"],
        document["items"]
    );
    Ok(())
}

// Read the actual framed publishDiagnostics notification, never a recreated
// push projection. The production quarantine-restoration path publishes the
// committed selection; the ordinary refresh and rollback share its marker.
#[test]
fn omission_actual_push_document_and_workspace_payload_parity() -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt};
    let harness = parity_backend()?;
    let runtime = harness.runtime;
    runtime.block_on(async {
        let (client_io, server_io) = tokio::io::duplex(128 * 1024);
        let (server_read, server_write) = tokio::io::split(server_io);
        // Drive the real client socket while retaining the tested backend for
        // precise snapshot injection. The idle service receives no requests.
        let (idle_service, _idle_socket) =
            LspService::new(|client| Backend::new(client, PathBuf::from(".")));
        let server = tokio::spawn(async move {
            tower_lsp_server::Server::new(server_read, server_write, harness._socket)
                .serve(idle_service)
                .await;
        });
        let mut reader = tokio::io::BufReader::new(client_io);
        let mut service = harness.service;
        let init = tower_lsp_server::jsonrpc::Request::build("initialize")
            .id(1)
            .params(serde_json::json!({"capabilities": {}, "rootUri": "file:///workspace"}))
            .finish();
        let initialization = tower::Service::call(&mut service, init)
            .await
            .map_err(|error| error.to_string())?;
        assert!(
            initialization.is_some_and(|response| response.is_ok()),
            "initialize must succeed"
        );
        let outcome = async {
            let uri = parity_uri("wire.rs")?;
            for (budget, count, expected_selected, expected_scope) in [
                (DiagnosticBudget::default(), 51, 50, "document"),
                (
                    DiagnosticBudget {
                        max_items_per_document: 2,
                        max_items_per_workspace_response: 2,
                        ..Default::default()
                    },
                    2,
                    1,
                    "workspace",
                ),
                (
                    DiagnosticBudget {
                        max_serialized_bytes: 1,
                        ..Default::default()
                    },
                    2,
                    1,
                    "workspace",
                ),
                (
                    DiagnosticBudget {
                        max_items_per_document: 1,
                        max_items_per_workspace_response: 1,
                        ..Default::default()
                    },
                    1,
                    0,
                    "workspace",
                ),
                (DiagnosticBudget::default(), 1, 1, "document"),
            ] {
                let mut fixture = bounded_workspace(&uri, count, &budget);
                let workspace_limited = budget.max_items_per_workspace_response <= 2;
                if workspace_limited {
                    let other_uri = parity_uri("earlier.rs")?;
                    let other = vec![parity_diagnostic("gap:a-first", true)];
                    fixture
                        .snapshot
                        .diagnostics_by_uri
                        .insert(other_uri.clone(), other.clone());
                    fixture.batches.push(DiagnosticBatch {
                        uri: other_uri,
                        diagnostics: other,
                    });
                    fixture.snapshot.delivery_selection =
                        Some(Arc::new(DiagnosticDeliverySelection::evaluate(
                            &fixture.snapshot.diagnostics_by_uri,
                            &budget,
                            "fixed-profile",
                            "fixed-evidence",
                        )));
                }
                let snapshot = commit(service.inner(), fixture)?;
                assert_eq!(
                    applied_result(stored_selection(&snapshot)?)?.selected.len(),
                    expected_selected + usize::from(workspace_limited)
                );
                service
                    .inner()
                    .restore_document_diagnostics(&uri, false, None)
                    .await;
                let publication = tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        let mut length = None;
                        loop {
                            let mut line = String::new();
                            if reader
                                .read_line(&mut line)
                                .await
                                .map_err(|error| error.to_string())?
                                == 0
                            {
                                return Err("wire ended before publication".to_string());
                            }
                            if line == "\r\n" {
                                break;
                            }
                            if let Some(value) = line.strip_prefix("Content-Length:") {
                                length = Some(
                                    value
                                        .trim()
                                        .parse::<usize>()
                                        .map_err(|error| error.to_string())?,
                                );
                            }
                        }
                        let mut bytes = vec![0; length.ok_or("missing frame length")?];
                        reader
                            .read_exact(&mut bytes)
                            .await
                            .map_err(|error| error.to_string())?;
                        let message: serde_json::Value =
                            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                        if message["method"] == "textDocument/publishDiagnostics" {
                            return Ok(message);
                        }
                    }
                })
                .await
                .map_err(|error| error.to_string())??;
                assert_eq!(publication["params"]["uri"], uri.as_str());
                let document = service
                    .inner()
                    .diagnostic(DocumentDiagnosticParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        identifier: None,
                        previous_result_id: None,
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await
                    .map_err(|error| error.to_string())?;
                let document = serde_json::to_value(document).map_err(|error| error.to_string())?;
                let workspace = service
                    .inner()
                    .workspace_diagnostic(WorkspaceDiagnosticParams {
                        identifier: None,
                        previous_result_ids: Vec::new(),
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await
                    .map_err(|error| error.to_string())?;
                let workspace =
                    serde_json::to_value(workspace).map_err(|error| error.to_string())?;
                let push_items = &publication["params"]["diagnostics"];
                assert_eq!(
                    push_items.as_array().map(Vec::len),
                    Some(expected_selected + usize::from(count > expected_selected))
                );
                assert_eq!(
                    document["items"], *push_items,
                    "document pull must match actual push payload"
                );
                let workspace_item = workspace["items"]
                    .as_array()
                    .ok_or("workspace items missing")?
                    .iter()
                    .find(|item| item["uri"] == uri.as_str())
                    .ok_or("workspace document missing")?;
                assert_eq!(
                    workspace_item["items"], *push_items,
                    "workspace pull must match actual push payload"
                );
                assert_eq!(workspace_item["resultId"], document["resultId"]);
                assert_eq!(
                    snapshot.diagnostic_count(),
                    count + usize::from(workspace_limited),
                    "marker must not become evidence"
                );
                if count == expected_selected {
                    assert!(
                        push_items
                            .as_array()
                            .ok_or("push items missing")?
                            .iter()
                            .all(|item| item["data"]["diagnostic_id"].is_string())
                    );
                    continue;
                }
                let marker = &push_items[expected_selected];
                assert_eq!(marker["code"], "ripr-diagnostic-budget-omitted");
                assert_eq!(marker["data"]["scope"], expected_scope);
                assert_eq!(marker["data"]["selected_count"], expected_selected);
                assert_eq!(marker["data"]["omitted_count"], count - expected_selected);
                assert_eq!(marker["data"]["total_count"], count);
                assert_eq!(marker["data"]["byte_budget"], budget.max_serialized_bytes);
                assert_eq!(
                    marker["data"]["count_budget"],
                    if expected_scope == "document" {
                        budget.max_items_per_document
                    } else {
                        budget.max_items_per_workspace_response
                    }
                );
                assert_eq!(
                    marker["data"]["complete_evidence_identity"],
                    "fixed-evidence"
                );
                assert!(
                    marker["data"]["retrieval_route"]
                        .as_str()
                        .is_some_and(|route| !route.is_empty())
                );
            }
            Ok::<_, String>(())
        }
        .await;
        server.abort();
        let _ = server.await;
        outcome
    })
}
