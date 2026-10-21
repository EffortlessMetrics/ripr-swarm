//! Manifest authority for importing a workspace member's own crate.
//!
//! The same-name import gate treats `use <crate>::owner;` as binding the
//! changed owner only when `<crate>` is one of the workspace's own crate
//! names. `RustIndex.package_names` holds the root manifest's names alone, so
//! in a virtual workspace a test in `orders` that writes `use pricing::score;`
//! read as a foreign import and its exact pin on `score` was refused.
//!
//! This authority answers, for one test file and one owner file, which crate
//! identifiers name the owner's library crate from the test's crate, read
//! from the manifests themselves (RIPR-SPEC-0197):
//!
//! - the owner must compose under its package's library root, the default
//!   `src/lib.rs` (an explicit `[lib] path` naming exactly that file is
//!   still the default root, #7097) with `autolib` on, because another
//!   crate can only import a library;
//! - a test in the same package names it by its library identifier (`[lib]
//!   name`, else the package name with hyphens as underscores);
//! - a test in another package names it by the key of a `[dependencies]` or
//!   `[dev-dependencies]` entry (including `[target.*]` tables) whose `path`
//!   resolves to the owner's package directory, directly or through
//!   `[workspace.dependencies]` for `workspace = true`. A `package =` rename
//!   must name the owner's package, and the import name is then the key;
//!   without it the key must be the package name and the import name is the
//!   library identifier.
//!
//! Anything unread, unparsable or unexpected answers nothing, so the gate
//! keeps treating the import as foreign: `package.workspace`, a manifest
//! between either file and the root with a `[patch]` for the name or any
//! `[replace]`, or a `.cargo/config` that mentions the name or sets `paths`,
//! `[patch]`, `[source]` or `include`.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::drop_in::{config_mentions, normalized, patches};

/// Dependency tables whose crates a test target can import. Build
/// dependencies reach only the build script.
const TEST_DEPENDENCY_TABLES: &[&str] = &["dependencies", "dev-dependencies", "dev_dependencies"];

/// Import names per (test file directory, owner crate root).
type ImportNames = BTreeMap<(PathBuf, PathBuf), Vec<String>>;
/// Foreign-export answers per (owner crate root, item name).
type ForeignExports = BTreeMap<(PathBuf, String), bool>;

/// Per-analysis cache of member-crate import names, keyed by the test file's
/// directory and the owner's crate root. Clones share the cache.
#[derive(Clone, Debug, Default)]
pub(crate) struct MemberCrates {
    root: Option<PathBuf>,
    names: Arc<Mutex<ImportNames>>,
    /// Per (crate root, item name): whether the library may export a
    /// foreign item of that name, computed by the classifier.
    foreign_exports: Arc<Mutex<ForeignExports>>,
}

