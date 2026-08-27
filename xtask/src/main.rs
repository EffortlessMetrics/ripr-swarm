Warning: truncated output (original token count: 180579)
Total output lines: 20296

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
            .is_…130579 tokens truncated…minator(packet: &Value) -> Option<String> {
    pr_string_path(packet, &["missing_discriminator"])
        .or_else(|| {
            audit_get(packet, &["missing_discriminators"])
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| pr_string_path(item, &["value"]))
        })
        .or_else(|| pr_string_path(packet, &["candidate_value_or_observer"]))
        .or_else(|| pr_string_path(packet, &["assertion_shape"]))
}

fn pr_packet_focused_proof_intent(packet: &Value) -> Option<String> {
    pr_string_path(packet, &["focused_proof_intent"])
        .or_else(|| pr_string_path(packet, &["recommended_repair"]))
        .or_else(|| {
            pr_string_path(packet, &["assertion_shape"])
                .map(|assertion| format!("Add or strengthen `{assertion}`."))
        })
}

fn pr_related_test_summary(packet: &Value) -> Option<String> {
    let related = audit_get(packet, &["related_test_or_observer"])?;
    let file = pr_string_path(related, &["file"])?;
    let name = pr_string_path(related, &["name"]);
    let line = pr_usize_path(related, &["line"]);
    Some(match (name, line) {
        (Some(name), Some(line)) => format!("`{}:{line}` `{}`", md_escape(&file), md_escape(&name)),
        (Some(name), None) => format!("`{}` `{}`", md_escape(&file), md_escape(&name)),
        (None, Some(line)) => format!("`{}:{line}`", md_escape(&file)),
        (None, None) => format!("`{}`", md_escape(&file)),
    })
}

