#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use ripr::output::receipt_lifecycle::{
    RECEIPT_FOUND, RECEIPT_GAP_MISMATCH, RECEIPT_MISSING, RECEIPT_MOVEMENT_IMPROVED,
    RECEIPT_MOVEMENT_UNCHANGED, RECEIPT_NOT_APPLICABLE, receipt_lifecycle_state_from_movement,
    receipt_lifecycle_state_is_present,
};
use ripr::output::start_here_state::{
    START_HERE_ACTIONABLE_GAP, START_HERE_CLEAN, START_HERE_MISSING_ARTIFACTS,
    START_HERE_PREVIEW_LIMITED,
};

mod agent_skills;
mod branch_inventory;
mod cache;
mod command;
pub mod convergence;
mod dispatch;
mod dogfood;
mod evidence_audit;
mod evidence_promotion;
mod evidence_quality;
mod fixture_contracts;
mod no_panic;
mod policy;
mod public_api_surface;
mod repo_readiness;
mod schema_pattern;
mod types;
pub(crate) use types::*;
mod reports;
mod ripr_swarm;
mod run;
mod rust_judged_panel;
mod verification_contracts;
mod version;
mod windows_advisory;

use command::{
    CommandCatalogEntry, XtaskCommand, command_catalog, known_command_root, known_commands,
};
#[cfg(test)]
use command::{help_message, unknown_command_message};
#[cfg(test)]
pub(crate) use dogfood::{
    BunUbPreviewSummaryArgs, ConfiguredBridgeInventoryArgs, CrossLanguageOracleGraphCase,
    CrossLanguageOracleGraphRawRef, DogfoodBunUbCrossLanguageRun,
    DogfoodBunUbCrossLanguageScenario, DogfoodEditorFirstPrBridgeRun, DogfoodEditorGapCockpitRun,
    DogfoodFindingAlignmentRun, DogfoodFindingAlignmentScenario, DogfoodFirstActionRun,
    DogfoodFirstPrRun, DogfoodFrontPanelRun, DogfoodGateRun, DogfoodGeneratedCiCockpitRun,
    DogfoodLanguagePreviewRun, DogfoodPrInlineCommentRun, DogfoodPreviewProjectionRuns,
    DogfoodPythonNoActionEvalScenario, DogfoodPythonRankedFinding, DogfoodPythonRealRepoEvalRun,
    DogfoodPythonRealRepoEvalScenario, DogfoodPythonStaticLimitEvalScenario,
    DogfoodRealRepairAttemptRun, DogfoodRealRepairAttemptScenario, DogfoodReportInputs,
    DogfoodReportPacketIndexRun, DogfoodRun, DogfoodSurfaceProjectionAlignmentRun,
    DogfoodTypescriptPreviewRepairLoopRun, DogfoodTypescriptPreviewRepairLoopScenario,
    DogfoodUserSurfaceProjectionRun, GENERATED_CI_FIRST_ACTION_REPAIR,
    GENERATED_CI_FIRST_PR_REPAIR, GENERATED_CI_FRONT_PANEL_REPAIR,
    GENERATED_CI_PACKET_INDEX_REPAIR, TypeScriptBunUbCalibrationCase,
    bun_ub_calibration_report_markdown, bun_ub_calibration_report_value,
    bun_ub_preview_summary_markdown, bun_ub_preview_summary_report_value,
    configured_bridge_inventory_markdown, configured_bridge_inventory_report_value,
    cross_language_oracle_graph_cases, cross_language_oracle_graph_corpus_path,
    dogfood_bun_ub_cross_language_scenarios, dogfood_class_counts,
    dogfood_editor_first_pr_bridge_run, dogfood_editor_first_pr_bridge_scenarios,
    dogfood_editor_gap_cockpit_run, dogfood_editor_gap_cockpit_scenarios,
    dogfood_first_action_scenarios, dogfood_first_pr_metrics, dogfood_first_pr_run,
    dogfood_first_pr_scenarios, dogfood_gate_adoption_run, dogfood_gate_adoption_scenarios,
    dogfood_generated_ci_cockpit_run_from_workflow, dogfood_language_preview_run,
    dogfood_language_preview_scenarios, dogfood_pr_inline_comment_run,
    dogfood_pr_inline_comment_scenarios, dogfood_pr_review_front_panel_run,
    dogfood_pr_review_front_panel_scenarios, dogfood_push_python_quality_ratio_json,
    dogfood_push_python_ranked_findings_json, dogfood_python_no_action_eval_scenarios,
    dogfood_python_ranked_findings, dogfood_python_real_repo_eval_scenarios,
    dogfood_python_static_limit_eval_scenarios, dogfood_report_json, dogfood_report_markdown,
    dogfood_report_packet_index_run, dogfood_report_packet_index_scenarios,
    dogfood_typescript_false_actionable_audit_summary,
    dogfood_typescript_preview_repair_loop_scenarios, finding_alignment_verify_command_is_missing,
    json_number_after, parse_bun_ub_preview_summary_args, parse_configured_bridge_inventory_args,
    repo_rooted_fixture_path, typescript_bun_ub_calibration_cases,
    typescript_preview_false_actionable_audit_cases,
};
pub(crate) use dogfood::{
    DogfoodSurfaceProjectionAlignmentScenario, DogfoodUserSurfaceProjectionScenario,
    bun_ub_calibration_impl, bun_ub_preview_summary_impl, configured_bridge_inventory_impl,
    cross_language_oracle_graph_case_errors, cross_language_oracle_graph_cases_at,
    cross_language_oracle_route_quality_push_markdown,
    cross_language_oracle_route_quality_report_value, dogfood_bun_ub_cross_language_run,
    dogfood_bun_ub_cross_language_scenarios_at, dogfood_finding_alignment_run,
    dogfood_finding_alignment_scenarios, dogfood_python_no_action_eval_run,
    dogfood_python_no_action_eval_scenarios_at, dogfood_python_real_repo_eval_run,
    dogfood_python_real_repo_eval_scenarios_at, dogfood_python_repair_routing_quality_summary,
    dogfood_python_static_limit_eval_run, dogfood_python_static_limit_eval_scenarios_at,
    dogfood_real_repair_attempt_run, dogfood_real_repair_attempt_scenarios,
    dogfood_surface_projection_alignment_run, dogfood_surface_projection_alignment_scenarios,
    dogfood_typescript_preview_repair_loop_run,
    dogfood_typescript_preview_repair_loop_scenarios_at, dogfood_user_surface_projection_run,
    dogfood_user_surface_projection_scenarios, json_string_array_field, json_summary_count,
    typescript_bun_ub_calibration_case_errors, typescript_bun_ub_calibration_cases_at,
    typescript_preview_false_actionable_audit_case_errors,
    typescript_preview_false_actionable_audit_cases_at,
};
#[cfg(test)]
pub(crate) use evidence_audit::{
    AuditActionableGapProjectionInput, LANE1_EVIDENCE_AUDIT_DEFAULT_TIMEOUT_MS,
    Lane1EvidenceAuditRepoExposureGeneration, Lane1EvidenceAuditRepoExposureOutcome,
    audit_actionable_gap_projection_exclusion_reasons,
    audit_actionable_gap_verify_command_with_source, lane1_evidence_audit_from_repo_exposure,
    lane1_evidence_audit_limited_report, lane1_evidence_audit_limited_report_from_run_limitation,
    lane1_evidence_audit_repo_exposure_args,
    lane1_evidence_audit_report_from_complete_repo_exposure, lane1_evidence_audit_timeout_error,
    lane1_evidence_audit_timeout_ms_from_env, lane1_parse_cache_store_skip_status,
    lane1_repo_exposure_cache_store_limitation, lane1_repo_exposure_file_looks_complete,
    lane1_repo_exposure_large_cache_preflight_limitation_for_root,
    lane1_static_limitation_backlog_json, parse_actionable_gap_outcomes_args,
    parse_lane1_evidence_audit_sample_seam_limit, static_limitation_category,
    static_limitation_repair_route_for_subroute, static_limitation_subroute,
    write_lane1_evidence_audit_repo_exposure_with_runner,
};
pub(crate) use evidence_audit::{
    CROSS_LANGUAGE_ORACLE_VISIBILITY_REPAIR_ROUTE,
    CROSS_LANGUAGE_ORACLE_VISIBILITY_UNRESOLVED_CATEGORY,
    CROSS_LANGUAGE_TARGET_UNRESOLVED_CATEGORY, CROSS_LANGUAGE_TARGET_UNRESOLVED_REPAIR_ROUTE,
    EVIDENCE_QUALITY_SCORECARD_AUDIT_REGENERATION_FAILED, LANE1_EVIDENCE_AUDIT_TOP_LIMIT,
    Lane1EvidenceAuditAlignmentClassCoverage, Lane1EvidenceAuditBuilder, Lane1EvidenceAuditReport,
    Lane1EvidenceAuditRunLimitation, Lane1EvidenceClassWorkItem, Lane1RuntimeStatus,
    Lane1StaticLimitationBacklogSample, actionable_gap_id_candidates,
    actionable_gap_outcome_state_counts_from_entries, actionable_gap_outcomes_json,
    actionable_gap_outcomes_markdown, actionable_gap_outcomes_missing_verify_result_count,
    actionable_gap_outcomes_report_from_values, actionable_gap_outcomes_report_impl,
    actionable_gap_push_id_candidate, audit_actionable_gap_target_test_shape, audit_array,
    audit_bool, audit_count_rows_json, audit_count_rows_map, audit_get,
    audit_guidance_field_is_missing, audit_identifier_slug, audit_increment, audit_markdown_cell,
    audit_non_empty_string, audit_push_count, audit_push_projection_exclusion_reason,
    audit_push_value_counts_table_limited, audit_slug, audit_string, audit_string_array,
    audit_structured_raw_evidence_refs_count, audit_usize, audit_verify_command_is_missing,
    audit_verify_command_is_unbounded_repo_exposure_snapshot_compare,
    lane1_actionable_gap_packets_json, lane1_actionable_gap_packets_markdown,
    lane1_evidence_audit_json, lane1_evidence_audit_markdown, lane1_evidence_audit_report_impl,
    lane1_runtime_status_from_report_value, lane1_runtime_status_full, lane1_runtime_status_json,
    lane1_runtime_status_limited_input, lane1_runtime_status_priority,
    lane1_runtime_status_push_markdown, lane1_runtime_status_with_input_path,
    lane1_static_limitation_backlog_sample_json, repo_exposure_latency_trace_json,
    static_limitation_backlog_packet_non_claims, static_limitation_repair_route,
    static_limitation_unlock_condition, static_limitation_why_not_actionable,
};
#[cfg(test)]
pub(crate) use evidence_promotion::{
    EVIDENCE_PROMOTION_EXTERNAL_JSON, EVIDENCE_PROMOTION_EXTERNAL_MD,
    EVIDENCE_PROMOTION_HONESTY_CORPUS, EvidencePromotionExternalCase,
    EvidencePromotionExternalLaunch, EvidencePromotionExternalRun,
    EvidencePromotionSemanticAssertion, ExpectedRepairPacketDetail,
    evidence_promotion_external_failure_kind, evidence_promotion_external_semantic_violations,
    evidence_promotion_human_class_line_matches, evidence_promotion_human_oracle_line_matches,
    evidence_promotion_pure_failure_kind, evidence_promotion_semantic_violations,
    validate_evidence_promotion_honesty_corpus_at, write_evidence_promotion_external_report,
};
pub(crate) use evidence_promotion::{
    check_evidence_promotion_honesty, validate_evidence_promotion_honesty_corpus,
};
pub(crate) use evidence_quality::{
    EVIDENCE_QUALITY_TREND_PREVIOUS_ARTIFACT_UNAVAILABLE, audit_evidence_class_work_queue,
    evidence_quality_scorecard_report_impl, evidence_quality_trend_report_impl,
    generated_at_unix_ms, report_has_run_limitations,
};
#[cfg(test)]
pub(crate) use evidence_quality::{
    EvidenceQualityScorecardInput, EvidenceQualityScorecardInputs, EvidenceQualityScorecardReport,
    EvidenceQualityTrendInputs, EvidenceQualityTrendReport,
    evidence_quality_scorecard_audit_regeneration_failure_audit,
    evidence_quality_scorecard_from_values, evidence_quality_scorecard_json,
    evidence_quality_scorecard_markdown, evidence_quality_trend_from_values,
    evidence_quality_trend_json, evidence_quality_trend_markdown,
    finding_alignment_raw_to_canonical_ratio,
};
pub(crate) use fixture_contracts::{
    BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE, BUN_FFI_NEGATIVE_OFFSET_TS_TEST_SURFACE,
    BUN_MARKDOWN_TS_TEST_FILE, BUN_NODE_FS_TS_TEST_FILE, BUN_UB_CROSS_LANGUAGE_DOGFOOD_CORPUS,
    BUN_WRITE_TS_TEST_FILE, CROSS_LANGUAGE_ORACLE_GRAPH_CORPUS, EVIDENCE_RECORD_CONTRACT_CORPUS,
    PYTHON_REAL_REPO_EVAL_CORPUS, REAL_REPAIR_ATTEMPTS_CORPUS, SURFACE_PROJECTION_ALIGNMENT_CORPUS,
    TYPESCRIPT_BUN_UB_CALIBRATION_CORPUS, TYPESCRIPT_PREVIEW_FALSE_ACTIONABLE_AUDIT_CORPUS,
    TYPESCRIPT_PREVIEW_REPAIR_LOOP_CORPUS, USER_SURFACE_PROJECTION_ALIGNMENT_CORPUS,
    USER_SURFACE_PROJECTION_REQUIRED_SURFACES, check_fixture_contracts,
    editor_first_pr_bridge_case_requires_first_screen_contract,
    validate_assistant_loop_health_fixture_corpus,
    validate_assistant_loop_health_fixture_corpus_at,
    validate_editor_first_pr_bridge_first_screen_contract,
    validate_first_successful_pr_actionable_json, validate_first_successful_pr_actionable_markdown,
};
#[cfg(test)]
pub(crate) use fixture_contracts::{
    BUN_UB_CROSS_LANGUAGE_DOGFOOD_REQUIRED_CASES, CROSS_LANGUAGE_ORACLE_GRAPH_REQUIRED_CASES,
    EDITOR_ADOPTION_ASSURANCE_CASES, EDITOR_FIRST_RUN_USABILITY_CASES,
    PYTHON_REAL_REPO_EVAL_REQUIRED_CASES, PYTHON_REAL_REPO_EVAL_REQUIRED_NO_ACTION_CASES,
    PYTHON_REAL_REPO_EVAL_REQUIRED_STATIC_LIMIT_CASES, REAL_REPAIR_ATTEMPTS_REQUIRED_CASES,
    TYPESCRIPT_BUN_UB_CALIBRATION_REQUIRED_CASES,
    TYPESCRIPT_PREVIEW_FALSE_ACTIONABLE_AUDIT_REQUIRED_CASES,
    TYPESCRIPT_PREVIEW_REPAIR_LOOP_REQUIRED_CASES, USER_SURFACE_PROJECTION_REQUIRED_RUN_STATUSES,
    editor_adoption_assurance_case_fails_closed, first_successful_pr_outcome_input_path,
    user_surface_projection_required_run_status_violations,
    user_surface_projection_source_alignment_errors, validate_actionable_gap_outcomes_fixture_case,
    validate_actionable_gap_outcomes_fixture_corpus,
    validate_actionable_gap_outcomes_fixture_corpus_at,
    validate_cross_language_oracle_graph_fixture_corpus_at,
    validate_editor_adoption_assurance_fixture_corpus,
    validate_editor_first_run_usability_fixture_corpus, validate_editor_gap_cockpit_fixture_case,
    validate_editor_gap_cockpit_fixture_corpus, validate_evidence_quality_benchmark_corpus_value,
    validate_evidence_quality_benchmark_fixture_corpus_at,
    validate_evidence_record_contract_fixture_corpus_at,
    validate_finding_alignment_dogfood_fixture_corpus,
    validate_first_successful_pr_fixture_corpus_at, validate_first_successful_pr_outcome_receipt,
    validate_gap_decision_ledger_corpus_value, validate_gap_decision_ledger_fixture_corpus_at,
    validate_lane1_evidence_quality_failure_fixture_corpus_at,
    validate_perl_lsp_facts_exporter_fixture_corpus,
    validate_perl_lsp_facts_exporter_fixture_corpus_at,
    validate_perl_real_repo_eval_fixture_corpus, validate_perl_real_repo_eval_fixture_corpus_at,
    validate_pr_inline_comment_publisher_fixture_corpus_at,
    validate_pr_review_front_panel_fixture_corpus_at,
    validate_report_packet_index_fixture_corpus_at, validate_swarm_plan_packet_fixture_case,
    validate_swarm_plan_packet_fixture_corpus,
    validate_typescript_bun_ub_calibration_fixture_corpus_at,
};
use no_panic::{
    contains_word, parse_string_value, parse_toml_key_value, parse_usize_value,
    strip_toml_value_comment,
};
use policy::{
    check_allow_attributes, check_ci_lane_whitelist, check_doc_roles, check_droid_review_config,
    check_executable_files, check_file_policy, check_local_context, check_network_policy,
    check_no_panic_family, check_positioning_language, check_process_policy, check_product_copy,
    check_proof_packs, check_release_targets, check_static_language, check_workflows,
};
use public_api_surface::public_api_surface;
#[cfg(test)]
use repo_readiness::{PrReadyStep, pr_ready_next_action, pr_ready_status_from_report_status};
use repo_readiness::{
    cockpit_json, cockpit_markdown, pr_ready_json, pr_ready_markdown, pr_ready_status,
    run_readiness_step,
};
#[cfg(test)]
pub(crate) use reports::release_server::{
    ReleaseServerAsset, normalize_release_version, release_server_archive, release_server_assets,
    release_server_manifest, release_server_readme, required_release_arg,
};
use reports::release_server::{create_zip_archive, sha256_file};
#[cfg(test)]
pub(crate) use reports::{
    BADGE_ENDPOINT_FILES, BadgeArtifactJob, BadgeBasisReport, BadgeBasisSignal,
    BadgeCanonicalProjection, BadgeCountBreakdown, BadgeEndpointSnapshot, BadgeNativeAuditSnapshot,
    BadgeNativeSlot, REPO_BADGE_ARTIFACT_DEFAULT_TIMEOUT_MS, REPO_BADGE_ARTIFACT_TIMEOUT_ENV,
    RepoBadgeArtifactOptions, badge_artifact_command_args, badge_artifact_command_label,
    badge_artifact_jobs, badge_artifact_native_slot, badge_artifacts_impl_with_runners,
    badge_artifacts_summary_markdown, badge_basis_canonical_projection,
    badge_basis_derived_ripr_plus_snapshot, badge_basis_needs_repo_badge_plus_job,
    badge_basis_report_json, badge_basis_report_markdown, badge_basis_seam_native_counts,
    badge_endpoint_violation, badge_native_audit_snapshot, check_badge_diff_policy_with_context,
    compute_badge_endpoint_violations, copy_badge_endpoints_from_reports, error_ripr_plus_receipt,
    extract_json_object_usize_map, extract_json_string, extract_json_warnings,
    limited_badge_artifacts_json, limited_badge_artifacts_markdown,
    parse_repo_badge_artifact_options, parse_repo_exposure_summary_counts,
    read_repo_exposure_summary_artifact, repo_badge_artifact_command_args,
    repo_badge_artifact_jobs, repo_badge_artifact_stdout_from_output,
    repo_badge_artifact_timeout_ms_from_env, repo_badge_artifacts_summary_markdown,
    ripr_plus_receipt_from_badge, ripr_plus_receipt_from_options,
    ripr_plus_receipt_from_repo_badge_json, ripr_plus_receipt_from_repo_exposure_summary_json,
    ripr_plus_receipt_from_repo_exposure_summary_json_with_source, ripr_plus_receipt_markdown,
    run_repo_badge_artifact_command, validate_shields_endpoint_bytes,
    write_badge_artifacts_after_build, write_badge_artifacts_from_diff,
};
pub(crate) use reports::{
    FixtureCheckFormat, fixture_dirs, goldens_check, is_manifest_only_fixture_dir,
    normalize_fixture_human_output, normalize_fixture_json_output, ripr_fixture_binary,
    run_fixture_check, yes_no,
};
#[cfg(test)]
pub(crate) use reports::{
    GoldenDriftEntry, GoldenDriftSemantics, first_line_difference, fixture_cache_dir,
    fixture_contract_violations, golden_assistant_loop_health_contract_violations_at,
    golden_drift_semantics, golden_drift_type, goldens_check_failure_message,
    json_string_values_for_key, normalize_golden_text, parse_reason, run_fixture,
    run_fixture_outputs, validate_bless_reason,
};
#[cfg(test)]
pub(crate) use reports::{
    MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT, MutationOutcomeRecord,
    SarifMissingBaseline, SarifPolicyMode, SarifPolicyResult, SarifPolicyThreshold,
    build_mutation_calibration_report, build_sarif_policy_report, mutation_calibration_report_json,
    mutation_calibration_report_markdown, parse_mutation_calibration_args,
    parse_mutation_outcomes_json, parse_sarif_policy_args, parse_sarif_policy_results,
    read_mutation_input_json, sarif_policy_report_json, sarif_policy_report_markdown,
};
use reports::{
    check_badge_diff_policy, dogfood, fixtures, metrics_report, pr_summary, receipts_write,
    reports_index, rust_repair_trust_report_value_at, test_oracle_report,
};
#[cfg(test)]
use reports::{lsp_cockpit_report, targeted_test_outcome};
pub(crate) use ripr_swarm::{
    RiprSwarmAttemptLedgerEntry, RiprSwarmReadinessInput, ripr_swarm,
    ripr_swarm_attempt_related_target_file, ripr_swarm_attempt_workspace_relative_file_token,
    ripr_swarm_plan_field_missing, ripr_swarm_plan_from_actionable_gaps_value,
    ripr_swarm_plan_packet_is_high_confidence, ripr_swarm_plan_packets_json,
    ripr_swarm_plan_related_target_file, ripr_swarm_plan_summary_json,
    ripr_swarm_push_static_limitation_backlog_markdown, ripr_swarm_readiness_from_values,
    ripr_swarm_readiness_json, ripr_swarm_route_quality_report,
};
#[cfg(test)]
pub(crate) use ripr_swarm::{
    RiprSwarmAttemptLedgerReport, RiprSwarmCommand, RiprSwarmReadinessNextActionSources,
    RiprSwarmRepairRouteQualityRow, parse_ripr_swarm_args, parse_ripr_swarm_plan_args,
    ripr_swarm_attempt_allowed_file_line, ripr_swarm_attempt_dry_run_from_actionable_gaps_value,
    ripr_swarm_attempt_dry_run_markdown, ripr_swarm_attempt_ledger_entries_from_value,
    ripr_swarm_attempt_ledger_from_values,
    ripr_swarm_attempt_ledger_from_values_with_real_repair_attempts,
    ripr_swarm_attempt_ledger_json, ripr_swarm_attempt_ledger_latest_attempts,
    ripr_swarm_attempt_ledger_markdown, ripr_swarm_attempt_ledger_repair_route_quality,
    ripr_swarm_plan_blocked_packets, ripr_swarm_plan_blocked_report,
    ripr_swarm_plan_blocked_state_examples_json, ripr_swarm_plan_json, ripr_swarm_plan_markdown,
    ripr_swarm_plan_ready_packets, ripr_swarm_read_optional_json,
    ripr_swarm_readiness_limited_runtime_command, ripr_swarm_readiness_markdown,
    ripr_swarm_readiness_next_actions, ripr_swarm_readiness_state, ripr_swarm_readiness_summary,
    ripr_swarm_readiness_top_limitation_routes, ripr_swarm_repair_route_quality_attempt_is_failure,
    ripr_swarm_repair_route_quality_failure_count, ripr_swarm_repair_route_quality_success_rate,
    ripr_swarm_route_quality_from_ledger_value, ripr_swarm_route_quality_report_json,
};
use run::{
    TimedFileOutput, TimedOutput, capture_output, capture_output_with_timeout,
    capture_stdout_to_file_with_timeout, command_success_owned, run, run_in_dir,
    run_in_dir_with_envs, run_output, run_output_optional, run_output_owned, run_owned,
    run_with_envs,
};

