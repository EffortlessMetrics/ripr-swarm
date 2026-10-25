//! Which `macro_rules!` definitions a file sees from other files (#6925).
//!
//! The test-generator expander reads one file. A table whose generator is
//! defined in `tests/macros.rs` and invoked in `tests/cases.rs` needs that
//! definition spliced in front of `cases.rs`. Rust's textual scope gives
//! two routes, and this module resolves exactly those:
//!
//! - `#[macro_use] mod name;` in the file: the macros `name`'s file defines
//!   (and the macros of its own `#[macro_use]` modules) are in scope;
//! - the file is a child module `mod name;` of a parent that defines the
//!   macro (or imports it with `#[macro_use]`) before that declaration.
//!
//! Anything else resolves to nothing, which leaves the invocation
//! unexpanded: an unresolved or ambiguous module declaration (a `#[path]`,
//! another `cfg`, no file, several candidate files, several parents) never
//! guesses. `#[macro_export]` paths, `use`-imported macros and `include!`
//! are out of scope.

use crate::analysis::sync_memo::SyncMemo;
use crate::analysis::syntax::{MacroScope, macro_scope};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Child modules followed through `#[macro_use]` and parents followed
/// upward. A deeper chain is refused.
const MAX_DEPTH: usize = 4;
/// Imported definitions one file may carry; beyond it the file gets none.
const MAX_IMPORTED_BYTES: usize = 64 * 1024;
/// Directory names whose `.rs` files are crate roots of their own.
const ROOT_DIRECTORIES: [&str; 3] = ["tests", "benches", "examples"];

#[derive(Debug, Default)]
pub(crate) struct MacroVisibility {
    sources: BTreeMap<PathBuf, Arc<str>>,
    /// Names of macros whose definition mentions a test; a file that never
    /// spells one of them followed by `!` needs nothing resolved.
    generators: BTreeSet<String>,
    scopes: SyncMemo<BTreeMap<PathBuf, Option<Arc<MacroScope>>>>,
}

impl Clone for MacroVisibility {
    fn clone(&self) -> Self {
        Self {
            sources: self.sources.clone(),
            generators: self.generators.clone(),
            scopes: SyncMemo::default(),
        }
    }
}

impl MacroVisibility {
    pub(crate) fn new(files: impl Iterator<Item = (PathBuf, Arc<str>)>) -> Self {
        let sources: BTreeMap<PathBuf, Arc<str>> = files.collect();
        let mut generators = BTreeSet::new();
        for source in sources.values() {
            if !source.contains("macro_rules") || !source.contains("test") {
                continue;
            }
            for scoped in macro_scope(source).unwrap_or_default().macros {
                if scoped.text.contains("test") {
                    generators.insert(scoped.name);
                }
            }
        }
        Self {
            sources,
            generators,
            scopes: SyncMemo::default(),
        }
    }

    /// One line of `macro_rules!` items `file` sees from other files, or an
    /// empty string.
    pub(crate) fn imported_definitions(&self, file: &Path) -> String {
        let Some(source) = self.sources.get(file) else {
            return String::new();
        };
        if !self
            .generators
            .iter()
            .any(|name| source.contains(&format!("{name}!")))
        {
            return String::new();
        }
        let mut definitions = Vec::<String>::new();
        let mut visited = BTreeSet::new();
        self.collect_inherited(file, 0, &mut visited, &mut definitions);
        self.collect_imports(file, usize::MAX, 0, &mut visited, &mut definitions);
        let mut seen = BTreeSet::new();
        definitions.retain(|text| seen.insert(text.clone()));
        let joined = definitions.join(" ");
        if joined.len() > MAX_IMPORTED_BYTES {
            return String::new();
        }
        joined
    }

    fn scope(&self, file: &Path) -> Option<Arc<MacroScope>> {
        let cached = self.scopes.borrow().get(file).cloned();
        if let Some(cached) = cached {
            return cached;
        }
        let scope = self
            .sources
            .get(file)
            .and_then(|source| macro_scope(source))
            .map(Arc::new);
        self.scopes
            .borrow()
            .insert(file.to_path_buf(), scope.clone());
        scope
    }

