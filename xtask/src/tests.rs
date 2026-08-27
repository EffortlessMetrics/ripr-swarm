Warning: truncated output (original token count: 453258)
... 764454 bytes omitted ...

//! Tests for the xtask command surface, extracted from `main.rs` (#2119)
//! as the first behavior-preserving decomposition slice: the module
//! body moved verbatim; test names and module path (`crate::tests`) are
//! unchanged.

use std::io::Read;

use crate::acquire_test_cwd_write_guard;
use ripr::output::receipt_lifecycle::{
    RECEIPT_MISSING, RECEIPT_MOVEMENT_IMPROVED, RECEIPT_NOT_APPLICABLE,
};

use super::PrActionableInput;
use super::RiprSwarmAttemptLedgerEntry;
use super::RiprSwarmAttemptLedgerReport;
use super::RiprSwarmCommand;
use super::RiprSwarmReadinessInput;
use super::XtaskCommand;
use super::dispatch;
use super::is_network_policy_candidate;
use super::lane1_runtime_status_full;
use super::policy::droid_review::{
    active_yaml_lines, check_droid_action_refs, check_droid_common,
    check_droid_security_scan_config, forbids_active_line, has_active_line, strip_yaml_comment,
};
use super::ripr_swarm_attempt_ledger_latest_attempts;
use super::ripr_swarm_attempt_ledger_repair_route_quality;
use super::ripr_swarm_repair_route_quality_attempt_is_failure;
use super::ripr_swarm_repair_route_quality_failure_count;
use super::ripr_swarm_repair_route_quality_success_rate;
use super::ripr_swarm_route_quality_from_ledger_value;
use super::ripr_swarm_route_quality_report_json;
use super::run::{
    TimedFileOutput, TimedOutput, capture_output, command_success_owned, run, run_output,
    run_output_optional, run_output_owned,
};
use super::scratch_gc_concurrency_violations;
use super::validate_bless_reason;
use super::{
    BUN_UB_CROSS_LANGUAGE_DOGFOOD_REQUIRED_CASES, BadgeArtifactJob, BadgeBasisReport,
    BadgeBasisSignal, BadgeCanonicalProjection, BadgeCountBreakdown, BadgeEndpointSnapshot,
    BadgeNativeAuditSnapshot, BadgeNativeSlot, Capability, ChangedPath, CheckReport, CheckStatus,
    CheckViolation, CiFullEvidenceGate, CommandCatalogEntry, CwdCommand, DOC_ARTIFACT_LEDGER,
    DogfoodBunUbCrossLanguageScenario, DogfoodEditorFirstPrBridgeRun, DogfoodEditorGapCockpitRun,
    DogfoodFindingAlignmentRun, DogfoodFindingAlignmentScenario, DogfoodFirstActionRun,
    DogfoodFirstPrRun, DogfoodFrontPanelRun, DogfoodGateRun, DogfoodGeneratedCiCockpitRun,
    DogfoodLanguagePreviewRun, DogfoodPrInlineCommentRun, DogfoodPreviewProjectionRuns,
    DogfoodPythonNoActionEvalScenario, DogfoodPythonRealRepoEvalScenario,
    DogfoodPythonStaticLimitEvalScenario, DogfoodRealRepairAttemptScenario, DogfoodReportInputs,
    DogfoodReportPacketIndexRun, DogfoodRun, DogfoodSurfaceProjectionAlignmentScenario,
    DogfoodTypescriptPreviewRepairLoopScenario, DogfoodUserSurfaceProjectionScenario,
    EVIDENCE_QUALITY_SCORECARD_AUDIT_REGENERATION_FAILED,
    EVIDENCE_QUALITY_TREND_PREVIOUS_ARTIFACT_UNAVAILABLE, EvidenceQualityScorecardInput,
    EvidenceQualityScorecardInputs, EvidenceQualityScorecardReport, EvidenceQualityTrendInputs,
    EvidenceQualityTrendReport, FixKind, GENERATED_CI_FIRST_ACTION_REPAIR,
    GENERATED_CI_FIRST_PR_REPAIR, GENERATED_CI_FRONT_PANEL_REPAIR,
    GENERATED_CI_PACKET_INDEX_REPAIR, GhPrStatusPullRequest, GhPrStatusReview,
    Lane1EvidenceAuditRepoExposureGeneration, Lane1EvidenceAuditRepoExposureOutcome,
    LocalContextAllow, LspCockpitFixture, LspCockpitReport, MarkdownLink,
    PYTHON_REAL_REPO_EVAL_REQUIRED_CASES, PYTHON_REAL_REPO_EVAL_REQUIRED_NO_ACTION_CASES,
    PYTHON_REAL_REPO_EVAL_REQUIRED_STATIC_LIMIT_CASES, PrTriageCheck, PrTriageFinding,
    PrTriagePullRequest, REAL_REPAIR_ATTEMPTS_CORPUS, REAL_REPAIR_ATTEMPTS_REQUIRED_CASES,
    REPO_BADGE_ARTIFACT_DEFAULT_TIMEOUT_MS, REPO_BADGE_ARTIFACT_TIMEOUT_ENV,
    REPO_EXPOSURE_SUMMARY_REPORT_DEFAULT_TIMEOUT_MS, REPO_EXPOSURE_SUMMARY_REPORT_TIMEOUT_ENV,
    ReceiptRecord, RepoBadgeArtifactOptions, RepoExposureLatencyReport, RepoExposureLatencyRun,
    RepoExposureLatencyTrace, ReportIndexEntry, ReportIndexRepoOpsArtifact,
    RiprSwarmReadinessNextActionSources, SUPPORT_TIERS_PATH, SarifPolicyMode, SarifPolicyResult,
    SarifPolicyThreshold, StaticLanguageAllowEntry, StaticLanguageMatcher,
    TYPESCRIPT_BUN_UB_CALIBRATION_REQUIRED_CASES,
    TYPESCRIPT_PREVIEW_FALSE_ACTIONABLE_AUDIT_REQUIRED_CASES,
    TYPESCRIPT_PREVIEW_REPAIR_LOOP_REQUIRED_CASES, TestOracleClass,
    USER_SURFACE_PROJECTION_REQUIRED_RUN_STATUSES, USER_SURFACE_PROJECTION_REQUIRED_SURFACES,
    WorktreeDoctorFinding, WorktreeDoctorSeverity, actionable_gap_outcomes_json,
    actionable_gap_outcomes_markdown, actionable_gap_outcomes_report_from_values,
    actionable_gap_outcomes_report_impl, badge_artifact_command_args, badge_artifact_command_label,
    badge_artifact_jobs, badge_artifact_native_slot, badge_artifacts_impl_with_runners,
    badge_artifacts_summary_markdown, badge_basis_canonical_projection,
    badge_basis_derived_ripr_plus_snapshot, badge_basis_needs_repo_badge_plus_job,
    badge_basis_report_json, badge_basis_report_markdown, badge_basis_seam_native_counts,
    badge_diff_policy_violations, badge_native_audit_snapshot, build_lsp_cockpit_report,
    build_repo_exposure_latency_report, build_targeted_test_outcome_report, check_allow_attributes,
    check_badge_diff_policy_with_context, check_doc_artifacts, check_droid_review_config,
    check_executable_files, check_file_policy, check_local_context, check_network_policy,
    check_no_panic_family, check_process_policy, check_static_language, check_support_tiers,
    check_workflows, ci_enforced_xtask_invocations, ci_full_evidence_gates, cockpit_json,
    cockpit_markdown, command_catalog, command_catalog_ci_drift_violations,
    command_catalog_violations, commands_report_json, commands_report_markdown, critic_findings,
    days_from_civil, doc_artifact_kind_matches_path, doc_artifact_violations,
    dogfood_bun_ub_cross_language_run, dogfood_bun_ub_cross_language_scenarios,
    dogfood_class_counts, dogfood_editor_first_pr_bridge_run,
    dogfood_editor_first_pr_bridge_scenarios, dogfood_editor_gap_cockpit_run,
    dogfood_editor_gap_cockpit_scenarios, dogfood_finding_alignment_run,
    dogfood_finding_alignment_scenarios, dogfood_first_action_scenarios, dogfood_first_pr_metrics,
    dogfood_first_pr_run, dogfood_first_pr_scenarios, dogfood_gate_adoption_run,
    dogfood_gate_adoption_scenarios, dogfood_generated_ci_cockpit_run_from_workflow,
    dogfood_language_preview_run, dogfood_language_preview_scenarios,
    dogfood_pr_inline_comment_run, dogfood_pr_inline_comment_scenarios,
    dogfood_pr_review_front_panel_run, dogfood_pr_review_front_panel_scenarios,
    dogfood_push_python_quality_ratio_json, dogfood_python_no_action_eval_run,
    dogfood_python_no_action_eval_scenarios, dogfood_python_real_repo_eval_run,
    dogfood_python_real_repo_eval_scenarios, dogfood_python_repair_routing_quality_summary,
    dogfood_python_static_limit_eval_run, dogfood_python_static_limit_eval_scenarios,
    dogfood_real_repair_attempt_run, dogfood_real_repair_attempt_scenarios, dogfood_report_json,
    dogfood_report_markdown, dogfood_report_packet_index_run,
    dogfood_report_packet_index_scenarios, dogfood_surface_projection_alignment_run,
    dogfood_surface_projection_alignment_scenarios, dogfood_typescript_preview_repair_loop_run,
    dogfood_typescript_preview_repair_loop_scenarios, dogfood_user_surface_projection_run,
    dogfood_user_surface_projection_scenarios, error_ripr_plus_receipt, evidence_health_args,
    evidence_quality_scorecard_audit_regeneration_failure_audit,
    evidence_quality_scorecard_from_values, evidence_quality_scorecard_json,
    evidence_quality_scorecard_markdown, evidence_quality_trend_from_values,
    evidence_quality_trend_json, evidence_quality_trend_markdown,
    evidence_quality_trend_report_impl, extract_json_object_usize_map, extract_json_string,
    extract_json_warnings, extract_workflow_run_blocks, finding_alignment_raw_to_canonical_ratio,
    finding_alignment_verify_command_is_missing, finish_traceability_report,
    finish_worktree_doctor_report, first_line_difference, generated_clean_violations,
    gh_pr_safe_next_action, gh_pr_status_json, gh_pr_status_markdown, gh_pr_status_readiness,
    github_event_pull_request_title_from_text, glob_matches, golden_changes_without_blessing,
    golden_drift_semantics, guarded_allow_attribute_lints, guarded_allow_attributes_in_text,
    help_message, install_hooks_in, is_badge_refresh_context, is_bdd_test_name,
    is_dependency_surface_candidate, is_generated_candidate, is_non_rust_programming_candidate,
    is_public_badge_basis_surface, is_receipt_status, is_ripr_managed_hook, is_snake_case_id,
    is_spec_id, json_escape, json_number_after, json_string_values_for_key, json_summary_count,
    known_commands, known_xtask_command, lane1_actionable_gap_packets_json,
    lane1_actionable_gap_packets_markdown, lane1_evidence_audit_from_repo_exposure,
    lane1_evidence_audit_json, lane1_evidence_audit_limited_report, lane1_evidence_audit_markdown,
    lane1_evidence_audit_repo_exposure_args,
    lane1_evidence_audit_report_from_complete_repo_exposure, lane1_evidence_audit_timeout_error,
    lane1_readiness_packet_specs, limited_badge_artifacts_json, limited_badge_artifacts_markdown,
    line_has_static_language_inline_allow, local_context_line_findings, local_markdown_target,
    lsp_cockpit_report, lsp_cockpit_report_json, lsp_cockpit_report_markdown,
    markdown_links_in_text, mutation_calibration_report_json, mutation_calibration_report_markdown,
    next_checkpoints_from_capabilities, next_spec_id_from_ids,
    non_rust_programming_retention_reason, normalize_fixture_human_output,
    normalize_fixture_json_output, normalize_golden_text, normalize_path,
    parse_actionable_gap_outcomes_args, parse_doc_artifact_ledger_text,
    parse_file_policy_allowlist, parse_gh_pr_status_args, parse_gh_pr_status_pull_request,
    parse_inline_array, parse_mutation_calibration_args, parse_mutation_outcomes_json,
    parse_pr_triage_pull_requests, parse_reason, parse_repo_badge_artifact_options,
    parse_repo_exposure_static_seams, parse_repo_exposure_summary_counts,
    parse_required_status_contexts, parse_ripr_swarm_args, parse_ripr_swarm_plan_args,
    parse_sarif_policy_args, parse_sarif_policy_results, parse_static_language_allowlist,
    parse_targeted_test_outcome_args, pr_actionable_delta_front_panel_from_inputs,
    pr_body_validation_warning, pr_checks_summary, pr_ready_json, pr_ready_markdown,
    pr_ready_next_action, pr_ready_status, pr_ready_status_from_report_status,
    pr_sensitive_file_reason, pr_shape_warnings, pr_summary_body, pr_title_family,
    pr_triage_findings, pr_triage_json, pr_triage_markdown, pr_triage_queue_dispositions,
    precommit_report_body, public_badge_basis_violations, public_contract_rows, read_json_value,
    read_lsp_cockpit_json_value, read_mutation_input_json, read_repo_exposure_summary_artifact,
    receipt_json, receipt_specs, receipt_status_from_reports, repo_badge_artifact_command_args,
    repo_badge_artifact_jobs, repo_badge_artifact_stdout_from_output,
    repo_badge_artifact_timeout_ms_from_env, repo_badge_artifacts_summary_markdown,
    repo_exposure_latency_json, repo_exposure_latency_markdown, repo_exposure_latency_run,
    repo_exposure_latency_run_from_output, repo_exposure_latency_status,
    repo_exposure_latency_trace, repo_exposure_summary_report_timeout_ms_from_env, repo_root,
    repo_seam_inventory_command_args_for_root, report_index_lane1_overall_status,
    report_index_lane1_readiness_packets, report_index_missing_artifact_count,
    report_index_missing_expected, report_index_next_commands, report_index_repo_ops_packets,
    report_index_repo_ops_status, report_status_from_text,
    repository_owned_review_thread_mutation_violations, ripr_command_literals_in_text,
    ripr_debug_binary, ripr_plus_receipt_from_badge, ripr_plus_receipt_from_options,
    ripr_plus_receipt_from_repo_badge_json, ripr_plus_receipt_from_repo_exposure_summary_json,
    ripr_plus_receipt_from_repo_exposure_summary_json_with_source, ripr_plus_receipt_markdown,
    ripr_pre_commit_hook, ripr_swarm_attempt_allowed_file_line,
    ripr_swarm_attempt_dry_run_from_actionable_gaps_value, ripr_swarm_attempt_dry_run_markdown,
    ripr_swarm_attempt_ledger_from_values,
    ripr_swarm_attempt_ledger_from_values_with_real_repair_attempts,
    ripr_swarm_attempt_ledger_json, ripr_swarm_attempt_ledger_markdown,
    ripr_swarm_plan_blocked_packets, ripr_swarm_plan_blocked_report,
    ripr_swarm_plan_from_actionable_gaps_value, ripr_swarm_plan_json, ripr_swarm_plan_markdown,
    ripr_swarm_plan_packet_is_high_confidence, ripr_swarm_plan_ready_packets,
    ripr_swarm_read_optional_json, ripr_swarm_readiness_from_values, ripr_swarm_readiness_json,
    ripr_swarm_readiness_markdown, ripr_swarm_readiness_next_actions, ripr_swarm_readiness_summary,
    routed_rust_workflow_contract_violations,
    routed_rust_workflow_contract_violations_with_reusable, run_ci_full_evidence_gates,
    run_repo_badge_artifact_command, sarif_policy_report_json, sarif_policy_report_markdown,
    select_vscode_test_server, should_scan_static_language_path, should_skip_path,
    sorted_allowlist_content, sorted_capability_blocks_content, sorted_command_catalog_content,
    sorted_markdown_index_table_content, sorted_traceability_behavior_blocks_content,
    spec_id_from_path, spec_ids_in_text, spec_numbering_violations, specs,
    static_language_allowlist_covers, static_language_violation_message, suggested_fixes_patch,
    suspicious_runtime_file_names, targeted_test_outcome, targeted_test_outcome_report_json,
    targeted_test_outcome_report_markdown, test_efficiency_entry, test_efficiency_report_json,
    test_efficiency_report_markdown, test_oracle_report_json, test_oracle_report_markdown,
    test_oracle_tests_in_text, traceability_recommended_fixes, unknown_command_message,
    user_surface_projection_required_run_status_violations,
    validate_actionable_gap_outcomes_fixture_case, validate_actionable_gap_outcomes_fixture_corpus,
    validate_local_context_allowlist, validate_swarm_plan_packet_fixture_case,
    validate_swarm_plan_packet_fixture_corpus, vscode_compile_command, vscode_extension_dir,
    vscode_package_command, vscode_package_version, vscode_test_e2e_command,
    windows_absolute_path_tokens, workflow_bare_self_hosted_violations,
    workflow_review_thread_mutation_violations, workflow_runtime_violations, worktree,
    worktree_doctor_findings, write_badge_artifacts_after_build, write_badge_artifacts_from_diff,
    write_evidence_health_report_with_runner, write_evidence_health_report_with_runners,
    write_lane1_evidence_audit_repo_exposure_with_runner, write_repo_exposure_latency_report,
    write_repo_exposure_summary_report_with_runner,
};
use super::{
    DeclaredIntent, LocalContextFinding, MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT,
    PrReadyStep, TestEfficiencyEntry, TestEfficiencyValue, TestIntentDeclaration, TestIntentKind,
    TestIntentReportSummary, apply_duplicate_discriminator_groups, apply_test_intent_to_entries,
    build_mutation_calibration_report, parse_test_intent_manifest, test_efficiency_metrics,
};
use super::{MutationOutcomeRecord, StaticSeamRecord};
use super::{SarifMissingBaseline, build_sarif_policy_report};
use super::{
    audit_push_value_counts_table_limited, static_limitation_category,
    static_limitation_repair_route,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ripr-xtask-{name}-{stamp}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn write_evidence_promotion_check(fixture: &Path, classification: &str) -> Result<(), String> {
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [{"id": classification, "classification": classification}]
    });
    write_evidence_promotion_check_json(fixture, check_json)
}

