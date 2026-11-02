use std::collections::{BTreeMap, BTreeSet};

use super::currentness::analyze_cross_language_edge_registry_currentness;
use super::digest::{cross_language_edge_content_digest, cross_language_edge_semantic_key};
use super::model::LoadedCrossLanguageEdgeRegistryV1;
use super::model::*;

const MOVEMENT_REPORT_SCHEMA: &str = "ripr_cross_language_edge_movement_report.v1";

pub fn compare_cross_language_edge_registries(
    base: &LoadedCrossLanguageEdgeRegistryV1,
    head: &LoadedCrossLanguageEdgeRegistryV1,
) -> Result<CrossLanguageEdgeRegistryMovementReportV1, CrossLanguageEdgeRegistryErrorV1> {
    let head_currentness = analyze_cross_language_edge_registry_currentness(head)?;
    let mut base_by_key: BTreeMap<String, &CrossLanguageEdgeV1> = BTreeMap::new();
    let mut head_by_key: BTreeMap<String, &CrossLanguageEdgeV1> = BTreeMap::new();
    let mut base_by_id: BTreeMap<String, &CrossLanguageEdgeV1> = BTreeMap::new();
    let mut head_by_id: BTreeMap<String, &CrossLanguageEdgeV1> = BTreeMap::new();

    for edge in &base.registry.edges {
        base_by_key.insert(cross_language_edge_semantic_key(edge), edge);
        base_by_id.insert(edge.edge_id.clone(), edge);
    }
    for edge in &head.registry.edges {
        head_by_key.insert(cross_language_edge_semantic_key(edge), edge);
        head_by_id.insert(edge.edge_id.clone(), edge);
    }

    let mut records = Vec::new();
    let mut seen_keys = BTreeSet::new();

    for (semantic_key, head_edge) in &head_by_key {
        seen_keys.insert(semantic_key.clone());
        if let Some(base_edge) = base_by_key.get(semantic_key) {
            let base_digest = cross_language_edge_content_digest(base_edge)?;
            let head_digest = cross_language_edge_content_digest(head_edge)?;
            let movement_kind = if base_digest == head_digest {
                CrossLanguageEdgeMovementKindV1::Unchanged
            } else {
                CrossLanguageEdgeMovementKindV1::Replaced
            };
            records.push(CrossLanguageEdgeMovementRecordV1 {
                movement_kind: refine_head_disposition(movement_kind, head_edge, &head_currentness),
                semantic_key: semantic_key.clone(),
                base_edge_id: Some(base_edge.edge_id.clone()),
                head_edge_id: Some(head_edge.edge_id.clone()),
            });
            continue;
        }

        if let Some(base_edge) = base_by_id.get(&head_edge.edge_id) {
            records.push(CrossLanguageEdgeMovementRecordV1 {
                movement_kind: CrossLanguageEdgeMovementKindV1::Moved,
                semantic_key: semantic_key.clone(),
                base_edge_id: Some(base_edge.edge_id.clone()),
                head_edge_id: Some(head_edge.edge_id.clone()),
            });
            continue;
        }

        records.push(CrossLanguageEdgeMovementRecordV1 {
            movement_kind: refine_head_disposition(
                CrossLanguageEdgeMovementKindV1::HeadOnlyAdded,
                head_edge,
                &head_currentness,
            ),
            semantic_key: semantic_key.clone(),
            base_edge_id: None,
            head_edge_id: Some(head_edge.edge_id.clone()),
        });
    }

    for (semantic_key, base_edge) in &base_by_key {
        if seen_keys.contains(semantic_key) {
            continue;
        }
        records.push(CrossLanguageEdgeMovementRecordV1 {
            movement_kind: CrossLanguageEdgeMovementKindV1::BaseOnlyDeleted,
            semantic_key: semantic_key.clone(),
            base_edge_id: Some(base_edge.edge_id.clone()),
            head_edge_id: None,
        });
    }

    records.sort_by(|left, right| {
        left.semantic_key.cmp(&right.semantic_key).then_with(|| {
            left.movement_kind
                .as_str()
                .cmp(right.movement_kind.as_str())
        })
    });

    Ok(CrossLanguageEdgeRegistryMovementReportV1 {
        schema_version: MOVEMENT_REPORT_SCHEMA.into(),
        base_semantic_digest: base.semantic_digest.clone(),
        head_semantic_digest: head.semantic_digest.clone(),
        records,
    })
}

impl CrossLanguageEdgeMovementKindV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HeadOnlyAdded => "head_only_added",
            Self::BaseOnlyDeleted => "base_only_deleted",
            Self::Unchanged => "unchanged",
            Self::Moved => "moved",
            Self::Replaced => "replaced",
            Self::StaleOnHead => "stale_on_head",
            Self::InvalidOnHead => "invalid_on_head",
            Self::UnsupportedFamilyOnHead => "unsupported_family_on_head",
        }
    }
}

fn refine_head_disposition(
    default: CrossLanguageEdgeMovementKindV1,
    head_edge: &CrossLanguageEdgeV1,
    head_currentness: &CrossLanguageEdgeRegistryCurrentnessReportV1,
) -> CrossLanguageEdgeMovementKindV1 {
    if default == CrossLanguageEdgeMovementKindV1::Unchanged {
        return default;
    }
    match head_edge.disposition {
        EdgeDispositionV1::Stale => CrossLanguageEdgeMovementKindV1::StaleOnHead,
        EdgeDispositionV1::UnsupportedFamily => {
            CrossLanguageEdgeMovementKindV1::UnsupportedFamilyOnHead
        }
        EdgeDispositionV1::UnresolvedLeg => CrossLanguageEdgeMovementKindV1::InvalidOnHead,
        EdgeDispositionV1::PositiveRegistered => {
            if head_currentness
                .effective_edges
                .iter()
                .find(|record| record.edge_id == head_edge.edge_id)
                .is_some_and(|record| !record.positive_relation_available)
            {
                CrossLanguageEdgeMovementKindV1::InvalidOnHead
            } else {
                default
            }
        }
    }
}
