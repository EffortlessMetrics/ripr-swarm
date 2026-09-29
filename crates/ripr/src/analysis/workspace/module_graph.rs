//! Module-tree reachability evidence for changed Rust files (#4435).
//!
//! The layout rule alone decides too much: any file under `src/` (or below a
//! declared crate root outside `src/`) seeded diff probes even when no `mod`
//! names it, so rustc never compiles it; and the out-of-line modules of an
//! external crate root (`[lib] path = "../shared/lib.rs"`) never seeded,
//! because their nearest manifest is not the declaring package.
//!
//! This pass walks the module tree of the packages that could compile a
//! changed file, from every Cargo target root, following `mod`, `#[path]` and
//! literal `include!` edges, and records two facts on the source-role
//! context:
//!
//! - **orphans**: the owning package's walk is complete and does not reach
//!   the file, so no target compiles it. It seeds no probes.
//! - **external module sources**: a package whose `[lib]`/`[[bin]]` root sits
//!   outside its own directory reaches the file from that root. It seeds.
//!
//! Unknown never proves absence. A walk that meets an edge the scan cannot
//! resolve (dynamic `#[path]`, `cfg_if!`-wrapped declarations, a parse error,
//! the file bound) is incomplete, and the package keeps the layout rule.
//! Roots are over-collected on purpose (every autodiscovered and declared
//! target, whatever `autobins`/`autotests` say): an extra root can only
//! reach more files, which keeps a verdict of "unreached" conservative.
//!
//! The pass runs only over the files the caller names (the changed files
//! for the diff loop, the finding anchors for the LSP partition), so both
//! surfaces reach the same verdict for the same file and neither pays for
//! packages the diff never touched.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::cargo_targets::{
    collect_explicit_paths, declared_targets_from_manifest, lexical, normalize, owning_package_dir,
};
use super::source_role::SourceRoleContext;
use crate::analysis::syntax::{RustModuleTreeEdge, RustModuleTreeScan, rust_module_tree_scan};

/// Files one package walk may visit before it stops and reports itself
/// incomplete.
const MAX_WALK_FILES: usize = 20_000;

/// How a reached file anchors its own default `mod name;` children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChildAnchor {
    /// Crate roots and `mod.rs` files: children live beside the file.
    Directory,
    /// Ordinary module files: children live under `<dir>/<stem>/`.
    Stem,
    /// Files loaded through `#[path]` or `include!`: try both anchors, so
    /// the walk over-reaches rather than miss a child.
    Both,
}

/// Where a search found a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    /// Reached from a `[lib]` or `[[bin]]` root.
    Production,
    /// Reached only from a test, bench, example or build-script root.
    Evidence,
}

/// One package's module-tree walk, explored lazily.
///
/// A declared file is usually a few `mod` hops below its crate root, so the
/// walk searches toward each asked-for file (the pending entry sharing the
/// longest path prefix with it goes first) and stops when it is found. Only
/// a file the walk cannot find costs a full traversal, and the explored
/// state carries over to the next file asked about. Production roots are
/// exhausted before evidence roots, so a file found in the evidence phase is
/// known to be unreachable from production.
#[derive(Debug)]
struct PackageWalk {
    /// Pending `(file, anchor)` entries for the current phase.
    queue: Vec<(PathBuf, ChildAnchor)>,
    /// The evidence roots, queued once the production phase is exhausted.
    evidence_roots: Option<Vec<PathBuf>>,
    phase: Origin,
    /// Entries already expanded in the current phase.
    visited: BTreeSet<PathBuf>,
    /// Every reached file with the first phase that reached it.
    reached: BTreeMap<PathBuf, Origin>,
    production_roots: BTreeSet<PathBuf>,
    production_root_read: bool,
    /// Parsed module-tree scans, so the evidence phase re-parses nothing.
    scans: BTreeMap<PathBuf, Option<RustModuleTreeScan>>,
    /// False once any edge could not be resolved.
    complete: bool,
}

