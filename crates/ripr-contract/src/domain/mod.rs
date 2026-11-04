pub mod analysis_execution_identity;
pub mod candidate_relation;
pub mod canonical_item;
pub mod causal_delta;
pub mod classification;
pub mod command_spec;
pub mod confidence_basis;
pub mod evidence;
pub mod evidence_state;
pub mod executed_control;
pub mod external_verification;
pub mod external_verification_native;
pub mod feedback;
pub mod finding_test_evidence;
pub mod git_candidate;
pub mod guard_refusal;
pub mod identity;
pub mod intervention_study;
pub mod language;
pub mod language_capability;
#[cfg(test)]
pub mod language_capability_drift;
pub mod limit_guidance;
pub mod probe;
pub mod project_facts;
pub mod refusal_remedy;
pub mod summary;
pub mod support;
pub mod test_candidate_set;
pub mod test_evidence_identity;
pub mod test_evidence_summary;
pub mod typescript_oracle_seam;
pub mod typescript_preview_related_test;

pub use analysis_execution_identity::{
    AnalysisAttemptId, CompletedAnalysisSnapshotId, ContinuationId, DiagnosticResultId,
};
pub use candidate_relation::CandidateRelation;
pub use canonical_item::{CanonicalItemId, CanonicalItemParseError};
pub use causal_delta::{
    AttributionBasis, CanonicalDelta, CanonicalEvidenceState, ComparisonConfidence,
    ComparisonCoverage, DeltaAttribution, GapState, compare_fixture_delta,
};
pub use classification::ExposureClass;
pub use classification::{
    ASSERTION_CONTEXT_UNESTABLISHED, ASSERTION_NOT_CREDITED_PREFIX,
    ASSERTION_SHAPED_INFECTION_UNKNOWN_NEXT_STEP, ASSERTION_SHAPED_NO_STATIC_PATH_NEXT_STEP,
    ASSERTION_SHAPED_OWNER_REASON, ASSERTION_SHAPED_REACHABLE_UNREVEALED_NEXT_STEP,
    ASSERTION_SHAPED_WEAKLY_EXPOSED_NEXT_STEP, BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP,
    BUILD_SCRIPT_NO_STATIC_PATH_NEXT_STEP, CALLEE_ONLY_REACH_PREFIX,
    LIMITATION_ANALYZER_ROUTE_PREFIX, LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX,
    LIMITATION_HELPER_BOUNDARY_PREFIX, LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX,
    LIMITATION_NON_CLAIM_PREFIX, NO_STATIC_PATH_NEXT_STEP, TRANSITIVE_REACH_WITNESS_PREFIX,
};
pub use command_spec::{
    CancellationPolicy, CommandAuthorityBoundary, CommandCostClass, CommandExecutionMode,
    CommandPlatform, CommandRole, CommandSpec, CommandSpecValidationError, EnvironmentAssignment,
    EnvironmentPolicy, ExpectedResultParser, NetworkPolicy, StdinPolicy,
    actionable_verify_source_is_supported,
    verify_command_is_unbounded_repo_exposure_snapshot_compare,
};
pub use command_spec::{
    command_tokens, python_pytest_verify_display, python_unittest_verify_display,
};
pub use confidence_basis::ConfidenceCap;
pub use confidence_basis::{
    ConfidenceBasisEntry, ConfidenceBasisInputs, basis_entries, why_capped_line,
};
pub use evidence::BOUNDARY_INPUT_OBSERVED_SUMMARY;
pub use evidence::indefinite_article;
pub use evidence::{
    Confidence, OracleKind, OracleStrength, RelationConfidence, RelationReason, RevealEvidence,
    RiprEvidence, StageEvidence, StageState,
};
pub use evidence_state::EvidenceState;
pub use executed_control::{
    EXECUTED_CONTROL_PACKET_KIND, EXECUTED_CONTROL_SCHEMA_VERSION, ExecutedControlPacketV1,
    ObligationSatisfaction, ResultState,
};
pub use feedback::{
    ActorKind, FEEDBACK_NOTE_MAX_BYTES, FEEDBACK_SCHEMA_VERSION, FeedbackJudgment, FeedbackPayload,
    FeedbackReason, FeedbackReceipt, ReferenceState, ResultIdentity, ReviewStatus,
    classify_reference,
};
pub use git_candidate::{
    GitCandidateBase, GitCandidateDiffSemantics, GitCandidateSubject, GitCandidateSubjectError,
    GitHashFormat, GitObjectId, GitTreeish,
};
pub use guard_refusal::{
    GuardRefusalKind, assertion_not_credited_entry_for_test, assertion_not_credited_reason,
    guard_refusal_digest_classification_hint, guard_refusal_kind, guard_refusal_limitation_edge,
};
pub use identity::{
    GOVERNED_IDENTITY_SURFACES, IDENTITY_REGISTRY_JSON_PATH, IDENTITY_REGISTRY_MARKDOWN_PATH,
    REQUIRED_TAXONOMY_KINDS, identity_field_disposition, identity_registry_canonical_json,
    identity_registry_markdown, identity_registry_violations,
};
#[cfg(any(test, feature = "test-fixtures"))]
pub use intervention_study::example_preregistered_study;
pub use intervention_study::{
    InterventionStudyError, RIPR_INTERVENTION_STUDY_SCHEMA_VERSION, RiprInterventionStudyV1,
    codes as intervention_study_codes,
};
#[cfg(feature = "lang-perl")]
pub use language::perl_fact_packet_guidance;
pub use language::{LanguageId, LanguageStatus, OwnerKind, StaticLimitKind};
pub use language::{
    PERL_FACT_EXPORTER, enablement_tip_languages, format_languages_enabled_toml, rust_markers_at,
};
pub use language::{PYTEST_VERIFY_PROGRAM, is_pytest_verify_command};
pub use probe::{
    ARM_UNSELECTED_REASON_PREFIX, GUARD_UNENTERED_REASON_PREFIX, STRONG_CHECK_UNTIED_NEXT_STEP,
    exact_assertion_fact, input_boundary_fact, is_unselected_input_reason,
    row_exact_discriminator_fact,
};
pub use probe::{
    ActivationEvidence, AnalysisScope, DeltaKind, Finding, FindingCanonicalGap, FlowSinkFact,
    FlowSinkKind, MissingDiscriminatorFact, ORACLE_ALIGNMENT_VALUES, Probe, ProbeFamily,
    RelatedTest, RelatedTestMiss, RelatedTestMissSupport, RelatedTestRole,
    SOURCE_CURRENTNESS_VALUES, SourceCurrentness, StopReason, ValueContext, ValueFact,
};
pub use refusal_remedy::RefusalRemedy;
// Public so the `ripr` facade can re-export it; not part of a product API (ADR 0026).
pub use probe::LIVE_PYTHON_TRUTHINESS_ALIGNMENT;
pub use probe::MISSING_DISCRIMINATOR_VALUE_PREFIX;
pub use probe::boundary_input_observed_with_weak_oracle;
pub use probe::declared_const_name;
pub use project_facts::{
    CanonicalSourceRole, FactCurrentnessV1, PROJECT_FACTS_SCHEMA_VERSION, ProjectFactsV1,
    SourceUnitFactV1, TestEnvironmentFactV1,
};
pub use summary::{LanguageFileCount, Summary};
pub use support::{ProbeId, SourceLocation, SymbolId};
pub use test_candidate_set::{TestCandidateNextRoute, TestCandidateRow, TestCandidateSet};
pub use test_evidence_identity::normalize_transport_path;
pub use test_evidence_summary::{TestEvidenceEntry, TestEvidenceSummary};
pub use typescript_oracle_seam::ts_oracle_kind_matches_seam;
pub use typescript_preview_related_test::typescript_script_preview_related_test;
