//! Facade for the domain compartment (ADR 0027, #8382/#12726).
//!
//! The domain DTOs live in `ripr-contract` and are re-exported here at their
//! established `ripr::domain` paths, item by item, so every existing
//! consumer keeps its import path and the public API is unchanged (the
//! explicit list is what `check-public-api` records; no glob). Seven modules
//! stay in this crate because they consume analysis/agent/app helpers that
//! have not moved: `context_packet`, `diagnostic_witness`, `next_action`,
//! `fix_instruction`, `repair_card`, and `verification_result`.

pub(crate) use ripr_contract::domain::ARM_UNSELECTED_REASON_PREFIX;
pub(crate) use ripr_contract::domain::ASSERTION_CONTEXT_UNESTABLISHED;
pub(crate) use ripr_contract::domain::ASSERTION_NOT_CREDITED_PREFIX;
pub(crate) use ripr_contract::domain::ASSERTION_SHAPED_INFECTION_UNKNOWN_NEXT_STEP;
pub(crate) use ripr_contract::domain::ASSERTION_SHAPED_NO_STATIC_PATH_NEXT_STEP;
pub(crate) use ripr_contract::domain::ASSERTION_SHAPED_OWNER_REASON;
pub(crate) use ripr_contract::domain::ASSERTION_SHAPED_REACHABLE_UNREVEALED_NEXT_STEP;
pub(crate) use ripr_contract::domain::ASSERTION_SHAPED_WEAKLY_EXPOSED_NEXT_STEP;
pub use ripr_contract::domain::ActivationEvidence;
pub(crate) use ripr_contract::domain::ActorKind;
pub(crate) use ripr_contract::domain::AnalysisAttemptId;
pub use ripr_contract::domain::AnalysisScope;
pub use ripr_contract::domain::AttributionBasis;
pub(crate) use ripr_contract::domain::BOUNDARY_INPUT_OBSERVED_SUMMARY;
pub(crate) use ripr_contract::domain::BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP;
pub(crate) use ripr_contract::domain::BUILD_SCRIPT_NO_STATIC_PATH_NEXT_STEP;
pub(crate) use ripr_contract::domain::CALLEE_ONLY_REACH_PREFIX;
pub use ripr_contract::domain::CancellationPolicy;
pub use ripr_contract::domain::CandidateRelation;
pub use ripr_contract::domain::CanonicalDelta;
pub use ripr_contract::domain::CanonicalEvidenceState;
pub use ripr_contract::domain::CanonicalItemId;
pub use ripr_contract::domain::CanonicalItemParseError;
pub use ripr_contract::domain::CommandAuthorityBoundary;
pub use ripr_contract::domain::CommandCostClass;
pub use ripr_contract::domain::CommandExecutionMode;
pub use ripr_contract::domain::CommandPlatform;
pub use ripr_contract::domain::CommandRole;
pub use ripr_contract::domain::CommandSpec;
pub use ripr_contract::domain::CommandSpecValidationError;
pub use ripr_contract::domain::ComparisonConfidence;
pub use ripr_contract::domain::ComparisonCoverage;
pub(crate) use ripr_contract::domain::CompletedAnalysisSnapshotId;
pub use ripr_contract::domain::Confidence;
pub(crate) use ripr_contract::domain::ConfidenceBasisEntry;
pub(crate) use ripr_contract::domain::ConfidenceBasisInputs;
pub use ripr_contract::domain::ConfidenceCap;
pub(crate) use ripr_contract::domain::ContinuationId;
pub use ripr_contract::domain::DeltaAttribution;
pub use ripr_contract::domain::DeltaKind;
pub(crate) use ripr_contract::domain::DiagnosticResultId;
pub(crate) use ripr_contract::domain::EXECUTED_CONTROL_PACKET_KIND;
pub(crate) use ripr_contract::domain::EXECUTED_CONTROL_SCHEMA_VERSION;
pub use ripr_contract::domain::EnvironmentAssignment;
pub use ripr_contract::domain::EnvironmentPolicy;
pub(crate) use ripr_contract::domain::EvidenceState;
pub(crate) use ripr_contract::domain::ExecutedControlPacketV1;
pub use ripr_contract::domain::ExpectedResultParser;
pub use ripr_contract::domain::ExposureClass;
pub(crate) use ripr_contract::domain::FEEDBACK_NOTE_MAX_BYTES;
pub(crate) use ripr_contract::domain::FEEDBACK_SCHEMA_VERSION;
pub(crate) use ripr_contract::domain::FeedbackJudgment;
pub(crate) use ripr_contract::domain::FeedbackPayload;
pub(crate) use ripr_contract::domain::FeedbackReason;
pub(crate) use ripr_contract::domain::FeedbackReceipt;
pub use ripr_contract::domain::Finding;
pub use ripr_contract::domain::FindingCanonicalGap;
pub use ripr_contract::domain::FlowSinkFact;
pub use ripr_contract::domain::FlowSinkKind;
pub use ripr_contract::domain::GOVERNED_IDENTITY_SURFACES;
pub(crate) use ripr_contract::domain::GUARD_UNENTERED_REASON_PREFIX;
pub use ripr_contract::domain::GapState;
pub use ripr_contract::domain::GitCandidateBase;
pub use ripr_contract::domain::GitCandidateDiffSemantics;
pub use ripr_contract::domain::GitCandidateSubject;
pub use ripr_contract::domain::GitCandidateSubjectError;
pub use ripr_contract::domain::GitHashFormat;
pub use ripr_contract::domain::GitObjectId;
pub use ripr_contract::domain::GitTreeish;
pub(crate) use ripr_contract::domain::GuardRefusalKind;
pub use ripr_contract::domain::IDENTITY_REGISTRY_JSON_PATH;
pub use ripr_contract::domain::IDENTITY_REGISTRY_MARKDOWN_PATH;
pub(crate) use ripr_contract::domain::InterventionStudyError;
pub(crate) use ripr_contract::domain::LIMITATION_ANALYZER_ROUTE_PREFIX;
pub(crate) use ripr_contract::domain::LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX;
pub(crate) use ripr_contract::domain::LIMITATION_HELPER_BOUNDARY_PREFIX;
pub(crate) use ripr_contract::domain::LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX;
pub(crate) use ripr_contract::domain::LIMITATION_NON_CLAIM_PREFIX;
pub(crate) use ripr_contract::domain::LIVE_PYTHON_TRUTHINESS_ALIGNMENT;
pub use ripr_contract::domain::LanguageFileCount;
pub use ripr_contract::domain::LanguageId;
pub use ripr_contract::domain::LanguageStatus;
pub(crate) use ripr_contract::domain::MISSING_DISCRIMINATOR_VALUE_PREFIX;
pub use ripr_contract::domain::MissingDiscriminatorFact;
pub(crate) use ripr_contract::domain::NO_STATIC_PATH_NEXT_STEP;
pub use ripr_contract::domain::NetworkPolicy;
pub use ripr_contract::domain::ORACLE_ALIGNMENT_VALUES;
pub(crate) use ripr_contract::domain::ObligationSatisfaction;
pub use ripr_contract::domain::OracleKind;
pub use ripr_contract::domain::OracleStrength;
pub use ripr_contract::domain::OwnerKind;
pub(crate) use ripr_contract::domain::PERL_FACT_EXPORTER;
pub use ripr_contract::domain::Probe;
pub use ripr_contract::domain::ProbeFamily;
pub use ripr_contract::domain::ProbeId;
pub use ripr_contract::domain::REQUIRED_TAXONOMY_KINDS;
pub(crate) use ripr_contract::domain::RIPR_INTERVENTION_STUDY_SCHEMA_VERSION;
pub(crate) use ripr_contract::domain::ReferenceState;
pub(crate) use ripr_contract::domain::RefusalRemedy;
pub use ripr_contract::domain::RelatedTest;
pub use ripr_contract::domain::RelatedTestMiss;
pub use ripr_contract::domain::RelatedTestMissSupport;
pub use ripr_contract::domain::RelatedTestRole;
pub use ripr_contract::domain::RelationConfidence;
pub use ripr_contract::domain::RelationReason;
pub(crate) use ripr_contract::domain::ResultIdentity;
pub(crate) use ripr_contract::domain::ResultState;
pub use ripr_contract::domain::RevealEvidence;
pub(crate) use ripr_contract::domain::ReviewStatus;
pub use ripr_contract::domain::RiprEvidence;
pub(crate) use ripr_contract::domain::RiprInterventionStudyV1;
pub use ripr_contract::domain::SOURCE_CURRENTNESS_VALUES;
pub(crate) use ripr_contract::domain::STRONG_CHECK_UNTIED_NEXT_STEP;
pub use ripr_contract::domain::SourceCurrentness;
pub use ripr_contract::domain::SourceLocation;
pub use ripr_contract::domain::StageEvidence;
pub use ripr_contract::domain::StageState;
pub use ripr_contract::domain::StaticLimitKind;
pub use ripr_contract::domain::StdinPolicy;
pub use ripr_contract::domain::StopReason;
pub use ripr_contract::domain::Summary;
pub use ripr_contract::domain::SymbolId;
pub(crate) use ripr_contract::domain::TRANSITIVE_REACH_WITNESS_PREFIX;
pub use ripr_contract::domain::TestCandidateNextRoute;
pub use ripr_contract::domain::TestCandidateRow;
pub use ripr_contract::domain::TestCandidateSet;
pub use ripr_contract::domain::TestEvidenceEntry;
pub use ripr_contract::domain::TestEvidenceSummary;
pub use ripr_contract::domain::ValueContext;
pub use ripr_contract::domain::ValueFact;
pub use ripr_contract::domain::actionable_verify_source_is_supported;
pub(crate) use ripr_contract::domain::assertion_not_credited_entry_for_test;
pub(crate) use ripr_contract::domain::assertion_not_credited_reason;
pub(crate) use ripr_contract::domain::basis_entries;
pub(crate) use ripr_contract::domain::boundary_input_observed_with_weak_oracle;
pub(crate) use ripr_contract::domain::classify_reference;
pub(crate) use ripr_contract::domain::command_tokens;
pub use ripr_contract::domain::compare_fixture_delta;
pub(crate) use ripr_contract::domain::declared_const_name;
pub(crate) use ripr_contract::domain::enablement_tip_languages;
pub(crate) use ripr_contract::domain::exact_assertion_fact;
#[cfg(test)]
pub(crate) use ripr_contract::domain::example_preregistered_study;
pub(crate) use ripr_contract::domain::format_languages_enabled_toml;
pub(crate) use ripr_contract::domain::guard_refusal_digest_classification_hint;
pub(crate) use ripr_contract::domain::guard_refusal_kind;
pub(crate) use ripr_contract::domain::guard_refusal_limitation_edge;
pub use ripr_contract::domain::identity_field_disposition;
pub use ripr_contract::domain::identity_registry_canonical_json;
pub use ripr_contract::domain::identity_registry_markdown;
pub use ripr_contract::domain::identity_registry_violations;
pub(crate) use ripr_contract::domain::indefinite_article;
pub(crate) use ripr_contract::domain::input_boundary_fact;
pub(crate) use ripr_contract::domain::intervention_study_codes;
pub(crate) use ripr_contract::domain::is_pytest_verify_command;
pub(crate) use ripr_contract::domain::is_unselected_input_reason;
pub(crate) use ripr_contract::domain::normalize_transport_path;
#[cfg(feature = "lang-perl")]
pub(crate) use ripr_contract::domain::perl_fact_packet_guidance;
pub(crate) use ripr_contract::domain::python_pytest_verify_display;
pub(crate) use ripr_contract::domain::python_unittest_verify_display;
pub(crate) use ripr_contract::domain::row_exact_discriminator_fact;
pub(crate) use ripr_contract::domain::rust_markers_at;
pub use ripr_contract::domain::verify_command_is_unbounded_repo_exposure_snapshot_compare;
pub(crate) use ripr_contract::domain::why_capped_line;
pub(crate) use ripr_contract::domain::{
    CanonicalSourceRole, FactCurrentnessV1, PROJECT_FACTS_SCHEMA_VERSION, ProjectFactsV1,
    SourceUnitFactV1, TestEnvironmentFactV1, ts_oracle_kind_matches_seam,
    typescript_script_preview_related_test,
};

