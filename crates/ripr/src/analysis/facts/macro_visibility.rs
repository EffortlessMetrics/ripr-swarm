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
use crate::analysis::syntax::{MacroScope, first_macro_call, macro_scope};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

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
    /// Module name to the files that declare it with a `#[path]` or another
    /// attribute; built on first use.
    attributed: OnceLock<BTreeMap<String, BTreeSet<PathBuf>>>,
}

impl Clone for MacroVisibility {
    fn clone(&self) -> Self {
        Self {
            sources: self.sources.clone(),
            generators: self.generators.clone(),
            scopes: SyncMemo::default(),
            attributed: OnceLock::new(),
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
            attributed: OnceLock::new(),
        }
    }

    /// One line of `macro_rules!` items `file` sees from other files, or an
    /// empty string.
    pub(crate) fn imported_definitions(&self, file: &Path) -> String {
        let Some(source) = self.sources.get(file) else {
            return String::new();
        };
        // A `#[macro_use]` module declared after the first call is not in
        // scope at it.
        let Some(first_call) = first_macro_call(source, &self.generators) else {
            return String::new();
        };
        let mut walk = Walk::default();
        if self.scope(file).is_none_or(|scope| scope.opaque) {
            return String::new();
        }
        self.collect_inherited(file, 0, &mut walk);
        self.collect_imports(file, first_call, 0, &mut walk);
        if walk.refused {
            return String::new();
        }
        let mut seen = BTreeSet::new();
        walk.out.retain(|text| seen.insert(text.clone()));
        let joined = walk.out.join(" ");
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
    fn collect_imports(&self, file: &Path, before: usize, depth: usize, walk: &mut Walk) {
        if depth > MAX_DEPTH {
            walk.refused = true;
            return;
        }
        let Some(scope) = self.scope(file) else {
            walk.refused = true;
            return;
        };
        walk.active.insert(file.to_path_buf());
        for module in scope
            .modules
            .iter()
            .filter(|module| module.macro_use && module.offset < before)
        {
            // A `#[macro_use]` module that is not resolved could define a
            // macro that shadows the one that is.
            let child = module
                .plain
                .then(|| self.child_file(file, &module.name))
                .flatten();
            let child_scope = child.as_ref().and_then(|child| self.scope(child));
            let (Some(child), Some(child_scope)) = (child, child_scope) else {
                walk.refused = true;
                continue;
            };
            if child_scope.opaque {
                walk.refused = true;
            }
            // A module that is still being walked is a cycle through the
            // anchor alternatives, not a program rustc accepts; one already
            // finished is reused.
            if walk.active.contains(&child) {
                walk.refused = true;
                continue;
            }
            if !walk.visited.insert(child.clone()) {
                continue;
            }
            walk.out
                .extend(child_scope.macros.iter().map(|scoped| scoped.text.clone()));
            self.collect_imports(&child, usize::MAX, depth + 1, walk);
        }
        walk.active.remove(file);
    }

    /// Definitions a parent makes visible to `file` through the position of
    /// its `mod` declaration, then the parent's own inheritance.
    fn collect_inherited(&self, file: &Path, depth: usize, walk: &mut Walk) {
        let Some((parent, offset)) = self.parent(file) else {
            return;
        };
        if depth >= MAX_DEPTH {
            walk.refused = true;
            return;
        }
        if !walk.visited.insert(parent.clone()) {
            walk.refused = true;
            return;
        }
        let Some(scope) = self.scope(&parent) else {
            walk.refused = true;
            return;
        };
        if scope.opaque {
            walk.refused = true;
        }
        walk.out.extend(
            scope
                .macros
                .iter()
                .filter(|scoped| scoped.offset < offset)
                .map(|scoped| scoped.text.clone()),
        );
        self.collect_imports(&parent, offset, depth + 1, walk);
        self.collect_inherited(&parent, depth + 1, walk);
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
        // A `#[path]` or cfg'd declaration of the name anywhere can include
        // the same file in another crate, where the macros differ.
        let other_declaration = self
            .attributed_declarations()
            .get(&name)
            .is_some_and(|files| files.iter().any(|candidate| candidate != file));
        (found.len() == 1 && !other_declaration).then(|| found.remove(0))
    }

    fn attributed_declarations(&self) -> &BTreeMap<String, BTreeSet<PathBuf>> {
        self.attributed.get_or_init(|| {
            let mut index = BTreeMap::<String, BTreeSet<PathBuf>>::new();
            for path in self.sources.keys() {
                let Some(scope) = self.scope(path) else {
                    continue;
                };
                for module in scope.modules.iter().filter(|module| !module.plain) {
                    index
                        .entry(module.name.clone())
                        .or_default()
                        .insert(path.clone());
                }
            }
            index
        })
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

/// `tests/`, `benches/` or `examples/` of a package. A `src/tests/` is an
/// ordinary module directory.
fn is_root_directory(directory: &Path) -> bool {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| ROOT_DIRECTORIES.contains(&name))
        && directory
            .parent()
            .and_then(Path::file_name)
            .is_none_or(|parent| parent != "src")
}

/// State of one resolution walk.
#[derive(Default)]
struct Walk {
    visited: BTreeSet<PathBuf>,
    /// Files whose imports are being walked now.
    active: BTreeSet<PathBuf>,
    out: Vec<String>,
    /// Something that could define or shadow a macro was not resolved.
    refused: bool,
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

    fn imported(files: &[(&str, &str)], file: &str) -> String {
        visibility(files).imported_definitions(Path::new(file))
    }

    #[test]
    fn an_unresolved_macro_use_module_could_shadow_so_nothing_is_imported() {
        let generator = |tag: &str| {
            format!("macro_rules! t {{ ($n:ident) => {{ #[test] fn $n() {{ {tag} }} }}; }}\n")
        };
        let cases = "t!(a);\n";
        let v1 = generator("1");
        for declaration in [
            "#[macro_use] #[path = \"m2.rs\"] mod m2;\n",
            "#[cfg(feature = \"x\")] #[macro_use] mod m2;\n",
            "#[macro_use] mod missing;\n",
        ] {
            let main = format!("{v1}{declaration}mod cases;\n");
            let files = [
                ("tests/main.rs", main.as_str()),
                ("tests/m2.rs", "macro_rules! t { () => {} }\n"),
                ("tests/cases.rs", cases),
            ];
            assert_eq!(imported(&files, "tests/cases.rs"), "", "{declaration}");
        }
        let inline = format!(
            "{v1}#[macro_use]\nmod inner {{ macro_rules! t {{ () => {{}} }} }}\nmod cases;\n"
        );
        let files = [
            ("tests/main.rs", inline.as_str()),
            ("tests/cases.rs", cases),
        ];
        assert_eq!(imported(&files, "tests/cases.rs"), "", "inline macro_use");
    }

    #[test]
    fn a_definition_that_cannot_be_listed_refuses_the_older_one() {
        let main = format!(
            "{GENERATOR}macro_rules! t {{ ($n:ident) => {{ #[test] fn $n() {{ \"a\nb\"; }} }}; }}\nmod cases;\n"
        );
        let files = [
            ("tests/main.rs", main.as_str()),
            ("tests/cases.rs", "t!(a);\n"),
        ];
        assert_eq!(imported(&files, "tests/cases.rs"), "");
    }

    #[test]
    fn a_macro_use_module_declared_after_the_call_is_not_in_scope() {
        let main = "t!(a);\n#[macro_use]\nmod macros;\n";
        let files = [("tests/main.rs", main), ("tests/macros.rs", GENERATOR)];
        assert_eq!(imported(&files, "tests/main.rs"), "");
        let before = "#[macro_use]\nmod macros;\nt!(a);\n";
        let files = [("tests/main.rs", before), ("tests/macros.rs", GENERATOR)];
        assert!(defines_t(&imported(&files, "tests/main.rs")));
    }

    #[test]
    fn a_path_or_cfg_declaration_of_the_module_elsewhere_makes_the_parent_ambiguous() {
        let main = format!("{GENERATOR}mod cases;\n");
        let other = "#[path = \"../tests/cases.rs\"]\nmod cases;\n";
        let files = [
            ("tests/main.rs", main.as_str()),
            ("tests/other.rs", other),
            ("tests/cases.rs", "t!(a);\n"),
        ];
        assert_eq!(imported(&files, "tests/cases.rs"), "");
    }

    #[test]
    fn a_src_tests_directory_is_a_module_directory_not_a_crate_root() {
        assert!(is_root_directory(Path::new("tests")));
        assert!(is_root_directory(Path::new("crates/x/tests")));
        assert!(!is_root_directory(Path::new("src/tests")));
        assert!(!is_root_directory(Path::new("crates/x/src/tests")));
    }

    #[test]
    fn a_generator_exactly_at_the_depth_limit_is_followed() {
        let mut files: Vec<(String, String)> =
            vec![("src/lib.rs".to_string(), format!("{GENERATOR}mod m1;\n"))];
        for depth in 1..=MAX_DEPTH {
            let next = if depth == MAX_DEPTH {
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
        assert!(defines_t(&imported(&refs, &deepest)));
    }

    #[test]
    fn only_a_real_call_orders_the_macro_use_modules() {
        let before = [
            "assert!(x);\n#[macro_use]\nmod macros;\nt!(a);\n",
            "fn helper() { /* t!(earlier); */ }\n#[macro_use]\nmod macros;\nt!(a);\n",
            "const S: &str = \"t!(earlier)\";\n#[macro_use]\nmod macros;\nt!(a);\n",
            "macro_rules! unused { () => { t!(earlier); } }\n#[macro_use]\nmod macros;\nt!(a);\n",
        ];
        for main in before {
            let files = [("tests/main.rs", main), ("tests/macros.rs", GENERATOR)];
            assert!(defines_t(&imported(&files, "tests/main.rs")), "{main}");
        }
    }

    #[test]
    fn a_cycle_through_the_anchor_alternatives_is_refused() {
        let a = "#[macro_use]\nmod b;\nt!(x);\n";
        let b = format!("{GENERATOR}#[macro_use]\nmod a;\n");
        let files = [("tests/a.rs", a), ("tests/b.rs", b.as_str())];
        assert_eq!(imported(&files, "tests/a.rs"), "");
    }
}