fn pr_actionable_packets(report: &Value) -> Vec<&Value> {
    audit_get(report, &["packets"])
        .and_then(Value::as_array)
        .map(|packets| {
            packets
                .iter()
                .filter(|packet| {
                    pr_string_path(packet, &["gap_state"]).as_deref() == Some("actionable")
                        && pr_bool_path(packet, &["public_projection_eligible"]).unwrap_or(true)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn pr_changed_path_set(changes: &[ChangedPath]) -> BTreeSet<String> {
    changes
        .iter()
        .map(|change| normalize_slashes(&change.path))
        .collect()
}

fn pr_packet_touches_changed_path(packet: &Value, changed_paths: &BTreeSet<String>) -> bool {
    if changed_paths.is_empty() {
        return false;
    }
    pr_packet_paths(packet)
        .iter()
        .any(|path| changed_paths.contains(path))
}

fn pr_packet_paths(packet: &Value) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for path in [
        pr_string_path(packet, &["source_file"]),
        pr_string_path(packet, &["primary_anchor", "file"]),
        pr_string_path(packet, &["related_test_or_observer", "file"]),
    ]
    .into_iter()
    .flatten()
    {
        paths.insert(normalize_slashes(&path));
    }
    if let Some(raw_findings) = audit_get(packet, &["raw_findings"]).and_then(Value::as_array) {
        for raw in raw_findings {
            if let Some(path) = pr_string_path(raw, &["file"]) {
                paths.insert(normalize_slashes(&path));
            }
        }
    }
    paths
}

fn pr_packet_static_limitation_count(packet: &Value) -> usize {
    audit_get(packet, &["static_limitations"])
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn pr_usize_path(value: &Value, path: &[&str]) -> Option<usize> {
    audit_get(value, path).and_then(json_scalar_as_usize)
}

fn pr_string_path(value: &Value, path: &[&str]) -> Option<String> {
    audit_get(value, path).and_then(json_scalar_as_string)
}

fn pr_bool_path(value: &Value, path: &[&str]) -> Option<bool> {
    audit_get(value, path).and_then(Value::as_bool)
}

fn report_index_markdown(
    status: &str,
    reports: &[ReportIndexEntry],
    receipts: &[ReportIndexEntry],
    missing: &[String],
    next_commands: &[String],
) -> String {
    let mut body = format!("# ripr report index\n\nStatus: {status}\n\n");
    body.push_str("This is the reviewer front door for generated `ripr` artifacts.\n\n");

    body.push_str("\n## Summary\n\n");
    body.push_str(&format!("- available reports: {}\n", reports.len()));
    body.push_str(&format!("- available receipts: {}\n", receipts.len()));
    body.push_str(&format!("- missing expected reports: {}\n", missing.len()));
    body.push_str(&format!(
        "- failed reports: {}\n",
        reports
            .iter()
            .filter(|entry| entry.status == "fail")
            .count()
    ));
    body.push_str(&format!(
        "- warning reports: {}\n",
        reports
            .iter()
            .filter(|entry| entry.status == "warn")
            .count()
    ));
    let lane1_packets = report_index_lane1_readiness_packets(reports);
    body.push_str(&format!(
        "- lane1 readiness status: `{}`\n",
        report_index_lane1_overall_status(&lane1_packets)
    ));
    body.push_str(&format!(
        "- missing lane1 readiness artifacts: {}\n",
        report_index_missing_artifact_count(&lane1_packets)
    ));

    body.push_str("\n## Suggested Reviewer Path\n\n");
    body.push_str("1. Read `target/ripr/reports/pr-summary.md`.\n");
    body.push_str("2. Read `target/ripr/reports/critic.md`, if present.\n");
    body.push_str("3. Inspect `target/ripr/reports/fixtures.md` and `target/ripr/reports/goldens.md` when fixtures or output changed.\n");
    body.push_str(
        "4. Inspect `target/ripr/reports/golden-drift.md`, if present and output changed.\n",
    );
    body.push_str("5. Inspect `target/ripr/receipts/check-pr.json`, when receipts exist.\n");

    body.push_str("\n## Key Report Status\n\n");
    for file in [
        "pr-summary.md",
        "check-pr.md",
        "pr-shape.md",
        "fixtures.md",
        "goldens.md",
        "golden-drift.md",
        "allow-attributes.md",
        "local-context.md",
        "test-oracles.md",
        "dogfood.md",
        "metrics.md",
    ] {
        body.push_str(&format!(
            "- `{file}`: {}\n",
            status_for_report(reports, file)
        ));
    }

    body.push_str("\n## Lane 1 Evidence Readiness\n\n");
    body.push_str(
        "| Artifact group | Status | Artifacts | Next command |\n| --- | --- | --- | --- |\n",
    );
    for packet in &lane1_packets {
        body.push_str(&format!(
            "| {} | `{}` | {} | `{}` |\n",
            markdown_cell(packet.label),
            markdown_cell(&packet.status),
            markdown_cell(&repo_ops_artifacts_markdown(&packet.artifacts)),
            markdown_cell(packet.command)
        ));
    }

    let repo_ops_packets = report_index_repo_ops_packets(reports, receipts);
    body.push_str("\n## Repo-Ops Packets\n\n");
    body.push_str("| Packet | Status | Artifacts | Next command |\n| --- | --- | --- | --- |\n");
    for packet in &repo_ops_packets {
        body.push_str(&format!(
            "| {} | `{}` | {} | `{}` |\n",
            markdown_cell(packet.label),
            markdown_cell(&packet.status),
            markdown_cell(&repo_ops_artifacts_markdown(&packet.artifacts)),
            markdown_cell(packet.command)
        ));
    }

    body.push_str("\n## Available Reports\n\n");
    if reports.is_empty() {
        body.push_str("- None detected.\n");
    } else {
        body.push_str("| Report | Status |\n| --- | --- |\n");
        for entry in reports {
            body.push_str(&format!(
                "| `{}` | `{}` |\n",
                markdown_cell(&entry.path),
                markdown_cell(&entry.status)
            ));
        }
    }

    body.push_str("\n## Missing Expected Reports\n\n");
    write_path_list(&mut body, missing);

    body.push_str("\n## Receipts\n\n");
    if receipts.is_empty() {
        body.push_str("- None detected.\n");
    } else {
        for receipt in receipts {
            body.push_str(&format!("- `{}`\n", receipt.path));
        }
    }

    body.push_str("\n## Suggested Next Commands\n\n");
    for command in next_commands {
        body.push_str(&format!("- `{command}`\n"));
    }

    body
}

fn report_index_json(
    status: &str,
    reports: &[ReportIndexEntry],
    receipts: &[ReportIndexEntry],
    missing: &[String],
    next_commands: &[String],
) -> String {
    let mut body = String::from("{\n");
    body.push_str("  \"schema_version\": \"0.1\",\n");
    body.push_str(&format!("  \"status\": \"{}\",\n", json_escape(status)));
    body.push_str("  \"reports\": [\n");
    write_report_index_entry_array(&mut body, reports);
    body.push_str("  ],\n");
    body.push_str("  \"receipts\": [\n");
    write_report_index_entry_array(&mut body, receipts);
    body.push_str("  ],\n");
    body.push_str("  \"repo_ops_packets\": [\n");
    write_report_index_repo_ops_packet_array(
        &mut body,
        &report_index_repo_ops_packets(reports, receipts),
    );
    body.push_str("  ],\n");
    let lane1_packets = report_index_lane1_readiness_packets(reports);
    body.push_str("  \"lane1_readiness\": {\n");
    body.push_str(&format!(
        "    \"status\": \"{}\",\n",
        json_escape(&report_index_lane1_overall_status(&lane1_packets))
    ));
    body.push_str(&format!(
        "    \"missing_artifacts\": {},\n",
        report_index_missing_artifact_count(&lane1_packets)
    ));
    body.push_str(&format!(
        "    \"warning_artifacts\": {},\n",
        report_index_warning_artifact_count(&lane1_packets)
    ));
    body.push_str(&format!(
        "    \"failing_artifacts\": {},\n",
        report_index_failing_artifact_count(&lane1_packets)
    ));
    body.push_str("    \"packets\": [\n");
    write_report_index_repo_ops_packet_array(&mut body, &lane1_packets);
    body.push_str("    ]\n");
    body.push_str("  },\n");
    body.push_str("  \"missing_expected_reports\": [");
    write_json_string_array(&mut body, missing);
    body.push_str("],\n");
    body.push_str("  \"suggested_next_commands\": [");
    write_json_string_array(&mut body, next_commands);
    body.push_str("]\n");
    body.push_str("}\n");
    body
}

fn repo_ops_artifacts_markdown(artifacts: &[ReportIndexRepoOpsArtifact]) -> String {
    artifacts
        .iter()
        .map(|artifact| format!("{} ({})", artifact.path, artifact.status))
        .collect::<Vec<_>>()
        .join("<br>")
}

fn write_report_index_entry_array(body: &mut String, entries: &[ReportIndexEntry]) {
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str("    {\n");
        body.push_str(&format!(
            "      \"file\": \"{}\",\n",
            json_escape(&entry.file)
        ));
        body.push_str(&format!(
            "      \"path\": \"{}\",\n",
            json_escape(&entry.path)
        ));
        body.push_str(&format!(
            "      \"status\": \"{}\"\n",
            json_escape(&entry.status)
        ));
        body.push_str("    }");
    }
    if !entries.is_empty() {
        body.push('\n');
    }
}

fn write_report_index_repo_ops_packet_array(
    body: &mut String,
    packets: &[ReportIndexRepoOpsPacket],
) {
    for (index, packet) in packets.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str("    {\n");
        body.push_str(&format!("      \"id\": \"{}\",\n", json_escape(packet.id)));
        body.push_str(&format!(
            "      \"label\": \"{}\",\n",
            json_escape(packet.label)
        ));
        body.push_str(&format!(
            "      \"status\": \"{}\",\n",
            json_escape(&packet.status)
        ));
        body.push_str(&format!(
            "      \"next_command\": \"{}\",\n",
            json_escape(packet.command)
        ));
        body.push_str(&format!(
            "      \"description\": \"{}\",\n",
            json_escape(packet.description)
        ));
        body.push_str("      \"artifacts\": [\n");
        for (artifact_index, artifact) in packet.artifacts.iter().enumerate() {
            if artifact_index > 0 {
                body.push_str(",\n");
            }
            body.push_str("        {\n");
            body.push_str(&format!(
                "          \"path\": \"{}\",\n",
                json_escape(&artifact.path)
            ));
            body.push_str(&format!(
                "          \"status\": \"{}\",\n",
                json_escape(&artifact.status)
            ));
            body.push_str(&format!(
                "          \"available\": {}\n",
                artifact.available
            ));
            body.push_str("        }");
        }
        if !packet.artifacts.is_empty() {
            body.push('\n');
        }
        body.push_str("      ]\n");
        body.push_str("    }");
    }
    if !packets.is_empty() {
        body.push('\n');
    }
}

fn status_for_report(reports: &[ReportIndexEntry], file: &str) -> String {
    reports
        .iter()
        .find(|entry| entry.file == file)
        .map(|entry| entry.status.clone())
        .unwrap_or_else(|| "missing".to_string())
}

fn pr_shape_warnings(changes: &[ChangedPath]) -> Vec<String> {
    let mut warnings = Vec::new();
    let has_production = changes
        .iter()
        .any(|change| is_production_path(&change.path));
    let has_evidence = changes
        .iter()
        .any(|change| is_evidence_path(&change.path) && !is_production_path(&change.path));
    if has_production && !has_evidence {
        warnings.push(
            "Production code changed without an evidence/support file. Add or update a spec, test, fixture, golden, metric, or doc, or explain why this is a pure refactor."
                .to_string(),
        );
    }

    let analysis_changed = changes
        .iter()
        .any(|change| change.path.starts_with("crates/ripr/src/analysis/"));
    let analysis_evidence = changes.iter().any(|change| {
        is_spec_path(&change.path)
            || is_test_path(&change.path)
            || is_fixture_path(&change.path)
            || change.path.starts_with("metrics/")
            || change.path == ".ripr/traceability.toml"
    });
    if analysis_changed && !analysis_evidence {
        warnings.push(
            "Analysis code changed without spec/test/fixture/metric/traceability evidence. Analyzer PRs should carry behavior evidence unless this is a narrow mechanical refactor."
                .to_string(),
        );
    }

    let output_changed = changes.iter().any(|change| {
        change.path.starts_with("crates/ripr/src/output/")
            || change.path.starts_with("crates/ripr/src/domain/")
            || change.path == "crates/ripr/src/lsp.rs"
    });
    let output_evidence = changes.iter().any(|change| {
        change.path == "docs/OUTPUT_SCHEMA.md"
            || change.path == "policy/output_contracts.txt"
            || is_golden_path(&change.path)
            || is_fixture_path(&change.path)
    });
    if output_changed && !output_evidence {
        warnings.push(
            "Output-facing code changed without output schema, contract registry, fixture, or golden evidence. Add the matching output evidence or explain why output is unchanged."
                .to_string(),
        );
    }

    let policy_changed = changes.iter().any(|change| is_policy_path(&change.path));
    let policy_docs_changed = changes.iter().any(|change| {
        matches!(
            change.path.as_str(),
            "AGENTS.md" | "CONTRIBUTING.md" | "README.md" | "docs/CI.md" | "docs/PR_AUTOMATION.md"
        )
    });
    if policy_changed && !policy_docs_changed {
        warnings.push(
            "Policy metadata changed without front-door process docs. Update AGENTS, CONTRIBUTING, README, docs/CI.md, or docs/PR_AUTOMATION.md when policy behavior changes."
                .to_string(),
        );
    }

    let xtask_changed = changes
        .iter()
        .any(|change| change.path.starts_with("xtask/"));
    let xtask_docs_changed = changes.iter().any(|change| {
        matches!(
            change.path.as_str(),
            "AGENTS.md"
                | "CONTRIBUTING.md"
                | "README.md"
                | "docs/CI.md"
                | "docs/PR_AUTOMATION.md"
                | "docs/TESTING.md"
        )
    });
    if xtask_changed && !xtask_docs_changed {
        warnings.push(
            "xtask behavior changed without command/process docs. Update the relevant front-door docs or explain why the command surface is unchanged."
                .to_string(),
        );
    }

    warnings
}

fn pr_shape_report_body(warnings: &[String]) -> String {
    let status = if warnings.is_empty() { "pass" } else { "warn" };
    let mut body = format!("# ripr PR shape report\n\nStatus: {status}\n\n");
    body.push_str(
        "This report is advisory. It highlights likely missing evidence before review.\n\n",
    );
    body.push_str("## Warnings\n\n");
    if warnings.is_empty() {
        body.push_str("None detected.\n");
    } else {
        for warning in warnings {
            body.push_str("```text\n");
            body.push_str(warning);
            body.push_str("\n```\n\n");
        }
    }
    body
}

fn critic_findings(
    changes: &[ChangedPath],
    reports: &[ReportIndexEntry],
    receipts: &[ReportIndexEntry],
) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    let analysis_changed = changes.iter().any(|change| is_analysis_path(&change.path));
    let analysis_evidence = changes.iter().any(|change| {
        is_spec_path(&change.path)
            || is_test_path(&change.path)
            || is_fixture_path(&change.path)
            || is_golden_path(&change.path)
            || change.path.starts_with("metrics/")
            || change.path == ".ripr/traceability.toml"
    });
    if analysis_changed && !analysis_evidence {
        findings.push(CriticFinding {
            id: "analysis_without_behavior_evidence",
            severity: "warn",
            message: "Analyzer code changed without spec, test, fixture, golden, metric, or traceability evidence.",
            evidence: paths_matching(changes, is_analysis_path),
            recommended_action:
                "Add focused behavior evidence or document why this is a mechanical refactor.",
        });
    }
    if analysis_changed && missing_or_bad_report(reports, "fixtures.md") {
        findings.push(CriticFinding {
            id: "analysis_missing_fixture_report",
            severity: "warn",
            message: "Analyzer code changed without a passing fixture report in target/ripr/reports.",
            evidence: vec![format_report_status(reports, "fixtures.md")],
            recommended_action: "Run `cargo xtask fixtures` before review.",
        });
    }
    if analysis_changed && missing_or_bad_report(reports, "goldens.md") {
        findings.push(CriticFinding {
            id: "analysis_missing_golden_report",
            severity: "warn",
            message: "Analyzer code changed without a passing golden report in target/ripr/reports.",
            evidence: vec![format_report_status(reports, "goldens.md")],
            recommended_action: "Run `cargo xtask goldens check` before review.",
        });
    }

    let output_changed = changes
        .iter()
        .any(|change| is_output_surface_path(&change.path));
    let output_evidence = changes.iter().any(|change| {
        change.path == "docs/OUTPUT_SCHEMA.md"
            || change.path == "policy/output_contracts.txt"
            || is_fixture_path(&change.path)
            || is_golden_path(&change.path)
    });
    if output_changed && !output_evidence {
        findings.push(CriticFinding {
            id: "output_without_contract_or_golden_evidence",
            severity: "warn",
            message: "Output-facing code changed without output schema, contract, fixture, or golden evidence.",
            evidence: paths_matching(changes, is_output_surface_path),
            recommended_action:
                "Add output-contract and fixture/golden evidence, or document why rendered output is unchanged.",
        });
    }
    if output_changed && missing_or_bad_report(reports, "output-contracts.md") {
        findings.push(CriticFinding {
            id: "output_missing_contract_report",
            severity: "warn",
            message: "Output-facing code changed without a passing output-contract report.",
            evidence: vec![format_report_status(reports, "output-contracts.md")],
            recommended_action: "Run `cargo xtask check-output-contracts` before review.",
        });
    }
    if output_changed && missing_or_bad_report(reports, "golden-drift.md") {
        findings.push(CriticFinding {
            id: "output_missing_golden_drift_report",
            severity: "warn",
            message: "Output-facing code changed without a semantic golden-drift report.",
            evidence: vec![format_report_status(reports, "golden-drift.md")],
            recommended_action: "Run `cargo xtask golden-drift` before review.",
        });
    }
    if output_changed
        && !changes
            .iter()
            .any(|change| change.path == "policy/output_contracts.txt")
    {
        findings.push(CriticFinding {
            id: "public_output_terms_without_registry_update",
            severity: "warn",
            message: "Public output surface changed without an output contract registry update.",
            evidence: paths_matching(changes, is_output_surface_path),
            recommended_action:
                "Confirm no public output terms changed, or update `policy/output_contracts.txt`.",
        });
    }

    let capability_docs_changed = changes
        .iter()
        .any(|change| change.path == "docs/CAPABILITY_MATRIX.md");
    let capability_metrics_changed = changes
        .iter()
        .any(|change| change.path == "metrics/capabilities.toml");
    if capability_docs_changed && !capability_metrics_changed {
        findings.push(CriticFinding {
            id: "capability_docs_without_metrics",
            severity: "warn",
            message: "Capability matrix changed without machine-readable capability metrics.",
            evidence: paths_matching(changes, |path| path == "docs/CAPABILITY_MATRIX.md"),
            recommended_action:
                "Update `metrics/capabilities.toml` or document why the change is prose-only.",
        });
    }
    if (capability_docs_changed || capability_metrics_changed)
        && missing_or_bad_report(reports, "capabilities.md")
    {
        findings.push(CriticFinding {
            id: "capability_missing_report",
            severity: "warn",
            message: "Capability state changed without a passing capability report.",
            evidence: vec![format_report_status(reports, "capabilities.md")],
            recommended_action: "Run `cargo xtask check-capabilities` before review.",
        });
    }

    let missing_blessings = golden_changes_without_blessing(changes);
    if !missing_blessings.is_empty() {
        findings.push(CriticFinding {
            id: "golden_changed_without_blessing_reason",
            severity: "warn",
            message: "Fixture expected output changed without a matching blessing reason changelog.",
            evidence: missing_blessings,
            recommended_action:
                "Record the intentional output change in the fixture expected-output changelog.",
        });
    }

    let policy_changed = changes.iter().any(|change| is_policy_path(&change.path));
    let process_docs_changed = changes.iter().any(|change| {
        matches!(
            change.path.as_str(),
            "AGENTS.md" | "CONTRIBUTING.md" | "README.md" | "docs/CI.md" | "docs/PR_AUTOMATION.md"
        )
    });
    if policy_changed && !process_docs_changed {
        findings.push(CriticFinding {
            id: "policy_without_process_docs",
            severity: "warn",
            message: "Policy files or workflows changed without process documentation.",
            evidence: paths_matching(changes, is_policy_path),
            recommended_action:
                "Update front-door process docs or document why behavior did not change.",
        });
    }

    let extension_changed = changes
        .iter()
        .any(|change| change.path.starts_with("editors/vscode/"));
    if extension_changed {
        findings.push(CriticFinding {
            id: "extension_requires_package_evidence",
            severity: "warn",
            message: "VS Code extension files changed; local xtask reports do not prove npm compile/package evidence.",
            evidence: paths_matching(changes, |path| path.starts_with("editors/vscode/")),
            recommended_action: "Verify `npm run compile` and `npm run package`, or inspect the CI vscode job.",
        });
    }

    if missing_or_bad_report(reports, "pr-summary.md") {
        findings.push(CriticFinding {
            id: "missing_pr_summary",
            severity: "warn",
            message: "The reviewer packet is missing a PR summary report.",
            evidence: vec![format_report_status(reports, "pr-summary.md")],
            recommended_action: "Run `cargo xtask pr-summary` before review.",
        });
    }
    if missing_or_bad_report(reports, "pr-shape.md") {
        findings.push(CriticFinding {
            id: "missing_pr_shape_report",
            severity: "warn",
            message: "The advisory PR shape report is missing or not passing.",
            evidence: vec![format_report_status(reports, "pr-shape.md")],
            recommended_action: "Run `cargo xtask check-pr-shape` before review.",
        });
    }
    if receipts.is_empty() {
        findings.push(CriticFinding {
            id: "missing_receipts",
            severity: "warn",
            message: "No machine-readable receipts were found for this reviewer packet.",
            evidence: vec!["target/ripr/receipts: missing or empty".to_string()],
            recommended_action: "Run `cargo xtask receipts` and `cargo xtask receipts check`.",
        });
    }

    findings
}

fn missing_or_bad_report(reports: &[ReportIndexEntry], file: &str) -> bool {
    !matches!(
        status_for_report(reports, file).as_str(),
        "pass" | "present"
    )
}

fn format_report_status(reports: &[ReportIndexEntry], file: &str) -> String {
    format!(
        "target/ripr/reports/{file}: {}",
        status_for_report(reports, file)
    )
}

fn golden_changes_without_blessing(changes: &[ChangedPath]) -> Vec<String> {
    let changed_paths = changes
        .iter()
        .map(|change| change.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut missing = Vec::new();
    for change in changes.iter().filter(|change| is_golden_path(&change.path)) {
        let Some(fixture) = fixture_name_from_expected_output(&change.path) else {
            continue;
        };
        let changelog = format!("fixtures/{fixture}/expected/CHANGELOG.md");
        if !changed_paths.contains(changelog.as_str()) {
            missing.push(format!(
                "{} changed without `{changelog}`",
                format_changed_path(change)
            ));
        }
    }
    missing.sort();
    missing.dedup();
    missing
}

fn fixture_name_from_expected_output(path: &str) -> Option<String> {
    let rest = path.strip_prefix("fixtures/")?;
    let (fixture, after_fixture) = rest.split_once('/')?;
    if after_fixture.starts_with("expected/") && after_fixture != "expected/CHANGELOG.md" {
        Some(fixture.to_string())
    } else {
        None
    }
}

fn critic_markdown(
    findings: &[CriticFinding],
    reports: &[ReportIndexEntry],
    receipts: &[ReportIndexEntry],
) -> String {
    let status = if findings.is_empty() { "pass" } else { "warn" };
    let mut body = format!("# ripr critic report\n\nStatus: {status}\n\n");
    body.push_str("Mode: advisory\n\n");
    body.push_str("This report is a deterministic adversarial review packet. It flags likely missing evidence from the current diff, reports, and receipts. It does not block CI.\n\n");

    body.push_str("## Findings\n\n");
    if findings.is_empty() {
        body.push_str("None detected.\n\n");
    } else {
        for finding in findings {
            body.push_str(&format!(
                "### {} ({})\n\n{}\n\n",
                finding.id, finding.severity, finding.message
            ));
            body.push_str("Evidence:\n\n");
            write_path_list(&mut body, &finding.evidence);
            body.push_str("\nRecommended action:\n\n```text\n");
            body.push_str(finding.recommended_action);
            body.push_str("\n```\n\n");
        }
    }

    body.push_str("## Inputs\n\n");
    body.push_str(&format!("- reports available: {}\n", reports.len()));
    body.push_str(&format!("- receipts available: {}\n\n", receipts.len()));
    body.push_str("## Next Commands\n\n");
    body.push_str("```bash\n");
    body.push_str("cargo xtask pr-summary\n");
    body.push_str("cargo xtask reports index\n");
    body.push_str("cargo xtask receipts\n");
    body.push_str("cargo xtask receipts check\n");
    body.push_str("```\n");
    body
}

fn critic_json(
    findings: &[CriticFinding],
    reports: &[ReportIndexEntry],
    receipts: &[ReportIndexEntry],
) -> String {
    let status = if findings.is_empty() { "pass" } else { "warn" };
    let mut body = String::new();
    body.push_str("{\n");
    body.push_str("  \"schema_version\": \"0.1\",\n");
    body.push_str("  \"mode\": \"advisory\",\n");
    body.push_str(&format!("  \"status\": \"{status}\",\n"));
    body.push_str(&format!("  \"reports_available\": {},\n", reports.len()));
    body.push_str(&format!("  \"receipts_available\": {},\n", receipts.len()));
    body.push_str("  \"findings\": [\n");
    for (index, finding) in findings.iter().enumerate() {
        if index > 0 {
            body.push_str(",\n");
        }
        body.push_str("    {\n");
        body.push_str(&format!("      \"id\": \"{}\",\n", json_escape(finding.id)));
        body.push_str(&format!(
            "      \"severity\": \"{}\",\n",
            json_escape(finding.severity)
        ));
        body.push_str(&format!(
            "      \"message\": \"{}\",\n",
            json_escape(finding.message)
        ));
        body.push_str("      \"evidence\": [");
        write_json_string_array(&mut body, &finding.evidence);
        body.push_str("],\n");
        body.push_str(&format!(
            "      \"recommended_action\": \"{}\"\n",
            json_escape(finding.recommended_action)
        ));
        body.push_str("    }");
    }
    if !findings.is_empty() {
        body.push('\n');
    }
    body.push_str("  ]\n");
    body.push_str("}\n");
    body
}

fn write_path_list(body: &mut String, paths: &[String]) {
    if paths.is_empty() {
        body.push_str("- None detected.\n");
        return;
    }
    for path in paths {
        body.push_str(&format!("- `{path}`\n"));
    }
}

fn paths_matching(changes: &[ChangedPath], predicate: fn(&str) -> bool) -> Vec<String> {
    changes
        .iter()
        .filter(|change| predicate(&change.path))
        .map(format_changed_path)
        .collect()
}

fn detected_surface_rows(changes: &[ChangedPath]) -> Vec<(&'static str, Vec<String>)> {
    vec![
        (
            "Rust product code",
            paths_matching(changes, |path| path.starts_with("crates/ripr/src/")),
        ),
        (
            "Rust tests",
            paths_matching(changes, |path| path.starts_with("crates/ripr/tests/")),
        ),
        (
            "Automation/tooling",
            paths_matching(changes, |path| path.starts_with("xtask/")),
        ),
        (
            "Fixtures",
            paths_matching(changes, |path| path.starts_with("fixtures/")),
        ),
        (
            "Goldens",
            paths_matching(changes, |path| {
                path.contains("/expected/") || path.contains("/golden")
            }),
        ),
        (
            "Docs",
            paths_matching(changes, |path| {
                path.starts_with("docs/")
                    || is_plan_path(path)
                    || matches!(
                        path,
                        "README.md" | "AGENTS.md" | "CONTRIBUTING.md" | "CHANGELOG.md"
                    )
            }),
        ),
        (
            "Policies",
            paths_matching(changes, |path| {
                path.starts_with("policy/") || path.starts_with(".ripr/")
            }),
        ),
        (
            "Workflows",
            paths_matching(changes, |path| path.starts_with(".github/")),
        ),
        (
            "Extension",
            paths_matching(changes, |path| path.starts_with("editors/vscode/")),
        ),
    ]
}

fn is_human_output_path(path: &str) -> bool {
    path == "crates/ripr/src/output/human.rs" || path.starts_with("crates/ripr/src/output/human/")
}

fn public_contract_rows(changes: &[ChangedPath]) -> Vec<(&'static str, Vec<String>)> {
    vec![
        (
            "CLI",
            paths_matching(changes, |path| {
                path.starts_with("crates/ripr/src/cli/")
                    || path == "crates/ripr/src/main.rs"
                    || path.starts_with("docs/reference/cli")
            }),
        ),
        (
            "JSON",
            paths_matching(changes, |path| {
                path == "crates/ripr/src/output/json.rs"
                    || path.starts_with("crates/ripr/src/output/json/")
                    || path == "docs/OUTPUT_SCHEMA.md"
            }),
        ),
        (
            "Human output",
            paths_matching(changes, is_human_output_path),
        ),
        (
            "LSP",
            paths_matching(changes, |path| {
                path == "crates/ripr/src/lsp.rs" || path.starts_with("editors/vscode/")
            }),
        ),
        (
            "GitHub/SARIF",
            paths_matching(changes, |path| {
                path == "crates/ripr/src/output/github.rs"
                    || path.to_ascii_lowercase().contains("sarif")
            }),
        ),
        (
            "Config",
            paths_matching(changes, |path| {
                path == "ripr.toml.example" || path.contains("config") || path.contains("ripr-toml")
            }),
        ),
        (
            "Docs",
            paths_matching(changes, |path| {
                path.starts_with("docs/")
                    || is_plan_path(path)
                    || matches!(
                        path,
                        "README.md" | "AGENTS.md" | "CONTRIBUTING.md" | "CHANGELOG.md"
                    )
            }),
        ),
    ]
}

fn policy_exception_rows(changes: &[ChangedPath]) -> Vec<(&'static str, Vec<String>)> {
    vec![
        (
            "Non-Rust files",
            paths_matching(changes, |path| {
                is_file_policy_candidate(path) && !path.ends_with(".rs")
            }),
        ),
        (
            "Executable files",
            paths_matching(changes, |path| path == "policy/executable_allowlist.txt"),
        ),
        (
            "Panic-family allowlist",
            paths_matching(changes, |path| path == ".ripr/no-panic-allowlist.txt"),
        ),
        (
            "Static-language allowlist",
            paths_matching(changes, |path| {
                path == STATIC_LANGUAGE_ALLOWLIST_PATH
                    || path == STATIC_LANGUAGE_ALLOWLIST_LEGACY_PATH
            }),
        ),
        (
            "Workflow budget",
            paths_matching(changes, |path| path == "policy/workflow_allowlist.txt"),
        ),
        (
            "Dependencies",
            paths_matching(changes, |path| {
                path == "policy/dependency_allowlist.txt" || is_dependency_surface_candidate(path)
            }),
        ),
    ]
}

fn reviewer_focus(changes: &[ChangedPath]) -> Vec<String> {
    let mut focus = Vec::new();
    for predicate in [
        is_production_path as fn(&str) -> bool,
        is_test_path,
        is_spec_path,
        is_fixture_path,
        is_golden_path,
        is_automation_path,
        is_policy_path,
    ] {
        for path in paths_matching(changes, predicate) {
            let raw_path = strip_status_suffix(&path).to_string();
            if !focus.contains(&raw_path) {
                focus.push(raw_path);
            }
            if focus.len() >= 8 {
                return focus;
            }
        }
    }
    focus
}

fn is_production_path(path: &str) -> bool {
    path.starts_with("crates/ripr/src/") || path.starts_with("editors/vscode/src/")
}

fn is_evidence_path(path: &str) -> bool {
    is_test_path(path)
        || is_spec_path(path)
        || is_fixture_path(path)
        || is_golden_path(path)
        || is_automation_path(path)
        || is_policy_path(path)
        || is_plan_path(path)
        || path.starts_with("docs/")
        || path.starts_with("metrics/")
        || matches!(
            path,
            "README.md" | "AGENTS.md" | "CONTRIBUTING.md" | "CHANGELOG.md"
        )
}

fn is_test_path(path: &str) -> bool {
    path.starts_with("crates/ripr/tests/") || path.contains("/tests/")
}

fn is_spec_path(path: &str) -> bool {
    path.starts_with("docs/specs/") || path == "docs/SPEC_FORMAT.md"
}

fn is_fixture_path(path: &str) -> bool {
    path.starts_with("fixtures/")
}

fn is_golden_path(path: &str) -> bool {
    path.contains("/expected/") || path.contains("/golden")
}

fn is_automation_path(path: &str) -> bool {
    path.starts_with("xtask/")
}

fn is_policy_path(path: &str) -> bool {
    path.starts_with("policy/") || path.starts_with(".ripr/") || path.starts_with(".github/")
}

fn format_changed_path(change: &ChangedPath) -> String {
    let status = change
        .statuses
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    if status.is_empty() {
        change.path.clone()
    } else {
        format!("{} ({status})", change.path)
    }
}

fn strip_status_suffix(path: &str) -> &str {
    match path.rsplit_once(" (") {
        Some((raw_path, _)) => raw_path,
        None => path,
    }
}

fn read_path_allowlist(path: &str) -> Result<BTreeSet<String>, String> {
    let mut allowed = BTreeSet::new();
    let text = read_text_lossy(Path::new(path))?;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        allowed.insert(normalize_slashes(trimmed));
    }
    Ok(allowed)
}

fn read_count_allowlist(path: &str) -> Result<BTreeMap<(String, String), usize>, String> {
    let mut allowed = BTreeMap::new();
    let text = read_text_lossy(Path::new(path))?;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts = trimmed.split('|').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(format!(
                "{path}:{} expected path|pattern|max_count|reason",
                line_number + 1
            ));
        }
        let max_count = parts[2]
            .parse::<usize>()
            .map_err(|err| format!("{path}:{} invalid max_count: {err}", line_number + 1))?;
        allowed.insert(
            (normalize_slashes(parts[0]), parts[1].to_string()),
            max_count,
        );
    }
    Ok(allowed)
}