#[doc(hidden)]
pub mod context_packet;
mod diagnostic_witness;
mod fix_instruction;
#[doc(hidden)]
pub mod guarded_edit;
#[doc(hidden)]
pub mod language_availability;
mod next_action;
mod repair_card;
mod verification_result;

pub use diagnostic_witness::{
    DiagnosticConfidence, DiagnosticFixSite, DiagnosticSourceLocation, DiagnosticWitness,
    DiagnosticWitnessLimitation,
};
pub(crate) use diagnostic_witness::{
    contract_confidence, perl_suggested_related_test, python_suggested_related_test,
};
pub use fix_instruction::{
    FixInstructionIdentityContext, FixInstructionState, FixInstructionSummary,
    InstructionIdentityParseError, InstructionInstanceId, InstructionSemanticId,
    InstructionSnapshotCurrentness,
};
pub use next_action::{
    CanonicalNextActionV1, NextActionAlternative, NextActionCheckCase, NextActionClass,
    NextActionCommandRef, NextActionCurrentness, NextActionDiffSource, NextActionProducer,
    NextActionStop, NextActionSubject, NextActionTransition,
};
pub(crate) use next_action::{
    NO_SCOPE_BASE_COMMAND, NextActionAttemptView, NextActionDoctorRecovery, NextActionInput,
    NextActionPilotDelegation, WHOLE_REPO_PILOT_COMMAND, current_command_platform,
    select_canonical_next_action, whole_repo_pilot_command,
};
pub use repair_card::{
    AgentCardRefusalKind, CardCurrentnessGoal, DEFAULT_REPAIR_CARD_MAX_DETAIL_ITEMS,
    DEFAULT_REPAIR_CARD_MAX_INLINE_DETAIL_BYTES, DEFAULT_REPAIR_CARD_MAX_SERIALIZED_BYTES,
    EditCageGoal, FocusedExecutionGoal, MAX_REPAIR_CARD_REJECTED_ALTERNATIVES,
    MutationConfirmationGoal, REPAIR_CARD_BUDGET_VERSION, REPAIR_CARD_CLAIM_BOUNDARY,
    REPAIR_CARD_SCHEMA_VERSION, RepairCardAssertionGoal, RepairCardAttempt, RepairCardBudget,
    RepairCardCommandRef, RepairCardDetailFamily, RepairCardDetailRef, RepairCardDetailState,
    RepairCardDetailSummary, RepairCardDoneWhen, RepairCardOmissionClass,
    RepairCardProposedTestKind, RepairCardReadinessFacts, RepairCardRejectedAlternative,
    RepairCardSnapshot, RepairCardSnapshotCurrentness, RepairCardSubject, RepairCardTarget,
    RepairCardTestKind, RepairCardV1, StaticMovementGoal, repair_card_route_exposable,
};
pub(crate) use verification_result::CommandSpecDigest;
pub use verification_result::{
    VERIFICATION_EXECUTION_RESULT_SCHEMA_VERSION, VerificationCurrentnessV1,
    VerificationExecutionResultV1, VerificationExecutionResultValidationError,
    VerificationProcessDispositionV1, command_spec_sha256,
};