    /// The `#[macro_use]` modules of `file` declared before byte `before`,
    /// with the definitions their files make visible.
    fn collect_imports(
        &self,
        file: &Path,
        before: usize,
        depth: usize,
        visited: &mut BTreeSet<PathBuf>,
        out: &mut Vec<String>,
    ) {
        if depth >= MAX_DEPTH {
            return;
        }
        let Some(scope) = self.scope(file) else {
            return;
        };
        for module in scope
            .modules
            .iter()
            .filter(|module| module.macro_use && module.plain && module.offset < before)
        {
            let Some(child) = self.child_file(file, &module.name) else {
                continue;
            };
            if !visited.insert(child.clone()) {
                continue;
            }
            if let Some(child_scope) = self.scope(&child) {
                out.extend(child_scope.macros.iter().map(|scoped| scoped.text.clone()));
            }
            self.collect_imports(&child, usize::MAX, depth + 1, visited, out);
        }
    }

    /// Definitions a parent makes visible to `file` through the position of
    /// its `mod` declaration, then the parent's own inheritance.
    fn collect_inherited(
        &self,
        file: &Path,
        depth: usize,
        visited: &mut BTreeSet<PathBuf>,
        out: &mut Vec<String>,
    ) {
        if depth >= MAX_DEPTH {
            return;
        }
        let Some((parent, offset)) = self.parent(file) else {
            return;
        };
        if !visited.insert(parent.clone()) {
            return;
        }
        if let Some(scope) = self.scope(&parent) {
            out.extend(
                scope
                    .macros
                    .iter()
                    .filter(|scoped| scoped.offset < offset)
                    .map(|scoped| scoped.text.clone()),
            );
        }
        self.collect_imports(&parent, offset, depth + 1, visited, out);
        self.collect_inherited(&parent, depth + 1, visited, out);
    }

    /// The unique file whose top-level `mod name;` resolves to `file`, and
    /// the byte offset of that declaration.
    fn parent(&self, file: &Path) -> Option<(PathBuf, usize)> {
        let (name, location) = module_identity(file)?;
        let mut found = Vec::new();
        for candidate in self.declarer_candidates(&location) {
            if candidate == file || !self.sources.contains_key(&candidate) {
                continue;
            }
            let Some(scope) = self.scope(&candidate) else {
                continue;
            };
            let declares = scope.modules.iter().find(|module| {
                module.name == name
                    && module.plain
                    && self.child_file(&candidate, &module.name).as_deref() == Some(file)
            });
            if let Some(module) = declares {
                found.push((candidate, module.offset));
            }
        }
        (found.len() == 1).then(|| found.remove(0))
    }

    /// Files whose `mod name;` could resolve into `location`.
    fn declarer_candidates(&self, location: &Path) -> Vec<PathBuf> {
        let mut candidates = BTreeSet::new();
        for root in ["mod.rs", "lib.rs", "main.rs"] {
            candidates.insert(location.join(root));
        }
        if is_root_directory(location) {
            for path in self.sources.keys() {
                if path.parent() == Some(location) {
                    candidates.insert(path.clone());
                }
            }
        }
        if let (Some(grandparent), Some(name)) = (location.parent(), location.file_name()) {
            candidates.insert(grandparent.join(Path::new(name).with_extension("rs")));
        }
        candidates.into_iter().collect()
    }

    /// The one file `mod name;` in `declarer` resolves to, over every
    /// anchor the declarer could have. Zero or several candidates is
    /// unresolved.
    fn child_file(&self, declarer: &Path, name: &str) -> Option<PathBuf> {
        let directory = declarer.parent()?;
        let stem = declarer.file_stem()?.to_str()?;
        let root_like = matches!(
            declarer.file_name().and_then(|name| name.to_str()),
            Some("lib.rs" | "main.rs" | "mod.rs" | "build.rs")
        );
        let mut bases = Vec::new();
        if root_like || is_root_directory(directory) {
            bases.push(directory.to_path_buf());
        }
        if !root_like {
            bases.push(directory.join(stem));
        }
        let mut found = BTreeSet::new();
        for base in bases {
            for candidate in [
                base.join(format!("{name}.rs")),
                base.join(name).join("mod.rs"),
            ] {
                if self.sources.contains_key(&candidate) {
                    found.insert(candidate);
                }
            }
        }
        (found.len() == 1)
            .then(|| found.into_iter().next())
            .flatten()
    }
}

