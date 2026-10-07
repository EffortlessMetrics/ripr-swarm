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
//!   `src/lib.rs` with no `[lib] path` and `autolib` on, because another
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

/// Per-analysis cache of member-crate import names, keyed by the test file's
/// directory and the owner's crate root. Clones share the cache.
#[derive(Clone, Debug, Default)]
pub(crate) struct MemberCrates {
    root: Option<PathBuf>,
    names: Arc<Mutex<BTreeMap<(PathBuf, PathBuf), Vec<String>>>>,
    /// Per (crate root, item name): whether the library may export a
    /// foreign item of that name, computed by the classifier.
    foreign_exports: Arc<Mutex<BTreeMap<(PathBuf, String), bool>>>,
}

impl MemberCrates {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: Some(root.to_path_buf()),
            names: Arc::default(),
            foreign_exports: Arc::default(),
        }
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
    for table in test_dependency_tables(&test.manifest) {
        for (key, spec) in table {
            if let Some(name) = import_name(
                key,
                spec,
                &test.directory,
                workspace,
                &owner,
                owner_name,
                &library,
            ) {
                names.push(name);
            }
        }
    }
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
    let moved = manifest
        .get("lib")
        .and_then(|lib| lib.get("path"))
        .is_some();
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
    fn anything_else_names_nothing() -> Result<(), String> {
        let path = "[dependencies]\npricing = { path = \"../pricing\" }\n";
        let plain = orders(path);
        let cases: Vec<(String, Vec<(&str, String)>, &str, &str)> = vec![
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
