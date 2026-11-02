use sha2::{Digest, Sha256};

use super::model::*;

/// Canonical portable semantic digest for a registry document.
///
/// Volatile transport fields such as `semantic_digest` and subject
/// `display_name` locators are excluded. Edge order is normalized by `edge_id`.
pub fn cross_language_edge_registry_semantic_digest(
    registry: &CrossLanguageEdgeRegistryV1,
) -> Result<String, CrossLanguageEdgeRegistryErrorV1> {
    let mut edges = registry.edges.clone();
    for edge in &mut edges {
        edge.source.display_name = None;
        edge.target.display_name = None;
    }
    edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
    let mut supported = registry.supported_boundary_families.clone();
    supported.sort();
    let mut producers = registry.registered_producers.clone();
    producers.sort_by(|left, right| left.producer_id.cmp(&right.producer_id));
    let canonical = CrossLanguageEdgeRegistryV1 {
        schema_version: registry.schema_version.clone(),
        proof_generation: registry.proof_generation.clone(),
        currentness_generation: registry.currentness_generation.clone(),
        supported_boundary_families: supported,
        registered_producers: producers,
        edges,
        semantic_digest: None,
    };
    let encoded =
        serde_json::to_vec(&canonical).map_err(|error| CrossLanguageEdgeRegistryErrorV1 {
            code: CrossLanguageEdgeRegistryErrorCodeV1::CompletenessConflict,
            message: format!("registry semantic serialization failed: {error}"),
        })?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}
