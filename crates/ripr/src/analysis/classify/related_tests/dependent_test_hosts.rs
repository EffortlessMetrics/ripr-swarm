use super::{DependencyEdgeContext, has_callable_forward_dependency, nearest_manifest_identity};
use crate::analysis::workspace::PathDependencyGraphStatus;
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

impl DependencyEdgeContext<'_> {
    /// Attach this inspection hint only after all stronger limits have settled.
    pub(in crate::analysis) fn disclose_dependent_test_host(
        &self,
        finding: &mut Finding,
        owner_file: &Path,
    ) {
        if finding.class == ExposureClass::NoStaticPath
            && finding.static_limit_kind.is_none()
            && finding.related_tests.is_empty()
            && let Some(host) = self.dependent_test_host(owner_file)
        {
            finding.evidence.push(format!(
                "The directly dependent package at `{host}` hosts indexed tests. Whether those tests reach this owner is unresolved."
            ));
        }
    }

    /// One directly dependent manifest with an indexed test. The graph and
    /// the test inventory establish a place to inspect, never owner reach.
    fn dependent_test_host(&self, owner_file: &Path) -> Option<&str> {
        if self.adjacency.status() != PathDependencyGraphStatus::Complete {
            return None;
        }
        let owner = nearest_manifest_identity(self.manifest_dir_prefixes, owner_file)?;
        let dependents = self.adjacency.reverse_neighbors(&owner)?;
        let hosts = self.test_host_manifests.get_or_init(|| {
            self.index
                .tests()
                .iter()
                .filter_map(|test| {
                    nearest_manifest_identity(self.manifest_dir_prefixes, &test.file)
                })
                .collect()
        });
        dependents.iter().find_map(|manifest| {
            (manifest != &owner
                && hosts.contains(manifest)
                && has_callable_forward_dependency(self.adjacency, manifest, &owner))
            .then_some(manifest.as_str())
        })
    }
}