impl MemberCrates {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: Some(root.to_path_buf()),
            names: Arc::default(),
            foreign_exports: Arc::default(),
        }
    }

    /// Whether manifest authority is available: synthetic indexes built
    /// without a workspace root have none, so crate-identity checks that
    /// need it fall back to the index's package names (#7097).
    pub(crate) fn has_manifest_root(&self) -> bool {
        self.root.is_some()
    }

    /// `compute`'s answer for (`owner_root`, `name`), computed once per
    /// analysis.
    pub(crate) fn may_export_foreign(
        &self,
        owner_root: &Path,
        name: &str,
        compute: impl FnOnce() -> bool,
    ) -> bool {
        let key = (owner_root.to_path_buf(), name.to_string());
        if let Some(known) = self
            .foreign_exports
            .lock()
            .ok()
            .and_then(|known| known.get(&key).copied())
        {
            return known;
        }
        let answer = compute();
        if let Ok(mut known) = self.foreign_exports.lock() {
            known.insert(key, answer);
        }
        answer
    }

    /// The crate identifiers through which `test_file`'s crate imports the
    /// library whose crate root is `owner_root`, the root module file the
    /// owner composes under. Both paths are root-relative, as index paths
    /// are; an absolute path is used as is.
    pub(crate) fn import_names(&self, test_file: &Path, owner_root: &Path) -> Vec<String> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        // Fold `.` and `..` on every side alike: the analysis root is often
        // `.`, and a lexical file path no longer starts with it.
        let root = lexical(root);
        let test_file = lexical(&root.join(test_file));
        let owner_root = lexical(&root.join(owner_root));
        let (Some(test_dir), Some(owner_dir)) = (test_file.parent(), owner_root.parent()) else {
            return Vec::new();
        };
        let key = (test_dir.to_path_buf(), owner_root.clone());
        if let Some(names) = self
            .names
            .lock()
            .ok()
            .and_then(|names| names.get(&key).cloned())
        {
            return names;
        }
        let names = resolve(&root, test_dir, owner_dir, &owner_root).unwrap_or_default();
        if let Ok(mut cache) = self.names.lock() {
            cache.insert(key, names.clone());
        }
        names
    }

    /// Whether `crate_root`, a root-relative `src/lib.rs` or `src/main.rs`,
    /// is its package's only reading of that file as a crate root: the
    /// manifest leaves `autolib`/`autobins` on and moves no target with a
    /// `path` (a `[[bin]]` naming `src/main.rs` itself excepted). A moved
    /// `[lib] path` makes `src/lib.rs` a module of another root, whose
    /// `crate::` differs. Without manifest authority, or with an unreadable
    /// manifest, nothing is a default root.
    pub(crate) fn is_default_target_root(&self, crate_root: &Path) -> bool {
        self.default_target_root_with_flags(crate_root, &["autolib", "autobins"])
    }

    /// Unit-test identity follows the discovery flag for this root's target
    /// kind. Disabling automatic binaries does not disable the default library
    /// (or vice versa). Keep the broader root predicate above unchanged for its
    /// other consumers, and retain its conservative moved-target checks.
    pub(crate) fn is_default_unit_target_root(&self, crate_root: &Path) -> bool {
        if crate_root
            .parent()
            .and_then(Path::file_name)
            .is_none_or(|name| name != "src")
        {
            return false;
        }
        let flag = match crate_root.file_name().and_then(|name| name.to_str()) {
            Some("lib.rs") => "autolib",
            Some("main.rs") => "autobins",
            _ => return false,
        };
        self.default_target_root_with_flags(crate_root, &[flag])
    }

    fn default_target_root_with_flags(&self, crate_root: &Path, required_flags: &[&str]) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let root = lexical(root);
        let crate_root = lexical(&root.join(crate_root));
        let Some(directory) = crate_root.parent().and_then(Path::parent) else {
            return false;
        };
        let Some((package, _)) = nearest_package(&root, directory) else {
            return false;
        };
        if package.directory != directory {
            return false;
        }
        let manifest = &package.manifest;
        let flag_untrusted = |key: &str| {
            manifest
                .get("package")
                .and_then(|package| package.get(key))
                .is_some_and(|value| match value.as_bool() {
                    Some(enabled) => !enabled && required_flags.contains(&key),
                    None => true,
                })
        };
        if flag_untrusted("autolib") || flag_untrusted("autobins") {
            return false;
        }
        let names_path = |path: &toml::Value, default: &str| {
            path.as_str()
                .is_some_and(|text| lexical(Path::new(text)) == Path::new("src").join(default))
        };
        let lib_moved = manifest
            .get("lib")
            .and_then(|lib| lib.get("path"))
            .is_some_and(|path| !names_path(path, "lib.rs"));
        let target_moved = ["bin", "test", "bench", "example"].iter().any(|kind| {
            manifest.get(*kind).is_some_and(|targets| {
                targets.as_array().is_none_or(|targets| {
                    targets.iter().any(|target| {
                        target
                            .get("path")
                            .is_some_and(|path| !(*kind == "bin" && names_path(path, "main.rs")))
                    })
                })
            })
        });
        !lib_moved && !target_moved
    }

    /// Whether the packages holding `files` (root-relative) depend on no
    /// crate outside them: every dependency of every table is a plain
    /// `path` dependency, directly or through `[workspace.dependencies]`,
    /// on another of those packages, none is a proc-macro library, and no
    /// `.cargo` configuration sits between them and the root. A renamed or
    /// outside crate could otherwise bring a trait under a name the source
    /// reads as the standard library's or the workspace's own (#8059
    /// review). Without manifest authority nothing is closed.
    pub(crate) fn is_dependency_closed<'a>(
        &self,
        files: impl IntoIterator<Item = &'a PathBuf>,
    ) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let root = lexical(root);
        let mut packages: Vec<(Package, Vec<PathBuf>)> = Vec::new();
        let mut indexed = std::collections::BTreeSet::new();
        for file in files {
            let file = lexical(&root.join(file));
            indexed.insert(file.clone());
            let Some(directory) = file.parent() else {
                return false;
            };
            if packages
                .iter()
                .any(|(_, chain)| chain.first().is_some_and(|known| known == directory))
            {
                continue;
            }
            let Some(found) = nearest_package(&root, directory) else {
                return false;
            };
            if !packages
                .iter()
                .any(|(known, _)| known.directory == found.0.directory)
            {
                packages.push(found);
            }
        }
        let directories: Vec<&Path> = packages
            .iter()
            .map(|(package, _)| package.directory.as_path())
            .collect();
        packages.iter().all(|(package, chain)| {
            let manifest = &package.manifest;
            // A target `path` may put a crate root outside the package, and
            // a Rust file the index skipped (`fixtures/`, `node_modules/`)
            // may still be a module some root compiles (#8059 re-review).
            let moved = ["lib", "bin", "test", "bench", "example"]
                .iter()
                .any(|kind| {
                    manifest
                        .get(*kind)
                        .is_some_and(|targets| match targets.as_array() {
                            Some(targets) => {
                                targets.iter().any(|target| target.get("path").is_some())
                            }
                            None => targets.get("path").is_some(),
                        })
                });
            if moved || !rust_files_indexed(&package.directory, &indexed) {
                return false;
            }
            if manifest.get("lib").is_some_and(|lib| {
                lib.get("proc-macro").is_some() || lib.get("proc_macro").is_some()
            }) || chain.iter().any(|directory| {
                ["config.toml", "config"]
                    .iter()
                    .any(|name| directory.join(".cargo").join(name).exists())
            }) {
                return false;
            }
            let Some(manifests) = ancestor_manifests(chain) else {
                return false;
            };
            if manifests.iter().any(|(_, manifest)| {
                manifest.contains_key("patch") || manifest.contains_key("replace")
            }) {
                return false;
            }
            let workspace = manifests.iter().find_map(|(directory, manifest)| {
                manifest
                    .get("workspace")
                    .and_then(toml::Value::as_table)
                    .map(|workspace| (directory.as_path(), workspace))
            });
            all_dependency_tables(&package.manifest)
                .iter()
                .flat_map(|table| table.iter())
                .all(|(key, spec)| {
                    internal_path(key, spec, &package.directory, workspace)
                        .is_some_and(|target| directories.contains(&target.as_path()))
                })
        })
    }
}

