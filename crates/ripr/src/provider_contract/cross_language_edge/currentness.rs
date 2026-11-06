use std::collections::BTreeMap;

use super::digest::{cross_language_edge_content_digest, cross_language_edge_semantic_key};
use super::model::LoadedCrossLanguageEdgeRegistryV1;
use super::model::*;

const CURRENTNESS_REPORT_SCHEMA: &str = "ripr_cross_language_edge_currentness_report.v1";
const MAX_ANOMALIES: usize = 32;
const MAX_GUIDANCE: usize = 8;

pub fn analyze_cross_language_edge_registry_currentness(
    loaded: &LoadedCrossLanguageEdgeRegistryV1,
) -> Result<CrossLanguageEdgeRegistryCurrentnessReportV1, CrossLanguageEdgeRegistryErrorV1> {
    let mut effective_edges = Vec::with_capacity(loaded.registry.edges.len());
    let mut anomalies = Vec::new();
    let mut recovery_guidance = Vec::new();
    let mut semantic_index: BTreeMap<String, Vec<&CrossLanguageEdgeV1>> = BTreeMap::new();
    let mut artifact_index: BTreeMap<
        String,
        Vec<(&CrossLanguageEdgeV1, &CrossLanguageGeneratedArtifactV1)>,
    > = BTreeMap::new();

    for edge in &loaded.registry.edges {
        semantic_index
            .entry(cross_language_edge_semantic_key(edge))
            .or_default()
            .push(edge);
        for artifact in &edge.generated_artifacts {
            artifact_index
                .entry(artifact.artifact_id.clone())
                .or_default()
                .push((edge, artifact));
        }
    }

    detect_duplicate_and_conflicting_edges(&semantic_index, &mut anomalies);
    detect_reversed_positive_edges(&loaded.registry.edges, &mut anomalies);
    detect_over_broad_edges(&loaded.registry.edges, &mut anomalies);
    detect_artifact_cross_bind(&artifact_index, &mut anomalies);

    for edge in &loaded.registry.edges {
        let (effective_disposition, first_unresolved_leg, positive_relation_available) =
            effective_edge_state(loaded, edge, &mut anomalies, &mut recovery_guidance);
        effective_edges.push(CrossLanguageEdgeEffectiveRecordV1 {
            edge_id: edge.edge_id.clone(),
            effective_disposition,
            first_unresolved_leg,
            positive_relation_available,
        });
    }

    effective_edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
    anomalies.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.edge_id.cmp(&right.edge_id))
    });
    anomalies.truncate(MAX_ANOMALIES);
    recovery_guidance.sort();
    recovery_guidance.dedup();
    recovery_guidance.truncate(MAX_GUIDANCE);

    Ok(CrossLanguageEdgeRegistryCurrentnessReportV1 {
        schema_version: CURRENTNESS_REPORT_SCHEMA.into(),
        proof_generation: loaded.registry.proof_generation.clone(),
        currentness_generation: loaded.registry.currentness_generation.clone(),
        semantic_digest: loaded.semantic_digest.clone(),
        effective_edges,
        anomalies,
        recovery_guidance,
    })
}

pub fn subjects_related_by_effective_registered_edge(
    left: &CrossLanguageSubjectV1,
    right: &CrossLanguageSubjectV1,
    loaded: &LoadedCrossLanguageEdgeRegistryV1,
) -> Result<bool, CrossLanguageEdgeRegistryErrorV1> {
    let report = analyze_cross_language_edge_registry_currentness(loaded)?;
    Ok(loaded.registry.edges.iter().any(|edge| {
        report
            .effective_edges
            .iter()
            .find(|record| record.edge_id == edge.edge_id)
            .is_some_and(|record| {
                record.positive_relation_available
                    && super::validate::edge_connects_subjects(edge, left, right)
            })
    }))
}

fn effective_edge_state(
    loaded: &LoadedCrossLanguageEdgeRegistryV1,
    edge: &CrossLanguageEdgeV1,
    anomalies: &mut Vec<CrossLanguageEdgeRegistryAnomalyV1>,
    guidance: &mut Vec<String>,
) -> (EdgeDispositionV1, Option<String>, bool) {
    if edge.disposition != EdgeDispositionV1::PositiveRegistered {
        return (edge.disposition, edge.first_unresolved_leg.clone(), false);
    }

    let Some(producer) = loaded.producer_registration(&edge.producer_id) else {
        push_anomaly(
            anomalies,
            CrossLanguageEdgeRegistryAnomalyKindV1::MissingProducerRegistration,
            &edge.edge_id,
            None,
            "positive edge references a missing producer registration",
        );
        guidance.push(format!(
            "register producer `{}` or downgrade edge `{}` from positive_registered",
            edge.producer_id, edge.edge_id
        ));
        return (
            EdgeDispositionV1::UnresolvedLeg,
            Some("missing_producer_registration".into()),
            false,
        );
    };

    if edge.producer_basis == EdgeProducerBasisV1::StaleProducerFact
        || edge.currentness == EdgeCurrentnessV1::Stale
    {
        guidance.push(format!(
            "refresh producer `{}` facts before trusting edge `{}`",
            edge.producer_id, edge.edge_id
        ));
        return (EdgeDispositionV1::Stale, None, false);
    }

    if !producer.supported_families.contains(&edge.boundary_family) {
        push_anomaly(
            anomalies,
            CrossLanguageEdgeRegistryAnomalyKindV1::DanglingEdge,
            &edge.edge_id,
            None,
            "registered producer does not admit the edge boundary family",
        );
        return (
            EdgeDispositionV1::UnresolvedLeg,
            Some("unsupported_producer_family".into()),
            false,
        );
    }

    if edge.first_unresolved_leg.is_some() {
        return (
            EdgeDispositionV1::UnresolvedLeg,
            edge.first_unresolved_leg.clone(),
            false,
        );
    }

    (EdgeDispositionV1::PositiveRegistered, None, true)
}

