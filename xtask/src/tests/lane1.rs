use super::*;

#[test]
fn lane1_evidence_quality_failure_corpus_is_valid() -> Result<(), String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest must have workspace parent".to_string())?;
    let corpus =
        repo_root.join("fixtures/boundary_gap/expected/evidence-quality-failures/corpus.json");
    let mut violations = Vec::new();
    super::super::validate_lane1_evidence_quality_failure_fixture_corpus_at(
        &corpus,
        &mut violations,
    )?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn lane1_evidence_quality_failure_guard_reports_contract_drift() -> Result<(), String> {
    let root = temp_dir("lane1-evidence-quality-failure-invalid");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "kind": "wrong",
  "schema_version": "0.2",
  "spec": "RIPR-SPEC-9999",
  "source_report": {
    "command": "cargo xtask lane1-evidence-audit",
    "report": "target/ripr/reports/lane1-evidence-audit.json",
    "summary": {
      "raw_headline_gaps": "many"
    }
  },
  "cases": [
    {
      "id": "duplicate_canonical_gap_overcount_suppressions_match_arm",
      "case_kind": "surprise",
      "description": "bad duplicate case",
      "source": "audit",
      "audit_signal": {
        "category": "duplicate_canonical_gap",
        "metric_path": "canonical_gap_groups.largest[0]",
        "observed_count": 1,
        "evidence": "gap"
      },
      "expected_repo_exposure": {
        "source": "repo-exposure-json",
        "evidence_record": {
          "schema_version": "0.2",
          "seam_id": "bad",
          "canonical_gap_id": null,
          "canonical_gap_group_size": null,
          "canonical_gap_reason": null,
          "owner": "owner",
          "location": {"file": "src/lib.rs", "line": 1},
          "seam_kind": "match_arm",
          "grip_class": "activation_unknown",
          "headline_eligible": true,
          "evidence_path": {
            "reach": {"state": "yes"},
            "activate": {},
            "propagate": {},
            "observe": {},
            "discriminate": {}
          },
          "counts": {
            "observed_values": 0,
            "missing_discriminators": 0,
            "static_limitations": 0,
            "related_tests_total": 0
          },
          "top_related_test": null,
          "recommendation": {},
          "actionability": {},
          "calibration": {},
          "static_limitations": []
        }
      },
      "expected_claims": [],
      "must_not_claim": []
    }
  ]
}
"#,
    );

    let mut violations = Vec::new();
    super::super::validate_lane1_evidence_quality_failure_fixture_corpus_at(
        &corpus,
        &mut violations,
    )?;
    let report = violations.join("\n");

    assert!(report.contains("kind must be lane1_evidence_quality_failure_corpus"));
    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("spec must be RIPR-SPEC-0032"));
    assert!(report.contains("source_report.summary"));
    assert!(report.contains("unsupported case_kind surprise"));
    assert!(report.contains("evidence_record.schema_version must be 0.1"));
    assert!(report.contains("is missing string field confidence"));
    assert!(report.contains("top_related_test must be an object"));
    assert!(report.contains("recommendation"));
    assert!(report.contains("actionability.has_concrete_guidance must be boolean"));
    assert!(report.contains("calibration"));
    assert!(report.contains("expected_claims must be a non-empty string array"));
    assert!(report.contains("must_not_claim must be a non-empty string array"));
    assert!(report.contains("must pin canonical_gap_id"));
    assert!(report.contains("must pin group size greater than 1 or pin corrected group size 1"));
    assert!(report.contains("is missing case missing_equality_boundary_discriminator"));
    assert!(report.contains("must include at least one negative_guard case"));
    Ok(())
}

#[test]
fn lane1_repo_exposure_file_completion_check_requires_seams_and_closing_brace() -> Result<(), String>
{
    let root = temp_dir("lane1-repo-exposure-complete");
    let complete = root.join("complete.json");
    write(
        &complete,
        r#"{
  "schema_version": "0.3",
  "seams": []
}
"#,
    );
    if !super::super::lane1_repo_exposure_file_looks_complete(&complete)? {
        return Err("complete repo-exposure JSON should be accepted".to_string());
    }

    let missing_seams = root.join("missing-seams.json");
    write(
        &missing_seams,
        r#"{
  "schema_version": "0.3"
}
"#,
    );
    if super::super::lane1_repo_exposure_file_looks_complete(&missing_seams)? {
        return Err("repo-exposure JSON without seams should be rejected".to_string());
    }

    let truncated = root.join("truncated.json");
    write(
        &truncated,
        r#"{
  "schema_version": "0.3",
  "seams": [
"#,
    );
    if super::super::lane1_repo_exposure_file_looks_complete(&truncated)? {
        return Err("truncated repo-exposure JSON should be rejected".to_string());
    }

    Ok(())
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_times_out_with_trace() -> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-timeout");
    let output_path = root.join("repo-exposure.json");
    write(&output_path, "stale");
    let timeout = Duration::from_millis(5);
    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        timeout,
        |binary, args, output_path_seen, timeout_seen| {
            assert_eq!(binary, Path::new("ripr"));
            assert_eq!(timeout_seen, timeout);
            assert_eq!(args, lane1_evidence_audit_repo_exposure_args().as_slice());
            assert_eq!(output_path_seen, output_path.as_path());
            Ok(TimedFileOutput {
                    status: None,
                    stderr: "ripr_repo_exposure_latency phase=evidence_for_seams_progress status=processed_500_of_37361 duration_ms=9983\n".to_string(),
                    duration: timeout,
                    timed_out: true,
                    stdout_bytes: 0,
                })
        },
    )?;

    let Lane1EvidenceAuditRepoExposureOutcome::TimedOut(diagnostics) = outcome else {
        return Err("timeout should produce a bounded timeout outcome".to_string());
    };
    assert_eq!(diagnostics.status, "timeout");
    assert_eq!(diagnostics.timeout_ms, 5);
    assert_eq!(diagnostics.latency_trace_events_total, 1);
    assert_eq!(
        diagnostics.latency_trace_tail[0],
        RepoExposureLatencyTrace {
            phase: "evidence_for_seams_progress".to_string(),
            status: "processed_500_of_37361".to_string(),
            duration_ms: 9983,
        }
    );
    assert!(
        !output_path.exists(),
        "timed-out repo exposure generation should remove stale or partial output"
    );

    Ok(())
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_accepts_complete_json_after_timeout()
-> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-timeout-complete");
    let output_path = root.join("repo-exposure.json");
    let timeout = Duration::from_millis(5);
    let stdout = "{\n  \"schema_version\": \"0.3\",\n  \"seams\": []\n}\n";
    let stderr = concat!(
        "ripr_repo_exposure_latency phase=evidence_for_seams status=ok duration_ms=112847\n",
        "ripr_repo_exposure_latency phase=total status=sampled_computed duration_ms=115904\n",
    );

    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        timeout,
        |_binary, _args, output_path_seen, _timeout_seen| {
            write(output_path_seen, stdout);
            Ok(TimedFileOutput {
                status: Some(failure_exit_status()),
                stderr: stderr.to_string(),
                duration: Duration::from_millis(8),
                timed_out: true,
                stdout_bytes: stdout.len(),
            })
        },
    )?;

    let Lane1EvidenceAuditRepoExposureOutcome::Complete(diagnostics) = outcome else {
        return Err("complete timeout capture should remain downstream-consumable".to_string());
    };
    assert_eq!(diagnostics.status, "timeout_complete");
    assert_eq!(diagnostics.timeout_ms, 5);
    assert_eq!(diagnostics.stdout_bytes, stdout.len());
    assert_eq!(diagnostics.exit_code, failure_exit_status().code());
    assert_eq!(
        fs::read_to_string(&output_path).map_err(|err| err.to_string())?,
        stdout
    );
    assert_eq!(diagnostics.latency_trace_events_total, 2);
    assert_eq!(
        diagnostics.latency_trace_tail[1],
        RepoExposureLatencyTrace {
            phase: "total".to_string(),
            status: "sampled_computed".to_string(),
            duration_ms: 115904,
        }
    );

    Ok(())
}

#[test]
fn lane1_evidence_audit_default_timeout_preempts_live_abort_window() {
    assert_eq!(
        super::super::LANE1_EVIDENCE_AUDIT_DEFAULT_TIMEOUT_MS,
        240_000
    );
}

#[test]
fn lane1_evidence_audit_timeout_ms_uses_positive_env_override_only() {
    assert_eq!(
        super::super::lane1_evidence_audit_timeout_ms_from_env(None),
        super::super::LANE1_EVIDENCE_AUDIT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        super::super::lane1_evidence_audit_timeout_ms_from_env(Some(String::new())),
        super::super::LANE1_EVIDENCE_AUDIT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        super::super::lane1_evidence_audit_timeout_ms_from_env(Some("0".to_string())),
        super::super::LANE1_EVIDENCE_AUDIT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        super::super::lane1_evidence_audit_timeout_ms_from_env(Some("not-a-timeout".to_string())),
        super::super::LANE1_EVIDENCE_AUDIT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        super::super::lane1_evidence_audit_timeout_ms_from_env(Some("300000".to_string())),
        300_000
    );
}

#[test]
fn lane1_evidence_audit_sample_limit_parser_accepts_positive_integer_only() {
    assert_eq!(
        super::super::parse_lane1_evidence_audit_sample_seam_limit("5000"),
        Some(5000)
    );
    assert_eq!(
        super::super::parse_lane1_evidence_audit_sample_seam_limit(" 7 "),
        Some(7)
    );
    assert_eq!(
        super::super::parse_lane1_evidence_audit_sample_seam_limit("0"),
        None
    );
    assert_eq!(
        super::super::parse_lane1_evidence_audit_sample_seam_limit("not-a-number"),
        None
    );
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_writes_success_json() -> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-success");
    let output_path = root.join("repo-exposure.json");
    let timeout = Duration::from_millis(50);
    let stdout = "{\n  \"schema_version\": \"0.3\",\n  \"seams\": []\n}\n";

    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        timeout,
        |binary, args, output_path_seen, timeout_seen| {
            assert_eq!(binary, Path::new("ripr"));
            assert_eq!(timeout_seen, timeout);
            assert_eq!(args, lane1_evidence_audit_repo_exposure_args().as_slice());
            assert_eq!(output_path_seen, output_path.as_path());
            write(output_path_seen, stdout);
            Ok(TimedFileOutput {
                status: Some(success_exit_status()),
                stderr:
                    "ripr_repo_exposure_latency phase=evidence_for_seams status=ok duration_ms=42\n"
                        .to_string(),
                duration: Duration::from_millis(1),
                timed_out: false,
                stdout_bytes: stdout.len(),
            })
        },
    )?;
    let Lane1EvidenceAuditRepoExposureOutcome::Complete(diagnostics) = outcome else {
        return Err("success should produce a complete repo-exposure outcome".to_string());
    };

    assert_eq!(
        fs::read_to_string(&output_path).map_err(|err| err.to_string())?,
        stdout
    );
    assert_eq!(diagnostics.status, "pass");
    assert_eq!(diagnostics.duration_ms, 1);
    assert_eq!(diagnostics.timeout_ms, 50);
    assert_eq!(diagnostics.latency_trace_events_total, 1);
    assert_eq!(
        diagnostics.latency_trace_tail[0],
        RepoExposureLatencyTrace {
            phase: "evidence_for_seams".to_string(),
            status: "ok".to_string(),
            duration_ms: 42,
        }
    );

    Ok(())
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_limits_incomplete_success_json()
-> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-success-incomplete");
    let output_path = root.join("repo-exposure.json");
    let partial = "{\n  \"schema_version\": \"0.3\",\n  \"seams\": [\n";

    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        Duration::from_millis(50),
        |_binary, _args, output_path_seen, _timeout_seen| {
            write(output_path_seen, partial);
            Ok(TimedFileOutput {
                status: Some(success_exit_status()),
                stderr:
                    "ripr_repo_exposure_latency phase=total status=computed duration_ms=867012\n"
                        .to_string(),
                duration: Duration::from_millis(867),
                timed_out: false,
                stdout_bytes: partial.len(),
            })
        },
    )?;

    let diagnostics =
        lane1_failed_incomplete_outcome(outcome, "incomplete success repo-exposure JSON")?;
    assert_eq!(diagnostics.status, "pass_incomplete");
    assert_eq!(
        diagnostics.failure_reason.as_deref(),
        Some("repo exposure exited successfully but captured JSON was incomplete")
    );
    assert_eq!(diagnostics.exit_code, Some(0));
    assert_eq!(diagnostics.stdout_bytes, partial.len());
    assert!(
        !output_path.exists(),
        "incomplete success capture should be removed"
    );

    let report = lane1_evidence_audit_limited_report(".", diagnostics);
    let json = lane1_evidence_audit_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(value["status"], "advisory");
    assert_eq!(value["run_status"], "limited_incomplete_input");
    assert_eq!(
        value["runtime_status"]["limitation_category"],
        "lane1_repo_exposure_incomplete"
    );
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    assert_eq!(
        value["run_limitations"][0]["category"],
        "lane1_repo_exposure_incomplete"
    );
    assert_eq!(
        value["run_limitations"][0]["run_status"],
        "limited_incomplete_input"
    );
    assert_eq!(
        value["inputs"]["repo_exposure_generation"]["status"],
        "pass_incomplete"
    );
    assert_eq!(value["summary"]["raw_headline_gaps"], 0);

    let packets_json = lane1_actionable_gap_packets_json(&report)?;
    let packets: Value = serde_json::from_str(&packets_json).map_err(|err| err.to_string())?;
    assert_eq!(packets["run_status"], "limited_incomplete_input");
    assert_eq!(packets["summary"]["actionable_gaps"], 0);
    let packets_markdown = lane1_actionable_gap_packets_markdown(&report);
    assert!(packets_markdown.contains("## Runtime Status"));
    assert!(packets_markdown.contains("| State | `limited_incomplete_input` |"));
    assert!(
        packets_markdown.contains("| Limitation category | `lane1_repo_exposure_incomplete` |")
    );
    assert!(packets_markdown.contains("| Downstream consumable | `false` |"));

    let markdown = lane1_evidence_audit_markdown(&report);
    assert!(markdown.contains("Run Limitations"));
    assert!(markdown.contains("lane1_repo_exposure_incomplete"));
    Ok(())
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_limits_missing_success_json() -> Result<(), String>
{
    let root = temp_dir("lane1-repo-exposure-success-missing");
    let output_path = root.join("repo-exposure.json");

    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        Duration::from_millis(50),
        |_binary, _args, _output_path_seen, _timeout_seen| {
            Ok(TimedFileOutput {
                status: Some(success_exit_status()),
                stderr: String::new(),
                duration: Duration::from_millis(2),
                timed_out: false,
                stdout_bytes: 0,
            })
        },
    )?;

    let diagnostics =
        lane1_failed_incomplete_outcome(outcome, "missing success repo-exposure JSON")?;
    assert_eq!(diagnostics.status, "pass_incomplete");
    assert!(
        diagnostics.failure_reason.is_some(),
        "missing success capture should record an inspection failure"
    );
    let reason = diagnostics.failure_reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("failed to open captured repo exposure"),
        "unexpected inspection failure reason: {reason}"
    );
    assert_eq!(diagnostics.exit_code, Some(0));
    assert!(!output_path.exists());

    let report = lane1_evidence_audit_limited_report(".", diagnostics);
    assert_eq!(
        report.run_limitations[0].category,
        "lane1_repo_exposure_incomplete"
    );
    Ok(())
}

#[test]
fn lane1_repo_exposure_cache_store_skip_becomes_named_run_limitation() -> Result<(), String> {
    let generation = Lane1EvidenceAuditRepoExposureGeneration {
        command: "target/debug/ripr check --format repo-exposure-json".to_string(),
        timeout_ms: 1_200_000,
        status: "pass".to_string(),
        failure_reason: None,
        duration_ms: 1_086_312,
        exit_code: Some(0),
        stdout_bytes: 1_234,
        stderr_bytes: 456,
        latency_trace_events_total: 4,
        latency_trace_tail: vec![
            RepoExposureLatencyTrace {
                phase: "cold_compute".to_string(),
                status: "ok".to_string(),
                duration_ms: 1_001_581,
            },
            RepoExposureLatencyTrace {
                phase: "cache_store".to_string(),
                status: "start_classified_38927_limit_20000".to_string(),
                duration_ms: 0,
            },
            RepoExposureLatencyTrace {
                phase: "cache_store".to_string(),
                status: "ignored_skipped_large_entry_seams_38927_limit_20000".to_string(),
                duration_ms: 12,
            },
            RepoExposureLatencyTrace {
                phase: "total".to_string(),
                status: "computed".to_string(),
                duration_ms: 1_086_312,
            },
        ],
    };

    let Some(limitation) = super::super::lane1_repo_exposure_cache_store_limitation(&generation)
    else {
        return Err("cache-store skip trace should produce a named limitation".to_string());
    };
    assert_eq!(
        limitation.category,
        "lane1_repo_exposure_cache_store_skipped_large_entry"
    );
    assert_eq!(limitation.phase, "repo_exposure_cache_store");
    assert_eq!(limitation.input, "classified_seams_38927_limit_20000");
    assert_eq!(limitation.observed_seams, Some(38_927));
    assert_eq!(limitation.cache_limit, Some(20_000));
    assert!(
        limitation
            .repair_route
            .contains("RIPR_REPO_SEAM_CACHE_LIMIT=38927"),
        "repair route should name the configurable cache limit, got {}",
        limitation.repair_route
    );
    Ok(())
}

