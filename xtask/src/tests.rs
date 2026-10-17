//! Tests for the xtask command surface, extracted from `main.rs` (#2119)
//! as the first behavior-preserving decomposition slice: the module
//! body moved verbatim; test names and module path (`crate::tests`) are
//! unchanged.
//!
//! The `#[test]` functions were later split by command family into the sibling
//! modules under `tests/` (move-only; each starts with `use super::*;`). This file
//! keeps the shared helpers, imports and fixtures those modules reuse. Test
//! names gain the module segment (`tests::<module>::<name>`); tests that sat in
//! the flat file are found by name with `rg 'fn <name>' xtask/src/tests/`.

mod discarded_matcher_honesty;

mod fixture_cache;

#[path = "precommit_source_oracle.rs"]
mod precommit_source_oracle;

#[cfg(windows)]
mod review_conversations;

mod badge;
mod check;
mod docs;
mod dogfood;
mod evidence;
mod lane1;
mod misc_a;
mod misc_b;
mod misc_c;
mod pr;
mod repo;
mod report;
mod ripr_swarm;
mod rust_corpus_projection;
mod test_analysis;

/// Best-effort temp-dir teardown for tests. The `io::Result` is matched
/// with `if let` so a `#[must_use]` cleanup failure is an explicit ignore.
fn ignore_remove_dir_all(path: impl AsRef<std::path::Path>) {
    if let Ok(()) = std::fs::remove_dir_all(path) {}
}

use std::io::Read;

use crate::acquire_test_cwd_write_guard;

use crate::python_judged_panel_replay::sha256_hex;

use ripr::output::receipt_lifecycle::{
    RECEIPT_MISSING, RECEIPT_MOVEMENT_IMPROVED, RECEIPT_NOT_APPLICABLE,
};

use super::PrActionableInput;

use super::RiprSwarmAttemptLedgerEntry;

use super::RiprSwarmAttemptLedgerReport;

use super::RiprSwarmCommand;

use super::RiprSwarmReadinessInput;

use super::XtaskCommand;

use super::add_name_status_bytes;

use super::add_porcelain_bytes;

use super::dispatch;

use super::is_network_policy_candidate;

use super::lane1_runtime_status_full;

use super::next_pending_heading;

use super::policy::droid_review::{
    active_yaml_lines, check_droid_action_refs, check_droid_common,
    check_droid_security_scan_config, forbids_active_line, has_active_line, strip_yaml_comment,
};

use super::reports::{
    test_oracle_report_impl_for_roots, test_oracle_report_json, test_oracle_report_markdown,
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
    ArtifactRouterInput, BUN_UB_CROSS_LANGUAGE_DOGFOOD_REQUIRED_CASES, BadgeArtifactJob,
    BadgeBasisReport, BadgeBasisSignal, BadgeCanonicalProjection, BadgeCountBreakdown,
    BadgeEndpointSnapshot, BadgeNativeAuditSnapshot, BadgeNativeSlot, Capability, ChangedPath,
    CheckReport, CheckStatus, CheckViolation, CiFullEvidenceGate, CommandCatalogEntry, CwdCommand,
    DOC_ARTIFACT_LEDGER, DogfoodBunUbCrossLanguageScenario, DogfoodEditorFirstPrBridgeRun,
    DogfoodEditorGapCockpitRun, DogfoodFindingAlignmentRun, DogfoodFindingAlignmentScenario,
    DogfoodFirstActionRun, DogfoodFirstPrRun, DogfoodFrontPanelRun, DogfoodGateRun,
    DogfoodGeneratedCiCockpitRun, DogfoodLanguagePreviewRun, DogfoodPrInlineCommentRun,
    DogfoodPreviewProjectionRuns, DogfoodPythonNoActionEvalScenario,
    DogfoodPythonRealRepoEvalScenario, DogfoodPythonStaticLimitEvalScenario,
    DogfoodRealRepairAttemptScenario, DogfoodReportInputs, DogfoodReportPacketIndexRun, DogfoodRun,
    DogfoodSurfaceProjectionAlignmentScenario, DogfoodTypescriptPreviewRepairLoopScenario,
    DogfoodUserSurfaceProjectionScenario, EVIDENCE_QUALITY_SCORECARD_AUDIT_REGENERATION_FAILED,
    EVIDENCE_QUALITY_TREND_PREVIOUS_ARTIFACT_UNAVAILABLE, EvidenceQualityScorecardInput,
    EvidenceQualityScorecardInputs, EvidenceQualityScorecardReport, EvidenceQualityTrendInputs,
    EvidenceQualityTrendReport, FixKind, GENERATED_CI_COCKPIT_COMMAND,
    GENERATED_CI_FIRST_ACTION_REPAIR, GENERATED_CI_FIRST_PR_REPAIR,
    GENERATED_CI_FRONT_PANEL_REPAIR, GENERATED_CI_PACKET_INDEX_REPAIR, GhPrStatusPullRequest,
    GhPrStatusReview, Lane1EvidenceAuditRepoExposureGeneration,
    Lane1EvidenceAuditRepoExposureOutcome, LocalContextAllow, LocalMarkdownTarget,
    LspCockpitFixture, LspCockpitReport, MarkdownLink, PYTHON_REAL_REPO_EVAL_REQUIRED_CASES,
    PYTHON_REAL_REPO_EVAL_REQUIRED_NO_ACTION_CASES,
    PYTHON_REAL_REPO_EVAL_REQUIRED_STATIC_LIMIT_CASES, PrTriageCheck, PrTriageFinding,
    PrTriagePullRequest, REAL_REPAIR_ATTEMPTS_CORPUS, REAL_REPAIR_ATTEMPTS_REQUIRED_CASES,
    REPO_BADGE_ARTIFACT_DEFAULT_TIMEOUT_MS, REPO_BADGE_ARTIFACT_TIMEOUT_ENV,
    REPO_EXPOSURE_SUMMARY_REPORT_DEFAULT_TIMEOUT_MS, REPO_EXPOSURE_SUMMARY_REPORT_TIMEOUT_ENV,
    ReceiptRecord, RepoBadgeArtifactOptions, RepoExposureCpuCost, RepoExposureLatencyReport,
    RepoExposureLatencyRun, RepoExposureLatencyTrace, RepoExposureMeasurement, ReportIndexEntry,
    ReportIndexRepoOpsArtifact, RiprSwarmReadinessNextActionSources, RoutedRustEventRoute,
    SUPPORT_TIERS_PATH, SarifPolicyMode, SarifPolicyResult, SarifPolicyThreshold,
    StaticLanguageAllowEntry, StaticLanguageMatcher, TYPESCRIPT_BUN_UB_CALIBRATION_REQUIRED_CASES,
    TYPESCRIPT_PREVIEW_FALSE_ACTIONABLE_AUDIT_REQUIRED_CASES,
    TYPESCRIPT_PREVIEW_REPAIR_LOOP_REQUIRED_CASES, TestOracleClass,
    UNREGISTERED_SPEC_MESSAGE_LIMIT, USER_SURFACE_PROJECTION_REQUIRED_RUN_STATUSES,
    USER_SURFACE_PROJECTION_REQUIRED_SURFACES, WorktreeDoctorFinding, WorktreeDoctorSeverity,
    actionable_gap_outcomes_json, actionable_gap_outcomes_markdown,
    actionable_gap_outcomes_report_from_values, actionable_gap_outcomes_report_impl,
    allow_attribute_budget_violations, artifact_router_path_violation, badge_artifact_command_args,
    badge_artifact_command_label, badge_artifact_jobs, badge_artifact_native_slot,
    badge_artifacts_impl_with_runners, badge_artifacts_summary_markdown,
    badge_basis_canonical_projection, badge_basis_derived_ripr_plus_snapshot,
    badge_basis_needs_repo_badge_plus_job, badge_basis_report_json, badge_basis_report_markdown,
    badge_basis_seam_native_counts, badge_diff_policy_violations, badge_native_audit_snapshot,
    build_lsp_cockpit_report, build_repo_exposure_latency_report,
    build_targeted_test_outcome_report, check_allow_attributes,
    check_badge_diff_policy_with_context, check_doc_artifacts,
    check_doc_artifacts_with_unregistered_baseline, check_droid_review_config,
    check_executable_files, check_file_policy, check_local_context, check_network_policy,
    check_no_panic_family, check_process_policy, check_static_language, check_support_tiers,
    check_workflows, ci_enforced_xtask_invocations, ci_full_evidence_gates, cockpit_json,
    cockpit_markdown, command_catalog, command_catalog_ci_drift_violations,
    command_catalog_violations, commands_report_json, commands_report_markdown, critic_findings,
    days_from_civil, doc_artifact_kind_matches_path, doc_artifact_violations,
    dogfood_bun_ub_cross_language_run, dogfood_bun_ub_cross_language_scenarios,
    dogfood_class_counts, dogfood_editor_first_pr_bridge_run,
    dogfood_editor_first_pr_bridge_scenarios, dogfood_editor_gap_cockpit_run,
    dogfood_editor_gap_cockpit_scenarios, dogfood_failed_families, dogfood_finding_alignment_run,
    dogfood_finding_alignment_scenarios, dogfood_first_action_run, dogfood_first_action_scenarios,
    dogfood_first_pr_metrics, dogfood_first_pr_run, dogfood_first_pr_scenarios,
    dogfood_gate_adoption_run, dogfood_gate_adoption_scenarios, dogfood_gate_result,
    dogfood_generated_ci_cockpit_run_from_surfaces, dogfood_language_preview_run,
    dogfood_language_preview_scenarios, dogfood_pr_inline_comment_run,
    dogfood_pr_inline_comment_scenarios, dogfood_pr_review_front_panel_run,
    dogfood_pr_review_front_panel_scenarios, dogfood_push_python_quality_ratio_json,
    dogfood_python_no_action_eval_run, dogfood_python_no_action_eval_scenarios,
    dogfood_python_real_repo_eval_run, dogfood_python_real_repo_eval_scenarios,
    dogfood_python_repair_routing_quality_summary, dogfood_python_static_limit_eval_run,
    dogfood_python_static_limit_eval_scenarios, dogfood_real_repair_attempt_run,
    dogfood_real_repair_attempt_scenarios, dogfood_report_json, dogfood_report_markdown,
    dogfood_report_packet_index_run, dogfood_report_packet_index_scenarios, dogfood_report_status,
    dogfood_surface_projection_alignment_run, dogfood_surface_projection_alignment_scenarios,
    dogfood_typescript_preview_repair_loop_run, dogfood_typescript_preview_repair_loop_scenarios,
    dogfood_user_surface_projection_run, dogfood_user_surface_projection_scenarios,
    error_ripr_plus_receipt, evidence_health_args,
    evidence_quality_scorecard_audit_regeneration_failure_audit,
    evidence_quality_scorecard_from_values, evidence_quality_scorecard_json,
    evidence_quality_scorecard_markdown, evidence_quality_trend_from_values,
    evidence_quality_trend_json, evidence_quality_trend_markdown,
    evidence_quality_trend_report_impl, extract_json_object_usize_map, extract_json_string,
    extract_json_warnings, extract_workflow_run_blocks, finding_alignment_raw_to_canonical_ratio,
    finding_alignment_verify_command_is_missing, finish_traceability_report,
    finish_worktree_doctor_report, first_line_difference, front_panel_case_inputs,
    generated_clean_violations, gh_pr_safe_next_action, gh_pr_status_json, gh_pr_status_markdown,
    gh_pr_status_readiness, github_event_pull_request_title_from_text, glob_matches,
    golden_changes_without_blessing, golden_drift_semantics, guarded_allow_attribute_lints,
    guarded_allow_attributes_in_text, heading_slug, heading_slugs, help_message, install_hooks_in,
    is_badge_refresh_context, is_bdd_test_name, is_dependency_surface_candidate,
    is_generated_candidate, is_non_rust_programming_candidate, is_public_badge_basis_surface,
    is_receipt_status, is_ripr_managed_hook, is_snake_case_id, is_spec_id, json_escape,
    json_number_after, json_string_values_for_key, json_summary_count, known_commands,
    known_xtask_command, lane1_actionable_gap_packets_json, lane1_actionable_gap_packets_markdown,
    lane1_evidence_audit_from_repo_exposure, lane1_evidence_audit_json,
    lane1_evidence_audit_limited_report, lane1_evidence_audit_markdown,
    lane1_evidence_audit_repo_exposure_args,
    lane1_evidence_audit_report_from_complete_repo_exposure, lane1_evidence_audit_timeout_error,
    lane1_readiness_packet_specs, limited_badge_artifacts_json, limited_badge_artifacts_markdown,
    line_has_static_language_inline_allow, local_context_line_findings, local_markdown_target,
    lsp_cockpit_report, lsp_cockpit_report_json, lsp_cockpit_report_markdown,
    markdown_links_in_text, missing_anchor_violation, mutation_calibration_report_json,
    mutation_calibration_report_markdown, next_checkpoints_from_capabilities,
    next_spec_id_from_ids, non_rust_programming_retention_reason, normalize_fixture_human_output,
    normalize_fixture_json_output, normalize_golden_text, normalize_path,
    parse_actionable_gap_outcomes_args, parse_doc_artifact_ledger_text,
    parse_file_policy_allowlist, parse_gh_pr_status_args, parse_gh_pr_status_pull_request,
    parse_inline_array, parse_mutation_calibration_args, parse_mutation_outcomes_json,
    parse_pr_triage_pull_requests, parse_reason, parse_repo_badge_artifact_options,
    parse_repo_exposure_static_seams, parse_repo_exposure_summary_counts,
    parse_required_status_contexts, parse_ripr_swarm_args, parse_ripr_swarm_plan_args,
    parse_sarif_policy_args, parse_sarif_policy_results, parse_static_language_allowlist,
    parse_targeted_test_outcome_args, pin_report_packet_index_generated_at,
    pr_actionable_delta_front_panel_from_inputs, pr_body_validation_warning, pr_checks_summary,
    pr_ready_json, pr_ready_markdown, pr_ready_next_action, pr_ready_status,
    pr_ready_status_from_report_status, pr_sensitive_file_reason, pr_shape_warnings,
    pr_summary_body, pr_title_family, pr_triage_findings, pr_triage_json, pr_triage_markdown,
    pr_triage_queue_dispositions, precommit_report_body, public_badge_basis_violations,
    public_contract_rows, read_badge_artifact_diff_governed, read_json_value,
    read_lsp_cockpit_json_value, read_mutation_input_json, read_repo_exposure_summary_artifact,
    receipt_json, receipt_specs, receipt_status_from_reports, repo_badge_artifact_command_args,
    repo_badge_artifact_jobs, repo_badge_artifact_stdout_from_output,
    repo_badge_artifact_timeout_ms_from_env, repo_badge_artifacts_summary_markdown,
    repo_exposure_file_fact_cache_from_stderr, repo_exposure_latency_json,
    repo_exposure_latency_markdown, repo_exposure_latency_run,
    repo_exposure_latency_run_from_output, repo_exposure_latency_status,
    repo_exposure_latency_trace, repo_exposure_resource_cost_from_stderr,
    repo_exposure_summary_report_timeout_ms_from_env, repo_root,
    repo_seam_inventory_command_args_for_root, report_index_lane1_overall_status,
    report_index_lane1_readiness_packets, report_index_missing_artifact_count,
    report_index_missing_expected, report_index_next_commands, report_index_repo_ops_packets,
    report_index_repo_ops_status, report_packet_index_case_id_violation,
    report_packet_index_generated_at_violation, report_packet_index_render_plan,
    report_status_from_text, repository_owned_review_thread_mutation_violations,
    reverse_direction_unregistered_violation, ripr_command_literals_in_text, ripr_debug_binary,
    ripr_plus_receipt_from_badge, ripr_plus_receipt_from_options,
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
    routed_rust_event_route, routed_rust_ready_event_contract_violations,
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
    test_efficiency_report_markdown, test_oracle_tests_in_text, traceability_recommended_fixes,
    unknown_command_message, user_surface_projection_required_run_status_violations,
    validate_actionable_gap_outcomes_fixture_case, validate_actionable_gap_outcomes_fixture_corpus,
    validate_local_context_allowlist, validate_swarm_plan_packet_fixture_case,
    validate_swarm_plan_packet_fixture_corpus, vscode_compile_command, vscode_extension_dir,
    vscode_package_command, vscode_package_version, vscode_test_e2e_command,
    windows_absolute_path_tokens, workflow_bare_self_hosted_violations,
    workflow_review_thread_mutation_violations, workflow_runtime_violations, worktree,
    worktree_doctor_findings, write_badge_artifacts_after_build, write_badge_artifacts_from_diff,
    write_badge_input_identity, write_evidence_health_report_with_runner,
    write_evidence_health_report_with_runners,
    write_lane1_evidence_audit_repo_exposure_with_runner, write_repo_exposure_latency_report,
    write_repo_exposure_summary_report_with_runner,
};

use super::{
    DeclaredIntent, LocalContextFinding, MUTATION_CALIBRATION_AGREEMENT_SAMPLE_LIMIT,
    MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT, PrReadyStep, TestEfficiencyEntry,
    TestEfficiencyValue, TestIntentDeclaration, TestIntentKind, TestIntentReportSummary,
    apply_duplicate_discriminator_groups, apply_test_intent_to_entries,
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

fn semver_external_case_for_test() -> super::EvidencePromotionExternalCase {
    super::EvidencePromotionExternalCase {
            id: "rust_semver_matches_greater_external_limitation".to_string(),
            language: "rust".to_string(),
            external_repo: "https://github.com/dtolnay/semver".to_string(),
            external_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
            external_patch: PathBuf::from(
                "fixtures/evidence-promotion-honesty-corpus/patches/semver-matches-greater.diff",
            ),
            external_command:
                "ripr check --root {checkout} --diff {external_patch} --mode fast --json"
                    .to_string(),
            runtime_budget_seconds: 120,
            artifact_budget_bytes: 10_485_760,
            assertions: vec![
                super::EvidencePromotionSemanticAssertion::MustSeeChangedFile {
                    path: "src/eval.rs".to_string(),
                },
                super::EvidencePromotionSemanticAssertion::MustNotReportClean,
                super::EvidencePromotionSemanticAssertion::MustDiscloseScope,
                super::EvidencePromotionSemanticAssertion::MustEmitLimitation {
                    expected_limit_kind: "rust_integration_public_api_path_unresolved"
                        .to_string(),
                },
                super::EvidencePromotionSemanticAssertion::MustNotEmitRepairPacket,
                super::EvidencePromotionSemanticAssertion::MustDiscloseWitness,
                super::EvidencePromotionSemanticAssertion::MustDiscloseLimitationDetail,
                super::EvidencePromotionSemanticAssertion::ExpectedLimitationDetail {
                    last_established_edge:
                        "test `test_basic` (tests/test_version_req.rs:38) -> entry `assert_match_all`"
                            .to_string(),
                    first_unresolved_edge:
                        "entry `assert_match_all` -> owner `matches_greater` through a transitive Rust helper path"
                            .to_string(),
                    non_claim:
                        "named limitation only; ripr cannot confirm or deny that this path observes the change"
                            .to_string(),
                },
                super::EvidencePromotionSemanticAssertion::ExpectedLimitationRoute {
                    route: "analysis/rust-public-api-transitive-reach".to_string(),
                },
                super::EvidencePromotionSemanticAssertion::MustNotPromote,
                super::EvidencePromotionSemanticAssertion::MaximumClass {
                    class: "no_static_path".to_string(),
                },
            ],
        }
}

fn semver_limitation_check_json() -> serde_json::Value {
    // A `--diff` launch has no revision `base` by construction (#3952, #7880).
    serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "mode": "fast",
        "root": "target/ripr/evidence-promotion-honesty/checkouts/semver",
        "summary": {"findings": 1},
        "findings": [
            {
                "id": "probe:src_eval.rs:predicate:87a89f28",
                "classification": "no_static_path",
                "probe": {"file": "target/ripr/evidence-promotion-honesty/checkouts/semver/src/eval.rs"},
                "static_limit_kind": "rust_integration_public_api_path_unresolved",
                "static_limitation": {
                    "kind": "rust_integration_public_api_path_unresolved",
                    "last_established_edge": "test `test_basic` (tests/test_version_req.rs:38) -> entry `assert_match_all`",
                    "first_unresolved_edge": "entry `assert_match_all` -> owner `matches_greater` through a transitive Rust helper path",
                    "analyzer_route": "analysis/rust-public-api-transitive-reach",
                    "non_claim": "named limitation only; ripr cannot confirm or deny that this path observes the change"
                },
                "evidence": [
                    "ripr saw a test reaching public API that may call toward this change through a transitive path it does not fully trace.",
                    "For example, the test `test_basic` (tests/test_version_req.rs:38) calls `assert_match_all`, an entry point that may lead here.",
                    "limitation_last_established_edge: test `test_basic` (tests/test_version_req.rs:38) -> entry `assert_match_all`",
                    "limitation_first_unresolved_edge: entry `assert_match_all` -> owner `matches_greater` through a transitive Rust helper path",
                    "limitation_analyzer_route: analysis/rust-public-api-transitive-reach",
                    "limitation_non_claim: named limitation only; ripr cannot confirm or deny that this path observes the change"
                ]
            }
        ]
    })
}

