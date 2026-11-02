use super::*;
use crate::lsp::state::{CommittedDiagnostics, DiagnosticsGeneration};
use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, Position, Range};

fn generation_diagnostic(uri_index: usize, diagnostic_index: usize) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: diagnostic_index as u32,
                character: 0,
            },
            end: Position {
                line: diagnostic_index as u32,
                character: 80,
            },
        },
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String(format!(
            "ripr-generation-{uri_index}-{diagnostic_index}"
        ))),
        code_description: None,
        source: Some("ripr".to_string()),
        message: format!("generation payload {uri_index}:{diagnostic_index}"),
        related_information: None,
        tags: None,
        data: Some(serde_json::json!({
            "delivery_eligible": true,
            "headline_eligible": true,
            "gap_id": format!("gap:generation:{uri_index}:{diagnostic_index}"),
        })),
    }
}

fn many_uri_workspace(
    uri_count: usize,
    diagnostics_per_uri: usize,
) -> Result<WorkspaceDiagnostics, String> {
    let mut diagnostics_by_uri = BTreeMap::new();
    let mut batches = Vec::new();
    for uri_index in 0..uri_count {
        let uri = test_uri(&format!("file:///workspace/src/gen_{uri_index}.rs"))?;
        let diagnostics = (0..diagnostics_per_uri)
            .map(|diagnostic_index| generation_diagnostic(uri_index, diagnostic_index))
            .collect::<Vec<_>>();
        diagnostics_by_uri.insert(uri.clone(), diagnostics.clone());
        batches.push(DiagnosticBatch { uri, diagnostics });
    }
    let snapshot = sample_analysis_snapshot(
        PathBuf::from("/workspace"),
        test_uri("file:///workspace/src/gen_0.rs")?,
        Vec::new(),
        Vec::new(),
    );
    let snapshot = AnalysisSnapshot {
        diagnostics_by_uri: diagnostics_by_uri.into(),
        ..snapshot
    };
    Ok(WorkspaceDiagnostics { snapshot, batches })
}

fn commit_workspace(backend: &Backend, workspace: WorkspaceDiagnostics) -> Result<(), String> {
    let transaction = backend
        .prepare_refresh_transaction(workspace)
        .ok_or_else(|| "expected refresh transaction".to_string())?;
    let super::super::backend::RefreshTransaction {
        plan,
        snapshot,
        pending_analyzed,
        pending_entered,
        ..
    } = transaction;
    backend
        .commit_refresh_snapshot(snapshot, &plan, &pending_analyzed, &pending_entered)
        .ok_or_else(|| "expected snapshot commit".to_string())?;
    Ok(())
}

fn diagnostic_bytes(diagnostics: &[Diagnostic]) -> Result<String, String> {
    serde_json::to_string(diagnostics).map_err(|err| format!("serialize diagnostics: {err}"))
}

#[test]
fn diagnostics_generation_clone_shares_the_map_and_deep_clone_does_not() -> Result<(), String> {
    let mut by_uri = BTreeMap::new();
    by_uri.insert(
        test_uri("file:///workspace/src/lib.rs")?,
        vec![generation_diagnostic(0, 0)],
    );
    let generation = DiagnosticsGeneration::from_map(by_uri);
    let shared = generation.clone();
    if !generation.ptr_eq(&shared) {
        return Err("Clone must share the immutable map".to_string());
    }
    if generation.generation_id() != shared.generation_id() {
        return Err("shared handles must keep the same generation id".to_string());
    }
    let deep_before = DiagnosticsGeneration::deep_clone_count();
    let owned = generation.deep_clone_map();
    let rebuilt = DiagnosticsGeneration::from_map(owned);
    if DiagnosticsGeneration::deep_clone_count() != deep_before.saturating_add(1) {
        return Err("deep_clone_map must count a full-map clone".to_string());
    }
    if generation.ptr_eq(&rebuilt) {
        return Err("a deep map clone must not share the original allocation".to_string());
    }
    if generation != rebuilt {
        return Err("deep-cloned contents must stay equal".to_string());
    }
    Ok(())
}