#[test]
fn lane1_cache_store_skip_status_parser_rejects_malformed_statuses() {
    for status in [
        "ignored_skipped_large_entry_seams_38927",
        "ignored_skipped_large_entry_seams_38927_limit_",
        "ignored_skipped_large_entry_seams_not-a-number_limit_20000",
        "skipped_large_entry_seams_38927_limit_20000",
    ] {
        assert_eq!(
            super::super::lane1_parse_cache_store_skip_status(status),
            None,
            "malformed cache-store skip status should not parse: {status}"
        );
    }
}

#[test]
fn lane1_repo_exposure_large_cache_preflight_skip_becomes_named_run_limitation()
-> Result<(), String> {
    let root = temp_dir("lane1-large-cache-preflight");
    let cache_file = root
        .join("target")
        .join("ripr")
        .join("cache")
        .join("repo-seam-facts")
        .join("v1")
        .join("large.json");
    if let Some(parent) = cache_file.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(&cache_file, vec![0_u8; 11]).map_err(|err| err.to_string())?;

    let Some(limitation) =
        super::super::lane1_repo_exposure_large_cache_preflight_limitation_for_root(&root, 10)?
    else {
        return Err("oversized Lane 1 cache should produce a named limitation".to_string());
    };

    assert_eq!(
        limitation.category,
        "lane1_repo_exposure_large_cache_preflight_skip"
    );
    assert_eq!(limitation.phase, "repo_seam_facts_cache");
    assert!(
        limitation.input.ends_with("target/ripr/cache"),
        "input should name target/ripr/cache, got {}",
        limitation.input
    );
    assert!(
        limitation.repair_route.contains("cargo xtask cache report")
            && limitation
                .repair_route
                .contains("cargo xtask cache gc --dry-run"),
        "repair route should point at cache report/gc, got {}",
        limitation.repair_route
    );

    let report =
        super::super::lane1_evidence_audit_limited_report_from_run_limitation(".", limitation);
    let json = lane1_evidence_audit_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["run_status"], "limited_large_cache_skip");
    assert_eq!(value["runtime_status"]["downstream_consumable"], false);
    assert_eq!(
        value["runtime_status"]["input_kind"],
        "repo-seam-facts-cache"
    );
    assert!(
        value["runtime_status"]["input_path"]
            .as_str()
            .is_some_and(|path| path.ends_with("target/ripr/cache"))
    );
    assert_eq!(value["summary"]["static_limitations_total"], 1);
    assert_eq!(
        value["run_limitations"][0]["category"],
        "lane1_repo_exposure_large_cache_preflight_skip"
    );
    assert_eq!(
        value["run_limitations"][0]["repair_route"],
        "run cargo xtask cache report && cargo xtask cache gc --dry-run --max-size-gb 1 --ttl-days 14, then review and rerun without --dry-run before lane1-evidence-audit"
    );
    Ok(())
}

#[test]
fn lane1_repo_exposure_large_cache_preflight_ignores_missing_cache() -> Result<(), String> {
    let root = temp_dir("lane1-large-cache-preflight-missing");
    let limitation =
        super::super::lane1_repo_exposure_large_cache_preflight_limitation_for_root(&root, 10)?;
    assert!(limitation.is_none());
    Ok(())
}

#[test]
fn lane1_evidence_audit_complete_report_counts_cache_store_run_limitation() -> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-cache-store-limitation");
    let output_path = root.join("repo-exposure.json");
    write(
        &output_path,
        r#"{
  "schema_version": "0.3",
  "scope": "repo",
  "seams": [
    {
      "evidence_record": {"schema_version":"0.1","seam_id":"seam-cache","location":{"file":"src/cache.rs","line":10},"seam_kind":"predicate_boundary","headline_eligible":true,"related_tests_total":0,"related_tests":[],"static_limitations":[],"raw_findings":[]}
    }
  ]
}"#,
    );
    let generation = Lane1EvidenceAuditRepoExposureGeneration {
        command: "target/debug/ripr check --format repo-exposure-json".to_string(),
        timeout_ms: 1_200_000,
        status: "pass".to_string(),
        failure_reason: None,
        duration_ms: 1_086_312,
        exit_code: Some(0),
        stdout_bytes: 1_234,
        stderr_bytes: 456,
        latency_trace_events_total: 2,
        latency_trace_tail: vec![
            RepoExposureLatencyTrace {
                phase: "cache_store".to_string(),
                status: "ignored_skipped_large_entry_seams_38927_limit_20000".to_string(),
                duration_ms: 12,
            },
            RepoExposureLatencyTrace {
                phase: "total".to_string(),
                status: "computed".to_string(),
                duration_ms: 1_086_312,
            },
        ],
    };

    let report =
        lane1_evidence_audit_report_from_complete_repo_exposure(".", &output_path, generation);
    let json = lane1_evidence_audit_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["run_status"], "limited_large_cache_skip");
    assert_eq!(value["runtime_status"]["downstream_consumable"], true);
    assert_eq!(value["summary"]["static_limitations_total"], 1);
    assert_eq!(
        value["static_limitations"]["by_category"]["lane1_repo_exposure_cache_store_skipped_large_entry"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["static_limitations"]["repair_routes"]["report/lane1-audit-bounded-diagnostics"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["run_limitations"][0]["category"],
        "lane1_repo_exposure_cache_store_skipped_large_entry"
    );
    assert_eq!(
        value["run_limitations"][0]["run_status"],
        "limited_large_cache_skip"
    );
    assert_eq!(
        value["run_limitations"][0]["input_kind"],
        "repo-seam-facts-cache"
    );
    assert_eq!(
        value["run_limitations"][0]["input"],
        "classified_seams_38927_limit_20000"
    );
    assert_eq!(
        value["run_limitations"][0]["observed_seams"],
        serde_json::Value::from(38_927)
    );
    assert_eq!(
        value["run_limitations"][0]["cache_limit"],
        serde_json::Value::from(20_000)
    );
    assert!(
        value["run_limitations"][0]["repair_route"]
            .as_str()
            .is_some_and(|route| route.contains("RIPR_REPO_SEAM_CACHE_LIMIT=38927"))
    );
    assert_eq!(
        value["inputs"]["repo_exposure_generation"]["status"],
        "pass"
    );
    Ok(())
}

#[test]
fn lane1_evidence_audit_sampled_report_keeps_counts_and_names_limits() -> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-sampled-report");
    let output_path = root.join("repo-exposure.json");
    write(
        &output_path,
        r#"{
  "schema_version": "0.3",
  "scope": "repo",
  "seams": [
    {
      "evidence_record": {"schema_version":"0.1","seam_id":"seam-sampled","canonical_gap_id":"gap-sampled","canonical_gap_group_size":1,"raw_findings":[{"file":"src/lib.rs","line":10,"kind":"reachable_unrevealed","expression":"amount >= threshold","probe_kind":"predicate_boundary","source_id":"seam-sampled","evidence_record_ref":"seam-sampled"}],"canonical_item":{"canonical_gap_id":"gap-sampled","raw_group_size":1,"canonical_item_kind":"gap","evidence_class":"predicate_boundary","gap_state":"actionable","actionability":"add_boundary_assertion","primary_anchor":{"file":"src/lib.rs","line":10,"kind":"reachable_unrevealed","source_id":"seam-sampled","reason":"record_location"},"raw_spans":[{"file":"src/lib.rs","start_line":10,"end_line":10,"kind":"reachable_unrevealed","source_id":"seam-sampled"}],"why":"Missing equality boundary assertion.","recommended_repair":"Add exact boundary assertion.","repair_route":{"repair_kind":"add_boundary_assertion","target_test_type":"boundary_discriminator","suggested_assertion":"assert_eq!(price(100, 100), 90)"},"related_test":null,"verify_command":"ripr agent verify --root . --json","receipt_command":"cargo xtask receipts emit --kind lane1-actionable-gap","confidence":{"basis":"static_only","notes":[]}},"owner":"pricing::price","location":{"file":"src/lib.rs","line":10},"seam_kind":"predicate_boundary","grip_class":"reachable_unrevealed","headline_eligible":true,"evidence_path":{"reach":{"state":"yes","confidence":"high","summary":"related owner reachable"},"activate":{"state":"unknown","confidence":"low","summary":"missing equality"},"propagate":{"state":"yes","confidence":"medium","summary":"return value"},"observe":{"state":"yes","confidence":"medium","summary":"assertion exists"},"discriminate":{"state":"no","confidence":"medium","summary":"equality missing"}},"missing_discriminators":[{"value":"100","reason":"missing_boundary_equality","flow_sink":{"kind":"return_value","text":"amount - 10","line":11}}],"related_tests_total":0,"related_tests":[],"recommendation":{"action":"add_or_strengthen_test","reason":"missing boundary","assertion_shape":{"kind":"exact_return_value","example":"assert_eq!(price(100, 100), 90)"},"verify_command":"ripr agent verify --root . --json"},"static_limitations":[]}
    }
  ]
}"#,
    );
    let generation = Lane1EvidenceAuditRepoExposureGeneration {
        command: "target/debug/ripr check --format repo-exposure-json".to_string(),
        timeout_ms: 120_000,
        status: "pass".to_string(),
        failure_reason: None,
        duration_ms: 65_000,
        exit_code: Some(0),
        stdout_bytes: 4_096,
        stderr_bytes: 512,
        latency_trace_events_total: 1,
        latency_trace_tail: vec![RepoExposureLatencyTrace {
            phase: "repo_exposure_seam_limit".to_string(),
            status: "limit_5000_of_39663".to_string(),
            duration_ms: 0,
        }],
    };

    let report =
        lane1_evidence_audit_report_from_complete_repo_exposure(".", &output_path, generation);
    let json = lane1_evidence_audit_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(
        value["inputs"]["repo_exposure_generation"]["status"],
        "pass"
    );
    assert_eq!(value["run_status"], "limited_sampled_input");
    assert_eq!(value["runtime_status"]["state"], "limited_sampled_input");
    assert_eq!(value["finding_alignment"]["summary"]["actionable_gaps"], 1);
    assert_eq!(
        value["run_limitations"][0]["run_status"],
        "limited_sampled_input"
    );
    assert_eq!(
        value["run_limitations"][0]["category"],
        "lane1_repo_exposure_sampled"
    );
    assert_eq!(
        value["run_limitations"][0]["input"],
        "repo-exposure-json:limit_5000_of_39663"
    );
    let backlog = super::super::lane1_static_limitation_backlog_json(&report);
    let backlog_packets = backlog["limitation_backlog_packets"]
        .as_array()
        .ok_or_else(|| "expected limitation backlog packets".to_string())?;
    assert!(
        backlog_packets
            .iter()
            .all(|packet| packet["repair_route"] != "report/lane1-audit-bounded-diagnostics"),
        "runtime diagnostics should remain in run_limitations, not packet backlog: {backlog_packets:?}"
    );
    let top_routes = backlog["top_repair_routes"]
        .as_array()
        .ok_or_else(|| "expected top repair routes".to_string())?;
    assert!(
        top_routes
            .iter()
            .all(|route| route["repair_route"] != "report/lane1-audit-bounded-diagnostics"),
        "runtime diagnostics should stay out of packet-backed top repair routes: {top_routes:?}"
    );
    assert!(lane1_evidence_audit_markdown(&report).contains("lane1_repo_exposure_sampled"));

    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn lane1_evidence_audit_limits_incomplete_success_repo_exposure_artifact() -> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-complete-parse-failed");
    let output_path = root.join("repo-exposure.json");
    write(&output_path, "");
    let generation = Lane1EvidenceAuditRepoExposureGeneration {
        command: "ripr check --format repo-exposure-json".to_string(),
        timeout_ms: 50,
        status: "pass".to_string(),
        failure_reason: None,
        duration_ms: 218,
        exit_code: Some(0),
        stdout_bytes: 0,
        stderr_bytes: 123,
        latency_trace_events_total: 1,
        latency_trace_tail: vec![RepoExposureLatencyTrace {
            phase: "evidence_for_seams_progress".to_string(),
            status: "processed_18500_of_38654".to_string(),
            duration_ms: 218802,
        }],
    };

    let report =
        lane1_evidence_audit_report_from_complete_repo_exposure(".", &output_path, generation);
    assert_eq!(
        report
            .repo_exposure_generation
            .as_ref()
            .map(|generation| generation.status.as_str()),
        Some("pass_incomplete")
    );
    assert_eq!(report.run_limitations.len(), 1);
    assert_eq!(
        report.run_limitations[0].category,
        "lane1_repo_exposure_incomplete"
    );
    assert_eq!(report.run_limitations[0].exit_code, Some(0));
    assert_eq!(report.run_limitations[0].stdout_bytes, Some(0));
    assert_eq!(
        report.run_limitations[0].latency_trace_tail[0].status,
        "processed_18500_of_38654"
    );
    Ok(())
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_limits_runner_error() -> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-runner-error");
    let output_path = root.join("repo-exposure.json");
    write(&output_path, "stale");

    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        Duration::from_millis(50),
        |_binary, _args, _output_path_seen, _timeout_seen| {
            Err("failed to spawn repo exposure".to_string())
        },
    )?;

    let Lane1EvidenceAuditRepoExposureOutcome::FailedIncomplete(diagnostics) = outcome else {
        return Err("runner error should produce a bounded limitation".to_string());
    };
    assert_eq!(diagnostics.status, "runner_error");
    assert_eq!(
        diagnostics.failure_reason.as_deref(),
        Some("failed to spawn repo exposure")
    );
    assert!(
        !output_path.exists(),
        "runner errors should not leave stale repo exposure input"
    );

    let report = lane1_evidence_audit_limited_report(".", diagnostics);
    let json = lane1_evidence_audit_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    assert_eq!(
        value["run_limitations"][0]["category"],
        "lane1_repo_exposure_runner_error"
    );
    assert_eq!(
        value["inputs"]["repo_exposure_generation"]["failure_reason"],
        "failed to spawn repo exposure"
    );
    assert_eq!(
        value["static_limitations"]["by_category"]["lane1_repo_exposure_runner_error"],
        1
    );

    let packets_json = lane1_actionable_gap_packets_json(&report)?;
    let packets: Value = serde_json::from_str(&packets_json).map_err(|err| err.to_string())?;
    assert_eq!(packets["summary"]["actionable_gaps"], 0);

    let markdown = lane1_evidence_audit_markdown(&report);
    assert!(markdown.contains("lane1_repo_exposure_runner_error"));
    assert!(markdown.contains("failed to spawn repo exposure"));
    Ok(())
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_accepts_complete_json_from_nonzero_status()
-> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-nonzero-complete");
    let output_path = root.join("repo-exposure.json");
    let stdout = "{\n  \"schema_version\": \"0.3\",\n  \"seams\": []\n}\n";

    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        Duration::from_millis(50),
        |_binary, _args, output_path_seen, _timeout_seen| {
            write(output_path_seen, stdout);
            Ok(TimedFileOutput {
                status: Some(failure_exit_status()),
                stderr: "nonfatal stderr after full JSON".to_string(),
                duration: Duration::from_millis(1),
                timed_out: false,
                stdout_bytes: stdout.len(),
            })
        },
    )?;
    let Lane1EvidenceAuditRepoExposureOutcome::Complete(diagnostics) = outcome else {
        return Err(
            "complete nonzero repo-exposure output should produce a complete outcome".to_string(),
        );
    };

    assert_eq!(diagnostics.status, "nonzero_complete");
    assert_eq!(
        fs::read_to_string(&output_path).map_err(|err| err.to_string())?,
        stdout
    );

    Ok(())
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_limits_incomplete_nonzero_json()
-> Result<(), String> {
    let root = temp_dir("lane1-repo-exposure-nonzero-incomplete");
    let output_path = root.join("repo-exposure.json");
    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        Duration::from_millis(50),
        |_binary, _args, output_path_seen, _timeout_seen| {
            let partial = "{\n  \"schema_version\": \"0.3\",\n  \"seams\": [\n";
            write(output_path_seen, partial);
            Ok(TimedFileOutput {
                status: Some(failure_exit_status()),
                stderr: "repo exposure failed".to_string(),
                duration: Duration::from_millis(1),
                timed_out: false,
                stdout_bytes: partial.len(),
            })
        },
    )?;

    let Lane1EvidenceAuditRepoExposureOutcome::FailedIncomplete(diagnostics) = outcome else {
        return Err(
            "incomplete nonzero repo-exposure JSON should produce a bounded limitation".to_string(),
        );
    };
    assert_eq!(diagnostics.status, "fail");
    assert_eq!(diagnostics.exit_code, failure_exit_status().code());
    assert_eq!(diagnostics.stderr_bytes, "repo exposure failed".len());
    assert!(
        !output_path.exists(),
        "incomplete nonzero repo exposure should remove partial output"
    );
    Ok(())
}