/// Records module-tree evidence for `candidates` (workspace-relative Rust
/// paths) on `context`. See the module docs for the two recorded facts.
///
/// Returns, for each candidate reached through an external crate root, the
/// declaring package's root prefix (`/`-separated with a trailing `/`, empty
/// for the workspace root), so diff scoping can keep that package's tests.
pub(crate) fn apply_module_graph_evidence<'a, I>(
    workspace_root: &Path,
    context: &mut SourceRoleContext,
    candidates: I,
) -> BTreeMap<PathBuf, String>
where
    I: IntoIterator<Item = &'a Path>,
{
    let mut external_packages = BTreeMap::new();
    let mut walks: BTreeMap<PathBuf, Option<PackageWalk>> = BTreeMap::new();
    let mut external_declarers: Option<Vec<(PathBuf, PathBuf)>> = None;
    for candidate in candidates {
        if candidate.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let relative = normalize(candidate);
        let anchored = lexical(&normalize(&workspace_root.join(&relative)));
        let owner = owning_package_dir(workspace_root, &workspace_root.join(&relative))
            .map(|dir| lexical(&normalize(&dir)));
        let mut owner_proves_unreached = false;
        if let Some(walk) = owner.and_then(|dir| {
            walks
                .entry(dir.clone())
                .or_insert_with(|| PackageWalk::new(&dir))
                .as_mut()
        }) {
            if walk.find(workspace_root, &anchored).is_some() {
                continue;
            }
            owner_proves_unreached = walk.proves_unreached();
        }

        // A package whose production root sits outside its own directory
        // resolves that root's modules there, so its walk can reach a file
        // no layout or owning manifest attributes to it.
        let declarers = external_declarers
            .get_or_insert_with(|| external_root_declarers(workspace_root))
            .iter()
            .filter(|(_, module_dir)| anchored.starts_with(module_dir))
            .map(|(package_dir, _)| package_dir.clone())
            .collect::<BTreeSet<_>>();
        let mut declarers_prove_unreached = true;
        let mut declaring_package = None;
        for package_dir in declarers {
            let Some(walk) = walks
                .entry(package_dir.clone())
                .or_insert_with(|| PackageWalk::new(&package_dir))
                .as_mut()
            else {
                declarers_prove_unreached = false;
                continue;
            };
            match walk.find(workspace_root, &anchored) {
                Some(Origin::Production) => {
                    declaring_package.get_or_insert(package_dir);
                }
                // Reached only from a test, bench or example root: compiled,
                // just not as production. No grant, and no orphan.
                Some(Origin::Evidence) => declarers_prove_unreached = false,
                None => declarers_prove_unreached &= walk.proves_unreached(),
            }
        }
        if let Some(package_dir) = declaring_package {
            if let Some(prefix) = package_prefix(workspace_root, &package_dir) {
                external_packages.insert(relative.clone(), prefix);
            }
            context.declared_production_sources.insert(relative);
        } else if owner_proves_unreached && declarers_prove_unreached {
            context.module_graph_orphans.insert(relative);
        }
    }
    external_packages
}

/// Workspace-relative package root prefix of an anchored package directory.
fn package_prefix(workspace_root: &Path, package_dir: &Path) -> Option<String> {
    let relative = package_dir
        .strip_prefix(lexical(&normalize(workspace_root)))
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    Some(if relative.is_empty() {
        relative
    } else {
        format!("{relative}/")
    })
}