#[test]
fn prepare_and_commit_share_one_generation_without_deep_cloning() -> Result<(), String> {
    let (service, _socket) = LspService::new(|client| Backend::new(client, PathBuf::from(".")));
    let backend = service.inner();
    let first = many_uri_workspace(32, 4)?;
    let constructions_before = DiagnosticsGeneration::map_construction_count();
    let deep_before = DiagnosticsGeneration::deep_clone_count();
    let transaction = backend
        .prepare_refresh_transaction(first)
        .ok_or_else(|| "expected first refresh transaction".to_string())?;
    let first_generation = transaction.snapshot.diagnostics_by_uri.clone();
    let super::super::backend::RefreshTransaction {
        plan,
        snapshot,
        pending_analyzed,
        pending_entered,
        ..
    } = transaction;
    backend
        .commit_refresh_snapshot(snapshot, &plan, &pending_analyzed, &pending_entered)
        .ok_or_else(|| "expected first commit".to_string())?;
    if DiagnosticsGeneration::map_construction_count() != constructions_before {
        return Err("prepare/commit must not construct another full diagnostic map".to_string());
    }
    if DiagnosticsGeneration::deep_clone_count() != deep_before {
        return Err("prepare/commit must not deep-clone the diagnostic map".to_string());
    }
    let committed = backend
        .last_diagnostics_generation_for_test()
        .ok_or_else(|| "expected committed generation".to_string())?;
    if !first_generation.ptr_eq(&committed) {
        return Err(
            "commit must share the snapshot generation; a deep map clone fails this check"
                .to_string(),
        );
    }
    let latest = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "expected committed snapshot".to_string())?;
    if !latest.diagnostics_by_uri.ptr_eq(&committed) {
        return Err("snapshot inspection must share the committed generation".to_string());
    }

    let second = many_uri_workspace(32, 4)?;
    let constructions_before_second = DiagnosticsGeneration::map_construction_count();
    let deep_before_second = DiagnosticsGeneration::deep_clone_count();
    let second_transaction = backend
        .prepare_refresh_transaction(second)
        .ok_or_else(|| "expected second refresh transaction".to_string())?;
    if !second_transaction
        .previous_diagnostics
        .generation()
        .ptr_eq(&committed)
    {
        return Err("prepare must Arc-share the previous committed generation".to_string());
    }
    if second_transaction
        .snapshot
        .diagnostics_by_uri
        .ptr_eq(&committed)
    {
        return Err("the next generation must be distinct from the previous one".to_string());
    }
    if DiagnosticsGeneration::map_construction_count() != constructions_before_second
        || DiagnosticsGeneration::deep_clone_count() != deep_before_second
    {
        return Err("the second prepare must not clone the previous map".to_string());
    }
    Ok(())
}

#[test]
fn quarantine_overlay_clears_affected_uris_without_mutating_the_generation() -> Result<(), String> {
    let workspace = many_uri_workspace(16, 3)?;
    let one = test_uri("file:///workspace/src/gen_0.rs")?;
    let many = (0..8)
        .map(|index| test_uri(&format!("file:///workspace/src/gen_{index}.rs")))
        .collect::<Result<Vec<_>, _>>()?;
    let generation = workspace.snapshot.diagnostics_by_uri.clone();
    let overlay_one = CommittedDiagnostics::from_generation_with_cleared(
        generation.clone(),
        std::iter::once(one.clone()),
    );
    if !overlay_one.generation().ptr_eq(&generation) {
        return Err("one-URI quarantine must share the base generation".to_string());
    }
    if overlay_one.get(&one) != Some(&[] as &[Diagnostic]) {
        return Err("quarantined URI must read as an empty overlay".to_string());
    }
    let other = test_uri("file:///workspace/src/gen_1.rs")?;
    if overlay_one.get(&other) != generation.get(&other).map(Vec::as_slice) {
        return Err("unquarantined URIs must still read the shared generation".to_string());
    }
    if generation
        .get(&one)
        .is_none_or(|diagnostics| diagnostics.is_empty())
    {
        return Err("the base generation must keep the quarantined URI's payload".to_string());
    }

    let overlay_many =
        CommittedDiagnostics::from_generation_with_cleared(generation.clone(), many.clone());
    if !overlay_many.generation().ptr_eq(&generation) {
        return Err("many-URI quarantine must share the base generation".to_string());
    }
    for uri in &many {
        if overlay_many.get(uri) != Some(&[] as &[Diagnostic]) {
            return Err(format!("overlay must clear {}", uri.as_str()));
        }
        if generation
            .get(uri)
            .is_none_or(|diagnostics| diagnostics.is_empty())
        {
            return Err(format!("base generation lost payload for {}", uri.as_str()));
        }
    }
    Ok(())
}

