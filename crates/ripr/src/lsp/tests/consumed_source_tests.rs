use super::*;

const SOURCE_A: &str =
    "pub fn gate_state(flag: bool) -> bool {\n    if flag { true } else { false }\n}\n";
const SOURCE_B: &str =
    "pub fn gate_state(flag: bool) -> bool {\n    if !flag { true } else { false }\n}\n";

#[test]
fn rust_prepare_refuses_missing_conflicting_and_unloaded_paths_but_preserves_non_rust()
-> Result<(), String> {
    let root = unique_lsp_test_root("consumed-source-refusal")?;
    let rust_path = root.path().join("lib.rs");
    let python_path = root.path().join("app.py");
    fs::write(&rust_path, SOURCE_A).map_err(|error| format!("write Rust A: {error}"))?;
    fs::write(&python_path, "value = 1\n").map_err(|error| format!("write Python A: {error}"))?;
    let rust_uri = file_uri_for_path(&rust_path)?;
    let python_uri = file_uri_for_path(&python_path)?;
    let mut documents = DocumentStore::default();
    documents.open(quarantine_open_params(&rust_uri, SOURCE_A));
    documents.open(quarantine_open_params(&python_uri, "value = 1\n"));
    fs::write(&rust_path, SOURCE_B).map_err(|error| format!("write Rust B: {error}"))?;
    let python_b = "value = 2\n";
    fs::write(&python_path, python_b).map_err(|error| format!("write Python B: {error}"))?;
    let mut captured = crate::analysis::consumed_source::ConsumedRustSources::default();
    captured.record(std::path::Path::new("lib.rs"), Some(SOURCE_A.as_bytes()));
    let (pending, _) = documents.pending_analyzed_digests(root.path(), &captured);
    if pending.get(&rust_uri).and_then(Option::as_ref) != Some(&content_digest(SOURCE_A.as_bytes()))
        || pending.get(&python_uri).and_then(Option::as_ref)
            != Some(&content_digest(python_b.as_bytes()))
    {
        return Err("Rust preparation reread disk B or changed non-Rust behavior".into());
    }
    documents.note_refresh_analyzed(None, &pending, &[]);
    let mut missing = crate::analysis::consumed_source::ConsumedRustSources::default();
    missing.record(std::path::Path::new("lib.rs"), None);
    let mut conflicting = captured.clone();
    conflicting.record(std::path::Path::new("lib.rs"), Some(SOURCE_B.as_bytes()));
    for refused in [Default::default(), missing, conflicting] {
        let (pending, _) = documents.pending_analyzed_digests(root.path(), &refused);
        if !matches!(pending.get(&rust_uri), Some(None)) {
            return Err("missing, unloaded or conflicting Rust input borrowed a digest".into());
        }
        documents.note_refresh_analyzed(None, &pending, &[]);
        if !documents
            .state_for_uri(&rust_uri)
            .is_some_and(|state| state.is_quarantined())
        {
            return Err("unavailable Rust commitment did not quarantine".into());
        }
    }
    let (pending, _) = documents.pending_analyzed_digests(root.path(), &captured);
    documents.note_refresh_analyzed(None, &pending, &[]);
    if documents
        .state_for_uri(&rust_uri)
        .is_none_or(|state| state.is_quarantined())
    {
        return Err("a later valid captured input did not recover quarantine".into());
    }
    Ok(())
}

