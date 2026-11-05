//! Run-scoped Cargo target admission for ordinary library test observers.

use super::{BTreeMap, OwnerPinSyntax, Path, PathBuf, RustIndex, SyncMemo};
use crate::analysis::workspace::ManifestInventory;

pub(super) type TargetInventories =
    SyncMemo<BTreeMap<(PathBuf, String, String), ManifestInventory>>;

impl OwnerPinSyntax {
    pub(in crate::analysis) fn admits_ordinary_library_test_target(
        &self,
        index: &RustIndex,
        file: &Path,
    ) -> bool {
        let Some(workspace) = index.workspace_authority.as_ref() else {
            return false;
        };
        let Some(source) = workspace.files.get(file) else {
            return false;
        };
        // A cached target decision cannot outlive the indexed source and its
        // owning manifest. Rebuilt indexes get a distinct workspace identity.
        if !workspace.validates_target_digest(file, file, &source.source_digest) {
            return false;
        }
        // A virtual workspace's default-member selector is not necessarily
        // the source file's nearest owning manifest. Bind its current bytes
        // too, using the shared bounded reader and source digest authority.
        let Ok(manifest) =
            crate::bounded_input::read_metadata_toml(workspace.root.join("Cargo.toml"))
        else {
            return false;
        };
        let root_manifest = crate::analysis::facts::source_digest(manifest.as_bytes());
        self.ordinary_library_targets
            .borrow_mut()
            .entry((
                workspace.root.clone(),
                workspace.workspace_identity.clone(),
                root_manifest,
            ))
            .or_default()
            .ordinary_library_test_target(&workspace.root, file)
    }
}
