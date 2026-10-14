use super::omission_tests::{document_report, drained_parity_backend, result_id, workspace_report};
use super::*;
use crate::lsp::actions::code_action_response;
use crate::lsp::client_features::{ClientFeatureProfile, RiprEditorClientCapabilities};
use crate::lsp::diagnostic_budget::{DiagnosticBudget, DiagnosticDeliverySelection};
use crate::lsp::diagnostic_catalog::DIAGNOSTIC_QUARANTINE_WITHDRAWN_CODE;
use crate::lsp::state::DocumentStalenessReason;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp_server::ls_types::{
    CodeActionContext, CodeActionOrCommand, CodeActionParams, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, PreviousResultId, Range, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, VersionedTextDocumentIdentifier,
};

const CLEAN_TEXT: &str = "fn clean() {}\n";
const DIRTY_TEXT: &str = "fn dirty() {}\n";
const COPY_CONTEXT_COMMAND: &str = "ripr.copyContext";
const REFRESH_ANALYSIS_TITLE: &str = "Refresh Analysis - Saved Workspace Check";

fn assert_quarantine_marker(
    items: &serde_json::Value,
    reason: &str,
    recovery: &str,
) -> Result<(), String> {
    let items = items
        .as_array()
        .ok_or_else(|| format!("items missing: {items}"))?;
    if items.len() != 1 {
        return Err(format!(
            "expected exactly one withdrawal marker, got {items:?}"
        ));
    }
    let marker = &items[0];
    if marker["severity"] != 3 {
        return Err(format!(
            "marker must be Information (3), got {}",
            marker["severity"]
        ));
    }
    if marker["code"] != DIAGNOSTIC_QUARANTINE_WITHDRAWN_CODE {
        return Err(format!("wrong marker code: {marker}"));
    }
    if marker["source"] != "ripr" {
        return Err(format!("marker source must be ripr: {marker}"));
    }
    if marker["data"]["kind"] != "delivery_limitation" {
        return Err(format!("marker must be a delivery limitation: {marker}"));
    }
    if marker["data"]["reason"] != reason {
        return Err(format!("wrong marker reason: {marker}"));
    }
    if marker["data"]["recovery"] != recovery {
        return Err(format!("wrong marker recovery: {marker}"));
    }
    let message = marker["message"]
        .as_str()
        .ok_or_else(|| format!("marker message missing: {marker}"))?;
    if !message.contains("paused") {
        return Err(format!("marker must name the pause: {message}"));
    }
    if reason == "buffer_diverges_from_analyzed_saved_content"
        && (!message.contains("Save the file") || !message.contains("restore"))
    {
        return Err(format!(
            "diverging buffer must name save to restore: {message}"
        ));
    }
    if items
        .iter()
        .any(|item| item["data"]["diagnostic_id"].is_string())
    {
        return Err(format!(
            "withdrawal must drop saved-state findings, got {items:?}"
        ));
    }
    Ok(())
}

fn disk_backed_uri(name: &str) -> Result<(PathBuf, Uri), String> {
    // Non-Rust extensions use the persisted saved digest, so a matching
    // on-disk file can leave quarantine after the injected snapshot commits.
    let dir = std::env::temp_dir().join("ripr-7771-quarantine-marker");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, CLEAN_TEXT).map_err(|error| error.to_string())?;
    let uri = crate::lsp::uri::file_uri_for_path(&path)?;
    Ok((path, uri))
}

fn open_clean(harness: &ParityHarness, uri: &Uri) {
    harness
        .runtime
        .block_on(harness.service.inner().did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "python".to_string(),
                version: 1,
                text: CLEAN_TEXT.to_string(),
            },
        }));
}

fn change_text(harness: &ParityHarness, uri: &Uri, version: i32, text: &str) {
    harness.runtime.block_on(
        harness
            .service
            .inner()
            .did_change(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.to_string(),
                }],
            }),
    );
}

fn served_workspace(uri: &Uri) -> WorkspaceDiagnostics {
    let mut workspace = parity_workspace_diagnostics(vec![(
        uri.clone(),
        vec![parity_diagnostic("gap:paused-01", true)],
    )]);
    workspace.snapshot.delivery_selection = Some(Arc::new(DiagnosticDeliverySelection::evaluate(
        &workspace.snapshot.diagnostics_by_uri,
        &DiagnosticBudget::default(),
        "fixed-profile",
        "fixed-evidence",
    )));
    workspace
}