fn harness_limitation_case(
    assertions: Vec<super::EvidencePromotionSemanticAssertion>,
) -> super::EvidencePromotionExternalCase {
    super::EvidencePromotionExternalCase {
        id: "rust_harness_dead_construction_no_exposed_credit".to_string(),
        language: "rust".to_string(),
        external_repo: String::new(),
        external_commit: String::new(),
        external_patch: PathBuf::new(),
        external_command: String::new(),
        runtime_budget_seconds: 0,
        artifact_budget_bytes: 0,
        assertions,
    }
}

fn harness_check_json(limitation_code: Option<&str>) -> serde_json::Value {
    let limitations = limitation_code
        .map(|code| serde_json::json!([{"code": code, "file": "tests/mimic.rs", "line": 5}]))
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::json!({
        "findings": [],
        "test_harnesses": [
            {"registration_id": "mimic-suite", "limitations": limitations}
        ]
    })
}

fn write_editor_adoption_assurance_corpus(root: &Path) {
    let corpus = root.join("fixtures/editor_adoption_assurance");
    write(
        &corpus.join("SPEC.md"),
        "# Fixture Corpus: editor_adoption_assurance\n\nSpec: RIPR-SPEC-0054\n\n## Given\n\nSetup artifacts exist.\n\n## When\n\nThe adoption assurance contract is checked.\n\n## Then\n\nVisible states are pinned.\n\n## Must Not\n\n- Claim source edits.\n",
    );
    for case in super::EDITOR_ADOPTION_ASSURANCE_CASES {
        write_editor_adoption_status(
            root,
            case,
            editor_adoption_status(case),
            editor_adoption_projection(case),
        );
        write_editor_adoption_actions(root, case, &editor_adoption_actions(case));
        write_editor_adoption_first_pr(root, case, editor_adoption_first_pr_state(case));
        write_editor_adoption_receipt(
            root,
            case,
            editor_adoption_receipt_state(case),
            false,
            false,
        );
        write_editor_adoption_diagnostics(root, case, *case == "first_pr_packet_ready");
        write(
            &root.join(format!(
                "fixtures/editor_adoption_assurance/{case}/expected/setup-diagnosis.md"
            )),
            "# RIPR setup diagnosis\n\nCompatibility: compatible.\n\nWorkspace: selected.\n\nFirst PR packet: projected.\n\nReceipt: projected.\n\nNext safe action: refresh or inspect.\n\nLimits: no source edits; not a gate decision; not runtime proof.\n",
        );
    }
}

fn editor_adoption_status(case: &str) -> &'static str {
    match case {
        "setup_ok" => "setup_ok",
        "server_missing" => "server_missing",
        "server_version_mismatch" => "server_version_mismatch",
        "no_workspace" => "no_workspace",
        "multi_root" => "multi_root_ambiguous",
        "wrong_root_artifact" => "wrong_root_artifact",
        "stale_receipt" => "receipt_stale",
        "first_pr_packet_ready" => "first_pr_packet_ready",
        "first_pr_packet_mismatch" => "first_pr_packet_mismatch",
        "preview_adapter_unavailable" => "preview_adapter_unavailable",
        _ => "unknown",
    }
}

fn editor_adoption_projection(case: &str) -> &'static str {
    if super::editor_adoption_assurance_case_fails_closed(case) {
        "fail_closed"
    } else {
        "active"
    }
}

fn editor_adoption_actions(case: &str) -> Vec<&'static str> {
    if case == "first_pr_packet_ready" {
        vec![
            "First PR: open packet",
            "First PR: copy repair packet",
            "First PR: copy verify command",
            "First PR: copy receipt command",
            "Refresh Analysis - Saved Workspace Check",
        ]
    } else {
        vec!["Refresh Analysis - Saved Workspace Check"]
    }
}

fn editor_adoption_first_pr_state(case: &str) -> &'static str {
    match case {
        "setup_ok" => "missing",
        "wrong_root_artifact" => "wrong_root",
        "first_pr_packet_ready" => "top_repairable_gap",
        "first_pr_packet_mismatch" => "gap_mismatch",
        _ => "not_projected",
    }
}

fn editor_adoption_receipt_state(case: &str) -> &'static str {
    match case {
        "setup_ok" | "first_pr_packet_ready" => "receipt_missing",
        "wrong_root_artifact" => "receipt_wrong_root",
        "stale_receipt" => "receipt_stale",
        "first_pr_packet_mismatch" => "receipt_gap_mismatch",
        _ => RECEIPT_NOT_APPLICABLE,
    }
}

fn write_editor_adoption_status(root: &Path, case: &str, status: &str, projection: &str) {
    write(
        &root.join(format!(
            "fixtures/editor_adoption_assurance/{case}/expected/vscode-status.json"
        )),
        &serde_json::json!({
            "schema_version": "0.1",
            "fixture": format!("editor_adoption_assurance/{case}"),
            "status": status,
            "projection": projection,
            "next_safe_action": "refresh or inspect"
        })
        .to_string(),
    );
}

fn write_editor_adoption_actions(root: &Path, case: &str, titles: &[&str]) {
    let actions = titles
        .iter()
        .map(|title| {
            serde_json::json!({
                "title": title,
                "command": match *title {
                    "First PR: open packet" => "ripr.openFirstPrPacket",
                    "First PR: copy repair packet" => "ripr.copyFirstPrRepairPacket",
                    "First PR: copy verify command" => "ripr.copyFirstPrVerifyCommand",
                    "First PR: copy receipt command" => "ripr.copyFirstPrReceiptCommand",
                    _ => "ripr.refresh",
                }
            })
        })
        .collect::<Vec<_>>();
    write(
        &root.join(format!(
            "fixtures/editor_adoption_assurance/{case}/expected/lsp-code-actions.json"
        )),
        &serde_json::json!({
            "schema_version": "0.1",
            "fixture": format!("editor_adoption_assurance/{case}"),
            "actions": actions
        })
        .to_string(),
    );
}

fn write_editor_adoption_first_pr(root: &Path, case: &str, packet_state: &str) {
    write(
        &root.join(format!(
            "fixtures/editor_adoption_assurance/{case}/expected/first-pr-status.json"
        )),
        &serde_json::json!({
            "schema_version": "0.1",
            "fixture": format!("editor_adoption_assurance/{case}"),
            "packet_state": packet_state,
            "runtime_adequacy_claim": false,
            "mutation_proof_claim": false,
            "policy_gate_claim": false,
            "pr_ready_claim": false
        })
        .to_string(),
    );
}

fn write_editor_adoption_receipt(
    root: &Path,
    case: &str,
    receipt_state: &str,
    runtime_adequacy_claim: bool,
    gate_eligibility_claim: bool,
) {
    write(
        &root.join(format!(
            "fixtures/editor_adoption_assurance/{case}/expected/receipt-status.json"
        )),
        &serde_json::json!({
            "schema_version": "0.1",
            "fixture": format!("editor_adoption_assurance/{case}"),
            "receipt_state": receipt_state,
            "runtime_adequacy_claim": runtime_adequacy_claim,
            "mutation_proof_claim": false,
            "gate_eligibility_claim": gate_eligibility_claim,
            "merge_readiness_claim": false
        })
        .to_string(),
    );
}

fn write_editor_adoption_diagnostics(root: &Path, case: &str, include_diagnostic: bool) {
    let diagnostics = if include_diagnostic {
        vec![serde_json::json!({
            "source": "ripr",
            "message": "gap",
            "data": {
                "canonical_gap_id": "gap:rust:pricing:discount:threshold-boundary",
                "language": "rust",
                "language_status": "stable"
            }
        })]
    } else {
        Vec::new()
    };
    write(
        &root.join(format!(
            "fixtures/editor_adoption_assurance/{case}/expected/lsp-diagnostics.json"
        )),
        &serde_json::json!({
            "schema_version": "0.1",
            "fixture": format!("editor_adoption_assurance/{case}"),
            "diagnostics": diagnostics
        })
        .to_string(),
    );
}

fn write_editor_first_run_usability_corpus(root: &Path) {
    let corpus = root.join("fixtures/editor_first_run_usability");
    write(
        &corpus.join("SPEC.md"),
        "# Fixture Corpus: editor_first_run_usability\n\nSpec: RIPR-SPEC-0049\nSpec: RIPR-SPEC-0050\n\n## Given\n\nEditor setup artifacts exist.\n\n## When\n\nThe first-run usability contract is checked.\n\n## Then\n\nVisible states are pinned.\n\n## Must Not\n\n- Claim source edits.\n",
    );
    for case in super::EDITOR_FIRST_RUN_USABILITY_CASES {
        write_editor_first_run_status(
            root,
            case,
            editor_first_run_projection(case),
            editor_first_run_next_action(case),
        );
        write_editor_first_run_actions(root, case, &editor_first_run_actions(case));
        write_editor_first_run_receipt(
            root,
            case,
            editor_first_run_receipt_state(case),
            false,
            false,
        );
        write(
            &root.join(format!(
                "fixtures/editor_first_run_usability/{case}/expected/setup-diagnosis.md"
            )),
            "# RIPR setup diagnosis\n\nNext safe action: refresh or inspect.\n\nLimits: no source edits.\n",
        );
    }
}

fn editor_first_run_projection(case: &str) -> &'static str {
    match case {
        "server_missing"
        | "language_disabled"
        | "adapter_unavailable"
        | "artifact_stale"
        | "receipt_gap_mismatch" => "fail_closed",
        _ => "active",
    }
}

fn editor_first_run_next_action(case: &str) -> Option<&'static str> {
    match case {
        "setup_ok" | "receipt_found" | "receipt_improved" | "receipt_unchanged" => {
            Some("copy first repair packet")
        }
        _ => None,
    }
}

fn editor_first_run_actions(case: &str) -> Vec<&'static str> {
    match case {
        "setup_ok" | "receipt_found" | "receipt_improved" | "receipt_unchanged" => {
            vec![
                "Copy first repair packet",
                "Refresh Analysis - Saved Workspace Check",
            ]
        }
        _ => vec!["Refresh Analysis - Saved Workspace Check"],
    }
}

fn editor_first_run_receipt_state(case: &str) -> &'static str {
    match case {
        "receipt_improved" => "receipt_movement_improved",
        "receipt_unchanged" => "receipt_movement_unchanged",
        "receipt_found" => "receipt_found",
        "receipt_gap_mismatch" => "receipt_gap_mismatch",
        "artifact_stale" => "receipt_stale",
        _ => "receipt_missing",
    }
}

fn write_editor_first_run_status(
    root: &Path,
    case: &str,
    projection: &str,
    next_safe_action: Option<&str>,
) {
    let mut status = serde_json::json!({
        "schema_version": "0.1",
        "fixture": format!("editor_first_run_usability/{case}"),
        "projection": projection
    });
    if let Some(next_safe_action) = next_safe_action {
        status["next_safe_action"] = serde_json::json!(next_safe_action);
    }
    write(
        &root.join(format!(
            "fixtures/editor_first_run_usability/{case}/expected/vscode-status.json"
        )),
        &status.to_string(),
    );
}

fn write_editor_first_run_actions(root: &Path, case: &str, titles: &[&str]) {
    let actions = titles
            .iter()
            .map(|title| {
                serde_json::json!({
                    "title": title,
                    "command": if *title == "Copy first repair packet" {
                        "ripr.copyContext"
                    } else {
                        "ripr.refresh"
                    },
                    "arguments": if *title == "Copy first repair packet" {
                        serde_json::json!([{
                            "packet": "RIPR first repair packet\nGap identity: gap:test\nSuggested action:\n- Add one focused assertion.\nVerify command:\nripr agent verify --root <root> --json\nReceipt command:\nripr agent receipt --root <root> --json\nLimits and non-claims:\n- Static editor evidence only."
                        }])
                    } else {
                        serde_json::json!([])
                    },
                })
            })
            .collect::<Vec<_>>();
    write(
        &root.join(format!(
            "fixtures/editor_first_run_usability/{case}/expected/lsp-code-actions.json"
        )),
        &serde_json::json!({
            "schema_version": "0.1",
            "actions": actions
        })
        .to_string(),
    );
}

fn write_editor_first_run_receipt(
    root: &Path,
    case: &str,
    receipt_state: &str,
    runtime_adequacy_claim: bool,
    gate_eligibility_claim: bool,
) {
    write(
        &root.join(format!(
            "fixtures/editor_first_run_usability/{case}/expected/receipt-status.json"
        )),
        &serde_json::json!({
            "schema_version": "0.1",
            "receipt_state": receipt_state,
            "runtime_adequacy_claim": runtime_adequacy_claim,
            "gate_eligibility_claim": gate_eligibility_claim
        })
        .to_string(),
    );
}

fn write_editor_gap_cockpit_corpus(root: &Path, valid_preview_hover_order: bool) {
    let corpus = root.join("fixtures/editor_gap_cockpit");
    write(
        &corpus.join("SPEC.md"),
        "# Fixture Corpus: editor_gap_cockpit\n\nSpec: RIPR-SPEC-0047\n\n## Given\n\nArtifacts exist.\n\n## When\n\nThe LSP cockpit report runs.\n\n## Then\n\nCases are pinned.\n\n## Must Not\n\n- Edit source.\n",
    );
    for (case, language, language_status, diagnostics, actions, hover) in [
        (
            "rust_actionable",
            "rust",
            "stable",
            true,
            vec![
                "Inspect gap: copy repair packet",
                "Write targeted test: open best related test",
                "Verify after test: copy verify command",
                "Review result: copy receipt command",
                "Refresh Analysis - Saved Workspace Check",
            ],
            "## Evidence boundary\n\nLanguage: rust\n\n## Gap state\n\nactionable\n\n## Limits\n\nStatic evidence only.\n",
        ),
        (
            "typescript_preview_static_limit",
            "typescript",
            "preview",
            true,
            vec![
                "Inspect gap: copy repair packet",
                "Inspect gap: copy static-limit note",
                "Refresh Analysis - Saved Workspace Check",
            ],
            if valid_preview_hover_order {
                "## Evidence boundary\n\nStatic limit: mocked_module\n\n## Gap state\n\nactionable\n\nSuggested action: add one assertion.\n\n## Limits\n\nStatic evidence only.\n"
            } else {
                "## Evidence boundary\n\nSuggested action: add one assertion.\n\n## Gap state\n\nactionable\n\nStatic limit: mocked_module\n\n## Limits\n\nStatic evidence only.\n"
            },
        ),
        (
            "python_preview_static_limit",
            "python",
            "preview",
            true,
            vec![
                "Inspect gap: copy repair packet",
                "Inspect gap: copy static-limit note",
                "Refresh Analysis - Saved Workspace Check",
            ],
            "## Evidence boundary\n\nStatic limit: missing_import_graph\n\n## Gap state\n\nactionable\n\nSuggested action: add one assertion.\n\n## Limits\n\nStatic evidence only.\n",
        ),
        (
            "disabled_language",
            "python",
            "preview",
            false,
            vec!["Refresh Analysis - Saved Workspace Check"],
            "## Evidence boundary\n\nLanguage disabled.\n\n## Gap state\n\ndisabled\n\n## Limits\n\nStatic evidence only.\n",
        ),
        (
            "wrong_root",
            "rust",
            "stable",
            false,
            vec!["Refresh Analysis - Saved Workspace Check"],
            "## Evidence boundary\n\nWrong root.\n\n## Gap state\n\nwrong_root\n\n## Limits\n\nStatic evidence only.\n",
        ),
        (
            "stale_artifact",
            "rust",
            "stable",
            false,
            vec!["Refresh Analysis - Saved Workspace Check"],
            "## Evidence boundary\n\nStale.\n\n## Gap state\n\nstale\n\n## Limits\n\nStatic evidence only.\n",
        ),
        (
            "no_actionable_gap",
            "rust",
            "stable",
            false,
            vec!["Refresh Analysis - Saved Workspace Check"],
            "## Evidence boundary\n\nNo action.\n\n## Gap state\n\nno action\n\n## Limits\n\nStatic evidence only.\n",
        ),
    ] {
        let expected = corpus.join(case).join("expected");
        let diagnostic_items = if diagnostics {
            vec![serde_json::json!({
                "uri": "src/lib.rs",
                "message": "gap",
                "data": {
                    "canonical_gap_id": format!("gap:{case}"),
                    "language": language,
                    "language_status": language_status
                }
            })]
        } else {
            Vec::new()
        };
        let action_items = actions
            .iter()
            .map(|title| {
                serde_json::json!({
                    "title": title,
                    "command": if *title == "Refresh Analysis - Saved Workspace Check" {
                        "ripr.refresh"
                    } else {
                        "ripr.copyContext"
                    },
                    "arguments": []
                })
            })
            .collect::<Vec<_>>();
        write(
            &expected.join("lsp-diagnostics.json"),
            &serde_json::json!({
                "fixture": format!("editor_gap_cockpit/{case}"),
                "diagnostics": diagnostic_items
            })
            .to_string(),
        );
        write(
            &expected.join("lsp-code-actions.json"),
            &serde_json::json!({
                "fixture": format!("editor_gap_cockpit/{case}"),
                "actions": action_items
            })
            .to_string(),
        );
        write(&expected.join("lsp-hover.md"), hover);
        write(
            &expected.join("vscode-status.json"),
            &serde_json::json!({
                "schema_version": "0.1",
                "fixture": format!("editor_gap_cockpit/{case}"),
                "states": []
            })
            .to_string(),
        );
        write(
            &expected.join("gap-projection.json"),
            &serde_json::json!({
                "schema_version": "0.1",
                "case": case
            })
            .to_string(),
        );
    }
}

