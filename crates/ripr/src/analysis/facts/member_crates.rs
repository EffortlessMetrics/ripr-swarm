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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::drop_in::{config_mentions, normalized, patches};

/// Dependency tables whose crates a test target can import. Build
/// dependencies reach only the build script.
const TEST_DEPENDENCY_TABLES: &[&str] = &["dependencies", "dev-dependencies", "dev_dependencies"];

/// Import names per (test file directory, owner crate root).
type ImportNames = BTreeMap<(PathBuf, PathBuf), Vec<String>>;
/// Foreign-export answers per (owner crate root, item name).
type ForeignExports = BTreeMap<(PathBuf, String), bool>;
/// Selected library root and its witnessed literal macro-export lines.
type LibraryMacroExports = (PathBuf, BTreeMap<String, usize>);
/// Negative-only export witnesses per (consumer directory, imported crate).
type ImportedMacroExports = BTreeMap<(PathBuf, String), Option<Arc<LibraryMacroExports>>>;

/// Per-analysis cache of member-crate import names, keyed by the test file's
/// directory and the owner's crate root. Clones share the cache.
#[derive(Clone, Debug, Default)]
pub(crate) struct MemberCrates {
    root: Option<PathBuf>,
    names: Arc<Mutex<ImportNames>>,
    /// Per (crate root, item name): whether the library may export a
    /// foreign item of that name, computed by the classifier.
    foreign_exports: Arc<Mutex<ForeignExports>>,
    /// All possible library target roots in the selected manifest epoch.
    /// Missing inventory authority never establishes binary-only ownership.
    library_roots: Arc<OnceLock<Option<BTreeSet<PathBuf>>>>,
    /// Kept for this analysis only. A missing entry or macro is not an
    /// absence certificate and must never clear a broader ambiguity.
    imported_macro_exports: Arc<Mutex<ImportedMacroExports>>,
}