/// `(package dir, root module dir)` for every package manifest in the
/// workspace whose `[lib]` or `[[bin]]` path leaves its own directory.
fn external_root_declarers(workspace_root: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut declarers = Vec::new();
    for prefix in crate::analysis::seam_cache::workspace_manifest_dir_prefixes(workspace_root) {
        let package_dir = lexical(&normalize(&workspace_root.join(&prefix)));
        let Some((_, manifest)) = read_manifest(&package_dir) else {
            continue;
        };
        for root in production_roots(&manifest, &package_dir) {
            if root.starts_with(&package_dir) {
                continue;
            }
            if let Some(module_dir) = root.parent() {
                declarers.push((package_dir.clone(), module_dir.to_path_buf()));
            }
        }
    }
    declarers.sort();
    declarers.dedup();
    declarers
}

/// The package manifest text and its parsed value, or `None` for a missing,
/// invalid or virtual (workspace-only) manifest, which compiles nothing.
fn read_manifest(package_dir: &Path) -> Option<(String, toml::Value)> {
    let text = std::fs::read_to_string(package_dir.join("Cargo.toml")).ok()?;
    let value = toml::from_str::<toml::Value>(&text).ok()?;
    value.get("package")?;
    Some((text, value))
}

/// Explicit `[lib]`/`[[bin]]` paths plus the autodiscovered `src/lib.rs`,
/// `src/main.rs` and `src/bin/` roots.
fn production_roots(manifest: &toml::Value, package_dir: &Path) -> BTreeSet<PathBuf> {
    let mut roots = BTreeSet::new();
    collect_explicit_paths(manifest.get("bin"), package_dir, &mut roots);
    if let Some(lib) = manifest.get("lib") {
        collect_explicit_paths(
            Some(&toml::Value::Array(vec![lib.clone()])),
            package_dir,
            &mut roots,
        );
    }
    let src = package_dir.join("src");
    roots.insert(src.join("lib.rs"));
    roots.insert(src.join("main.rs"));
    roots.extend(autodiscovered(&src.join("bin")));
    roots
        .into_iter()
        .map(|root| lexical(&normalize(&root)))
        .collect()
}

/// Test, bench, example and build-script roots, declared and autodiscovered.
fn evidence_roots(
    manifest: &toml::Value,
    manifest_text: &str,
    package_dir: &Path,
) -> BTreeSet<PathBuf> {
    let declared = declared_targets_from_manifest(manifest_text, package_dir);
    let mut roots = BTreeSet::new();
    roots.extend(declared.tests);
    roots.extend(declared.benches);
    roots.extend(declared.build_script);
    collect_explicit_paths(manifest.get("example"), package_dir, &mut roots);
    for dir in ["tests", "benches", "examples"] {
        roots.extend(autodiscovered(&package_dir.join(dir)));
    }
    roots
        .into_iter()
        .map(|root| lexical(&normalize(&root)))
        .collect()
}

/// Cargo's autodiscovery shapes under one directory: `<dir>/<name>.rs` and
/// `<dir>/<name>/main.rs`.
fn autodiscovered(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            roots.push(path.join("main.rs"));
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            roots.push(path);
        }
    }
    roots
}

impl PackageWalk {
    /// A walk over the package in `package_dir`, or `None` when the
    /// directory holds no package manifest.
    fn new(package_dir: &Path) -> Option<Self> {
        let (manifest_text, manifest) = read_manifest(package_dir)?;
        let production_roots = production_roots(&manifest, package_dir);
        let evidence_roots = evidence_roots(&manifest, &manifest_text, package_dir);
        Some(Self {
            queue: production_roots
                .iter()
                .map(|root| (root.clone(), ChildAnchor::Directory))
                .collect(),
            evidence_roots: Some(evidence_roots.into_iter().collect()),
            phase: Origin::Production,
            visited: BTreeSet::new(),
            reached: BTreeMap::new(),
            production_roots,
            production_root_read: false,
            scans: BTreeMap::new(),
            complete: true,
        })
    }

    /// Where the walk reaches `target`, exploring only as far as needed.
    fn find(&mut self, workspace_root: &Path, target: &Path) -> Option<Origin> {
        loop {
            if let Some(origin) = self.reached.get(target) {
                return Some(*origin);
            }
            if !self.step(workspace_root, target) {
                return None;
            }
        }
    }

