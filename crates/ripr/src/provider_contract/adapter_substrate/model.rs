use serde::{Deserialize, Serialize};

/// Schema version an [`AdapterSubstrateQualificationV1`] must carry in
/// `schema_version`; any other value fails validation.
pub const RIPR_ADAPTER_SUBSTRATE_QUALIFICATION_SCHEMA_VERSION: &str =
    "ripr_adapter_substrate_qualification.v1";

/// What kind of substrate backs a language adapter. Validation applies
/// class-specific rules; see [`AdapterSubstrateQualificationV1::validate`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterSubstrateClassV1 {
    /// Parser linked into the `ripr` process; may not expose a
    /// compiler-resolved layer under a syntax-facts-only ceiling.
    InProcessParser,
    /// Generated parser, whether generated at build time or checked in as
    /// tables. Requires a nonblank `build_script` disposition (`none` is
    /// accepted, for checked-in tables) and a `generated_code` disposition
    /// other than `none`.
    GeneratedParser,
    /// Compiler or language-server index service.
    CompilerIndexService,
    /// Separate helper process; requires a registered syntax-only
    /// `required_fallback` for when the helper is unavailable.
    ExternalHelper,
    /// Exchanges evidence packets with an external producer. The receipt
    /// ceiling and every layer ceiling must be `PacketFactsOnly` or
    /// `NoLocalSemanticAuthority`, and every layer must be syntax-only.
    PacketProducerConsumer,
    /// Reads project build metadata.
    ProjectBuildMetadataReader,
    /// Native library; requires nonempty `packaging.platform_limits`.
    NativeLibrary,
    /// Combination of the classes above; must state typed limitations.
    Hybrid,
}

impl AdapterSubstrateClassV1 {
    /// Every substrate class, in declaration order.
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

    /// The snake_case name used in serialized receipts.
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

/// How much resolution one layer of a substrate performs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterSubstrateLayerClassV1 {
    /// Syntax only, no name or type resolution.
    SyntaxOnly,
    /// Resolved by a compiler.
    CompilerResolved,
    /// Semantic analysis beyond syntax.
    Semantic,
    /// Mix of syntax and resolved facts.
    Hybrid,
}

/// Whether a referenced authority (an ADR or a safety receipt) is current.
/// `Stale` references fail validation; `Partial` ones are accepted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityCurrentnessV1 {
    /// Matches the current generation.
    Current,
    /// Out of date; fails validation.
    Stale,
    /// Partly current.
    Partial,
}

/// Whether and how the substrate ships in the `ripr` package.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageInclusionDispositionV1 {
    /// Included in the default build.
    DefaultIncluded,
    /// Available behind an optional feature.
    OptionalFeature,
    /// Deliberately left out of the package.
    ExplicitlyExcluded,
    /// Not packaged at all.
    NotPackaged,
}

/// The strongest kind of semantic fact a substrate or layer may support.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticClaimCeilingV1 {
    /// Syntax facts only.
    SyntaxFactsOnly,
    /// Compiler facts, falling back to syntax. As the receipt ceiling it
    /// requires at least one compiler-resolved, semantic or hybrid layer. A
    /// syntax-only layer may not use it as its own ceiling.
    CompilerFactsWithFallback,
    /// Only facts carried by external packets.
    PacketFactsOnly,
    /// Facts limited by native platform support.
    NativePlatformLimited,
    /// No local semantic authority.
    NoLocalSemanticAuthority,
    /// Not supported.
    Unsupported,
}

/// A reference to an authority document or receipt by id and generation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateAuthorityRefV1 {
    /// What the reference points at.
    pub ref_kind: AdapterSubstrateAuthorityRefKindV1,
    /// Nonblank id of the referenced document or receipt.
    pub ref_id: String,
    /// Nonblank generation of the referenced item.
    pub ref_generation: String,
    /// Whether the reference is current.
    pub currentness: AuthorityCurrentnessV1,
}

/// What an [`AdapterSubstrateAuthorityRefV1`] points at.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterSubstrateAuthorityRefKindV1 {
    /// An architecture decision record; the only kind `adr_ref` accepts.
    AdrDocument,
    /// A safety-harness receipt.
    SafetyHarnessReceipt,
    /// A capability manifest.
    CapabilityManifest,
}

/// One pinned crate or tool the substrate depends on. Validation requires
/// every field nonblank and `dependency_id` unique within the receipt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateDependencyRefV1 {
    /// Receipt-local id that layers name in `dependency_ids`.
    pub dependency_id: String,
    /// Crate or tool name.
    pub crate_or_tool: String,
    /// Exact pinned version.
    pub exact_version: String,
    /// Grammar or schema generation the dependency provides.
    pub grammar_or_schema_generation: String,
}