fn read_count_policy_allowlist(path: &str) -> Result<BTreeMap<(String, String), usize>, String> {
    let mut allowed = BTreeMap::new();
    let text = read_text_lossy(Path::new(path))?;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts = trimmed.split('|').collect::<Vec<_>>();
        if parts.len() != 5 {
            return Err(format!(
                "{path}:{} expected path|pattern|max_count|owner|reason",
                line_number + 1
            ));
        }
        if parts[0].trim().is_empty()
            || parts[1].trim().is_empty()
            || parts[3].trim().is_empty()
            || parts[4].trim().is_empty()
        {
            return Err(format!(
                "{path}:{} allowlist entries require path, pattern, owner, and reason",
                line_number + 1
            ));
        }
        let max_count = parts[2]
            .parse::<usize>()
            .map_err(|err| format!("{path}:{} invalid max_count: {err}", line_number + 1))?;
        allowed.insert(
            (normalize_slashes(parts[0]), parts[1].to_string()),
            max_count,
        );
    }
    Ok(allowed)
}

fn read_local_context_allowlist(path: &str) -> Result<Vec<LocalContextAllow>, String> {
    let mut allowed = Vec::new();
    let text = read_text_lossy(Path::new(path))?;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts = trimmed.split('|').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(format!(
                "{path}:{} expected path|pattern|max_count|reason",
                line_number + 1
            ));
        }
        if parts[0].trim().is_empty() || parts[1].trim().is_empty() || parts[3].trim().is_empty() {
            return Err(format!(
                "{path}:{} allowlist entries require path, pattern, and reason",
                line_number + 1
            ));
        }
        let max_count = parts[2]
            .parse::<usize>()
            .map_err(|err| format!("{path}:{} invalid max_count: {err}", line_number + 1))?;
        allowed.push(LocalContextAllow {
            path: normalize_slashes(parts[0].trim()),
            pattern: parts[1].trim().to_string(),
            max_count,
            line: line_number + 1,
        });
    }
    Ok(allowed)
}

