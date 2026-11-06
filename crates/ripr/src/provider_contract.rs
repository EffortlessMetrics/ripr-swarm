//! Exact-snapshot RIPR provider contracts for external proof orchestration (#3298).
//!
//! The contract exposes RIPR-owned static evidence. It does not run project
//! tests, mutation tools, proof packs, builds, network clients, or source edits.
//!
//! # Consuming these DTOs
//!
//! Deserialization performs **no** validation: every invariant (snapshot
//! digest binding, completeness rules, closed vocabularies) lives in
//! `validate`, and a consumer that skips it has no contract. A document
//! that parses can still be wrong — an uppercase git object id in a
//! `snapshot_id` deserializes cleanly and is rejected only by
//! `validate()`. Always deserialize **and** validate before acting on a
//! receipt, capability set, or request.
//!
//! Forward compatibility is fail-closed by design (see `docs/SEMVER.md`):
//! `deny_unknown_fields` means an added field breaks old readers, so any
//! additive change mints a new `ripr_*_v1` schema version. Old readers
//! reject new writers loudly rather than misreading them.

mod cross_language_edge;
mod model;
mod validate;

pub use cross_language_edge::{
    BoundaryFamilyV1, CrossLanguageEdgeDirectionV1, CrossLanguageEdgeEffectiveRecordV1,
    CrossLanguageEdgeMovementKindV1, CrossLanguageEdgeMovementRecordV1,
    CrossLanguageEdgeRegistryAnomalyKindV1, CrossLanguageEdgeRegistryAnomalyV1,
    CrossLanguageEdgeRegistryCurrentnessReportV1, CrossLanguageEdgeRegistryErrorCodeV1,
    CrossLanguageEdgeRegistryErrorV1, CrossLanguageEdgeRegistryMovementReportV1,
    CrossLanguageEdgeRegistryV1, CrossLanguageEdgeV1, CrossLanguageGeneratedArtifactV1,
    CrossLanguageProjectIdentityV1, CrossLanguageSubjectV1, EdgeCurrentnessV1, EdgeDispositionV1,
    EdgeExactnessV1, EdgeLimitationV1, EdgeNonClaimV1, EdgeObligationV1, EdgeProducerBasisV1,
    EdgeStrengthV1, LoadedCrossLanguageEdgeRegistryV1,
    RIPR_CROSS_LANGUAGE_EDGE_REGISTRY_SCHEMA_VERSION,
    analyze_cross_language_edge_registry_currentness, compare_cross_language_edge_registries,
    cross_language_edge_content_digest, cross_language_edge_registry_semantic_digest,
    cross_language_edge_semantic_key, edge_connects_subjects,
    load_cross_language_edge_registry_from_json, subjects_related_by_effective_registered_edge,
    subjects_related_by_registered_edge,
};
pub use model::*;

#[cfg(test)]
mod tests;
