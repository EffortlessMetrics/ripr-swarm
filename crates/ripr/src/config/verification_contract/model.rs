//! DTOs for `RepositoryVerificationContractV1` (#5223 / #6428).

use std::collections::BTreeMap;

pub(crate) const REPOSITORY_VERIFICATION_CONTRACT_SCHEMA_VERSION: &str = "1";
pub(crate) const REPOSITORY_VERIFICATION_CONTRACT_COMPILER_GENERATION: &str = "vp29a-1";

/// Closed admitted source classes (#5223).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum VerificationContractSourceClass {
    RiprBuiltinPolicy,
    TrackedRiprConfiguration,
    CargoManifestOrMetadata,
    NextestOrRunnerConfiguration,
    RegisteredHarnessManifest,
    RepositoryProofOrProductGateRegistry,
    SpecializedProofRegistry,
    ExecutionContextRegistry,
    ProviderCapabilityRegistry,
    CommandCatalog,
    ValidatedGeneratedManifest,
    LocalOperatorRestriction,
    RuntimeObservationReference,
}

impl VerificationContractSourceClass {
    pub(crate) const ALL: [Self; 13] = [
        Self::RiprBuiltinPolicy,
        Self::TrackedRiprConfiguration,
        Self::CargoManifestOrMetadata,
        Self::NextestOrRunnerConfiguration,
        Self::RegisteredHarnessManifest,
        Self::RepositoryProofOrProductGateRegistry,
        Self::SpecializedProofRegistry,
        Self::ExecutionContextRegistry,
        Self::ProviderCapabilityRegistry,
        Self::CommandCatalog,
        Self::ValidatedGeneratedManifest,
        Self::LocalOperatorRestriction,
        Self::RuntimeObservationReference,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::RiprBuiltinPolicy => "ripr_builtin_policy",
            Self::TrackedRiprConfiguration => "tracked_ripr_configuration",
            Self::CargoManifestOrMetadata => "cargo_manifest_or_metadata",
            Self::NextestOrRunnerConfiguration => "nextest_or_runner_configuration",
            Self::RegisteredHarnessManifest => "registered_harness_manifest",
            Self::RepositoryProofOrProductGateRegistry => {
                "repository_proof_or_product_gate_registry"
            }
            Self::SpecializedProofRegistry => "specialized_proof_registry",
            Self::ExecutionContextRegistry => "execution_context_registry",
            Self::ProviderCapabilityRegistry => "provider_capability_registry",
            Self::CommandCatalog => "command_catalog",
            Self::ValidatedGeneratedManifest => "validated_generated_manifest",
            Self::LocalOperatorRestriction => "local_operator_restriction",
            Self::RuntimeObservationReference => "runtime_observation_reference",
        }
    }
}

/// One admitted source generation participating in the contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepositoryVerificationContractSourceV1 {
    pub source_id: String,
    pub class: VerificationContractSourceClass,
    pub schema_generation: String,
    pub location: String,
    pub trust_class: String,
    pub owned_domains: Vec<String>,
    pub content_digest: String,
    pub limitations: Vec<String>,
}

/// Normalized declaration row reference with field-level provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepositoryVerificationDeclarationRefV1 {
    pub declaration_id: String,
    pub kind: String,
    pub owner_registry: String,
    pub source_id: String,
    pub exact_location: String,
    pub semantic_fields_digest: String,
}

/// Maps declaration domains to authoritative native/product owners.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RepositoryVerificationAuthorityMapV1 {
    pub by_domain: BTreeMap<String, String>,
}

/// One material conflict surfaced by compilation (never first-error-only).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerificationContractConflictV1 {
    pub code: String,
    pub message: String,
    pub source_ids: Vec<String>,
    pub repair_locations: Vec<String>,
}

/// Closed compiler decision states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VerificationContractDecisionState {
    Complete,
    Partial,
    Conflicting,
}

/// Compiled join over current declaration sources (#5223).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepositoryVerificationContractV1 {
    pub schema_version: String,
    pub compiler_generation: String,
    pub repository_root: String,
    pub config_identity_digest: String,
    pub sources: Vec<RepositoryVerificationContractSourceV1>,
    pub declarations: Vec<RepositoryVerificationDeclarationRefV1>,
    pub authority_map: RepositoryVerificationAuthorityMapV1,
    pub conflicts: Vec<VerificationContractConflictV1>,
    pub missing_registrations: Vec<String>,
    pub semantic_digest: String,
}

/// Decision envelope with accepted/rejected rows (#5223).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepositoryVerificationContractDecisionV1 {
    pub state: VerificationContractDecisionState,
    pub contract: RepositoryVerificationContractV1,
    pub rejected_stronger_states: Vec<String>,
}