fn validate_local_context_allowlist(allowlist: &[LocalContextAllow]) -> Vec<String> {
    let mut violations = Vec::new();
    for entry in allowlist {
        if !is_local_context_candidate(&entry.path) {
            violations.push(format!(
                "Path: policy/local_context_allowlist.txt\nProblem: local context allowlist entry targets a file type that is not scanned\nPattern: {}\nCount: 1, allowed: 0\nLines: {}\nWhy this matters: Local context exceptions should stay narrow and reviewable.\nRecommended fixes:\n1. Remove the stale exception.\n2. If the file should be scanned, add its extension to the checker intentionally.",
                entry.pattern, entry.line
            ));
        }
        if forbidden_local_context_allowlist_pattern(&entry.pattern) {
            violations.push(format!(
                "Path: policy/local_context_allowlist.txt\nProblem: local context allowlist tries to permit real machine or session state\nPattern: {}\nCount: 1, allowed: 0\nLines: {}\nWhy this matters: Real machine paths, Codex memory paths, and sandbox paths must be removed, not allowlisted.\nRecommended fixes:\n1. Delete the local context from the committed file.\n2. Keep only generic examples in durable docs.",
                entry.pattern, entry.line
            ));
        }
    }
    violations
}

fn forbidden_local_context_allowlist_pattern(pattern: &str) -> bool {
    let lower = pattern.to_ascii_lowercase();
    if lower.contains(concat!(".", "codex"))
        || lower.contains(concat!("memory", ".md"))
        || lower.contains(concat!("sandbox:", "/mnt", "/data"))
        || lower.contains(concat!("/mnt", "/data"))
        || lower.contains(concat!("contentreference", "[oaicite"))
    {
        return true;
    }
    for token in windows_absolute_path_tokens(pattern) {
        let generic_example = token
            .to_ascii_lowercase()
            .replace('/', "\\")
            .contains(concat!(":\\", "path", "\\to\\"));
        if !generic_example {
            return true;
        }
    }
    !unix_home_path_tokens(pattern).is_empty()
}

fn read_glob_allowlist(path: &str) -> Result<Vec<GlobAllow>, String> {
    let mut allowed = Vec::new();
    let text = read_text_lossy(Path::new(path))?;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts = trimmed.split('|').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(format!(
                "{path}:{} expected glob|kind|owner|reason",
                line_number + 1
            ));
        }
        let entry = GlobAllow {
            glob: normalize_slashes(parts[0]),
        };
        if entry.glob.is_empty()
            || parts[1].trim().is_empty()
            || parts[2].trim().is_empty()
            || parts[3].trim().is_empty()
        {
            return Err(format!(
                "{path}:{} allowlist entries require glob, kind, owner, and reason",
                line_number + 1
            ));
        }
        allowed.push(entry);
    }
    Ok(allowed)
}

pub(crate) fn read_file_policy_allowlist(path: &str) -> Result<Vec<GlobAllow>, String> {
    let entries = parse_file_policy_allowlist(path)?;
    Ok(entries
        .into_iter()
        .map(|entry| GlobAllow {
            glob: normalize_slashes(&entry.glob.unwrap_or_default()),
        })
        .collect())
}

pub(crate) fn read_file_policy_test_commands(path: &str) -> Result<Vec<(usize, String)>, String> {
    let entries = parse_file_policy_allowlist(path)?;
    Ok(entries
        .into_iter()
        .flat_map(|entry| {
            entry
                .covered_by
                .unwrap_or_default()
                .into_iter()
                .filter(|command| is_cargo_test_command(command))
                .map(move |command| (entry.line, command))
        })
        .collect())
}

pub(crate) fn is_cargo_test_command(command: &str) -> bool {
    let mut words = command.split_whitespace();
    words.next() == Some("cargo") && words.next() == Some("test")
}