fn is_root_directory(directory: &Path) -> bool {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| ROOT_DIRECTORIES.contains(&name))
}

/// The module name a file would be declared as, and the directory that
/// holds its siblings (`tests/common/mod.rs` is `common` in `tests/`).
fn module_identity(file: &Path) -> Option<(String, PathBuf)> {
    let directory = file.parent()?;
    let stem = file.file_stem()?.to_str()?;
    if matches!(
        file.file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "main.rs" | "build.rs")
    ) {
        return None;
    }
    if stem == "mod" {
        let name = directory.file_name()?.to_str()?.to_string();
        return Some((name, directory.parent()?.to_path_buf()));
    }
    Some((stem.to_string(), directory.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENERATOR: &str = "macro_rules! t { ($n:ident) => { #[test] fn $n() {} }; }\n";

    fn visibility(files: &[(&str, &str)]) -> MacroVisibility {
        MacroVisibility::new(
            files
                .iter()
                .map(|(path, text)| (PathBuf::from(path), Arc::<str>::from(*text))),
        )
    }

    fn defines_t(text: &str) -> bool {
        text.starts_with("macro_rules! t ")
    }

    #[test]
    fn a_module_file_inherits_through_every_anchor_layout() {
        let invoke = "t!(a);\n";
        let layouts: [(&[(&str, &str)], &str); 3] = [
            // Library crate root: children beside it.
            (
                &[("src/lib.rs", "mod a;\n"), ("src/a.rs", invoke)],
                "src/a.rs",
            ),
            // A non-root module file: children under `src/a/`.
            (
                &[
                    ("src/lib.rs", "mod a;\n"),
                    ("src/a.rs", "mod b;\n"),
                    ("src/a/b.rs", invoke),
                ],
                "src/a/b.rs",
            ),
            // A `mod.rs` directory module.
            (
                &[
                    ("src/lib.rs", "mod a;\n"),
                    ("src/a/mod.rs", "mod b;\n"),
                    ("src/a/b.rs", invoke),
                ],
                "src/a/b.rs",
            ),
        ];
        for (files, invoking) in layouts {
            let mut sources: Vec<(String, String)> = files
                .iter()
                .map(|(path, text)| (path.to_string(), text.to_string()))
                .collect();
            // The crate root defines the generator before declaring `mod a;`.
            sources[0].1 = format!("{GENERATOR}{}", sources[0].1);
            let refs: Vec<(&str, &str)> = sources
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_str()))
                .collect();
            let imported = visibility(&refs).imported_definitions(Path::new(invoking));
            assert!(defines_t(&imported), "{invoking}: {imported:?}");
        }
    }

    #[test]
    fn a_file_with_no_generator_call_resolves_nothing() {
        let visibility = visibility(&[
            ("tests/main.rs", &format!("{GENERATOR}mod cases;\n")),
            ("tests/cases.rs", "fn unrelated() {}\n"),
        ]);
        assert_eq!(
            visibility.imported_definitions(Path::new("tests/cases.rs")),
            ""
        );
    }

    #[test]
    fn a_chain_deeper_than_the_limit_is_refused() {
        let mut files: Vec<(String, String)> =
            vec![("src/lib.rs".to_string(), format!("{GENERATOR}mod m1;\n"))];
        for depth in 1..=MAX_DEPTH + 1 {
            let next = if depth == MAX_DEPTH + 1 {
                "t!(x);\n".to_string()
            } else {
                format!("mod m{};\n", depth + 1)
            };
            let directory = (1..depth).map(|n| format!("m{n}/")).collect::<String>();
            files.push((format!("src/{directory}m{depth}.rs"), next));
        }
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let deepest = files[files.len() - 1].0.clone();
        let imported = visibility(&refs).imported_definitions(Path::new(&deepest));
        assert_eq!(
            imported,
            "",
            "the generator is {} modules up",
            MAX_DEPTH + 1
        );
    }
}
