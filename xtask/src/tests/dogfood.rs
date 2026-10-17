use super::*;

#[test]
fn dogfood_helpers_summarize_json_output() {
    let json = r#"{
  "summary": {"findings":2},
  "findings": [
    {"classification": "weakly_exposed", "stop_reasons": []},
    {"classification": "static_unknown", "stop_reasons": ["syntax unknown"]}
  ]
}"#;

    let counts = dogfood_class_counts(json);

    assert_eq!(json_number_after(json, "\"findings\":"), Some(2));
    assert_eq!(counts.get("weakly_exposed").copied(), Some(1));
    assert_eq!(counts.get("static_unknown").copied(), Some(1));
}

#[test]
fn dogfood_first_pr_metrics_count_adoption_states() {
    let runs = [
        sample_first_pr_run("boundary-gap", "actionable", "top_gap"),
        sample_first_pr_run("output-contract-gap", "actionable", "top_gap"),
        sample_first_pr_run("empty-diff", "no_action", "empty_diff"),
        sample_first_pr_run("missing", "blocked", "missing_artifact"),
        sample_first_pr_run("stale", "blocked", "stale_artifact"),
        sample_first_pr_run("wrong-root", "blocked", "wrong_root"),
        sample_first_pr_run("malformed", "blocked", "malformed_artifact"),
        sample_first_pr_run("timeout", "blocked", "timeout"),
    ];

    let metrics = dogfood_first_pr_metrics(&runs);

    assert_eq!(metrics.packets_total, 8);
    assert_eq!(metrics.top_gap_selected_total, 2);
    assert_eq!(metrics.no_action_total, 1);
    assert_eq!(metrics.blocked_total, 5);
    assert_eq!(metrics.missing_artifact_total, 1);
    assert_eq!(metrics.stale_artifact_total, 1);
    assert_eq!(metrics.wrong_root_total, 1);
    assert_eq!(metrics.malformed_artifact_total, 1);
    assert_eq!(metrics.timeout_total, 1);
}

