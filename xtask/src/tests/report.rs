use super::*;

#[test]
fn report_packet_index_fixture_corpus_guard_accepts_complete_contract() -> Result<(), String> {
    let root = temp_dir("report-packet-index-valid");
    let base = root.join("report-packet-index");
    let report = root.join("index.json");
    let markdown = root.join("index.md");
    write_report_packet_index_corpus(&base, &report, &markdown, "pass", 0, 0, true);
    write(
        &report,
        r#"{
  "schema_version": "0.1",
  "kind": "report_packet_index",
  "status": "pass",
  "summary": {
    "missing_expected": 0,
    "failures": 0,
    "warnings": 0,
    "start_here": "target/ripr/reports/pr-review-front-panel.md",
    "gate_authority": "target/ripr/reports/gate-decision.md",
    "advisory": true
  },
  "groups": [
    {
      "group": "start_here",
      "entries": [{"status": "available"}]
    },
    {
      "group": "pr_review_story",
      "entries": [{"status": "pass"}]
    }
  ],
  "missing_expected": [],
  "limits": ["Advisory report-packet index only."]
}
"#,
    );
    write(
        &markdown,
        "# RIPR Report Packet Index\n\nStatus: pass\n\nStart here:\n- PR review front panel: target/ripr/reports/pr-review-front-panel.md\n- Gate authority: target/ripr/reports/gate-decision.md\n",
    );

    let mut violations = Vec::new();
    super::super::validate_report_packet_index_fixture_corpus_at(&base, &mut violations)?;

    assert_eq!(violations, Vec::<String>::new());
    Ok(())
}

#[test]
fn report_packet_index_fixture_corpus_guard_requires_a_renderable_corpus() -> Result<(), String> {
    let root = temp_dir("report-packet-index-unrenderable");
    let base = root.join("report-packet-index");
    let report = root.join("index.json");
    let markdown = root.join("index.md");
    write_report_packet_index_corpus(&base, &report, &markdown, "pass", 0, 0, true);

    // Strip the two fields the dogfood render needs. Without them the gate
    // falls back to comparing two committed declarations, which is the state
    // #3972 reports.
    let corpus_path = base.join("corpus.json");
    let corpus = fs::read_to_string(&corpus_path)
        .map_err(|err| format!("failed to read the synthetic corpus: {err}"))?;
    let stripped = corpus
        .lines()
        .filter(|line| !line.contains("\"canonical_command\"") && !line.contains("\"packet_root\""))
        .collect::<Vec<_>>()
        .join("\n");
    write(&corpus_path, &format!("{stripped}\n"));

    let mut violations = Vec::new();
    super::super::validate_report_packet_index_fixture_corpus_at(&base, &mut violations)?;

    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("missing canonical_command")),
        "a corpus with no canonical_command renders nothing: {violations:?}"
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("is missing packet_root")),
        "a case with no packet_root has no producer input: {violations:?}"
    );
    Ok(())
}

#[test]
fn report_packet_index_fixture_corpus_guard_reports_contract_drift() -> Result<(), String> {
    let root = temp_dir("report-packet-index-invalid");
    let base = root.join("report-packet-index");
    let report = root.join("index.json");
    let markdown = root.join("index.md");
    write_report_packet_index_corpus(&base, &report, &markdown, "fail", 1, 1, true);
    write(
        &report,
        r#"{
  "schema_version": "0.2",
  "kind": "wrong_kind",
  "status": "warn",
  "summary": {
    "missing_expected": 0,
    "failures": 0,
    "warnings": 0,
    "start_here": null,
    "gate_authority": null,
    "advisory": false
  },
  "groups": [
    {
      "group": "unknown",
      "entries": [{"status": "mystery"}]
    }
  ],
  "missing_expected": [
    {"reason": "surprise"}
  ],
  "limits": ["Does not run mutation testing."]
}
"#,
    );
    write(&markdown, "# Wrong\n\nStatus: warn\n");

    let mut violations = Vec::new();
    super::super::validate_report_packet_index_fixture_corpus_at(&base, &mut violations)?;
    let report = violations.join("\n");

    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("report kind must be report_packet_index"));
    assert!(report.contains("expected status fail"));
    assert!(report.contains("summary.advisory must be true"));
    assert!(report.contains("expected missing_expected=1, got 0"));
    assert!(report.contains("expected failures=1, got 0"));
    assert!(report.contains("expected start_here_available=true, got false"));
    assert!(report.contains("expected gate_authority_present=true, got false"));
    assert!(report.contains("groups must use known group vocabulary"));
    assert!(report.contains("entries must use known status vocabulary"));
    assert!(report.contains("is missing required group start_here"));
    assert!(report.contains("missing_expected entries must use known reasons"));
    assert!(report.contains("report is missing advisory index limit"));
    assert!(report.contains("Markdown must use the report-packet index heading"));
    assert!(report.contains("Markdown must pin status fail"));
    assert!(report.contains("Markdown must name start-here artifacts"));
    assert!(report.contains("Markdown must name gate authority"));
    assert!(report.contains("Markdown must list missing expected artifacts"));
    Ok(())
}