/// Whether every `.rs` file under `directory` is in `indexed`, the package's
/// own `target/` excepted. A symbolic link or an unreadable directory
/// answers no.
fn rust_files_indexed(directory: &Path, indexed: &std::collections::BTreeSet<PathBuf>) -> bool {
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            return false;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                return false;
            };
            let Ok(kind) = entry.file_type() else {
                return false;
            };
            let path = lexical(&entry.path());
            if kind.is_symlink() {
                return false;
            }
            if kind.is_dir() {
                if !(current == directory && entry.file_name() == "target") {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && !indexed.contains(&path)
            {
                return false;
            }
        }
    }
    true
}

/// Every dependency table of `manifest`, build dependencies and `[target.*]`
/// tables included.
fn all_dependency_tables(manifest: &toml::Table) -> Vec<&toml::Table> {
    const TABLES: &[&str] = &[
        "dependencies",
        "dev-dependencies",
        "dev_dependencies",
        "build-dependencies",
        "build_dependencies",
    ];
    let direct = TABLES
        .iter()
        .filter_map(|name| manifest.get(*name).and_then(toml::Value::as_table));
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(toml::Value::as_table)
        .flat_map(|target| {
            TABLES
                .iter()
                .filter_map(|name| target.get(*name).and_then(toml::Value::as_table))
        });
    direct.chain(targets).collect()
}

/// The directory a plain, unrenamed `path` dependency entry names.
fn internal_path(
    key: &str,
    spec: &toml::Value,
    directory: &Path,
    workspace: Option<(&Path, &toml::Table)>,
) -> Option<PathBuf> {
    let spec = spec.as_table()?;
    let (source, base) = if spec.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
        if spec.contains_key("package") || spec.contains_key("path") {
            return None;
        }
        let (directory, workspace) = workspace?;
        let inherited = workspace
            .get("dependencies")
            .and_then(toml::Value::as_table)?
            .get(key)?
            .as_table()?;
        (inherited, directory)
    } else {
        (spec, directory)
    };
    if ["git", "registry", "package"]
        .iter()
        .any(|field| source.contains_key(*field))
    {
        return None;
    }
    let path = source.get("path")?.as_str()?;
    Some(lexical(&base.join(path)))
}

