use serde::{Deserialize, Serialize};

/// Schema version a [`CrossLanguageEdgeRegistryV1`] must carry in
/// `schema_version`; any other value fails validation.
pub const RIPR_CROSS_LANGUAGE_EDGE_REGISTRY_SCHEMA_VERSION: &str =
    "ripr_cross_language_edge_registry.v1";

/// Closed boundary-family vocabulary for registered cross-language edges.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryFamilyV1 {
    /// A foreign-function-interface binding.
    FfiBinding,
    /// A binding produced by a code generator.
    GeneratedBinding,
    /// A native addon loaded by a managed runtime.
    NativeAddon,
    /// A WebAssembly import or export boundary.
    WasmBoundary,
    /// A subprocess command-line call. Strength is capped at
    /// [`EdgeStrengthV1::ProcessBoundary`] and the edge may not be bidirectional.
    SubprocessCli,
    /// A shared schema or wire protocol. Strength is capped at
    /// [`EdgeStrengthV1::VerificationObligation`].
    SchemaProtocol,
    /// A serialization contract. Strength is capped at
    /// [`EdgeStrengthV1::VerificationObligation`].
    SerializationContract,
    /// A generated API client.
    GeneratedClient,
    /// A shared resource such as a file or database. Strength is capped at
    /// [`EdgeStrengthV1::VerificationObligation`].
    SharedResource,
    /// A hand-registered bridge.
    ManualRegisteredBridge,
    /// A dynamic boundary that cannot be resolved statically. Strength is
    /// capped at [`EdgeStrengthV1::Unsupported`] and the edge can never be
    /// positively registered.
    UnknownDynamicBoundary,
}

impl BoundaryFamilyV1 {
    /// Every boundary family, in declaration order.
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

    /// The snake_case name used in serialized registries and semantic keys.
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
    /// No supported relation; the weakest strength.
    Unsupported,
    /// The relation crosses a process boundary.
    ProcessBoundary,
    /// The relation leaves a verification obligation.
    VerificationObligation,
    /// A registered binding connects the two subjects.
    RegisteredBinding,
    /// Behavior propagates across the edge, within limits.
    PropagationLimited,
    /// An oracle across the edge is established; the strongest strength.
    OracleEstablished,
}

impl EdgeStrengthV1 {
    /// Every strength, weakest first. The derived ordering follows the same
    /// order.
    pub const ALL: [Self; 6] = [
        Self::Unsupported,
        Self::ProcessBoundary,
        Self::VerificationObligation,
        Self::RegisteredBinding,
        Self::PropagationLimited,
        Self::OracleEstablished,
    ];
}

/// Which way a cross-language edge carries behavior.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossLanguageEdgeDirectionV1 {
    /// From the source subject to the target subject.
    SourceToTarget,
    /// From the target subject to the source subject.
    TargetToSource,
    /// Both ways. Not allowed for [`BoundaryFamilyV1::SubprocessCli`].
    Bidirectional,
}

impl CrossLanguageEdgeDirectionV1 {
    /// Every direction, in declaration order.
    pub const ALL: [Self; 3] = [
        Self::SourceToTarget,
        Self::TargetToSource,
        Self::Bidirectional,
    ];
}

/// What an edge's claim rests on.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeProducerBasisV1 {
    /// A fact from a registered producer; required for a positive edge.
    RegisteredProducerFact,
    /// A manual entry with no producer support.
    UnsupportedManual,
    /// A producer fact that is out of date.
    StaleProducerFact,
}

/// How exactly an edge's endpoints were resolved.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeExactnessV1 {
    /// Both endpoints resolved exactly.
    Exact,
    /// Resolved in part.
    Partial,
    /// Not resolved; a positive edge may not carry it.
    Unsupported,
}

/// Whether an edge's facts are current.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeCurrentnessV1 {
    /// Current; required for a positive edge.
    Current,
    /// Out of date; caps the edge at [`EdgeStrengthV1::Unsupported`].
    Stale,
    /// Only some facts are current; caps the edge at
    /// [`EdgeStrengthV1::VerificationObligation`].
    PartialFacts,
}

/// The disposition a producer declared for an edge.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeDispositionV1 {
    /// A positive registered relation. Requires a registered producer that
    /// admits the family, a `RegisteredProducerFact` basis, distinct source and
    /// target languages, current facts, an exactness other than `Unsupported`,
    /// an effective strength above `Unsupported`, and no `first_unresolved_leg`.
    PositiveRegistered,
    /// The boundary family is not supported; may not claim a registered
    /// producer fact.
    UnsupportedFamily,
    /// A leg of the edge is unresolved; requires a nonblank
    /// `first_unresolved_leg`.
    UnresolvedLeg,
    /// The edge is stale; requires `Stale` currentness.
    Stale,
}

