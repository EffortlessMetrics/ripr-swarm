//! Bounded registry identity receipt for SPEC-0197 rule 7a.
//!
//! Cargo's executable and ordinary registry-cache integrity are instrument
//! assumptions. This does not certify arbitrary CLI configuration or a compiler.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";
const CHECKSUM: &str = "a23eb6b1614318a8071c9b2521f36b424b2c83db5eb3a0fead4a6c0809af6e61";
const BUDGET: u64 = 8 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(crate) struct AnyhowIdentity {
    pub(crate) semantic_digest: String,
    pub(crate) validation_digest: String,
    pub(crate) library_root: PathBuf,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_optional(path: &Path) -> Option<Option<Vec<u8>>> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => {
            let meta = std::fs::metadata(path).ok()?;
            if !meta.is_file() || meta.len() > BUDGET {
                return None;
            }
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .ok()?
                .take(BUDGET + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            (bytes.len() as u64 <= BUDGET).then_some(Some(bytes))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some(None),
        _ => None,
    }
}

fn directory_identity(path: &Path) -> Option<String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => {
            if !std::fs::metadata(path).ok()?.is_dir() {
                return None;
            }
            Some(path.canonicalize().ok()?.to_str()?.into())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some("absent".into()),
        _ => None,
    }
}

fn parse(bytes: &[u8]) -> Option<toml::Value> {
    toml::from_str(std::str::from_utf8(bytes).ok()?).ok()
}

fn manifest_allowed(value: &toml::Value) -> bool {
    value.get("patch").is_none() && value.get("replace").is_none()
}

fn manifest_eligible(value: &toml::Value) -> bool {
    if !manifest_allowed(value)
        || value
            .get("package")
            .and_then(|p| p.get("workspace"))
            .is_some()
        || value.get("target").is_some()
        || value
            .get("lib")
            .and_then(|l| l.get("harness"))
            .is_some_and(|v| v.as_bool() != Some(true))
        || value
            .get("lib")
            .and_then(|l| l.get("test"))
            .is_some_and(|v| v.as_bool() != Some(true))
        || value
            .get("lib")
            .and_then(|l| l.get("proc-macro"))
            .is_some_and(|v| v.as_bool() != Some(false))
    {
        return false;
    }
    let Some(dep) = value.get("dependencies").and_then(|d| d.get("anyhow")) else {
        return false;
    };
    match dep {
        toml::Value::String(version) => version == "=1.0.100" || version == "1.0.100",
        toml::Value::Table(table) => {
            table.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "version" | "optional" | "default-features" | "features"
                )
            }) && table
                .get("version")
                .and_then(toml::Value::as_str)
                .is_some_and(|v| v == "=1.0.100" || v == "1.0.100")
                && table
                    .get("optional")
                    .is_none_or(|v| v.as_bool() == Some(false))
        }
        _ => false,
    }
}

/// Cheap eligibility refusal before the metadata process is considered.
pub(crate) fn eligible_manifest(root: &Path) -> bool {
    read_optional(&root.join("Cargo.toml"))
        .flatten()
        .and_then(|bytes| parse(&bytes))
        .is_some_and(|value| manifest_eligible(&value))
}

fn config_allowed(value: &toml::Value) -> bool {
    ["source", "paths", "patch", "replace"]
        .iter()
        .all(|key| value.get(key).is_none())
        && value.get("env").is_none_or(|env| {
            env.as_table()
                .is_some_and(|table| !table.keys().any(|key| key.starts_with("CARGO_")))
        })
}

fn environment_allowed(env: &BTreeMap<String, String>) -> bool {
    !env.keys().any(|key| {
        ["CARGO_SOURCE_", "CARGO_REGISTRIES_", "CARGO_REGISTRY_"]
            .iter()
            .any(|prefix| key.starts_with(prefix))
    })
}

// Select OS-string names first: unrelated bytes are not receipt inputs.
// Cargo prefixes remain relevant even when the rest of the name is invalid.
fn environment_key(key: String, case_insensitive: bool) -> String {
    if case_insensitive {
        key.to_ascii_uppercase()
    } else {
        key
    }
}

fn relevant_environment_name_with_case(name: &std::ffi::OsStr, case_insensitive: bool) -> bool {
    let bytes = if case_insensitive {
        name.as_encoded_bytes().to_ascii_uppercase()
    } else {
        name.as_encoded_bytes().to_vec()
    };
    bytes.starts_with(b"CARGO_")
        || [
            "PATH",
            "HOME",
            "USERPROFILE",
            "RUSTUP_HOME",
            "RUSTUP_TOOLCHAIN",
            "RUSTFLAGS",
        ]
        .iter()
        .any(|key| bytes == key.as_bytes())
}