#[test]
fn dogfood_reports_are_advisory() -> Result<(), String> {
    let run = DogfoodRun {
        name: "boundary_gap".to_string(),
        root: Path::new("fixtures/boundary_gap/input").to_path_buf(),
        diff: Path::new("fixtures/boundary_gap/diff.patch").to_path_buf(),
        actual_dir: Path::new("target/ripr/dogfood/boundary_gap").to_path_buf(),
        duration_ms: 10,
        findings: 1,
        class_counts: [("weakly_exposed".to_string(), 1usize)]
            .into_iter()
            .collect(),
        stop_reason_mentions: 1,
        errors: Vec::new(),
    };
    let gate_run = DogfoodGateRun {
            name: "calibrated-high-confidence-new-gap".to_string(),
            mode: "calibrated-gate".to_string(),
            actual_dir: Path::new(
                "target/ripr/dogfood/gate-adoption/calibrated-high-confidence-new-gap",
            )
            .to_path_buf(),
            json_path: Path::new(
                "target/ripr/dogfood/gate-adoption/calibrated-high-confidence-new-gap/gate-decision.json",
            )
            .to_path_buf(),
            markdown_path: Path::new(
                "target/ripr/dogfood/gate-adoption/calibrated-high-confidence-new-gap/gate-decision.md",
            )
            .to_path_buf(),
            duration_ms: 10,
            status: "blocked".to_string(),
            blocking: 1,
            acknowledged: 0,
            advisory: 0,
            expected_status: "blocked".to_string(),
            expected_blocking: 1,
            expected_acknowledged: 0,
            expected_advisory: 0,
            exit_success: false,
            expected_exit_success: false,
            errors: Vec::new(),
        };
    let first_action_run = DogfoodFirstActionRun {
            name: "actionable".to_string(),
            rendered: true,
            expected_dir: Path::new("fixtures/boundary_gap/expected/first-useful-action/actionable")
                .to_path_buf(),
            json_path: Path::new(
                "fixtures/boundary_gap/expected/first-useful-action/actionable/first-useful-action.json",
            )
            .to_path_buf(),
            markdown_path: Path::new(
                "fixtures/boundary_gap/expected/first-useful-action/actionable/first-useful-action.md",
            )
            .to_path_buf(),
            status: "actionable".to_string(),
            action_kind: "write_focused_test".to_string(),
            audience: "developer".to_string(),
            selected: true,
            static_movement: "unknown".to_string(),
            expected_status: "actionable".to_string(),
            expected_action_kind: "write_focused_test".to_string(),
            expected_audience: "developer".to_string(),
            expected_selected: true,
            expected_static_movement: "unknown".to_string(),
            errors: Vec::new(),
        };
    let first_pr_run = DogfoodFirstPrRun {
        name: "boundary-gap".to_string(),
        expected_dir: Path::new("fixtures/first_successful_pr/boundary-gap/expected").to_path_buf(),
        json_path: Path::new("fixtures/first_successful_pr/boundary-gap/expected/start-here.json")
            .to_path_buf(),
        markdown_path: Path::new(
            "fixtures/first_successful_pr/boundary-gap/expected/start-here.md",
        )
        .to_path_buf(),
        status: "actionable".to_string(),
        state: "top_gap".to_string(),
        top_gap_kind: "MissingBoundaryAssertion".to_string(),
        verify_command: Some("cargo xtask fixtures boundary_gap".to_string()),
        next_command: None,
        expected_status: "actionable".to_string(),
        expected_state: "top_gap".to_string(),
        description: "boundary gap receipt".to_string(),
        errors: Vec::new(),
    };
    let front_panel_run = DogfoodFrontPanelRun {
            name: "actionable".to_string(),
            rendered: true,
            report_path: Path::new(
                "fixtures/boundary_gap/expected/pr-review-front-panel/actionable/pr-review-front-panel.json",
            )
            .to_path_buf(),
            markdown_path: Path::new(
                "fixtures/boundary_gap/expected/pr-review-front-panel/actionable/pr-review-front-panel.md",
            )
            .to_path_buf(),
            status: "advisory".to_string(),
            top_issue_state: "actionable".to_string(),
            policy_state: "new_policy_eligible".to_string(),
            placement: "changed_line".to_string(),
            movement_state: "unknown".to_string(),
            coverage_grip_state: "not_available".to_string(),
            new_policy_eligible: 1,
            baseline_resolved: 0,
            blocking_candidates: 0,
            warnings: 0,
            expected_status: "advisory".to_string(),
            expected_top_issue_state: "actionable".to_string(),
            expected_policy_state: "new_policy_eligible".to_string(),
            expected_placement: "changed_line".to_string(),
            expected_movement_state: "unknown".to_string(),
            expected_coverage_grip_state: "not_available".to_string(),
            expected_new_policy_eligible: 1,
            expected_baseline_resolved: 0,
            expected_blocking_candidates: 0,
            expected_warnings: 0,
            reason: "The front panel should show the top focused-test action first.".to_string(),
            errors: Vec::new(),
        };
    let report_packet_index_run = DogfoodReportPacketIndexRun {
        name: "complete_packet".to_string(),
        packet_root: Path::new(
            "fixtures/boundary_gap/expected/report-packet-index/complete-packet/packet",
        )
        .to_path_buf(),
        render_command: "ripr reports index --root .".to_string(),
        rendered: true,
        actual_dir: Path::new("target/ripr/dogfood/report-packet-index/complete_packet")
            .to_path_buf(),
        json_path: Path::new(
            "fixtures/boundary_gap/expected/report-packet-index/complete-packet/index.json",
        )
        .to_path_buf(),
        markdown_path: Path::new(
            "fixtures/boundary_gap/expected/report-packet-index/complete-packet/index.md",
        )
        .to_path_buf(),
        status: "pass".to_string(),
        missing_expected: 0,
        failures: 0,
        warnings: 0,
        start_here_available: true,
        gate_authority_present: true,
        groups: vec!["start_here".to_string(), "policy_gates".to_string()],
        expected_status: "pass".to_string(),
        expected_missing_expected: 0,
        expected_failures: 0,
        expected_warnings: 0,
        expected_start_here_available: true,
        expected_gate_authority_present: true,
        expected_required_groups: vec!["start_here".to_string(), "policy_gates".to_string()],
        reason: "complete packet receipt".to_string(),
        expected_report: Path::new(
            "fixtures/boundary_gap/expected/report-packet-index/complete-packet/index.json",
        )
        .to_path_buf(),
        expected_markdown: Path::new(
            "fixtures/boundary_gap/expected/report-packet-index/complete-packet/index.md",
        )
        .to_path_buf(),
        errors: Vec::new(),
    };
    let generated_ci_run = DogfoodGeneratedCiCockpitRun {
        name: "generated-pr-ci-review-workflow".to_string(),
        command: GENERATED_CI_COCKPIT_COMMAND.to_string(),
        duration_ms: 10,
        start_here: true,
        repair_commands: 4,
        expected_repair_commands: 4,
        gate_authority_boundary: true,
        default_advisory: true,
        artifact_upload: true,
        language_grouping_status: "checked".to_string(),
        errors: Vec::new(),
    };
    let language_preview_run = DogfoodLanguagePreviewRun {
        name: "python_mixed_language_no_cross_route".to_string(),
        language: "python".to_string(),
        root: Path::new("fixtures/python_mixed_language_no_cross_route/input").to_path_buf(),
        diff: Path::new("fixtures/python_mixed_language_no_cross_route/diff.patch").to_path_buf(),
        actual_dir: Path::new(
            "target/ripr/dogfood/language-preview/python_mixed_language_no_cross_route",
        )
        .to_path_buf(),
        json_path: Path::new(
            "target/ripr/dogfood/language-preview/python_mixed_language_no_cross_route/check.json",
        )
        .to_path_buf(),
        human_path: Path::new(
            "target/ripr/dogfood/language-preview/python_mixed_language_no_cross_route/human.txt",
        )
        .to_path_buf(),
        duration_ms: 10,
        findings: 1,
        language_findings: 1,
        preview_findings: 1,
        missing_preview_status: 0,
        related_tests: 0,
        classifications: vec!["no_static_path".to_string()],
        static_limit_kinds: Vec::new(),
        expected_findings: 1,
        expected_preview_findings: 1,
        expected_missing_preview_status: 0,
        expected_related_tests: 0,
        expected_classifications: vec!["no_static_path".to_string()],
        expected_static_limit_kinds: Vec::new(),
        preview_enabled: true,
        reason: "mixed-language fixture must not cross-route related tests".to_string(),
        errors: Vec::new(),
    };
    let editor_gap_cockpit_run = DogfoodEditorGapCockpitRun {
            name: "typescript_preview_static_limit".to_string(),
            expected_dir: Path::new(
                "fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected",
            )
            .to_path_buf(),
            projection_path: Path::new(
                "fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/gap-projection.json",
            )
            .to_path_buf(),
            diagnostics_path: Path::new(
                "fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/lsp-diagnostics.json",
            )
            .to_path_buf(),
            hover_path: Path::new(
                "fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/lsp-hover.md",
            )
            .to_path_buf(),
            code_actions_path: Path::new(
                "fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/lsp-code-actions.json",
            )
            .to_path_buf(),
            status_path: Path::new(
                "fixtures/editor_gap_cockpit/typescript_preview_static_limit/expected/vscode-status.json",
            )
            .to_path_buf(),
            state: "actionable".to_string(),
            language: Some("typescript".to_string()),
            language_status: Some("preview".to_string()),
            diagnostics_projected: 1,
            actual_diagnostics: 1,
            fail_closed: false,
            actions_projected: vec![
                "copy_repair_packet".to_string(),
                "copy_static_limit_note".to_string(),
                "refresh".to_string(),
            ],
            actual_actions: 3,
            static_limit_kind: Some("mocked_module".to_string()),
            hover_static_before_action: true,
            expected_state: "actionable".to_string(),
            expected_language: Some("typescript".to_string()),
            expected_language_status: Some("preview".to_string()),
            expected_diagnostics: 1,
            expected_fail_closed: false,
            expected_actions: vec![
                "copy_repair_packet".to_string(),
                "copy_static_limit_note".to_string(),
                "refresh".to_string(),
            ],
            expected_static_limit_kind: Some("mocked_module".to_string()),
            reason: "preview static-limit fixture".to_string(),
            errors: Vec::new(),
        };
    let editor_first_pr_bridge_run = DogfoodEditorFirstPrBridgeRun {
            name: "packet_found_repairable".to_string(),
            expected_dir: Path::new(
                "fixtures/editor_first_pr_bridge/packet_found_repairable/expected",
            )
            .to_path_buf(),
            packet_path: Path::new(
                "fixtures/editor_first_pr_bridge/packet_found_repairable/expected/first-pr-status.json",
            )
            .to_path_buf(),
            diagnostics_path: Path::new(
                "fixtures/editor_first_pr_bridge/packet_found_repairable/expected/lsp-diagnostics.json",
            )
            .to_path_buf(),
            code_actions_path: Path::new(
                "fixtures/editor_first_pr_bridge/packet_found_repairable/expected/lsp-code-actions.json",
            )
            .to_path_buf(),
            status_path: Path::new(
                "fixtures/editor_first_pr_bridge/packet_found_repairable/expected/vscode-status.json",
            )
            .to_path_buf(),
            diagnosis_path: Path::new(
                "fixtures/editor_first_pr_bridge/packet_found_repairable/expected/setup-diagnosis.md",
            )
            .to_path_buf(),
            packet_state: "top_repairable_gap".to_string(),
            safe_actions: vec![
                "open_packet".to_string(),
                "copy_summary".to_string(),
                "copy_repair_packet".to_string(),
                "copy_verify_command".to_string(),
                "copy_receipt_command".to_string(),
                "copy_regeneration_guidance".to_string(),
            ],
            suppressed_actions: Vec::new(),
            receipt_movement: None,
            diagnostics: 1,
            action_commands: vec![
                "ripr.openFirstPrPacket".to_string(),
                "ripr.copyFirstPrSummary".to_string(),
                "ripr.copyFirstPrRepairPacket".to_string(),
                "ripr.copyFirstPrVerifyCommand".to_string(),
                "ripr.copyFirstPrReceiptCommand".to_string(),
                "ripr.copyFirstPrRegenerationGuidance".to_string(),
                "ripr.refresh".to_string(),
            ],
            first_pr_actions: vec![
                "open_packet".to_string(),
                "copy_summary".to_string(),
                "copy_repair_packet".to_string(),
                "copy_verify_command".to_string(),
                "copy_receipt_command".to_string(),
                "copy_regeneration_guidance".to_string(),
            ],
            fail_closed: false,
            expected_packet_state: "top_repairable_gap".to_string(),
            expected_safe_actions: vec![
                "open_packet".to_string(),
                "copy_summary".to_string(),
                "copy_repair_packet".to_string(),
                "copy_verify_command".to_string(),
                "copy_receipt_command".to_string(),
                "copy_regeneration_guidance".to_string(),
            ],
            expected_suppressed_actions: Vec::new(),
            expected_diagnostics: 1,
            expected_fail_closed: false,
            expected_receipt_movement: None,
            runtime_adequacy_claim: false,
            mutation_proof_claim: false,
            policy_gate_claim: false,
            pr_ready_claim: false,
            reason: "repairable first-pr bridge receipt".to_string(),
            errors: Vec::new(),
        };
    let finding_alignment_run = DogfoodFindingAlignmentRun {
        name: "config_policy_rendered_label_unobserved".to_string(),
        source_pr: "EffortlessMetrics/ripr#1016".to_string(),
        canonical_gap_id: "gap:config_policy_rendered_label_unobserved".to_string(),
        evidence_class: "config_or_policy_constant".to_string(),
        raw_findings_total: 2,
        canonical_items_total: 1,
        raw_finding_summary:
            "A rendered policy label aligns into one canonical output-observer item.".to_string(),
        gap_state: "actionable".to_string(),
        actionability: "add_output_observer".to_string(),
        user_outcome: "actionable_gap".to_string(),
        repair_kind: "output_observer".to_string(),
        target_test_type: "report_render_or_golden".to_string(),
        verify_command: "cargo xtask evidence-quality-scorecard".to_string(),
        static_limitation_category: None,
        static_limitation_repair_route: None,
        raw_findings_supporting_only: true,
        recommended_repair: "Add or update a report-render or golden observer.".to_string(),
        before_after_context:
            "Before/after context is pinned by finding-alignment dogfood receipts.".to_string(),
        must_not_claim: vec![
            "Do not infer actionability from raw static class.".to_string(),
            "Do not recommend mutation testing first.".to_string(),
        ],
        reason: "actionable config-policy alignment receipt".to_string(),
        errors: Vec::new(),
    };
    let surface_projection_alignment_run = super::super::DogfoodSurfaceProjectionAlignmentRun {
            name: "receipt_improved_top_next_action_alignment".to_string(),
            canonical_gap_id: "gap:boundary-discriminator-004".to_string(),
            packet_id: "boundary-discriminator-004".to_string(),
            repair_kind: "add_boundary_assertion".to_string(),
            verify_command: "cargo test -p ripr-swarm boundary_discriminator_004".to_string(),
            receipt_command: "cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004".to_string(),
            receipt_state: "receipt_movement_improved".to_string(),
            outcome: "evidence_improved".to_string(),
            top_next_action_kind: "attempt_ready_packet".to_string(),
            top_next_action_command: Some(
                "cargo xtask ripr-swarm attempt --packet boundary-discriminator-004 --dry-run"
                    .to_string(),
            ),
            readiness_status: "advisory".to_string(),
            attempted_packets: 1,
            improved_packets: 1,
            advisory_consumers: vec![
                "badge".to_string(),
                "lsp".to_string(),
                "pr_comment".to_string(),
                "ci".to_string(),
            ],
            reason: "single receipt-backed canonical gap projection alignment".to_string(),
            errors: Vec::new(),
        };
    let real_repair_attempt_run = super::super::DogfoodRealRepairAttemptRun {
        name: "repair_route_quality_metrics_improved".to_string(),
        source_ref: "EffortlessMetrics/ripr-swarm#415".to_string(),
        canonical_gap_id: "gap:repair-route-quality-metrics-missing".to_string(),
        packet_id: "repair-route-quality-metrics-001".to_string(),
        language: None,
        evidence_class: None,
        source_file: None,
        repair_kind: "add_repair_route_quality_metrics".to_string(),
        target_test_or_observer_shape: "readiness route-quality metric assertions".to_string(),
        verify_command: "cargo xtask check-pr".to_string(),
        verify_result: "pass".to_string(),
        receipt_command: "cargo xtask ripr-swarm readiness".to_string(),
        receipt_path: Some("target/ripr/reports/swarm-readiness.json".to_string()),
        receipt_state: "receipt_movement_improved".to_string(),
        actor_kind: "codex".to_string(),
        before_gap_state: "actionable".to_string(),
        after_gap_state: "actionable".to_string(),
        outcome: "evidence_improved".to_string(),
        attempted_repair: "Added repair-route quality report metrics.".to_string(),
        evidence_movement: "Readiness can now show repair_kind success-rate evidence.".to_string(),
        operator_note: "Route-quality proof stayed advisory.".to_string(),
        must_not_change: vec![
            "public badge semantics".to_string(),
            "CI gate defaults".to_string(),
        ],
        raw_evidence_refs: vec!["metrics/capabilities.toml".to_string()],
        missing_receipt_reason: None,
        reason: "real merged repair-loop PR improved route-quality evidence".to_string(),
        errors: Vec::new(),
    };
    let python_real_repo_eval_run = super::super::DogfoodPythonRealRepoEvalRun {
            name: "tiny_controlled_pytest_boundary_receipt".to_string(),
            repo_shape: "tiny_controlled_pytest".to_string(),
            source_kind: "scratch_repo".to_string(),
            source_ref: "target/ripr/python-real-repo-evals/tiny-controlled-pytest-receipt"
                .to_string(),
            command: "ripr check --root target/ripr/python-real-repo-evals/tiny-controlled-pytest-receipt --base HEAD~1 --format json".to_string(),
            runtime_ms: 114,
            top_finding_summary:
                "Python predicate-boundary repair card for calculate_discount amount >= threshold"
                    .to_string(),
            canonical_gap_id:
                "gap:python:app/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold"
                    .to_string(),
            repair_card_present: true,
            repair_action: "strengthen_existing_test".to_string(),
            agent_packet_present: true,
            agent_packet_task:
                "Strengthen the existing pytest boundary test for amount == threshold."
                    .to_string(),
            agent_packet_command: "ripr agent packet --root target/ripr/python-real-repo-evals/tiny-controlled-pytest-receipt --gap-ledger gap-ledger.json --gap-id gap:python:app/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold --json".to_string(),
            agent_packet_allowed_files: vec!["tests/test_pricing.py".to_string()],
            agent_packet_forbidden_files: vec!["app/pricing.py".to_string()],
            agent_packet_stop_if: vec![
                "import cannot be resolved".to_string(),
                "expected boundary value is ambiguous".to_string(),
                "production code edit appears necessary".to_string(),
            ],
            changed_owner: "python:app/pricing.py::calculate_discount".to_string(),
            missing_discriminator: "amount == threshold".to_string(),
            suggested_test_file: "tests/test_pricing.py".to_string(),
            suggested_test_name: "test_calculate_discount_smoke".to_string(),
            verify_command: "pytest tests/test_pricing.py::test_calculate_discount_smoke"
                .to_string(),
            verify_result: "pass".to_string(),
            verify_summary: "1 passed in 0.30s".to_string(),
            after_command: "ripr check --root target/ripr/python-real-repo-evals/tiny-controlled-pytest-receipt --base HEAD~1 --format json".to_string(),
            after_runtime_ms: 58,
            receipt_command: "ripr outcome --before before-check.json --after after-check.json --format json".to_string(),
            receipt_result: "pass".to_string(),
            gap_movement: "closed".to_string(),
            closed_gaps: 1,
            usability: "usable".to_string(),
            false_positive_notes: "none observed".to_string(),
            limitation_notes:
                "scoped repair routing is usable alpha; broader Python static facts remain preview/advisory"
                    .to_string(),
            unsupported_limitations: Vec::new(),
            ranked_top_3_findings: vec![
                super::super::DogfoodPythonRankedFinding {
                    rank: 1,
                    canonical_gap_id:
                        "gap:python:app/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold"
                            .to_string(),
                    repair_card_present: true,
                    usability: "usable".to_string(),
                    missing_discriminator: "amount == threshold".to_string(),
                    suggested_test_file: "tests/test_pricing.py".to_string(),
                    verify_command: "pytest tests/test_pricing.py::test_calculate_discount_smoke"
                        .to_string(),
                    false_positive_notes: "none observed".to_string(),
                    reason: "rank 1 repair card matched the closed Python receipt".to_string(),
                },
                super::super::DogfoodPythonRankedFinding {
                    rank: 2,
                    canonical_gap_id:
                        "gap:python:app/invoice.py:invoice_total:return_value:return_value:subtotal+tax+service_fee"
                            .to_string(),
                    repair_card_present: true,
                    usability: "usable".to_string(),
                    missing_discriminator: "return value == subtotal + tax + service_fee"
                        .to_string(),
                    suggested_test_file: "tests/test_invoice.py".to_string(),
                    verify_command: "pytest tests/test_invoice.py::test_invoice_total_smoke"
                        .to_string(),
                    false_positive_notes: "none observed".to_string(),
                    reason: "rank 2 repair card was concrete, placed, and verifiable"
                        .to_string(),
                },
                super::super::DogfoodPythonRankedFinding {
                    rank: 3,
                    canonical_gap_id:
                        "gap:python:app/validation.py:require_positive:exception_path:error_path:valueerror_positive_required"
                            .to_string(),
                    repair_card_present: true,
                    usability: "usable".to_string(),
                    missing_discriminator: "raises ValueError matching \"positive required\""
                        .to_string(),
                    suggested_test_file: "tests/test_validation.py".to_string(),
                    verify_command: "pytest tests/test_validation.py::test_rejects_zero_amount"
                        .to_string(),
                    false_positive_notes: "none observed".to_string(),
                    reason: "rank 3 repair card was concrete, placed, and verifiable"
                        .to_string(),
                },
            ],
            ranked_top_3_limit_reason: None,
            claim_boundary: vec![
                "Broader Python static facts remain preview/advisory".to_string(),
                "No arbitrary imports or tests were run by RIPR".to_string(),
                "No support-tier promotion".to_string(),
            ],
            reason: "sample Python real-repo eval receipt".to_string(),
            errors: Vec::new(),
        };
    let typescript_preview_repair_loop_run = super::super::DogfoodTypescriptPreviewRepairLoopRun {
            name: "typescript_boundary_predicate_proof".to_string(),
            source_fixture: "fixtures/typescript_boundary_gap".to_string(),
            source_finding_id: "probe:src_discount.ts:typescript_preview:2396aec1".to_string(),
            language: "typescript".to_string(),
            classification: "weakly_exposed".to_string(),
            changed_owner: "applyDiscount".to_string(),
            probe_family: "predicate".to_string(),
            oracle_kind: "unknown".to_string(),
            oracle_strength: "unknown".to_string(),
            gap_state: "advisory".to_string(),
            actionability_category: "incomplete_repair_packet".to_string(),
            static_limit_kind: None,
            repair_packet_ready: false,
            expected_test_or_observer_shape: "exact equality-boundary assertion".to_string(),
            verify_command: "npm test -- discount".to_string(),
            verify_result: "not_run".to_string(),
            receipt_command: "ripr agent receipt --source fixtures/typescript_boundary_gap"
                .to_string(),
            receipt_state: "fixture_receipt_recorded".to_string(),
            outcome: "proof_improved".to_string(),
            why_not_actionable: "preview lacks complete packet fields".to_string(),
            repair_route: "project canonical TypeScript repair packet fields later".to_string(),
            operator_note: "proof stays advisory".to_string(),
            must_not_change: vec![
                "production behavior".to_string(),
                "default gate or badge policy".to_string(),
            ],
            raw_evidence_refs: vec![
                "fixtures/typescript_boundary_gap/expected/check.json#probe:src_discount.ts:typescript_preview:2396aec1"
                    .to_string(),
            ],
            non_claims: vec![
                "no provider calls".to_string(),
                "no source edits".to_string(),
                "no generated tests".to_string(),
                "no runtime Jest/Vitest execution".to_string(),
                "no mutation execution".to_string(),
                "no default gates".to_string(),
                "no public badge contribution".to_string(),
                "no baseline authority".to_string(),
                "no RIPR Zero authority".to_string(),
                "no support-tier promotion".to_string(),
            ],
            reason: "sample TypeScript preview repair-loop receipt".to_string(),
            errors: Vec::new(),
        };
    let bun_ub_cross_language_run = super::super::DogfoodBunUbCrossLanguageRun {
            name: "bun_blob_31648_known_good".to_string(),
            source_case: "bun_blob_shared_and_resizable_present".to_string(),
            route_quality_case: "bun_blob_complete_ts_discriminated_advisory".to_string(),
            rust_file: "src/jsc/Blob.rs".to_string(),
            rust_owner: "Blob::from_js_without_defer_gc".to_string(),
            rust_boundary: "array_buffer.shared || array_buffer.resizable".to_string(),
            ts_test_file: super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE.to_string(),
            expected_state: "rust_ungripped_ts_discriminated".to_string(),
            observed_state: "rust_ungripped_ts_discriminated".to_string(),
            missing_discriminators: Vec::new(),
            missing_graph_legs: Vec::new(),
            suggested_test_file: "not_applicable".to_string(),
            manual_verdict: "agrees".to_string(),
            operator_action: "no_missing_bridge_discriminator".to_string(),
            review_before:
                "Rust-only review would manually grep for SharedArrayBuffer and maxByteLength"
                    .to_string(),
            review_after:
                "TypeScript/Bun preview credits configured Blob stable-byte witness evidence"
                    .to_string(),
            bridge_verdict: "configured_binding_edge_credited".to_string(),
            placement_verdict: "not_needed".to_string(),
            proof_mode: "advisory_cross_language_preview".to_string(),
            receipt_state: "closed".to_string(),
            repair_packet_ready: false,
            authority_boundary: "preview_advisory_only".to_string(),
            raw_evidence_refs: vec![
                "fixtures/typescript-bun-ub-calibration/corpus.json#bun_blob_shared_and_resizable_present".to_string(),
                "fixtures/cross-language-oracle-graph-corpus/corpus.json#bun_blob_complete_ts_discriminated_advisory".to_string(),
            ],
            non_claims: vec![
                "no source edits".to_string(),
                "no generated tests".to_string(),
                "no runtime Bun execution".to_string(),
                "no mutation execution".to_string(),
                "no default gates".to_string(),
                "no public badge contribution".to_string(),
                "no baseline authority".to_string(),
                "no RIPR Zero authority".to_string(),
                "no support-tier promotion".to_string(),
                "no public repair packet".to_string(),
                "no full cross-language proof".to_string(),
            ],
            reason: "sample Bun UB cross-language receipt".to_string(),
            errors: Vec::new(),
        };
    let user_surface_projection_run = super::super::DogfoodUserSurfaceProjectionRun {
            name: "badge_actionable_count_from_canonical_state".to_string(),
            surface: "badge".to_string(),
            artifact: "badge-basis".to_string(),
            headline: "ripr: 8 actionable".to_string(),
            run_status: "full".to_string(),
            projection_basis: "canonical_actionable_gap".to_string(),
            canonical_gap_id: "gap:boundary-discriminator-004".to_string(),
            packet_id: "boundary-discriminator-004".to_string(),
            repair_kind: "add_boundary_assertion".to_string(),
            top_next_action_kind: "attempt_ready_packet".to_string(),
            verify_command: "cargo test -p ripr-swarm boundary_discriminator_004".to_string(),
            receipt_command: "cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004".to_string(),
            source_alignment_case: "receipt_improved_top_next_action_alignment".to_string(),
            limitation_category: String::new(),
            runtime_repair_command: String::new(),
            actionable_count: 8,
            raw_findings_total: 312,
            consumes_canonical_state: true,
            reinterprets_raw_findings: false,
            raw_findings_headline: false,
            advisory: true,
            blocking_default: false,
            limited_state_visible: true,
            stale_state_visible: true,
            reason: "badge projection should use canonical actionable state".to_string(),
            errors: Vec::new(),
        };
    let pr_inline_comment_run = DogfoodPrInlineCommentRun {
            name: "publishable_changed_line".to_string(),
            actual_dir: Path::new(
                "fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line",
            )
            .to_path_buf(),
            json_path: Path::new(
                "fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line/comment-publish-plan.json",
            )
            .to_path_buf(),
            markdown_path: Path::new(
                "fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line/comment-publish-plan.md",
            )
            .to_path_buf(),
            status: "advisory".to_string(),
            mode: "plan".to_string(),
            publishable: 1,
            skipped: 0,
            blocked: 0,
            safe_to_publish: false,
            operations: vec!["create".to_string()],
            skip_reasons: Vec::new(),
            blocked_reasons: Vec::new(),
            expected_status: "advisory".to_string(),
            expected_mode: "plan".to_string(),
            expected_publishable: 1,
            expected_skipped: 0,
            expected_blocked: 0,
            expected_safe_to_publish: false,
            expected_operations: vec!["create".to_string()],
            expected_skip_reasons: Vec::new(),
            expected_blocked_reasons: Vec::new(),
            reason: "publishable plan receipt".to_string(),
            expected_report: Path::new(
                "fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line/comment-publish-plan.json",
            )
            .to_path_buf(),
            expected_markdown: Path::new(
                "fixtures/boundary_gap/expected/pr-inline-comment-publisher/publishable-changed-line/comment-publish-plan.md",
            )
            .to_path_buf(),
            errors: Vec::new(),
        };
    let generated_ci_runs = [generated_ci_run];
    let language_preview_runs = [language_preview_run];
    let editor_gap_cockpit_runs = [editor_gap_cockpit_run];
    let editor_first_pr_bridge_runs = [editor_first_pr_bridge_run];
    let finding_alignment_runs = [finding_alignment_run];
    let surface_projection_alignment_runs = [surface_projection_alignment_run];
    let real_repair_attempt_runs = [real_repair_attempt_run];
    let python_real_repo_eval_runs = [python_real_repo_eval_run];
    let python_static_limit_eval_runs = [
        dogfood_python_static_limit_eval_run(&valid_python_static_limit_eval_scenario()),
        dogfood_python_static_limit_eval_run(
            &valid_python_unresolved_fixture_static_limit_eval_scenario(),
        ),
    ];
    let python_no_action_eval_runs = [
        dogfood_python_no_action_eval_run(&valid_python_no_action_eval_scenario()),
        dogfood_python_no_action_eval_run(&valid_python_already_observed_no_action_eval_scenario()),
        dogfood_python_no_action_eval_run(&valid_python_heuristic_no_action_eval_scenario()),
    ];
    let typescript_preview_repair_loop_runs = [typescript_preview_repair_loop_run];
    let bun_ub_cross_language_runs = [bun_ub_cross_language_run];
    let user_surface_projection_runs = [user_surface_projection_run];
    let preview_projection_runs = DogfoodPreviewProjectionRuns {
        generated_ci_cockpit: &generated_ci_runs,
        language_preview: &language_preview_runs,
        editor_gap_cockpit: &editor_gap_cockpit_runs,
        editor_first_pr_bridge: &editor_first_pr_bridge_runs,
    };
    let markdown_runs = [run];
    let markdown_gate_runs = [gate_run];
    let markdown_first_action_runs = [first_action_run];
    let markdown_first_pr_runs = [first_pr_run];
    let markdown_front_panel_runs = [front_panel_run];
    let markdown_report_packet_index_runs = [report_packet_index_run];
    let markdown_pr_inline_comment_runs = [pr_inline_comment_run];
    let markdown_inputs = DogfoodReportInputs {
        runs: &markdown_runs,
        gate_runs: &markdown_gate_runs,
        first_action_runs: &markdown_first_action_runs,
        first_pr_runs: &markdown_first_pr_runs,
        front_panel_runs: &markdown_front_panel_runs,
        report_packet_index_runs: &markdown_report_packet_index_runs,
        preview_projection_runs: &preview_projection_runs,
        finding_alignment_runs: &finding_alignment_runs,
        surface_projection_alignment_runs: &surface_projection_alignment_runs,
        real_repair_attempt_runs: &real_repair_attempt_runs,
        python_real_repo_eval_runs: &python_real_repo_eval_runs,
        python_static_limit_eval_runs: &python_static_limit_eval_runs,
        python_no_action_eval_runs: &python_no_action_eval_runs,
        typescript_preview_repair_loop_runs: &typescript_preview_repair_loop_runs,
        bun_ub_cross_language_runs: &bun_ub_cross_language_runs,
        user_surface_projection_runs: &user_surface_projection_runs,
        pr_inline_comment_runs: &markdown_pr_inline_comment_runs,
    };
    let empty_runs = [];
    let empty_gate_runs = [];
    let empty_first_action_runs = [];
    let empty_front_panel_runs = [];
    let empty_report_packet_index_runs = [];
    let empty_pr_inline_comment_runs = [];
    let json_inputs = DogfoodReportInputs {
        runs: &empty_runs,
        gate_runs: &empty_gate_runs,
        first_action_runs: &empty_first_action_runs,
        first_pr_runs: &markdown_first_pr_runs,
        front_panel_runs: &empty_front_panel_runs,
        report_packet_index_runs: &empty_report_packet_index_runs,
        preview_projection_runs: &preview_projection_runs,
        finding_alignment_runs: &finding_alignment_runs,
        surface_projection_alignment_runs: &surface_projection_alignment_runs,
        real_repair_attempt_runs: &real_repair_attempt_runs,
        python_real_repo_eval_runs: &python_real_repo_eval_runs,
        python_static_limit_eval_runs: &python_static_limit_eval_runs,
        python_no_action_eval_runs: &python_no_action_eval_runs,
        typescript_preview_repair_loop_runs: &typescript_preview_repair_loop_runs,
        bun_ub_cross_language_runs: &bun_ub_cross_language_runs,
        user_surface_projection_runs: &user_surface_projection_runs,
        pr_inline_comment_runs: &empty_pr_inline_comment_runs,
    };
    let markdown = dogfood_report_markdown(&markdown_inputs);
    let json = dogfood_report_json(&json_inputs);
    assert!(dogfood_failed_families(&json_inputs).is_empty());
    assert_eq!(dogfood_report_status(&json_inputs), "pass");
    // A failing family the #2411 exit list used to skip must still fail the
    // command, and through the same owner as the report status (#4309).
    let failing_generated_ci_runs = [dogfood_generated_ci_cockpit_run_from_surfaces(
        "generated-pr-ci-review-workflow",
        "cargo run --quiet -p ripr -- init --ci github --dry-run",
        10,
        "name: RIPR",
        "",
    )];
    let failing_preview_projection_runs = DogfoodPreviewProjectionRuns {
        generated_ci_cockpit: &failing_generated_ci_runs,
        ..preview_projection_runs
    };
    let failing_inputs = DogfoodReportInputs {
        preview_projection_runs: &failing_preview_projection_runs,
        ..json_inputs
    };
    let failed = dogfood_failed_families(&failing_inputs);
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(
        failed[0].starts_with("generated-pr-ci-review-workflow: "),
        "{failed:?}"
    );
    assert_eq!(dogfood_report_status(&failing_inputs), "warn");
    assert_eq!(dogfood_gate_result(&json_inputs), Ok(()));
    let gate_error = dogfood_gate_result(&failing_inputs)
        .err()
        .unwrap_or_default();
    assert!(
        gate_error.contains("generated-pr-ci-review-workflow: "),
        "{gate_error}"
    );
    // Families that run no producer say so, so they cannot read as producer
    // evidence (#4267).
    let report: serde_json::Value =
        serde_json::from_str(&json).map_err(|err| format!("dogfood JSON: {err}"))?;
    for family in ["first_successful_pr", "editor_gap_cockpit"] {
        assert_eq!(
            report[family]["evidence_source"], "committed_declarations",
            "{family}"
        );
        assert_eq!(report[family]["rendered_cases"], 0, "{family}");
    }
    assert_eq!(
        markdown
            .matches("- Evidence source: committed declarations only;")
            .count(),
        2
    );

    assert!(markdown.contains("Mode: advisory"));
    assert!(markdown.contains("boundary_gap"));
    assert!(markdown.contains("First Useful Action Receipts"));
    assert!(markdown.contains("First Successful PR Receipts"));
    assert!(markdown.contains("PR Review Front Panel Receipts"));
    assert!(markdown.contains("Report Packet Index Receipts"));
    assert!(markdown.contains("Generated CI Cockpit Receipts"));
    assert!(markdown.contains("Language Preview Receipts"));
    assert!(markdown.contains("Editor Gap Cockpit Receipts"));
    assert!(markdown.contains("Editor First-PR Bridge Receipts"));
    assert!(markdown.contains("Finding Alignment Receipts"));
    assert!(markdown.contains("Surface Projection Alignment Receipts"));
    assert!(markdown.contains("Real Repair Attempt Receipts"));
    assert!(markdown.contains("Python Real-Repo Eval Receipts"));
    assert!(markdown.contains("static-limit no-action cases: 2"));
    assert!(markdown.contains("ordinary no-action cases: 3"));
    assert!(markdown.contains("Python Static-Limit Eval Receipts"));
    assert!(markdown.contains("dynamic_dispatch_no_packet_eval"));
    assert!(markdown.contains("unresolved_fixture_no_packet_eval"));
    assert!(markdown.contains("Static-limit no-action cases: 2"));
    assert!(markdown.contains("Python No-Action Eval Receipts"));
    assert!(markdown.contains("no_related_test_no_packet_eval"));
    assert!(markdown.contains("already_observed_no_packet_eval"));
    assert!(markdown.contains("heuristic_only_no_packet_eval"));
    assert!(markdown.contains("Ordinary no-action cases: 3"));
    assert!(markdown.contains("no_related_test"));
    assert!(markdown.contains("already_observed"));
    assert!(markdown.contains("heuristic_only"));
    assert!(markdown.contains("unresolved_pytest_fixture"));
    assert!(markdown.contains("Python Repair-Routing Quality Metrics"));
    assert!(markdown.contains("Top-3 actionable precision: 3 / 3 ranked findings"));
    assert!(markdown.contains("Full top-3 capture cases: 1 / 1 evals"));
    assert!(markdown.contains("TypeScript Preview Repair-Loop Receipts"));
    assert!(markdown.contains("TypeScript False-Actionable Audit"));
    assert!(markdown.contains("False actionable: 0 / 15 checked rows"));
    assert!(markdown.contains("Bun UB Cross-Language Witness Receipts"));
    assert!(markdown.contains("bun_blob_31648_known_good"));
    assert!(markdown.contains("User Surface Projection Alignment Receipts"));
    assert!(markdown.contains("PR Inline Comment Publisher Receipts"));
    assert!(markdown.contains("Gate Adoption Receipts"));
    assert!(markdown.contains("Default CI blocking: no"));
    assert!(markdown.contains("first_run_packets_total"));
    assert!(markdown.contains("first_run_top_gap_selected_total"));
    assert!(markdown.contains("actionable"));
    assert!(markdown.contains("complete_packet"));
    assert!(markdown.contains("publishable_changed_line"));
    assert!(markdown.contains("calibrated-high-confidence-new-gap"));
    assert!(json.contains("\"advisory\": true"));
    assert!(json.contains("\"default_ci_blocking\": false"));
    assert!(json.contains("\"first_successful_pr\""));
    assert!(json.contains("\"first_run_packets_total\": 1"));
    assert!(json.contains("\"first_run_top_gap_selected_total\": 1"));
    assert!(json.contains("\"report_packet_index\""));
    assert!(json.contains("\"generated_ci_cockpit\""));
    assert!(json.contains("\"language_preview\""));
    assert!(json.contains("\"editor_first_pr_bridge\""));
    assert!(json.contains("\"finding_alignment\""));
    assert!(json.contains("\"surface_projection_alignment\""));
    assert!(json.contains("\"real_repair_attempts\""));
    assert!(json.contains("\"python_real_repo_evals\""));
    assert!(json.contains("\"static_limit_cases\": 2"));
    assert!(json.contains("\"static_limit_no_action_cases\": 2"));
    assert!(json.contains("\"static_limit_no_action_distribution\""));
    assert!(json.contains("\"dynamic_dispatch_no_packet_eval\""));
    assert!(json.contains("\"unresolved_fixture_no_packet_eval\""));
    assert!(json.contains("\"agent_packet_present\": false"));
    assert!(json.contains("\"gap_movement\": \"no_receipt\""));
    assert!(json.contains("\"typescript_preview_repair_loop\""));
    assert!(json.contains("\"typescript_false_actionable_audit\""));
    assert!(json.contains("\"bun_ub_cross_language_witnesses\""));
    assert!(json.contains("\"repair_route_quality_metrics_improved\""));
    assert!(json.contains("\"tiny_controlled_pytest_boundary_receipt\""));
    assert!(json.contains("\"typescript_boundary_predicate_proof\""));
    assert!(json.contains("\"user_surface_projection_alignment\""));
    assert!(json.contains("\"badge_actionable_count_from_canonical_state\""));
    let value: Value =
        serde_json::from_str(&json).map_err(|err| format!("dogfood JSON invalid: {err}"))?;
    let editor_gap_cockpit = value
        .get("editor_gap_cockpit")
        .ok_or_else(|| "editor_gap_cockpit section missing".to_string())?;
    assert_eq!(
        editor_gap_cockpit
            .get("receipt_dir")
            .and_then(Value::as_str),
        Some("fixtures/editor_gap_cockpit")
    );
    let cases = editor_gap_cockpit
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "editor_gap_cockpit cases missing".to_string())?;
    assert_eq!(cases.len(), 1);
    let case = cases
        .first()
        .ok_or_else(|| "editor_gap_cockpit case missing".to_string())?;
    assert_eq!(
        case.get("name").and_then(Value::as_str),
        Some("typescript_preview_static_limit")
    );
    assert_eq!(
        case.get("state").and_then(Value::as_str),
        Some("actionable")
    );
    assert_eq!(
        case.get("language").and_then(Value::as_str),
        Some("typescript")
    );
    assert_eq!(
        case.get("language_status").and_then(Value::as_str),
        Some("preview")
    );
    assert_eq!(
        case.get("diagnostics_projected").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        case.get("static_limit_kind").and_then(Value::as_str),
        Some("mocked_module")
    );
    assert_eq!(
        case.get("hover_static_before_action")
            .and_then(Value::as_bool),
        Some(true)
    );
    let actions = case
        .get("actions_projected")
        .and_then(Value::as_array)
        .ok_or_else(|| "editor_gap_cockpit actions missing".to_string())?
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        vec!["copy_repair_packet", "copy_static_limit_note", "refresh"]
    );
    let editor_first_pr_bridge = value
        .get("editor_first_pr_bridge")
        .ok_or_else(|| "editor_first_pr_bridge section missing".to_string())?;
    assert_eq!(
        editor_first_pr_bridge
            .get("receipt_dir")
            .and_then(Value::as_str),
        Some("fixtures/editor_first_pr_bridge")
    );
    let bridge_cases = editor_first_pr_bridge
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "editor_first_pr_bridge cases missing".to_string())?;
    assert_eq!(bridge_cases.len(), 1);
    let bridge_case = bridge_cases
        .first()
        .ok_or_else(|| "editor_first_pr_bridge case missing".to_string())?;
    assert_eq!(
        bridge_case.get("name").and_then(Value::as_str),
        Some("packet_found_repairable")
    );
    assert_eq!(
        bridge_case.get("packet_state").and_then(Value::as_str),
        Some("top_repairable_gap")
    );
    assert_eq!(
        bridge_case.get("diagnostics").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        bridge_case
            .get("runtime_adequacy_claim")
            .and_then(Value::as_bool),
        Some(false)
    );
    let finding_alignment = value
        .get("finding_alignment")
        .ok_or_else(|| "finding_alignment section missing".to_string())?;
    assert_eq!(
        finding_alignment.get("receipt_dir").and_then(Value::as_str),
        Some("fixtures/finding-alignment-dogfood")
    );
    let alignment_cases = finding_alignment
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "finding_alignment cases missing".to_string())?;
    assert_eq!(alignment_cases.len(), 1);
    assert_eq!(
        alignment_cases
            .first()
            .and_then(|case| case.get("user_outcome"))
            .and_then(Value::as_str),
        Some("actionable_gap")
    );
    let surface_projection = value
        .get("surface_projection_alignment")
        .ok_or_else(|| "surface_projection_alignment section missing".to_string())?;
    assert_eq!(
        surface_projection
            .get("receipt_dir")
            .and_then(Value::as_str),
        Some("fixtures/surface-projection-alignment")
    );
    let surface_cases = surface_projection
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "surface_projection_alignment cases missing".to_string())?;
    assert_eq!(surface_cases.len(), 1);
    let surface_case = surface_cases
        .first()
        .ok_or_else(|| "surface_projection_alignment case missing".to_string())?;
    assert_eq!(
        surface_case
            .get("top_next_action_kind")
            .and_then(Value::as_str),
        Some("attempt_ready_packet")
    );
    assert_eq!(
        surface_case.get("outcome").and_then(Value::as_str),
        Some("evidence_improved")
    );
    let real_attempts = value
        .get("real_repair_attempts")
        .ok_or_else(|| "real_repair_attempts section missing".to_string())?;
    assert_eq!(
        real_attempts.get("receipt_dir").and_then(Value::as_str),
        Some("fixtures/real-repair-attempts")
    );
    let real_attempt_summary = real_attempts
        .get("summary")
        .ok_or_else(|| "real_repair_attempts summary missing".to_string())?;
    assert_eq!(
        real_attempt_summary
            .get("attempted")
            .and_then(Value::as_u64),
        Some(1)
    );
    let real_attempt_cases = real_attempts
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "real_repair_attempts cases missing".to_string())?;
    assert_eq!(real_attempt_cases.len(), 1);
    let real_attempt_case = real_attempt_cases
        .first()
        .ok_or_else(|| "real_repair_attempts case missing".to_string())?;
    assert_eq!(
        real_attempt_case.get("outcome").and_then(Value::as_str),
        Some("evidence_improved")
    );
    let python_evals = value
        .get("python_real_repo_evals")
        .ok_or_else(|| "python_real_repo_evals section missing".to_string())?;
    assert_eq!(
        python_evals.get("receipt_dir").and_then(Value::as_str),
        Some("fixtures/python-real-repo-evals")
    );
    let python_eval_summary = python_evals
        .get("summary")
        .ok_or_else(|| "python_real_repo_evals summary missing".to_string())?;
    assert_eq!(
        python_eval_summary.get("closed").and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        python_eval_summary
            .get("no_action_cases")
            .and_then(Value::as_u64),
        Some(3)
    );
    let python_eval_cases = python_evals
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "python_real_repo_evals cases missing".to_string())?;
    assert_eq!(python_eval_cases.len(), 1);
    assert_eq!(
        python_eval_cases[0]
            .get("gap_movement")
            .and_then(Value::as_str),
        Some("closed")
    );
    assert_eq!(
        python_eval_cases[0]
            .get("missing_discriminator")
            .and_then(Value::as_str),
        Some("amount == threshold")
    );
    let python_no_action_cases = python_evals
        .get("no_action_cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "python_real_repo_evals no_action_cases missing".to_string())?;
    assert_eq!(python_no_action_cases.len(), 3);
    assert!(
        python_no_action_cases.iter().any(|case| {
            case.get("no_action_kind").and_then(Value::as_str) == Some("no_related_test")
                && case.get("agent_packet_present").and_then(Value::as_bool) == Some(false)
        }),
        "no-related-test no-action case should stay packet-free"
    );
    assert!(
        python_no_action_cases.iter().any(|case| {
            case.get("no_action_kind").and_then(Value::as_str) == Some("already_observed")
                && case.get("classification").and_then(Value::as_str) == Some("exposed")
                && case.get("agent_packet_present").and_then(Value::as_bool) == Some(false)
        }),
        "already-observed no-action case should stay exposed but packet-free"
    );
    assert!(
        python_no_action_cases.iter().any(|case| {
            case.get("no_action_kind").and_then(Value::as_str) == Some("heuristic_only")
                && case.get("classification").and_then(Value::as_str) == Some("weakly_exposed")
                && case.get("agent_packet_present").and_then(Value::as_bool) == Some(false)
        }),
        "heuristic-only no-action case should stay weakly exposed but packet-free"
    );
    let python_quality = value
        .get("python_repair_routing_quality")
        .ok_or_else(|| "python_repair_routing_quality section missing".to_string())?;
    assert_eq!(
        python_quality["quality_gate"]["status"],
        serde_json::Value::from("pass")
    );
    assert_eq!(
        python_quality["summary"]["top_1_actionable_precision"]["status"],
        serde_json::Value::from("pass")
    );
    assert_eq!(
        python_quality["summary"]["top_3_actionable_precision"]["status"],
        serde_json::Value::from("pass")
    );
    assert_eq!(
        python_quality["summary"]["top_3_actionable_precision"]["count"],
        serde_json::Value::from(3)
    );
    assert_eq!(
        python_quality["summary"]["full_top_3_capture_cases"]["status"],
        serde_json::Value::from("pass")
    );
    assert_eq!(
        python_quality["summary"]["full_top_3_capture_cases"]["count"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        python_quality["summary"]["ranked_top_3_cases_with_capture"]["status"],
        serde_json::Value::from("pass")
    );
    assert_eq!(
        python_quality["summary"]["receipt_closure_rate"]["count"],
        serde_json::Value::from(1)
    );
    assert_eq!(
        python_quality["summary"]["ordinary_no_action_cases"],
        serde_json::Value::from(3)
    );
    assert!(
        python_quality["ordinary_no_action_distribution"]
            .as_array()
            .ok_or_else(|| "ordinary_no_action_distribution missing".to_string())?
            .iter()
            .any(|entry| entry["kind"].as_str() == Some("no_related_test")
                && entry["cases"].as_u64() == Some(1))
    );
    assert!(
        python_quality["ordinary_no_action_distribution"]
            .as_array()
            .ok_or_else(|| "ordinary_no_action_distribution missing".to_string())?
            .iter()
            .any(|entry| entry["kind"].as_str() == Some("already_observed")
                && entry["cases"].as_u64() == Some(1))
    );
    assert!(
        python_quality["ordinary_no_action_distribution"]
            .as_array()
            .ok_or_else(|| "ordinary_no_action_distribution missing".to_string())?
            .iter()
            .any(|entry| entry["kind"].as_str() == Some("heuristic_only")
                && entry["cases"].as_u64() == Some(1))
    );
    let typescript_preview_repair_loop = value
        .get("typescript_preview_repair_loop")
        .ok_or_else(|| "typescript_preview_repair_loop section missing".to_string())?;
    assert_eq!(
        typescript_preview_repair_loop
            .get("receipt_dir")
            .and_then(Value::as_str),
        Some("fixtures/typescript-preview-repair-loop")
    );
    let typescript_preview_repair_loop_summary = typescript_preview_repair_loop
        .get("summary")
        .ok_or_else(|| "typescript_preview_repair_loop summary missing".to_string())?;
    assert_eq!(
        typescript_preview_repair_loop_summary
            .get("repair_packet_ready")
            .and_then(Value::as_u64),
        Some(0)
    );
    let typescript_preview_repair_loop_cases = typescript_preview_repair_loop
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "typescript_preview_repair_loop cases missing".to_string())?;
    assert_eq!(typescript_preview_repair_loop_cases.len(), 1);
    assert_eq!(
        typescript_preview_repair_loop_cases[0]
            .get("outcome")
            .and_then(Value::as_str),
        Some("proof_improved")
    );
    let typescript_false_actionable_audit = value
        .get("typescript_false_actionable_audit")
        .ok_or_else(|| "typescript_false_actionable_audit section missing".to_string())?;
    assert_eq!(
        typescript_false_actionable_audit["quality_gate"]["status"],
        serde_json::Value::from("pass")
    );
    let typescript_false_actionable_summary = typescript_false_actionable_audit
        .get("summary")
        .ok_or_else(|| "typescript_false_actionable_audit summary missing".to_string())?;
    assert_eq!(
        typescript_false_actionable_summary
            .get("cases")
            .and_then(Value::as_u64),
        Some(15)
    );
    assert_eq!(
        typescript_false_actionable_summary
            .get("false_actionable_rate")
            .and_then(|value| value.get("count"))
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        typescript_false_actionable_summary
            .get("false_actionable_rate")
            .and_then(|value| value.get("checked"))
            .and_then(Value::as_u64),
        Some(15)
    );
    assert_eq!(
        typescript_false_actionable_summary
            .get("repair_packet_ready_violation_rate")
            .and_then(|value| value.get("count"))
            .and_then(Value::as_u64),
        Some(0)
    );
    let typescript_false_actionable_cases = typescript_false_actionable_audit
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "typescript_false_actionable_audit cases missing".to_string())?;
    assert_eq!(typescript_false_actionable_cases.len(), 15);
    assert!(
        typescript_false_actionable_cases
            .iter()
            .all(|case| { case.get("false_actionable").and_then(Value::as_bool) == Some(false) })
    );
    let bun_ub_cross_language = value
        .get("bun_ub_cross_language_witnesses")
        .ok_or_else(|| "bun_ub_cross_language_witnesses section missing".to_string())?;
    assert_eq!(
        bun_ub_cross_language
            .get("receipt_dir")
            .and_then(Value::as_str),
        Some("fixtures/bun-ub-cross-language-dogfood")
    );
    let bun_ub_cross_language_summary = bun_ub_cross_language
        .get("summary")
        .ok_or_else(|| "bun_ub_cross_language_witnesses summary missing".to_string())?;
    assert_eq!(
        bun_ub_cross_language_summary
            .get("repair_packet_ready")
            .and_then(Value::as_u64),
        Some(0)
    );
    let bun_ub_cross_language_cases = bun_ub_cross_language
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "bun_ub_cross_language_witnesses cases missing".to_string())?;
    assert_eq!(bun_ub_cross_language_cases.len(), 1);
    assert_eq!(
        bun_ub_cross_language_cases[0]
            .get("operator_action")
            .and_then(Value::as_str),
        Some("no_missing_bridge_discriminator")
    );
    assert!(json.contains("\"pr_inline_comment_publisher\""));
    Ok(())
}