#[test]
fn report_status_parser_reads_markdown_and_json() {
    assert_eq!(
        report_status_from_text("# Report\n\nStatus: warn\n\nBody"),
        Some("warn".to_string())
    );
    assert_eq!(
        report_status_from_text("{\n  \"status\": \"pass\"\n}"),
        Some("pass".to_string())
    );
    assert_eq!(report_status_from_text("# Report\n\nBody"), None);
}

#[test]
fn report_index_preserves_test_efficiency_not_run_with_explanation() -> Result<(), String> {
    with_temp_cwd("report-index-efficiency-not-run", |root| {
        write(
            &root.join("target/ripr/reports/test-efficiency.json"),
            r#"{
  "schema_version": "0.1",
  "status": "not_run",
  "advisory": true,
  "explanation": "No tests were selected; test-efficiency evidence was not established for this report. Run the report from the workspace root containing the Rust test sources.",
  "tests": [],
  "metrics": {"tests_scanned": 0}
}"#,
        );
        write(
            &root.join("target/ripr/reports/test-efficiency.md"),
            "# ripr test efficiency\n\nStatus: not_run\n\nTests scanned: 0\n\nExplanation: No tests were selected; test-efficiency evidence was not established for this report. Run the report from the workspace root containing the Rust test sources.\n",
        );
        let reports = super::super::report_index_entries()?;
        assert_eq!(reports.len(), 2);
        for entry in &reports {
            assert_eq!(entry.status, "not_run", "{}", entry.file);
        }
        let json: Value = serde_json::from_str(&super::super::report_index_json(
            "pass",
            &reports,
            &[],
            &[],
            &[],
        ))
        .map_err(|err| err.to_string())?;
        let indexed = json["reports"]
            .as_array()
            .ok_or("missing indexed reports")?;
        assert_eq!(indexed.len(), 2);
        for entry in indexed {
            assert_eq!(entry["status"], "not_run");
        }
        let markdown = super::super::report_index_markdown("pass", &reports, &[], &[], &[]);
        for file in ["test-efficiency.json", "test-efficiency.md"] {
            assert!(
                markdown
                    .lines()
                    .any(|line| line.contains(file) && line.contains("not_run"))
            );
        }
        Ok(())
    })
}

#[test]
fn report_index_missing_expected_tracks_changed_surfaces() {
    let reports = vec![ReportIndexEntry {
        file: "pr-summary.md".to_string(),
        path: "target/ripr/reports/pr-summary.md".to_string(),
        status: "pass".to_string(),
    }];
    let changes = vec![
        ChangedPath {
            path: "docs/PR_AUTOMATION.md".to_string(),
            statuses: BTreeSet::from(["M".to_string()]),
        },
        ChangedPath {
            path: ".ripr/goals/active.toml".to_string(),
            statuses: BTreeSet::from(["M".to_string()]),
        },
    ];

    let missing = report_index_missing_expected(&reports, &changes);

    assert!(missing.contains(&"target/ripr/reports/check-pr.md".to_string()));
    assert!(missing.contains(&"target/ripr/reports/doc-index.md".to_string()));
    assert!(missing.contains(&"target/ripr/reports/markdown-links.md".to_string()));
    assert!(!missing.contains(&"target/ripr/reports/campaign.md".to_string()));
    assert!(!missing.contains(&"target/ripr/reports/goals-next.md".to_string()));
}

