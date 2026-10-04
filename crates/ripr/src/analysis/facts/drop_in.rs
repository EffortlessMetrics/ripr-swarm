//! Manifest authority for drop-in assertion crates.
//!
//! `use pretty_assertions::assert_eq;` keeps the standard assertion's meaning
//! only when the `pretty_assertions` crate name resolves to the crates.io
//! package. Cargo can bind that name to any package: a dependency key with
//! `package = ..`, a `path`, `git` or `registry` source, or a `[patch]` /
//! `[replace]` entry. None of those leaves a trace in Rust source, so the
//! macro-binding scan asks this authority before it trusts the import.
//!
//! A file is verified only when the nearest `Cargo.toml` at or below the
//! analysis root declares the crate, every declaration is a plain registry
//! requirement (resolved through `[workspace.dependencies]` for
//! `workspace = true`), and no manifest or `.cargo/config` between the file
//! and the root can substitute it (RIPR-SPEC-0197). Anything unread,
//! unparsable or unexpected is unverified. Cargo configuration outside the
//! analysis root (`$CARGO_HOME`, directories above the root) is not read,
//! and a file compiled by a sibling package's target `path` is judged by its
//! nearest manifest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const DEPENDENCY_TABLES: &[&str] = &[
    "dependencies",
    "dev-dependencies",
    "dev_dependencies",
    "build-dependencies",
    "build_dependencies",
];

/// Keys a plain registry requirement may carry. Any other key (`package`,
/// `path`, `git`, `registry`, `artifact`, ..) can change which package the
/// name resolves to, or is unknown, so it fails closed.
const PLAIN_KEYS: &[&str] = &[
    "version",
    "features",
    "default-features",
    "default_features",
    "optional",
    "public",
];

/// Per-analysis cache of drop-in crate verdicts, keyed by the importing
/// file's directory. Clones share the cache.
#[derive(Clone, Debug, Default)]
pub(crate) struct DropInManifests {
    root: Option<PathBuf>,
    verdicts: Arc<Mutex<BTreeMap<(PathBuf, String), bool>>>,
}

impl DropInManifests {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: Some(root.to_path_buf()),
            verdicts: Arc::default(),
        }
    }

    /// Whether `krate`, imported by `file`, is the plain registry package of
    /// that name. `file` is root-relative, as index and dependent-scope paths
    /// are; an absolute path is used as is.
    pub(crate) fn verified(&self, file: &Path, krate: &str) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let file = root.join(file);
        let Some(directory) = file.parent() else {
            return false;
        };
        let key = (directory.to_path_buf(), krate.to_string());
        if let Some(verdict) = self
            .verdicts
            .lock()
            .ok()
            .and_then(|verdicts| verdicts.get(&key).copied())
        {
            return verdict;
        }
        let verdict = verify(root, directory, krate);
        if let Ok(mut verdicts) = self.verdicts.lock() {
            verdicts.insert(key, verdict);
        }
        verdict
    }
}

fn verify(root: &Path, directory: &Path, krate: &str) -> bool {
    // The directories from the file's own up to the root, inclusive. A file
    // outside the root has no readable crate manifest.
    let directories: Vec<&Path> = directory
        .ancestors()
        .take_while(|candidate| candidate.starts_with(root))
        .collect();
    if directories.is_empty() {
        return false;
    }
    let mut manifests = Vec::new();
    for dir in &directories {
        if config_mentions(dir, krate) {
            return false;
        }
        match std::fs::read_to_string(dir.join("Cargo.toml")) {
            Ok(text) => match text.parse::<toml::Table>() {
                Ok(manifest) => manifests.push(manifest),
                Err(_) => return false,
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return false,
        }
    }
    let Some(own) = manifests.first() else {
        return false;
    };
    // `package.workspace` can name a workspace off the ancestor chain, whose
    // dependencies and patches ripr did not read.
    if own
        .get("package")
        .and_then(|package| package.get("workspace"))
        .is_some()
    {
        return false;
    }
    if manifests.iter().any(|manifest| patches(manifest, krate)) {
        return false;
    }
    let workspace = manifests
        .iter()
        .find_map(|manifest| manifest.get("workspace").and_then(toml::Value::as_table));
    let mut declared = false;
    for table in dependency_tables(own) {
        for (key, spec) in table {
            if normalized(key) != krate {
                continue;
            }
            declared = true;
            if !plain(spec, krate, workspace) {
                return false;
            }
        }
    }
    declared
}

/// Every dependency table of one manifest, including `[target.*]` ones.
fn dependency_tables(manifest: &toml::Table) -> Vec<&toml::Table> {
    let direct = DEPENDENCY_TABLES
        .iter()
        .filter_map(|name| manifest.get(*name).and_then(toml::Value::as_table));
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(toml::Value::as_table)
        .flat_map(|target| {
            DEPENDENCY_TABLES
                .iter()
                .filter_map(|name| target.get(*name).and_then(toml::Value::as_table))
        });
    direct.chain(targets).collect()
}

fn plain(spec: &toml::Value, krate: &str, workspace: Option<&toml::Table>) -> bool {
    match spec {
        toml::Value::String(_) => true,
        toml::Value::Table(table) => {
            if table.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                let inherited = workspace
                    .and_then(|workspace| workspace.get("dependencies"))
                    .and_then(toml::Value::as_table)
                    .and_then(|dependencies| {
                        dependencies
                            .iter()
                            .find(|(key, _)| normalized(key) == krate)
                            .map(|(_, spec)| spec)
                    });
                return table
                    .keys()
                    .all(|key| key == "workspace" || PLAIN_KEYS.contains(&key.as_str()))
                    && inherited.is_some_and(|inherited| plain(inherited, krate, None));
            }
            table.keys().all(|key| PLAIN_KEYS.contains(&key.as_str()))
        }
        _ => false,
    }
}