/// A reference to a safety-harness receipt. Validation requires a nonblank
/// `receipt_id` and `harness_generation`, a non-stale `currentness`, and all
/// four containment flags `true`; it does not open the referenced receipt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateSafetyRefV1 {
    /// Id of the safety receipt; unique among `safety_refs`.
    pub receipt_id: String,
    /// Generation of the safety harness that produced the receipt.
    pub harness_generation: String,
    /// The receipt established that no project code runs.
    pub no_execution: bool,
    /// The receipt established no network access.
    pub no_network: bool,
    /// The receipt established that reads stay inside the root.
    pub root_containment: bool,
    /// The receipt established a bounded process tree.
    pub process_tree_bounded: bool,
    /// Whether the receipt is current; `Stale` fails validation.
    pub currentness: AuthorityCurrentnessV1,
}

/// A claim ceiling with its explanation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateClaimCeilingV1 {
    /// The ceiling itself.
    pub ceiling: SemanticClaimCeilingV1,
    /// Nonblank explanation of the ceiling.
    pub message: String,
}

/// A typed limitation of the substrate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateLimitationV1 {
    /// Limitation id; unique within the receipt.
    pub limitation_id: String,
    /// Free-text category.
    pub category: String,
    /// Limitation text. A `|locator:` suffix is checkout-local and is left
    /// out of the semantic digest.
    pub message: String,
}

/// A claim the substrate explicitly does not make, with the reason.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateNonClaimV1 {
    /// The claim not made; unique within the receipt.
    pub claim: String,
    /// Why.
    pub reason: String,
}

/// What shipping the substrate implies for the package. Validation requires
/// every string field nonblank and `license_spdx` not `unknown`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstratePackagingImplicationsV1 {
    /// SPDX license expression; may not be `unknown`.
    pub license_spdx: String,
    /// Minimum supported Rust version the substrate needs.
    pub msrv: String,
    /// Description of any `unsafe` code it brings in.
    pub unsafe_surface: String,
    /// Build-script disposition.
    pub build_script: String,
    /// Generated-code disposition; `none` counts as absent for a generated
    /// parser.
    pub generated_code: String,
    /// Native runtime requirements.
    pub native_runtime: String,
    /// Platform limits; must be nonempty for a native library.
    pub platform_limits: Vec<String>,
    /// Rough compile-cost class.
    pub compile_size_class: String,
    /// Rough binary-size class.
    pub binary_size_class: String,
    /// Free-text distribution notes.
    pub distribution_notes: String,
}

/// How the substrate behaves on difficult input, as free-text dispositions.
/// Validation requires every field nonblank and `unavailable_tool` not
/// `clean_result`: a missing tool must never read as a clean result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateInputBehaviorProfileV1 {
    /// Whether reported spans are exact.
    pub exact_span: String,
    /// Behavior on malformed or partial input.
    pub malformed_partial_input: String,
    /// Source-map handling.
    pub source_map: String,
    /// Behavior on generated code.
    pub generated_code: String,
    /// Behavior when a tool or runtime is unavailable; may not be
    /// `clean_result`.
    pub unavailable_tool: String,
}

/// One layer of a substrate and the claims it may support.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateLayerV1 {
    /// Nonblank layer id, unique within the receipt.
    pub layer_id: String,
    /// How much resolution the layer performs.
    pub layer_class: AdapterSubstrateLayerClassV1,
    /// Ceiling for this layer; its message must be nonblank.
    pub claim_ceiling: SubstrateClaimCeilingV1,
    /// Dependencies the layer uses; each must name an entry in the
    /// receipt's `dependencies`.
    pub dependency_ids: Vec<String>,
}

/// Records an earlier qualification this one replaces.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstrateSupersessionRecordV1 {
    /// Id of the replaced qualification; unique within the history.
    pub superseded_qualification_id: String,
    /// Why.
    pub reason: String,
}