#[test]
fn dogfood_finding_alignment_scenarios_have_checked_receipts() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_finding_alignment_scenarios();
        for required in [
            ("presentation_text_actionable_output_observer", "actionable"),
            (
                "presentation_text_already_observed_output",
                "already_observed",
            ),
            ("config_policy_internal_metadata_no_action", "internal_only"),
            ("config_policy_rendered_label_unobserved", "actionable"),
            ("config_policy_behavior_selector_unobserved", "actionable"),
            (
                "config_policy_behavior_selector_observed",
                "already_observed",
            ),
            ("config_policy_flow_unknown_limitation", "static_limitation"),
            (
                "config_policy_opaque_lookup_limitation",
                "static_limitation",
            ),
            (
                "config_policy_opaque_lookup_supported_observer",
                "actionable",
            ),
            ("predicate_boundary_actionable_scorecard_lead", "actionable"),
            (
                "call_presence_runtime_static_only_trend",
                "static_limitation",
            ),
        ] {
            assert!(
                scenarios.iter().any(|scenario| {
                    scenario.name == required.0 && scenario.gap_state == required.1
                }),
                "{} finding-alignment dogfood receipt should be checked as {}",
                required.0,
                required.1
            );
        }

        for scenario in scenarios {
            let run = dogfood_finding_alignment_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} finding-alignment receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_finding_alignment_validation_reports_actionable_drift() {
    let mut scenario = valid_finding_alignment_scenario("", "actionable");
    scenario.source_pr = "ripr#1016".to_string();
    scenario.evidence_class = "unknown".to_string();
    scenario.raw_findings_total = 0;
    scenario.canonical_items_total = 1;
    scenario.canonical_gap_id = "unknown".to_string();
    scenario.raw_finding_summary = "missing raw finding summary".to_string();
    scenario.user_outcome = "no_action".to_string();
    scenario.repair_kind = "unknown".to_string();
    scenario.target_test_type = "none".to_string();
    scenario.verify_command = "unknown".to_string();
    scenario.static_limitation_category = Some("presentation_text_visibility_unknown".to_string());
    scenario.raw_findings_supporting_only = false;
    scenario.recommended_repair = "Escalate to mutation testing first.".to_string();
    scenario.before_after_context = "missing before/after context".to_string();
    scenario.must_not_claim = vec!["Keep advisory.".to_string()];

    let report = dogfood_finding_alignment_run(&scenario).errors.join("\n");

    assert!(report.contains("case id must be present"));
    assert!(report.contains("source_pr should name a real RIPR PR"));
    assert!(report.contains("canonical_gap_id must be present"));
    assert!(report.contains("unsupported evidence class"));
    assert!(report.contains("raw findings 0 must be >= canonical items 1"));
    assert!(report.contains("raw findings must be marked supporting-only"));
    assert!(report.contains("raw_finding_summary must be present"));
    assert!(report.contains("must not route first to mutation testing"));
    assert!(report.contains("before_after_context must be present"));
    assert!(report.contains("must_not_claim guards should preserve"));
    assert!(report.contains("actionable case should have actionable_gap outcome"));
    assert!(report.contains("actionable case must carry a concrete repair kind"));
    assert!(report.contains("actionable case must carry a target test type"));
    assert!(report.contains("actionable case must carry a verify command"));
    assert!(report.contains("actionable case should not carry a static limitation category"));
}