impl MemberCrates {
    /// Whether `cargo test` runs the doc examples of the library whose crate
    /// root is `crate_root`, a root-relative `src/lib.rs` (#6648): the
    /// manifest keeps that file as its default library root
    /// ([`Self::is_default_target_root`]), leaves `[lib] doctest` on and
    /// builds a Rust library a doctest can link (`lib`, `rlib` or
    /// `proc-macro`, or no `crate-type`). Without manifest authority, or with
    /// any value it cannot read, the answer is no.
    pub(crate) fn runs_library_doctests(&self, crate_root: &Path) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        if !crate_root.ends_with(Path::new("src").join("lib.rs"))
            || !self.is_default_target_root(crate_root)
        {
            return false;
        }
        let root = lexical(root);
        let crate_root = lexical(&root.join(crate_root));
        let Some(directory) = crate_root.parent().and_then(Path::parent) else {
            return false;
        };
        let Some((package, _)) = nearest_package(&root, directory) else {
            return false;
        };
        if package.directory != directory || !is_default_library_root(&package, &crate_root) {
            return false;
        }
        let Some(lib) = package.manifest.get("lib") else {
            return true;
        };
        let doctest_on = lib
            .get("doctest")
            .is_none_or(|value| value.as_bool() == Some(true));
        let linkable = lib.get("crate-type").is_none_or(|kinds| {
            kinds.as_array().is_some_and(|kinds| {
                kinds.iter().any(|kind| {
                    kind.as_str()
                        .is_some_and(|kind| matches!(kind, "lib" | "rlib"))
                })
            })
        });
        // A proc-macro crate exports no plain `pub fn` an example could
        // call, so it registers nothing.
        let not_proc_macro = lib
            .get("proc-macro")
            .is_none_or(|value| value.as_bool() == Some(false));
        doctest_on && linkable && not_proc_macro
    }
}

/// One package manifest: its directory and parsed table.
struct Package {
    directory: PathBuf,
    manifest: toml::Table,
}

impl Package {
    fn name(&self) -> Option<&str> {
        self.manifest.get("package")?.get("name")?.as_str()
    }

    /// The library's crate identifier: `[lib] name`, else the package name.
    fn library_identifier(&self) -> Option<String> {
        let lib = self
            .manifest
            .get("lib")
            .and_then(|lib| lib.get("name"))
            .and_then(toml::Value::as_str);
        lib.or_else(|| self.name()).map(normalized)
    }
}

fn resolve(
    root: &Path,
    test_dir: &Path,
    owner_dir: &Path,
    owner_root: &Path,
) -> Option<Vec<String>> {
    let (owner, owner_chain) = nearest_package(root, owner_dir)?;
    let (test, test_chain) = nearest_package(root, test_dir)?;
    let owner_name = owner.name()?;
    let library = owner.library_identifier()?;
    if !is_default_library_root(&owner, owner_root) {
        return None;
    }
    // Cargo configuration and manifests between either file and the root can
    // substitute the owner's package under a name Rust source never shows.
    let mentions = [normalized(owner_name), library.clone()];
    if owner_chain
        .iter()
        .chain(&test_chain)
        .any(|directory| mentions.iter().any(|name| config_mentions(directory, name)))
    {
        return None;
    }
    if test.directory == owner.directory {
        // A dependency keyed by the library's own name would shadow it.
        let shadowed = test_dependency_tables(&test.manifest)
            .iter()
            .any(|table| table.keys().any(|key| normalized(key) == library));
        return (!shadowed).then(|| vec![library]);
    }
    let manifests = ancestor_manifests(&test_chain)?;
    if manifests
        .iter()
        .any(|(_, manifest)| mentions.iter().any(|name| patches(manifest, name)))
    {
        return None;
    }
    let workspace = manifests.iter().find_map(|(directory, manifest)| {
        manifest
            .get("workspace")
            .and_then(toml::Value::as_table)
            .map(|workspace| (directory.as_path(), workspace))
    });
    let mut names = Vec::new();
    // Crate names some entry binds to another package. Target tables are
    // read without their predicates, so a name bound to the owner on one
    // target and elsewhere on another is ambiguous and admitted nowhere.
    let mut elsewhere = Vec::new();
    for table in test_dependency_tables(&test.manifest) {
        for (key, spec) in table {
            match import_name(
                key,
                spec,
                &test.directory,
                workspace,
                &owner,
                owner_name,
                &library,
            ) {
                Some(name) => names.push(name),
                None => elsewhere.push(normalized(key)),
            }
        }
    }
    names.retain(|name| !elsewhere.contains(name));
    names.sort();
    names.dedup();
    Some(names)
}

