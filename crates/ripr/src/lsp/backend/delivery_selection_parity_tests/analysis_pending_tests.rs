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
    let names_state = if recovery == "analysis_running" {
        message.contains("analysis is in progress")
    } else {
        message.contains("not analyzed")
    };
    if !names_state {
        return Err(format!(
            "marker must name the pre-snapshot state: {message}"
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

/// #14447: while an analysis attempt is queued or running the marker says
/// so and does not ask the user to start one; once the attempt ends without
/// a snapshot, the start route returns under its own result id.
#[test]
fn analysis_pending_marker_names_a_running_attempt_instead_of_asking_to_start_one()
-> Result<(), String> {
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("pending.rs")?;
    for state in [AnalysisAttemptState::Queued, AnalysisAttemptState::Running] {
        harness
            .service
            .inner()
            .set_analysis_attempt_state_for_test(state);
        let report = document_report(&harness, &uri, None)?;
        assert_eq!(
            result_id(&report)?,
            "ripr:analysis-pending:analysis_running"
        );
        assert_pending_marker_with_recovery(
            &report["items"],
            "analysis_running",
            "when it finishes",
        )?;
    }
    harness
        .service
        .inner()
        .set_analysis_attempt_state_for_test(AnalysisAttemptState::Failed);
    let report = document_report(&harness, &uri, None)?;
    assert_eq!(result_id(&report)?, ANALYSIS_PENDING_RESULT_ID);
    assert_pending_marker(&report["items"])
}

/// A failed attempt with no snapshot names its recorded failure and fix
/// instead of the cold-start route: a bare save/refresh fails the same way
/// (for example outside a Git work tree). A different failure gets a
/// different result id so the client never holds stale reason text, and a
/// `failed` state with no recorded failure keeps the start route.
#[test]
fn analysis_pending_marker_names_the_recorded_failure_after_a_failed_attempt() -> Result<(), String>
{
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("pending.rs")?;
    let not_git = "workspace analysis failed: This root is not inside a Git work tree (the analysis did not run). Run from inside a repository.";
    harness
        .service
        .inner()
        .set_analysis_attempt_failure_for_test(AnalysisFailureKind::AnalysisError, not_git);
    let report = document_report(&harness, &uri, None)?;
    let failed_id = result_id(&report)?;
    if !failed_id.starts_with("ripr:analysis-pending:fix_failure_then_refresh:") {
        return Err(format!(
            "failed attempt must carry its own result id: {failed_id}"
        ));
    }
    let marker = report["items"]
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| format!("marker missing: {report}"))?;
    if marker["code"] != DIAGNOSTIC_ANALYSIS_PENDING_CODE
        || marker["data"]["state"] != "no_snapshot"
        || marker["data"]["recovery"] != "fix_failure_then_refresh"
        || marker["data"]["failure_kind"] != "analysis_error"
    {
        return Err(format!("failed-attempt marker shape drifted: {marker}"));
    }
    let message = marker["message"].as_str().unwrap_or_default();
    for needle in [
        "not inside a Git work tree",
        "Run from inside a repository",
        "not evidence of a clean workspace",
        "Fix that cause",
    ] {
        if !message.contains(needle) {
            return Err(format!("marker must contain {needle:?}: {message}"));
        }
    }
    if message.contains("has not analyzed this workspace yet")
        || message.contains("to start analysis")
    {
        return Err(format!(
            "a failed attempt must not read as cold start: {message}"
        ));
    }
    if result_id(&document_report(&harness, &uri, Some(failed_id.clone()))?)? != failed_id {
        return Err("the same failure must keep its result id".to_string());
    }
    // Same kind, different detail: the result id folds the detail too, so
    // two distinct analysis errors never share a cached marker.
    harness
        .service
        .inner()
        .set_analysis_attempt_failure_for_test(
            AnalysisFailureKind::AnalysisError,
            "workspace analysis failed: could not read the Git index.",
        );
    let same_kind = document_report(&harness, &uri, Some(failed_id.clone()))?;
    let same_kind_id = result_id(&same_kind)?;
    if same_kind_id == failed_id
        || !same_kind["items"][0]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("could not read the Git index")
    {
        return Err(format!(
            "a same-kind failure with new detail must re-render: {same_kind}"
        ));
    }
    harness
        .service
        .inner()
        .set_analysis_attempt_failure_for_test(
            AnalysisFailureKind::BaseUnresolvable,
            "could not resolve a default base. This repository has no commits yet; commit once.",
        );
    let other = document_report(&harness, &uri, Some(failed_id.clone()))?;
    let other_id = result_id(&other)?;
    if other_id == failed_id || other["items"][0]["data"]["failure_kind"] != "base_unresolvable" {
        return Err(format!("a different failure must re-render: {other}"));
    }
    harness
        .service
        .inner()
        .set_analysis_attempt_state_for_test(AnalysisAttemptState::Running);
    let running = document_report(&harness, &uri, None)?;
    assert_eq!(
        result_id(&running)?,
        "ripr:analysis-pending:analysis_running"
    );
    Ok(())
}

/// #15461 review: a push-client save that starts a retry after a failed
/// attempt re-sends "running", not the failure's "fix that cause" route,
/// while a blocked route still wins and nothing changes when no attempt is
/// starting.
#[test]
fn push_marker_recovery_shows_running_when_a_retry_starts_after_a_failure() {
    let failed = AnalysisPendingRecovery::AnalysisFailed {
        kind: AnalysisFailureKind::AnalysisError,
        detail: "workspace analysis failed: not a Git work tree.".to_string(),
    };
    assert_eq!(
        Backend::push_marker_recovery(failed.clone(), true),
        AnalysisPendingRecovery::AnalysisRunning
    );
    assert_eq!(Backend::push_marker_recovery(failed.clone(), false), failed);
    assert_eq!(
        Backend::push_marker_recovery(AnalysisPendingRecovery::SaveOrRefresh, true),
        AnalysisPendingRecovery::AnalysisRunning
    );
    assert_eq!(
        Backend::push_marker_recovery(AnalysisPendingRecovery::FixConfiguration, true),
        AnalysisPendingRecovery::FixConfiguration
    );
}

/// #14447 review: a queued or running attempt never hides a blocked route.
/// While the root authority or a latched configuration failure blocks the
/// next refresh, the marker keeps naming that block, not "in progress".
#[test]
fn analysis_pending_blocked_routes_win_over_a_running_attempt() -> Result<(), String> {
    let harness = drained_parity_backend()?;
    let uri = parity_uri("pending.rs")?;
    harness
        .service
        .inner()
        .set_analysis_attempt_state_for_test(AnalysisAttemptState::Running);
    harness
        .runtime
        .block_on(harness.service.inner().apply_workspace_root_authority(
            WorkspaceRootAuthority::unavailable("test root unavailable"),
        ));
    let report = document_report(&harness, &uri, None)?;
    assert_eq!(
        result_id(&report)?,
        "ripr:analysis-pending:use_single_workspace_root_and_restart"
    );
    select_root(&harness);
    harness
        .service
        .inner()
        .set_analysis_attempt_state_for_test(AnalysisAttemptState::Running);
    harness
        .service
        .inner()
        .set_configuration_failure("changed ripr.toml input is invalid");
    let report = document_report(&harness, &uri, None)?;
    assert_eq!(
        result_id(&report)?,
        "ripr:analysis-pending:fix_configuration"
    );
    Ok(())
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
        "ripr:analysis-pending:use_single_workspace_root_and_restart"
    );
    assert_pending_marker_with_recovery(
        &report["items"],
        "use_single_workspace_root_and_restart",
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
        Some("ripr:analysis-pending:use_single_workspace_root_and_restart".to_string()),
    )?;
    assert_eq!(restored["kind"], "full");
    assert_eq!(result_id(&restored)?, ANALYSIS_PENDING_RESULT_ID);
    assert_pending_marker(&restored["items"])
}

const WITHDRAWN_FIX_CONFIGURATION_RESULT_ID: &str =
    "ripr:analysis-pending:fix_configuration:withdrawn";

/// The #8731 withdrawal marker: a configuration failure latched after a
/// snapshot committed must serve a typed marker that names the pause, not
/// the never-analyzed state (a snapshot exists and stays retained).
fn assert_withdrawal_marker(items: &serde_json::Value) -> Result<(), String> {
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
    if marker["data"]["kind"] != "delivery_limitation" {
        return Err(format!("marker must be a delivery limitation: {marker}"));
    }
    if marker["data"]["state"] != "withdrawn_configuration_failure" {
        return Err(format!(
            "withdrawal must name its state, not the never-analyzed one: {marker}"
        ));
    }
    if marker["data"]["recovery"] != "fix_configuration" {
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
    if !message.contains("withdrawn") {
        return Err(format!(
            "marker must name the withdrawal, not a never-analyzed state: {message}"
        ));
    }
    if !message.contains("not evidence of a clean workspace") {
        return Err(format!(
            "marker must refuse the clean-workspace claim: {message}"
        ));
    }
    if !message.contains("Fix the workspace configuration") {
        return Err(format!(
            "marker must name the actual recovery route: {message}"
        ));
    }
    Ok(())
}

#[test]
fn analysis_pending_document_pull_withdraws_committed_snapshot_when_configuration_failure_latched()
-> Result<(), String> {
    // #8731: a configuration failure latched after a snapshot committed left
    // the pre-failure findings served at full weight with no in-editor signal
    // that analysis is paused. The committed snapshot stays retained (the
    // retrieval routes can still read it), but the pull handlers must serve
    // the typed pending marker under the FixConfiguration recovery route
    // instead of the stale payload.
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("committed.rs")?;
    commit(
        harness.service.inner(),
        parity_workspace_diagnostics(vec![(
            uri.clone(),
            vec![parity_diagnostic("gap:committed", true)],
        )]),
    )?;
    let served = document_report(&harness, &uri, None)?;
    let served_id = result_id(&served)?;
    if served_id == WITHDRAWN_FIX_CONFIGURATION_RESULT_ID {
        return Err(format!(
            "pre-failure serving must not use the pending identity: {served_id}"
        ));
    }

    harness
        .service
        .inner()
        .set_configuration_failure("changed ripr.toml input is invalid");
    let withdrawn = document_report(&harness, &uri, None)?;
    assert_eq!(
        result_id(&withdrawn)?,
        WITHDRAWN_FIX_CONFIGURATION_RESULT_ID
    );
    assert_withdrawal_marker(&withdrawn["items"])?;
    // The stale served identity must not answer `unchanged` behind the
    // withdrawal: the client re-renders the marker instead of keeping the
    // pre-failure payload.
    let transitioned = document_report(&harness, &uri, Some(served_id))?;
    assert_eq!(transitioned["kind"], "full");
    assert_eq!(
        result_id(&transitioned)?,
        WITHDRAWN_FIX_CONFIGURATION_RESULT_ID
    );
    Ok(())
}

#[test]
fn analysis_pending_workspace_pull_withdraws_committed_snapshot_when_configuration_failure_latched()
-> Result<(), String> {
    // #8731: the workspace pull path serves the same withdrawal marker per
    // tracked URI; a cached pending identity answers `unchanged` while the
    // stale snapshot identity re-renders.
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("committed.rs")?;
    commit(
        harness.service.inner(),
        parity_workspace_diagnostics(vec![(
            uri.clone(),
            vec![parity_diagnostic("gap:committed", true)],
        )]),
    )?;
    harness
        .service
        .inner()
        .set_configuration_failure("changed ripr.toml input is invalid");
    let report = workspace_report(
        &harness,
        vec![PreviousResultId {
            uri: uri.clone(),
            value: String::new(),
        }],
    )?;
    let items = report["items"]
        .as_array()
        .ok_or_else(|| format!("workspace items missing: {report}"))?;
    if items.len() != 1 {
        return Err(format!("expected one withdrawal item, got {report}"));
    }
    if items[0]["uri"] != serde_json::json!(uri.to_string()) {
        return Err(format!(
            "withdrawal item must ride the tracked URI: {report}"
        ));
    }
    assert_eq!(
        items[0]["resultId"],
        serde_json::json!(WITHDRAWN_FIX_CONFIGURATION_RESULT_ID)
    );
    assert_withdrawal_marker(&items[0]["items"])?;

    let cached = workspace_report(
        &harness,
        vec![PreviousResultId {
            uri: uri.clone(),
            value: WITHDRAWN_FIX_CONFIGURATION_RESULT_ID.to_string(),
        }],
    )?;
    let cached_items = cached["items"]
        .as_array()
        .ok_or_else(|| format!("workspace items missing: {cached}"))?;
    if cached_items.len() != 1 {
        return Err(format!("expected one unchanged item, got {cached}"));
    }
    assert_eq!(cached_items[0]["kind"], "unchanged");
    Ok(())
}

/// #15312: a refresh that fails because the requested base does not resolve
/// withdraws the retained snapshot on both pull paths. Its findings answer an
/// earlier base, so serving them unmarked would read as the current answer.
/// Any other failure keeps the retained snapshot in service, and so does a
/// retry that is already running.
#[test]
fn unresolvable_base_failure_withdraws_the_retained_snapshot_and_other_failures_do_not()
-> Result<(), String> {
    let harness = drained_parity_backend()?;
    select_root(&harness);
    let uri = parity_uri("committed.rs")?;
    commit(
        harness.service.inner(),
        parity_workspace_diagnostics(vec![(
            uri.clone(),
            vec![parity_diagnostic("gap:committed", true)],
        )]),
    )?;
    let served = document_report(&harness, &uri, None)?;
    let served_id = result_id(&served)?;

    // Discriminating negative: a failure that leaves the question unchanged
    // (same base, older save) keeps the committed payload.
    harness
        .service
        .inner()
        .set_analysis_attempt_failure_for_test(
            AnalysisFailureKind::AnalysisError,
            "workspace analysis failed: could not read the Git index.",
        );
    let retained = document_report(&harness, &uri, None)?;
    if result_id(&retained)? != served_id || retained["items"] != served["items"] {
        return Err(format!(
            "a non-base failure must keep the retained snapshot in service: {retained}"
        ));
    }

    let detail = "workspace analysis failed: the base `no-such-ref` does not resolve to a commit (the analysis did not run).";
    harness
        .service
        .inner()
        .set_analysis_attempt_failure_for_test(AnalysisFailureKind::BaseUnresolvable, detail);
    let withdrawn = document_report(&harness, &uri, Some(served_id.clone()))?;
    assert_eq!(withdrawn["kind"], "full");
    let withdrawn_id = result_id(&withdrawn)?;
    if !withdrawn_id.starts_with("ripr:analysis-pending:fix_failure_then_refresh:")
        || !withdrawn_id.ends_with(":withdrawn")
    {
        return Err(format!(
            "withdrawal must carry its own result id: {withdrawn_id}"
        ));
    }
    let items = withdrawn["items"]
        .as_array()
        .ok_or_else(|| format!("items missing: {withdrawn}"))?;
    let [marker] = items.as_slice() else {
        return Err(format!("exactly one withdrawal marker expected: {items:?}"));
    };
    if marker["code"] != DIAGNOSTIC_ANALYSIS_PENDING_CODE
        || marker["severity"] != 3
        || marker["data"]["kind"] != "delivery_limitation"
        || marker["data"]["state"] != "withdrawn_analysis_failure"
        || marker["data"]["recovery"] != "fix_failure_then_refresh"
        || marker["data"]["failure_kind"] != "base_unresolvable"
        || marker["data"]["diagnostic_id"].is_string()
    {
        return Err(format!("withdrawal marker shape drifted: {marker}"));
    }
    let message = marker["message"].as_str().unwrap_or_default();
    for needle in [
        "previously reported findings are withdrawn",
        "not evidence of a clean workspace",
        "the base `no-such-ref` does not resolve",
        "Fix that cause",
    ] {
        if !message.contains(needle) {
            return Err(format!("marker must contain {needle:?}: {message}"));
        }
    }
    if message.contains("analysis is paused") || message.contains("no findings are reported") {
        return Err(format!(
            "the marker must not read as a configuration pause or a cold start: {message}"
        ));
    }

    // The workspace pull serves the same marker for the tracked URI.
    let report = workspace_report(
        &harness,
        vec![PreviousResultId {
            uri: uri.clone(),
            value: served_id.clone(),
        }],
    )?;
    let workspace_items = report["items"]
        .as_array()
        .ok_or_else(|| format!("workspace items missing: {report}"))?;
    let [item] = workspace_items.as_slice() else {
        return Err(format!("expected one withdrawal item, got {report}"));
    };
    if item["resultId"] != serde_json::json!(withdrawn_id)
        || item["items"][0]["data"]["state"] != "withdrawn_analysis_failure"
    {
        return Err(format!(
            "workspace pull must serve the withdrawal marker: {report}"
        ));
    }

    // A retry in progress no longer holds a recorded failure, so the
    // retained payload is served again until that attempt ends.
    harness
        .service
        .inner()
        .set_analysis_attempt_state_for_test(AnalysisAttemptState::Running);
    let retrying = document_report(&harness, &uri, Some(withdrawn_id))?;
    assert_eq!(retrying["kind"], "full");
    if result_id(&retrying)? != served_id {
        return Err(format!(
            "a running retry must serve the retained snapshot: {retrying}"
        ));
    }
    Ok(())
}