/// A `[patch.<source>]` entry Cargo may match to the crate (by key, or by a
/// `package =` rename, which Cargo matches on), or any `[replace]` entry:
/// its package-ID specs (`name@version`, `url#name@version`) are not worth
/// parsing for a deprecated table.
fn patches(manifest: &toml::Table, krate: &str) -> bool {
    let patched = manifest
        .get("patch")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|sources| sources.values())
        .filter_map(toml::Value::as_table)
        .flat_map(|entries| entries.iter())
        .any(|(key, spec)| {
            normalized(key) == krate
                || spec
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|package| normalized(package) == krate)
        });
    let replaced = manifest
        .get("replace")
        .and_then(toml::Value::as_table)
        .is_some_and(|entries| !entries.is_empty());
    patched || replaced
}

/// A `.cargo/config` that can substitute a package without naming it in a
/// way ripr resolves: a `paths` override, a `[patch]` or `[source]` table,
/// an `include` of another config, or any mention of the crate. An
/// unreadable or unparsable config fails closed too.
fn config_mentions(directory: &Path, krate: &str) -> bool {
    let hyphenated = krate.replace('_', "-");
    ["config.toml", "config"].iter().any(|name| {
        match std::fs::read_to_string(directory.join(".cargo").join(name)) {
            Ok(text) => {
                text.contains(krate)
                    || text.contains(&hyphenated)
                    || text.parse::<toml::Table>().map_or(true, |config| {
                        ["paths", "patch", "source", "include"]
                            .iter()
                            .any(|key| config.contains_key(*key))
                    })
            }
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        }
    })
}

fn normalized(name: &str) -> String {
    name.replace('-', "_")
}