/// Process-wide fair reader-writer gate serialising tests that mutate the process
/// working directory. The xtask test suite spawns subprocesses (rustc, cargo,
/// git) that inherit the current cwd; meanwhile other tests (`with_temp_cwd`,
/// `with_repo_cwd`) call `std::env::set_current_dir` and delete the temporary
/// directory on teardown. Without serialisation, a subprocess spawn can land
/// in the tiny window between `set_current_dir(temp)` and
/// `remove_dir_all(temp)`, producing "Could not locate working directory" or
/// exit-status-1 failures in unrelated `run_output*` / `capture_output*`
/// tests.
///
/// Readers still run in parallel, while a waiting writer blocks new readers.
/// The standard library `RwLock` admitted enough new readers under Windows
/// parallel test load to starve the dispatch test's writer indefinitely (see
/// #2875), so this small gate makes writer admission explicit without
/// serialising the entire spawning-test lane. See issues #2044 and #2124.
#[cfg(test)]
struct CwdLockState {
    active_readers: usize,
    active_writer: bool,
    waiting_writers: usize,
}

#[cfg(test)]
pub(crate) struct CwdLock {
    state: std::sync::Mutex<CwdLockState>,
    available: std::sync::Condvar,
}

#[cfg(test)]
impl CwdLock {
    fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(CwdLockState {
                active_readers: 0,
                active_writer: false,
                waiting_writers: 0,
            }),
            available: std::sync::Condvar::new(),
        }
    }

    /// Acquire a shared reader slot.
    ///
    /// This gate is not reentrant: callers must not acquire another reader
    /// while holding a `CwdReadGuard`, because a queued writer would block the
    /// nested acquisition while the existing guard blocks that writer.
    fn read(&self) -> CwdReadGuard<'_> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while state.active_writer || state.waiting_writers > 0 {
            state = self
                .available
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        state.active_readers += 1;
        CwdReadGuard { lock: self }
    }

    /// Acquire the exclusive writer slot.
    ///
    /// This gate is not reentrant: callers must not acquire a writer while
    /// holding either a `CwdReadGuard` or a `CwdWriteGuard`.
    fn write(&self) -> CwdWriteGuard<'_> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.waiting_writers += 1;
        while state.active_writer || state.active_readers > 0 {
            state = self
                .available
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        state.waiting_writers = state.waiting_writers.saturating_sub(1);
        state.active_writer = true;
        CwdWriteGuard { lock: self }
    }
}

#[cfg(test)]
pub(crate) struct CwdReadGuard<'a> {
    lock: &'a CwdLock,
}

#[cfg(test)]
impl Drop for CwdReadGuard<'_> {
    fn drop(&mut self) {
        let active_readers = {
            let mut state = self
                .lock
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.active_readers = state.active_readers.saturating_sub(1);
            state.active_readers
        };
        if active_readers == 0 {
            self.lock.available.notify_all();
        }
    }
}

#[cfg(test)]
pub(crate) struct CwdWriteGuard<'a> {
    lock: &'a CwdLock,
}

#[cfg(test)]
impl Drop for CwdWriteGuard<'_> {
    fn drop(&mut self) {
        {
            let mut state = self
                .lock
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.active_writer = false;
        }
        self.lock.available.notify_all();
    }
}

#[cfg(test)]
pub(crate) static CWD_LOCK: std::sync::OnceLock<CwdLock> = std::sync::OnceLock::new();

/// Acquire the exclusive write guard for cwd-manipulating tests (those that
/// call `set_current_dir` or delete a temp cwd). Excludes every other cwd-
/// sensitive test — both writers and readers.
#[cfg(test)]
pub(crate) fn acquire_test_cwd_write_guard() -> CwdWriteGuard<'static> {
    CWD_LOCK.get_or_init(CwdLock::new).write()
}

/// Acquire the shared read guard for subprocess-spawning tests (those that
/// inherit the process cwd but do not change it). Multiple spawning tests can
/// hold the read guard concurrently; they are only excluded while a
/// cwd-manipulating test holds the write guard.
#[cfg(test)]
pub(crate) fn acquire_test_cwd_read_guard() -> CwdReadGuard<'static> {
    CWD_LOCK.get_or_init(CwdLock::new).read()
}

fn main() {
    let command = XtaskCommand::parse(std::env::args().skip(1));
    let result = dispatch::execute(command);
    if let Err(err) = result {
        eprintln!("xtask: {err}");
        std::process::exit(1);
    }
}

fn ci_fast() -> Result<(), String> {
    ci_fast_with_envs(&[])
}

fn ci_fast_with_envs(envs: &[(&str, &str)]) -> Result<(), String> {
    run_with_envs("cargo", &["fmt", "--check"], envs)?;
    run_with_envs("cargo", &["check", "--workspace", "--all-targets"], envs)?;
    run_with_envs("cargo", &["test", "--workspace"], envs)?;
    run_policy_checks()
}

/// The `cargo xtask` gate commands `precommit()` runs, in run order (the
/// leading `cargo fmt --check` is not an xtask command and stays out). The
/// command-catalog CI drift check expands an enforced `cargo xtask precommit`
/// workflow invocation into these gates (issue #2258), so the routed-rust
/// lanes can invoke the shared table once instead of enumerating each gate.
/// Keep aligned with `precommit()` and `precommit_report_body()`; the
/// `precommit_gate_commands_match_report` test pins the report to this list.
const PRECOMMIT_GATE_COMMANDS: &[&str] = &[
    "check-static-language",
    "check-no-panic-family",
    "check-allow-attributes",
    "check-local-context",
    "check-file-policy",
    "check-executable-files",
    "check-workflows",
    "check-droid-review-config",
    "check-spec-format",
    "check-spec-numbering",
    "check-fixture-contracts",
    "check-rust-judged-panel",
    "check-traceability",
    "check-capabilities",
    "check-workspace-shape",
    "check-architecture",
    "check-public-api",
    "check-output-contracts",
    "check-doc-artifacts",
    "check-doc-index",
    "check-readme-state",
    "markdown-links",
    "check-pr-shape",
    "check-command-catalog",
    "check-generated",
    "check-badge-diff-policy",
    "check-generated-clean",
    "check-proof-packs",
    "check-release-targets",
    "check-dependencies",
    "check-process-policy",
    "check-network-policy",
    "check-lint-policy",
];

fn precommit() -> Result<(), String> {
    ensure_reports_dir()?;
    run("cargo", &["fmt", "--check"])?;
    check_static_language()?;
    check_no_panic_family()?;
    check_allow_attributes()?;
    check_local_context()?;
    check_file_policy()?;
    check_executable_files()?;
    check_workflows()?;
    check_droid_review_config()?;
    check_spec_format()?;
    check_spec_numbering()?;
    check_fixture_contracts()?;
    check_rust_judged_panel()?;
    check_traceability()?;
    check_capabilities()?;
    check_workspace_shape()?;
    check_architecture()?;
    check_public_api()?;
    check_output_contracts()?;
    check_doc_artifacts()?;
    check_doc_index()?;
    check_readme_state()?;
    markdown_links()?;
    check_pr_shape()?;
    check_command_catalog()?;
    check_generated()?;
    check_badge_diff_policy()?;
    check_generated_clean()?;
    check_proof_packs()?;
    check_release_targets()?;
    check_dependencies()?;
    check_process_policy()?;
    check_network_policy()?;
    check_lint_policy()?;
    let body = precommit_report_body();
    write_report("precommit.md", &body)
}

fn check_rust_judged_panel() -> Result<(), String> {
    rust_judged_panel::check_canonical()
}

/// Diff-aware fast gate runner (#2343). Runs only the gates relevant to
/// changed files, plus a cheap always-run floor. Target: sub-30s for
/// doc-only changes, ~2min for Rust source changes.
pub(crate) fn check_fast() -> Result<(), String> {
    ensure_reports_dir()?;
    let changed = changed_files_vs_origin_main().unwrap_or_default();
    let categories = categorize_changed_files(&changed);
    let mut ran = Vec::new();
    let mut skipped = Vec::new();

    // Always run: these are cheap and catch cross-cutting drift.
    run("cargo", &["fmt", "--check"])?;
    ran.push("fmt --check");
    check_static_language()?;
    ran.push("check-static-language");
    check_command_catalog()?;
    ran.push("check-command-catalog");

    // Conditional gates based on changed file categories.
    if categories.rust_src {
        check_no_panic_family()?;
        ran.push("check-no-panic-family");
        check_allow_attributes()?;
        ran.push("check-allow-attributes");
        check_file_policy()?;
        ran.push("check-file-policy");
        run(
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        )?;
        ran.push("clippy");
    } else {
        skipped.extend_from_slice(&[
            "check-no-panic-family",
            "check-allow-attributes",
            "check-file-policy",
            "clippy",
        ]);
    }

    if categories.workflow {
        check_workflows()?;
        ran.push("check-workflows");
    }

    if categories.policy {
        check_process_policy()?;
        ran.push("check-process-policy");
        check_network_policy()?;
        ran.push("check-network-policy");
    }

    if categories.fixture {
        check_fixture_contracts()?;
        ran.push("check-fixture-contracts");
    } else {
        skipped.push("check-fixture-contracts");
    }

    // Always run these (cheap, cross-cutting).
    check_generated()?;
    ran.push("check-generated");
    check_generated_clean()?;
    ran.push("check-generated-clean");
    check_lint_policy()?;
    ran.push("check-lint-policy");

    eprintln!(
        "check-fast: ran {} gate(s){}, base=origin/main, {} file(s) changed",
        ran.len(),
        if skipped.is_empty() {
            String::new()
        } else {
            format!(", skipped {} ({})", skipped.len(), skipped.join(", "))
        },
        changed.len()
    );

    let body = format!(
        "# check-fast report\n\nStatus: pass\n\nRan:\n{}\n\nSkipped:\n{}\n",
        ran.iter()
            .map(|g| format!("- {g}"))
            .collect::<Vec<_>>()
            .join("\n"),
        if skipped.is_empty() {
            "- none".to_string()
        } else {
            skipped
                .iter()
                .map(|g| format!("- {g}"))
                .collect::<Vec<_>>()
                .join("\n")
        }
    );
    write_report("check-fast.md", &body)
}

struct ChangedFileCategories {
    rust_src: bool,
    workflow: bool,
    policy: bool,
    fixture: bool,
}

fn categorize_changed_files(files: &[String]) -> ChangedFileCategories {
    ChangedFileCategories {
        rust_src: files.iter().any(|f| {
            f.ends_with(".rs") && (f.contains("crates/ripr/src") || f.contains("xtask/src"))
        }),
        workflow: files.iter().any(|f| f.starts_with(".github/workflows/")),
        policy: files
            .iter()
            .any(|f| f.starts_with("policy/") || f.starts_with(".ripr/")),
        fixture: files.iter().any(|f| f.starts_with("fixtures/")),
    }
}

fn changed_files_vs_origin_main() -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "origin/main...HEAD"])
        .output()
        .map_err(|err| format!("git diff --name-only failed: {err}"))?;
    if !output.status.success() {
        return Err(
            "git diff --name-only origin/main...HEAD failed; if origin/main is not available, run `git fetch origin main` first".to_string(),
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().map(String::from).collect())
}

fn check_pr() -> Result<(), String> {
    ensure_reports_dir()?;
    let temp_env = check_pr_temp_env()?;
    let temp_env_refs = temp_env
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    // #3036: gates are lazy closures evaluated strictly in order — the first
    // failure stops the sequence, so later gates neither spend work nor emit
    // side effects after the outcome is already decided.
    let gates = [
        CheckPrGate {
            name: "ci-fast",
            reproduce: "cargo xtask ci-fast",
            run: &|| ci_fast_with_envs(&temp_env_refs),
        },
        CheckPrGate {
            name: "clippy",
            reproduce: "cargo clippy --workspace --all-targets -- -D warnings",
            run: &|| {
                run_with_envs(
                    "cargo",
                    &[
                        "clippy",
                        "--workspace",
                        "--all-targets",
                        "--",
                        "-D",
                        "warnings",
                    ],
                    &temp_env_refs,
                )
                .and_then(|status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(format!("clippy exited with {status}"))
                    }
                })
            },
        },
        CheckPrGate {
            name: "doc",
            reproduce: "cargo doc --workspace --no-deps",
            run: &|| {
                run_with_envs(
                    "cargo",
                    &["doc", "--workspace", "--no-deps"],
                    &temp_env_refs,
                )
                .and_then(|status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(format!("doc exited with {status}"))
                    }
                })
            },
        },
    ];
    run_check_pr_gates(&gates, &|failure| {
        write_report("check-pr.md", &check_pr_report(Some(failure)))
    })?;
    // If all gates passed, fall through to the original success path.
    pr_summary()?;
    write_report("check-pr.md", &check_pr_report(None))?;
    suggested_fixes()?;
    receipts_write()?;
    pr_summary()?;
    reports_index()
}

/// One check-pr sub-gate: a name, the command that reproduces just that gate,
/// and the lazy closure that runs it (#3036).
struct CheckPrGate<'a> {
    name: &'a str,
    reproduce: &'a str,
    run: &'a dyn Fn() -> Result<(), String>,
}

/// The first failed gate plus the bounded evidence the failure report
/// retains (#3036): the reproduce command, up to five CRLF-normalized error
/// lines (with an explicit truncation marker when more were dropped), and
/// the gates that were never invoked.
struct CheckPrGateFailure {
    name: String,
    reproduce: String,
    bounded_error: String,
    not_run: Vec<String>,
    baseline: BaselineFailureComparison,
}

struct BaselineFailureComparison {
    status: &'static str,
    detail: String,
}

/// Run the gates in order and stop at the first failure (#3036). On failure,
/// the current failure report is published via `write_failure_report` before
/// the labeled gate error is returned; a report-publication failure is
/// returned as a distinct error that still carries the original gate
/// diagnostic and reproduce command, so "gate failed" and "report failed"
/// are never conflated and no failure evidence is discarded precisely when
/// it is most needed.
fn run_check_pr_gates(
    gates: &[CheckPrGate],
    write_failure_report: &dyn Fn(&CheckPrGateFailure) -> Result<(), String>,
) -> Result<(), String> {
    for (index, gate) in gates.iter().enumerate() {
        if let Err(err) = (gate.run)() {
            let failure = CheckPrGateFailure {
                name: gate.name.to_string(),
                reproduce: gate.reproduce.to_string(),
                bounded_error: bound_first_failure_lines(&err),
                not_run: gates[index + 1..]
                    .iter()
                    .map(|later| later.name.to_string())
                    .collect(),
                baseline: compare_failure_with_origin_main(gate.name),
            };
            write_failure_report(&failure).map_err(|write_err| {
                format!(
                    "check-pr gate `{}` failed, and publishing the failure report also failed: {write_err}\nreproduce: {}\n{err}",
                    gate.name, gate.reproduce
                )
            })?;
            return label_check_pr_gate(gate.name, gate.reproduce, Err(err));
        }
    }
    Ok(())
}

fn compare_failure_with_origin_main(gate_name: &str) -> BaselineFailureComparison {
    let divergence = match run_output(
        "git",
        &["rev-list", "--left-right", "--count", "origin/main...HEAD"],
    ) {
        Ok(value) => value,
        Err(err) => {
            return BaselineFailureComparison {
                status: "NOT_PROVEN",
                detail: format!("origin/main is unavailable: {err}"),
            };
        }
    };
    let mut counts = divergence.split_whitespace();
    let behind = counts.next().and_then(|value| value.parse::<u64>().ok());
    let ahead = counts.next().and_then(|value| value.parse::<u64>().ok());
    if matches!((behind, ahead), (Some(0), Some(_))) {
        return BaselineFailureComparison {
            status: "NOT_PROVEN",
            detail: "the branch is not behind origin/main".to_string(),
        };
    }
    if !matches!((behind, ahead), (Some(_), Some(_))) {
        return BaselineFailureComparison {
            status: "NOT_PROVEN",
            detail: format!("could not parse origin/main divergence: {divergence:?}"),
        };
    }
    if gate_name == "ci-fast" {
        return BaselineFailureComparison {
            status: "NOT_PROVEN",
            detail: "ci-fast has no stable inner-failure identity for an inherited comparison"
                .to_string(),
        };
    }
    let root = match std::env::current_dir() {
        Ok(root) => root,
        Err(err) => {
            return BaselineFailureComparison {
                status: "NOT_PROVEN",
                detail: format!("could not resolve the PR checkout: {err}"),
            };
        }
    };
    let worktree = root
        .join("target")
        .join("tmp")
        .join("check-pr")
        .join("origin-main");
    if let Err(err) = fs::create_dir_all(worktree.parent().unwrap_or(&worktree)) {
        return BaselineFailureComparison {
            status: "NOT_PROVEN",
            detail: format!("could not create the comparison directory: {err}"),
        };
    }
    let worktree_text = worktree.to_string_lossy().into_owned();
    let _ = run("git", &["worktree", "prune", "--expire", "now"]);
    if worktree.exists() {
        let _ = run("git", &["worktree", "remove", "--force", &worktree_text]);
        if let Err(err) = fs::remove_dir_all(&worktree) {
            return BaselineFailureComparison {
                status: "NOT_PROVEN",
                detail: format!("could not clear the previous comparison worktree: {err}"),
            };
        }
    }
    if let Err(err) = run(
        "git",
        &[
            "worktree",
            "add",
            "--detach",
            "--quiet",
            &worktree_text,
            "origin/main",
        ],
    ) {
        return BaselineFailureComparison {
            status: "NOT_PROVEN",
            detail: format!("could not create the origin/main comparison worktree: {err}"),
        };
    }
    let result = run_origin_gate(gate_name, &worktree);
    let cleanup = run("git", &["worktree", "remove", "--force", &worktree_text]);
    if let Err(err) = cleanup {
        eprintln!("warning: failed to remove comparison worktree: {err}");
    }
    match result {
        Ok(()) => BaselineFailureComparison {
            status: "PR_INTRODUCED",
            detail: "the gate passed on origin/main and failed on this branch".to_string(),
        },
        Err(err) => BaselineFailureComparison {
            status: "INHERITED",
            detail: format!("the same gate failed on origin/main: {err}"),
        },
    }
}

fn run_origin_gate(gate_name: &str, worktree: &Path) -> Result<(), String> {
    let args: &[&str] = match gate_name {
        "ci-fast" => &["xtask", "ci-fast"],
        "clippy" => &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        "doc" => &["doc", "--workspace", "--no-deps"],
        _ => return Err(format!("unsupported check-pr gate: {gate_name}")),
    };
    run_in_dir(Path::new("cargo"), args, worktree).map(|_| ())
}

/// Bound the retained first-failure evidence to five lines with CRLF
/// normalized away (#3036). An empty error stays empty; the report composer
/// is responsible for making that state explicit. When lines are dropped, an
/// explicit truncation marker records how many, so bounded evidence never
/// presents itself as the whole diagnostic.
fn bound_first_failure_lines(err: &str) -> String {
    let normalized = err.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut kept: Vec<String> = lines
        .iter()
        .take(5)
        .map(|line| (*line).to_string())
        .collect();
    if lines.len() > 5 {
        kept.push(format!("(+{} more lines truncated)", lines.len() - 5));
    }
    kept.join("\n  ")
}

/// Attribute a `check-pr` sub-gate failure to the gate that produced it, with the
/// command that reproduces *just* that gate and a pointer to the reports. Without
/// this, a clippy or doc failure surfaces a bare tool error with no indication it
/// came from `check-pr` or how to re-run only the failing step.
fn label_check_pr_gate<T>(
    name: &str,
    reproduce: &str,
    result: Result<T, String>,
) -> Result<T, String> {
    result.map_err(|err| {
        format!(
            "check-pr gate `{name}` failed\nreproduce: {reproduce}\nreports: target/ripr/reports/check-pr.md\n{err}"
        )
    })
}

fn check_pr_temp_env() -> Result<Vec<(String, String)>, String> {
    let temp_dir = std::env::current_dir()
        .map_err(|err| format!("failed to resolve current directory for check-pr temp dir: {err}"))?
        .join("target")
        .join("tmp")
        .join("check-pr");
    fs::create_dir_all(&temp_dir).map_err(|err| {
        format!(
            "failed to create check-pr temp dir {}: {err}",
            temp_dir.display()
        )
    })?;
    let temp_dir = temp_dir.to_string_lossy().into_owned();
    Ok(vec![
        ("TEMP".to_string(), temp_dir.clone()),
        ("TMP".to_string(), temp_dir.clone()),
        ("TMPDIR".to_string(), temp_dir),
    ])
}

fn run_policy_checks() -> Result<(), String> {
    check_static_language()?;
    check_no_panic_family()?;
    check_allow_attributes()?;
    check_local_context()?;
    check_file_policy()?;
    check_executable_files()?;
    check_workflows()?;
    check_droid_review_config()?;
    check_spec_format()?;
    check_spec_numbering()?;
    check_fixture_contracts()?;
    check_traceability()?;
    check_capabilities()?;
    check_workspace_shape()?;
    check_architecture()?;
    check_public_api()?;
    check_output_contracts()?;
    check_doc_artifacts()?;
    check_doc_index()?;
    check_readme_state()?;
    markdown_links()?;
    check_pr_shape()?;
    check_command_catalog()?;
    check_generated()?;
    check_badge_diff_policy()?;
    check_generated_clean()?;
    check_proof_packs()?;
    check_release_targets()?;
    check_dependencies()?;
    check_process_policy()?;
    check_network_policy()?;
    check_lint_policy()
}

fn ci_full() -> Result<(), String> {
    check_pr()?;
    run_ci_full_evidence_gates(&ci_full_evidence_gates())?;
    run("cargo", &["package", "-p", "ripr", "--list"])?;
    run("cargo", &["publish", "-p", "ripr", "--dry-run"]).map(|_| ())
}

#[derive(Debug, Eq, PartialEq)]
struct CwdCommand {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
}

fn run_cwd_command(command: &CwdCommand) -> Result<(), String> {
    let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
    run_in_dir(&command.program, &args, &command.cwd).map(|_| ())
}

fn vscode_compile() -> Result<(), String> {
    run_cwd_command(&vscode_compile_command())
}

fn vscode_package() -> Result<(), String> {
    let extension_dir = vscode_extension_dir();
    let dist = extension_dir.join("dist");
    fs::create_dir_all(&dist)
        .map_err(|err| format!("failed to create {}: {err}", dist.display()))?;
    let version = vscode_package_version(&extension_dir.join("package.json"))?;
    run_cwd_command(&vscode_package_command(&version))
}

fn vscode_test() -> Result<(), String> {
    // #2437: npm test must actually run tests, not just compile.
    // Delegate to the e2e runner which builds ripr, compiles TS, and
    // invokes the mocha/test-electron suite.
    vscode_test_e2e()
}

fn vscode_test_e2e() -> Result<(), String> {
    let provided_server = std::env::var_os("RIPR_TEST_SERVER_PATH")
        .and_then(|value| (!value.is_empty()).then_some(value))
        .map(PathBuf::from);
    let (server_path, build_server) =
        select_vscode_test_server(provided_server.as_deref(), &vscode_test_server_path()?)?;
    if build_server {
        run("cargo", &["build", "-p", "ripr"])?;
    }
    let packaged_server = if build_server {
        Some(stage_vscode_test_server_archive(&server_path)?)
    } else {
        None
    };
    vscode_compile()?;
    let workspace_path = vscode_test_workspace_path()?;
    let mut envs = vec![(
        "RIPR_TEST_WORKSPACE_PATH",
        path_to_utf8(&workspace_path, "VS Code test workspace path")?,
    )];
    if provided_server.is_none() {
        let packaged = packaged_server.as_ref().ok_or_else(|| {
            "default VS Code test server was not staged through the release archive shape"
                .to_string()
        })?;
        envs.push((
            "RIPR_TEST_SERVER_PATH",
            path_to_utf8(&packaged.executable, "packaged VS Code test server path")?,
        ));
        envs.push((
            "RIPR_TEST_PACKAGED_SERVER_PATH",
            path_to_utf8(&packaged.executable, "packaged VS Code identity path")?,
        ));
        envs.push(("RIPR_TEST_PACKAGED_SERVER_SHA256", packaged.sha256.as_str()));
    }
    run_cwd_command_with_envs(&vscode_test_e2e_command(), &envs)
}

struct PackagedVscodeTestServer {
    executable: PathBuf,
    sha256: String,
}

