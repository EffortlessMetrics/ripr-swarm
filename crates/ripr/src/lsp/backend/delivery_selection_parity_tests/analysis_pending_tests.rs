use super::omission_tests::{document_report, drained_parity_backend, result_id, workspace_report};
use super::*;
use crate::lsp::diagnostic_catalog::DIAGNOSTIC_ANALYSIS_PENDING_CODE;
use tower_lsp_server::ls_types::PreviousResultId;

const ANALYSIS_PENDING_RESULT_ID: &str = "ripr:analysis-pending:save_or_refresh";

/// A fresh `Backend` starts with the root authority awaiting
/// initialization, which is a blocked state. Tests pinning the ordinary
/// cold-start route select a root first, mirroring a completed
/// `initialize` with a workspace root.
fn select_root(harness: &ParityHarness) {
    harness
        .runtime
        .block_on(harness.service.inner().apply_workspace_root_authority(
            WorkspaceRootAuthority::selected(PathBuf::from("/workspace")),
        ));
}

fn assert_pending_marker(items: &serde_json::Value) -> Result<(), String> {
    assert_pending_marker_with_recovery(items, "save_or_refresh", "Save a file")
}

fn assert_pending_marker_with_recovery(
    items: &serde_json::Value,
    recovery: &str,
    route_hint: &str,
) -> Result<(), String> {
    let items = items
        .as_array()
        .ok_or_else(|| format!("items missing: {items}"))?;
    if items.len() != 1 {
        return Err(format!(
            "expected exactly one analysis-pending marker, got {items:?}"
        ));
    }
    let marker = &items[0];
    if marker["severity"] != 3 {
        return Err(format!(
            "marker must be Information (3), got {}",
            marker["severity"]
        ));
    }
    if marker["code"] != DIAGNOSTIC_ANALYSIS_PENDING_CODE {
        return Err(format!("wrong marker code: {marker}"));
    }
    if marker["source"] != "ripr" {
        return Err(format!("marker source must be ripr: {marker}"));
    }
    if marker["data"]["kind"] != "delivery_limitation" {
        return Err(format!("marker must be a delivery limitation: {marker}"));
    }
    if marker["data"]["state"] != "no_snapshot" {
        return Err(format!("wrong marker state: {marker}"));
    }
    if marker["data"]["recovery"] != recovery {
        return Err(format!("wrong marker recovery: {marker}"));
    }
    if marker["data"]["diagnostic_id"].is_string() {
        return Err(format!(
            "pending marker must never carry a finding identity: {marker}"
        ));
    }
    let message = marker["message"]
        .as_str()
        .ok_or_else(|| format!("marker message missing: {marker}"))?;
    if !message.contains("not analyzed") {
        return Err(format!(
            "marker must name the never-analyzed state: {message}"
        ));
    }
    if !message.contains("not evidence of a clean workspace") {
        return Err(format!(
            "marker must refuse the clean-workspace claim: {message}"
        ));
    }
    if !message.contains(route_hint) {
        return Err(format!(
            "marker must name the actual recovery route: {message}"
        ));
    }
    if recovery == "save_or_refresh" && !message.contains("ripr.refresh") {
        return Err(format!(
            "unblocked route must name `ripr.refresh`: {message}"
        ));
    }
    if recovery != "save_or_refresh" && message.contains("ripr.refresh") {
        return Err(format!(
            "blocked state must not advertise save/refresh, which returns before analysis: {message}"
        ));
    }
    Ok(())
}

#[test]
fn analysis_pending_document_pull_serves_typed_marker_instead_of_empty_report() -> Result<(), String>
{
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("pending.rs")?;
    let report = document_report(&harness, &uri, None)?;
    assert_eq!(result_id(&report)?, ANALYSIS_PENDING_RESULT_ID);
    assert_pending_marker(&report["items"])
}

#[test]
fn analysis_pending_document_pull_answers_unchanged_for_the_cached_pending_id() -> Result<(), String>
{
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("pending.rs")?;
    let first = document_report(&harness, &uri, None)?;
    let pending_id = result_id(&first)?;
    let second = document_report(&harness, &uri, Some(pending_id.clone()))?;
    assert_eq!(second["kind"], "unchanged");
    assert_eq!(result_id(&second)?, pending_id);
    assert!(
        second["items"].is_null(),
        "an unchanged report carries no items: {second}"
    );
    Ok(())
}

