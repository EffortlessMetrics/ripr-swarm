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
    let mut used_base = BTreeSet::new();
    let mut used_head = BTreeSet::new();
    let mut records = Vec::new();

    for base_edge in &base.registry.edges {
        let Some(head_edge) = head
            .registry
            .edges
            .iter()
            .find(|edge| edge.edge_id == base_edge.edge_id)
        else {
            continue;
        };
        used_base.insert(base_edge.edge_id.clone());
        used_head.insert(head_edge.edge_id.clone());
        let base_key = cross_language_edge_semantic_key(base_edge);
        let head_key = cross_language_edge_semantic_key(head_edge);
        let kind = if base_key != head_key {
            CrossLanguageEdgeMovementKindV1::Moved
        } else {
            movement_kind_for_matching_content(base_edge, head_edge)?
        };
        records.push(movement_record(
            refine_head_disposition(kind, head_edge, &head_currentness),
            &head_key,
            Some(base_edge.edge_id.clone()),
            Some(head_edge.edge_id.clone()),
        ));
    }

    let mut head_by_key: BTreeMap<String, Vec<&CrossLanguageEdgeV1>> = BTreeMap::new();
    let mut base_by_key: BTreeMap<String, Vec<&CrossLanguageEdgeV1>> = BTreeMap::new();
    for edge in &head.registry.edges {
        if !used_head.contains(&edge.edge_id) {
            head_by_key
                .entry(cross_language_edge_semantic_key(edge))
                .or_default()
                .push(edge);
        }
    }
    for edge in &base.registry.edges {
        if !used_base.contains(&edge.edge_id) {
            base_by_key
                .entry(cross_language_edge_semantic_key(edge))
                .or_default()
                .push(edge);
        }
    }

    for (semantic_key, base_edges) in &base_by_key {
        let Some(head_edges) = head_by_key.get(semantic_key) else {
            continue;
        };
        for base_edge in base_edges {
            if used_base.contains(&base_edge.edge_id) {
                continue;
            }
            let Some(head_edge) = head_edges
                .iter()
                .find(|edge| !used_head.contains(&edge.edge_id))
            else {
                continue;
            };
            used_base.insert(base_edge.edge_id.clone());
            used_head.insert(head_edge.edge_id.clone());
            let kind = movement_kind_for_matching_content(base_edge, head_edge)?;
            records.push(movement_record(
                refine_head_disposition(kind, head_edge, &head_currentness),
                semantic_key,
                Some(base_edge.edge_id.clone()),
                Some(head_edge.edge_id.clone()),
            ));
        }
    }

    for head_edge in &head.registry.edges {
        if used_head.contains(&head_edge.edge_id) {
            continue;
        }
        records.push(movement_record(
            refine_head_disposition(
                CrossLanguageEdgeMovementKindV1::HeadOnlyAdded,
                head_edge,
                &head_currentness,
            ),
            &cross_language_edge_semantic_key(head_edge),
            None,
            Some(head_edge.edge_id.clone()),
        ));
    }

    for base_edge in &base.registry.edges {
        if used_base.contains(&base_edge.edge_id) {
            continue;
        }
        records.push(movement_record(
            CrossLanguageEdgeMovementKindV1::BaseOnlyDeleted,
            &cross_language_edge_semantic_key(base_edge),
            Some(base_edge.edge_id.clone()),
            None,
        ));
    }

    records.sort_by(|left, right| {
        left.semantic_key
            .cmp(&right.semantic_key)
            .then_with(|| {
                left.movement_kind
                    .as_str()
                    .cmp(right.movement_kind.as_str())
            })
            .then_with(|| left.head_edge_id.cmp(&right.head_edge_id))
            .then_with(|| left.base_edge_id.cmp(&right.base_edge_id))
    });

    Ok(CrossLanguageEdgeRegistryMovementReportV1 {
        schema_version: MOVEMENT_REPORT_SCHEMA.into(),
        base_semantic_digest: base.semantic_digest.clone(),
        head_semantic_digest: head.semantic_digest.clone(),
        records,
    })
}

fn movement_kind_for_matching_content(
    base_edge: &CrossLanguageEdgeV1,
    head_edge: &CrossLanguageEdgeV1,
) -> Result<CrossLanguageEdgeMovementKindV1, CrossLanguageEdgeRegistryErrorV1> {
    let base_digest = cross_language_edge_content_digest(base_edge)?;
    let head_digest = cross_language_edge_content_digest(head_edge)?;
    Ok(if base_digest == head_digest {
        CrossLanguageEdgeMovementKindV1::Unchanged
    } else {
        CrossLanguageEdgeMovementKindV1::Replaced
    })
}

fn movement_record(
    movement_kind: CrossLanguageEdgeMovementKindV1,
    semantic_key: &str,
    base_edge_id: Option<String>,
    head_edge_id: Option<String>,
) -> CrossLanguageEdgeMovementRecordV1 {
    CrossLanguageEdgeMovementRecordV1 {
        movement_kind,
        semantic_key: semantic_key.into(),
        base_edge_id,
        head_edge_id,
    }
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