fn stage_vscode_test_server_archive(
    server_path: &Path,
) -> Result<PackagedVscodeTestServer, String> {
    let root = std::env::current_dir()
        .map_err(|err| {
            format!("failed to resolve repository root for VS Code server proof: {err}")
        })?
        .join("target")
        .join("ripr")
        .join("vscode-server-archive");
    let package = root.join("package");
    let extracted = root.join("extracted");
    let archive = root.join("ripr-server-test.zip");
    if root.exists() {
        fs::remove_dir_all(&root)
            .map_err(|err| format!("failed to reset {}: {err}", root.display()))?;
    }
    fs::create_dir_all(&package)
        .map_err(|err| format!("failed to create {}: {err}", package.display()))?;
    let executable_name = server_path.file_name().ok_or_else(|| {
        format!(
            "VS Code test server path has no file name: {}",
            server_path.display()
        )
    })?;
    fs::copy(server_path, package.join(executable_name)).map_err(|err| {
        format!(
            "failed to stage {} for archive proof: {err}",
            server_path.display()
        )
    })?;
    create_zip_archive(&package, &archive)?;
    fs::create_dir_all(&extracted)
        .map_err(|err| format!("failed to create {}: {err}", extracted.display()))?;
    let archive_file = fs::File::open(&archive)
        .map_err(|err| format!("failed to open {}: {err}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(archive_file)
        .map_err(|err| format!("failed to read {}: {err}", archive.display()))?;
    if zip.len() != 1 {
        return Err(format!(
            "VS Code server proof archive contained {} entries, expected 1",
            zip.len()
        ));
    }
    let mut member = zip
        .by_index(0)
        .map_err(|err| format!("failed to read archive member: {err}"))?;
    if Path::new(member.name()).file_name() != Some(executable_name) {
        return Err(format!(
            "VS Code server proof archive member `{}` did not match the built executable",
            member.name()
        ));
    }
    let executable = extracted.join(executable_name);
    let mut output = fs::File::create(&executable)
        .map_err(|err| format!("failed to create {}: {err}", executable.display()))?;
    std::io::copy(&mut member, &mut output)
        .map_err(|err| format!("failed to extract {}: {err}", executable.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .map_err(|err| format!("failed to mark {} executable: {err}", executable.display()))?;
    }
    let built_sha = sha256_file(server_path)?;
    let extracted_sha = sha256_file(&executable)?;
    if built_sha != extracted_sha {
        return Err("VS Code server archive extraction changed the executable digest".to_string());
    }
    Ok(PackagedVscodeTestServer {
        executable,
        sha256: extracted_sha,
    })
}

fn select_vscode_test_server(
    provided: Option<&Path>,
    built: &Path,
) -> Result<(PathBuf, bool), String> {
    if let Some(path) = provided {
        if !path.is_file() {
            return Err(format!(
                "RIPR_TEST_SERVER_PATH does not name an installed server binary: {}",
                path.display()
            ));
        }
        return Ok((path.to_path_buf(), false));
    }
    Ok((built.to_path_buf(), true))
}

fn vscode_compile_command() -> CwdCommand {
    CwdCommand {
        program: vscode_local_bin("tsc"),
        args: vec!["-p".to_string(), "./".to_string()],
        cwd: vscode_extension_dir(),
    }
}

fn vscode_package_command(version: &str) -> CwdCommand {
    CwdCommand {
        program: vscode_local_bin("vsce"),
        args: vec![
            "package".to_string(),
            "--out".to_string(),
            format!("dist/ripr-{version}.vsix"),
        ],
        cwd: vscode_extension_dir(),
    }
}

fn vscode_test_e2e_command() -> CwdCommand {
    CwdCommand {
        program: PathBuf::from("node"),
        args: vec!["out/test/runTest.js".to_string()],
        cwd: vscode_extension_dir(),
    }
}

fn run_cwd_command_with_envs(command: &CwdCommand, envs: &[(&str, &str)]) -> Result<(), String> {
    let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
    run_in_dir_with_envs(&command.program, &args, &command.cwd, envs).map(|_| ())
}

fn vscode_test_server_path() -> Result<PathBuf, String> {
    let binary = if cfg!(windows) { "ripr.exe" } else { "ripr" };
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            repo_root()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("target")
        });
    Ok(target_dir.join("debug").join(binary))
}

fn vscode_test_workspace_path() -> Result<PathBuf, String> {
    Ok(repo_root()?
        .join("fixtures")
        .join("boundary_gap")
        .join("input"))
}

fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf).ok_or_else(|| {
        format!(
            "failed to resolve repo root from {}",
            manifest_dir.display()
        )
    })
}

fn path_to_utf8<'a>(path: &'a Path, label: &str) -> Result<&'a str, String> {
    path.to_str()
        .ok_or_else(|| format!("{label} is not valid UTF-8: {}", path.display()))
}

fn vscode_extension_dir() -> PathBuf {
    repo_root()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("editors/vscode")
}

fn vscode_local_bin(name: &str) -> PathBuf {
    let executable = if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        name.to_string()
    };
    vscode_extension_dir()
        .join("node_modules")
        .join(".bin")
        .join(executable)
}

fn vscode_package_version(package_json: &Path) -> Result<String, String> {
    let value = read_json_value(package_json)?;
    value
        .get("version")
        .and_then(Value::as_str)
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{} is missing a string version", package_json.display()))
}

fn ci_full_evidence_gates() -> [CiFullEvidenceGate; 5] {
    [
        CiFullEvidenceGate {
            name: "fixtures",
            run: ci_full_fixtures,
        },
        CiFullEvidenceGate {
            name: "goldens check",
            run: goldens_check,
        },
        CiFullEvidenceGate {
            name: "test-oracle-report",
            run: test_oracle_report,
        },
        CiFullEvidenceGate {
            name: "dogfood",
            run: dogfood,
        },
        CiFullEvidenceGate {
            name: "metrics",
            run: metrics_report,
        },
    ]
}

fn ci_full_fixtures() -> Result<(), String> {
    fixtures(None)
}

fn run_ci_full_evidence_gates(gates: &[CiFullEvidenceGate]) -> Result<(), String> {
    for gate in gates {
        (gate.run)()
            .map_err(|err| format!("ci-full evidence gate `{}` failed: {err}", gate.name))?;
    }
    Ok(())
}

fn install_hooks(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err("install-hooks does not accept arguments".to_string());
    }

    let hook = install_hooks_in(Path::new("."))?;
    eprintln!("installed hook: {}", hook.display());
    Ok(())
}

fn install_hooks_in(root: &Path) -> Result<PathBuf, String> {
    let git_dir = root.join(".git");
    if !git_dir.is_dir() {
        return Err(format!(
            "missing .git directory under {}; run from a git worktree",
            root.display()
        ));
    }

    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)
        .map_err(|err| format!("failed to create {}: {err}", hooks_dir.display()))?;

    let hook = hooks_dir.join("pre-commit");
    if hook.exists() {
        let current = fs::read_to_string(&hook)
            .map_err(|err| format!("failed to read {}: {err}", hook.display()))?;
        if !is_ripr_managed_hook(&current) {
            return Err(format!(
                "refusing to overwrite unmanaged hook at {}; remove it or install the ripr precommit hook manually",
                hook.display()
            ));
        }
    }

    fs::write(&hook, ripr_pre_commit_hook())
        .map_err(|err| format!("failed to write {}: {err}", hook.display()))?;
    make_hook_executable(&hook)?;
    Ok(hook)
}

fn ripr_pre_commit_hook() -> String {
    "#!/usr/bin/env sh\n# ripr-managed pre-commit hook\nset -eu\ncargo xtask precommit\n"
        .to_string()
}

fn is_ripr_managed_hook(text: &str) -> bool {
    text.contains("# ripr-managed pre-commit hook")
}

#[cfg(unix)]
fn make_hook_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|err| format!("failed to read {} metadata: {err}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|err| format!("failed to set executable bit on {}: {err}", path.display()))
}

#[cfg(not(unix))]
fn make_hook_executable(path: &Path) -> Result<(), String> {
    // Windows does not use the POSIX exec bit, so the hook runs there
    // regardless — but a checkout later used from WSL or Linux would
    // silently skip it (#2091). Say so at install time instead of
    // no-oping quietly.
    // Display the POSIX form: backslashes from a Windows display path are
    // escapes in a WSL/Linux shell, so the printed remedy would target the
    // wrong file (#2177 review).
    let posix_path = path.display().to_string().replace('\\', "/");
    eprintln!(
        "install-hooks: {} was written without an executable bit (unused on this platform); \
         if this checkout is later used from WSL or Linux, run `chmod +x {}`",
        posix_path, posix_path
    );
    Ok(())
}

fn shape() -> Result<(), String> {
    ensure_reports_dir()?;
    run("cargo", &["fmt"])?;
    let sorted = sort_allowlist_files()?;
    // Surface the tracked-file rewrites before the contributor commits: a
    // silent `policy/*.txt` / `.ripr/*.txt` rewrite is a surprising diff in
    // a PR that never touched policy (#2088).
    if let Some(notice) = shape_rewrite_notice(&sorted) {
        eprintln!("{notice}");
    }
    let body = shape_report_body(&sorted);
    write_report("shape.md", &body)
}

/// The stderr notice listing tracked allowlist files `shape` rewrote, or
/// `None` when nothing changed (#2088).
fn shape_rewrite_notice(sorted: &[String]) -> Option<String> {
    if sorted.is_empty() {
        return None;
    }
    let mut notice = format!(
        "shape: rewrote {} tracked allowlist file(s) (review before `git add`):",
        sorted.len()
    );
    for path in sorted {
        notice.push_str(&format!("\n  - {path}"));
    }
    Some(notice)
}

fn fix_pr() -> Result<(), String> {
    shape()?;
    pr_summary()?;
    let body = "# ripr fix-pr report\n\nStatus: pass\n\nActions:\n\n- Ran `cargo xtask shape`.\n- Ran `cargo xtask pr-summary`.\n\nReports:\n\n- `target/ripr/reports/shape.md`\n- `target/ripr/reports/pr-summary.md`\n\nNext commands:\n\n```bash\ncargo xtask check-pr\n```\n";
    write_report("fix-pr.md", body)
}

fn commands_report() -> Result<(), String> {
    let entries = command_catalog();
    write_report("commands.md", &commands_report_markdown(&entries))?;
    write_report("commands.json", &commands_report_json(&entries))
}

fn pr_ready() -> Result<(), String> {
    let steps = vec![
        run_readiness_step(
            "worktree_doctor",
            "cargo xtask worktree doctor",
            "target/ripr/reports/worktree-doctor.md",
            true,
            worktree_doctor,
        ),
        run_readiness_step(
            "command_mutability_catalog",
            "cargo xtask commands",
            "target/ripr/reports/commands.md",
            false,
            commands_report,
        ),
        run_readiness_step(
            "pr_summary",
            "cargo xtask pr-summary",
            "target/ripr/reports/pr-summary.md",
            false,
            pr_summary,
        ),
        run_readiness_step(
            "critic",
            "cargo xtask critic",
            "target/ripr/reports/critic.md",
            false,
            critic_impl,
        ),
        run_readiness_step(
            "receipts_check",
            "cargo xtask receipts check",
            "target/ripr/reports/receipts.md",
            false,
            receipts_check,
        ),
        run_readiness_step(
            "suggested_fixes",
            "cargo xtask suggested-fixes",
            "target/ripr/reports/suggested-fixes.md",
            false,
            suggested_fixes,
        ),
        run_readiness_step(
            "generated_clean",
            "cargo xtask check-generated-clean",
            "target/ripr/reports/generated-clean.md",
            true,
            check_generated_clean,
        ),
        run_readiness_step(
            "badge_diff_policy",
            "cargo xtask check-badge-diff-policy",
            "target/ripr/reports/badge-diff-policy.md",
            true,
            check_badge_diff_policy,
        ),
    ];
    let status = pr_ready_status(&steps);
    write_report("pr-ready.md", &pr_ready_markdown(&steps))?;
    write_report("pr-ready.json", &pr_ready_json(&steps))?;
    let index_result = reports_index();

    if status == "fail" {
        let _ = index_result;
        Err(
            "pr-ready found blocking repo-ops issues; see target/ripr/reports/pr-ready.md"
                .to_string(),
        )
    } else {
        index_result
    }
}

fn cockpit() -> Result<(), String> {
    let steps = vec![
        run_readiness_step(
            "worktree_doctor",
            "cargo xtask worktree doctor",
            "target/ripr/reports/worktree-doctor.md",
            true,
            worktree_doctor,
        ),
        run_readiness_step(
            "command_mutability_catalog",
            "cargo xtask commands",
            "target/ripr/reports/commands.md",
            false,
            commands_report,
        ),
        run_readiness_step(
            "command_catalog_check",
            "cargo xtask check-command-catalog",
            "target/ripr/reports/command-catalog.md",
            true,
            check_command_catalog,
        ),
        run_readiness_step(
            "spec_numbering",
            "cargo xtask check-spec-numbering",
            "target/ripr/reports/spec-numbering.md",
            true,
            check_spec_numbering,
        ),
        run_readiness_step(
            "pr_triage",
            "cargo xtask pr-triage-report",
            "target/ripr/reports/pr-triage.md",
            false,
            reports::pr_triage_report,
        ),
        run_readiness_step(
            "generated_clean",
            "cargo xtask check-generated-clean",
            "target/ripr/reports/generated-clean.md",
            true,
            check_generated_clean,
        ),
        run_readiness_step(
            "badge_diff_policy",
            "cargo xtask check-badge-diff-policy",
            "target/ripr/reports/badge-diff-policy.md",
            true,
            check_badge_diff_policy,
        ),
    ];

    write_report("cockpit.md", &cockpit_markdown(&steps))?;
    write_report("cockpit.json", &cockpit_json(&steps))?;
    reports_index()?;

    if steps
        .iter()
        .any(|step| step.required && step.status == "fail")
    {
        Err("repo cockpit found blocking repo-ops issues".to_string())
    } else {
        Ok(())
    }
}

fn check_command_catalog() -> Result<(), String> {
    let commands = known_commands();
    let catalog = command_catalog();
    let mut violations = command_catalog_violations(&commands, &catalog);
    violations.extend(command_catalog_ci_drift_violations_for_repo(&catalog)?);

    finish_policy_report(
        PolicyReportSpec {
            report_file: "command-catalog.md",
            check: "check-command-catalog",
            why_it_matters: "The command mutability catalog is the repo-ops map for agents. Every xtask command must stay classified so workers know what is safe to run, what writes generated evidence, what requires judgment, and which checks a CI workflow enforces.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Add a command catalog entry for every new xtask command.",
                "Remove catalog entries for commands that no longer exist.",
                "Document writes for mutating, external-state, and argument-dependent commands.",
                "Mark external-state mutations as judgment-required.",
                "Keep ci_enforced aligned with the commands CI workflows invoke without advisory shielding.",
            ],
            rerun_command: "cargo xtask check-command-catalog",
            exception_template: None,
        },
        &violations,
    )
}

fn command_catalog_violations(
    commands: &[&'static str],
    catalog: &[CommandCatalogEntry],
) -> Vec<String> {
    let known_roots = command_roots(commands.iter().copied());
    let catalog_roots = command_roots(catalog.iter().map(|entry| entry.command));
    let mut violations = Vec::new();

    for root in known_roots.difference(&catalog_roots) {
        violations.push(format!(
            "command `{root}` is listed in help but missing from the command mutability catalog"
        ));
    }
    for root in catalog_roots.difference(&known_roots) {
        violations.push(format!(
            "command catalog entry `{root}` does not match any known xtask command"
        ));
    }
    if let Some(order_violation) = command_catalog_order_violation(commands, catalog) {
        violations.push(order_violation);
    }

    let mut seen = BTreeSet::<&str>::new();
    for entry in catalog {
        if !seen.insert(entry.command) {
            violations.push(format!(
                "command catalog has duplicate entry `{}`",
                entry.command
            ));
        }
        if !is_command_mutability(entry.mutability) {
            violations.push(format!(
                "command `{}` uses unknown mutability `{}`",
                entry.command, entry.mutability
            ));
        }
        if entry.writes.trim().is_empty() {
            violations.push(format!("command `{}` must document writes", entry.command));
        }
        if entry.mutability == "external_state_mutating" && !entry.judgment_required {
            violations.push(format!(
                "external-state mutating command `{}` must be judgment-required",
                entry.command
            ));
        }
        if entry.mutability == "argument_dependent" {
            let notes = entry.notes.to_ascii_lowercase();
            if !(notes.contains("depending")
                || notes.contains("--check")
                || notes.contains("--propose")
                || notes.contains("default"))
            {
                violations.push(format!(
                    "argument-dependent command `{}` must explain when it writes",
                    entry.command
                ));
            }
        }
    }

    violations
}

fn command_catalog_order_violation(
    commands: &[&'static str],
    catalog: &[CommandCatalogEntry],
) -> Option<String> {
    let mut order = BTreeMap::new();
    for (index, command) in commands.iter().enumerate() {
        order.insert(*command, index);
    }

    let mut last = None;
    for entry in catalog {
        let Some(index) = order.get(entry.command).copied() else {
            continue;
        };
        if last.is_some_and(|last| index < last) {
            return Some(
                "command catalog entries must follow the xtask help catalog order".to_string(),
            );
        }
        last = Some(index);
    }
    None
}

fn command_roots<'a>(commands: impl Iterator<Item = &'a str>) -> BTreeSet<&'a str> {
    commands.map(known_command_root).collect()
}

fn is_command_mutability(value: &str) -> bool {
    matches!(
        value,
        "mutating"
            | "non_mutating_check"
            | "report_only"
            | "external_state_read"
            | "external_state_mutating"
            | "argument_dependent"
    )
}

/// One `cargo xtask` invocation extracted from a workflow run block:
/// the command root and its bare subcommand (empty when none).
type WorkflowXtaskInvocation = (String, String);

/// Scans one workflow file and returns the xtask invocations CI enforces:
/// run-block commands without `|| true` shielding and outside any job or step
/// marked `continue-on-error: true`. Line-scanning on purpose: check-workflows
/// reads workflows the same way and the repo does not take a YAML dependency.
fn ci_enforced_xtask_invocations(workflow: &str) -> BTreeSet<WorkflowXtaskInvocation> {
    let mut enforced = BTreeSet::new();
    let mut job_continue_on_error = false;
    let mut step_continue_on_error = false;
    let mut pending: Vec<WorkflowXtaskInvocation> = Vec::new();
    let mut run_block_indent: Option<usize> = None;

    for line in workflow.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        if let Some(run_indent) = run_block_indent {
            if trimmed.is_empty() || indent > run_indent {
                if let Some(invocation) = workflow_run_xtask_invocation(trimmed) {
                    pending.push(invocation);
                }
                continue;
            }
            run_block_indent = None;
        }

        if indent == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-') {
            flush_workflow_step_invocations(
                &mut pending,
                &mut enforced,
                job_continue_on_error || step_continue_on_error,
            );
            job_continue_on_error = false;
            step_continue_on_error = false;
            continue;
        }
        if indent == 4 && trimmed == "continue-on-error: true" {
            job_continue_on_error = true;
            continue;
        }
        if indent == 6 && trimmed.starts_with("- ") {
            flush_workflow_step_invocations(
                &mut pending,
                &mut enforced,
                job_continue_on_error || step_continue_on_error,
            );
            step_continue_on_error = false;
            if let Some(inline) = trimmed.strip_prefix("- run:") {
                let inline = inline.trim();
                if inline.starts_with('|') || inline.starts_with('>') {
                    run_block_indent = Some(indent);
                } else if let Some(invocation) = workflow_run_xtask_invocation(inline) {
                    pending.push(invocation);
                }
            }
            continue;
        }
        if indent == 8 {
            if trimmed == "continue-on-error: true" {
                step_continue_on_error = true;
                continue;
            }
            if let Some(inline) = trimmed.strip_prefix("run:") {
                let inline = inline.trim();
                if inline.starts_with('|') || inline.starts_with('>') {
                    run_block_indent = Some(indent);
                } else if let Some(invocation) = workflow_run_xtask_invocation(inline) {
                    pending.push(invocation);
                }
            }
        }
    }
    flush_workflow_step_invocations(
        &mut pending,
        &mut enforced,
        job_continue_on_error || step_continue_on_error,
    );
    enforced
}

/// Moves the current step's invocations into the enforced set unless the
/// enclosing job or step is advisory (`continue-on-error: true`).
fn flush_workflow_step_invocations(
    pending: &mut Vec<WorkflowXtaskInvocation>,
    enforced: &mut BTreeSet<WorkflowXtaskInvocation>,
    advisory: bool,
) {
    if !advisory {
        enforced.extend(pending.drain(..));
    } else {
        pending.clear();
    }
}

/// Extracts the xtask command invoked by one run-block line, if the line
/// invokes `cargo xtask` directly. Advisory `|| true` shielding is handled by
/// the caller's step scan, so this only parses the command shape.
fn workflow_run_xtask_invocation(line: &str) -> Option<WorkflowXtaskInvocation> {
    if line.contains("|| true") {
        return None;
    }
    let rest = line.strip_prefix("cargo xtask ")?;
    let mut tokens = rest.split_whitespace();
    let root = tokens.next()?.trim_matches('"');
    if root.is_empty() || root.starts_with('$') {
        return None;
    }
    let subcommand = tokens
        .next()
        .filter(|token| {
            token.starts_with(|c: char| c.is_ascii_lowercase())
                && token.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        })
        .unwrap_or_default();
    Some((root.to_string(), subcommand.to_string()))
}

fn catalog_command_matches_ci_invocation(
    command: &str,
    (root, subcommand): &WorkflowXtaskInvocation,
) -> bool {
    let mut words = command.split_whitespace();
    if words.next() != Some(root.as_str()) {
        return false;
    }
    subcommand.is_empty() || words.next() == Some(subcommand.as_str())
}

fn command_catalog_ci_drift_violations(
    catalog: &[CommandCatalogEntry],
    enforced: &BTreeSet<WorkflowXtaskInvocation>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for entry in catalog {
        let expected = enforced
            .iter()
            .any(|invocation| catalog_command_matches_ci_invocation(entry.command, invocation));
        if expected && !entry.ci_enforced {
            violations.push(format!(
                "command `{}` is invoked by a CI workflow without advisory shielding but the catalog marks ci_enforced=false",
                entry.command
            ));
        }
        if !expected && entry.ci_enforced {
            violations.push(format!(
                "command `{}` is marked ci_enforced=true but no CI workflow invokes it in an enforced lane",
                entry.command
            ));
        }
    }
    violations
}

fn command_catalog_ci_drift_violations_for_repo(
    catalog: &[CommandCatalogEntry],
) -> Result<Vec<String>, String> {
    let mut enforced = BTreeSet::new();
    for path in collect_files(Path::new(".github/workflows"))? {
        let normalized = normalize_path(&path);
        if !(normalized.ends_with(".yml") || normalized.ends_with(".yaml")) {
            continue;
        }
        let text = read_text_lossy(&path)?;
        enforced.extend(ci_enforced_xtask_invocations(&text));
    }
    expand_precommit_ci_invocations(&mut enforced);
    Ok(command_catalog_ci_drift_violations(catalog, &enforced))
}

/// An enforced `cargo xtask precommit` invocation transitively enforces every
/// gate in the precommit table (issue #2258): expand it so gates invoked only
/// through precommit still count as CI-enforced. Advisory precommit
/// invocations never reach the enforced set, so they expand nothing.
fn expand_precommit_ci_invocations(enforced: &mut BTreeSet<WorkflowXtaskInvocation>) {
    if !enforced.contains(&("precommit".to_string(), String::new())) {
        return;
    }
    for gate in PRECOMMIT_GATE_COMMANDS {
        enforced.insert(((*gate).to_string(), String::new()));
    }
}

fn commands_report_markdown(entries: &[CommandCatalogEntry]) -> String {
    let mut body = "# ripr command mutability catalog\n\n".to_string();
    body.push_str("Status: pass\n");
    body.push_str("Mode: advisory\n\n");
    body.push_str("Purpose:\n\n");
    body.push_str(
        "- distinguish commands that may edit the worktree from checks and generated reports\n",
    );
    body.push_str("- keep generated evidence separate from authored source-of-truth\n");
    body.push_str("- make judgment-required operations visible before agents run them\n");
    body.push_str(
        "- mark commands a CI workflow enforces so load-bearing gates stay distinct from advisory checks\n\n",
    );
    body.push_str("Boundaries:\n\n");
    body.push_str("- `check-pr` is non-mutating for tracked files\n");
    body.push_str("- `target/ripr/**` outputs are generated evidence\n");
    body.push_str("- judgment-required commands need explicit review before use\n");
    body.push_str(
        "- CI enforced means a CI workflow fails when the command fails; unmarked commands are advisory or local-only\n\n",
    );
    body.push_str("| Command | Mutability | Writes | Judgment required | CI enforced | Notes |\n");
    body.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for entry in entries {
        body.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} |\n",
            markdown_cell(entry.command),
            markdown_cell(entry.mutability),
            markdown_cell(entry.writes),
            if entry.judgment_required { "yes" } else { "no" },
            if entry.ci_enforced { "yes" } else { "no" },
            markdown_cell(entry.notes)
        ));
    }
    body
}