/// The project and package a subject belongs to.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageProjectIdentityV1 {
    /// Nonblank project id.
    pub project_id: String,
    /// Optional package id; nonblank when present.
    pub package_id: Option<String>,
    /// `sha256:` digest of the project configuration.
    pub config_digest: String,
}

/// A generated artifact an edge depends on, bound to its generator input
/// and output.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageGeneratedArtifactV1 {
    /// Nonblank artifact id.
    pub artifact_id: String,
    /// Nonblank id of the generator that produced it.
    pub generator_id: String,
    /// `sha256:` digest of the generator input.
    pub input_digest: String,
    /// `sha256:` digest of the generated output.
    pub output_digest: String,
}

/// One endpoint of a cross-language edge: a subject in one language and
/// project. Two subjects are the same only when language, `subject_id`, body
/// digest and project all match; a shared display name is not enough.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageSubjectV1 {
    /// Nonblank language of the subject.
    pub language: String,
    /// Nonblank subject id within that language.
    pub subject_id: String,
    /// `sha256:` digest of the subject body.
    pub subject_body_digest: String,
    /// Optional display name, nonblank when present. Left out of digests and
    /// identity comparisons.
    pub display_name: Option<String>,
    /// The project the subject belongs to.
    pub project: CrossLanguageProjectIdentityV1,
}

/// A verification obligation an edge leaves open.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeObligationV1 {
    /// Nonblank obligation id.
    pub obligation_id: String,
    /// Nonblank description.
    pub description: String,
    /// Strength the obligation allows; the edge's effective strength may not
    /// exceed it.
    pub required_strength: EdgeStrengthV1,
}

/// A stated limitation of an edge. Every field must be nonblank.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeLimitationV1 {
    /// Limitation id.
    pub limitation_id: String,
    /// Free-text category.
    pub category: String,
    /// Limitation text.
    pub message: String,
}

/// A claim an edge explicitly does not make. Both fields must be nonblank.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeNonClaimV1 {
    /// The claim not made.
    pub claim: String,
    /// Why.
    pub reason: String,
}

/// A producer registered to emit edges for some boundary families.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeProducerRegistrationV1 {
    /// Nonblank producer id, unique within the registry.
    pub producer_id: String,
    /// Nonblank producer version.
    pub producer_version: String,
    /// Families the producer may emit; nonempty, and each must be in the
    /// registry's `supported_boundary_families`.
    pub supported_families: Vec<BoundaryFamilyV1>,
}

/// One typed, producer-owned relation between two distinct subjects. A
/// positive registered edge must join two different languages; other
/// dispositions may join subjects in the same language.
///
/// Validation requires distinct source and target subjects and a boundary
/// family the registry supports, and caps `declared_strength`,
/// `effective_strength` and `propagation_ceiling` at the family's ceiling.
/// `effective_strength` and `propagation_ceiling` are also capped at the
/// weakest leg: the declared strength, the propagation ceiling, each
/// obligation's required strength, [`EdgeStrengthV1::RegisteredBinding`] when
/// a leg is unresolved, and the cap from `currentness`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeV1 {
    /// Nonblank edge id, unique within the registry.
    pub edge_id: String,
    /// Source subject.
    pub source: CrossLanguageSubjectV1,
    /// Target subject; must differ from the source.
    pub target: CrossLanguageSubjectV1,
    /// Which way behavior crosses the edge.
    pub direction: CrossLanguageEdgeDirectionV1,
    /// The kind of boundary; must be supported by the registry.
    pub boundary_family: BoundaryFamilyV1,
    /// Producer that emitted the edge; for a positive edge it must be nonblank
    /// and registered for this family.
    pub producer_id: String,
    /// What the claim rests on.
    pub producer_basis: EdgeProducerBasisV1,
    /// How exactly the endpoints were resolved.
    pub exactness: EdgeExactnessV1,
    /// Strength the producer declared.
    pub declared_strength: EdgeStrengthV1,
    /// Strength after every cap; see the type docs.
    pub effective_strength: EdgeStrengthV1,
    /// Most propagation the edge may claim.
    pub propagation_ceiling: EdgeStrengthV1,
    /// Generated artifacts the edge depends on.
    pub generated_artifacts: Vec<CrossLanguageGeneratedArtifactV1>,
    /// Open verification obligations.
    pub obligations: Vec<EdgeObligationV1>,
    /// The first unresolved leg, if any. Required for `UnresolvedLeg` and
    /// forbidden for `PositiveRegistered`.
    pub first_unresolved_leg: Option<String>,
    /// Whether the edge's facts are current.
    pub currentness: EdgeCurrentnessV1,
    /// Declared disposition.
    pub disposition: EdgeDispositionV1,
    /// Stated limitations.
    pub limitations: Vec<EdgeLimitationV1>,
    /// Claims explicitly not made.
    pub non_claims: Vec<EdgeNonClaimV1>,
}

