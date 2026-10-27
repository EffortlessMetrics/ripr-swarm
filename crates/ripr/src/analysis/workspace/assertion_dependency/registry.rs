//! Resolved registry-edge receipts for audited external semantics.

use super::{SOURCE, digest, metadata_probe, parse, read_optional, snapshot};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const JSON_CHECKSUM: &str = "c841b55ecdae098c80dcae9cf767f6f8a0c2cdb3416bbef72181df4d0fe73f14";

/// Audited serde_json 1.0.151 registry expansion under ordinary Cargo/cache
/// integrity assumptions. The resolved dependency edge, not its spelling in a
/// lockfile, binds each importing file's package to this implementation.
#[derive(Clone, Debug)]
pub(crate) struct LiteralJsonIdentity {
    metadata: serde_json::Value,
    package_id: String,
    manifest_digests: BTreeMap<PathBuf, String>,
    pub(crate) validation_digest: String,
}

impl LiteralJsonIdentity {
    pub(crate) fn admits_file(&self, root: &Path, file: &Path) -> bool {
        let file = root.join(file);
        let Some(owner_manifest) = file
            .parent()
            .and_then(|parent| {
                parent
                    .ancestors()
                    .take_while(|dir| dir.starts_with(root))
                    .map(|dir| dir.join("Cargo.toml"))
                    .find(|path| path.is_file())
            })
            .and_then(|path| path.canonicalize().ok())
        else {
            return false;
        };
        self.manifest_digests
            .get(&owner_manifest)
            .is_some_and(|expected| {
                read_optional(&owner_manifest)
                    .flatten()
                    .is_some_and(|bytes| digest(&bytes) == *expected)
            })
            && self.admits_manifest(&owner_manifest)
    }

    fn admits_manifest(&self, manifest: &Path) -> bool {
        let Some(packages) = self
            .metadata
            .get("packages")
            .and_then(serde_json::Value::as_array)
        else {
            return false;
        };
        let owners: Vec<_> = packages
            .iter()
            .filter(|p| {
                p.get("manifest_path")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|p| Path::new(p).canonicalize().ok())
                    .as_deref()
                    == Some(manifest)
            })
            .collect();
        let [owner] = owners.as_slice() else {
            return false;
        };
        let Some(deps) = owner
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
        else {
            return false;
        };
        let deps: Vec<_> = deps
            .iter()
            .filter(|d| d.get("name").and_then(serde_json::Value::as_str) == Some("serde_json"))
            .collect();
        // Normal and dev declarations may share one canonical package edge.
        // Every declaration and every edge-kind must agree; build, renamed,
        // conditional, optional and duplicate contexts remain unsupported.
        if deps.is_empty()
            || deps.len() > 2
            || deps.iter().any(|dep| {
                dep.get("source").and_then(serde_json::Value::as_str) != Some(SOURCE)
                    || dep.get("optional").and_then(serde_json::Value::as_bool) != Some(false)
                    || !dep.get("rename").is_some_and(serde_json::Value::is_null)
                    || !dep.get("target").is_some_and(serde_json::Value::is_null)
                    || !dep
                        .get("kind")
                        .is_some_and(|k| k.is_null() || k.as_str() == Some("dev"))
            })
        {
            return false;
        }
        let contexts: std::collections::BTreeSet<_> = deps
            .iter()
            .map(|dep| dep.get("kind").and_then(serde_json::Value::as_str))
            .collect();
        if contexts.len() != deps.len() {
            return false;
        }
        let Some(nodes) = self
            .metadata
            .pointer("/resolve/nodes")
            .and_then(serde_json::Value::as_array)
        else {
            return false;
        };
        let Some(owner_id) = owner.get("id").and_then(serde_json::Value::as_str) else {
            return false;
        };
        let Some(node) = nodes
            .iter()
            .find(|node| node.get("id").and_then(serde_json::Value::as_str) == Some(owner_id))
        else {
            return false;
        };
        let Some(edges) = node.get("deps").and_then(serde_json::Value::as_array) else {
            return false;
        };
        let edges: Vec<_> = edges
            .iter()
            .filter(|d| d.get("name").and_then(serde_json::Value::as_str) == Some("serde_json"))
            .collect();
        let [edge] = edges.as_slice() else {
            return false;
        };
        edge.get("pkg").and_then(serde_json::Value::as_str) == Some(self.package_id.as_str())
            && edge
                .get("dep_kinds")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|kinds| {
                    kinds.len() == contexts.len()
                        && kinds.iter().all(|kind| {
                            kind.get("kind")
                                .is_some_and(|k| k.is_null() || k.as_str() == Some("dev"))
                                && kind.get("target").is_some_and(serde_json::Value::is_null)
                        })
                        && kinds
                            .iter()
                            .map(|kind| kind.get("kind").and_then(serde_json::Value::as_str))
                            .collect::<std::collections::BTreeSet<_>>()
                            == contexts
                })
    }
}