#[test]
fn dogfood_finding_alignment_validation_reports_no_action_and_limitation_drift() {
    let mut observed = valid_finding_alignment_scenario("observed", "already_observed");
    observed.user_outcome = "actionable_gap".to_string();
    observed.repair_kind = "output_observer".to_string();
    let observed_report = dogfood_finding_alignment_run(&observed).errors.join("\n");
    assert!(observed_report.contains("already_observed case should have no_action outcome"));
    assert!(observed_report.contains("already_observed case should use no_action repair kind"));

    let mut internal = valid_finding_alignment_scenario("internal", "internal_only");
    internal.user_outcome = "actionable_gap".to_string();
    internal.repair_kind = "inspect_visibility".to_string();
    let internal_report = dogfood_finding_alignment_run(&internal).errors.join("\n");
    assert!(internal_report.contains("internal_only case should have no_action outcome"));
    assert!(internal_report.contains("internal_only case should use no_action repair kind"));

    let mut limitation = valid_finding_alignment_scenario("limitation", "static_limitation");
    limitation.user_outcome = "no_action".to_string();
    limitation.static_limitation_category = None;
    limitation.static_limitation_repair_route = None;
    let limitation_report = dogfood_finding_alignment_run(&limitation).errors.join("\n");
    assert!(
        limitation_report.contains("static_limitation case should have static_limitation outcome")
    );
    assert!(limitation_report.contains("static limitation case must name a limitation category"));
    assert!(
        limitation_report.contains("static limitation case must name a limitation repair route")
    );

    let unknown = valid_finding_alignment_scenario("unknown", "mystery");
    assert!(
        dogfood_finding_alignment_run(&unknown)
            .errors
            .join("\n")
            .contains("unsupported gap_state `mystery`")
    );
}

#[test]
fn dogfood_surface_projection_alignment_receipts_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_surface_projection_alignment_scenarios();
        assert!(
            scenarios.iter().any(|scenario| {
                scenario.name == "receipt_improved_top_next_action_alignment"
                    && scenario.outcome == "evidence_improved"
            }),
            "surface projection alignment dogfood receipt should pin improved receipt case"
        );

        for scenario in scenarios {
            let run = dogfood_surface_projection_alignment_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} surface projection alignment receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_surface_projection_alignment_reports_identity_drift() {
    let mut scenario = valid_surface_projection_alignment_scenario();
    scenario.packet_id = "wrong-packet".to_string();
    scenario.advisory_consumers = vec!["badge".to_string()];

    let report = dogfood_surface_projection_alignment_run(&scenario)
        .errors
        .join("\n");

    assert!(report.contains("attempt ledger packet_id must be wrong-packet"));
    assert!(report.contains("readiness top_next_action.packet_id must be wrong-packet"));
    assert!(report.contains("advisory_consumers must include lsp"));
    assert!(report.contains("advisory_consumers must include pr_comment"));
    assert!(report.contains("advisory_consumers must include ci"));
}

#[test]
fn dogfood_surface_projection_alignment_rejects_missing_limitation_backlog_packet()
-> Result<(), String> {
    with_repo_cwd(|| {
        let mut scenario = dogfood_surface_projection_alignment_scenarios()
            .into_iter()
            .find(|scenario| scenario.name == "static_limitation_backlog_alignment")
            .ok_or("surface projection corpus must include static limitation backlog case")?;
        scenario.swarm_plan["static_limitation_backlog"]["limitation_backlog_packets"] =
            serde_json::json!([]);

        let report = dogfood_surface_projection_alignment_run(&scenario)
            .errors
            .join("\n");

        assert!(
            report.contains("static limitation backlog must include limitation_backlog_packets")
        );
        assert!(report.contains(
            "readiness static_limitation_backlog must preserve limitation_backlog_packets"
        ));
        Ok(())
    })
}

#[test]
fn dogfood_surface_projection_alignment_rejects_incomplete_limitation_backlog_packet()
-> Result<(), String> {
    with_repo_cwd(|| {
        let mut scenario = dogfood_surface_projection_alignment_scenarios()
            .into_iter()
            .find(|scenario| scenario.name == "static_limitation_backlog_alignment")
            .ok_or("surface projection corpus must include static limitation backlog case")?;
        scenario.swarm_plan["static_limitation_backlog"]["limitation_backlog_packets"] = serde_json::json!([
            {
                "packet_id": scenario.packet_id,
                "canonical_gap_id": scenario.canonical_gap_id,
                "limitation_category": "activation_owner_call_absent",
                "repair_route": "analysis/related-test-affinity-owner-call-tracing",
                "signal_count": 1,
                "public_projection_eligible": true,
                "swarm_ready": true
            }
        ]);

        let report = dogfood_surface_projection_alignment_run(&scenario)
            .errors
            .join("\n");

        assert!(report.contains("limitation backlog packet must include sample_canonical_gap_ids"));
        assert!(report.contains("limitation backlog packet must include sample_sources"));
        assert!(report.contains(
            "limitation backlog packet must preserve non_claims denying public repair status"
        ));
        assert!(
            report.contains("limitation backlog packet must not expose public_projection_eligible")
        );
        assert!(report.contains("limitation backlog packet must not expose swarm_ready"));
        assert!(report.contains(
            "readiness limitation backlog packet must preserve sample_canonical_gap_ids"
        ));
        assert!(
            report.contains("readiness limitation backlog packet must preserve sample_sources")
        );
        assert!(report.contains("readiness limitation backlog packet must preserve non_claims"));
        assert!(report.contains(
            "readiness limitation backlog packet must not expose public_projection_eligible"
        ));
        assert!(report.contains("readiness limitation backlog packet must not expose swarm_ready"));
        assert!(report.contains("top_limitation_routes must preserve sample_canonical_gap_ids"));
        assert!(report.contains("top_limitation_routes must preserve sample_sources"));
        assert!(report.contains(
            "top_limitation_routes must preserve non_claims denying public repair status"
        ));
        Ok(())
    })
}

#[test]
fn dogfood_real_repair_attempt_receipts_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_real_repair_attempt_scenarios();
        for required in REAL_REPAIR_ATTEMPTS_REQUIRED_CASES {
            assert!(
                scenarios.iter().any(|scenario| {
                    scenario.name == required.0 && scenario.outcome == required.1
                }),
                "{} real repair attempt should be checked as {}",
                required.0,
                required.1
            );
        }
        assert!(
            scenarios.len() >= 3,
            "real repair attempts should include multiple attempts"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.outcome == "evidence_unchanged"),
            "real repair attempts should include an unchanged non-success"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.outcome == "attempted_no_receipt"),
            "real repair attempts should include a named missing-receipt case"
        );

        for scenario in scenarios {
            let run = dogfood_real_repair_attempt_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} real repair attempt receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_real_repair_attempt_reports_contract_drift() {
    let mut scenario = valid_real_repair_attempt_scenario();
    scenario.name.clear();
    scenario.source_ref = "unknown".to_string();
    scenario.canonical_gap_id = "repair-route-quality-metrics-missing".to_string();
    scenario.verify_result = "maybe".to_string();
    scenario.receipt_command = scenario.verify_command.clone();
    scenario.receipt_path = None;
    scenario.outcome = "evidence_improved".to_string();
    scenario.must_not_change.clear();
    scenario.raw_evidence_refs.clear();

    let report = dogfood_real_repair_attempt_run(&scenario).errors.join("\n");

    assert!(report.contains("case id must be present"));
    assert!(report.contains("source_ref must name the PR"));
    assert!(report.contains("canonical_gap_id must use gap: identity"));
    assert!(report.contains("verify_result must be pass"));
    assert!(report.contains("receipt_path must be present"));
    assert!(report.contains("receipt_command must stay distinct"));
    assert!(report.contains("must_not_change must name bounded edit constraints"));
    assert!(report.contains("raw_evidence_refs must keep lineage"));

    let mut missing_receipt = valid_real_repair_attempt_scenario();
    missing_receipt.outcome = "attempted_no_receipt".to_string();
    missing_receipt.receipt_path = None;
    missing_receipt.missing_receipt_reason = None;

    let missing_report = dogfood_real_repair_attempt_run(&missing_receipt)
        .errors
        .join("\n");
    assert!(missing_report.contains("attempted_no_receipt must include missing_receipt_reason"));
}

#[test]
fn dogfood_real_repair_attempt_rejects_movement_contradictions() {
    let mut mismatched_receipt = valid_real_repair_attempt_scenario();
    mismatched_receipt.outcome = "evidence_improved".to_string();
    mismatched_receipt.receipt_state = "receipt_movement_regressed".to_string();
    mismatched_receipt.evidence_movement =
        "evidence_regressed despite an improved outcome".to_string();

    let report = dogfood_real_repair_attempt_run(&mismatched_receipt)
        .errors
        .join("\n");
    assert!(report.contains(
        "receipt_state for evidence_improved must not claim conflicting movement terms: regressed"
    ));
    assert!(report.contains(
            "evidence_movement for evidence_improved must not claim conflicting movement tokens: evidence_regressed"
        ));

    let mut missing_receipt_claim = valid_real_repair_attempt_scenario();
    missing_receipt_claim.outcome = "attempted_no_receipt".to_string();
    missing_receipt_claim.receipt_path = None;
    missing_receipt_claim.receipt_state = "receipt_missing".to_string();
    missing_receipt_claim.missing_receipt_reason =
        Some("operator stopped before writing a receipt".to_string());
    missing_receipt_claim.evidence_movement =
        "evidence_improved without receipt evidence".to_string();

    let missing_report = dogfood_real_repair_attempt_run(&missing_receipt_claim)
        .errors
        .join("\n");
    assert!(missing_report.contains(
            "evidence_movement for attempted_no_receipt must not claim conflicting movement tokens: evidence_improved"
        ));
}

#[test]
fn dogfood_python_real_repo_eval_receipts_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_python_real_repo_eval_scenarios();
        for required in PYTHON_REAL_REPO_EVAL_REQUIRED_CASES {
            assert!(
                scenarios.iter().any(|scenario| {
                    scenario.name == required.0 && scenario.gap_movement == required.1
                }),
                "{} Python real-repo eval receipt should be checked as {}",
                required.0,
                required.1
            );
        }
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.repo_shape == "tiny_controlled_pytest"),
            "Python real-repo eval receipts should include the tiny controlled pytest case"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.gap_movement == "closed"),
            "Python real-repo eval receipts should include a closed gap"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.ranked_top_3_findings.len() == 3),
            "Python real-repo eval receipts should include a full top-3 repair-card capture"
        );

        let runs = scenarios
            .iter()
            .map(dogfood_python_real_repo_eval_run)
            .collect::<Vec<_>>();
        let quality = dogfood_python_repair_routing_quality_summary(&runs);
        assert_eq!(quality.gate_status, "pass");
        assert_eq!(quality.top_1_actionable_usable, quality.cases);
        assert_eq!(quality.top_3_cases_with_ranked_capture, quality.cases);
        assert_eq!(
            quality.top_3_actionable_usable,
            quality.top_3_ranked_findings_checked
        );
        assert_eq!(quality.verify_command_valid, quality.cases);
        assert_eq!(quality.agent_packet_bounded, quality.cases);
        assert_eq!(quality.concrete_discriminator, quality.cases);
        assert_eq!(quality.suggested_test_location, quality.cases);
        assert_eq!(quality.false_actionable, 0);
        assert_eq!(quality.crashes, 0);
        assert!(quality.receipt_closed > 0);
        assert!(
            quality.full_top_3_capture_cases > 0,
            "Python real-repo eval receipts should include a full top-3 repair-card capture"
        );
        assert!(
            quality
                .unsupported_limitation_distribution
                .iter()
                .any(|(kind, count)| kind == "dynamic_route_registration" && *count == 1)
        );

        let static_limit_scenarios = dogfood_python_static_limit_eval_scenarios();
        for required in PYTHON_REAL_REPO_EVAL_REQUIRED_STATIC_LIMIT_CASES {
            assert!(
                static_limit_scenarios.iter().any(|scenario| {
                    scenario.name == required.0 && scenario.static_limit_kind == required.1
                }),
                "{} Python static-limit eval should be checked as {}",
                required.0,
                required.1
            );
        }
        assert!(
            static_limit_scenarios
                .iter()
                .any(|scenario| !scenario.agent_packet_present && !scenario.repair_card_present),
            "Python static-limit evals should record no-packet/no-card cases"
        );
        for scenario in static_limit_scenarios {
            let run = dogfood_python_static_limit_eval_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} Python static-limit eval should validate: {:?}",
                run.name,
                run.errors
            );
        }

        let no_action_scenarios = dogfood_python_no_action_eval_scenarios();
        for required in PYTHON_REAL_REPO_EVAL_REQUIRED_NO_ACTION_CASES {
            assert!(
                no_action_scenarios.iter().any(|scenario| {
                    scenario.name == required.0 && scenario.no_action_kind == required.1
                }),
                "{} Python no-action eval should be checked as {}",
                required.0,
                required.1
            );
        }
        assert!(
            no_action_scenarios
                .iter()
                .any(|scenario| !scenario.agent_packet_present && !scenario.repair_card_present),
            "Python no-action evals should record no-packet/no-card cases"
        );
        for scenario in no_action_scenarios {
            let run = dogfood_python_no_action_eval_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} Python no-action eval should validate: {:?}",
                run.name,
                run.errors
            );
        }

        for scenario in scenarios {
            let run = dogfood_python_real_repo_eval_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} Python real-repo eval receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_python_repair_routing_quality_flags_noisy_routes() {
    let empty_quality = dogfood_python_repair_routing_quality_summary(&[]);
    assert_eq!(empty_quality.gate_status, "review");
    assert_eq!(empty_quality.cases, 0);
    assert_eq!(empty_quality.receipt_closed, 0);

    let mut unusable_card = valid_python_real_repo_eval_scenario();
    unusable_card.usability = "needs_review".to_string();

    let mut bad_verify = valid_python_real_repo_eval_scenario();
    bad_verify.verify_result = "fail".to_string();
    bad_verify.gap_movement = "unchanged".to_string();
    bad_verify.closed_gaps = 0;

    let mut vague_discriminator = valid_python_real_repo_eval_scenario();
    vague_discriminator.missing_discriminator = "uncertain".to_string();

    let mut missing_location = valid_python_real_repo_eval_scenario();
    missing_location.suggested_test_file = "src/pricing.py".to_string();

    let mut false_actionable = valid_python_real_repo_eval_scenario();
    false_actionable.false_positive_notes = "possible false actionability".to_string();

    let mut not_closed = valid_python_real_repo_eval_scenario();
    not_closed.gap_movement = "unchanged".to_string();
    not_closed.closed_gaps = 0;

    let mut unsupported = valid_python_real_repo_eval_scenario();
    unsupported.unsupported_limitations = vec![
        "decorator_indirection".to_string(),
        "dynamic_route_registration".to_string(),
    ];

    let mut crashed = dogfood_python_real_repo_eval_run(&valid_python_real_repo_eval_scenario());
    crashed
        .errors
        .push("simulated parser/report contract error".to_string());

    let runs = vec![
        dogfood_python_real_repo_eval_run(&unusable_card),
        dogfood_python_real_repo_eval_run(&bad_verify),
        dogfood_python_real_repo_eval_run(&vague_discriminator),
        dogfood_python_real_repo_eval_run(&missing_location),
        dogfood_python_real_repo_eval_run(&false_actionable),
        dogfood_python_real_repo_eval_run(&not_closed),
        dogfood_python_real_repo_eval_run(&unsupported),
        crashed,
    ];
    let quality = dogfood_python_repair_routing_quality_summary(&runs);

    assert_eq!(quality.gate_status, "review");
    assert_eq!(quality.cases, 8);
    assert_eq!(quality.top_1_actionable_usable, 6);
    assert_eq!(quality.top_3_cases_with_ranked_capture, 8);
    assert_eq!(quality.full_top_3_capture_cases, 0);
    assert_eq!(quality.top_3_ranked_findings_checked, 8);
    assert_eq!(quality.top_3_actionable_usable, 8);
    assert_eq!(quality.verify_command_valid, 7);
    assert_eq!(quality.agent_packet_bounded, 7);
    assert_eq!(quality.concrete_discriminator, 7);
    assert_eq!(quality.suggested_test_location, 7);
    assert_eq!(quality.false_actionable, 1);
    assert_eq!(quality.crashes, 3);
    assert_eq!(quality.receipt_closed, 6);
    assert!(
        quality
            .unsupported_limitation_distribution
            .iter()
            .any(|(kind, count)| kind == "decorator_indirection" && *count == 1)
    );
    assert!(
        quality
            .unsupported_limitation_distribution
            .iter()
            .any(|(kind, count)| kind == "dynamic_route_registration" && *count == 1)
    );
}

#[test]
fn dogfood_python_quality_ratio_json_marks_review_and_not_measured() {
    let mut body = String::new();
    dogfood_push_python_quality_ratio_json(&mut body, "empty", 0, 0, true, "no cases");
    dogfood_push_python_quality_ratio_json(&mut body, "higher_pass", 2, 2, true, "all good");
    dogfood_push_python_quality_ratio_json(&mut body, "higher_review", 1, 2, true, "missing");
    dogfood_push_python_quality_ratio_json(&mut body, "lower_pass", 0, 2, false, "none");
    dogfood_push_python_quality_ratio_json(&mut body, "lower_review", 1, 2, false, "noisy");

    assert!(body.contains("\"empty\": { \"status\": \"not_measured\""));
    assert!(body.contains("\"higher_pass\": { \"status\": \"pass\""));
    assert!(body.contains("\"higher_review\": { \"status\": \"review\""));
    assert!(body.contains("\"lower_pass\": { \"status\": \"pass\""));
    assert!(body.contains("\"lower_review\": { \"status\": \"review\""));
}

#[test]
fn dogfood_python_static_limit_eval_rejects_actionable_overclaiming() {
    let mut scenario = valid_python_static_limit_eval_scenario();
    scenario.repair_card_present = true;
    scenario.agent_packet_present = true;
    scenario.verify_result = "pass".to_string();
    scenario.receipt_result = "pass".to_string();
    scenario.gap_movement = "closed".to_string();
    scenario.stop_reasons.clear();
    scenario.claim_boundary.clear();

    let report = dogfood_python_static_limit_eval_run(&scenario)
        .errors
        .join("\n");

    assert!(report.contains("static-limit evals must not emit repair cards"));
    assert!(report.contains("static-limit evals must not emit agent packets"));
    assert!(report.contains("verify_result=not_applicable"));
    assert!(report.contains("receipt_result=not_applicable"));
    assert!(report.contains("gap_movement=no_receipt"));
    assert!(report.contains("stop_reasons must include the static-limit stop reason"));
    assert!(report.contains("claim_boundary must include No repair packet emitted"));
}