#[cfg(test)]
pub(crate) fn temp_workspace(label: &str, files: &[(&str, &str)]) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-drop-in-{label}-{}-{nanos}",
        std::process::id()
    ));
    for (path, text) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&path, text).map_err(|error| error.to_string())?;
    }
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KRATE: &str = "pretty_assertions";
    const FILE: &str = "src/lib.rs";

    fn verified(label: &str, files: &[(&str, &str)], file: &str) -> Result<bool, String> {
        let root = temp_workspace(label, files)?;
        let verdict = DropInManifests::new(&root).verified(Path::new(file), KRATE);
        // Relative and root-joined spellings of one file agree.
        let joined = DropInManifests::new(&root).verified(&root.join(file), KRATE);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(verdict, joined, "{label}");
        Ok(verdict)
    }

    fn manifest(dependencies: &str) -> String {
        format!("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n{dependencies}\n")
    }

    #[test]
    fn a_plain_registry_requirement_is_verified() -> Result<(), String> {
        for dependencies in [
            "[dev-dependencies]\npretty_assertions = \"1\"",
            "[dev-dependencies]\npretty_assertions = { version = \"1\", default-features = false }",
            "[dependencies]\npretty_assertions = { version = \"1\", optional = true }",
            "[target.'cfg(unix)'.dev-dependencies]\npretty_assertions = \"1\"",
        ] {
            assert!(
                verified("plain", &[("Cargo.toml", &manifest(dependencies))], FILE)?,
                "{dependencies}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_crate_name_cargo_may_bind_elsewhere_is_unverified() -> Result<(), String> {
        for dependencies in [
            // Not declared at all, or declared under another source.
            "[dev-dependencies]\nsimilar = \"2\"",
            "[dev-dependencies]\npretty_assertions = { package = \"fake-assertions\", version = \"1\" }",
            "[dev-dependencies]\npretty-assertions = { package = \"fake-assertions\", version = \"1\" }",
            "[dev-dependencies]\npretty_assertions = { path = \"../fake\" }",
            "[dev-dependencies]\npretty_assertions = { git = \"https://example.invalid/fake\" }",
            "[dev-dependencies]\npretty_assertions = { version = \"1\", registry = \"internal\" }",
            "[dev-dependencies]\npretty_assertions = \"1\"\n[target.'cfg(unix)'.dev-dependencies]\npretty_assertions = { path = \"../fake\" }",
            "[dev-dependencies]\npretty_assertions = \"1\"\n[patch.crates-io]\npretty_assertions = { path = \"../fake\" }",
            "[dev-dependencies]\npretty_assertions = \"1\"\n[replace]\n\"pretty_assertions:1.4.0\" = { path = \"../fake\" }",
            "[dev-dependencies]\npretty_assertions = { workspace = true }",
            // Cargo matches a patch on its `package`, whatever the key.
            "[dev-dependencies]\npretty_assertions = \"1\"\n[patch.crates-io]\nanything = { path = \"fake\", package = \"pretty_assertions\" }",
            // Package-ID-spec `[replace]` keys.
            "[dev-dependencies]\npretty_assertions = \"1\"\n[replace]\n\"pretty_assertions@1.4.1\" = { path = \"fake\" }",
            "[dev-dependencies]\npretty_assertions = \"1\"\n[replace]\n\"https://github.com/rust-lang/crates.io-index#pretty_assertions@1.4.1\" = { path = \"fake\" }",
        ] {
            assert!(
                !verified("aliased", &[("Cargo.toml", &manifest(dependencies))], FILE)?,
                "{dependencies}"
            );
        }
        assert!(!verified("unparsed", &[("Cargo.toml", "[package")], FILE)?);
        assert!(!verified("no-manifest", &[], FILE)?);
        Ok(())
    }

    #[test]
    fn a_member_reads_its_own_manifest_and_the_workspace_above_it() -> Result<(), String> {
        let member = "crates/a/src/lib.rs";
        let plain_member = manifest("[dev-dependencies]\npretty_assertions = { workspace = true }");
        let workspace = |dependency: &str| {
            format!(
                "[workspace]\nmembers = [\"crates/a\"]\n\n[workspace.dependencies]\n{dependency}\n"
            )
        };
        assert!(verified(
            "inherited",
            &[
                ("Cargo.toml", &workspace("pretty_assertions = \"1\"")),
                ("crates/a/Cargo.toml", &plain_member),
            ],
            member,
        )?);
        assert!(!verified(
            "inherited-alias",
            &[
                (
                    "Cargo.toml",
                    &workspace("pretty_assertions = { package = \"fake\", version = \"1\" }"),
                ),
                ("crates/a/Cargo.toml", &plain_member),
            ],
            member,
        )?);
        // The root declares it plainly, but the member's own manifest decides.
        assert!(!verified(
            "member-alias",
            &[
                (
                    "Cargo.toml",
                    &manifest("[dev-dependencies]\npretty_assertions = \"1\"")
                ),
                (
                    "crates/a/Cargo.toml",
                    &manifest("[dev-dependencies]\npretty_assertions = { path = \"../fake\" }"),
                ),
            ],
            member,
        )?);
        // A root patch applies to every member.
        assert!(!verified(
            "root-patch",
            &[
                (
                    "Cargo.toml",
                    &format!(
                        "{}\n[patch.crates-io]\npretty-assertions = {{ path = \"fake\" }}\n",
                        workspace("pretty_assertions = \"1\"")
                    ),
                ),
                ("crates/a/Cargo.toml", &plain_member),
            ],
            member,
        )?);
        // `package.workspace` names a workspace off the ancestor chain.
        assert!(!verified(
            "foreign-workspace",
            &[
                ("Cargo.toml", &workspace("pretty_assertions = \"1\"")),
                (
                    "crates/a/Cargo.toml",
                    &format!(
                        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nworkspace = \"../../ws2\"\n\n[dev-dependencies]\npretty_assertions = {{ workspace = true }}\n"
                    ),
                ),
            ],
            member,
        )?);
        // A `paths` override substitutes a same-named package unnamed.
        assert!(!verified(
            "config-paths",
            &[
                (
                    "Cargo.toml",
                    &manifest("[dev-dependencies]\npretty_assertions = \"1\"")
                ),
                (".cargo/config.toml", "paths = [\"vendorx\"]\n"),
            ],
            FILE,
        )?);
        // An unrelated config setting leaves the verdict alone.
        assert!(verified(
            "config-unrelated",
            &[
                (
                    "Cargo.toml",
                    &manifest("[dev-dependencies]\npretty_assertions = \"1\"")
                ),
                (".cargo/config.toml", "[build]\njobs = 2\n"),
            ],
            FILE,
        )?);
        assert!(!verified(
            "config-patch",
            &[
                (
                    "Cargo.toml",
                    &manifest("[dev-dependencies]\npretty_assertions = \"1\"")
                ),
                (
                    ".cargo/config.toml",
                    "[patch.crates-io]\npretty_assertions = { path = \"fake\" }\n",
                ),
            ],
            FILE,
        )?);
        Ok(())
    }

    #[test]
    fn a_file_outside_the_root_is_unverified() -> Result<(), String> {
        let root = temp_workspace(
            "outside",
            &[(
                "Cargo.toml",
                &manifest("[dev-dependencies]\npretty_assertions = \"1\""),
            )],
        )?;
        let outside = std::env::temp_dir().join("elsewhere/src/lib.rs");
        let verdict = DropInManifests::new(&root).verified(&outside, KRATE);
        let unrooted = DropInManifests::default().verified(Path::new(FILE), KRATE);
        let _ = std::fs::remove_dir_all(&root);
        assert!(!verdict);
        assert!(!unrooted);
        Ok(())
    }
}