fn write_evidence_promotion_check_json(fixture: &Path, check_json: Value) -> Result<(), String> {
    write(
        &fixture.join("expected/check.json"),
        &serde_json::to_string_pretty(&check_json).map_err(|err| err.to_string())?,
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_vacuous_non_promotion_charter() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-non-vacuity");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py-empty");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    write_evidence_promotion_check_json(
        &py_fixture,
        serde_json::json!({
            "summary": {"findings": 0},
            "findings": []
        }),
    )?;
    for fixture in [&ts_fixture, &rust_fixture] {
        write_evidence_promotion_check(fixture, "weakly_exposed")?;
    }
    for fixture in [&rust_control_fixture, &ts_control_fixture] {
        write_evidence_promotion_check(fixture, "exposed")?;
    }

    let guarded_non_promotion = |id: &str, language: &str, source_fixture: &Path| {
        serde_json::json!({
            "id": id,
            "language": language,
            "tier": "pure",
            "source_fixture": source_fixture,
            "assertions": [
                {"type": "must_not_report_clean"},
                {"type": "must_not_promote"}
            ]
        })
    };
    let promoted_control = |id: &str, language: &str, source_fixture: &Path| {
        serde_json::json!({
            "id": id,
            "language": language,
            "tier": "pure",
            "source_fixture": source_fixture,
            "assertions": [{"type": "must_promote"}]
        })
    };
    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_vacuous_non_promotion",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "assertions": [{"type": "must_not_promote"}]
            },
            guarded_non_promotion("ts_guarded", "typescript", &ts_fixture),
            guarded_non_promotion("rust_guarded", "rust", &rust_fixture),
            promoted_control("rust_control", "rust", &rust_control_fixture),
            promoted_control("ts_control", "typescript", &ts_control_fixture)
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");
    assert!(
        report.contains("py_vacuous_non_promotion")
            && report.contains("`must_not_promote` requires `must_not_report_clean`")
            && report.contains("cannot pass vacuously"),
        "expected the manifest-level non-vacuity violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_missing_unknown_and_impure_tiers() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-tier-contract");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_missing_tier",
                "language": "python",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_unknown_tier",
                "language": "typescript",
                "tier": "external-ish",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_pure_with_external_claim",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_fixture,
                "external_repo": "https://github.com/dtolnay/semver",
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(
        report.contains("py_missing_tier") && report.contains("`tier` is required"),
        "expected missing tier violation, got {violations:?}"
    );
    assert!(
        report.contains("ts_unknown_tier") && report.contains("unknown tier `external-ish`"),
        "expected unknown tier violation, got {violations:?}"
    );
    assert!(
        report.contains("rust_pure_with_external_claim")
            && report.contains("tier `pure` must not")
            && report.contains("external_repo"),
        "expected pure-tier external metadata violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_incomplete_pinned_external_tier() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-pinned-external-tier");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_external_incomplete",
                "language": "rust",
                "tier": "pinned_external",
                "source_fixture": rust_fixture,
                "external_commit": "main",
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");

    assert!(
        report.contains("rust_external_incomplete")
            && report.contains("tier `pinned_external`")
            && report.contains("external_repo")
            && report.contains("external_command")
            && report.contains("external_commit")
            && report.contains("external_patch")
            && report.contains("runtime_budget_seconds")
            && report.contains("artifact_budget_bytes"),
        "expected pinned-external metadata violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_accepts_complete_pinned_external_tier() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-pinned-external-complete");
    let corpus = root.join("corpus.json");
    let patch_path = root.join("patches/semver-boundary.diff");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    write(&patch_path, "diff --git a/src/lib.rs b/src/lib.rs\n");
    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_non_promoted",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_fixture,
                "must_remain_non_promoted": true
            },
            {
                "id": "rust_external_complete",
                "language": "rust",
                "tier": "pinned_external",
                "source_fixture": rust_fixture,
                "external_repo": "https://github.com/dtolnay/semver",
                "external_command": "ripr check --root {checkout} --diff {external_patch} --mode fast --json",
                "external_commit": "0123456789abcdef0123456789abcdef01234567",
                "external_patch": patch_path,
                "runtime_budget_seconds": 120,
                "artifact_budget_bytes": 10485760,
                "must_remain_non_promoted": true,
                "expected_max_class": "no_static_path"
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "expected_promoted": true
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "expected_promoted": true
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_accepts_typed_assertion_vocabulary() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-typed-assertions");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_report = root.join("reports/rust-rich.json");
    let packet_report = root.join("reports/rust-packet.json");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    write_evidence_promotion_check(&py_fixture, "weakly_exposed")?;
    write_evidence_promotion_check(&ts_fixture, "weakly_exposed")?;
    write_evidence_promotion_check(&rust_control_fixture, "exposed")?;
    write_evidence_promotion_check(&ts_control_fixture, "exposed")?;
    write(
            &rust_report,
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": "0.2",
                "tool": "ripr",
                "mode": "fast",
                "root": ".",
                "base": "HEAD",
                "analysis_scope": {"completeness": "complete"},
                "summary": {"findings": 1},
                "findings": [
                    {
                        "id": "probe:src_lib.rs:predicate:typed",
                        "classification": "no_static_path",
                        "oracle_kind": "unknown",
                        "oracle_strength": "unknown",
                        "probe": {"file": "src/lib.rs"},
                        "static_limit_kind": "rust_transitive_reach_unresolved",
                        "static_limitation": {
                            "kind": "rust_transitive_reach_unresolved",
                            "last_established_edge": "test `typed_case` (tests/typed.rs:1) -> entry `entry`",
                            "first_unresolved_edge": "entry `entry` -> owner `changed` through a transitive Rust helper path",
                            "analyzer_route": "analysis/rust-public-api-transitive-reach",
                            "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                        },
                        "verify_command": "cargo test typed_case",
                        "evidence": [
                            "For example, the test `typed_case` (tests/typed.rs:1) calls `entry`, an entry point that may lead here.",
                            "limitation_last_established_edge: test `typed_case` (tests/typed.rs:1) -> entry `entry`",
                            "limitation_first_unresolved_edge: entry `entry` -> owner `changed` through a transitive Rust helper path",
                            "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                            "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                        ]
                    }
                ]
            }))
            .map_err(|err| err.to_string())?,
        );
    write(
        &packet_report,
        &serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "0.2",
            "tool": "ripr",
            "mode": "fast",
            "root": ".",
            "base": "HEAD",
            "analysis_scope": {"completeness": "complete"},
            "summary": {"findings": 1},
            "findings": [
                {
                    "id": "gap:typed-packet",
                    "classification": "reachable_unrevealed",
                    "repair_packet_ready": true,
                    "verify_command": "cargo test typed_packet",
                    "receipt_command": "ripr receipt write typed-packet",
                    "repair_packet": {
                        "allowed_edit_surface": ["tests/typed_packet.rs"],
                        "assertion_shape": "assert_eq!(actual, expected)",
                        "authority_boundary": "test",
                        "canonical_gap_id": "gap:typed-packet",
                        "file": "src/lib.rs",
                        "forbidden_files": ["src/lib.rs"],
                        "gap_id": "gap:typed-packet",
                        "language": "rust",
                        "language_status": "stable",
                        "line": 12,
                        "must_not_change": ["Do not edit production code."],
                        "receipt_command": "ripr receipt write typed-packet",
                        "repair_kind": "AddBoundaryAssertion",
                        "target_test": "tests/typed_packet.rs::typed_packet",
                        "verify_command": "cargo test typed_packet"
                    },
                    "preview_actionability": {
                        "raw_evidence_refs": [
                            {
                                "file": "src/lib.rs",
                                "kind": "rust_probe",
                                "line": 12,
                                "source_id": "gap:typed-packet"
                            }
                        ]
                    }
                }
            ]
        }))
        .map_err(|err| err.to_string())?,
    );

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "assertions": [
                    {"type": "must_not_report_clean"},
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "weakly_exposed"}
                ]
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "assertions": [
                    {"type": "must_not_report_clean"},
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "weakly_exposed"}
                ]
            },
            {
                "id": "rust_typed_vocabulary",
                "language": "rust",
                "tier": "pure",
                "source_report": rust_report,
                "assertions": [
                    {"type": "must_not_report_clean"},
                    {"type": "must_disclose_scope"},
                    {"type": "must_see_changed_file", "path": "src/lib.rs"},
                    {
                        "type": "must_emit_limitation",
                        "expected_limit_kind": "rust_transitive_reach_unresolved"
                    },
                    {"type": "must_have_verify_command"},
                    {"type": "must_not_emit_repair_packet"},
                    {"type": "must_disclose_witness"},
                    {"type": "must_disclose_limitation_detail"},
                    {
                        "type": "expected_limitation_detail",
                        "last_established_edge": "test `typed_case` (tests/typed.rs:1) -> entry `entry`",
                        "first_unresolved_edge": "entry `entry` -> owner `changed` through a transitive Rust helper path",
                        "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                    },
                    {
                        "type": "expected_limitation_route",
                        "route": "analysis/rust-public-api-transitive-reach"
                    },
                    {"type": "must_not_claim_no_tests_found"},
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "no_static_path"},
                    {"type": "expected_class", "class": "no_static_path"},
                    {"type": "expected_oracle", "kind": "unknown", "strength": "unknown"},
                    {"type": "expected_completeness", "completeness": "complete"}
                ]
            },
            {
                "id": "rust_packet_commands",
                "language": "rust",
                "tier": "pure",
                "source_report": packet_report,
                "assertions": [
                    {"type": "must_have_verify_command"},
                    {"type": "must_have_receipt_command"},
                    {"type": "must_emit_repair_packet"},
                    {"type": "must_disclose_repair_packet_detail"},
                    {
                        "type": "expected_repair_packet_detail",
                        "canonical_gap_id": "gap:typed-packet",
                        "source_file": "src/lib.rs",
                        "source_line": 12,
                        "target_test": "tests/typed_packet.rs::typed_packet",
                        "assertion_shape": "assert_eq!(actual, expected)",
                        "authority_boundary": "test",
                        "repair_kind": "AddBoundaryAssertion",
                        "verify_command": "cargo test typed_packet",
                        "receipt_command": "ripr receipt write typed-packet",
                        "allowed_edit_surface": ["tests/typed_packet.rs"],
                        "forbidden_files": ["src/lib.rs"]
                    },
                    {"type": "must_not_have_contradictory_packet_messaging"}
                ]
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "assertions": [{"type": "must_promote"}]
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "assertions": [{"type": "must_promote"}]
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    assert!(
        violations.is_empty(),
        "unexpected typed assertion violations: {violations:?}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_duplicate_keys_in_case_object() -> Result<(), String> {
    // issue #2277: a hand-spliced case object with duplicate keys parses under
    // serde_json's last-wins rule and silently drops the earlier case's pin —
    // the gate must fail closed instead of blessing the loss.
    let root = temp_dir("evidence-promotion-honesty-duplicate-keys");
    let corpus = root.join("corpus.json");
    write(
        &corpus,
        r#"{
  "cases": [
    {
      "id": "ts_hoc_wrapped_owner",
      "language": "typescript",
      "tier": "pure",
      "source_fixture": "fixtures/typescript_adversarial_hoc_wrapped_owner",
      "vector": "higher_order_wrapper_obscures_owner_identity",
      "id": "ts_mocked_owner_module_unrelated_assertion",
      "language": "typescript",
      "tier": "pure",
      "source_fixture": "fixtures/typescript_adversarial_owner_module_mock",
      "vector": "mocked_owner_module_strong_oracle_observes_unrelated_sink",
      "assertions": [{ "type": "must_not_promote" }]
    }
  ]
}
"#,
    );

    let mut violations = Vec::new();
    let result = super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations);

    let err = match result {
        Err(err) => err,
        Ok(()) => {
            return Err(format!(
                "duplicate-key corpus must fail closed, got Ok with violations {violations:?}"
            ));
        }
    };
    assert!(
        err.contains("duplicate key `id`"),
        "expected duplicate-key parse failure, got {err}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_honesty_rejects_unknown_assertion_type() -> Result<(), String> {
    let root = temp_dir("evidence-promotion-honesty-unknown-assertion");
    let corpus = root.join("corpus.json");
    let py_fixture = root.join("fixtures/py");
    let ts_fixture = root.join("fixtures/ts");
    let rust_fixture = root.join("fixtures/rust");
    let rust_control_fixture = root.join("fixtures/rust-control");
    let ts_control_fixture = root.join("fixtures/ts-control");

    for (fixture, classification) in [
        (&py_fixture, "weakly_exposed"),
        (&ts_fixture, "weakly_exposed"),
        (&rust_fixture, "no_static_path"),
        (&rust_control_fixture, "exposed"),
        (&ts_control_fixture, "exposed"),
    ] {
        write_evidence_promotion_check(fixture, classification)?;
    }

    let corpus_json = serde_json::json!({
        "cases": [
            {
                "id": "py_non_promoted",
                "language": "python",
                "tier": "pure",
                "source_fixture": py_fixture,
                "assertions": [
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "weakly_exposed"}
                ]
            },
            {
                "id": "ts_non_promoted",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_fixture,
                "assertions": [
                    {"type": "must_not_promote"},
                    {"type": "maximum_class", "class": "weakly_exposed"}
                ]
            },
            {
                "id": "rust_bad_assertion",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_fixture,
                "assertions": [
                    {"type": "must_not_promote"},
                    {"type": "must_guess"}
                ]
            },
            {
                "id": "rust_control",
                "language": "rust",
                "tier": "pure",
                "source_fixture": rust_control_fixture,
                "assertions": [{"type": "must_promote"}]
            },
            {
                "id": "ts_control",
                "language": "typescript",
                "tier": "pure",
                "source_fixture": ts_control_fixture,
                "assertions": [{"type": "must_promote"}]
            }
        ]
    });
    write(
        &corpus,
        &serde_json::to_string_pretty(&corpus_json).map_err(|err| err.to_string())?,
    );

    let mut violations = Vec::new();
    super::validate_evidence_promotion_honesty_corpus_at(&corpus, &mut violations)?;
    let report = violations.join("\n");
    assert!(report.contains("rust_bad_assertion"), "{report}");
    assert!(
        report.contains("unknown assertion type `must_guess`"),
        "{report}"
    );
    Ok(())
}