#[test]
fn dogfood_python_static_limit_eval_accepts_generated_file_exclusion() {
    let mut scenario = valid_python_static_limit_eval_scenario();
    scenario.name = "generated_file_no_packet_eval".to_string();
    scenario.repo_shape = "generated_python_file".to_string();
    scenario.source_ref = "target/ripr/python-real-repo-evals/generated-file-limit".to_string();
    scenario.command = "ripr check --root target/ripr/python-real-repo-evals/generated-file-limit --base HEAD~1 --format json".to_string();
    scenario.runtime_ms = 4513;
    scenario.finding_id = "excluded:src/schema_pb2.py:generated_file".to_string();
    scenario.changed_owner = "python:src/schema_pb2.py::<generated>".to_string();
    scenario.static_limit_kind = "generated_file".to_string();
    scenario.classification = "excluded".to_string();
    scenario.stop_reasons = vec!["generated_file_excluded".to_string()];
    scenario.related_test_file = "tests/test_schema.py".to_string();
    scenario.related_test_name = "test_status_generated_value".to_string();
    scenario.why_not_actionable = "static limit generated_file prevents bounded repair routing because the changed file is detectable generated Python code".to_string();
    scenario.limitation_notes = "generated Python diffs stay out of repair-card, packet, verify, and receipt success metrics".to_string();
    scenario.reason = "generated-file exclusion dogfood records an honest no-action result instead of a repair card".to_string();

    let report = dogfood_python_static_limit_eval_run(&scenario)
        .errors
        .join("\n");

    assert!(report.is_empty(), "{report}");
}

#[test]
fn dogfood_python_no_action_eval_accepts_no_related_test() {
    let report = dogfood_python_no_action_eval_run(&valid_python_no_action_eval_scenario())
        .errors
        .join("\n");

    assert!(report.is_empty(), "{report}");
}

#[test]
fn dogfood_python_no_action_eval_accepts_already_observed() {
    let report =
        dogfood_python_no_action_eval_run(&valid_python_already_observed_no_action_eval_scenario())
            .errors
            .join("\n");

    assert!(report.is_empty(), "{report}");
}

#[test]
fn dogfood_python_no_action_eval_rejects_actionable_overclaiming() {
    let mut scenario = valid_python_no_action_eval_scenario();
    scenario.repair_card_present = true;
    scenario.agent_packet_present = true;
    scenario.verify_result = "pass".to_string();
    scenario.receipt_result = "pass".to_string();
    scenario.gap_movement = "closed".to_string();
    scenario.stop_reasons.clear();
    scenario.claim_boundary.clear();

    let report = dogfood_python_no_action_eval_run(&scenario)
        .errors
        .join("\n");

    assert!(report.contains("no-action evals must not emit repair cards"));
    assert!(report.contains("no-action evals must not emit agent packets"));
    assert!(report.contains("verify_result=not_applicable"));
    assert!(report.contains("receipt_result=not_applicable"));
    assert!(report.contains("gap_movement=no_receipt"));
    assert!(report.contains("stop_reasons must explain the no-action decision"));
    assert!(report.contains("claim_boundary must include No repair packet emitted"));
}

#[test]
fn dogfood_python_ranked_finding_parser_defaults_missing_reason() {
    let case = serde_json::json!({
        "ranked_top_3_findings": [
            {
                "rank": 1,
                "canonical_gap_id": "gap:python:app/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold",
                "repair_card_present": true,
                "usability": "usable",
                "missing_discriminator": "amount == threshold",
                "suggested_test_file": "tests/test_pricing.py",
                "verify_command": "pytest tests/test_pricing.py::test_calculate_discount_smoke",
                "false_positive_notes": "none observed"
            }
        ]
    });

    let findings = super::super::dogfood_python_ranked_findings(&case);

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].reason,
        "ranked Python finding did not document a reason"
    );
}

#[test]
fn dogfood_python_ranked_findings_json_separates_multiple_entries() {
    let findings = vec![
        valid_python_ranked_finding(1),
        valid_python_ranked_finding(2),
    ];
    let mut body = String::new();

    super::super::dogfood_push_python_ranked_findings_json(&mut body, &findings);

    assert!(body.contains("}, {"));
    assert!(body.contains("\"rank\": 1"));
    assert!(body.contains("\"rank\": 2"));
}

#[test]
fn dogfood_python_real_repo_eval_rejects_overclaiming() {
    let mut scenario = valid_python_real_repo_eval_scenario();
    scenario.canonical_gap_id = "gap:rust:src/lib.rs:predicate_boundary".to_string();
    scenario.changed_owner = "app/pricing.py::calculate_discount".to_string();
    scenario.repair_card_present = false;
    scenario.gap_movement = "closed".to_string();
    scenario.closed_gaps = 0;
    scenario.verify_result = "not_run".to_string();
    scenario.limitation_notes = "dynamic routing remains unsupported".to_string();
    scenario.claim_boundary.clear();

    let report = dogfood_python_real_repo_eval_run(&scenario)
        .errors
        .join("\n");

    assert!(report.contains("canonical_gap_id must use gap:python"));
    assert!(report.contains("changed_owner must use python: identity"));
    assert!(report.contains("repair_card_present must be true"));
    assert!(report.contains("closed gap movement must record closed_gaps > 0"));
    assert!(report.contains("closed gap movement requires verify_result=pass"));
    assert!(report.contains(
        "limitation_notes mention unsupported behavior but unsupported_limitations is empty"
    ));
    assert!(report.contains("claim_boundary must keep preview boundary denials visible"));
}

#[test]
fn dogfood_python_real_repo_eval_rejects_unranked_top_3_capture() {
    let mut scenario = valid_python_real_repo_eval_scenario();
    scenario.ranked_top_3_findings.clear();
    scenario.ranked_top_3_limit_reason = None;

    let report = dogfood_python_real_repo_eval_run(&scenario)
        .errors
        .join("\n");

    assert!(report.contains("ranked_top_3_findings must capture at least the top Python finding"));

    let mut malformed = valid_python_real_repo_eval_scenario();
    malformed.ranked_top_3_findings[0].rank = 4;
    malformed.ranked_top_3_findings[0].canonical_gap_id =
        "gap:rust:src/lib.rs:predicate_boundary".to_string();
    malformed.ranked_top_3_findings[0].usability = "needs_review".to_string();
    malformed.ranked_top_3_findings[0].missing_discriminator = "uncertain".to_string();
    malformed.ranked_top_3_findings[0].suggested_test_file = "src/pricing.py".to_string();
    malformed.ranked_top_3_findings[0].verify_command = "tox".to_string();
    malformed.ranked_top_3_findings[0].false_positive_notes =
        "possible false actionability".to_string();

    let report = dogfood_python_real_repo_eval_run(&malformed)
        .errors
        .join("\n");

    assert!(report.contains("ranked_top_3_findings rank must be between 1 and 3"));
    assert!(report.contains("ranked_top_3_findings rank 4 canonical_gap_id must use gap:python"));
    assert!(report.contains(
            "ranked_top_3_findings rank 4 must be usable, concrete, placed, verifiable, and false-positive clean"
        ));
    assert!(report.contains("ranked_top_3_findings must include rank 1"));

    let mut too_many = valid_python_real_repo_eval_scenario();
    too_many.ranked_top_3_findings = vec![
        valid_python_ranked_finding(1),
        valid_python_ranked_finding(2),
        valid_python_ranked_finding(3),
        valid_python_ranked_finding(4),
    ];

    let report = dogfood_python_real_repo_eval_run(&too_many)
        .errors
        .join("\n");

    assert!(report.contains("ranked_top_3_findings must capture at most three findings"));

    let mut missing_limit_reason = valid_python_real_repo_eval_scenario();
    missing_limit_reason.ranked_top_3_limit_reason = Some(" ".to_string());

    let report = dogfood_python_real_repo_eval_run(&missing_limit_reason)
        .errors
        .join("\n");

    assert!(
        report.contains("ranked_top_3_limit_reason must explain fewer-than-three ranked findings")
    );

    let mut duplicate_rank = valid_python_real_repo_eval_scenario();
    duplicate_rank.ranked_top_3_findings = vec![
        valid_python_ranked_finding(1),
        valid_python_ranked_finding(1),
    ];

    let report = dogfood_python_real_repo_eval_run(&duplicate_rank)
        .errors
        .join("\n");

    assert!(report.contains("ranked_top_3_findings rank 1 is duplicated"));

    let mut missing_card = valid_python_real_repo_eval_scenario();
    missing_card.ranked_top_3_findings[0].repair_card_present = false;

    let report = dogfood_python_real_repo_eval_run(&missing_card)
        .errors
        .join("\n");

    assert!(report.contains("ranked_top_3_findings rank 1 must be repair-card-backed"));

    let mut missing_reason = valid_python_real_repo_eval_scenario();
    missing_reason.ranked_top_3_findings[0].reason.clear();

    let report = dogfood_python_real_repo_eval_run(&missing_reason)
        .errors
        .join("\n");

    assert!(report.contains("ranked_top_3_findings rank 1 must document a reason"));

    let mut verify_mismatch = valid_python_real_repo_eval_scenario();
    verify_mismatch.ranked_top_3_findings[0].verify_command =
        "pytest tests/test_pricing.py::test_calculate_discount_boundary".to_string();

    let report = dogfood_python_real_repo_eval_run(&verify_mismatch)
        .errors
        .join("\n");

    assert!(report.contains(
        "ranked_top_3_findings rank 1 verify_command must match the recorded top finding"
    ));
}

#[test]
fn dogfood_python_real_repo_eval_accepts_unittest_ranked_verify_command() {
    let mut scenario = valid_python_real_repo_eval_scenario();
    let verify_command =
        "python -m unittest tests.test_pricing.PricingTests.test_calculate_discount_smoke";
    scenario.verify_command = verify_command.to_string();
    scenario.ranked_top_3_findings[0].verify_command = verify_command.to_string();

    let run = dogfood_python_real_repo_eval_run(&scenario);

    assert!(
        run.errors.is_empty(),
        "unittest verify commands should validate for ranked Python dogfood findings: {:?}",
        run.errors
    );
}

#[test]
fn dogfood_python_real_repo_eval_checks_ranked_top_3_boundaries() {
    let mut valid_top_3 = valid_python_real_repo_eval_scenario();
    let mut second = valid_top_3.ranked_top_3_findings[0].clone();
    second.rank = 2;
    second.canonical_gap_id = "gap:python:app/cart.py:total:return_value:total".to_string();
    second.missing_discriminator = "cart total includes tax".to_string();
    second.suggested_test_file = "cart_test.py".to_string();
    second.verify_command = "python -m unittest cart_test.CartTotalTest".to_string();
    second.false_positive_notes = "none".to_string();
    second.reason = "rank 2 uses unittest verification and a module-level test file".to_string();

    let mut third = valid_top_3.ranked_top_3_findings[0].clone();
    third.rank = 3;
    third.canonical_gap_id = "gap:python:app/orders.py:submit:side_effect:email".to_string();
    third.missing_discriminator = "submission sends confirmation email".to_string();
    third.suggested_test_file = "app/tests/test_orders.py".to_string();
    third.reason = "rank 3 uses a package-local test path".to_string();

    valid_top_3.ranked_top_3_findings.push(second);
    valid_top_3.ranked_top_3_findings.push(third);
    valid_top_3.ranked_top_3_limit_reason = None;

    assert!(
        dogfood_python_real_repo_eval_run(&valid_top_3)
            .errors
            .is_empty()
    );
    let quality =
        dogfood_python_repair_routing_quality_summary(&[dogfood_python_real_repo_eval_run(
            &valid_top_3,
        )]);
    assert_eq!(quality.full_top_3_capture_cases, 1);
    assert_eq!(quality.gate_status, "pass");

    let mut missing_limit_reason = valid_python_real_repo_eval_scenario();
    missing_limit_reason.ranked_top_3_limit_reason = None;
    let report = dogfood_python_real_repo_eval_run(&missing_limit_reason)
        .errors
        .join("\n");
    assert!(
        report.contains("ranked_top_3_limit_reason must explain fewer-than-three ranked findings")
    );

    let mut malformed = valid_top_3;
    malformed
        .ranked_top_3_findings
        .push(malformed.ranked_top_3_findings[0].clone());
    malformed.ranked_top_3_findings[3].rank = 2;
    malformed.ranked_top_3_findings[3].reason.clear();
    let report = dogfood_python_real_repo_eval_run(&malformed)
        .errors
        .join("\n");
    assert!(report.contains("ranked_top_3_findings must capture at most three findings"));
    assert!(report.contains("ranked_top_3_findings rank 2 is duplicated"));
    assert!(report.contains("ranked_top_3_findings rank 2 must document a reason"));
}

#[test]
fn dogfood_typescript_preview_repair_loop_receipts_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_typescript_preview_repair_loop_scenarios();
        for required in TYPESCRIPT_PREVIEW_REPAIR_LOOP_REQUIRED_CASES {
            assert!(
                scenarios.iter().any(|scenario| {
                    scenario.name == required.0 && scenario.outcome == required.1
                }),
                "{} TypeScript preview repair-loop receipt should be checked as {}",
                required.0,
                required.1
            );
        }
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.language == "typescript"),
            "TypeScript preview repair-loop receipts should include TypeScript"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.language == "javascript"),
            "TypeScript preview repair-loop receipts should include JavaScript"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.gap_state == "static_limitation"),
            "TypeScript preview repair-loop receipts should include a static limitation"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.outcome == "weak_oracle_downgraded"),
            "TypeScript preview repair-loop receipts should include a weak-oracle downgrade"
        );
        assert!(
            scenarios.iter().any(|scenario| {
                scenario.name == "typescript_complete_boundary_packet_closed"
                    && scenario.repair_packet_ready
                    && scenario.gap_state == "actionable"
                    && scenario.actionability_category == "complete_repair_packet"
                    && scenario.outcome == "resolved"
                    && scenario.verify_result == "pass"
            }),
            "TypeScript preview repair-loop receipts should include the closed complete advisory packet"
        );
        assert!(
            scenarios.iter().any(|scenario| {
                scenario.must_have_verify_command
                    && scenario.must_have_receipt_command
                    && scenario.must_not_invent_verify_command
                    && scenario.must_not_emit_repair_packet
            }),
            "TypeScript preview repair-loop receipts should pin operator commands without source packet authority"
        );

        for scenario in scenarios {
            let run = dogfood_typescript_preview_repair_loop_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} TypeScript preview repair-loop receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_bun_ub_cross_language_receipts_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_bun_ub_cross_language_scenarios();
        for required in BUN_UB_CROSS_LANGUAGE_DOGFOOD_REQUIRED_CASES {
            assert!(
                scenarios.iter().any(|scenario| {
                    scenario.name == required.0 && scenario.observed_state == required.1
                }),
                "{} Bun UB cross-language dogfood receipt should be checked as {}",
                required.0,
                required.1
            );
        }
        assert!(
            scenarios
                .iter()
                .all(|scenario| !scenario.repair_packet_ready),
            "Bun UB cross-language dogfood receipts must not claim repair packets"
        );
        assert!(
            scenarios.iter().any(|scenario| {
                scenario.observed_state == "rust_ungripped_ts_missing_discriminator"
                    && scenario
                        .missing_discriminators
                        .contains(&"resizable_array_buffer".to_string())
                    && scenario.suggested_test_file
                        == super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE
            }),
            "Bun UB dogfood should suggest blob.test.ts for stripped resizable"
        );
        assert!(
            scenarios.iter().any(|scenario| {
                scenario.observed_state == "ts_mention_not_observer"
                    && scenario.operator_action == "reject_token_mention"
                    && scenario.suggested_test_file == "not_applicable"
            }),
            "Bun UB dogfood should reject mention-only token evidence"
        );
        assert!(
            scenarios.iter().any(|scenario| {
                scenario.observed_state == "public_reachable_panic_boundary_unrevealed"
                    && scenario
                        .missing_discriminators
                        .contains(&"negative_offset".to_string())
                    && scenario
                        .missing_graph_legs
                        .iter()
                        .any(|missing| missing == "external_oracle:negative_offset_panic_boundary")
                    && scenario
                        .missing_graph_legs
                        .iter()
                        .any(|missing| missing == "safe_external_observer_target")
                    && scenario.suggested_test_file == "not_applicable"
                    && scenario.operator_action == "keep_panic_boundary_limitation"
            }),
            "Bun UB dogfood should keep the FFI panic boundary as a limitation"
        );

        for scenario in scenarios {
            let run = dogfood_bun_ub_cross_language_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} Bun UB cross-language dogfood receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_bun_ub_cross_language_rejects_overclaiming() {
    let mut scenario = valid_bun_ub_cross_language_dogfood_scenario();
    scenario.expected_state = "rust_ungripped_ts_missing_discriminator".to_string();
    scenario.authority_boundary = "gate_eligible".to_string();
    scenario.repair_packet_ready = true;
    scenario.non_claims.clear();

    let report = dogfood_bun_ub_cross_language_run(&scenario)
        .errors
        .join("\n");
    assert!(report.contains("expected_state and observed_state must match"));
    assert!(report.contains("authority_boundary must be preview_advisory_only"));
    assert!(report.contains("repair_packet_ready must remain false"));
    assert!(report.contains("non_claims must keep preview boundary denials visible"));

    let mut missing = valid_bun_ub_cross_language_dogfood_scenario();
    missing.observed_state = "rust_ungripped_ts_missing_discriminator".to_string();
    missing.expected_state = "rust_ungripped_ts_missing_discriminator".to_string();
    missing.operator_action = "suggest_shared_case".to_string();
    missing.placement_verdict = "not_applicable".to_string();
    let missing_report = dogfood_bun_ub_cross_language_run(&missing)
        .errors
        .join("\n");
    assert!(missing_report.contains("must name resizable_array_buffer"));
    assert!(missing_report.contains("must suggest the configured Bun Blob TypeScript test file"));
    assert!(missing_report.contains("action must name the missing resizable case"));

    let mut mention = valid_bun_ub_cross_language_dogfood_scenario();
    mention.observed_state = "ts_mention_not_observer".to_string();
    mention.expected_state = "ts_mention_not_observer".to_string();
    mention.operator_action = "suggest_typescript_blob_case".to_string();
    mention.suggested_test_file = super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE.to_string();
    mention.missing_discriminators = vec!["resizable_array_buffer".to_string()];
    mention.bridge_verdict = "configured_binding_edge_credited".to_string();
    let mention_report = dogfood_bun_ub_cross_language_run(&mention)
        .errors
        .join("\n");
    assert!(mention_report.contains("must not turn token text into a missing discriminator"));
    assert!(mention_report.contains("must not suggest placement"));
    assert!(mention_report.contains("must reject the token mention"));
    assert!(mention_report.contains("cannot credit a token mention"));

    let mut panic_boundary = valid_bun_ub_cross_language_dogfood_scenario();
    panic_boundary.name = "bun_ffi_negative_offset_panic_boundary".to_string();
    panic_boundary.source_case = "bun_ffi_negative_offset_panic_boundary_limitation".to_string();
    panic_boundary.route_quality_case =
        "bun_ffi_negative_offset_panic_boundary_limitation".to_string();
    panic_boundary.rust_file = "src/bun.js/bindings/FFIObject.rs".to_string();
    panic_boundary.rust_owner = "FFIObject::read".to_string();
    panic_boundary.rust_boundary = "usize::try_from(to_int32()).expect(\"int cast\")".to_string();
    panic_boundary.ts_test_file = super::super::BUN_FFI_NEGATIVE_OFFSET_TS_TEST_SURFACE.to_string();
    panic_boundary.expected_state = "public_reachable_panic_boundary_unrevealed".to_string();
    panic_boundary.observed_state = "public_reachable_panic_boundary_unrevealed".to_string();
    panic_boundary.missing_discriminators.clear();
    panic_boundary.missing_graph_legs.clear();
    panic_boundary.suggested_test_file =
        super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE.to_string();
    panic_boundary.operator_action = "suggest_ffi_test".to_string();
    panic_boundary.placement_verdict =
        "suggested_test_file:test/js/bun/ffi/ffi.test.ts".to_string();
    panic_boundary.bridge_verdict = "unknown".to_string();
    panic_boundary.proof_mode = "repair_packet_preview".to_string();
    let panic_boundary_report = dogfood_bun_ub_cross_language_run(&panic_boundary)
        .errors
        .join("\n");
    assert!(panic_boundary_report.contains("must name negative_offset as missing"));
    assert!(panic_boundary_report.contains("negative-offset oracle unresolved"));
    assert!(panic_boundary_report.contains("safe external observer target unresolved"));
    assert!(panic_boundary_report.contains("must not suggest placement"));
    assert!(panic_boundary_report.contains("must keep the named limitation"));
    assert!(panic_boundary_report.contains("placement must remain not_applicable"));
    assert!(panic_boundary_report.contains("must document the FFI bridge"));
    assert!(panic_boundary_report.contains("proof mode must stay a limitation receipt"));
}

