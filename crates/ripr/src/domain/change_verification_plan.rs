//! Internal change-verification plan contract (`ChangeVerificationPlanV1`, #5163).
//!
//! One portable representation of what verification is known, useful, missing,
//! and conservatively required for one exact base→head or base→worktree change.
//! Built as a pure projection over producer-owned facts; it does not recompute
//! witness, relation, target, or execution meaning.

pub const CHANGE_VERIFICATION_PLAN_SCHEMA_VERSION: &str = "change_verification_plan.v1";

pub const CHANGE_VERIFICATION_PLAN_CLAIM_BOUNDARY: &str = "Static change-verification plan projected from producer facts; it does not establish safe omission, runtime outcomes, or completion.";

/// Plan-level relation evidence vocabulary (#5163). Distinct from the shared
/// witness relation reason strings; normalization happens in the projection.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PlanRelationEvidenceClass {
    ResolvedMayDepend,
    ConservativeMayDepend,
    ValidationObligation,
    ObservedExecution,
    AffinityCandidate,
    OpaqueBoundary,
}

#[expect(
    dead_code,
    reason = "wire-stable plan vocabulary token for VP2+ consumers"
)]
impl PlanRelationEvidenceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResolvedMayDepend => "resolved_may_depend",
            Self::ConservativeMayDepend => "conservative_may_depend",
            Self::ValidationObligation => "validation_obligation",
            Self::ObservedExecution => "observed_execution",
            Self::AffinityCandidate => "affinity_candidate",
            Self::OpaqueBoundary => "opaque_boundary",
        }
    }

    pub fn may_include_in_impacted_set(self) -> bool {
        matches!(
            self,
            Self::ResolvedMayDepend | Self::ConservativeMayDepend | Self::ValidationObligation
        )
    }

    pub fn may_authorize_exclusion(self) -> bool {
        false
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum WidenedObligationScope {
    ExactTests,
    TestBinary,
    Target,
    Package,
    ReverseDependants,
    Workspace,
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum VerificationObligationKind {
    Build,
    Test,
    NonTest,
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum VerificationCommandPurpose {
    FastValidation,
    ConservativeRegression,
    BroaderFallback,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanSubjectIdentity {
    pub repository: String,
    pub base_revision: Option<String>,
    pub head_revision: Option<String>,
    pub worktree_identity: Option<String>,
    pub analyzer_generation: Option<String>,
    pub schema_generation: Option<String>,
    pub language: Option<String>,
    pub config_profile: Option<String>,
    pub features: Vec<String>,
    pub manifest_lock_inputs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanChangedSubject {
    pub subject_identity: String,
    pub change_kind: String,
    pub behavior_family: String,
    pub base_source_identity: Option<String>,
    pub head_source_identity: Option<String>,
    pub source_locations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanTestIdentity {
    pub test_name: String,
    pub file: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanImpactedTest {
    pub test: ChangeVerificationPlanTestIdentity,
    pub evidence_class: PlanRelationEvidenceClass,
    pub relation_basis: String,
    pub path_identities: Vec<String>,
    pub base_presence: String,
    pub head_presence: String,
    pub target_state: String,
    pub currentness: String,
    pub applicability: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanStageStates {
    pub reach: String,
    pub activate: String,
    pub propagate: String,
    pub observe: String,
    pub discriminate: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanValidatingTest {
    pub test: ChangeVerificationPlanTestIdentity,
    pub validated_subjects: Vec<String>,
    pub stages: ChangeVerificationPlanStageStates,
    pub oracle_kind: String,
    pub oracle_strength: String,
    pub oracle_alignment: String,
    pub first_unresolved_edge: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanUncoveredSubject {
    pub subject_identity: String,
    pub earliest_missing_stage: String,
    pub missing_detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanWidenedObligation {
    pub scope: WidenedObligationScope,
    pub obligation_kind: VerificationObligationKind,
    pub widening_reason: String,
    pub originating_edge: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanCommand {
    pub command_id: String,
    pub purpose: VerificationCommandPurpose,
    pub state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanCompleteness {
    pub complete_for_declared_model: bool,
    pub conservative_with_fallback: bool,
    pub limited: bool,
    pub stale: bool,
    pub unsupported: bool,
    pub not_proven: bool,
    pub declared_model: String,
    pub unsupported_families: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeVerificationPlanV1 {
    pub schema_version: String,
    pub plan_id: String,
    pub semantic_digest: String,
    pub subject_identity: ChangeVerificationPlanSubjectIdentity,
    pub changed_subjects: Vec<ChangeVerificationPlanChangedSubject>,
    pub impacted_tests: Vec<ChangeVerificationPlanImpactedTest>,
    pub validating_tests: Vec<ChangeVerificationPlanValidatingTest>,
    pub uncovered_changed_subjects: Vec<ChangeVerificationPlanUncoveredSubject>,
    pub widened_obligations: Vec<ChangeVerificationPlanWidenedObligation>,
    pub commands: Vec<ChangeVerificationPlanCommand>,
    pub completeness: ChangeVerificationPlanCompleteness,
    pub limitations: Vec<String>,
    pub non_claims: Vec<String>,
    pub claim_boundary: String,
}

impl ChangeVerificationPlanV1 {
    pub fn digest_matches(&self) -> bool {
        crate::change_verification_plan_digest::change_verification_plan_integrity_matches(self)
    }
}