#[test]
fn lane1_evidence_audit_repo_exposure_generation_limits_missing_exit_status() -> Result<(), String>
{
    let root = temp_dir("lane1-repo-exposure-no-status");
    let output_path = root.join("repo-exposure.json");
    let outcome = write_lane1_evidence_audit_repo_exposure_with_runner(
        &output_path,
        Path::new("ripr"),
        Duration::from_millis(50),
        |_binary, _args, output_path_seen, _timeout_seen| {
            let stdout = "{\n  \"schema_version\": \"0.3\",\n  \"seams\": []\n}\n";
            write(output_path_seen, stdout);
            Ok(TimedFileOutput {
                status: None,
                stderr: "missing status stderr".to_string(),
                duration: Duration::from_millis(1),
                timed_out: false,
                stdout_bytes: stdout.len(),
            })
        },
    )?;

    let Lane1EvidenceAuditRepoExposureOutcome::FailedIncomplete(diagnostics) = outcome else {
        return Err("missing exit status should produce a bounded limitation".to_string());
    };
    assert_eq!(diagnostics.status, "missing_exit_status");
    assert_eq!(diagnostics.exit_code, None);
    assert_eq!(diagnostics.stderr_bytes, "missing status stderr".len());
    assert!(
        !output_path.exists(),
        "missing exit status should remove captured repo exposure"
    );
    Ok(())
}

#[test]
fn lane1_evidence_audit_timeout_error_uses_stderr_tail_without_trace() {
    let output = TimedFileOutput {
        status: None,
        stderr: "first\nsecond\nthird\n".to_string(),
        duration: Duration::from_millis(10),
        timed_out: true,
        stdout_bytes: 0,
    };
    let err = lane1_evidence_audit_timeout_error(
        Path::new("ripr"),
        &lane1_evidence_audit_repo_exposure_args(),
        Duration::from_millis(10),
        &output,
    );
    assert!(err.contains("timed out after 10 ms"));
    assert!(err.contains("stderr tail:"));
    assert!(err.contains("first\nsecond\nthird"));
}

#[test]
fn lane1_evidence_audit_limited_report_names_timeout_limitation() -> Result<(), String> {
    let generation = Lane1EvidenceAuditRepoExposureGeneration {
        command: "ripr check --format repo-exposure-json".to_string(),
        timeout_ms: 12,
        status: "timeout".to_string(),
        failure_reason: None,
        duration_ms: 13,
        exit_code: None,
        stdout_bytes: 0,
        stderr_bytes: 91,
        latency_trace_events_total: 1,
        latency_trace_tail: vec![RepoExposureLatencyTrace {
            phase: "evidence_for_seams_progress".to_string(),
            status: "processed_500_of_38445".to_string(),
            duration_ms: 11,
        }],
    };
    let report = lane1_evidence_audit_limited_report(".", generation);
    let json = lane1_evidence_audit_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(
        value["run_limitations"][0]["category"],
        "lane1_repo_exposure_timeout"
    );
    assert_eq!(
        value["run_limitations"][0]["repair_route"],
        "inspect repo-exposure latency trace, increase RIPR_LANE1_EVIDENCE_AUDIT_TIMEOUT_MS for slower machines, or add fixture-backed analyzer narrowing for the slow phase"
    );
    assert_eq!(
        value["static_limitations"]["by_category"]["lane1_repo_exposure_timeout"],
        1
    );
    let backlog = super::super::lane1_static_limitation_backlog_json(&report);
    assert_eq!(
        backlog["top_categories"][0]["repair_route"],
        "report/lane1-audit-bounded-diagnostics"
    );
    let top_routes = backlog["top_repair_routes"]
        .as_array()
        .ok_or_else(|| "expected top repair routes".to_string())?;
    assert!(
        top_routes
            .iter()
            .all(|route| route["repair_route"] != "report/lane1-audit-bounded-diagnostics"),
        "report-only runtime diagnostics should stay out of packet-backed top repair routes: {top_routes:?}"
    );
    assert_eq!(value["summary"]["static_limitations_total"], 1);
    assert_eq!(
        value["inputs"]["repo_exposure_generation"]["latency_trace_tail"][0]["phase"],
        "evidence_for_seams_progress"
    );

    let markdown = lane1_evidence_audit_markdown(&report);
    assert!(markdown.contains("Run Limitations"));
    assert!(markdown.contains("| Static limitations | 1 |"));
    assert!(markdown.contains("lane1_repo_exposure_timeout"));
    assert!(markdown.contains("processed_500_of_38445"));
    Ok(())
}

#[test]
fn lane1_evidence_audit_limited_report_names_incomplete_limitation() -> Result<(), String> {
    let generation = Lane1EvidenceAuditRepoExposureGeneration {
        command: "ripr check --format repo-exposure-json".to_string(),
        timeout_ms: 1200,
        status: "fail".to_string(),
        failure_reason: None,
        duration_ms: 540,
        exit_code: Some(1),
        stdout_bytes: 0,
        stderr_bytes: 91,
        latency_trace_events_total: 1,
        latency_trace_tail: vec![RepoExposureLatencyTrace {
            phase: "evidence_for_seams_progress".to_string(),
            status: "processed_28500_of_38521".to_string(),
            duration_ms: 537,
        }],
    };
    let report = lane1_evidence_audit_limited_report(".", generation);
    let json = lane1_evidence_audit_json(&report)?;
    let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(
        value["run_limitations"][0]["category"],
        "lane1_repo_exposure_incomplete"
    );
    assert_eq!(
        value["run_limitations"][0]["summary"],
        "Lane 1 repo-exposure generation ended before producing complete repo-exposure JSON; partial repo-exposure JSON was discarded and no user test debt is claimed from this limited artifact."
    );
    assert_eq!(
        value["static_limitations"]["by_category"]["lane1_repo_exposure_incomplete"],
        1
    );
    assert_eq!(value["summary"]["static_limitations_total"], 1);

    let markdown = lane1_evidence_audit_markdown(&report);
    assert!(markdown.contains("| Static limitations | 1 |"));
    assert!(markdown.contains("lane1_repo_exposure_incomplete"));
    assert!(markdown.contains("processed_28500_of_38521"));
    Ok(())
}

#[test]
fn lane1_evidence_audit_preserves_case_variant_discriminator_values_as_rows() -> Result<(), String>
{
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "path-upper",
                  "headline_eligible": true,
                  "file": "src/path.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "path-upper",
                    "location": {"file": "src/path.rs", "line": 10},
                    "seam_kind": "predicate_boundary",
                    "missing_discriminators": [
                      {"value": "Path (boundary value)", "reason": "boundary value not observed"}
                    ],
                    "related_tests_total": 0,
                    "related_tests": [],
                    "static_limitations": [],
                    "raw_findings": []
                  }
                },
                {
                  "seam_id": "path-lower",
                  "headline_eligible": true,
                  "file": "src/path.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "path-lower",
                    "location": {"file": "src/path.rs", "line": 20},
                    "seam_kind": "predicate_boundary",
                    "missing_discriminators": [
                      {"value": "path (boundary value)", "reason": "boundary value not observed"}
                    ],
                    "related_tests_total": 0,
                    "related_tests": [],
                    "static_limitations": [],
                    "raw_findings": []
                  }
                }
              ]
            }"#,
    )?;
    let json = lane1_evidence_audit_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let by_value = &value["missing_discriminator_classes"]["by_value"];

    assert!(
        by_value.is_array(),
        "missing discriminator values must be rows, not object keys"
    );
    assert_eq!(audit_count_row(by_value, "Path (boundary value)")?, 1);
    assert_eq!(audit_count_row(by_value, "path (boundary value)")?, 1);
    Ok(())
}