fn parse_file_policy_allowlist(path: &str) -> Result<Vec<FilePolicyAllowEntry>, String> {
    let text = read_text_lossy(Path::new(path))?;
    let mut entries = Vec::new();
    let mut current = FilePolicyAllowEntry::default();
    let mut in_entry = false;

    let lines = text.lines().collect::<Vec<_>>();
    let mut idx = 0;
    while idx < lines.len() {
        let line_number = idx + 1;
        let trimmed = lines[idx].trim();
        idx += 1;
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "[[allow]]" {
            if in_entry {
                validate_file_policy_allow_entry(path, &current)?;
                entries.push(current);
            }
            current = FilePolicyAllowEntry {
                line: line_number,
                ..FilePolicyAllowEntry::default()
            };
            in_entry = true;
            continue;
        }
        let Some((key, value)) = parse_toml_key_value(trimmed) else {
            continue;
        };
        if !in_entry {
            continue;
        }
        match key {
            "glob" => current.glob = Some(parse_string_value(value, path, line_number)?),
            "kind" => current.kind = Some(parse_string_value(value, path, line_number)?),
            "owner" => current.owner = Some(parse_string_value(value, path, line_number)?),
            "surface" => current.surface = Some(parse_string_value(value, path, line_number)?),
            "classification" => {
                current.classification = Some(parse_string_value(value, path, line_number)?)
            }
            "reason" => current.reason = Some(parse_string_value(value, path, line_number)?),
            "generated_by" => {
                current.generated_by = Some(parse_string_value(value, path, line_number)?)
            }
            "covered_by" => {
                let value = collect_toml_array_value(path, line_number, value, &lines, &mut idx)?;
                current.covered_by = Some(parse_inline_array(&value)?);
            }
            "expires" | "retired" => {}
            other => {
                return Err(format!(
                    "{path}:{line_number} unsupported non-Rust allowlist field `{other}`"
                ));
            }
        }
    }

    if in_entry {
        validate_file_policy_allow_entry(path, &current)?;
        entries.push(current);
    }
    if entries.is_empty() {
        return Err(format!("{path} has no [[allow]] entries"));
    }
    Ok(entries)
}

fn collect_toml_array_value(
    path: &str,
    line_number: usize,
    first_value: &str,
    lines: &[&str],
    idx: &mut usize,
) -> Result<String, String> {
    let mut value = first_value.trim().to_string();
    if !value.starts_with('[') || value.ends_with(']') {
        return Ok(value);
    }
    while *idx < lines.len() {
        let next = lines[*idx].trim();
        *idx += 1;
        value.push(' ');
        value.push_str(next);
        if next.ends_with(']') {
            return Ok(value);
        }
    }
    Err(format!(
        "{path}:{line_number} unterminated non-Rust allowlist array"
    ))
}

fn validate_file_policy_allow_entry(
    path: &str,
    entry: &FilePolicyAllowEntry,
) -> Result<(), String> {
    let required = [
        ("glob", entry.glob.as_deref()),
        ("kind", entry.kind.as_deref()),
        ("owner", entry.owner.as_deref()),
        ("surface", entry.surface.as_deref()),
        ("classification", entry.classification.as_deref()),
        ("reason", entry.reason.as_deref()),
    ];
    for (field, value) in required {
        if value.unwrap_or_default().trim().is_empty() {
            return Err(format!(
                "{path}:{} non-Rust allowlist entry requires `{field}`",
                entry.line
            ));
        }
    }
    let covered_by = entry.covered_by.as_ref().ok_or_else(|| {
        format!(
            "{path}:{} non-Rust allowlist entry requires `covered_by`",
            entry.line
        )
    })?;
    if covered_by.iter().any(|value| value.trim().is_empty()) {
        return Err(format!(
            "{path}:{} non-Rust allowlist `covered_by` values must be non-empty",
            entry.line
        ));
    }
    Ok(())
}

fn read_workflow_budgets(path: &str) -> Result<BTreeMap<String, WorkflowBudget>, String> {
    let mut budgets = BTreeMap::new();
    let text = read_text_lossy(Path::new(path))?;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts = trimmed.split('|').collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(format!(
                "{path}:{} expected path|max_non_empty_lines|reason",
                line_number + 1
            ));
        }
        let max_non_empty_lines = parts[1].parse::<usize>().map_err(|err| {
            format!(
                "{path}:{} invalid max_non_empty_lines: {err}",
                line_number + 1
            )
        })?;
        let budget = WorkflowBudget {
            path: normalize_slashes(parts[0]),
            max_non_empty_lines,
            reason: parts[2].trim().to_string(),
        };
        if budget.reason.is_empty() {
            return Err(format!(
                "{path}:{} reason must not be empty",
                line_number + 1
            ));
        }
        budgets.insert(budget.path.clone(), budget);
    }
    Ok(budgets)
}

fn read_path_allowlist_optional(path: &str) -> Result<BTreeSet<String>, String> {
    if Path::new(path).exists() {
        read_path_allowlist(path)
    } else {
        Ok(BTreeSet::new())
    }
}

fn spec_id_from_file_name(file_name: &str) -> Option<String> {
    let mut parts = file_name.split('-');
    let prefix = parts.next()?;
    let kind = parts.next()?;
    let number = parts.next()?;
    if prefix == "RIPR"
        && kind == "SPEC"
        && number.len() == 4
        && number.chars().all(|value| value.is_ascii_digit())
    {
        Some(format!("{prefix}-{kind}-{number}"))
    } else {
        None
    }
}

fn spec_status(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("Status: "))
        .map(|value| value.trim().to_string())
}

fn required_spec_headings() -> Vec<&'static str> {
    vec![
        "## Problem",
        "## Behavior",
        "## Required Evidence",
        "## Non-Goals",
        "## Acceptance Examples",
        "## Test Mapping",
        "## Implementation Mapping",
        "## Metrics",
    ]
}

fn has_markdown_heading(text: &str, heading: &str) -> bool {
    text.lines().any(|line| line.trim_end() == heading)
}

pub(crate) fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_files_inner(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let normalized = normalize_path(path);
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_normalized = normalize_path(relative);
    if should_skip_path(&relative_normalized) {
        return Ok(());
    }
    let metadata =
        fs::metadata(path).map_err(|err| format!("failed to inspect {normalized}: {err}"))?;
    if metadata.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in
            fs::read_dir(path).map_err(|err| format!("failed to read {normalized}: {err}"))?
        {
            let entry = entry.map_err(|err| format!("failed to read {normalized}: {err}"))?;
            collect_files_inner(root, &entry.path(), files)?;
        }
    }
    Ok(())
}

fn tracked_files() -> Result<Vec<String>, String> {
    let output = run_output("git", &["ls-files"])?;
    Ok(output
        .lines()
        .map(normalize_slashes)
        .filter(|path| !path.is_empty())
        .collect())
}

fn should_skip_path(path: &str) -> bool {
    path == ".git"
        || path.starts_with(".git/")
        || path == ".claude"
        || path.starts_with(".claude/")
        || path == "target"
        || path.starts_with("target/")
        || path.ends_with("/target")
        || path.contains("/target/")
        || path == ".ripr/release"
        || path.starts_with(".ripr/release/")
        || path.ends_with("/.vscode-test")
        || path.contains("/.vscode-test/")
        || path.ends_with("/node_modules")
        || path.contains("/node_modules/")
        || path.ends_with("/out")
        || path.contains("/out/")
        || path.ends_with("/dist")
        || path.contains("/dist/")
}

fn is_static_language_candidate(path: &str) -> bool {
    // Skip fixture CHANGELOG files (#2338): these contain bless reasons that
    // may legitimately reference banned words (e.g. "reword 'proven'"). They
    // are bookkeeping, not product output or source prose.
    if path.contains("/expected/CHANGELOG.md") || path == "expected/CHANGELOG.md" {
        return false;
    }
    let extensions = [
        ".md", ".rs", ".txt", ".json", ".toml", ".yml", ".yaml", ".ts", ".tsx", ".js", ".jsx",
    ];
    extensions.iter().any(|extension| path.ends_with(extension))
}

pub(crate) fn read_text_lossy(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn guarded_allow_attribute_lints() -> BTreeSet<&'static str> {
    [
        "clippy::unwrap_used",
        "clippy::expect_used",
        "clippy::panic",
        "clippy::todo",
        "clippy::unimplemented",
        "clippy::dbg_macro",
        "unwrap_used",
        "expect_used",
        "panic",
        "todo",
        "unimplemented",
        "dbg_macro",
        "unsafe_code",
        "dead_code",
        "unused_imports",
        "unused_variables",
        "warnings",
    ]
    .into_iter()
    .collect()
}

fn guarded_allow_attributes_in_text(
    text: &str,
    guarded: &BTreeSet<&'static str>,
) -> Vec<(usize, String)> {
    let bytes = text.as_bytes();
    let mut findings = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'#' {
            index += 1;
            continue;
        }

        let line = byte_line_number(text, index);
        let mut cursor = index + 1;
        if cursor < bytes.len() && bytes[cursor] == b'!' {
            cursor += 1;
        }
        cursor = skip_ascii_whitespace(bytes, cursor);
        if cursor >= bytes.len() || bytes[cursor] != b'[' {
            index += 1;
            continue;
        }
        cursor += 1;
        cursor = skip_ascii_whitespace(bytes, cursor);

        let ident_start = cursor;
        while cursor < bytes.len() && (bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'_')
        {
            cursor += 1;
        }
        let kind = &text[ident_start..cursor];
        if kind != "allow" && kind != "expect" {
            index += 1;
            continue;
        }
        cursor = skip_ascii_whitespace(bytes, cursor);
        if cursor >= bytes.len() || bytes[cursor] != b'(' {
            index += 1;
            continue;
        }

        let Some((content_start, content_end, next_index)) = attribute_paren_span(bytes, cursor)
        else {
            index += 1;
            continue;
        };
        for lint in attribute_lints(&text[content_start..content_end]) {
            if guarded.contains(lint.as_str()) {
                findings.push((line, format!("{kind}({lint})")));
            }
        }
        index = next_index;
    }
    findings
}