fn commands_report_json(entries: &[CommandCatalogEntry]) -> String {
    let mut body = "{\n".to_string();
    body.push_str("  \"schema_version\": \"0.1\",\n");
    body.push_str("  \"mode\": \"advisory\",\n");
    body.push_str("  \"commands\": [\n");
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str("    {\n");
        body.push_str(&format!(
            "      \"command\": \"{}\",\n",
            json_escape(entry.command)
        ));
        body.push_str(&format!(
            "      \"mutability\": \"{}\",\n",
            json_escape(entry.mutability)
        ));
        body.push_str(&format!(
            "      \"writes\": \"{}\",\n",
            json_escape(entry.writes)
        ));
        body.push_str(&format!(
            "      \"judgment_required\": {},\n",
            entry.judgment_required
        ));
        body.push_str(&format!("      \"ci_enforced\": {},\n", entry.ci_enforced));
        body.push_str(&format!(
            "      \"notes\": \"{}\"\n",
            json_escape(entry.notes)
        ));
        body.push_str("    }");
    }
    body.push_str("\n  ]\n");
    body.push_str("}\n");
    body
}

fn suggested_fixes() -> Result<(), String> {
    ensure_reports_dir()?;
    let (patch, files) = suggested_fixes_patch()?;
    write_report("suggested-fixes.patch", &patch)?;
    write_report(
        "suggested-fixes.md",
        &suggested_fixes_report_body(&files, patch.is_empty()),
    )
}

fn suggested_fixes_patch() -> Result<(String, Vec<String>), String> {
    let mut patch = String::new();
    let mut files = Vec::new();
    for path in deterministic_suggested_fix_allowlist_files()? {
        let original = read_text_lossy(&path)?;
        let sorted = sorted_allowlist_content(&original);
        if sorted == original {
            continue;
        }
        append_whole_file_patch(&mut patch, &path, &original, &sorted);
        files.push(normalize_path(&path));
    }
    for path in deterministic_suggested_fix_docs_index_files() {
        let original = read_text_lossy(&path)?;
        let sorted = sorted_markdown_index_table_content(&original);
        if sorted == original {
            continue;
        }
        append_whole_file_patch(&mut patch, &path, &original, &sorted);
        files.push(normalize_path(&path));
    }
    for path in deterministic_suggested_fix_traceability_files() {
        let original = read_text_lossy(&path)?;
        let sorted = sorted_traceability_behavior_blocks_content(&original);
        if sorted == original {
            continue;
        }
        append_whole_file_patch(&mut patch, &path, &original, &sorted);
        files.push(normalize_path(&path));
    }
    for path in deterministic_suggested_fix_capability_files() {
        let original = read_text_lossy(&path)?;
        let sorted = sorted_capability_blocks_content(&original);
        if sorted == original {
            continue;
        }
        append_whole_file_patch(&mut patch, &path, &original, &sorted);
        files.push(normalize_path(&path));
    }
    for path in deterministic_suggested_fix_command_catalog_files() {
        let original = read_text_lossy(&path)?;
        let sorted = sorted_command_catalog_content(&original);
        if sorted == original {
            continue;
        }
        append_whole_file_patch(&mut patch, &path, &original, &sorted);
        files.push(normalize_path(&path));
    }
    files.sort();
    Ok((patch, files))
}

fn deterministic_suggested_fix_allowlist_files() -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for root in [Path::new(".ripr"), Path::new("policy")] {
        if !root.exists() {
            continue;
        }
        for path in collect_files(root)? {
            if path.extension().and_then(|value| value.to_str()) == Some("txt") {
                paths.push(path);
            }
        }
    }
    paths.sort_by_key(|path| normalize_path(path));
    Ok(paths)
}

fn deterministic_suggested_fix_docs_index_files() -> Vec<PathBuf> {
    [
        PathBuf::from("docs/adr/README.md"),
        PathBuf::from("docs/specs/README.md"),
    ]
    .into_iter()
    .filter(|path| path.exists())
    .collect()
}

fn deterministic_suggested_fix_traceability_files() -> Vec<PathBuf> {
    [PathBuf::from(".ripr/traceability.toml")]
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

fn deterministic_suggested_fix_capability_files() -> Vec<PathBuf> {
    [PathBuf::from("metrics/capabilities.toml")]
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

fn deterministic_suggested_fix_command_catalog_files() -> Vec<PathBuf> {
    [PathBuf::from("xtask/src/command.rs")]
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

fn sorted_markdown_index_table_content(text: &str) -> String {
    let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    let Some(header_index) = lines
        .iter()
        .position(|line| is_markdown_index_table_header(line))
    else {
        return text.to_string();
    };
    let Some(separator) = lines.get(header_index + 1) else {
        return text.to_string();
    };
    if !is_markdown_index_table_separator(separator) {
        return text.to_string();
    }

    let rows_start = header_index + 2;
    let mut rows_end = rows_start;
    while rows_end < lines.len() && is_markdown_table_row(&lines[rows_end]) {
        rows_end += 1;
    }
    if rows_end <= rows_start {
        return text.to_string();
    }

    let mut sorted_rows = lines[rows_start..rows_end].to_vec();
    sorted_rows.sort_by_key(|line| line.to_ascii_lowercase());
    if sorted_rows == lines[rows_start..rows_end] {
        return text.to_string();
    }

    lines.splice(rows_start..rows_end, sorted_rows);
    let mut output = lines.join("\n");
    if text.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn is_markdown_index_table_header(line: &str) -> bool {
    matches!(
        line.trim(),
        "| ADR | Status | Decision |" | "| Spec | Status | Topic |"
    )
}

fn is_markdown_index_table_separator(line: &str) -> bool {
    line.trim() == "| --- | --- | --- |"
}

fn is_markdown_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|')
}

fn sorted_traceability_behavior_blocks_content(text: &str) -> String {
    let mut block_starts = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim() == "[[behavior]]" {
            block_starts.push(offset);
        }
        offset += line.len();
    }
    if block_starts.len() <= 1 {
        return text.to_string();
    }

    let prefix = &text[..block_starts[0]];
    let mut ids = BTreeSet::new();
    let mut blocks = Vec::new();
    for (index, start) in block_starts.iter().copied().enumerate() {
        let end = block_starts.get(index + 1).copied().unwrap_or(text.len());
        let block = &text[start..end];
        if toml_array_table_block_is_unsafe_to_sort(block, "[[behavior]]") {
            return text.to_string();
        }
        let Some(id) = traceability_behavior_block_id(block) else {
            return text.to_string();
        };
        if !is_spec_id(&id) || !ids.insert(id.clone()) {
            return text.to_string();
        }
        blocks.push((id, block));
    }

    let original_order = blocks.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    blocks.sort_by(|left, right| left.0.cmp(&right.0));
    let sorted_order = blocks.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    if original_order == sorted_order {
        return text.to_string();
    }

    let mut output = prefix.to_string();
    for (_, block) in blocks {
        output.push_str(block);
    }
    output
}

fn sorted_capability_blocks_content(text: &str) -> String {
    let mut block_starts = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim() == "[[capability]]" {
            block_starts.push(offset);
        }
        offset += line.len();
    }
    if block_starts.len() <= 1 {
        return text.to_string();
    }

    let (capabilities, parse_violations) =
        parse_capabilities_manifest_text("metrics/capabilities.toml", text);
    if !parse_violations.is_empty() || capabilities.len() != block_starts.len() {
        return text.to_string();
    }

    let prefix = &text[..block_starts[0]];
    let mut ids = BTreeSet::new();
    let mut blocks = Vec::new();
    for ((index, start), capability) in block_starts
        .iter()
        .copied()
        .enumerate()
        .zip(capabilities.iter())
    {
        let end = block_starts.get(index + 1).copied().unwrap_or(text.len());
        let block = &text[start..end];
        if toml_array_table_block_is_unsafe_to_sort(block, "[[capability]]") {
            return text.to_string();
        }
        let Some((spec, id)) = capability_sort_key(capability) else {
            return text.to_string();
        };
        if !ids.insert(id.clone()) {
            return text.to_string();
        }
        blocks.push(((spec, id), block));
    }

    let original_order = blocks
        .iter()
        .map(|((spec, id), _)| (spec.clone(), id.clone()))
        .collect::<Vec<_>>();
    blocks.sort_by(|left, right| left.0.cmp(&right.0));
    let sorted_order = blocks
        .iter()
        .map(|((spec, id), _)| (spec.clone(), id.clone()))
        .collect::<Vec<_>>();
    if original_order == sorted_order {
        return text.to_string();
    }

    let mut output = prefix.to_string();
    for (_, block) in blocks {
        output.push_str(block);
    }
    let (sorted_capabilities, sorted_violations) =
        parse_capabilities_manifest_text("metrics/capabilities.toml", &output);
    let Some(sorted_capability_order) = capability_sort_keys(&sorted_capabilities) else {
        return text.to_string();
    };
    if !sorted_violations.is_empty()
        || sorted_capabilities.len() != block_starts.len()
        || sorted_capability_order != sorted_order
    {
        return text.to_string();
    }
    output
}

fn sorted_command_catalog_content(text: &str) -> String {
    let start_marker = "pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {\n    vec![\n";
    let Some(start) = text.find(start_marker) else {
        return text.to_string();
    };
    let entries_start = start + start_marker.len();
    let Some(end_relative) = text[entries_start..].find("\n    ]\n}") else {
        return text.to_string();
    };
    let entries_end = entries_start + end_relative;
    let body = &text[entries_start..entries_end];
    let Some(sorted_body) = sorted_command_catalog_entry_blocks(body) else {
        return text.to_string();
    };
    if sorted_body == body {
        return text.to_string();
    }

    let mut output = String::with_capacity(text.len());
    output.push_str(&text[..entries_start]);
    output.push_str(&sorted_body);
    output.push_str(&text[entries_end..]);
    output
}

fn sorted_command_catalog_entry_blocks(body: &str) -> Option<String> {
    let mut order = BTreeMap::new();
    for (index, command) in known_commands().iter().enumerate() {
        order.insert(command.to_string(), index);
    }

    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_block = false;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim();
        if !in_block {
            if trimmed.is_empty() {
                current.push_str(line);
                continue;
            }
            if trimmed != "command_entry(" {
                return None;
            }
            in_block = true;
            current.push_str(line);
            continue;
        }

        current.push_str(line);
        if trimmed == ")," {
            let command = command_catalog_entry_block_command(&current)?;
            let index = *order.get(&command)?;
            blocks.push((index, blocks.len(), std::mem::take(&mut current)));
            in_block = false;
        }
    }
    if in_block || !current.trim().is_empty() || blocks.len() <= 1 {
        return None;
    }

    let original_order = blocks
        .iter()
        .map(|(order, original_index, _)| (*order, *original_index))
        .collect::<Vec<_>>();
    blocks.sort_by_key(|(order, original_index, _)| (*order, *original_index));
    let sorted_order = blocks
        .iter()
        .map(|(order, original_index, _)| (*order, *original_index))
        .collect::<Vec<_>>();
    if original_order == sorted_order {
        return Some(body.to_string());
    }

    Some(blocks.into_iter().map(|(_, _, block)| block).collect())
}

fn command_catalog_entry_block_command(block: &str) -> Option<String> {
    for line in block.lines().skip(1) {
        let trimmed = line.trim().trim_end_matches(',').trim();
        if !trimmed.starts_with('"') {
            continue;
        }
        return parse_quoted_value(trimmed).ok();
    }
    None
}

fn capability_sort_keys(capabilities: &[Capability]) -> Option<Vec<(String, String)>> {
    capabilities.iter().map(capability_sort_key).collect()
}

fn capability_sort_key(capability: &Capability) -> Option<(String, String)> {
    let id = capability.id.as_ref()?;
    let spec = capability.spec.as_ref()?;
    if !is_snake_case_id(id) || !is_spec_id(spec) {
        return None;
    }
    Some((spec.clone(), id.clone()))
}

fn toml_array_table_block_is_unsafe_to_sort(block: &str, table_header: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut active_array = false;
    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == table_header {
            continue;
        }
        if active_array {
            if trimmed.starts_with(']') {
                active_array = false;
            } else if parse_array_item(trimmed).is_err() {
                return true;
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !seen.insert(key.to_string()) {
            return true;
        }
        let value = value.trim();
        if value == "[" {
            active_array = true;
        } else if value.starts_with('[') && parse_inline_array(value).is_err() {
            return true;
        }
    }
    active_array
}

fn traceability_behavior_block_id(block: &str) -> Option<String> {
    for line in block.lines() {
        let trimmed = line.trim();
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "id" {
            continue;
        }
        return parse_quoted_value(value).ok();
    }
    None
}

fn append_whole_file_patch(body: &mut String, path: &Path, before: &str, after: &str) {
    let normalized = normalize_path(path);
    let before_count = patch_line_count(before);
    let after_count = patch_line_count(after);
    body.push_str(&format!("diff --git a/{normalized} b/{normalized}\n"));
    body.push_str(&format!("--- a/{normalized}\n"));
    body.push_str(&format!("+++ b/{normalized}\n"));
    body.push_str(&format!(
        "@@ {} {} @@\n",
        patch_hunk_range(false, before_count),
        patch_hunk_range(true, after_count)
    ));
    for line in before.lines() {
        body.push('-');
        body.push_str(line);
        body.push('\n');
    }
    for line in after.lines() {
        body.push('+');
        body.push_str(line);
        body.push('\n');
    }
}

fn patch_line_count(text: &str) -> usize {
    text.lines().count()
}

fn patch_hunk_range(addition: bool, count: usize) -> String {
    let sign = if addition { '+' } else { '-' };
    if count == 0 {
        format!("{sign}0,0")
    } else {
        format!("{sign}1,{count}")
    }
}

fn suggested_fixes_report_body(files: &[String], patch_empty: bool) -> String {
    let mut body = "# ripr suggested fixes\n\nStatus: pass\n\n".to_string();
    body.push_str("Patch: `target/ripr/reports/suggested-fixes.patch`\n\n");
    body.push_str("Scope:\n\n");
    body.push_str("- deterministic allowlist ordering under `.ripr/*.txt` and `policy/*.txt`\n");
    body.push_str("- deterministic docs index table ordering for specs and ADRs\n");
    body.push_str("- deterministic traceability behavior block ordering by spec id\n");
    body.push_str("- deterministic capability block ordering by spec ID and capability ID\n");
    body.push_str("- deterministic command catalog ordering by xtask help order\n");
    body.push_str("- no badge value edits\n");
    body.push_str("- no golden blessings\n");
    body.push_str("- no baselines, suppressions, dependency exceptions, or schema changes\n\n");
    if patch_empty {
        body.push_str("No deterministic patch suggestions were found.\n");
    } else {
        body.push_str("Suggested patch files:\n\n");
        for file in files {
            body.push_str(&format!("- `{file}`\n"));
        }
    }
    body
}

pub(crate) fn pr_summary_impl() -> Result<(), String> {
    let changes = collect_pr_changes()?;
    let mut body = pr_summary_body(&changes);
    // Advisory proof-route section; computation failure degrades to an
    // in-section note and never fails pr-summary.
    body.push_str(&reports::pr_summary_proof_route_section());
    write_report("pr-summary.md", &body)
}

fn check_pr_shape() -> Result<(), String> {
    let changes = collect_pr_changes()?;
    let warnings = pr_shape_warnings(&changes);
    write_report("pr-shape.md", &pr_shape_report_body(&warnings))
}

pub(crate) fn pr_triage_report_impl() -> Result<(), String> {
    let prs = collect_open_prs_for_triage()?;
    let today = current_epoch_day()?;
    let generated_at = generated_at_unix_ms()?;
    let findings = pr_triage_findings(&prs, today);
    write_report("pr-triage.md", &pr_triage_markdown(&prs, &findings, today))?;
    write_report(
        "pr-triage.json",
        &pr_triage_json(&prs, &findings, today, &generated_at),
    )
}

pub(crate) fn gh_pr_status_impl(args: &[String]) -> Result<(), String> {
    let number = parse_gh_pr_status_args(args)?;
    let pr = collect_gh_pr_status(number)?;
    let mut warnings = Vec::new();
    let (required_contexts, required_contexts_available) =
        match collect_required_status_contexts(&pr.base_ref_name) {
            Ok(contexts) => (contexts, true),
            Err(err) => {
                warnings.push(format!(
                    "required status context lookup failed; using status rollup only: {err}"
                ));
                (Vec::new(), false)
            }
        };
    let readiness = gh_pr_status_readiness(
        &pr,
        &required_contexts,
        required_contexts_available,
        warnings,
    );
    let body = gh_pr_status_markdown(&pr, &required_contexts, &readiness);
    write_report("gh-pr-status.md", &body)?;
    write_report(
        "gh-pr-status.json",
        &gh_pr_status_json(&pr, &required_contexts, &readiness),
    )?;
    println!(
        "PR #{} safe next action: {}",
        pr.number, readiness.safe_next_action
    );
    Ok(())
}

fn parse_gh_pr_status_args(args: &[String]) -> Result<u64, String> {
    if args.is_empty() {
        return Err("cargo xtask gh-pr-status requires `--pr <number>`".to_string());
    }
    let mut pr_number = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--pr" {
            let Some(value) = args.get(index + 1) else {
                return Err("cargo xtask gh-pr-status requires a number after `--pr`".to_string());
            };
            pr_number = Some(parse_positive_u64(value, "--pr")?);
            index += 2;
            continue;
        }
        if pr_number.is_none() && !arg.starts_with('-') {
            pr_number = Some(parse_positive_u64(arg, "PR number")?);
            index += 1;
            continue;
        }
        return Err(format!(
            "unknown gh-pr-status argument `{arg}`; use `cargo xtask gh-pr-status --pr <number>`"
        ));
    }
    pr_number.ok_or_else(|| "cargo xtask gh-pr-status requires `--pr <number>`".to_string())
}

fn parse_positive_u64(value: &str, label: &str) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|err| format!("{label} must be a positive integer: {err}"))?;
    if parsed == 0 {
        return Err(format!("{label} must be greater than zero"));
    }
    Ok(parsed)
}

fn collect_gh_pr_status(number: u64) -> Result<GhPrStatusPullRequest, String> {
    let fields = [
        "number",
        "title",
        "isDraft",
        "mergeStateStatus",
        "headRefName",
        "baseRefName",
        "reviewDecision",
        "latestReviews",
        "statusCheckRollup",
    ]
    .join(",");
    let output = run_output_owned(
        "gh",
        &[
            "pr".to_string(),
            "view".to_string(),
            number.to_string(),
            "--json".to_string(),
            fields,
        ],
    )?;
    parse_gh_pr_status_pull_request(&output)
}

fn collect_required_status_contexts(base_ref_name: &str) -> Result<Vec<String>, String> {
    let repo_output = run_output("gh", &["repo", "view", "--json", "owner,name"])?;
    let repo_value: Value = serde_json::from_str(&repo_output)
        .map_err(|err| format!("failed to parse gh repo JSON: {err}"))?;
    let owner = repo_value
        .get("owner")
        .and_then(|owner| owner.get("login"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("gh repo JSON is missing owner.login: {repo_value}"))?;
    let name = repo_value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("gh repo JSON is missing name: {repo_value}"))?;
    let branch = if base_ref_name.trim().is_empty() {
        "main"
    } else {
        base_ref_name.trim()
    };
    let endpoint = format!(
        "repos/{owner}/{name}/branches/{branch}/protection/required_status_checks/contexts"
    );
    let output = run_output_owned("gh", &["api".to_string(), endpoint])?;
    parse_required_status_contexts(&output)
}

fn parse_gh_pr_status_pull_request(text: &str) -> Result<GhPrStatusPullRequest, String> {
    let item: Value =
        serde_json::from_str(text).map_err(|err| format!("failed to parse gh PR JSON: {err}"))?;
    let number = item
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("gh PR JSON is missing numeric `number`: {item}"))?;
    let checks = item
        .get("statusCheckRollup")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(parse_pr_triage_check)
        .collect::<Vec<_>>();
    let reviews = item
        .get("latestReviews")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(parse_gh_pr_status_review)
        .collect::<Vec<_>>();
    Ok(GhPrStatusPullRequest {
        number,
        title: json_value_string(&item, "title"),
        is_draft: item
            .get("isDraft")
            .and_then(Value::as_bool)
            .is_some_and(|value| value),
        merge_state_status: json_value_string(&item, "mergeStateStatus"),
        head_ref_name: json_value_string(&item, "headRefName"),
        base_ref_name: json_value_string(&item, "baseRefName"),
        review_decision: json_value_string(&item, "reviewDecision"),
        checks,
        reviews,
    })
}

fn parse_gh_pr_status_review(value: &Value) -> GhPrStatusReview {
    let author = match value
        .get("author")
        .and_then(|author| author.get("login"))
        .and_then(Value::as_str)
    {
        Some(login) => login.to_string(),
        None => String::new(),
    };
    GhPrStatusReview {
        author,
        state: json_value_string(value, "state"),
    }
}

fn parse_required_status_contexts(text: &str) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|err| format!("failed to parse required status contexts JSON: {err}"))?;
    let Some(items) = value.as_array() else {
        return Err("required status contexts JSON must be an array".to_string());
    };
    let mut contexts = items
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    contexts.sort();
    contexts.dedup();
    Ok(contexts)
}

fn gh_pr_status_readiness(
    pr: &GhPrStatusPullRequest,
    required_contexts: &[String],
    required_contexts_available: bool,
    warnings: Vec<String>,
) -> GhPrStatusReadiness {
    let failed_checks = pr
        .checks
        .iter()
        .filter(|check| pr_check_failed(check))
        .map(format_gh_pr_check_status)
        .collect::<Vec<_>>();
    let pending_checks = pr
        .checks
        .iter()
        .filter(|check| pr_check_pending(check))
        .map(format_gh_pr_check_status)
        .collect::<Vec<_>>();
    let required_checks_outstanding = pr
        .checks
        .iter()
        .filter(|check| pr_check_pending(check))
        .filter(|check| {
            !required_contexts_available
                || required_contexts
                    .iter()
                    .any(|context| context.eq_ignore_ascii_case(&check.name))
        })
        .map(format_gh_pr_check_status)
        .collect::<Vec<_>>();
    let droid_checks = pr
        .checks
        .iter()
        .filter(|check| check.name.to_ascii_lowercase().contains("droid"))
        .map(format_gh_pr_check_status)
        .collect::<Vec<_>>();
    let behind_main = pr.merge_state_status.eq_ignore_ascii_case("BEHIND");
    let mut readiness = GhPrStatusReadiness {
        behind_main,
        required_contexts_available,
        required_checks_outstanding,
        failed_checks,
        pending_checks,
        droid_checks,
        safe_next_action: String::new(),
        warnings,
    };
    readiness.safe_next_action = gh_pr_safe_next_action(pr, &readiness).to_string();
    readiness
}

fn gh_pr_safe_next_action(
    pr: &GhPrStatusPullRequest,
    readiness: &GhPrStatusReadiness,
) -> &'static str {
    if !readiness.failed_checks.is_empty()
        || pr.review_decision.eq_ignore_ascii_case("CHANGES_REQUESTED")
    {
        return "inspect failure";
    }
    if readiness.behind_main {
        return "rebase";
    }
    if pr.is_draft
        || pr.review_decision.eq_ignore_ascii_case("REVIEW_REQUIRED")
        || !readiness.required_checks_outstanding.is_empty()
        || !readiness.pending_checks.is_empty()
    {
        return "wait";
    }
    let merge_state = pr.merge_state_status.to_ascii_uppercase();
    match merge_state.as_str() {
        "CLEAN" => "merge",
        "BLOCKED" | "DRAFT" | "HAS_HOOKS" | "UNKNOWN" | "UNSTABLE" => "wait",
        "DIRTY" => "inspect failure",
        _ => "inspect failure",
    }
}

fn format_gh_pr_check_status(check: &PrTriageCheck) -> String {
    let status = if check.status.trim().is_empty() {
        "unknown"
    } else {
        check.status.trim()
    };
    let conclusion = if check.conclusion.trim().is_empty() {
        "unknown"
    } else {
        check.conclusion.trim()
    };
    format!("{} (status={status}, conclusion={conclusion})", check.name)
}

