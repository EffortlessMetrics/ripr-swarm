//! RIPR-SPEC-0180: persistent saved-edit custody during full-refresh supersession.
use super::*;
use tower_lsp_server::ls_types::InitializedParams;

const INITIAL: &str = "pub fn gate_state(flag: bool) -> bool { !flag }\n";
const INTERMEDIATE: &str = "pub fn gate_state(flag: bool) -> bool { flag && false }\n";
const NEWEST: &str = "pub fn gate_state(flag: bool) -> bool { flag || true }\n";
const NEWEST_TEST: &str = "#[test]\nfn observes_gate() {\n    assert!(lsp_scope::gate_state(false));\n    assert!(lsp_scope::gate_state(true));\n}\n";

fn finding_signature(snapshot: &AnalysisSnapshot) -> Result<Vec<String>, String> {
    let mut signatures = snapshot
        .findings
        .iter()
        .map(|finding| {
            serde_json::to_string(&(
                &finding.probe,
                &finding.class,
                &finding.ripr,
                &finding.related_tests,
            ))
            .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    signatures.sort();
    Ok(signatures)
}

#[tokio::test]
async fn saved_edit_burst_supersedes_full_and_publishes_only_newest_state() -> Result<(), String> {
    let root = unique_lsp_test_root("full-refresh-saved-edit-burst")?;
    write_lsp_scope_fixture(root.path())?;
    let companion_path = root.path().join("src/companion.rs");
    let companion = "pub fn unchanged(value: u8) -> u8 { value }\n";
    fs::write(&companion_path, companion).map_err(|error| error.to_string())?;
    for args in [
        vec!["init"],
        vec!["config", "user.email", "ripr@example.invalid"],
        vec!["config", "user.name", "RIPR Test"],
        vec!["add", "Cargo.toml", "src", "tests"],
        vec!["commit", "-m", "base"],
    ] {
        run_lsp_scope_git(root.path(), &args)?;
    }
    let path = root.path().join("src/lib.rs");
    let test_path = root.path().join("tests/end_to_end.rs");
    fs::write(&path, INITIAL).map_err(|error| error.to_string())?;
    let uri = file_uri_for_path(&path)?;
    let test_uri = file_uri_for_path(&test_path)?;
    let companion_uri = file_uri_for_path(&companion_path)?;
    let (service, socket) =
        LspService::new(|client| Backend::new(client, root.path().to_path_buf()));
    // Exercise real handlers in one persistent backend; framed delivery is
    // covered by the existing lifecycle tests. Drop output so it fails fast.
    drop(socket);
    let backend = service.inner();
    let init = serde_json::from_value(serde_json::json!({
        "processId": null, "rootUri": file_uri_for_path(root.path())?.as_str(),
        "capabilities": {}, "initializationOptions": {
            "baseRef": "HEAD", "checkMode": "instant", "diagnosticProfile": "full",
            "seamDiagnostics": true
        }
    }))
    .map_err(|error| error.to_string())?;
    backend
        .initialize(init)
        .await
        .map_err(|error| error.to_string())?;
    backend.initialized(InitializedParams {}).await;
    backend
        .did_open(quarantine_open_params(&uri, INITIAL))
        .await;
    let initial_test = fs::read_to_string(&test_path).map_err(|error| error.to_string())?;
    backend
        .did_open(quarantine_open_params(&test_uri, &initial_test))
        .await;
    backend
        .did_open(quarantine_open_params(&companion_uri, companion))
        .await;
    // Establish a completed saved-state analysis that actually indexed the
    // unchanged companion. Opening a clean file alone deliberately skips a
    // refresh; cache hits must not depend on the obsolete attempt warming it.
    backend
        .refresh_diagnostics(RefreshScope::Full, RefreshReason::ExplicitRefresh)
        .await;
    let prior = backend
        .latest_analysis_snapshot()
        .ok_or("didOpen produced no snapshot")?;
    assert!(
        !prior.findings.is_empty(),
        "setup needs actual production findings"
    );
    assert_eq!(
        prior
            .rust_consumed_sources
            .digest(Path::new("src/companion.rs")),
        Some(content_digest(companion.as_bytes())),
        "setup must have analyzed the unchanged file"
    );
    let config = backend
        .analysis_config()
        .ok_or("no accepted configuration")?;
    let (witness, release) = backend.install_consumed_source_barrier_for_test(config.clone())?;
    let before = backend.refresh_telemetry_for_test();
    let worker = backend.refresh_scheduler_for_test().analysis_thread();
    let jobs_before = worker.job_threads_for_test().len();
    let gate = backend.refresh_scheduler_for_test().execution_gate();
    let (unblock, blocked) = std::sync::mpsc::channel();
    let (locked, ready) = tokio::sync::oneshot::channel();
    let execution = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let _guard = gate.lock().map_err(|error| error.to_string())?;
        locked
            .send(())
            .map_err(|_| "gate readiness receiver dropped")?;
        match blocked.recv_timeout(Duration::from_secs(30)) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Ok(()),
            Err(error) => Err(format!("execution gate was not released: {error}")),
        }
    });
    ready.await.map_err(|error| error.to_string())?;
    let controller = async {
        // Hold the existing execution gate until real save handlers have
        // queued their newest input. The obsolete full job has reached the
        // production analysis thread, but cannot pass its first checkpoint.
        tokio::time::timeout(Duration::from_secs(5), async {
            while worker.job_threads_for_test().len() == jobs_before {
                tokio::task::yield_now().await;
            }
        })
        .await
        .map_err(|error| format!("full job did not reach analysis thread: {error}"))?;
        let active = backend
            .refresh_scheduler_for_test()
            .active_request_for_test()
            .ok_or("full request disappeared before saved edits")?;
        if active.scope != RefreshScope::Full
            || !backend.is_current_refresh_generation(active.generation)
        {
            return Err("setup did not hold the current full request".to_string());
        }
        for (version, source) in [(2, INTERMEDIATE), (3, NEWEST)] {
            backend
                .did_change(quarantine_change_params(&uri, version, source))
                .await;
            if !report_is_quarantine_withdrawal(&pull_document_json(backend, &uri, None).await?) {
                return Err("dirty production edit retained current diagnostics".to_string());
            }
            fs::write(&path, source).map_err(|error| error.to_string())?;
            backend.did_save(quarantine_save_params(&uri, source)).await;
        }
        backend
            .did_change(quarantine_change_params(&test_uri, 2, NEWEST_TEST))
            .await;
        fs::write(&test_path, NEWEST_TEST).map_err(|error| error.to_string())?;
        backend
            .did_save(quarantine_save_params(&test_uri, NEWEST_TEST))
            .await;
        if backend
            .latest_analysis_snapshot()
            .ok_or("lost prior snapshot")?
            .refresh
            .snapshot_id
            != prior.refresh.snapshot_id
        {
            return Err("superseded full replaced the prior snapshot".to_string());
        }
        drop(unblock);
        if active.cancellation.abort_kind()
            != Some(crate::analysis::cancellation::AnalysisAbortKind::Superseded)
        {
            return Err(
                "obsolete full token remained live before the production checkpoint".to_string(),
            );
        }
        // This existing barrier is after stale-result suppression, so it
        // checks newest-result custody, independently of token cancellation.
        let (generation, produced) = witness.await.map_err(|error| error.to_string())?;
        release
            .send(())
            .map_err(|_| "refresh dropped producer barrier")?;
        eprintln!(
            "producer witness: generation={generation} current={} seams_deferred={} findings={}",
            backend.is_current_refresh_generation(generation),
            produced.seams_deferred,
            produced.findings.len()
        );
        if !backend.is_current_refresh_generation(generation)
            || !produced.seams_deferred
            || produced.findings.is_empty()
        {
            return Err("newest interactive result lost current, nonempty custody".to_string());
        }
        Ok::<_, String>(active)
    };
    let ((), controlled) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(
            backend.refresh_diagnostics(RefreshScope::Full, RefreshReason::ExplicitRefresh),
            controller
        )
    })
    .await
    .map_err(|error| error.to_string())?;
    execution.await.map_err(|error| error.to_string())??;
    let obsolete = controlled?;
    assert_eq!(
        obsolete.cancellation.observed_abort(),
        Some(crate::analysis::cancellation::AnalysisAbortKind::Superseded),
        "the production worker must observe cancellation after its execution gate opens"
    );
    let after = backend.refresh_telemetry_for_test();
    assert_eq!(
        after.analyses_started - before.analyses_started,
        2,
        "one obsolete and one newest analysis"
    );
    assert_eq!(after.requests_coalesced - before.requests_coalesced, 3);
    assert_eq!(after.snapshots_published - before.snapshots_published, 1);
    assert_eq!(
        after.completed_but_superseded - before.completed_but_superseded,
        1
    );
    assert_eq!(after.pending_queue_high_water, 1);
    let newest = backend
        .latest_analysis_snapshot()
        .ok_or("newest save produced no snapshot")?;
    assert_ne!(newest.refresh.snapshot_id, prior.refresh.snapshot_id);
    assert!(
        newest.seams_deferred,
        "newest save keeps its interactive scope and disclosure"
    );
    assert!(
        !newest.findings.is_empty(),
        "speed must not come from dropping findings"
    );
    for (uri, version, text) in [(&uri, 3, NEWEST), (&test_uri, 2, NEWEST_TEST)] {
        let document = backend
            .document_state_for_test(uri)
            .ok_or("lost edited document")?;
        assert_eq!(document.version, Some(version));
        assert_eq!(
            document.analyzed_saved_digest,
            Some(content_digest(text.as_bytes()))
        );
        assert!(
            !document.is_quarantined(),
            "newest analyzed bytes must be current"
        );
    }
    assert!(
        newest
            .findings
            .iter()
            .flat_map(|finding| &finding.related_tests)
            .any(|related| related.name.contains("observes_gate")),
        "newest assertion edit needs actual related-test attribution"
    );
    let open_paths = backend.open_rust_index_paths_for_root(root.path());
    let independent =
        crate::lsp::diagnostics::workspace_diagnostics_with_config_and_open_rust_paths(
            root.path(),
            &config,
            true,
            &open_paths,
        )?;
    assert_eq!(
        finding_signature(&newest)?,
        finding_signature(&independent.snapshot)?,
        "latest probes, classifications, RIPR evidence and test attribution must match independent same-state analysis"
    );
    assert_eq!(
        newest
            .rust_consumed_sources
            .digest(Path::new("src/companion.rs")),
        Some(content_digest(companion.as_bytes()))
    );
    assert!(
        newest.rust_consumed_sources.file_fact_cache.hits > 0,
        "unchanged inputs should reuse cached facts"
    );
    let pull = pull_document_json(backend, &uri, None).await?;
    assert!(!report_is_quarantine_withdrawal(&pull));
    assert!(
        report_kind_and_items(&pull).1 > 0,
        "newest diagnostics must remain nonempty"
    );
    let result_id = pull["resultId"]
        .as_str()
        .ok_or("pull omitted result id")?
        .to_string();
    let unchanged = pull_document_json(backend, &uri, Some(result_id)).await?;
    assert_eq!(unchanged["kind"], "unchanged");
    let settled = backend.refresh_telemetry_for_test();
    backend.did_save(quarantine_save_params(&uri, NEWEST)).await;
    backend
        .did_save(quarantine_save_params(&test_uri, NEWEST_TEST))
        .await;
    assert_eq!(
        backend.refresh_telemetry_for_test().analyses_started,
        settled.analyses_started,
        "unchanged saves must retain the newest snapshot without another analysis"
    );
    eprintln!(
        "persistent edit witness: newest_findings={} analyses=2 coalesced=3 published=1 fact_hits={} latest_save_ms={:?}",
        newest.findings.len(),
        newest.rust_consumed_sources.file_fact_cache.hits,
        after.latest_save_to_snapshot_ms
    );
    backend
        .shutdown()
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