#[test]
fn report_index_expects_golden_drift_for_output_changes() {
    let reports = vec![
        ReportIndexEntry {
            file: "pr-summary.md".to_string(),
            path: "target/ripr/reports/pr-summary.md".to_string(),
            status: "pass".to_string(),
        },
        ReportIndexEntry {
            file: "check-pr.md".to_string(),
            path: "target/ripr/reports/check-pr.md".to_string(),
            status: "pass".to_string(),
        },
    ];
    let changes = vec![ChangedPath {
        path: "crates/ripr/src/output/human.rs".to_string(),
        statuses: BTreeSet::from(["M".to_string()]),
    }];

    let missing = report_index_missing_expected(&reports, &changes);

    assert!(missing.contains(&"target/ripr/reports/golden-drift.md".to_string()));
}

#[test]
fn report_index_tracks_lane1_readiness_artifacts() {
    let missing_packets = report_index_lane1_readiness_packets(&[]);

    assert_eq!(report_index_lane1_overall_status(&missing_packets), "warn");
    assert_eq!(report_index_missing_artifact_count(&missing_packets), 12);
    assert!(
        missing_packets
            .iter()
            .all(|packet| packet.status == "missing")
    );

    let complete_reports = lane1_readiness_packet_specs()
        .iter()
        .flat_map(|spec| spec.artifacts.iter())
        .filter_map(|path| path.strip_prefix("target/ripr/reports/"))
        .map(|file| ReportIndexEntry {
            file: file.to_string(),
            path: format!("target/ripr/reports/{file}"),
            status: "advisory".to_string(),
        })
        .collect::<Vec<_>>();
    let complete_packets = report_index_lane1_readiness_packets(&complete_reports);

    assert_eq!(
        report_index_lane1_overall_status(&complete_packets),
        "present"
    );
    assert_eq!(report_index_missing_artifact_count(&complete_packets), 0);
    assert!(
        complete_packets
            .iter()
            .all(|packet| packet.status == "present")
    );
}

#[test]
fn report_index_status_keeps_failing_lane1_artifacts_advisory() {
    let reports = vec![ReportIndexEntry {
        file: "badge-basis.json".to_string(),
        path: "target/ripr/reports/badge-basis.json".to_string(),
        status: "fail".to_string(),
    }];
    let packets = report_index_lane1_readiness_packets(&reports);
    let commands = report_index_next_commands(&[], &packets);

    assert_eq!(
        super::super::report_index_status(&reports, &[], &[]),
        "warn"
    );
    assert_eq!(report_index_lane1_overall_status(&packets), "fail");
    assert_eq!(
        super::super::report_index_failing_artifact_count(&packets),
        1
    );
    assert!(commands.contains(&"cargo xtask badge-basis".to_string()));
}

#[test]
fn report_index_marks_limited_lane1_artifacts_as_warning() -> Result<(), String> {
    with_temp_cwd("report-index-limited-lane1", |root| {
        write(
            &root.join("target/ripr/reports/lane1-evidence-audit.json"),
            r#"{
  "schema_version": "0.9",
  "status": "advisory",
  "run_limitations": [
    {
      "category": "lane1_repo_exposure_incomplete",
      "summary": "Repo exposure capture was incomplete."
    }
  ],
  "summary": {
    "raw_headline_gaps": 0
  }
}
"#,
        );

        let reports = super::super::report_index_entries()?;
        let audit_entry = reports
            .iter()
            .find(|entry| entry.file == "lane1-evidence-audit.json")
            .ok_or_else(|| "missing lane1 audit report entry".to_string())?;
        assert_eq!(audit_entry.status, "warn");

        let packets = super::super::report_index_lane1_readiness_packets(&reports);
        let audit_packet = packets
            .iter()
            .find(|packet| packet.id == "lane1_evidence_audit")
            .ok_or_else(|| "missing lane1 evidence audit packet".to_string())?;
        assert_eq!(audit_packet.status, "warn");
        assert_eq!(
            super::super::report_index_status(&reports, &[], &[]),
            "warn"
        );
        Ok(())
    })
}

#[test]
fn report_index_status_keeps_non_lane1_failures_blocking() {
    let reports = vec![ReportIndexEntry {
        file: "check-pr.md".to_string(),
        path: "target/ripr/reports/check-pr.md".to_string(),
        status: "fail".to_string(),
    }];

    assert_eq!(
        super::super::report_index_status(&reports, &[], &[]),
        "fail"
    );
}

