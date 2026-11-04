use serde::{Deserialize, Serialize};

pub const RIPR_ADAPTER_SUBSTRATE_QUALIFICATION_SCHEMA_VERSION: &str =
    "ripr_adapter_substrate_qualification.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterSubstrateClassV1 {
    InProcessParser,
    GeneratedParser,
    CompilerIndexService,
    ExternalHelper,
    PacketProducerConsumer,
    ProjectBuildMetadataReader,
    NativeLibrary,
    Hybrid,
}

impl AdapterSubstrateClassV1 {
    pub const ALL: [Self; 8] = [
        Self::InProcessParser,
        Self::GeneratedParser,
        Self::CompilerIndexService,
        Self::ExternalHelper,
        Self::PacketProducerConsumer,
        Self::ProjectBuildMetadataReader,
        Self::NativeLibrary,
        Self::Hybrid,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::InProcessParser => "in_process_parser",
            Self::GeneratedParser => "generated_parser",
            Self::CompilerIndexService => "compiler_index_service",
            Self::ExternalHelper => "external_helper",
            Self::PacketProducerConsumer => "packet_producer_consumer",
            Self::ProjectBuildMetadataReader => "project_build_metadata_reader",
            Self::NativeLibrary => "native_library",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterSubstrateLayerClassV1 {
    SyntaxOnly,
    CompilerResolved,
    Semantic,
    Hybrid,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityCurrentnessV1 {
    Current,
    Stale,
    Partial,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageInclusionDispositionV1 {
    DefaultIncluded,
    OptionalFeature,
    ExplicitlyExcluded,
    NotPackaged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticClaimCeilingV1 {
    SyntaxFactsOnly,
    CompilerFactsWithFallback,
    PacketFactsOnly,
    NativePlatformLimited,
    NoLocalSemanticAuthority,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateAuthorityRefV1 {
    pub ref_kind: AdapterSubstrateAuthorityRefKindV1,
    pub ref_id: String,
    pub ref_generation: String,
    pub currentness: AuthorityCurrentnessV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterSubstrateAuthorityRefKindV1 {
    AdrDocument,
    SafetyHarnessReceipt,
    CapabilityManifest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateDependencyRefV1 {
    pub dependency_id: String,
    pub crate_or_tool: String,
    pub exact_version: String,
    pub grammar_or_schema_generation: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateSafetyRefV1 {
    pub receipt_id: String,
    pub harness_generation: String,
    pub no_execution: bool,
    pub no_network: bool,
    pub root_containment: bool,
    pub process_tree_bounded: bool,
    pub currentness: AuthorityCurrentnessV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateClaimCeilingV1 {
    pub ceiling: SemanticClaimCeilingV1,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateLimitationV1 {
    pub limitation_id: String,
    pub category: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateNonClaimV1 {
    pub claim: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstratePackagingImplicationsV1 {
    pub license_spdx: String,
    pub msrv: String,
    pub unsafe_surface: String,
    pub build_script: String,
    pub generated_code: String,
    pub native_runtime: String,
    pub platform_limits: Vec<String>,
    pub compile_size_class: String,
    pub binary_size_class: String,
    pub distribution_notes: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateInputBehaviorProfileV1 {
    pub exact_span: String,
    pub malformed_partial_input: String,
    pub source_map: String,
    pub generated_code: String,
    pub unavailable_tool: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateLayerV1 {
    pub layer_id: String,
    pub layer_class: AdapterSubstrateLayerClassV1,
    pub claim_ceiling: SubstrateClaimCeilingV1,
    pub dependency_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateSupersessionRecordV1 {
    pub superseded_qualification_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateQualificationV1 {
    pub schema_version: String,
    pub qualification_id: String,
    pub controller: String,
    pub language: String,
    pub dialect: Option<String>,
    pub framework: Option<String>,
    pub platform: String,
    pub analysis_modes: Vec<String>,
    pub substrate_class: AdapterSubstrateClassV1,
    /// When TypeScript and JavaScript share one implementation, both receipts
    /// must repeat the same family id while keeping distinct `language` cells.
    pub shared_adapter_family_id: Option<String>,
    pub layers: Vec<AdapterSubstrateLayerV1>,
    pub dependencies: Vec<AdapterSubstrateDependencyRefV1>,
    pub adr_ref: AdapterSubstrateAuthorityRefV1,
    pub producer_owner: String,
    pub source_input_identity: String,
    pub project_configuration_identity: String,
    pub platform_identity: String,
    pub currentness_generation: String,
    pub input_behavior: SubstrateInputBehaviorProfileV1,
    pub packaging: SubstratePackagingImplicationsV1,
    pub safety_refs: Vec<AdapterSubstrateSafetyRefV1>,
    pub package_inclusion_disposition: PackageInclusionDispositionV1,
    pub claim_ceiling: SubstrateClaimCeilingV1,
    pub required_fallback: Option<String>,
    pub limitations: Vec<SubstrateLimitationV1>,
    pub non_claims: Vec<SubstrateNonClaimV1>,
    pub semantic_digest: Option<String>,
    pub supersession_history: Vec<SubstrateSupersessionRecordV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterSubstrateQualificationErrorCodeV1 {
    MissingField,
    UnsupportedSchema,
    DuplicateIdentity,
    UnknownFieldRejected,
    AuthorityViolation,
    IdentityMismatch,
    ImpossibleClaimCeiling,
    ContradictoryLayers,
    StaleAuthorityReference,
    MissingPackagingDisposition,
    UnsupportedSubstrateClass,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterSubstrateQualificationErrorV1 {
    pub code: AdapterSubstrateQualificationErrorCodeV1,
    pub message: String,
}

impl std::fmt::Display for AdapterSubstrateQualificationErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for AdapterSubstrateQualificationErrorV1 {}