#[test]
fn dogfood_bun_ub_cross_language_rejects_live_profile_drift() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_bun_ub_cross_language_scenarios();
        let scenario = |name: &str| -> Result<DogfoodBunUbCrossLanguageScenario, String> {
            let Some(found) = scenarios.iter().find(|scenario| scenario.name == name) else {
                return Err(format!("{name} should exist in Bun UB dogfood corpus"));
            };
            Ok(found.clone())
        };

        let mut copy_to_unshared = scenario("bun_array_buffer_copy_to_unshared_live_receipt")?;
        copy_to_unshared.source_case = "wrong".to_string();
        copy_to_unshared.route_quality_case = "wrong".to_string();
        copy_to_unshared.rust_file = "src/jsc/Blob.rs".to_string();
        copy_to_unshared.rust_owner = "Blob::from_js_without_defer_gc".to_string();
        copy_to_unshared.rust_boundary = "wrong".to_string();
        copy_to_unshared.ts_test_file = "test/js/bun/write.test.ts".to_string();
        let copy_report = dogfood_bun_ub_cross_language_run(&copy_to_unshared)
            .errors
            .join("\n");
        assert!(copy_report.contains("copy_to_unshared route-quality case"));
        assert!(copy_report.contains("copy_to_unshared Rust seam"));
        assert!(copy_report.contains("rust_owner must stay on copy_to_unshared"));
        assert!(copy_report.contains("copy_to_unshared stable-byte boundary"));
        assert!(copy_report.contains("Blob TypeScript witness file"));

        let mut markdown = scenario("bun_markdown_resizable_array_buffer_live_receipt")?;
        markdown.source_case = "wrong".to_string();
        markdown.route_quality_case = "wrong".to_string();
        markdown.rust_file = "src/jsc/Blob.rs".to_string();
        markdown.rust_owner = "Blob::from_js_without_defer_gc".to_string();
        markdown.rust_boundary = "wrong".to_string();
        markdown.ts_test_file = super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE.to_string();
        let markdown_report = dogfood_bun_ub_cross_language_run(&markdown)
            .errors
            .join("\n");
        assert!(markdown_report.contains("MarkdownObject route-quality case"));
        assert!(markdown_report.contains("MarkdownObject Rust seam"));
        assert!(markdown_report.contains("MarkdownObject::to_string"));
        assert!(markdown_report.contains("MarkdownObject resizable boundary"));
        assert!(markdown_report.contains("Bun markdown TypeScript test file"));

        let mut bridge_unknown = scenario("bun_blob_bridge_unknown_live_receipt")?;
        bridge_unknown.missing_discriminators = vec!["resizable_array_buffer".to_string()];
        bridge_unknown.missing_graph_legs.clear();
        bridge_unknown.suggested_test_file =
            super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE.to_string();
        bridge_unknown.operator_action = "suggest_blob_test".to_string();
        bridge_unknown.placement_verdict =
            "suggested_test_file:test/js/web/fetch/blob.test.ts".to_string();
        bridge_unknown.bridge_verdict = "configured_binding_edge_credited".to_string();
        bridge_unknown.proof_mode = "observable_red_green".to_string();
        let bridge_report = dogfood_bun_ub_cross_language_run(&bridge_unknown)
            .errors
            .join("\n");
        assert!(bridge_report.contains("bridge_unknown must not report missing discriminators"));
        assert!(bridge_report.contains("missing binding_or_ffi_edge"));
        assert!(bridge_report.contains("must not suggest TypeScript placement"));
        assert!(bridge_report.contains("must route to bridge inspection"));
        assert!(bridge_report.contains("placement must remain not_applicable"));
        assert!(bridge_report.contains("must document the missing bridge leg"));
        assert!(bridge_report.contains("proof mode must stay bridge_unknown"));

        let mut node_fs = scenario("bun_node_fs_scalar_write_manifest_only_receipt")?;
        node_fs.missing_discriminators = vec!["scalar_write".to_string()];
        node_fs.missing_graph_legs.clear();
        node_fs.bridge_verdict = "configured_binding_edge_credited".to_string();
        node_fs.suggested_test_file = "not_applicable".to_string();
        node_fs.placement_verdict = "actionable".to_string();
        node_fs.proof_mode = "helper_gated".to_string();
        let node_fs_report = dogfood_bun_ub_cross_language_run(&node_fs)
            .errors
            .join("\n");
        assert!(node_fs_report.contains("must not invent missing discriminators"));
        assert!(node_fs_report.contains("must name missing graph legs"));
        assert!(node_fs_report.contains("manifest-only bridge evidence uncredited"));
        assert!(node_fs_report.contains("node_fs_scalar_write bridge leg missing"));
        assert!(node_fs_report.contains("scalar-write oracle leg missing"));
        assert!(node_fs_report.contains("typed witness path"));
        assert!(node_fs_report.contains("placement must remain non-actionable"));
        assert!(node_fs_report.contains("proof mode must be observable_red_green"));

        let mut bun_write = scenario("bun_write_helper_gated_manifest_only_receipt")?;
        bun_write.missing_graph_legs.clear();
        bun_write.bridge_verdict = "configured_binding_edge_credited".to_string();
        bun_write.suggested_test_file = super::super::BUN_WRITE_TS_TEST_FILE.to_string();
        bun_write.placement_verdict = "suggested_test_file:test/js/bun/write.test.ts".to_string();
        bun_write.proof_mode = "observable_red_green".to_string();
        let bun_write_report = dogfood_bun_ub_cross_language_run(&bun_write)
            .errors
            .join("\n");
        assert!(bun_write_report.contains("must name missing graph legs"));
        assert!(bun_write_report.contains("manifest-only bridge evidence uncredited"));
        assert!(bun_write_report.contains("binding_or_ffi_edge:bun_write_sink"));
        assert!(bun_write_report.contains("helper:bun_write_fixture_helper"));
        assert!(bun_write_report.contains("external_oracle:stable_byte_write"));
        assert!(bun_write_report.contains("must not suggest placement"));
        assert!(bun_write_report.contains("placement must remain not_applicable"));
        assert!(bun_write_report.contains("proof mode must stay helper_gated"));

        let mut unsupported = valid_bun_ub_cross_language_dogfood_scenario();
        unsupported.observed_state = "named_static_limitation".to_string();
        unsupported.expected_state = "named_static_limitation".to_string();
        unsupported.bridge_verdict = "manifest_only_bridge_not_credited".to_string();
        unsupported.missing_graph_legs = vec!["binding_or_ffi_edge:unknown".to_string()];
        let unsupported_report = dogfood_bun_ub_cross_language_run(&unsupported)
            .errors
            .join("\n");
        assert!(unsupported_report.contains("supported manifest-only profile"));

        Ok(())
    })
}

#[test]
fn dogfood_typescript_false_actionable_audit_summary_flags_packet_ready_rows() -> Result<(), String>
{
    with_repo_cwd(|| {
        let cases = super::super::typescript_preview_false_actionable_audit_cases();
        let summary = super::super::dogfood_typescript_false_actionable_audit_summary(&cases);
        assert_eq!(summary.gate_status, "pass");
        assert_eq!(summary.cases, 15);
        assert_eq!(summary.must_remain_non_actionable, 15);
        assert_eq!(summary.false_actionable, 0);
        assert_eq!(summary.repair_packet_ready_true, 0);

        let mut noisy_cases = cases;
        noisy_cases[0].repair_packet_ready = true;
        let noisy_summary =
            super::super::dogfood_typescript_false_actionable_audit_summary(&noisy_cases);
        assert_eq!(noisy_summary.gate_status, "review");
        assert_eq!(noisy_summary.false_actionable, 1);
        assert_eq!(noisy_summary.repair_packet_ready_true, 1);
        Ok(())
    })
}

#[test]
fn dogfood_typescript_preview_repair_loop_rejects_overclaiming() {
    let mut scenario = valid_typescript_preview_repair_loop_scenario();
    scenario.language_status = "alpha".to_string();
    scenario.authority_boundary = "gate_eligible".to_string();
    scenario.gap_state = "actionable".to_string();
    scenario.outcome = "resolved".to_string();
    scenario.non_claims.clear();

    let report = dogfood_typescript_preview_repair_loop_run(&scenario)
        .errors
        .join("\n");

    assert!(report.contains("language_status must be preview"));
    assert!(report.contains("authority_boundary must be preview_advisory_only"));
    assert!(report.contains("repair_packet_ready=false must not be paired"));
    assert!(report.contains("repair_packet_ready=false must not claim resolved"));
    assert!(report.contains("non_claims must keep preview boundary denials visible"));

    let mut static_limit = valid_typescript_preview_repair_loop_scenario();
    static_limit.static_limit_kind = Some("mocked_module".to_string());
    static_limit.gap_state = "advisory".to_string();
    let static_report = dogfood_typescript_preview_repair_loop_run(&static_limit)
        .errors
        .join("\n");
    assert!(static_report.contains("static_limit_kind requires gap_state=static_limitation"));

    let mut weak_overclaim = valid_typescript_preview_repair_loop_scenario();
    weak_overclaim.outcome = "weak_oracle_downgraded".to_string();
    weak_overclaim.oracle_kind = "exact_value".to_string();
    weak_overclaim.oracle_strength = "strong".to_string();
    weak_overclaim.gap_state = "already_observed".to_string();
    weak_overclaim.actionability_category = "strong_oracle_observed".to_string();
    let weak_report = dogfood_typescript_preview_repair_loop_run(&weak_overclaim)
        .errors
        .join("\n");
    assert!(weak_report.contains("weak_oracle_downgraded must not use a strong oracle"));
    assert!(
        weak_report
            .contains("weak_oracle_downgraded must not claim already-observed actionability")
    );

    let mut placeholder_command = valid_typescript_preview_repair_loop_scenario();
    placeholder_command.verify_command = "verify_command_unknown".to_string();
    placeholder_command.receipt_command = "receipt_command_unknown".to_string();
    let placeholder_report = dogfood_typescript_preview_repair_loop_run(&placeholder_command)
        .errors
        .join("\n");
    assert!(
        placeholder_report
            .contains("must_have_verify_command requires a concrete operator verify_command")
    );
    assert!(
        placeholder_report
            .contains("must_have_receipt_command requires a concrete operator receipt_command")
    );

    let mut packet_overclaim = valid_typescript_preview_repair_loop_scenario();
    packet_overclaim.repair_packet_ready = true;
    let packet_report = dogfood_typescript_preview_repair_loop_run(&packet_overclaim)
        .errors
        .join("\n");
    assert!(
        packet_report.contains("must_not_emit_repair_packet requires repair_packet_ready=false")
    );
}

#[test]
fn dogfood_typescript_preview_repair_loop_rejects_invented_source_verify_command() {
    with_temp_cwd(
        "typescript-preview-repair-loop-invented-source-verify",
        |root| {
            let fixture = root.join("fixtures/ts/expected/check.json");
            write(
                &fixture,
                r#"{
  "findings": [
    {
      "id": "probe:src_discount.ts:typescript_preview:2396aec1",
      "classification": "weakly_exposed",
      "language": "typescript",
      "language_status": "preview",
      "oracle_kind": "unknown",
      "oracle_strength": "unknown",
      "static_limit_kind": null,
      "evidence": [
        "typescript_verify_command: npm test -- invented"
      ],
      "probe": {
        "family": "predicate",
        "owner": "fixture::applyDiscount"
      },
      "preview_actionability": {
        "gap_state": "advisory",
        "actionability_category": "incomplete_repair_packet",
        "authority_boundary": "preview_advisory_only",
        "repair_packet_ready": false,
        "why_not_actionable": "TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract",
        "repair_route": "project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available"
      },
      "typescript_preview_card": {
        "repair_packet_ready": false,
        "verify": {
          "command": "npm test -- invented"
        }
      }
    }
  ]
}
"#,
            );

            let mut scenario = valid_typescript_preview_repair_loop_scenario();
            scenario.source_fixture = "fixtures/ts".to_string();
            let report = dogfood_typescript_preview_repair_loop_run(&scenario)
                .errors
                .join("\n");

            assert!(report.contains(
                "must_not_invent_verify_command rejects source typescript_verify_command evidence"
            ));
            assert!(report.contains(
                "must_not_invent_verify_command requires source preview card verify.command to stay absent"
            ));
        },
    );
}

#[test]
fn dogfood_user_surface_projection_alignment_receipts_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_user_surface_projection_scenarios();
        let surfaces = scenarios
            .iter()
            .map(|scenario| scenario.surface.as_str())
            .collect::<BTreeSet<_>>();
        for required in USER_SURFACE_PROJECTION_REQUIRED_SURFACES {
            assert!(
                surfaces.contains(required),
                "{required} user surface projection receipt should be checked"
            );
        }
        let run_statuses = scenarios
            .iter()
            .map(|scenario| scenario.run_status.as_str())
            .collect::<BTreeSet<_>>();
        for required in USER_SURFACE_PROJECTION_REQUIRED_RUN_STATUSES {
            assert!(
                run_statuses.contains(required),
                "{required} user surface projection receipt should be checked"
            );
        }

        for scenario in scenarios {
            let run = dogfood_user_surface_projection_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} user surface projection receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_user_surface_projection_alignment_requires_limited_and_stale_statuses() {
    let scenarios = vec![valid_user_surface_projection_scenario()];
    let report = user_surface_projection_required_run_status_violations(&scenarios).join("\n");

    assert!(!report.contains("run_status full"));
    assert!(report.contains("run_status limited_large_cache_skip"));
    assert!(report.contains("run_status limited_incomplete_input"));
    assert!(report.contains("run_status limited_sampled_input"));
    assert!(report.contains("run_status limited_stale_input"));
}

#[test]
fn dogfood_user_surface_projection_alignment_reports_contract_drift() {
    let mut scenario = valid_user_surface_projection_scenario();
    scenario.name.clear();
    scenario.surface = "raw_dashboard".to_string();
    scenario.run_status = "limited_stale_input".to_string();
    scenario.projection_basis = "raw_findings".to_string();
    scenario.canonical_gap_id = "boundary-discriminator-004".to_string();
    scenario.raw_findings_total = scenario.actionable_count;
    scenario.consumes_canonical_state = false;
    scenario.reinterprets_raw_findings = true;
    scenario.raw_findings_headline = true;
    scenario.advisory = false;
    scenario.blocking_default = true;
    scenario.limited_state_visible = false;
    scenario.stale_state_visible = false;
    scenario.receipt_command = scenario.verify_command.clone();

    let report = dogfood_user_surface_projection_run(&scenario)
        .errors
        .join("\n");

    assert!(report.contains("case id must be present"));
    assert!(report.contains("unsupported surface raw_dashboard"));
    assert!(
        report.contains("limited_stale_input projection_basis must be canonical_runtime_status")
    );
    assert!(report.contains("canonical_gap_id must use gap: identity"));
    assert!(report.contains("surface must consume canonical state"));
    assert!(report.contains("surface must not reinterpret raw findings"));
    assert!(report.contains("surface must not headline raw findings"));
    assert!(report.contains("surface must remain advisory"));
    assert!(report.contains("surface must not be blocking by default"));
    assert!(report.contains("surface must make limited state visible"));
    assert!(report.contains("surface must make stale state visible"));
    assert!(report.contains("receipt_command must stay distinct from verify_command"));
    assert!(report.contains("limited or stale run_status must not carry packet identity"));
    assert!(report.contains("limited or stale run_status must not carry packet repair commands"));
    assert!(report.contains("limited or stale run_status must not carry source_alignment_case"));
    assert!(report.contains("limited_stale_input must name a limitation_category"));
    assert!(report.contains("limited_stale_input must provide a runtime_repair_command"));
    assert!(report.contains("limited_stale_input must route resolve_limited_runtime_status"));
    assert!(report.contains("limited_stale_input headline must make stale state visible"));
    assert!(report.contains("limited or stale run_status must not headline an actionable count"));
}

#[test]
fn dogfood_user_surface_projection_alignment_accepts_limited_runtime_status() {
    let mut scenario = valid_user_surface_projection_scenario();
    scenario.name = "badge_limited_state_from_canonical_runtime".to_string();
    scenario.headline = "ripr: limited".to_string();
    scenario.run_status = "limited_sampled_input".to_string();
    scenario.projection_basis = "canonical_runtime_status".to_string();
    scenario.canonical_gap_id.clear();
    scenario.packet_id.clear();
    scenario.repair_kind.clear();
    scenario.top_next_action_kind = "resolve_limited_runtime_status".to_string();
    scenario.verify_command.clear();
    scenario.receipt_command.clear();
    scenario.source_alignment_case.clear();
    scenario.limitation_category = "lane1_repo_exposure_sampled".to_string();
    scenario.runtime_repair_command = "cargo xtask lane1-evidence-audit".to_string();

    let run = dogfood_user_surface_projection_run(&scenario);

    assert!(run.errors.is_empty(), "{:?}", run.errors);
}

#[test]
fn dogfood_user_surface_projection_alignment_rejects_wrong_runtime_repair_route() {
    let mut scenario = valid_user_surface_projection_scenario();
    scenario.name = "badge_limited_state_from_canonical_runtime".to_string();
    scenario.headline = "ripr: limited".to_string();
    scenario.run_status = "limited_large_cache_skip".to_string();
    scenario.projection_basis = "canonical_runtime_status".to_string();
    scenario.canonical_gap_id.clear();
    scenario.packet_id.clear();
    scenario.repair_kind.clear();
    scenario.top_next_action_kind = "resolve_limited_runtime_status".to_string();
    scenario.verify_command.clear();
    scenario.receipt_command.clear();
    scenario.source_alignment_case.clear();
    scenario.limitation_category = "lane1_repo_exposure_sampled".to_string();
    scenario.runtime_repair_command = "cargo xtask lane1-evidence-audit".to_string();

    let report = dogfood_user_surface_projection_run(&scenario)
        .errors
        .join("\n");

    assert!(
        report.contains(
            "limited_large_cache_skip limitation_category must be limited_large_cache_skip"
        )
    );
    assert!(report.contains(
            "limited_large_cache_skip runtime_repair_command must be cargo xtask cache report && cargo xtask cache gc --dry-run"
        ));
}

#[test]
fn dogfood_user_surface_projection_alignment_rejects_limited_static_limitation_backlog_projection()
{
    let mut scenario = valid_user_surface_projection_scenario();
    scenario.name = "badge_limited_static_limitation_backlog_from_runtime".to_string();
    scenario.headline = "ripr: limited".to_string();
    scenario.run_status = "limited_sampled_input".to_string();
    scenario.projection_basis = "canonical_limitation_backlog".to_string();
    scenario.canonical_gap_id =
        "gap:activation-owner-call-absent-assertion-target-affinity".to_string();
    scenario.packet_id =
        "limitation:activation-owner-call-absent-assertion-target-affinity".to_string();
    scenario.repair_kind = "sharpen_static_limitation_route".to_string();
    scenario.top_next_action_kind = "route_static_limitation_backlog".to_string();
    scenario.verify_command =
        "cargo test -p ripr evidence_record_splits_assertion_target_owner_call_absence_limitation"
            .to_string();
    scenario.receipt_command = "cargo xtask check-fixture-contracts".to_string();
    scenario.source_alignment_case = "static_limitation_backlog_alignment".to_string();
    scenario.limitation_category = "lane1_repo_exposure_sampled".to_string();
    scenario.runtime_repair_command = "cargo xtask lane1-evidence-audit".to_string();

    let report = dogfood_user_surface_projection_run(&scenario)
        .errors
        .join("\n");

    assert!(
        report.contains("limited_sampled_input projection_basis must be canonical_runtime_status")
    );
    assert!(report.contains("limited_sampled_input must route resolve_limited_runtime_status"));
    assert!(report.contains("limited or stale run_status must not carry packet identity"));
    assert!(report.contains("limited or stale run_status must not carry packet repair commands"));
    assert!(report.contains("limited or stale run_status must not carry source_alignment_case"));
    assert!(
        !report.contains("limited_incomplete_input must name a limitation_category"),
        "correct runtime limitation category should not be rejected: {report}"
    );
    assert!(
        !report.contains("limited_incomplete_input must provide a runtime_repair_command"),
        "correct runtime repair route should not be rejected: {report}"
    );
}