#[test]
fn report_index_next_commands_include_missing_lane1_readiness() {
    let packets = report_index_lane1_readiness_packets(&[]);
    let commands = report_index_next_commands(&[], &packets);

    assert!(commands.contains(&"cargo xtask evidence-health".to_string()));
    assert!(commands.contains(&"cargo xtask lane1-evidence-audit".to_string()));
    assert!(commands.contains(&"cargo xtask evidence-quality-scorecard".to_string()));
    assert!(commands.contains(&"cargo xtask evidence-quality-trend".to_string()));
    assert!(commands.contains(&"cargo xtask badge-basis".to_string()));
}

#[test]
fn report_index_repo_ops_packets_are_structured_for_agents() {
    let reports = [
        ("commands.md", "present"),
        ("commands.json", "present"),
        ("cockpit.md", "actionable"),
        ("cockpit.json", "actionable"),
        ("generated-clean.md", "pass"),
        ("gh-pr-status.md", "warn"),
        ("gh-pr-status.json", "warn"),
        ("pr-ready.md", "actionable"),
        ("pr-ready.json", "actionable"),
        ("suggested-fixes.md", "pass"),
        ("suggested-fixes.patch", "present"),
        ("check-pr.md", "pass"),
    ]
    .into_iter()
    .map(|(file, status)| ReportIndexEntry {
        file: file.to_string(),
        path: format!("target/ripr/reports/{file}"),
        status: status.to_string(),
    })
    .collect::<Vec<_>>();
    let receipts = vec![ReportIndexEntry {
        file: "check-pr.json".to_string(),
        path: "target/ripr/receipts/check-pr.json".to_string(),
        status: "pass".to_string(),
    }];

    let packets = report_index_repo_ops_packets(&reports, &receipts);

    assert_eq!(
        packets
            .iter()
            .find(|packet| packet.id == "command_mutability_catalog")
            .map(|packet| packet.status.as_str()),
        Some("present")
    );
    assert_eq!(
        packets
            .iter()
            .find(|packet| packet.id == "gh_pr_status")
            .map(|packet| packet.status.as_str()),
        Some("warn")
    );
    assert_eq!(
        packets
            .iter()
            .find(|packet| packet.id == "worktree_doctor")
            .map(|packet| packet.status.as_str()),
        Some("missing")
    );
    assert_eq!(
        packets
            .iter()
            .find(|packet| packet.id == "repo_cockpit")
            .map(|packet| packet.status.as_str()),
        Some("actionable")
    );
    assert_eq!(
        packets
            .iter()
            .find(|packet| packet.id == "pr_ready")
            .map(|packet| packet.status.as_str()),
        Some("actionable")
    );
    assert_eq!(
        packets
            .iter()
            .find(|packet| packet.id == "check_pr")
            .map(|packet| packet.status.as_str()),
        Some("pass")
    );
}

#[test]
fn report_packet_index_render_plan_reads_the_documented_command() -> Result<(), String> {
    let (args, json, markdown) = report_packet_index_render_plan(
        "ripr reports index --root . --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md",
    )?;
    assert_eq!(args.first().map(String::as_str), Some("reports"));
    assert!(!args.iter().any(|arg| arg == "ripr"));
    assert_eq!(json, Path::new("target/ripr/reports/index.json"));
    assert_eq!(markdown, Path::new("target/ripr/reports/index.md"));
    Ok(())
}

#[test]
fn report_packet_index_render_plan_names_a_command_it_cannot_run() {
    let missing_out_md = report_packet_index_render_plan(
        "ripr reports index --root . --out target/ripr/reports/index.json",
    )
    .expect_err("a command without --out-md names no Markdown to compare");
    assert!(
        missing_out_md.contains("--out-md"),
        "error should name the missing flag: {missing_out_md}"
    );

    let wrong_program = report_packet_index_render_plan("cargo xtask reports index")
        .expect_err("only the ripr binary renders this corpus");
    assert!(
        wrong_program.contains("must start with `ripr`"),
        "error should name the expected program: {wrong_program}"
    );

    let empty = report_packet_index_render_plan("   ")
        .expect_err("an empty canonical_command renders nothing");
    assert!(
        empty.contains("is empty"),
        "error should say the command is empty: {empty}"
    );
}