fn gh_pr_status_markdown(
    pr: &GhPrStatusPullRequest,
    required_contexts: &[String],
    readiness: &GhPrStatusReadiness,
) -> String {
    let mut body = String::new();
    body.push_str("# GitHub PR Status\n\n");
    body.push_str(&format!("- PR: #{} {}\n", pr.number, pr.title));
    body.push_str(&format!(
        "- Branch: `{}` -> `{}`\n",
        pr.head_ref_name, pr.base_ref_name
    ));
    body.push_str(&format!(
        "- mergeable_state: `{}`\n",
        value_or_unknown(&pr.merge_state_status)
    ));
    body.push_str(&format!(
        "- review_decision: `{}`\n",
        value_or_unknown(&pr.review_decision)
    ));
    body.push_str(&format!("- draft: `{}`\n", pr.is_draft));
    body.push_str(&format!("- behind main: `{}`\n", readiness.behind_main));
    body.push_str(&format!(
        "- safe next action: `{}`\n\n",
        readiness.safe_next_action
    ));

    body.push_str("## Required Checks Outstanding\n\n");
    if readiness.required_contexts_available {
        body.push_str(&format!(
            "- Required context lookup: available ({} context(s))\n",
            required_contexts.len()
        ));
    } else {
        body.push_str("- Required context lookup: unavailable; using status rollup only\n");
    }
    append_markdown_list(&mut body, &readiness.required_checks_outstanding, "None");

    body.push_str("\n## Failed Checks\n\n");
    append_markdown_list(&mut body, &readiness.failed_checks, "None");

    body.push_str("\n## Pending Checks\n\n");
    append_markdown_list(&mut body, &readiness.pending_checks, "None");

    body.push_str("\n## Reviews\n\n");
    body.push_str(&format!(
        "- Review decision: `{}`\n",
        value_or_unknown(&pr.review_decision)
    ));
    if pr.reviews.is_empty() {
        body.push_str("- No latest reviews returned\n");
    } else {
        for review in &pr.reviews {
            body.push_str(&format!(
                "- {}: `{}`\n",
                value_or_unknown(&review.author),
                value_or_unknown(&review.state)
            ));
        }
    }

    body.push_str("\n## Droid Status\n\n");
    append_markdown_list(
        &mut body,
        &readiness.droid_checks,
        "No droid check entries returned",
    );

    body.push_str("\n## Warnings\n\n");
    append_markdown_list(&mut body, &readiness.warnings, "None");
    body
}

fn gh_pr_status_json(
    pr: &GhPrStatusPullRequest,
    required_contexts: &[String],
    readiness: &GhPrStatusReadiness,
) -> String {
    let advisory_checks = gh_pr_advisory_checks(pr, required_contexts, readiness);
    let mut body = "{\n".to_string();
    body.push_str("  \"schema_version\": \"0.1\",\n");
    body.push_str("  \"mode\": \"advisory\",\n");
    body.push_str(&format!("  \"pr_number\": {},\n", pr.number));
    body.push_str(&format!("  \"title\": \"{}\",\n", json_escape(&pr.title)));
    body.push_str("  \"branch\": {\n");
    body.push_str(&format!(
        "    \"head\": \"{}\",\n",
        json_escape(&pr.head_ref_name)
    ));
    body.push_str(&format!(
        "    \"base\": \"{}\"\n",
        json_escape(&pr.base_ref_name)
    ));
    body.push_str("  },\n");
    body.push_str(&format!(
        "  \"merge_state\": \"{}\",\n",
        json_escape(&pr.merge_state_status)
    ));
    body.push_str(&format!("  \"behind_main\": {},\n", readiness.behind_main));
    body.push_str(&format!("  \"draft\": {},\n", pr.is_draft));
    body.push_str(&format!(
        "  \"required_contexts_available\": {},\n",
        readiness.required_contexts_available
    ));
    body.push_str("  \"required_checks_outstanding\": [");
    write_json_string_array(&mut body, &readiness.required_checks_outstanding);
    body.push_str("],\n");
    body.push_str("  \"failed_checks\": [");
    write_json_string_array(&mut body, &readiness.failed_checks);
    body.push_str("],\n");
    body.push_str("  \"pending_checks\": [");
    write_json_string_array(&mut body, &readiness.pending_checks);
    body.push_str("],\n");
    body.push_str("  \"advisory_checks\": [");
    write_json_string_array(&mut body, &advisory_checks);
    body.push_str("],\n");
    body.push_str("  \"droid_status\": [");
    write_json_string_array(&mut body, &readiness.droid_checks);
    body.push_str("],\n");
    body.push_str(&format!(
        "  \"review_decision\": \"{}\",\n",
        json_escape(&pr.review_decision)
    ));
    body.push_str("  \"reviews\": [\n");
    for (index, review) in pr.reviews.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str("    {\n");
        body.push_str(&format!(
            "      \"author\": \"{}\",\n",
            json_escape(&review.author)
        ));
        body.push_str(&format!(
            "      \"state\": \"{}\"\n",
            json_escape(&review.state)
        ));
        body.push_str("    }");
    }
    body.push_str("\n  ],\n");
    body.push_str(&format!(
        "  \"safe_next_action\": \"{}\",\n",
        json_escape(&readiness.safe_next_action)
    ));
    body.push_str("  \"warnings\": [");
    write_json_string_array(&mut body, &readiness.warnings);
    body.push_str("]\n");
    body.push_str("}\n");
    body
}

fn gh_pr_advisory_checks(
    pr: &GhPrStatusPullRequest,
    required_contexts: &[String],
    readiness: &GhPrStatusReadiness,
) -> Vec<String> {
    if !readiness.required_contexts_available {
        return Vec::new();
    }
    pr.checks
        .iter()
        .filter(|check| {
            !required_contexts
                .iter()
                .any(|context| context.eq_ignore_ascii_case(&check.name))
        })
        .map(format_gh_pr_check_status)
        .collect()
}

fn value_or_unknown(value: &str) -> &str {
    if value.trim().is_empty() {
        "unknown"
    } else {
        value.trim()
    }
}

fn append_markdown_list(body: &mut String, items: &[String], empty: &str) {
    if items.is_empty() {
        body.push_str(&format!("- {empty}\n"));
        return;
    }
    for item in items {
        body.push_str(&format!("- {item}\n"));
    }
}

fn collect_open_prs_for_triage() -> Result<Vec<PrTriagePullRequest>, String> {
    let fields = [
        "number",
        "title",
        "body",
        "isDraft",
        "createdAt",
        "updatedAt",
        "mergeStateStatus",
        "headRefName",
        "baseRefName",
        "reviewDecision",
        "labels",
        "files",
        "statusCheckRollup",
    ]
    .join(",");
    let output = run_output(
        "gh",
        &[
            "pr", "list", "--limit", "100", "--state", "open", "--json", &fields,
        ],
    )?;
    parse_pr_triage_pull_requests(&output)
}

fn parse_pr_triage_pull_requests(text: &str) -> Result<Vec<PrTriagePullRequest>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|err| format!("failed to parse gh PR JSON: {err}"))?;
    let Some(items) = value.as_array() else {
        return Err("gh PR JSON must be an array".to_string());
    };
    let mut prs = Vec::new();
    for item in items {
        let number = item
            .get("number")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("gh PR JSON item is missing numeric `number`: {item}"))?;
        let title = json_value_string(item, "title");
        let body = json_value_string(item, "body");
        let mut files = item
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|file| file.get("path").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        let mut labels = item
            .get("labels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|label| {
                label
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        labels.sort();
        labels.dedup();
        let checks = item
            .get("statusCheckRollup")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(parse_pr_triage_check)
            .collect::<Vec<_>>();
        prs.push(PrTriagePullRequest {
            number,
            title,
            body,
            is_draft: item
                .get("isDraft")
                .and_then(Value::as_bool)
                .is_some_and(|value| value),
            created_at: json_value_string(item, "createdAt"),
            updated_at: json_value_string(item, "updatedAt"),
            merge_state_status: json_value_string(item, "mergeStateStatus"),
            head_ref_name: json_value_string(item, "headRefName"),
            base_ref_name: json_value_string(item, "baseRefName"),
            review_decision: json_value_string(item, "reviewDecision"),
            labels,
            files,
            checks,
        });
    }
    prs.sort_by_key(|pr| pr.number);
    Ok(prs)
}

fn parse_pr_triage_check(value: &Value) -> PrTriageCheck {
    let name = match value
        .get("name")
        .or_else(|| value.get("context"))
        .and_then(Value::as_str)
    {
        Some(name) => name.to_string(),
        None => String::new(),
    };
    let status = match value
        .get("status")
        .and_then(Value::as_str)
        .or_else(|| value.get("state").and_then(Value::as_str))
    {
        Some(status) => status.to_string(),
        None => String::new(),
    };
    let conclusion = match value
        .get("conclusion")
        .and_then(Value::as_str)
        .or_else(|| value.get("state").and_then(Value::as_str))
    {
        Some(conclusion) => conclusion.to_string(),
        None => String::new(),
    };
    PrTriageCheck {
        name,
        status,
        conclusion,
    }
}

fn json_value_string(value: &Value, key: &str) -> String {
    match value.get(key).and_then(Value::as_str) {
        Some(item) => item.to_string(),
        None => String::new(),
    }
}

fn current_epoch_day() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock is before UNIX_EPOCH: {err}"))?;
    Ok((duration.as_secs() / 86_400) as i64)
}

fn pr_triage_findings(prs: &[PrTriagePullRequest], today: i64) -> Vec<PrTriageFinding> {
    let mut findings = Vec::new();
    findings.extend(pr_triage_title_family_findings(prs));
    findings.extend(pr_triage_file_set_findings(prs));
    findings.extend(pr_triage_stale_draft_findings(prs, today));
    findings.extend(pr_triage_behind_findings(prs));
    findings.extend(pr_triage_validation_findings(prs));
    findings.extend(pr_triage_sensitive_surface_findings(prs));
    findings.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then_with(|| left.prs.cmp(&right.prs))
            .then_with(|| left.message.cmp(&right.message))
    });
    findings
}

fn pr_triage_title_family_findings(prs: &[PrTriagePullRequest]) -> Vec<PrTriageFinding> {
    let mut families = BTreeMap::<String, Vec<&PrTriagePullRequest>>::new();
    for pr in prs {
        let family = pr_title_family(&pr.title);
        if !family.is_empty() {
            families.entry(family).or_default().push(pr);
        }
    }
    families
        .into_iter()
        .filter_map(|(family, prs)| {
            if prs.len() < 2 {
                return None;
            }
            Some(PrTriageFinding {
                category: "same title family".to_string(),
                severity: "warn".to_string(),
                message: format!("{} open PRs share title family `{family}`", prs.len()),
                prs: prs.iter().map(|pr| pr.number).collect(),
                details: prs.iter().map(|pr| format_pr_triage_ref(pr)).collect(),
                recommended_action:
                    "Choose the canonical PR and close, retitle, or restack the duplicate variants."
                        .to_string(),
            })
        })
        .collect()
}

fn pr_triage_file_set_findings(prs: &[PrTriagePullRequest]) -> Vec<PrTriageFinding> {
    let mut file_sets = BTreeMap::<String, Vec<&PrTriagePullRequest>>::new();
    for pr in prs {
        if pr.files.is_empty() {
            continue;
        }
        file_sets.entry(pr.files.join("\n")).or_default().push(pr);
    }
    file_sets
        .into_iter()
        .filter_map(|(file_set, prs)| {
            if prs.len() < 2 {
                return None;
            }
            let preview = file_set
                .lines()
                .take(8)
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ");
            Some(PrTriageFinding {
                category: "same changed file set".to_string(),
                severity: "warn".to_string(),
                message: format!("{} open PRs touch the same changed file set", prs.len()),
                prs: prs.iter().map(|pr| pr.number).collect(),
                details: vec![
                    format!(
                        "PRs: {}",
                        prs.iter()
                            .map(|pr| format_pr_triage_ref(pr))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                    format!("Files: {preview}"),
                ],
                recommended_action:
                    "Pick the canonical branch before review work drifts across equivalent diffs."
                        .to_string(),
            })
        })
        .collect()
}

fn pr_triage_stale_draft_findings(prs: &[PrTriagePullRequest], today: i64) -> Vec<PrTriageFinding> {
    const STALE_DRAFT_DAYS: i64 = 7;
    prs.iter()
        .filter_map(|pr| {
            if !pr.is_draft {
                return None;
            }
            let created_day = pr_created_day(&pr.created_at)?;
            let age_days = today - created_day;
            if age_days < STALE_DRAFT_DAYS {
                return None;
            }
            Some(PrTriageFinding {
                category: "stale draft".to_string(),
                severity: "warn".to_string(),
                message: format!("#{} has been draft for {age_days} day(s)", pr.number),
                prs: vec![pr.number],
                details: vec![format_pr_triage_ref(pr)],
                recommended_action:
                    "Refresh the draft, mark it ready for review, or close it if superseded."
                        .to_string(),
            })
        })
        .collect()
}

fn pr_triage_behind_findings(prs: &[PrTriagePullRequest]) -> Vec<PrTriageFinding> {
    prs.iter()
        .filter(|pr| pr.merge_state_status.eq_ignore_ascii_case("BEHIND"))
        .map(|pr| PrTriageFinding {
            category: "behind main".to_string(),
            severity: "warn".to_string(),
            message: format!("#{} is behind {}", pr.number, pr.base_ref_name),
            prs: vec![pr.number],
            details: vec![format!(
                "{} merge_state_status={}",
                format_pr_triage_ref(pr),
                pr.merge_state_status
            )],
            recommended_action: "Update the branch before relying on CI or merge-readiness state."
                .to_string(),
        })
        .collect()
}

fn pr_triage_validation_findings(prs: &[PrTriagePullRequest]) -> Vec<PrTriageFinding> {
    let mut findings = Vec::new();
    for pr in prs {
        let mut details = Vec::new();
        let failed = pr
            .checks
            .iter()
            .filter(|check| pr_check_failed(check))
            .map(|check| format!("{}={}", check.name, check.conclusion))
            .collect::<Vec<_>>();
        if !failed.is_empty() {
            details.push(format!("failed checks: {}", failed.join(", ")));
        }
        let pending = pr
            .checks
            .iter()
            .filter(|check| pr_check_pending(check))
            .map(|check| check.name.clone())
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            details.push(format!("pending checks: {}", pending.join(", ")));
        }
        if pr.checks.is_empty() {
            details.push("no check rollup entries returned".to_string());
        }
        if let Some(body_warning) = pr_body_validation_warning(&pr.body) {
            details.push(body_warning);
        }
        if details.is_empty() {
            continue;
        }
        findings.push(PrTriageFinding {
            category: "incomplete validation".to_string(),
            severity: "warn".to_string(),
            message: format!("#{} needs validation follow-up", pr.number),
            prs: vec![pr.number],
            details,
            recommended_action: "Inspect the failing or missing validation before merge; update the PR body with the actual commands run.".to_string(),
        });
    }
    findings
}

fn pr_triage_sensitive_surface_findings(prs: &[PrTriagePullRequest]) -> Vec<PrTriageFinding> {
    let mut findings = Vec::new();
    for pr in prs {
        let mut details = Vec::new();
        for file in &pr.files {
            if let Some(reason) = pr_sensitive_file_reason(file) {
                details.push(format!("`{file}`: {reason}"));
            }
        }
        if details.is_empty() {
            continue;
        }
        findings.push(PrTriageFinding {
            category: "policy-sensitive surface".to_string(),
            severity: "warn".to_string(),
            message: format!(
                "#{} touches policy, gate, or generated workflow surfaces",
                pr.number
            ),
            prs: vec![pr.number],
            details,
            recommended_action:
                "Route review through the owning lane and check for policy authority drift."
                    .to_string(),
        });
    }
    findings
}

fn pr_triage_queue_dispositions(
    prs: &[PrTriagePullRequest],
    findings: &[PrTriageFinding],
) -> Vec<PrTriageQueueDisposition> {
    prs.iter()
        .map(|pr| pr_triage_queue_disposition(pr, findings))
        .collect()
}

fn pr_triage_queue_disposition(
    pr: &PrTriagePullRequest,
    findings: &[PrTriageFinding],
) -> PrTriageQueueDisposition {
    let (disposition, reason, recommended_action) = if pr_has_superseded_signal(pr) {
        (
            "superseded",
            "PR title, body, or labels declare it superseded",
            "Close or replace only after confirming the successor PR is current.",
        )
    } else if pr_has_duplicate_signal(pr) {
        (
            "close_duplicate",
            "PR title, body, or labels declare it duplicate",
            "Close only after confirming the canonical PR is selected.",
        )
    } else if pr_has_finding(findings, pr.number, "same title family")
        || pr_has_finding(findings, pr.number, "same changed file set")
        || pr_has_finding(findings, pr.number, "stale draft")
    {
        (
            "needs_owner_decision",
            "duplicate or stale work needs canonical owner selection",
            "Choose the canonical branch, refresh the stale draft, or close superseded variants.",
        )
    } else if pr.merge_state_status.eq_ignore_ascii_case("BEHIND") {
        (
            "needs_rebase",
            "branch is behind its base",
            "Update the branch before relying on CI or merge-readiness state.",
        )
    } else if pr_has_finding(findings, pr.number, "policy-sensitive surface") {
        (
            "do_not_touch_wrong_lane",
            "PR touches policy, generated badge, workflow, or gate-sensitive files",
            "Leave ownership to the matching lane unless explicitly assigned.",
        )
    } else if pr_has_finding(findings, pr.number, "incomplete validation")
        || pr.checks.iter().any(pr_check_failed)
        || pr.checks.iter().any(pr_check_pending)
    {
        (
            "needs_fresh_validation",
            "validation is pending, failing, absent, or not fully recorded",
            "Wait for checks, inspect failures, or update the PR body with actual commands run.",
        )
    } else if pr.is_draft || pr.review_decision.eq_ignore_ascii_case("REVIEW_REQUIRED") {
        (
            "needs_review",
            "PR is draft or still requires review",
            "Finish review before merge or further queue disposition.",
        )
    } else if pr.merge_state_status.eq_ignore_ascii_case("CLEAN") {
        (
            "merge_candidate",
            "PR is current, non-draft, and has no detected validation or policy-sensitive queue risks",
            "Review the diff and merge if the scope is still the canonical path.",
        )
    } else {
        (
            "needs_review",
            "merge state is not clean enough for an automatic queue recommendation",
            "Inspect the PR state before taking action.",
        )
    };
    PrTriageQueueDisposition {
        pr_number: pr.number,
        disposition: disposition.to_string(),
        reason: reason.to_string(),
        recommended_action: recommended_action.to_string(),
    }
}

fn pr_has_finding(findings: &[PrTriageFinding], number: u64, category: &str) -> bool {
    findings
        .iter()
        .any(|finding| finding.category == category && finding.prs.contains(&number))
}

fn pr_has_superseded_signal(pr: &PrTriagePullRequest) -> bool {
    pr_triage_title_or_label_contains(pr, "superseded")
        || pr.body.to_ascii_lowercase().contains("superseded by")
}

fn pr_has_duplicate_signal(pr: &PrTriagePullRequest) -> bool {
    pr_triage_title_or_label_contains(pr, "duplicate")
        || pr.body.to_ascii_lowercase().contains("duplicate of")
}

fn pr_triage_title_or_label_contains(pr: &PrTriagePullRequest, needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    pr.title.to_ascii_lowercase().contains(&needle)
        || pr
            .labels
            .iter()
            .any(|label| label.to_ascii_lowercase().contains(&needle))
}

fn pr_check_failed(check: &PrTriageCheck) -> bool {
    matches!(
        check.conclusion.to_ascii_uppercase().as_str(),
        "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE"
    )
}

fn pr_check_pending(check: &PrTriageCheck) -> bool {
    matches!(
        check.status.to_ascii_uppercase().as_str(),
        "PENDING" | "EXPECTED" | "IN_PROGRESS" | "QUEUED" | "WAITING" | "REQUESTED"
    )
}

fn pr_body_validation_warning(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    if !lower.contains("validation") {
        return Some("PR body has no validation section".to_string());
    }
    if lower.contains("pre-existing failure") || lower.contains("preexisting failure") {
        return Some("PR body mentions pre-existing validation failures".to_string());
    }
    if lower.contains("not run") || lower.contains("not_run") {
        return Some("PR body lists validation that was not run".to_string());
    }
    if !lower.contains("cargo xtask check-pr") {
        return Some("PR body validation does not list cargo xtask check-pr".to_string());
    }
    None
}

fn pr_sensitive_file_reason(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if path.starts_with(".github/workflows/") {
        return Some("workflow behavior can alter generated CI or branch checks");
    }
    if path.starts_with("policy/") || path.starts_with(".ripr/") || path.starts_with("docs/policy/")
    {
        return Some("policy ledger or policy documentation");
    }
    if lower.starts_with("badges/") {
        return Some("generated public badge endpoint surface");
    }
    if lower.contains("gate") || lower.contains("baseline") || lower.contains("suppression") {
        return Some("policy authority, gate, baseline, or suppression semantics");
    }
    if lower.contains("generated") && (lower.contains("ci") || lower.contains("workflow")) {
        return Some("generated CI workflow surface");
    }
    None
}

fn pr_title_family(title: &str) -> String {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            push_title_family_token(&mut tokens, &current);
            current.clear();
        }
    }
    if !current.is_empty() {
        push_title_family_token(&mut tokens, &current);
    }
    tokens.join(" ")
}

fn push_title_family_token(tokens: &mut Vec<String>, token: &str) {
    if matches!(token, "wip" | "draft" | "v2" | "v3" | "variant") {
        return;
    }
    tokens.push(token.to_string());
}

fn pr_created_day(created_at: &str) -> Option<i64> {
    let date = created_at.get(0..10)?;
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    Some(days_from_civil(year, month, day))
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let mut y = i64::from(year);
    let m = i64::from(month);
    let d = i64::from(day);
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let month_for_formula = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * month_for_formula + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn pr_triage_markdown(
    prs: &[PrTriagePullRequest],
    findings: &[PrTriageFinding],
    today: i64,
) -> String {
    let status = if findings.is_empty() { "pass" } else { "warn" };
    let dispositions = pr_triage_queue_dispositions(prs, findings);
    let disposition_by_pr = dispositions
        .iter()
        .map(|disposition| (disposition.pr_number, disposition))
        .collect::<BTreeMap<_, _>>();
    let mut body = format!("# ripr PR triage report\n\nStatus: {status}\n\n");
    body.push_str("Mode: advisory\n\n");
    body.push_str("This report summarizes open PR queue risks for agents. It does not close, merge, update, or mutate PRs.\n\n");
    body.push_str(&format!("Open PRs scanned: {}\n\n", prs.len()));

    body.push_str("## Findings\n\n");
    if findings.is_empty() {
        body.push_str("None detected.\n\n");
    } else {
        for finding in findings {
            body.push_str(&format!(
                "### {} ({})\n\n{}\n\n",
                finding.category, finding.severity, finding.message
            ));
            if !finding.prs.is_empty() {
                body.push_str("PRs:\n\n");
                for number in &finding.prs {
                    body.push_str(&format!("- #{}\n", number));
                }
                body.push('\n');
            }
            body.push_str("Details:\n\n");
            write_path_list(&mut body, &finding.details);
            body.push_str("\nRecommended action:\n\n```text\n");
            body.push_str(&finding.recommended_action);
            body.push_str("\n```\n\n");
        }
    }

    body.push_str("## Queue Disposition\n\n");
    if dispositions.is_empty() {
        body.push_str("- None detected.\n");
    } else {
        body.push_str("| PR | Disposition | Reason | Recommended action |\n");
        body.push_str("| --- | --- | --- | --- |\n");
        for disposition in &dispositions {
            body.push_str(&format!(
                "| #{} | `{}` | {} | {} |\n",
                disposition.pr_number,
                markdown_escape_table(&disposition.disposition),
                markdown_escape_table(&disposition.reason),
                markdown_escape_table(&disposition.recommended_action)
            ));
        }
    }
    body.push('\n');

    body.push_str("## Open PRs\n\n");
    if prs.is_empty() {
        body.push_str("- None detected.\n");
    } else {
        body.push_str("| PR | Disposition | Draft | Age | Merge state | Checks | Files |\n");
        body.push_str("| --- | --- | --- | ---: | --- | --- | ---: |\n");
        for pr in prs {
            let age = pr_age_label(pr, today);
            let disposition = disposition_by_pr
                .get(&pr.number)
                .map(|item| item.disposition.as_str())
                .unwrap_or("unknown");
            body.push_str(&format!(
                "| #{} {} | `{}` | {} | {} | {} | {} | {} |\n",
                pr.number,
                markdown_escape_table(&pr.title),
                markdown_escape_table(disposition),
                pr.is_draft,
                age,
                markdown_escape_table(&pr.merge_state_status),
                markdown_escape_table(&pr_checks_summary(&pr.checks)),
                pr.files.len()
            ));
        }
    }
    body.push_str("\n## Next Commands\n\n```bash\n");
    body.push_str("cargo xtask pr-triage-report\n");
    body.push_str("gh pr view <number> --json mergeStateStatus,statusCheckRollup,files\n");
    body.push_str("```\n");
    body
}

fn pr_triage_json(
    prs: &[PrTriagePullRequest],
    findings: &[PrTriageFinding],
    today: i64,
    generated_at: &str,
) -> String {
    let status = if findings.is_empty() { "pass" } else { "warn" };
    let mut body = "{\n".to_string();
    body.push_str("  \"schema_version\": \"0.1\",\n");
    body.push_str("  \"mode\": \"advisory\",\n");
    body.push_str(&format!("  \"status\": \"{}\",\n", json_escape(status)));
    body.push_str(&format!(
        "  \"generated_at\": \"{}\",\n",
        json_escape(generated_at)
    ));
    let dispositions = pr_triage_queue_dispositions(prs, findings);
    body.push_str("  \"open_prs\": [\n");
    for (index, pr) in prs.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        write_pr_triage_pull_request_json(&mut body, pr, today);
    }
    body.push_str("\n  ],\n");
    body.push_str("  \"queue_disposition\": [\n");
    for (index, disposition) in dispositions.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        write_pr_triage_queue_disposition_json(&mut body, disposition, 4);
    }
    body.push_str("\n  ],\n");
    body.push_str("  \"findings\": [\n");
    for (index, finding) in findings.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        write_pr_triage_finding_json(&mut body, finding, 4);
    }
    body.push_str("\n  ],\n");
    body.push_str("  \"recommended_actions\": [\n");
    for (index, finding) in findings.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str("    {\n");
        body.push_str(&format!(
            "      \"category\": \"{}\",\n",
            json_escape(&finding.category)
        ));
        body.push_str("      \"prs\": ");
        write_json_u64_array(&mut body, &finding.prs);
        body.push_str(",\n");
        body.push_str(&format!(
            "      \"action\": \"{}\"\n",
            json_escape(&finding.recommended_action)
        ));
        body.push_str("    }");
    }
    body.push_str("\n  ]\n");
    body.push_str("}\n");
    body
}