fn attribute_paren_span(bytes: &[u8], open: usize) -> Option<(usize, usize, usize)> {
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((open + 1, index, index + 1));
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn attribute_lints(content: &str) -> Vec<String> {
    content
        .split(',')
        .filter_map(|part| {
            let lint = part.trim();
            if lint.is_empty() || lint.contains('=') {
                None
            } else {
                Some(lint.to_string())
            }
        })
        .collect()
}

fn attribute_lint_name(attribute: &str) -> Option<&str> {
    let (_, rest) = attribute.split_once('(')?;
    Some(rest.strip_suffix(')').unwrap_or(rest).trim())
}

fn skip_ascii_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn byte_line_number(text: &str, byte_index: usize) -> usize {
    text.as_bytes()[..byte_index]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn allow_attribute_line_summary(lines: &[usize]) -> String {
    let mut unique = lines.to_vec();
    unique.sort_unstable();
    unique.dedup();
    unique
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn local_context_findings_for_path(path: &str) -> Result<Vec<LocalContextFinding>, String> {
    let mut findings = Vec::new();
    let Some(file_name) = path.rsplit('/').next() else {
        return Ok(findings);
    };

    if suspicious_runtime_file_names()
        .iter()
        .any(|name| file_name.eq_ignore_ascii_case(name))
    {
        findings.push(LocalContextFinding {
            path: path.to_string(),
            line: None,
            pattern: file_name.to_string(),
            problem: "committed runtime/session artifact filename".to_string(),
        });
    }

    if !is_local_context_candidate(path) {
        return Ok(findings);
    }

    let text = read_text_lossy(Path::new(path))?;
    for (line_index, line) in text.lines().enumerate() {
        for (pattern, problem) in local_context_line_findings(line) {
            findings.push(LocalContextFinding {
                path: path.to_string(),
                line: Some(line_index + 1),
                pattern,
                problem,
            });
        }
    }
    Ok(findings)
}

fn local_context_line_findings(line: &str) -> Vec<(String, String)> {
    let mut findings = BTreeSet::<(String, String)>::new();

    for token in windows_absolute_path_tokens(line) {
        findings.insert((token, "local absolute Windows path".to_string()));
    }
    for token in unix_home_path_tokens(line) {
        findings.insert((token, "local absolute Unix home path".to_string()));
    }

    let lower = line.to_ascii_lowercase();
    for (marker, problem) in local_context_markers() {
        if lower.contains(&marker.to_ascii_lowercase()) {
            findings.insert((marker, problem));
        }
    }

    if contains_recorded_date(line) {
        findings.insert((
            recorded_on_pattern().to_string(),
            "session timestamp language".to_string(),
        ));
    }
    if lower.contains(concat!("working tree", " is dirty before")) {
        findings.insert((
            concat!("working tree", " is dirty before").to_string(),
            "transient local worktree state".to_string(),
        ));
    }
    if lower.contains(concat!("before any", " codex edits")) {
        findings.insert((
            concat!("before any", " Codex edits").to_string(),
            "transient Codex session state".to_string(),
        ));
    }
    if lower.contains(concat!("current local", " state")) {
        findings.insert((
            concat!("current local", " state").to_string(),
            "transient local state language".to_string(),
        ));
    }
    if lower.contains(concat!("current", " branch:")) {
        findings.insert((
            concat!("Current", " branch:").to_string(),
            "transient local branch state".to_string(),
        ));
    }

    for token in file_reference_tokens(line) {
        let problem = if token.starts_with("file_") {
            "opaque uploaded file artifact reference"
        } else {
            "chat transcript file reference"
        };
        findings.insert((token, problem.to_string()));
    }

    findings.into_iter().collect()
}

fn local_context_markers() -> Vec<(String, String)> {
    vec![
        (
            concat!(".", "codex").to_string(),
            "Codex local memory path".to_string(),
        ),
        (
            concat!("MEMORY", ".md").to_string(),
            "Codex memory artifact".to_string(),
        ),
        (
            concat!("sandbox:", "/mnt", "/data").to_string(),
            "sandbox runtime path".to_string(),
        ),
        (
            concat!("/mnt", "/data/").to_string(),
            "sandbox runtime path".to_string(),
        ),
        (
            concat!("contentReference", "[oaicite").to_string(),
            "chat citation artifact".to_string(),
        ),
    ]
}

fn suspicious_runtime_file_names() -> Vec<String> {
    vec![
        concat!("CURRENT", "_STATE.md").to_string(),
        concat!("SESSION", "_STATE.md").to_string(),
        "SCRATCHPAD.md".to_string(),
        concat!("NOTES", "_FROM", "_RUN.md").to_string(),
        concat!("CODEX", "_STATE.md").to_string(),
        concat!("codex", "-", "memory", ".md").to_string(),
        "transcript.md".to_string(),
        "chat.md".to_string(),
    ]
}

fn windows_absolute_path_tokens(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index + 2 < bytes.len() {
        let token_boundary = index == 0 || is_local_context_token_delimiter(bytes[index - 1]);
        if token_boundary
            && bytes[index].is_ascii_alphabetic()
            && bytes[index + 1] == b':'
            && (bytes[index + 2] == b'\\' || bytes[index + 2] == b'/')
        {
            let start = index;
            index += 3;
            while index < bytes.len() && !is_local_context_token_delimiter(bytes[index]) {
                index += 1;
            }
            tokens.push(line[start..index].to_string());
        } else {
            index += 1;
        }
    }
    tokens
}

fn unix_home_path_tokens(line: &str) -> Vec<String> {
    ["/Users/", "/home/"]
        .iter()
        .flat_map(|prefix| absolute_path_tokens_with_prefix(line, prefix))
        .collect()
}

fn absolute_path_tokens_with_prefix(line: &str, prefix: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut search_start = 0;
    while let Some(offset) = line[search_start..].find(prefix) {
        let start = search_start + offset;
        let mut end = start + prefix.len();
        let bytes = line.as_bytes();
        let name_start = end;
        while end < line.len()
            && bytes[end] != b'/'
            && !is_local_context_token_delimiter(bytes[end])
        {
            end += 1;
        }
        if end == name_start || end >= line.len() || bytes[end] != b'/' {
            search_start = end.max(start + prefix.len());
            continue;
        }
        end += 1;
        while end < line.len() && !is_local_context_token_delimiter(bytes[end]) {
            end += 1;
        }
        tokens.push(line[start..end].to_string());
        search_start = end;
    }
    tokens
}

fn is_local_context_token_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'`' | b'"' | b'\'' | b')' | b']' | b'}' | b'<' | b'>' | b',' | b';'
        )
}

fn contains_recorded_date(line: &str) -> bool {
    let marker = recorded_on_marker();
    let Some(offset) = line.find(marker) else {
        return false;
    };
    let date = &line[offset + marker.len()..];
    date.len() >= 10
        && date.as_bytes()[0..4].iter().all(u8::is_ascii_digit)
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[5..7].iter().all(u8::is_ascii_digit)
        && date.as_bytes()[7] == b'-'
        && date.as_bytes()[8..10].iter().all(u8::is_ascii_digit)
}

fn recorded_on_marker() -> &'static str {
    concat!("Recorded", " on ")
}

fn recorded_on_pattern() -> &'static str {
    concat!("Recorded", " on <date>")
}

fn file_reference_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"file_") {
            let start = index;
            index += "file_".len();
            let hex_start = index;
            while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
                index += 1;
            }
            if index - hex_start >= 8 {
                tokens.push(line[start..index].to_string());
            }
            continue;
        }
        if bytes[index..].starts_with(b"turn") {
            let start = index;
            index += "turn".len();
            let digit_start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index > digit_start && bytes[index..].starts_with(b"file") {
                index += "file".len();
                let file_digit_start = index;
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
                if index > file_digit_start {
                    tokens.push(line[start..index].to_string());
                    continue;
                }
            }
            index = start + 1;
            continue;
        }
        index += 1;
    }
    tokens
}

fn local_context_line_summary(lines: &[Option<usize>]) -> String {
    let mut concrete = lines.iter().flatten().copied().collect::<Vec<_>>();
    concrete.sort_unstable();
    concrete.dedup();
    if concrete.is_empty() {
        "file name".to_string()
    } else {
        concrete
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn is_local_context_candidate(path: &str) -> bool {
    let extensions = [
        ".md", ".rs", ".txt", ".json", ".toml", ".yml", ".yaml", ".ts", ".tsx",
    ];
    extensions.iter().any(|extension| path.ends_with(extension))
}

fn write_local_context_json(violations: &[String]) -> Result<(), String> {
    let status = if violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let mut body = format!(
        "{{\n  \"schema_version\": \"0.1\",\n  \"status\": \"{status}\",\n  \"violation_count\": {},\n  \"violations\": [",
        violations.len()
    );
    for (index, violation) in violations.iter().enumerate() {
        if index > 0 {
            body.push(',');
        }
        body.push_str("\n    \"");
        body.push_str(&json_escape(violation));
        body.push('"');
    }
    if !violations.is_empty() {
        body.push('\n');
    }
    body.push_str("  ]\n}\n");
    write_report("local-context.json", &body)
}

pub(crate) fn normalize_path(path: &Path) -> String {
    normalize_slashes(&path.to_string_lossy())
        .trim_start_matches("./")
        .to_string()
}

fn normalize_slashes(value: &str) -> String {
    value.replace('\\', "/")
}

pub(crate) fn is_file_policy_candidate(path: &str) -> bool {
    let extensions = [
        ".bash", ".c", ".cjs", ".cpp", ".cs", ".go", ".h", ".hpp", ".java", ".js", ".json", ".kt",
        ".lua", ".mjs", ".php", ".pl", ".ps1", ".py", ".rb", ".sh", ".swift", ".toml", ".ts",
        ".tsx", ".yaml", ".yml", ".zsh",
    ];
    extensions.iter().any(|extension| path.ends_with(extension))
}

pub(crate) fn is_non_rust_programming_candidate(path: &str) -> bool {
    let extensions = [
        ".bash", ".c", ".cjs", ".cpp", ".cs", ".go", ".h", ".hpp", ".java", ".js", ".kt", ".lua",
        ".mjs", ".php", ".pl", ".ps1", ".py", ".rb", ".sh", ".swift", ".ts", ".tsx", ".zsh",
    ];
    extensions.iter().any(|extension| path.ends_with(extension))
}

pub(crate) fn non_rust_programming_retention_reason(path: &str) -> Option<&'static str> {
    if path.starts_with("editors/vscode/") && path.ends_with(".ts") {
        return Some(
            "VS Code extension source and tests must run in the VS Code Extension Host TypeScript API.",
        );
    }

    if path.starts_with("fixtures/")
        && (path.ends_with(".ts")
            || path.ends_with(".tsx")
            || path.ends_with(".js")
            || path.ends_with(".jsx")
            || path.ends_with(".py"))
    {
        return Some(
            "Fixture workspaces may contain TypeScript / JavaScript / Python source as analyzed inputs for the Campaign 27 preview adapters (RIPR-SPEC-0027 / RIPR-SPEC-0028).",
        );
    }

    None
}

fn is_generated_candidate(path: &str) -> bool {
    path == "Cargo.lock"
        || path.ends_with("/package-lock.json")
        || path == "package-lock.json"
        || path.starts_with("target/")
        || path.contains("/target/")
        || path.starts_with(".ripr/release/")
        || path.starts_with("dist/")
        || path.contains("/dist/")
        || path.ends_with(".vsix")
        || path.ends_with(".zip")
        || path.ends_with(".tar.gz")
        || path.ends_with(".sha256")
}

fn is_dependency_surface_candidate(path: &str) -> bool {
    let Some(file_name) = path.rsplit('/').next() else {
        return false;
    };
    matches!(
        file_name,
        "Cargo.toml"
            | "Cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "npm-shrinkwrap.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "requirements.txt"
            | "pyproject.toml"
            | "poetry.lock"
            | "Pipfile"
            | "Pipfile.lock"
            | "go.mod"
            | "go.sum"
            | "pom.xml"
            | "build.gradle"
            | "settings.gradle"
            | "gradle.lockfile"
            | "Gemfile"
            | "Gemfile.lock"
    )
}

fn is_process_policy_candidate(path: &str) -> bool {
    path.ends_with(".rs") || path.ends_with(".ts")
}

fn is_network_policy_candidate(path: &str) -> bool {
    path.ends_with(".rs")
        || path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".py")
        || path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".sh")
        || path.ends_with(".ps1")
        || path.ends_with(".yml")
        || path.ends_with(".yaml")
}