#[test]
fn actual_saved_producer_commits_raw_bytes_on_cold_and_warm_cache_paths() -> Result<(), String> {
    let root = unique_lsp_test_root("consumed-source-cache")?;
    write_lsp_scope_fixture(root.path())?;
    run_lsp_scope_git(root.path(), &["init"])?;
    run_lsp_scope_git(
        root.path(),
        &["config", "user.email", "ripr@example.invalid"],
    )?;
    run_lsp_scope_git(root.path(), &["config", "user.name", "RIPR Test"])?;
    run_lsp_scope_git(
        root.path(),
        &["add", "Cargo.toml", "src/lib.rs", "tests/end_to_end.rs"],
    )?;
    run_lsp_scope_git(root.path(), &["commit", "-m", "base"])?;
    let path = root.path().join("src/lib.rs");
    let config = LspAnalysisConfig {
        base_ref: Some("HEAD".to_string()),
        mode: Mode::Instant,
        diagnostic_profile: crate::config::LspDiagnosticProfile::Full,
        ..LspAnalysisConfig::default()
    };
    let crlf = SOURCE_A.replace('\n', "\r\n");
    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend_from_slice(SOURCE_A.as_bytes());
    let mut invalid = SOURCE_A.as_bytes().to_vec();
    invalid.extend_from_slice(b"// invalid byte: \xff\n");
    for (label, bytes) in [
        ("plain", SOURCE_A.as_bytes()),
        ("BOM", bom.as_slice()),
        ("CRLF", crlf.as_bytes()),
        ("invalid UTF-8", invalid.as_slice()),
    ] {
        fs::write(&path, bytes).map_err(|error| format!("write {label}: {error}"))?;
        let expected =
            content_digest(&fs::read(&path).map_err(|error| format!("read {label}: {error}"))?);
        let cold =
            crate::lsp::diagnostics::workspace_diagnostics_with_config(root.path(), &config, true)?;
        let cold_stats = &cold.snapshot.rust_consumed_sources.file_fact_cache;
        if cold.snapshot.findings.is_empty() || cold_stats.misses == 0 || cold_stats.stores == 0 {
            return Err(format!(
                "SETUP: {label} lacks actual nonempty cold producer: {cold_stats:?}"
            ));
        }
        let warm =
            crate::lsp::diagnostics::workspace_diagnostics_with_config(root.path(), &config, true)?;
        let warm_stats = &warm.snapshot.rust_consumed_sources.file_fact_cache;
        if warm.snapshot.findings.is_empty() || warm_stats.hits == 0 || warm_stats.misses != 0 {
            return Err(format!(
                "SETUP: {label} lacks actual nonempty warm hit: {warm_stats:?}"
            ));
        }
        for (stage, snapshot) in [("cold", &cold.snapshot), ("warm", &warm.snapshot)] {
            if snapshot
                .rust_consumed_sources
                .digest(&snapshot.root, &path)
                .as_ref()
                != Some(&expected)
            {
                return Err(format!(
                    "{label} {stage} commitment differs from actual raw loaded bytes"
                ));
            }
        }
    }
    let (legacy_output, _legacy_origins) = crate::app::check_workspace_worktree_with_origins(
        config.check_input(root.path()),
        config.repo_config(),
    )?;
    if legacy_output.findings.is_empty() {
        return Err("existing private two-tuple wrapper lost the actual producer findings".into());
    }
    Ok(())
}

