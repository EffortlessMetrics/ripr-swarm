//! Source-preserving recovery through the real gap tool/resource owners.
//! Payload guards establish which byte boundary each case actually reaches.

use super::*;
use serde_json::json;

fn captured_server(
    source: workspace::RefreshSource,
    root: PathBuf,
    evidence: String,
) -> Result<McpServer, String> {
    let server = McpServer::new(WorkspaceStatus::resolve_with_root(None).0, None)
        .map_err(|error| error.to_string())?;
    let outcome = crate::analysis_outcome::AnalysisOutcome::new(
        crate::analysis_outcome::AnalysisOutcomeKind::CompleteWithFindings,
        Default::default(),
        crate::analysis_outcome::AnalysisOutcomeCounts {
            changed_file_count: 1,
            changed_line_count: 1,
            candidate_line_count: 1,
            probe_count: 1,
            finding_count: 1,
        },
        Vec::new(),
    )
    .map_err(|error| error.to_string())?;
    let mut finding = gaps::test_finding()?;
    finding.evidence = vec![evidence];
    let output = crate::app::CheckOutput {
        value_origins: Default::default(),
        schema_version: crate::app::CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
        kani_source_inventory: None,
        harness_projections: Vec::new(),
        tool: "ripr".to_string(),
        mode: crate::app::Mode::Draft,
        root,
        base: Some("captured-nondefault-base".to_string()),
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
    let snapshot = workspace::Snapshot::from_output_admitted(
        &output,
        Some("root:sha256:captured-recovery"),
        &super::super::repair_admission::SeamAdmission::unevaluated(),
        source,
    )
    .map_err(|failure| failure.detail)?;
    let desired_source = match source {
        workspace::RefreshSource::Committed => workspace::RefreshSource::Worktree,
        workspace::RefreshSource::Worktree => workspace::RefreshSource::Committed,
    };
    *server.session.blocking_lock() = workspace::WorkspaceSession {
        last_good: Some(Arc::new(snapshot)),
        // A later failed request cannot relabel the captured evidence.
        last_failure: Some(workspace::AttemptFailure::new(
            workspace::CODE_ANALYSIS_FAILED,
            "a newer request failed",
            "inspect that request",
        )),
        desired_source,
        desired_base: Some("newer-request-base".to_string()),
        ..Default::default()
    };
    Ok(server)
}

fn assert_captured_recovery(recovery: &str, source: workspace::RefreshSource) {
    let expected = match source {
        workspace::RefreshSource::Committed => "--committed --json",
        workspace::RefreshSource::Worktree => "--worktree --json",
    };
    let wrong = match source {
        workspace::RefreshSource::Committed => "--worktree",
        workspace::RefreshSource::Worktree => "--committed",
    };
    assert!(recovery.contains(expected), "{recovery}");
    assert!(!recovery.contains(wrong), "{recovery}");
    assert!(recovery.contains("captured recovery root"), "{recovery}");
    assert!(recovery.contains("captured-nondefault-base"), "{recovery}");
    assert!(!recovery.contains("newer-request-base"), "{recovery}");
    assert!(recovery.contains("--root "), "{recovery}");
    assert!(recovery.contains("--base "), "{recovery}");
    assert!(recovery.contains("not immutable snapshot bytes"));
    assert!(recovery.contains("related tests are capped per finding"));
    assert!(recovery.contains("a large findings array can omit findings"));
}

#[test]
fn captured_source_recovery_covers_raw_tool_and_resource_envelope_overflow() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    for source in [
        workspace::RefreshSource::Committed,
        workspace::RefreshSource::Worktree,
    ] {
        for (case, evidence) in [
            ("raw", "x".repeat(super::super::MAX_RESPONSE_BYTES)),
            ("tool-envelope", "x".repeat(70_000)),
            ("resource-envelope", "\n".repeat(42_000)),
        ] {
            let root = std::env::temp_dir().join("captured recovery root 'quoted'");
            let server = captured_server(source, root, evidence)?;
            runtime.block_on(async {
                let budget = super::super::outer_envelope_budget(&rmcp::model::RequestId::String(
                    "q".repeat(16_000).into(),
                ));
                let raw = server.session.lock().await.get_gap("gap:test:1", None);
                if case == "raw" {
                    let failure = raw
                        .err()
                        .ok_or("raw fixture must exceed the document bound")?;
                    assert_eq!(failure.code, workspace::CODE_RESULT_TOO_LARGE);
                    assert_captured_recovery(&failure.recovery, source);
                } else {
                    let document = raw.map_err(|failure| failure.detail)?;
                    let raw_bytes = serde_json::to_vec(&document)
                        .map_err(|error| error.to_string())?
                        .len();
                    assert!(raw_bytes < super::super::MAX_RESPONSE_BYTES);
                    if case == "tool-envelope" {
                        assert!(protocol::structured_envelope_bytes(&document)? > budget);
                    } else {
                        let pretty = serde_json::to_string_pretty(&document)
                            .map_err(|error| error.to_string())?;
                        let bytes =
                            serde_json::to_vec(&json_envelope("ripr://gap/gap:test:1", pretty))
                                .map_err(|error| error.to_string())?
                                .len();
                        assert!(
                            bytes > budget,
                            "resource fixture must reach its envelope guard"
                        );
                    }
                }
                let mut arguments = serde_json::Map::new();
                arguments.insert("canonical_id".to_string(), json!("gap:test:1"));
                let response = server
                    .get_gap_tool(Some(arguments), budget)
                    .await
                    .map_err(|error| error.to_string())?;
                let rmcp::model::CallToolResponse::Complete(result) = response else {
                    return Err("oversized gap must return a complete typed refusal".to_string());
                };
                let wire = serde_json::to_value(result).map_err(|error| error.to_string())?;
                assert_eq!(wire["isError"], true);
                assert_eq!(
                    wire["structuredContent"]["failure"]["code"],
                    "result_too_large"
                );
                let recovery = wire
                    .pointer("/structuredContent/failure/recovery")
                    .and_then(Value::as_str)
                    .ok_or("tool refusal lost recovery")?;
                assert_captured_recovery(recovery, source);
                assert!(
                    serde_json::to_vec(&wire)
                        .map_err(|error| error.to_string())?
                        .len()
                        < budget
                );
                if case != "tool-envelope" {
                    let error = server
                        .get_gap_resource("ripr://gap/gap:test:1", "gap:test:1", budget)
                        .await
                        .err()
                        .ok_or("oversized resource must refuse")?;
                    let data = error
                        .data
                        .as_ref()
                        .ok_or("resource refusal lost typed data")?;
                    assert_eq!(data["code"], "result_too_large");
                    let recovery = data["recovery"].as_str().ok_or("resource lost recovery")?;
                    assert_captured_recovery(recovery, source);
                }
                Ok::<(), String>(())
            })?;
        }
    }
    Ok(())
}