fn json_path(path: &Path) -> String {
    json_escape(&path.display().to_string())
}

fn write_assistant_loop_health_corpus(
    base: &Path,
    report: &Path,
    markdown: &Path,
    proof: &Path,
    expected_status: &str,
    repair_queue: usize,
) {
    write(&base.join("README.md"), "# Assistant Loop Health Corpus\n");
    write(proof, "{}\n");
    let expected = format!(
        r#"{{
        "status": "{expected_status}",
        "proofs": 1,
        "complete": 1,
        "partial": 0,
        "missing_required_input": 0,
        "missing_optional_input": 0,
        "improved": 1,
        "unchanged": 0,
        "regressed": 0,
        "unknown_movement": 0,
        "warnings": 0,
        "repair_queue": {repair_queue}
      }}"#
    );
    let cases = [
        "complete_improved",
        "partial_missing_optional",
        "missing_required_input",
        "unchanged_after_attempt",
        "regressed_after_attempt",
        "warning_heavy",
        "multi_proof",
    ]
    .into_iter()
    .map(|id| {
        format!(
            r#"{{
      "id": "{id}",
      "proofs": ["{}"],
      "expected_report": "{}",
      "expected_markdown": "{}",
      "expected": {expected}
    }}"#,
            json_path(proof),
            json_path(report),
            json_path(markdown)
        )
    })
    .collect::<Vec<_>>()
    .join(",\n");
    write(
        &base.join("corpus.json"),
        &format!(
            r#"{{
  "kind": "assistant_loop_health_corpus",
  "spec": "RIPR-SPEC-0022",
  "cases": [
{cases}
  ]
}}
"#
        ),
    );
}

fn write_pr_review_front_panel_corpus(
    base: &Path,
    report: &Path,
    markdown: &Path,
    expected_status: &str,
    blocking_candidates: usize,
) {
    write(&base.join("README.md"), "# PR Review Front Panel Corpus\n");
    let expected = format!(
        r#"{{
        "status": "{expected_status}",
        "top_issue_state": "actionable",
        "policy_state": "new_policy_eligible",
        "placement": "changed_line",
        "movement_state": "unknown",
        "coverage_grip_state": "not_available",
        "new_policy_eligible": 1,
        "baseline_resolved": 0,
        "blocking_candidates": {blocking_candidates},
        "warnings": 0
      }}"#
    );
    let cases = [
        "advisory_only",
        "actionable",
        "summary_only",
        "acknowledged",
        "suppressed",
        "baseline_resolved",
        "blocked",
        "missing_proof",
        "coverage_flat_grip_improved",
        "stale_same_path_receipt_fails_closed",
    ]
    .into_iter()
    .map(|id| {
        format!(
            r#"{{
      "id": "{id}",
      "expected_report": "{}",
      "expected_markdown": "{}",
      "expected": {expected}
    }}"#,
            json_path(report),
            json_path(markdown)
        )
    })
    .collect::<Vec<_>>()
    .join(",\n");
    write(
        &base.join("corpus.json"),
        &format!(
            r#"{{
  "kind": "pr_review_front_panel_corpus",
  "spec": "RIPR-SPEC-0023",
  "cases": [
{cases}
  ]
}}
"#
        ),
    );
}

fn write_report_packet_index_corpus(
    base: &Path,
    report: &Path,
    markdown: &Path,
    expected_status: &str,
    missing_expected: usize,
    failures: usize,
    gate_authority_present: bool,
) {
    write(&base.join("README.md"), "# Report Packet Index Corpus\n");
    // The corpus contract requires a real packet directory. These guard tests
    // exercise the declaration checks only, so the tree's contents do not
    // matter; `write` is this file's owner for creating one.
    let packet_root = base.join("packet");
    write(
        &packet_root.join("README.md"),
        "# Synthetic packet root for the report-packet-index corpus guard tests\n",
    );
    let expected = format!(
        r#"{{
        "status": "{expected_status}",
        "missing_expected": {missing_expected},
        "failures": {failures},
        "warnings": 0,
        "start_here_available": true,
        "gate_authority_present": {gate_authority_present},
        "required_groups": ["start_here", "pr_review_story"]
      }}"#
    );
    let cases = [
        "complete_packet",
        "sparse_advisory",
        "missing_front_panel",
        "blocked_gate",
        "missing_assistant_proof",
        "missing_receipts",
        "coverage_grip_present",
    ]
    .into_iter()
    .map(|id| {
        format!(
            r#"{{
      "id": "{id}",
      "packet_root": "{}",
      "expected_report": "{}",
      "expected_markdown": "{}",
      "expected": {expected}
    }}"#,
            json_path(&packet_root),
            json_path(report),
            json_path(markdown)
        )
    })
    .collect::<Vec<_>>()
    .join(",\n");
    write(
        &base.join("corpus.json"),
        &format!(
            r#"{{
  "kind": "report_packet_index_corpus",
  "spec": "RIPR-SPEC-0024",
  "canonical_command": "ripr reports index --root . --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md",
  "cases": [
{cases}
  ]
}}
"#
        ),
    );
}

fn write_pr_inline_comment_publisher_corpus(
    base: &Path,
    report: &Path,
    markdown: &Path,
    expected_mode: &str,
    publishable: usize,
    skipped: usize,
    blocked: usize,
) {
    write(
        &base.join("README.md"),
        "# PR Inline Comment Publisher Corpus\n",
    );
    let expected = format!(
        r#"{{
        "status": "advisory",
        "mode": "{expected_mode}",
        "publishable": {publishable},
        "skipped": {skipped},
        "blocked": {blocked},
        "safe_to_publish": false,
        "operations": ["create"],
        "skip_reasons": [],
        "blocked_reasons": []
      }}"#
    );
    let cases = [
        "publishable_changed_line",
        "summary_only_excluded",
        "cap_overflow",
        "dedupe_upsert",
        "stale_existing",
        "fork_or_no_token",
        "missing_input",
    ]
    .into_iter()
    .map(|id| {
        format!(
            r#"{{
      "id": "{id}",
      "expected_report": "{}",
      "expected_markdown": "{}",
      "expected": {expected}
    }}"#,
            json_path(report),
            json_path(markdown)
        )
    })
    .collect::<Vec<_>>()
    .join(",\n");
    write(
        &base.join("corpus.json"),
        &format!(
            r#"{{
  "kind": "pr_inline_comment_publisher_corpus",
  "spec": "RIPR-SPEC-0025",
  "cases": [
{cases}
  ]
}}
"#
        ),
    );
}

fn write_editor_gap_case_expected(
    root: &Path,
    case: &str,
    diagnostics: &str,
    actions: &str,
    hover: &str,
) {
    let expected = root.join(case).join("expected");
    write(
        &expected.join("gap-projection.json"),
        &format!(
            r#"{{
  "schema_version": "0.1",
  "case": "{case}"
}}
"#
        ),
    );
    write(&expected.join("lsp-diagnostics.json"), diagnostics);
    write(&expected.join("lsp-code-actions.json"), actions);
    write(&expected.join("lsp-hover.md"), hover);
    write(
        &expected.join("vscode-status.json"),
        r#"{
  "status": "pass"
}
"#,
    );
}

#[cfg(unix)]
fn success_exit_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatusExt::from_raw(0)
}

#[cfg(windows)]
fn success_exit_status() -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatusExt::from_raw(0)
}

#[cfg(unix)]
fn failure_exit_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatusExt::from_raw(1 << 8)
}

#[cfg(windows)]
fn failure_exit_status() -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatusExt::from_raw(1)
}

fn evidence_quality_benchmark_corpus_path() -> Result<PathBuf, String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest must have workspace parent".to_string())?;
    Ok(repo_root.join("fixtures/evidence-quality-benchmark/corpus.json"))
}

fn evidence_quality_benchmark_corpus_value() -> Result<Value, String> {
    let corpus = evidence_quality_benchmark_corpus_path()?;
    super::read_json_value(&corpus)
}

fn evidence_quality_benchmark_violations(corpus: &Value) -> Vec<String> {
    let mut violations = Vec::new();
    super::validate_evidence_quality_benchmark_corpus_value(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/evidence-quality-benchmark/corpus.json"),
        corpus,
        &mut violations,
    );
    violations
}

fn evidence_quality_benchmark_case_mut<'a>(
    corpus: &'a mut Value,
    case_id: &str,
) -> Result<&'a mut Value, String> {
    let cases = corpus
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "benchmark corpus must have cases array".to_string())?;
    cases
        .iter_mut()
        .find(|case| case.get("id").and_then(Value::as_str) == Some(case_id))
        .ok_or_else(|| format!("benchmark corpus is missing case {case_id}"))
}

fn perl_lsp_facts_exporter_corpus_path() -> Result<PathBuf, String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest must have workspace parent".to_string())?;
    Ok(repo_root.join("fixtures/perl_lsp_facts_exporter/corpus.json"))
}

fn perl_real_repo_eval_corpus_path() -> Result<PathBuf, String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest must have workspace parent".to_string())?;
    Ok(repo_root.join("fixtures/perl-real-repo-evals/corpus.json"))
}

use super::fixture_contracts::{
    validate_perl_packet_contract_migration_corpus,
    validate_perl_packet_contract_migration_corpus_at,
};

fn perl_packet_contract_migration_corpus_path() -> Result<PathBuf, String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest must have workspace parent".to_string())?;
    Ok(repo_root.join("fixtures/perl_packet_contract_migration/corpus.json"))
}

pub(crate) fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|err| err.to_string())?;
    for entry in fs::read_dir(source).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let target = destination.join(entry.file_name());
        if entry.file_type().map_err(|err| err.to_string())?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

/// Copies the committed perl packet contract migration fixture into the temp
/// cwd under `fixtures/...` so the corpus's repo-root-relative paths resolve
/// against the copy, then returns the copied fixture root.
fn copy_perl_packet_contract_migration_fixture(root: &Path) -> Result<PathBuf, String> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let fixture_root = root.join("fixtures/perl_packet_contract_migration");
    copy_dir_recursive(
        &repo_root.join("fixtures/perl_packet_contract_migration"),
        &fixture_root,
    )?;
    Ok(fixture_root)
}

fn gap_decision_ledger_corpus_path() -> Result<PathBuf, String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest must have workspace parent".to_string())?;
    Ok(repo_root.join("fixtures/gap-decision-ledger/corpus.json"))
}

fn gap_decision_ledger_corpus_value() -> Result<Value, String> {
    let corpus = gap_decision_ledger_corpus_path()?;
    super::read_json_value(&corpus)
}

fn gap_decision_ledger_violations(corpus: &Value) -> Vec<String> {
    let mut violations = Vec::new();
    super::validate_gap_decision_ledger_corpus_value(
        Path::new("fixtures/gap-decision-ledger/corpus.json"),
        corpus,
        &mut violations,
    );
    violations
}

fn gap_decision_ledger_case_mut<'a>(
    corpus: &'a mut Value,
    case_id: &str,
) -> Result<&'a mut Value, String> {
    let cases = corpus
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "gap-decision ledger corpus must have cases array".to_string())?;
    cases
        .iter_mut()
        .find(|case| case.get("id").and_then(Value::as_str) == Some(case_id))
        .ok_or_else(|| format!("gap-decision ledger corpus is missing case {case_id}"))
}

fn first_successful_pr_corpus_path() -> Result<PathBuf, String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest must have workspace parent".to_string())?;
    Ok(repo_root.join("fixtures/first_successful_pr/corpus.json"))
}

pub(crate) fn with_temp_cwd<T>(name: &str, f: impl FnOnce(&Path) -> T) -> T {
    let lock = acquire_test_cwd_write_guard();
    let old = std::env::current_dir().unwrap();
    let root = temp_dir(name);
    std::env::set_current_dir(&root).unwrap();

    let out = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&root))) {
        Ok(result) => result,
        Err(panic_payload) => {
            if let Ok(()) = std::env::set_current_dir(&old) {}
            drop(lock);
            ignore_remove_dir_all(&root);
            std::panic::resume_unwind(panic_payload);
        }
    };

    std::env::set_current_dir(old).unwrap();
    drop(lock);
    ignore_remove_dir_all(&root);
    out
}

fn with_repo_cwd<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let guard = acquire_test_cwd_write_guard();
    let old =
        std::env::current_dir().map_err(|err| format!("failed to capture current dir: {err}"))?;
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Some(repo_root) = manifest_dir.parent() else {
        drop(guard);
        return Err(format!(
            "failed to resolve repo root from {}",
            manifest_dir.display()
        ));
    };
    std::env::set_current_dir(repo_root).map_err(|err| format!("failed to set repo cwd: {err}"))?;
    // Restore the cwd even when the guarded body panics, mirroring
    // `with_temp_cwd`: a leaked repo-root cwd would poison every other
    // cwd-sensitive test in the process.
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic_payload) => {
            if let Ok(()) = std::env::set_current_dir(&old) {}
            drop(guard);
            std::panic::resume_unwind(panic_payload);
        }
    };
    let restore =
        std::env::set_current_dir(&old).map_err(|err| format!("failed to restore cwd: {err}"));
    drop(guard);
    restore?;
    result
}

fn targeted_static_seam(id: &str, grip_class: &str) -> StaticSeamRecord {
    StaticSeamRecord {
        seam_id: id.to_string(),
        seam_kind: "predicate_boundary".to_string(),
        file: "src/pricing.rs".to_string(),
        line: 42,
        seam_grip_class: grip_class.to_string(),
        oracle_kind: "exact_value".to_string(),
        oracle_strength: "weak".to_string(),
        observed_values: vec!["50".to_string()],
        missing_discriminators: Vec::new(),
    }
}

fn mutation_record(
    mutant_id: &str,
    seam_id: Option<&str>,
    runtime_outcome: &str,
) -> MutationOutcomeRecord {
    mutation_record_at(mutant_id, seam_id, "src/pricing.rs", 42, runtime_outcome)
}

fn mutation_record_at(
    mutant_id: &str,
    seam_id: Option<&str>,
    file: &str,
    line: usize,
    runtime_outcome: &str,
) -> MutationOutcomeRecord {
    MutationOutcomeRecord {
        mutant_id: Some(mutant_id.to_string()),
        seam_id: seam_id.map(str::to_string),
        file: Some(file.to_string()),
        line: Some(line),
        mutation_operator: "replace >= with >".to_string(),
        runtime_outcome: runtime_outcome.to_string(),
        duration: None,
        test_command: Some("cargo test pricing".to_string()),
    }
}

fn allowlist_with_docs_markdown_globs() -> Vec<StaticLanguageAllowEntry> {
    vec![
        StaticLanguageAllowEntry::new_path("AGENTS.md", "test", "test fixture"),
        StaticLanguageAllowEntry::new_glob("docs/*.md", "test", "test fixture"),
        StaticLanguageAllowEntry::new_glob("docs/**/*.md", "test", "test fixture"),
    ]
}

fn sample_rust_conversion_candidates() -> Vec<super::RustConversionCandidate> {
    vec![
        super::RustConversionCandidate {
            path: "scripts/check.py".to_string(),
            line: None,
            kind: "non_rust_programming_without_retention_rule".to_string(),
            priority: "high".to_string(),
            current_surface: "unapproved non-Rust implementation or automation".to_string(),
            recommendation: "Move this behavior into xtask.".to_string(),
            reason: "Rust is the default implementation surface.".to_string(),
        },
        super::RustConversionCandidate {
            path: ".github/workflows/pr-plan.yml".to_string(),
            line: Some(42),
            kind: "workflow_shell_logic".to_string(),
            priority: "medium".to_string(),
            current_surface: "GitHub Actions shell run block".to_string(),
            recommendation: "Move report assembly into cargo xtask.".to_string(),
            reason: "Workflow shell is harder to type-check and test.".to_string(),
        },
        super::RustConversionCandidate {
            path: "editors/vscode/src/extension.ts".to_string(),
            line: None,
            kind: "retained_external_runtime".to_string(),
            priority: "retained".to_string(),
            current_surface: "VS Code extension TypeScript".to_string(),
            recommendation: "Keep this code in the editor adapter.".to_string(),
            reason: "The VS Code Extension Host API is TypeScript-native.".to_string(),
        },
    ]
}

/// A pushed `v*` tag must not start publication, and the extension workflow
/// must neither create the GitHub Release nor replace an attached asset. Each
/// channel is dispatched explicitly per docs/RELEASE_TRANSACTION.md; #1646 owns
/// the full single-writer topology.
/// Keys directly under a workflow's block-form `on:` mapping, in order.
/// `None` for a missing or inline `on:` (`on: push`, `on: [push]`).
fn workflow_trigger_keys(workflow: &str) -> Option<Vec<&str>> {
    let (_, rest) = workflow.split_once("\non:\n")?;
    Some(
        rest.lines()
            .take_while(|line| line.is_empty() || line.starts_with(' ') || line.starts_with('#'))
            .filter_map(|line| line.strip_prefix("  "))
            .filter(|line| !line.starts_with(' ') && !line.starts_with('#'))
            .filter_map(|line| line.split_once(':').map(|(key, _)| key))
            .collect(),
    )
}

fn sample_first_pr_run(name: &str, status: &str, state: &str) -> DogfoodFirstPrRun {
    let expected_dir = Path::new("fixtures/first_successful_pr")
        .join(name)
        .join("expected");
    DogfoodFirstPrRun {
        name: name.to_string(),
        expected_dir: expected_dir.clone(),
        json_path: expected_dir.join("start-here.json"),
        markdown_path: expected_dir.join("start-here.md"),
        status: status.to_string(),
        state: state.to_string(),
        top_gap_kind: "none".to_string(),
        verify_command: None,
        next_command: None,
        expected_status: status.to_string(),
        expected_state: state.to_string(),
        description: "sample first-pr run".to_string(),
        errors: Vec::new(),
    }
}

fn valid_finding_alignment_scenario(
    name: &str,
    gap_state: &str,
) -> DogfoodFindingAlignmentScenario {
    DogfoodFindingAlignmentScenario {
        name: name.to_string(),
        source_pr: "EffortlessMetrics/ripr#1016".to_string(),
        canonical_gap_id: format!("gap:{name}"),
        evidence_class: "presentation_text".to_string(),
        raw_findings_total: 2,
        canonical_items_total: 1,
        raw_finding_summary: "Declaration and literal raw findings align into one canonical item."
            .to_string(),
        gap_state: gap_state.to_string(),
        actionability: "add_output_observer".to_string(),
        user_outcome: "actionable_gap".to_string(),
        repair_kind: "output_observer".to_string(),
        target_test_type: "help_output_snapshot".to_string(),
        verify_command: "cargo xtask evidence-quality-scorecard".to_string(),
        static_limitation_category: None,
        static_limitation_repair_route: None,
        raw_findings_supporting_only: true,
        recommended_repair: "Add or update an output observer.".to_string(),
        before_after_context: "Before and after scorecard context is pinned by this dogfood case."
            .to_string(),
        must_not_claim: vec![
            "Do not infer actionability from raw static class.".to_string(),
            "Do not recommend mutation testing first.".to_string(),
        ],
        reason: "fixture-backed dogfood receipt".to_string(),
    }
}