fn relevant_environment_name(name: &std::ffi::OsStr) -> bool {
    relevant_environment_name_with_case(name, cfg!(windows))
}

// Capture flag changes for currentness without interpreting compiler flags.
fn environment_identity_inputs(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.iter()
        .filter(|(key, _)| relevant_environment_name(std::ffi::OsStr::new(key)))
        .map(|(key, value)| (format!("env:{key}"), digest(value.as_bytes())))
        .collect()
}

fn snapshot(root: &Path) -> Option<BTreeMap<String, String>> {
    let root = root.canonicalize().ok()?;
    let env = std::env::vars_os()
        .filter(|(key, _)| relevant_environment_name(key))
        .map(|(key, value)| {
            Some((
                environment_key(key.into_string().ok()?, cfg!(windows)),
                value.into_string().ok()?,
            ))
        })
        .collect::<Option<BTreeMap<_, _>>>()?;
    if !environment_allowed(&env) {
        return None;
    }
    let home = match env.get("CARGO_HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home),
        Some(_) => return None,
        None => PathBuf::from(env.get(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?)
            .join(".cargo"),
    };
    // Relative homes are resolved by Cargo against the process directory.
    let home = if home.is_absolute() {
        home
    } else {
        root.join(home)
    };
    let mut identities = BTreeMap::new();
    identities.insert("effective-home".into(), home.to_str()?.into());
    identities.insert("effective-home-resolved".into(), directory_identity(&home)?);
    identities.extend(environment_identity_inputs(&env));
    let mut configs = vec![home.join("config"), home.join("config.toml")];
    let mut manifests = Vec::new();
    for dir in root.ancestors() {
        configs.extend([dir.join(".cargo/config"), dir.join(".cargo/config.toml")]);
        manifests.extend([dir.join("Cargo.toml"), dir.join("Cargo.lock")]);
    }
    for path in configs {
        identities.insert(
            format!("config-dir:{}", path.parent()?.to_str()?),
            directory_identity(path.parent()?)?,
        );
        let bytes = read_optional(&path)?;
        if let Some(ref bytes) = bytes {
            identities.insert(
                format!("resolved:{}", path.to_str()?),
                path.canonicalize().ok()?.to_str()?.into(),
            );
            if !config_allowed(&parse(bytes)?) {
                return None;
            }
        }
        identities.insert(
            format!("config:{}", path.to_str()?),
            bytes
                .as_ref()
                .map_or_else(|| "absent".into(), |b| digest(b)),
        );
    }
    for path in manifests {
        let bytes = read_optional(&path)?;
        if bytes.is_some() {
            identities.insert(
                format!("resolved:{}", path.to_str()?),
                path.canonicalize().ok()?.to_str()?.into(),
            );
        }
        if path.file_name()? == "Cargo.toml"
            && let Some(ref bytes) = bytes
            && !manifest_allowed(&parse(bytes)?)
        {
            return None;
        }
        identities.insert(
            format!("input:{}", path.to_str()?),
            bytes
                .as_ref()
                .map_or_else(|| "absent".into(), |b| digest(b)),
        );
    }
    Some(identities)
}

/// Frozen config/environment/manifest identity, without launching Cargo.
pub(crate) fn config_snapshot_identity(root: &Path) -> Option<String> {
    Some(digest(&serde_json::to_vec(&snapshot(root)?).ok()?))
}

fn valid_lock(bytes: &[u8]) -> bool {
    let Some(value) = parse(bytes) else {
        return false;
    };
    let Some(packages) = value.get("package").and_then(toml::Value::as_array) else {
        return false;
    };
    let found = packages
        .iter()
        .filter(|p| p.get("name").and_then(toml::Value::as_str) == Some("anyhow"))
        .collect::<Vec<_>>();
    found.len() == 1
        && found.iter().all(|p| {
            p.get("version").and_then(toml::Value::as_str) == Some("1.0.100")
                && p.get("source").and_then(toml::Value::as_str) == Some(SOURCE)
                && p.get("checksum").and_then(toml::Value::as_str) == Some(CHECKSUM)
        })
}

fn resolved_library(metadata: &serde_json::Value, root: &Path) -> Option<(PathBuf, String)> {
    let packages = metadata.get("packages")?.as_array()?;
    let owners = packages
        .iter()
        .filter(|p| {
            p.get("manifest_path")
                .and_then(serde_json::Value::as_str)
                .and_then(|p| Path::new(p).canonicalize().ok())
                == Some(root.join("Cargo.toml"))
        })
        .collect::<Vec<_>>();
    let [owner] = owners.as_slice() else {
        return None;
    };
    let dependencies = owner.get("dependencies")?.as_array()?;
    let direct = dependencies
        .iter()
        .filter(|d| d.get("name").and_then(serde_json::Value::as_str) == Some("anyhow"))
        .collect::<Vec<_>>();
    let [dependency] = direct.as_slice() else {
        return None;
    };
    if dependency.get("source")?.as_str()? != SOURCE
        || dependency.get("optional")?.as_bool()?
        || !dependency.get("kind")?.is_null()
        || !dependency.get("rename")?.is_null()
        || !dependency.get("target")?.is_null()
    {
        return None;
    }
    let anyhow = packages
        .iter()
        .filter(|p| p.get("name").and_then(serde_json::Value::as_str) == Some("anyhow"))
        .collect::<Vec<_>>();
    let [anyhow] = anyhow.as_slice() else {
        return None;
    };
    if anyhow.get("version")?.as_str()? != "1.0.100" || anyhow.get("source")?.as_str()? != SOURCE {
        return None;
    }
    let id = anyhow.get("id")?.as_str()?;
    let owner_id = owner.get("id")?.as_str()?;
    let nodes = metadata.get("resolve")?.get("nodes")?.as_array()?;
    let node = nodes
        .iter()
        .find(|node| node.get("id").and_then(serde_json::Value::as_str) == Some(owner_id))?;
    let edges = node
        .get("deps")?
        .as_array()?
        .iter()
        .filter(|edge| edge.get("name").and_then(serde_json::Value::as_str) == Some("anyhow"))
        .collect::<Vec<_>>();
    let [edge] = edges.as_slice() else {
        return None;
    };
    if edge.get("pkg")?.as_str()? != id {
        return None;
    }
    let kinds = edge.get("dep_kinds")?.as_array()?;
    if kinds.len() != 1
        || !kinds.iter().all(|kind| {
            kind.get("kind").is_some_and(serde_json::Value::is_null)
                && kind.get("target").is_some_and(serde_json::Value::is_null)
        })
    {
        return None;
    }
    let libs = owner
        .get("targets")?
        .as_array()?
        .iter()
        .filter(|t| {
            t.get("kind")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|k| k.len() == 1 && k.iter().all(|v| v.as_str() == Some("lib")))
        })
        .collect::<Vec<_>>();
    let [lib] = libs.as_slice() else {
        return None;
    };
    if !lib.get("test")?.as_bool()? {
        return None;
    }
    Some((
        PathBuf::from(lib.get("src_path")?.as_str()?)
            .canonicalize()
            .ok()?,
        id.into(),
    ))
}