/// The import name a dependency entry gives the owner's library, when the
/// entry is a path dependency on the owner's package.
fn import_name(
    key: &str,
    spec: &toml::Value,
    test_directory: &Path,
    workspace: Option<(&Path, &toml::Table)>,
    owner: &Package,
    owner_name: &str,
    library: &str,
) -> Option<String> {
    let spec = spec.as_table()?;
    let (source, base) = if spec.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
        // A `package` rename on the inheriting side is not allowed by Cargo;
        // fail closed if one appears.
        if spec.contains_key("package") || spec.contains_key("path") {
            return None;
        }
        let (directory, workspace) = workspace?;
        let inherited = workspace
            .get("dependencies")
            .and_then(toml::Value::as_table)?
            .get(key)?
            .as_table()?;
        (inherited, directory)
    } else {
        (spec, test_directory)
    };
    let path = source.get("path")?.as_str()?;
    if source.contains_key("git") || source.contains_key("registry") {
        return None;
    }
    if lexical(&base.join(path)) != owner.directory {
        return None;
    }
    match source.get("package").map(toml::Value::as_str) {
        Some(Some(package)) if normalized(package) == normalized(owner_name) => {
            Some(normalized(key))
        }
        Some(_) => None,
        None if normalized(key) == normalized(owner_name) => Some(library.to_string()),
        None => None,
    }
}