fn valid_surface_projection_alignment_scenario() -> DogfoodSurfaceProjectionAlignmentScenario {
    let canonical_gap_id = "gap:boundary-discriminator-004";
    let packet_id = "boundary-discriminator-004";
    let repair_kind = "add_boundary_assertion";
    let verify_command = "cargo test -p ripr-swarm boundary_discriminator_004";
    let receipt_command = "cargo xtask receipts write --packet boundary-discriminator-004 --canonical-gap gap:boundary-discriminator-004";
    let attempt = serde_json::json!({
        "packet_id": packet_id,
        "canonical_gap_id": canonical_gap_id,
        "attempt_id": "attempt:boundary-discriminator-004:evidence-improved",
        "evidence_class": "predicate_boundary",
        "repair_kind": repair_kind,
        "actor_kind": "human",
        "receipt_path": "target/ripr/receipts/boundary-discriminator-004.json",
        "verify_command": verify_command,
        "receipt_command": receipt_command,
        "before_gap_state": "actionable",
        "after_gap_state": "actionable",
        "outcome": "evidence_improved",
        "timestamp": "2026-05-27T20:00:00Z",
        "receipt_state": "receipt_movement_improved",
        "movement_source": "agent_receipt",
        "reason": "receipt-backed dogfood attempt improved evidence"
    });
    DogfoodSurfaceProjectionAlignmentScenario {
        name: "receipt_improved_top_next_action_alignment".to_string(),
        canonical_gap_id: canonical_gap_id.to_string(),
        packet_id: packet_id.to_string(),
        evidence_class: "predicate_boundary".to_string(),
        repair_kind: repair_kind.to_string(),
        verify_command: verify_command.to_string(),
        receipt_command: receipt_command.to_string(),
        receipt_state: "receipt_movement_improved".to_string(),
        outcome: "evidence_improved".to_string(),
        expected_top_next_action_kind: "attempt_ready_packet".to_string(),
        advisory_consumers: vec![
            "badge".to_string(),
            "lsp".to_string(),
            "pr_comment".to_string(),
            "ci".to_string(),
        ],
        must_not_change: vec![
            "production behavior".to_string(),
            "public API".to_string(),
            "snapshot format".to_string(),
        ],
        swarm_plan: serde_json::json!({
            "schema_version": "0.1",
            "tool": "ripr",
            "report": "swarm-plan",
            "source_summary": {
                "actionable_gaps": 1,
                "public_projection_eligible_packets": 1
            },
            "summary": {
                "swarm_ready_packets": 1,
                "blocked_packets": 0,
                "missing_verify_command": 0,
                "missing_receipt_command": 0,
                "static_limitation_packets": 0,
                "high_confidence_packets": 1
            },
            "top_ready_packets": [
                {
                    "packet_id": packet_id,
                    "canonical_gap_id": canonical_gap_id,
                    "evidence_class": "predicate_boundary",
                    "repair_kind": repair_kind,
                    "swarm_state": "queued"
                }
            ],
            "top_blocked_packets": []
        }),
        actionable_gap_outcomes: serde_json::json!({
            "schema_version": "0.1",
            "tool": "ripr",
            "report": "actionable-gap-outcomes",
            "summary": {
                "outcomes_total": 1,
                "not_attempted": 0,
                "evidence_improved": 1,
                "evidence_unchanged": 0,
                "evidence_regressed": 0,
                "resolved": 0,
                "orphaned_receipts": 0
            }
        }),
        attempt_ledger: serde_json::json!({
            "schema_version": "0.1",
            "tool": "ripr",
            "report": "swarm-attempt-ledger",
            "summary": {
                "attempts_total": 1,
                "canonical_gaps_total": 1,
                "not_attempted": 0,
                "attempted_no_receipt": 0,
                "receipt_present": 0,
                "evidence_improved": 1,
                "evidence_unchanged": 0,
                "evidence_regressed": 0,
                "resolved": 0,
                "unknown": 0,
                "orphaned_receipts": 0
            },
            "repair_route_quality": [
                {
                    "repair_kind": repair_kind,
                    "repair_kind_attempted": 1,
                    "repair_kind_improved": 1,
                    "repair_kind_unchanged": 0,
                    "repair_kind_regressed": 0,
                    "repair_kind_resolved": 0,
                    "repair_kind_attempted_no_receipt": 0,
                    "repair_kind_receipt_present": 0,
                    "repair_kind_unknown": 0,
                    "repair_kind_success_rate": 1.0
                }
            ],
            "top_missing_evidence_fields": [],
            "latest_attempts": [attempt.clone()],
            "attempts": [attempt],
            "orphaned_receipts": []
        }),
        reason: "single receipt-backed canonical gap projection alignment".to_string(),
    }
}

fn valid_real_repair_attempt_scenario() -> DogfoodRealRepairAttemptScenario {
    DogfoodRealRepairAttemptScenario {
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
    }
}

fn valid_python_real_repo_eval_scenario() -> DogfoodPythonRealRepoEvalScenario {
    DogfoodPythonRealRepoEvalScenario {
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
            receipt_command:
                "ripr outcome --before before-check.json --after after-check.json --format json"
                    .to_string(),
            receipt_result: "pass".to_string(),
            gap_movement: "closed".to_string(),
            closed_gaps: 1,
            usability: "usable".to_string(),
            false_positive_notes: "none observed".to_string(),
            limitation_notes:
                "scoped repair routing is usable alpha; broader Python static facts remain preview/advisory"
                    .to_string(),
            unsupported_limitations: Vec::new(),
            ranked_top_3_findings: vec![super::DogfoodPythonRankedFinding {
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
            }],
            ranked_top_3_limit_reason: Some(
                "focused eval emitted one Python repair card; no rank 2 or 3 repairable finding was present"
                    .to_string(),
            ),
            claim_boundary: vec![
                "Broader Python static facts remain preview/advisory".to_string(),
                "No arbitrary imports or tests were run by RIPR".to_string(),
                "No support-tier promotion".to_string(),
            ],
            reason: "tiny controlled repo proved one Python repair card closes with receipt"
                .to_string(),
        }
}

fn valid_python_ranked_finding(rank: usize) -> super::DogfoodPythonRankedFinding {
    let mut finding = valid_python_real_repo_eval_scenario().ranked_top_3_findings[0].clone();
    finding.rank = rank;
    if rank > 1 {
        finding.canonical_gap_id =
            format!("gap:python:app/pricing.py:calculate_discount:return_value:rank-{rank}");
        finding.missing_discriminator = format!("amount == threshold + {rank}");
        finding.verify_command =
            format!("pytest tests/test_pricing.py::test_calculate_discount_rank_{rank}");
        finding.reason = format!("rank {rank} repair card remained usable in dogfood capture");
    }
    finding
}

fn valid_python_static_limit_eval_scenario() -> DogfoodPythonStaticLimitEvalScenario {
    DogfoodPythonStaticLimitEvalScenario {
            name: "dynamic_dispatch_no_packet_eval".to_string(),
            repo_shape: "dynamic_dispatch_pytest".to_string(),
            source_kind: "scratch_repo".to_string(),
            source_ref: "target/ripr/python-real-repo-evals/dynamic-dispatch-limit".to_string(),
            command: "ripr check --root target/ripr/python-real-repo-evals/dynamic-dispatch-limit --base HEAD~1 --format json".to_string(),
            runtime_ms: 233,
            finding_id: "probe:src_runtime.py:2:python_preview".to_string(),
            changed_owner: "python:src/runtime.py::dispatch".to_string(),
            static_limit_kind: "dynamic_dispatch".to_string(),
            classification: "static_unknown".to_string(),
            stop_reasons: vec!["dynamic_dispatch_unresolved".to_string()],
            related_test_file: "tests/test_runtime.py".to_string(),
            related_test_name: "test_dispatch_total".to_string(),
            why_not_actionable:
                "static limit `dynamic_dispatch` prevents bounded repair routing".to_string(),
            repair_card_present: false,
            agent_packet_present: false,
            verify_command: "not_applicable_static_limit".to_string(),
            verify_result: "not_applicable".to_string(),
            receipt_command: "not_applicable_static_limit".to_string(),
            receipt_result: "not_applicable".to_string(),
            gap_movement: "no_receipt".to_string(),
            false_positive_notes: "none observed".to_string(),
            limitation_notes: "dynamic dispatch stays visible as a no-packet static limitation"
                .to_string(),
            claim_boundary: vec![
                "Broader Python static facts remain preview/advisory".to_string(),
                "No repair packet emitted".to_string(),
                "No support-tier promotion".to_string(),
            ],
            reason:
                "dynamic dispatch dogfood records an honest no-action result instead of a repair card"
                    .to_string(),
        }
}

fn valid_python_unresolved_fixture_static_limit_eval_scenario()
-> DogfoodPythonStaticLimitEvalScenario {
    DogfoodPythonStaticLimitEvalScenario {
            name: "unresolved_fixture_no_packet_eval".to_string(),
            repo_shape: "fixture_sourced_pytest".to_string(),
            source_kind: "scratch_repo".to_string(),
            source_ref: "target/ripr/python-real-repo-evals/unresolved-fixture-limit".to_string(),
            command: "ripr check --root target/ripr/python-real-repo-evals/unresolved-fixture-limit --base HEAD~1 --format json".to_string(),
            runtime_ms: 7700,
            finding_id: "probe:src_pricing.py:2:python_preview".to_string(),
            changed_owner: "python:src/pricing.py::apply_discount".to_string(),
            static_limit_kind: "unresolved_pytest_fixture".to_string(),
            classification: "static_unknown".to_string(),
            stop_reasons: vec!["static_probe_unknown".to_string()],
            related_test_file: "tests/test_pricing.py".to_string(),
            related_test_name: "test_apply_discount_fixture_case".to_string(),
            why_not_actionable:
                "static limit unresolved_pytest_fixture prevents bounded repair routing"
                    .to_string(),
            repair_card_present: false,
            agent_packet_present: false,
            verify_command: "not_applicable_static_limit".to_string(),
            verify_result: "not_applicable".to_string(),
            receipt_command: "not_applicable_static_limit".to_string(),
            receipt_result: "not_applicable".to_string(),
            gap_movement: "no_receipt".to_string(),
            false_positive_notes: "none observed".to_string(),
            limitation_notes:
                "pytest fixture-sourced values stay visible as a no-packet static limitation"
                    .to_string(),
            claim_boundary: vec![
                "Broader Python static facts remain preview/advisory".to_string(),
                "No repair packet emitted".to_string(),
                "No support-tier promotion".to_string(),
            ],
            reason: "unresolved pytest fixture dogfood records an honest no-action result instead of a repair card".to_string(),
        }
}

fn valid_python_no_action_eval_scenario() -> DogfoodPythonNoActionEvalScenario {
    DogfoodPythonNoActionEvalScenario {
            name: "no_related_test_no_packet_eval".to_string(),
            repo_shape: "unrelated_pytest_text_mention".to_string(),
            source_kind: "local_repo".to_string(),
            source_ref: "fixtures/python_unrelated_test_mention".to_string(),
            command: "ripr check --root fixtures/python_unrelated_test_mention/input --diff fixtures/python_unrelated_test_mention/diff.patch --format json".to_string(),
            runtime_ms: 7181,
            finding_id: "probe:src_pricing.py:2:python_preview".to_string(),
            changed_owner: "python:src/pricing.py::apply_discount".to_string(),
            no_action_kind: "no_related_test".to_string(),
            classification: "no_static_path".to_string(),
            stop_reasons: vec!["no_related_test".to_string()],
            related_test_file: "not_applicable_no_related_test".to_string(),
            related_test_name: "not_applicable_no_related_test".to_string(),
            why_not_actionable: "no_action no_related_test prevents bounded repair routing because no related pytest or unittest test statically calls the changed owner".to_string(),
            repair_card_present: false,
            agent_packet_present: false,
            verify_command: "not_applicable_no_action".to_string(),
            verify_result: "not_applicable".to_string(),
            receipt_command: "not_applicable_no_action".to_string(),
            receipt_result: "not_applicable".to_string(),
            gap_movement: "no_receipt".to_string(),
            false_positive_notes: "none observed".to_string(),
            limitation_notes: "no related static test path stays visible as no_static_path with no repair-card, packet, verify, or receipt success claim".to_string(),
            claim_boundary: vec![
                "Broader Python static facts remain preview/advisory".to_string(),
                "No repair packet emitted".to_string(),
                "No support-tier promotion".to_string(),
                "No arbitrary imports or tests were run by RIPR".to_string(),
            ],
            reason: "no-related-test dogfood records an ordinary no-action result instead of a repair card".to_string(),
        }
}

fn valid_python_already_observed_no_action_eval_scenario() -> DogfoodPythonNoActionEvalScenario {
    DogfoodPythonNoActionEvalScenario {
            name: "already_observed_no_packet_eval".to_string(),
            repo_shape: "strong_pytest_oracle".to_string(),
            source_kind: "local_repo".to_string(),
            source_ref: "fixtures/python_strong_oracle".to_string(),
            command: "ripr check --root fixtures/python_strong_oracle/input --diff fixtures/python_strong_oracle/diff.patch --format json".to_string(),
            runtime_ms: 327,
            finding_id: "probe:src_discount.py:2:python_preview".to_string(),
            changed_owner: "python:src/discount.py::apply_discount".to_string(),
            no_action_kind: "already_observed".to_string(),
            classification: "exposed".to_string(),
            stop_reasons: vec!["already_observed_strong_oracle".to_string()],
            related_test_file: "tests/test_discount.py".to_string(),
            related_test_name: "test_apply_discount_boundary".to_string(),
            why_not_actionable: "no_action already_observed prevents bounded repair routing because the related pytest test already carries an exact-value oracle for the changed behavior".to_string(),
            repair_card_present: false,
            agent_packet_present: false,
            verify_command: "not_applicable_no_action".to_string(),
            verify_result: "not_applicable".to_string(),
            receipt_command: "not_applicable_no_action".to_string(),
            receipt_result: "not_applicable".to_string(),
            gap_movement: "no_receipt".to_string(),
            false_positive_notes: "none observed".to_string(),
            limitation_notes: "strong related pytest evidence stays visible as exposed/already-observed with no repair-card, packet, verify success, or receipt movement claim".to_string(),
            claim_boundary: vec![
                "Broader Python static facts remain preview/advisory".to_string(),
                "No repair packet emitted".to_string(),
                "No support-tier promotion".to_string(),
                "No arbitrary imports or tests were run by RIPR".to_string(),
            ],
            reason: "already-observed dogfood records an ordinary no-action result: exact current test evidence means there is no missing proof to route.".to_string(),
        }
}

fn valid_python_heuristic_no_action_eval_scenario() -> DogfoodPythonNoActionEvalScenario {
    DogfoodPythonNoActionEvalScenario {
            name: "heuristic_only_no_packet_eval".to_string(),
            repo_shape: "pytest_test_name_similarity".to_string(),
            source_kind: "local_repo".to_string(),
            source_ref: "fixtures/python_related_test_name_similarity".to_string(),
            command: "ripr check --root fixtures/python_related_test_name_similarity/input --diff fixtures/python_related_test_name_similarity/diff.patch --format json".to_string(),
            runtime_ms: 361,
            finding_id: "probe:src_pricing.py:2:python_preview".to_string(),
            changed_owner: "python:src/pricing.py::apply_discount".to_string(),
            no_action_kind: "heuristic_only".to_string(),
            classification: "weakly_exposed".to_string(),
            stop_reasons: vec!["heuristic_only_related_test_link".to_string()],
            related_test_file: "tests/test_checkout.py".to_string(),
            related_test_name: "test_apply_discount_boundary_case".to_string(),
            why_not_actionable: "no_action heuristic_only prevents bounded repair routing because the related pytest test is linked only by test-name similarity and does not statically call the changed owner".to_string(),
            repair_card_present: false,
            agent_packet_present: false,
            verify_command: "not_applicable_no_action".to_string(),
            verify_result: "not_applicable".to_string(),
            receipt_command: "not_applicable_no_action".to_string(),
            receipt_result: "not_applicable".to_string(),
            gap_movement: "no_receipt".to_string(),
            false_positive_notes: "none observed".to_string(),
            limitation_notes: "heuristic test-name similarity stays visible as weakly_exposed evidence with no repair-card, packet, verify success, or receipt movement claim".to_string(),
            claim_boundary: vec![
                "Broader Python static facts remain preview/advisory".to_string(),
                "No repair packet emitted".to_string(),
                "No support-tier promotion".to_string(),
                "No arbitrary imports or tests were run by RIPR".to_string(),
            ],
            reason: "heuristic-only dogfood records an ordinary no-action result: uncertain related-test proximity is not safe enough to route human or agent test-repair work.".to_string(),
        }
}

fn valid_typescript_bun_ub_calibration_case() -> super::TypeScriptBunUbCalibrationCase {
    super::TypeScriptBunUbCalibrationCase {
        name: "valid".to_string(),
        source: "#31648-valid".to_string(),
        language: "typescript".to_string(),
        language_status: "preview".to_string(),
        rust_file: "src/jsc/Blob.rs".to_string(),
        rust_owner: "Blob::from_js_without_defer_gc".to_string(),
        rust_boundary: "array_buffer.shared || array_buffer.resizable".to_string(),
        ts_test_file: "test/js/web/fetch/blob.test.ts".to_string(),
        ts_entrypoints: vec!["new Blob".to_string(), "blob.arrayBuffer".to_string()],
        shared_array_buffer: true,
        resizable_array_buffer: true,
        view_backed_blob_input: true,
        stable_byte_copy_oracle: true,
        max_byte_length_mention_only: false,
        expected_verdict: "ts_discriminated".to_string(),
        expected_missing_discriminators: Vec::new(),
        bridge_confidence: "configured_hint".to_string(),
        expected_action: "no_missing_bridge_discriminator".to_string(),
        suggested_test_file: "not_applicable".to_string(),
        suggested_shape: None,
        repair_packet_ready: false,
        authority_boundary: "preview_advisory_only".to_string(),
        non_claims: vec![
            "provider evidence not asserted".to_string(),
            "source edits not suggested".to_string(),
            "generated tests not emitted".to_string(),
            "runtime Bun execution not required".to_string(),
            "mutation execution not run".to_string(),
            "default gates unchanged".to_string(),
            "public badge unchanged".to_string(),
            "baseline unchanged".to_string(),
            "RIPR Zero unchanged".to_string(),
            "support-tier promotion unchanged".to_string(),
        ],
        reason: "valid calibration case".to_string(),
    }
}