fn process_policy_patterns() -> Vec<String> {
    [
        concat!("use std::process::", "Command"),
        concat!("Command", "::new"),
        concat!("child", "_process"),
        concat!("cp.", "spawn"),
        concat!("cp.", "exec("),
        concat!("cp.", "execFile"),
    ]
    .iter()
    .map(|value| value.to_string())
    .collect()
}

fn network_policy_patterns() -> Vec<String> {
    [
        // Original patterns.
        concat!("https", ".get"),
        concat!("fetch", "("),
        concat!("req", "west"),
        concat!("u", "req"),
        concat!("Tcp", "Stream"),
        concat!("cu", "rl"),
        concat!("w", "get"),
        // Expanded patterns (#2412): cover common Rust/JS networking crates
        // that were previously invisible to the gate. Split with concat! so
        // the gate does not flag its own source (same technique as the
        // original patterns above).
        concat!("hy", "per"),
        concat!("isa", "hc"),
        concat!("atto", "httpc"),
        concat!("min", "req"),
        concat!("tokio::", "net"),
        concat!("std::net::", "Tcp"),
        concat!("to", "nic::"),
        concat!("tungste", "nite"),
        concat!("ssh", "2::"),
        concat!("req", "west::Client"),
        // Expanded patterns (#2903): UDP, web frameworks, HTTP/3, async
        // runtimes, and PowerShell web cmdlets that were still invisible.
        concat!("Udp", "Socket"),
        concat!("ax", "um"),
        concat!("act", "ix"),
        concat!("soc", "ket2"),
        concat!("qu", "inn"),
        concat!("async-", "std"),
        concat!("Invoke-", "WebRequest"),
        concat!("Invoke-", "RestMethod"),
    ]
    .iter()
    .map(|value| value.to_string())
    .collect()
}

fn shell_fetch_tool_name() -> &'static str {
    concat!("cu", "rl")
}

pub(crate) fn matches_any_glob(allowlist: &[GlobAllow], path: &str) -> bool {
    allowlist
        .iter()
        .any(|entry| glob_matches(&entry.glob, path))
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern_parts = pattern.split('/').collect::<Vec<_>>();
    let path_parts = path.split('/').collect::<Vec<_>>();
    glob_parts_match(&pattern_parts, &path_parts)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StaticLanguageMatcher {
    Path(String),
    Glob(String),
}

impl StaticLanguageMatcher {
    fn as_str(&self) -> &str {
        match self {
            StaticLanguageMatcher::Path(value) | StaticLanguageMatcher::Glob(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StaticLanguageAllowEntry {
    matcher: StaticLanguageMatcher,
    owner: String,
    reason: String,
}

#[cfg(test)]
impl StaticLanguageAllowEntry {
    fn new_path(
        path: impl Into<String>,
        owner: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            matcher: StaticLanguageMatcher::Path(path.into()),
            owner: owner.into(),
            reason: reason.into(),
        }
    }

    fn new_glob(
        glob: impl Into<String>,
        owner: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            matcher: StaticLanguageMatcher::Glob(glob.into()),
            owner: owner.into(),
            reason: reason.into(),
        }
    }
}

const STATIC_LANGUAGE_ALLOWLIST_PATH: &str = ".ripr/static-language-allowlist.toml";
const STATIC_LANGUAGE_ALLOWLIST_LEGACY_PATH: &str = ".ripr/static-language-allowlist.txt";
const STATIC_LANGUAGE_ALLOWED_GLOBS: &[&str] = &["docs/*.md", "docs/**/*.md"];

fn parse_static_language_allowlist(text: &str) -> (Vec<StaticLanguageAllowEntry>, Vec<String>) {
    let mut entries: Vec<StaticLanguageAllowEntry> = Vec::new();
    let mut violations = Vec::new();
    let mut schema_seen = false;
    let mut current: Option<PendingAllowEntry> = None;

    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "[[allow]]" {
            if let Some(pending) = current.take() {
                finalize_static_language_entry(pending, &mut entries, &mut violations);
            }
            current = Some(PendingAllowEntry::new(line_number));
            continue;
        }
        let Some((key, raw_value)) = trimmed.split_once('=') else {
            violations.push(format!(
                "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} expected `key = value`"
            ));
            continue;
        };
        let key = key.trim();
        let raw_value = raw_value.trim();
        if let Some(pending) = current.as_mut() {
            match key {
                "path" => assign_static_language_field(
                    raw_value,
                    line_number,
                    &mut violations,
                    |parsed| pending.path = Some((parsed, line_number)),
                ),
                "glob" => assign_static_language_field(
                    raw_value,
                    line_number,
                    &mut violations,
                    |parsed| pending.glob = Some((parsed, line_number)),
                ),
                "owner" => assign_static_language_field(
                    raw_value,
                    line_number,
                    &mut violations,
                    |parsed| pending.owner = Some((parsed, line_number)),
                ),
                "reason" => assign_static_language_field(
                    raw_value,
                    line_number,
                    &mut violations,
                    |parsed| pending.reason = Some((parsed, line_number)),
                ),
                _ => violations.push(format!(
                    "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} unsupported `[[allow]]` field `{key}`"
                )),
            }
        } else if key == "schema_version" {
            schema_seen = true;
            match raw_value.parse::<u32>() {
                Ok(1) => {}
                Ok(other) => violations.push(format!(
                    "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} schema_version = {other} is not supported (expected 1)"
                )),
                Err(_) => violations.push(format!(
                    "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} schema_version must be an integer literal"
                )),
            }
        } else {
            violations.push(format!(
                "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} unsupported top-level field `{key}`"
            ));
        }
    }

    if let Some(pending) = current.take() {
        finalize_static_language_entry(pending, &mut entries, &mut violations);
    }

    if !schema_seen {
        violations.push(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH} is missing required `schema_version = 1` header"
        ));
    }

    let mut seen_matchers: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in &entries {
        let matcher = entry.matcher.as_str();
        if let Some(&first) = seen_matchers.get(matcher) {
            violations.push(format!(
                "{STATIC_LANGUAGE_ALLOWLIST_PATH} matcher `{matcher}` is duplicated (first declared near line {first})"
            ));
        } else {
            seen_matchers.insert(matcher, 0);
        }
    }

    (entries, violations)
}

struct PendingAllowEntry {
    block_line: usize,
    path: Option<(String, usize)>,
    glob: Option<(String, usize)>,
    owner: Option<(String, usize)>,
    reason: Option<(String, usize)>,
}

impl PendingAllowEntry {
    fn new(block_line: usize) -> Self {
        Self {
            block_line,
            path: None,
            glob: None,
            owner: None,
            reason: None,
        }
    }
}

fn assign_static_language_field<F>(
    raw_value: &str,
    line_number: usize,
    violations: &mut Vec<String>,
    mut assign: F,
) where
    F: FnMut(String),
{
    match parse_quoted_value(raw_value) {
        Ok(parsed) => assign(parsed),
        Err(message) => violations.push(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} {message}"
        )),
    }
}

fn finalize_static_language_entry(
    pending: PendingAllowEntry,
    entries: &mut Vec<StaticLanguageAllowEntry>,
    violations: &mut Vec<String>,
) {
    let block_line = pending.block_line;
    let path_value = pending.path;
    let glob_value = pending.glob;
    let owner_value = pending.owner;
    let reason_value = pending.reason;

    let matcher = match (path_value, glob_value) {
        (Some((path, line)), None) => match validate_static_language_path(&path, line) {
            Ok(()) => Some(StaticLanguageMatcher::Path(path)),
            Err(message) => {
                violations.push(message);
                None
            }
        },
        (None, Some((glob, line))) => match validate_static_language_glob(&glob, line) {
            Ok(()) => Some(StaticLanguageMatcher::Glob(glob)),
            Err(message) => {
                violations.push(message);
                None
            }
        },
        (Some(_), Some(_)) => {
            violations.push(format!(
                "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{block_line} `[[allow]]` entry has both `path` and `glob`; declare exactly one"
            ));
            None
        }
        (None, None) => {
            violations.push(format!(
                "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{block_line} `[[allow]]` entry must declare either `path` or `glob`"
            ));
            None
        }
    };

    let owner = match owner_value {
        Some((value, line)) => {
            if value.trim().is_empty() {
                violations.push(format!(
                    "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line} `owner` is blank; name a responsible team or maintainer"
                ));
                None
            } else {
                Some(value)
            }
        }
        None => {
            violations.push(format!(
                "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{block_line} `[[allow]]` entry is missing required `owner`"
            ));
            None
        }
    };

    let reason = match reason_value {
        Some((value, line)) => {
            if value.trim().is_empty() {
                violations.push(format!(
                    "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line} `reason` is blank; explain why this matcher is exempt"
                ));
                None
            } else {
                Some(value)
            }
        }
        None => {
            violations.push(format!(
                "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{block_line} `[[allow]]` entry is missing required `reason`"
            ));
            None
        }
    };

    if let (Some(matcher), Some(owner), Some(reason)) = (matcher, owner, reason) {
        entries.push(StaticLanguageAllowEntry {
            matcher,
            owner,
            reason,
        });
    }
}

fn validate_static_language_path(path: &str, line_number: usize) -> Result<(), String> {
    if path.is_empty() {
        return Err(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} `path` is empty"
        ));
    }
    if path.contains('\\') {
        return Err(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} `path` `{path}` uses backslashes; use `/` separators"
        ));
    }
    if is_absolute_path_like(path) {
        return Err(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} `path` `{path}` is absolute; entries must be repository-relative"
        ));
    }
    if path.contains('*') || path.contains('?') {
        return Err(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} `path` `{path}` contains glob characters; use `glob = ...` instead"
        ));
    }
    Ok(())
}

fn validate_static_language_glob(glob: &str, line_number: usize) -> Result<(), String> {
    if glob.is_empty() {
        return Err(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} `glob` is empty"
        ));
    }
    if glob.contains('\\') {
        return Err(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} `glob` `{glob}` uses backslashes; use `/` separators"
        ));
    }
    if is_absolute_path_like(glob) {
        return Err(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} `glob` `{glob}` is absolute; entries must be repository-relative"
        ));
    }
    if !STATIC_LANGUAGE_ALLOWED_GLOBS.contains(&glob) {
        return Err(format!(
            "{STATIC_LANGUAGE_ALLOWLIST_PATH}:{line_number} `glob` `{glob}` is not in the scoped set; current allowed globs: {}",
            STATIC_LANGUAGE_ALLOWED_GLOBS.join(", ")
        ));
    }
    Ok(())
}

