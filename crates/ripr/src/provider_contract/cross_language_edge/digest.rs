use sha2::{Digest, Sha256};

use super::model::*;

fn direction_token(direction: CrossLanguageEdgeDirectionV1) -> &'static str {
    match direction {
        CrossLanguageEdgeDirectionV1::SourceToTarget => "source_to_target",
        CrossLanguageEdgeDirectionV1::TargetToSource => "target_to_source",
        CrossLanguageEdgeDirectionV1::Bidirectional => "bidirectional",
    }
}

/// Stable relationship slot for base/head movement and duplicate detection.
pub fn cross_language_edge_semantic_key(edge: &CrossLanguageEdgeV1) -> String {
    format!(
        "{}|{}|{}|{}",
        edge.source.portable_identity_key(),
        edge.target.portable_identity_key(),
        edge.boundary_family.as_str(),
        direction_token(edge.direction)
    )
}

/// Content digest for an edge independent of `edge_id` and display locators.
pub fn cross_language_edge_content_digest(
    edge: &CrossLanguageEdgeV1,
) -> Result<String, CrossLanguageEdgeRegistryErrorV1> {
    let mut normalized = edge.clone();
    normalized.edge_id.clear();
    normalized.source.display_name = None;
    normalized.target.display_name = None;
    let encoded =
        serde_json::to_vec(&normalized).map_err(|error| CrossLanguageEdgeRegistryErrorV1 {
            code: CrossLanguageEdgeRegistryErrorCodeV1::CompletenessConflict,
            message: format!("edge content serialization failed: {error}"),
        })?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

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