#[test]
fn lane1_evidence_audit_counts_quality_gaps_from_evidence_record() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(".", lane1_audit_sample_json())?;
    let json = lane1_evidence_audit_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["summary"]["seams_total"], 4);
    assert_eq!(value["summary"]["raw_headline_gaps"], 4);
    assert_eq!(value["summary"]["evidence_records_total"], 3);
    assert_eq!(value["summary"]["evidence_records_missing"], 1);
    assert_eq!(value["summary"]["canonical_gap_groups_total"], 1);
    assert_eq!(value["summary"]["duplicate_looking_groups_total"], 1);
    assert_eq!(value["summary"]["missing_discriminators_total"], 2);
    assert_eq!(value["summary"]["static_limitations_total"], 1);
    assert_eq!(value["summary"]["low_or_opaque_top_related_tests"], 1);
    assert_eq!(value["summary"]["calibrated_records"], 1);
    assert_eq!(value["summary"]["uncalibrated_records"], 2);
    assert_eq!(
        value["canonical_gap_groups"]["largest"][0]["canonical_gap_id"],
        "gap:shared"
    );
    assert_eq!(
        value["canonical_gap_groups"]["largest"][0]["count"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        audit_count_row(
            &value["missing_discriminator_classes"]["by_reason"],
            "boundary value not observed"
        )?,
        2
    );
    assert_eq!(
        audit_count_row(
            &value["missing_discriminator_classes"]["by_value"],
            "amount == threshold"
        )?,
        2
    );
    assert_eq!(
        audit_count_row(
            &value["static_limitations"]["by_reason"],
            "opaque helper value"
        )?,
        1
    );
    assert_eq!(
        value["static_limitations"]["by_category"]["opaque_helper_call"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["static_limitations"]["repair_routes"]["analysis/oracle-semantics-audit-fixes"],
        serde_json::Value::from(1)
    );
    let actionable_packets_json = lane1_actionable_gap_packets_json(&report)?;
    let actionable_packets: serde_json::Value =
        serde_json::from_str(&actionable_packets_json).map_err(|err| err.to_string())?;
    assert_eq!(
        actionable_packets["static_limitation_backlog"]["top_categories"][0]["category"],
        "opaque_helper_call"
    );
    assert_eq!(
        actionable_packets["static_limitation_backlog"]["top_categories"][0]["repair_route"],
        "analysis/oracle-semantics-audit-fixes"
    );
    assert_eq!(
        actionable_packets["static_limitation_backlog"]["top_repair_routes"][0]["repair_route"],
        "analysis/oracle-semantics-audit-fixes"
    );
    assert_eq!(
        value["related_test_ranking"]["top_confidence_counts"]["low"],
        serde_json::Value::from(1)
    );
    let alignment = &value["finding_alignment"]["summary"];
    assert_eq!(alignment["raw_signals"], serde_json::Value::from(3));
    assert_eq!(alignment["canonical_items"], serde_json::Value::from(3));
    assert_eq!(
        alignment["aligned_raw_findings"],
        serde_json::Value::from(3)
    );
    assert_eq!(alignment["actionable_gaps"], serde_json::Value::from(2));
    assert_eq!(alignment["static_limitations"], serde_json::Value::from(1));
    assert_eq!(
        alignment["calibrated_supported"],
        serde_json::Value::from(1)
    );
    assert_eq!(alignment["uncalibrated"], serde_json::Value::from(2));
    assert_eq!(
        alignment["presentation_text_total"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        alignment["finding_alignment_raw_signals_total"],
        serde_json::Value::from(3)
    );
    let coverage = &value["finding_alignment"]["coverage"];
    assert_eq!(
        coverage["static_unknown_without_named_limitation"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        coverage["canonical_items_without_repair_route"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        coverage["canonical_items_without_verify_command"],
        serde_json::Value::from(0)
    );
    let top_lists = &value["finding_alignment"]["actionable_gap_top_lists"];
    assert_eq!(
        top_lists["top_actionable_gap_classes"][0]["label"],
        "predicate_boundary"
    );
    assert_eq!(
        top_lists["top_actionable_gap_classes"][0]["count"],
        serde_json::Value::from(2)
    );
    assert_eq!(top_lists["top_actionable_files"][0]["label"], "src/lib.rs");
    assert_eq!(
        top_lists["top_repair_kinds"][0]["label"],
        "add_boundary_assertion"
    );
    assert_eq!(
        top_lists["top_missing_discriminator_kinds"][0]["label"],
        "return_value"
    );
    assert!(
        top_lists["top_static_limitation_reasons"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "non-action static limitations should stay out of actionable top lists"
    );
    let runtime_rows = value["finding_alignment"]["runtime_confidence_by_class"]
        .as_array()
        .ok_or_else(|| "runtime confidence rows should be an array".to_string())?;
    let predicate_runtime = runtime_rows
        .iter()
        .find(|row| row["evidence_class"] == "predicate_boundary")
        .ok_or_else(|| "missing predicate_boundary runtime confidence row".to_string())?;
    assert_eq!(
        predicate_runtime["canonical_items"],
        serde_json::Value::from(2)
    );
    assert_eq!(predicate_runtime["static_only"], serde_json::Value::from(2));
    assert_eq!(
        predicate_runtime["uncalibrated"],
        serde_json::Value::from(2)
    );
    let call_runtime = runtime_rows
        .iter()
        .find(|row| row["evidence_class"] == "call_presence")
        .ok_or_else(|| "missing call_presence runtime confidence row".to_string())?;
    assert_eq!(
        call_runtime["calibrated_supported"],
        serde_json::Value::from(1)
    );
    assert_eq!(call_runtime["uncalibrated"], serde_json::Value::from(0));
    assert_eq!(
        value["calibration_availability"]["runtime_confidence_by_class"],
        value["finding_alignment"]["runtime_confidence_by_class"]
    );
    let class_rows = coverage["alignment_coverage_by_class"]
        .as_array()
        .ok_or_else(|| "alignment coverage rows should be an array".to_string())?;
    let predicate = class_rows
        .iter()
        .find(|row| row["evidence_class"] == "predicate_boundary")
        .ok_or_else(|| "missing predicate_boundary coverage row".to_string())?;
    assert_eq!(predicate["canonical_items"], serde_json::Value::from(2));
    assert_eq!(predicate["actionable_items"], serde_json::Value::from(2));
    let call_presence = class_rows
        .iter()
        .find(|row| row["evidence_class"] == "call_presence")
        .ok_or_else(|| "missing call_presence coverage row".to_string())?;
    assert_eq!(
        call_presence["static_limitation_items"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        call_presence["static_limitation_categories"]["opaque_helper_call"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        call_presence["static_limitation_repair_routes"]["analysis/oracle-semantics-audit-fixes"],
        serde_json::Value::from(1)
    );
    let work_queue = coverage["evidence_class_work_queue"]
        .as_array()
        .ok_or_else(|| "evidence class work queue should be an array".to_string())?;
    let top_work_item = work_queue
        .first()
        .ok_or_else(|| "evidence class work queue should not be empty".to_string())?;
    assert_eq!(top_work_item["evidence_class"], "predicate_boundary");
    assert_eq!(
        top_work_item["dominant_signal"],
        "actionable_canonical_gaps"
    );
    assert_eq!(
        top_work_item["next_repair"],
        "dogfood/actionable-gap-repair-loop"
    );

    let fields = value["evidence_record_field_health"]
        .as_array()
        .ok_or_else(|| "field health should be an array".to_string())?;
    let canonical = fields
        .iter()
        .find(|field| field["field"] == "canonical_gap_id")
        .ok_or_else(|| "canonical_gap_id health missing".to_string())?;
    assert_eq!(canonical["present"], serde_json::Value::from(2));
    assert_eq!(canonical["null"], serde_json::Value::from(1));
    Ok(())
}

#[test]
fn lane1_evidence_audit_reports_alignment_coverage_holes() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "raw-config",
                  "headline_eligible": true,
                  "file": "src/policy.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "seam-config",
                    "canonical_gap_id": null,
                    "owner": "policy::labels",
                    "location": {"file": "src/policy.rs", "line": 10},
                    "seam_kind": "config_or_policy_constant",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 0,
                    "related_tests": [],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "alignment missing", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [],
                    "raw_findings": [
                      {
                        "file": "src/policy.rs",
                        "line": 10,
                        "kind": "exposed",
                        "probe_kind": "config_or_policy_constant",
                        "expression": "pub const POLICY_LABEL: &str ="
                      },
                      {
                        "file": "src/policy.rs",
                        "line": 10,
                        "kind": "static_unknown",
                        "probe_kind": "static_unknown",
                        "expression": "pub const POLICY_LABEL: &str = \"internal\";"
                      }
                    ]
                  }
                }
              ]
            }"#,
    )?;
    let json = lane1_evidence_audit_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let coverage = &value["finding_alignment"]["coverage"];

    assert_eq!(
        coverage["unaligned_raw_findings_by_class"]["config_or_policy_constant"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        coverage["alignment_coverage_by_class"][0]["unaligned_raw_findings"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        coverage["top_unaligned_examples"][0]["evidence_class"],
        "config_or_policy_constant"
    );
    assert_eq!(
        coverage["top_unaligned_examples"][0]["file"],
        "src/policy.rs"
    );
    assert_eq!(
        coverage["top_unaligned_examples"][0]["line"],
        serde_json::Value::from(10)
    );
    assert_eq!(coverage["top_unaligned_examples"][0]["kind"], "exposed");
    assert_eq!(
        coverage["top_unaligned_examples"][0]["expression"],
        "pub const POLICY_LABEL: &str ="
    );
    assert_eq!(
        coverage["top_unaligned_examples"][0]["reason"],
        "missing canonical_item"
    );
    assert_eq!(
        coverage["same_line_duplicate_groups"][0]["raw_findings"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        coverage["same_line_duplicate_groups"][0]["file"],
        "src/policy.rs"
    );
    assert_eq!(
        coverage["same_line_duplicate_groups"][0]["line"],
        serde_json::Value::from(10)
    );
    assert_eq!(
        coverage["same_line_duplicate_groups"][0]["evidence_classes"][0],
        "config_or_policy_constant"
    );
    assert_eq!(
        coverage["same_line_duplicate_groups"][0]["example_expression"],
        "pub const POLICY_LABEL: &str ="
    );
    assert_eq!(
        coverage["same_line_duplicate_groups"][0]["kinds"][0],
        "exposed"
    );
    assert_eq!(
        coverage["same_line_duplicate_groups"][0]["kinds"][1],
        "static_unknown"
    );

    let markdown = lane1_evidence_audit_markdown(&report);
    assert!(markdown.contains("Unaligned raw finding class"));
    assert!(markdown.contains("| Evidence class | File | Line | Kind | Reason | Expression |"));
    assert!(markdown.contains(
        "| File | Line | Raw findings | Evidence classes | Kinds | Example expression |"
    ));
    Ok(())
}

#[test]
fn lane1_static_limitation_backlog_emits_analyzer_packets() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "idx-offset-local",
                  "headline_eligible": true,
                  "file": "src/window.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "idx-offset-local",
                    "canonical_gap_id": "gap:idx-offset-local",
                    "owner": "window::read",
                    "location": {"file": "src/window.rs", "line": 44},
                    "seam_kind": "predicate_boundary",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 1,
                    "related_tests": [],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "local boundary operand unresolved", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [
                      {
                        "stage": "activate",
                        "state": "unknown",
                        "reason": "local/computed operand cannot be mapped to a safe test input",
                        "category": "activation_boundary_input_unresolved",
                        "repair_route": "analysis/local-computed-boundary-operand-resolution"
                      }
                    ],
                    "raw_findings": [
                      {"file": "src/window.rs", "line": 44, "kind": "static_unknown", "probe_kind": "predicate_boundary", "expression": "idx >= offset"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:idx-offset-local",
                      "canonical_item_kind": "limitation",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "raw_findings": [
                        {"file": "src/window.rs", "line": 44, "kind": "static_unknown", "expression": "idx >= offset"}
                      ],
                      "raw_group_size": 1,
                      "why": "local/computed boundary operand cannot be mapped to a safe test input",
                      "recommended_repair": "Improve local/computed operand resolution before emitting a repair packet.",
                      "verify_command": null,
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let actionable_json = lane1_actionable_gap_packets_json(&report)?;
    let value: serde_json::Value =
        serde_json::from_str(&actionable_json).map_err(|err| err.to_string())?;
    let audit_json = lane1_evidence_audit_json(&report)?;
    let audit_value: serde_json::Value =
        serde_json::from_str(&audit_json).map_err(|err| err.to_string())?;
    assert_eq!(
        audit_value["static_limitation_backlog"]["limitation_backlog_packets"][0]["packet_id"],
        "limitation:activation_boundary_input_unresolved:analysis-local-computed-boundary-operand-resolution"
    );
    let packet = &value["static_limitation_backlog"]["limitation_backlog_packets"][0];
    assert_eq!(
        packet["packet_id"],
        "limitation:activation_boundary_input_unresolved:analysis-local-computed-boundary-operand-resolution"
    );
    assert_eq!(
        packet["limitation_category"],
        "activation_boundary_input_unresolved"
    );
    assert_eq!(
        packet["repair_route"],
        "analysis/local-computed-boundary-operand-resolution"
    );
    assert_eq!(
        value["static_limitation_backlog"]["top_categories"][0]["repair_route"],
        "analysis/local-computed-boundary-operand-resolution"
    );
    assert_eq!(packet["signal_count"], serde_json::Value::from(1));
    assert_eq!(packet["dominant_evidence_class"], "predicate_boundary");
    assert_eq!(
        packet["sample_canonical_gap_ids"][0],
        "gap:idx-offset-local"
    );
    assert_eq!(
        packet["sample_sources"][0]["source_file"],
        serde_json::Value::from("src/window.rs")
    );
    assert_eq!(
        packet["sample_sources"][0]["line"],
        serde_json::Value::from(44)
    );
    assert_eq!(
        packet["sample_sources"][0]["expression"],
        serde_json::Value::from("idx >= offset")
    );
    assert_eq!(
        packet["sample_sources"][0]["limitation_reason"],
        serde_json::Value::from("local/computed operand cannot be mapped to a safe test input")
    );
    assert!(
        packet["why_not_actionable"]
            .as_str()
            .is_some_and(|why| why.contains("activation inputs"))
    );
    assert!(packet["unlock_condition"].as_str().is_some_and(|unlock| {
        unlock.contains("analysis/local-computed-boundary-operand-resolution")
    }));
    assert!(packet["non_claims"].as_array().is_some_and(|claims| {
        claims
            .iter()
            .any(|claim| claim == "do not invent exact candidate values")
    }));
    assert!(packet["non_claims"].as_array().is_some_and(|claims| {
        claims
            .iter()
            .any(|claim| claim == "do not invent exact boundary candidate values")
    }));
    assert_eq!(
        value["summary"]["actionable_gaps"],
        serde_json::Value::from(0)
    );
    assert_eq!(value["packets"].as_array().map(Vec::len), Some(0));

    let markdown = lane1_actionable_gap_packets_markdown(&report);
    assert!(markdown.contains("### Backlog Packets"));
    assert!(markdown.contains(
            "limitation:activation_boundary_input_unresolved:analysis-local-computed-boundary-operand-resolution"
        ));
    assert!(markdown.contains("analysis/local-computed-boundary-operand-resolution"));
    Ok(())
}

#[test]
fn lane1_static_limitation_backlog_splits_same_category_by_repair_route() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "idx-offset-local",
                  "headline_eligible": true,
                  "file": "src/window.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "idx-offset-local",
                    "canonical_gap_id": "gap:idx-offset-local",
                    "owner": "window::read",
                    "location": {"file": "src/window.rs", "line": 44},
                    "seam_kind": "predicate_boundary",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 1,
                    "related_tests": [],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "local boundary operand unresolved", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [
                      {
                        "stage": "activate",
                        "state": "unknown",
                        "reason": "local/computed operand cannot be mapped to a safe test input",
                        "category": "activation_boundary_input_unresolved",
                        "repair_route": "analysis/local-computed-boundary-operand-resolution"
                      }
                    ],
                    "raw_findings": [
                      {"file": "src/window.rs", "line": 44, "kind": "static_unknown", "probe_kind": "predicate_boundary", "expression": "idx >= offset"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:idx-offset-local",
                      "canonical_item_kind": "limitation",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "raw_findings": [
                        {"file": "src/window.rs", "line": 44, "kind": "static_unknown", "expression": "idx >= offset"}
                      ],
                      "raw_group_size": 1,
                      "why": "local/computed boundary operand cannot be mapped to a safe test input",
                      "recommended_repair": "Improve local/computed operand resolution before emitting a repair packet.",
                      "verify_command": null,
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                },
                {
                  "seam_id": "idx-offset-iterator",
                  "headline_eligible": true,
                  "file": "src/window.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "idx-offset-iterator",
                    "canonical_gap_id": "gap:idx-offset-iterator",
                    "owner": "window::scan",
                    "location": {"file": "src/window.rs", "line": 77},
                    "seam_kind": "predicate_boundary",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 1,
                    "related_tests": [],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "iterator boundary operand unresolved", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [
                      {
                        "stage": "activate",
                        "state": "unknown",
                        "reason": "iterator operand cannot be mapped to a safe test input",
                        "category": "activation_boundary_input_unresolved",
                        "repair_route": "analysis/iterator-boundary-operand-resolution"
                      }
                    ],
                    "raw_findings": [
                      {"file": "src/window.rs", "line": 77, "kind": "static_unknown", "probe_kind": "predicate_boundary", "expression": "idx >= offset"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:idx-offset-iterator",
                      "canonical_item_kind": "limitation",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "raw_findings": [
                        {"file": "src/window.rs", "line": 77, "kind": "static_unknown", "expression": "idx >= offset"}
                      ],
                      "raw_group_size": 1,
                      "why": "iterator boundary operand cannot be mapped to a safe test input",
                      "recommended_repair": "Improve iterator operand resolution before emitting a repair packet.",
                      "verify_command": null,
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                },
                {
                  "seam_id": "call-name-member",
                  "headline_eligible": true,
                  "file": "src/window.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "call-name-member",
                    "canonical_gap_id": "gap:call-name-member",
                    "owner": "window::filter_call",
                    "location": {"file": "src/window.rs", "line": 88},
                    "seam_kind": "predicate_boundary",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 1,
                    "related_tests": [],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "member boundary operand unresolved", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [
                      {
                        "stage": "activate",
                        "state": "unknown",
                        "reason": "member boundary operand cannot be mapped to a safe test input",
                        "category": "activation_boundary_input_unresolved",
                        "repair_route": "analysis/local-member-boundary-operand-resolution"
                      }
                    ],
                    "raw_findings": [
                      {"file": "src/window.rs", "line": 88, "kind": "static_unknown", "probe_kind": "predicate_boundary", "expression": "call.name != owner_name"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:call-name-member",
                      "canonical_item_kind": "limitation",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "raw_findings": [
                        {"file": "src/window.rs", "line": 88, "kind": "static_unknown", "expression": "call.name != owner_name"}
                      ],
                      "raw_group_size": 1,
                      "why": "member boundary operand cannot be mapped to a safe test input",
                      "recommended_repair": "Improve member operand resolution before emitting a repair packet.",
                      "verify_command": null,
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let actionable_json = lane1_actionable_gap_packets_json(&report)?;
    let value: serde_json::Value =
        serde_json::from_str(&actionable_json).map_err(|err| err.to_string())?;
    let packets = value["static_limitation_backlog"]["limitation_backlog_packets"]
        .as_array()
        .ok_or_else(|| "missing limitation backlog packets".to_string())?;

    let local_packet = packets
            .iter()
            .find(|packet| {
                packet["packet_id"]
                    == "limitation:activation_boundary_input_unresolved:analysis-local-computed-boundary-operand-resolution"
            })
            .ok_or_else(|| "missing local/computed backlog packet".to_string())?;
    let iterator_packet = packets
            .iter()
            .find(|packet| {
                packet["packet_id"]
                    == "limitation:activation_boundary_input_unresolved:analysis-iterator-boundary-operand-resolution"
            })
            .ok_or_else(|| "missing iterator backlog packet".to_string())?;
    let member_packet = packets
            .iter()
            .find(|packet| {
                packet["packet_id"]
                    == "limitation:activation_boundary_input_unresolved:analysis-local-member-boundary-operand-resolution"
            })
            .ok_or_else(|| "missing member backlog packet".to_string())?;

    assert_eq!(packets.len(), 3);
    assert_eq!(
        local_packet["limitation_category"],
        "activation_boundary_input_unresolved"
    );
    assert_eq!(
        iterator_packet["limitation_category"],
        "activation_boundary_input_unresolved"
    );
    assert_eq!(
        member_packet["limitation_category"],
        "activation_boundary_input_unresolved"
    );
    assert_eq!(
        local_packet["repair_route"],
        "analysis/local-computed-boundary-operand-resolution"
    );
    assert_eq!(
        iterator_packet["repair_route"],
        "analysis/iterator-boundary-operand-resolution"
    );
    assert_eq!(
        member_packet["repair_route"],
        "analysis/local-member-boundary-operand-resolution"
    );
    assert_eq!(
        local_packet["sample_canonical_gap_ids"][0],
        "gap:idx-offset-local"
    );
    assert_eq!(
        iterator_packet["sample_canonical_gap_ids"][0],
        "gap:idx-offset-iterator"
    );
    assert_eq!(
        member_packet["sample_canonical_gap_ids"][0],
        "gap:call-name-member"
    );
    assert_eq!(value["packets"].as_array().map(Vec::len), Some(0));
    Ok(())
}

#[test]
fn lane1_static_limitation_backlog_splits_target_affinity_subroutes() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "bare-alias-target-affinity",
                  "headline_eligible": true,
                  "file": "tests/target_affinity.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "bare-alias-target-affinity",
                    "canonical_gap_id": "gap:bare-alias-target-affinity",
                    "owner": "target_affinity::render_pipeline",
                    "location": {"file": "tests/target_affinity.rs", "line": 12},
                    "seam_kind": "call_presence",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 1,
                    "related_tests": [
                      {"test_name": "bare_alias_mentions_target", "file": "tests/target_affinity.rs", "line": 12, "relation_reason": "assertion_target_affinity", "relation_confidence": "medium", "oracle_kind": "exact", "oracle_strength": "strong"}
                    ],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "bare alias target affinity cannot prove owner call", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [
                      {
                        "stage": "activate",
                        "state": "unknown",
                        "reason": "call-presence target affinity is present but the bare alias cannot be resolved",
                        "category": "activation_owner_call_absent_call_presence_target_affinity",
                        "limitation_subroute": "bare_alias_unsupported",
                        "repair_route": "analysis/call-presence-target-affinity-owner-call-tracing"
                      }
                    ],
                    "raw_findings": [
                      {"file": "tests/target_affinity.rs", "line": 12, "kind": "static_unknown", "probe_kind": "call_presence", "expression": "format_output()"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:bare-alias-target-affinity",
                      "canonical_item_kind": "limitation",
                      "evidence_class": "call_presence",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "raw_findings": [
                        {"file": "tests/target_affinity.rs", "line": 12, "kind": "static_unknown", "expression": "format_output()"}
                      ],
                      "raw_group_size": 1,
                      "why": "bare alias target affinity cannot prove owner call",
                      "recommended_repair": "Keep this as analyzer backlog until alias resolution is explicit.",
                      "verify_command": null,
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                },
                {
                  "seam_id": "ambiguous-import-target-affinity",
                  "headline_eligible": true,
                  "file": "tests/target_affinity.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "ambiguous-import-target-affinity",
                    "canonical_gap_id": "gap:ambiguous-import-target-affinity",
                    "owner": "target_affinity::render_report",
                    "location": {"file": "tests/target_affinity.rs", "line": 24},
                    "seam_kind": "call_presence",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 1,
                    "related_tests": [
                      {"test_name": "ambiguous_import_mentions_target", "file": "tests/target_affinity.rs", "line": 24, "relation_reason": "assertion_target_affinity", "relation_confidence": "medium", "oracle_kind": "exact", "oracle_strength": "strong"}
                    ],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "ambiguous import target affinity cannot prove owner call", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [
                      {
                        "stage": "activate",
                        "state": "unknown",
                        "reason": "call-presence target affinity is present but imported owner names are ambiguous",
                        "category": "activation_owner_call_absent_call_presence_target_affinity",
                        "limitation_subroute": "ambiguous_imported_owner",
                        "repair_route": "analysis/call-presence-target-affinity-owner-call-tracing"
                      }
                    ],
                    "raw_findings": [
                      {"file": "tests/target_affinity.rs", "line": 24, "kind": "static_unknown", "probe_kind": "call_presence", "expression": "format_report()"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:ambiguous-import-target-affinity",
                      "canonical_item_kind": "limitation",
                      "evidence_class": "call_presence",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "raw_findings": [
                        {"file": "tests/target_affinity.rs", "line": 24, "kind": "static_unknown", "expression": "format_report()"}
                      ],
                      "raw_group_size": 1,
                      "why": "ambiguous import target affinity cannot prove owner call",
                      "recommended_repair": "Keep this as analyzer backlog until import resolution is explicit.",
                      "verify_command": null,
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let actionable_json = lane1_actionable_gap_packets_json(&report)?;
    let value: serde_json::Value =
        serde_json::from_str(&actionable_json).map_err(|err| err.to_string())?;
    let packets = value["static_limitation_backlog"]["limitation_backlog_packets"]
        .as_array()
        .ok_or_else(|| "missing limitation backlog packets".to_string())?;

    assert_eq!(packets.len(), 2);
    assert!(packets.iter().any(|packet| {
            packet["limitation_subroute"] == "bare_alias_unsupported"
                && packet["packet_id"]
                    == "limitation:activation_owner_call_absent_call_presence_target_affinity:bare_alias_unsupported:analysis-call-presence-target-affinity-owner-call-tracing"
        }));
    assert!(packets.iter().any(|packet| {
            packet["limitation_subroute"] == "ambiguous_imported_owner"
                && packet["packet_id"]
                    == "limitation:activation_owner_call_absent_call_presence_target_affinity:ambiguous_imported_owner:analysis-call-presence-target-affinity-owner-call-tracing"
        }));
    assert_eq!(
        value["static_limitation_backlog"]["top_categories"][0]["count"],
        serde_json::Value::from(2)
    );
    assert_eq!(
        value["static_limitation_backlog"]["top_subroutes"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(value["packets"].as_array().map(Vec::len), Some(0));

    let markdown = lane1_actionable_gap_packets_markdown(&report);
    assert!(markdown.contains("### Top Subroutes"));
    assert!(markdown.contains("bare_alias_unsupported"));
    assert!(markdown.contains("ambiguous_imported_owner"));
    Ok(())
}

#[test]
fn lane1_static_limitation_backlog_splits_call_presence_target_affinity_by_expression_shape() {
    let limitation = serde_json::json!({});
    let method_chain = serde_json::json!({
        "raw_findings": [
            {
                "expression": "entry\n        .evidence\n        .missing_discriminators\n        .iter()"
            }
        ]
    });
    let associated_call = serde_json::json!({
        "raw_findings": [
            {
                "expression": "u64::from(*byte)"
            }
        ]
    });
    let function_call = serde_json::json!({
        "raw_findings": [
            {
                "expression": "missing_evidence(context.probe, &class)"
            }
        ]
    });
    let no_expression = serde_json::json!({});

    assert_eq!(
        crate::static_limitation_subroute(
            &method_chain,
            &limitation,
            "activation_owner_call_absent_call_presence_target_affinity",
            "analysis/call-presence-target-affinity-owner-call-tracing",
            "call_presence"
        ),
        "call_presence_target_affinity_method_chain_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &associated_call,
            &limitation,
            "activation_owner_call_absent_call_presence_target_affinity",
            "analysis/call-presence-target-affinity-owner-call-tracing",
            "call_presence"
        ),
        "call_presence_target_affinity_associated_call_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &function_call,
            &limitation,
            "activation_owner_call_absent_call_presence_target_affinity",
            "analysis/call-presence-target-affinity-owner-call-tracing",
            "call_presence"
        ),
        "call_presence_target_affinity_function_call_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &no_expression,
            &limitation,
            "activation_owner_call_absent_call_presence_target_affinity",
            "analysis/call-presence-target-affinity-owner-call-tracing",
            "call_presence"
        ),
        "call_presence_target_affinity_missing_owner_call"
    );
}

#[test]
fn lane1_static_limitation_backlog_splits_related_test_affinity_call_presence_by_expression_shape()
{
    let limitation = serde_json::json!({});
    fn affinity_record(reason: &str, expression: Option<&str>) -> Value {
        let mut record = serde_json::json!({
            "related_tests": [
                {
                    "relation_reason": reason
                }
            ]
        });
        if let Some(expression) = expression {
            record["raw_findings"] = serde_json::json!([
                {
                    "expression": expression
                }
            ]);
        }
        record
    }

    assert_eq!(
        crate::static_limitation_subroute(
            &affinity_record(
                "same_test_file",
                Some("entry\n        .evidence\n        .missing_discriminators\n        .iter()"),
            ),
            &limitation,
            "activation_owner_call_absent_affinity_only",
            "analysis/related-test-affinity-owner-call-tracing",
            "call_presence"
        ),
        "same_test_file_call_presence_method_chain_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &affinity_record("same_test_file", Some("u64::from(*byte)")),
            &limitation,
            "activation_owner_call_absent_affinity_only",
            "analysis/related-test-affinity-owner-call-tracing",
            "call_presence"
        ),
        "same_test_file_call_presence_associated_call_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &affinity_record(
                "same_test_file",
                Some("missing_evidence(context.probe, &class)"),
            ),
            &limitation,
            "activation_owner_call_absent_affinity_only",
            "analysis/related-test-affinity-owner-call-tracing",
            "call_presence"
        ),
        "same_test_file_call_presence_function_call_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &affinity_record("owner_named_test", Some("Vec::new()")),
            &limitation,
            "activation_owner_call_absent_affinity_only",
            "analysis/related-test-affinity-owner-call-tracing",
            "return_value"
        ),
        "owner_named_test_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &affinity_record("same_test_file", None),
            &limitation,
            "activation_owner_call_absent_affinity_only",
            "analysis/related-test-affinity-owner-call-tracing",
            "call_presence"
        ),
        "same_test_file_missing_owner_call"
    );
}

#[test]
fn lane1_static_limitation_backlog_splits_same_file_owner_call_absence_by_expression_shape() {
    let limitation = serde_json::json!({});
    fn same_file_record(expression: Option<&str>) -> Value {
        let mut record = serde_json::json!({
            "related_tests": [
                {
                    "relation_reason": "same_test_file"
                }
            ]
        });
        if let Some(expression) = expression {
            record["raw_findings"] = serde_json::json!([
                {
                    "expression": expression
                }
            ]);
        }
        record
    }

    assert_eq!(
        crate::static_limitation_subroute(
            &same_file_record(Some("entry.evidence.missing_discriminators.iter()")),
            &limitation,
            "activation_owner_call_absent_same_file_only",
            "analysis/same-file-owner-call-tracing",
            "call_presence"
        ),
        "same_file_only_call_presence_receiver_method_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_repair_route_for_subroute(
            "activation_owner_call_absent_same_file_only",
            "same_file_only_call_presence_receiver_method_missing_owner_call",
            "analysis/same-file-owner-call-tracing",
        ),
        "analysis/same-file-receiver-method-owner-call-tracing"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &same_file_record(Some("entry.evidence.missing_discriminators")),
            &limitation,
            "activation_owner_call_absent_same_file_only",
            "analysis/same-file-owner-call-tracing",
            "call_presence"
        ),
        "same_file_only_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_repair_route_for_subroute(
            "activation_owner_call_absent_same_file_only",
            "same_file_only_missing_owner_call",
            "analysis/same-file-owner-call-tracing",
        ),
        "analysis/same-file-owner-call-tracing"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &same_file_record(Some("u64::from(*byte)")),
            &limitation,
            "activation_owner_call_absent_same_file_only",
            "analysis/same-file-owner-call-tracing",
            "call_presence"
        ),
        "same_file_only_call_presence_associated_call_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &same_file_record(Some("missing_evidence(context.probe, &class)")),
            &limitation,
            "activation_owner_call_absent_same_file_only",
            "analysis/same-file-owner-call-tracing",
            "call_presence"
        ),
        "same_file_only_call_presence_function_call_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &same_file_record(Some("description.value")),
            &limitation,
            "activation_owner_call_absent_same_file_only",
            "analysis/same-file-owner-call-tracing",
            "call_presence"
        ),
        "same_file_only_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &same_file_record(Some("return Vec::new()")),
            &limitation,
            "activation_owner_call_absent_same_file_only",
            "analysis/same-file-owner-call-tracing",
            "return_value"
        ),
        "same_file_only_return_value_missing_owner_call"
    );
    assert_eq!(
        crate::static_limitation_subroute(
            &same_file_record(None),
            &limitation,
            "activation_owner_call_absent_same_file_only",
            "analysis/same-file-owner-call-tracing",
            "call_presence"
        ),
        "same_file_only_missing_owner_call"
    );
}

