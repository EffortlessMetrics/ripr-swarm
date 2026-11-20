use super::{DependencyEdgeContext, has_callable_forward_dependency, nearest_manifest_identity};
use crate::analysis::workspace::PathDependencyGraphStatus;
use std::path::Path;

impl DependencyEdgeContext<'_> {
    /// One directly dependent manifest with an indexed test. The graph and
    /// the test inventory establish a place to inspect, never owner reach.
    pub(in crate::analysis) fn dependent_test_host(&self, owner_file: &Path) -> Option<&str> {
        if self.adjacency.status() != PathDependencyGraphStatus::Complete {
            return None;
        }
        // Probe locations can be rooted while the manifest inventory and
        // index paths are workspace-relative.
        let owner_file = if owner_file.is_absolute() {
            owner_file.strip_prefix(self.root).ok()?
        } else {
            owner_file.strip_prefix(self.root).unwrap_or(owner_file)
        };
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