#[test]
fn quarantined_pull_and_push_serve_one_information_marker() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    let (_path, uri) = disk_backed_uri("paused.py")?;
    open_clean(&harness, &uri);
    commit(harness.service.inner(), served_workspace(&uri))?;
    let served = document_report(&harness, &uri, None)?;
    assert_eq!(served["kind"], "full");
    assert_eq!(served["items"].as_array().map(Vec::len), Some(1));
    assert!(served["items"][0]["data"]["diagnostic_id"].is_string());
    let served_id = result_id(&served)?;
    if served_id.ends_with(":quarantined") {
        return Err(format!(
            "clean document must not use a quarantined id: {served}"
        ));
    }

    change_text(&harness, &uri, 2, DIRTY_TEXT);
    let state = harness
        .service
        .inner()
        .document_state_for_test(&uri)
        .ok_or_else(|| "expected document state".to_string())?;
    if !state.is_quarantined() {
        return Err("dirty buffer must be quarantined".to_string());
    }

    let document = document_report(&harness, &uri, None)?;
    assert_eq!(document["kind"], "full");
    assert_quarantine_marker(
        &document["items"],
        "buffer_diverges_from_analyzed_saved_content",
        "save_to_restore",
    )?;
    let quarantine_id = result_id(&document)?;
    if !quarantine_id.ends_with(":quarantined") {
        return Err(format!(
            "withdrawn result id must stay distinct: {quarantine_id}"
        ));
    }
    if !quarantine_id.contains("buffer_diverges_from_analyzed_saved_content") {
        return Err(format!(
            "withdrawn result id must fold the reason: {quarantine_id}"
        ));
    }
    assert_ne!(quarantine_id, served_id);
    assert_eq!(
        document_report(&harness, &uri, Some(quarantine_id.clone()))?["kind"],
        "unchanged"
    );
    assert_eq!(
        document_report(&harness, &uri, Some(served_id))?["kind"],
        "full"
    );

    let workspace = workspace_report(&harness, Vec::new())?;
    assert_eq!(workspace["items"][0]["items"], document["items"]);
    assert_eq!(workspace["items"][0]["resultId"], document["resultId"]);
    assert_eq!(workspace["items"][0]["version"], 2);
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

    let last = harness
        .service
        .inner()
        .last_diagnostics_for_uri_for_test(&uri)
        .ok_or_else(|| "push withdrawal must update the client-visible baseline".to_string())?;
    let last_json = serde_json::to_value(last).map_err(|error| error.to_string())?;
    assert_eq!(last_json, document["items"]);
    Ok(())
}

#[test]
fn saving_matching_buffer_retracts_the_marker() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    let (_path, uri) = disk_backed_uri("restore.py")?;
    open_clean(&harness, &uri);
    commit(harness.service.inner(), served_workspace(&uri))?;
    change_text(&harness, &uri, 2, DIRTY_TEXT);
    let withdrawn = document_report(&harness, &uri, None)?;
    assert_quarantine_marker(
        &withdrawn["items"],
        "buffer_diverges_from_analyzed_saved_content",
        "save_to_restore",
    )?;
    let withdrawn_id = result_id(&withdrawn)?;

    change_text(&harness, &uri, 3, CLEAN_TEXT);
    let restored = document_report(&harness, &uri, Some(withdrawn_id))?;
    assert_eq!(restored["kind"], "full");
    assert_eq!(restored["items"].as_array().map(Vec::len), Some(1));
    assert!(
        restored["items"]
            .as_array()
            .ok_or("restored items missing")?
            .iter()
            .all(|item| item["data"]["diagnostic_id"].is_string())
    );
    let restored_id = result_id(&restored)?;
    if restored_id.ends_with(":quarantined") {
        return Err(format!(
            "matching buffer must lift the withdrawal: {restored}"
        ));
    }
    let last = harness
        .service
        .inner()
        .last_diagnostics_for_uri_for_test(&uri)
        .ok_or_else(|| "restore must update the client-visible baseline".to_string())?;
    if last.iter().any(|diagnostic| {
        matches!(
            &diagnostic.code,
            Some(tower_lsp_server::ls_types::NumberOrString::String(code))
                if code == DIAGNOSTIC_QUARANTINE_WITHDRAWN_CODE
        )
    }) {
        return Err("restore must retract the quarantine marker".to_string());
    }
    Ok(())
}

#[test]
fn quarantine_marker_has_no_finding_context_action() -> Result<(), String> {
    let marker = crate::lsp::diagnostics::quarantine_withdrawal_diagnostic(
        DocumentStalenessReason::BufferDivergesFromAnalyzedSavedContent,
    );
    let uri: Uri = "file:///workspace/src/paused.py"
        .parse()
        .map_err(|error| format!("parse URI: {error}"))?;
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range::default(),
        context: CodeActionContext {
            diagnostics: vec![marker],
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    let mut features = ClientFeatureProfile::unsupported();
    features.ripr_editor = Some(RiprEditorClientCapabilities {
        version: "test".to_string(),
        commands: vec![COPY_CONTEXT_COMMAND.to_string()],
        guarded_test_edit: false,
    });
    let actions = code_action_response(&params, None, &features);
    let titles = actions
        .iter()
        .filter_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => Some(action.title.as_str()),
            CodeActionOrCommand::Command(command) => Some(command.title.as_str()),
        })
        .collect::<Vec<_>>();
    if titles != [REFRESH_ANALYSIS_TITLE] {
        return Err(format!(
            "quarantine marker must not mint a finding action, got {titles:?}"
        ));
    }
    Ok(())
}