#[test]
fn lane1_static_limitation_backlog_specializes_same_file_affinity_unlock_condition() {
    let unlock = crate::static_limitation_unlock_condition(
        "activation_owner_call_absent_affinity_only",
        "same_test_file_call_presence_function_call_missing_owner_call",
        "analysis/related-test-affinity-owner-call-tracing",
    );

    assert!(unlock.contains("bounded production call graph"));
    assert!(unlock.contains("keep the limitation non-actionable"));
    assert!(unlock.contains("direct or helper owner call is demonstrated"));

    let generic_unlock = crate::static_limitation_unlock_condition(
        "activation_owner_call_absent_affinity_only",
        "import_path_affinity_missing_owner_call",
        "analysis/related-test-affinity-owner-call-tracing",
    );
    assert!(generic_unlock.contains("related tests can be tied to direct or helper owner calls"));
    assert!(!generic_unlock.contains("bounded production call graph"));

    let same_file_claims = crate::static_limitation_backlog_packet_non_claims(
        "activation_owner_call_absent_affinity_only",
        Some("same_test_file_call_presence_function_call_missing_owner_call"),
    );
    assert!(
        same_file_claims
            .iter()
            .any(|claim| claim == "do not treat same-file affinity as owner-call evidence")
    );

    let generic_claims = crate::static_limitation_backlog_packet_non_claims(
        "activation_owner_call_absent_affinity_only",
        Some("import_path_affinity_missing_owner_call"),
    );
    assert!(
        !generic_claims
            .iter()
            .any(|claim| claim == "do not treat same-file affinity as owner-call evidence")
    );

    let same_file_unlock = crate::static_limitation_unlock_condition(
        "activation_owner_call_absent_same_file_only",
        "same_file_only_call_presence_function_call_missing_owner_call",
        "analysis/same-file-owner-call-tracing",
    );
    assert!(same_file_unlock.contains("bounded production call graph"));

    let same_file_claims = crate::static_limitation_backlog_packet_non_claims(
        "activation_owner_call_absent_same_file_only",
        Some("same_file_only_call_presence_function_call_missing_owner_call"),
    );
    assert!(
        same_file_claims
            .iter()
            .any(|claim| claim == "do not treat same-file proximity as owner-call evidence")
    );

    let receiver_method_unlock = crate::static_limitation_unlock_condition(
        "activation_owner_call_absent_same_file_only",
        "same_file_only_call_presence_receiver_method_missing_owner_call",
        "analysis/same-file-receiver-method-owner-call-tracing",
    );
    assert!(receiver_method_unlock.contains("receiver method call"));
    assert!(receiver_method_unlock.contains("bounded direct or helper owner-call path"));
    assert!(receiver_method_unlock.contains("non-actionable"));

    let receiver_method_claims = crate::static_limitation_backlog_packet_non_claims(
        "activation_owner_call_absent_same_file_only",
        Some("same_file_only_call_presence_receiver_method_missing_owner_call"),
    );
    assert!(
        receiver_method_claims
            .iter()
            .any(|claim| claim == "do not treat receiver method text as owner-call evidence")
    );
}

