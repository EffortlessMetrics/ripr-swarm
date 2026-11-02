//! Project resource dependency edges into verification-plan obligations (#5171).
#![expect(
    dead_code,
    reason = "policy:5171-vp3r-slice-a: plan projection awaits ChangeVerificationPlanV1 consumer (#5163)"
)]

use crate::domain::{
    ResourceAccessKindV1, ResourceCompletenessClassV1, ResourceDependencyEdgeV1, ResourceKindV1,
    ResourcePresenceV1, SymbolId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceValidationObligationV1 {
    pub(crate) resource_path: PathBuf,
    pub(crate) consumer_id: SymbolId,
    pub(crate) change_kind: ResourceChangeKindV1,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ResourceChangeKindV1 {
    Modified,
    Deleted,
    AddedRead,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourcePlanWideningV1 {
    pub(crate) reason: String,
    pub(crate) package_identities: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResourcePlanProjectionV1 {
    pub(crate) validation_obligations: Vec<ResourceValidationObligationV1>,
    pub(crate) impacted_consumers: BTreeSet<SymbolId>,
    pub(crate) widened_packages: Vec<ResourcePlanWideningV1>,
    pub(crate) limitations: Vec<String>,
}

/// Map indexed resource edges plus a normalized resource change set into plan
/// obligations. Does not execute tests or choose Cargo scope.
pub(crate) fn project_resource_plan(
    edges: &[ResourceDependencyEdgeV1],
    changed_resources: &BTreeSet<PathBuf>,
    deleted_resources: &BTreeSet<PathBuf>,
    newly_read_resources: &BTreeSet<PathBuf>,
    package_for_file: impl Fn(&Path) -> Option<String>,
) -> ResourcePlanProjectionV1 {
    let mut projection = ResourcePlanProjectionV1::default();
    if changed_resources.is_empty()
        && deleted_resources.is_empty()
        && newly_read_resources.is_empty()
    {
        return projection;
    }

    let mut exact_by_resource: BTreeMap<PathBuf, Vec<&ResourceDependencyEdgeV1>> = BTreeMap::new();
    for edge in edges {
        if edge.completeness_class == ResourceCompletenessClassV1::Exact
            && edge.access_kind == ResourceAccessKindV1::ExactLiteralRepositoryPath
        {
            exact_by_resource
                .entry(edge.resource_path.clone())
                .or_default()
                .push(edge);
        }
    }

    for path in changed_resources {
        if let Some(consumers) = exact_by_resource.get(path) {
            for edge in consumers {
                projection
                    .validation_obligations
                    .push(ResourceValidationObligationV1 {
                        resource_path: path.clone(),
                        consumer_id: edge.consumer_id.clone(),
                        change_kind: ResourceChangeKindV1::Modified,
                    });
                projection
                    .impacted_consumers
                    .insert(edge.consumer_id.clone());
            }
        }
    }

    for path in deleted_resources {
        for edge in edges.iter().filter(|edge| edge.resource_path == *path) {
            if edge.base_presence == ResourcePresenceV1::Present {
                projection
                    .validation_obligations
                    .push(ResourceValidationObligationV1 {
                        resource_path: path.clone(),
                        consumer_id: edge.consumer_id.clone(),
                        change_kind: ResourceChangeKindV1::Deleted,
                    });
                projection
                    .impacted_consumers
                    .insert(edge.consumer_id.clone());
            }
        }
    }

    for path in newly_read_resources {
        if let Some(consumers) = exact_by_resource.get(path) {
            for edge in consumers {
                projection
                    .validation_obligations
                    .push(ResourceValidationObligationV1 {
                        resource_path: path.clone(),
                        consumer_id: edge.consumer_id.clone(),
                        change_kind: ResourceChangeKindV1::AddedRead,
                    });
                projection
                    .impacted_consumers
                    .insert(edge.consumer_id.clone());
            }
        }
    }

    let mut opaque_packages = BTreeSet::new();
    for edge in edges {
        if edge.resource_kind != ResourceKindV1::OpaqueOrDynamicResource {
            continue;
        }
        let touches_change = changed_resources.contains(&edge.resource_path)
            || deleted_resources.contains(&edge.resource_path)
            || newly_read_resources.contains(&edge.resource_path);
        if !touches_change {
            continue;
        }
        if let Some(package) = package_for_file(&edge.consumer_file) {
            opaque_packages.insert(package);
        }
        projection.limitations.push(format!(
            "opaque_or_dynamic_resource:{}:{}",
            edge.resource_path.to_string_lossy().replace('\\', "/"),
            edge.consumer_id.0
        ));
    }

    if projection.impacted_consumers.is_empty()
        && (!changed_resources.is_empty() || !deleted_resources.is_empty())
        && edges.iter().any(|edge| {
            edge.completeness_class == ResourceCompletenessClassV1::Opaque
                || edge.access_kind == ResourceAccessKindV1::DynamicOrUnresolved
        })
    {
        projection
            .limitations
            .push("resource_plan_empty_with_opaque_boundary: widen_fallback_required".to_string());
    }

    if !opaque_packages.is_empty() {
        projection.widened_packages.push(ResourcePlanWideningV1 {
            reason: "opaque_or_dynamic_resource".to_string(),
            package_identities: opaque_packages,
        });
    }

    projection.validation_obligations.sort_by(|left, right| {
        left.resource_path
            .cmp(&right.resource_path)
            .then(left.consumer_id.0.cmp(&right.consumer_id.0))
            .then(left.change_kind.cmp(&right.change_kind))
    });
    projection
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ResourceAccessKindV1, ResourceCompletenessClassV1, ResourceDependencyEdgeV1,
        ResourceKindV1, ResourcePresenceV1,
    };

    fn exact_edge(resource: &str, consumer: &str, file: &str) -> ResourceDependencyEdgeV1 {
        let resource_path = PathBuf::from(resource);
        let consumer_id = SymbolId(format!("{file}::{consumer}"));
        let digest = ResourceDependencyEdgeV1::mint_semantic_digest(
            ResourceKindV1::CompileTimeEmbedded,
            &resource_path,
            &consumer_id,
            ResourceAccessKindV1::ExactLiteralRepositoryPath,
            ResourceCompletenessClassV1::Exact,
        );
        ResourceDependencyEdgeV1 {
            edge_id: ResourceDependencyEdgeV1::mint_edge_id(&digest),
            semantic_digest: digest,
            resource_kind: ResourceKindV1::CompileTimeEmbedded,
            resource_path,
            content_identity: "content:unknown".to_string(),
            consumer_id,
            consumer_file: PathBuf::from(file),
            consumer_line: 1,
            access_kind: ResourceAccessKindV1::ExactLiteralRepositoryPath,
            resolution_basis: "include_str_literal".to_string(),
            base_presence: ResourcePresenceV1::Present,
            head_presence: ResourcePresenceV1::Present,
            completeness_class: ResourceCompletenessClassV1::Exact,
            first_unresolved_edge: None,
            limitations: Vec::new(),
            non_claims: vec!["no_test_adequacy".to_string()],
        }
    }

    #[test]
    fn changed_embedded_resource_selects_exact_consumer() {
        let edges = vec![exact_edge(
            "fixtures/data.json",
            "reads_fixture",
            "tests/it.rs",
        )];
        let mut changed = BTreeSet::new();
        changed.insert(PathBuf::from("fixtures/data.json"));
        let projection =
            project_resource_plan(&edges, &changed, &BTreeSet::new(), &BTreeSet::new(), |_| {
                Some("pkg-a".to_string())
            });
        assert_eq!(projection.impacted_consumers.len(), 1);
        assert_eq!(projection.validation_obligations.len(), 1);
        assert_eq!(
            projection.validation_obligations[0].change_kind,
            ResourceChangeKindV1::Modified
        );
    }

    #[test]
    fn deleted_resource_retains_base_consumer_obligation() {
        let mut edge = exact_edge("fixtures/old.json", "reads_old", "tests/it.rs");
        edge.head_presence = ResourcePresenceV1::Absent;
        let edges = vec![edge];
        let mut deleted = BTreeSet::new();
        deleted.insert(PathBuf::from("fixtures/old.json"));
        let projection =
            project_resource_plan(&edges, &BTreeSet::new(), &deleted, &BTreeSet::new(), |_| {
                Some("pkg-a".to_string())
            });
        assert_eq!(projection.validation_obligations.len(), 1);
        assert_eq!(
            projection.validation_obligations[0].change_kind,
            ResourceChangeKindV1::Deleted
        );
    }

    #[test]
    fn empty_exact_consumers_with_opaque_boundary_records_fallback_limitation() {
        let mut edge = exact_edge("generated/out.txt", "dynamic", "src/lib.rs");
        edge.resource_kind = ResourceKindV1::OpaqueOrDynamicResource;
        edge.access_kind = ResourceAccessKindV1::DynamicOrUnresolved;
        edge.completeness_class = ResourceCompletenessClassV1::Opaque;
        let edges = vec![edge];
        let mut changed = BTreeSet::new();
        changed.insert(PathBuf::from("generated/out.txt"));
        let projection =
            project_resource_plan(&edges, &changed, &BTreeSet::new(), &BTreeSet::new(), |_| {
                Some("pkg-a".to_string())
            });
        assert!(projection.impacted_consumers.is_empty());
        assert!(
            projection
                .limitations
                .iter()
                .any(|limit| limit.contains("resource_plan_empty_with_opaque_boundary")),
            "{:?}",
            projection.limitations
        );
        assert_eq!(projection.widened_packages.len(), 1);
    }
}