/// Versioned registry of cross-language edges and the producers allowed to
/// emit them.
///
/// Deserializing checks only the shape and rejects unknown fields. Call
/// [`Self::validate`], or load through
/// [`load_cross_language_edge_registry_from_json`](super::load_cross_language_edge_registry_from_json),
/// before relying on a registry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeRegistryV1 {
    /// Must equal [`RIPR_CROSS_LANGUAGE_EDGE_REGISTRY_SCHEMA_VERSION`].
    pub schema_version: String,
    /// Nonblank proof generation.
    pub proof_generation: String,
    /// Nonblank generation used to judge currentness.
    pub currentness_generation: String,
    /// Families this registry supports; nonempty and without duplicates.
    pub supported_boundary_families: Vec<BoundaryFamilyV1>,
    /// Registered producers; ids unique.
    pub registered_producers: Vec<CrossLanguageEdgeProducerRegistrationV1>,
    /// The edges; ids unique.
    pub edges: Vec<CrossLanguageEdgeV1>,
    /// Optional `sha256:` digest; when present it must equal
    /// [`cross_language_edge_registry_semantic_digest`](super::cross_language_edge_registry_semantic_digest).
    pub semantic_digest: Option<String>,
}

/// Why a cross-language edge registry was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossLanguageEdgeRegistryErrorCodeV1 {
    /// A required value is blank, or a required list is empty.
    MissingField,
    /// `schema_version` is not the supported version.
    UnsupportedSchema,
    /// A duplicate edge id, producer id or supported family.
    DuplicateEdgeId,
    /// Reserved: no current check returns it. Unknown fields fail JSON
    /// parsing, which the loader reports as `MalformedIdentity`.
    UnknownFieldRejected,
    /// A positive edge lacks a registered producer for its family or a
    /// registered-producer basis, or an unsupported-family edge claims one.
    AuthorityViolation,
    /// `semantic_digest` does not match the registry content.
    IdentityMismatch,
    /// A strength exceeds the family ceiling or the weakest leg, or a positive
    /// edge is `Unsupported`.
    IncompatibleFamilyStrength,
    /// The source and target are the same subject, or a subprocess edge is
    /// bidirectional.
    ContradictoryDirection,
    /// A positive edge is not current, has an unresolved leg or unsupported
    /// exactness. Digest serialization failures also use this code.
    CompletenessConflict,
    /// A digest is not `sha256:` plus 64 hex digits, or the registry JSON
    /// failed to parse.
    MalformedIdentity,
    /// A `Stale` disposition without `Stale` currentness.
    StaleCurrentness,
    /// A family is not supported where it is used, or a disposition is not
    /// allowed for the edge.
    UnsupportedDisposition,
}

/// A registry failure: a code plus a human-readable message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossLanguageEdgeRegistryErrorV1 {
    /// Why the registry was refused.
    pub code: CrossLanguageEdgeRegistryErrorCodeV1,
    /// Human-readable detail.
    pub message: String,
}

impl std::fmt::Display for CrossLanguageEdgeRegistryErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for CrossLanguageEdgeRegistryErrorV1 {}

/// Base/head movement classification aligned with dependency-index match kinds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossLanguageEdgeMovementKindV1 {
    /// The edge exists only on head. A stale, unresolved-leg or
    /// unsupported-family head edge, or a positive head edge whose positive
    /// relation the head currentness analysis withdraws, is reported with the
    /// matching `*OnHead` kind instead.
    HeadOnlyAdded,
    /// The edge exists only on base.
    BaseOnlyDeleted,
    /// Same relationship slot and content on both sides. Head currentness is
    /// not consulted, so an unchanged edge that a new head anomaly demotes is
    /// still reported here.
    Unchanged,
    /// The same `edge_id` now names a different relationship slot. Refined
    /// to an `*OnHead` kind like `HeadOnlyAdded`.
    Moved,
    /// Same relationship slot, different content. Refined to an `*OnHead`
    /// kind like `HeadOnlyAdded`.
    Replaced,
    /// Present on head (added, moved or replaced), and the head edge is stale.
    StaleOnHead,
    /// Present on head (added, moved or replaced), and the head edge has an
    /// unresolved leg or its positive relation is not available.
    InvalidOnHead,
    /// Present on head (added, moved or replaced), and the head edge's family
    /// is unsupported.
    UnsupportedFamilyOnHead,
}

