use super::*;

const SOURCE_A: &str =
    "pub fn gate_state(flag: bool) -> bool {\n    if flag { true } else { false }\n}\n";
const SOURCE_B: &str =
    "pub fn gate_state(flag: bool) -> bool {\n    if !flag { true } else { false }\n}\n";

#[test]
fn clean_open_rust_file_is_indexed_without_seeding_findings() -> Result<(), String> {
    let root = unique_lsp_test_root("consumed-source-clean-open")?;
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
    let raw = fs::read(&path).map_err(|error| format!("read clean Rust source: {error}"))?;
    let config = LspAnalysisConfig {
        base_ref: Some("HEAD".to_string()),
        mode: Mode::Instant,
        diagnostic_profile: crate::config::LspDiagnosticProfile::Full,
        ..LspAnalysisConfig::default()
    };
    let index_only = std::iter::once(std::path::PathBuf::from("src/lib.rs")).collect();
    let ordinary =
        crate::lsp::diagnostics::workspace_diagnostics_with_config(root.path(), &config, true)?;
    if !ordinary.snapshot.findings.is_empty()
        || ordinary
            .snapshot
            .rust_consumed_sources
            .digest(Path::new("src/lib.rs"))
            .is_some()
    {
        return Err("SETUP: clean ordinary diff unexpectedly indexed or probed source".into());
    }
    let cold = crate::lsp::diagnostics::workspace_diagnostics_with_config_and_open_rust_paths(
        root.path(),
        &config,
        true,
        &index_only,
    )?;
    let warm = crate::lsp::diagnostics::workspace_diagnostics_with_config_and_open_rust_paths(
        root.path(),
        &config,
        true,
        &index_only,
    )?;
    let expected = content_digest(&raw);
    for (stage, result) in [("cold", &cold), ("warm", &warm)] {
        if result
            .snapshot
            .rust_consumed_sources
            .digest(Path::new("src/lib.rs"))
            != Some(expected.clone())
            || !result.snapshot.findings.is_empty()
        {
            return Err(format!(
                "{stage} clean open source was not index-only captured"
            ));
        }
    }
    if cold.snapshot.rust_consumed_sources.file_fact_cache.misses == 0
        || warm.snapshot.rust_consumed_sources.file_fact_cache.hits == 0
    {
        return Err("clean open file did not exercise cold and warm fact-cache loads".into());
    }
    let foreign = std::iter::once(std::path::PathBuf::from("outside.rs")).collect();
    let refused = crate::lsp::diagnostics::workspace_diagnostics_with_config_and_open_rust_paths(
        root.path(),
        &config,
        true,
        &foreign,
    )?;
    if refused
        .snapshot
        .rust_consumed_sources
        .digest(Path::new("src/lib.rs"))
        .is_some()
    {
        return Err("undiscovered open path admitted unrelated Rust source".into());
    }
    let untracked_path = root.path().join("src/untracked.rs");
    fs::write(&untracked_path, SOURCE_B)
        .map_err(|error| format!("write untracked Rust source: {error}"))?;
    let untracked = std::iter::once(std::path::PathBuf::from("src/untracked.rs")).collect();
    let refused = crate::lsp::diagnostics::workspace_diagnostics_with_config_and_open_rust_paths(
        root.path(),
        &config,
        true,
        &untracked,
    )?;
    if refused
        .snapshot
        .rust_consumed_sources
        .digest(Path::new("src/untracked.rs"))
        .is_some()
    {
        return Err("discovered but untracked open Rust source acquired authority".into());
    }
    let uri = file_uri_for_path(&path)?;
    let source_a = String::from_utf8(raw)
        .map_err(|error| format!("fixture clean Rust source is not UTF-8: {error}"))?;
    let mut documents = DocumentStore::default();
    documents.open(quarantine_open_params(&uri, &source_a));
    fs::write(&path, SOURCE_B)
        .map_err(|error| format!("write later clean-path disk B: {error}"))?;
    documents.change(
        quarantine_change_params(&uri, 2, SOURCE_B),
        &tower_lsp_server::ls_types::PositionEncodingKind::UTF16,
    );
    let (pending, _) =
        documents.pending_analyzed_digests(root.path(), &warm.snapshot.rust_consumed_sources);
    if pending.get(&uri).and_then(Option::as_ref) != Some(&expected) {
        return Err("clean open producer A was relabeled as later disk or buffer B".into());
    }
    documents.note_refresh_analyzed(None, &pending, &[]);
    if documents
        .state_for_uri(&uri)
        .is_none_or(|state| !state.is_quarantined())
    {
        return Err("clean A snapshot served as current for later buffer B".into());
    }
    Ok(())
}

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
                .digest(Path::new("src/lib.rs"))
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
    .map_err(|_barrier_error| "actual didOpen exceeded setup deadline".to_string())?;
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
        let (generation, produced) = witness.await.map_err(|_barrier_error| {
            "actual producer did not reach preparation barrier".to_string()
        })?;
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
        // The editor buffer reaches B while disk still holds A: a buffer equal
        // to disk is saved content (#11153) and would legitimately supersede
        // this generation, which is not the race under test. Disk becomes B
        // right after, before the refresh commits.
        backend
            .did_change(quarantine_change_params(&uri, 2, SOURCE_B))
            .await;
        fs::write(&path, SOURCE_B).map_err(|err| format!("write B failed: {err}"))?;
        let actual_b =
            fs::read(&path).map_err(|err| format!("SETUP: read actual disk B failed: {err}"))?;
        let actual_b_digest = content_digest(&actual_b);
        if actual_b_digest != content_digest(SOURCE_B.as_bytes()) {
            return Err("SETUP: actual disk B does not match intended B".to_string());
        }
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
            .map_err(|_barrier_error| "refresh dropped barrier release".to_string())?;
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
    .map_err(|_barrier_error| "actual A-to-B refresh exceeded test deadline".to_string())?;
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
    }
    // Both a retained A commit and conservative refusal must withhold A's
    // line-local evidence from B. Quarantine bookkeeping alone is not proof
    // that the real pull route and committed push baseline honor it.
    let stale_report = pull_document_json(backend, &uri, None).await?;
    let last = backend.last_diagnostics_for_uri_for_test(&uri);
    if !report_is_quarantine_withdrawal(&stale_report) {
        return Err(format!(
            "BEHAVIORAL: completed A still serves stale diagnostics for B; {detail}"
        ));
    }
    if !last.as_ref().is_some_and(Vec::is_empty)
        && !last_diagnostics_are_quarantine_withdrawal(last)
    {
        return Err(format!(
            "BEHAVIORAL: completed A still serves stale diagnostics for B; {detail}"
        ));
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
    .map_err(|_barrier_error| "actual B recovery refresh exceeded test deadline".to_string())?;
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
        .digest(Path::new("src/lib.rs"))
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

#[cfg(unix)]
struct ConsumedSourceAliasFixture {
    temp: TempLspRoot,
    root: PathBuf,
    alias: PathBuf,
    file: PathBuf,
}

#[cfg(unix)]
fn consumed_source_alias_fixture(name: &str) -> Result<ConsumedSourceAliasFixture, String> {
    let temp = unique_lsp_test_root(name)?;
    let root = temp.path().join("project");
    fs::create_dir_all(&root).map_err(|error| format!("create alias fixture: {error}"))?;
    write_lsp_scope_fixture(&root)?;
    fs::write(root.join("src/lib.rs"), SOURCE_A)
        .map_err(|error| format!("write alias fixture A: {error}"))?;
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize alias fixture: {error}"))?;
    let alias = temp.path().join("linked-project");
    std::os::unix::fs::symlink(&root, &alias)
        .map_err(|error| format!("link alias fixture: {error}"))?;
    let file = root.join("src/lib.rs");
    Ok(ConsumedSourceAliasFixture {
        temp,
        root,
        alias,
        file,
    })
}

#[cfg(unix)]
#[test]
fn admitted_root_aliases_resolve_captured_identity_and_preserve_refusals() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("consumed-root-alias")?;
    let canonical_uri = file_uri_for_path(&fixture.file)?;
    let lexical_uri = file_uri_for_path(&fixture.alias.join("src/lib.rs"))?;
    let expected = content_digest(SOURCE_A.as_bytes());
    let mut captured = crate::analysis::consumed_source::ConsumedRustSources::default();
    captured.record(Path::new("src/lib.rs"), Some(SOURCE_A.as_bytes()));
    for (label, root, uri) in [
        ("ordinary", &fixture.root, &canonical_uri),
        ("lexical alias", &fixture.alias, &lexical_uri),
        (
            "canonical URI under aliased root",
            &fixture.alias,
            &canonical_uri,
        ),
    ] {
        if !crate::lsp::uri::file_uri_is_within_root(root, uri) {
            return Err(format!("SETUP: {label} is not admitted by URI containment"));
        }
        let mut documents = DocumentStore::default();
        documents.open(quarantine_open_params(uri, SOURCE_A));
        let (pending, _) = documents.pending_analyzed_digests(root, &captured);
        if pending.get(uri).and_then(Option::as_ref) != Some(&expected) {
            return Err(format!(
                "BEHAVIORAL: admitted {label} lost its consumed identity: {:?}",
                pending.get(uri)
            ));
        }
        documents.note_refresh_analyzed(None, &pending, &[]);
        if documents
            .state_for_uri(uri)
            .is_none_or(|state| state.is_quarantined())
        {
            return Err(format!("BEHAVIORAL: fresh {label} stayed quarantined"));
        }
        let mut missing = crate::analysis::consumed_source::ConsumedRustSources::default();
        missing.record(Path::new("src/lib.rs"), None);
        let mut conflicting = captured.clone();
        conflicting.record(Path::new("src/lib.rs"), Some(SOURCE_B.as_bytes()));
        for unavailable in [Default::default(), missing, conflicting] {
            let (pending, _) = documents.pending_analyzed_digests(root, &unavailable);
            if !matches!(pending.get(uri), Some(None)) {
                return Err(format!("{label} borrowed an unavailable source commitment"));
            }
            documents.note_refresh_analyzed(None, &pending, &[]);
            if documents
                .state_for_uri(uri)
                .is_none_or(|state| !state.is_quarantined())
            {
                return Err(format!("{label} did not quarantine an unavailable input"));
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn consumed_identity_rejects_outside_and_symlink_escaped_documents() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("consumed-root-refusal")?;
    let outside = fixture.temp.path().join("outside.rs");
    fs::write(&outside, SOURCE_A).map_err(|error| format!("write outside control: {error}"))?;
    let escaped = fixture.root.join("src/escaped.rs");
    std::os::unix::fs::symlink(&outside, &escaped)
        .map_err(|error| format!("link escaped control: {error}"))?;
    let mut captured = crate::analysis::consumed_source::ConsumedRustSources::default();
    captured.record(Path::new("outside.rs"), Some(SOURCE_A.as_bytes()));
    captured.record(Path::new("src/escaped.rs"), Some(SOURCE_A.as_bytes()));
    for root in [&fixture.root, &fixture.alias] {
        for path in [&outside, &escaped, &fixture.alias.join("src/escaped.rs")] {
            let uri = file_uri_for_path(path)?;
            if crate::lsp::uri::file_uri_is_within_root(root, &uri) {
                return Err(format!(
                    "SETUP: outside control was admitted: {}",
                    path.display()
                ));
            }
            let mut documents = DocumentStore::default();
            documents.open(quarantine_open_params(&uri, SOURCE_A));
            let (pending, _) = documents.pending_analyzed_digests(root, &captured);
            if !matches!(pending.get(&uri), Some(None)) {
                return Err(format!(
                    "BEHAVIORAL: refused outside path borrowed a captured key: {}",
                    path.display()
                ));
            }
            documents.note_refresh_analyzed(None, &pending, &[]);
            if documents
                .state_for_uri(&uri)
                .is_none_or(|state| !state.is_quarantined())
            {
                return Err("a refused outside document became current".into());
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn real_saved_refresh_admits_canonical_uri_under_symlinked_root() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("consumed-alias-producer")?;
    run_lsp_scope_git(&fixture.root, &["init"])?;
    run_lsp_scope_git(
        &fixture.root,
        &["config", "user.email", "ripr@example.invalid"],
    )?;
    run_lsp_scope_git(&fixture.root, &["config", "user.name", "RIPR Test"])?;
    run_lsp_scope_git(
        &fixture.root,
        &["add", "Cargo.toml", "src/lib.rs", "tests/end_to_end.rs"],
    )?;
    run_lsp_scope_git(&fixture.root, &["commit", "-m", "base"])?;
    let uri = file_uri_for_path(&fixture.file)?;
    if !crate::lsp::uri::file_uri_is_within_root(&fixture.alias, &uri) {
        return Err("SETUP: canonical document URI not admitted under symlinked root".into());
    }
    let (service, socket) = LspService::new(|client| Backend::new(client, fixture.alias.clone()));
    drop(socket);
    let backend = service.inner();
    backend.initialize_test_workspace_root();
    let config = LspAnalysisConfig {
        base_ref: Some("HEAD".to_string()),
        mode: Mode::Instant,
        diagnostic_profile: crate::config::LspDiagnosticProfile::Full,
        ..LspAnalysisConfig::default()
    };
    let (witness, release) = backend.install_consumed_source_barrier_for_test(config)?;
    let controller = async {
        let (_, produced) = witness
            .await
            .map_err(|error| format!("alias producer barrier: {error}"))?;
        release
            .send(())
            .map_err(|error| format!("alias producer release closed: {error:?}"))?;
        Ok::<_, String>(produced)
    };
    let ((), produced) = tokio::time::timeout(Duration::from_mins(2), async {
        tokio::join!(
            backend.did_open(quarantine_open_params(&uri, SOURCE_A)),
            controller
        )
    })
    .await
    .map_err(|error| format!("alias producer didOpen deadline: {error}"))?;
    let produced = produced?;
    if produced.root != fixture.alias || !produced.findings.is_empty() {
        return Err("SETUP: clean aliased producer changed the root or seeded findings".into());
    }
    let expected = content_digest(SOURCE_A.as_bytes());
    let state = backend
        .document_state_for_test(&uri)
        .ok_or("alias document state missing")?;
    eprintln!(
        "alias producer: root={} uri={} captured={:?} analyzed={:?} quarantined={}",
        produced.root.display(),
        uri.as_str(),
        produced
            .rust_consumed_sources
            .digest(Path::new("src/lib.rs")),
        state.analyzed_saved_digest,
        state.is_quarantined()
    );
    if state.analyzed_saved_digest.as_ref() != Some(&expected) || state.is_quarantined() {
        return Err(
            "BEHAVIORAL: fresh canonical document under symlinked root stayed quarantined".into(),
        );
    }
    // Buffer reaches B while disk still holds A (a buffer equal to disk is
    // saved content, #11153); disk becomes B before the explicit refresh.
    backend
        .did_change(quarantine_change_params(&uri, 2, SOURCE_B))
        .await;
    fs::write(&fixture.file, SOURCE_B).map_err(|error| format!("write aliased B: {error}"))?;
    if backend
        .document_state_for_test(&uri)
        .is_none_or(|state| !state.is_quarantined())
    {
        return Err("aliased B escaped quarantine before analysis".into());
    }
    tokio::time::timeout(
        Duration::from_mins(2),
        backend.refresh_diagnostics(RefreshScope::Interactive, RefreshReason::ExplicitRefresh),
    )
    .await
    .map_err(|error| format!("alias B refresh deadline: {error}"))?;
    let snapshot = backend
        .latest_analysis_snapshot()
        .ok_or("alias B snapshot missing")?;
    if !snapshot
        .findings
        .iter()
        .any(|finding| finding.probe.expression.contains("!flag"))
    {
        return Err("SETUP: actual aliased B producer lacks B-specific evidence".into());
    }
    let state = backend
        .document_state_for_test(&uri)
        .ok_or("alias B document missing")?;
    if state.analyzed_saved_digest != Some(content_digest(SOURCE_B.as_bytes()))
        || state.is_quarantined()
    {
        return Err("fresh aliased B failed to recover consumed currentness".into());
    }
    let report = pull_document_json(backend, &uri, None).await?;
    if report
        .get("resultId")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|id| id.ends_with(":quarantined"))
    {
        return Err("aliased B pull still reports quarantine after actual analysis".into());
    }
    eprintln!(
        "alias delivery: current=true snapshot_findings={} canonical_pull_items={}",
        snapshot.findings.len(),
        report_kind_and_items(&report).1
    );
    let stored_uri = file_uri_for_path(&fixture.alias.join("src/lib.rs"))?;
    let expected_count = snapshot.served_diagnostics_for_uri(&stored_uri).len();
    if expected_count == 0 || report_kind_and_items(&report).1 != expected_count {
        return Err(
            "BEHAVIORAL: current canonical URI did not receive its actual B diagnostics".into(),
        );
    }
    if !snapshot.diagnostic_uri_index_is_current() {
        return Err("actual alias snapshot committed without its URI index".into());
    }
    backend
        .did_change(quarantine_change_params(&uri, 3, SOURCE_A))
        .await;
    let dirty = pull_document_json(backend, &uri, None).await?;
    if !report_is_quarantine_withdrawal(&dirty) {
        return Err("alias lookup bypassed per-document quarantine after a new edit".into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn snapshot_alias_lookup_is_root_scoped_unique_and_exact_preferred() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("consumed-alias-lookup")?;
    let canonical_uri = file_uri_for_path(&fixture.file)?;
    let lexical_uri = file_uri_for_path(&fixture.alias.join("src/lib.rs"))?;
    let first = tower_lsp_server::ls_types::Diagnostic::new_simple(
        Default::default(),
        "first stored identity".to_string(),
    );
    let second = tower_lsp_server::ls_types::Diagnostic::new_simple(
        Default::default(),
        "second stored identity".to_string(),
    );
    let mut snapshot = sample_analysis_snapshot(
        fixture.alias.clone(),
        lexical_uri.clone(),
        vec![first.clone()],
        Vec::new(),
    );
    snapshot.prepare_diagnostic_uri_index();
    if snapshot.diagnostics_for_uri(&canonical_uri) != Some([first.clone()].as_slice())
        || snapshot.served_diagnostics_for_uri(&canonical_uri) != vec![first.clone()]
    {
        return Err(
            "BEHAVIORAL: unique admitted root alias did not resolve stored diagnostics".into(),
        );
    }
    snapshot
        .diagnostics_by_uri
        .insert(canonical_uri.clone(), vec![second.clone()]);
    snapshot.prepare_diagnostic_uri_index();
    for (uri, expected) in [(&lexical_uri, &first), (&canonical_uri, &second)] {
        let actual = snapshot
            .diagnostics_for_uri(uri)
            .and_then(|items| items.first());
        if actual != Some(expected) {
            return Err("exact URI lost precedence to a different stored alias".into());
        }
    }
    let second_alias = fixture.temp.path().join("second-alias");
    std::os::unix::fs::symlink(&fixture.root, &second_alias)
        .map_err(|error| format!("create second root alias: {error}"))?;
    let ambiguous_uri = file_uri_for_path(&second_alias.join("src/lib.rs"))?;
    if snapshot.diagnostics_for_uri(&ambiguous_uri).is_some()
        || !snapshot
            .served_diagnostics_for_uri(&ambiguous_uri)
            .is_empty()
    {
        return Err(
            "ambiguous root-relative fallback selected an arbitrary stored identity".into(),
        );
    }
    let outside_uri = file_uri_for_path(&fixture.temp.path().join("outside/src/lib.rs"))?;
    if snapshot.diagnostics_for_uri(&outside_uri).is_some() {
        return Err("outside-root request borrowed a stored relative identity".into());
    }
    let inner_link = fixture.root.join("src/linked.rs");
    std::os::unix::fs::symlink(&fixture.file, &inner_link)
        .map_err(|error| format!("create in-workspace lexical alias: {error}"))?;
    let inner_uri = file_uri_for_path(&fixture.alias.join("src/linked.rs"))?;
    snapshot.diagnostics_by_uri.clear();
    snapshot
        .diagnostics_by_uri
        .insert(inner_uri.clone(), vec![first.clone()]);
    snapshot.prepare_diagnostic_uri_index();
    if snapshot.diagnostics_for_uri(&canonical_uri).is_some() {
        return Err("distinct in-workspace lexical source keys were collapsed".into());
    }
    if snapshot.diagnostics_for_uri(&inner_uri) != Some([first].as_slice()) {
        return Err("the exact in-workspace lexical identity was lost".into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn consumed_projection_uses_physical_parent_resolution() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("consumed-physical-parent")?;
    let physical_root = fixture.root.join("real");
    fs::create_dir_all(physical_root.join("deep"))
        .map_err(|error| format!("create physical root: {error}"))?;
    fs::create_dir_all(physical_root.join("src"))
        .map_err(|error| format!("create physical source directory: {error}"))?;
    let file = physical_root.join("src/lib.rs");
    fs::write(&file, SOURCE_A).map_err(|error| format!("write physical source A: {error}"))?;
    let link = fixture.root.join("linked-deep");
    std::os::unix::fs::symlink(physical_root.join("deep"), &link)
        .map_err(|error| format!("link physical root control: {error}"))?;
    let root_with_parent = link.join("..");
    if root_with_parent
        .canonicalize()
        .map_err(|error| error.to_string())?
        != physical_root
    {
        return Err("SETUP: symlink/.. did not reach the intended physical root".into());
    }
    let uri = file_uri_for_path(&file)?;
    if !crate::lsp::uri::file_uri_is_within_root(&root_with_parent, &uri) {
        return Err("SETUP: physical root control was not admitted".into());
    }
    let relative = crate::lsp::uri::file_uri_relative_to_root(&root_with_parent, &uri);
    if relative.as_deref() != Some(Path::new("src/lib.rs")) {
        return Err(format!(
            "BEHAVIORAL: symlink/.. used a lexical key instead of the physical root: {relative:?}"
        ));
    }
    let mut captured = crate::analysis::consumed_source::ConsumedRustSources::default();
    captured.record(Path::new("src/lib.rs"), Some(SOURCE_A.as_bytes()));
    let mut documents = DocumentStore::default();
    documents.open(quarantine_open_params(&uri, SOURCE_A));
    let (pending, _) = documents.pending_analyzed_digests(&root_with_parent, &captured);
    if pending.get(&uri).and_then(Option::as_ref) != Some(&content_digest(SOURCE_A.as_bytes())) {
        return Err("physical root projection did not retain its captured identity".into());
    }
    documents.note_refresh_analyzed(None, &pending, &[]);
    if documents
        .state_for_uri(&uri)
        .is_none_or(|state| state.is_quarantined())
    {
        return Err("physical root control remained quarantined".into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn snapshot_alias_misses_do_not_recanonicalize_stored_documents() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("consumed-alias-cost")?;
    let lexical_uri = file_uri_for_path(&fixture.alias.join("src/lib.rs"))?;
    let diagnostic = tower_lsp_server::ls_types::Diagnostic::new_simple(
        Default::default(),
        "stored identity".to_string(),
    );
    let mut snapshot = sample_analysis_snapshot(
        fixture.alias.clone(),
        lexical_uri,
        vec![diagnostic.clone()],
        Vec::new(),
    );
    for index in 0..128 {
        let uri = file_uri_for_path(&fixture.alias.join(format!("src/neighbor_{index}.rs")))?;
        snapshot
            .diagnostics_by_uri
            .insert(uri, vec![diagnostic.clone()]);
    }
    snapshot.prepare_diagnostic_uri_index();
    let clean_file = fixture.root.join("src/clean.rs");
    fs::write(&clean_file, SOURCE_A).map_err(|error| format!("write clean file: {error}"))?;
    let clean_uri = file_uri_for_path(&clean_file)?;
    let before = crate::lsp::uri::canonical_projection_count_for_test();
    for _ in 0..8 {
        if snapshot.diagnostics_for_uri(&clean_uri).is_some() {
            return Err("clean document borrowed a neighbor's diagnostic".into());
        }
    }
    let lookups = crate::lsp::uri::canonical_projection_count_for_test() - before;
    if lookups != 16 {
        return Err(format!(
            "BEHAVIORAL: eight alias misses repeated stored-path canonicalization: {lookups} projections"
        ));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn snapshot_alias_index_refuses_changed_roots_and_same_count_keys() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("consumed-alias-stale-index")?;
    let canonical_uri = file_uri_for_path(&fixture.file)?;
    let lexical_uri = file_uri_for_path(&fixture.alias.join("src/lib.rs"))?;
    let diagnostic = tower_lsp_server::ls_types::Diagnostic::new_simple(
        Default::default(),
        "stored identity".to_string(),
    );
    let mut snapshot = sample_analysis_snapshot(
        fixture.alias.clone(),
        lexical_uri,
        vec![diagnostic.clone()],
        Vec::new(),
    );
    if snapshot.diagnostics_for_uri(&canonical_uri).is_some() {
        return Err("unprepared alias lookup scanned the snapshot".into());
    }
    snapshot.prepare_diagnostic_uri_index();
    let second_alias = fixture.temp.path().join("replacement-alias");
    std::os::unix::fs::symlink(&fixture.root, &second_alias)
        .map_err(|error| format!("create replacement alias: {error}"))?;
    let replacement = file_uri_for_path(&second_alias.join("src/lib.rs"))?;
    snapshot.diagnostics_by_uri.clear();
    snapshot
        .diagnostics_by_uri
        .insert(replacement, vec![diagnostic.clone()]);
    if snapshot.diagnostic_uri_index_is_current()
        || snapshot.diagnostics_for_uri(&canonical_uri).is_some()
    {
        return Err("stale alias index accepted same-count replacement keys".into());
    }
    let (service, socket) = LspService::new(|client| Backend::new(client, fixture.alias.clone()));
    drop(socket);
    let backend = service.inner();
    let batches = snapshot
        .diagnostics_by_uri
        .iter()
        .map(
            |(uri, diagnostics)| crate::lsp::diagnostics::DiagnosticBatch {
                uri: uri.clone(),
                diagnostics: diagnostics.clone(),
            },
        )
        .collect();
    let plan = crate::lsp::diagnostics::diagnostic_refresh_plan(&Default::default(), batches);
    backend
        .commit_refresh_snapshot(snapshot.clone(), &plan, &Default::default(), &[])
        .ok_or_else(|| "stale-index direct commit failed".to_string())?;
    let committed = backend
        .latest_analysis_snapshot()
        .ok_or_else(|| "stale-index direct commit had no snapshot".to_string())?;
    if !committed.diagnostic_uri_index_is_current()
        || committed.diagnostics_for_uri(&canonical_uri) != Some([diagnostic.clone()].as_slice())
    {
        return Err("commit did not rebuild the same-count stale alias index".into());
    }
    snapshot.prepare_diagnostic_uri_index();
    if snapshot.diagnostics_for_uri(&canonical_uri) != Some([diagnostic.clone()].as_slice()) {
        return Err("rebuilding the alias index did not restore the replacement key".into());
    }
    snapshot.root = fixture.temp.path().join("outside");
    if snapshot.diagnostic_uri_index_is_current()
        || snapshot.diagnostics_for_uri(&canonical_uri).is_some()
    {
        return Err("stale alias index accepted a changed root".into());
    }
    snapshot.prepare_diagnostic_uri_index();
    if snapshot.diagnostics_for_uri(&canonical_uri).is_some() {
        return Err("rebuilding an outside-root index admitted the prior document".into());
    }
    snapshot.root = fixture.root.clone();
    snapshot.prepare_diagnostic_uri_index();
    if snapshot.diagnostics_for_uri(&canonical_uri) != Some([diagnostic].as_slice()) {
        return Err("ordinary-root rebuilt alias index failed to recover".into());
    }
    Ok(())
}

/// #11763: the per-refresh batch of open-document keys must equal the
/// per-URI projection for regular files, missing files, a file symlink, a
/// directory symlink and a root alias, since the batch only changes how often
/// directories are canonicalized.
#[cfg(unix)]
#[test]
fn batch_relative_keys_equal_the_per_uri_projection() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("batch-relative-keys")?;
    let src = fixture.root.join("src");
    fs::write(src.join("a.rs"), SOURCE_A).map_err(|error| format!("write a.rs: {error}"))?;
    std::os::unix::fs::symlink(src.join("a.rs"), src.join("linked.rs"))
        .map_err(|error| format!("link file: {error}"))?;
    fs::create_dir_all(fixture.root.join("real_dir"))
        .map_err(|error| format!("create dir: {error}"))?;
    fs::write(fixture.root.join("real_dir/b.rs"), SOURCE_A)
        .map_err(|error| format!("write b.rs: {error}"))?;
    std::os::unix::fs::symlink(fixture.root.join("real_dir"), fixture.root.join("dir_link"))
        .map_err(|error| format!("link dir: {error}"))?;
    let outside = fixture.temp.path().join("outside.rs");
    fs::write(&outside, SOURCE_A).map_err(|error| format!("write outside: {error}"))?;
    let paths = [
        src.join("a.rs"),
        src.join("linked.rs"),
        src.join("missing.rs"),
        src.join("nested/missing.rs"),
        fixture.root.join("dir_link/b.rs"),
        fixture.root.join("real_dir/b.rs"),
        fixture.alias.join("src/a.rs"),
        fixture.alias.join("src/missing.rs"),
        outside,
    ];
    let uris = paths
        .iter()
        .map(|path| file_uri_for_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    for root in [&fixture.root, &fixture.alias] {
        let expected = uris
            .iter()
            .filter_map(|uri| crate::lsp::uri::file_uri_relative_to_root(root, uri))
            .collect::<Vec<_>>();
        let batch = crate::lsp::uri::file_uris_relative_to_root(root, &uris);
        if batch != expected || expected.len() < 6 {
            return Err(format!(
                "BEHAVIORAL: batch keys diverged from the per-URI projection under {root:?}\nexpected {expected:?}\nbatch    {batch:?}"
            ));
        }
    }
    Ok(())
}

/// The performance claim of #11763, observed through the projector's counting
/// hook: 40 regular files in two directories cost two directory
/// canonicalizations in total, not one per file, and a second pass over the
/// same URIs on the same projector costs none. Only hosts with the
/// directory-cache fast path (Linux and other non-macOS Unix) make the claim;
/// elsewhere every URI is resolved in full and the count stays zero.
#[cfg(unix)]
#[test]
fn projector_resolves_each_directory_once_not_once_per_file() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("projector-resolution-count")?;
    let src = fixture.root.join("src");
    fs::create_dir_all(fixture.root.join("tests"))
        .map_err(|error| format!("create tests dir: {error}"))?;
    let mut uris = Vec::new();
    for index in 0..20 {
        for dir in [src.clone(), fixture.root.join("tests")] {
            let path = dir.join(format!("f{index}.rs"));
            fs::write(&path, SOURCE_A).map_err(|error| format!("write {path:?}: {error}"))?;
            uris.push(file_uri_for_path(&path)?);
        }
    }
    let mut projector = crate::lsp::uri::RelativeKeyProjector::new(&fixture.root);
    for _ in 0..2 {
        for uri in &uris {
            if projector.relative(uri).is_none() {
                return Err(format!("BEHAVIORAL: {uri:?} was not projected"));
            }
        }
    }
    let expected = if cfg!(all(unix, not(target_os = "macos"))) {
        2
    } else {
        0
    };
    if projector.directory_resolutions() != expected {
        return Err(format!(
            "BEHAVIORAL: {} directory resolutions for 80 projections, expected {expected}",
            projector.directory_resolutions()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn reference_open_rust_index_paths(
    root: &Path,
    store: &DocumentStore,
) -> std::collections::BTreeSet<PathBuf> {
    store
        .documents
        .keys()
        .filter_map(|uri| crate::lsp::uri::file_uri_relative_to_root(root, uri))
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect()
}

#[cfg(unix)]
fn open_for_index_test(store: &mut DocumentStore, path: &Path) -> Result<(), String> {
    store.open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(
            file_uri_for_path(path)?,
            "rust".to_string(),
            1,
            SOURCE_A.to_string(),
        ),
    });
    Ok(())
}

#[cfg(unix)]
#[test]
fn open_rust_index_paths_match_reference_and_bound_canonicalization() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("open-rust-index")?;
    let outside = fixture.temp.path().join("outside.rs");
    fs::write(&outside, SOURCE_A).map_err(|error| format!("write outside: {error}"))?;
    let src = fixture.root.join("src");
    fs::write(src.join("notes.txt"), "x").map_err(|error| format!("write txt: {error}"))?;
    std::os::unix::fs::symlink(src.join("lib.rs"), src.join("link_to_rs.txt"))
        .map_err(|error| format!("link txt: {error}"))?;
    std::os::unix::fs::symlink(src.join("notes.txt"), src.join("link_to_txt.rs"))
        .map_err(|error| format!("link rs: {error}"))?;
    let edge_paths = vec![
        fixture.file.clone(),
        src.join("notes.txt"),
        src.join("link_to_rs.txt"),
        src.join("link_to_txt.rs"),
        src.join("missing.rs"),
        src.join("Makefile"),
        outside,
        fixture.alias.join("src/lib.rs"),
        fixture.alias.join("src/other.rs"),
        fixture.alias.join("src/notes.txt"),
    ];
    let mut store = DocumentStore::default();
    for path in &edge_paths {
        open_for_index_test(&mut store, path)?;
    }
    for root in [&fixture.root, &fixture.alias] {
        let expected = reference_open_rust_index_paths(root, &store);
        if expected.is_empty() {
            return Err("fixture must produce indexed keys".into());
        }
        let uris = store.documents.keys().cloned().collect::<Vec<_>>();
        for uri in &uris {
            store.invalidate_rust_index_key(uri);
        }
        let actual = store.open_rust_index_paths(root);
        if actual != expected {
            return Err(format!("index drifted: {actual:?} != {expected:?}"));
        }
        // A cached second pass must agree too.
        if store.open_rust_index_paths(root) != expected {
            return Err("cached index drifted".into());
        }
    }
    // Non-.rs files under the root never touch the filesystem; .rs files
    // resolve once and then come from the cache.
    let mut bulk = DocumentStore::default();
    for index in 0..200 {
        open_for_index_test(&mut bulk, &src.join(format!("doc_{index}.txt")))?;
    }
    let before = crate::lsp::uri::canonical_projection_count_for_test();
    if !bulk.open_rust_index_paths(&fixture.root).is_empty() {
        return Err("non-rust documents must not be indexed".into());
    }
    let non_rust = crate::lsp::uri::canonical_projection_count_for_test() - before;
    if non_rust != 0 {
        return Err(format!("non-.rs documents canonicalized {non_rust} times"));
    }
    for index in 0..200 {
        open_for_index_test(&mut bulk, &src.join(format!("doc_{index}.rs")))?;
    }
    let mut single = DocumentStore::default();
    open_for_index_test(&mut single, &src.join("doc_0.rs"))?;
    let before = crate::lsp::uri::canonical_projection_count_for_test();
    single.open_rust_index_paths(&fixture.root);
    let per_document = crate::lsp::uri::canonical_projection_count_for_test() - before;
    if per_document == 0 {
        return Err("a .rs document must be resolved at least once".into());
    }
    let before = crate::lsp::uri::canonical_projection_count_for_test();
    let first = bulk.open_rust_index_paths(&fixture.root);
    let first_cost = crate::lsp::uri::canonical_projection_count_for_test() - before;
    // One projector serves the whole cold pass (#11763): where the directory
    // cache applies (Linux and other non-macOS Unix) 200 regular files in one
    // directory cost the same as one, because only the root is canonicalized
    // through the counted path; elsewhere each document is still resolved in
    // full (the root once, then one projection per document).
    let expected_cost = if cfg!(all(unix, not(target_os = "macos"))) {
        per_document
    } else {
        1 + 200 * (per_document - 1)
    };
    if first.len() != 200 || first_cost != expected_cost {
        return Err(format!(
            "first pass: {} keys, {first_cost} projections (expected {expected_cost})",
            first.len()
        ));
    }
    let before = crate::lsp::uri::canonical_projection_count_for_test();
    for _ in 0..5 {
        if bulk.open_rust_index_paths(&fixture.root) != first {
            return Err("repeat refresh changed the index".into());
        }
    }
    let repeat = crate::lsp::uri::canonical_projection_count_for_test() - before;
    if repeat != 0 {
        return Err(format!("repeat refreshes canonicalized {repeat} times"));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn open_rust_index_cache_invalidation_is_scoped_to_the_affected_documents() -> Result<(), String> {
    let fixture = consumed_source_alias_fixture("open-rust-index-invalidate")?;
    let src = fixture.root.join("src");
    let outside = fixture.temp.path().join("outside.rs");
    fs::write(&outside, SOURCE_A).map_err(|error| format!("write outside: {error}"))?;
    let link = src.join("sym.rs");
    std::os::unix::fs::symlink(&fixture.file, &link)
        .map_err(|error| format!("link inside: {error}"))?;
    let key = PathBuf::from("src/sym.rs");
    let uri = file_uri_for_path(&link)?;
    let other_uri = file_uri_for_path(&fixture.file)?;
    let mut store = DocumentStore::default();
    open_for_index_test(&mut store, &link)?;
    open_for_index_test(&mut store, &fixture.file)?;
    let projections = |store: &mut DocumentStore| {
        let before = crate::lsp::uri::canonical_projection_count_for_test();
        let paths = store.open_rust_index_paths(&fixture.root);
        (
            paths,
            crate::lsp::uri::canonical_projection_count_for_test() - before,
        )
    };
    let (paths, cold) = projections(&mut store);
    if !paths.contains(&key) || cold == 0 {
        return Err("in-root symlink must be indexed by a cold resolution".into());
    }
    // Retarget outside the root: containment now fails, but the cache serves
    // the stale key until that document's entry is invalidated.
    fs::remove_file(&link).map_err(|error| format!("unlink: {error}"))?;
    std::os::unix::fs::symlink(&outside, &link)
        .map_err(|error| format!("relink outside: {error}"))?;
    let (paths, count) = projections(&mut store);
    if !paths.contains(&key) || count != 0 {
        return Err("cache must serve the stale key with no resolution".into());
    }
    // Invalidating another document, or an event elsewhere, keeps the link warm.
    store.invalidate_rust_index_key(&other_uri);
    store.invalidate_rust_index_keys_under(&file_uri_for_path(&fixture.temp.path().join("logs"))?);
    store.invalidate_rust_index_keys_under(&file_uri_for_path(&src.join("sym.rs.bak"))?);
    if !store.rust_index_key_is_cached(&uri) || store.rust_index_key_is_cached(&other_uri) {
        return Err("unrelated invalidation must touch only its own document".into());
    }
    let (paths, count) = projections(&mut store);
    // The other document re-resolved (a cold pass costs something) while the
    // retargeted symlink kept its stale cached key.
    if !paths.contains(&key) || count == 0 {
        return Err(format!(
            "only the invalidated document may re-resolve: {count} projections"
        ));
    }
    // An event at the document, or at an ancestor directory, drops it.
    // An ancestor directory event also drops the sibling below it.
    for event in [link.clone(), src.clone()] {
        store.invalidate_rust_index_keys_under(&file_uri_for_path(&event)?);
        if store.rust_index_key_is_cached(&uri) {
            return Err(format!("event at {event:?} must drop the entry"));
        }
        let (paths, count) = projections(&mut store);
        if paths.contains(&key) || count == 0 {
            return Err(format!(
                "retargeted symlink must leave the index after {event:?}: {count}"
            ));
        }
    }
    // Reopen and close drop the entry.
    open_for_index_test(&mut store, &link)?;
    if store.rust_index_key_is_cached(&uri) {
        return Err("reopen must drop the cached entry".into());
    }
    store.open_rust_index_paths(&fixture.root);
    store.close(DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
    });
    if store.rust_index_key_is_cached(&uri) {
        return Err("close must drop the cached entry".into());
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn save_and_watched_file_handlers_invalidate_only_affected_index_keys() -> Result<(), String>
{
    let fixture = consumed_source_alias_fixture("open-rust-index-handlers")?;
    let outside = fixture.temp.path().join("outside.rs");
    fs::write(&outside, SOURCE_A).map_err(|error| format!("write outside: {error}"))?;
    let link = fixture.root.join("src/sym.rs");
    let key = PathBuf::from("src/sym.rs");
    let uri = file_uri_for_path(&link)?;
    let other_uri = file_uri_for_path(&fixture.file)?;
    let unrelated = file_uri_for_path(&fixture.temp.path().join("target/build.log"))?;
    let root = fixture.root.clone();
    let (service, socket) = LspService::new(|client| Backend::new(client, root.clone()));
    drop(socket);
    let backend = service.inner();
    let watched = |target: &tower_lsp_server::ls_types::Uri| {
        tower_lsp_server::ls_types::DidChangeWatchedFilesParams {
            changes: vec![tower_lsp_server::ls_types::FileEvent::new(
                target.clone(),
                tower_lsp_server::ls_types::FileChangeType::CHANGED,
            )],
        }
    };
    let save = |target: &tower_lsp_server::ls_types::Uri, text: &str| DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(target.clone()),
        text: Some(text.to_string()),
    };
    let count_refresh = || {
        let before = crate::lsp::uri::canonical_projection_count_for_test();
        let paths = backend.open_rust_index_paths_for_root(&root);
        (
            paths,
            crate::lsp::uri::canonical_projection_count_for_test() - before,
        )
    };
    for handler in ["save", "watched"] {
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&fixture.file, &link)
            .map_err(|error| format!("link inside: {error}"))?;
        for target in [&uri, &other_uri] {
            backend
                .did_open(DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(
                        target.clone(),
                        "rust".to_string(),
                        1,
                        SOURCE_A.to_string(),
                    ),
                })
                .await;
        }
        let (paths, cold) = count_refresh();
        if !paths.contains(&key) || cold == 0 {
            return Err(format!("{handler}: in-root symlink must be indexed"));
        }
        fs::remove_file(&link).map_err(|error| format!("unlink: {error}"))?;
        std::os::unix::fs::symlink(&outside, &link)
            .map_err(|error| format!("relink outside: {error}"))?;
        // An unrelated event keeps every key warm: no resolution at all.
        if handler == "save" {
            backend.did_save(save(&unrelated, SOURCE_A)).await;
        } else {
            backend.did_change_watched_files(watched(&unrelated)).await;
        }
        let (paths, count) = count_refresh();
        if !paths.contains(&key) || count != 0 {
            return Err(format!(
                "{handler}: unrelated event must keep the cache warm ({count} projections)"
            ));
        }
        // The affected document re-resolves; its sibling does not.
        if handler == "save" {
            backend
                .did_save(save(&uri, &format!("{SOURCE_A}\n// edited\n")))
                .await;
        } else {
            backend.did_change_watched_files(watched(&uri)).await;
        }
        let (paths, count) = count_refresh();
        if paths.contains(&key) || count == 0 {
            return Err(format!(
                "{handler}: affected document must re-resolve alone ({count} projections)"
            ));
        }
        for target in [&uri, &other_uri] {
            backend
                .did_close(DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(target.clone()),
                })
                .await;
        }
    }
    Ok(())
}