#[test]
fn evidence_promotion_semantic_assertions_reject_projection_drift() {
    let assertions = vec![
        super::EvidencePromotionSemanticAssertion::MustNotHaveVerifyCommand,
        super::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand,
        super::EvidencePromotionSemanticAssertion::MustEmitRepairPacket,
        super::EvidencePromotionSemanticAssertion::MustNotEmitLimitation,
        super::EvidencePromotionSemanticAssertion::ExpectedCompleteness {
            completeness: "limited".to_string(),
        },
        super::EvidencePromotionSemanticAssertion::ExpectedChangedRustFiles { count: 2 },
    ];
    let check_json = serde_json::json!({
        "analysis_scope": {"completeness": "complete"},
        "summary": {"changed_rust_files": 1, "findings": 1},
        "findings": [
            {
                "id": "projection-drift",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "verify_command": "cargo test",
                "receipt_command": "ripr receipt write projection-drift"
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "projection_drift",
        Some("fixtures/projection_drift"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("must_not_have_verify_command"), "{report}");
    assert!(report.contains("must_not_have_receipt_command"), "{report}");
    assert!(report.contains("must_emit_repair_packet"), "{report}");
    assert!(report.contains("must_not_emit_limitation"), "{report}");
    assert!(report.contains("expected_completeness"), "{report}");
    assert!(report.contains("expected_changed_rust_files"), "{report}");
    assert!(
        report.contains("summary.changed_rust_files `2`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_expected_changed_rust_files() {
    let assertions =
        vec![super::EvidencePromotionSemanticAssertion::ExpectedChangedRustFiles { count: 0 }];
    let check_json = serde_json::json!({
        "summary": {"changed_rust_files": 0, "findings": 0},
        "findings": []
    });

    let violations = super::evidence_promotion_semantic_violations(
        "changed_rust_files",
        Some("fixtures/changed_rust_files"),
        &assertions,
        &check_json,
        None,
        false,
    );
    assert!(
        violations.is_empty(),
        "expected changed Rust file count should pass: {violations:?}"
    );

    let missing_count = serde_json::json!({
        "summary": {"findings": 0},
        "findings": []
    });
    let report = super::evidence_promotion_semantic_violations(
        "missing_changed_rust_files",
        Some("fixtures/changed_rust_files"),
        &assertions,
        &missing_count,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_changed_rust_files"), "{report}");
    assert!(report.contains("<missing>"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_pin_zero_findings() {
    let assertions = vec![
        super::EvidencePromotionSemanticAssertion::MustNotPromote,
        super::EvidencePromotionSemanticAssertion::ExpectedFindingCount { count: 0 },
    ];
    let clean = serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "mode": "fast",
        "root": "fixtures/no_behavior/input",
        "base": "origin/main",
        "summary": {"findings": 0},
        "findings": []
    });
    let violations = super::evidence_promotion_semantic_violations(
        "zero_findings",
        Some("fixtures/no_behavior"),
        &assertions,
        &clean,
        None,
        false,
    );
    assert!(
        violations.is_empty(),
        "an exact zero-finding no-behavior case should be non-vacuous: {violations:?}"
    );

    let regressed = serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "mode": "fast",
        "root": "fixtures/no_behavior/input",
        "base": "origin/main",
        "summary": {"findings": 1},
        "findings": [{"id": "false-exposed", "classification": "exposed"}]
    });
    let report = super::evidence_promotion_semantic_violations(
        "zero_findings",
        Some("fixtures/no_behavior"),
        &assertions,
        &regressed,
        None,
        false,
    )
    .join("\n");
    assert!(report.contains("expected_finding_count"), "{report}");
    assert!(report.contains("promoted to exposed"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_verify_command_projection() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustHaveVerifyCommand];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "verify-human-missing",
                "classification": "weakly_exposed",
                "verify_command": "jest tests/discount.test.ts"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

TypeScript repair packet (advisory)
  status: packet-ready
";

    let report = super::evidence_promotion_semantic_violations(
        "verify_human_missing",
        Some("fixtures/verify_human_missing"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_have_verify_command"), "{report}");
    assert!(
        report.contains("missing verify_command `jest tests/discount.test.ts`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_invented_verify_command() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustNotHaveVerifyCommand];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "verify-human-invented",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Static limitation
  rust_transitive_reach_unresolved
  verify: cargo test transitive_path
";

    let report = super::evidence_promotion_semantic_violations(
        "verify_human_invented",
        Some("fixtures/verify_human_invented"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_not_have_verify_command"), "{report}");
    assert!(report.contains("cargo test transitive_path"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_invented_receipt_command() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "receipt-human-invented",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Static limitation
  rust_transitive_reach_unresolved
  receipt: ripr receipt write receipt-human-invented
";

    let report = super::evidence_promotion_semantic_violations(
        "receipt_human_invented",
        Some("fixtures/receipt_human_invented"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_not_have_receipt_command"), "{report}");
    assert!(
        report.contains("ripr receipt write receipt-human-invented"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_unavailable_human_receipt_status() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustNotHaveReceiptCommand];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "receipt-human-unavailable",
                "classification": "weakly_exposed"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Python repair card (preview/advisory)
  receipt: unavailable_until_python_gap_ledger
";

    let report = super::evidence_promotion_semantic_violations(
        "receipt_human_unavailable",
        Some("fixtures/receipt_human_unavailable"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(report.is_empty(), "{report:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_oracle_drift() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "smoke_only".to_string(),
        strength: "smoke".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:oracle-drift",
                "classification": "weakly_exposed",
                "oracle_kind": "exact_value",
                "oracle_strength": "strong"
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "oracle_drift",
        Some("fixtures/typescript_oracle_drift"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_oracle"), "{report}");
    assert!(report.contains("oracle_kind `exact_value`"), "{report}");
    assert!(report.contains("oracle_strength `strong`"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_oracle_projection() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "exact_value".to_string(),
        strength: "strong".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:oracle-human-drift",
                "classification": "exposed",
                "oracle_kind": "exact_value",
                "oracle_strength": "strong"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Evidence
  - related test tests/score.test.ts:3 reaches score
";

    let report = super::evidence_promotion_semantic_violations(
        "oracle_human_drift",
        Some("fixtures/typescript_oracle_human_drift"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("expected_oracle"), "{report}");
    assert!(report.contains("fixture human output"), "{report}");
    assert!(
        report.contains("oracle projection `exact_value/strong`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_human_oracle_projection() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "exact_value".to_string(),
        strength: "strong".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:oracle-human-projection",
                "classification": "exposed",
                "oracle_kind": "exact_value",
                "oracle_strength": "strong"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

TypeScript preview
  oracle: exact_value (strong)
";

    let report = super::evidence_promotion_semantic_violations(
        "oracle_human_projection",
        Some("fixtures/typescript_oracle_human_projection"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(report.is_empty(), "{report:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_human_oracle_golden() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "exact_value".to_string(),
        strength: "strong".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:oracle-missing-human",
                "classification": "exposed",
                "oracle_kind": "exact_value",
                "oracle_strength": "strong"
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "oracle_missing_human",
        Some("fixtures/typescript_oracle_missing_human"),
        &assertions,
        &check_json,
        None,
        true,
    )
    .join("\n");

    assert!(report.contains("expected_oracle"), "{report}");
    assert!(report.contains("expected/human-full.txt"), "{report}");
}

#[test]
fn evidence_promotion_human_oracle_line_matches_normalized_projection() {
    assert!(super::evidence_promotion_human_oracle_line_matches(
        "  oracle: exact_value (strong)",
        "exact_value",
        "strong"
    ));
    assert!(super::evidence_promotion_human_oracle_line_matches(
        "current test evidence: oracle_strength : strong; oracle_kind = exact_value.",
        "exact_value",
        "strong"
    ));
    assert!(super::evidence_promotion_human_oracle_line_matches(
        "current test evidence: oracle_kind=exact_value, oracle_strength=strong",
        "exact_value",
        "strong"
    ));
    assert!(!super::evidence_promotion_human_oracle_line_matches(
        "Strongest extracted oracle kind: `exact_value` (rank 5)",
        "exact_value",
        "strong"
    ));
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_class_projection() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::ExpectedClass {
        class: "weakly_exposed".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:class-human-drift",
                "classification": "weakly_exposed"
            }
        ]
    });
    let human_text = "\
RIPR static exposure report

Static exposure
  exposed (info, confidence 0.60)
";

    let report = super::evidence_promotion_semantic_violations(
        "class_human_drift",
        Some("fixtures/typescript_class_human_drift"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("expected_class"), "{report}");
    assert!(
        report.contains("project class `weakly_exposed`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_human_class_golden() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::ExpectedClass {
        class: "weakly_exposed".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:typescript:class-missing-human",
                "classification": "weakly_exposed"
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "class_missing_human",
        Some("fixtures/typescript_class_missing_human"),
        &assertions,
        &check_json,
        None,
        true,
    )
    .join("\n");

    assert!(report.contains("expected_class"), "{report}");
    assert!(report.contains("expected/human-full.txt"), "{report}");
}

#[test]
fn evidence_promotion_human_class_line_matches_exact_class_token() {
    assert!(super::evidence_promotion_human_class_line_matches(
        "  weakly_exposed (warning, confidence 0.40)",
        "weakly_exposed"
    ));
    assert!(super::evidence_promotion_human_class_line_matches(
        "  exposed (info, confidence 0.60)",
        "exposed"
    ));
    assert!(!super::evidence_promotion_human_class_line_matches(
        "  weakly_exposed (warning, confidence 0.40)",
        "exposed"
    ));
}

#[test]
fn evidence_promotion_semantic_assertions_reject_expected_oracle_without_findings() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::ExpectedOracle {
        kind: "exact_value".to_string(),
        strength: "strong".to_string(),
    }];
    let check_json = serde_json::json!({
        "summary": {"findings": 0},
        "findings": []
    });

    let report = super::evidence_promotion_semantic_violations(
        "oracle_empty",
        Some("fixtures/typescript_oracle_empty"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_oracle"), "{report}");
    assert!(report.contains("requires at least one finding"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_receipt_command() {
    let assertions = vec![
        super::EvidencePromotionSemanticAssertion::MustHaveReceiptCommand,
        super::EvidencePromotionSemanticAssertion::MustEmitRepairPacket,
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-missing-receipt",
                "classification": "reachable_unrevealed",
                "repair_packet_ready": true,
                "verify_command": "cargo test packet_missing_receipt"
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "packet_missing_receipt",
        Some("fixtures/packet_missing_receipt"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("must_have_receipt_command"), "{report}");
    assert!(
        report.contains("requires a non-empty receipt_command"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_repair_packet_detail() {
    let assertions =
        vec![super::EvidencePromotionSemanticAssertion::MustDiscloseRepairPacketDetail];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-missing-detail",
                "classification": "weakly_exposed",
                "repair_packet_ready": true,
                "typescript_repair_packet": {
                    "allowed_edit_surface": ["tests/discount.test.ts"],
                    "assertion_shape": "expect(result).toBe(50)",
                    "authority_boundary": "preview_advisory_only",
                    "canonical_gap_id": "gap:typescript:discount",
                    "file": "src/discount.ts",
                    "forbidden_files": ["src/discount.ts"],
                    "gap_id": "probe:discount",
                    "language": "typescript",
                    "language_status": "preview",
                    "line": 2,
                    "must_not_change": ["Do not edit production code."],
                    "receipt_command": "ripr outcome --before baseline --after repair",
                    "repair_kind": "AddBoundaryAssertion",
                    "verify_command": "jest tests/discount.test.ts"
                }
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "packet_missing_detail",
        Some("fixtures/packet_missing_detail"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(
        report.contains("must_disclose_repair_packet_detail"),
        "{report}"
    );
    assert!(
        report.contains("target_test:missing target test"),
        "{report}"
    );
    assert!(
        report.contains("$.raw_evidence_refs:missing raw evidence refs"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_wrong_repair_packet_detail() {
    let assertions = vec![
        super::EvidencePromotionSemanticAssertion::ExpectedRepairPacketDetail {
            detail: super::ExpectedRepairPacketDetail {
                canonical_gap_id: "gap:typescript:discount".to_string(),
                source_file: "src/discount.ts".to_string(),
                source_line: 2,
                target_test: "tests/discount.test.ts::discount boundary".to_string(),
                assertion_shape: "expect(result).toBe(50)".to_string(),
                authority_boundary: "preview_advisory_only".to_string(),
                repair_kind: "AddBoundaryAssertion".to_string(),
                verify_command: "jest tests/discount.test.ts".to_string(),
                receipt_command: "ripr outcome --before baseline --after repair".to_string(),
                allowed_edit_surface: vec!["tests/discount.test.ts".to_string()],
                forbidden_files: vec!["src/discount.ts".to_string()],
            },
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-wrong-detail",
                "classification": "weakly_exposed",
                "repair_packet_ready": true,
                "typescript_repair_packet": {
                    "allowed_edit_surface": ["tests/discount.test.ts"],
                    "assertion_shape": "expect(result).toBe(50)",
                    "authority_boundary": "preview_advisory_only",
                    "canonical_gap_id": "gap:typescript:discount",
                    "file": "src/discount.ts",
                    "forbidden_files": ["src/discount.ts"],
                    "gap_id": "probe:discount",
                    "language": "typescript",
                    "language_status": "preview",
                    "line": 2,
                    "must_not_change": ["Do not edit production code."],
                    "receipt_command": "ripr outcome --before baseline --after repair",
                    "repair_kind": "AddBoundaryAssertion",
                    "target_test": "tests/wrong.test.ts::discount boundary",
                    "verify_command": "jest tests/wrong.test.ts"
                }
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "packet_wrong_detail",
        Some("fixtures/packet_wrong_detail"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_repair_packet_detail"), "{report}");
    assert!(
            report.contains(
                "target_test:expected `tests/discount.test.ts::discount boundary` got `tests/wrong.test.ts::discount boundary`"
            ),
            "{report}"
        );
    assert!(
        report.contains(
            "verify_command:expected `jest tests/discount.test.ts` got `jest tests/wrong.test.ts`"
        ),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_contradictory_packet_messaging() {
    let assertions =
        vec![super::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-contradictory-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": true,
                    "gap_state": "actionable",
                    "actionability_category": "complete_repair_packet"
                },
                "typescript_repair_packet": {
                    "canonical_gap_id": "gap:typescript:discount",
                    "verify_command": "jest tests/discount.test.ts",
                    "receipt_command": "ripr outcome --before baseline --after repair"
                },
                "evidence": [
                    "owner: applyDiscount",
                    "gap_state: advisory",
                    "actionability_category: incomplete_repair_packet",
                    "why_not_actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract",
                    "repair_route: project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available",
                    "missing_actionability_fields: canonical_gap_id",
                    "evidence_needed_to_promote: canonical gap identity"
                ]
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "packet_contradictory_messaging",
        Some("fixtures/packet_contradictory_messaging"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(
        report.contains("must_not_have_contradictory_packet_messaging"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence[1]:blocked gap_state evidence"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence[3]:blocked why-not-actionable evidence"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_contradictory_packet_messaging() {
    let assertions =
        vec![super::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-human-contradictory-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": true,
                    "gap_state": "actionable",
                    "actionability_category": "complete_repair_packet"
                },
                "typescript_repair_packet": {
                    "canonical_gap_id": "gap:typescript:discount",
                    "verify_command": "jest tests/discount.test.ts",
                    "receipt_command": "ripr outcome --before baseline --after repair"
                },
                "evidence": [
                    "owner: applyDiscount",
                    "gap_state: actionable",
                    "actionability_category: complete_repair_packet"
                ]
            }
        ]
    });
    let human_text = "\
TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  status: not actionable
  why not actionable: TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract
  missing fields: verify_command, receipt_command
";

    let report = super::evidence_promotion_semantic_violations(
        "packet_human_contradictory_messaging",
        Some("fixtures/packet_human_contradictory_messaging"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(
        report.contains("must_not_have_contradictory_packet_messaging"),
        "{report}"
    );
    assert!(
        report.contains(
            "expected/human-full.txt:status: not actionable:blocked not-actionable status"
        ),
        "{report}"
    );
    assert!(
            report.contains("expected/human-full.txt:missing fields: verify_command, receipt_command:blocked missing-fields line"),
            "{report}"
        );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_human_complete_packet_messaging() {
    let assertions =
        vec![super::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-human-complete-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": true,
                    "gap_state": "actionable",
                    "actionability_category": "complete_repair_packet"
                },
                "typescript_repair_packet": {
                    "canonical_gap_id": "gap:typescript:discount",
                    "verify_command": "jest tests/discount.test.ts",
                    "receipt_command": "ripr outcome --before baseline --after repair"
                },
                "evidence": [
                    "owner: applyDiscount",
                    "gap_state: actionable",
                    "actionability_category: complete_repair_packet",
                    "why_actionable: complete repair packet"
                ]
            }
        ]
    });
    let human_text = "\
TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  source: applyDiscount at src/discount.ts:2
  related test: tests/discount.test.ts::discount
  oracle: expect(result).toBe(50)
  edit surface: tests/discount.test.ts
  verify: jest tests/discount.test.ts
  receipt: ripr outcome --before baseline --after repair
  why actionable: complete repair packet
  authority: preview_advisory_only
";

    let report = super::evidence_promotion_semantic_violations(
        "packet_human_complete_messaging",
        Some("fixtures/packet_human_complete_messaging"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(report.is_empty(), "{report:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_accept_human_mixed_packet_and_blocked_messaging() {
    let assertions =
        vec![super::EvidencePromotionSemanticAssertion::MustNotHaveContradictoryPacketMessaging];
    let check_json = serde_json::json!({
        "summary": {"findings": 2},
        "findings": [
            {
                "id": "packet-human-complete-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": true,
                    "gap_state": "actionable",
                    "actionability_category": "complete_repair_packet"
                },
                "typescript_repair_packet": {
                    "canonical_gap_id": "gap:typescript:discount",
                    "verify_command": "jest tests/discount.test.ts",
                    "receipt_command": "ripr outcome --before baseline --after repair"
                },
                "evidence": [
                    "owner: applyDiscount",
                    "gap_state: actionable",
                    "actionability_category: complete_repair_packet",
                    "why_actionable: complete repair packet"
                ]
            },
            {
                "id": "packet-human-blocked-messaging",
                "classification": "weakly_exposed",
                "preview_actionability": {
                    "repair_packet_ready": false,
                    "gap_state": "advisory",
                    "actionability_category": "incomplete_repair_packet"
                },
                "evidence": [
                    "owner: shippingTotal",
                    "gap_state: advisory",
                    "actionability_category: incomplete_repair_packet",
                    "why_not_actionable: TypeScript preview lacks a complete repair packet contract"
                ]
            }
        ]
    });
    let human_text = "\
TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  source: applyDiscount at src/discount.ts:2
  related test: tests/discount.test.ts::discount
  oracle: expect(result).toBe(50)
  edit surface: tests/discount.test.ts
  verify: jest tests/discount.test.ts
  receipt: ripr outcome --before baseline --after repair
  why actionable: complete repair packet
  authority: preview_advisory_only

TypeScript repair packet (advisory)
  canonical gap: gap:typescript:shipping
  status: not actionable
  why not actionable: TypeScript preview lacks a complete repair packet contract
  missing fields: verify_command, receipt_command
";

    let report = super::evidence_promotion_semantic_violations(
        "packet_human_mixed_messaging",
        Some("fixtures/packet_human_mixed_messaging"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(report.is_empty(), "{report:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_repair_packet_detail() {
    let assertions =
        vec![super::EvidencePromotionSemanticAssertion::MustDiscloseRepairPacketDetail];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-human-missing-detail",
                "classification": "weakly_exposed",
                "repair_packet_ready": true,
                "typescript_repair_packet": {
                    "allowed_edit_surface": ["tests/discount.test.ts"],
                    "assertion_shape": "expect(result).toBe(50)",
                    "authority_boundary": "preview_advisory_only",
                    "canonical_gap_id": "gap:typescript:discount",
                    "file": "src/discount.ts",
                    "forbidden_files": ["src/discount.ts"],
                    "gap_id": "probe:discount",
                    "language": "typescript",
                    "language_status": "preview",
                    "line": 2,
                    "must_not_change": ["Do not edit production code."],
                    "receipt_command": "ripr outcome --before baseline --after repair",
                    "repair_kind": "AddBoundaryAssertion",
                    "target_test": "tests/discount.test.ts::discount",
                    "verify_command": "jest tests/discount.test.ts"
                },
                "preview_actionability": {
                    "raw_evidence_refs": [
                        {
                            "file": "src/discount.ts",
                            "kind": "typescript_preview_probe",
                            "line": 2,
                            "source_id": "probe:discount"
                        }
                    ]
                }
            }
        ]
    });
    let human_text = "\
TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  source: applyDiscount at src/discount.ts:2
  related test: tests/discount.test.ts::discount
  oracle: expect(result).toBe(50)
  edit surface: tests/discount.test.ts
  verify: jest tests/discount.test.ts
  must not change:
    - Do not edit production code.
  authority: preview_advisory_only
";

    let report = super::evidence_promotion_semantic_violations(
        "packet_human_missing_detail",
        Some("fixtures/packet_human_missing_detail"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(
        report.contains("must_disclose_repair_packet_detail"),
        "{report}"
    );
    assert!(
        report.contains("expected/human-full.txt:missing receipt command `receipt:`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_wrong_repair_packet_detail() {
    let assertions = vec![
        super::EvidencePromotionSemanticAssertion::ExpectedRepairPacketDetail {
            detail: super::ExpectedRepairPacketDetail {
                canonical_gap_id: "gap:typescript:discount".to_string(),
                source_file: "src/discount.ts".to_string(),
                source_line: 2,
                target_test: "tests/discount.test.ts::discount boundary".to_string(),
                assertion_shape: "expect(result).toBe(50)".to_string(),
                authority_boundary: "preview_advisory_only".to_string(),
                repair_kind: "AddBoundaryAssertion".to_string(),
                verify_command: "jest tests/discount.test.ts".to_string(),
                receipt_command: "ripr outcome --before baseline --after repair".to_string(),
                allowed_edit_surface: vec!["tests/discount.test.ts".to_string()],
                forbidden_files: vec!["src/discount.ts".to_string()],
            },
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "packet-human-wrong-detail",
                "classification": "weakly_exposed",
                "repair_packet_ready": true,
                "typescript_repair_packet": {
                    "allowed_edit_surface": ["tests/discount.test.ts"],
                    "assertion_shape": "expect(result).toBe(50)",
                    "authority_boundary": "preview_advisory_only",
                    "canonical_gap_id": "gap:typescript:discount",
                    "file": "src/discount.ts",
                    "forbidden_files": ["src/discount.ts"],
                    "gap_id": "probe:discount",
                    "language": "typescript",
                    "language_status": "preview",
                    "line": 2,
                    "must_not_change": ["Do not edit production code."],
                    "receipt_command": "ripr outcome --before baseline --after repair",
                    "repair_kind": "AddBoundaryAssertion",
                    "target_test": "tests/discount.test.ts::discount boundary",
                    "verify_command": "jest tests/discount.test.ts"
                }
            }
        ]
    });
    let human_text = "\
Preview actionability
  related test: tests/discount.test.ts::discount boundary
  verify: jest tests/discount.test.ts
  authority: preview_advisory_only

TypeScript repair packet (advisory)
  canonical gap: gap:typescript:discount
  source: applyDiscount at src/discount.ts:2
  related test: tests/wrong.test.ts::discount boundary
  oracle: expect(result).toBe(50)
  edit surface: tests/discount.test.ts
  verify: jest tests/wrong.test.ts
  receipt: ripr outcome --before baseline --after repair
  must not change:
    - Do not edit production code.
  authority: preview_advisory_only
";

    let report = super::evidence_promotion_semantic_violations(
        "packet_human_wrong_detail",
        Some("fixtures/packet_human_wrong_detail"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("expected_repair_packet_detail"), "{report}");
    assert!(
            report.contains(
                "expected/human-full.txt:missing target test `tests/discount.test.ts::discount boundary`"
            ),
            "{report}"
        );
    assert!(
        report.contains(
            "expected/human-full.txt:missing verify command `jest tests/discount.test.ts`"
        ),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_no_tests_claim_with_witness() {
    let assertions = vec![
        super::EvidencePromotionSemanticAssertion::MustDiscloseWitness,
        super::EvidencePromotionSemanticAssertion::MustNotClaimNoTestsFound,
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "ripr": {
                    "infect": {
                        "summary": "No tests were found, so activation/infection cannot be estimated"
                    }
                },
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here.",
                    "No tests were found, so activation/infection cannot be estimated"
                ]
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "witnessed_no_tests_claim",
        Some("fixtures/witnessed_no_tests_claim"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("must_not_claim_no_tests_found"), "{report}");
    assert!(report.contains("$.findings[0].evidence[1]"), "{report}");
    assert!(
        report.contains("$.findings[0].ripr.infect.summary"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_witness_projection() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });
    let human_text = "Static limitation\nNo statically reachable test path was found\n";

    let report = super::evidence_promotion_semantic_violations(
        "witnessed_human_missing_projection",
        Some("fixtures/witnessed_human_missing_projection"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_disclose_witness"), "{report}");
    assert!(
        report.contains("expected/human-full.txt:missing Where to look"),
        "{report}"
    );
    assert!(
        report.contains(
            "expected/human-full.txt:missing witness `For example, the test `integration_path`"
        ),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_mismatched_witness_projection() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });
    let human_text = concat!(
        "Where to look\n",
        "  For example, the test `stale_path` (tests/it.rs:9) calls `outer`, an entry point that may lead here.\n",
    );

    let report = super::evidence_promotion_semantic_violations(
        "witnessed_human_mismatched_projection",
        Some("fixtures/witnessed_human_mismatched_projection"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_disclose_witness"), "{report}");
    assert!(
        report.contains(
            "expected/human-full.txt:missing witness `For example, the test `integration_path`"
        ),
        "{report}"
    );
    assert!(!report.contains("missing Where to look"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_human_witness_golden() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustDiscloseWitness];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "witnessed_missing_human_golden",
        Some("fixtures/witnessed_missing_human_golden"),
        &assertions,
        &check_json,
        None,
        true,
    )
    .join("\n");

    assert!(report.contains("must_disclose_witness"), "{report}");
    assert!(
        report.contains("requires fixture human output at `expected/human-full.txt`"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_no_tests_claim_with_witness() {
    let assertions = vec![
        super::EvidencePromotionSemanticAssertion::MustDiscloseWitness,
        super::EvidencePromotionSemanticAssertion::MustNotClaimNoTestsFound,
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_lib.rs:predicate:witnessed",
                "classification": "no_static_path",
                "ripr": {
                    "infect": {
                        "summary": "No statically reachable test path was found, so activation/infection cannot be estimated"
                    }
                },
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });
    let human_text = concat!(
        "Where to look\n",
        "  For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here.\n",
        "No tests were found, so activation/infection cannot be estimated\n",
    );

    let report = super::evidence_promotion_semantic_violations(
        "witnessed_human_no_tests_claim",
        Some("fixtures/witnessed_human_no_tests_claim"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(report.contains("must_not_claim_no_tests_found"), "{report}");
    assert!(report.contains("expected/human-full.txt:3"), "{report}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_missing_limitation_detail() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "For example, the test `integration_path` (tests/it.rs:4) calls `outer`, an entry point that may lead here."
                ]
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "missing_limitation_detail",
        Some("fixtures/missing_limitation_detail"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(
        report.contains("must_disclose_limitation_detail"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence:missing last established edge"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence:missing first unresolved edge"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence:missing analyzer route"),
        "{report}"
    );
    assert!(
        report.contains("$.findings[0].evidence:missing non-claim"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_human_missing_limitation_detail() {
    let assertions = vec![super::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "limitation_last_established_edge: test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "limitation_first_unresolved_edge: entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ],
                "static_limitation": {
                    "kind": "rust_transitive_reach_unresolved",
                    "last_established_edge": "test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "first_unresolved_edge": "entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "analyzer_route": "analysis/rust-public-api-transitive-reach",
                    "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                }
            }
        ]
    });
    let human_text = "Static limitation\n  rust_transitive_reach_unresolved\n";

    let report = super::evidence_promotion_semantic_violations(
        "human_missing_limitation_detail",
        Some("fixtures/human_missing_limitation_detail"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    )
    .join("\n");

    assert!(
        report.contains("must_disclose_limitation_detail"),
        "{report}"
    );
    assert!(
        report.contains("expected/human-full.txt:missing Limitation detail"),
        "{report}"
    );
    assert!(
        report.contains("expected/human-full.txt:missing detail `last established edge:"),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_accept_limitation_detail_projection() {
    let assertions = vec![
            super::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail,
            super::EvidencePromotionSemanticAssertion::ExpectedLimitationDetail {
                last_established_edge:
                    "test `integration_path` (tests/it.rs:4) -> entry `outer`".to_string(),
                first_unresolved_edge:
                    "entry `outer` -> owner `inner` through a transitive Rust helper path"
                        .to_string(),
                non_claim:
                    "named limitation only; ripr cannot confirm or deny that this path observes the change"
                        .to_string(),
            },
            super::EvidencePromotionSemanticAssertion::ExpectedLimitationRoute {
                route: "analysis/rust-public-api-transitive-reach".to_string(),
            },
        ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "limitation_last_established_edge: test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "limitation_first_unresolved_edge: entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ],
                "static_limitation": {
                    "kind": "rust_transitive_reach_unresolved",
                    "last_established_edge": "test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "first_unresolved_edge": "entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "analyzer_route": "analysis/rust-public-api-transitive-reach",
                    "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                }
            }
        ]
    });
    let human_text = concat!(
        "Limitation detail\n",
        "  last established edge: test `integration_path` (tests/it.rs:4) -> entry `outer`\n",
        "  first unresolved edge: entry `outer` -> owner `inner` through a transitive Rust helper path\n",
        "  analyzer route: analysis/rust-public-api-transitive-reach\n",
        "  non-claim: named limitation only; ripr cannot confirm or deny that this path observes the change\n",
    );

    let violations = super::evidence_promotion_semantic_violations(
        "limitation_detail_projection",
        Some("fixtures/limitation_detail_projection"),
        &assertions,
        &check_json,
        Some(human_text),
        true,
    );

    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn evidence_promotion_semantic_assertions_reject_wrong_limitation_detail() {
    let assertions = vec![
            super::EvidencePromotionSemanticAssertion::ExpectedLimitationDetail {
                last_established_edge:
                    "test `integration_path` (tests/it.rs:4) -> entry `outer`".to_string(),
                first_unresolved_edge:
                    "entry `outer` -> owner `inner` through a transitive Rust helper path"
                        .to_string(),
                non_claim:
                    "named limitation only; ripr cannot confirm or deny that this path observes the change"
                        .to_string(),
            },
        ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "limitation_last_established_edge: test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "limitation_first_unresolved_edge: entry `other` -> owner `inner` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ]
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "wrong_limitation_detail",
        Some("fixtures/wrong_limitation_detail"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_limitation_detail"), "{report}");
    assert!(
        report.contains(
            "$.findings[0].evidence:first unresolved edge:entry `other` -> owner `inner`"
        ),
        "{report}"
    );
}

#[test]
fn evidence_promotion_semantic_assertions_reject_wrong_limitation_route() {
    let assertions = vec![
        super::EvidencePromotionSemanticAssertion::ExpectedLimitationRoute {
            route: "analysis/rust-public-api-transitive-reach".to_string(),
        },
    ];
    let check_json = serde_json::json!({
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_internal.rs:predicate:inner",
                "classification": "no_static_path",
                "static_limit_kind": "rust_transitive_reach_unresolved",
                "evidence": [
                    "limitation_last_established_edge: test `integration_path` (tests/it.rs:4) -> entry `outer`",
                    "limitation_first_unresolved_edge: entry `outer` -> owner `inner` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/generic-static-limitation",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ]
            }
        ]
    });

    let report = super::evidence_promotion_semantic_violations(
        "wrong_limitation_route",
        Some("fixtures/wrong_limitation_route"),
        &assertions,
        &check_json,
        None,
        false,
    )
    .join("\n");

    assert!(report.contains("expected_limitation_route"), "{report}");
    assert!(
        report.contains("$.findings[0].evidence:analysis/generic-static-limitation"),
        "{report}"
    );
}

#[test]
fn evidence_promot…212152 tokens truncated…  .parent()
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

fn latency_run_with_status(format: &str, status: &str) -> RepoExposureLatencyRun {
    RepoExposureLatencyRun {
        format: format.to_string(),
        status: status.to_string(),
        duration_ms: 1,
        exit_code: None,
        stdout_bytes: 0,
        stderr_bytes: 0,
        trace: Vec::new(),
    }
}

#[test]
fn lsp_cockpit_report_command_writes_markdown_and_json() -> Result<(), String> {
    with_repo_cwd(|| {
        lsp_cockpit_report()?;
        let markdown = fs::read_to_string("target/ripr/reports/lsp-cockpit.md")
            .map_err(|err| format!("failed to read lsp cockpit markdown: {err}"))?;
        let json = fs::read_to_string("target/ripr/reports/lsp-cockpit.json")
            .map_err(|err| format!("failed to read lsp cockpit JSON: {err}"))?;
        assert!(markdown.contains("# ripr LSP cockpit report"));
        assert!(json.contains("\"schema_version\": \"0.1\""));
        Ok(())
    })
}

#[test]
fn defaults_first_example_corpus_index_names_required_operator_artifacts() -> Result<(), String> {
    with_repo_cwd(|| {
        let text = fs::read_to_string("fixtures/EXAMPLE_CORPUS.md")
            .map_err(|err| format!("failed to read example corpus index: {err}"))?;
        for required_text in [
            "Boundary gap",
            "Missing equality boundary",
            "Weak oracle",
            "Exact error variant",
            "Opaque fixture/builder",
            "Optional calibration",
            "targeted-test-outcome.json",
            "mutation-calibration.json",
            "lsp-code-actions.json",
            "editor-agent-loop/agent-packet.json",
            "editor-agent-loop/agent-brief.json",
            "editor-agent-loop/agent-verify.json",
            "editor-agent-loop/agent-receipt.json",
            "editor-agent-loop/operator-cockpit.json",
        ] {
            assert!(
                text.contains(required_text),
                "example corpus index should mention {required_text}"
            );
        }
        for required_path in [
            "fixtures/boundary_gap/expected/lsp-diagnostics.json",
            "fixtures/boundary_gap/expected/lsp-code-actions.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/agent-packet.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/agent-brief.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/agent-verify.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/agent-receipt.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/operator-cockpit.json",
            "fixtures/boundary_gap/expected/editor-agent-loop/operator-cockpit.md",
            "fixtures/boundary_gap/calibration/before-targeted-test.repo-exposure.json",
            "fixtures/boundary_gap/calibration/after-targeted-test.repo-exposure.json",
            "fixtures/boundary_gap/calibration/targeted-test-outcome.json",
            "fixtures/boundary_gap/calibration/targeted-test-outcome.md",
            "fixtures/boundary_gap/calibration/runtime-mutants.json",
            "fixtures/boundary_gap/calibration/mutation-calibration.json",
            "fixtures/boundary_gap/calibration/mutation-calibration.md",
            "fixtures/opaque_fixture_builder/expected/check.json",
            "fixtures/opaque_fixture_builder/expected/human.txt",
        ] {
            assert!(
                Path::new(required_path).exists(),
                "example corpus artifact should exist: {required_path}"
            );
        }
        Ok(())
    })
}

#[test]
fn vscode_command_literal_extraction_finds_ripr_commands() {
    let commands = ripr_command_literals_in_text(
        "await vscode.commands.executeCommand('ripr.copyContext');\ncommand: \"ripr.collectContext\"",
    );
    assert_eq!(
        commands,
        vec![
            "ripr.collectContext".to_string(),
            "ripr.copyContext".to_string()
        ]
    );
}

#[test]
fn targeted_test_outcome_args_parse_before_and_after() -> Result<(), String> {
    let args = vec![
        "--before".to_string(),
        "before.json".to_string(),
        "--after".to_string(),
        "after.json".to_string(),
    ];
    let parsed = parse_targeted_test_outcome_args(&args)?;
    assert_eq!(parsed.before, PathBuf::from("before.json"));
    assert_eq!(parsed.after, PathBuf::from("after.json"));
    Ok(())
}

#[test]
fn targeted_test_outcome_report_buckets_seam_movement() -> Result<(), String> {
    let mut before_moved = targeted_static_seam("seam-moved", "weakly_gripped");
    before_moved.missing_discriminators = vec!["threshold equality".to_string()];
    before_moved.oracle_strength = "weak".to_string();
    let before = vec![
        before_moved,
        targeted_static_seam("seam-regressed", "weakly_gripped"),
        targeted_static_seam("seam-same", "strongly_gripped"),
        targeted_static_seam("seam-removed", "ungripped"),
    ];

    let mut after_moved = targeted_static_seam("seam-moved", "strongly_gripped");
    after_moved.observed_values = vec!["50".to_string(), "100".to_string()];
    after_moved.oracle_strength = "strong".to_string();
    let after = vec![
        after_moved,
        targeted_static_seam("seam-regressed", "ungripped"),
        targeted_static_seam("seam-same", "strongly_gripped"),
        targeted_static_seam("seam-new", "weakly_gripped"),
    ];

    let report = build_targeted_test_outcome_report(
        &before,
        &after,
        "before.json".to_string(),
        "after.json".to_string(),
    )?;
    assert_eq!(report.moved.len(), 1);
    assert_eq!(report.moved[0].seam_id, "seam-moved");
    assert_eq!(report.moved[0].direction, "improved");
    assert!(
        report.moved[0]
            .evidence_delta
            .iter()
            .any(|delta| delta.contains("missing discriminator no longer reported"))
    );
    assert!(
        report.moved[0]
            .evidence_delta
            .iter()
            .any(|delta| delta.contains("stronger related oracle visible"))
    );
    assert_eq!(report.regressed.len(), 1);
    assert_eq!(report.unchanged.len(), 1);
    assert_eq!(report.new.len(), 1);
    assert_eq!(report.removed.len(), 1);
    assert_eq!(report.before_counts.get("weakly_gripped"), Some(&2));
    assert_eq!(report.after_counts.get("strongly_gripped"), Some(&2));
    Ok(())
}

#[test]
fn targeted_test_outcome_json_and_markdown_are_structured() -> Result<(), String> {
    let before = vec![
        targeted_static_seam("seam-a", "weakly_gripped"),
        targeted_static_seam("seam-same", "weakly_gripped"),
    ];
    let mut after_same = targeted_static_seam("seam-same", "weakly_gripped");
    after_same.observed_values = vec!["50".to_string(), "100".to_string()];
    let after = vec![
        targeted_static_seam("seam-a", "strongly_gripped"),
        after_same,
    ];
    let report = build_targeted_test_outcome_report(
        &before,
        &after,
        "target/ripr/before.json".to_string(),
        "target/ripr/after.json".to_string(),
    )?;

    let json = targeted_test_outcome_report_json(&report)?;
    let value: Value = serde_json::from_str(&json)
        .map_err(|err| format!("targeted-test outcome JSON should parse: {err}"))?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["status"], "advisory");
    assert_eq!(value["summary"]["moved"], 1);
    assert_eq!(
        value["review_receipt"]["movement_after_verification"][0],
        "1 improved, 0 changed without ranking higher, 0 regressed, 1 unchanged."
    );
    assert!(
        value["review_receipt"]["focused_proof_added"][0]
            .as_str()
            .is_some_and(|text| text.contains("new observed value: 100"))
    );
    assert!(
        value["review_receipt"]["reviewer_should_not_believe"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item == "Merge approval."))
    );

    let markdown = targeted_test_outcome_report_markdown(&report);
    assert!(markdown.contains("# ripr targeted-test outcome report"));
    assert!(markdown.contains("| moved | 1 |"));
    assert!(markdown.contains("## Unchanged"));
    assert!(markdown.contains("seam-same"));
    assert!(markdown.contains("new observed value: 100"));
    assert!(markdown.contains("## Review Receipt"));
    assert!(markdown.contains("### What focused proof changed?"));
    assert!(markdown.contains("### Reviewer should not believe"));
    assert!(markdown.contains("weakly_gripped -> strongly_gripped"));
    Ok(())
}

#[test]
fn targeted_test_outcome_command_writes_markdown_and_json() -> Result<(), String> {
    with_temp_cwd("targeted-test-outcome", |_root| {
        let before = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "predicate_boundary",
      "file": "src/pricing.rs",
      "line": 42,
      "grip_class": "weakly_gripped",
      "related_tests": [
        {"oracle_kind": "exact_value", "oracle_strength": "weak"}
      ],
      "observed_values": ["50"],
      "missing_discriminators": [
        {"value": "threshold equality", "reason": "not observed"}
      ]
    }
  ]
}"#;
        let after = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "predicate_boundary",
      "file": "src/pricing.rs",
      "line": 42,
      "grip_class": "strongly_gripped",
      "related_tests": [
        {"oracle_kind": "exact_value", "oracle_strength": "strong"}
      ],
      "observed_values": ["50", "100"],
      "missing_discriminators": []
    }
  ]
}"#;
        write(Path::new("before.json"), before);
        write(Path::new("after.json"), after);
        targeted_test_outcome(&[
            "--before".to_string(),
            "before.json".to_string(),
            "--after".to_string(),
            "after.json".to_string(),
        ])?;
        let markdown = fs::read_to_string("target/ripr/reports/targeted-test-outcome.md")
            .map_err(|err| format!("failed to read targeted-test outcome markdown: {err}"))?;
        let json = fs::read_to_string("target/ripr/reports/targeted-test-outcome.json")
            .map_err(|err| format!("failed to read targeted-test outcome JSON: {err}"))?;
        assert!(markdown.contains("# ripr targeted-test outcome report"));
        assert!(markdown.contains("## Review Receipt"));
        assert!(json.contains("\"schema_version\": \"0.1\""));
        assert!(json.contains("\"moved\": 1"));
        assert!(json.contains("\"review_receipt\""));
        Ok(())
    })
}

fn ci_full_ok_gate() -> Result<(), String> {
    if std::env::var_os("RIPR_XTASK_TEST_FAIL_OK_GATE").is_some() {
        return Err("unexpected test env".to_string());
    }
    Ok(())
}

fn ci_full_err_gate() -> Result<(), String> {
    Err("boom".to_string())
}

#[test]
fn ci_full_evidence_gates_pin_release_review_order() {
    let names = ci_full_evidence_gates()
        .iter()
        .map(|gate| gate.name)
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "fixtures",
            "goldens check",
            "test-oracle-report",
            "dogfood",
            "metrics"
        ]
    );
}

#[test]
fn ci_full_evidence_gate_runner_accepts_successful_gates() -> Result<(), String> {
    let gates = [CiFullEvidenceGate {
        name: "ok",
        run: ci_full_ok_gate,
    }];

    run_ci_full_evidence_gates(&gates)
}

#[test]
fn ci_full_evidence_gate_runner_names_failing_gate() -> Result<(), String> {
    let gates = [
        CiFullEvidenceGate {
            name: "ok",
            run: ci_full_ok_gate,
        },
        CiFullEvidenceGate {
            name: "bad",
            run: ci_full_err_gate,
        },
    ];

    let error = run_ci_full_evidence_gates(&gates)
        .err()
        .ok_or_else(|| "expected failing gate".to_string())?;

    assert!(error.contains("`bad`"));
    assert!(error.contains("boom"));
    Ok(())
}

#[test]
fn sarif_policy_passes_when_no_new_results() {
    let current = vec![sarif_policy_result(
        "ripr.finding.weakly_exposed",
        "warning",
        "same",
    )];
    let baseline = current.clone();
    let report = build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("baseline.sarif.json".to_string()),
        &current,
        Some(&baseline),
        false,
    );

    assert_eq!(report.status, "pass");
    assert!(report.new_results.is_empty());
    assert_eq!(report.current_compared_results, 1);
    assert_eq!(report.baseline_compared_results, 1);
}

#[test]
fn sarif_policy_flags_new_warning_result() {
    let current = vec![sarif_policy_result(
        "ripr.seam.weakly_gripped",
        "warning",
        "new",
    )];
    let baseline = vec![sarif_policy_result(
        "ripr.seam.weakly_gripped",
        "warning",
        "old",
    )];
    let report = build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("baseline.sarif.json".to_string()),
        &current,
        Some(&baseline),
        false,
    );

    assert_eq!(report.status, "new_results");
    assert_eq!(report.new_results.len(), 1);
    assert_eq!(report.new_results[0].fingerprint, "new");
}

#[test]
fn sarif_policy_ignores_note_when_threshold_warning() {
    let current = vec![sarif_policy_result("ripr.seam.opaque", "note", "new-note")];
    let baseline: Vec<SarifPolicyResult> = Vec::new();
    let report = build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("baseline.sarif.json".to_string()),
        &current,
        Some(&baseline),
        false,
    );

    assert_eq!(report.status, "pass");
    assert!(report.new_results.is_empty());
    assert_eq!(report.current_compared_results, 0);
}

#[test]
fn sarif_policy_missing_baseline_is_advisory_by_default() {
    let current = vec![sarif_policy_result("ripr.seam.ungripped", "warning", "new")];
    let report = build_sarif_policy_report(
        SarifPolicyMode::FailOnNewWarning,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("missing-baseline.sarif.json".to_string()),
        &current,
        None,
        true,
    );

    assert_eq!(report.status, "advisory_missing_baseline");
    assert!(report.new_results.is_empty());
    assert!(report.baseline_missing);
}

#[test]
fn sarif_policy_parses_results_and_skips_suppressions() -> Result<(), String> {
    let text = sarif_policy_test_sarif(vec![
        sarif_policy_json_result("ripr.finding.weakly_exposed", "warning", "visible", false),
        sarif_policy_json_result("ripr.finding.weakly_exposed", "warning", "hidden", true),
    ])?;
    let results = parse_sarif_policy_results(&text, "test SARIF")?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].fingerprint, "visible");
    assert_eq!(results[0].uri, "src/lib.rs");
    assert_eq!(results[0].line, Some(12));
    Ok(())
}

#[test]
fn sarif_policy_args_parse_mode_threshold_and_missing_baseline() -> Result<(), String> {
    let args = vec![
        "--current".to_string(),
        "current.sarif.json".to_string(),
        "--baseline".to_string(),
        "baseline.sarif.json".to_string(),
        "--mode".to_string(),
        "fail-on-new-warning".to_string(),
        "--threshold".to_string(),
        "note".to_string(),
        "--missing-baseline".to_string(),
        "error".to_string(),
    ];

    let parsed = parse_sarif_policy_args(&args)?;

    assert_eq!(parsed.current, PathBuf::from("current.sarif.json"));
    assert_eq!(parsed.baseline, Some(PathBuf::from("baseline.sarif.json")));
    assert_eq!(parsed.mode, SarifPolicyMode::FailOnNewWarning);
    assert_eq!(parsed.threshold, SarifPolicyThreshold::Note);
    assert_eq!(parsed.missing_baseline, SarifMissingBaseline::Error);
    Ok(())
}

#[test]
fn sarif_policy_report_json_and_markdown_are_structured() -> Result<(), String> {
    let current = vec![sarif_policy_result(
        "ripr.seam.weakly_gripped",
        "warning",
        "new",
    )];
    let baseline: Vec<SarifPolicyResult> = Vec::new();
    let report = build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("baseline.sarif.json".to_string()),
        &current,
        Some(&baseline),
        false,
    );

    let json = sarif_policy_report_json(&report)?;
    let markdown = sarif_policy_report_markdown(&report);

    assert!(json.contains("\"schema_version\": \"0.1\""));
    assert!(json.contains("\"new_results_total\": 1"));
    assert!(markdown.contains("# ripr SARIF policy report"));
    assert!(markdown.contains("ripr.seam.weakly_gripped"));
    Ok(())
}

fn sarif_policy_result(rule_id: &str, level: &str, fingerprint: &str) -> SarifPolicyResult {
    SarifPolicyResult {
        key: format!("{rule_id}|{fingerprint}"),
        rule_id: rule_id.to_string(),
        level: level.to_string(),
        fingerprint: fingerprint.to_string(),
        uri: "src/lib.rs".to_string(),
        line: Some(12),
        message: "static exposure result".to_string(),
    }
}

fn sarif_policy_json_result(
    rule_id: &str,
    level: &str,
    fingerprint: &str,
    suppressed: bool,
) -> Value {
    let mut result = serde_json::json!({
        "ruleId": rule_id,
        "level": level,
        "message": { "text": "static exposure result" },
        "partialFingerprints": {
            "riprFingerprintV1": fingerprint
        },
        "locations": [
            {
                "physicalLocation": {
                    "artifactLocation": { "uri": "src/lib.rs" },
                    "region": { "startLine": 12 }
                }
            }
        ]
    });
    if suppressed && let Some(object) = result.as_object_mut() {
        object.insert(
            "suppressions".to_string(),
            serde_json::json!([{ "kind": "external" }]),
        );
    }
    result
}

fn sarif_policy_test_sarif(results: Vec<Value>) -> Result<String, String> {
    let value = serde_json::json!({
        "version": "2.1.0",
        "runs": [
            {
                "results": results
            }
        ]
    });
    serde_json::to_string(&value).map_err(|err| err.to_string())
}

#[test]
fn install_hooks_creates_missing_hook() -> Result<(), String> {
    let root = temp_dir("install-hooks-create");
    fs::create_dir(root.join(".git")).map_err(|err| err.to_string())?;

    let hook = install_hooks_in(&root)?;
    let text = fs::read_to_string(&hook).map_err(|err| err.to_string())?;

    assert_eq!(hook, root.join(".git").join("hooks").join("pre-commit"));
    assert!(is_ripr_managed_hook(&text));
    assert!(text.contains("cargo xtask precommit"));
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn install_hooks_is_idempotent_for_managed_hook() -> Result<(), String> {
    let root = temp_dir("install-hooks-idempotent");
    let hook = root.join(".git").join("hooks").join("pre-commit");
    let stale_managed_hook = ripr_pre_commit_hook().replace("cargo xtask precommit", "echo old");
    write(&hook, &stale_managed_hook);

    let first = install_hooks_in(&root)?;
    let second = install_hooks_in(&root)?;
    let text = fs::read_to_string(&hook).map_err(|err| err.to_string())?;

    assert_eq!(first, hook);
    assert_eq!(second, hook);
    assert_eq!(text, ripr_pre_commit_hook());
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn install_hooks_refuses_unmanaged_existing_hook() -> Result<(), String> {
    let root = temp_dir("install-hooks-unmanaged");
    let hook = root.join(".git").join("hooks").join("pre-commit");
    let user_hook = "#!/usr/bin/env sh\necho user hook\n";
    write(&hook, user_hook);

    let error = install_hooks_in(&root)
        .err()
        .ok_or_else(|| "expected unmanaged hook refusal".to_string())?;
    let text = fs::read_to_string(&hook).map_err(|err| err.to_string())?;

    assert!(error.contains("refusing to overwrite unmanaged hook"));
    assert_eq!(text, user_hook);
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn install_hooks_errors_outside_git_worktree() -> Result<(), String> {
    let root = temp_dir("install-hooks-outside-git");

    let error = install_hooks_in(&root)
        .err()
        .ok_or_else(|| "expected missing git worktree error".to_string())?;

    assert!(error.contains("missing .git directory"));
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn agent_seam_packet_command_args_use_requested_root_and_format() {
    let args =
        repo_seam_inventory_command_args_for_root("agent-seam-packets-json", "fixtures/demo");
    assert_eq!(
        args,
        vec![
            "run",
            "-p",
            "ripr",
            "--quiet",
            "--",
            "check",
            "--root",
            "fixtures/demo",
            "--format",
            "agent-seam-packets-json",
        ]
    );
    assert!(known_xtask_command("agent-seam-packets"));
}

#[test]
fn mutation_calibration_args_parse_root_and_input_paths() -> Result<(), String> {
    let args = vec![
        "fixtures/demo".to_string(),
        "--mutants-json".to_string(),
        "target/mutants/outcomes.json".to_string(),
        "--repo-exposure-json".to_string(),
        "target/ripr/reports/repo-exposure.json".to_string(),
    ];

    let parsed = parse_mutation_calibration_args(&args)?;

    assert_eq!(parsed.root, "fixtures/demo");
    assert_eq!(
        parsed.mutants_json,
        PathBuf::from("target/mutants/outcomes.json")
    );
    assert_eq!(
        parsed.repo_exposure_json,
        Some(PathBuf::from("target/ripr/reports/repo-exposure.json"))
    );
    assert!(known_xtask_command("mutation-calibration"));
    Ok(())
}

#[test]
fn mutation_calibration_imports_static_seams_and_runtime_outcomes() -> Result<(), String> {
    let static_json = r#"{
          "schema_version": "0.2",
          "scope": "repo",
          "seams": [
            {
              "seam_id": "abc123",
              "kind": "predicate_boundary",
              "file": "src/pricing.rs",
              "line": 42,
              "grip_class": "weakly_gripped",
              "related_tests": [
                {
                  "oracle_kind": "broad_error",
                  "oracle_strength": "weak"
                },
                {
                  "oracle_kind": "exact_value",
                  "oracle_strength": "strong"
                }
              ],
              "observed_values": ["50", "10000"],
              "missing_discriminators": [
                {"value": "amount == discount_threshold", "reason": "equality boundary"}
              ]
            }
          ]
        }"#;
    let runtime_json = r#"{
          "outcomes": [
            {
              "mutant": {
                "id": "m1",
                "seam_id": "abc123",
                "file": "src/pricing.rs",
                "line": 42,
                "operator": "replace >= with >"
              },
              "outcome": "caught",
              "duration_ms": 123,
              "test_command": "cargo test pricing"
            }
          ]
        }"#;

    let seams = parse_repo_exposure_static_seams(static_json)?;
    let mutants = parse_mutation_outcomes_json(runtime_json)?;

    assert_eq!(seams.len(), 1);
    assert_eq!(seams[0].oracle_kind, "exact_value");
    assert_eq!(seams[0].oracle_strength, "strong");
    assert_eq!(
        seams[0].missing_discriminators,
        vec!["amount == discount_threshold (equality boundary)"]
    );
    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0].mutation_operator, "replace >= with >");
    assert_eq!(mutants[0].runtime_outcome, "caught");
    assert_eq!(mutants[0].duration, Some("123".to_string()));
    Ok(())
}

#[test]
fn mutation_calibration_merges_mutants_and_outcomes_by_mutant_id() -> Result<(), String> {
    let runtime_json = r#"{
          "mutants": [
            {
              "id": "m1",
              "file": "src/pricing.rs",
              "line": 42,
              "operator": "replace >= with >"
            }
          ],
          "outcomes": [
            {
              "mutant_id": "m1",
              "outcome": "caught",
              "duration_ms": 123,
              "test_command": "cargo test pricing"
            }
          ]
        }"#;

    let mutants = parse_mutation_outcomes_json(runtime_json)?;

    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0].mutant_id, Some("m1".to_string()));
    assert_eq!(mutants[0].file, Some("src/pricing.rs".to_string()));
    assert_eq!(mutants[0].line, Some(42));
    assert_eq!(mutants[0].mutation_operator, "replace >= with >");
    assert_eq!(mutants[0].runtime_outcome, "caught");
    assert_eq!(mutants[0].duration, Some("123".to_string()));
    Ok(())
}

#[test]
fn mutation_calibration_imports_span_based_mutant_locations() -> Result<(), String> {
    let runtime_json = r#"{
          "mutants": [
            {
              "id": "m1",
              "operator": "replace >= with >",
              "span": {
                "file_name": "src/pricing.rs",
                "start": { "line": 42, "column": 13 },
                "end": { "line": 42, "column": 15 }
              }
            }
          ],
          "outcomes": [
            {
              "mutant_id": "m1",
              "outcome": "caught"
            }
          ]
        }"#;

    let mutants = parse_mutation_outcomes_json(runtime_json)?;

    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0].file, Some("src/pricing.rs".to_string()));
    assert_eq!(mutants[0].line, Some(42));
    assert_eq!(mutants[0].runtime_outcome, "caught");
    Ok(())
}

#[test]
fn mutation_calibration_directory_input_combines_outcomes_and_mutants() -> Result<(), String> {
    let dir = temp_dir("mutation-calibration-dir");
    write(
        &dir.join("mutants.json"),
        r#"{
              "mutants": [
                {
                  "id": "m1",
                  "file": "src/pricing.rs",
                  "line": 42,
                  "operator": "replace >= with >"
                }
              ]
            }"#,
    );
    write(
        &dir.join("outcomes.json"),
        r#"{
              "outcomes": [
                {
                  "mutant_id": "m1",
                  "outcome": "caught",
                  "duration_ms": 123
                }
              ]
            }"#,
    );

    let input = read_mutation_input_json(&dir)?;
    let mutants = parse_mutation_outcomes_json(&input)?;
    let remove_result = fs::remove_dir_all(&dir);
    if let Err(err) = remove_result {
        return Err(format!(
            "failed to remove temp dir {}: {err}",
            dir.display()
        ));
    }

    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0].file, Some("src/pricing.rs".to_string()));
    assert_eq!(mutants[0].runtime_outcome, "caught");
    Ok(())
}

#[test]
fn mutation_calibration_joins_by_seam_id_then_file_line() {
    let static_seams = vec![
        StaticSeamRecord {
            seam_id: "seam-a".to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: "weakly_gripped".to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "strong".to_string(),
            observed_values: vec!["50".to_string()],
            missing_discriminators: vec!["amount == discount_threshold".to_string()],
        },
        StaticSeamRecord {
            seam_id: "seam-b".to_string(),
            seam_kind: "error_variant".to_string(),
            file: "src/auth.rs".to_string(),
            line: 11,
            seam_grip_class: "ungripped".to_string(),
            oracle_kind: "unknown".to_string(),
            oracle_strength: "unknown".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        },
    ];
    let runtime_mutants = vec![
        MutationOutcomeRecord {
            mutant_id: Some("m1".to_string()),
            seam_id: Some("seam-a".to_string()),
            file: None,
            line: None,
            mutation_operator: "replace >= with >".to_string(),
            runtime_outcome: "caught".to_string(),
            duration: Some("55".to_string()),
            test_command: Some("cargo test".to_string()),
        },
        MutationOutcomeRecord {
            mutant_id: Some("m2".to_string()),
            seam_id: None,
            file: Some(".\\src\\auth.rs".to_string()),
            line: Some(11),
            mutation_operator: "replace error variant".to_string(),
            runtime_outcome: "timeout".to_string(),
            duration: None,
            test_command: None,
        },
        MutationOutcomeRecord {
            mutant_id: Some("m3".to_string()),
            seam_id: None,
            file: Some("src/other.rs".to_string()),
            line: Some(99),
            mutation_operator: "replace value".to_string(),
            runtime_outcome: "caught".to_string(),
            duration: None,
            test_command: None,
        },
    ];

    let report = build_mutation_calibration_report(static_seams, runtime_mutants);

    assert_eq!(report.matched.len(), 2);
    assert_eq!(report.matched[0].join_method, "seam_id");
    assert_eq!(report.matched[1].join_method, "file_line");
    assert_eq!(report.unmatched_mutants.len(), 1);
    assert!(report.static_without_runtime.is_empty());
}

#[test]
fn mutation_calibration_summarizes_static_runtime_agreement() -> Result<(), String> {
    let static_seams = vec![
        targeted_static_seam("gap-runtime-signal", "weakly_gripped"),
        targeted_static_seam("gap-runtime-clean", "ungripped"),
        targeted_static_seam("gap-inconclusive", "reachable_unrevealed"),
        targeted_static_seam("clean-runtime-clean", "strongly_gripped"),
        targeted_static_seam("clean-runtime-signal", "strongly_gripped"),
    ];
    let runtime_mutants = vec![
        mutation_record("m1", Some("gap-runtime-signal"), "missed"),
        mutation_record("m2", Some("gap-runtime-clean"), "caught"),
        mutation_record("m3", Some("gap-inconclusive"), "unviable"),
        mutation_record("m4", Some("clean-runtime-clean"), "caught"),
        mutation_record("m5", Some("clean-runtime-signal"), "missed"),
        mutation_record_at("m6", None, "src/other.rs", 99, "missed"),
    ];

    let report = build_mutation_calibration_report(static_seams, runtime_mutants);

    assert_eq!(report.agreement.static_gap_and_runtime_signal, 1);
    assert_eq!(report.agreement.static_gap_without_runtime_signal, 2);
    assert_eq!(report.agreement.runtime_signal_without_static_gap, 2);
    assert_eq!(report.agreement.static_clean_and_runtime_clean, 1);
    assert_eq!(report.agreement.runtime_inconclusive, 1);
    assert_eq!(report.static_only_findings.len(), 2);
    assert_eq!(report.missed_runtime_signals.len(), 2);

    let json = mutation_calibration_report_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("failed to parse calibration JSON: {err}"))?;
    assert_eq!(value["agreement"]["static_gap_and_runtime_signal"], 1);
    assert_eq!(value["agreement"]["runtime_signal_without_static_gap"], 2);
    assert_eq!(
        value["missed_runtime_signals"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["static_only_findings"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["matches"][0]["confidence_label"],
        "supports_static_gap"
    );
    assert!(json.contains(r#""confidence_label": "contradicts_static_gap""#));
    assert!(json.contains(r#""confidence_label": "supports_static_clean""#));
    assert!(json.contains(r#""confidence_label": "contradicts_static_clean""#));
    assert!(json.contains(r#""confidence_label": "runtime_only_signal""#));
    assert!(json.contains(r#""confidence_label": "no_runtime_data""#));

    let markdown = mutation_calibration_report_markdown(&report);
    assert!(markdown.contains("## Static/runtime agreement"));
    assert!(markdown.contains("static_gap_and_runtime_signal"));
    assert!(markdown.contains("Confidence label"));
    assert!(markdown.contains("Runtime signals without static gaps"));
    assert!(markdown.contains("Static gaps without runtime signals"));
    Ok(())
}

#[test]
fn mutation_calibration_reports_ambiguous_file_line_without_selecting_first() -> Result<(), String>
{
    let static_seams = vec![
        StaticSeamRecord {
            seam_id: "seam-a".to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: "weakly_gripped".to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "strong".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        },
        StaticSeamRecord {
            seam_id: "seam-b".to_string(),
            seam_kind: "return_value".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: "ungripped".to_string(),
            oracle_kind: "unknown".to_string(),
            oracle_strength: "unknown".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        },
    ];
    let runtime_mutants = vec![MutationOutcomeRecord {
        mutant_id: Some("m1".to_string()),
        seam_id: None,
        file: Some("src/pricing.rs".to_string()),
        line: Some(42),
        mutation_operator: "replace >= with >".to_string(),
        runtime_outcome: "caught".to_string(),
        duration: None,
        test_command: None,
    }];

    let report = build_mutation_calibration_report(static_seams, runtime_mutants);

    assert!(report.matched.is_empty());
    assert_eq!(report.ambiguous_file_line.len(), 1);
    assert_eq!(report.ambiguous_file_line[0].candidates.len(), 2);
    assert!(report.unmatched_mutants.is_empty());
    assert!(report.static_without_runtime.is_empty());
    let json = mutation_calibration_report_json(&report)?;
    assert!(json.contains(r#""confidence_label": "ambiguous_runtime_join""#));
    Ok(())
}

#[test]
fn mutation_calibration_uses_same_static_without_runtime_sample_limit_for_json_and_markdown()
-> Result<(), String> {
    let seams = (0..51)
        .map(|idx| StaticSeamRecord {
            seam_id: format!("seam-{idx:02}"),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: idx + 1,
            seam_grip_class: "weakly_gripped".to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "strong".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        })
        .collect::<Vec<_>>();
    let report = build_mutation_calibration_report(seams, Vec::new());

    let json = mutation_calibration_report_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("failed to parse calibration JSON: {err}"))?;
    let Some(sample) = value["static_without_runtime_sample"].as_array() else {
        return Err("missing static_without_runtime_sample array".to_string());
    };
    let markdown = mutation_calibration_report_markdown(&report);
    let Some(static_without_runtime_section) = markdown
        .split("## Static Seams Without Runtime Data")
        .nth(1)
    else {
        return Err("missing Static Seams Without Runtime Data section".to_string());
    };
    let markdown_rows = static_without_runtime_section
        .lines()
        .filter(|line| line.starts_with("| `seam-"))
        .count();

    assert_eq!(
        sample.len(),
        MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT
    );
    assert_eq!(
        markdown_rows,
        MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT
    );
    Ok(())
}

#[test]
fn mutation_calibration_reports_are_advisory_and_structured() -> Result<(), String> {
    let report = build_mutation_calibration_report(
        vec![StaticSeamRecord {
            seam_id: "seam-a".to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: "weakly_gripped".to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "strong".to_string(),
            observed_values: vec!["50".to_string()],
            missing_discriminators: vec!["amount == discount_threshold".to_string()],
        }],
        vec![MutationOutcomeRecord {
            mutant_id: Some("m1".to_string()),
            seam_id: Some("seam-a".to_string()),
            file: Some("src/pricing.rs".to_string()),
            line: Some(42),
            mutation_operator: "replace >= with >".to_string(),
            runtime_outcome: "caught".to_string(),
            duration: Some("55".to_string()),
            test_command: Some("cargo test pricing".to_string()),
        }],
    );

    let json = mutation_calibration_report_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("failed to parse calibration JSON: {err}"))?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["status"], "advisory");
    assert_eq!(value["metrics"]["matched_total"], 1);
    assert_eq!(value["metrics"]["ambiguous_file_line_total"], 0);
    assert_eq!(
        value["matches"][0]["static"]["missing_discriminators"][0],
        "amount == discount_threshold"
    );
    assert_eq!(
        value["matches"][0]["runtime"]["test_command"],
        "cargo test pricing"
    );

    let markdown = mutation_calibration_report_markdown(&report);
    assert!(markdown.contains("Status: advisory"));
    assert!(markdown.contains("weakly_gripped"));
    assert!(markdown.contains("replace >= with >"));
    Ok(())
}

#[test]
fn parse_workspace_lints_section_extracts_clippy_levels() {
    let cargo = r#"
[workspace]
members = ["crates/ripr"]

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
# leading comment
panic = "deny"
todo = "deny"
allow_attributes_without_reason = "deny" # trailing comment

[profile.release]
lto = true
"#;
    let clippy = super::parse_workspace_lints_section(cargo, "clippy");
    assert_eq!(clippy.get("panic").map(String::as_str), Some("deny"));
    assert_eq!(clippy.get("todo").map(String::as_str), Some("deny"));
    assert_eq!(
        clippy
            .get("allow_attributes_without_reason")
            .map(String::as_str),
        Some("deny")
    );
    assert!(!clippy.contains_key("unsafe_code"));

    let rust = super::parse_workspace_lints_section(cargo, "rust");
    assert_eq!(rust.get("unsafe_code").map(String::as_str), Some("forbid"));
    assert!(!rust.contains_key("panic"));
}

#[test]
fn parse_clippy_lints_ledger_separates_active_and_planned() {
    let ledger = r#"
schema_version = "0.1"

[policy]
unsafe = "forbid"

[[active.panic_family]]
name = "clippy::panic"
level = "deny"

[[active.silent_failure]]
name = "clippy::map_err_ignore"
level = "deny"

[[planned]]
name = "clippy::same_length_and_capacity"
level = "deny"
activate_when_msrv = "1.94"
"#;
    let (entries, violations) = super::parse_clippy_lints_ledger(ledger);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    let names: Vec<(&str, bool)> = entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.is_planned))
        .collect();
    assert_eq!(
        names,
        vec![
            ("clippy::panic", false),
            ("clippy::map_err_ignore", false),
            ("clippy::same_length_and_capacity", true),
        ]
    );
}

#[test]
fn ledger_name_to_lookup_strips_clippy_prefix() {
    assert_eq!(
        super::ledger_name_to_lookup("clippy::panic"),
        ("panic", "clippy")
    );
    assert_eq!(
        super::ledger_name_to_lookup("clippy::cast_sign_loss"),
        ("cast_sign_loss", "clippy")
    );
    assert_eq!(
        super::ledger_name_to_lookup("unsafe_code"),
        ("unsafe_code", "rust")
    );
}

fn ci_budget_fixture() -> &'static str {
    r#"
schema_version = "0.1"
policy_state = "advisory-ledger"
enforcement = "none"
owner = "repo-maintainers"
reason = "test fixture"
unit = "lem"

[defaults]
required_pr_budget = "small"
advisory_pr_budget = "large"
release_budget = "release"

[[budget_band]]
id = "small"
min_lem = 0
max_lem = 5
posture = "required"
description = "small"

[[budget_band]]
id = "large"
min_lem = 6
max_lem = 60
posture = "advisory"
description = "large"

[[budget_band]]
id = "release"
min_lem = 61
posture = "on_demand_release"
description = "release"

[[label]]
name = "full-ci"
effect = "run_all"
budget_effect = "release"
notes = "test"
"#
}

fn ci_lanes_fixture() -> &'static str {
    r#"
schema_version = "0.1"
policy_state = "advisory-ledger"
enforcement = "none"
owner = "repo-maintainers"
reason = "test fixture"

[[artifact_family]]
id = "ci-plan"
paths = ["target/ci/ci-plan.json"]

[[lane]]
id = "docs"
posture = "required"
workflow = ".github/workflows/ci.yml"
jobs = ["rust"]
commands = ["cargo xtask check-doc-index"]
estimated_lem = 1
artifact_families = []
description = "docs"

[[lane]]
id = "coverage"
posture = "advisory"
workflow = ".github/workflows/coverage.yml"
jobs = ["rust-coverage"]
commands = ["cargo llvm-cov"]
estimated_lem = 18
artifact_families = ["ci-plan"]
description = "coverage"

[[lane]]
id = "release-package"
posture = "on_demand_release"
workflow = ".github/workflows/ci.yml"
jobs = ["rust"]
commands = ["cargo package -p ripr --list"]
estimated_lem = 6
artifact_families = ["ci-plan"]
description = "release"
"#
}

fn ci_risk_packs_fixture() -> &'static str {
    r#"
schema_version = "0.1"
policy_state = "advisory-ledger"
enforcement = "none"
owner = "repo-maintainers"
reason = "test fixture"

[risk_pack.docs_only]
paths = ["docs/**"]
required = ["docs"]
advisory = ["coverage"]
on_demand = ["release-package"]
artifact_families = ["ci-plan"]
estimated_lem = 5
owner = "repo-maintainers"
reason = "docs"
"#
}

fn ci_exceptions_fixture() -> &'static str {
    r#"
schema_version = "0.1"
policy_state = "advisory-ledger"
enforcement = "none"
owner = "repo-maintainers"
reason = "test fixture"

[[exception]]
id = "legacy-ci"
kind = "legacy_posture"
paths = [".github/workflows/ci.yml"]
lanes = ["docs"]
current_behavior = "current"
target_behavior = "target"
review_note = "review"
"#
}

fn ci_document(path: &str, text: &str) -> Result<super::CiLedgerDocument, String> {
    let (document, violations) = super::parse_ci_ledger_document(path, text);
    if violations.is_empty() {
        Ok(document)
    } else {
        Err(format!("{path} fixture parse violations: {violations:?}"))
    }
}

fn ci_fixture_violations(
    budget: &str,
    lanes: &str,
    risk_packs: &str,
    exceptions: &str,
) -> Result<Vec<String>, String> {
    let budget = ci_document("policy/ci-budget.toml", budget)?;
    let lanes = ci_document("policy/ci-lane-whitelist.toml", lanes)?;
    let risk_packs = ci_document("policy/ci-risk-packs.toml", risk_packs)?;
    let exceptions = ci_document("policy/ci-whitelist-exceptions.toml", exceptions)?;
    Ok(super::ci_lane_whitelist_violations(
        &budget,
        &lanes,
        &risk_packs,
        &exceptions,
    ))
}

#[test]
fn check_ci_lane_whitelist_accepts_current_policy_ledgers() -> Result<(), String> {
    with_repo_cwd(super::check_ci_lane_whitelist_impl)
}

#[test]
fn check_ci_lane_whitelist_rejects_unknown_risk_pack_lane() -> Result<(), String> {
    let risk_packs =
        ci_risk_packs_fixture().replace(r#"required = ["docs"]"#, r#"required = ["missing-lane"]"#);
    let violations = ci_fixture_violations(
        ci_budget_fixture(),
        ci_lanes_fixture(),
        &risk_packs,
        ci_exceptions_fixture(),
    )?;
    if !violations
        .iter()
        .any(|violation| violation.contains("references unknown lane `missing-lane`"))
    {
        return Err(format!(
            "expected unknown lane violation, got {violations:?}"
        ));
    }
    Ok(())
}

#[test]
fn check_ci_lane_whitelist_rejects_unknown_artifact_family() -> Result<(), String> {
    let lanes = ci_lanes_fixture().replace(
        r#"artifact_families = ["ci-plan"]"#,
        r#"artifact_families = ["missing-family"]"#,
    );
    let violations = ci_fixture_violations(
        ci_budget_fixture(),
        &lanes,
        ci_risk_packs_fixture(),
        ci_exceptions_fixture(),
    )?;
    if !violations
        .iter()
        .any(|violation| violation.contains("unknown artifact family `missing-family`"))
    {
        return Err(format!(
            "expected unknown artifact family violation, got {violations:?}"
        ));
    }
    Ok(())
}

#[test]
fn check_ci_lane_whitelist_detects_budget_label_drift() -> Result<(), String> {
    let budget = ci_budget_fixture().replace(
        r#"budget_effect = "release""#,
        r#"budget_effect = "missing-band""#,
    );
    let violations = ci_fixture_violations(
        &budget,
        ci_lanes_fixture(),
        ci_risk_packs_fixture(),
        ci_exceptions_fixture(),
    )?;
    if !violations
        .iter()
        .any(|violation| violation.contains("unknown budget band `missing-band`"))
    {
        return Err(format!(
            "expected budget band drift violation, got {violations:?}"
        ));
    }
    Ok(())
}

fn proof_packs_fixture() -> &'static str {
    r#"
version = "0.1"
state = "manifest-only"
unknown_surface_policy = "full-proof"

[[pack]]
id = "docs-spec"
paths = ["docs/specs/**"]
required_commands = ["cargo xtask check-spec-format"]
advisory_commands = ["cargo xtask markdown-links"]
ci_lane = "docs"
proves = "Spec documents keep their required shape."
does_not_prove = "It does not demonstrate compiled analyzer behavior."

[[pack]]
id = "release-package"
paths = ["Cargo.toml", "Cargo.lock", "crates/ripr/Cargo.toml", "CHANGELOG.md", "editors/vscode/package.json", "editors/vscode/package-lock.json", ".github/workflows/publish-extension.yml", ".github/workflows/release-server-binaries.yml"]
required_commands = ["cargo test --workspace", "cargo clippy --workspace --all-targets -- -D warnings", "cargo xtask check-pr", "cargo package -p ripr --list", "cargo publish -p ripr --dry-run", "cargo xtask release-readiness"]
advisory_commands = []
ci_lane = "release-readiness-proof"
proves = "Release surfaces pay the full release proof."
does_not_prove = "It does not demonstrate marketplace publish success."
never_routed = true
"#
}

fn proof_pack_fixture_violations(text: &str) -> Result<Vec<String>, String> {
    let manifest = ci_document("policy/proof-packs.toml", text)?;
    let lane_ids: std::collections::BTreeSet<String> = ["docs", "release-readiness-proof"]
        .iter()
        .map(|lane| (*lane).to_string())
        .collect();
    Ok(super::proof_pack_violations(&manifest, &lane_ids))
}

fn require_proof_pack_violation(violations: &[String], needle: &str) -> Result<(), String> {
    if violations
        .iter()
        .any(|violation| violation.contains(needle))
    {
        Ok(())
    } else {
        Err(format!(
            "expected violation containing `{needle}`, got {violations:?}"
        ))
    }
}

#[test]
fn check_proof_packs_accepts_current_manifest() -> Result<(), String> {
    with_repo_cwd(super::check_proof_packs_impl)
}

#[test]
fn check_proof_packs_accepts_well_formed_fixture() -> Result<(), String> {
    let violations = proof_pack_fixture_violations(proof_packs_fixture())?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "expected no violations for well-formed fixture, got {violations:?}"
        ))
    }
}

#[test]
fn check_proof_packs_rejects_duplicate_pack_id() -> Result<(), String> {
    let manifest =
        proof_packs_fixture().replace(r#"id = "docs-spec""#, r#"id = "release-package""#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(&violations, "duplicate proof pack id `release-package`")
}

#[test]
fn check_proof_packs_rejects_empty_paths() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(r#"paths = ["docs/specs/**"]"#, r#"paths = []"#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `docs-spec` must cover at least one path",
    )
}

#[test]
fn check_proof_packs_rejects_empty_required_commands() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(
        r#"required_commands = ["cargo xtask check-spec-format"]"#,
        r#"required_commands = []"#,
    );
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `docs-spec` must name at least one required command",
    )
}

#[test]
fn check_proof_packs_rejects_unknown_command() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(
        r#"required_commands = ["cargo xtask check-spec-format"]"#,
        r#"required_commands = ["cargo xtask not-a-real-command"]"#,
    );
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "unknown repo command `cargo xtask not-a-real-command`",
    )
}

#[test]
fn check_proof_packs_rejects_unknown_non_xtask_command() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(
        r#"advisory_commands = ["cargo xtask markdown-links"]"#,
        r#"advisory_commands = ["make proof"]"#,
    );
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(&violations, "unknown repo command `make proof`")
}

#[test]
fn check_proof_packs_rejects_unknown_ci_lane() -> Result<(), String> {
    let manifest =
        proof_packs_fixture().replace(r#"ci_lane = "docs""#, r#"ci_lane = "missing-lane""#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(&violations, "references unknown ci_lane `missing-lane`")
}

#[test]
fn check_proof_packs_rejects_release_pack_without_never_routed() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace("never_routed = true", "never_routed = false");
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `release-package` must set `never_routed = true`",
    )
}

#[test]
fn check_proof_packs_rejects_release_pack_missing_release_command() -> Result<(), String> {
    // Dropping a release gate from the release-package pack must fail: the
    // pack has to name the full release proof (release-proof protection
    // slice, docs/PROOF_ROUTING.md).
    let manifest = proof_packs_fixture().replace(r#""cargo test --workspace", "#, "");
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `release-package` must require `cargo test --workspace`",
    )
}

#[test]
fn check_proof_packs_rejects_release_pack_missing_release_surface() -> Result<(), String> {
    // Dropping a release surface (here the changelog) from the
    // release-package pack must fail: every release surface has to trip
    // release_proof_required.
    let manifest = proof_packs_fixture().replace(r#", "CHANGELOG.md""#, "");
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `release-package` must cover release surface `CHANGELOG.md`",
    )
}

#[test]
fn check_proof_packs_rejects_missing_release_pack() -> Result<(), String> {
    let manifest =
        proof_packs_fixture().replace(r#"id = "release-package""#, r#"id = "other-pack""#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "must define a `release-package` pack with `never_routed = true`",
    )
}

#[test]
fn check_proof_packs_rejects_unexpected_unknown_surface_policy() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(
        r#"unknown_surface_policy = "full-proof""#,
        r#"unknown_surface_policy = "cheapest-lane""#,
    );
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "`unknown_surface_policy` should be `full-proof`, got `cheapest-lane`",
    )
}

#[test]
fn check_proof_packs_rejects_non_manifest_only_state_drift() -> Result<(), String> {
    let manifest =
        proof_packs_fixture().replace(r#"state = "manifest-only""#, r#"state = "routing""#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "`state` should be `manifest-only`, got `routing`",
    )
}

#[test]
fn proof_pack_command_known_set_accepts_xtask_roots_and_repo_commands() {
    assert!(super::proof_pack_command_is_known(
        "cargo xtask goldens check"
    ));
    assert!(super::proof_pack_command_is_known("cargo test --workspace"));
    assert!(!super::proof_pack_command_is_known("cargo xtask "));
    assert!(!super::proof_pack_command_is_known("rm -rf /"));
}

#[test]
fn check_lint_policy_detects_active_entry_missing_in_cargo_toml() {
    let cargo = r#"
[workspace.lints.clippy]
panic = "deny"
"#;
    let ledger = r#"
[[active.panic_family]]
name = "clippy::panic"
level = "deny"

[[active.silent_failure]]
name = "clippy::map_err_ignore"
level = "deny"
"#;
    let cargo_clippy = super::parse_workspace_lints_section(cargo, "clippy");
    let (entries, parse_violations) = super::parse_clippy_lints_ledger(ledger);
    assert!(parse_violations.is_empty());
    let mut violations = Vec::new();
    for entry in &entries {
        let (bare, _) = super::ledger_name_to_lookup(&entry.name);
        if !cargo_clippy.contains_key(bare) {
            violations.push(entry.name.clone());
        }
    }
    assert_eq!(violations, vec!["clippy::map_err_ignore".to_string()]);
}

#[test]
fn check_lint_policy_detects_planned_entry_already_active_in_cargo_toml() {
    let cargo = r#"
[workspace.lints.clippy]
indexing_slicing = "deny"
"#;
    let ledger = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
"#;
    let cargo_clippy = super::parse_workspace_lints_section(cargo, "clippy");
    let (entries, _violations) = super::parse_clippy_lints_ledger(ledger);
    let mut leaks = Vec::new();
    for entry in entries.iter().filter(|entry| entry.is_planned) {
        let (bare, _) = super::ledger_name_to_lookup(&entry.name);
        if cargo_clippy.contains_key(bare) {
            leaks.push(entry.name.clone());
        }
    }
    assert_eq!(leaks, vec!["clippy::indexing_slicing".to_string()]);
}

#[test]
fn check_lint_policy_detects_level_drift() {
    let cargo = r#"
[workspace.lints.clippy]
panic = "warn"
"#;
    let ledger = r#"
[[active.panic_family]]
name = "clippy::panic"
level = "deny"
"#;
    let cargo_clippy = super::parse_workspace_lints_section(cargo, "clippy");
    let (entries, _violations) = super::parse_clippy_lints_ledger(ledger);
    let mut drift = Vec::new();
    for entry in entries.iter().filter(|entry| !entry.is_planned) {
        let (bare, _) = super::ledger_name_to_lookup(&entry.name);
        let level = cargo_clippy.get(bare).cloned().unwrap_or_default();
        if level != entry.level {
            drift.push((entry.name.clone(), entry.level.clone(), level));
        }
    }
    assert_eq!(
        drift,
        vec![(
            "clippy::panic".to_string(),
            "deny".to_string(),
            "warn".to_string()
        )]
    );
}

// RIPR-SPEC-0080 route-quality standalone report tests

#[test]
fn route_quality_success_rate_is_null_when_attempted_is_zero() {
    let row = super::RiprSwarmRepairRouteQualityRow {
        repair_kind: "add_call_observer".to_string(),
        attempted: 0,
        improved: 0,
        resolved: 0,
        expected_unchanged: 0,
        ..super::RiprSwarmRepairRouteQualityRow::default()
    };
    assert_eq!(
        ripr_swarm_repair_route_quality_success_rate(&row),
        serde_json::Value::Null
    );
}

#[test]
fn route_quality_success_rate_computed_when_attempted_nonzero() {
    let row = super::RiprSwarmRepairRouteQualityRow {
        repair_kind: "add_call_observer".to_string(),
        attempted: 4,
        improved: 2,
        resolved: 1,
        expected_unchanged: 0,
        unchanged: 1,
        ..super::RiprSwarmRepairRouteQualityRow::default()
    };
    // success = improved(2) + resolved(1) + expected_unchanged(0) = 3; rate = 3/4 = 0.75
    let rate = ripr_swarm_repair_route_quality_success_rate(&row);
    assert_eq!(rate, serde_json::json!(0.75));
    assert!(rate != serde_json::Value::Null);
    // Confirm the JSON value is a number, not null
    assert!(rate.is_number());
}

#[test]
fn route_quality_empty_input_produces_empty_not_zero_filled_report() -> Result<(), String> {
    let report = ripr_swarm_route_quality_from_ledger_value(
        "unix_ms:1".to_string(),
        "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
        "missing".to_string(),
        Some("no ledger found".to_string()),
        None,
    );
    assert_eq!(report.repair_route_quality_latest, vec![]);
    assert_eq!(report.repair_route_quality_historical, vec![]);
    assert_eq!(report.language_repair_route_quality_latest, vec![]);
    assert_eq!(report.language_repair_route_quality_historical, vec![]);
    assert_eq!(report.status, "blocked");
    let json_str = ripr_swarm_route_quality_report_json(&report)?;
    let value: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|err| err.to_string())?;
    assert_eq!(value["repair_route_quality_latest"], serde_json::json!([]));
    assert_eq!(
        value["repair_route_quality_historical"],
        serde_json::json!([])
    );
    assert_eq!(
        value["language_repair_route_quality_latest"],
        serde_json::json!([])
    );
    assert_eq!(
        value["language_repair_route_quality_historical"],
        serde_json::json!([])
    );
    // Confirm forbidden keys are absent
    assert!(value.get("top_orphan_receipt_sources").is_none());
    assert!(value.get("stale_receipt_count").is_none());
    assert!(value.get("top_limitation_routes").is_none());
    Ok(())
}

#[test]
fn route_quality_cross_validates_with_attempt_ledger_repair_route_quality() -> Result<(), String> {
    // Build a minimal attempt ledger value with two attempts and check that
    // route_quality.repair_route_quality_latest matches repair_route_quality in the
    // swarm-attempt-ledger, differing only by the added success_rate field.
    let ledger_value = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": { "state": "full", "downstream_consumable": true },
        "repair_route_quality": [],
        "language_repair_route_quality": [],
        "historical_repair_route_quality": [],
        "historical_language_repair_route_quality": [],
        "attempts": [
            {
                "packet_id": "packet-a",
                "canonical_gap_id": "gap-a",
                "attempt_id": "attempt-a-1",
                "repair_kind": "add_assertion",
                "outcome": "evidence_improved",
                "actor_kind": "agent",
                "verify_command": "cargo test",
                "verify_result": "pass",
                "receipt_state": "receipt_present",
                "reason": "test"
            },
            {
                "packet_id": "packet-b",
                "canonical_gap_id": "gap-b",
                "attempt_id": "attempt-b-1",
                "repair_kind": "add_assertion",
                "outcome": "evidence_unchanged",
                "actor_kind": "agent",
                "verify_command": "cargo test",
                "verify_result": "pass",
                "receipt_state": "receipt_present",
                "reason": "test"
            }
        ],
        "latest_attempts": []
    });

    let route_quality_report = ripr_swarm_route_quality_from_ledger_value(
        "unix_ms:2".to_string(),
        "test".to_string(),
        "read".to_string(),
        None,
        Some(&ledger_value),
    );
    let attempts = super::ripr_swarm_attempt_ledger_entries_from_value(&ledger_value);
    let latest_attempts = ripr_swarm_attempt_ledger_latest_attempts(&attempts);
    let ledger_rows = ripr_swarm_attempt_ledger_repair_route_quality(&latest_attempts);

    // The route-quality report latest rows must match the ledger rows (same counts)
    assert_eq!(
        route_quality_report.repair_route_quality_latest.len(),
        ledger_rows.len()
    );
    for (rq_row, ledger_row) in route_quality_report
        .repair_route_quality_latest
        .iter()
        .zip(ledger_rows.iter())
    {
        assert_eq!(rq_row.repair_kind, ledger_row.repair_kind);
        assert_eq!(rq_row.attempted, ledger_row.attempted);
        assert_eq!(rq_row.improved, ledger_row.improved);
        assert_eq!(rq_row.unchanged, ledger_row.unchanged);
        assert_eq!(rq_row.resolved, ledger_row.resolved);
        assert_eq!(rq_row.regressed, ledger_row.regressed);
        assert_eq!(rq_row.attempted_no_receipt, ledger_row.attempted_no_receipt);
        assert_eq!(rq_row.receipt_present, ledger_row.receipt_present);
        assert_eq!(
            rq_row.missing_verify_result,
            ledger_row.missing_verify_result
        );
        assert_eq!(rq_row.expected_unchanged, ledger_row.expected_unchanged);
        assert_eq!(rq_row.unknown, ledger_row.unknown);
    }

    // Verify the JSON output includes success_rate for each row and no forbidden keys
    let json_str = ripr_swarm_route_quality_report_json(&route_quality_report)?;
    let value: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|err| err.to_string())?;
    let rq_latest = value["repair_route_quality_latest"]
        .as_array()
        .ok_or("repair_route_quality_latest must be an array")?;
    assert!(
        !rq_latest.is_empty(),
        "expected non-empty route quality rows"
    );
    for row in rq_latest {
        assert!(
            row.get("repair_kind_success_rate").is_some(),
            "each row must have repair_kind_success_rate"
        );
    }
    assert!(value.get("top_orphan_receipt_sources").is_none());
    assert!(value.get("stale_receipt_count").is_none());
    assert!(value.get("top_limitation_routes").is_none());
    Ok(())
}

/// Issue #2258: every routed-rust required lane must invoke the shared
/// `cargo xtask precommit` gate table instead of enumerating a subset, so the
/// lanes cannot drift from the precommit contract again.
fn routed_rust_workflow_text() -> Result<String, String> {
    let path = repo_root()?.join(".github/workflows/routed-rust.yml");
    std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))
}

/// Extracts the run-block lines of each `- name: <step>` whose name matches
/// `step_name`, stopping at the next step (`- ` at the same indent).
fn routed_rust_step_run_blocks(workflow: &str, step_name: &str) -> Vec<Vec<String>> {
    let marker = format!("- name: {step_name}");
    let mut blocks = Vec::new();
    let mut current: Option<Vec<String>> = None;
    for line in workflow.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- name: ") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            if trimmed == marker {
                current = Some(Vec::new());
            }
            continue;
        }
        if let Some(block) = current.as_mut() {
            block.push(line.to_string());
        }
    }
    if let Some(block) = current.take() {
        blocks.push(block);
    }
    blocks
}

/// Returns an error unless `lines` mention `cargo xtask precommit` exactly
/// once as a bare invocation line. A commented-out (`# cargo xtask
/// precommit`) or otherwise decorated mention does not count as an
/// invocation, so drift toward a disabled line fails instead of passing.
fn require_single_bare_precommit_line(lines: &[String], context: &str) -> Result<(), String> {
    let mentions: Vec<&str> = lines
        .iter()
        .map(|line| line.trim())
        .filter(|trimmed| trimmed.contains("cargo xtask precommit"))
        .collect();
    if mentions != ["cargo xtask precommit"] {
        return Err(format!(
            "{context} must contain exactly one bare `cargo xtask precommit` invocation line (commented or decorated mentions do not count), found {mentions:?}"
        ));
    }
    Ok(())
}

#[test]
fn routed_rust_required_lanes_run_full_precommit_table() -> Result<(), String> {
    let workflow = routed_rust_workflow_text()?;
    let blocks = routed_rust_step_run_blocks(&workflow, "Required Rust gates");
    if blocks.len() != 4 {
        return Err(format!(
            "routed-rust.yml must have exactly 4 `Required Rust gates` steps, found {}",
            blocks.len()
        ));
    }
    for (index, block) in blocks.iter().enumerate() {
        require_single_bare_precommit_line(
            block,
            &format!("Required Rust gates step {}", index + 1),
        )?;
        if block.iter().any(|line| line.contains("if: false")) {
            return Err(format!(
                "Required Rust gates step {} must not guard gates behind `if: false`",
                index + 1
            ));
        }
        for lane_only in [
            "cargo xtask check-evidence-promotion-honesty",
            "cargo xtask check-dependencies",
            "cargo xtask check-process-policy",
            "cargo xtask check-network-policy",
            "cargo xtask goldens check",
            "cargo xtask fixtures",
        ] {
            if !block.iter().any(|line| line.trim() == lane_only) {
                return Err(format!(
                    "Required Rust gates step {} must keep lane-only gate `{lane_only}` enumerated",
                    index + 1
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn routed_rust_docs_gate_runs_full_precommit_table() -> Result<(), String> {
    let workflow = routed_rust_workflow_text()?;
    let docs_gate = workflow
        .split("\n  docs-gate:")
        .nth(1)
        .ok_or("routed-rust.yml must declare a docs-gate job")?;
    let docs_gate = docs_gate
        .split("\n  result:")
        .next()
        .ok_or("docs-gate job must precede the result job")?;
    let lines: Vec<String> = docs_gate.lines().map(str::to_string).collect();
    require_single_bare_precommit_line(
        &lines,
        "docs-gate job (docs-only PRs must run the full gate table)",
    )?;
    Ok(())
}

#[test]
fn routed_rust_precommit_invocation_count_is_five() -> Result<(), String> {
    let workflow = routed_rust_workflow_text()?;
    let count = workflow.matches("cargo xtask precommit").count();
    if count != 5 {
        return Err(format!(
            "routed-rust.yml must invoke `cargo xtask precommit` exactly 5 times (4 required lanes + docs-gate), found {count}"
        ));
    }
    Ok(())
}

/// Pins the drift-check expansion table to the precommit report so the two
/// cannot drift apart silently.
#[test]
fn precommit_gate_commands_match_report() -> Result<(), String> {
    let report = super::precommit_report_body();
    let report_gates: Vec<&str> = report
        .lines()
        .filter_map(|line| {
            line.strip_prefix("- `cargo xtask ")
                .and_then(|rest| rest.strip_suffix('`'))
        })
        .collect();
    let expected: Vec<&str> = super::PRECOMMIT_GATE_COMMANDS.to_vec();
    if report_gates != expected {
        return Err(format!(
            "precommit report gates {report_gates:?} must match PRECOMMIT_GATE_COMMANDS {expected:?}"
        ));
    }
    for required in [
        "check-dependencies",
        "check-process-policy",
        "check-network-policy",
    ] {
        if !expected.contains(&required) {
            return Err(format!(
                "precommit must include the policy gate `{required}`"
            ));
        }
    }
    Ok(())
}

/// An enforced `cargo xtask precommit` invocation must expand into every gate
/// precommit runs; an advisory invocation must not.
#[test]
fn precommit_ci_invocation_expansion_covers_precommit_gates() -> Result<(), String> {
    let workflow = r#"
jobs:
  rust:
    steps:
      - name: Required Rust gates
        run: |
          cargo xtask precommit
      - name: Advisory precommit
        continue-on-error: true
        run: |
          cargo xtask precommit
"#;
    let mut enforced = super::ci_enforced_xtask_invocations(workflow);
    super::expand_precommit_ci_invocations(&mut enforced);
    for gate in super::PRECOMMIT_GATE_COMMANDS {
        let invocation = ((*gate).to_string(), String::new());
        if !enforced.contains(&invocation) {
            return Err(format!(
                "enforced precommit invocation must expand to `{gate}`"
            ));
        }
    }
    let mut without_precommit: std::collections::BTreeSet<super::WorkflowXtaskInvocation> =
        std::collections::BTreeSet::new();
    super::expand_precommit_ci_invocations(&mut without_precommit);
    if !without_precommit.is_empty() {
        return Err("expansion must not add gates when precommit is not enforced".to_string());
    }
    Ok(())
}

/// Pins PRECOMMIT_GATE_COMMANDS to the gate calls `precommit()` actually
/// executes, in order. The drift-check expansion trusts the const, so it must
/// match the real executed sequence, not only the report prose
/// (`precommit_gate_commands_match_report` covers the report direction).
#[test]
fn precommit_gate_commands_match_executed_precommit_source() -> Result<(), String> {
    let path = repo_root()?.join("xtask/src/main.rs");
    let source =
        std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let start = source
        .find("\nfn precommit() -> Result<(), String> {")
        .ok_or("xtask/src/main.rs must define `fn precommit()`")?;
    let body = &source[start..];
    let end = body
        .find("precommit_report_body")
        .ok_or("precommit body extraction must stop before `precommit_report_body`")?;
    let body = &body[..end];
    let executed: Vec<String> = body
        .lines()
        .filter_map(|line| {
            let call = line.trim().strip_suffix("()?;")?;
            if call == "markdown_links" || call.starts_with("check_") {
                Some(call.replace('_', "-"))
            } else {
                None
            }
        })
        .collect();
    let expected: Vec<String> = super::PRECOMMIT_GATE_COMMANDS
        .iter()
        .map(|gate| (*gate).to_string())
        .collect();
    if executed != expected {
        return Err(format!(
            "gates executed by `precommit()` {executed:?} must match PRECOMMIT_GATE_COMMANDS {expected:?}"
        ));
    }
    Ok(())
}

/// Advisory-only `cargo xtask precommit` invocations (`continue-on-error`
/// steps and `|| true` shielding) must not put precommit or any of its gates
/// into the expanded enforced set.
#[test]
fn precommit_ci_invocation_expansion_ignores_advisory_invocations() -> Result<(), String> {
    let workflow = r#"
jobs:
  rust:
    steps:
      - name: Advisory step precommit
        continue-on-error: true
        run: |
          cargo xtask precommit
      - name: Shielded precommit
        run: |
          cargo xtask precommit || true
"#;
    let mut enforced = super::ci_enforced_xtask_invocations(workflow);
    super::expand_precommit_ci_invocations(&mut enforced);
    if enforced.contains(&("precommit".to_string(), String::new())) {
        return Err("advisory precommit invocations must not enter the enforced set".to_string());
    }
    for gate in super::PRECOMMIT_GATE_COMMANDS {
        let invocation = ((*gate).to_string(), String::new());
        if enforced.contains(&invocation) {
            return Err(format!(
                "advisory precommit invocations must not expand to `{gate}`"
            ));
        }
    }
    Ok(())
}

/// Every gate the drift-check expansion credits must be cataloged as
/// CI-enforced, so the expansion set and the catalog flags can never disagree
/// (precommit itself is enforced, so its whole table is enforced).
#[test]
fn precommit_gate_commands_are_catalog_ci_enforced() -> Result<(), String> {
    let catalog = command_catalog();
    for gate in super::PRECOMMIT_GATE_COMMANDS {
        // Catalog entries may carry argument suffixes (for example
        // `check-no-panic-family [--propose]`); match by command root, the
        // same way the drift check matches workflow invocations.
        let entry = catalog
            .iter()
            .find(|entry| entry.command.split_whitespace().next() == Some(*gate))
            .ok_or_else(|| format!("missing catalog entry for `{gate}`"))?;
        if !entry.ci_enforced {
            return Err(format!(
                "precommit gate `{gate}` must be cataloged ci_enforced=true because enforced precommit invocations expand to it"
            ));
        }
    }
    Ok(())
}

#[test]
fn count_policy_gates_have_no_stale_bounds() -> Result<(), String> {
    // Regression for #2413: the count-based gates now check bidirectionally.
    // A stale max_count (actual < allowed) or an orphaned entry (pattern not
    // found) must produce a violation. This test runs the real gates against
    // the real allowlists to catch drift introduced by refactoring that drops
    // process/network occurrences without updating the allowlist.
    //
    // If this test fails, tighten the stale max_count or remove the orphaned
    // entry in the cited allowlist file.
    use crate::{check_local_context, check_network_policy, check_process_policy};
    with_repo_cwd(|| {
        check_process_policy()?;
        check_network_policy()?;
        check_local_context()?;
        Ok(())
    })
}

/// Bind the #3054 claim one level above `run_fixture_outputs`: the
/// golden-comparing entry point must also consume the cache the runner cleared.
///
/// `goldens_check`, `goldens_bless`, and `golden_drift` all reach a fixture only
/// through `collect_golden_runs -> run_fixture -> run_fixture_outputs`, and the
/// rebuild lives in `ripr_fixture_binary`, the single function every spawn
/// reaches the binary through. Asserting here covers the aggregate wrappers
/// without running all 222 fixtures: a stale entry seeded into the pinned cache
/// must be gone after a golden-comparing run, and that run must still agree with
/// its committed goldens.
#[test]
fn golden_comparison_runs_consume_the_cache_the_runner_cleared() -> Result<(), String> {
    with_repo_cwd(|| {
        let name = "all_no_path_disclosure";
        let fixture = PathBuf::from("fixtures").join(name);
        let leaked = fixture.join("input").join("target");
        let _ = fs::remove_dir_all(&leaked);

        let cache_dir = super::fixture_cache_dir(name)?;
        let stale = cache_dir
            .join("repo-file-facts")
            .join("0.2")
            .join("stale-3054-golden.json");
        let stale_parent = stale
            .parent()
            .ok_or_else(|| "seeded cache entry should have a parent".to_string())?;
        fs::create_dir_all(stale_parent)
            .map_err(|err| format!("create {}: {err}", stale_parent.display()))?;
        fs::write(&stale, b"{\"stale\":true}")
            .map_err(|err| format!("write {}: {err}", stale.display()))?;

        let run = super::run_fixture(&fixture)?;

        assert!(
            !stale.exists(),
            "a golden-comparing run must discard cached facts computed by a previous binary: {}",
            stale.display()
        );
        assert!(
            !leaked.exists(),
            "a golden-comparing run must not write its cache into the tracked corpus: {}",
            leaked.display()
        );
        assert!(
            run.comparisons_all_match(),
            "the fixture must still agree with its committed goldens after the cache clear"
        );
        Ok(())
    })
}

#[test]
fn release_pin_ruleset_requires_fully_qualified_tag_ref() -> Result<(), String> {
    const REQUIRED_PATTERN: &str = "refs/tags/ripr-release-*";
    const SHORT_PATTERN: &str = "ripr-release-*";
    const JQ_PREDICATE: &str = r#"(.target == "tag" and .enforcement == "active") and (.conditions.ref_name.include == [$tag]) and (any(.rules[]?; .type == "update")) and (any(.rules[]?; .type == "deletion"))"#;

    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/release_control/pin-ruleset.json"
    ))
    .map_err(|error| format!("failed to parse pin ruleset fixture: {error}"))?;

    let accepts_required_pin = |ruleset: &Value| {
        ruleset.get("name").and_then(Value::as_str) == Some("release-transaction-pins")
            && ruleset.get("target").and_then(Value::as_str) == Some("tag")
            && ruleset.get("enforcement").and_then(Value::as_str) == Some("active")
            && ruleset
                .pointer("/conditions/ref_name/include")
                .and_then(Value::as_array)
                .is_some_and(|include| {
                    include.len() == 1
                        && include.first().and_then(Value::as_str) == Some(REQUIRED_PATTERN)
                })
            && ruleset
                .get("rules")
                .and_then(Value::as_array)
                .is_some_and(|rules| {
                    rules
                        .iter()
                        .any(|rule| rule.get("type").and_then(Value::as_str) == Some("update"))
                        && rules.iter().any(|rule| {
                            rule.get("type").and_then(Value::as_str) == Some("deletion")
                        })
                })
    };

    if !accepts_required_pin(&fixture) {
        return Err("fully qualified tag ruleset fixture was rejected".to_string());
    }

    let mut short = fixture.clone();
    short["conditions"]["ref_name"]["include"] = serde_json::json!([SHORT_PATTERN]);
    if accepts_required_pin(&short) {
        return Err("unqualified tag pattern was accepted as a protected pin".to_string());
    }

    let mut mixed = fixture.clone();
    mixed["conditions"]["ref_name"]["include"] =
        serde_json::json!([REQUIRED_PATTERN, SHORT_PATTERN]);
    if accepts_required_pin(&mixed) {
        return Err("mixed qualified and unqualified patterns were accepted".to_string());
    }

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest should have a repository parent".to_string())?;
    let runbook = fs::read_to_string(repo_root.join("docs/RELEASE_TRANSACTION.md"))
        .map_err(|error| format!("failed to read release transaction runbook: {error}"))?;
    if !runbook.contains("--arg tag \"refs/tags/ripr-release-*\"")
        || runbook.contains("--arg tag \"ripr-release-*\"")
        || !runbook.contains("exact `refs/tags/ripr-release-*` pattern")
        || !runbook.contains("PIN_TAG=\"ripr-release-${VERSION}-${SWARM_PARENT}\"")
        || !runbook.contains(&format!(
            "jq -e --arg tag \"{REQUIRED_PATTERN}\" '{JQ_PREDICATE}'"
        ))
    {
        return Err("runbook does not carry the fully qualified ruleset pattern".to_string());
    }

    let jq_root = temp_dir("release-pin-ruleset-jq");
    let run_jq_predicate = |ruleset: &Value, name: &str| -> Result<bool, String> {
        let path = jq_root.join(name);
        let input = serde_json::to_vec(ruleset)
            .map_err(|error| format!("failed to serialize jq predicate fixture: {error}"))?;
        fs::write(&path, input)
            .map_err(|error| format!("failed to write jq predicate fixture: {error}"))?;
        let path_text = path
            .to_str()
            .ok_or_else(|| "jq predicate fixture path was not UTF-8".to_string())?;
        let args = vec![
            "-e".to_string(),
            "--arg".to_string(),
            "tag".to_string(),
            REQUIRED_PATTERN.to_string(),
            JQ_PREDICATE.to_string(),
            path_text.to_string(),
        ];
        // The jq executable may be a Windows package-manager shim. Keep its
        // inherited cwd stable while it is spawned: another test must not
        // switch to and remove a temporary cwd in this window.
        let _cwd_guard = super::acquire_test_cwd_read_guard();
        command_success_owned("jq", &args)
    };

    if !run_jq_predicate(&fixture, "full.json")? {
        return Err("documented jq predicate rejected the full fixture".to_string());
    }
    if run_jq_predicate(&short, "short.json")? {
        return Err("documented jq predicate accepted the short fixture".to_string());
    }
    if run_jq_predicate(&mixed, "mixed.json")? {
        return Err("documented jq predicate accepted the mixed fixture".to_string());
    }

    let template: Value = serde_json::from_str(include_str!(
        "../../docs/release-candidates/0.11.0-live-head-selection.json"
    ))
    .map_err(|error| format!("failed to parse live-head template: {error}"))?;
    let remote_binding = template
        .pointer("/pin_recipe/remote_binding")
        .and_then(Value::as_str)
        .ok_or_else(|| "live-head template remote binding is missing".to_string())?;
    if !remote_binding.contains(REQUIRED_PATTERN)
        || remote_binding.contains("matches ripr-release-*")
    {
        return Err("live-head template does not carry the fully qualified pattern".to_string());
    }
    if template
        .pointer("/pin_recipe/protected_candidate_tag_format")
        .and_then(Value::as_str)
        != Some(
            "refs/tags/ripr-release-0.11.0-<SWARM_PARENT> (protected candidate tag; local verifier ref remains refs/ripr/release-0.11.0-<SWARM_PARENT>)",
        )
    {
        return Err("candidate tag format drifted from the release contract".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// check-pr first-failure gate sequencing and report authority (#3036)
// ---------------------------------------------------------------------------

/// A deliberately wrong `run_check_pr_gates` that evaluates every closure
/// fails the invocation-count oracles below: later gates must record zero
/// invocations once an earlier gate has failed.
#[test]
fn check_pr_first_gate_failure_stops_later_gates() {
    let invocations = [std::cell::Cell::new(0), std::cell::Cell::new(0)];
    let run_a = || {
        invocations[0].set(invocations[0].get() + 1);
        Err("first gate exploded".to_string())
    };
    let run_b = || {
        invocations[1].set(invocations[1].get() + 1);
        Ok(())
    };
    let gates = [
        super::CheckPrGate {
            name: "gate-a",
            reproduce: "run a",
            run: &run_a,
        },
        super::CheckPrGate {
            name: "gate-b",
            reproduce: "run b",
            run: &run_b,
        },
    ];
    let published = std::cell::RefCell::new(Vec::new());
    let result = super::run_check_pr_gates(&gates, &|failure| {
        published
            .borrow_mut()
            .push(super::check_pr_report(Some(failure)));
        Ok(())
    });

    assert_eq!(
        [invocations[0].get(), invocations[1].get()],
        [1, 0],
        "later gates must not be invoked after the first failure"
    );
    assert!(
        matches!(result, Err(ref err) if err.contains("check-pr gate `gate-a` failed") && err.contains("reproduce: run a")),
        "the returned error must name the failed gate and reproduce command: {result:?}"
    );
    assert_eq!(
        published.borrow().len(),
        1,
        "exactly one failure report is published"
    );
    let report = published.borrow()[0].clone();
    assert!(report.contains("Status: fail"));
    assert!(
        !report.contains("Status: pass"),
        "a failure report must never carry the success status"
    );
    assert!(report.contains("Gate: `gate-a`"));
    assert!(report.contains("Reproduce: `run a`"));
    assert!(report.contains("first gate exploded"));
    assert!(
        report.contains("Not run after the first failure:\n- `gate-b`\n"),
        "the report must state that later gates were not run: {report}"
    );
}

#[test]
fn check_pr_middle_gate_failure_runs_earlier_and_skips_later() {
    let invocations = [
        std::cell::Cell::new(0),
        std::cell::Cell::new(0),
        std::cell::Cell::new(0),
    ];
    let run_a = || {
        invocations[0].set(invocations[0].get() + 1);
        Ok(())
    };
    let run_b = || {
        invocations[1].set(invocations[1].get() + 1);
        Err("middle gate exploded".to_string())
    };
    let run_c = || {
        invocations[2].set(invocations[2].get() + 1);
        Ok(())
    };
    let gates = [
        super::CheckPrGate {
            name: "gate-a",
            reproduce: "run a",
            run: &run_a,
        },
        super::CheckPrGate {
            name: "gate-b",
            reproduce: "run b",
            run: &run_b,
        },
        super::CheckPrGate {
            name: "gate-c",
            reproduce: "run c",
            run: &run_c,
        },
    ];
    let published = std::cell::RefCell::new(Vec::new());
    let result = super::run_check_pr_gates(&gates, &|failure| {
        published
            .borrow_mut()
            .push(super::check_pr_report(Some(failure)));
        Ok(())
    });

    assert_eq!(
        [
            invocations[0].get(),
            invocations[1].get(),
            invocations[2].get()
        ],
        [1, 1, 0],
        "earlier gates run, the failed gate runs once, later gates never run"
    );
    assert!(result.is_err());
    let report = published.borrow()[0].clone();
    assert!(report.contains("Gate: `gate-b`"));
    assert!(
        report.contains("Not run after the first failure:\n- `gate-c`\n"),
        "only the gates after the failure are listed as not run: {report}"
    );
}

#[test]
fn check_pr_all_gates_pass_publishes_no_failure_block() {
    let run_a = || Ok(());
    let gates = [super::CheckPrGate {
        name: "gate-a",
        reproduce: "run a",
        run: &run_a,
    }];
    let published = std::cell::RefCell::new(Vec::new());
    let result = super::run_check_pr_gates(&gates, &|failure| {
        published
            .borrow_mut()
            .push(super::check_pr_report(Some(failure)));
        Ok(())
    });

    assert!(matches!(result, Ok(())));
    assert!(
        published.borrow().is_empty(),
        "no failure report may be published when every gate passes"
    );
    // The success artifact is pinned byte-for-byte so the shared composer
    // cannot drift the long-standing pass report.
    assert_eq!(
        super::check_pr_report(None),
        "# ripr check-pr report\n\nStatus: pass\n\nChecks:\n\n- `cargo xtask ci-fast`\n- `cargo clippy --workspace --all-targets -- -D warnings`\n- `cargo doc --workspace --no-deps`\n- `cargo xtask pr-summary`\n\nReports:\n\n- `target/ripr/reports/pr-summary.md`\n- `target/ripr/reports/check-pr.md`\n\nRelease/package gates are intentionally left to `cargo xtask ci-full` or release-specific workflows.\n"
    );
}

#[test]
fn check_pr_failure_report_handles_empty_error_text() {
    let run_a = || Err(String::new());
    let gates = [super::CheckPrGate {
        name: "gate-a",
        reproduce: "run a",
        run: &run_a,
    }];
    let published = std::cell::RefCell::new(Vec::new());
    let result = super::run_check_pr_gates(&gates, &|failure| {
        published
            .borrow_mut()
            .push(super::check_pr_report(Some(failure)));
        Ok(())
    });
    assert!(
        result.is_err(),
        "an empty gate error is still a failure, not a pass: {result:?}"
    );

    let report = published.borrow()[0].clone();
    assert!(
        report.contains("(gate produced no error output)"),
        "an empty gate error must be explicit, not invisible: {report}"
    );
    assert!(
        report.contains("(none — the failed gate was the last)"),
        "a failure on the last gate has an explicit empty not-run list: {report}"
    );
    assert!(
        report.contains("pr-summary.md` (not refreshed"),
        "the failure path returns before pr_summary() runs, so the report must mark it stale: {report}"
    );
}

#[test]
fn check_pr_bounded_error_lines_are_crlf_normalized_and_capped() {
    let fewer = super::bound_first_failure_lines("one\ntwo");
    assert_eq!(
        fewer, "one\n  two",
        "fewer than five lines are all retained"
    );

    let more = super::bound_first_failure_lines("l1\nl2\nl3\nl4\nl5\nl6\nl7");
    assert_eq!(
        more,
        "l1\n  l2\n  l3\n  l4\n  l5\n  (+2 more lines truncated)"
    );
    assert!(
        !more.contains("l6"),
        "more than five lines are bounded to five: {more}"
    );
    assert!(
        more.contains("truncated"),
        "dropped lines must leave an explicit marker, not silent truncation: {more}"
    );

    let crlf = super::bound_first_failure_lines("alpha\r\nbeta\r\n");
    assert_eq!(crlf, "alpha\n  beta");
    assert!(!crlf.contains('\r'), "CRLF input is normalized: {crlf:?}");
}

#[test]
fn check_pr_failure_report_replaces_stale_success_report() -> Result<(), String> {
    with_repo_cwd(|| {
        // Preserve any pre-existing report so the test leaves no false
        // failure artifact behind for a real check-pr reader.
        let report_path = Path::new("target")
            .join("ripr")
            .join("reports")
            .join("check-pr.md");
        let original = fs::read(&report_path).ok();
        let outcome = (|| -> Result<(), String> {
            super::write_report("check-pr.md", &super::check_pr_report(None))?;
            let run_a = || Err("stale-replacement probe".to_string());
            let gates = [super::CheckPrGate {
                name: "gate-a",
                reproduce: "run a",
                run: &run_a,
            }];
            let result = super::run_check_pr_gates(&gates, &|failure| {
                super::write_report("check-pr.md", &super::check_pr_report(Some(failure)))
            });
            assert!(result.is_err());

            let text = fs::read_to_string("target/ripr/reports/check-pr.md")
                .map_err(|err| format!("read check-pr.md: {err}"))?;
            assert!(
                text.contains("Status: fail"),
                "the failed current run must replace the stale success report"
            );
            assert!(!text.contains("Status: pass"));
            Ok(())
        })();
        let restore = match &original {
            Some(bytes) => fs::write(&report_path, bytes).map_err(|err| {
                format!("failed to restore {}: {err}", normalize_path(&report_path))
            }),
            None => match fs::remove_file(&report_path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(format!(
                    "failed to clear {}: {err}",
                    normalize_path(&report_path)
                )),
            },
        };
        restore?;
        outcome
    })
}

#[test]
fn check_pr_report_publication_failure_is_distinguishable() {
    let run_a = || Err("gate exploded".to_string());
    let gates = [super::CheckPrGate {
        name: "gate-a",
        reproduce: "run a",
        run: &run_a,
    }];
    let result = super::run_check_pr_gates(&gates, &|_failure| Err("disk full".to_string()));

    assert!(
        matches!(result, Err(ref err) if err.contains("gate-a")
            && err.contains("reproduce: run a")
            && err.contains("gate exploded")
            && err.contains("disk full")
            && err.contains("publishing the failure report also failed")),
        "a failed gate plus failed report publication must stay distinguishable and preserve the gate diagnostic and reproduce command: {result:?}"
    );
}