fn valid_cross_language_oracle_graph_case() -> super::CrossLanguageOracleGraphCase {
    super::CrossLanguageOracleGraphCase {
        name: "valid".to_string(),
        source: "#910-valid".to_string(),
        profile: "bun_blob_array_buffer".to_string(),
        profile_status: "active".to_string(),
        language: "typescript".to_string(),
        language_status: "preview".to_string(),
        rust_file: "src/jsc/Blob.rs".to_string(),
        rust_line: Some(3420),
        rust_owner: "Blob::from_js_without_defer_gc".to_string(),
        rust_boundary: "array_buffer.shared || array_buffer.resizable".to_string(),
        binding_edge_kind: "configured_bridge".to_string(),
        binding_edge_confidence: "configured_hint".to_string(),
        external_callsite_file: "test/js/web/fetch/blob.test.ts".to_string(),
        external_callsite_line: Some(1280),
        external_entrypoints: vec!["new Blob".to_string(), "blob.arrayBuffer".to_string()],
        shared_array_buffer: true,
        resizable_array_buffer: true,
        view_backed_blob_input: true,
        stable_byte_copy_oracle: true,
        max_byte_length_mention_only: false,
        external_oracle_file: "test/js/web/fetch/blob.test.ts".to_string(),
        external_oracle_line: Some(1286),
        external_oracle_kind: "stable_byte_copy".to_string(),
        oracle_strength: "strong".to_string(),
        expected_state: "rust_ungripped_ts_discriminated".to_string(),
        gap_state: "already_observed".to_string(),
        limitation_category: "not_applicable".to_string(),
        repair_route: "manual-review/cross-language-advisory-witness".to_string(),
        authority_boundary: "preview_advisory_only".to_string(),
        public_projection_eligible: false,
        repair_packet_ready: false,
        suggested_test_file: "not_applicable".to_string(),
        allowed_edit_surface: Vec::new(),
        verify_command: None,
        receipt_command: None,
        missing_discriminators: Vec::new(),
        missing_graph_legs: Vec::new(),
        unlock_condition: "not_applicable".to_string(),
        proof_mode: "observable_red_green".to_string(),
        raw_evidence_refs: valid_cross_language_oracle_graph_raw_refs(true),
        non_claims: vec![
            "no provider calls".to_string(),
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
            "no TypeScript Rust parity claim".to_string(),
            "no full cross-language proof".to_string(),
            "no verify command".to_string(),
            "no receipt command".to_string(),
            "no allowed edit surface".to_string(),
        ],
        reason: "valid cross-language oracle graph case".to_string(),
    }
}

fn valid_cross_language_oracle_graph_raw_refs(
    include_binding: bool,
) -> Vec<super::CrossLanguageOracleGraphRawRef> {
    let mut raw_refs = vec![
        super::CrossLanguageOracleGraphRawRef {
            leg: "rust_seam".to_string(),
            file: "src/jsc/Blob.rs".to_string(),
            line: Some(3420),
            kind: "rust_owner".to_string(),
            source_id: "#910".to_string(),
            sample: "Blob::from_js_without_defer_gc checks the ArrayBuffer boundary".to_string(),
        },
        super::CrossLanguageOracleGraphRawRef {
            leg: "boundary_discriminator".to_string(),
            file: "test/js/web/fetch/blob.test.ts".to_string(),
            line: Some(1282),
            kind: "typescript_discriminator".to_string(),
            source_id: "#910".to_string(),
            sample: "SharedArrayBuffer and resizable ArrayBuffer samples are visible".to_string(),
        },
        super::CrossLanguageOracleGraphRawRef {
            leg: "external_callsite".to_string(),
            file: "test/js/web/fetch/blob.test.ts".to_string(),
            line: Some(1280),
            kind: "typescript_callsite".to_string(),
            source_id: "#910".to_string(),
            sample: "new Blob and blob.arrayBuffer are visible".to_string(),
        },
        super::CrossLanguageOracleGraphRawRef {
            leg: "external_oracle".to_string(),
            file: "test/js/web/fetch/blob.test.ts".to_string(),
            line: Some(1286),
            kind: "typescript_assertion".to_string(),
            source_id: "#910".to_string(),
            sample: "stable copied bytes are asserted".to_string(),
        },
    ];
    if include_binding {
        raw_refs.push(super::CrossLanguageOracleGraphRawRef {
            leg: "binding_edge".to_string(),
            file: "src/jsc/Blob.rs".to_string(),
            line: Some(3420),
            kind: "configured_bridge".to_string(),
            source_id: "profiles.bun_ub".to_string(),
            sample: "configured bridge maps Blob tests to the Rust owner".to_string(),
        });
    }
    raw_refs
}

fn assert_contains_error(errors: &[String], expected: &str) {
    assert!(
        errors.iter().any(|error| error.contains(expected)),
        "expected error containing {expected:?}; got {errors:?}"
    );
}

fn typescript_bun_ub_calibration_corpus_json(cases: &[String]) -> String {
    format!(
        r#"{{
  "schema_version": "0.1",
  "kind": "typescript_bun_ub_calibration_corpus",
  "spec": "RIPR-SPEC-0027",
  "authority_boundary": "preview_advisory_only",
  "cases": [{}]
}}"#,
        cases.join(",")
    )
}

fn typescript_bun_ub_calibration_case_json(case: &super::TypeScriptBunUbCalibrationCase) -> String {
    let missing_json = case
        .expected_missing_discriminators
        .iter()
        .map(|discriminator| format!(r#""{discriminator}""#))
        .collect::<Vec<_>>()
        .join(", ");
    let non_claims_json = case
        .non_claims
        .iter()
        .map(|non_claim| format!(r#""{non_claim}""#))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"{{
  "id": "{id}",
  "language": "{language}",
  "language_status": "{language_status}",
  "rust_seam": {{
    "file": "{rust_file}",
    "line": 3420,
    "owner": "{rust_owner}",
    "boundary": "{rust_boundary}"
  }},
  "ts_test_file": "{ts_test_file}",
  "observed_ts_facts": {{
    "shared_array_buffer": {shared},
    "resizable_array_buffer": {resizable},
    "view_backed_blob_input": {view_backed_blob_input},
    "stable_byte_copy_oracle": {stable_byte_copy_oracle},
    "max_byte_length_mention_only": {max_byte_length_mention_only}
  }},
  "expected_verdict": "{expected_verdict}",
  "expected_missing_discriminators": [{missing_json}],
  "bridge_confidence": "{bridge_confidence}",
  "suggested_test_file": "{suggested_test_file}",
  "repair_packet_ready": {repair_packet_ready},
  "authority_boundary": "{authority_boundary}",
  "non_claims": [{non_claims_json}],
  "reason": "{reason}"
}}"#,
        id = case.name,
        language = case.language,
        language_status = case.language_status,
        rust_file = case.rust_file,
        rust_owner = case.rust_owner,
        rust_boundary = case.rust_boundary,
        ts_test_file = case.ts_test_file,
        shared = case.shared_array_buffer,
        resizable = case.resizable_array_buffer,
        view_backed_blob_input = case.view_backed_blob_input,
        stable_byte_copy_oracle = case.stable_byte_copy_oracle,
        max_byte_length_mention_only = case.max_byte_length_mention_only,
        expected_verdict = case.expected_verdict,
        bridge_confidence = case.bridge_confidence,
        suggested_test_file = case.suggested_test_file,
        repair_packet_ready = case.repair_packet_ready,
        authority_boundary = case.authority_boundary,
        reason = case.reason
    )
}

fn cross_language_oracle_graph_corpus_json(cases: &[String]) -> String {
    format!(
        r#"{{
  "schema_version": "0.1",
  "kind": "cross_language_oracle_graph_corpus",
  "spec": "RIPR-SPEC-0062",
  "authority_boundary": "preview_advisory_only",
  "cases": [{}]
}}"#,
        cases.join(",")
    )
}

fn cross_language_oracle_graph_case_json(case: &super::CrossLanguageOracleGraphCase) -> String {
    let entrypoints_json = test_json_string_array(&case.external_entrypoints);
    let allowed_edit_surface_json = test_json_string_array(&case.allowed_edit_surface);
    let missing_discriminators_json = test_json_string_array(&case.missing_discriminators);
    let missing_graph_legs_json = test_json_string_array(&case.missing_graph_legs);
    let raw_refs_json = cross_language_oracle_graph_raw_refs_json(&case.raw_evidence_refs);
    let non_claims_json = test_json_string_array(&case.non_claims);
    let verify_command_json = case
        .verify_command
        .as_ref()
        .map(|command| {
            format!(
                r#",
        "verify_command": "{command}""#
            )
        })
        .unwrap_or_default();
    let receipt_command_json = case
        .receipt_command
        .as_ref()
        .map(|command| {
            format!(
                r#",
        "receipt_command": "{command}""#
            )
        })
        .unwrap_or_default();
    format!(
        r#"{{
  "id": "{id}",
  "source": "{source}",
  "profile": "{profile}",
  "profile_status": "{profile_status}",
  "language": "{language}",
  "language_status": "{language_status}",
  "rust_seam": {{
    "file": "{rust_file}",
    "line": {rust_line},
    "owner": "{rust_owner}",
    "boundary": "{rust_boundary}"
  }},
  "binding_edge": {{
    "kind": "{binding_edge_kind}",
    "confidence": "{binding_edge_confidence}"
  }},
  "external_callsite": {{
    "file": "{external_callsite_file}",
    "line": {external_callsite_line},
    "entrypoints": [{entrypoints_json}]
  }},
  "observed_ts_facts": {{
    "shared_array_buffer": {shared_array_buffer},
    "resizable_array_buffer": {resizable_array_buffer},
    "view_backed_blob_input": {view_backed_blob_input},
    "stable_byte_copy_oracle": {stable_byte_copy_oracle},
    "max_byte_length_mention_only": {max_byte_length_mention_only}
  }},
  "external_oracle": {{
    "file": "{external_oracle_file}",
    "line": {external_oracle_line},
    "kind": "{external_oracle_kind}",
    "strength": "{oracle_strength}"
  }},
  "expected": {{
    "state": "{expected_state}",
    "gap_state": "{gap_state}",
    "limitation_category": "{limitation_category}",
    "repair_route": "{repair_route}",
    "authority_boundary": "{authority_boundary}",
    "public_projection_eligible": {public_projection_eligible},
    "repair_packet_ready": {repair_packet_ready},
    "suggested_test_file": "{suggested_test_file}",
    "allowed_edit_surface": [{allowed_edit_surface_json}]{verify_command_json}{receipt_command_json},
    "missing_discriminators": [{missing_discriminators_json}],
    "missing_graph_legs": [{missing_graph_legs_json}],
    "unlock_condition": "{unlock_condition}",
    "proof_mode": "{proof_mode}"
  }},
  "raw_evidence_refs": [{raw_refs_json}],
  "non_claims": [{non_claims_json}],
  "reason": "{reason}"
}}"#,
        id = case.name,
        source = case.source,
        profile = case.profile,
        profile_status = case.profile_status,
        language = case.language,
        language_status = case.language_status,
        rust_file = case.rust_file,
        rust_line = case.rust_line.unwrap_or(0),
        rust_owner = case.rust_owner,
        rust_boundary = case.rust_boundary,
        binding_edge_kind = case.binding_edge_kind,
        binding_edge_confidence = case.binding_edge_confidence,
        external_callsite_file = case.external_callsite_file,
        external_callsite_line = case.external_callsite_line.unwrap_or(0),
        shared_array_buffer = case.shared_array_buffer,
        resizable_array_buffer = case.resizable_array_buffer,
        view_backed_blob_input = case.view_backed_blob_input,
        stable_byte_copy_oracle = case.stable_byte_copy_oracle,
        max_byte_length_mention_only = case.max_byte_length_mention_only,
        external_oracle_file = case.external_oracle_file,
        external_oracle_line = case.external_oracle_line.unwrap_or(0),
        external_oracle_kind = case.external_oracle_kind,
        oracle_strength = case.oracle_strength,
        expected_state = case.expected_state,
        gap_state = case.gap_state,
        limitation_category = case.limitation_category,
        repair_route = case.repair_route,
        authority_boundary = case.authority_boundary,
        public_projection_eligible = case.public_projection_eligible,
        repair_packet_ready = case.repair_packet_ready,
        suggested_test_file = case.suggested_test_file,
        unlock_condition = case.unlock_condition,
        proof_mode = case.proof_mode,
        reason = case.reason
    )
}

fn cross_language_oracle_graph_raw_refs_json(
    raw_refs: &[super::CrossLanguageOracleGraphRawRef],
) -> String {
    raw_refs
        .iter()
        .map(|raw_ref| {
            format!(
                r#"{{
      "leg": "{leg}",
      "file": "{file}",
      "line": {line},
      "kind": "{kind}",
      "source_id": "{source_id}",
      "sample": "{sample}"
    }}"#,
                leg = raw_ref.leg,
                file = raw_ref.file,
                line = raw_ref.line.unwrap_or(0),
                kind = raw_ref.kind,
                source_id = raw_ref.source_id,
                sample = raw_ref.sample
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn test_json_string_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!(r#""{value}""#))
        .collect::<Vec<_>>()
        .join(", ")
}

fn valid_bun_ub_cross_language_dogfood_scenario() -> DogfoodBunUbCrossLanguageScenario {
    DogfoodBunUbCrossLanguageScenario {
            name: "bun_blob_31648_known_good".to_string(),
            source_case: "bun_blob_shared_and_resizable_present".to_string(),
            route_quality_case: "bun_blob_complete_ts_discriminated_advisory".to_string(),
            rust_file: "src/jsc/Blob.rs".to_string(),
            rust_owner: "Blob::from_js_without_defer_gc".to_string(),
            rust_boundary: "array_buffer.shared || array_buffer.resizable".to_string(),
            ts_test_file: super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE.to_string(),
            expected_state: "rust_ungripped_ts_discriminated".to_string(),
            observed_state: "rust_ungripped_ts_discriminated".to_string(),
            missing_discriminators: Vec::new(),
            missing_graph_legs: Vec::new(),
            suggested_test_file: "not_applicable".to_string(),
            manual_verdict: "agrees".to_string(),
            operator_action: "no_missing_bridge_discriminator".to_string(),
            review_before:
                "Rust-only review would manually grep test/js for SharedArrayBuffer and maxByteLength"
                    .to_string(),
            review_after:
                "TypeScript/Bun preview credits the configured Blob stable-byte witness"
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
            reason: "sample Bun UB cross-language dogfood receipt".to_string(),
        }
}

fn valid_typescript_preview_repair_loop_scenario() -> DogfoodTypescriptPreviewRepairLoopScenario {
    DogfoodTypescriptPreviewRepairLoopScenario {
            name: "typescript_boundary_predicate_proof".to_string(),
            source_fixture: "fixtures/typescript_boundary_gap".to_string(),
            source_finding_id: "probe:src_discount.ts:typescript_preview:2396aec1".to_string(),
            language: "typescript".to_string(),
            language_status: "preview".to_string(),
            classification: "weakly_exposed".to_string(),
            changed_owner: "applyDiscount".to_string(),
            probe_family: "predicate".to_string(),
            oracle_kind: "unknown".to_string(),
            oracle_strength: "unknown".to_string(),
            gap_state: "advisory".to_string(),
            actionability_category: "incomplete_repair_packet".to_string(),
            static_limit_kind: None,
            repair_packet_ready: false,
            must_have_verify_command: true,
            must_have_receipt_command: true,
            must_not_invent_verify_command: true,
            must_not_emit_repair_packet: true,
            authority_boundary: "preview_advisory_only".to_string(),
            expected_test_or_observer_shape: "exact equality-boundary assertion".to_string(),
            verify_command: "npm test -- discount".to_string(),
            verify_result: "not_run".to_string(),
            receipt_command: "ripr agent receipt --source fixtures/typescript_boundary_gap"
                .to_string(),
            receipt_state: "fixture_receipt_recorded".to_string(),
            outcome: "proof_improved".to_string(),
            why_not_actionable: "TypeScript preview has owner, related-test, oracle, and probe evidence but lacks a complete repair packet contract".to_string(),
            repair_route: "project canonical TypeScript repair packet fields only after verify, receipt, evidence refs, and edit boundaries are available".to_string(),
            operator_note: "proof stays advisory".to_string(),
            must_not_change: vec![
                "production behavior".to_string(),
                "default gate or badge policy".to_string(),
            ],
            raw_evidence_refs: vec![
                "fixtures/typescript_boundary_gap/expected/check.json#probe:src_discount.ts:typescript_preview:2396aec1".to_string(),
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
        }
}

fn valid_user_surface_projection_scenario() -> DogfoodUserSurfaceProjectionScenario {
    DogfoodUserSurfaceProjectionScenario {
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
        }
}

fn duplicate_entry(
    name: &str,
    line: usize,
    class: &'static str,
    oracle_kind: &str,
    oracle_strength: &'static str,
    owners: &[&str],
    activations: &[(&'static str, &str)],
) -> TestEfficiencyEntry {
    TestEfficiencyEntry {
        path: Path::new("tests/example.rs").to_path_buf(),
        name: name.to_string(),
        line,
        class,
        oracle_kind: oracle_kind.to_string(),
        oracle_strength,
        reached_owners: owners.iter().map(|s| s.to_string()).collect(),
        observed_values: activations
            .iter()
            .enumerate()
            .map(|(index, (context, value))| TestEfficiencyValue {
                line: line + index,
                context,
                value: (*value).to_string(),
                text: (*value).to_string(),
            })
            .collect(),
        reasons: Vec::new(),
        static_limitations: Vec::new(),
        duplicate_group_id: None,
        declared_intent: None,
    }
}

// -------- test-intent v1 --------

fn intent_entry(name: &str, path: &str, line: usize, class: &'static str) -> TestEfficiencyEntry {
    TestEfficiencyEntry {
        path: Path::new(path).to_path_buf(),
        name: name.to_string(),
        line,
        class,
        oracle_kind: "exact assertion".to_string(),
        oracle_strength: "strong",
        reached_owners: Vec::new(),
        observed_values: Vec::new(),
        reasons: Vec::new(),
        static_limitations: Vec::new(),
        duplicate_group_id: None,
        declared_intent: None,
    }
}

fn declaration(test: &str, path: Option<&str>, intent: TestIntentKind) -> TestIntentDeclaration {
    TestIntentDeclaration {
        test: test.to_string(),
        path: path.map(|p| p.to_string()),
        intent,
        owner: "team".to_string(),
        reason: "stated reason".to_string(),
        block_line: 10,
    }
}

const STUB_RIPR_NATIVE_JSON: &str = r#"{
  "schema_version": "0.1",
  "kind": "ripr",
  "label": "ripr",
  "message": "3",
  "status": "warn",
  "color": "yellow",
  "counts": {
    "unsuppressed_exposure_gaps": 3,
    "unsuppressed_test_efficiency_findings": 0,
    "intentional_test_efficiency_findings": 0,
    "suppressed_exposure_gaps": 1,
    "suppressed_test_efficiency_findings": 0,
    "unknowns": 0,
    "unknowns_test_efficiency": 0,
    "analyzed_findings": 4,
    "analyzed_tests": 0
  },
  "reason_counts": {
    "no_assertion_detected": 0,
    "smoke_oracle_only": 0,
    "relational_oracle": 0,
    "broad_oracle": 0,
    "assertion_may_not_match_detected_owner": 0,
    "opaque_helper_or_fixture_boundary": 0,
    "no_activation_literal_detected": 0,
    "expected_value_computed_from_detected_owner_path": 0,
    "duplicate_activation_and_oracle_shape": 0
  },
  "policy": {
    "include_unknowns": false,
    "fail_on_nonzero": false,
    "test_intent_path": ".ripr/test_intent.toml",
    "suppressions_path": ".ripr/suppressions.toml"
  },
  "warnings": ["first warning", "second warning"]
}"#;

const STUB_RIPR_PLUS_NATIVE_JSON: &str = r#"{
  "schema_version": "0.1",
  "kind": "ripr_plus",
  "label": "ripr+",
  "message": "7",
  "status": "warn",
  "color": "orange",
  "counts": {
    "unsuppressed_exposure_gaps": 0,
    "unsuppressed_test_efficiency_findings": 7,
    "intentional_test_efficiency_findings": 2,
    "suppressed_exposure_gaps": 0,
    "suppressed_test_efficiency_findings": 1,
    "unknowns": 0,
    "unknowns_test_efficiency": 3,
    "analyzed_findings": 0,
    "analyzed_tests": 308
  },
  "reason_counts": {
    "no_assertion_detected": 1,
    "smoke_oracle_only": 1,
    "relational_oracle": 1,
    "broad_oracle": 4,
    "assertion_may_not_match_detected_owner": 0,
    "opaque_helper_or_fixture_boundary": 3,
    "no_activation_literal_detected": 3,
    "expected_value_computed_from_detected_owner_path": 0,
    "duplicate_activation_and_oracle_shape": 1
  },
  "policy": {
    "include_unknowns": false,
    "fail_on_nonzero": false,
    "test_intent_path": ".ripr/test_intent.toml",
    "suppressions_path": ".ripr/suppressions.toml"
  },
  "warnings": []
}"#;

// ---- #4003: the badge input must come from the actual resolved base through
// the shared authorities, with a pinned Git presentation. These are real-Git
// fixture tests: a mocked diff runner cannot prove the Git boundary. ----

fn badge_fixture_git(args: &[&str]) -> Result<(), String> {
    let status = run("git", args)?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("badge fixture git command failed: {args:?}"))
    }
}

fn badge_fixture_commit(message: &str) -> Result<(), String> {
    badge_fixture_git(&[
        "-c",
        "user.email=badge-fixture@example.com",
        "-c",
        "user.name=badge fixture",
        "commit",
        "--quiet",
        "-m",
        message,
    ])
}

fn badge_fixture_base_and_edit_commits() -> Result<(), String> {
    fs::write("subject.txt", "kept line\nbadge-secret-line\n").map_err(|err| err.to_string())?;
    badge_fixture_git(&["add", "."])?;
    badge_fixture_commit("badge fixture base")?;
    badge_fixture_git(&["checkout", "--quiet", "-b", "feature/badge-input"])?;
    fs::write(
        "subject.txt",
        "kept line\nbadge-secret-line\nbadge-secret-line added\n",
    )
    .map_err(|err| err.to_string())?;
    badge_fixture_git(&["add", "."])?;
    badge_fixture_commit("badge fixture edit")
}

fn assert_badge_commit_identity(value: &str, label: &str) -> Result<(), String> {
    if value.len() < 40 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "expected a resolved {label} commit identity, got {value:?}"
        ));
    }
    Ok(())
}