impl MemberCrates {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: Some(root.to_path_buf()),
            names: Arc::default(),
            foreign_exports: Arc::default(),
            library_roots: Arc::default(),
            imported_macro_exports: Arc::default(),
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
        let names = resolve(&root, test_dir, owner_dir, &owner_root, false).unwrap_or_default();
        if let Ok(mut cache) = self.names.lock() {
            cache.insert(key, names.clone());
        }
        names
    }

    /// A selected local library that this consumer imports as `krate`, with
    /// a literal unconditional root export of `name` (#11810). This supplies
    /// only a refusal witness; `None` never certifies that an import is safe.
    /// Registry/git crates, unknown placement and incomplete inventories are
    /// outside this bounded query. Source-role/discovery selection is unchanged.
    pub(crate) fn imported_literal_macro_export(
        &self,
        consumer: &Path,
        krate: &str,
        name: &str,
    ) -> Option<(PathBuf, usize)> {
        let read_root = self.root.as_deref()?;
        let root = snapshot_root(read_root)?;
        let consumer = lexical(&root.join(consumer));
        let directory = consumer.parent()?;
        let key = (
            directory.to_path_buf(),
            krate.trim_start_matches("r#").to_string(),
        );
        let cached = self
            .imported_macro_exports
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key).cloned());
        let exports = match cached {
            Some(exports) => exports,
            None => {
                let exports = self
                    .local_library_macro_exports(read_root, &root, directory, &key.1)
                    .map(Arc::new);
                if let Ok(mut cache) = self.imported_macro_exports.lock() {
                    cache.insert(key, exports.clone());
                }
                exports
            }
        }?;
        exports
            .1
            .get(name.trim_start_matches("r#"))
            .map(|line| (exports.0.clone(), *line))
    }

    fn local_library_macro_exports(
        &self,
        read_root: &Path,
        root: &Path,
        consumer_directory: &Path,
        krate: &str,
    ) -> Option<LibraryMacroExports> {
        let listed_root = lexical(read_root);
        let libraries = self
            .library_roots
            .get_or_init(|| selected_library_roots(read_root))
            .as_ref()?;
        let mut selected = None;
        for library in libraries {
            crate::analysis::cancellation::checkpoint().ok()?;
            // The shared negative inventory includes possible library roots
            // outside this workspace. They cannot be a local export witness;
            // an unrelated external declaration must not hide a bound local one.
            let Ok(relative) = library.strip_prefix(&listed_root) else {
                continue;
            };
            let absolute = lexical(&root.join(relative));
            let owner_directory = absolute.parent()?;
            // Unlike import_names' worktree convenience query, this uses the
            // selected manifest epoch, including configuration substitutions.
            if resolve(root, consumer_directory, owner_directory, &absolute, true)
                .is_some_and(|names| names.iter().any(|name| name == krate))
            {
                if selected.is_some() {
                    return None;
                }
                selected = Some(relative.to_path_buf());
            }
        }
        let relative = selected?;
        let bytes =
            crate::analysis::committed_source::read_source_bytes(root, &relative).ok()??;
        // Bound the auxiliary syntax query, not a dependency source index.
        // Large or unsupported libraries contribute no affirmative witness.
        if bytes.len() > 256 * 1024 {
            return None;
        }
        let source = std::str::from_utf8(&bytes).ok()?;
        let exports = crate::analysis::syntax::literal_root_macro_exports(source);
        Some((relative, exports))
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

    /// Whether `crate_root`, a root-relative file, is the library root its
    /// package's manifest names through `[lib] path` (RIPR-SPEC-0171 example
    /// 5). Default `src/lib.rs` roots stay with
    /// [`Self::is_default_target_root`]; this admits only a root the manifest
    /// moved there. Without manifest authority the answer is no.
    pub(crate) fn is_custom_lib_path_root(&self, crate_root: &Path) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let root = lexical(root);
        let crate_root = lexical(&root.join(crate_root));
        let Some(directory) = crate_root.parent() else {
            return false;
        };
        let Some((package, _)) = nearest_package(&root, directory) else {
            return false;
        };
        package
            .manifest
            .get("lib")
            .is_some_and(|lib| lib.get("path").is_some())
            && is_library_root(&package, &crate_root)
    }

    /// An established binary root cannot export macros to another crate,
    /// even when an opaque invocation emits `#[macro_export]`. Read the
    /// selected source epoch, and refuse unknown or overlapping library
    /// declarations instead of inferring a crate kind from a directory name.
    pub(crate) fn is_binary_target_root(&self, crate_root: &Path) -> bool {
        let Some(read_root) = &self.root else {
            return false;
        };
        let root = lexical(read_root);
        let file = lexical(&root.join(crate_root));
        let Some((directory, manifest)) = file
            .parent()
            .and_then(|parent| committed_package(read_root, &root, parent))
        else {
            return false;
        };
        let Some(package) = manifest.get("package").and_then(toml::Value::as_table) else {
            return false;
        };
        if package.contains_key("workspace") {
            return false;
        }
        // Cargo permits a sibling package's [lib] path to name this physical
        // source too. Nearest/ancestor manifests alone cannot certify that
        // every interpretation is a non-importable binary.
        if self
            .library_roots
            .get_or_init(|| selected_library_roots(read_root))
            .as_ref()
            .is_none_or(|roots| roots.contains(&file))
        {
            return false;
        }
        // Cargo accepts `./parse.rs`; a `..` or absolute path stays refused.
        let relative_path = |text: &str| {
            let path = Path::new(text);
            let mut joined = directory.to_path_buf();
            for part in path.components() {
                match part {
                    Component::Normal(name) => joined.push(name),
                    Component::CurDir => {}
                    _ => return None,
                }
            }
            (joined != directory).then_some(joined)
        };
        let explicit = manifest
            .get("bin")
            .and_then(toml::Value::as_array)
            .is_some_and(|bins| {
                bins.iter().any(|bin| {
                    bin.get("path")
                        .and_then(toml::Value::as_str)
                        .and_then(relative_path)
                        .is_some_and(|path| path == file)
                })
            });
        let automatic = package
            .get("autobins")
            .is_none_or(|value| value.as_bool() == Some(true))
            && (file == directory.join("src/main.rs")
                || (file.parent() == Some(directory.join("src/bin").as_path())
                    && file.extension().is_some_and(|extension| extension == "rs")));
        explicit || automatic
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

    /// A direct automatic `src/bin/<name>.rs` root. Reuse the default
    /// binary's manifest admission, including its conservative moved-target
    /// refusals. Explicit targets with autodiscovery disabled stay unresolved.
    pub(crate) fn is_default_binary_root(&self, crate_root: &Path) -> bool {
        let Some(bin) = crate_root.parent().filter(|dir| dir.ends_with("src/bin")) else {
            return false;
        };
        let Some(src) = bin.parent() else {
            return false;
        };
        if crate_root.extension().is_none_or(|ext| ext != "rs")
            || !self.default_target_root_with_flags_snapshot(
                &src.join("main.rs"),
                &["autobins"],
                true,
            )
        {
            return false;
        }
        let (Some(root), Some(package_dir)) = (&self.root, src.parent()) else {
            return false;
        };
        let Some(root) = snapshot_root(root) else {
            return false;
        };
        let Some((package, _)) =
            nearest_package_snapshot(&root, &lexical(&root.join(package_dir)), true)
        else {
            return false;
        };
        let metadata = package.manifest.get("package");
        if let Some(enabled) = metadata.and_then(|package| package.get("autobins")) {
            return enabled.as_bool() == Some(true);
        }
        // Cargo's 2015 default disables discovery when *any* target is
        // manually declared. Unknown/inherited editions need an explicit flag.
        match metadata.and_then(|package| package.get("edition")) {
            Some(edition) if matches!(edition.as_str(), Some("2018" | "2021" | "2024")) => true,
            None => !["lib", "bin", "example", "test", "bench"]
                .iter()
                .any(|kind| package.manifest.get(*kind).is_some()),
            Some(edition) if edition.as_str() == Some("2015") => {
                !["lib", "bin", "example", "test", "bench"]
                    .iter()
                    .any(|kind| package.manifest.get(*kind).is_some())
            }
            Some(_) => false,
        }
    }

    fn default_target_root_with_flags(&self, crate_root: &Path, required_flags: &[&str]) -> bool {
        self.default_target_root_with_flags_snapshot(crate_root, required_flags, false)
    }

    fn default_target_root_with_flags_snapshot(
        &self,
        crate_root: &Path,
        required_flags: &[&str],
        snapshot: bool,
    ) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let root = if snapshot {
            let Some(root) = snapshot_root(root) else {
                return false;
            };
            root
        } else {
            lexical(root)
        };
        let crate_root = lexical(&root.join(crate_root));
        let Some(directory) = crate_root.parent().and_then(Path::parent) else {
            return false;
        };
        let Some((package, _)) = nearest_package_snapshot(&root, directory, snapshot) else {
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

    /// Require the binary's manifest to bind the imported name to the indexed
    /// proc-macro library, rather than a registry or namesake dependency.
    pub(crate) fn binary_imports_macro(&self, file: &Path, macro_file: &Path, name: &str) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let Some(root) = snapshot_root(root) else {
            return false;
        };
        let file = lexical(&root.join(file));
        let macro_absolute = lexical(&root.join(macro_file));
        let (Some(test_dir), Some(owner_dir)) = (file.parent(), macro_absolute.parent()) else {
            return false;
        };
        let Some((owner, _)) = nearest_package_snapshot(&root, owner_dir, true) else {
            return false;
        };
        if owner
            .manifest
            .get("lib")
            .and_then(|lib| lib.get("proc-macro").or_else(|| lib.get("proc_macro")))
            .and_then(toml::Value::as_bool)
            != Some(true)
        {
            return false;
        }
        resolve(&root, test_dir, owner_dir, &macro_absolute, true).is_some_and(|names| {
            names
                .iter()
                .any(|bound| bound == name.trim_start_matches("r#"))
        })
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

/// A negative inventory: extra roots only retain the conservative refusal.
/// An absent overlay entry is not a manifest in this source epoch; an unread
/// or malformed manifest leaves the inventory unknown. Library paths may
/// leave their declaring package, unlike the binary paths certified above.
fn selected_library_roots(read_root: &Path) -> Option<BTreeSet<PathBuf>> {
    let root = lexical(read_root);
    let mut roots = BTreeSet::new();
    for directory in crate::analysis::workspace::target_manifest_directories(read_root)? {
        let relative = directory.strip_prefix(&root).ok()?.join("Cargo.toml");
        let Some(bytes) =
            crate::analysis::committed_source::read_source_bytes(read_root, &relative).ok()?
        else {
            continue;
        };
        let manifest = std::str::from_utf8(&bytes)
            .ok()?
            .parse::<toml::Table>()
            .ok()?;
        if !manifest.contains_key("package") && !manifest.contains_key("project") {
            continue;
        }
        let path = match manifest.get("lib") {
            None => "src/lib.rs",
            Some(lib) => match lib.as_table()?.get("path") {
                None => "src/lib.rs",
                Some(path) => path.as_str()?,
            },
        };
        roots.insert(lexical(&directory.join(path)));
    }
    Some(roots)
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
        let Some(package) = self.default_library_package(crate_root) else {
            return false;
        };
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

impl MemberCrates {
    /// The package whose default library root is `crate_root`, a
    /// root-relative `src/lib.rs`, or `None` without manifest authority, for
    /// a moved or non-default root, or for any manifest value it cannot read.
    fn default_library_package(&self, crate_root: &Path) -> Option<Package> {
        let root = self.root.as_ref()?;
        if !crate_root.ends_with(Path::new("src").join("lib.rs"))
            || !self.is_default_target_root(crate_root)
        {
            return None;
        }
        let root = lexical(root);
        let crate_root = lexical(&root.join(crate_root));
        let directory = crate_root.parent().and_then(Path::parent)?;
        let (package, _) = nearest_package(&root, directory)?;
        if package.directory != directory || !is_default_library_root(&package, &crate_root) {
            return None;
        }
        Some(package)
    }

    /// The cargo features a plain `cargo test` of the library rooted at
    /// `crate_root` enables: the closure of the manifest's `[features]
    /// default = [..]` over plain feature names (RIPR-SPEC-0279, #12230).
    /// A `dep:name` or `name/feature` entry names a dependency, not a
    /// feature of this package, so it adds nothing; leaving it out can only
    /// shrink the set. `None` when the package cannot be read, or its
    /// `default` or a followed feature entry is not an array of strings:
    /// the caller then resolves no feature gate.
    pub(crate) fn default_library_features(
        &self,
        crate_root: &Path,
    ) -> Option<std::collections::BTreeSet<String>> {
        let package = self.default_library_package(crate_root)?;
        let Some(table) = package.manifest.get("features") else {
            return Some(std::collections::BTreeSet::new());
        };
        let table = table.as_table()?;
        // `visited` bounds the walk; `enabled` is what the walk credits.
        // They differ only by the distinguished `default` name, which is
        // walked but never credited as a feature, so a list that names
        // `default` (directly or through a cycle) still terminates.
        let mut enabled = std::collections::BTreeSet::new();
        let mut visited = std::collections::BTreeSet::new();
        let mut pending = vec!["default".to_string()];
        while let Some(name) = pending.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            if name != "default" {
                enabled.insert(name.clone());
            }
            // A name with no `[features]` entry is an optional dependency's
            // implicit feature; it is on when listed and walks no further.
            let Some(entry) = table.get(&name) else {
                continue;
            };
            for member in entry.as_array()? {
                let member = member.as_str()?;
                if member.starts_with("dep:") || member.contains('/') {
                    continue;
                }
                if !visited.contains(member) {
                    pending.push(member.to_string());
                }
            }
        }
        Some(enabled)
    }
}

impl MemberCrates {
    /// Whether `file` (root-relative) is a module of the library of a
    /// proc-macro package (`[lib] proc-macro = true`). Tests reach such code
    /// only through derive/attribute expansion, which ripr does not follow.
    ///
    /// Only library code qualifies: the file must sit under the library's
    /// source directory (`src/`, or the parent of `[lib] path`) and must not
    /// be a build script, `src/main.rs`, `src/bin/**`, or the file or
    /// directory of any explicit `[[bin]]`/`[[example]]`/`[[test]]`/
    /// `[[bench]]` target; `tests/`, `benches/` and `examples/` fall outside
    /// the library directory. The manifest is read through the committed
    /// source overlay, so a dirty `Cargo.toml` cannot change a committed-
    /// history finding. A missing, unreadable or virtual manifest, or
    /// absent manifest authority, answers no (fail closed).
    pub(crate) fn is_proc_macro_library_file(&self, file: &Path) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let root = lexical(root);
        let file_abs = lexical(&root.join(file));
        let Some(directory) = file_abs.parent() else {
            return false;
        };
        let Some((package, manifest)) =
            committed_package(self.root.as_deref().unwrap_or(&root), &root, directory)
        else {
            return false;
        };
        let lib = manifest.get("lib");
        let proc_macro = lib
            .and_then(|lib| lib.get("proc-macro").or_else(|| lib.get("proc_macro")))
            .and_then(toml::Value::as_bool)
            == Some(true);
        if !proc_macro {
            return false;
        }
        let lib_dir = lib
            .and_then(|lib| lib.get("path"))
            .and_then(toml::Value::as_str)
            .map_or_else(
                || package.join("src"),
                |path| {
                    lexical(&package.join(path))
                        .parent()
                        .map_or_else(|| package.join("src"), Path::to_path_buf)
                },
            );
        if !file_abs.starts_with(&lib_dir) {
            return false;
        }
        let relative = |path: &str| lexical(&package.join(path));
        // `[lib] path` may sit in the package root, making the library
        // directory an ancestor of these target directories.
        if file_abs == relative("src/main.rs")
            || file_abs.starts_with(relative("src/bin"))
            || file_abs == relative("build.rs")
            || ["tests", "benches", "examples", "target"]
                .iter()
                .any(|dir| file_abs.starts_with(relative(dir)))
        {
            return false;
        }
        if let Some(build) = manifest
            .get("package")
            .and_then(|package| package.get("build"))
            .and_then(toml::Value::as_str)
            && file_abs == relative(build)
        {
            return false;
        }
        for kind in ["bin", "example", "test", "bench"] {
            let Some(targets) = manifest.get(kind).and_then(toml::Value::as_array) else {
                continue;
            };
            for target in targets {
                let Some(path) = target.get("path").and_then(toml::Value::as_str) else {
                    continue;
                };
                let target_path = relative(path);
                let target_dir = target_path.parent().map(Path::to_path_buf);
                if file_abs == target_path
                    || target_dir
                        .as_ref()
                        .is_some_and(|dir| *dir != lib_dir && file_abs.starts_with(dir))
                {
                    return false;
                }
            }
        }
        true
    }

    /// Whether `file` (root-relative) is the entry file of an explicit
    /// `[[bin]]`, `[[example]]`, `[[test]]` or `[[bench]]` target whose
    /// `path` the nearest package manifest names (#11123). Such a file roots
    /// a crate of its own, so a module tree rooted there is not part of any
    /// other target's crate. It says nothing about a file the manifest does
    /// not name: an undeclared orphan stays unplaced. The manifest is read
    /// through the committed-source overlay; a missing, unreadable or
    /// virtual manifest, or absent manifest authority, answers no.
    pub(crate) fn is_declared_target_entry(&self, file: &Path) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let root = lexical(root);
        let file_abs = lexical(&root.join(file));
        let Some(directory) = file_abs.parent() else {
            return false;
        };
        let Some((package, manifest)) =
            committed_package(self.root.as_deref().unwrap_or(&root), &root, directory)
        else {
            return false;
        };
        ["bin", "example", "test", "bench"].iter().any(|kind| {
            manifest
                .get(*kind)
                .and_then(toml::Value::as_array)
                .is_some_and(|targets| {
                    targets.iter().any(|target| {
                        target
                            .get("path")
                            .and_then(toml::Value::as_str)
                            .is_some_and(|path| lexical(&package.join(path)) == file_abs)
                    })
                })
        })
    }
}

