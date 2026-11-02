//! Resolved registry-edge receipts for audited external semantics.

use super::{
    MetadataProbeFailure, SOURCE, digest, metadata_probe_result, parse, read_optional, snapshot,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const JSON_CHECKSUM: &str = "c841b55ecdae098c80dcae9cf767f6f8a0c2cdb3416bbef72181df4d0fe73f14";

/// An audited registry implementation under ordinary Cargo/cache integrity
/// assumptions. The resolved dependency edge, not its spelling in a lockfile,
/// binds each importing file's package to this implementation.
#[derive(Clone, Debug)]
pub(crate) struct RegistryIdentity {
    crate_name: &'static str,
    input_snapshot: BTreeMap<String, String>,
    metadata: serde_json::Value,
    package_id: String,
    manifest_digests: BTreeMap<PathBuf, String>,
    proptest_target_configs: BTreeMap<PathBuf, String>,
    pub(crate) validation_digest: String,
}

pub(crate) type LiteralJsonIdentity = RegistryIdentity;

const PROPTEST_CHECKSUM: &str = "4b45fcc2344c680f5025fe57779faef368840d0bd1f42f216291f0dc4ace4744";

pub(crate) fn eligible_proptest_root(root: &Path) -> bool {
    if !eligible_root(root, "proptest") {
        return false;
    }
    // Cheap refusal keeps unaudited/default-newer proptest users off the
    // full metadata path. A lockfile only selects a candidate for proof.
    let Some(packages) = locked_packages(root, "proptest") else {
        return false;
    };
    let [package] = packages.as_slice() else {
        return false;
    };
    package.get("version").and_then(toml::Value::as_str) == Some("1.11.0")
        && package.get("source").and_then(toml::Value::as_str) == Some(SOURCE)
        && package.get("checksum").and_then(toml::Value::as_str) == Some(PROPTEST_CHECKSUM)
}

/// The `[[package]]` entries named `name` in the nearest `Cargo.lock` at or
/// above `root`. `None` is no readable lockfile; an empty list is a lockfile
/// that does not lock the crate. This is the one lockfile reader of the audited
/// registry profiles.
fn locked_packages(root: &Path, name: &str) -> Option<Vec<toml::Value>> {
    let lock = root
        .ancestors()
        .map(|directory| directory.join("Cargo.lock"))
        .find(|path| path.is_file())
        .and_then(|path| read_optional(&path).flatten())
        .and_then(|bytes| parse(&bytes))?;
    let packages = lock.get("package")?.as_array()?;
    Some(
        packages
            .iter()
            .filter(|package| package.get("name").and_then(toml::Value::as_str) == Some(name))
            .cloned()
            .collect(),
    )
}

/// RIPR-SPEC-0369: whether the lockfile locks `name` exactly once, from the
/// crates.io registry, at one of the `audited` versions. An unlocked, repeated,
/// path, git, other-registry or unaudited-version package is not audited.
pub(crate) fn locked_at_audited_version(root: &Path, name: &str, audited: &[&str]) -> bool {
    let Some(packages) = locked_packages(root, name) else {
        return false;
    };
    let [package] = packages.as_slice() else {
        return false;
    };
    package.get("source").and_then(toml::Value::as_str) == Some(SOURCE)
        && package
            .get("version")
            .and_then(toml::Value::as_str)
            .is_some_and(|version| audited.contains(&version))
}

/// The function macro, Just strategy and assertion failure path audited in
/// proptest 1.11.0. Cargo/cache integrity has the same instrument assumptions
/// as the other registry receipts; a matching spelling is never authority.
pub(crate) fn verified_proptest_identity(root: &Path) -> Option<RegistryIdentity> {
    verified_proptest_identity_result(root).ok().flatten()
}

/// `Ok(None)` is a refused admission premise, not a failed metadata command.
/// `Err` retains only the operational failure observed during this verification.
/// No error grants provider, target, execution or assertion authority.
pub(crate) fn verified_proptest_identity_result(
    root: &Path,
) -> Result<Option<RegistryIdentity>, MetadataProbeFailure> {
    if !eligible_proptest_root(root) || !proptest_configuration_allowed(root) {
        return Ok(None);
    }
    let Some(mut identity) =
        verified_registry_identity_result(root, "proptest", "1.11.0", PROPTEST_CHECKSUM)?
    else {
        return Ok(None);
    };
    Ok((|| {
        let configurations = proptest_target_configurations(root, &identity.metadata)?;
        if !proptest_configuration_allowed(root)
            || snapshot(root).as_ref() != Some(&identity.input_snapshot)
            || proptest_target_configurations(root, &identity.metadata).as_ref()
                != Some(&configurations)
        {
            return None;
        }
        // Preserve the existing target/configuration receipt exactly.
        identity.validation_digest =
            digest(&serde_json::to_vec(&(&identity.validation_digest, &configurations)).ok()?);
        identity.proptest_target_configs = configurations;
        Some(identity)
    })())
}

fn proptest_target_configurations(
    root: &Path,
    metadata: &serde_json::Value,
) -> Option<BTreeMap<PathBuf, String>> {
    let root = root.canonicalize().ok()?;
    let mut configurations = BTreeMap::new();
    for package in metadata.get("packages")?.as_array()? {
        let manifest = Path::new(package.get("manifest_path")?.as_str()?)
            .canonicalize()
            .ok()?;
        if !manifest.starts_with(&root) {
            continue;
        }
        for target in package.get("targets")?.as_array()? {
            if !target
                .get("kind")?
                .as_array()?
                .iter()
                .any(|kind| matches!(kind.as_str(), Some("lib" | "test")))
            {
                continue;
            }
            let source = Path::new(target.get("src_path")?.as_str()?)
                .canonicalize()
                .ok()?;
            let directory = source.parent()?;
            if !directory.starts_with(&root) || !proptest_configuration_allowed(directory) {
                return None;
            }
            configurations.insert(
                directory.to_path_buf(),
                super::config_snapshot_identity(directory)?,
            );
        }
    }
    Some(configurations)
}

/// Function-form proptest! contextualizes even an explicit Config, so an
/// environment override can make a default property execute zero cases.
/// The initial profile accepts no PROPTEST_* override, including Cargo [env].
/// Absence is checked before/after resolution and again at file admission.
fn proptest_configuration_allowed(root: &Path) -> bool {
    let is_property = |name: &std::ffi::OsStr| {
        let bytes = if cfg!(windows) {
            name.as_encoded_bytes().to_ascii_uppercase()
        } else {
            name.as_encoded_bytes().to_vec()
        };
        bytes.starts_with(b"PROPTEST_")
    };
    if std::env::vars_os().any(|(name, _)| is_property(&name)) {
        return false;
    }
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(home) = home::env::cargo_home_with_cwd_env(&home::env::OS_ENV, &root) else {
        return false;
    };
    let home = if home.is_absolute() {
        home
    } else {
        root.join(home)
    };
    let mut configs = vec![home.join("config"), home.join("config.toml")];
    for directory in root.ancestors() {
        configs.extend([
            directory.join(".cargo/config"),
            directory.join(".cargo/config.toml"),
        ]);
    }
    configs.iter().all(|path| match read_optional(path) {
        Some(None) => true,
        Some(Some(bytes)) => parse(&bytes).is_some_and(|value| {
            value.get("env").is_none_or(|env| {
                env.as_table().is_some_and(|table| {
                    table
                        .keys()
                        .all(|name| !is_property(std::ffi::OsStr::new(name)))
                })
            })
        }),
        None => false,
    })
}

impl RegistryIdentity {
    /// The empty-String profile depends on the consumer's standard crate
    /// names as well as the provider identity: provider macro expansions use
    /// `extern crate alloc` and `::core`. Reuse the resolved consumer metadata;
    /// a canonical proptest edge cannot authorize a substituted standard root.
    pub(crate) fn proptest_standard_crates_unshadowed(&self, root: &Path, file: &Path) -> bool {
        let Ok(file) = root.join(file).canonicalize() else {
            return false;
        };
        let Some(packages) = self
            .metadata
            .get("packages")
            .and_then(serde_json::Value::as_array)
        else {
            return false;
        };
        let owners = packages
            .iter()
            .filter(|package| {
                package
                    .get("targets")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|targets| {
                        targets.iter().any(|target| {
                            target
                                .get("src_path")
                                .and_then(serde_json::Value::as_str)
                                .and_then(|path| Path::new(path).canonicalize().ok())
                                .as_ref()
                                == Some(&file)
                        })
                    })
            })
            .collect::<Vec<_>>();
        let [owner] = owners.as_slice() else {
            return false;
        };
        let ordinary_name = |name: &serde_json::Value| {
            name.as_str().is_some_and(|name| {
                !matches!(name.replace('-', "_").as_str(), "std" | "core" | "alloc")
            })
        };
        // Declarations catch renamed, optional and inactive dependencies;
        // resolved edge names additionally catch a dependency's [lib] name.
        if !owner
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|dependencies| {
                dependencies.iter().all(|dependency| {
                    dependency
                        .get("rename")
                        .filter(|rename| !rename.is_null())
                        .or_else(|| dependency.get("name"))
                        .is_some_and(ordinary_name)
                })
            })
            || !owner
                .get("targets")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|targets| {
                    // An integration test also receives its own package's library as
                    // an extern crate, even though it is absent from resolve.deps.
                    targets
                        .iter()
                        .all(|target| target.get("name").is_some_and(ordinary_name))
                })
        {
            return false;
        }
        let Some(owner_id) = owner.get("id").and_then(serde_json::Value::as_str) else {
            return false;
        };
        let Some(nodes) = self
            .metadata
            .pointer("/resolve/nodes")
            .and_then(serde_json::Value::as_array)
        else {
            return false;
        };
        let nodes = nodes
            .iter()
            .filter(|node| node.get("id").and_then(serde_json::Value::as_str) == Some(owner_id))
            .collect::<Vec<_>>();
        let [node] = nodes.as_slice() else {
            return false;
        };
        node.get("deps")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|edges| {
                edges
                    .iter()
                    .all(|edge| edge.get("name").is_some_and(ordinary_name))
            })
    }

    /// This first property profile covers a real lib/test target root, not a
    /// file that merely looks like a test or a configured harness subject.
    pub(crate) fn admits_proptest_target(&self, root: &Path, file: &Path) -> bool {
        if self.crate_name != "proptest"
            || snapshot(root).as_ref() != Some(&self.input_snapshot)
            || !self.admits_file(root, file)
        {
            return false;
        }
        let default_features = [
            "default",
            "std",
            "fork",
            "timeout",
            "bit-set",
            "regex-syntax",
            "rusty-fork",
            "tempfile",
        ];
        if !self
            .metadata
            .pointer("/resolve/nodes")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|nodes| {
                nodes
                    .iter()
                    .find(|node| {
                        node.get("id").and_then(serde_json::Value::as_str)
                            == Some(self.package_id.as_str())
                    })
                    .and_then(|node| node.get("features"))
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|features| {
                        features
                            .iter()
                            .any(|feature| feature.as_str() == Some("std"))
                            && features.iter().all(|feature| {
                                feature
                                    .as_str()
                                    .is_some_and(|name| default_features.contains(&name))
                            })
                    })
            })
        {
            return false;
        }
        let Ok(file) = root.join(file).canonicalize() else {
            return false;
        };
        let Some(directory) = file.parent() else {
            return false;
        };
        let Some(expected_config) = self.proptest_target_configs.get(directory) else {
            return false;
        };
        if !proptest_configuration_allowed(directory)
            || super::config_snapshot_identity(directory).as_ref() != Some(expected_config)
        {
            return false;
        }
        let Some(packages) = self
            .metadata
            .get("packages")
            .and_then(serde_json::Value::as_array)
        else {
            return false;
        };
        packages.iter().any(|package| {
            let Some(manifest) = package
                .get("manifest_path")
                .and_then(serde_json::Value::as_str)
            else {
                return false;
            };
            let Ok(manifest_path) = Path::new(manifest).canonicalize() else {
                return false;
            };
            if !self.admits_manifest(&manifest_path)
                || package
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| name.replace('-', "_") == "proptest")
            {
                return false;
            }
            // Only the audited default feature declaration. A renamed,
            // optional or conditional dependency has already failed the
            // shared resolved-edge check.
            if !package
                .get("dependencies")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|dependencies| {
                    dependencies
                        .iter()
                        .filter(|dependency| {
                            dependency.get("name").and_then(serde_json::Value::as_str)
                                == Some("proptest")
                        })
                        .all(|dependency| {
                            dependency
                                .get("uses_default_features")
                                .and_then(serde_json::Value::as_bool)
                                == Some(true)
                                && dependency
                                    .get("features")
                                    .and_then(serde_json::Value::as_array)
                                    .is_some_and(Vec::is_empty)
                        })
                })
            {
                return false;
            }
            let Some(bytes) = read_optional(Path::new(manifest)).flatten() else {
                return false;
            };
            let Some(manifest_value) = parse(&bytes) else {
                return false;
            };
            // A custom libtest replacement or conditionally supplied target is
            // outside the profile even when metadata says test=true.
            let standard_target = |target: &toml::Value| {
                ["harness", "test"].iter().all(|key| {
                    target
                        .get(key)
                        .is_none_or(|value| value.as_bool() == Some(true))
                })
            };
            if manifest_value.get("target").is_some()
                || manifest_value
                    .get("lib")
                    .is_some_and(|target| !standard_target(target))
                || manifest_value.get("test").is_some_and(|tests| {
                    !tests
                        .as_array()
                        .is_some_and(|tests| tests.iter().all(standard_target))
                })
            {
                return false;
            }
            package
                .get("targets")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|targets| {
                    targets.iter().any(|target| {
                        target
                            .get("src_path")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|path| Path::new(path).canonicalize().ok())
                            .as_ref()
                            == Some(&file)
                            && target.get("test").and_then(serde_json::Value::as_bool) == Some(true)
                            && target.get("name").and_then(serde_json::Value::as_str)
                                != Some("proptest")
                            && target
                                .get("edition")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|edition| matches!(edition, "2018" | "2021" | "2024"))
                            && target
                                .get("kind")
                                .and_then(serde_json::Value::as_array)
                                .is_some_and(|kinds| {
                                    kinds.len() == 1
                                        && kinds.iter().all(|kind| {
                                            matches!(kind.as_str(), Some("lib" | "test"))
                                        })
                                })
                            && target.get("required-features").is_none_or(|features| {
                                features.as_array().is_some_and(Vec::is_empty)
                            })
                    })
                })
        })
    }

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
            .filter(|d| d.get("name").and_then(serde_json::Value::as_str) == Some(self.crate_name))
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
            .filter(|d| d.get("name").and_then(serde_json::Value::as_str) == Some(self.crate_name))
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
    eligible_root(root, "serde_json")
}

