use super::*;

#[test]
fn repo_contract_report_command_writes_indexable_report_files() -> Result<(), String> {
    with_temp_cwd("repo-contract-report-command", |root| {
        write_repo_contract_report_fixture(root, true);

        dispatch::execute(XtaskCommand::RepoContractReport)?;

        let markdown_path = root
            .join(super::super::reports_dir())
            .join("source-of-truth-graph.md");
        let json_path = root
            .join(super::super::reports_dir())
            .join("source-of-truth-graph.json");
        let markdown = fs::read_to_string(markdown_path)
            .map_err(|err| format!("failed to read graph markdown: {err}"))?;
        let json = fs::read_to_string(json_path)
            .map_err(|err| format!("failed to read graph json: {err}"))?;
        assert!(markdown.contains("Mode: advisory"));
        let value: Value = serde_json::from_str(&json).map_err(|err| err.to_string())?;
        assert_eq!(value["schema_version"], "0.1");
        assert_eq!(value["report_id"], "source_of_truth_graph");
        assert_eq!(value["status"], "pass");
        let rust_repair_rows = value["support_tiers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["capability"] == "Rust gap repair loop")
            .collect::<Vec<_>>();
        assert_eq!(rust_repair_rows.len(), 1, "{value:#}");
        assert_eq!(
            rust_repair_rows
                .first()
                .and_then(|row| row.get("tier"))
                .and_then(Value::as_str),
            Some("`usable alpha`")
        );
        Ok(())
    })
}

#[test]
fn repo_contract_report_treats_goal_manifest_as_optional_historical() -> Result<(), String> {
    with_temp_cwd("repo-contract-report-missing-active-goal", |root| {
        write_repo_contract_report_fixture(root, false);

        let summary = super::super::repo_contract_summary(root)?;
        if summary
            .missing_links
            .iter()
            .any(|link| link.contains(".ripr/goals/active.toml"))
        {
            return Err(format!(
                "missing historical goal manifest must not be a violation: {:#?}",
                summary.missing_links
            ));
        }
        Ok(())
    })
}

#[test]
fn repo_contract_report_requires_support_tier_file() {
    with_temp_cwd("repo-contract-report-missing-support-tier", |root| {
        write_doc_artifact_fixture(
            root,
            "docs/proposals/RIPR-PROP-0001-source-of-truth.md",
            "RIPR-PROP-0001",
        );
        write_doc_artifact_ledger_fixture(
            root,
            r#"schema_version = "1.0"

[[artifact]]
id = "RIPR-PROP-0001"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0001-source-of-truth.md"
status = "accepted"
owner = "repo-infra"
"#,
        );

        let error = super::super::repo_contract_summary(root)
            .expect_err("missing support tiers should fail summary generation");
        assert!(error.contains(SUPPORT_TIERS_PATH), "{error}");
    });
}

#[test]
fn repo_badge_artifact_jobs_has_exactly_four_entries_with_repo_prefix() -> Result<(), String> {
    let jobs = repo_badge_artifact_jobs();
    let expected = vec![
        BadgeArtifactJob {
            format: "repo-badge-json",
            output_file: "repo-ripr-badge.json",
        },
        BadgeArtifactJob {
            format: "repo-badge-shields",
            output_file: "repo-ripr-badge-shields.json",
        },
        BadgeArtifactJob {
            format: "repo-badge-plus-json",
            output_file: "repo-ripr-plus-badge.json",
        },
        BadgeArtifactJob {
            format: "repo-badge-plus-shields",
            output_file: "repo-ripr-plus-badge-shields.json",
        },
    ];
    if jobs == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {jobs:?}"))
    }
}

#[test]
fn repo_badge_artifact_command_args_does_not_use_git_diff() -> Result<(), String> {
    // Load-bearing regression: repo-scope artifacts MUST NOT depend on
    // `git diff origin/main...HEAD`. On `main`, that diff is empty, so a
    // diff-driven repo badge would always report 0 regardless of repo
    // state. badge_artifacts_summary_markdown_includes the diff in
    // diff-scope, so this test pins the absence here.
    for format in [
        "repo-badge-json",
        "repo-badge-shields",
        "repo-badge-plus-json",
        "repo-badge-plus-shields",
    ] {
        let args = repo_badge_artifact_command_args(format, None);
        for arg in &args {
            if arg == "--diff" || arg == "--base" {
                return Err(format!(
                    "repo-scope command args must not contain `--diff` or `--base` for {format:?}: {args:?}"
                ));
            }
            if arg.contains("origin/main") || arg.contains("badge-input.diff") {
                return Err(format!(
                    "repo-scope command args must not reference origin/main or the diff input for {format:?}: {args:?}"
                ));
            }
        }
        if args.last().map(String::as_str) != Some(format) {
            return Err(format!(
                "expected last arg to be {format:?}, got {:?}",
                args.last()
            ));
        }
    }
    Ok(())
}

#[test]
fn repo_badge_artifact_command_args_can_use_gap_ledger_without_diff() -> Result<(), String> {
    let args =
        repo_badge_artifact_command_args("repo-badge-json", Some(Path::new("gap-ledger.json")));
    if args.iter().any(|arg| arg == "--diff" || arg == "--base") {
        return Err(format!(
            "gap-ledger repo-scope command args must remain diff-free: {args:?}"
        ));
    }
    let gap_arg = args
        .windows(2)
        .find(|window| window[0] == "--gap-ledger")
        .map(|window| window[1].as_str());
    if gap_arg != Some("gap-ledger.json") {
        return Err(format!(
            "expected --gap-ledger gap-ledger.json in repo args, got {args:?}"
        ));
    }
    Ok(())
}

#[test]
fn repo_badge_artifact_timeout_reports_no_public_endpoint_claim() -> Result<(), String> {
    let args = repo_badge_artifact_command_args("repo-badge-json", None);
    let message = repo_badge_artifact_stdout_from_output(
        "cargo",
        &args,
        "repo-badge-json",
        Duration::from_millis(12),
        TimedOutput {
            status: None,
            stdout: "partial badge output".to_string(),
            stderr: "still scanning repo seams".to_string(),
            duration: Duration::from_millis(13),
            timed_out: true,
        },
    )
    .expect_err("timeout should fail closed");

    assert!(message.contains("timed out after 12 ms"));
    assert!(message.contains("repo-badge-json"));
    assert!(message.contains("no public badge endpoint was refreshed"));
    assert!(message.contains("no public badge count is claimed"));
    assert!(message.contains(REPO_BADGE_ARTIFACT_TIMEOUT_ENV));
    Ok(())
}