fn write_pr_triage_pull_request_json(body: &mut String, pr: &PrTriagePullRequest, today: i64) {
    body.push_str("    {\n");
    body.push_str(&format!("      \"number\": {},\n", pr.number));
    body.push_str(&format!(
        "      \"title\": \"{}\",\n",
        json_escape(&pr.title)
    ));
    body.push_str(&format!("      \"is_draft\": {},\n", pr.is_draft));
    body.push_str(&format!(
        "      \"created_at\": \"{}\",\n",
        json_escape(&pr.created_at)
    ));
    body.push_str(&format!(
        "      \"updated_at\": \"{}\",\n",
        json_escape(&pr.updated_at)
    ));
    body.push_str("      \"age_days\": ");
    match pr_created_day(&pr.created_at) {
        Some(created_day) => body.push_str(&(today - created_day).to_string()),
        None => body.push_str("null"),
    }
    body.push_str(",\n");
    body.push_str(&format!(
        "      \"merge_state_status\": \"{}\",\n",
        json_escape(&pr.merge_state_status)
    ));
    body.push_str(&format!(
        "      \"head_ref_name\": \"{}\",\n",
        json_escape(&pr.head_ref_name)
    ));
    body.push_str(&format!(
        "      \"base_ref_name\": \"{}\",\n",
        json_escape(&pr.base_ref_name)
    ));
    body.push_str(&format!(
        "      \"review_decision\": \"{}\",\n",
        json_escape(&pr.review_decision)
    ));
    body.push_str(&format!(
        "      \"checks_summary\": \"{}\",\n",
        json_escape(&pr_checks_summary(&pr.checks))
    ));
    body.push_str("      \"labels\": [");
    write_json_string_array(body, &pr.labels);
    body.push_str("],\n      \"files\": [");
    write_json_string_array(body, &pr.files);
    body.push_str("],\n      \"checks\": [\n");
    for (index, check) in pr.checks.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str("        {\n");
        body.push_str(&format!(
            "          \"name\": \"{}\",\n",
            json_escape(&check.name)
        ));
        body.push_str(&format!(
            "          \"status\": \"{}\",\n",
            json_escape(&check.status)
        ));
        body.push_str(&format!(
            "          \"conclusion\": \"{}\"\n",
            json_escape(&check.conclusion)
        ));
        body.push_str("        }");
    }
    body.push_str("\n      ]\n");
    body.push_str("    }");
}

fn write_pr_triage_queue_disposition_json(
    body: &mut String,
    disposition: &PrTriageQueueDisposition,
    indent: usize,
) {
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent + 2);
    body.push_str(&format!("{pad}{{\n"));
    body.push_str(&format!(
        "{inner}\"pr_number\": {},\n",
        disposition.pr_number
    ));
    body.push_str(&format!(
        "{inner}\"disposition\": \"{}\",\n",
        json_escape(&disposition.disposition)
    ));
    body.push_str(&format!(
        "{inner}\"reason\": \"{}\",\n",
        json_escape(&disposition.reason)
    ));
    body.push_str(&format!(
        "{inner}\"recommended_action\": \"{}\"\n",
        json_escape(&disposition.recommended_action)
    ));
    body.push_str(&format!("{pad}}}"));
}

fn write_pr_triage_finding_json(body: &mut String, finding: &PrTriageFinding, indent: usize) {
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent + 2);
    body.push_str(&format!("{pad}{{\n"));
    body.push_str(&format!(
        "{inner}\"category\": \"{}\",\n",
        json_escape(&finding.category)
    ));
    body.push_str(&format!(
        "{inner}\"severity\": \"{}\",\n",
        json_escape(&finding.severity)
    ));
    body.push_str(&format!(
        "{inner}\"message\": \"{}\",\n",
        json_escape(&finding.message)
    ));
    body.push_str(&format!("{inner}\"prs\": "));
    write_json_u64_array(body, &finding.prs);
    body.push_str(&format!(",\n{inner}\"details\": ["));
    write_json_string_array(body, &finding.details);
    body.push_str(&format!(
        "],\n{inner}\"recommended_action\": \"{}\"\n",
        json_escape(&finding.recommended_action)
    ));
    body.push_str(&format!("{pad}}}"));
}

fn write_json_u64_array(body: &mut String, values: &[u64]) {
    body.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            body.push_str(", ");
        }
        body.push_str(&value.to_string());
    }
    body.push(']');
}

fn pr_age_label(pr: &PrTriagePullRequest, today: i64) -> String {
    match pr_created_day(&pr.created_at) {
        Some(created) => format!("{}d", today - created),
        None => "unknown".to_string(),
    }
}

fn pr_checks_summary(checks: &[PrTriageCheck]) -> String {
    if checks.is_empty() {
        return "none".to_string();
    }
    let failed = checks.iter().filter(|check| pr_check_failed(check)).count();
    let pending = checks
        .iter()
        .filter(|check| pr_check_pending(check))
        .count();
    if failed > 0 {
        format!("{failed} failed, {pending} pending")
    } else if pending > 0 {
        format!("{pending} pending")
    } else {
        format!("{} passed", checks.len())
    }
}

fn format_pr_triage_ref(pr: &PrTriagePullRequest) -> String {
    format!("#{} {}", pr.number, pr.title)
}

fn markdown_escape_table(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

pub(crate) fn critic_impl() -> Result<(), String> {
    ensure_reports_dir()?;
    let changes = collect_pr_changes()?;
    let reports = report_index_entries()?;
    let receipts = receipt_index_entries()?;
    let findings = critic_findings(&changes, &reports, &receipts);
    write_report(
        "critic.md",
        &critic_markdown(&findings, &reports, &receipts),
    )?;
    write_report("critic.json", &critic_json(&findings, &reports, &receipts))
}

pub(crate) fn reports_impl(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("index") => reports_index(),
        Some(other) => Err(format!(
            "unknown reports command `{other}`\nusage: cargo xtask reports index"
        )),
        None => Err("missing reports command\nusage: cargo xtask reports index".to_string()),
    }
}

pub(crate) fn receipts_impl(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None => receipts_write(),
        Some("check") => receipts_check(),
        Some(other) => Err(format!(
            "unknown receipts command `{other}`\nusage: cargo xtask receipts\n       cargo xtask receipts check"
        )),
    }
}

pub(crate) fn reports_index_impl() -> Result<(), String> {
    let changes = collect_pr_changes()?;
    let reports = report_index_entries()?;
    let receipts = receipt_index_entries()?;
    let missing = report_index_missing_expected(&reports, &changes);
    let lane1_packets = report_index_lane1_readiness_packets(&reports);
    let status = report_index_status(&reports, &missing, &[]);
    let next_commands = report_index_next_commands(&missing, &lane1_packets);

    let markdown = report_index_markdown(status, &reports, &receipts, &missing, &next_commands);
    let json = report_index_json(status, &reports, &receipts, &missing, &next_commands);
    write_report("index.md", &markdown)?;
    write_report("index.json", &json)
}

pub(crate) fn receipts_write_impl() -> Result<(), String> {
    ensure_receipts_dir()?;
    let git = receipt_git_metadata();
    let mut records = Vec::new();
    for spec in receipt_specs() {
        let reports = spec
            .reports
            .iter()
            .map(|report| format!("target/ripr/reports/{report}"))
            .collect::<Vec<_>>();
        let status = receipt_status_from_reports(&reports);
        let record = ReceiptRecord {
            file: spec.file.to_string(),
            command: spec.command.to_string(),
            status,
            reports,
        };
        write_receipt(spec.file, &receipt_json(&record, &git))?;
        records.push(record);
    }
    write_report(
        "receipts.md",
        &receipts_report_markdown("pass", &records, &[]),
    )
}

fn receipts_check() -> Result<(), String> {
    let violations = receipts_check_violations()?;
    let records = read_receipt_records();
    let status = if violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    write_report(
        "receipts.md",
        &receipts_report_markdown(status, &records, &violations),
    )?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err("receipt validation failed; see target/ripr/reports/receipts.md".to_string())
    }
}

fn receipt_specs() -> Vec<ReceiptSpec> {
    vec![
        ReceiptSpec {
            file: "shape.json",
            command: "cargo xtask shape",
            reports: &["shape.md"],
        },
        ReceiptSpec {
            file: "fix-pr.json",
            command: "cargo xtask fix-pr",
            reports: &["fix-pr.md", "shape.md", "pr-summary.md"],
        },
        ReceiptSpec {
            file: "ci-fast.json",
            command: "cargo xtask ci-fast",
            reports: &[
                "static-language.md",
                "no-panic-family.md",
                "allow-attributes.md",
                "local-context.md",
                "local-context.json",
                "file-policy.md",
                "executable-files.md",
                "workflows.md",
                "spec-format.md",
                "fixture-contracts.md",
                "traceability.md",
                "capabilities.md",
                "workspace-shape.md",
                "architecture.md",
                "public-api.md",
                "output-contracts.md",
                "doc-index.md",
                "readme-state.md",
                "markdown-links.md",
                "campaign.md",
                "pr-shape.md",
                "generated.md",
                "dependencies.md",
                "process-policy.md",
                "network-policy.md",
            ],
        },
        ReceiptSpec {
            file: "check-pr.json",
            command: "cargo xtask check-pr",
            reports: &["check-pr.md", "pr-summary.md"],
        },
        ReceiptSpec {
            file: "fixtures.json",
            command: "cargo xtask fixtures",
            reports: &["fixtures.md"],
        },
        ReceiptSpec {
            file: "goldens.json",
            command: "cargo xtask goldens check",
            reports: &["goldens.md"],
        },
        ReceiptSpec {
            file: "test-oracles.json",
            command: "cargo xtask test-oracle-report",
            reports: &["test-oracles.md", "test-oracles.json"],
        },
        ReceiptSpec {
            file: "badge-artifacts.json",
            command: "cargo xtask badge-artifacts",
            reports: &[
                "ripr-badge.json",
                "ripr-badge-shields.json",
                "ripr-plus-badge.json",
                "ripr-plus-badge-shields.json",
                "ripr-badges.md",
            ],
        },
        ReceiptSpec {
            file: "repo-badge-artifacts.json",
            command: "cargo xtask repo-badge-artifacts",
            reports: &[
                "repo-ripr-badge.json",
                "repo-ripr-badge-shields.json",
                "repo-ripr-plus-badge.json",
                "repo-ripr-plus-badge-shields.json",
                "repo-ripr-badges.md",
            ],
        },
        ReceiptSpec {
            file: "dogfood.json",
            command: "cargo xtask dogfood",
            reports: &["dogfood.md", "dogfood.json"],
        },
        ReceiptSpec {
            file: "metrics.json",
            command: "cargo xtask metrics",
            reports: &["metrics.md", "metrics.json"],
        },
    ]
}

fn receipt_status_from_reports(reports: &[String]) -> String {
    let mut saw_report = false;
    let mut saw_warn = false;
    for report in reports {
        let path = Path::new(report);
        if !path.exists() {
            continue;
        }
        saw_report = true;
        match report_entry_status(path).as_str() {
            "fail" | "failed" => return "failed".to_string(),
            "warn" | "warning" => saw_warn = true,
            _ => {}
        }
    }
    if !saw_report {
        "missing".to_string()
    } else if saw_warn {
        "warn".to_string()
    } else {
        "passed".to_string()
    }
}

fn receipt_git_metadata() -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    values.insert(
        "branch".to_string(),
        git_value(&["rev-parse", "--abbrev-ref", "HEAD"]),
    );
    values.insert("commit".to_string(), git_value(&["rev-parse", "HEAD"]));
    values
}

fn git_value(args: &[&str]) -> String {
    let value = run_output_optional("git", args).unwrap_or_default();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn receipt_json(record: &ReceiptRecord, git: &BTreeMap<String, String>) -> String {
    let branch = git.get("branch").map(String::as_str).unwrap_or("unknown");
    let commit = git.get("commit").map(String::as_str).unwrap_or("unknown");
    let mut body = String::from("{\n");
    body.push_str("  \"schema_version\": \"0.1\",\n");
    body.push_str(&format!(
        "  \"command\": \"{}\",\n",
        json_escape(&record.command)
    ));
    body.push_str(&format!(
        "  \"status\": \"{}\",\n",
        json_escape(&record.status)
    ));
    body.push_str("  \"duration_ms\": 0,\n");
    body.push_str("  \"git\": {\n");
    body.push_str(&format!("    \"branch\": \"{}\",\n", json_escape(branch)));
    body.push_str(&format!("    \"commit\": \"{}\"\n", json_escape(commit)));
    body.push_str("  },\n");
    body.push_str("  \"reports\": [");
    write_json_string_array(&mut body, &record.reports);
    body.push_str("]\n");
    body.push_str("}\n");
    body
}

fn receipts_check_violations() -> Result<Vec<String>, String> {
    let mut violations = Vec::new();
    for spec in receipt_specs() {
        let path = receipts_dir().join(spec.file);
        if !path.exists() {
            violations.push(format!("missing receipt `{}`", normalize_path(&path)));
            continue;
        }
        let text = read_text_lossy(&path)?;
        if !text.contains("\"schema_version\": \"0.1\"") {
            violations.push(format!(
                "`{}` is missing schema_version 0.1",
                normalize_path(&path)
            ));
        }
        if !text.contains("\"command\"") {
            violations.push(format!("`{}` is missing command", normalize_path(&path)));
        }
        if !text.contains("\"status\"") {
            violations.push(format!("`{}` is missing status", normalize_path(&path)));
        }
        if !text.contains("\"git\"") {
            violations.push(format!(
                "`{}` is missing git metadata",
                normalize_path(&path)
            ));
        }
        if !text.contains("\"reports\"") {
            violations.push(format!(
                "`{}` is missing report paths",
                normalize_path(&path)
            ));
        }
        if let Some(status) = report_status_from_text(&text) {
            if !is_receipt_status(&status) {
                violations.push(format!(
                    "`{}` has unknown status `{status}`",
                    normalize_path(&path)
                ));
            }
        } else {
            violations.push(format!(
                "`{}` has no parseable status",
                normalize_path(&path)
            ));
        }
    }
    Ok(violations)
}

fn is_receipt_status(status: &str) -> bool {
    matches!(status, "passed" | "warn" | "failed" | "missing")
}

fn read_receipt_records() -> Vec<ReceiptRecord> {
    let mut records = Vec::new();
    for spec in receipt_specs() {
        let path = receipts_dir().join(spec.file);
        let status = if path.exists() {
            report_entry_status(&path)
        } else {
            "missing".to_string()
        };
        let reports = spec
            .reports
            .iter()
            .map(|report| format!("target/ripr/reports/{report}"))
            .collect::<Vec<_>>();
        records.push(ReceiptRecord {
            file: spec.file.to_string(),
            command: spec.command.to_string(),
            status,
            reports,
        });
    }
    records
}

fn receipts_report_markdown(
    status: &str,
    records: &[ReceiptRecord],
    violations: &[String],
) -> String {
    let mut body = format!("# ripr receipts report\n\nStatus: {status}\n\n");
    body.push_str("Receipts are machine-readable evidence for gate and report runs.\n\n");
    body.push_str("## Receipts\n\n");
    body.push_str("| Receipt | Command | Status |\n| --- | --- | --- |\n");
    for record in records {
        body.push_str(&format!(
            "| `{}` | `{}` | `{}` |\n",
            markdown_cell(&format!("target/ripr/receipts/{}", record.file)),
            markdown_cell(&record.command),
            markdown_cell(&record.status)
        ));
    }
    body.push_str("\n## Validation\n\n");
    if violations.is_empty() {
        body.push_str("- All required receipts are present and structurally valid.\n");
    } else {
        for violation in violations {
            body.push_str(&format!("- {violation}\n"));
        }
    }
    body
}

fn precommit_report_body() -> String {
    "# ripr precommit report\n\nStatus: pass\n\nChecks:\n\n- `cargo fmt --check`\n- `cargo xtask check-static-language`\n- `cargo xtask check-no-panic-family`\n- `cargo xtask check-allow-attributes`\n- `cargo xtask check-local-context`\n- `cargo xtask check-file-policy`\n- `cargo xtask check-executable-files`\n- `cargo xtask check-workflows`\n- `cargo xtask check-droid-review-config`\n- `cargo xtask check-spec-format`\n- `cargo xtask check-spec-numbering`\n- `cargo xtask check-fixture-contracts`\n- `cargo xtask check-rust-judged-panel`\n- `cargo xtask check-traceability`\n- `cargo xtask check-capabilities`\n- `cargo xtask check-workspace-shape`\n- `cargo xtask check-architecture`\n- `cargo xtask check-public-api`\n- `cargo xtask check-output-contracts`\n- `cargo xtask check-doc-artifacts`\n- `cargo xtask check-doc-index`\n- `cargo xtask check-readme-state`\n- `cargo xtask markdown-links`\n- `cargo xtask check-pr-shape`\n- `cargo xtask check-command-catalog`\n- `cargo xtask check-generated`\n- `cargo xtask check-badge-diff-policy`\n- `cargo xtask check-generated-clean`\n- `cargo xtask check-proof-packs`\n- `cargo xtask check-release-targets`\n- `cargo xtask check-dependencies`\n- `cargo xtask check-process-policy`\n- `cargo xtask check-network-policy`\n- `cargo xtask check-lint-policy`\n\nNext command:\n\n```bash\ncargo xtask check-pr\n```\n".to_string()
}

/// Compose the check-pr report for either terminal state (#3036). One
/// authority owns the status line, checks inventory, and footer so the
/// success and failure artifacts cannot drift apart; a failure report names
/// the failed gate, its reproduce command, bounded error lines, and the
/// gates that were never run, never carries a `Status: pass` line, and
/// marks `pr-summary.md` as not refreshed because the failure path returns
/// before `pr_summary()` runs.
fn check_pr_report(failure: Option<&CheckPrGateFailure>) -> String {
    let status = if failure.is_some() { "fail" } else { "pass" };
    let mut body = format!("# ripr check-pr report\n\nStatus: {status}\n");
    if let Some(failure) = failure {
        let first_lines = if failure.bounded_error.is_empty() {
            "(gate produced no error output)"
        } else {
            failure.bounded_error.as_str()
        };
        body.push_str(&format!(
            "\n## First actionable failure\n\nGate: `{}`\nReproduce: `{}`\n\nFirst lines:\n  {first_lines}\n\nNot run after the first failure:\n",
            failure.name, failure.reproduce
        ));
        if failure.not_run.is_empty() {
            body.push_str("- (none — the failed gate was the last)\n");
        } else {
            for name in &failure.not_run {
                body.push_str(&format!("- `{name}`\n"));
            }
        }
        body.push_str(&format!(
            "\n## Inherited-failure comparison (advisory)\n\nStatus: {}\nDetail: {}\n",
            failure.baseline.status, failure.baseline.detail
        ));
    }
    let pr_summary_entry = if failure.is_some() {
        // The failure path returns before pr_summary() runs, so advertising
        // it without the marker would present a stale artifact as current.
        "- `target/ripr/reports/pr-summary.md` (not refreshed — the run stopped at the first failed gate)"
    } else {
        "- `target/ripr/reports/pr-summary.md`"
    };
    body.push_str(&format!("\nChecks:\n\n- `cargo xtask ci-fast`\n- `cargo clippy --workspace --all-targets -- -D warnings`\n- `cargo doc --workspace --no-deps`\n- `cargo xtask pr-summary`\n\nReports:\n\n{pr_summary_entry}\n- `target/ripr/reports/check-pr.md`\n\nRelease/package gates are intentionally left to `cargo xtask ci-full` or release-specific workflows.\n"));
    body
}

pub(crate) fn finish_policy_report(
    spec: PolicyReportSpec<'_>,
    violations: &[String],
) -> Result<(), String> {
    let status = if violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let mut body = format!("# {}\n\nStatus: {status}\n\n", spec.check);
    body.push_str("## Why This Matters\n\n");
    body.push_str(spec.why_it_matters);
    body.push_str("\n\n");

    if violations.is_empty() {
        body.push_str("## Violations\n\nNone detected.\n\n");
    } else {
        body.push_str("## Violations\n\n");
        for violation in violations {
            body.push_str("```text\n");
            body.push_str(violation);
            body.push_str("\n```\n\n");
        }
    }

    if !violations.is_empty() {
        body.push_str("## Fix Kind\n\n```text\n");
        body.push_str(fix_kind_name(&spec.fix_kind));
        body.push_str("\n```\n\n");

        body.push_str("## Recommended Fixes\n\n");
        for (index, fix) in spec.recommended_fixes.iter().enumerate() {
            body.push_str(&format!("{}. {fix}\n", index + 1));
        }
        body.push('\n');

        if let Some(template) = spec.exception_template {
            body.push_str("## Exception Template\n\n```text\n");
            body.push_str(template);
            body.push_str("\n```\n\n");
        }
    }

    body.push_str("## Rerun\n\n```bash\n");
    body.push_str(spec.rerun_command);
    body.push_str("\n```\n");

    write_report(spec.report_file, &body)?;

    if violations.is_empty() {
        println!(
            "{}: pass (target/ripr/reports/{})",
            spec.check, spec.report_file
        );
        Ok(())
    } else {
        Err(format!(
            "{} failed; see target/ripr/reports/{}\n{}",
            spec.check,
            spec.report_file,
            violations.join("\n")
        ))
    }
}

fn fix_kind_name(fix_kind: &FixKind) -> &'static str {
    match fix_kind {
        FixKind::AutoFixable => "auto_fixable",
        FixKind::AuthorDecisionRequired => "author_decision_required",
        FixKind::ReviewerDecisionRequired => "reviewer_decision_required",
        FixKind::PolicyExceptionRequired => "policy_exception_required",
    }
}