/// Pure manifest parse behind [`eligible_json_root`]; callers own the read.
pub(super) fn eligible_json_manifest_bytes(bytes: &[u8]) -> bool {
    eligible_manifest_bytes(bytes, "serde_json")
}

fn eligible_root(root: &Path, crate_name: &str) -> bool {
    read_optional(&root.join("Cargo.toml"))
        .flatten()
        .is_some_and(|b| eligible_manifest_bytes(&b, crate_name))
}

fn eligible_manifest_bytes(bytes: &[u8], crate_name: &str) -> bool {
    parse(bytes).is_some_and(|manifest| {
        ["dependencies", "dev-dependencies", "workspace"]
            .iter()
            .any(|key| {
                let table = if *key == "workspace" {
                    manifest.get(key).and_then(|w| w.get("dependencies"))
                } else {
                    manifest.get(key)
                };
                table.and_then(|t| t.get(crate_name)).is_some()
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
    verified_registry_identity(root, "serde_json", "1.0.151", JSON_CHECKSUM)
}

fn verified_registry_identity(
    root: &Path,
    crate_name: &'static str,
    version: &str,
    checksum: &str,
) -> Option<RegistryIdentity> {
    verified_registry_identity_result(root, crate_name, version, checksum)
        .ok()
        .flatten()
}

fn decode_registry_metadata(bytes: &[u8]) -> Result<serde_json::Value, MetadataProbeFailure> {
    serde_json::from_slice(bytes).map_err(|_error| MetadataProbeFailure::InvalidJson)
}

fn verified_registry_identity_result(
    root: &Path,
    crate_name: &'static str,
    version: &str,
    checksum: &str,
) -> Result<Option<RegistryIdentity>, MetadataProbeFailure> {
    if !eligible_root(root, crate_name) {
        return Ok(None);
    }
    let Ok(root) = root.canonicalize() else {
        return Ok(None);
    };
    let Some(before) = snapshot(&root) else {
        return Ok(None);
    };
    let bytes = metadata_probe_result(&root)?;
    let metadata = decode_registry_metadata(&bytes)?;
    // All existing identity checks still refuse admission. A refused provider
    // must not be redescribed as a missing dependency or failed Cargo process.
    let Some((package_id, manifest_digests)) = (|| {
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
            .filter(|p| p.get("name").and_then(toml::Value::as_str) == Some(crate_name))
            .collect();
        let [package] = found.as_slice() else {
            return None;
        };
        if package.get("version")?.as_str()? != version
            || package.get("source")?.as_str()? != SOURCE
            || package.get("checksum")?.as_str()? != checksum
        {
            return None;
        }
        let packages = metadata.get("packages")?.as_array()?;
        let found: Vec<_> = packages
            .iter()
            .filter(|p| p.get("name").and_then(serde_json::Value::as_str) == Some(crate_name))
            .collect();
        let [package] = found.as_slice() else {
            return None;
        };
        if package.get("version")?.as_str()? != version
            || package.get("source")?.as_str()? != SOURCE
        {
            return None;
        }
        let package_id = package.get("id")?.as_str()?.to_string();
        let manifest_digests = json_manifest_digests(&metadata, &root)?;
        Some((package_id, manifest_digests))
    })() else {
        return Ok(None);
    };
    // Preserve the existing confirming probe. This is not an added diagnostic
    // retry: either original probe may supply the observed operational failure.
    let confirmed_bytes = metadata_probe_result(&root)?;
    Ok((|| {
        if bytes != confirmed_bytes || manifest_digests != json_manifest_digests(&metadata, &root)?
        {
            return None;
        }
        let after = snapshot(&root)?;
        if before != after {
            return None;
        }
        Some(RegistryIdentity {
            crate_name,
            input_snapshot: after.clone(),
            metadata,
            package_id,
            manifest_digests: manifest_digests.clone(),
            proptest_target_configs: BTreeMap::new(),
            validation_digest: digest(
                &serde_json::to_vec(&(
                    after,
                    manifest_digests,
                    digest(&bytes),
                    crate_name,
                    version,
                    checksum,
                ))
                .ok()?,
            ),
        })
    })())
}

#[cfg(test)]
mod metadata_diagnostic_tests;