    /// Whether a completed walk proves every unreached file unreachable. A
    /// package with no readable library or binary root is not a tree this
    /// walk can judge (a partial checkout, a manifest ahead of its sources).
    fn proves_unreached(&self) -> bool {
        self.complete && self.production_root_read
    }

    /// Expands one pending entry, preferring the one nearest `target`.
    /// Returns false once both phases are exhausted.
    fn step(&mut self, workspace_root: &Path, target: &Path) -> bool {
        let Some(position) = self
            .queue
            .iter()
            .enumerate()
            .max_by_key(|(_, (file, _))| shared_prefix_len(file, target))
            .map(|(position, _)| position)
        else {
            return match self.evidence_roots.take() {
                Some(roots) => {
                    self.phase = Origin::Evidence;
                    self.visited.clear();
                    self.queue = roots
                        .into_iter()
                        .map(|root| (root, ChildAnchor::Directory))
                        .collect();
                    true
                }
                None => false,
            };
        };
        let (file, anchor) = self.queue.swap_remove(position);
        if !self.visited.insert(file.clone()) {
            return true;
        }
        if !self.scans.contains_key(&file) {
            if self.scans.len() >= MAX_WALK_FILES {
                // Stop exploring: the tree is too large to judge.
                self.complete = false;
                self.queue.clear();
                self.evidence_roots = None;
                return false;
            }
            let scan = match read_source(workspace_root, &file) {
                SourceRead::Text(source) => Some(rust_module_tree_scan(&source)),
                SourceRead::Absent => None,
                SourceRead::Unreadable => {
                    self.complete = false;
                    None
                }
            };
            self.scans.insert(file.clone(), scan);
        }
        let Some(Some(scan)) = self.scans.get(&file) else {
            return true;
        };
        let edges = scan.edges.clone();
        self.complete &= scan.complete;
        if self.phase == Origin::Production && self.production_roots.contains(&file) {
            self.production_root_read = true;
        }
        self.reached.entry(file.clone()).or_insert(self.phase);
        let directory = file.parent().map(Path::to_path_buf).unwrap_or_default();
        for edge in edges {
            match edge {
                RustModuleTreeEdge::Default { inline, name } => {
                    for base in child_bases(&file, &directory, anchor) {
                        let base = inline.iter().fold(base, |base, segment| base.join(segment));
                        self.queue
                            .push((base.join(format!("{name}.rs")), ChildAnchor::Stem));
                        self.queue
                            .push((base.join(&name).join("mod.rs"), ChildAnchor::Directory));
                    }
                }
                RustModuleTreeEdge::Path(target) | RustModuleTreeEdge::Include(target) => {
                    self.queue
                        .push((lexical(&directory.join(target)), ChildAnchor::Both));
                }
            }
        }
        true
    }
}

/// Number of leading path components two paths share.
fn shared_prefix_len(left: &Path, right: &Path) -> usize {
    left.components()
        .zip(right.components())
        .take_while(|(left, right)| left == right)
        .count()
}

/// The directories a file's default `mod name;` children resolve under.
fn child_bases(file: &Path, directory: &Path, anchor: ChildAnchor) -> Vec<PathBuf> {
    let stem_dir = file
        .file_stem()
        .map(|stem| directory.join(stem))
        .unwrap_or_else(|| directory.to_path_buf());
    let is_mod_rs = file.file_name().is_some_and(|name| name == "mod.rs");
    match anchor {
        ChildAnchor::Directory => vec![directory.to_path_buf()],
        ChildAnchor::Stem if is_mod_rs => vec![directory.to_path_buf()],
        ChildAnchor::Stem => vec![stem_dir],
        ChildAnchor::Both => vec![directory.to_path_buf(), stem_dir],
    }
}

enum SourceRead {
    Text(String),
    Absent,
    Unreadable,
}