fn write_doc_artifact_fixture(root: &Path, path: &str, id: &str) {
    write(&root.join(path), &format!("# {id}\n"));
}

fn write_doc_artifact_ledger_fixture(root: &Path, body: &str) {
    write(&root.join(DOC_ARTIFACT_LEDGER), body);
}

fn write_support_tier_fixture(root: &Path, rows: &str) {
    write(
        &root.join(SUPPORT_TIERS_PATH),
        &format!(
            r#"# Support Tiers

## Current Support Map

| Capability | Tier | Surface | Proof | Known limits |
| --- | --- | --- | --- | --- |
{rows}
"#
        ),
    );
    write(
        &root.join("README.md"),
        "See [Support tiers](docs/status/SUPPORT_TIERS.md).\n",
    );
    write(
        &root.join("docs/specs/RIPR-SPEC-0001-alpha.md"),
        "# RIPR-SPEC-0001: Alpha\n\nSupport-tier impact:\n\n- See [SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md).\n",
    );
}

fn write_repo_contract_report_fixture(root: &Path, include_active_goal: bool) {
    write_doc_artifact_fixture(
        root,
        "docs/proposals/RIPR-PROP-0001-source-of-truth.md",
        "RIPR-PROP-0001",
    );
    write_doc_artifact_fixture(
        root,
        "docs/proposals/RIPR-PROP-0002-old.md",
        "RIPR-PROP-0002",
    );
    write_doc_artifact_fixture(root, "docs/adr/RIPR-ADR-0001-open.md", "RIPR-ADR-0001");
    write_doc_artifact_fixture(
        root,
        "docs/specs/RIPR-SPEC-0001-source-of-truth.md",
        "RIPR-SPEC-0001",
    );
    write_doc_artifact_fixture(
        root,
        "plans/source-of-truth/implementation-plan.md",
        "PLAN-0001",
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

[[artifact]]
id = "RIPR-PROP-0002"
kind = "proposal"
path = "docs/proposals/RIPR-PROP-0002-old.md"
status = "superseded"
owner = "repo-infra"
replacement = "RIPR-PROP-0001"

[[artifact]]
id = "RIPR-ADR-0001"
kind = "adr"
path = "docs/adr/RIPR-ADR-0001-open.md"
status = "proposed"
owner = "repo-infra"
linked_proposal = "RIPR-PROP-0001"

[[artifact]]
id = "RIPR-SPEC-0001"
kind = "spec"
path = "docs/specs/RIPR-SPEC-0001-source-of-truth.md"
status = "accepted"
owner = "repo-infra"
linked_proposal = "RIPR-PROP-0001"

[[artifact]]
id = "PLAN-0001"
kind = "plan"
path = "plans/source-of-truth/implementation-plan.md"
status = "active"
owner = "repo-infra"
linked_proposal = "RIPR-PROP-0001"
linked_spec = "RIPR-SPEC-0001"
"#,
    );
    write(
        &root.join("policy/workflow_allowlist.txt"),
        "# workflow allowlist\n",
    );
    write_support_tier_fixture(
        root,
        "| Rust gap repair loop | `usable alpha` | CLI and editor | `cargo xtask rust-repair-trust-report` | Governed promotion authority is incomplete. |\n| Source-of-truth artifact graph | `stable building block` | docs | `cargo xtask check-doc-artifacts` | Registered graph only. |\n| Source-of-truth workflow | `stable building block` | CI | `cargo xtask check-doc-artifacts` | Workflow contract only. |\n",
    );

    if include_active_goal {
        write(
            &root.join("docs/IMPLEMENTATION_CAMPAIGNS.md"),
            "# Campaigns\n\nsource-of-truth-control-plane\n\n| Work item | Status |\n| --- | --- |\n| `docs/ledger` | done |\n| `docs/report` | ready |\n| `docs/report-json` | ready |\n| `docs/wait` | blocked |\n| `docs/wait-dependent` | blocked |\n",
        );
        write(
            &root.join(".ripr/goals/active.toml"),
            r#"id = "source-of-truth-control-plane"
title = "Source of Truth Control Plane"
status = "active"
end_state = [
  "Repo carries a contract graph.",
]

[[work_item]]
id = "docs/ledger"
status = "done"
branch = "docs-ledger"
stackable = false
acceptance = "Ledger exists."
commands = ["cargo xtask check-doc-artifacts"]

[[work_item]]
id = "docs/report"
status = "ready"
branch = "xtask-repo-contract-report"
stackable = false
proposal = "RIPR-PROP-0001"
spec = "RIPR-SPEC-0001"
plan = "PLAN-0001"
acceptance = "Generate graph report."
commands = ["cargo xtask repo-contract-report"]

[[work_item]]
id = "docs/report-json"
status = "ready"
branch = "xtask-repo-contract-report-json"
stackable = false
acceptance = "Generate graph JSON."
commands = ["cargo xtask repo-contract-report"]

[[work_item]]
id = "docs/wait"
status = "blocked"
branch = "docs-wait"
stackable = false
acceptance = "Blocked item is recorded."
blocked_reason = "Waiting on runner proof."

[[work_item]]
id = "docs/wait-dependent"
status = "blocked"
branch = "docs-wait-dependent"
stackable = false
acceptance = "Blocked dependency item is recorded."
blocked_by = ["docs/wait"]
blocked_reason = "Waiting on docs/wait."
"#,
        );
    }
}

fn slice_test_repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn python_marker_auto_docs_passing_copy() -> (&'static str, &'static str, &'static str) {
    (
        "with no `ripr.toml` keep Python preview off",
        r#"With no `ripr.toml` enables Python preview enabled = ["rust"] keeps Python off"#,
        r#"with no `ripr.toml` Python project markers enabled = ["rust"]"#,
    )
}

fn badge_endpoint_tempdir(label: &str) -> Result<std::path::PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("ripr-badge-endpoint-{label}-{stamp}-{pid}"));
    std::fs::create_dir_all(&dir).map_err(|err| format!("create temp dir {label}: {err}"))?;
    Ok(dir)
}

fn write_fixture_shields(dir: &std::path::Path, name: &str, body: &[u8]) -> Result<(), String> {
    std::fs::write(dir.join(name), body).map_err(|err| format!("write {name}: {err}"))
}

fn valid_droid_security_scan_yaml() -> &'static str {
    r#"
on:
  workflow_dispatch:
  schedule:
    - cron: "0 8 * * 1"
concurrency:
  group: droid-security-scan-${{ github.repository }}
  cancel-in-progress: false
jobs:
  droid-security-scan:
    permissions:
      contents: read
      pull-requests: write
      issues: write
      actions: read
    env:
      MINIMAX_API_KEY: ${{ secrets.MINIMAX_API_KEY }}
    steps:
      - name: Configure MiniMax BYOK for Factory Droid
        run: |
          cat > "$HOME/.factory/settings.json" <<'JSON'
          {"apiKey": "${MINIMAX_API_KEY}"}
          JSON
      - name: Ensure GitHub CLI for Droid Action
        run: |
          if command -v gh >/dev/null 2>&1; then exit 0; fi
          gh_archive="gh_2.82.1_linux_amd64.tar.gz"
          gh_sha256="afada88676dfccea384e6cc28ae990b3e31bbc55f9d75c4697f902c757fa462b"
          printf '%s  %s\n' "$gh_sha256" "$gh_archive" > gh.sha256
          sha256sum -c gh.sha256
          echo "/tmp/gh_2.82.1_linux_amd64/bin" >> "$GITHUB_PATH"
      - uses: EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec
        env:
          ANTHROPIC_AUTH_TOKEN: ""
          ANTHROPIC_BASE_URL: ""
        with:
          upload_debug_artifacts: false
          security_scan_schedule: true
          security_scan_days: 7
          security_model: "custom:MiniMax-M3-0"
          security_severity_threshold: medium
          security_block_on_critical: true
          security_block_on_high: false
          show_full_output: false
"#
}

fn droid_security_scan_violations(yaml: &str) -> Vec<String> {
    let mut violations = Vec::new();
    check_droid_security_scan_config(&mut violations, "security.yml", yaml);
    violations
}

fn changed_path(path: &str, statuses: &[&str]) -> ChangedPath {
    ChangedPath {
        path: path.to_string(),
        statuses: statuses
            .iter()
            .map(|status| (*status).to_string())
            .collect::<BTreeSet<_>>(),
    }
}

fn doctor_has_error(findings: &[WorktreeDoctorFinding], text: &str) -> bool {
    findings.iter().any(|finding| {
        finding.severity == WorktreeDoctorSeverity::Error && finding.message.contains(text)
    })
}

fn doctor_has_warning(findings: &[WorktreeDoctorFinding], text: &str) -> bool {
    findings.iter().any(|finding| {
        finding.severity == WorktreeDoctorSeverity::Warning && finding.message.contains(text)
    })
}

fn triage_pr(number: u64, title: &str, files: &[&str]) -> PrTriagePullRequest {
    PrTriagePullRequest {
        number,
        title: title.to_string(),
        body: "Validation\n- cargo xtask check-pr\n".to_string(),
        is_draft: false,
        created_at: "2026-05-14T00:00:00Z".to_string(),
        updated_at: "2026-05-14T00:00:00Z".to_string(),
        merge_state_status: "CLEAN".to_string(),
        head_ref_name: format!("branch-{number}"),
        base_ref_name: "main".to_string(),
        review_decision: String::new(),
        labels: Vec::new(),
        files: files.iter().map(|file| file.to_string()).collect(),
        checks: vec![PrTriageCheck {
            name: "rust".to_string(),
            status: "COMPLETED".to_string(),
            conclusion: "SUCCESS".to_string(),
        }],
    }
}

fn gh_pr_status_pr(checks: Vec<PrTriageCheck>) -> GhPrStatusPullRequest {
    GhPrStatusPullRequest {
        number: 42,
        title: "devex: merge readiness".to_string(),
        is_draft: false,
        merge_state_status: "CLEAN".to_string(),
        head_ref_oid: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        head_ref_name: "devex-gh-pr-status".to_string(),
        base_ref_name: "main".to_string(),
        review_decision: "APPROVED".to_string(),
        checks,
        reviews: vec![GhPrStatusReview {
            author: "droid".to_string(),
            state: "APPROVED".to_string(),
            commit_oid: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        }],
        provider_review_observations: Default::default(),
    }
}

fn gh_pr_status_check(name: &str, status: &str, conclusion: &str) -> PrTriageCheck {
    PrTriageCheck {
        name: name.to_string(),
        status: status.to_string(),
        conclusion: conclusion.to_string(),
    }
}

fn assert_gh_pr_status_arg_error(args: &[String]) -> Result<(), String> {
    match parse_gh_pr_status_args(args) {
        Ok(number) => Err(format!(
            "expected gh-pr-status args to fail, parsed #{number}"
        )),
        Err(_) => Ok(()),
    }
}

fn assert_packet_coverage_report() -> Result<(), String> {
    let report = crate::read_text_lossy(Path::new("target/ripr/reports/file-policy.md"))?;
    let common = report
        .lines()
        .find(|line| {
            line.ends_with("`cargo test -p xtask --bin xtask --locked --offline portable_consumer`")
        })
        .ok_or("common packet coverage is absent from the policy report")?;
    let common_tests = packet_coverage_selected_tests(common)?;
    for subject in [
        "portable_consumer::tests::consumer_source_does_not_search_path_or_open_a_network_client",
        "portable_consumer::tests::packet_digest_matches_the_producer_formula",
    ] {
        if !common_tests.contains(&subject) || !common.contains("declared=all; applicable;") {
            return Err(format!(
                "common packet subject not selected: {subject}: {common}"
            ));
        }
    }
    let native = report
        .lines()
        .find(|line| {
            line.ends_with(
                "`cargo test -p ripr --locked --offline --test portable_consumer_packet`",
            )
        })
        .ok_or("native packet coverage is absent from the policy report")?;
    if packet_coverage_selected_count(common)? < 2 {
        return Err(format!(
            "common packet selection lost required subjects: {common}"
        ));
    }
    if cfg!(windows) {
        if !common.contains("host=windows; declared=all; applicable;")
            || !native
                .contains("host=windows; declared=unix; not_applicable; selected=not_enumerated;")
            || native.contains("tests=[")
        {
            return Err(format!(
                "Windows packet applicability drifted: {common}\n{native}"
            ));
        }
    } else {
        let native_tests = packet_coverage_selected_tests(native)?;
        if !native.contains("host=unix; declared=unix; applicable;")
            || packet_coverage_selected_count(native)? < 2
        {
            return Err(format!(
                "Unix native packet selector was not retained: {native}"
            ));
        }
        for subject in [
            "native_packet_analyzes_a_boundary_gap_without_path_or_compiler_fallback",
            "native_packet_pilot_consumes_the_summary_artifact",
        ] {
            if !native_tests.contains(&subject) {
                return Err(format!(
                    "Unix native packet subject missing: {subject}: {native}"
                ));
            }
        }
    }
    Ok(())
}

fn packet_coverage_selected_count(row: &str) -> Result<usize, String> {
    row.split_once("; selected=")
        .and_then(|(_, rest)| rest.split_once(';'))
        .ok_or_else(|| format!("selected count absent from coverage row: {row}"))?
        .0
        .parse()
        .map_err(|error| format!("invalid selected count in {row}: {error}"))
}

fn packet_coverage_selected_tests(row: &str) -> Result<Vec<&str>, String> {
    let tests = row
        .rsplit_once("; `")
        .and_then(|(fields, _)| fields.split_once("; tests=["))
        .and_then(|(_, tests)| tests.strip_suffix(']'))
        .ok_or_else(|| format!("selected tests field missing or malformed: {row}"))?;
    let selected: Vec<_> = tests.split(", ").collect();
    if selected.iter().any(|test| {
        test.is_empty() || test.contains([',', '[', ']', ';']) || test.contains(char::is_whitespace)
    }) || selected.len() != packet_coverage_selected_count(row)?
    {
        return Err(format!("selected identities and count disagree: {row}"));
    }
    Ok(selected)
}

fn packet_coverage_report_fixture() -> String {
    let host = if cfg!(windows) { "windows" } else { "unix" };
    let native = if cfg!(windows) {
        "not_applicable; selected=not_enumerated".to_string()
    } else {
        concat!(
            "applicable; selected=2; tests=[",
            "native_packet_analyzes_a_boundary_gap_without_path_or_compiler_fallback, ",
            "native_packet_pilot_consumes_the_summary_artifact]",
        )
        .to_string()
    };
    format!(
        concat!(
            "- line 1; host={host}; declared=all; applicable; selected=2; tests=[",
            "portable_consumer::tests::consumer_source_does_not_search_path_or_open_a_network_client, ",
            "portable_consumer::tests::packet_digest_matches_the_producer_formula]; ",
            "`cargo test -p xtask --bin xtask --locked --offline portable_consumer`\n",
            "- line 1; host={host}; declared=unix; {native}; ",
            "`cargo test -p ripr --locked --offline --test portable_consumer_packet`\n",
        ),
        host = host,
        native = native,
    )
}

fn file_policy_toml_coverage_fixture(field: &str, array: &str) -> String {
    let common = if field == "covered_by" {
        ""
    } else {
        "covered_by = [\"cargo xtask check-file-policy\"]\n"
    };
    format!(
        "[[allow]]\nglob = \"policy/*.toml\"\nkind = \"policy\"\n\
         owner = \"xtask\"\nsurface = \"policy\"\nclassification = \"config\"\n\
         reason = \"TOML coverage control\"\n{common}{field} = {array}\n"
    )
}

fn lane1_failed_incomplete_outcome(
    outcome: Lane1EvidenceAuditRepoExposureOutcome,
    context: &str,
) -> Result<Lane1EvidenceAuditRepoExposureGeneration, String> {
    match outcome {
        Lane1EvidenceAuditRepoExposureOutcome::FailedIncomplete(diagnostics) => Ok(diagnostics),
        _ => Err(format!(
            "{context} should produce a bounded incomplete limitation"
        )),
    }
}