/// Initial scope: the analysis root declares serde_json itself or through its
/// workspace dependency table. Other layouts remain unestablished.
pub(crate) fn eligible_json_root(root: &Path) -> bool {
    read_optional(&root.join("Cargo.toml"))
        .flatten()
        .and_then(|b| parse(&b))
        .is_some_and(|manifest| {
            ["dependencies", "dev-dependencies", "workspace"]
                .iter()
                .any(|key| {
                    let table = if *key == "workspace" {
                        manifest.get(key).and_then(|w| w.get("dependencies"))
                    } else {
                        manifest.get(key)
                    };
                    table.and_then(|t| t.get("serde_json")).is_some()
                })
        })
}

pub(super) fn json_manifest_digests(
    metadata: &serde_json::Value,
    root: &Path,
) -> Option<BTreeMap<PathBuf, String>> {
    let mut digests = BTreeMap::new();
    for package in metadata.get("packages")?.as_array()? {
        let path = PathBuf::from(package.get("manifest_path")?.as_str()?)
            .canonicalize()
            .ok()?;
        if path.starts_with(root) {
            let bytes = read_optional(&path)??;
            digests.insert(path, digest(&bytes));
        }
    }
    Some(digests)
}

pub(crate) fn verified_json_identity(root: &Path) -> Option<LiteralJsonIdentity> {
    if !eligible_json_root(root) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let before = snapshot(&root)?;
    let bytes = metadata_probe(&root)?;
    let metadata: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let workspace = Path::new(metadata.get("workspace_root")?.as_str()?)
        .canonicalize()
        .ok()?;
    if !root.starts_with(&workspace) {
        return None;
    }
    let lock = parse(&read_optional(&workspace.join("Cargo.lock"))??)?;
    let packages = lock.get("package")?.as_array()?;
    let found: Vec<_> = packages
        .iter()
        .filter(|p| p.get("name").and_then(toml::Value::as_str) == Some("serde_json"))
        .collect();
    let [package] = found.as_slice() else {
        return None;
    };
    if package.get("version")?.as_str()? != "1.0.151"
        || package.get("source")?.as_str()? != SOURCE
        || package.get("checksum")?.as_str()? != JSON_CHECKSUM
    {
        return None;
    }
    let packages = metadata.get("packages")?.as_array()?;
    let found: Vec<_> = packages
        .iter()
        .filter(|p| p.get("name").and_then(serde_json::Value::as_str) == Some("serde_json"))
        .collect();
    let [package] = found.as_slice() else {
        return None;
    };
    if package.get("version")?.as_str()? != "1.0.151" || package.get("source")?.as_str()? != SOURCE
    {
        return None;
    }
    let package_id = package.get("id")?.as_str()?.to_string();
    let manifest_digests = json_manifest_digests(&metadata, &root)?;
    // Discovery metadata supplies the member path set. Bracket a confirming
    // probe with those manifests: old metadata must not acquire a receipt for
    // member bytes first observed after Cargo resolved a different dependency.
    let confirmed_bytes = metadata_probe(&root)?;
    if bytes != confirmed_bytes || manifest_digests != json_manifest_digests(&metadata, &root)? {
        return None;
    }
    let after = snapshot(&root)?;
    if before != after {
        return None;
    }
    Some(LiteralJsonIdentity {
        metadata,
        package_id,
        manifest_digests: manifest_digests.clone(),
        validation_digest: digest(
            &serde_json::to_vec(&(after, manifest_digests, digest(&bytes), JSON_CHECKSUM)).ok()?,
        ),
    })
}