// Probe identity/expression/location are producer evidence. Preparation may
// legitimately annotate the surrounding Finding, so do not compare that DTO.
fn producer_probe_signature(snapshot: &AnalysisSnapshot) -> Result<Vec<String>, String> {
    let mut signature = snapshot
        .findings
        .iter()
        .map(|finding| {
            serde_json::to_string(&finding.probe)
                .map_err(|err| format!("SETUP: encode actual probe witness failed: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    signature.sort();
    Ok(signature)
}

#[tokio::test]
async fn completed_saved_analysis_keeps_consumed_a_when_disk_and_buffer_become_b()
-> Result<(), String> {
    let root = unique_lsp_test_root("consumed-source-a-to-b")?;
    write_lsp_scope_fixture(root.path())?;
    run_lsp_scope_git(root.path(), &["init"])?;
    run_lsp_scope_git(
        root.path(),
        &["config", "user.email", "ripr@example.invalid"],
    )?;
    run_lsp_scope_git(root.path(), &["config", "user.name", "RIPR Test"])?;
    run_lsp_scope_git(
        root.path(),
        &["add", "Cargo.toml", "src/lib.rs", "tests/end_to_end.rs"],
    )?;
    run_lsp_scope_git(root.path(), &["commit", "-m", "base"])?;
    let path = root.path().join("src/lib.rs");
    fs::write(&path, SOURCE_A).map_err(|err| format!("write A failed: {err}"))?;
    let uri = file_uri_for_path(&path)?;
    let (service, socket) =
        LspService::new(|client| Backend::new(client, root.path().to_path_buf()));
    // Established direct-backend pattern: output fails fast rather than blocking
    // a bounded channel. The claim is internal refresh custody, not delivery.
    drop(socket);
    let backend = service.inner();
    backend.initialize_test_workspace_root();
    tokio::time::timeout(
        Duration::from_mins(2),
        backend.did_open(quarantine_open_params(&uri, SOURCE_A)),
    )
    .await
    .map_err(|_| "actual didOpen exceeded setup deadline".to_string())?;
    let prior = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "SETUP: didOpen did not establish a prior actual snapshot".to_string())?;
    let prior_signature = producer_probe_signature(&prior)?;
    let config = LspAnalysisConfig {
        base_ref: Some("HEAD".to_string()),
        mode: Mode::Instant,
        diagnostic_profile: crate::config::LspDiagnosticProfile::Full,
        ..LspAnalysisConfig::default()
    };
    let (witness, release) = backend.install_consumed_source_barrier_for_test(config)?;
    let controller = async {
        let (generation, produced) = witness
            .await
            .map_err(|_| "actual producer did not reach preparation barrier".to_string())?;
        if produced.root != root.path()
            || produced.findings.is_empty()
            || !produced.findings.iter().any(|finding| {
                finding.probe.location.file == path
                    && finding.probe.expression.contains("flag")
                    && finding
                        .probe
                        .owner
                        .as_ref()
                        .is_some_and(|owner| owner.0.contains("gate_state"))
            })
        {
            let probes = produced
                .findings
                .iter()
                .take(8)
                .map(|finding| {
                    serde_json::json!({
                        "id": finding.probe.id,
                        "path": finding.probe.location.file,
                        "line": finding.probe.location.line,
                        "column": finding.probe.location.column,
                        "owner": finding.probe.owner,
                        "expression": finding.probe.expression,
                    })
                })
                .collect::<Vec<_>>();
            let detail = serde_json::json!({
                "root": produced.root,
                "expected_root": root.path(),
                "expected_path": path,
                "expected_owner_contains": "gate_state",
                "expected_expression_contains": "flag",
                "findings_count": produced.findings.len(),
                "probes_first_eight": probes,
                "base": produced.base,
                "mode": produced.mode.as_str(),
                "seams_deferred": produced.seams_deferred,
                "partial_scope": format!("{:?}", produced.partial_scope),
                "component_outcomes": format!("{:?}", produced.component_outcomes),
                "out_of_scope_test_file_findings": produced.out_of_scope_test_file_findings,
                "analysis_outcome": produced.analysis_outcome.as_ref().map(|outcome| {
                    serde_json::json!({"kind": outcome.kind, "counts": outcome.counts, "limitations": outcome.limitations})
                }),
            });
            return Err(format!(
                "SETUP: actual A producer lacks the required nonempty source subject; {detail}"
            ));
        }
        if !backend.is_current_refresh_generation(generation) {
            return Err("SETUP: A generation was not current at barrier".to_string());
        }
        fs::write(&path, SOURCE_B).map_err(|err| format!("write B failed: {err}"))?;
        let actual_b =
            fs::read(&path).map_err(|err| format!("SETUP: read actual disk B failed: {err}"))?;
        let actual_b_digest = content_digest(&actual_b);
        if actual_b_digest != content_digest(SOURCE_B.as_bytes()) {
            return Err("SETUP: actual disk B does not match intended B".to_string());
        }
        backend
            .did_change(quarantine_change_params(&uri, 2, SOURCE_B))
            .await;
        if !backend.is_current_refresh_generation(generation) {
            return Err(
                "SETUP: didChange superseded A; attribution oracle was not reached".to_string(),
            );
        }
        eprintln!(
            "consumed-source witness: generation={generation} current_before_and_after_B=true actual_A_findings={}",
            produced.findings.len()
        );
        release
            .send(())
            .map_err(|_| "refresh dropped barrier release".to_string())?;
        let signature = producer_probe_signature(&produced)?;
        let id = produced.refresh.snapshot_id.ok_or_else(|| {
            "SETUP: actual A producer has no refresh snapshot identity".to_string()
        })?;
        Ok::<_, String>((id, signature, actual_b_digest))
    };
    // Independently hash the bytes on disk immediately before the real refresh.
    let actual_a =
        fs::read(&path).map_err(|err| format!("SETUP: read actual disk A failed: {err}"))?;
    let digest_a = content_digest(&actual_a);
    let digest_b = content_digest(SOURCE_B.as_bytes());
    if digest_a != content_digest(SOURCE_A.as_bytes()) || digest_a == digest_b {
        return Err("SETUP: actual disk A does not match distinct intended A".to_string());
    }
    let ((), produced_id) = tokio::time::timeout(Duration::from_mins(2), async {
        tokio::join!(
            backend.refresh_diagnostics(RefreshScope::Interactive, RefreshReason::ExplicitRefresh),
            controller
        )
    })
    .await
    .map_err(|_| "actual A-to-B refresh exceeded test deadline".to_string())?;
    let (produced_id, produced_signature, actual_b_digest) = produced_id?;
    let committed = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "refresh did not commit an actual snapshot".to_string())?;
    let document = backend
        .document_state_for_test(&uri)
        .ok_or_else(|| "actual open document state missing".to_string())?;
    let detail = format!(
        "disk_A={digest_a} disk_B={actual_b_digest} analyzed={:?} quarantined={} actual_A_count={} committed_count={}",
        document.analyzed_saved_digest,
        document.is_quarantined(),
        produced_signature.len(),
        committed.findings.len()
    );
    eprintln!("consumed-source attribution: {detail}");
    if document.analyzed_saved_digest.as_deref() == Some(digest_b.as_str()) {
        return Err(format!(
            "BEHAVIORAL: completed A evidence was attributed to reread B; {detail}"
        ));
    }
    if !document.is_quarantined() {
        return Err(format!(
            "BEHAVIORAL: B buffer is incorrectly current against completed A evidence; {detail}"
        ));
    }
    if committed.refresh.snapshot_id.as_deref() == Some(produced_id.as_str()) {
        if producer_probe_signature(&committed)? != produced_signature {
            return Err(format!(
                "SETUP: committed snapshot does not retain actual A probe evidence; {detail}"
            ));
        }
        if document.analyzed_saved_digest.as_deref() != Some(digest_a.as_str()) {
            return Err(format!(
                "BEHAVIORAL: committed A lacks consumed-A attribution; {detail}"
            ));
        }
    } else {
        // The prerequisite allows conservative refusal. It must preserve the
        // prior actual snapshot and keep B quarantined, rather than mint B.
        if committed.refresh.snapshot_id != prior.refresh.snapshot_id
            || producer_probe_signature(&committed)? != prior_signature
        {
            return Err(format!(
                "SETUP: neither actual A commit nor unchanged prior refusal reached; {detail}"
            ));
        }
        let report = pull_document_json(backend, &uri, None).await?;
        if report_kind_and_items(&report).1 != 0 {
            return Err(format!(
                "BEHAVIORAL: refused A preparation still serves stale diagnostics for B; {detail}"
            ));
        }
    }
    // A later real analysis of the persisted B input must recover currentness.
    // No test-owned commitment, forced snapshot or didSave digest supplies it.
    let actual_b = fs::read(&path).map_err(|error| format!("read recovery B: {error}"))?;
    if content_digest(&actual_b) != digest_b {
        return Err("SETUP: recovery disk bytes no longer equal independently witnessed B".into());
    }
    tokio::time::timeout(
        Duration::from_mins(2),
        backend.refresh_diagnostics(RefreshScope::Interactive, RefreshReason::ExplicitRefresh),
    )
    .await
    .map_err(|_| "actual B recovery refresh exceeded test deadline".to_string())?;
    let recovered = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "SETUP: B recovery produced no committed snapshot".to_string())?;
    if recovered.refresh.snapshot_id == committed.refresh.snapshot_id
        || !recovered.findings.iter().any(|finding| {
            finding.probe.location.file == path
                && finding.probe.expression.contains("!flag")
                && finding
                    .probe
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.0.contains("gate_state"))
        })
    {
        return Err("SETUP: recovery lacks a fresh nonempty actual B producer".into());
    }
    let recovered_document = backend
        .document_state_for_test(&uri)
        .ok_or_else(|| "SETUP: B recovery lost the actual document state".to_string())?;
    if recovered
        .rust_consumed_sources
        .digest(&recovered.root, &path)
        .as_ref()
        != Some(&digest_b)
        || recovered_document.analyzed_saved_digest.as_ref() != Some(&digest_b)
        || recovered_document.is_quarantined()
    {
        return Err(format!(
            "BEHAVIORAL: fresh B did not recover consumed-B currentness; analyzed={:?}, quarantined={}",
            recovered_document.analyzed_saved_digest,
            recovered_document.is_quarantined()
        ));
    }
    let served = pull_document_json(backend, &uri, None).await?;
    let expected_count = recovered.diagnostics_by_uri.get(&uri).map_or(0, Vec::len);
    if expected_count == 0 || report_kind_and_items(&served).1 != expected_count {
        return Err("BEHAVIORAL: recovered B pull does not serve its committed diagnostics".into());
    }
    Ok(())
}