fn detect_duplicate_and_conflicting_edges(
    semantic_index: &BTreeMap<String, Vec<&CrossLanguageEdgeV1>>,
    anomalies: &mut Vec<CrossLanguageEdgeRegistryAnomalyV1>,
) {
    for edges in semantic_index.values() {
        if edges.len() < 2 {
            continue;
        }
        for window in edges.windows(2) {
            let left = window[0];
            let right = window[1];
            if left.edge_id == right.edge_id {
                continue;
            }
            let left_digest = cross_language_edge_content_digest(left).ok();
            let right_digest = cross_language_edge_content_digest(right).ok();
            let kind = if left_digest.is_some() && left_digest == right_digest {
                CrossLanguageEdgeRegistryAnomalyKindV1::DuplicateSemanticEdge
            } else {
                CrossLanguageEdgeRegistryAnomalyKindV1::ConflictingEdge
            };
            push_anomaly(
                anomalies,
                kind,
                &left.edge_id,
                Some(right.edge_id.clone()),
                "duplicate semantic relationship slot",
            );
        }
    }
}

fn detect_reversed_positive_edges(
    edges: &[CrossLanguageEdgeV1],
    anomalies: &mut Vec<CrossLanguageEdgeRegistryAnomalyV1>,
) {
    for left in edges {
        if left.disposition != EdgeDispositionV1::PositiveRegistered {
            continue;
        }
        for right in edges {
            if left.edge_id >= right.edge_id
                || right.disposition != EdgeDispositionV1::PositiveRegistered
                || left.boundary_family != right.boundary_family
            {
                continue;
            }
            if subjects_swap(left, right) {
                push_anomaly(
                    anomalies,
                    CrossLanguageEdgeRegistryAnomalyKindV1::ReversedEdge,
                    &left.edge_id,
                    Some(right.edge_id.clone()),
                    "bidirectional positive claims on the same endpoint pair",
                );
            }
        }
    }
}

fn detect_over_broad_edges(
    edges: &[CrossLanguageEdgeV1],
    anomalies: &mut Vec<CrossLanguageEdgeRegistryAnomalyV1>,
) {
    for edge in edges {
        if edge.disposition != EdgeDispositionV1::PositiveRegistered {
            continue;
        }
        if edge.direction == CrossLanguageEdgeDirectionV1::Bidirectional
            && edge.propagation_ceiling >= EdgeStrengthV1::PropagationLimited
        {
            push_anomaly(
                anomalies,
                CrossLanguageEdgeRegistryAnomalyKindV1::OverBroadEdge,
                &edge.edge_id,
                None,
                "bidirectional edge claims in-process propagation ceiling",
            );
        }
    }
}

fn detect_artifact_cross_bind(
    artifact_index: &BTreeMap<
        String,
        Vec<(&CrossLanguageEdgeV1, &CrossLanguageGeneratedArtifactV1)>,
    >,
    anomalies: &mut Vec<CrossLanguageEdgeRegistryAnomalyV1>,
) {
    for bindings in artifact_index.values() {
        for left_index in 0..bindings.len() {
            let (left_edge, left_artifact) = bindings[left_index];
            for &(right_edge, right_artifact) in bindings.iter().skip(left_index + 1) {
                if left_edge.edge_id == right_edge.edge_id {
                    continue;
                }
                if left_artifact.generator_id != right_artifact.generator_id
                    || left_artifact.input_digest != right_artifact.input_digest
                    || left_artifact.output_digest != right_artifact.output_digest
                {
                    continue;
                }
                if project_scope_key(&left_edge.source.project)
                    == project_scope_key(&right_edge.source.project)
                    && project_scope_key(&left_edge.target.project)
                        == project_scope_key(&right_edge.target.project)
                {
                    continue;
                }
                push_anomaly(
                    anomalies,
                    CrossLanguageEdgeRegistryAnomalyKindV1::ArtifactCrossBind,
                    &left_edge.edge_id,
                    Some(right_edge.edge_id.clone()),
                    "generated artifact identity cannot cross-bind across project scopes",
                );
            }
        }
    }
}

fn project_scope_key(project: &CrossLanguageProjectIdentityV1) -> String {
    project.portable_key()
}

fn subjects_swap(left: &CrossLanguageEdgeV1, right: &CrossLanguageEdgeV1) -> bool {
    super::validate::edge_connects_subjects(left, &right.target, &right.source)
}

fn push_anomaly(
    anomalies: &mut Vec<CrossLanguageEdgeRegistryAnomalyV1>,
    kind: CrossLanguageEdgeRegistryAnomalyKindV1,
    edge_id: &str,
    related_edge_id: Option<String>,
    message: impl Into<String>,
) {
    anomalies.push(CrossLanguageEdgeRegistryAnomalyV1 {
        kind,
        edge_id: edge_id.into(),
        related_edge_id,
        message: message.into(),
    });
}