/// A registry-level problem the currentness analysis found.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossLanguageEdgeRegistryAnomalyKindV1 {
    /// Two edges state the same relationship with identical content.
    DuplicateSemanticEdge,
    /// Two edges share a relationship slot with different content.
    ConflictingEdge,
    /// Two positive edges of one family claim the same endpoint pair, in
    /// either orientation. Identical same-direction duplicates are flagged
    /// too, so both lose their positive relation.
    ReversedEdge,
    /// A positive bidirectional edge claims a propagation ceiling of
    /// `PropagationLimited` or more.
    OverBroadEdge,
    /// A positive edge's producer does not admit its family.
    DanglingEdge,
    /// One generated artifact identity binds edges in different project scopes.
    ArtifactCrossBind,
    /// A positive edge names a producer that is not registered.
    MissingProducerRegistration,
}

/// One anomaly found by the currentness analysis.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeRegistryAnomalyV1 {
    /// What kind of anomaly.
    pub kind: CrossLanguageEdgeRegistryAnomalyKindV1,
    /// The edge the anomaly names.
    pub edge_id: String,
    /// The other edge involved, for pairwise anomalies.
    pub related_edge_id: Option<String>,
    /// Human-readable detail.
    pub message: String,
}

/// An edge's disposition after the currentness analysis. An edge declared
/// positive can be demoted here, for example by an anomaly or a missing
/// producer registration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeEffectiveRecordV1 {
    /// The edge.
    pub edge_id: String,
    /// Disposition after the analysis.
    pub effective_disposition: EdgeDispositionV1,
    /// The first unresolved leg after the analysis, if any.
    pub first_unresolved_leg: Option<String>,
    /// Whether a consumer may treat the edge as a positive relation.
    pub positive_relation_available: bool,
}

/// Result of
/// [`analyze_cross_language_edge_registry_currentness`](super::analyze_cross_language_edge_registry_currentness).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeRegistryCurrentnessReportV1 {
    /// Always `ripr_cross_language_edge_currentness_report.v1`.
    pub schema_version: String,
    /// Copied from the registry.
    pub proof_generation: String,
    /// Copied from the registry.
    pub currentness_generation: String,
    /// The loaded registry's semantic digest.
    pub semantic_digest: String,
    /// One record per edge, sorted by `edge_id`.
    pub effective_edges: Vec<CrossLanguageEdgeEffectiveRecordV1>,
    /// Anomalies sorted by kind then edge, capped at 32. Edge demotion uses
    /// the full set, not this capped list.
    pub anomalies: Vec<CrossLanguageEdgeRegistryAnomalyV1>,
    /// Suggested fixes, sorted, deduplicated and capped at 8.
    pub recovery_guidance: Vec<String>,
}

/// How one relationship moved between base and head.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeMovementRecordV1 {
    /// What happened to the relationship.
    pub movement_kind: CrossLanguageEdgeMovementKindV1,
    /// The relationship slot, from
    /// [`cross_language_edge_semantic_key`](super::cross_language_edge_semantic_key).
    pub semantic_key: String,
    /// The base edge, absent when the edge is only on head.
    pub base_edge_id: Option<String>,
    /// The head edge, absent when the edge is only on base.
    pub head_edge_id: Option<String>,
}

/// Result of
/// [`compare_cross_language_edge_registries`](super::compare_cross_language_edge_registries).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossLanguageEdgeRegistryMovementReportV1 {
    /// Always `ripr_cross_language_edge_movement_report.v1`.
    pub schema_version: String,
    /// Semantic digest of the base registry.
    pub base_semantic_digest: String,
    /// Semantic digest of the head registry.
    pub head_semantic_digest: String,
    /// Movement records, sorted by semantic key, then kind, then edge ids.
    pub records: Vec<CrossLanguageEdgeMovementRecordV1>,
}

/// A registry that has been parsed and validated, with its semantic digest.
/// Built by
/// [`load_cross_language_edge_registry_from_json`](super::load_cross_language_edge_registry_from_json).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedCrossLanguageEdgeRegistryV1 {
    /// The validated registry.
    pub registry: CrossLanguageEdgeRegistryV1,
    /// Its computed semantic digest.
    pub semantic_digest: String,
}