/// Reads a module file through the same committed-source view the analysis
/// reads, so a committed-history diff walks HEAD content.
fn read_source(workspace_root: &Path, file: &Path) -> SourceRead {
    let root = lexical(&normalize(workspace_root));
    let bytes = match file.strip_prefix(&root) {
        Ok(relative) => {
            match crate::analysis::committed_source::read_source_bytes(workspace_root, relative) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => return SourceRead::Absent,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return SourceRead::Absent;
                }
                Err(_) => return SourceRead::Unreadable,
            }
        }
        // A module outside the analyzed workspace still belongs to the tree.
        Err(_) => match std::fs::read(file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return SourceRead::Absent;
            }
            Err(_) => return SourceRead::Unreadable,
        },
    };
    match String::from_utf8(bytes) {
        Ok(text) => SourceRead::Text(text),
        Err(_) => SourceRead::Unreadable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, files: &[(&str, &str)]) -> Result<PathBuf, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("ripr-module-graph-{name}-{stamp}"));
        for (path, text) in files {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(&path, text).map_err(|error| error.to_string())?;
        }
        Ok(root)
    }

    fn evidence_for(root: &Path, candidates: &[&str]) -> SourceRoleContext {
        let mut context = SourceRoleContext::empty();
        apply_module_graph_evidence(root, &mut context, candidates.iter().map(Path::new));
        context
    }

    const MANIFEST: &str = "[package]\nname='tree'\nversion='0.1.0'\nedition='2021'\n";

    #[test]
    fn walk_follows_mod_rs_bin_roots_and_evidence_roots() -> Result<(), String> {
        let root = fixture(
            "anchors",
            &[
                ("Cargo.toml", MANIFEST),
                ("src/lib.rs", "pub mod a;\n"),
                ("src/a/mod.rs", "pub mod b;\n"),
                ("src/a/b.rs", ""),
                ("src/bin/tool.rs", "mod cli;\nfn main() {}\n"),
                ("src/bin/cli.rs", ""),
                (
                    "tests/it.rs",
                    "#[path = \"../src/shared_by_tests.rs\"]\nmod shared;\n",
                ),
                ("src/shared_by_tests.rs", ""),
                ("src/orphan.rs", ""),
                ("src/a/orphan.rs", ""),
            ],
        )?;
        let context = evidence_for(
            &root,
            &[
                "src/a/b.rs",
                "src/bin/cli.rs",
                "src/shared_by_tests.rs",
                "src/orphan.rs",
                "src/a/orphan.rs",
            ],
        );
        assert_eq!(
            context.module_graph_orphans,
            BTreeSet::from([
                PathBuf::from("src/a/orphan.rs"),
                PathBuf::from("src/orphan.rs")
            ])
        );
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }

    #[test]
    fn walk_proves_nothing_without_a_readable_production_root() -> Result<(), String> {
        // No `src/lib.rs`, `src/main.rs` or declared root: the tree is
        // unknown, so a loose `src/` file keeps the layout rule.
        let root = fixture(
            "no-root",
            &[("Cargo.toml", MANIFEST), ("src/a.rs", ""), ("src/b.rs", "")],
        )?;
        let context = evidence_for(&root, &["src/a.rs"]);
        assert!(context.module_graph_orphans.is_empty());
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }

    #[test]
    fn walk_proves_nothing_for_virtual_or_missing_manifests() -> Result<(), String> {
        let root = fixture(
            "virtual",
            &[
                ("Cargo.toml", "[workspace]\nmembers=[]\n"),
                ("src/lib.rs", ""),
                ("src/orphan.rs", ""),
                ("loose/src/lib.rs", ""),
                ("loose/src/orphan.rs", ""),
            ],
        )?;
        let context = evidence_for(&root, &["src/orphan.rs", "loose/src/orphan.rs"]);
        assert!(context.module_graph_orphans.is_empty());
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }
}
