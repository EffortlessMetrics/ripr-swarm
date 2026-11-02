use serde::{Deserialize, Serialize};

pub const RIPR_CROSS_LANGUAGE_EDGE_REGISTRY_SCHEMA_VERSION: &str =
    "ripr_cross_language_edge_registry.v1";

/// Closed boundary-family vocabulary for registered cross-language edges.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryFamilyV1 {
    FfiBinding,
    GeneratedBinding,
    NativeAddon,
    WasmBoundary,
    SubprocessCli,
    SchemaProtocol,
    SerializationContract,
    GeneratedClient,
    SharedResource,
    ManualRegisteredBridge,
    UnknownDynamicBoundary,
}

impl BoundaryFamilyV1 {
    pub const ALL: [Self; 11] = [
        Self::FfiBinding,
        Self::GeneratedBinding,
        Self::NativeAddon,
        Self::WasmBoundary,
        Self::SubprocessCli,
        Self::SchemaProtocol,
        Self::SerializationContract,
        Self::GeneratedClient,
        Self::SharedResource,
        Self::ManualRegisteredBridge,
        Self::UnknownDynamicBoundary,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FfiBinding => "ffi_binding",
            Self::GeneratedBinding => "generated_binding",
            Self::NativeAddon => "native_addon",
            Self::WasmBoundary => "wasm_boundary",
            Self::SubprocessCli => "subprocess_cli",
            Self::SchemaProtocol => "schema_protocol",
            Self::SerializationContract => "serialization_contract",
            Self::GeneratedClient => "generated_client",
            Self::SharedResource => "shared_resource",
            Self::ManualRegisteredBridge => "manual_registered_bridge",
            Self::UnknownDynamicBoundary => "unknown_dynamic_boundary",
        }
    }
}

/// Declared cross-language edge strength. Effective strength cannot exceed the
/// weakest unresolved leg on the edge.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeStrengthV1 {
    Unsupported,
    ProcessBoundary,
    VerificationObligation,
    RegisteredBinding,
    PropagationLimited,
    OracleEstablished,
}

impl EdgeStrengthV1 {
    pub const ALL: [Self; 6] = [
        Self::Unsupported,
        Self::ProcessBoundary,
        Self::VerificationObligation,
        Self::RegisteredBinding,
        Self::PropagationLimited,
        Self::OracleEstablished,
    ];
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossLanguageEdgeDirectionV1 {
    SourceToTarget,
    TargetToSource,
    Bidirectional,
}

impl CrossLanguageEdgeDirectionV1 {
    pub const ALL: [Self; 3] = [
        Self::SourceToTarget,
        Self::TargetToSource,
        Self::Bidirectional,
    ];
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeProducerBasisV1 {
    RegisteredProducerFact,
    UnsupportedManual,
    StaleProducerFact,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeExactnessV1 {
    Exact,
    Partial,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeCurrentnessV1 {
    Current,
    Stale,
    PartialFacts,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeDispositionV1 {
    PositiveRegistered,
    UnsupportedFamily,
    UnresolvedLeg,
    Stale,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageProjectIdentityV1 {
    pub project_id: String,
    pub package_id: Option<String>,
    pub config_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageGeneratedArtifactV1 {
    pub artifact_id: String,
    pub generator_id: String,
    pub input_digest: String,
    pub output_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageSubjectV1 {
    pub language: String,
    pub subject_id: String,
    pub subject_body_digest: String,
    pub display_name: Option<String>,
    pub project: CrossLanguageProjectIdentityV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeObligationV1 {
    pub obligation_id: String,
    pub description: String,
    pub required_strength: EdgeStrengthV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeLimitationV1 {
    pub limitation_id: String,
    pub category: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeNonClaimV1 {
    pub claim: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeProducerRegistrationV1 {
    pub producer_id: String,
    pub producer_version: String,
    pub supported_families: Vec<BoundaryFamilyV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeV1 {
    pub edge_id: String,
    pub source: CrossLanguageSubjectV1,
    pub target: CrossLanguageSubjectV1,
    pub direction: CrossLanguageEdgeDirectionV1,
    pub boundary_family: BoundaryFamilyV1,
    pub producer_id: String,
    pub producer_basis: EdgeProducerBasisV1,
    pub exactness: EdgeExactnessV1,
    pub declared_strength: EdgeStrengthV1,
    pub effective_strength: EdgeStrengthV1,
    pub propagation_ceiling: EdgeStrengthV1,
    pub generated_artifacts: Vec<CrossLanguageGeneratedArtifactV1>,
    pub obligations: Vec<EdgeObligationV1>,
    pub first_unresolved_leg: Option<String>,
    pub currentness: EdgeCurrentnessV1,
    pub disposition: EdgeDispositionV1,
    pub limitations: Vec<EdgeLimitationV1>,
    pub non_claims: Vec<EdgeNonClaimV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeRegistryV1 {
    pub schema_version: String,
    pub proof_generation: String,
    pub currentness_generation: String,
    pub supported_boundary_families: Vec<BoundaryFamilyV1>,
    pub registered_producers: Vec<CrossLanguageEdgeProducerRegistrationV1>,
    pub edges: Vec<CrossLanguageEdgeV1>,
    pub semantic_digest: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossLanguageEdgeRegistryErrorCodeV1 {
    MissingField,
    UnsupportedSchema,
    DuplicateEdgeId,
    UnknownFieldRejected,
    AuthorityViolation,
    IdentityMismatch,
    IncompatibleFamilyStrength,
    ContradictoryDirection,
    CompletenessConflict,
    MalformedIdentity,
    StaleCurrentness,
    UnsupportedDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossLanguageEdgeRegistryErrorV1 {
    pub code: CrossLanguageEdgeRegistryErrorCodeV1,
    pub message: String,
}

impl std::fmt::Display for CrossLanguageEdgeRegistryErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for CrossLanguageEdgeRegistryErrorV1 {}