#[test]
fn escaped_long_root_recovery_refuses_without_truncating_a_command() -> Result<(), String> {
    let root = PathBuf::from("\"\\".repeat(workspace::MAX_FAILURE_RECOVERY_BYTES));
    let server = captured_server(
        workspace::RefreshSource::Worktree,
        root,
        "\n".repeat(42_000),
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let budget = super::super::outer_envelope_budget(&rmcp::model::RequestId::String(
            "q".repeat(16_000).into(),
        ));
        let mut arguments = serde_json::Map::new();
        arguments.insert("canonical_id".to_string(), json!("gap:test:1"));
        let response = server
            .get_gap_tool(Some(arguments), budget)
            .await
            .map_err(|error| error.to_string())?;
        let rmcp::model::CallToolResponse::Complete(result) = response else {
            return Err("long recovery must return a typed refusal".to_string());
        };
        let wire = serde_json::to_value(result).map_err(|error| error.to_string())?;
        assert_eq!(
            wire["structuredContent"]["failure"]["code"],
            "result_too_large"
        );
        assert_eq!(
            wire["structuredContent"]["failure"]["recovery"],
            workspace::RECOVERY_TOO_LONG
        );
        assert!(!workspace::RECOVERY_TOO_LONG.contains("ripr check"));
        assert!(
            serde_json::to_vec(&wire)
                .map_err(|error| error.to_string())?
                .len()
                < budget
        );
        let error = server
            .get_gap_resource("ripr://gap/gap:test:1", "gap:test:1", budget)
            .await
            .err()
            .ok_or("oversized resource envelope must refuse")?;
        assert_eq!(
            error.data.as_ref().ok_or("resource lost data")?["recovery"],
            workspace::RECOVERY_TOO_LONG
        );
        assert!(
            serde_json::to_vec(&error)
                .map_err(|error| error.to_string())?
                .len()
                < budget
        );
        Ok(())
    })
}