fn check_static_language_impl() -> Result<(), String> {
    let report_spec = PolicyReportSpec {
        report_file: "static-language.md",
        check: "check-static-language",
        why_it_matters: "Static output must preserve the boundary between draft exposure evidence and real mutation results.",
        fix_kind: FixKind::ReviewerDecisionRequired,
        recommended_fixes: &[
            "Rewrite static product output to use the approved exposure vocabulary.",
            "For an innocent word in a comment, append `ripr-allow: static-language: <reason>` to that single line.",
            "If a whole file is explanatory documentation, add a reasoned `[[allow]]` entry to the static-language allowlist.",
        ],
        rerun_command: "cargo xtask check-static-language",
        exception_template: Some(
            ".ripr/static-language-allowlist.toml entry:\n[[allow]]\npath = \"path/to/file.md\"\nowner = \"team\"\nreason = \"why this file may quote prohibited vocabulary\"",
        ),
    };

    let allowed = match load_static_language_allowlist() {
        Ok(entries) => entries,
        Err(violations) => return finish_policy_report(report_spec, &violations),
    };
    let forbidden = forbidden_static_terms();
    let mut violations = Vec::new();

    for path in collect_files(Path::new("."))? {
        let normalized = normalize_path(&path);
        if !should_scan_static_language_path(&allowed, &normalized) {
            continue;
        }
        let text = read_text_lossy(&path)?;
        for (line_number, line) in text.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            // A reasoned inline allow suppresses this line's forbidden terms.
            // It is finer-grained than a whole-file allowlist entry (smaller
            // bypass surface, reviewable in the diff) and must carry a reason.
            if line_has_static_language_inline_allow(&lower) {
                continue;
            }
            for term in &forbidden {
                if contains_word(&lower, term) {
                    violations.push(static_language_violation_message(
                        &normalized,
                        line_number + 1,
                        term,
                        line,
                    ));
                }
            }
        }
    }

    finish_policy_report(report_spec, &violations)
}

fn check_allow_attributes_impl() -> Result<(), String> {
    let allowlist = read_count_allowlist(".ripr/allow-attributes.txt")?;
    let guarded = guarded_allow_attribute_lints();
    let mut counts = BTreeMap::<(String, String), Vec<usize>>::new();

    for path in tracked_files()? {
        if !path.ends_with(".rs") {
            continue;
        }
        let file_path = Path::new(&path);
        if !file_path.exists() {
            continue;
        }
        let text = read_text_lossy(file_path)?;
        for (line, attribute) in guarded_allow_attributes_in_text(&text, &guarded) {
            counts
                .entry((path.clone(), attribute))
                .or_default()
                .push(line);
        }
    }

    let mut violations = Vec::new();
    for ((path, attribute), lines) in &counts {
        let allowed = allowlist
            .get(&(path.clone(), attribute.clone()))
            .copied()
            .unwrap_or(0);
        if lines.len() > allowed {
            violations.push(format!(
                "{path}:{} contains `{attribute}` {} time(s), allowed {allowed}\n  preferred: fix the lint or add a narrow allowlist entry with a reason",
                allow_attribute_line_summary(lines),
                lines.len()
            ));
        }
    }

    for ((path, attribute), allowed) in &allowlist {
        if !guarded.contains(attribute_lint_name(attribute).unwrap_or(attribute)) {
            violations.push(format!(
                ".ripr/allow-attributes.txt contains unsupported guarded attribute `{attribute}` for {path}; remove stale or out-of-scope exceptions"
            ));
            continue;
        }
        let actual = counts
            .get(&(path.clone(), attribute.clone()))
            .map(Vec::len)
            .unwrap_or(0);
        if actual > *allowed {
            violations.push(format!(
                "{path} contains `{attribute}` {actual} time(s), allowed {allowed}"
            ));
        }
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "allow-attributes.md",
            check: "check-allow-attributes",
            why_it_matters: "Lint suppressions should not be used to hide repo guardrails. If a suppression is unavoidable, it needs a narrow reviewed exception with a reason.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Remove the lint suppression and fix the underlying warning.",
                "If the suppression is temporary and intentional, add a narrow allowlist entry with a reason.",
                "Do not allowlist panic-family, unsafe, or broad warning suppressions unless the PR explicitly owns that exception.",
            ],
            rerun_command: "cargo xtask check-allow-attributes",
            exception_template: Some(
                ".ripr/allow-attributes.txt entry:\npath/to/file.rs|allow(clippy::unwrap_used)|1|reason",
            ),
        },
        &violations,
    )
}

fn check_local_context_impl() -> Result<(), String> {
    let allowlist = read_local_context_allowlist("policy/local_context_allowlist.txt")?;
    let mut violations = validate_local_context_allowlist(&allowlist);
    let mut grouped = BTreeMap::<(String, String), (BTreeSet<String>, Vec<Option<usize>>)>::new();

    for path in tracked_files()? {
        let file_path = Path::new(&path);
        if !file_path.exists() || path == "policy/local_context_allowlist.txt" {
            continue;
        }

        for finding in local_context_findings_for_path(&path)? {
            let entry = grouped
                .entry((finding.path.clone(), finding.pattern.clone()))
                .or_insert_with(|| (BTreeSet::new(), Vec::new()));
            entry.0.insert(finding.problem);
            entry.1.push(finding.line);
        }
    }

    let allowed = allowlist
        .iter()
        .map(|entry| ((entry.path.clone(), entry.pattern.clone()), entry.max_count))
        .collect::<BTreeMap<_, _>>();

    for ((path, pattern), (problems, lines)) in &grouped {
        let actual = lines.len();
        let allowed_count = allowed
            .get(&(path.clone(), pattern.clone()))
            .copied()
            .unwrap_or(0);
        if actual <= allowed_count && actual == 0 {
            continue;
        }
        if actual > allowed_count {
            let line_summary = local_context_line_summary(lines);
            violations.push(format!(
                "Path: {path}\nProblem: {}\nPattern: {pattern}\nCount: {actual}, allowed: {allowed_count}\nLines: {line_summary}\nWhy this matters: Repository docs should contain durable project state, not local runtime/session state from one machine or Codex run.\nRecommended fixes:\n1. Delete runtime/session artifacts instead of committing them.\n2. Move durable learnings to docs/LEARNINGS.md.\n3. Move generated state to target/ripr/reports, target/ripr/receipts, or target/ripr/learning.",
                problems.iter().cloned().collect::<Vec<_>>().join("; ")
            ));
        } else if actual < allowed_count && actual > 0 {
            // Stale bound: the allowlist grants more budget than the code uses
            // (#2413). Flag so the bound is tightened.
            violations.push(format!(
                "Path: {path}\nPattern: {pattern}\nStale allowlist bound: max_count={allowed_count} but actual={actual}; tighten to {actual}"
            ));
        }
    }

    // Flag orphaned allowlist entries whose path/pattern no longer appears in
    // any tracked file (count dropped to zero or the file was removed).
    for entry in &allowlist {
        let key = (entry.path.clone(), entry.pattern.clone());
        if !grouped.contains_key(&key) && entry.max_count > 0 {
            violations.push(format!(
                "Path: {}\nPattern: {}\nOrphaned allowlist entry: max_count={} but pattern not found in any tracked file; remove the entry",
                entry.path, entry.pattern, entry.max_count
            ));
        }
    }

    write_local_context_json(&violations)?;
    finish_policy_report(
        PolicyReportSpec {
            report_file: "local-context.md",
            check: "check-local-context",
            why_it_matters: "Repository state must be durable and portable. Machine paths, Codex memory paths, sandbox references, local transcripts, and session-state documents belong in generated artifacts or local notes, not committed repo knowledge.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Delete committed runtime/session artifacts.",
                "Move durable repo knowledge to docs/LEARNINGS.md or campaign/capability metadata.",
                "Move generated state to target/ripr/reports, target/ripr/receipts, or target/ripr/learning.",
                "Use policy/local_context_allowlist.txt only for narrow generic examples with a reason.",
            ],
            rerun_command: "cargo xtask check-local-context",
            exception_template: Some(
                "policy/local_context_allowlist.txt entry:\npath|pattern|max_count|reason",
            ),
        },
        &violations,
    )
}

pub(crate) fn rust_conversion_candidates() -> Result<(), String> {
    let candidates = collect_rust_conversion_candidates()?;
    write_report(
        "rust-conversion-candidates.md",
        &rust_conversion_candidates_markdown(&candidates),
    )?;
    write_report(
        "rust-conversion-candidates.json",
        &rust_conversion_candidates_json(&candidates)?,
    )
}

fn collect_rust_conversion_candidates() -> Result<Vec<RustConversionCandidate>, String> {
    let mut candidates = Vec::new();

    for path in tracked_files()? {
        if is_non_rust_programming_candidate(&path) {
            candidates.extend(non_rust_source_conversion_candidate(&path));
        }
    }

    for path in tracked_files()?
        .into_iter()
        .filter(|path| path.starts_with(".github/workflows/"))
        .filter(|path| path.ends_with(".yml") || path.ends_with(".yaml"))
    {
        let text = read_text_lossy(Path::new(&path))?;
        for block in extract_workflow_run_blocks(&text) {
            if let Some(candidate) = workflow_run_conversion_candidate(&path, &block) {
                candidates.push(candidate);
            }
        }
    }

    candidates.sort_by(|left, right| {
        conversion_priority_rank(&left.priority)
            .cmp(&conversion_priority_rank(&right.priority))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.kind.cmp(&right.kind))
    });
    Ok(candidates)
}

fn non_rust_source_conversion_candidate(path: &str) -> Option<RustConversionCandidate> {
    if path.starts_with("editors/vscode/") && path.ends_with(".ts") {
        return Some(RustConversionCandidate {
            path: path.to_string(),
            line: None,
            kind: "retained_external_runtime".to_string(),
            priority: "retained".to_string(),
            current_surface: "VS Code extension TypeScript".to_string(),
            recommendation: "Keep this code in the editor adapter; only move server behavior into ripr Rust modules or xtask.".to_string(),
            reason: "The VS Code Extension Host API is TypeScript-native, so this is an approved adapter boundary rather than core automation.".to_string(),
        });
    }

    if path.starts_with("fixtures/") {
        return Some(RustConversionCandidate {
            path: path.to_string(),
            line: None,
            kind: "retained_fixture_input".to_string(),
            priority: "retained".to_string(),
            current_surface: "fixture workspace input".to_string(),
            recommendation: "Keep as fixture input unless the fixture no longer maps to a spec; move reusable fixture orchestration into Rust/xtask instead.".to_string(),
            reason: "Fixture workspaces are analyzed inputs for preview language adapters, not repository automation.".to_string(),
        });
    }

    Some(RustConversionCandidate {
        path: path.to_string(),
        line: None,
        kind: "non_rust_programming_without_retention_rule".to_string(),
        priority: "high".to_string(),
        current_surface: "unapproved non-Rust implementation or automation".to_string(),
        recommendation: "Move this behavior into Rust under crates/ripr for product logic or xtask for repository automation.".to_string(),
        reason: "Rust is the default implementation surface, and this file has no approved external-runtime or fixture retention rule.".to_string(),
    })
}

fn workflow_run_conversion_candidate(
    path: &str,
    block: &RunBlock,
) -> Option<RustConversionCandidate> {
    let text = block.text.trim();
    if text.is_empty() || text.starts_with("cargo xtask ") {
        return None;
    }
    if workflow_run_is_external_runtime(text) {
        return Some(RustConversionCandidate {
            path: path.to_string(),
            line: Some(block.line_number),
            kind: "retained_external_runtime".to_string(),
            priority: "retained".to_string(),
            current_surface: "workflow command for an external toolchain".to_string(),
            recommendation: "Keep as a direct workflow call unless repo-owned report assembly grows around it; put repo-owned assembly into xtask.".to_string(),
            reason: "The command delegates to Cargo, npm, Codecov, or another external tool rather than implementing repo policy in shell.".to_string(),
        });
    }

    if workflow_run_contains_shell_logic(text) {
        return Some(RustConversionCandidate {
            path: path.to_string(),
            line: Some(block.line_number),
            kind: "workflow_shell_logic".to_string(),
            priority: "medium".to_string(),
            current_surface: "GitHub Actions shell run block".to_string(),
            recommendation: "Move repo-owned file/report/summary assembly into a focused cargo xtask command, then leave this workflow step as a short cargo xtask invocation.".to_string(),
            reason: "Workflow shell is harder to type-check and test than Rust/xtask, especially when it creates local reports or step summaries.".to_string(),
        });
    }

    None
}

fn workflow_run_is_external_runtime(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("cargo ")
        || trimmed.starts_with("npm ")
        || trimmed.starts_with("code ")
        || trimmed.starts_with("npx ")
        || trimmed.starts_with("codecov")
}

fn workflow_run_contains_shell_logic(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let markers = [
        "mkdir ",
        "printf ",
        "echo ",
        "git diff",
        "ls ",
        "tee ",
        "cat ",
        "if ",
        "fi",
        "for ",
        "while ",
        "<<",
        "&&",
        "||",
        "$github_step_summary",
        "$github_env",
    ];
    text.lines().count() > 1 || markers.iter().any(|marker| lower.contains(marker))
}

fn conversion_priority_rank(priority: &str) -> u8 {
    match priority {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        "retained" => 3,
        _ => 4,
    }
}

fn rust_conversion_candidates_markdown(candidates: &[RustConversionCandidate]) -> String {
    let actionable = candidates
        .iter()
        .filter(|candidate| candidate.priority != "retained")
        .count();
    let retained = candidates.len().saturating_sub(actionable);
    let mut body = format!(
        "# ripr Rust conversion candidates\n\nStatus: advisory\n\nActionable candidates: {actionable}\nRetained boundaries inspected: {retained}\n\nThis report keeps the Rust-first policy actionable without treating fixture inputs or editor adapter code as core implementation debt.\n\n"
    );

    body.push_str("## Actionable candidates\n\n");
    if actionable == 0 {
        body.push_str("No unretained non-Rust source files or workflow shell migration candidates were found.\n");
    } else {
        body.push_str("| Priority | Path | Line | Kind | Recommendation | Reason |\n");
        body.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for candidate in candidates
            .iter()
            .filter(|candidate| candidate.priority != "retained")
        {
            push_rust_conversion_candidate_row(&mut body, candidate);
        }
    }

    body.push_str("\n## Retained boundaries\n\n");
    if retained == 0 {
        body.push_str("No retained non-Rust runtime or fixture boundaries were found.\n");
    } else {
        body.push_str("| Path | Line | Surface | Recommendation | Reason |\n");
        body.push_str("| --- | --- | --- | --- | --- |\n");
        for candidate in candidates
            .iter()
            .filter(|candidate| candidate.priority == "retained")
        {
            let line = candidate
                .line
                .map(|line| line.to_string())
                .unwrap_or_else(|| "-".to_string());
            body.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                markdown_cell(&candidate.path),
                line,
                markdown_cell(&candidate.current_surface),
                markdown_cell(&candidate.recommendation),
                markdown_cell(&candidate.reason)
            ));
        }
    }

    body.push_str("\nNext command:\n\n```bash\ncargo xtask check-file-policy\n```\n");
    body
}

fn push_rust_conversion_candidate_row(body: &mut String, candidate: &RustConversionCandidate) {
    let line = candidate
        .line
        .map(|line| line.to_string())
        .unwrap_or_else(|| "-".to_string());
    body.push_str(&format!(
        "| {} | `{}` | {} | {} | {} | {} |\n",
        markdown_cell(&candidate.priority),
        markdown_cell(&candidate.path),
        line,
        markdown_cell(&candidate.kind),
        markdown_cell(&candidate.recommendation),
        markdown_cell(&candidate.reason)
    ));
}

fn rust_conversion_candidates_json(
    candidates: &[RustConversionCandidate],
) -> Result<String, String> {
    let items = candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "path": candidate.path,
                "line": candidate.line,
                "kind": candidate.kind,
                "priority": candidate.priority,
                "current_surface": candidate.current_surface,
                "recommendation": candidate.recommendation,
                "reason": candidate.reason,
            })
        })
        .collect::<Vec<_>>();
    let value = serde_json::json!({
        "schema_version": 1,
        "actionable_count": candidates.iter().filter(|candidate| candidate.priority != "retained").count(),
        "retained_count": candidates.iter().filter(|candidate| candidate.priority == "retained").count(),
        "candidates": items,
    });
    serde_json::to_string_pretty(&value)
        .map_err(|err| format!("failed to serialize rust-conversion-candidates report: {err}"))
}

fn check_executable_files_impl() -> Result<(), String> {
    let allowlist = read_path_allowlist_optional("policy/executable_allowlist.txt")?;
    let output = run_output("git", &["ls-files", "--stage"])?;
    let mut violations = Vec::new();

    for line in output.lines() {
        let Some((mode, path)) = parse_git_stage_line(line) else {
            continue;
        };
        let normalized = normalize_slashes(path);
        if mode == "100755" && !allowlist.contains(&normalized) {
            violations.push(format!(
                "checked-in executable file is not allowlisted: {normalized}\n  preferred: use cargo xtask instead of executable scripts"
            ));
        }
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "executable-files.md",
            check: "check-executable-files",
            why_it_matters: "Checked-in executable scripts make automation drift away from the Rust-first xtask surface.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Remove the executable bit from ordinary files.",
                "Move script behavior into xtask.",
                "If an executable file is truly required, add a reviewed allowlist entry.",
            ],
            rerun_command: "cargo xtask check-executable-files",
            exception_template: Some("policy/executable_allowlist.txt entry:\npath/to/file"),
        },
        &violations,
    )
}

fn check_workflows_impl() -> Result<(), String> {
    let budgets = read_workflow_budgets("policy/workflow_allowlist.txt")?;
    let runtime_allowlist = read_count_allowlist("policy/workflow_action_runtime_allowlist.txt")?;
    let mut violations = Vec::new();

    for path in collect_files(Path::new(".github/workflows"))? {
        let normalized = normalize_path(&path);
        if !(normalized.ends_with(".yml") || normalized.ends_with(".yaml")) {
            continue;
        }
        let Some(budget) = budgets.get(&normalized) else {
            violations.push(format!(
                "missing workflow budget for {normalized} in policy/workflow_allowlist.txt"
            ));
            continue;
        };
        let text = read_text_lossy(&path)?;
        violations.extend(workflow_runtime_violations(
            &normalized,
            &text,
            &runtime_allowlist,
        ));
        violations.extend(workflow_review_thread_mutation_violations(
            &normalized,
            &text,
        ));
        violations.extend(workflow_bare_self_hosted_violations(&normalized, &text));
        violations.extend(workflow_plain_scalar_comment_violations(&normalized, &text));
        violations.extend(scratch_gc_concurrency_violations(&normalized, &text));
        for block in extract_workflow_run_blocks(&text) {
            if block.non_empty_lines > budget.max_non_empty_lines {
                violations.push(format!(
                    "{normalized}:{} run block has {} non-empty line(s), allowed {} ({})",
                    block.line_number,
                    block.non_empty_lines,
                    budget.max_non_empty_lines,
                    budget.reason
                ));
            }
            let lower = block.text.to_ascii_lowercase();
            if lower.contains(shell_fetch_tool_name()) && lower.contains("| sh") {
                violations.push(format!(
                    "{normalized}:{} run block contains network fetch piped to sh",
                    block.line_number
                ));
            }
            if lower.contains(shell_fetch_tool_name()) && lower.contains("| bash") {
                violations.push(format!(
                    "{normalized}:{} run block contains network fetch piped to bash",
                    block.line_number
                ));
            }
        }
    }
    violations.extend(repository_owned_review_thread_mutation_violations()?);
    validate_assistant_loop_health_fixture_corpus(&mut violations)?;
    violations.extend(routed_rust_workflow_contract_violations_for_repo()?);

    finish_policy_report(
        PolicyReportSpec {
            report_file: "workflows.md",
            check: "check-workflows",
            why_it_matters: "GitHub Actions should orchestrate xtask, Cargo, and npm commands instead of hiding complex shell logic in workflow YAML.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Move complex workflow logic into xtask or an npm script owned by the extension surface.",
                "Keep workflow run blocks under the documented line budget.",
                "Use Node-24-backed action majors where official releases exist.",
                "Use Node 24 for VS Code extension build and publish workflows.",
                "Add or adjust a workflow budget entry only when the workflow surface is intentionally larger.",
            ],
            rerun_command: "cargo xtask check-workflows",
            exception_template: Some(
                "policy/workflow_allowlist.txt entry:\n.github/workflows/name.yml|max_non_empty_lines|reason\n\npolicy/workflow_action_runtime_allowlist.txt entry:\n.github/workflows/name.yml|action/ref|max_count|reason",
            ),
        },
        &violations,
    )
}

/// Keep the scratch-GC matrix isolated by pool.
///
/// A workflow-level concurrency group serializes the whole matrix behind the
/// slowest or unavailable self-hosted pool. The resulting pending-run
/// eviction is especially dangerous here because `cancelled` is not a failed
/// workflow and therefore produces no useful CI signal.
fn scratch_gc_concurrency_violations(path: &str, text: &str) -> Vec<String> {
    const WORKFLOW: &str = ".github/workflows/scratch-gc.yml";
    const GROUP: &str = "group: scratch-gc-${{ github.repository }}-${{ matrix.pool }}";

    if path != WORKFLOW {
        return Vec::new();
    }

    let lines: Vec<&str> = text.lines().collect();
    let has_top_level_concurrency = lines
        .iter()
        .any(|line| line.trim_start().len() == line.len() && line.trim() == "concurrency:");
    let mut in_scratch_job = false;
    let mut in_concurrency = false;
    let mut concurrency_lines = Vec::new();
    for line in &lines {
        let indent = line.len() - line.trim_start().len();
        if *line == "  scratch-gc:" {
            in_scratch_job = true;
            continue;
        }
        if in_scratch_job && indent == 2 && !line.trim().is_empty() {
            in_scratch_job = false;
            in_concurrency = false;
        }
        if in_scratch_job && indent == 4 && line.trim() == "concurrency:" {
            in_concurrency = true;
            continue;
        }
        if in_concurrency {
            if indent <= 4 && !line.trim().is_empty() {
                in_concurrency = false;
            } else {
                concurrency_lines.push(line.trim());
            }
        }
    }
    let has_pool_group = concurrency_lines.contains(&GROUP);
    let has_non_cancelling_pool_queue = concurrency_lines.contains(&"cancel-in-progress: false");

    let mut violations = Vec::new();
    if has_top_level_concurrency {
        violations.push(format!(
            "{WORKFLOW}: scratch-GC concurrency must be job-level and keyed by matrix.pool; workflow-level concurrency starves the matrix when one pool is unavailable"
        ));
    }
    if !has_pool_group || !has_non_cancelling_pool_queue {
        violations.push(format!(
            "{WORKFLOW}: scratch-GC must preserve a non-cancelling per-pool concurrency group ({GROUP})"
        ));
    }
    violations
}

/// Workflow automation must not perform review-thread resolution without adjudication.
///
/// A workflow-side review-thread mutation turns provider failure or an
/// unreviewed finding into an apparently resolved conversation. Review-thread
/// resolution remains an explicit, evidence-backed operator action; the policy
/// rejects common GraphQL/name variants so a renamed blind resolver cannot
/// re-enter unnoticed. Repository-owned delegated automation is scanned too,
/// so a local composite action or xtask helper cannot hide the mutation.
fn workflow_review_thread_mutation_violations(path: &str, text: &str) -> Vec<String> {
    if text.lines().any(review_thread_mutation_line) {
        return vec![format!(
            "{path}: workflow contains a review-thread resolution mutation; review-thread resolution requires explicit adjudication outside automated workflow mutation"
        )];
    }
    Vec::new()
}

fn review_thread_mutation_line(line: &str) -> bool {
    let normalized = line
        .chars()
        .map(|character| character.to_ascii_lowercase())
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    normalized.contains(&review_thread_mutation_token())
}

fn review_thread_mutation_token() -> String {
    [
        'r', 'e', 's', 'o', 'l', 'v', 'e', 'r', 'e', 'v', 'i', 'e', 'w', 't', 'h', 'r', 'e', 'a',
        'd',
    ]
    .into_iter()
    .collect()
}

fn repository_owned_review_thread_mutation_violations() -> Result<Vec<String>, String> {
    let mut violations = Vec::new();

    for root in [
        Path::new(".github/actions"),
        Path::new(".github/scripts"),
        Path::new("scripts"),
        Path::new("tools"),
        Path::new("xtask/src"),
    ] {
        if !root.exists() {
            continue;
        }
        for path in collect_files(root)? {
            let normalized = normalize_path(&path);
            if normalized.ends_with("xtask/src/tests.rs") {
                continue;
            }
            let text = read_text_lossy(&path)?;
            if text.lines().any(review_thread_mutation_line) {
                violations.push(format!(
                    "{normalized}: repository-owned automation contains a review-thread resolution mutation; review-thread resolution requires explicit adjudication outside automated workflow mutation"
                ));
            }
        }
    }

    Ok(violations)
}