fn complete_evidence_health_json_fixture() -> String {
    serde_json::json!({
        "schema_version": "0.2",
        "tool": "ripr",
        "scope": "repo",
        "status": "advisory",
        "inputs": {
            "root": ".",
            "mutation_calibration": Value::Null,
        },
        "metrics": {
            "seams_total": 0,
            "headline_eligible_total": 0,
            "weakly_gripped_total": 0,
            "ungripped_total": 0,
            "grip_class_counts": {},
            "stage_state_counts": {},
            "unknown_stage_counts": {},
            "unknown_stop_reason_counts": {},
            "missing_discriminators_total": 0,
            "seams_with_missing_discriminators": 0,
            "missing_discriminator_counts": [],
            "observed_values_total": 0,
            "seams_with_observed_values": 0,
            "observed_value_context_counts": {},
            "related_tests_total": 0,
            "seams_with_related_tests": 0,
            "related_test_confidence_counts": {},
            "oracle_strength_counts": {},
            "oracle_kind_counts": {},
            "opaque_oracle_count": 0,
        },
        "evidence_quality": {
            "canonical_gap_groups_total": 0,
            "duplicate_looking_groups_total": 0,
            "largest_canonical_groups": [],
            "actionability_class_counts": {},
            "static_limitation_stage_counts": {},
            "static_limitation_reason_counts": [],
            "static_limitation_category_counts": {},
            "calibration_availability_counts": {},
            "movement_availability": {
                "records_with_seam_id": 0,
                "records_with_canonical_gap_id": 0,
                "records_with_complete_evidence_path": 0,
                "records_with_recommendation": 0,
                "records_with_verify_command": 0,
            },
            "top_evidence_quality_risks": [],
        },
        "calibration": {
            "status": "not_provided",
            "source": Value::Null,
            "matched_total": 0,
            "static_without_runtime_total": 0,
            "runtime_without_static_total": 0,
            "ambiguous_file_line_total": 0,
            "unmatched_runtime_total": 0,
        },
        "top_static_limitations": [],
    })
    .to_string()
}

fn expect_ripr_swarm_plan_arg_error(args: &[&str]) -> Result<String, String> {
    let owned_args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    match parse_ripr_swarm_plan_args(&owned_args) {
        Ok(parsed) => Err(format!("accepted invalid ripr swarm plan args: {parsed:?}")),
        Err(err) => Ok(err),
    }
}

fn lane1_scorecard_sample_audit_value() -> Result<serde_json::Value, String> {
    let report = lane1_evidence_audit_from_repo_exposure(".", lane1_audit_sample_json())?;
    let json = lane1_evidence_audit_json(&report)?;
    serde_json::from_str(&json).map_err(|err| err.to_string())
}

fn scorecard_minimal_audit_value(
    low_top_related: usize,
    static_limitations: usize,
    duplicate_groups: usize,
    calibrated_records: usize,
    uncalibrated_records: usize,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": "0.1",
        "report": "lane1-evidence-audit",
        "inputs": {"root": "."},
        "summary": {
            "raw_headline_gaps": 12,
            "canonical_gap_groups_total": 5,
            "duplicate_looking_groups_total": duplicate_groups,
            "missing_discriminators_total": 4,
            "static_limitations_total": static_limitations,
            "related_tests_total": 20,
            "low_or_opaque_top_related_tests": low_top_related,
            "calibrated_records": calibrated_records,
            "uncalibrated_records": uncalibrated_records,
            "evidence_records_total": 12,
            "evidence_records_missing": 0
        },
        "canonical_gap_groups": {"total": 5, "largest": []},
        "duplicate_looking_groups": [],
        "missing_discriminator_classes": {"by_reason": [], "by_flow_sink": {}, "by_value": []},
        "static_limitations": {
            "by_reason": [],
            "by_stage": {},
            "by_category": {},
            "repair_routes": {}
        },
        "oracle_semantics_distribution": {
            "by_semantics": [],
            "oracle_kind_counts": {"unknown": 0},
            "oracle_strength_counts": {"unknown": 0}
        },
        "related_test_ranking": {
            "all_confidence_counts": {"high": 19, "low": low_top_related},
            "top_confidence_counts": {"low": low_top_related}
        },
        "movement_availability": {"records_with_canonical_gap_id": 5},
        "calibration_availability": {
            "availability_counts": {},
            "confidence_counts": {},
            "agreement_counts": {},
            "calibrated_records": calibrated_records,
            "uncalibrated_records": uncalibrated_records
        }
    })
}

fn push_scorecard_unknown(value: &mut serde_json::Value, kind: &str) -> Result<(), String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "scorecard value should be an object".to_string())?;
    let unknowns = object
        .entry("unknowns".to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "scorecard unknowns should be an array".to_string())?;
    unknowns.push(serde_json::json!({
        "kind": kind,
        "summary": "limited input",
        "next_repair": "rerun Lane 1 reports"
    }));
    Ok(())
}

fn scorecard_inputs_for_test(previous_loaded: bool) -> EvidenceQualityScorecardInputs {
    EvidenceQualityScorecardInputs {
        lane1_evidence_audit: scorecard_input_for_test(
            "target/ripr/reports/lane1-evidence-audit.json",
            "loaded",
        ),
        evidence_health: scorecard_input_for_test(
            "target/ripr/reports/evidence-health.json",
            "missing",
        ),
        previous_scorecard: scorecard_input_for_test(
            "target/ripr/reports/evidence-quality-scorecard.json",
            if previous_loaded { "loaded" } else { "missing" },
        ),
        capability_matrix: scorecard_input_for_test("docs/CAPABILITY_MATRIX.md", "loaded"),
        capabilities: scorecard_input_for_test("metrics/capabilities.toml", "loaded"),
        traceability: scorecard_input_for_test(".ripr/traceability.toml", "loaded"),
    }
}

fn scorecard_input_for_test(path: &str, status: &str) -> EvidenceQualityScorecardInput {
    EvidenceQualityScorecardInput {
        path: path.to_string(),
        status: status.to_string(),
        schema_version: None,
        sha256: None,
        note: None,
    }
}

fn trend_inputs_for_test(previous_loaded: bool) -> EvidenceQualityTrendInputs {
    EvidenceQualityTrendInputs {
        current_scorecard: scorecard_input_for_test(
            "target/ripr/reports/evidence-quality-scorecard.json",
            "loaded",
        ),
        previous_artifact: scorecard_input_for_test(
            "target/ripr/reports/evidence-quality-scorecard.previous.json",
            if previous_loaded { "loaded" } else { "missing" },
        ),
        capability_matrix: scorecard_input_for_test("docs/CAPABILITY_MATRIX.md", "loaded"),
        traceability: scorecard_input_for_test(".ripr/traceability.toml", "loaded"),
    }
}

fn trend_direction_for(
    report: &EvidenceQualityTrendReport,
    metric: &str,
) -> Result<String, String> {
    report
        .metric_trends
        .iter()
        .find(|trend| trend.metric == metric)
        .map(|trend| trend.direction.clone())
        .ok_or_else(|| format!("missing trend metric {metric}"))
}

fn set_static_category_count(
    value: &mut serde_json::Value,
    category: &str,
    count: usize,
) -> Result<(), String> {
    let static_limitations = value
        .get_mut("static_limitations")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| "missing static_limitations object".to_string())?;
    let by_category = static_limitations
        .get_mut("by_category")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| "missing static_limitations.by_category object".to_string())?;
    by_category.insert(category.to_string(), serde_json::json!(count));
    Ok(())
}

fn set_runtime_confidence_class_counts(
    value: &mut serde_json::Value,
    evidence_class: &str,
    canonical_items: usize,
    calibrated_supported: usize,
    static_only: usize,
) -> Result<(), String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "scorecard value should be an object".to_string())?;
    let calibration_coverage = object
        .entry("calibration_coverage".to_string())
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| "calibration_coverage should be an object".to_string())?;
    let rows = calibration_coverage
        .entry("by_evidence_class".to_string())
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or_else(|| "by_evidence_class should be an array".to_string())?;
    rows.push(serde_json::json!({
        "evidence_class": evidence_class,
        "canonical_items": canonical_items,
        "calibrated_supported": calibrated_supported,
        "fixture_backed": 0,
        "static_only": static_only,
        "unknown_confidence": 0,
        "uncalibrated": canonical_items.saturating_sub(calibrated_supported),
        "actionable_items": 0,
        "static_limitation_items": 0,
    }));
    Ok(())
}

fn set_summary_count(
    value: &mut serde_json::Value,
    metric: &str,
    count: usize,
) -> Result<(), String> {
    let summary = value
        .get_mut("summary")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| "missing summary object".to_string())?;
    summary.insert(metric.to_string(), serde_json::json!(count));
    Ok(())
}

fn scorecard_status_for(
    report: &EvidenceQualityScorecardReport,
    class: &str,
) -> Result<String, String> {
    report
        .maturity_by_class
        .iter()
        .find(|row| row.class == class)
        .map(|row| row.status.clone())
        .ok_or_else(|| format!("missing maturity row {class}"))
}

fn audit_count_row(rows: &Value, label: &str) -> Result<usize, String> {
    let row = rows
        .as_array()
        .ok_or_else(|| format!("expected count rows for {label}"))?
        .iter()
        .find(|row| row["label"].as_str() == Some(label))
        .ok_or_else(|| format!("missing count row {label}"))?;
    row["count"]
        .as_u64()
        .map(|count| count as usize)
        .ok_or_else(|| format!("missing count for {label}"))
}

fn lane1_audit_sample_json() -> &'static str {
    r#"{
          "schema_version": "0.3",
          "scope": "repo",
          "seams": [
            {
              "seam_id": "raw-1",
              "headline_eligible": true,
              "file": "src/lib.rs",
              "evidence_record": {
                "schema_version": "0.1",
                "seam_id": "seam-1",
                "canonical_gap_id": "gap:shared",
                "canonical_gap_group_size": 2,
                "canonical_gap_reason": "same owner, seam kind, flow sink, missing discriminator, and assertion shape",
                "owner": "pricing::discount",
                "location": {"file": "src/lib.rs", "line": 12},
                "seam_kind": "predicate_boundary",
                "grip_class": "weakly_gripped",
                "headline_eligible": true,
                "evidence_path": {
                  "reach": {"state": "yes", "confidence": "high", "summary": "direct call"},
                  "activate": {"state": "yes", "confidence": "high", "summary": "value observed"},
                  "propagate": {"state": "weak", "confidence": "medium", "summary": "return value"},
                  "observe": {"state": "weak", "confidence": "medium", "summary": "assertion nearby"},
                  "discriminate": {"state": "no", "confidence": "unknown", "summary": "boundary missing"}
                },
                "observed_values": [{"value": "101", "line": 20, "text": "discount(101)", "context": "function_argument"}],
                "missing_discriminators": [{"value": "amount == threshold", "reason": "boundary value not observed", "flow_sink": {"kind": "return_value", "text": "discounted", "line": 12}}],
                "related_tests_total": 1,
                "related_tests": [{
                  "name": "discounts_large_orders",
                  "file": "tests/pricing.rs",
                  "line": 7,
                  "oracle_kind": "exact_value",
                  "oracle_strength": "strong",
                  "evidence_summary": "asserts exact discount",
                  "relation_reason": "direct_owner_call",
                  "relation_confidence": "high",
                  "oracle_semantics": {
                    "observes": "exact return value",
                    "missing": "boundary equality",
                    "upgrade_suggestion": "add equality boundary"
                  }
                }],
                "recommendation": {
                  "action": "write_targeted_test",
                  "reason": "missing discriminator",
                  "assertion_shape": {"kind": "exact_value", "example": "assert_eq!(...)"},
                  "verify_command": "ripr agent verify --root . --before before.json --after after.json --json"
                },
                "actionability": {"class": "actionable_assertion_upgrade"},
                "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                "static_limitations": [],
                "raw_findings": [{
                  "file": "src/lib.rs",
                  "line": 12,
                  "kind": "weakly_exposed",
                  "expression": "discount(amount)"
                }],
                "canonical_item": {
                  "canonical_gap_id": "gap:shared",
                  "canonical_item_kind": "gap",
                  "evidence_class": "predicate_boundary",
                  "gap_state": "actionable",
                  "actionability": "upgrade_assertion",
                  "group_reason": "same owner, seam kind, flow sink, missing discriminator, and assertion shape",
                  "raw_group_size": 1,
                  "why": "related tests reach the seam but miss the boundary discriminator",
                  "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                  "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_behavior",
                    "suggested_assertion": "assert_eq!(discount(threshold), expected_discount)"
                  },
                  "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                  "confidence": {"basis": "static_only", "notes": []}
                }
              }
            },
            {
              "seam_id": "raw-2",
              "headline_eligible": true,
              "file": "src/lib.rs",
              "evidence_record": {
                "schema_version": "0.1",
                "seam_id": "seam-2",
                "canonical_gap_id": "gap:shared",
                "canonical_gap_group_size": 2,
                "canonical_gap_reason": "same owner, seam kind, flow sink, missing discriminator, and assertion shape",
                "owner": "pricing::discount",
                "location": {"file": "src/lib.rs", "line": 18},
                "seam_kind": "predicate_boundary",
                "grip_class": "weakly_gripped",
                "headline_eligible": true,
                "evidence_path": {
                  "reach": {"state": "yes", "confidence": "high", "summary": "direct call"},
                  "activate": {"state": "yes", "confidence": "medium", "summary": "value nearby"},
                  "propagate": {"state": "weak", "confidence": "medium", "summary": "return value"},
                  "observe": {"state": "weak", "confidence": "medium", "summary": "broad assertion"},
                  "discriminate": {"state": "no", "confidence": "unknown", "summary": "boundary missing"}
                },
                "observed_values": [],
                "missing_discriminators": [{"value": "amount == threshold", "reason": "boundary value not observed", "flow_sink": {"kind": "return_value", "text": "discounted", "line": 18}}],
                "related_tests_total": 1,
                "related_tests": [{
                  "name": "discounts_smoke",
                  "file": "tests/pricing.rs",
                  "line": 22,
                  "oracle_kind": "broad_error",
                  "oracle_strength": "weak",
                  "evidence_summary": "only checks broad behavior",
                  "relation_reason": "same_test_file",
                  "relation_confidence": "low",
                  "oracle_semantics": {
                    "observes": "broad behavior",
                    "missing": "exact boundary",
                    "upgrade_suggestion": null
                  }
                }],
                "recommendation": {
                  "action": "write_targeted_test",
                  "reason": "missing discriminator",
                  "assertion_shape": {"kind": "exact_value", "example": "assert_eq!(...)"},
                  "verify_command": "ripr agent verify --root . --before before.json --after after.json --json"
                },
                "actionability": {"class": "actionable_assertion_upgrade"},
                "calibration": {"availability": "not_imported", "confidence": "unknown", "agreement": "no_runtime_data"},
                "static_limitations": [],
                "raw_findings": [{
                  "file": "src/lib.rs",
                  "line": 18,
                  "kind": "weakly_exposed",
                  "expression": "discount(amount)"
                }],
                "canonical_item": {
                  "canonical_gap_id": "gap:shared",
                  "canonical_item_kind": "gap",
                  "evidence_class": "predicate_boundary",
                  "gap_state": "actionable",
                  "actionability": "upgrade_assertion",
                  "group_reason": "same owner, seam kind, flow sink, missing discriminator, and assertion shape",
                  "raw_group_size": 1,
                  "why": "related tests reach the seam but miss the boundary discriminator",
                  "recommended_repair": "Add an equality-boundary assertion for the changed predicate.",
                  "repair_route": {
                    "repair_kind": "add_boundary_assertion",
                    "target_test_type": "boundary_behavior",
                    "suggested_assertion": "assert_eq!(discount(threshold), expected_discount)"
                  },
                  "verify_command": "ripr agent verify --root . --before before.json --after after.json --json",
                  "confidence": {"basis": "static_only", "notes": []}
                }
              }
            },
            {
              "seam_id": "raw-3",
              "headline_eligible": true,
              "file": "src/opaque.rs",
              "evidence_record": {
                "schema_version": "0.1",
                "seam_id": "seam-3",
                "canonical_gap_id": null,
                "canonical_gap_group_size": null,
                "canonical_gap_reason": null,
                "owner": "pricing::opaque",
                "location": {"file": "src/opaque.rs", "line": 30},
                "seam_kind": "call_presence",
                "grip_class": "activation_unknown",
                "headline_eligible": true,
                "evidence_path": {
                  "reach": {"state": "yes", "confidence": "medium", "summary": "same module"},
                  "activate": {"state": "unknown", "confidence": "unknown", "summary": "opaque helper"},
                  "propagate": {"state": "unknown", "confidence": "unknown", "summary": "opaque helper"},
                  "observe": {"state": "unknown", "confidence": "unknown", "summary": "opaque helper"},
                  "discriminate": {"state": "unknown", "confidence": "unknown", "summary": "opaque helper"}
                },
                "observed_values": [],
                "missing_discriminators": [],
                "related_tests_total": 0,
                "related_tests": [],
                "recommendation": {"action": "inspect_static_limitation", "reason": "opaque helper", "verify_command": null},
                "actionability": {"class": "static_limitation"},
                "calibration": {"availability": "imported", "confidence": "medium", "agreement": "static_runtime_agree"},
                "static_limitations": [{
                  "stage": "activate",
                  "state": "unknown",
                  "reason": "opaque helper value",
                  "category": "opaque_helper_call",
                  "repair_route": "analysis/oracle-semantics-audit-fixes"
                }],
                "raw_findings": [{
                  "file": "src/opaque.rs",
                  "line": 30,
                  "kind": "static_unknown",
                  "expression": "opaque_helper(value)"
                }],
                "canonical_item": {
                  "canonical_gap_id": null,
                  "canonical_item_kind": "limitation",
                  "evidence_class": "call_presence",
                  "gap_state": "static_limitation",
                  "actionability": "static_limitation",
                  "group_reason": null,
                  "raw_group_size": 1,
                  "why": "static evidence is opaque for this seam",
                  "recommended_repair": "Inspect static limitation `opaque_helper_call` via `analysis/oracle-semantics-audit-fixes`.",
                  "verify_command": null,
                  "confidence": {"basis": "calibrated", "notes": ["sample imported runtime agreement"]}
                }
              }
            },
            {
              "seam_id": "raw-4",
              "headline_eligible": true,
              "file": "src/missing.rs"
            }
          ]
        }"#
}