#[test]
fn commit_applies_quarantine_as_an_overlay_on_the_shared_generation() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to start test runtime: {err}"))?;
    runtime.block_on(async {
        let fixture = quarantine_fixture("generation-overlay")?;
        let (service, socket) =
            LspService::new(|client| Backend::new(client, fixture.root.clone()));
        drop(socket);
        let backend = service.inner();
        backend
            .did_open(quarantine_open_params(&fixture.uri_a, QUARANTINE_TEXT_A))
            .await;
        backend
            .did_open(quarantine_open_params(&fixture.uri_b, QUARANTINE_TEXT_B))
            .await;
        commit_quarantine_snapshot(backend, &fixture, QUARANTINE_TEXT_A)?;
        backend
            .did_change(quarantine_change_params(
                &fixture.uri_a,
                2,
                QUARANTINE_TEXT_A_DIRTY,
            ))
            .await;
        commit_quarantine_snapshot(backend, &fixture, QUARANTINE_TEXT_A)?;
        let after = backend
            .latest_analysis_snapshot()
            .ok_or_else(|| "expected snapshot after quarantined commit".to_string())?;
        let after_generation = backend
            .last_diagnostics_generation_for_test()
            .ok_or_else(|| "expected generation after quarantined commit".to_string())?;
        if !after.diagnostics_by_uri.ptr_eq(&after_generation) {
            return Err("quarantined commit must still share the snapshot generation".to_string());
        }
        if !backend.last_diagnostics_overlay_clears_uri_for_test(&fixture.uri_a) {
            return Err("quarantined URI must be overlay-cleared, not map-cloned".to_string());
        }
        let overlay_uris = backend.last_diagnostics_overlay_uris_for_test();
        if !overlay_uris.iter().any(|uri| uri == &fixture.uri_a)
            || overlay_uris.iter().any(|uri| uri == &fixture.uri_b)
        {
            return Err("overlay must name only the quarantined URI".to_string());
        }
        if after
            .diagnostics_by_uri
            .get(&fixture.uri_a)
            .is_none_or(std::vec::Vec::is_empty)
        {
            return Err("snapshot generation must keep the quarantined URI payload".to_string());
        }
        if backend
            .last_diagnostics_for_uri_for_test(&fixture.uri_a)
            .is_some_and(|diagnostics| !diagnostics.is_empty())
        {
            return Err("committed baseline for the quarantined URI must be empty".to_string());
        }
        let uri_b = backend.last_diagnostics_for_uri_for_test(&fixture.uri_b);
        if uri_b.as_deref()
            != after
                .diagnostics_by_uri
                .get(&fixture.uri_b)
                .map(Vec::as_slice)
        {
            return Err("unquarantined URI must still publish the shared generation".to_string());
        }
        Ok(())
    })
}

#[test]
fn cloned_snapshot_mutation_is_isolated_from_the_committed_generation() -> Result<(), String> {
    let (service, _socket) = LspService::new(|client| Backend::new(client, PathBuf::from(".")));
    let backend = service.inner();
    commit_workspace(backend, many_uri_workspace(8, 2)?)?;
    let mut cloned = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "expected committed snapshot".to_string())?;
    let original_id = cloned.diagnostics_by_uri.generation_id();
    cloned.diagnostics_by_uri.to_mut().clear();
    if cloned.diagnostics_by_uri.generation_id() == original_id {
        return Err("copy-on-write mutation must mint a new generation id".to_string());
    }
    let committed = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "expected committed snapshot after clone mutation".to_string())?;
    if committed.diagnostics_by_uri.is_empty() {
        return Err("mutating a clone must not clear the committed generation".to_string());
    }
    if committed.diagnostics_by_uri.generation_id() != original_id {
        return Err("the committed generation id must stay stable".to_string());
    }
    Ok(())
}