/// Flag a `run:` written as a plain YAML scalar that contains ` #`.
///
/// YAML treats ` #` in a plain scalar as the start of a comment, so the command
/// is silently truncated at that point. When the truncated remainder holds an
/// unterminated quote the shell fails with `unexpected EOF`, and when it does
/// not the step runs a *different, shorter* command with no error at all — the
/// worse outcome. A block scalar (`run: |`) has no comment rule and is immune.
///
/// This is enforced because it actually happened: a `printf` summary line
/// containing an issue reference was cut mid-string, and nothing local caught it
/// — `check-workflows` passed because it never parsed the YAML.
fn workflow_plain_scalar_comment_violations(path: &str, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some((_, after)) = line.split_once("run:") else {
            continue;
        };
        // Only plain scalars are affected; `run: |` and `run: >` are safe, and a
        // fully quoted scalar carries its own delimiters.
        let body = after.trim_start();
        if body.starts_with('|') || body.starts_with('>') || body.is_empty() {
            continue;
        }
        if body.starts_with('"') || body.starts_with('\'') {
            continue;
        }
        if body.contains(" #") {
            violations.push(format!(
                "{path}:{} plain-scalar `run:` contains ` #`, which YAML reads as a comment and truncates the command; use a `run: |` block scalar",
                index + 1
            ));
        }
    }
    violations
}

fn workflow_bare_self_hosted_violations(path: &str, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("runs-on:")
            && lower.contains('[')
            && lower.contains("self-hosted")
            && lower.contains("linux")
            && lower.contains("x64")
        {
            violations.push(format!(
                "{path}:{} bare inline self-hosted/linux/x64 runs-on is forbidden; use explicit group and capacity labels",
                index + 1
            ));
        }

        if line.trim() != "- self-hosted" {
            continue;
        }
        let end = (index + 17).min(lines.len());
        let start = index.saturating_sub(8);
        let window = &lines[start..end];
        let has_linux = window.iter().any(|candidate| candidate.trim() == "- linux");
        let has_x64 = window.iter().any(|candidate| candidate.trim() == "- x64");
        let has_group = window
            .iter()
            .any(|candidate| candidate.trim_start().starts_with("group: em-ci-"));
        let has_capacity = window.iter().any(|candidate| {
            matches!(
                candidate.trim(),
                "- em-ci"
                    | "- ci-nano"
                    | "- policy-nano"
                    | "- workflow-nano"
                    | "- rust-tiny"
                    | "- rust-medium"
                    | "- rust-large"
                    | "- rust-16gb"
                    | "- cx23"
                    | "- cx33"
                    | "- cx43"
                    | "- cx53"
                    | "- cpx42"
            )
        });
        if has_linux && has_x64 && !(has_group && has_capacity) {
            violations.push(format!(
                "{path}:{} bare self-hosted block lacks group/capacity labels",
                index + 1
            ));
        }
    }
    violations
}

fn routed_rust_workflow_contract_violations_for_repo() -> Result<Vec<String>, String> {
    let workflow_path = Path::new(".github/workflows/routed-rust.yml");
    if !workflow_path.exists() {
        return Ok(Vec::new());
    }

    let workflow = read_text_lossy(workflow_path)?;
    let reusable_workflow = if workflow.contains(ROUTED_RUST_REUSABLE_WORKFLOW_REF) {
        optional_policy_text(ROUTED_RUST_REUSABLE_WORKFLOW_PATH)?
    } else {
        None
    };
    let settings = optional_policy_text(".github/settings.yml")?;
    let lane_whitelist = optional_policy_text("policy/ci-lane-whitelist.toml")?;

    Ok(routed_rust_workflow_contract_violations_with_reusable(
        &workflow,
        reusable_workflow.as_deref(),
        settings.as_deref(),
        lane_whitelist.as_deref(),
    ))
}

fn optional_policy_text(path: &str) -> Result<Option<String>, String> {
    let path = Path::new(path);
    if path.exists() {
        read_text_lossy(path).map(Some)
    } else {
        Ok(None)
    }
}

/// Routed-rust jobs that must each carry an explicit `timeout-minutes`
/// deadline (issue #2230).
const ROUTED_RUST_DEADLINE_JOBS: [&str; 8] = [
    "route",
    "detect-docs-only",
    "rust-cx43",
    "rust-cpx42",
    "rust-cx53",
    "rust-github",
    "docs-gate",
    "result",
];

const ROUTED_RUST_IMPLEMENTATION_JOBS: [&str; 4] =
    ["rust-cx43", "rust-cpx42", "rust-cx53", "rust-github"];
const ROUTED_RUST_REUSABLE_WORKFLOW_PATH: &str = ".github/workflows/rust-gates.yml";
const ROUTED_RUST_REUSABLE_WORKFLOW_REF: &str = "uses: ./.github/workflows/rust-gates.yml";

/// Whether any line in the named job block satisfies `predicate`.
/// Job keys are exactly two-space-indented `name:` lines under `jobs:`;
/// anything deeper belongs to the current block.
fn routed_rust_job_block_any(
    workflow: &str,
    job: &str,
    mut predicate: impl FnMut(&str) -> bool,
) -> bool {
    let job_header = format!("{job}:");
    let mut in_block = false;
    for line in workflow.lines() {
        let job_level_key = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('-');
        if job_level_key {
            in_block = line.trim() == job_header;
            continue;
        }
        if in_block && !line.is_empty() && !line.starts_with(' ') {
            break;
        }
        if in_block && predicate(line) {
            return true;
        }
    }
    false
}

fn routed_rust_job_block_has_deadline(workflow: &str, job: &str) -> bool {
    routed_rust_job_block_any(workflow, job, |line| {
        !line.trim_start().starts_with('#') && line.contains("timeout-minutes:")
    })
}

fn routed_rust_job_uses_reusable_workflow(workflow: &str, job: &str) -> bool {
    routed_rust_job_block_any(workflow, job, |line| {
        line.strip_prefix("    ") == Some(ROUTED_RUST_REUSABLE_WORKFLOW_REF)
    })
}

fn routed_rust_job_block_has_with_value(workflow: &str, job: &str, value: &str) -> bool {
    let mut in_with = false;
    routed_rust_job_block_any(workflow, job, |line| {
        if line == "    with:" {
            in_with = true;
            return false;
        }
        if in_with && line.starts_with("    ") && !line.starts_with("     ") {
            in_with = false;
        }
        in_with && line.strip_prefix("      ") == Some(value)
    })
}

fn reusable_workflow_jobs(workflow: &str) -> Vec<String> {
    let mut jobs = Vec::new();
    let mut in_jobs = false;
    for line in workflow.lines() {
        if line.trim_end() == "jobs:" {
            in_jobs = true;
            continue;
        }
        if in_jobs && !line.is_empty() && !line.starts_with(' ') {
            break;
        }
        if in_jobs
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('-')
        {
            jobs.push(line.trim().trim_end_matches(':').to_string());
        }
    }
    jobs
}

#[cfg(test)]
fn routed_rust_workflow_contract_violations(
    workflow: &str,
    settings: Option<&str>,
    lane_whitelist: Option<&str>,
) -> Vec<String> {
    routed_rust_workflow_contract_violations_with_reusable(workflow, None, settings, lane_whitelist)
}

fn routed_rust_workflow_contract_violations_with_reusable(
    workflow: &str,
    reusable_workflow: Option<&str>,
    settings: Option<&str>,
    lane_whitelist: Option<&str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    let delegated_jobs: Vec<&str> = ROUTED_RUST_IMPLEMENTATION_JOBS
        .iter()
        .copied()
        .filter(|job| routed_rust_job_uses_reusable_workflow(workflow, job))
        .collect();
    let delegated = !delegated_jobs.is_empty();

    if !delegated_jobs.is_empty() && delegated_jobs.len() != ROUTED_RUST_IMPLEMENTATION_JOBS.len() {
        violations.push(format!(
            ".github/workflows/routed-rust.yml must keep all four implementation jobs inline or delegate all four to `{ROUTED_RUST_REUSABLE_WORKFLOW_PATH}`; delegated {} of 4",
            delegated_jobs.len()
        ));
    }

    let implementation_workflow = if delegated {
        match reusable_workflow {
            Some(reusable) => reusable,
            None => {
                violations.push(format!(
                    ".github/workflows/routed-rust.yml delegates implementation jobs to missing `{ROUTED_RUST_REUSABLE_WORKFLOW_PATH}`"
                ));
                ""
            }
        }
    } else {
        workflow
    };
    let implementation_copies = if delegated { 1 } else { 3 };
    let required_workflow_snippets = [
        (
            "org runner discovery",
            "orgs/EffortlessMetrics/actions/runners",
        ),
        (
            "runner read token fallback",
            "secrets.EM_RUNNER_READ_TOKEN || github.token",
        ),
        ("slurped idle runner query", "jq -s -e --arg model"),
        ("trusted fork fallback reason", "fork_or_untrusted_pr"),
        ("runner API fallback reason", "runner_api_failed"),
        ("no-idle fallback reason", "no_idle_runner"),
        (
            "runner capacity fallback reason",
            "runner_capacity_unavailable",
        ),
        ("CX43 idle route reason", "cx43_idle"),
        ("CPX42 idle route reason", "cpx42_idle"),
        ("CX53 idle route reason", "cx53_idle"),
        ("CX43 capacity label", "rust-medium"),
        ("CPX42 capacity label", "rust-16gb"),
        ("CX53 capacity label", "rust-large"),
        ("normalized result job", "name: Ripr Rust Small Result"),
        (
            "CX43 conditional implementation job",
            "if: needs.route.outputs.router_target == 'cx43'",
        ),
        (
            "CPX42 conditional implementation job",
            "if: needs.route.outputs.router_target == 'cpx42'",
        ),
        (
            "CX53 conditional implementation job",
            "if: needs.route.outputs.router_target == 'cx53'",
        ),
        (
            "hosted fallback conditional job",
            "needs.route.outputs.router_target == 'github'",
        ),
        (
            "hosted fallback docs-detection guard",
            "needs.detect-docs-only.result == 'success'",
        ),
        (
            "CX43 tempfail fallback predicate",
            "needs.rust-cx43.outputs.scratch_status == 'tempfail'",
        ),
        (
            "CPX42 tempfail fallback predicate",
            "needs.rust-cpx42.outputs.scratch_status == 'tempfail'",
        ),
        (
            "CX53 tempfail fallback predicate",
            "needs.rust-cx53.outputs.scratch_status == 'tempfail'",
        ),
        (
            "normalized tempfail fallback result",
            "disk-guard tempfailed; GitHub-hosted fallback succeeded",
        ),
        (
            "normalized docs detection failure",
            "docs-surface detection result was $DOCS_DETECT_RESULT",
        ),
    ];

    for (label, snippet) in required_workflow_snippets {
        if !workflow.contains(snippet) {
            violations.push(format!(
                ".github/workflows/routed-rust.yml is missing {label}: `{snippet}`"
            ));
        }
    }

    let scratch_tempfail_output = "scratch_status: ${{ steps.scratch.outputs.status }}";
    if !implementation_workflow.contains(scratch_tempfail_output) {
        violations.push(format!(
            "routed Rust implementation authority is missing self-hosted scratch tempfail output: `{scratch_tempfail_output}`"
        ));
    }

    if delegated {
        for (label, snippet) in [
            ("workflow_call trigger", "workflow_call:"),
            (
                "workflow_call scratch-status output",
                "      scratch_status:\n        value: ${{ jobs.rust-gates.outputs.scratch_status }}",
            ),
            ("disk-guard threshold input", "disk-guard-threshold:"),
            (
                "parameterized scratch free-space floor",
                "ci-disk-guard /mnt/ci-scratch \"${{ inputs.disk-guard-threshold }}\"",
            ),
        ] {
            if !implementation_workflow.contains(snippet) {
                violations.push(format!(
                    "{ROUTED_RUST_REUSABLE_WORKFLOW_PATH} is missing {label}: `{snippet}`"
                ));
            }
        }
        for (job, threshold) in [("rust-cx43", 35), ("rust-cpx42", 35), ("rust-cx53", 50)] {
            let value = format!("disk-guard-threshold: {threshold}");
            if !routed_rust_job_block_has_with_value(workflow, job, &value) {
                violations.push(format!(
                    ".github/workflows/routed-rust.yml delegated job `{job}` must pass `with.{value}`"
                ));
            }
        }
    } else {
        for (label, snippet) in [
            (
                "CX43/CPX42 scratch free-space floor",
                "ci-disk-guard /mnt/ci-scratch 35",
            ),
            (
                "CX53 scratch free-space floor",
                "ci-disk-guard /mnt/ci-scratch 50",
            ),
        ] {
            if !workflow.contains(snippet) {
                violations.push(format!(
                    ".github/workflows/routed-rust.yml is missing {label}: `{snippet}`"
                ));
            }
        }
    }

    let toolchain_temp_steps = implementation_workflow
        .matches("name: Prepare toolchain temp")
        .count();
    let toolchain_temp_mkdirs = implementation_workflow
        .matches("run: mkdir -p \"$TMPDIR\"")
        .count();
    if toolchain_temp_steps < implementation_copies || toolchain_temp_mkdirs < implementation_copies
    {
        violations.push(format!(
            "routed Rust implementation authority must include `Prepare toolchain temp` before setup; expected {implementation_copies} copy/copies, found {toolchain_temp_steps} step(s) and {toolchain_temp_mkdirs} mkdir command(s)"
        ));
    }

    let scratch_cargo_home =
        "CARGO_HOME: /mnt/ci-scratch/cargo-home/${{ github.run_id }}-${{ github.run_attempt }}";
    let scratch_cargo_homes = implementation_workflow.matches(scratch_cargo_home).count();
    let scratch_cargo_home_cleanups = implementation_workflow
        .matches("rm -rf \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" \"$TMPDIR\"")
        .count();
    if scratch_cargo_homes < implementation_copies
        || scratch_cargo_home_cleanups < implementation_copies
    {
        violations.push(format!(
            "routed Rust implementation authority must use scratch CARGO_HOME and clean it; expected {implementation_copies} copy/copies, found {scratch_cargo_homes} scratch home(s) and {scratch_cargo_home_cleanups} cleanup command(s)"
        ));
    }

    // Proof-routing slice 6 (docs/PROOF_ROUTING.md): every PR-evidence path must
    // emit the proof route as an advisory dry-run artifact. The command is
    // appended with `|| true` so a route-computation failure never fails the
    // lane, and it runs on all three self-hosted jobs and the hosted fallback so
    // the artifact cannot silently regress. No lane is skipped or gated by it.
    let proof_route_dry_runs = implementation_workflow
        .matches("cargo xtask proof route --base \"$BASE_SHA\" --head \"$HEAD_SHA\" || true")
        .count();
    let expected_proof_route_dry_runs = if delegated { 1 } else { 4 };
    if proof_route_dry_runs < expected_proof_route_dry_runs {
        violations.push(format!(
            "routed Rust implementation authority must emit the advisory proof-route dry-run artifact (`cargo xtask proof route --base \"$BASE_SHA\" --head \"$HEAD_SHA\" || true`); expected {expected_proof_route_dry_runs} copy/copies, found {proof_route_dry_runs}"
        ));
    }

    // Issue #2230 (PR #2228 hang): every routed-rust job carries an explicit
    // job deadline so a hung step fails the job in bounded time instead of
    // holding the required aggregate check open indefinitely. The check is
    // anchored to each named job block, not a global occurrence count: a
    // duplicate or stray `timeout-minutes:` token elsewhere cannot stand in
    // for a job that lost its deadline.
    for job in ROUTED_RUST_DEADLINE_JOBS {
        if delegated_jobs.contains(&job) {
            continue;
        }
        if !routed_rust_job_block_has_deadline(workflow, job) {
            violations.push(format!(
                ".github/workflows/routed-rust.yml job `{job}` must set an explicit `timeout-minutes` job deadline so a hung step fails in bounded time"
            ));
        }
    }
    if delegated {
        let reusable_jobs = reusable_workflow_jobs(implementation_workflow);
        if reusable_jobs.is_empty() {
            violations.push(format!(
                "{ROUTED_RUST_REUSABLE_WORKFLOW_PATH} deadline check analyzed zero `jobs:` entries"
            ));
        }
        for job in reusable_jobs
            .into_iter()
            .filter(|job| !routed_rust_job_block_has_deadline(implementation_workflow, job))
        {
            violations.push(format!(
                "{ROUTED_RUST_REUSABLE_WORKFLOW_PATH} job `{job}` must set an explicit `timeout-minutes` deadline"
            ));
        }
    }

    if workflow.contains("repos/${REPOSITORY}/actions/runners")
        || workflow.contains("repos/$REPOSITORY/actions/runners")
        || workflow.contains("repos/EffortlessMetrics/ripr-swarm/actions/runners")
    {
        violations.push(
            ".github/workflows/routed-rust.yml must use organization runner discovery, not repo-local runner discovery".to_string(),
        );
    }

    if !(workflow.contains("[ \"$EVENT_NAME\" = \"pull_request\" ]")
        && workflow.contains("[ \"$HEAD_REPO\" != \"$REPOSITORY\" ]"))
    {
        violations.push(
            ".github/workflows/routed-rust.yml must guard pull_request events from forks before selecting self-hosted runners".to_string(),
        );
    }

    for forbidden in [
        "github.event.pull_request.head.repo.full_name == github.repository",
        "github.event.pull_request.head.repo.full_name != github.repository",
    ] {
        if workflow.contains(forbidden) {
            violations.push(format!(
                ".github/workflows/routed-rust.yml must keep fork routing in the route job, not on self-hosted implementation job condition `{forbidden}`"
            ));
        }
    }

    if let Some(settings) = settings
        && (settings.contains("name: ripr-swarm") || settings.contains("Ripr Rust Small Result"))
    {
        if !settings.contains("Ripr Rust Small Result") {
            violations.push(
                ".github/settings.yml must require the normalized `Ripr Rust Small Result` check for ripr-swarm".to_string(),
            );
        }
        for forbidden in [
            "Route Ripr Rust Small",
            "Ripr Rust Small on CX43",
            "Ripr Rust Small on CPX42",
            "Ripr Rust Small on CX53",
            "Ripr Rust Small on GitHub Hosted",
        ] {
            if settings.contains(forbidden) {
                violations.push(format!(
                    ".github/settings.yml must not require conditional implementation job `{forbidden}`"
                ));
            }
        }
    }

    if let Some(lane_whitelist) = lane_whitelist
        && lane_whitelist.contains("routed-rust-small")
    {
        if !lane_whitelist.contains("workflow = \".github/workflows/routed-rust.yml\"") {
            violations.push(
                "policy/ci-lane-whitelist.toml must point `routed-rust-small` at `.github/workflows/routed-rust.yml`".to_string(),
            );
        }
        if !lane_whitelist.contains("jobs = [\"Ripr Rust Small Result\"]") {
            violations.push(
                "policy/ci-lane-whitelist.toml must list only `Ripr Rust Small Result` for the routed Rust lane".to_string(),
            );
        }
    }

    violations.sort();
    violations.dedup();
    violations
}

fn workflow_runtime_violations(
    path: &str,
    text: &str,
    allowlist: &BTreeMap<(String, String), usize>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (old_ref, new_ref) in deprecated_workflow_action_refs() {
        let count = text.matches(old_ref).count();
        if count > 0 {
            violations.push(format!(
                "{path} uses deprecated action runtime ref `{old_ref}` {count} time(s); use `{new_ref}`"
            ));
        }
    }

    if is_extension_node_workflow(path) {
        for (line_number, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if matches!(
                trimmed,
                "node-version: 20" | "node-version: '20'" | "node-version: \"20\""
            ) {
                violations.push(format!(
                    "{path}:{} uses Node 20 for extension tooling; use Node 24",
                    line_number + 1
                ));
            }
        }
    }

    for pattern in workflow_runtime_exception_patterns() {
        let count = text.matches(pattern).count();
        if count == 0 {
            continue;
        }
        let allowed = allowlist
            .get(&(path.to_string(), pattern.to_string()))
            .copied()
            .unwrap_or(0);
        if count > allowed {
            violations.push(format!(
                "{path} uses `{pattern}` {count} time(s), allowed {allowed}; add a reviewed workflow action runtime exception or upgrade the action"
            ));
        }
    }

    for ((allowed_path, pattern), allowed) in allowlist {
        if allowed_path != path {
            continue;
        }
        if !workflow_runtime_exception_patterns().contains(&pattern.as_str()) {
            violations.push(format!(
                "policy/workflow_action_runtime_allowlist.txt has unsupported exception `{pattern}` for {allowed_path}"
            ));
            continue;
        }
        let count = text.matches(pattern).count();
        if count > *allowed {
            violations.push(format!(
                "{path} uses `{pattern}` {count} time(s), allowed {allowed}"
            ));
        }
    }

    violations.sort();
    violations.dedup();
    violations
}

fn deprecated_workflow_action_refs() -> &'static [(&'static str, &'static str)] {
    &[
        ("actions/checkout@v4", "actions/checkout@v6"),
        ("actions/setup-node@v4", "actions/setup-node@v6"),
        ("actions/upload-artifact@v4", "actions/upload-artifact@v7"),
        (
            "actions/download-artifact@v4",
            "actions/download-artifact@v8",
        ),
        ("codecov/codecov-action@v4", "codecov/codecov-action@v6"),
    ]
}

fn workflow_runtime_exception_patterns() -> &'static [&'static str] {
    &["actions/dependency-review-action@v4"]
}

fn is_extension_node_workflow(path: &str) -> bool {
    matches!(
        path,
        ".github/workflows/ci.yml" | ".github/workflows/publish-extension.yml"
    )
}

fn check_spec_format() -> Result<(), String> {
    let mut violations = Vec::new();
    let spec_dir = Path::new("docs/specs");
    for path in collect_files(spec_dir)? {
        let normalized = normalize_path(&path);
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !file_name.starts_with("RIPR-SPEC-") || !file_name.ends_with(".md") {
            continue;
        }
        let Some(spec_id) = spec_id_from_file_name(file_name) else {
            violations.push(format!("{normalized} has invalid RIPR-SPEC filename"));
            continue;
        };
        let text = read_text_lossy(&path)?;
        let first_line = text.lines().next().unwrap_or_default();
        if !first_line.starts_with(&format!("# {spec_id}: ")) {
            violations.push(format!(
                "{normalized}:1 title must start with `# {spec_id}: `"
            ));
        }
        let status = spec_status(&text);
        match status.as_deref() {
            Some("proposed" | "planned" | "accepted" | "deprecated") => {}
            Some(value) => violations.push(format!("{normalized} has invalid status `{value}`")),
            None => violations.push(format!("{normalized} is missing `Status: ...`")),
        }
        for heading in required_spec_headings() {
            if !has_markdown_heading(&text, heading) {
                violations.push(format!("{normalized} is missing `{heading}`"));
            }
        }
        if text.contains("- \n") {
            violations.push(format!(
                "{normalized} contains empty placeholder list items"
            ));
        }
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "spec-format.md",
            check: "check-spec-format",
            why_it_matters: "Specs are the behavior contracts that let humans and agents trace intent to tests, code, outputs, and metrics.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Update the spec to match docs/SPEC_FORMAT.md.",
                "Use docs/templates/SPEC_TEMPLATE.md for new behavior specs.",
                "Keep planned specs explicit when implementation mapping is not available yet.",
            ],
            rerun_command: "cargo xtask check-spec-format",
            exception_template: None,
        },
        &violations,
    )
}


#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "xtask test code uses unwrap/expect for fail-fast assertion. Production paths are receipted via policy/no-panic-allowlist.toml; the test scope is governed by this single module-level expect."
)]
mod tests;


#[cfg(test)]
mod inherited_failure_tests {
    use super::*;
    #[test]
    fn failure_report_preserves_not_proven_baseline_state() {
        let report = check_pr_report(Some(&CheckPrGateFailure {
            name: "clippy".to_string(),
            reproduce: "cargo clippy".to_string(),
            bounded_error: "error".to_string(),
            not_run: vec![],
            baseline: BaselineFailureComparison {
                status: "NOT_PROVEN",
                detail: "unavailable".to_string(),
            },
        }));
        assert!(report.contains("Inherited-failure comparison"));
        assert!(report.contains("Status: NOT_PROVEN"));
    }
}