#[test]
fn dogfood_surface_projection_alignment_covers_route_quality_non_success() -> Result<(), String> {
    with_repo_cwd(|| {
        let fixture = std::fs::read_to_string(super::super::SURFACE_PROJECTION_ALIGNMENT_CORPUS)
            .map_err(|err| err.to_string())?;
        let value: serde_json::Value =
            serde_json::from_str(&fixture).map_err(|err| err.to_string())?;
        let cases = value["cases"]
            .as_array()
            .ok_or("surface projection corpus cases must be an array")?;

        let has_non_success_route_quality = cases.iter().any(|case| {
            let outcome = case["expected"]["outcome"].as_str().unwrap_or("");
            let top_next_action_kind = case["expected"]["top_next_action_kind"]
                .as_str()
                .unwrap_or("");
            let route_quality_has_non_success =
                case["artifacts"]["swarm_attempt_ledger"]["repair_route_quality"]
                    .as_array()
                    .is_some_and(|rows| {
                        rows.iter().any(|row| {
                            row["repair_kind_failure_count"].as_u64().unwrap_or(0) > 0
                                || row["repair_kind_unchanged"].as_u64().unwrap_or(0) > 0
                                || row["repair_kind_regressed"].as_u64().unwrap_or(0) > 0
                                || row["repair_kind_attempted_no_receipt"]
                                    .as_u64()
                                    .unwrap_or(0)
                                    > 0
                        })
                    });

            matches!(outcome, "evidence_unchanged" | "attempted_no_receipt")
                && top_next_action_kind != "attempt_ready_packet"
                && route_quality_has_non_success
        });

        if has_non_success_route_quality {
            Ok(())
        } else {
            Err(
                "surface projection corpus must include a non-success route-quality case"
                    .to_string(),
            )
        }
    })
}

#[test]
fn dogfood_surface_projection_alignment_covers_missing_receipt_route_quality() -> Result<(), String>
{
    with_repo_cwd(|| {
        let fixture = std::fs::read_to_string(super::super::SURFACE_PROJECTION_ALIGNMENT_CORPUS)
            .map_err(|err| err.to_string())?;
        let value: serde_json::Value =
            serde_json::from_str(&fixture).map_err(|err| err.to_string())?;
        let cases = value["cases"]
            .as_array()
            .ok_or("surface projection corpus cases must be an array")?;

        let has_missing_receipt_route_quality = cases.iter().any(|case| {
            let expected = &case["expected"];
            let route_quality = case["artifacts"]["swarm_attempt_ledger"]["repair_route_quality"]
                .as_array()
                .and_then(|rows| rows.first())
                .unwrap_or(&serde_json::Value::Null);
            let has_attempt_receipt_missing_field =
                case["artifacts"]["swarm_attempt_ledger"]["top_missing_evidence_fields"]
                    .as_array()
                    .is_some_and(|rows| {
                        rows.iter().any(|row| {
                            row["label"].as_str() == Some("attempt_receipt")
                                && row["count"].as_u64().unwrap_or(0) > 0
                        })
                    });

            expected["outcome"].as_str() == Some("attempted_no_receipt")
                && expected["top_next_action_kind"].as_str()
                    == Some("collect_missing_attempt_receipts")
                && route_quality["repair_kind_attempted_no_receipt"].as_u64() == Some(1)
                && route_quality["repair_kind_failure_count"].as_u64() == Some(1)
                && route_quality["repair_kind_dominant_failure_reason"].as_str()
                    == Some("attempted_no_receipt")
                && has_attempt_receipt_missing_field
        });

        if has_missing_receipt_route_quality {
            Ok(())
        } else {
            Err(
                "surface projection corpus must include an attempted_no_receipt route-quality case"
                    .to_string(),
            )
        }
    })
}

#[test]
fn dogfood_user_surface_projection_alignment_matches_route_quality_non_success_source()
-> Result<(), String> {
    with_repo_cwd(|| {
        let sources = super::super::dogfood_surface_projection_alignment_scenarios();
        let mut scenario = valid_user_surface_projection_scenario();
        scenario.name = "pr_comment_route_quality_degraded_from_canonical_outcome".to_string();
        scenario.surface = "pr_comment".to_string();
        scenario.artifact = "pr evidence summary".to_string();
        scenario.headline =
            "Repair route quality needs attention: add_output_observer unchanged".to_string();
        scenario.canonical_gap_id = "gap:output-observer-009".to_string();
        scenario.packet_id = "output-observer-009".to_string();
        scenario.repair_kind = "add_output_observer".to_string();
        scenario.top_next_action_kind = "improve_repair_route_quality".to_string();
        scenario.verify_command = "cargo test -p ripr-swarm output_observer_009".to_string();
        scenario.receipt_command =
                "cargo xtask receipts write --packet output-observer-009 --canonical-gap gap:output-observer-009".to_string();
        scenario.source_alignment_case = "receipt_unchanged_route_quality_alignment".to_string();
        scenario.reason =
            "PR comments should preserve canonical non-success route-quality state.".to_string();

        let report =
            super::super::user_surface_projection_source_alignment_errors(&scenario, &sources);

        assert!(
            report.is_empty(),
            "route-quality non-success source should align cleanly: {report:?}"
        );

        Ok(())
    })
}

#[test]
fn dogfood_user_surface_projection_alignment_matches_missing_receipt_source() -> Result<(), String>
{
    with_repo_cwd(|| {
        let sources = super::super::dogfood_surface_projection_alignment_scenarios();
        let scenarios = super::super::dogfood_user_surface_projection_scenarios();
        let scenario = scenarios
                .iter()
                .find(|scenario| scenario.name == "pr_comment_missing_receipt_from_canonical_outcome")
                .ok_or(
                    "user-surface corpus must include pr_comment_missing_receipt_from_canonical_outcome",
                )?;

        if scenario.top_next_action_kind != "collect_missing_attempt_receipts" {
            return Err(format!(
                "missing-receipt user surface must preserve collect_missing_attempt_receipts, got {}",
                scenario.top_next_action_kind
            ));
        }

        let report =
            super::super::user_surface_projection_source_alignment_errors(scenario, &sources);

        assert!(
            report.is_empty(),
            "missing-receipt source should align cleanly: {report:?}"
        );

        Ok(())
    })
}

#[test]
fn dogfood_user_surface_projection_alignment_covers_route_quality_non_success_all_surfaces()
-> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = super::super::dogfood_user_surface_projection_scenarios();
        let surfaces = scenarios
            .iter()
            .filter(|scenario| {
                scenario.source_alignment_case == "receipt_unchanged_route_quality_alignment"
                    && scenario.top_next_action_kind == "improve_repair_route_quality"
                    && scenario.canonical_gap_id == "gap:output-observer-009"
                    && scenario.packet_id == "output-observer-009"
                    && scenario.repair_kind == "add_output_observer"
            })
            .map(|scenario| scenario.surface.as_str())
            .collect::<std::collections::BTreeSet<_>>();

        let missing = ["badge", "lsp", "pr_comment", "ci"]
            .into_iter()
            .filter(|surface| !surfaces.contains(surface))
            .collect::<Vec<_>>();

        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "route-quality non-success user-surface projection is missing consumers: {}",
                missing.join(", ")
            ))
        }
    })
}

#[test]
fn dogfood_user_surface_projection_alignment_covers_static_limitation_backlog_all_surfaces()
-> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = super::super::dogfood_user_surface_projection_scenarios();
        let surfaces = scenarios
                .iter()
                .filter(|scenario| {
                    scenario.source_alignment_case == "static_limitation_backlog_alignment"
                        && scenario.top_next_action_kind == "route_static_limitation_backlog"
                        && scenario.projection_basis == "canonical_limitation_backlog"
                        && scenario.canonical_gap_id
                            == "gap:activation-owner-call-absent-assertion-target-affinity"
                        && scenario.packet_id
                            == "limitation:activation-owner-call-absent-assertion-target-affinity:assertion-target-affinity-owner-call-tracing"
                        && scenario.repair_kind == "sharpen_static_limitation_route"
                })
                .map(|scenario| scenario.surface.as_str())
                .collect::<std::collections::BTreeSet<_>>();

        let missing = ["badge", "lsp", "pr_comment", "ci"]
            .into_iter()
            .filter(|surface| !surfaces.contains(surface))
            .collect::<Vec<_>>();

        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "static-limitation backlog user-surface projection is missing consumers: {}",
                missing.join(", ")
            ))
        }
    })
}

#[test]
fn dogfood_user_surface_projection_alignment_matches_surface_projection_source() {
    let sources = vec![super::super::DogfoodSurfaceProjectionAlignmentScenario {
            name: "receipt_improved_top_next_action_alignment".to_string(),
            canonical_gap_id: "gap:boundary-discriminator-004".to_string(),
            packet_id: "boundary-discriminator-004".to_string(),
            evidence_class: "predicate_boundary".to_string(),
            repair_kind: "add_boundary_assertion".to_string(),
            verify_command: "cargo test -p ripr-swarm boundary_discriminator_004".to_string(),
            receipt_command: "cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004".to_string(),
            receipt_state: "receipt_movement_improved".to_string(),
            outcome: "evidence_improved".to_string(),
            expected_top_next_action_kind: "attempt_ready_packet".to_string(),
            advisory_consumers: vec![
                "badge".to_string(),
                "lsp".to_string(),
                "pr_comment".to_string(),
                "ci".to_string(),
            ],
            must_not_change: Vec::new(),
            swarm_plan: serde_json::Value::Null,
            actionable_gap_outcomes: serde_json::Value::Null,
            attempt_ledger: serde_json::Value::Null,
            reason: "unit source fixture".to_string(),
        }];
    let scenario = valid_user_surface_projection_scenario();

    let errors = super::super::user_surface_projection_source_alignment_errors(&scenario, &sources);
    assert!(errors.is_empty(), "{errors:?}");

    let mut mismatched = scenario;
    mismatched.packet_id = "boundary-discriminator-stale".to_string();
    let report =
        super::super::user_surface_projection_source_alignment_errors(&mismatched, &sources)
            .join("\n");
    assert!(report.contains(
        "packet_id must match source_alignment_case receipt_improved_top_next_action_alignment"
    ));

    let mut missing_source = mismatched;
    missing_source.packet_id = "boundary-discriminator-004".to_string();
    missing_source.source_alignment_case = "missing-source-case".to_string();
    let report =
        super::super::user_surface_projection_source_alignment_errors(&missing_source, &sources)
            .join("\n");
    assert!(report.contains(
            "source_alignment_case missing-source-case must exist in surface projection alignment corpus"
        ));
}

#[test]
fn dogfood_generated_ci_cockpit_receipts_are_checked() {
    assert!(
        GENERATED_CI_COCKPIT_COMMAND.contains("init --ci github --dry-run"),
        "receipt must name the workflow producer: {GENERATED_CI_COCKPIT_COMMAND}"
    );
    assert!(
        GENERATED_CI_COCKPIT_COMMAND.contains("reports ci-summary"),
        "receipt must name the summary producer: {GENERATED_CI_COCKPIT_COMMAND}"
    );
    assert!(
        GENERATED_CI_COCKPIT_COMMAND.contains("--base-ref main"),
        "receipt must name the first-run ci-summary stimulus: {GENERATED_CI_COCKPIT_COMMAND}"
    );
    let command = "cargo run --quiet -p ripr -- init --ci github --dry-run";
    let workflow = "\
name: RIPR
jobs:
  ripr:
    continue-on-error: ${{ vars.RIPR_GATE_MODE == '' || vars.RIPR_GATE_MODE == 'visible-only' }}
    steps:
      - name: Add RIPR advisory summary
        if: always()
        continue-on-error: true
        run: |
          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"
      - uses: actions/upload-artifact@v7
        with:
          path: |
            target/ripr/reports
";
    let summary = format!(
        "\
## RIPR advisory summary

RIPR is advisory static evidence. It does not edit source, generate tests, or run mutation testing.

### Start here
- Open `target/ripr/reports/start-here.md` first when it exists.
- Gate authority: `ripr gate evaluate` remains the pass/fail source only when `RIPR_GATE_MODE` is configured.
- {GENERATED_CI_FIRST_ACTION_REPAIR}
- {GENERATED_CI_FIRST_PR_REPAIR}
- {GENERATED_CI_FRONT_PANEL_REPAIR}
- {GENERATED_CI_PACKET_INDEX_REPAIR}

### Language preview grouping
- Grouped preview evidence languages: `javascript typescript`
- Boundary: preview-language groups are advisory presentation only; `ripr gate evaluate` remains pass/fail authority when explicitly configured.
- `typescript`: artifact_entries=`1`, preview_entries=`1`, missing_preview_status=`0`, static_limit_entries=`0`, classifications=`weakly_exposed=1`, static_limit_kinds=`none`, actionability_states=`actionable=1`, actionability_categories=`none`, repair_packet_ready=`1`, gate_impact=`none`
"
    );

    let run = dogfood_generated_ci_cockpit_run_from_surfaces(
        "generated-pr-ci-review-workflow",
        command,
        10,
        workflow,
        &summary,
    );
    assert!(run.errors.is_empty(), "{:?}", run.errors);
    assert!(run.start_here);
    assert_eq!(run.repair_commands, 4);
    assert!(run.gate_authority_boundary);
    assert!(run.default_advisory);
    assert!(run.artifact_upload);
    assert_eq!(run.language_grouping_status, "checked");

    let missing =
        dogfood_generated_ci_cockpit_run_from_surfaces("missing", command, 10, "name: RIPR", "");
    assert!(
        missing
            .errors
            .iter()
            .any(|error| error.contains("Start here"))
    );
    assert!(
        missing
            .errors
            .iter()
            .any(|error| error.contains("regeneration commands"))
    );

    // Removing one ci-summary string must fail the scenario again (#6958).
    let mutated = dogfood_generated_ci_cockpit_run_from_surfaces(
        "missing-start-here-heading",
        command,
        10,
        workflow,
        &summary.replace("### Start here", "### Start elsewhere"),
    );
    assert!(
        mutated
            .errors
            .iter()
            .any(|error| error.contains("Start here")),
        "{:?}",
        mutated.errors
    );
    assert!(!mutated.start_here);

    // Workflow text that still carries the retired shell cannot satisfy the
    // summary properties.
    let legacy = dogfood_generated_ci_cockpit_run_from_surfaces(
        "legacy-workflow-text",
        command,
        10,
        &format!("{workflow}{summary}"),
        "",
    );
    assert!(
        legacy
            .errors
            .iter()
            .any(|error| error.contains("Start here")),
        "{:?}",
        legacy.errors
    );
    assert!(
        legacy
            .errors
            .iter()
            .any(|error| error.contains("regeneration commands")),
        "{:?}",
        legacy.errors
    );

    // Generated CI names the PR range since #4260; the unscoped form it
    // replaced must not satisfy the first-pr repair command.
    let unscoped = summary.replace(
        GENERATED_CI_FIRST_PR_REPAIR,
        "ripr first-pr --root . --gap-ledger target/ripr/reports/gap-decision-ledger.json --first-action target/ripr/reports/first-useful-action.json --review-comments target/ripr/review/comments.json --agent-packet target/ripr/workflow/agent-packet.json --gate-decision target/ripr/reports/gate-decision.json --receipts-dir target/ripr/receipts --out-dir target/ripr/reports",
    );
    let stale = dogfood_generated_ci_cockpit_run_from_surfaces(
        "unscoped-first-pr",
        command,
        10,
        workflow,
        &unscoped,
    );
    assert_eq!(stale.repair_commands, 3);

    let names_elsewhere = summary.replace(
        "- `typescript`: artifact_entries=`1`, preview_entries=`1`, missing_preview_status=`0`, static_limit_entries=`0`, classifications=`weakly_exposed=1`, static_limit_kinds=`none`, actionability_states=`actionable=1`, actionability_categories=`none`, repair_packet_ready=`1`, gate_impact=`none`",
        "- `typescript`: configured preview/advisory; no language findings were emitted in this run; gate_impact=`none`.\nmissing_preview_status static_limit_kinds actionability_states",
    );
    let loose_metrics = dogfood_generated_ci_cockpit_run_from_surfaces(
        "grouping-metrics-not-on-typescript-row",
        command,
        10,
        workflow,
        &names_elsewhere,
    );
    assert_eq!(loose_metrics.language_grouping_status, "missing");

    let rust_only = dogfood_generated_ci_cockpit_run_from_surfaces(
        "rust-only-no-grouping",
        command,
        10,
        workflow,
        &summary.replace("### Language preview grouping", "### No language preview"),
    );
    assert_eq!(rust_only.language_grouping_status, "missing");
    assert!(
        rust_only
            .errors
            .iter()
            .any(|error| error.contains("preview-language grouping")),
        "{:?}",
        rust_only.errors
    );

    let disconnected = dogfood_generated_ci_cockpit_run_from_surfaces(
        "summary-without-workflow-invoke",
        command,
        10,
        &workflow.replace(
            "ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
            "echo skipped",
        ),
        &summary,
    );
    assert!(
        disconnected
            .errors
            .iter()
            .any(|error| error.contains("invoke ripr reports ci-summary")),
        "{:?}",
        disconnected.errors
    );

    let moved_invoke = format!(
        "{}      - name: Echo summary elsewhere\n        run: |\n          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"\n",
        workflow.replace(
            "          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
            "          echo skipped",
        )
    );
    let relocated = dogfood_generated_ci_cockpit_run_from_surfaces(
        "invoke-outside-advisory-summary-step",
        command,
        10,
        &moved_invoke,
        &summary,
    );
    assert!(
        relocated
            .errors
            .iter()
            .any(|error| error.contains("Add RIPR advisory summary step")),
        "{:?}",
        relocated.errors
    );

    let echoed_invoke = workflow.replace(
        "          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
        "          echo 'ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"'",
    );
    let echoed = dogfood_generated_ci_cockpit_run_from_surfaces(
        "echoed-summary-invoke",
        command,
        10,
        &echoed_invoke,
        &summary,
    );
    assert!(
        echoed
            .errors
            .iter()
            .any(|error| error.contains("invoke ripr reports ci-summary")),
        "{:?}",
        echoed.errors
    );

    let commented_invoke = workflow.replace(
        "          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
        "          # ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
    );
    let commented = dogfood_generated_ci_cockpit_run_from_surfaces(
        "commented-summary-invoke",
        command,
        10,
        &commented_invoke,
        &summary,
    );
    assert!(
        commented
            .errors
            .iter()
            .any(|error| error.contains("invoke ripr reports ci-summary")),
        "{:?}",
        commented.errors
    );

    let early_exit_invoke = workflow.replace(
        "          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
        "          exit 0\n          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
    );
    let early_exit = dogfood_generated_ci_cockpit_run_from_surfaces(
        "unreachable-summary-invoke",
        command,
        10,
        &early_exit_invoke,
        &summary,
    );
    assert!(
        early_exit
            .errors
            .iter()
            .any(|error| error.contains("invoke ripr reports ci-summary")),
        "{:?}",
        early_exit.errors
    );

    let early_exit_one_invoke = workflow.replace(
        "          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
        "          exit 1\n          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
    );
    let early_exit_one = dogfood_generated_ci_cockpit_run_from_surfaces(
        "unreachable-summary-invoke-exit-1",
        command,
        10,
        &early_exit_one_invoke,
        &summary,
    );
    assert!(
        early_exit_one
            .errors
            .iter()
            .any(|error| error.contains("invoke ripr reports ci-summary")),
        "{:?}",
        early_exit_one.errors
    );

    let install_guard_invoke = workflow.replace(
        "        run: |\n          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
        "        run: |\n          if [ \"${RIPR_INSTALL_OUTCOME:-}\" != success ]; then\n            echo \"::error::not installed\"; exit 1\n          fi\n          ripr reports ci-summary --root . >> \"$GITHUB_STEP_SUMMARY\"",
    );
    let install_guard = dogfood_generated_ci_cockpit_run_from_surfaces(
        "install-failure-exit-stays-conditional",
        command,
        10,
        &install_guard_invoke,
        &summary,
    );
    assert!(
        install_guard
            .errors
            .iter()
            .all(|error| !error.contains("invoke ripr reports ci-summary")),
        "{:?}",
        install_guard.errors
    );

    let blocking_summary_step = dogfood_generated_ci_cockpit_run_from_surfaces(
        "summary-step-without-continue-on-error",
        command,
        10,
        &workflow.replace(
            "      - name: Add RIPR advisory summary\n        if: always()\n        continue-on-error: true",
            "      - name: Add RIPR advisory summary\n        if: always()\n        continue-on-error: false",
        ),
        &summary,
    );
    assert!(
        blocking_summary_step
            .errors
            .iter()
            .any(|error| error.contains("continue-on-error: true")),
        "{:?}",
        blocking_summary_step.errors
    );

    let blocking_job = workflow.replace(
        "continue-on-error: ${{ vars.RIPR_GATE_MODE == '' || vars.RIPR_GATE_MODE == 'visible-only' }}",
        "continue-on-error: false",
    );
    let blocking = dogfood_generated_ci_cockpit_run_from_surfaces(
        "blocking-job",
        command,
        10,
        &blocking_job,
        &summary,
    );
    assert!(!blocking.default_advisory);
    assert!(
        blocking
            .errors
            .iter()
            .any(|error| error.contains("advisory by default")),
        "{:?}",
        blocking.errors
    );
}