fn write_packaging_test_vsix(
    path: &std::path::Path,
    members: &[(&str, &str)],
) -> Result<(), String> {
    let file = fs::File::create(path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for (name, body) in members {
        writer
            .start_file(*name, options)
            .map_err(|err| format!("failed to stage {name}: {err}"))?;
        std::io::Write::write_all(&mut writer, body.as_bytes())
            .map_err(|err| format!("failed to write {name}: {err}"))?;
    }
    writer
        .finish()
        .map_err(|err| format!("failed to seal {}: {err}", path.display()))?;
    Ok(())
}

fn write_symlink_license_txt_vsix(path: &std::path::Path, body: &str) -> Result<(), String> {
    let file = fs::File::create(path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    writer
        .add_symlink(
            "extension/LICENSE.txt",
            body,
            zip::write::SimpleFileOptions::default(),
        )
        .map_err(|err| format!("failed to stage symlink LICENSE.txt: {err}"))?;
    writer
        .finish()
        .map_err(|err| format!("failed to seal {}: {err}", path.display()))?;
    Ok(())
}

/// POSIX / APPNOTE constants used only to craft typed-entry fixtures. The
/// zip crate's public writer cannot set Unix type bits or the DOS directory
/// flag on a slash-less name, so tests write a regular LICENSE.txt member
/// and patch the central-directory metadata.
const UNIX_S_IFDIR: u32 = 0o040_000;

const UNIX_S_IFIFO: u32 = 0o010_000;

const UNIX_S_IFREG: u32 = 0o100_000;

const DOS_DIRECTORY_ATTRIBUTE: u32 = 0x10;

const DOS_ARCHIVE_ATTRIBUTE: u32 = 0x20;

const ZIP_SYSTEM_DOS: u8 = 0;

const ZIP_SYSTEM_UNIX: u8 = 3;

const ZIP_CENTRAL_HEADER_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];

const ZIP_CENTRAL_VERSION_MADE_BY_OFFSET: usize = 4;

const ZIP_CENTRAL_EXTERNAL_ATTRIBUTES_OFFSET: usize = 38;

fn write_typed_license_txt_vsix(
    path: &std::path::Path,
    body: &str,
    system: u8,
    external_attributes: u32,
) -> Result<(), String> {
    write_packaging_test_vsix(path, &[("extension/LICENSE.txt", body)])?;
    let central_header_start = {
        let file = fs::File::open(path)
            .map_err(|err| format!("failed to reopen {}: {err}", path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        archive
            .by_name("extension/LICENSE.txt")
            .map_err(|err| format!("typed LICENSE.txt fixture missing member: {err}"))?
            .central_header_start()
    };
    let mut bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let start = usize::try_from(central_header_start).map_err(|err| {
        format!("typed LICENSE.txt fixture central-directory offset overflow: {err}")
    })?;
    let version_end = start
        .checked_add(ZIP_CENTRAL_VERSION_MADE_BY_OFFSET + 2)
        .ok_or_else(|| "typed LICENSE.txt fixture central-directory offset overflow".to_string())?;
    let attributes_end = start
        .checked_add(ZIP_CENTRAL_EXTERNAL_ATTRIBUTES_OFFSET + 4)
        .ok_or_else(|| "typed LICENSE.txt fixture central-directory offset overflow".to_string())?;
    if bytes.get(start..start + 4) != Some(ZIP_CENTRAL_HEADER_SIGNATURE.as_slice()) {
        return Err("typed LICENSE.txt fixture lost its central-directory header".to_string());
    }
    {
        let Some(version_bytes) =
            bytes.get_mut(start + ZIP_CENTRAL_VERSION_MADE_BY_OFFSET..version_end)
        else {
            return Err("typed LICENSE.txt fixture is shorter than version-made-by".to_string());
        };
        let [lo, hi] = *version_bytes else {
            return Err("typed LICENSE.txt fixture version-made-by is truncated".to_string());
        };
        let patched = (u16::from(system) << 8) | (u16::from_le_bytes([lo, hi]) & 0x00ff);
        version_bytes.copy_from_slice(&patched.to_le_bytes());
    }
    let Some(attribute_bytes) =
        bytes.get_mut(start + ZIP_CENTRAL_EXTERNAL_ATTRIBUTES_OFFSET..attributes_end)
    else {
        return Err("typed LICENSE.txt fixture is shorter than external attributes".to_string());
    };
    attribute_bytes.copy_from_slice(&external_attributes.to_le_bytes());
    fs::write(path, bytes).map_err(|err| format!("failed to patch {}: {err}", path.display()))?;
    assert_typed_license_txt_fixture(path, system, external_attributes)
}

fn assert_typed_license_txt_fixture(
    path: &std::path::Path,
    system: u8,
    external_attributes: u32,
) -> Result<(), String> {
    let file = fs::File::open(path)
        .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| format!("failed to parse typed fixture {}: {err}", path.display()))?;
    let member = archive
        .by_name("extension/LICENSE.txt")
        .map_err(|err| format!("typed LICENSE.txt fixture missing member: {err}"))?;
    if !member.is_file() {
        return Err(
            "typed LICENSE.txt fixture must keep a slash-less name so zip::is_file stays true"
                .to_string(),
        );
    }
    let actual_attributes = {
        use zip::HasZipMetadata;
        member.get_metadata().external_attributes
    };
    if actual_attributes != external_attributes {
        return Err(format!(
            "typed LICENSE.txt fixture attributes {actual_attributes:#010x} != {external_attributes:#010x}"
        ));
    }
    let Some(mode) = member.unix_mode() else {
        if system == ZIP_SYSTEM_DOS && external_attributes == 0 {
            return Ok(());
        }
        return Err(
            "typed LICENSE.txt fixture lost unix_mode after the metadata patch".to_string(),
        );
    };
    if system == ZIP_SYSTEM_UNIX {
        let expected_mode = external_attributes >> 16;
        if mode != expected_mode {
            return Err(format!(
                "typed Unix LICENSE.txt fixture unix_mode {mode:#o} != {expected_mode:#o}"
            ));
        }
    }
    Ok(())
}

fn vsix_entry(name: &str, size: u64) -> super::VsixEntry {
    super::VsixEntry {
        name: name.to_string(),
        size,
        compressed_size: size / 4,
    }
}

const POINTER_LICENSE: &str = "\
MIT OR Apache-2.0

The ripr VS Code extension is distributed under the same license terms as the
ripr repository. See the repository root for LICENSE-MIT and LICENSE-APACHE.
";

const COMPLETE_MIT_NOTICE: &str = "\
MIT License

Copyright (c) 2026 ripr contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:
";

const COMPLETE_APACHE_NOTICE: &str = "\
Apache License
Version 2.0, January 2004
http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

\"License\" shall mean the terms and conditions for use, reproduction,
and distribution as defined by Sections 1 through 9 of this document.
";

fn complete_license_notices() -> super::RepositoryLicenseNotices {
    super::RepositoryLicenseNotices {
        mit: COMPLETE_MIT_NOTICE.as_bytes().to_vec(),
        apache: COMPLETE_APACHE_NOTICE.as_bytes().to_vec(),
    }
}

fn write_license_fixture(root: &Path, name: &str, body: &str) -> Result<(), String> {
    let path = root.join(name);
    fs::write(&path, body).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn packaged_license_txt(mit: &str, apache: &str) -> String {
    format!("MIT OR Apache-2.0\n\n{mit}\n\n{apache}\n")
}

fn vscode_job_block(workflow: &str) -> Result<&str, String> {
    const HEADER: &str = "\n  vscode:\n";
    let start = workflow
        .find(HEADER)
        .ok_or_else(|| "ci.yml is missing the vscode job".to_string())?;
    let mut offset = start + HEADER.len();
    while offset < workflow.len() {
        let rest = &workflow[offset..];
        let (line, line_end) = match rest.find('\n') {
            Some(idx) => (&rest[..idx], offset + idx + 1),
            None => (rest, workflow.len()),
        };
        if let Some(key) = line.strip_prefix("  ")
            && !key.starts_with([' ', '-', '#'])
            && key.contains(':')
            && key.starts_with(char::is_alphabetic)
        {
            return Ok(&workflow[start..offset]);
        }
        offset = line_end;
    }
    Ok(&workflow[start..])
}

fn vscode_diagnostics_upload_step(job: &str) -> Option<&str> {
    const MARKER: &str = "- name: Upload VS Code test host diagnostics";
    let start = job.find(MARKER)?;
    let rest = &job[start..];
    let end = rest.find("\n      - ").unwrap_or(rest.len());
    Some(&rest[..end])
}

fn vscode_host_diagnostics_contract_violations(job: &str) -> Vec<String> {
    let mut violations = Vec::new();
    if !job.contains("xvfb-run -a npm run test:e2e") {
        violations.push("vscode job is missing the e2e host step".to_string());
    }
    if job.contains("continue-on-error") {
        violations.push(
            "continue-on-error would launder the original VS Code test exit status".to_string(),
        );
    }
    let Some(upload) = vscode_diagnostics_upload_step(job) else {
        violations.push("vscode job is missing the failure-only diagnostics upload".to_string());
        return violations;
    };
    let conditions: Vec<&str> = upload
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("if:"))
        .collect();
    if conditions.as_slice() != ["if: failure()"] {
        violations.push(format!(
            "diagnostics upload if must be exactly failure(), got {conditions:?}"
        ));
    }
    if upload.contains("if: always()") || upload.contains("if: success()") {
        violations.push("diagnostics upload must not run on success".to_string());
    }
    if !upload.contains("actions/upload-artifact@v7") {
        violations.push("diagnostics upload must use actions/upload-artifact@v7".to_string());
    }
    if !upload.contains("target/ripr/vscode-test-host-diagnostics/") {
        violations
            .push("diagnostics upload must use the pinned host diagnostics folder".to_string());
    }
    if !upload.contains("retention-days: 5") {
        violations.push("diagnostics upload must cap retention at 5 days".to_string());
    }
    if !upload.contains("if-no-files-found: ignore") {
        violations
            .push("missing diagnostics must not replace the original test failure".to_string());
    }
    violations
}

fn lsp_fixture<'a>(
    report: &'a LspCockpitReport,
    name: &str,
) -> Result<&'a LspCockpitFixture, String> {
    report
        .fixtures
        .iter()
        .find(|fixture| fixture.fixture == name)
        .ok_or_else(|| format!("expected `{name}` LSP cockpit fixture"))
}

fn assert_preview_lsp_fixture(
    fixture: &str,
    language: &str,
    static_limit_kind: &str,
) -> Result<(), String> {
    let expected = repo_root()?.join("fixtures").join(fixture).join("expected");
    let diagnostics = read_lsp_cockpit_json_value(&expected.join("lsp-diagnostics.json"))?;
    let data = &diagnostics["diagnostics"][0]["data"];
    assert_eq!(data["language"], language);
    assert_eq!(data["language_status"], "preview");
    assert_eq!(data["static_limit_kind"], static_limit_kind);

    let actions = read_lsp_cockpit_json_value(&expected.join("lsp-code-actions.json"))?;
    assert_eq!(actions["actions"][0]["command"], "ripr.copyContext");
    assert_eq!(actions["actions"][1]["command"], "ripr.refresh");

    let hover = fs::read_to_string(expected.join("lsp-hover.md"))
        .map_err(|err| format!("failed to read preview hover fixture: {err}"))?;
    let boundary = hover
        .find("## Preview Boundary")
        .ok_or_else(|| format!("{fixture} hover missing preview boundary"))?;
    let static_limit = hover
        .find(&format!("Static limit: {static_limit_kind}"))
        .ok_or_else(|| format!("{fixture} hover missing static limit"))?;
    let evidence = hover
        .find("## RIPR Evidence")
        .ok_or_else(|| format!("{fixture} hover missing evidence section"))?;
    let action = hover
        .find("## Suggested Action")
        .ok_or_else(|| format!("{fixture} hover missing suggested action section"))?;
    if !(boundary < static_limit && static_limit < evidence && evidence < action) {
        return Err(format!(
            "{fixture} hover must show preview/static-limit text before evidence and action"
        ));
    }
    let static_limits = fs::read_to_string(expected.join("static-limits.md"))
        .map_err(|err| format!("failed to read preview static-limits fixture: {err}"))?;
    if !static_limits.contains(static_limit_kind) {
        return Err(format!(
            "{fixture} static-limits fixture missing `{static_limit_kind}`"
        ));
    }
    Ok(())
}

fn assert_preview_disabled_fixture() -> Result<(), String> {
    let expected = repo_root()?.join("fixtures/python_disabled/expected");
    let diagnostics = read_lsp_cockpit_json_value(&expected.join("lsp-diagnostics.json"))?;
    let diagnostics_array = diagnostics["diagnostics"]
        .as_array()
        .ok_or_else(|| "python_disabled diagnostics should be an array".to_string())?;
    assert!(
        diagnostics_array.is_empty(),
        "disabled Python fixture must not project preview diagnostics"
    );
    let status = read_lsp_cockpit_json_value(&expected.join("vscode-status.json"))?;
    assert_eq!(status["states"][0]["name"], "preview_disabled");
    let static_limits = fs::read_to_string(expected.join("static-limits.md"))
        .map_err(|err| format!("failed to read disabled static-limits fixture: {err}"))?;
    if !static_limits.contains("no Python preview diagnostics") {
        return Err("disabled fixture must explain no preview diagnostics".to_string());
    }
    Ok(())
}

/// One observed resource-cost receipt body, in the exact wire shape the
/// analyzer emits (#5213).
fn observed_resource_cost_receipt() -> String {
    serde_json::json!({
        "schema_version": "0.1",
        "observer": "ripr_process_self",
        "observer_pid": 4242,
        "host_os": "linux",
        "host_arch": "x86_64",
        "cpu": {
            "state": "observed",
            "source_unit": "linux_user_hz_clock_ticks",
            "source_unit_per_second": 100,
            "user_source": 1234,
            "system_source": 56,
            "user_ms": 12340,
            "system_ms": 560,
        },
        "peak_resident_bytes": {"state": "observed", "value": 41943040},
    })
    .to_string()
}

/// The same receipt from a host with no safe per-process source. The numbers
/// are absent, not zero. `macos` is unwired; Linux and Windows both observe.
fn unavailable_resource_cost_receipt() -> String {
    serde_json::json!({
        "schema_version": "0.1",
        "observer": "ripr_process_self",
        "observer_pid": 4242,
        "host_os": "macos",
        "host_arch": "x86_64",
        "cpu": {"state": "unavailable", "reason": "platform_not_supported"},
        "peak_resident_bytes": {"state": "unavailable", "reason": "platform_not_supported"},
    })
    .to_string()
}

/// The Windows-observed receipt shape: CPU in 100-nanosecond units at
/// 10 000 000 per second, peak working set already in bytes.
fn windows_observed_resource_cost_receipt() -> String {
    serde_json::json!({
        "schema_version": "0.1",
        "observer": "ripr_process_self",
        "observer_pid": 4242,
        "host_os": "windows",
        "host_arch": "x86_64",
        "cpu": {
            "state": "observed",
            "source_unit": "windows_hundred_nanoseconds",
            "source_unit_per_second": 10_000_000,
            "user_source": 156_250,
            "system_source": 625_000,
            "user_ms": 15,
            "system_ms": 62,
        },
        "peak_resident_bytes": {"state": "observed", "value": 15_069_184},
    })
    .to_string()
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
        file_fact_cache: None,
        file_fact_cache_limitation: Some("cache_phase_not_observed".to_string()),
        resource_cost: None,
        resource_cost_limitation: Some("resource_cost_receipt_not_observed".to_string()),
    }
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

/// Issue #2258: every routed-rust required lane must invoke the shared
/// `cargo xtask precommit` gate table instead of enumerating a subset, so the
/// lanes cannot drift from the precommit contract again.
fn routed_rust_workflow_text() -> Result<String, String> {
    let path = repo_root()?.join(".github/workflows/routed-rust.yml");
    std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))
}

fn reusable_rust_workflow_text() -> Result<String, String> {
    let path = repo_root()?.join(".github/workflows/rust-gates.yml");
    std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))
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

fn release_pin_ruleset_contract(repo_root: &Path, jq_root: &Path) -> Result<(), String> {
    use crate::reports::release::candidate_harness::{AdmittedSource, QualificationInput};
    use sha2::Digest;
    const REQUIRED_PATTERN: &str = "refs/tags/ripr-release-*";
    const SHORT_PATTERN: &str = "ripr-release-*";
    const JQ_PREDICATE: &str = r#"(.target == "tag" and .enforcement == "active") and (.conditions.ref_name.include == [$tag]) and (any(.rules[]?; .type == "update")) and (any(.rules[]?; .type == "deletion")) and (.conditions.ref_name.exclude == []) and (.bypass_actors == [])"#;

    let fixture: Value = serde_json::from_slice(
        &fs::read(repo_root.join("fixtures/release_control/pin-ruleset.json"))
            .map_err(|error| format!("failed to read pin ruleset fixture: {error}"))?,
    )
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
                .pointer("/conditions/ref_name/exclude")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            && ruleset
                .get("bypass_actors")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
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
    let mut invalid = Vec::new();
    for (name, patterns) in [
        ("short", serde_json::json!([SHORT_PATTERN])),
        (
            "mixed",
            serde_json::json!([REQUIRED_PATTERN, SHORT_PATTERN]),
        ),
        ("branch", serde_json::json!(["refs/heads/ripr-release-*"])),
        (
            "mismatched",
            serde_json::json!(["refs/tags/another-release-*"]),
        ),
    ] {
        let mut changed = fixture.clone();
        changed["conditions"]["ref_name"]["include"] = patterns;
        invalid.push((name, changed));
    }
    for (name, pointer, replacement) in [
        (
            "excluded",
            "/conditions/ref_name/exclude",
            serde_json::json!(["refs/tags/ripr-release-0.11.0-hidden"]),
        ),
        (
            "bypass",
            "/bypass_actors",
            serde_json::json!([{"actor_id": 1, "actor_type": "RepositoryRole", "bypass_mode": "always"}]),
        ),
        (
            "missing-exclusions",
            "/conditions/ref_name/exclude",
            Value::Null,
        ),
        ("missing-bypass", "/bypass_actors", Value::Null),
    ] {
        let mut changed = fixture.clone();
        *changed
            .pointer_mut(pointer)
            .ok_or_else(|| format!("fixture lacks {pointer}"))? = replacement;
        invalid.push((name, changed));
    }
    if !accepts_required_pin(&fixture) {
        return Err("fully qualified tag ruleset fixture was rejected".to_string());
    }
    for (name, ruleset) in &invalid {
        if accepts_required_pin(ruleset) {
            return Err(format!(
                "invalid {name} ruleset was accepted as a protected pin"
            ));
        }
    }
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
    let run_jq_predicate = |ruleset: &Value, name: &str| -> Result<bool, String> {
        let path = jq_root.join(format!("{name}.json"));
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
        let _cwd_guard = super::acquire_test_cwd_read_guard();
        command_success_owned("jq", &args)
    };
    if !run_jq_predicate(&fixture, "full")? {
        return Err("documented jq predicate rejected the full fixture".to_string());
    }
    for (name, ruleset) in &invalid {
        if run_jq_predicate(ruleset, name)? {
            return Err(format!(
                "documented jq predicate accepted the {name} fixture"
            ));
        }
    }
    let artifact = "docs/release-candidates/0.11.0-live-head-selection.json";
    let template_bytes = fs::read(repo_root.join(artifact))
        .map_err(|error| format!("failed to read live-head template: {error}"))?;
    let template: Value = serde_json::from_slice(&template_bytes)
        .map_err(|error| format!("failed to parse live-head template: {error}"))?;
    if template.get("schema_version").and_then(Value::as_str) != Some("1.1")
        || template.get("status").and_then(Value::as_str) != Some("active_selection_template")
        || template.get("candidate") != Some(&Value::Null)
        || template.get("pin") != Some(&Value::Null)
    {
        return Err("live-head schema-1.1 template must not carry a candidate pin".to_string());
    }
    let input = QualificationInput::new(
        repo_root.to_path_buf(),
        jq_root.join("unused-source"),
        PathBuf::from(artifact),
    )?
    .with_approved_manifest_digest(format!("{:x}", sha2::Sha256::digest(&template_bytes)))?;
    match AdmittedSource::admit(&input, "0.11.0") {
        Err(error) if error.contains("not pinned_exact_head") => Ok(()),
        Err(error) => Err(format!("wrong template admission refusal: {error}")),
        Ok(_) => {
            Err("correctly hashed selection template acquired candidate authority".to_string())
        }
    }
}

fn gh_pr_status_identity_response(fixture: &Value) -> String {
    let mut identity = fixture.clone();
    if identity.get("baseRefOid").is_none() {
        identity["baseRefOid"] = serde_json::json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    }
    serde_json::json!({"data":{"repository":{"pullRequest":identity}}}).to_string()
}