/// Versioned receipt recording how one language adapter's substrate was
/// qualified and the strongest claims it may support.
///
/// Deserializing checks only the shape and rejects unknown fields. Call
/// [`Self::validate`] for the qualification rules before relying on a
/// receipt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSubstrateQualificationV1 {
    /// Must equal [`RIPR_ADAPTER_SUBSTRATE_QUALIFICATION_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Nonblank id of this qualification.
    pub qualification_id: String,
    /// Nonblank controller name.
    pub controller: String,
    /// The one language this receipt qualifies; nonblank and may not
    /// contain a comma.
    pub language: String,
    /// Optional language dialect.
    pub dialect: Option<String>,
    /// Optional framework.
    pub framework: Option<String>,
    /// Nonblank target platform.
    pub platform: String,
    /// Analysis modes covered; must be nonempty.
    pub analysis_modes: Vec<String>,
    /// The kind of substrate.
    pub substrate_class: AdapterSubstrateClassV1,
    /// When TypeScript and JavaScript share one implementation, both receipts
    /// must repeat the same family id while keeping distinct `language` cells.
    /// Nonblank when present.
    pub shared_adapter_family_id: Option<String>,
    /// Substrate layers; must be nonempty.
    pub layers: Vec<AdapterSubstrateLayerV1>,
    /// Pinned dependencies the layers may reference.
    pub dependencies: Vec<AdapterSubstrateDependencyRefV1>,
    /// The governing ADR: must be an `AdrDocument` reference with a nonblank id
    /// and generation, and not stale.
    pub adr_ref: AdapterSubstrateAuthorityRefV1,
    /// Nonblank owner of the producer.
    pub producer_owner: String,
    /// Nonblank identity of the source input.
    pub source_input_identity: String,
    /// Nonblank identity of the project configuration.
    pub project_configuration_identity: String,
    /// Nonblank identity of the platform.
    pub platform_identity: String,
    /// Nonblank generation used to judge currentness.
    pub currentness_generation: String,
    /// Behavior on difficult input.
    pub input_behavior: SubstrateInputBehaviorProfileV1,
    /// Packaging implications.
    pub packaging: SubstratePackagingImplicationsV1,
    /// Safety-harness receipts; ids unique.
    pub safety_refs: Vec<AdapterSubstrateSafetyRefV1>,
    /// Whether and how the substrate ships.
    pub package_inclusion_disposition: PackageInclusionDispositionV1,
    /// Ceiling for the whole receipt; its message must be nonblank.
    pub claim_ceiling: SubstrateClaimCeilingV1,
    /// Fallback used when an external helper is unavailable; required for
    /// [`AdapterSubstrateClassV1::ExternalHelper`] and then must be a
    /// registered syntax-only fallback.
    pub required_fallback: Option<String>,
    /// Typed limitations; ids unique, and nonempty for a hybrid substrate.
    pub limitations: Vec<SubstrateLimitationV1>,
    /// Claims explicitly not made; `claim` values unique.
    pub non_claims: Vec<SubstrateNonClaimV1>,
    /// Optional `sha256:` digest; when present it must equal
    /// [`adapter_substrate_qualification_semantic_digest`](super::adapter_substrate_qualification_semantic_digest).
    pub semantic_digest: Option<String>,
    /// Earlier qualifications this one replaces.
    pub supersession_history: Vec<SubstrateSupersessionRecordV1>,
}

/// Why an [`AdapterSubstrateQualificationV1`] failed validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterSubstrateQualificationErrorCodeV1 {
    /// A required value is blank, or a required list is empty.
    MissingField,
    /// `schema_version` is not the supported version.
    UnsupportedSchema,
    /// Two entries in a list share an identity key.
    DuplicateIdentity,
    /// Reserved: an unknown field was rejected. `validate` does not return it;
    /// unknown fields fail at deserialization instead.
    UnknownFieldRejected,
    /// A reference or disposition oversteps its authority.
    AuthorityViolation,
    /// A digest is malformed or does not match the content.
    IdentityMismatch,
    /// A claim ceiling the substrate or layers cannot support.
    ImpossibleClaimCeiling,
    /// Layers contradict the substrate class or ceiling. The semantic digest
    /// also returns it if the receipt cannot be serialized.
    ContradictoryLayers,
    /// A referenced ADR or safety receipt is stale.
    StaleAuthorityReference,
    /// A required packaging disposition is missing or unknown.
    MissingPackagingDisposition,
    /// Reserved: the substrate class is not supported. `validate` does not
    /// return it.
    UnsupportedSubstrateClass,
}

/// A validation failure: a code plus a human-readable message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterSubstrateQualificationErrorV1 {
    /// Why validation failed.
    pub code: AdapterSubstrateQualificationErrorCodeV1,
    /// Human-readable detail.
    pub message: String,
}

impl std::fmt::Display for AdapterSubstrateQualificationErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for AdapterSubstrateQualificationErrorV1 {}