#[test]
fn analysis_pending_workspace_pull_serves_marker_per_tracked_uri() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("pending.rs")?;
    let cold = workspace_report(&harness, Vec::new())?;
    assert_eq!(
        cold["items"],
        serde_json::json!([]),
        "a cold client has no document URI to attach the marker to: {cold}"
    );

    let tracked = workspace_report(
        &harness,
        vec![PreviousResultId {
            uri: uri.clone(),
            value: String::new(),
        }],
    )?;
    let items = tracked["items"]
        .as_array()
        .ok_or_else(|| format!("workspace items missing: {tracked}"))?;
    if items.len() != 1 {
        return Err(format!("expected one pending item, got {tracked}"));
    }
    if items[0]["uri"] != serde_json::json!(uri.to_string()) {
        return Err(format!("pending item must ride the tracked URI: {tracked}"));
    }
    assert_eq!(
        items[0]["resultId"],
        serde_json::json!(ANALYSIS_PENDING_RESULT_ID)
    );
    assert_pending_marker(&items[0]["items"])?;

    let cached = workspace_report(
        &harness,
        vec![PreviousResultId {
            uri: uri.clone(),
            value: ANALYSIS_PENDING_RESULT_ID.to_string(),
        }],
    )?;
    let cached_items = cached["items"]
        .as_array()
        .ok_or_else(|| format!("workspace items missing: {cached}"))?;
    if cached_items.len() != 1 {
        return Err(format!("expected one unchanged item, got {cached}"));
    }
    assert_eq!(cached_items[0]["kind"], "unchanged");
    assert_eq!(
        cached_items[0]["resultId"],
        serde_json::json!(ANALYSIS_PENDING_RESULT_ID)
    );
    Ok(())
}

#[test]
fn analysis_pending_marker_retracts_when_the_first_snapshot_commits() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("pending.rs")?;
    let pending = document_report(&harness, &uri, None)?;
    assert_pending_marker(&pending["items"])?;

    // A zero-finding commit on another document is the state the empty
    // report used to claim: normal serving must resume with the real
    // (empty) set for the previously pending URI under a fresh, non-pending
    // identity, and the cached pending id must not be able to hide the
    // transition behind `unchanged`.
    let other_uri = parity_uri("committed.rs")?;
    commit(
        harness.service.inner(),
        parity_workspace_diagnostics(vec![(other_uri, Vec::new())]),
    )?;
    let served = document_report(&harness, &uri, None)?;
    let served_id = result_id(&served)?;
    if served_id == ANALYSIS_PENDING_RESULT_ID {
        return Err(format!(
            "committed snapshot must serve a fresh identity, got {served:?}"
        ));
    }
    assert_eq!(
        served["items"],
        serde_json::json!([]),
        "zero findings after a real commit is an honest empty set: {served}"
    );
    let transitioned =
        document_report(&harness, &uri, Some(ANALYSIS_PENDING_RESULT_ID.to_string()))?;
    assert_eq!(transitioned["kind"], "full");
    assert_eq!(result_id(&transitioned)?, served_id);
    Ok(())
}

#[test]
fn analysis_pending_blocked_root_names_its_own_recovery_route() -> Result<(), String> {
    // #7774 review: `refresh_diagnostics` returns before analysis while the
    // root authority blocks it, so the pending marker must not advertise
    // save/refresh in that state — it names the authority's own recovery
    // route, and the result identity folds the route so a later unblock
    // cannot hide behind `unchanged`.
    let harness = drained_parity_backend()?;
    let uri = parity_uri("pending.rs")?;
    harness
        .runtime
        .block_on(harness.service.inner().apply_workspace_root_authority(
            WorkspaceRootAuthority::unavailable("test root unavailable"),
        ));
    let report = document_report(&harness, &uri, None)?;
    assert_eq!(
        result_id(&report)?,
        "ripr:analysis-pending:select_root_and_restart"
    );
    assert_pending_marker_with_recovery(
        &report["items"],
        "select_root_and_restart",
        "Select a single workspace root",
    )?;

    // Restoring the root re-opens the ordinary save/refresh route under a
    // fresh identity, so a client holding the blocked-state report sees the
    // transition instead of a stale route.
    harness
        .runtime
        .block_on(harness.service.inner().apply_workspace_root_authority(
            WorkspaceRootAuthority::selected(PathBuf::from("/workspace")),
        ));
    let restored = document_report(
        &harness,
        &uri,
        Some("ripr:analysis-pending:select_root_and_restart".to_string()),
    )?;
    assert_eq!(restored["kind"], "full");
    assert_eq!(result_id(&restored)?, ANALYSIS_PENDING_RESULT_ID);
    assert_pending_marker(&restored["items"])
}