#[test]
fn repo_badge_artifact_nonzero_status_carries_stdout_and_stderr() -> Result<(), String> {
    let args = repo_badge_artifact_command_args("repo-badge-json", None);
    let message = repo_badge_artifact_stdout_from_output(
        "cargo",
        &args,
        "repo-badge-json",
        Duration::from_secs(90),
        TimedOutput {
            status: Some(failure_exit_status()),
            stdout: "partial stdout".to_string(),
            stderr: "repo badge failure".to_string(),
            duration: Duration::from_millis(13),
            timed_out: false,
        },
    )
    .expect_err("non-zero status should fail closed");

    assert!(message.contains("failed with"));
    assert!(message.contains("partial stdout"));
    assert!(message.contains("repo badge failure"));
    Ok(())
}

#[test]
fn repo_badge_artifact_stdout_returns_stdout_on_success() -> Result<(), String> {
    let args = repo_badge_artifact_command_args("repo-badge-json", None);
    let stdout = repo_badge_artifact_stdout_from_output(
        "cargo",
        &args,
        "repo-badge-json",
        Duration::from_secs(90),
        TimedOutput {
            status: Some(success_exit_status()),
            stdout: "badge json\n".to_string(),
            stderr: String::new(),
            duration: Duration::from_millis(13),
            timed_out: false,
        },
    )?;

    assert_eq!(stdout, "badge json\n");
    Ok(())
}

#[test]
fn repo_badge_artifact_command_runs_with_timeout() -> Result<(), String> {
    with_repo_cwd(|| {
        let args = vec!["--version".to_string()];
        let stdout = run_repo_badge_artifact_command(
            "rustc",
            &args,
            "repo-badge-json",
            Duration::from_secs(30),
        )?;

        assert!(stdout.contains("rustc"));
        Ok(())
    })
}

#[test]
fn repo_badge_artifact_missing_status_reports_format() -> Result<(), String> {
    let args = repo_badge_artifact_command_args("repo-badge-shields", None);
    let message = repo_badge_artifact_stdout_from_output(
        "cargo",
        &args,
        "repo-badge-shields",
        Duration::from_secs(90),
        TimedOutput {
            status: None,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_millis(13),
            timed_out: false,
        },
    )
    .expect_err("missing status should fail closed");

    assert!(message.contains("finished without an exit status"));
    assert!(message.contains("repo-badge-shields"));
    Ok(())
}

#[test]
fn repo_badge_artifact_timeout_ms_uses_positive_env_override_only() -> Result<(), String> {
    assert_eq!(
        repo_badge_artifact_timeout_ms_from_env(None),
        REPO_BADGE_ARTIFACT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        repo_badge_artifact_timeout_ms_from_env(Some(String::new())),
        REPO_BADGE_ARTIFACT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        repo_badge_artifact_timeout_ms_from_env(Some("0".to_string())),
        REPO_BADGE_ARTIFACT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        repo_badge_artifact_timeout_ms_from_env(Some("not-a-timeout".to_string())),
        REPO_BADGE_ARTIFACT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        repo_badge_artifact_timeout_ms_from_env(Some("120000".to_string())),
        120_000
    );
    Ok(())
}

#[test]
fn repo_exposure_summary_report_timeout_ms_uses_positive_env_override_only() -> Result<(), String> {
    assert_eq!(
        repo_exposure_summary_report_timeout_ms_from_env(None),
        REPO_EXPOSURE_SUMMARY_REPORT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        repo_exposure_summary_report_timeout_ms_from_env(Some(String::new())),
        REPO_EXPOSURE_SUMMARY_REPORT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        repo_exposure_summary_report_timeout_ms_from_env(Some("0".to_string())),
        REPO_EXPOSURE_SUMMARY_REPORT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        repo_exposure_summary_report_timeout_ms_from_env(Some("not-a-timeout".to_string())),
        REPO_EXPOSURE_SUMMARY_REPORT_DEFAULT_TIMEOUT_MS
    );
    assert_eq!(
        repo_exposure_summary_report_timeout_ms_from_env(Some("300000".to_string())),
        300_000
    );
    Ok(())
}

#[test]
fn repo_exposure_summary_report_timeout_writes_non_consumable_limitation() -> Result<(), String> {
    with_temp_cwd("repo-exposure-summary-timeout", |_root| {
        let args = repo_seam_inventory_command_args_for_root("repo-exposure-summary-json", ".");
        let output_path = Path::new("target/ripr/reports/repo-exposure-summary.json");
        write(output_path, "stale summary");

        write_repo_exposure_summary_report_with_runner(
            &args,
            Duration::from_millis(7),
            |_args, _timeout| {
                Ok(TimedOutput {
                        status: None,
                        stdout: "{\"format\":\"repo-exposure-summary-json\"".to_string(),
                        stderr: concat!(
                            "ripr_repo_exposure_latency phase=inventory_seams status=ok duration_ms=3\n",
                            "ripr_repo_exposure_latency phase=evidence_for_seams_progress status=processed_500_of_39021 duration_ms=7\n",
                        )
                        .to_string(),
                        duration: Duration::from_millis(8),
                        timed_out: true,
                    })
            },
        )?;

        let json_text = fs::read_to_string(output_path).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["status"], "warn");
        assert_eq!(value["basis"], "limited_runtime_status");
        assert_eq!(value["run_status"], "limited_timeout");
        assert_eq!(value["runtime_status"]["downstream_consumable"], false);
        assert_eq!(
            value["runtime_status"]["limitation_category"],
            "repo_exposure_summary_timeout"
        );
        assert_eq!(value["generation"]["status"], "timeout");
        assert_eq!(value["generation"]["timed_out"], true);
        assert_eq!(value["generation"]["latency_trace_events_total"], 2);
        assert_eq!(
            value["generation"]["latency_trace_tail"][1]["status"],
            "processed_500_of_39021"
        );
        assert!(
            value["metrics"]
                .as_object()
                .is_some_and(|map| map.is_empty())
        );
        assert!(value["top_files"].as_array().is_some_and(Vec::is_empty));
        assert_eq!(value["run_limitations"][0]["downstream_consumable"], false);
        assert!(
            value["run_limitations"][0]["repair_route"]
                .as_str()
                .unwrap_or_default()
                .contains(REPO_EXPOSURE_SUMMARY_REPORT_TIMEOUT_ENV)
        );
        assert!(!json_text.contains("stale summary"));
        assert!(!json_text.contains("unsuppressed_exposure_gaps"));
        Ok(())
    })
}