fn metadata_probe(root: &Path) -> Option<Vec<u8>> {
    if crate::config::repository_toolchain_path_pin(root).is_some() {
        return None;
    }
    let path = std::env::temp_dir().join(format!(
        "ripr-assertion-metadata-{}-{}.json",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .ok()?;
    let result = (|| {
        let mut command = std::process::Command::new("cargo");
        command
            .args([
                "metadata",
                "--offline",
                "--locked",
                "--format-version",
                "1",
                "--manifest-path",
                "Cargo.toml",
            ])
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .stdout(output)
            .stderr(std::process::Stdio::null());
        crate::process_owner::forbid_rustup_auto_install(&mut command);
        let mut child = crate::process_owner::OwnedProcess::spawn(command).ok()?;
        let start = std::time::Instant::now();
        loop {
            if std::fs::metadata(&path).ok()?.len() > BUDGET
                || start.elapsed() >= std::time::Duration::from_mins(2)
            {
                let _ = child.terminate_tree();
                return None;
            }
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(_)) | Err(_) => {
                    let _ = child.terminate_tree();
                    return None;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        let bytes = std::fs::read(&path).ok()?;
        (bytes.len() as u64 <= BUDGET).then_some(bytes)
    })();
    let _ = std::fs::remove_file(path);
    result
}

pub(crate) fn verified_anyhow_identity(package_root: &Path) -> Option<AnyhowIdentity> {
    let root = package_root.canonicalize().ok()?;
    if !eligible_manifest(&root) {
        return None;
    }
    let before = snapshot(&root)?;
    let bytes = metadata_probe(&root)?;
    let metadata = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
    let workspace = Path::new(metadata.get("workspace_root")?.as_str()?)
        .canonicalize()
        .ok()?;
    if !root.starts_with(&workspace) {
        return None;
    }
    let lock = read_optional(&workspace.join("Cargo.lock"))??;
    if !valid_lock(&lock) {
        return None;
    }
    let (library_root, package_id) = resolved_library(&metadata, &root)?;
    let after = snapshot(&root)?;
    if before != after {
        return None;
    }
    Some(AnyhowIdentity {
        semantic_digest: digest(format!("SPEC-0197-7a:{package_id}:{CHECKSUM}").as_bytes()),
        validation_digest: digest(&serde_json::to_vec(&(after, digest(&bytes))).ok()?),
        library_root,
    })
}

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

fn json_manifest_digests(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_member_snapshots_revoke_changed_or_missing_manifests()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-json-member-receipt-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("member"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let root = root.canonicalize()?;
            let member = root.join("member/Cargo.toml");
            std::fs::write(&member, "[dependencies]\nserde_json='=1.0.151'\n")?;
            let metadata = serde_json::json!({"packages":[{"manifest_path":member}]});
            let before = json_manifest_digests(&metadata, &root).ok_or("missing snapshot")?;
            assert_eq!(
                before,
                json_manifest_digests(&metadata, &root).ok_or("unstable snapshot")?
            );
            std::fs::write(&member, "[dependencies]\nserde_json={path='../fake'}\n")?;
            assert_ne!(
                before,
                json_manifest_digests(&metadata, &root).ok_or("missing changed snapshot")?
            );
            std::fs::remove_file(&member)?;
            assert!(json_manifest_digests(&metadata, &root).is_none());
            assert!(json_manifest_digests(&serde_json::json!({"packages":[{}]}), &root).is_none());
            Ok(())
        })();
        std::fs::remove_dir_all(&root)?;
        result
    }

    #[test]
    fn windows_environment_names_preserve_registry_and_home_authority() {
        for key in [
            "cargo_source_crates_io_replace_with",
            "Cargo_Registries_Fake_Index",
        ] {
            assert!(relevant_environment_name_with_case(
                std::ffi::OsStr::new(key),
                true
            ));
            assert!(!environment_allowed(&BTreeMap::from([(
                environment_key(key.into(), true),
                "fake".into()
            )])));
        }
        for key in ["cargo_home", "Path", "UserProfile"] {
            assert!(relevant_environment_name_with_case(
                std::ffi::OsStr::new(key),
                true
            ));
            assert_eq!(environment_key(key.into(), true), key.to_ascii_uppercase());
        }
        assert!(!relevant_environment_name_with_case(
            std::ffi::OsStr::new("cargo_home"),
            false
        ));
    }

    #[test]
    fn exact_regular_manifest_and_refusals() -> Result<(), String> {
        let source =
            "[package]\nname='subject'\nversion='0.1.0'\n[dependencies]\nanyhow='=1.0.100'\n";
        let value: toml::Value = toml::from_str(source).map_err(|e| e.to_string())?;
        assert!(manifest_eligible(&value));
        for change in [
            "anyhow={version='=1.0.100',optional=true}",
            "anyhow={version='=1.0.100',path='local'}",
            "anyhow={version='=1.0.100',package='other'}",
            "anyhow={workspace=true}",
            "anyhow='1'",
        ] {
            let changed: toml::Value = toml::from_str(&source.replace("anyhow='=1.0.100'", change))
                .map_err(|e| e.to_string())?;
            assert!(!manifest_eligible(&changed), "{change}");
        }
        for suffix in [
            "[lib]\nharness=false",
            "[lib]\ntest=false",
            "[patch.crates-io]\nanyhow={path='local'}",
            "[replace]\n'anyhow:1.0.100'={path='local'}",
            "[target.'cfg(unix)'.dependencies]\nfoo='1'",
        ] {
            let changed: toml::Value =
                toml::from_str(&format!("{source}{suffix}\n")).map_err(|e| e.to_string())?;
            assert!(!manifest_eligible(&changed), "{suffix}");
        }
        Ok(())
    }

    #[test]
    fn injected_config_and_environment_refusals() -> Result<(), String> {
        for source in [
            "[source.crates-io]\nreplace-with='fake'",
            "paths=['fake']",
            "[patch.crates-io]\nanyhow={path='fake'}",
            "[replace]\n'anyhow:1.0.100'={path='fake'}",
            "[env]\nCARGO_HOME='fake'",
            "[env]\nCARGO_REGISTRIES_TEST_INDEX={value='fake'}",
        ] {
            let value: toml::Value = toml::from_str(source).map_err(|e| e.to_string())?;
            assert!(!config_allowed(&value), "{source}");
        }
        let ordinary: toml::Value =
            toml::from_str("[build]\njobs=2\n[env]\nTMPDIR='target'").map_err(|e| e.to_string())?;
        assert!(config_allowed(&ordinary));
        for key in [
            "CARGO_SOURCE_CRATES_IO_REPLACE_WITH",
            "CARGO_REGISTRIES_FAKE_INDEX",
            "CARGO_REGISTRY_DEFAULT",
        ] {
            assert!(!environment_allowed(&BTreeMap::from([(
                key.into(),
                "fake".into()
            )])));
        }
        assert!(environment_allowed(&BTreeMap::from([
            ("CARGO_HOME".into(), "cache".into()),
            ("CARGO_BUILD_JOBS".into(), "2".into()),
            ("CARGO_TARGET_DIR".into(), "target".into())
        ])));
        Ok(())
    }

    #[test]
    fn flag_identity_changes_are_bound_without_flag_interpretation() {
        let empty = BTreeMap::new();
        for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"] {
            let first = BTreeMap::from([(key.into(), "--cfg harmless_a".into())]);
            let second = BTreeMap::from([(key.into(), "--cfg harmless_b".into())]);
            let identity = environment_identity_inputs(&first);
            assert!(identity.contains_key(&format!("env:{key}")));
            assert_ne!(identity, environment_identity_inputs(&empty));
            assert_ne!(identity, environment_identity_inputs(&second));
            // Hash binding is not an approval or interpretation of the flags.
            assert!(environment_allowed(&first));
        }
    }

    #[test]
    fn canonical_lock_requires_unique_checksum_identity() {
        let source = format!(
            "version=4\n[[package]]\nname='anyhow'\nversion='1.0.100'\nsource='{SOURCE}'\nchecksum='{CHECKSUM}'\n"
        );
        assert!(valid_lock(source.as_bytes()));
        assert!(!valid_lock(source.replace(CHECKSUM, "wrong").as_bytes()));
        assert!(!valid_lock(
            source
                .replace(SOURCE, "registry+https://fake.invalid/index")
                .as_bytes()
        ));
        assert!(!valid_lock(source.replace("1.0.100", "1.0.99").as_bytes()));
        assert!(!valid_lock(
            format!("{source}{}", source.replace("version=4\n", "")).as_bytes()
        ));
    }

    #[test]
    fn resolved_regular_edge_and_library_controls() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-anyhow-identity-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            std::fs::write(root.join("Cargo.toml"), "")?;
            std::fs::write(root.join("lib.rs"), "")?;
            let root = root.canonicalize()?;
            let id = "registry+https://github.com/rust-lang/crates.io-index#anyhow@1.0.100";
            let metadata = serde_json::json!({
                "packages": [
                    {"id":"owner", "manifest_path":root.join("Cargo.toml"), "dependencies":[{"name":"anyhow", "source":SOURCE, "optional":false,"kind":null,"rename":null,"target":null}],
                    "targets":[{"kind":["lib"],"test":true,"src_path":root.join("lib.rs")}]},
                    {"id":id,"name":"anyhow","version":"1.0.100","source":SOURCE}
                ],
                "resolve":{"nodes":[{"id":"owner","deps":[{"name":"anyhow","pkg":id,"dep_kinds":[{"kind":null,"target":null}]}]}]}
            });
            assert!(resolved_library(&metadata, &root).is_some());
            for (pointer, replacement) in [
                (
                    "/packages/0/dependencies/0/optional",
                    serde_json::json!(true),
                ),
                (
                    "/packages/0/dependencies/0/rename",
                    serde_json::json!("other"),
                ),
                ("/packages/0/dependencies/0/kind", serde_json::json!("dev")),
                (
                    "/packages/0/dependencies/0/target",
                    serde_json::json!("cfg(unix)"),
                ),
                ("/packages/0/targets/0/test", serde_json::json!(false)),
                ("/packages/1/source", serde_json::json!("path+file:///fake")),
                ("/resolve/nodes/0/deps/0/pkg", serde_json::json!("other")),
                (
                    "/resolve/nodes/0/deps/0/dep_kinds/0/kind",
                    serde_json::json!("dev"),
                ),
            ] {
                let mut changed = metadata.clone();
                let value = changed
                    .pointer_mut(pointer)
                    .ok_or("missing injected pointer")?;
                *value = replacement;
                assert!(resolved_library(&changed, &root).is_none(), "{pointer}");
            }
            Ok(())
        })();
        std::fs::remove_dir_all(root)?;
        result
    }
}