#[test]
fn report_packet_index_generated_at_violation_accepts_only_a_live_stamp() {
    assert_eq!(
        report_packet_index_generated_at_violation("unix_ms:1758672000000"),
        None
    );

    // The pin substitutes the observed stamp, so a renderer that stopped
    // emitting `unix_ms:<millis>` would otherwise surface only as whole-
    // document drift. Each of these has to be named.
    for observed in [
        "2026-05-10T12:00:00Z",
        "unix_ms:",
        "unix_ms:later",
        // The pinned golden value means the render did not stamp its own
        // clock, which is how a hand-written golden would look.
        "unix_ms:0",
    ] {
        assert!(
            report_packet_index_generated_at_violation(observed).is_some(),
            "`{observed}` should be reported as a generated_at violation"
        );
    }
}

#[test]
fn report_packet_index_case_id_violation_rejects_anything_but_one_component() {
    assert_eq!(
        report_packet_index_case_id_violation("complete_packet"),
        None
    );

    // The render clears the scratch directory named by the id, so each of
    // these would point `remove_dir_all` somewhere the corpus never named.
    for name in ["", ".", "..", "../other", "a/b", "trailing/"] {
        assert!(
            report_packet_index_case_id_violation(name).is_some(),
            "`{name}` should be rejected as a scratch directory component"
        );
    }
}

// ============================================================================
// Report rendering and status tests
// ============================================================================

#[test]
fn report_index_entries_contain_file_path_and_status() {
    let entry = ReportIndexEntry {
        file: "shape.md".to_string(),
        path: "target/ripr/reports/shape.md".to_string(),
        status: "pass".to_string(),
    };
    assert_eq!(entry.file, "shape.md");
    assert_eq!(entry.path, "target/ripr/reports/shape.md");
    assert_eq!(entry.status, "pass");
}

#[test]
fn report_index_repo_ops_status_preserves_actionable() {
    let artifacts = vec![ReportIndexRepoOpsArtifact {
        path: "target/ripr/reports/pr-ready.md".to_string(),
        available: true,
        status: "actionable".to_string(),
    }];
    assert_eq!(report_index_repo_ops_status(&artifacts), "actionable");
}

#[test]
fn report_commands_dispatch_through_report_facades() -> Result<(), String> {
    with_temp_cwd("report-dispatch", |_| {
        let commands = vec![
            XtaskCommand::PrSummary,
            XtaskCommand::Fixtures(vec!["missing".to_string()]),
            XtaskCommand::Goldens(vec!["unknown".to_string()]),
            XtaskCommand::Metrics,
            XtaskCommand::TestOracleReport,
            XtaskCommand::TestEfficiencyReport,
            XtaskCommand::ActionableGapOutcomes(Vec::new()),
            XtaskCommand::AgentSeamPackets(Some(".".to_string())),
            XtaskCommand::LspCockpitReport,
            XtaskCommand::OperatorCockpitReport,
            XtaskCommand::ReleaseReadiness(vec!["--version".to_string(), "0.5.0".to_string()]),
            XtaskCommand::TargetedTestOutcome(Vec::new()),
            XtaskCommand::MutationCalibration(Vec::new()),
            XtaskCommand::BunUbCalibration(Vec::new()),
            XtaskCommand::BunUbPreviewSummary(Vec::new()),
            XtaskCommand::ConfiguredBridgeInventory(Vec::new()),
            XtaskCommand::SarifPolicy(Vec::new()),
            XtaskCommand::Dogfood,
            XtaskCommand::Critic,
            XtaskCommand::Reports(vec!["index".to_string()]),
            XtaskCommand::Receipts(Vec::new()),
            XtaskCommand::GoldenDrift,
        ];
        let live_repo_analysis_commands = [
            "badge-artifacts",
            "repo-badge-artifacts",
            "repo-seam-inventory",
            "repo-exposure-report",
            "repo-exposure-latency-report",
            "evidence-health",
            "lane1-evidence-audit",
            "evidence-quality-scorecard",
            "evidence-quality-trend",
            "check-badge-endpoints",
        ];

        for command in commands {
            let label = format!("{command:?}");
            match dispatch::execute(command) {
                Ok(()) => {}
                Err(message) if !message.is_empty() => {}
                Err(_) => {
                    return Err(format!(
                        "{label} should either succeed or return an actionable error"
                    ));
                }
            }
        }
        for command_name in live_repo_analysis_commands {
            if !known_xtask_command(command_name) {
                return Err(format!(
                    "{command_name} should remain a known xtask command"
                ));
            }
        }

        Ok(())
    })
}