#[test]
fn repo_exposure_summary_report_success_writes_complete_summary() -> Result<(), String> {
    with_temp_cwd("repo-exposure-summary-success", |_root| {
        let args = repo_seam_inventory_command_args_for_root("repo-exposure-summary-json", ".");
        let output_path = Path::new("target/ripr/reports/repo-exposure-summary.json");
        let summary = r#"{
  "schema_version": "0.1",
  "format": "repo-exposure-summary-json",
  "scope": "repo",
  "basis": "canonical_actionable_gap",
  "metrics": {
    "raw_seams": 5,
    "unsuppressed_exposure_gaps": 2
  },
  "reason_breakdown": {
    "actionability": {
      "upgrade_assertion": 2
    }
  },
  "limits": {
    "top_files_limit": 25,
    "top_files_total": 0,
    "top_files_truncated": false
  },
  "top_files": []
}
"#;

        write_repo_exposure_summary_report_with_runner(
            &args,
            Duration::from_millis(7),
            |_args, _timeout| {
                Ok(TimedOutput {
                    status: Some(success_exit_status()),
                    stdout: summary.to_string(),
                    stderr: String::new(),
                    duration: Duration::from_millis(6),
                    timed_out: false,
                })
            },
        )?;

        let json_text = fs::read_to_string(output_path).map_err(|err| err.to_string())?;
        let value: Value = serde_json::from_str(&json_text).map_err(|err| err.to_string())?;
        assert_eq!(value["format"], "repo-exposure-summary-json");
        assert_eq!(value["basis"], "canonical_actionable_gap");
        assert_eq!(value["metrics"]["unsuppressed_exposure_gaps"], 2);
        assert!(value.get("runtime_status").is_none());
        Ok(())
    })
}