#[test]
fn dogfood_language_preview_scenarios_cover_projection_boundaries() {
    let scenarios = dogfood_language_preview_scenarios();
    let names = scenarios
        .iter()
        .map(|scenario| scenario.name.as_str())
        .collect::<BTreeSet<_>>();

    assert!(names.contains("typescript_mocked_module_limit"));
    assert!(names.contains("javascript_js_preview"));
    assert!(names.contains("python_missing_import_graph_limit"));
    assert!(names.contains("python_mixed_language_no_cross_route"));
    assert!(names.contains("python_disabled"));

    let javascript = scenarios
        .iter()
        .find(|scenario| scenario.name == "javascript_js_preview");
    assert!(
        javascript.is_some_and(|scenario| {
            scenario.preview_enabled
                && scenario.language == "javascript"
                && scenario.expected_preview_findings == 1
                && scenario.expected_related_tests == 1
        }),
        "javascript_js_preview must pin JavaScript preview metadata when TypeScript-family preview is enabled"
    );

    let disabled = scenarios
        .iter()
        .find(|scenario| scenario.name == "python_disabled");
    assert!(
        disabled.is_some_and(|scenario| {
            !scenario.preview_enabled
                && scenario.expected_findings == 0
                && scenario.expected_preview_findings == 0
        }),
        "python_disabled must pin the Rust-default/no-preview boundary"
    );

    let mixed = scenarios
        .iter()
        .find(|scenario| scenario.name == "python_mixed_language_no_cross_route");
    assert!(
        mixed.is_some_and(|scenario| {
            scenario.preview_enabled
                && scenario.expected_related_tests == 0
                && scenario
                    .expected_classifications
                    .contains(&"no_static_path".to_string())
        }),
        "mixed-language receipt must pin no cross-language related-test routing"
    );
}

#[test]
fn dogfood_language_preview_run_checks_static_limit_receipt() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenario = dogfood_language_preview_scenarios()
            .into_iter()
            .find(|scenario| scenario.name == "typescript_mocked_module_limit")
            .ok_or_else(|| {
                "typescript_mocked_module_limit dogfood scenario is missing".to_string()
            })?;

        let run = dogfood_language_preview_run(&scenario);

        assert!(run.errors.is_empty(), "{:?}", run.errors);
        assert_eq!(run.language, "typescript");
        assert!(run.preview_enabled);
        assert_eq!(run.findings, 1);
        assert_eq!(run.language_findings, 1);
        assert_eq!(run.preview_findings, 1);
        assert_eq!(run.missing_preview_status, 0);
        assert_eq!(run.related_tests, 1);
        // The #4103/#4102 relation gates hold a vi.mock'd owner module at
        // weakly_exposed with the mocked_module limit disclosed: the mocked
        // observation cannot witness the real changed sink. The fixture
        // goldens were blessed for exactly this classification.
        assert_eq!(run.classifications, vec!["weakly_exposed".to_string()]);
        assert_eq!(run.static_limit_kinds, vec!["mocked_module".to_string()]);
        assert!(run.json_path.exists());
        assert!(run.human_path.exists());
        Ok(())
    })
}

#[test]
fn dogfood_editor_gap_cockpit_scenarios_cover_fail_closed_states() {
    let scenarios = dogfood_editor_gap_cockpit_scenarios();
    let names = scenarios
        .iter()
        .map(|scenario| scenario.name.as_str())
        .collect::<BTreeSet<_>>();

    assert!(names.contains("rust_actionable"));
    assert!(names.contains("typescript_preview_static_limit"));
    assert!(names.contains("python_preview_static_limit"));
    assert!(names.contains("disabled_language"));
    assert!(names.contains("wrong_root"));
    assert!(names.contains("stale_artifact"));
    assert!(names.contains("no_actionable_gap"));

    for name in [
        "disabled_language",
        "wrong_root",
        "stale_artifact",
        "no_actionable_gap",
    ] {
        let scenario = scenarios.iter().find(|scenario| scenario.name == name);
        assert!(
            scenario.is_some_and(|scenario| {
                scenario.expected_fail_closed
                    && scenario.expected_diagnostics == 0
                    && scenario.expected_actions == vec!["refresh".to_string()]
            }),
            "{name} should fail closed with refresh only"
        );
    }
}

#[test]
fn dogfood_editor_gap_cockpit_run_checks_static_limit_ordering() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenario = dogfood_editor_gap_cockpit_scenarios()
            .into_iter()
            .find(|scenario| scenario.name == "typescript_preview_static_limit")
            .ok_or_else(|| {
                "typescript_preview_static_limit editor gap scenario is missing".to_string()
            })?;

        let run = dogfood_editor_gap_cockpit_run(&scenario);

        assert!(run.errors.is_empty(), "{:?}", run.errors);
        assert_eq!(run.language.as_deref(), Some("typescript"));
        assert_eq!(run.language_status.as_deref(), Some("preview"));
        assert_eq!(run.diagnostics_projected, 1);
        assert_eq!(run.actual_diagnostics, 1);
        assert!(!run.fail_closed);
        assert_eq!(run.static_limit_kind.as_deref(), Some("mocked_module"));
        assert!(run.hover_static_before_action);
        assert!(run.projection_path.exists());
        assert!(run.hover_path.exists());
        Ok(())
    })
}

#[test]
fn dogfood_editor_first_pr_bridge_scenarios_cover_fail_closed_states() {
    let scenarios = dogfood_editor_first_pr_bridge_scenarios();
    let names = scenarios
        .iter()
        .map(|scenario| scenario.name.as_str())
        .collect::<BTreeSet<_>>();

    for expected in [
        "setup_ok",
        "packet_missing",
        "packet_found_repairable",
        "packet_no_action",
        "packet_stale",
        "packet_wrong_root",
        "packet_malformed",
        "receipt_improved_packet_ready",
        "receipt_unchanged_packet_ready",
    ] {
        assert!(names.contains(expected), "{expected} scenario is missing");
    }

    for name in [
        "packet_missing",
        "packet_stale",
        "packet_wrong_root",
        "packet_malformed",
    ] {
        let scenario = scenarios.iter().find(|scenario| scenario.name == name);
        assert!(
            scenario.is_some_and(|scenario| {
                scenario.expected_fail_closed
                    && scenario.expected_diagnostics == 0
                    && scenario.expected_safe_actions
                        == vec!["copy_regeneration_guidance".to_string()]
            }),
            "{name} should fail closed with regeneration guidance only"
        );
    }
}

#[test]
fn dogfood_editor_first_pr_bridge_run_checks_repairable_packet() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenario = dogfood_editor_first_pr_bridge_scenarios()
            .into_iter()
            .find(|scenario| scenario.name == "packet_found_repairable")
            .ok_or_else(|| "packet_found_repairable bridge scenario is missing".to_string())?;

        let run = dogfood_editor_first_pr_bridge_run(&scenario);

        assert!(run.errors.is_empty(), "{:?}", run.errors);
        assert_eq!(run.packet_state, "top_repairable_gap");
        assert_eq!(run.diagnostics, 1);
        assert!(!run.fail_closed);
        assert_eq!(
            run.safe_actions,
            vec![
                "open_packet".to_string(),
                "copy_summary".to_string(),
                "copy_repair_packet".to_string(),
                "copy_verify_command".to_string(),
                "copy_receipt_command".to_string(),
                "copy_regeneration_guidance".to_string()
            ]
        );
        assert_eq!(run.first_pr_actions, run.safe_actions);
        assert!(!run.runtime_adequacy_claim);
        assert!(!run.mutation_proof_claim);
        assert!(!run.policy_gate_claim);
        assert!(!run.pr_ready_claim);
        assert!(run.packet_path.exists());
        assert!(run.diagnosis_path.exists());
        Ok(())
    })
}

#[test]
fn dogfood_editor_first_pr_bridge_run_checks_receipt_movement() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenario = dogfood_editor_first_pr_bridge_scenarios()
            .into_iter()
            .find(|scenario| scenario.name == "receipt_improved_packet_ready")
            .ok_or_else(|| {
                "receipt_improved_packet_ready bridge scenario is missing".to_string()
            })?;

        let run = dogfood_editor_first_pr_bridge_run(&scenario);

        assert!(run.errors.is_empty(), "{:?}", run.errors);
        assert_eq!(run.packet_state, "top_repairable_gap");
        assert_eq!(run.receipt_movement.as_deref(), Some("improved"));
        assert!(!run.pr_ready_claim);
        Ok(())
    })
}

#[test]
fn dogfood_gate_adoption_scenarios_have_checked_receipts() -> Result<(), String> {
    with_repo_cwd(|| {
        for scenario in dogfood_gate_adoption_scenarios() {
            assert!(
                Path::new(scenario.pr_guidance).exists(),
                "{} PR guidance input missing",
                scenario.name
            );
            for input in [
                scenario.labels_json,
                scenario.baseline,
                scenario.recommendation_calibration,
                scenario.mutation_calibration,
            ]
            .into_iter()
            .flatten()
            {
                assert!(
                    Path::new(input).exists(),
                    "{} input missing: {}",
                    scenario.name,
                    input
                );
            }

            let expected_dir = Path::new(scenario.expected_dir);
            let json_text = fs::read_to_string(expected_dir.join("gate-decision.json"))
                .map_err(|err| format!("{} gate-decision.json missing: {err}", scenario.name))?;
            let markdown = fs::read_to_string(expected_dir.join("gate-decision.md"))
                .map_err(|err| format!("{} gate-decision.md missing: {err}", scenario.name))?;
            let value: Value = serde_json::from_str(&json_text)
                .map_err(|err| format!("{} gate-decision.json invalid: {err}", scenario.name))?;

            assert_eq!(
                value.get("status").and_then(Value::as_str),
                Some(scenario.expected_status),
                "{} expected status should be pinned",
                scenario.name
            );
            assert_eq!(
                json_summary_count(&value, "blocking"),
                scenario.expected_blocking,
                "{} expected blocking count should be pinned",
                scenario.name
            );
            assert_eq!(
                json_summary_count(&value, "acknowledged"),
                scenario.expected_acknowledged,
                "{} expected acknowledged count should be pinned",
                scenario.name
            );
            assert_eq!(
                json_summary_count(&value, "advisory"),
                scenario.expected_advisory,
                "{} expected advisory count should be pinned",
                scenario.name
            );
            assert!(
                markdown.contains(&format!("Decision: {}", scenario.expected_status)),
                "{} Markdown should pin decision status",
                scenario.name
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_blocking_gate_report_is_self_contained() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenario = dogfood_gate_adoption_scenarios()
            .into_iter()
            .find(|scenario| scenario.name == "calibrated-high-confidence-new-gap")
            .ok_or_else(|| "calibrated blocking gate scenario is missing".to_string())?;
        let run = dogfood_gate_adoption_run(&scenario)?;
        if !run.errors.is_empty() {
            return Err(format!(
                "calibrated blocking gate dogfood drifted: {:?}",
                run.errors
            ));
        }
        let markdown = fs::read_to_string(&run.markdown_path).map_err(|err| {
            format!(
                "failed to read calibrated gate Markdown {}: {err}",
                normalize_path(&run.markdown_path)
            )
        })?;
        for required in [
            "Decision: blocked",
            "  - Gap state: `actionable`",
            "  - Gap: `gap:",
            "  - Seam: `",
            "  - Classification: `weakly_gripped`",
            "  - Changed owner:",
            "  - Changed behavior:",
            "  - Why it remains open:",
            "  - Near test:",
            "  - Add:",
            "  - Inspect: `ripr agent brief --root . --seam-id",
            "  - Boundary: `static_ripr_evidence_only`",
        ] {
            if !markdown.contains(required) {
                return Err(format!(
                    "blocking gate Markdown is not self-contained; missing {required:?}"
                ));
            }
        }
        // #3906 (F60-14): verify and receipt are labelled as the manual
        // alternative beside a carried repair start (which its after phase
        // follows), or as steps after the test edit without one.
        let (verify, receipt) = if markdown.contains("  - Start repair: `ripr agent repair ") {
            if !markdown.contains("  - After the test edit: run the `--attempt ... --phase after`")
            {
                return Err("blocking gate Markdown lacks the repair after phase".to_string());
            }
            (
                "  - Manual verify without a repair attempt (",
                "  - Manual receipt without a repair attempt (",
            )
        } else {
            (
                "  - Verify after the test edit: `",
                "  - Receipt after verify: `",
            )
        };
        for required in [verify, receipt] {
            if !markdown.contains(required) {
                return Err(format!(
                    "blocking gate Markdown is not self-contained; missing {required:?}"
                ));
            }
        }
        for forbidden in ["gh run download", "pr-guidance"] {
            if markdown.contains(forbidden) {
                return Err(format!(
                    "blocking gate Markdown still requires artifact archaeology: {forbidden:?}"
                ));
            }
        }
        Ok(())
    })
}

#[test]
fn dogfood_first_action_scenarios_have_checked_receipts() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_first_action_scenarios();
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.name == "actionable"),
            "actionable first-action receipt should be checked"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.name == "baseline-only"),
            "baseline-only first-action receipt should be checked"
        );
        assert!(
            scenarios.iter().any(|scenario| scenario.name == "stale")
                || scenarios
                    .iter()
                    .any(|scenario| scenario.name == "missing-required-artifact"),
            "stale or missing-input first-action receipt should be checked"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.name == "unchanged-after-attempt"),
            "unchanged-after-attempt first-action receipt should be checked"
        );
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.name == "no-actionable-seam"),
            "no-actionable-seam first-action receipt should be checked"
        );

        for scenario in scenarios {
            let run = dogfood_first_action_run(&scenario);
            assert!(
                run.rendered,
                "{} should render through ripr first-action",
                run.name
            );
            assert!(
                run.errors.is_empty(),
                "{} first-action render should match its route and goldens: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_first_pr_scenarios_have_checked_receipts() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_first_pr_scenarios();
        for required in [
            ("boundary-gap", "actionable", "top_gap"),
            ("output-contract-gap", "actionable", "top_gap"),
            ("empty-diff", "no_action", "empty_diff"),
            ("blocked-ledger", "blocked", "blocked_artifact"),
        ] {
            assert!(
                scenarios.iter().any(|scenario| {
                    scenario.name == required.0
                        && scenario.expected_status == required.1
                        && scenario.expected_state == required.2
                }),
                "{} first successful PR receipt should be checked as {}/{}",
                required.0,
                required.1,
                required.2
            );
        }

        for scenario in scenarios {
            let run = dogfood_first_pr_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} first successful PR receipt should validate: {:?}",
                run.name,
                run.errors
            );
            if run.expected_status == "actionable" {
                assert!(
                    run.verify_command.is_some(),
                    "{} actionable receipt should name a verify command",
                    run.name
                );
            }
            if run.expected_status == "blocked" {
                assert!(
                    run.next_command.is_some(),
                    "{} blocked receipt should name a next command",
                    run.name
                );
            }
        }

        Ok(())
    })
}

#[test]
fn dogfood_pr_review_front_panel_scenarios_have_checked_receipts() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_pr_review_front_panel_scenarios();
        for required in [
            ("actionable", "actionable"),
            ("acknowledged", "actionable"),
            ("suppressed", "baseline_only"),
            ("baseline_resolved", "already_improved"),
            ("blocked", "actionable"),
            ("missing_proof", "missing_required_input"),
            ("advisory_only", "no_actionable_seam"),
            ("coverage_flat_grip_improved", "already_improved"),
        ] {
            assert!(
                scenarios.iter().any(|scenario| {
                    scenario.name == required.0 && scenario.expected_top_issue_state == required.1
                }),
                "{} front-panel receipt should be checked as {}",
                required.0,
                required.1
            );
        }

        for scenario in scenarios {
            let run = dogfood_pr_review_front_panel_run(&scenario);
            assert!(
                run.errors.is_empty(),
                "{} front-panel receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_report_packet_index_scenarios_have_checked_receipts() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_report_packet_index_scenarios();
        for required in [
            ("complete_packet", "warn"),
            ("sparse_advisory", "warn"),
            ("missing_front_panel", "warn"),
            ("blocked_gate", "fail"),
            ("missing_assistant_proof", "warn"),
            ("missing_receipts", "warn"),
            ("coverage_grip_present", "warn"),
        ] {
            assert!(
                scenarios.iter().any(|scenario| scenario.name == required.0
                    && scenario.expected_status == required.1),
                "{} report-packet index receipt should be checked as {}",
                required.0,
                required.1
            );
        }

        for scenario in scenarios {
            let run = dogfood_report_packet_index_run(&scenario)?;
            assert!(
                run.errors.is_empty(),
                "{} report-packet index receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}

#[test]
fn dogfood_pr_inline_comment_scenarios_have_checked_receipts() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_pr_inline_comment_scenarios();
        for required in [
            ("publishable_changed_line", "plan"),
            ("summary_only_excluded", "plan"),
            ("cap_overflow", "plan"),
            ("dedupe_upsert", "plan"),
            ("stale_existing", "plan"),
            ("fork_or_no_token", "inline"),
            ("missing_input", "plan"),
        ] {
            assert!(
                scenarios
                    .iter()
                    .any(|scenario| scenario.name == required.0
                        && scenario.expected_mode == required.1),
                "{} PR inline comment receipt should be checked in {} mode",
                required.0,
                required.1
            );
        }

        for scenario in scenarios {
            let run = dogfood_pr_inline_comment_run(&scenario)?;
            assert!(
                run.errors.is_empty(),
                "{} PR inline comment receipt should validate: {:?}",
                run.name,
                run.errors
            );
        }

        Ok(())
    })
}