/// The nearest package manifest at or above `directory`, read through the
/// committed-source overlay. `None` for a virtual workspace, an unreadable
/// or unparsable manifest, or no manifest below `root`.
fn committed_package(
    read_root: &Path,
    root: &Path,
    directory: &Path,
) -> Option<(PathBuf, toml::Table)> {
    for candidate in directory
        .ancestors()
        .take_while(|candidate| candidate.starts_with(root))
    {
        let relative = candidate.strip_prefix(root).map_or_else(
            |_| PathBuf::from("Cargo.toml"),
            |rel| rel.join("Cargo.toml"),
        );
        match crate::analysis::committed_source::read_cargo_manifest(read_root, &relative) {
            Ok(Some(bytes)) => {
                let manifest = String::from_utf8(bytes).ok()?.parse::<toml::Table>().ok()?;
                manifest.get("package")?;
                return Some((candidate.to_path_buf(), manifest));
            }
            Ok(None) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    None
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
    snapshot: bool,
) -> Option<Vec<String>> {
    let (owner, owner_chain) = nearest_package_snapshot(root, owner_dir, snapshot)?;
    let (test, test_chain) = nearest_package_snapshot(root, test_dir, snapshot)?;
    let owner_name = owner.name()?;
    let library = owner.library_identifier()?;
    if !is_library_root(&owner, owner_root) {
        return None;
    }
    // Cargo configuration and manifests between either file and the root can
    // substitute the owner's package under a name Rust source never shows.
    let mentions = [normalized(owner_name), library.clone()];
    if snapshot
        && owner_chain
            .iter()
            .chain(&test_chain)
            .any(|directory| cargo_config_differs(root, directory))
    {
        return None;
    }
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
    let manifests = ancestor_manifests_snapshot(root, &test_chain, snapshot)?;
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
    nearest_package_snapshot(root, directory, false)
}

fn nearest_package_snapshot(
    root: &Path,
    directory: &Path,
    snapshot: bool,
) -> Option<(Package, Vec<PathBuf>)> {
    let chain: Vec<PathBuf> = directory
        .ancestors()
        .take_while(|candidate| candidate.starts_with(root))
        .map(Path::to_path_buf)
        .collect();
    for candidate in &chain {
        match read_manifest(root, candidate, snapshot) {
            Ok(Some(text)) => {
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
            Ok(None) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    None
}

/// Every manifest from the first directory of `chain` up to the root.
fn ancestor_manifests(chain: &[PathBuf]) -> Option<Vec<(PathBuf, toml::Table)>> {
    ancestor_manifests_snapshot(chain.last()?, chain, false)
}

fn ancestor_manifests_snapshot(
    root: &Path,
    chain: &[PathBuf],
    snapshot: bool,
) -> Option<Vec<(PathBuf, toml::Table)>> {
    let mut manifests = Vec::new();
    for directory in chain {
        match read_manifest(root, directory, snapshot) {
            Ok(Some(text)) => {
                manifests.push((directory.clone(), text.parse::<toml::Table>().ok()?))
            }
            Ok(None) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    Some(manifests)
}

fn snapshot_root(root: &Path) -> Option<PathBuf> {
    (if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    })
    .canonicalize()
    .ok()
}

fn read_manifest(root: &Path, directory: &Path, snapshot: bool) -> std::io::Result<Option<String>> {
    if !snapshot {
        return crate::bounded_input::read_metadata_toml(directory.join("Cargo.toml")).map(Some);
    }
    let Some(read_root) = snapshot_root(root) else {
        return Err(std::io::Error::other("workspace root unavailable"));
    };
    let relative = directory
        .strip_prefix(root)
        .map_err(std::io::Error::other)?
        .join("Cargo.toml");
    crate::analysis::committed_source::read_cargo_manifest(&read_root, &relative)?
        .map(|bytes| {
            // The committed object is as hostile as the worktree file: bound it
            // before the `toml` lexer multiplies its size (#11563).
            if bytes.len() as u64 > crate::bounded_input::MAX_METADATA_TOML_BYTES {
                return Err(std::io::Error::other(
                    "committed manifest exceeds the metadata TOML byte limit",
                ));
            }
            String::from_utf8(bytes).map_err(std::io::Error::other)
        })
        .transpose()
}

fn cargo_config_differs(root: &Path, directory: &Path) -> bool {
    let Some(read_root) = snapshot_root(root) else {
        return true;
    };
    let Ok(relative) = directory.strip_prefix(root) else {
        return true;
    };
    let config_dir = relative.join(".cargo");
    if crate::analysis::committed_source::unsupported_head_object(&read_root, &config_dir) {
        return true;
    }
    match std::fs::symlink_metadata(read_root.join(&config_dir)) {
        Ok(metadata) if !metadata.is_dir() => return true,
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return true,
        _ => {}
    }
    ["config", "config.toml"].iter().any(|name| {
        let path = relative.join(".cargo").join(name);
        match crate::analysis::committed_source::lookup(&read_root, &path) {
            crate::analysis::committed_source::CommittedSourceRead::Worktree => {
                match std::fs::symlink_metadata(read_root.join(&path)) {
                    Ok(metadata) => !metadata.is_file(),
                    Err(error) => error.kind() != std::io::ErrorKind::NotFound,
                }
            }
            crate::analysis::committed_source::CommittedSourceRead::AbsentAtHead => {
                !crate::analysis::committed_source::ignored_authority_file_is_absent(
                    &read_root, &path,
                )
            }
            crate::analysis::committed_source::CommittedSourceRead::Committed(_) => true,
        }
    })
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

/// Whether `owner_root` is the root file of `package`'s library: the file
/// `[lib] path` names, else `src/lib.rs` while `autolib` is on. A `[lib] path`
/// that is not a plain relative string, or that leaves the package, names no
/// root (RIPR-SPEC-0171 example 5: a custom `[lib] path` is a crate root as
/// much as `src/lib.rs`, and a dependent imports it by the library name).
fn is_library_root(package: &Package, owner_root: &Path) -> bool {
    let manifest = &package.manifest;
    // `autolib = false` disables only the implicit `src/lib.rs` target; an
    // explicit `[lib] path` is still a configured target (Cargo's
    // `normalize_lib` keeps it in both discovery modes).
    let relative = match manifest.get("lib").and_then(|lib| lib.get("path")) {
        None => {
            let autolib_off = manifest
                .get("package")
                .and_then(|package| package.get("autolib"))
                .and_then(toml::Value::as_bool)
                == Some(false);
            if autolib_off {
                return false;
            }
            Path::new("src").join("lib.rs")
        }
        Some(path) => match path.as_str() {
            Some(text) => lexical(Path::new(text)),
            None => return false,
        },
    };
    let stays_inside = relative
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)));
    stays_inside && owner_root == package.directory.join(relative)
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
    const MACRO_EXPORT: &str = "#[macro_export]\nmacro_rules! assert_eq { ($($t:tt)*) => {} }\n";

    #[test]
    fn literal_macro_exports_follow_selected_local_library_identity() -> Result<(), String> {
        for (dependencies, library, imported, expected) in [
            ("other={path='target/other'}", "", "other", true),
            ("other={path='target/other'}", "", "r#other", true),
            ("other={path='target/other'}", "", "unrelated", false),
            ("", "", "other", false),
            ("other='1'", "", "other", false),
            (
                "other={workspace=true}\n[workspace.dependencies]\nother={path='target/other'}",
                "",
                "other",
                true,
            ),
            (
                "renamed={package='other',path='target/other'}",
                "",
                "renamed",
                true,
            ),
            (
                "renamed={package='elsewhere',path='target/other'}",
                "",
                "renamed",
                false,
            ),
            (
                "other={path='target/other'}",
                "[lib]\nname='exported_macros'\npath='lib/macros.rs'\n",
                "exported_macros",
                true,
            ),
            (
                "other={path='target/other'}",
                "[lib]\nname='exported_macros'\npath='lib/macros.rs'\n",
                "other",
                false,
            ),
        ] {
            let manifest = format!(
                "[package]\nname='consumer'\nversion='0.0.0'\n[dev-dependencies]\n{dependencies}\n"
            );
            let exporter = format!("[package]\nname='other'\nversion='0.0.0'\n{library}");
            let root = temp_workspace(
                "literal-macro-identity",
                &[
                    ("Cargo.toml", &manifest),
                    ("tests/pin.rs", ""),
                    ("target/other/Cargo.toml", &exporter),
                    ("target/other/src/lib.rs", MACRO_EXPORT),
                    ("target/other/lib/macros.rs", MACRO_EXPORT),
                ],
            )?;
            let members = MemberCrates::new(&root);
            let witness = members.imported_literal_macro_export(
                Path::new("tests/pin.rs"),
                imported,
                "assert_eq",
            );
            let missing = members.imported_literal_macro_export(
                Path::new("tests/pin.rs"),
                imported,
                "assert_ne",
            );
            let _ = std::fs::remove_dir_all(&root);
            assert_eq!(
                witness.is_some(),
                expected,
                "{dependencies}; {library}; {imported}"
            );
            if let Some((path, line)) = witness {
                let selected = if library.is_empty() {
                    "target/other/src/lib.rs"
                } else {
                    "target/other/lib/macros.rs"
                };
                assert_eq!(path, Path::new(selected));
                assert_eq!(line, 2);
            }
            assert!(missing.is_none());
        }
        Ok(())
    }

    #[test]
    fn literal_macro_export_witness_reads_the_selected_manifest_and_source_epoch()
    -> Result<(), String> {
        use crate::analysis::committed_source::{CommittedSourceOverlay, with_overlay};
        let manifest = "[package]\nname='consumer'\nversion='0.0.0'\n[dev-dependencies]\nother={path='target/other'}\n";
        let no_dependency = "[package]\nname='consumer'\nversion='0.0.0'\n";
        for (worktree_manifest, worktree_source, selected_manifest, selected_source, expected) in [
            (manifest, MACRO_EXPORT, Some(manifest), Some(""), false),
            (manifest, "", Some(manifest), Some(MACRO_EXPORT), true),
            (
                manifest,
                MACRO_EXPORT,
                Some(no_dependency),
                Some(MACRO_EXPORT),
                false,
            ),
            (
                no_dependency,
                MACRO_EXPORT,
                Some(manifest),
                Some(MACRO_EXPORT),
                true,
            ),
            (manifest, MACRO_EXPORT, None, Some(MACRO_EXPORT), false),
            (manifest, MACRO_EXPORT, Some(manifest), None, false),
        ] {
            let root = temp_workspace(
                "literal-macro-epoch",
                &[
                    ("Cargo.toml", worktree_manifest),
                    ("tests/pin.rs", ""),
                    (
                        "target/other/Cargo.toml",
                        "[package]\nname='other'\nversion='0.0.0'\n",
                    ),
                    ("target/other/src/lib.rs", worktree_source),
                ],
            )?;
            let overlay = CommittedSourceOverlay::from_entries(
                &root,
                [
                    ("Cargo.toml", selected_manifest.map(str::as_bytes)),
                    (
                        "target/other/src/lib.rs",
                        selected_source.map(str::as_bytes),
                    ),
                ],
            );
            let witness = with_overlay(Some(std::sync::Arc::new(overlay)), || {
                MemberCrates::new(&root).imported_literal_macro_export(
                    Path::new("tests/pin.rs"),
                    "other",
                    "assert_eq",
                )
            });
            let _ = std::fs::remove_dir_all(&root);
            assert_eq!(witness.is_some(), expected);
        }
        Ok(())
    }

    #[test]
    fn literal_macro_export_does_not_bypass_the_output_set_input_boundary() -> Result<(), String> {
        let root = temp_workspace(
            "literal-macro-boundary",
            &[
                (
                    "Cargo.toml",
                    "[package]\nname='consumer'\nversion='0.0.0'\n[dev-dependencies]\nother={path='target/other'}\n",
                ),
                ("tests/pin.rs", ""),
                (
                    "target/other/Cargo.toml",
                    "[package]\nname='other'\nversion='0.0.0'\n",
                ),
                ("target/other/src/lib.rs", MACRO_EXPORT),
            ],
        )?;
        let boundary = crate::analysis::input_boundary::Boundary::new(&root)?;
        let witness = MemberCrates::new(&root).imported_literal_macro_export(
            Path::new("tests/pin.rs"),
            "other",
            "assert_eq",
        );
        let guarded = boundary.verify();
        drop(boundary);
        let _ = std::fs::remove_dir_all(&root);
        assert!(witness.is_some());
        assert!(
            guarded.is_err(),
            "excluded auxiliary source must not claim output-set custody"
        );
        Ok(())
    }

    #[test]
    fn an_unrelated_external_library_root_does_not_hide_a_local_macro_export() -> Result<(), String>
    {
        let root = temp_workspace(
            "literal-macro-external-root",
            &[
                (
                    "Cargo.toml",
                    "[package]\nname='consumer'\nversion='0.0.0'\n[dev-dependencies]\nother={path='target/other'}\n",
                ),
                ("tests/pin.rs", ""),
                (
                    "target/other/Cargo.toml",
                    "[package]\nname='other'\nversion='0.0.0'\n",
                ),
                ("target/other/src/lib.rs", MACRO_EXPORT),
                (
                    "unrelated/Cargo.toml",
                    "[package]\nname='unrelated'\nversion='0.0.0'\n[lib]\npath='../../outside.rs'\n",
                ),
            ],
        )?;
        let inventory = selected_library_roots(&root).ok_or("complete library inventory")?;
        assert!(inventory.iter().any(|path| !path.starts_with(&root)));
        let witness = MemberCrates::new(&root).imported_literal_macro_export(
            Path::new("tests/pin.rs"),
            "other",
            "assert_eq",
        );
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(witness, Some((PathBuf::from("target/other/src/lib.rs"), 2)));
        Ok(())
    }

    #[test]
    fn unchanged_index_reuse_refreshes_warmed_auxiliary_macro_export_witnesses()
    -> Result<(), String> {
        use crate::analysis::facts::{
            build_analysis_index_from_loaded_files, index_build_probe,
            reuse_cached_index_with_loaded_files,
        };
        let manifest = "[package]\nname='consumer'\nversion='0.0.0'\n[dev-dependencies]\nother={path='target/other'}\n";
        let no_dependency = "[package]\nname='consumer'\nversion='0.0.0'\n";
        let production = "pub fn value() -> u32 { 8 }\n";
        let test = "#[macro_use] extern crate other;\n#[test]\nfn pins() { assert_eq!(consumer::value(), 8); }\n";
        let root = temp_workspace(
            "literal-macro-reuse",
            &[
                ("Cargo.toml", manifest),
                ("src/lib.rs", production),
                ("tests/pin.rs", test),
                (
                    "target/other/Cargo.toml",
                    "[package]\nname='other'\nversion='0.0.0'\n",
                ),
                ("target/other/src/lib.rs", ""),
            ],
        )?;
        // Neither auxiliary Rust source nor a manifest is among the reused
        // parsed facts. Only those auxiliary bytes change in the loop below.
        let files = vec![
            (PathBuf::from("src/lib.rs"), production.as_bytes().to_vec()),
            (PathBuf::from("tests/pin.rs"), test.as_bytes().to_vec()),
        ];
        assert!(files.iter().all(|(path, _)| !path.starts_with("target")));
        let mut cached = build_analysis_index_from_loaded_files(&root, &files, &[])?;
        let original_hash = cached.files_content_hash.clone();
        assert!(
            cached
                .index
                .member_crates
                .imported_literal_macro_export(Path::new("tests/pin.rs"), "other", "assert_eq",)
                .is_none()
        );
        for (source, selected_manifest, expected) in [
            (MACRO_EXPORT, manifest, true),
            (MACRO_EXPORT, no_dependency, false),
            (MACRO_EXPORT, manifest, true),
            ("", manifest, false),
        ] {
            std::fs::write(root.join("target/other/src/lib.rs"), source)
                .map_err(|error| error.to_string())?;
            std::fs::write(root.join("Cargo.toml"), selected_manifest)
                .map_err(|error| error.to_string())?;
            let previous = cached.index.member_crates.clone();
            // A cloned index shares the old authority until the same-byte
            // reuse boundary installs fresh run-scoped memos.
            cached.index = cached.index.clone();
            let (reused, builds) = index_build_probe::observe(|| {
                reuse_cached_index_with_loaded_files(cached, &root, &files, &[])
            });
            cached = reused?;
            assert_eq!(builds, 0, "auxiliary refresh must not reparse the index");
            assert_eq!(cached.files_content_hash, original_hash);
            assert_eq!(cached.index.files().len(), files.len());
            assert!(!Arc::ptr_eq(
                &previous.imported_macro_exports,
                &cached.index.member_crates.imported_macro_exports,
            ));
            let witness = cached.index.member_crates.imported_literal_macro_export(
                Path::new("tests/pin.rs"),
                "other",
                "assert_eq",
            );
            assert_eq!(witness.is_some(), expected);
        }
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn binary_macro_boundaries_require_manifest_authority() -> Result<(), String> {
        let binary =
            "[package]\nname='fuzz'\nversion='0.0.0'\n[[bin]]\nname='parse'\npath='parse.rs'\n";
        for (manifest, file, expected) in [
            (binary.to_string(), "parse.rs", true),
            (
                binary.replace("path='parse.rs'", "path='./parse.rs'"),
                "parse.rs",
                true,
            ),
            (
                binary.replace("path='parse.rs'", "path='../parse.rs'"),
                "parse.rs",
                false,
            ),
            (
                format!("{binary}[lib]\npath='parse.rs'\n"),
                "parse.rs",
                false,
            ),
            (format!("{binary}[lib]\npath='lib.rs'\n"), "parse.rs", true),
            (PRICING.to_string(), "src/main.rs", true),
            (PRICING.to_string(), "src/bin/tool.rs", true),
            (PRICING.to_string(), "parse.rs", false),
            (PRICING.to_string(), "src/lib.rs", false),
            (format!("{PRICING}autobins=false\n"), "src/main.rs", false),
            ("[workspace]\n".to_string(), "src/main.rs", false),
            ("[package".to_string(), "src/main.rs", false),
        ] {
            let root = temp_workspace("binary-macro-root", &[("Cargo.toml", &manifest)])?;
            let observed = MemberCrates::new(&root).is_binary_target_root(Path::new(file));
            let _ = std::fs::remove_dir_all(&root);
            assert_eq!(observed, expected, "{file}: {manifest}");
        }
        assert!(!MemberCrates::default().is_binary_target_root(Path::new("src/main.rs")));
        let root = temp_workspace(
            "nested-binary-library",
            &[
                (
                    "Cargo.toml",
                    "[package]\nname='parent'\nversion='0.0.0'\n[lib]\npath='fuzz/parse.rs'\n",
                ),
                ("fuzz/Cargo.toml", binary),
            ],
        )?;
        assert!(!MemberCrates::new(&root).is_binary_target_root(Path::new("fuzz/parse.rs")));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn binary_macro_boundaries_refuse_sibling_library_targets() -> Result<(), String> {
        let root = temp_workspace(
            "sibling-binary-library",
            &[
                ("Cargo.toml", "[workspace]\nmembers=['fuzz','bridge']\n"),
                (
                    "fuzz/Cargo.toml",
                    "[package]\nname='fuzz'\nversion='0.0.0'\n[[bin]]\nname='parse'\npath='parse.rs'\n",
                ),
                (
                    "bridge/Cargo.toml",
                    "[package]\nname='bridge'\nversion='0.0.0'\n[lib]\npath='../fuzz/parse.rs'\n",
                ),
            ],
        )?;
        assert!(!MemberCrates::new(&root).is_binary_target_root(Path::new("fuzz/parse.rs")));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn binary_macro_boundaries_refuse_library_targets_in_build_output() -> Result<(), String> {
        let root = temp_workspace(
            "binary-library-under-target",
            &[
                (
                    "Cargo.toml",
                    "[workspace]\nmembers=['fuzz','target/bridge']\n",
                ),
                (
                    "fuzz/Cargo.toml",
                    "[package]\nname='fuzz'\nversion='0.0.0'\n[[bin]]\nname='parse'\npath='parse.rs'\n",
                ),
                (
                    "target/bridge/Cargo.toml",
                    "[package]\nname='bridge'\nversion='0.0.0'\n[lib]\npath='../../fuzz/parse.rs'\n",
                ),
            ],
        )?;
        assert!(!MemberCrates::new(&root).is_binary_target_root(Path::new("fuzz/parse.rs")));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn sibling_library_overlap_reads_the_selected_manifest_epoch() -> Result<(), String> {
        use crate::analysis::committed_source::{CommittedSourceOverlay, with_overlay};
        const SHARED: &str =
            "[package]\nname='bridge'\nversion='0.0.0'\n[lib]\npath='../fuzz/parse.rs'\n";
        const SEPARATE: &str =
            "[package]\nname='bridge'\nversion='0.0.0'\n[lib]\npath='src/lib.rs'\n";
        for (worktree, committed, expected) in [(SHARED, SEPARATE, true), (SEPARATE, SHARED, false)]
        {
            let root = temp_workspace(
                "sibling-binary-epoch",
                &[
                    ("Cargo.toml", "[workspace]\nmembers=['fuzz','bridge']\n"),
                    (
                        "fuzz/Cargo.toml",
                        "[package]\nname='fuzz'\nversion='0.0.0'\n[[bin]]\nname='parse'\npath='parse.rs'\n",
                    ),
                    ("bridge/Cargo.toml", worktree),
                ],
            )?;
            let overlay = CommittedSourceOverlay::from_entries(
                &root,
                [("bridge/Cargo.toml", Some(committed.as_bytes()))],
            );
            let members = MemberCrates::new(&root);
            let observed = with_overlay(Some(std::sync::Arc::new(overlay)), || {
                members.is_binary_target_root(Path::new("fuzz/parse.rs"))
            });
            assert_eq!(observed, expected);
            let _ = std::fs::remove_dir_all(&root);
        }
        Ok(())
    }

    #[test]
    fn binary_macro_boundaries_read_the_selected_manifest_epoch() -> Result<(), String> {
        use crate::analysis::committed_source::{CommittedSourceOverlay, with_overlay};
        let binary =
            "[package]\nname='pricing'\nversion='0.1.0'\n[[bin]]\nname='parse'\npath='parse.rs'\n";
        let library = "[package]\nname='pricing'\nversion='0.1.0'\n[[bin]]\nname='parse'\npath='parse.rs'\n[lib]\npath='parse.rs'\n";
        for (worktree, committed, expected) in [(binary, library, false), (library, binary, true)] {
            let root = temp_workspace("binary-macro-epoch", &[("Cargo.toml", worktree)])?;
            let overlay = CommittedSourceOverlay::from_entries(
                &root,
                [("Cargo.toml", Some(committed.as_bytes()))],
            );
            let members = MemberCrates::new(&root);
            let observed = with_overlay(Some(std::sync::Arc::new(overlay)), || {
                members.is_binary_target_root(Path::new("parse.rs"))
            });
            let _ = std::fs::remove_dir_all(&root);
            assert_eq!(observed, expected);
        }
        Ok(())
    }

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

    fn proc_macro_files(files: &[(&str, &str)], probes: &[&str]) -> Result<Vec<bool>, String> {
        let root = temp_workspace("proc-macro-lib", files)?;
        let members = MemberCrates::new(&root);
        let answers = probes
            .iter()
            .map(|file| members.is_proc_macro_library_file(Path::new(file)))
            .collect();
        let _ = std::fs::remove_dir_all(&root);
        Ok(answers)
    }

    const PM: &str =
        "[package]\nname = \"derive\"\nversion = \"0.1.0\"\n[lib]\nproc-macro = true\n";

    /// #11123: only a file the manifest names as an explicit target entry is
    /// a declared target root; neighbours, the autodiscovered layout, a file
    /// in another package and a missing manifest authority are not.
    #[test]
    fn only_an_explicit_target_path_is_a_declared_target_entry() -> Result<(), String> {
        let manifest = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"xtask\"\npath = \"xtask/main.rs\"\n\n[[example]]\nname = \"tour\"\npath = \"./tour/./demo.rs\"\n";
        let root = temp_workspace(
            "declared-target-entry",
            &[
                ("demo/Cargo.toml", manifest),
                ("other/Cargo.toml", PRICING),
                ("demo/xtask/main.rs", ""),
            ],
        )?;
        let members = MemberCrates::new(&root);
        let answers: Vec<bool> = [
            "demo/xtask/main.rs",
            "demo/tour/demo.rs",
            "demo/xtask/other.rs",
            "demo/src/main.rs",
            "demo/src/lib.rs",
            "other/xtask/main.rs",
            "missing/xtask/main.rs",
        ]
        .iter()
        .map(|file| members.is_declared_target_entry(Path::new(file)))
        .collect();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(answers, [true, true, false, false, false, false, false]);
        assert!(!MemberCrates::default().is_declared_target_entry(Path::new("demo/xtask/main.rs")));
        Ok(())
    }

    #[test]
    fn only_proc_macro_library_modules_are_proc_macro_library_files() -> Result<(), String> {
        let answers = proc_macro_files(
            &[("derive/Cargo.toml", PM), ("plain/Cargo.toml", PRICING)],
            &[
                "derive/src/lib.rs",
                "derive/src/expand/mod.rs",
                "plain/src/lib.rs",
                "derive/build.rs",
                "derive/src/main.rs",
                "derive/src/bin/tool.rs",
                "derive/tests/it.rs",
                "derive/benches/b.rs",
                "derive/examples/e.rs",
                "missing/src/lib.rs",
            ],
        )?;
        assert_eq!(
            answers,
            [
                true, true, false, false, false, false, false, false, false, false
            ]
        );
        assert!(
            !MemberCrates::default().is_proc_macro_library_file(Path::new("derive/src/lib.rs"))
        );
        Ok(())
    }

    #[test]
    fn a_package_root_lib_path_still_excludes_target_directories() -> Result<(), String> {
        let manifest = "[package]\nname = \"derive\"\nversion = \"0.1.0\"\n[lib]\nproc-macro = true\npath = \"lib.rs\"\n";
        let answers = proc_macro_files(
            &[("derive/Cargo.toml", manifest)],
            &[
                "derive/lib.rs",
                "derive/expand/mod.rs",
                "derive/tests/it.rs",
                "derive/benches/b.rs",
                "derive/examples/e.rs",
                "derive/build.rs",
            ],
        )?;
        assert_eq!(answers, [true, true, false, false, false, false]);
        Ok(())
    }

    #[test]
    fn underscore_spelling_and_explicit_targets_are_read() -> Result<(), String> {
        let manifest = "[package]\nname = \"derive\"\nversion = \"0.1.0\"\nbuild = \"src/gen/build.rs\"\n[lib]\nproc_macro = true\n[[bin]]\nname = \"tool\"\npath = \"src/tool/main.rs\"\n";
        let answers = proc_macro_files(
            &[("derive/Cargo.toml", manifest)],
            &[
                "derive/src/lib.rs",
                "derive/src/gen/build.rs",
                "derive/src/tool/main.rs",
                "derive/src/tool/cli.rs",
            ],
        )?;
        assert_eq!(answers, [true, false, false, false]);
        Ok(())
    }

    #[test]
    fn a_virtual_workspace_manifest_alone_is_not_a_proc_macro_package() -> Result<(), String> {
        let answers = proc_macro_files(
            &[("Cargo.toml", "[workspace]\nmembers = []\n")],
            &["src/lib.rs"],
        )?;
        assert_eq!(answers, [false]);
        Ok(())
    }

    // The manifest is read through the committed-source overlay: a dirty
    // working-tree `Cargo.toml` must not change a committed-history answer,
    // in either direction.
    #[test]
    fn a_dirty_manifest_does_not_flip_the_committed_answer() -> Result<(), String> {
        use crate::analysis::committed_source::{CommittedSourceOverlay, with_overlay};
        let plain = "[package]\nname = \"derive\"\nversion = \"0.1.0\"\n";
        let cases: [(&str, Option<&'static str>, bool); 3] = [
            (PM, Some(plain), false),
            (plain, Some(PM), true),
            // No manifest at HEAD: fail closed, whatever the worktree holds.
            (PM, None, false),
        ];
        for (worktree, committed, expected) in cases {
            let root = temp_workspace("proc-macro-dirty", &[("Cargo.toml", worktree)])?;
            let overlay = CommittedSourceOverlay::from_entries(
                &root,
                [("Cargo.toml", committed.map(str::as_bytes))],
            );
            let members = MemberCrates::new(&root);
            let answer = with_overlay(Some(std::sync::Arc::new(overlay)), || {
                members.is_proc_macro_library_file(Path::new("src/lib.rs"))
            });
            let _ = std::fs::remove_dir_all(&root);
            assert_eq!(answer, expected);
        }
        Ok(())
    }

    #[test]
    fn binary_macro_dependency_accepts_a_relative_workspace_root() -> Result<(), String> {
        let root = temp_workspace(
            "binary-macro-relative",
            &[
                (
                    "app/Cargo.toml",
                    "[package]\nname='app'\nversion='0.1.0'\n[dependencies]\nmy_macros={path='../macros'}\n",
                ),
                (
                    "macros/Cargo.toml",
                    "[package]\nname='my_macros'\nversion='0.1.0'\n[lib]\nproc-macro=true\n",
                ),
            ],
        )?;
        let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
        let absolute = root.canonicalize().map_err(|error| error.to_string())?;
        // A relative path cannot cross Windows volumes; on a single volume,
        // derive one without changing the process-wide current directory.
        if let Some(common) = cwd
            .ancestors()
            .find(|ancestor| absolute.starts_with(ancestor))
        {
            let mut relative = PathBuf::new();
            for _ in cwd
                .strip_prefix(common)
                .map_err(|error| error.to_string())?
                .components()
            {
                relative.push("..");
            }
            relative.push(
                absolute
                    .strip_prefix(common)
                    .map_err(|error| error.to_string())?,
            );
            let members = MemberCrates::new(&relative);
            assert!(members.is_default_binary_root(Path::new("app/src/bin/worker.rs")));
            assert!(members.binary_imports_macro(
                Path::new("app/src/bin/worker.rs"),
                Path::new("macros/src/lib.rs"),
                "my_macros"
            ));
        }
        let _ = std::fs::remove_dir_all(root);
        Ok(())
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
    fn a_custom_lib_path_root_names_the_library() -> Result<(), String> {
        // RIPR-SPEC-0171 example 5: `[lib] path = "lib/core.rs"` makes that
        // file the library root a dependent imports by name. The default
        // `src/lib.rs` is then a module of nothing, and a path that leaves
        // the package or is not a string names no root.
        let custom = format!("{PRICING}[lib]\npath = \"lib/core.rs\"\n");
        let files = [
            ("Cargo.toml", ROOT),
            ("pricing/Cargo.toml", custom.as_str()),
            ("orders/Cargo.toml", "[package]\nname = \"orders\"\n"),
        ];
        assert_eq!(
            names(&files, "pricing/tests/t.rs", "pricing/lib/core.rs")?,
            ["pricing"]
        );
        assert_eq!(
            names(&files, "pricing/tests/t.rs", "pricing/src/lib.rs")?,
            Vec::<String>::new()
        );
        for escaping in ["../other/lib.rs", "/abs/lib.rs"] {
            let manifest = format!("{PRICING}[lib]\npath = \"{escaping}\"\n");
            let files = [
                ("Cargo.toml", ROOT),
                ("pricing/Cargo.toml", manifest.as_str()),
                ("orders/Cargo.toml", "[package]\nname = \"orders\"\n"),
            ];
            assert_eq!(
                names(&files, "pricing/tests/t.rs", "other/lib.rs")?,
                Vec::<String>::new(),
                "{escaping}"
            );
        }
        Ok(())
    }

    #[test]
    fn only_a_manifest_moved_lib_path_is_a_custom_root() -> Result<(), String> {
        let custom = format!("{PRICING}[lib]\npath = \"lib/core.rs\"\n");
        let escaping = format!("{PRICING}[lib]\npath = \"../pricing2/lib.rs\"\n");
        let dotted = format!("{PRICING}[lib]\npath = \"./lib/core.rs\"\n");
        let autolib_off = format!("{PRICING}autolib = false\n[lib]\npath = \"lib/core.rs\"\n");
        let autolib_implicit_off = format!("{PRICING}autolib = false\n");
        let probe = |manifest: &str, file: &str| -> Result<bool, String> {
            let root = temp_workspace(
                "custom-lib-root",
                &[
                    ("pricing/Cargo.toml", manifest),
                    ("pricing2/Cargo.toml", "[package]\nname = \"pricing2\"\n"),
                    ("pricing/inner/Cargo.toml", "[package]\nname = \"inner\"\n"),
                ],
            )?;
            let answer = MemberCrates::new(&root).is_custom_lib_path_root(Path::new(file));
            let _ = std::fs::remove_dir_all(&root);
            Ok(answer)
        };
        assert!(probe(&custom, "pricing/lib/core.rs")?);
        assert!(!probe(&custom, "pricing/lib/other.rs")?);
        assert!(!probe(&custom, "pricing/src/lib.rs")?);
        assert!(!probe(PRICING, "pricing/src/lib.rs")?);
        assert!(!probe(&escaping, "pricing2/lib.rs")?);
        assert!(probe(&dotted, "pricing/lib/core.rs")?);
        // `autolib = false` turns off only the implicit target, not a `[lib] path`.
        assert!(probe(&autolib_off, "pricing/lib/core.rs")?);
        assert!(!probe(&autolib_implicit_off, "pricing/src/lib.rs")?);
        // A nested package owns its own files, whatever the outer manifest says.
        assert!(!probe(&custom, "pricing/inner/lib/core.rs")?);
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