#[test]
fn repo_badge_artifact_native_slot_reuses_diff_mapping() -> Result<(), String> {
    // The native-slot mapping is keyed on the format's `*-json` suffix,
    // not on diff-vs-repo prefix; repo formats reuse it so badge_artifacts
    // / repo_badge_artifacts share the same slotting helper.
    let cases = [
        ("repo-badge-json", Some(BadgeNativeSlot::Ripr)),
        ("repo-badge-plus-json", Some(BadgeNativeSlot::RiprPlus)),
        ("repo-badge-shields", None),
        ("repo-badge-plus-shields", None),
    ];
    for (format, expected) in cases {
        let actual = badge_artifact_native_slot(format);
        if actual != expected {
            return Err(format!(
                "for format {format:?} expected {expected:?}, got {actual:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn repo_badge_artifacts_summary_markdown_carries_baseline_disclaimer() -> Result<(), String> {
    let markdown = repo_badge_artifacts_summary_markdown(
        STUB_RIPR_NATIVE_JSON,
        STUB_RIPR_PLUS_NATIVE_JSON,
        None,
    );

    let must_contain = [
        "# ripr repo badges",
        "classified repo seams",
        "not against `git diff origin/main...HEAD`",
        "## ripr",
        "## ripr+",
        "- `repo-ripr-badge.json`",
        "- `repo-ripr-plus-badge-shields.json`",
    ];
    for expected in must_contain {
        if !markdown.contains(expected) {
            return Err(format!(
                "expected '{expected}' in repo markdown, got:\n{markdown}"
            ));
        }
    }
    Ok(())
}

#[test]
fn repo_badge_artifacts_summary_markdown_names_gap_ledger_basis() -> Result<(), String> {
    let markdown = repo_badge_artifacts_summary_markdown(
        STUB_RIPR_NATIVE_JSON,
        STUB_RIPR_PLUS_NATIVE_JSON,
        Some(Path::new("target/ripr/reports/gap-decision-ledger.json")),
    );

    for expected in [
        "explicit gap decision ledger",
        "policy-targeted `GapRecord` projection eligibility",
        "target/ripr/reports/gap-decision-ledger.json",
    ] {
        if !markdown.contains(expected) {
            return Err(format!(
                "expected '{expected}' in repo markdown, got:\n{markdown}"
            ));
        }
    }
    Ok(())
}

#[test]
fn repo_badge_artifacts_summary_markdown_omits_forbidden_terms() -> Result<(), String> {
    let markdown = repo_badge_artifacts_summary_markdown(
        STUB_RIPR_NATIVE_JSON,
        STUB_RIPR_PLUS_NATIVE_JSON,
        None,
    );

    // Repo badge output is public-facing: it must not borrow runtime
    // mutation language ("killed", "survived", "proven", "adequate")
    // and must not pretend to be a coverage metric.
    let lower = markdown.to_lowercase();
    for term in ["coverage", "uncovered", "killed", "proven", "adequate"] {
        if lower.contains(term) {
            return Err(format!(
                "forbidden term '{term}' found in repo markdown:\n{markdown}"
            ));
        }
    }
    for forbidden_word in ["survived", "untested"] {
        for window in lower.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if window == forbidden_word {
                return Err(format!(
                    "forbidden word '{forbidden_word}' found in repo markdown:\n{markdown}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn repo_exposure_latency_trace_parses_phase_lines() -> Result<(), String> {
    let traces = repo_exposure_latency_trace(
        "noise\nripr_repo_exposure_latency phase=cache_load status=hit duration_ms=7\n",
    );
    if traces.len() != 1 {
        return Err(format!("expected one trace line, got {}", traces.len()));
    }
    assert_eq!(traces[0].phase, "cache_load");
    assert_eq!(traces[0].status, "hit");
    assert_eq!(traces[0].duration_ms, 7);
    Ok(())
}

#[test]
fn repo_exposure_latency_retains_bounded_cache_failures_and_missing_state() -> Result<(), String> {
    let rows: Vec<_> = (0..32)
        .map(|i| {
            serde_json::json!({
                "path": format!("src/file_{i}.rs"),
                "stage": "write",
                "error": "portable failure"
            })
        })
        .collect();
    let receipt = serde_json::json!({
        "schema_version": "0.1", "hits": 2, "misses": 35,
        "invalidated": 1, "corrupt_ignored": 0, "stores": 0,
        "store_errors": 35, "store_failures": rows,
        "store_failures_dropped": 3
    });
    let run = repo_exposure_latency_run_from_output(
        "repo-exposure-json",
        TimedOutput {
            status: Some(success_exit_status()),
            stdout: "{}".to_string(),
            stderr: format!("ripr_file_fact_cache_receipt {receipt}\n"),
            duration: Duration::from_millis(3),
            timed_out: false,
        },
    );
    let cache = run
        .file_fact_cache
        .as_ref()
        .ok_or("missing cache receipt")?;
    assert_eq!(cache.store_errors, 35);
    assert_eq!(cache.store_failures.len(), 32);
    assert_eq!(cache.store_failures_dropped, 3);
    let report = RepoExposureLatencyReport {
        status: "pass".to_string(),
        timeout_ms: 10,
        binary: "target/debug/ripr".to_string(),
        runs: vec![run],
    };
    let json: Value = serde_json::from_str(&repo_exposure_latency_json(&report))
        .map_err(|err| format!("invalid latency JSON: {err}"))?;
    assert_eq!(
        json["runs"][0]["file_fact_cache"]["store_failures"][0]["path"],
        "src/file_0.rs"
    );
    assert_eq!(
        json["runs"][0]["file_fact_cache"]["store_failures_dropped"],
        3
    );
    let markdown = repo_exposure_latency_markdown(&report);
    assert!(markdown.contains("| src/file_0.rs | write | portable failure |"));
    assert!(markdown.contains("dropped failures: 3"));

    let (cache, limitation) = repo_exposure_file_fact_cache_from_stderr("noise\n");
    assert!(cache.is_none());
    assert_eq!(limitation.as_deref(), Some("cache_phase_not_observed"));
    let invalid = receipt
        .to_string()
        .replace("src/file_0.rs", "/outside/file_0.rs");
    let (cache, limitation) = repo_exposure_file_fact_cache_from_stderr(&format!(
        "ripr_file_fact_cache_receipt {invalid}"
    ));
    assert!(cache.is_none());
    assert_eq!(limitation.as_deref(), Some("invalid_cache_receipt"));
    let mut control_path = receipt.clone();
    control_path["store_failures"][0]["path"] = Value::String("src/a\nheading.rs".to_string());
    let (cache, limitation) = repo_exposure_file_fact_cache_from_stderr(&format!(
        "ripr_file_fact_cache_receipt {control_path}"
    ));
    assert!(cache.is_none());
    assert_eq!(limitation.as_deref(), Some("invalid_cache_receipt"));
    let mut dotted_path = receipt.clone();
    dotted_path["store_failures"][0]["path"] = Value::String("src/foo..rs".to_string());
    let (cache, limitation) = repo_exposure_file_fact_cache_from_stderr(&format!(
        "ripr_file_fact_cache_receipt {dotted_path}"
    ));
    assert!(cache.is_some());
    assert!(limitation.is_none());
    // A producer row with no portable spelling keeps its counters and says so.
    let mut unrepresentable = receipt.clone();
    unrepresentable["store_failures"][0]["path"] = Value::Null;
    let (cache, limitation) = repo_exposure_file_fact_cache_from_stderr(&format!(
        "ripr_file_fact_cache_receipt {unrepresentable}"
    ));
    assert!(limitation.is_none());
    let unrepresentable_report = RepoExposureLatencyReport {
        runs: vec![RepoExposureLatencyRun {
            file_fact_cache: cache,
            ..report.runs[0].clone()
        }],
        ..report.clone()
    };
    let json: Value = serde_json::from_str(&repo_exposure_latency_json(&unrepresentable_report))
        .map_err(|err| format!("invalid latency JSON: {err}"))?;
    assert_eq!(
        json["runs"][0]["file_fact_cache"]["store_failures"][0]["path"],
        Value::Null
    );
    assert!(
        repo_exposure_latency_markdown(&unrepresentable_report)
            .contains("| _unrepresentable path_ | write | portable failure |")
    );
    // `store_errors` is authoritative: retained rows plus the dropped count
    // must account for every failure, and rows are dropped only past the cap.
    let mut uncounted = receipt.clone();
    uncounted["store_errors"] = Value::from(36);
    let (cache, limitation) = repo_exposure_file_fact_cache_from_stderr(&format!(
        "ripr_file_fact_cache_receipt {uncounted}"
    ));
    assert!(cache.is_none());
    assert_eq!(limitation.as_deref(), Some("invalid_cache_receipt"));
    let mut early_drop = receipt.clone();
    early_drop["store_failures"] = serde_json::json!([]);
    early_drop["store_errors"] = Value::from(3);
    let (cache, limitation) = repo_exposure_file_fact_cache_from_stderr(&format!(
        "ripr_file_fact_cache_receipt {early_drop}"
    ));
    assert!(cache.is_none());
    assert_eq!(limitation.as_deref(), Some("invalid_cache_receipt"));
    let (cache, limitation) =
        repo_exposure_file_fact_cache_from_stderr("ripr_file_fact_cache_receipt {broken}\n");
    assert!(cache.is_none());
    assert_eq!(limitation.as_deref(), Some("malformed_cache_receipt"));
    let timeout = repo_exposure_latency_run_from_output(
        "repo-exposure-json",
        TimedOutput {
            status: None,
            stdout: "partial".to_string(),
            stderr: format!("ripr_file_fact_cache_receipt {receipt}\n"),
            duration: Duration::from_millis(10),
            timed_out: true,
        },
    );
    assert_eq!(timeout.status, "timeout");
    assert_eq!(
        timeout
            .file_fact_cache
            .as_ref()
            .map(|cache| cache.store_errors),
        Some(35)
    );
    assert!(timeout.file_fact_cache_limitation.is_none());
    let escaped = super::super::latency_markdown_cell("a`|<script>&\\\nline");
    assert_eq!(escaped, "a&#96;\\|&lt;script&gt;&amp;&#92; line");
    Ok(())
}

#[test]
fn repo_exposure_latency_report_json_and_markdown_are_structured() -> Result<(), String> {
    let runs = vec![
        RepoExposureLatencyRun {
            format: "repo-exposure-json".to_string(),
            status: "timeout".to_string(),
            duration_ms: 30_000,
            exit_code: None,
            stdout_bytes: 0,
            stderr_bytes: 91,
            trace: vec![
                RepoExposureLatencyTrace {
                    phase: "cache_load".to_string(),
                    status: "miss".to_string(),
                    duration_ms: 2,
                },
                RepoExposureLatencyTrace {
                    phase: "cold_compute".to_string(),
                    status: "ok".to_string(),
                    duration_ms: 29_998,
                },
            ],
            file_fact_cache: None,
            file_fact_cache_limitation: Some("cache_phase_not_observed".to_string()),
            resource_cost: None,
            resource_cost_limitation: Some("resource_cost_receipt_not_observed".to_string()),
        },
        RepoExposureLatencyRun {
            format: "repo-exposure-md".to_string(),
            status: "skipped_after_json_timeout".to_string(),
            duration_ms: 0,
            exit_code: None,
            stdout_bytes: 0,
            stderr_bytes: 0,
            trace: Vec::new(),
            file_fact_cache: None,
            file_fact_cache_limitation: Some("format_skipped".to_string()),
            resource_cost: None,
            resource_cost_limitation: Some("format_skipped".to_string()),
        },
    ];
    let report = RepoExposureLatencyReport {
        status: repo_exposure_latency_status(&runs),
        timeout_ms: 30_000,
        binary: "target/debug/ripr".to_string(),
        runs,
    };

    let json = repo_exposure_latency_json(&report);
    let value: Value =
        serde_json::from_str(&json).map_err(|err| format!("latency JSON should parse: {err}"))?;
    assert_eq!(value["schema_version"], "0.3");
    assert_eq!(value["report"], "repo-exposure-latency");
    assert_eq!(value["status"], "warn");
    assert_eq!(value["runs"][0]["trace"][0]["phase"], "cache_load");
    assert_eq!(value["runs"][0]["trace"][1]["phase"], "cold_compute");

    let markdown = repo_exposure_latency_markdown(&report);
    assert!(markdown.contains("# Repo Exposure Latency Report"));
    assert!(markdown.contains("`repo-exposure-json`"));
    assert!(markdown.contains("`skipped_after_json_timeout`"));
    assert!(markdown.contains("`cache_load`"));
    Ok(())
}

#[test]
fn repo_exposure_latency_debug_binary_uses_debug_ripr_name() -> Result<(), String> {
    let binary = ripr_debug_binary();
    let file_name = binary
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("debug binary should have a file name: {binary:?}"))?;
    let parent = binary
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("debug binary should have a parent directory: {binary:?}"))?;

    assert_eq!(file_name, format!("ripr{}", std::env::consts::EXE_SUFFIX));
    assert_eq!(parent, "debug");
    Ok(())
}

#[test]
fn repo_exposure_latency_report_json_records_exit_codes() -> Result<(), String> {
    let report = RepoExposureLatencyReport {
        status: "fail".to_string(),
        timeout_ms: 10,
        binary: "target/debug/ripr".to_string(),
        runs: vec![RepoExposureLatencyRun {
            format: "repo-exposure-json".to_string(),
            status: "fail".to_string(),
            duration_ms: 3,
            exit_code: Some(101),
            stdout_bytes: 4,
            stderr_bytes: 9,
            trace: Vec::new(),
            file_fact_cache: None,
            file_fact_cache_limitation: Some("cache_phase_not_observed".to_string()),
            resource_cost: None,
            resource_cost_limitation: Some("resource_cost_receipt_not_observed".to_string()),
        }],
    };

    let json = repo_exposure_latency_json(&report);
    let value: Value =
        serde_json::from_str(&json).map_err(|err| format!("latency JSON should parse: {err}"))?;
    assert_eq!(value["runs"][0]["exit_code"], 101);
    assert_eq!(value["runs"][0]["stdout_bytes"], 4);
    assert_eq!(value["runs"][0]["stderr_bytes"], 9);

    let markdown = repo_exposure_latency_markdown(&report);
    assert!(
        markdown.contains("| `repo-exposure-json` | `fail` | 3 ms | 101 | 4 bytes | 9 bytes |")
    );
    Ok(())
}

/// #5213: the consumer carries the analyzer's observed CPU and peak-memory
/// values into both report renderings, names the host it observed on, and
/// attributes the numbers to the analyzed process rather than to this harness.
#[test]
fn repo_exposure_latency_report_carries_observed_resource_cost() -> Result<(), String> {
    let run = repo_exposure_latency_run_from_output(
        "repo-exposure-json",
        TimedOutput {
            status: Some(success_exit_status()),
            stdout: "{}".to_string(),
            stderr: format!(
                "ripr_repo_exposure_latency phase=total status=ok duration_ms=5\n\
                 ripr_resource_cost_receipt {}\n",
                observed_resource_cost_receipt()
            ),
            duration: Duration::from_millis(6),
            timed_out: false,
        },
    );
    assert!(
        run.resource_cost_limitation.is_none(),
        "an observed receipt must carry no limitation: {:?}",
        run.resource_cost_limitation
    );
    let cost = run
        .resource_cost
        .as_ref()
        .ok_or("observed resource cost was dropped")?;
    assert_eq!(cost.observer, "ripr_process_self");
    assert_eq!(cost.observer_pid, 4242);
    assert_eq!(cost.host_os, "linux");
    assert_eq!(cost.host_arch, "x86_64");
    assert_eq!(
        cost.peak_resident_bytes,
        RepoExposureMeasurement::Observed { value: 41_943_040 }
    );
    let RepoExposureCpuCost::Observed {
        source_unit,
        source_unit_per_second,
        user_source,
        system_source,
        user_ms,
        system_ms,
    } = &cost.cpu
    else {
        return Err("observed CPU was not carried through".to_string());
    };
    assert_eq!(source_unit, "linux_user_hz_clock_ticks");
    assert_eq!(*source_unit_per_second, 100);
    assert_eq!((*user_source, *system_source), (1234, 56));
    assert_eq!((*user_ms, *system_ms), (12_340, 560));

    // The Windows-observed shape must carry through on the same terms: its
    // unit names a different rate, and its peak is already bytes rather than
    // kibibytes. Without this the consumer's Windows path is unproven on the
    // host that actually produces it.
    let windows = repo_exposure_latency_run_from_output(
        "repo-exposure-json",
        TimedOutput {
            status: Some(success_exit_status()),
            stdout: "{}".to_string(),
            stderr: format!(
                "ripr_resource_cost_receipt {}\n",
                windows_observed_resource_cost_receipt()
            ),
            duration: Duration::from_millis(80),
            timed_out: false,
        },
    );
    let windows_cost = windows
        .resource_cost
        .as_ref()
        .ok_or("observed Windows resource cost was dropped")?;
    assert_eq!(windows_cost.host_os, "windows");
    assert_eq!(
        windows_cost.peak_resident_bytes,
        RepoExposureMeasurement::Observed { value: 15_069_184 }
    );
    let RepoExposureCpuCost::Observed {
        source_unit,
        source_unit_per_second,
        user_source,
        system_source,
        user_ms,
        system_ms,
    } = &windows_cost.cpu
    else {
        return Err("observed Windows CPU was not carried through".to_string());
    };
    assert_eq!(source_unit, "windows_hundred_nanoseconds");
    assert_eq!(*source_unit_per_second, 10_000_000);
    assert_eq!((*user_source, *system_source), (156_250, 625_000));
    assert_eq!((*user_ms, *system_ms), (15, 62));
    // The millisecond fields must be recomputable from the raw source and the
    // named rate, on the Windows unit as well as the Linux one.
    for (source, ms) in [(*user_source, *user_ms), (*system_source, *system_ms)] {
        assert_eq!(ms, source.saturating_mul(1000) / 10_000_000);
    }

    let report = RepoExposureLatencyReport {
        status: "pass".to_string(),
        timeout_ms: 30_000,
        binary: "target/debug/ripr".to_string(),
        runs: vec![run, windows],
    };
    let value: Value = serde_json::from_str(&repo_exposure_latency_json(&report))
        .map_err(|err| format!("latency JSON should parse: {err}"))?;
    assert_eq!(value["schema_version"], "0.3");
    assert_eq!(
        value["runs"][0]["resource_cost"]["observer"],
        "ripr_process_self"
    );
    assert_eq!(value["runs"][0]["resource_cost"]["observer_pid"], 4242);
    assert_eq!(value["runs"][0]["resource_cost"]["host_os"], "linux");
    assert_eq!(value["runs"][0]["resource_cost"]["cpu"]["user_ms"], 12_340);
    assert_eq!(
        value["runs"][0]["resource_cost"]["peak_resident_bytes"]["value"],
        41_943_040
    );
    assert_eq!(value["runs"][0]["resource_cost_limitation"], Value::Null);

    let markdown = repo_exposure_latency_markdown(&report);
    assert!(markdown.contains("## Analyzer Resource Cost"), "{markdown}");
    assert!(
        markdown.contains("Observed on `linux`/`x86_64`"),
        "{markdown}"
    );
    assert!(
        markdown.contains("observer `ripr_process_self` (pid `4242`)"),
        "{markdown}"
    );
    assert!(
        markdown.contains("user 12340 ms (1234 source units)"),
        "{markdown}"
    );
    assert!(
        markdown.contains("Peak resident: 41943040 bytes."),
        "{markdown}"
    );
    // The Windows-observed run must be rendered with its own host and unit,
    // so a renderer that hardcoded the Linux rate cannot pass.
    assert!(
        markdown.contains("Observed on `windows`/`x86_64`"),
        "{markdown}"
    );
    assert!(
        markdown.contains("source unit `windows_hundred_nanoseconds` at 10000000 per second"),
        "{markdown}"
    );
    assert!(
        markdown.contains("user 15 ms (156250 source units)"),
        "{markdown}"
    );
    assert!(
        markdown.contains("Peak resident: 15069184 bytes."),
        "{markdown}"
    );
    Ok(())
}

/// #5213: an unavailable receipt stays unavailable in the report. It must not
/// be defaulted to zero CPU or zero memory, and a missing receipt must be
/// named rather than silently rendered as a zero row.
#[test]
fn repo_exposure_latency_report_preserves_unavailable_resource_cost() -> Result<(), String> {
    let unavailable = repo_exposure_latency_run_from_output(
        "repo-exposure-json",
        TimedOutput {
            status: Some(success_exit_status()),
            stdout: "{}".to_string(),
            stderr: format!(
                "ripr_resource_cost_receipt {}\n",
                unavailable_resource_cost_receipt()
            ),
            duration: Duration::from_millis(4),
            timed_out: false,
        },
    );
    let cost = unavailable
        .resource_cost
        .as_ref()
        .ok_or("the unavailable receipt itself must be retained")?;
    assert_eq!(
        cost.cpu,
        RepoExposureCpuCost::Unavailable {
            reason: "platform_not_supported".to_string()
        }
    );
    assert_eq!(
        cost.peak_resident_bytes,
        RepoExposureMeasurement::Unavailable {
            reason: "platform_not_supported".to_string()
        }
    );

    let missing = repo_exposure_latency_run_from_output(
        "repo-exposure-md",
        TimedOutput {
            status: Some(success_exit_status()),
            stdout: "# report".to_string(),
            stderr: "ripr: no trace switch was set\n".to_string(),
            duration: Duration::from_millis(4),
            timed_out: false,
        },
    );
    assert!(missing.resource_cost.is_none());
    assert_eq!(
        missing.resource_cost_limitation.as_deref(),
        Some("resource_cost_receipt_not_observed")
    );

    let report = RepoExposureLatencyReport {
        status: "pass".to_string(),
        timeout_ms: 30_000,
        binary: "target/debug/ripr".to_string(),
        runs: vec![unavailable, missing],
    };
    let value: Value = serde_json::from_str(&repo_exposure_latency_json(&report))
        .map_err(|err| format!("latency JSON should parse: {err}"))?;
    assert_eq!(
        value["runs"][0]["resource_cost"]["cpu"]["state"],
        "unavailable"
    );
    assert_eq!(
        value["runs"][0]["resource_cost"]["cpu"]["reason"],
        "platform_not_supported"
    );
    assert!(
        value["runs"][0]["resource_cost"]["cpu"]
            .get("user_ms")
            .is_none()
    );
    assert!(
        value["runs"][0]["resource_cost"]["peak_resident_bytes"]
            .get("value")
            .is_none()
    );
    assert_eq!(value["runs"][1]["resource_cost"], Value::Null);
    assert_eq!(
        value["runs"][1]["resource_cost_limitation"],
        "resource_cost_receipt_not_observed"
    );
    // Not one of the unavailable or missing paths may carry a fabricated zero.
    let rendered = repo_exposure_latency_json(&report);
    assert!(!rendered.contains("\"user_ms\": 0"), "{rendered}");
    assert!(!rendered.contains("\"value\": 0"), "{rendered}");

    let markdown = repo_exposure_latency_markdown(&report);
    assert!(
        markdown.contains("CPU unavailable: `platform_not_supported`"),
        "{markdown}"
    );
    assert!(
        markdown.contains("No zero CPU time is inferred."),
        "{markdown}"
    );
    assert!(
        markdown.contains("Unavailable: `resource_cost_receipt_not_observed`"),
        "{markdown}"
    );
    Ok(())
}

/// #5213: the consumer must refuse a receipt that answers an unavailable
/// observation with a zero number. A receipt carrying `"value": 0` where the
/// producer emits an explicit unavailable object is the wrong shape and is
/// rejected; the same shape with a genuine zero *observation* must survive, so
/// the rejection cannot pass by refusing everything.
#[test]
fn repo_exposure_latency_report_rejects_a_zero_standing_in_for_unavailable() -> Result<(), String> {
    let mut dishonest: Value = serde_json::from_str(&unavailable_resource_cost_receipt())
        .map_err(|err| format!("honest receipt must be valid JSON: {err}"))?;
    dishonest["peak_resident_bytes"]["value"] = Value::from(0);
    let zeroed = dishonest.to_string();
    assert_ne!(
        zeroed,
        unavailable_resource_cost_receipt(),
        "the control must actually differ from the honest receipt"
    );
    let (cost, limitation) =
        repo_exposure_resource_cost_from_stderr(&format!("ripr_resource_cost_receipt {zeroed}\n"));
    assert!(
        cost.is_none(),
        "an unavailable field must not carry a value at all: {zeroed}"
    );
    assert_eq!(
        limitation.as_deref(),
        Some("malformed_resource_cost_receipt")
    );

    // A zero *measurement* is legitimate and must survive. Removing this
    // assertion would let the control above pass by rejecting everything.
    let mut measured: Value = serde_json::from_str(&observed_resource_cost_receipt())
        .map_err(|err| format!("honest receipt must be valid JSON: {err}"))?;
    measured["peak_resident_bytes"]["value"] = Value::from(0);
    let measured_zero = measured.to_string();
    let (cost, limitation) = repo_exposure_resource_cost_from_stderr(&format!(
        "ripr_resource_cost_receipt {measured_zero}\n"
    ));
    assert!(limitation.is_none(), "{limitation:?}");
    assert_eq!(
        cost.map(|cost| cost.peak_resident_bytes),
        Some(RepoExposureMeasurement::Observed { value: 0 })
    );
    Ok(())
}

/// #5213: the CPU arm is refused on its own evidence. A zero `user_ms` smuggled
/// beside the `unavailable` state must not be accepted and silently dropped,
/// which is the same dishonesty the measurement arm guards against. Kept as a
/// separate test from the measurement arm so neither can mask the other.
#[test]
fn repo_exposure_latency_report_rejects_a_zero_in_the_unavailable_cpu_arm() -> Result<(), String> {
    let mut dishonest: Value = serde_json::from_str(&unavailable_resource_cost_receipt())
        .map_err(|err| format!("honest receipt must be valid JSON: {err}"))?;
    // A real Windows receipt reports CPU in hundred-nanosecond units, so a
    // smuggled `user_source` is the number a wrong producer would emit.
    dishonest["cpu"]["user_source"] = Value::from(0);
    let zeroed = dishonest.to_string();
    assert_ne!(
        zeroed,
        unavailable_resource_cost_receipt(),
        "the control must actually differ from the honest receipt"
    );
    let (cost, limitation) =
        repo_exposure_resource_cost_from_stderr(&format!("ripr_resource_cost_receipt {zeroed}\n"));
    assert!(
        cost.is_none(),
        "the unavailable CPU arm must not carry a number at all: {zeroed}"
    );
    assert_eq!(
        limitation.as_deref(),
        Some("malformed_resource_cost_receipt"),
        "{zeroed}"
    );

    // An honest unavailable CPU arm still parses, so the rejection above is
    // caused by the smuggled field and nothing else.
    let (cost, limitation) = repo_exposure_resource_cost_from_stderr(&format!(
        "ripr_resource_cost_receipt {}\n",
        unavailable_resource_cost_receipt()
    ));
    assert!(limitation.is_none(), "{limitation:?}");
    assert_eq!(
        cost.map(|cost| cost.cpu),
        Some(RepoExposureCpuCost::Unavailable {
            reason: "platform_not_supported".to_string()
        })
    );
    Ok(())
}

#[test]
fn repo_exposure_latency_report_names_bad_resource_cost_receipts() {
    for (body, expected) in [
        (
            "ripr_resource_cost_receipt {broken}\n".to_string(),
            "malformed_resource_cost_receipt",
        ),
        (
            format!(
                "ripr_resource_cost_receipt {}\nripr_resource_cost_receipt {}\n",
                observed_resource_cost_receipt(),
                observed_resource_cost_receipt()
            ),
            "duplicate_resource_cost_receipt",
        ),
        (
            format!(
                "ripr_resource_cost_receipt {}\n",
                observed_resource_cost_receipt()
                    .replace("ripr_process_self", "xtask_harness")
                    .replace("\"observer_pid\": 4242", "\"observer_pid\": 0")
            ),
            "invalid_resource_cost_receipt",
        ),
        (
            format!(
                "ripr_resource_cost_receipt {}\n",
                observed_resource_cost_receipt().replace("\"0.1\"", "\"9.9\"")
            ),
            "invalid_resource_cost_receipt",
        ),
    ] {
        let (cost, limitation) = repo_exposure_resource_cost_from_stderr(&body);
        assert!(cost.is_none(), "{expected} must not be accepted: {body}");
        assert_eq!(limitation.as_deref(), Some(expected), "{body}");
    }
}

#[test]
fn repo_exposure_latency_report_builder_skips_markdown_after_json_timeout() -> Result<(), String> {
    let mut formats = Vec::new();
    let report = build_repo_exposure_latency_report(
        Path::new("target/debug/ripr"),
        2_000,
        |_, format, _| {
            formats.push(format.to_string());
            Ok(latency_run_with_status(format, "timeout"))
        },
    )?;

    assert_eq!(formats, vec!["repo-exposure-json".to_string()]);
    assert_eq!(report.status, "warn");
    assert_eq!(report.timeout_ms, 2_000);
    assert_eq!(report.runs.len(), 2);
    assert_eq!(report.runs[0].status, "timeout");
    assert_eq!(report.runs[1].format, "repo-exposure-md");
    assert_eq!(report.runs[1].status, "skipped_after_json_timeout");
    Ok(())
}

#[test]
fn repo_exposure_latency_report_builder_runs_markdown_after_json_pass() -> Result<(), String> {
    let mut formats = Vec::new();
    let report = build_repo_exposure_latency_report(
        Path::new("target/debug/ripr"),
        30_000,
        |_, format, timeout| {
            formats.push(format.to_string());
            let mut run = latency_run_with_status(format, "pass");
            run.duration_ms = timeout.as_millis();
            Ok(run)
        },
    )?;

    assert_eq!(
        formats,
        vec![
            "repo-exposure-json".to_string(),
            "repo-exposure-md".to_string()
        ]
    );
    assert_eq!(report.status, "pass");
    assert_eq!(report.runs.len(), 2);
    assert_eq!(report.runs[0].duration_ms, 30_000);
    assert_eq!(report.runs[1].duration_ms, 30_000);
    Ok(())
}

#[test]
fn repo_exposure_latency_write_report_writes_markdown_and_json() -> Result<(), String> {
    with_temp_cwd("repo-exposure-latency-write", |_| {
        write_repo_exposure_latency_report(Path::new("target/debug/ripr"), 12, |_, format, _| {
            Ok(latency_run_with_status(format, "pass"))
        })?;

        let json = fs::read_to_string("target/ripr/reports/repo-exposure-latency.json")
            .map_err(|err| format!("failed to read latency JSON: {err}"))?;
        let markdown = fs::read_to_string("target/ripr/reports/repo-exposure-latency.md")
            .map_err(|err| format!("failed to read latency markdown: {err}"))?;

        assert!(json.contains("\"report\": \"repo-exposure-latency\""));
        assert!(markdown.contains("# Repo Exposure Latency Report"));
        Ok(())
    })
}

#[test]
fn repo_exposure_latency_run_invokes_binary_and_maps_failure() -> Result<(), String> {
    let run = repo_exposure_latency_run(
        Path::new("rustc"),
        "repo-exposure-json",
        Duration::from_secs(30),
    )?;

    assert_eq!(run.format, "repo-exposure-json");
    assert_eq!(run.status, "fail");
    assert!(!run.trace.iter().any(|trace| trace.phase.is_empty()));
    assert!(run.stderr_bytes > 0);
    Ok(())
}

#[test]
fn repo_exposure_latency_run_from_output_maps_status_and_trace() -> Result<(), String> {
    let output = TimedOutput {
        status: Some(success_exit_status()),
        stdout: "rustc 1.93.1\n".to_string(),
        stderr: String::new(),
        duration: Duration::from_millis(3),
        timed_out: false,
    };
    let pass_run = repo_exposure_latency_run_from_output("repo-exposure-json", output);
    assert_eq!(pass_run.status, "pass");
    assert_eq!(pass_run.format, "repo-exposure-json");
    assert!(pass_run.exit_code.is_some());
    assert!(pass_run.stdout_bytes > 0);

    let timeout_run = repo_exposure_latency_run_from_output(
        "repo-exposure-md",
        TimedOutput {
            status: None,
            stdout: "partial".to_string(),
            stderr: "ripr_repo_exposure_latency phase=cold_compute status=ok duration_ms=17\n"
                .to_string(),
            duration: Duration::from_millis(17),
            timed_out: true,
        },
    );
    assert_eq!(timeout_run.status, "timeout");
    assert_eq!(timeout_run.exit_code, None);
    assert_eq!(timeout_run.stdout_bytes, "partial".len());
    assert_eq!(timeout_run.trace[0].phase, "cold_compute");

    let fail_run = repo_exposure_latency_run_from_output(
        "repo-exposure-md",
        TimedOutput {
            status: None,
            stdout: String::new(),
            stderr: "failed".to_string(),
            duration: Duration::from_millis(3),
            timed_out: false,
        },
    );
    assert_eq!(fail_run.status, "fail");
    assert_eq!(fail_run.stderr_bytes, "failed".len());
    Ok(())
}

#[test]
fn repo_exposure_latency_status_and_empty_trace_markdown_are_stable() {
    let pass = latency_run_with_status("repo-exposure-json", "pass");
    let fail = latency_run_with_status("repo-exposure-json", "fail");
    let timeout = latency_run_with_status("repo-exposure-json", "timeout");
    let skipped = latency_run_with_status("repo-exposure-md", "skipped_after_json_timeout");

    assert_eq!(
        repo_exposure_latency_status(std::slice::from_ref(&pass)),
        "pass"
    );
    assert_eq!(repo_exposure_latency_status(&[timeout]), "warn");
    assert_eq!(repo_exposure_latency_status(&[skipped]), "warn");
    assert_eq!(
        repo_exposure_latency_status(&[
            latency_run_with_status("repo-exposure-json", "pass"),
            fail
        ]),
        "fail"
    );

    let report = RepoExposureLatencyReport {
        status: "pass".to_string(),
        timeout_ms: 30_000,
        binary: "target/debug/ripr".to_string(),
        runs: vec![pass],
    };
    let markdown = repo_exposure_latency_markdown(&report);
    assert!(markdown.contains("No analyzer trace lines were captured"));
}