#[test]
fn committed_view_matches_shared_generation_publish_bytes() -> Result<(), String> {
    let (service, _socket) = LspService::new(|client| Backend::new(client, PathBuf::from(".")));
    let backend = service.inner();
    let workspace = many_uri_workspace(12, 3)?;
    let expected = workspace
        .snapshot
        .diagnostics_by_uri
        .iter()
        .map(|(uri, diagnostics)| diagnostic_bytes(diagnostics).map(|bytes| (uri.clone(), bytes)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    commit_workspace(backend, workspace)?;
    let snapshot = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "expected committed snapshot".to_string())?;
    for (uri, expected_bytes) in expected {
        let snapshot_bytes = diagnostic_bytes(
            snapshot
                .diagnostics_by_uri
                .get(&uri)
                .ok_or_else(|| format!("snapshot missing {}", uri.as_str()))?,
        )?;
        let committed_bytes = diagnostic_bytes(
            &backend
                .last_diagnostics_for_uri_for_test(&uri)
                .ok_or_else(|| format!("committed view missing {}", uri.as_str()))?,
        )?;
        if snapshot_bytes != expected_bytes || committed_bytes != expected_bytes {
            return Err(format!(
                "publishDiagnostics bytes drifted for {}",
                uri.as_str()
            ));
        }
    }
    Ok(())
}

#[test]
fn shared_previous_generation_still_suppresses_unchanged_publish() -> Result<(), String> {
    let uri = test_uri("file:///workspace/src/gen_0.rs")?;
    let diagnostics = vec![generation_diagnostic(0, 0)];
    let mut by_uri = BTreeMap::new();
    by_uri.insert(uri.clone(), diagnostics.clone());
    let previous = CommittedDiagnostics::from_generation(DiagnosticsGeneration::from_map(by_uri));
    let plan = diagnostic_refresh_plan(&previous, vec![DiagnosticBatch { uri, diagnostics }]);
    if !plan.publish_batches.is_empty() || plan.unchanged_uri_count != 1 {
        return Err("unchanged shared generation must still suppress publication".to_string());
    }
    Ok(())
}

#[test]
fn refresh_plan_and_rollback_lookup_read_the_quarantine_overlay() -> Result<(), String> {
    let quarantined = test_uri("file:///workspace/src/gen_0.rs")?;
    let stable = test_uri("file:///workspace/src/gen_1.rs")?;
    let overlay_only = test_uri("file:///workspace/src/gen_overlay_only.rs")?;
    let quarantined_diagnostics = vec![generation_diagnostic(0, 0)];
    let stable_diagnostics = vec![generation_diagnostic(1, 0)];
    let mut by_uri = BTreeMap::new();
    by_uri.insert(quarantined.clone(), quarantined_diagnostics.clone());
    by_uri.insert(stable.clone(), stable_diagnostics.clone());
    let generation = DiagnosticsGeneration::from_map(by_uri);
    let previous = CommittedDiagnostics::from_generation_with_cleared(
        generation.clone(),
        [quarantined.clone(), overlay_only.clone()],
    );

    let previous_uris = previous.uris();
    if !previous_uris.contains(&quarantined)
        || !previous_uris.contains(&stable)
        || !previous_uris.contains(&overlay_only)
    {
        return Err(
            "refresh planning must enumerate generation keys and overlay-only keys together"
                .to_string(),
        );
    }
    if previous.get(&quarantined) != Some(&[] as &[Diagnostic]) {
        return Err(
            "effective lookup for a quarantined URI must be the empty overlay, not the generation"
                .to_string(),
        );
    }
    if previous.get(&stable) != Some(stable_diagnostics.as_slice()) {
        return Err("unquarantined URI lookup must still read the shared generation".to_string());
    }
    if previous.get(&overlay_only) != Some(&[] as &[Diagnostic]) {
        return Err("overlay-only URI lookup must be empty".to_string());
    }

    let plan = diagnostic_refresh_plan(
        &previous,
        vec![
            DiagnosticBatch {
                uri: quarantined.clone(),
                diagnostics: quarantined_diagnostics,
            },
            DiagnosticBatch {
                uri: stable.clone(),
                diagnostics: stable_diagnostics.clone(),
            },
        ],
    );
    if plan.publish_batches.len() != 1
        || plan.publish_batches[0].uri != quarantined
        || plan.unchanged_uri_count != 1
    {
        return Err(
            "overlay-cleared URI must republish; unquarantined matching URI must stay suppressed"
                .to_string(),
        );
    }
    if !plan.clear_uris.contains(&overlay_only) || plan.clear_uris.contains(&stable) {
        return Err(
            "overlay-only previous URI must be cleared; generation URI still in current must not"
                .to_string(),
        );
    }

    let mut rollback_uris = previous.uris();
    rollback_uris.extend(plan.current_uris.iter().cloned());
    for uri in rollback_uris {
        let raw = previous.get(&uri).unwrap_or(&[]);
        if uri == quarantined || uri == overlay_only {
            if !raw.is_empty() {
                return Err(format!(
                    "rollback lookup must restore the empty overlay for {}",
                    uri.as_str()
                ));
            }
        } else if uri == stable && raw != stable_diagnostics.as_slice() {
            return Err(
                "rollback lookup must restore the generation payload for the stable URI"
                    .to_string(),
            );
        }
    }
    if !previous.generation().ptr_eq(&generation) {
        return Err("planning against an overlay must still share the base generation".to_string());
    }
    Ok(())
}

#[test]
fn replacing_shared_clone_with_a_deep_map_clone_fails_the_sharing_control() -> Result<(), String> {
    let (service, _socket) = LspService::new(|client| Backend::new(client, PathBuf::from(".")));
    let backend = service.inner();
    commit_workspace(backend, many_uri_workspace(24, 3)?)?;
    let committed = backend
        .last_diagnostics_generation_for_test()
        .ok_or_else(|| "expected committed generation".to_string())?;
    let deep_before = DiagnosticsGeneration::deep_clone_count();
    let rebuilt = DiagnosticsGeneration::from_map(committed.deep_clone_map());
    if DiagnosticsGeneration::deep_clone_count() != deep_before.saturating_add(1) {
        return Err("the negative control must observe the deep clone".to_string());
    }
    if committed.ptr_eq(&rebuilt) {
        return Err("deep-cloned replacement must not share the generation".to_string());
    }
    let latest = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "expected committed snapshot".to_string())?;
    if !latest.diagnostics_by_uri.ptr_eq(&committed) {
        return Err("the production path must keep sharing; this is the control that would fail if commit cloned the map".to_string());
    }
    Ok(())
}