fn is_absolute_path_like(value: &str) -> bool {
    if value.starts_with('/') {
        return true;
    }
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

fn load_static_language_allowlist() -> Result<Vec<StaticLanguageAllowEntry>, Vec<String>> {
    if Path::new(STATIC_LANGUAGE_ALLOWLIST_LEGACY_PATH).exists() {
        return Err(vec![format!(
            "{STATIC_LANGUAGE_ALLOWLIST_LEGACY_PATH} still exists; the static-language allowlist moved to {STATIC_LANGUAGE_ALLOWLIST_PATH}. Delete the legacy `.txt` file to avoid split-brain policy."
        )]);
    }
    let path = Path::new(STATIC_LANGUAGE_ALLOWLIST_PATH);
    let text = read_text_lossy(path).map_err(|err| vec![err])?;
    let (entries, mut violations) = parse_static_language_allowlist(&text);
    for entry in &entries {
        if let StaticLanguageMatcher::Path(value) = &entry.matcher
            && !Path::new(value).exists()
        {
            violations.push(format!(
                "{STATIC_LANGUAGE_ALLOWLIST_PATH} matcher `{value}` does not exist on disk"
            ));
        }
    }
    if violations.is_empty() {
        Ok(entries)
    } else {
        Err(violations)
    }
}

fn static_language_allowlist_covers(allowlist: &[StaticLanguageAllowEntry], path: &str) -> bool {
    allowlist.iter().any(|entry| match &entry.matcher {
        StaticLanguageMatcher::Path(value) => value == path,
        StaticLanguageMatcher::Glob(value) => glob_matches(value, path),
    })
}

fn should_scan_static_language_path(allowlist: &[StaticLanguageAllowEntry], path: &str) -> bool {
    is_static_language_candidate(path) && !static_language_allowlist_covers(allowlist, path)
}

fn glob_parts_match(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }
    if pattern[0] == "**" {
        return glob_parts_match(&pattern[1..], path)
            || (!path.is_empty() && glob_parts_match(pattern, &path[1..]));
    }
    if path.is_empty() {
        return false;
    }
    segment_matches(pattern[0], path[0]) && glob_parts_match(&pattern[1..], &path[1..])
}

fn segment_matches(pattern: &str, value: &str) -> bool {
    let pattern_chars = pattern.chars().collect::<Vec<_>>();
    let value_chars = value.chars().collect::<Vec<_>>();
    segment_parts_match(&pattern_chars, &value_chars)
}

fn segment_parts_match(pattern: &[char], value: &[char]) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }
    if pattern[0] == '*' {
        return segment_parts_match(&pattern[1..], value)
            || (!value.is_empty() && segment_parts_match(pattern, &value[1..]));
    }
    !value.is_empty() && pattern[0] == value[0] && segment_parts_match(&pattern[1..], &value[1..])
}

fn parse_git_stage_line(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let mode = parts.next()?;
    let _object_type = parts.next()?;
    let _hash = parts.next()?;
    let stage_and_path = line.split('\t').nth(1)?;
    Some((mode, stage_and_path))
}

fn extract_workflow_run_blocks(text: &str) -> Vec<RunBlock> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if let Some(rest) = workflow_run_value(trimmed) {
            let indent = line.len() - trimmed.len();
            let run_value = rest.trim();
            if run_value == "|" || run_value == ">" || run_value == "|-" || run_value == ">-" {
                let mut block_lines = Vec::new();
                let mut next_idx = idx + 1;
                while next_idx < lines.len() {
                    let next = lines[next_idx];
                    let next_trimmed = next.trim_start();
                    let next_indent = next.len() - next_trimmed.len();
                    if !next_trimmed.is_empty() && next_indent <= indent {
                        break;
                    }
                    block_lines.push(next_trimmed.to_string());
                    next_idx += 1;
                }
                let non_empty_lines = block_lines
                    .iter()
                    .filter(|value| !value.trim().is_empty())
                    .count();
                blocks.push(RunBlock {
                    line_number: idx + 1,
                    non_empty_lines,
                    text: block_lines.join("\n"),
                });
                idx = next_idx;
                continue;
            }
            blocks.push(RunBlock {
                line_number: idx + 1,
                non_empty_lines: usize::from(!run_value.is_empty()),
                text: run_value.to_string(),
            });
        }
        idx += 1;
    }
    blocks
}

fn workflow_run_value(trimmed_line: &str) -> Option<&str> {
    trimmed_line
        .strip_prefix("run:")
        .or_else(|| trimmed_line.strip_prefix("- run:"))
}

fn forbidden_static_terms() -> Vec<String> {
    ["killed", "survived", "untested", "proven", "adequate"]
        .iter()
        .map(|value| value.to_string())
        .collect()
}

/// Inline marker that suppresses static-language violations for a single line —
/// finer-grained than a whole-file allowlist entry. It MUST be followed by a
/// reason so the suppression is reviewable and not a silent drive-by:
///   `<comment-prefix> ripr-allow: static-language: <reason>`
const STATIC_LANGUAGE_INLINE_ALLOW_MARKER: &str = "ripr-allow: static-language:";

/// True when the (already-lowercased) line carries the inline-allow marker
/// followed by a non-empty reason. A bare marker with no reason does NOT
/// suppress, so an intentional allow always records why.
fn line_has_static_language_inline_allow(lower_line: &str) -> bool {
    match lower_line.find(STATIC_LANGUAGE_INLINE_ALLOW_MARKER) {
        Some(index) => {
            let after = &lower_line[index + STATIC_LANGUAGE_INLINE_ALLOW_MARKER.len()..];
            !after.trim().is_empty()
        }
        None => false,
    }
}

/// A plain-English synonym hint for a prohibited static-language term, used to
/// speed fixes when the term appears in prose/comments rather than analyzer
/// output. The gate scans all tracked prose, not only output strings, so a hit
/// is often an innocent English word that just needs a neutral synonym.
fn static_language_synonym_hint(term: &str) -> &'static str {
    match term {
        "killed" => "removed/terminated",
        "survived" => "persisted/remained",
        "untested" => "unexercised/uncovered",
        "proven" => "demonstrated/established",
        "adequate" => "sufficient/enough",
        _ => "a neutral synonym",
    }
}

/// Build an actionable static-language violation: `path:line`, the offending
/// snippet, and a fix hint. The hint names the common confusion — the gate
/// scans all tracked prose, not just analyzer output — so a hit in a comment or
/// doc is fixed with a plain synonym, while a hit in real output must use the
/// approved exposure vocabulary.
fn static_language_violation_message(
    path: &str,
    line_number: usize,
    term: &str,
    line: &str,
) -> String {
    let trimmed = line.trim();
    let snippet = if trimmed.chars().count() > 80 {
        let mut shortened: String = trimmed.chars().take(77).collect();
        shortened.push_str("...");
        shortened
    } else {
        trimmed.to_string()
    };
    let hint = static_language_synonym_hint(term);
    format!(
        "{path}:{line_number} prohibited static-language term `{term}` in `{snippet}` \
— this gate scans all tracked prose, not just analyzer output. In output, use the \
approved exposure vocabulary (exposed/weakly_exposed/reachable_unrevealed/no_static_path/\
infection_unknown/propagation_unknown/static_unknown). In comments or docs, use a plain \
synonym (e.g. {hint}). To intentionally allow this line, append \
`{STATIC_LANGUAGE_INLINE_ALLOW_MARKER} <reason>`."
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
mod proposed_spec_age_tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn untracked_spec_is_not_proven() -> Result<(), String> {
        let path = std::env::temp_dir().join(format!("ripr-spec-age-{}", std::process::id()));
        fs::create_dir_all(path.join("docs/specs")).map_err(|e| e.to_string())?;
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !init.status.success() {
            return Err("git init failed".to_string());
        }
        let spec = path.join("docs/specs/space name-é.md");
        fs::write(&spec, "Status: proposed\n").map_err(|e| e.to_string())?;
        assert!(matches!(
            proposed_spec_age(&spec, &path, SystemTime::now()),
            ProposedSpecAge::NotProven { .. }
        ));
        let relative = Path::new("docs/specs/not-tracked.md");
        let ProposedSpecAge::NotProven { reason } =
            proposed_spec_age(relative, Path::new("."), SystemTime::now())
        else {
            return Err("relative production-shaped path did not fail closed".to_string());
        };
        assert!(reason.contains("Git age lookup"));
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn committed_age_is_read_from_git_not_filesystem_mtime() -> Result<(), String> {
        let path =
            std::env::temp_dir().join(format!("ripr-spec-age-{}-committed", std::process::id()));
        fs::create_dir_all(path.join("docs/specs")).map_err(|e| e.to_string())?;
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !init.status.success() {
            return Err("git init failed".to_string());
        }
        let relative = "docs/specs/space name-é.md";
        fs::write(path.join(relative), "Status: proposed\n").map_err(|e| e.to_string())?;
        let add = Command::new("git")
            .args(["add", "--", relative])
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !add.status.success() {
            return Err("git add failed".to_string());
        }
        let date = "2027-01-01T00:00:00Z";
        let commit = Command::new("git")
            .args([
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "-m",
                "fixture",
            ])
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !commit.status.success() {
            return Err("git commit failed".to_string());
        }
        let now = UNIX_EPOCH + Duration::from_hours(500_000);
        assert!(matches!(
            proposed_spec_age(&path.join(relative), &path, now),
            ProposedSpecAge::Current
        ));
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn old_commit_is_stale_after_checkout() -> Result<(), String> {
        let path = std::env::temp_dir().join(format!("ripr-spec-age-{}-old", std::process::id()));
        fs::create_dir_all(path.join("docs/specs")).map_err(|e| e.to_string())?;
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !init.status.success() {
            return Err("git init failed".to_string());
        }
        let relative = "docs/specs/space name-é.md";
        fs::write(path.join(relative), "Status: proposed\n").map_err(|e| e.to_string())?;
        let add = Command::new("git")
            .args(["add", "--", relative])
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !add.status.success() {
            return Err("git add failed".to_string());
        }
        let date = "2020-01-01T00:00:00Z";
        let commit = Command::new("git")
            .args([
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "-m",
                "fixture",
            ])
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;
        if !commit.status.success() {
            return Err("git commit failed".to_string());
        }
        let now = UNIX_EPOCH + Duration::from_hours(500_000);
        assert!(matches!(
            proposed_spec_age(&path.join(relative), &path, now),
            ProposedSpecAge::Stale { .. }
        ));
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
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