#[test]
fn lane1_evidence_audit_reports_aligned_supported_class_coverage() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "presentation-label",
                  "headline_eligible": true,
                  "file": "src/help.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "presentation-label",
                    "canonical_gap_id": "presentation_text::HELP_LABEL",
                    "owner": "HELP_LABEL",
                    "location": {"file": "src/help.rs", "line": 8},
                    "seam_kind": "presentation_text",
                    "grip_class": "weakly_gripped",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 0,
                    "related_tests": [],
                    "recommendation": {"action": "add_output_observer", "reason": "missing output observer", "verify_command": "cargo xtask evidence-quality-scorecard"},
                    "actionability": {"class": "actionable_output_observer"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [],
                    "raw_findings": [
                      {"file": "src/help.rs", "line": 8, "kind": "exposed", "probe_kind": "presentation_text", "expression": "pub const HELP_LABEL: &str ="},
                      {"file": "src/help.rs", "line": 9, "kind": "static_unknown", "probe_kind": "presentation_text", "expression": "\"help label\""}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "presentation_text::HELP_LABEL",
                      "canonical_item_kind": "gap",
                      "evidence_class": "presentation_text",
                      "gap_state": "actionable",
                      "actionability": "add_output_observer",
                      "raw_findings": [
                        {"file": "src/help.rs", "line": 8, "kind": "exposed", "expression": "pub const HELP_LABEL: &str ="},
                        {"file": "src/help.rs", "line": 9, "kind": "static_unknown", "expression": "\"help label\""}
                      ],
                      "raw_group_size": 2,
                      "why": "changed user-visible help text has no output observer",
                      "recommended_repair": "Add or update a help-output snapshot.",
                      "repair_route": {
                        "repair_kind": "output_observer",
                        "target_test_type": "help_output_snapshot",
                        "suggested_assertion": "Assert the rendered help output contains HELP_LABEL."
                      },
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                },
                {
                  "seam_id": "internal-policy-label",
                  "headline_eligible": true,
                  "file": "src/policy.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "internal-policy-label",
                    "canonical_gap_id": "config_or_policy_constant::INTERNAL_POLICY_LABEL",
                    "owner": "INTERNAL_POLICY_LABEL",
                    "location": {"file": "src/policy.rs", "line": 22},
                    "seam_kind": "config_or_policy_constant",
                    "grip_class": "weakly_gripped",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 0,
                    "related_tests": [],
                    "recommendation": {"action": "no_action", "reason": "internal policy metadata", "verify_command": null},
                    "actionability": {"class": "no_action_internal"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [],
                    "raw_findings": [
                      {"file": "src/policy.rs", "line": 22, "kind": "exposed", "probe_kind": "config_or_policy_constant", "expression": "pub const INTERNAL_POLICY_LABEL: &str ="},
                      {"file": "src/policy.rs", "line": 23, "kind": "weakly_exposed", "probe_kind": "config_or_policy_constant", "expression": "\"internal\""}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "config_or_policy_constant::INTERNAL_POLICY_LABEL",
                      "canonical_item_kind": "no_action",
                      "evidence_class": "config_or_policy_constant",
                      "gap_state": "internal_only",
                      "actionability": "no_action_internal",
                      "raw_findings": [
                        {"file": "src/policy.rs", "line": 22, "kind": "exposed", "expression": "pub const INTERNAL_POLICY_LABEL: &str ="},
                        {"file": "src/policy.rs", "line": 23, "kind": "weakly_exposed", "expression": "\"internal\""}
                      ],
                      "raw_group_size": 2,
                      "why": "changed policy label is internal metadata",
                      "recommended_repair": "No user test action; internal policy metadata is not user-visible behavior.",
                      "verify_command": null,
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                },
                {
                  "seam_id": "predicate-boundary-repair-route",
                  "headline_eligible": true,
                  "file": "src/pricing.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "predicate-boundary-repair-route",
                    "canonical_gap_id": "gap:predicate-boundary-repair-route",
                    "owner": "pricing::discounted_total",
                    "location": {"file": "src/pricing.rs", "line": 42},
                    "seam_kind": "predicate_boundary",
                    "grip_class": "weakly_gripped",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [{"value": "discount_threshold (equality boundary)", "reason": "observed values do not include the equality-boundary case"}],
                    "related_tests_total": 1,
                    "related_tests": [],
                    "recommendation": {"action": "write_targeted_test", "reason": "extend related test with boundary discriminator", "verify_command": "ripr agent verify --root . --before before.json --after after.json --json"},
                    "actionability": {"class": "actionable_related_test_extension"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [],
                    "raw_findings": [
                      {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "probe_kind": "predicate_boundary", "expression": "amount >= discount_threshold"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:predicate-boundary-repair-route",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "extend_related_test",
                      "raw_findings": [
                        {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                      ],
                      "raw_group_size": 1,
                      "why": "related tests reach the seam but miss the boundary discriminator",
                      "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                      "repair_route": {
                        "repair_kind": "add_boundary_assertion",
                        "target_test_type": "boundary_discriminator",
                        "suggested_assertion": "assert_eq!(discounted_total(/* discount_threshold (equality boundary) */), /* expected */)"
                      },
                      "related_test": {"name": "below_threshold_has_no_discount", "file": "tests/pricing.rs", "line": 10, "reason": "direct_owner_call"},
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;
    let json = lane1_evidence_audit_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let coverage = &value["finding_alignment"]["coverage"];
    let class_rows = coverage["alignment_coverage_by_class"]
        .as_array()
        .ok_or_else(|| "alignment coverage rows should be an array".to_string())?;
    let presentation = class_rows
        .iter()
        .find(|row| row["evidence_class"] == "presentation_text")
        .ok_or_else(|| "missing presentation_text coverage row".to_string())?;
    assert_eq!(presentation["raw_findings"], serde_json::Value::from(2));
    assert_eq!(
        presentation["aligned_raw_findings"],
        serde_json::Value::from(2)
    );
    assert_eq!(presentation["canonical_items"], serde_json::Value::from(1));
    assert_eq!(presentation["actionable_items"], serde_json::Value::from(1));
    let config_policy = class_rows
        .iter()
        .find(|row| row["evidence_class"] == "config_or_policy_constant")
        .ok_or_else(|| "missing config_or_policy_constant coverage row".to_string())?;
    assert_eq!(config_policy["raw_findings"], serde_json::Value::from(2));
    assert_eq!(
        config_policy["aligned_raw_findings"],
        serde_json::Value::from(2)
    );
    assert_eq!(config_policy["canonical_items"], serde_json::Value::from(1));
    assert_eq!(
        config_policy["internal_no_action_items"],
        serde_json::Value::from(1)
    );
    let predicate_boundary = class_rows
        .iter()
        .find(|row| row["evidence_class"] == "predicate_boundary")
        .ok_or_else(|| "missing predicate_boundary coverage row".to_string())?;
    assert_eq!(
        predicate_boundary["raw_findings"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        predicate_boundary["canonical_items"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        predicate_boundary["actionable_items"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        coverage["canonical_items_without_repair_route"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        coverage["canonical_items_without_verify_command"],
        serde_json::Value::from(0)
    );
    let top_lists = &value["finding_alignment"]["actionable_gap_top_lists"];
    assert_eq!(
        top_lists["top_actionable_gap_classes"][0]["label"],
        "predicate_boundary"
    );
    assert_eq!(
        top_lists["top_actionable_gap_classes"][0]["count"],
        serde_json::Value::from(1)
    );
    assert_eq!(top_lists["top_actionable_files"][0]["label"], "src/help.rs");
    assert_eq!(
        top_lists["top_repair_kinds"][0]["label"],
        "add_boundary_assertion"
    );
    assert_eq!(
        top_lists["top_missing_discriminator_kinds"][0]["label"],
        "observed values do not include the equality-boundary case"
    );
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_emit_agent_safe_work_items() -> Result<(), String> {
    let root = temp_dir("lane1-agent-safe-work-items");
    write(&root.join("tests/pricing.rs"), "");
    let root = root.to_string_lossy().to_string();
    let report = lane1_evidence_audit_from_repo_exposure(
        &root,
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-boundary-gap",
                  "headline_eligible": true,
                  "file": "src/pricing.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-boundary-gap",
                    "canonical_gap_id": "gap:packet-boundary-gap",
                    "location": {"file": "src/pricing.rs", "line": 42},
                    "missing_discriminators": [
                      {"value": "discount_threshold (equality boundary)", "reason": "observed values do not include the equality-boundary case"}
                    ],
                    "recommendation": {
                      "candidate_values": [
                        {"value": "input that hits the boundary: amount >= discount_threshold", "reason": "predicate boundary expression"}
                      ]
                    },
                    "raw_findings": [
                      {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:packet-boundary-gap",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "extend_related_test",
                      "raw_findings": [
                        {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                      ],
                      "why": "related tests reach the seam but miss equality at the threshold",
                      "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                      "repair_route": {
                        "repair_kind": "add_boundary_assertion",
                        "target_test_type": "boundary_discriminator",
                        "suggested_assertion": "assert_eq!(discounted_total(threshold), expected)"
                      },
                      "related_test": {"name": "below_threshold_has_no_discount", "file": "tests/pricing.rs", "line": 10, "reason": "direct_owner_call"},
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let audit_json = lane1_evidence_audit_json(&report)?;
    let audit_value: serde_json::Value =
        serde_json::from_str(&audit_json).map_err(|err| err.to_string())?;
    let embedded_packets = audit_value["finding_alignment"]["actionable_gap_packets"]
        .as_array()
        .ok_or_else(|| "audit JSON should include actionable gap packet array".to_string())?;
    assert_eq!(embedded_packets.len(), 1);
    assert_eq!(
        embedded_packets[0]["raw_findings_supporting_only"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        embedded_packets[0]["missing_discriminators"][0]["value"],
        "discount_threshold (equality boundary)"
    );
    assert_eq!(
        audit_value["finding_alignment"]["actionable_gap_packet_public_projection"]["public_projection_eligible_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        audit_value["finding_alignment"]["actionable_gap_packet_public_projection"]["projection_exclusion_reasons"]
            [0]["label"],
        "missing_receipt_command"
    );

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;
    assert_eq!(packet_value["report"], "actionable-gaps");
    assert_eq!(
        packet_value["summary"]["actionable_gaps"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        packet_value["packets"][0]["canonical_gap_id"],
        "gap:packet-boundary-gap"
    );
    assert_eq!(
        packet_value["packets"][0]["repair_kind"],
        "add_boundary_assertion"
    );
    assert_eq!(
        packet_value["packets"][0]["verify_command"],
        "cargo xtask evidence-quality-scorecard"
    );
    assert_eq!(
        packet_value["packets"][0]["repair_route_source"],
        "canonical_item.repair_route"
    );
    assert_eq!(
        packet_value["packets"][0]["verify_command_source"],
        "canonical_item.verify_command"
    );
    assert_eq!(packet_value["packets"][0]["receipt_source"], "missing");
    assert_eq!(
        packet_value["packets"][0]["public_projection_eligible"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        packet_value["packets"][0]["projection_exclusion_reasons"][0],
        "missing_receipt_command"
    );
    assert_eq!(
        packet_value["summary"]["public_projection_eligible_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        packet_value["summary"]["public_projection_excluded_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        packet_value["packets"][0]["candidate_value_or_observer"],
        "input that hits the boundary: amount >= discount_threshold"
    );
    assert_eq!(
        packet_value["packets"][0]["missing_discriminators"][0]["reason"],
        "observed values do not include the equality-boundary case"
    );
    assert_eq!(
        packet_value["packets"][0]["must_not_change"][0],
        "Do not infer actionability from raw static class."
    );

    let markdown = lane1_actionable_gap_packets_markdown(&report);
    assert!(markdown.contains("# Actionable Canonical Gap Packets"));
    assert!(markdown.contains("gap:packet-boundary-gap"));
    assert!(markdown.contains("discount_threshold (equality boundary)"));
    assert!(markdown.contains("excluded: missing_receipt_command"));
    assert!(markdown.contains("cargo xtask evidence-quality-scorecard"));
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_mark_public_projection_ready_with_receipt() -> Result<(), String> {
    let root = temp_dir("lane1-public-projection-ready");
    write(&root.join("tests/pricing.rs"), "");
    let root = root.to_string_lossy().to_string();
    let report = lane1_evidence_audit_from_repo_exposure(
        &root,
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-receipt-ready-gap",
                  "headline_eligible": true,
                  "file": "src/pricing.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-receipt-ready-gap",
                    "canonical_gap_id": "gap:packet-receipt-ready-gap",
                    "location": {"file": "src/pricing.rs", "line": 42},
                    "raw_findings": [
                      {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:packet-receipt-ready-gap",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "extend_related_test",
                      "raw_findings": [
                        {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                      ],
                      "why": "related tests reach the seam but miss equality at the threshold",
                      "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                      "repair_route": {
                        "repair_kind": "add_boundary_assertion",
                        "target_test_type": "boundary_discriminator",
                        "suggested_assertion": "assert_eq!(discounted_total(threshold), expected)"
                      },
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "receipt_command": "cargo xtask receipts check",
                      "related_test": {"name": "below_threshold_has_no_discount", "file": "tests/pricing.rs", "line": 10, "reason": "direct_owner_call"},
                      "confidence": {"basis": "fixture_backed", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;
    assert_eq!(
        packet_value["summary"]["public_projection_eligible_packets"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        packet_value["summary"]["public_projection_excluded_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        packet_value["packets"][0]["public_projection_eligible"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        packet_value["packets"][0]["projection_exclusion_reasons"],
        serde_json::json!([])
    );
    assert_eq!(
        packet_value["packets"][0]["receipt_command_or_path"],
        "cargo xtask receipts check"
    );
    assert_eq!(
        packet_value["packets"][0]["receipt_command"],
        "cargo xtask receipts check"
    );
    assert_eq!(
        packet_value["packets"][0]["receipt_source"],
        "canonical_item.receipt_command"
    );
    assert_eq!(
        packet_value["packets"][0]["target_test_shape"],
        "boundary_discriminator: assert_eq!(discounted_total(threshold), expected)"
    );
    assert_eq!(
        packet_value["packets"][0]["repair_route"]["repair_kind"],
        "add_boundary_assertion"
    );
    assert_eq!(
        packet_value["packets"][0]["repair_route"]["target_test_type"],
        "boundary_discriminator"
    );
    assert_eq!(
        packet_value["packets"][0]["repair_route"]["assertion_shape"],
        "assert_eq!(discounted_total(threshold), expected)"
    );
    assert_eq!(
        packet_value["packets"][0]["confidence"]["basis"],
        "fixture_backed"
    );
    assert_eq!(
        packet_value["packets"][0]["raw_evidence_refs"][0]["kind"],
        "weakly_exposed"
    );
    assert_eq!(
        packet_value["packets"][0]["allowed_edit_surface"][0],
        "tests/pricing.rs"
    );
    let swarm_plan = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &packet_value,
    );
    let ready = ripr_swarm_plan_ready_packets(&swarm_plan);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].canonical_gap_id, "gap:packet-receipt-ready-gap");
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_use_recommended_test_as_related_target() -> Result<(), String> {
    let root = temp_dir("lane1-recommended-test-target");
    write(&root.join("tests/lsp_tests.rs"), "");
    let root = root.to_string_lossy().to_string();
    let report = lane1_evidence_audit_from_repo_exposure(
        &root,
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-recommended-test-target",
                  "headline_eligible": true,
                  "file": "src/lsp.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-recommended-test-target",
                    "canonical_gap_id": "gap:packet-recommended-test-target",
                    "location": {"file": "src/lsp.rs", "line": 42},
                    "raw_findings": [
                      {"file": "src/lsp.rs", "line": 42, "kind": "ungripped", "expression": "tokio::io::stdin()"}
                    ],
                    "recommendation": {
                      "recommended_test": {
                        "file": "tests/lsp_tests.rs",
                        "name": "serve_stdio_call_presence_observer",
                        "reason": "recommended focused observer target"
                      }
                    },
                    "canonical_item": {
                      "canonical_gap_id": "gap:packet-recommended-test-target",
                      "canonical_item_kind": "gap",
                      "evidence_class": "call_presence",
                      "gap_state": "actionable",
                      "actionability": "add_focused_test",
                      "raw_findings": [
                        {"file": "src/lsp.rs", "line": 42, "kind": "ungripped", "expression": "tokio::io::stdin()"}
                      ],
                      "why": "write a focused test for the missing call observer",
                      "recommended_repair": "Add a call observer in tests/lsp_tests.rs.",
                      "repair_route": {
                        "repair_kind": "add_call_observer",
                        "target_test_type": "call_presence_observer",
                        "suggested_assertion": "// assert that serve_stdio called the expected target"
                      },
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "receipt_command": "cargo xtask receipts check",
                      "confidence": {"basis": "fixture_backed", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;

    assert_eq!(
        packet_value["packets"][0]["related_test_or_observer"]["file"],
        "tests/lsp_tests.rs"
    );
    assert_eq!(
        packet_value["packets"][0]["allowed_edit_surface"][0],
        "tests/lsp_tests.rs"
    );
    assert_eq!(
        packet_value["packets"][0]["public_projection_eligible"],
        serde_json::Value::Bool(true)
    );

    let swarm_plan = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &packet_value,
    );
    let ready = ripr_swarm_plan_ready_packets(&swarm_plan);
    assert_eq!(ready.len(), 1);
    assert_eq!(
        ready[0].canonical_gap_id,
        "gap:packet-recommended-test-target"
    );
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_exclude_missing_edit_surface_file() -> Result<(), String> {
    let root = temp_dir("lane1-missing-edit-surface-file");
    let root = root.to_string_lossy().to_string();
    let report = lane1_evidence_audit_from_repo_exposure(
        &root,
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-missing-edit-surface-file",
                  "headline_eligible": true,
                  "file": "src/lsp.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-missing-edit-surface-file",
                    "canonical_gap_id": "gap:packet-missing-edit-surface-file",
                    "location": {"file": "src/lsp.rs", "line": 42},
                    "raw_findings": [
                      {"file": "src/lsp.rs", "line": 42, "kind": "ungripped", "expression": "tokio::io::stdin()"}
                    ],
                    "recommendation": {
                      "recommended_test": {
                        "file": "tests/lsp_tests.rs",
                        "name": "serve_stdio_call_presence_observer",
                        "reason": "recommended focused observer target"
                      }
                    },
                    "canonical_item": {
                      "canonical_gap_id": "gap:packet-missing-edit-surface-file",
                      "canonical_item_kind": "gap",
                      "evidence_class": "call_presence",
                      "gap_state": "actionable",
                      "actionability": "add_focused_test",
                      "raw_findings": [
                        {"file": "src/lsp.rs", "line": 42, "kind": "ungripped", "expression": "tokio::io::stdin()"}
                      ],
                      "why": "write a focused test for the missing call observer",
                      "recommended_repair": "Add a call observer in tests/lsp_tests.rs.",
                      "repair_route": {
                        "repair_kind": "add_call_observer",
                        "target_test_type": "call_presence_observer",
                        "suggested_assertion": "// assert that serve_stdio called the expected target"
                      },
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "receipt_command": "cargo xtask receipts check",
                      "confidence": {"basis": "fixture_backed", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;

    assert_eq!(
        packet_value["packets"][0]["related_test_or_observer"]["file"],
        "tests/lsp_tests.rs"
    );
    assert_eq!(
        packet_value["packets"][0]["allowed_edit_surface"],
        serde_json::json!([])
    );
    assert_eq!(
        packet_value["packets"][0]["public_projection_eligible"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        packet_value["packets"][0]["projection_exclusion_reasons"],
        serde_json::json!(["missing_allowed_edit_surface"])
    );

    let swarm_plan = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &packet_value,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&swarm_plan);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(
        blocked[0].missing_context,
        vec!["allowed_edit_surface".to_string()]
    );
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_fail_closed_for_cross_language_target_unresolved()
-> Result<(), String> {
    let root = temp_dir("lane1-cross-language-target-unresolved");
    write(&root.join("test/js/web/fetch/blob.test.ts"), "");
    let root = root.to_string_lossy().to_string();
    let report = lane1_evidence_audit_from_repo_exposure(
        &root,
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-cross-language-target-unresolved",
                  "headline_eligible": true,
                  "file": "src/jsc/Blob.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-cross-language-target-unresolved",
                    "canonical_gap_id": "gap:packet-cross-language-target-unresolved",
                    "location": {"file": "src/jsc/Blob.rs", "line": 88},
                    "raw_findings": [
                      {"file": "src/jsc/Blob.rs", "line": 88, "kind": "weakly_exposed", "expression": "array_buffer.shared || array_buffer.resizable"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:packet-cross-language-target-unresolved",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "add_focused_test",
                      "raw_findings": [
                        {"file": "src/jsc/Blob.rs", "line": 88, "kind": "weakly_exposed", "expression": "array_buffer.shared || array_buffer.resizable"}
                      ],
                      "why": "binding seam has unresolved cross-language test target placement",
                      "recommended_repair": "Route target inference before suggesting a test file.",
                      "repair_route": {
                        "repair_kind": "add_boundary_assertion",
                        "target_test_type": "boundary_discriminator",
                        "suggested_assertion": "assert_eq!(blob_behavior(), expected)"
                      },
                      "related_test_or_observer": {
                        "file": "test/js/web/fetch/blob.test.ts",
                        "name": "blob copies shared buffers",
                        "line": 41
                      },
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "receipt_command": "cargo xtask receipts check",
                      "confidence": {"basis": "fixture_backed", "notes": []},
                      "static_limitations": [
                        {
                          "stage": "cross_language_test_target",
                          "state": "unknown",
                          "category": "cross_language_target_unresolved",
                          "repair_route": "analysis/cross-language-test-target-inference",
                          "reason": "external test target visibility is unresolved"
                        }
                      ]
                    }
                  }
                }
              ]
            }"#,
    )?;

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;

    assert_eq!(
        packet_value["packets"][0]["related_test_or_observer"]["file"],
        "test/js/web/fetch/blob.test.ts"
    );
    assert_eq!(
        packet_value["packets"][0]["allowed_edit_surface"],
        serde_json::json!([])
    );
    assert_eq!(
        packet_value["packets"][0]["public_projection_eligible"],
        serde_json::Value::Bool(false)
    );
    assert!(
        packet_value["packets"][0]["projection_exclusion_reasons"]
            .as_array()
            .is_some_and(|reasons| reasons
                .iter()
                .any(|reason| reason == "cross_language_target_unresolved")),
        "expected cross_language_target_unresolved in {}",
        packet_value["packets"][0]["projection_exclusion_reasons"]
    );

    let swarm_plan = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &packet_value,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&swarm_plan);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_static_limitation");
    assert_eq!(blocked[0].allowed_edit_surface, Vec::<String>::new());
    assert!(
        blocked[0]
            .projection_exclusion_reasons
            .iter()
            .any(|reason| reason == "cross_language_target_unresolved")
    );
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_exclude_missing_canonical_gap_id() -> Result<(), String> {
    let root = temp_dir("lane1-missing-canonical-gap-id");
    write(&root.join("tests/pricing.rs"), "");
    let root = root.to_string_lossy().to_string();
    let report = lane1_evidence_audit_from_repo_exposure(
        &root,
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-missing-canonical-gap-id",
                  "headline_eligible": true,
                  "file": "src/pricing.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-missing-canonical-gap-id",
                    "location": {"file": "src/pricing.rs", "line": 42},
                    "raw_findings": [
                      {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                    ],
                    "canonical_item": {
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "extend_related_test",
                      "raw_findings": [
                        {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                      ],
                      "why": "related tests reach the seam but miss equality at the threshold",
                      "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                      "repair_route": {
                        "repair_kind": "add_boundary_assertion",
                        "target_test_type": "boundary_discriminator",
                        "suggested_assertion": "assert_eq!(discounted_total(threshold), expected)"
                      },
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "receipt_command": "cargo xtask receipts check",
                      "related_test": {"name": "below_threshold_has_no_discount", "file": "tests/pricing.rs", "line": 10, "reason": "direct_owner_call"},
                      "confidence": {"basis": "fixture_backed", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;
    assert_eq!(
        packet_value["packets"][0]["canonical_gap_id"],
        "packet-missing-canonical-gap-id"
    );
    assert_eq!(
        packet_value["packets"][0]["public_projection_eligible"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        packet_value["packets"][0]["projection_exclusion_reasons"],
        serde_json::json!(["missing_canonical_gap_id"])
    );

    let swarm_plan = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &packet_value,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&swarm_plan);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    assert_eq!(blocked[0].missing_context, vec!["canonical_gap_id"]);
    Ok(())
}

#[test]
fn lane1_actionable_gap_public_projection_requires_actionable_gap_state() {
    let reasons = crate::audit_actionable_gap_projection_exclusion_reasons(
        crate::AuditActionableGapProjectionInput {
            canonical_gap_id: "gap:unresolved",
            gap_state: "unresolved",
            actionability: "extend_related_test",
            repair_kind: "add_boundary_assertion",
            target_test_type: "boundary_discriminator",
            assertion_shape: "assert_eq!(value, expected)",
            target_test_shape: "boundary_discriminator: assert_eq!(value, expected)",
            repair_route_present: true,
            repair_route_source: "canonical_item.repair_route",
            verify_command: "cargo test unresolved_gap",
            verify_command_source: "canonical_item.verify_command",
            receipt_command_or_path: Some("cargo xtask receipts check"),
            receipt_source: "canonical_item.receipt_command",
            typed_related_target_available: true,
            has_stable_canonical_gap_id: true,
            confidence_basis: "fixture_backed",
            must_not_change_count: 1,
            allowed_edit_surface_count: 1,
            raw_evidence_refs_count: 1,
            static_limitations_count: 0,
            cross_language_target_unresolved: false,
        },
    );

    assert_eq!(reasons, vec!["not_actionable_gap_state"]);
}

#[test]
fn lane1_actionable_gap_public_projection_requires_canonical_gap_id() {
    let reasons = crate::audit_actionable_gap_projection_exclusion_reasons(
        crate::AuditActionableGapProjectionInput {
            canonical_gap_id: "packet-fallback-seam-id",
            gap_state: "actionable",
            actionability: "extend_related_test",
            repair_kind: "add_boundary_assertion",
            target_test_type: "boundary_discriminator",
            assertion_shape: "assert_eq!(value, expected)",
            target_test_shape: "boundary_discriminator: assert_eq!(value, expected)",
            repair_route_present: true,
            repair_route_source: "canonical_item.repair_route",
            verify_command: "cargo test missing_canonical_gap_id",
            verify_command_source: "canonical_item.verify_command",
            receipt_command_or_path: Some("cargo xtask receipts check"),
            receipt_source: "canonical_item.receipt_command",
            typed_related_target_available: true,
            has_stable_canonical_gap_id: false,
            confidence_basis: "fixture_backed",
            must_not_change_count: 1,
            allowed_edit_surface_count: 1,
            raw_evidence_refs_count: 1,
            static_limitations_count: 0,
            cross_language_target_unresolved: false,
        },
    );

    assert_eq!(reasons, vec!["missing_canonical_gap_id"]);
}

#[test]
fn lane1_actionable_gap_public_projection_requires_allowed_edit_surface() {
    let reasons = crate::audit_actionable_gap_projection_exclusion_reasons(
        crate::AuditActionableGapProjectionInput {
            canonical_gap_id: "gap:missing-edit-surface",
            gap_state: "actionable",
            actionability: "extend_related_test",
            repair_kind: "add_boundary_assertion",
            target_test_type: "boundary_discriminator",
            assertion_shape: "assert_eq!(value, expected)",
            target_test_shape: "boundary_discriminator: assert_eq!(value, expected)",
            repair_route_present: true,
            repair_route_source: "canonical_item.repair_route",
            verify_command: "cargo test missing_edit_surface",
            verify_command_source: "canonical_item.verify_command",
            receipt_command_or_path: Some("cargo xtask receipts check"),
            receipt_source: "canonical_item.receipt_command",
            typed_related_target_available: true,
            has_stable_canonical_gap_id: true,
            confidence_basis: "fixture_backed",
            must_not_change_count: 1,
            allowed_edit_surface_count: 0,
            raw_evidence_refs_count: 1,
            static_limitations_count: 0,
            cross_language_target_unresolved: false,
        },
    );

    assert_eq!(reasons, vec!["missing_allowed_edit_surface"]);
}

#[test]
fn lane1_actionable_gap_public_projection_requires_repair_kind() {
    let reasons = crate::audit_actionable_gap_projection_exclusion_reasons(
        crate::AuditActionableGapProjectionInput {
            canonical_gap_id: "gap:missing-repair-kind",
            gap_state: "actionable",
            actionability: "extend_related_test",
            repair_kind: "repair_route_unknown",
            target_test_type: "boundary_discriminator",
            assertion_shape: "assert_eq!(value, expected)",
            target_test_shape: "boundary_discriminator: assert_eq!(value, expected)",
            repair_route_present: true,
            repair_route_source: "canonical_item.repair_route",
            verify_command: "cargo test missing_repair_kind",
            verify_command_source: "canonical_item.verify_command",
            receipt_command_or_path: Some("cargo xtask receipts check"),
            receipt_source: "canonical_item.receipt_command",
            typed_related_target_available: true,
            has_stable_canonical_gap_id: true,
            confidence_basis: "fixture_backed",
            must_not_change_count: 1,
            allowed_edit_surface_count: 1,
            raw_evidence_refs_count: 1,
            static_limitations_count: 0,
            cross_language_target_unresolved: false,
        },
    );

    assert_eq!(reasons, vec!["missing_repair_kind"]);
}

#[test]
fn lane1_actionable_gap_public_projection_requires_target_test_shape() {
    let reasons = crate::audit_actionable_gap_projection_exclusion_reasons(
        crate::AuditActionableGapProjectionInput {
            canonical_gap_id: "gap:missing-target-shape",
            gap_state: "actionable",
            actionability: "extend_related_test",
            repair_kind: "add_boundary_assertion",
            target_test_type: "boundary_discriminator",
            assertion_shape: "assert_eq!(value, expected)",
            target_test_shape: "target_test_shape_unknown",
            repair_route_present: true,
            repair_route_source: "canonical_item.repair_route",
            verify_command: "cargo test missing_target_shape",
            verify_command_source: "canonical_item.verify_command",
            receipt_command_or_path: Some("cargo xtask receipts check"),
            receipt_source: "canonical_item.receipt_command",
            typed_related_target_available: true,
            has_stable_canonical_gap_id: true,
            confidence_basis: "fixture_backed",
            must_not_change_count: 1,
            allowed_edit_surface_count: 1,
            raw_evidence_refs_count: 1,
            static_limitations_count: 0,
            cross_language_target_unresolved: false,
        },
    );

    assert_eq!(reasons, vec!["missing_target_test_shape"]);
}

#[test]
fn lane1_actionable_gap_public_projection_requires_confidence_basis() {
    let reasons = crate::audit_actionable_gap_projection_exclusion_reasons(
        crate::AuditActionableGapProjectionInput {
            canonical_gap_id: "gap:missing-confidence",
            gap_state: "actionable",
            actionability: "extend_related_test",
            repair_kind: "add_boundary_assertion",
            target_test_type: "boundary_discriminator",
            assertion_shape: "assert_eq!(value, expected)",
            target_test_shape: "boundary_discriminator: assert_eq!(value, expected)",
            repair_route_present: true,
            repair_route_source: "canonical_item.repair_route",
            verify_command: "cargo test missing_confidence",
            verify_command_source: "canonical_item.verify_command",
            receipt_command_or_path: Some("cargo xtask receipts check"),
            receipt_source: "canonical_item.receipt_command",
            typed_related_target_available: true,
            has_stable_canonical_gap_id: true,
            confidence_basis: "confidence_basis_unknown",
            must_not_change_count: 1,
            allowed_edit_surface_count: 1,
            raw_evidence_refs_count: 1,
            static_limitations_count: 0,
            cross_language_target_unresolved: false,
        },
    );

    assert_eq!(reasons, vec!["missing_confidence"]);
}

#[test]
fn lane1_actionable_gap_verify_command_uses_bounded_related_target_over_unbounded_snapshot() {
    let record = serde_json::json!({
        "recommendation": {
            "verify_command": "ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json"
        }
    });
    let canonical_item = serde_json::json!({
        "verify_command": "ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json",
        "related_test_or_observer": {
            "file": "crates/ripr/src/lsp/tests.rs",
            "line": 1,
            "name": "serve_stdio_call_presence_observer",
            "reason": "recommended_test_target"
        }
    });

    let (command, source) =
        crate::audit_actionable_gap_verify_command_with_source(&record, &canonical_item);

    assert_eq!(
        command,
        "cargo test -p ripr serve_stdio_call_presence_observer"
    );
    assert_eq!(source, "related_test_or_observer.name");
    let reasons = crate::audit_actionable_gap_projection_exclusion_reasons(
        crate::AuditActionableGapProjectionInput {
            canonical_gap_id: "gap:bounded-verify",
            gap_state: "actionable",
            actionability: "extend_related_test",
            repair_kind: "add_call_observer",
            target_test_type: "call_presence_observer",
            assertion_shape: "// assert that serve_stdio called the expected target",
            target_test_shape: "call_presence_observer: // assert that serve_stdio called the expected target",
            repair_route_present: true,
            repair_route_source: "canonical_item.repair_route",
            verify_command: &command,
            verify_command_source: &source,
            receipt_command_or_path: Some(
                "ripr agent receipt --root . --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-1 --json",
            ),
            receipt_source: "canonical_item.receipt_command",
            typed_related_target_available: true,
            has_stable_canonical_gap_id: true,
            confidence_basis: "fixture_backed",
            must_not_change_count: 1,
            allowed_edit_surface_count: 1,
            raw_evidence_refs_count: 1,
            static_limitations_count: 0,
            cross_language_target_unresolved: false,
        },
    );
    assert!(reasons.is_empty());
}

#[test]
fn lane1_actionable_gap_public_projection_rejects_unbounded_repo_exposure_verify_command() {
    let reasons = crate::audit_actionable_gap_projection_exclusion_reasons(
        crate::AuditActionableGapProjectionInput {
            canonical_gap_id: "gap:unbounded-verify",
            gap_state: "actionable",
            actionability: "extend_related_test",
            repair_kind: "add_call_observer",
            target_test_type: "call_presence_observer",
            assertion_shape: "// assert that serve_stdio called the expected target",
            target_test_shape: "call_presence_observer: // assert that serve_stdio called the expected target",
            repair_route_present: true,
            repair_route_source: "canonical_item.repair_route",
            verify_command: "ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json",
            verify_command_source: "canonical_item.verify_command",
            receipt_command_or_path: Some(
                "ripr agent receipt --root . --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-1 --json",
            ),
            receipt_source: "canonical_item.receipt_command",
            typed_related_target_available: true,
            has_stable_canonical_gap_id: true,
            confidence_basis: "fixture_backed",
            must_not_change_count: 1,
            allowed_edit_surface_count: 1,
            raw_evidence_refs_count: 1,
            static_limitations_count: 0,
            cross_language_target_unresolved: false,
        },
    );

    assert_eq!(reasons, vec!["unbounded_verify_command"]);
}

#[test]
fn lane1_static_limitation_backlog_keeps_samples_for_each_top_repair_route() -> Result<(), String> {
    let mut builder = super::super::Lane1EvidenceAuditBuilder::new(".");
    let dominant_route = "analysis/same-file-owner-call-tracing";
    for index in 0..=super::super::LANE1_EVIDENCE_AUDIT_TOP_LIMIT {
        let subroute = format!("same_file_subroute_{index}");
        for _ in 0..(super::super::LANE1_EVIDENCE_AUDIT_TOP_LIMIT + 1 - index) {
            builder.ingest_static_limitation_backlog_packet_sample(
                "activation_owner_call_absent_same_file_only",
                &subroute,
                dominant_route,
                super::super::Lane1StaticLimitationBacklogSample {
                    canonical_gap_id: Some(format!("gap:same-file-{index}")),
                    evidence_class: "call_presence".to_string(),
                    source_file: "crates/ripr/src/analysis/classify/activation.rs".to_string(),
                    line: Some(196 + index),
                    expression: Some("missing_boundary_discriminator(...)".to_string()),
                    limitation_reason: Some(
                        "same-file proximity is not owner-call proof".to_string(),
                    ),
                },
            );
        }
    }
    builder
        .static_repair_route_counts
        .insert(dominant_route.to_string(), 1000);

    let sampled_route = "analysis/macro-generated-value-fixtures";
    builder.ingest_static_limitation_backlog_packet_sample(
        "macro_generated_value",
        "macro_generated_value",
        sampled_route,
        super::super::Lane1StaticLimitationBacklogSample {
            canonical_gap_id: Some("gap:macro-sample".to_string()),
            evidence_class: "predicate_boundary".to_string(),
            source_file: "crates/ripr/src/analysis/macros.rs".to_string(),
            line: Some(88),
            expression: Some("macro_generated_case!(input)".to_string()),
            limitation_reason: Some("macro-generated value remains opaque".to_string()),
        },
    );
    builder
        .static_repair_route_counts
        .insert(sampled_route.to_string(), 1);

    let mut report = builder.finish(".".to_string(), None);
    report
        .static_limitation_repair_route_counts
        .insert("report/lane1-audit-bounded-diagnostics".to_string(), 2);

    let backlog = super::super::lane1_static_limitation_backlog_json(&report);
    let top_routes = super::super::ripr_swarm_readiness_top_limitation_routes(&backlog);
    assert!(
        top_routes
            .iter()
            .all(|route| route.repair_route != "report/lane1-audit-bounded-diagnostics"),
        "report-only runtime diagnostics should stay out of packet-backed top limitation routes: {top_routes:?}"
    );
    let sampled = top_routes
        .iter()
        .find(|route| route.repair_route == sampled_route)
        .ok_or_else(|| "expected low-count route to remain in top route projection".to_string())?;

    assert_eq!(
        sampled.sample_canonical_gap_ids,
        vec!["gap:macro-sample".to_string()]
    );
    assert_eq!(
        sampled.sample_limitation_category.as_deref(),
        Some("macro_generated_value")
    );
    assert_eq!(
        sampled.dominant_evidence_class.as_deref(),
        Some("predicate_boundary")
    );
    assert!(
        sampled
            .sample_packet_id
            .as_deref()
            .is_some_and(|packet| packet.contains("macro_generated_value")),
        "route sample must keep an inspectable backlog packet id: {sampled:?}"
    );
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_exclude_structured_route_without_typed_target() -> Result<(), String>
{
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-route-without-target-gap",
                  "headline_eligible": true,
                  "file": "src/pricing.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-route-without-target-gap",
                    "canonical_gap_id": "gap:packet-route-without-target-gap",
                    "location": {"file": "src/pricing.rs", "line": 42},
                    "raw_evidence_refs": [
                      {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                    ],
                    "raw_findings": [
                      {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:packet-route-without-target-gap",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "extend_related_test",
                      "raw_evidence_refs": [
                        {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                      ],
                      "raw_findings": [
                        {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                      ],
                      "why": "related tests reach the seam but miss equality at the threshold",
                      "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                      "repair_route": {
                        "repair_kind": "add_boundary_assertion",
                        "target_test_type": "boundary_discriminator",
                        "suggested_assertion": "assert_eq!(discounted_total(threshold), expected)"
                      },
                      "candidate_value_or_observer": "input that hits the boundary branch",
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "receipt_command": "cargo xtask receipts check",
                      "confidence": {"basis": "fixture_backed", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;
    assert_eq!(
        packet_value["packets"][0]["repair_route"]["repair_kind"],
        "add_boundary_assertion"
    );
    assert_eq!(
        packet_value["packets"][0]["public_projection_eligible"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        packet_value["packets"][0]["projection_exclusion_reasons"],
        serde_json::json!([
            "missing_related_test_or_observer",
            "missing_allowed_edit_surface"
        ])
    );
    let swarm_plan = ripr_swarm_plan_from_actionable_gaps_value(
        10,
        Path::new("target/ripr/reports/actionable-gaps.json"),
        &packet_value,
    );
    let blocked = ripr_swarm_plan_blocked_packets(&swarm_plan);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].swarm_state, "blocked_by_missing_context");
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_require_canonical_assertion_for_public_projection()
-> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-fallback-assertion-gap",
                  "headline_eligible": true,
                  "file": "src/pricing.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-fallback-assertion-gap",
                    "canonical_gap_id": "gap:packet-fallback-assertion-gap",
                    "location": {"file": "src/pricing.rs", "line": 42},
                    "recommendation": {
                      "assertion_shape": {"kind": "exact_value"}
                    },
                    "raw_findings": [
                      {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:packet-fallback-assertion-gap",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "extend_related_test",
                      "raw_findings": [
                        {"file": "src/pricing.rs", "line": 42, "kind": "weakly_exposed", "expression": "amount >= discount_threshold"}
                      ],
                      "why": "related tests reach the seam but miss equality at the threshold",
                      "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                      "repair_route": {
                        "repair_kind": "add_boundary_assertion",
                        "target_test_type": "boundary_discriminator"
                      },
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "receipt_command": "cargo xtask receipts check",
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;
    assert_eq!(packet_value["packets"][0]["assertion_shape"], "exact_value");
    assert_eq!(packet_value["packets"][0]["repair_route_source"], "missing");
    assert_eq!(
        packet_value["packets"][0]["public_projection_eligible"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        packet_value["packets"][0]["projection_exclusion_reasons"],
        serde_json::json!([
            "missing_repair_route",
            "missing_related_test_or_observer",
            "missing_allowed_edit_surface"
        ])
    );
    assert_eq!(
        packet_value["summary"]["public_projection_eligible_packets"],
        serde_json::Value::from(0)
    );
    Ok(())
}

#[test]
fn lane1_actionable_gap_packets_keep_observed_gaps_out_of_public_projection() -> Result<(), String>
{
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "packet-observed-gap",
                  "headline_eligible": true,
                  "file": "src/pricing.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "packet-observed-gap",
                    "canonical_gap_id": "gap:packet-observed-gap",
                    "location": {"file": "src/pricing.rs", "line": 42},
                    "raw_findings": [
                      {"file": "src/pricing.rs", "line": 42, "kind": "exposed", "expression": "amount >= discount_threshold"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:packet-observed-gap",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "already_observed",
                      "actionability": "already_observed",
                      "raw_findings": [
                        {"file": "src/pricing.rs", "line": 42, "kind": "exposed", "expression": "amount >= discount_threshold"}
                      ],
                      "why": "related tests already observe the changed boundary",
                      "recommended_repair": "No repair is needed for this observed gap.",
                      "repair_route": {
                        "repair_kind": "add_boundary_assertion",
                        "target_test_type": "boundary_discriminator",
                        "suggested_assertion": "assert_eq!(discounted_total(threshold), expected)"
                      },
                      "verify_command": "cargo xtask evidence-quality-scorecard",
                      "receipt_command": "cargo xtask receipts check",
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;

    let packet_json = lane1_actionable_gap_packets_json(&report)?;
    let packet_value: serde_json::Value =
        serde_json::from_str(&packet_json).map_err(|err| err.to_string())?;
    assert_eq!(
        packet_value["summary"]["public_projection_eligible_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(
        packet_value["summary"]["public_projection_excluded_packets"],
        serde_json::Value::from(0)
    );
    assert_eq!(packet_value["packets"], serde_json::json!([]));
    Ok(())
}

#[test]
fn lane1_evidence_audit_requires_structured_repair_route_for_actionable_items() -> Result<(), String>
{
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "prose-only-repair",
                  "headline_eligible": true,
                  "file": "src/lib.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "prose-only-repair",
                    "canonical_gap_id": "gap:prose-only-repair",
                    "owner": "pricing::discount",
                    "location": {"file": "src/lib.rs", "line": 12},
                    "seam_kind": "predicate_boundary",
                    "grip_class": "weakly_gripped",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 0,
                    "related_tests": [],
                    "recommendation": {"action": "write_targeted_test", "reason": "missing discriminator", "verify_command": "ripr agent verify --root . --before before.json --after after.json --json"},
                    "actionability": {"class": "actionable_assertion_upgrade"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [],
                    "raw_findings": [
                      {"file": "src/lib.rs", "line": 12, "kind": "weakly_exposed", "expression": "discount(amount)"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:prose-only-repair",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "upgrade_assertion",
                      "raw_findings": [
                        {"file": "src/lib.rs", "line": 12, "kind": "weakly_exposed", "expression": "discount(amount)"}
                      ],
                      "raw_group_size": 1,
                      "why": "related tests reach the seam but miss the boundary discriminator",
                      "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                      "verify_command": "verify_command_unknown",
                      "confidence": {"basis": "static_only", "notes": []}
                    }
                  }
                }
              ]
            }"#,
    )?;
    let json = lane1_evidence_audit_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let coverage = &value["finding_alignment"]["coverage"];

    assert_eq!(
        coverage["canonical_items_without_repair_route"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        coverage["canonical_items_without_verify_command"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        value["finding_alignment"]["actionable_gap_top_lists"]["top_repair_route_unknowns"][0]["label"],
        "predicate_boundary"
    );
    assert_eq!(
        value["finding_alignment"]["actionable_gap_top_lists"]["top_verify_command_unknowns"][0]["label"],
        "predicate_boundary"
    );
    let packet = &value["finding_alignment"]["actionable_gap_packets"][0];
    assert_eq!(
        packet["verify_command"],
        "ripr agent verify --root . --before before.json --after after.json --json"
    );
    assert_eq!(
        packet["verify_command_source"],
        "evidence_record.recommendation.verify_command"
    );
    assert_eq!(packet["repair_route_source"], "missing");
    assert_eq!(
        packet["projection_exclusion_reasons"],
        serde_json::json!([
            "missing_repair_route",
            "missing_repair_kind",
            "missing_target_test_shape",
            "missing_verify_command",
            "missing_receipt_command",
            "missing_related_test_or_observer",
            "missing_allowed_edit_surface"
        ])
    );
    let actionable_static_limitation_reasons =
        value["finding_alignment"]["actionable_gap_top_lists"]["top_static_limitation_reasons"]
            .as_array()
            .ok_or_else(|| "top_static_limitation_reasons should be an array".to_string())?;
    assert!(
        actionable_static_limitation_reasons.is_empty(),
        "static limitations without actionable gap records must not appear in actionable top lists"
    );
    Ok(())
}

#[test]
fn lane1_evidence_audit_limits_static_top_reasons_to_actionable_items() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "actionable-static-limit",
                  "headline_eligible": true,
                  "file": "src/lib.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "actionable-static-limit",
                    "canonical_gap_id": "gap:actionable-static-limit",
                    "location": {"file": "src/lib.rs", "line": 12},
                    "missing_discriminators": [],
                    "static_limitations": [
                      {"reason": "preview import graph", "stage": "reach", "state": "unknown"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:actionable-static-limit",
                      "canonical_item_kind": "gap",
                      "evidence_class": "predicate_boundary",
                      "gap_state": "actionable",
                      "actionability": "upgrade_assertion",
                      "repair_route": {"repair_kind": "add_boundary_assertion"},
                      "verify_command": "cargo xtask evidence-quality-scorecard"
                    }
                  }
                },
                {
                  "seam_id": "non-action-static-limit",
                  "headline_eligible": true,
                  "file": "src/opaque.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "non-action-static-limit",
                    "canonical_gap_id": null,
                    "location": {"file": "src/opaque.rs", "line": 30},
                    "missing_discriminators": [],
                    "static_limitations": [
                      {"reason": "opaque helper value", "stage": "activate", "state": "unknown"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": null,
                      "canonical_item_kind": "limitation",
                      "evidence_class": "call_presence",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "verify_command": null
                    }
                  }
                }
              ]
            }"#,
    )?;
    let json = lane1_evidence_audit_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    assert_eq!(
        audit_count_row(
            &value["static_limitations"]["by_reason"],
            "preview import graph"
        )?,
        1
    );
    assert_eq!(
        audit_count_row(
            &value["static_limitations"]["by_reason"],
            "opaque helper value"
        )?,
        1
    );
    let top_static =
        &value["finding_alignment"]["actionable_gap_top_lists"]["top_static_limitation_reasons"];
    assert_eq!(top_static[0]["label"], "preview import graph");
    assert_eq!(top_static[0]["count"], serde_json::Value::from(1));
    assert_eq!(
        top_static
            .as_array()
            .ok_or_else(|| "top static reasons should be an array".to_string())?
            .len(),
        1
    );
    Ok(())
}

#[test]
fn lane1_evidence_audit_rejects_generic_static_unknown_limitation_category() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(
        ".",
        r#"{
              "schema_version": "0.3",
              "scope": "repo",
              "seams": [
                {
                  "seam_id": "generic-static-unknown",
                  "headline_eligible": true,
                  "file": "src/opaque.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "generic-static-unknown",
                    "canonical_gap_id": "gap:generic-static-unknown",
                    "owner": "opaque::generic",
                    "location": {"file": "src/opaque.rs", "line": 10},
                    "seam_kind": "call_presence",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 0,
                    "related_tests": [],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "generic limitation", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [
                      {"category": "static_unknown", "repair_route": "analysis/static-limitation-taxonomy"}
                    ],
                    "raw_findings": [
                      {"file": "src/opaque.rs", "line": 10, "kind": "static_unknown", "expression": "opaque(value)"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:generic-static-unknown",
                      "canonical_item_kind": "limitation",
                      "evidence_class": "call_presence",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "raw_findings": [
                        {"file": "src/opaque.rs", "line": 10, "kind": "static_unknown", "expression": "opaque(value)"}
                      ],
                      "static_limitations": [
                        {"category": "static_unknown", "repair_route": "analysis/static-limitation-taxonomy"}
                      ],
                      "recommended_repair": "Inspect static limitation",
                      "verify_command": null
                    }
                  }
                },
                {
                  "seam_id": "named-static-limitation",
                  "headline_eligible": true,
                  "file": "src/helper.rs",
                  "evidence_record": {
                    "schema_version": "0.1",
                    "seam_id": "named-static-limitation",
                    "canonical_gap_id": "gap:named-static-limitation",
                    "owner": "helper::named",
                    "location": {"file": "src/helper.rs", "line": 12},
                    "seam_kind": "call_presence",
                    "grip_class": "static_unknown",
                    "headline_eligible": true,
                    "evidence_path": {},
                    "observed_values": [],
                    "missing_discriminators": [],
                    "related_tests_total": 0,
                    "related_tests": [],
                    "recommendation": {"action": "inspect_static_limitation", "reason": "opaque helper", "verify_command": null},
                    "actionability": {"class": "static_limitation"},
                    "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                    "static_limitations": [
                      {"category": "opaque_helper_call", "repair_route": "analysis/oracle-semantics-audit-fixes"}
                    ],
                    "raw_findings": [
                      {"file": "src/helper.rs", "line": 12, "kind": "static_unknown", "expression": "helper(value)"}
                    ],
                    "canonical_item": {
                      "canonical_gap_id": "gap:named-static-limitation",
                      "canonical_item_kind": "limitation",
                      "evidence_class": "call_presence",
                      "gap_state": "static_limitation",
                      "actionability": "static_limitation",
                      "raw_findings": [
                        {"file": "src/helper.rs", "line": 12, "kind": "static_unknown", "expression": "helper(value)"}
                      ],
                      "static_limitations": [
                        {"category": "opaque_helper_call", "repair_route": "analysis/oracle-semantics-audit-fixes"}
                      ],
                      "recommended_repair": "Inspect opaque helper assertion path",
                      "verify_command": null
                    }
                  }
                }
              ]
            }"#,
    )?;
    let json = lane1_evidence_audit_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let coverage = &value["finding_alignment"]["coverage"];

    assert_eq!(
        coverage["static_unknown_without_named_limitation"],
        serde_json::Value::from(1)
    );
    Ok(())
}

#[test]
fn lane1_evidence_audit_markdown_names_required_sections() -> Result<(), String> {
    let report = lane1_evidence_audit_from_repo_exposure(".", lane1_audit_sample_json())?;
    let markdown = lane1_evidence_audit_markdown(&report);

    assert!(markdown.contains("Lane 1 evidence quality audit"));
    assert!(markdown.contains("Repo Exposure Generation"));
    assert!(markdown.contains("Largest Canonical Gap Groups"));
    assert!(markdown.contains("Finding Alignment"));
    assert!(markdown.contains("Finding Alignment Coverage"));
    assert!(markdown.contains("Actionable Canonical Gap Top Lists"));
    assert!(markdown.contains("Repair kind"));
    assert!(markdown.contains("Static unknown without named limitation"));
    assert!(markdown.contains("Raw-to-canonical ratio"));
    assert!(markdown.contains("Duplicate-Looking Groups"));
    assert!(markdown.contains("Missing Discriminator Classes"));
    assert!(markdown.contains("opaque_helper_call"));
    assert!(markdown.contains("analysis/oracle-semantics-audit-fixes"));
    assert!(markdown.contains("Evidence Record Field Health"));
    assert!(markdown.contains("Top Files By Unresolved Evidence Debt"));
    assert!(markdown.contains("gap:shared"));
    Ok(())
}

#[test]
fn lane1_evidence_audit_json_reports_generation_diagnostics() -> Result<(), String> {
    let mut report = lane1_evidence_audit_from_repo_exposure(".", lane1_audit_sample_json())?;
    report.repo_exposure_generation = Some(Lane1EvidenceAuditRepoExposureGeneration {
        command: "target/debug/ripr check --root . --mode instant --format repo-exposure-json"
            .to_string(),
        timeout_ms: 60_000,
        status: "pass".to_string(),
        failure_reason: None,
        duration_ms: 42_000,
        exit_code: Some(0),
        stdout_bytes: 1024,
        stderr_bytes: 128,
        latency_trace_events_total: 3,
        latency_trace_tail: vec![RepoExposureLatencyTrace {
            phase: "evidence_for_seams_progress".to_string(),
            status: "processed_5000_of_38124".to_string(),
            duration_ms: 41_000,
        }],
    });

    let json = lane1_evidence_audit_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
    let generation = &value["inputs"]["repo_exposure_generation"];

    assert_eq!(generation["status"], "pass");
    assert_eq!(generation["duration_ms"], serde_json::Value::from(42_000));
    assert_eq!(
        generation["latency_trace_tail"][0]["phase"],
        "evidence_for_seams_progress"
    );
    assert_eq!(
        generation["latency_trace_tail"][0]["status"],
        "processed_5000_of_38124"
    );
    Ok(())
}

#[test]
fn lane1_evidence_audit_normalizes_static_limitation_taxonomy() {
    for (stage, state, reason, expected) in [
        (
            "activate",
            "unknown",
            "No direct owner call observed for value-insensitive seam `Vec::new()`",
            "activation_owner_call_absent",
        ),
        (
            "activate",
            "unknown",
            "owner call target is unresolved for value-insensitive seam `Vec::new()`",
            "activation_owner_call_unresolved",
        ),
        (
            "activate",
            "unknown",
            "No concrete activation values observed for seam `threshold`",
            "activation_value_unresolved",
        ),
        (
            "activate",
            "unknown",
            "cross-file constant boundary is unresolved",
            "cross_file_constant_unresolved",
        ),
        (
            "activate",
            "unknown",
            "macro generated value hides literal",
            "macro_generated_value",
        ),
        (
            "observe",
            "unknown",
            "opaque helper hides field",
            "opaque_helper_call",
        ),
        (
            "propagate",
            "unknown",
            "dynamic dispatch target is opaque",
            "dynamic_dispatch",
        ),
        (
            "observe",
            "unknown",
            "mock expectation shape is unsupported",
            "unsupported_mock_shape",
        ),
        (
            "observe",
            "unknown",
            "snapshot field is unknown",
            "snapshot_field_unknown",
        ),
        (
            "propagate",
            "unknown",
            "side-effect sink is unknown",
            "side_effect_sink_unknown",
        ),
        (
            "classification",
            "opaque",
            "seam is classified opaque",
            "opaque_static_evidence",
        ),
        (
            "reach",
            "unknown",
            "no related tests",
            "reachability_static_unknown",
        ),
        (
            "activate",
            "unknown",
            "missing fact",
            "activation_static_unknown",
        ),
        (
            "propagate",
            "unknown",
            "missing sink",
            "propagation_static_unknown",
        ),
        (
            "observe",
            "unknown",
            "missing oracle",
            "observation_static_unknown",
        ),
        (
            "discriminate",
            "unknown",
            "missing exact assertion",
            "discrimination_static_unknown",
        ),
        (
            "unknown",
            "unknown",
            "missing stage",
            "static_limitation_unclassified",
        ),
    ] {
        assert_eq!(
            static_limitation_category(stage, state, reason),
            expected,
            "unexpected category for {stage}/{state}: {reason}"
        );
    }

    for (category, expected) in [
        (
            "activation_owner_call_absent",
            "analysis/owner-call-absence-triage",
        ),
        (
            "activation_owner_call_absent_call_presence_target_affinity",
            "analysis/call-presence-target-affinity-owner-call-tracing",
        ),
        (
            "activation_owner_call_absent_assertion_target_affinity",
            "analysis/assertion-target-affinity-owner-call-tracing",
        ),
        (
            "activation_owner_call_absent_affinity_only",
            "analysis/related-test-affinity-owner-call-tracing",
        ),
        (
            "activation_owner_call_absent_same_file_only",
            "analysis/same-file-owner-call-tracing",
        ),
        (
            "activation_owner_call_unresolved",
            "analysis/related-test-ranking-audit-fixes",
        ),
        (
            "activation_value_unresolved",
            "analysis/value-resolution-audit-fixes",
        ),
        (
            "cross_file_constant_unresolved",
            "analysis/cross-file-constant-resolution",
        ),
        (
            "macro_generated_value",
            "analysis/macro-generated-value-fixtures",
        ),
        (
            "opaque_helper_call",
            "analysis/oracle-semantics-audit-fixes",
        ),
        ("dynamic_dispatch", "calibration/runtime-fixtures-v3"),
        (
            "unsupported_mock_shape",
            "analysis/oracle-semantics-audit-fixes",
        ),
        (
            "snapshot_field_unknown",
            "analysis/oracle-semantics-audit-fixes",
        ),
        (
            "side_effect_sink_unknown",
            "analysis/oracle-semantics-audit-fixes",
        ),
        (
            "opaque_static_evidence",
            "analysis/static-limitation-taxonomy",
        ),
        (
            "reachability_static_unknown",
            "analysis/related-test-ranking-audit-fixes",
        ),
        (
            "activation_static_unknown",
            "analysis/static-limitation-taxonomy",
        ),
        (
            "propagation_static_unknown",
            "analysis/static-limitation-taxonomy",
        ),
        (
            "observation_static_unknown",
            "analysis/oracle-semantics-audit-fixes",
        ),
        (
            "discrimination_static_unknown",
            "analysis/oracle-semantics-audit-fixes",
        ),
        (
            "static_limitation_unclassified",
            "analysis/static-limitation-taxonomy",
        ),
        ("unknown", "analysis/static-limitation-taxonomy"),
    ] {
        assert_eq!(
            static_limitation_repair_route(category),
            expected,
            "unexpected repair route for {category}"
        );
    }

    let mut markdown = String::new();
    audit_push_value_counts_table_limited(
        &mut markdown,
        "Static limitation category",
        &serde_json::json!({}),
        &["missing"],
        5,
    );
    assert!(markdown.contains("No static limitation category counts were reported."));
}

#[test]
fn lane1_evidence_audit_rejects_repo_exposure_without_seams() {
    let err = lane1_evidence_audit_from_repo_exposure(".", r#"{"schema_version":"0.3"}"#)
        .expect_err("missing seams should fail");
    assert!(err.contains("missing `seams` array"));
}