/// The nearest package manifest at or above `directory` inside `root`, plus
/// the directories from `directory` up to the root. A nearer manifest
/// without `[package]`, an unreadable or unparsable one, or one naming
/// `package.workspace` answers nothing.
fn nearest_package(root: &Path, directory: &Path) -> Option<(Package, Vec<PathBuf>)> {
    let chain: Vec<PathBuf> = directory
        .ancestors()
        .take_while(|candidate| candidate.starts_with(root))
        .map(Path::to_path_buf)
        .collect();
    for candidate in &chain {
        match std::fs::read_to_string(candidate.join("Cargo.toml")) {
            Ok(text) => {
                let manifest = text.parse::<toml::Table>().ok()?;
                let package = manifest.get("package")?;
                if package.get("workspace").is_some() {
                    return None;
                }
                return Some((
                    Package {
                        directory: candidate.clone(),
                        manifest,
                    },
                    chain,
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    None
}

/// Every manifest from the first directory of `chain` up to the root.
fn ancestor_manifests(chain: &[PathBuf]) -> Option<Vec<(PathBuf, toml::Table)>> {
    let mut manifests = Vec::new();
    for directory in chain {
        match std::fs::read_to_string(directory.join("Cargo.toml")) {
            Ok(text) => manifests.push((directory.clone(), text.parse::<toml::Table>().ok()?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    Some(manifests)
}

/// Whether `owner_root` is the package's library target: the default
/// `src/lib.rs`, with no `[lib] path` moving the target and `autolib` on. A
/// module of `src/main.rs` or of a `src/bin/` target sits under `src/` too,
/// but no other crate can import it.
fn is_default_library_root(package: &Package, owner_root: &Path) -> bool {
    let manifest = &package.manifest;
    // An explicit `[lib] path` that still names the default root is not a
    // move (#7097): fixtures and real manifests spell `src/lib.rs` out.
    // Anything else — or an unreadable value — keeps refusing.
    let moved = manifest
        .get("lib")
        .and_then(|lib| lib.get("path"))
        .is_some_and(|path| {
            path.as_str()
                .is_none_or(|text| lexical(Path::new(text)) != Path::new("src").join("lib.rs"))
        });
    let autolib_off = manifest
        .get("package")
        .and_then(|package| package.get("autolib"))
        .and_then(toml::Value::as_bool)
        == Some(false);
    !moved && !autolib_off && owner_root == package.directory.join("src").join("lib.rs")
}

fn test_dependency_tables(manifest: &toml::Table) -> Vec<&toml::Table> {
    let direct = TEST_DEPENDENCY_TABLES
        .iter()
        .filter_map(|name| manifest.get(*name).and_then(toml::Value::as_table));
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(toml::Value::as_table)
        .flat_map(|target| {
            TEST_DEPENDENCY_TABLES
                .iter()
                .filter_map(|name| target.get(*name).and_then(toml::Value::as_table))
        });
    direct.chain(targets).collect()
}

/// `path` with `.` dropped and `..` folded into its parent, without touching
/// the file system.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::drop_in::temp_workspace;
    use super::*;

    const ROOT: &str = "[workspace]\nmembers = [\"pricing\", \"orders\"]\n";
    const PRICING: &str = "[package]\nname = \"pricing\"\nversion = \"0.1.0\"\n";

    fn names(files: &[(&str, &str)], test: &str, owner: &str) -> Result<Vec<String>, String> {
        let root = temp_workspace("member-crates", files)?;
        let names = MemberCrates::new(&root).import_names(Path::new(test), Path::new(owner));
        let _ = std::fs::remove_dir_all(&root);
        Ok(names)
    }

    fn orders(dependencies: &str) -> String {
        format!("[package]\nname = \"orders\"\nversion = \"0.1.0\"\n\n{dependencies}")
    }

    fn default_root(manifest: &str, crate_root: &str) -> Result<bool, String> {
        let root = temp_workspace("default-root", &[("pricing/Cargo.toml", manifest)])?;
        let default = MemberCrates::new(&root).is_default_target_root(Path::new(crate_root));
        let _ = std::fs::remove_dir_all(&root);
        Ok(default)
    }

    #[test]
    fn unit_target_discovery_flags_do_not_change_shared_root_authority() -> Result<(), String> {
        for (flags, lib, main, shared) in [
            ("", true, true, true),
            ("autolib = false\n", false, true, false),
            ("autobins = false\n", true, false, false),
            ("autolib = false\nautobins = false\n", false, false, false),
            ("autolib = true\nautobins = true\n", true, true, true),
            ("autolib = \"bad\"\n", false, false, false),
            ("autobins = \"bad\"\n", false, false, false),
            ("[lib]\npath = \"src/real.rs\"\n", false, false, false),
            (
                "[[test]]\nname = \"t\"\npath = \"src/lib.rs\"\n",
                false,
                false,
                false,
            ),
        ] {
            let manifest = format!("{PRICING}{flags}");
            let root = temp_workspace("unit-target-root", &[("pricing/Cargo.toml", &manifest)])?;
            let authority = MemberCrates::new(&root);
            for (path, expected) in [("pricing/src/lib.rs", lib), ("pricing/src/main.rs", main)] {
                assert_eq!(
                    authority.is_default_unit_target_root(Path::new(path)),
                    expected,
                    "{path}: {flags}"
                );
                assert_eq!(
                    authority.is_default_target_root(Path::new(path)),
                    shared,
                    "shared {path}: {flags}"
                );
            }
            assert!(!authority.is_default_unit_target_root(Path::new("pricing/src/other.rs")));
            assert!(!authority.is_default_unit_target_root(Path::new("pricing/tests/lib.rs")));
            let _ = std::fs::remove_dir_all(&root);
        }
        assert!(!MemberCrates::default().is_default_unit_target_root(Path::new("src/lib.rs")));
        Ok(())
    }

    #[test]
    fn only_an_unmoved_target_root_is_a_default_root() -> Result<(), String> {
        // #7686 review: `[lib] path = "src/real.rs"` makes `src/lib.rs` a
        // module whose `crate::` is another file's.
        let lib = "pricing/src/lib.rs";
        assert!(default_root(PRICING, lib)?);
        assert!(default_root(PRICING, "pricing/src/main.rs")?);
        assert!(default_root(
            &format!("{PRICING}[lib]\npath = \"src/lib.rs\"\n"),
            lib
        )?);
        assert!(default_root(
            &format!("{PRICING}[[bin]]\nname = \"p\"\npath = \"src/main.rs\"\n"),
            lib
        )?);
        for moved in [
            "[lib]\npath = \"src/real.rs\"\n",
            "[lib]\npath = 5\n",
            "[[bin]]\nname = \"p\"\npath = \"src/other.rs\"\n",
            "[[example]]\nname = \"e\"\npath = \"src/lib.rs\"\n",
        ] {
            assert!(!default_root(&format!("{PRICING}{moved}"), lib)?, "{moved}");
        }
        for flag in ["autolib = false\n", "autobins = false\n"] {
            assert!(!default_root(&format!("{PRICING}{flag}"), lib)?, "{flag}");
        }
        // No manifest at the package directory, or no manifest authority.
        assert!(!default_root(PRICING, "other/src/lib.rs")?);
        assert!(!MemberCrates::default().is_default_target_root(Path::new(lib)));
        Ok(())
    }

    #[test]
    fn a_path_dependency_names_the_owner_library() -> Result<(), String> {
        let plain = orders("[dependencies]\npricing = { path = \"../pricing\" }\n");
        let files = [
            ("Cargo.toml", ROOT),
            ("pricing/Cargo.toml", PRICING),
            ("orders/Cargo.toml", plain.as_str()),
        ];
        assert_eq!(
            names(&files, "orders/tests/t.rs", "pricing/src/lib.rs")?,
            ["pricing"]
        );
        assert_eq!(
            names(&files, "./orders/tests/t.rs", "./pricing/src/lib.rs")?,
            ["pricing"]
        );
        let dev = orders("[dev-dependencies]\npricing = { path = \"../pricing\" }\n");
        let files = [
            ("Cargo.toml", ROOT),
            ("pricing/Cargo.toml", PRICING),
            ("orders/Cargo.toml", dev.as_str()),
        ];
        assert_eq!(
            names(&files, "orders/tests/t.rs", "pricing/src/lib.rs")?,
            ["pricing"]
        );
        Ok(())
    }

    #[test]
    fn renames_and_library_names_follow_cargo() -> Result<(), String> {
        let renamed =
            orders("[dependencies]\nprice = { package = \"pricing\", path = \"../pricing\" }\n");
        let files = [
            ("Cargo.toml", ROOT),
            ("pricing/Cargo.toml", PRICING),
            ("orders/Cargo.toml", renamed.as_str()),
        ];
        assert_eq!(
            names(&files, "orders/tests/t.rs", "pricing/src/lib.rs")?,
            ["price"]
        );
        let lib =
            "[package]\nname = \"pricing-core\"\nversion = \"0.1.0\"\n[lib]\nname = \"pcore\"\n";
        let keyed = orders("[dependencies]\npricing-core = { path = \"../pricing\" }\n");
        let files = [
            ("Cargo.toml", ROOT),
            ("pricing/Cargo.toml", lib),
            ("orders/Cargo.toml", keyed.as_str()),
        ];
        assert_eq!(
            names(&files, "orders/tests/t.rs", "pricing/src/lib.rs")?,
            ["pcore"]
        );
        Ok(())
    }

    #[test]
    fn workspace_inherited_path_dependency_names_the_owner() -> Result<(), String> {
        let root = "[workspace]\nmembers = [\"pricing\", \"orders\"]\n[workspace.dependencies]\npricing = { path = \"pricing\" }\n";
        let inherits = orders("[dependencies]\npricing = { workspace = true }\n");
        let files = [
            ("Cargo.toml", root),
            ("pricing/Cargo.toml", PRICING),
            ("orders/Cargo.toml", inherits.as_str()),
        ];
        assert_eq!(
            names(&files, "orders/tests/t.rs", "pricing/src/lib.rs")?,
            ["pricing"]
        );
        Ok(())
    }

    #[test]
    fn the_owners_own_package_names_its_library() -> Result<(), String> {
        let files = [
            ("Cargo.toml", ROOT),
            ("pricing/Cargo.toml", PRICING),
            ("orders/Cargo.toml", "[package]\nname = \"orders\"\n"),
        ];
        assert_eq!(
            names(&files, "pricing/tests/t.rs", "pricing/src/lib.rs")?,
            ["pricing"]
        );
        // A dev-dependency keyed by the library's own name shadows it.
        let shadowing =
            format!("{PRICING}[dev-dependencies]\npricing = {{ path = \"../fake\" }}\n");
        let files = [
            ("Cargo.toml", ROOT),
            ("pricing/Cargo.toml", shadowing.as_str()),
            ("orders/Cargo.toml", "[package]\nname = \"orders\"\n"),
        ];
        assert_eq!(
            names(&files, "pricing/tests/t.rs", "pricing/src/lib.rs")?,
            Vec::<String>::new()
        );
        Ok(())
    }

    #[test]
    fn an_explicit_default_lib_path_still_names_the_library() -> Result<(), String> {
        // #7097: `[lib] path = "src/lib.rs"` spells the default root out —
        // with or without an explicit `[lib] name` — so the owner's own
        // package still names its library. A genuinely moved root
        // (`src/core.rs`, pinned by "moved library" below) and an
        // unreadable value keep naming nothing.
        for manifest in [
            format!("{PRICING}[lib]\npath = \"src/lib.rs\"\n"),
            format!("{PRICING}[lib]\nname = \"pricing\"\npath = \"src/lib.rs\"\n"),
        ] {
            let files = [
                ("Cargo.toml", ROOT),
                ("pricing/Cargo.toml", manifest.as_str()),
                ("orders/Cargo.toml", "[package]\nname = \"orders\"\n"),
            ];
            assert_eq!(
                names(&files, "pricing/tests/t.rs", "pricing/src/lib.rs")?,
                ["pricing"]
            );
        }
        let unreadable = format!("{PRICING}[lib]\npath = 5\n");
        let files = [
            ("Cargo.toml", ROOT),
            ("pricing/Cargo.toml", unreadable.as_str()),
            ("orders/Cargo.toml", "[package]\nname = \"orders\"\n"),
        ];
        assert_eq!(
            names(&files, "pricing/tests/t.rs", "pricing/src/lib.rs")?,
            Vec::<String>::new()
        );
        Ok(())
    }

    #[test]
    fn anything_else_names_nothing() -> Result<(), String> {
        let path = "[dependencies]\npricing = { path = \"../pricing\" }\n";
        let plain = orders(path);
        type Case<'a> = (String, Vec<(&'a str, String)>, &'a str, &'a str);
        let cases: Vec<Case> = vec![
            // No dependency on the owner's package.
            (
                "no dependency".into(),
                vec![("orders/Cargo.toml", orders(""))],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            // A registry dependency of the same name is another package.
            (
                "registry".into(),
                vec![(
                    "orders/Cargo.toml",
                    orders("[dependencies]\npricing = \"1\"\n"),
                )],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            // A path to some other directory.
            (
                "other path".into(),
                vec![(
                    "orders/Cargo.toml",
                    orders("[dependencies]\npricing = { path = \"../vendor/pricing\" }\n"),
                )],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            // A `package` rename that names another package.
            (
                "wrong package".into(),
                vec![(
                    "orders/Cargo.toml",
                    orders(
                        "[dependencies]\npricing = { package = \"other\", path = \"../pricing\" }\n",
                    ),
                )],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            // A key that is not the package name, without `package`.
            (
                "wrong key".into(),
                vec![(
                    "orders/Cargo.toml",
                    orders("[dependencies]\nprice = { path = \"../pricing\" }\n"),
                )],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            // One key bound to the owner on one target and to another
            // package on another.
            (
                "conflicting targets".into(),
                vec![(
                    "orders/Cargo.toml",
                    orders(
                        "[target.'cfg(windows)'.dependencies]\npricing = { path = \"../pricing\" }\n[target.'cfg(unix)'.dependencies]\npricing = { package = \"fake-pricing\", path = \"../fake\" }\n",
                    ),
                )],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            // Build dependencies are not visible to tests.
            (
                "build dependency".into(),
                vec![(
                    "orders/Cargo.toml",
                    orders("[build-dependencies]\npricing = { path = \"../pricing\" }\n"),
                )],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            // The owner is in a binary target, which no crate can import.
            (
                "binary owner".into(),
                vec![("orders/Cargo.toml", plain.clone())],
                "orders/tests/t.rs",
                "pricing/src/main.rs",
            ),
            (
                "bin dir owner".into(),
                vec![("orders/Cargo.toml", plain.clone())],
                "orders/tests/t.rs",
                "pricing/src/bin/tool.rs",
            ),
            // A module under `src/` that composes under another root.
            (
                "module of the binary".into(),
                vec![("orders/Cargo.toml", plain.clone())],
                "orders/tests/t.rs",
                "pricing/src/cli.rs",
            ),
            // A manifest that moves or disables the default library.
            (
                "moved library".into(),
                vec![
                    ("orders/Cargo.toml", plain.clone()),
                    (
                        "pricing/Cargo.toml",
                        format!("{PRICING}[lib]\npath = \"src/core.rs\"\n"),
                    ),
                ],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            (
                "autolib off".into(),
                vec![
                    ("orders/Cargo.toml", plain.clone()),
                    (
                        "pricing/Cargo.toml",
                        PRICING.replace("version", "autolib = false\nversion"),
                    ),
                ],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            (
                "test owner".into(),
                vec![("orders/Cargo.toml", plain.clone())],
                "orders/tests/t.rs",
                "pricing/tests/t.rs",
            ),
            // A patch or config can substitute the package.
            (
                "patch".into(),
                vec![(
                    "orders/Cargo.toml",
                    format!("{plain}[patch.crates-io]\npricing = {{ path = \"../x\" }}\n"),
                )],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            (
                "config".into(),
                vec![
                    ("orders/Cargo.toml", plain.clone()),
                    (".cargo/config.toml", "[paths]\n".into()),
                ],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
            // `package.workspace` points at a workspace ripr did not read.
            (
                "package.workspace".into(),
                vec![(
                    "orders/Cargo.toml",
                    format!("[package]\nname = \"orders\"\nworkspace = \"..\"\n\n{path}"),
                )],
                "orders/tests/t.rs",
                "pricing/src/lib.rs",
            ),
        ];
        for (label, extra, test, owner) in cases {
            let mut files: Vec<(&str, &str)> =
                vec![("Cargo.toml", ROOT), ("pricing/Cargo.toml", PRICING)];
            files.extend(extra.iter().map(|(path, text)| (*path, text.as_str())));
            assert_eq!(names(&files, test, owner)?, Vec::<String>::new(), "{label}");
        }
        Ok(())
    }
}
