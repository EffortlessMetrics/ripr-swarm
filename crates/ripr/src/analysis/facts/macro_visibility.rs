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
//! guesses.
//!
//! A third route is a `#[macro_export]` macro named by path (#10298): the
//! file spells `use crate::name;` and exactly one definition of `name` exists
//! in the file's crate, plain `#[macro_export]`, with nothing else in the
//! crate able to define or export a macro of that name. Cross-crate paths,
//! renamed or grouped imports and `include!` are out of scope.

use crate::analysis::sync_memo::SyncMemo;
use crate::analysis::syntax::{
    ImportedMacros, MacroScope, first_macro_call, generator_calls, macro_scope,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Child modules followed through `#[macro_use]` and parents followed
/// upward. A deeper chain is refused.
const MAX_DEPTH: usize = 4;
/// Imported definitions one file may carry; beyond it the file gets none.
const MAX_IMPORTED_BYTES: usize = 64 * 1024;
/// Parent steps followed to find a file's crate root.
const CRATE_ROOT_DEPTH: usize = 8;
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
    /// Generator name to every file that defines a macro of that name, with
    /// the definition when it is a plain `#[macro_export]`; files that spell
    /// `macro_export` somewhere unlisted. Built on first use.
    exports: OnceLock<ExportIndex>,
}

#[derive(Debug, Default)]
struct ExportIndex {
    definitions: BTreeMap<String, Vec<(PathBuf, Option<String>)>>,
    opaque: BTreeSet<PathBuf>,
}

impl Clone for MacroVisibility {
    fn clone(&self) -> Self {
        Self {
            sources: self.sources.clone(),
            generators: self.generators.clone(),
            scopes: SyncMemo::default(),
            attributed: OnceLock::new(),
            exports: OnceLock::new(),
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
            exports: OnceLock::new(),
        }
    }

    /// The `macro_rules!` items `file` sees from other files, or nothing.
    pub(crate) fn imported_macros(&self, file: &Path) -> ImportedMacros {
        let Some(source) = self.sources.get(file) else {
            return ImportedMacros::default();
        };
        // A `#[macro_use]` module declared after the first call is not in
        // scope at it.
        let Some(first_call) = first_macro_call(source, &self.generators) else {
            return ImportedMacros::default();
        };
        let mut walk = Walk::default();
        let Some(own) = self.scope(file).filter(|scope| !scope.opaque) else {
            return ImportedMacros::default();
        };
        self.collect_inherited(file, 0, &mut walk);
        self.collect_imports(file, first_call, 0, &mut walk);
        let mut path_imports = Vec::new();
        let mut blanked_prefixes = Vec::new();
        let calls = generator_calls(source, &self.generators).unwrap_or_default();
        let local: BTreeSet<&str> = own.macros.iter().map(|m| m.name.as_str()).collect();
        let imported_by_use: BTreeSet<&str> =
            own.uses.iter().map(|import| import.name.as_str()).collect();
        for (name, calls) in &calls {
            let by_use = imported_by_use.contains(name.as_str());
            // A bare call with no `use` could be a glob import or an
            // extern-crate macro; the splice would shadow it. A file that
            // defines the name itself is not importing it.
            let usable = if by_use {
                !calls.nested_bare
            } else {
                !calls.bare && !calls.crate_prefixes.is_empty()
            };
            if !usable || local.contains(name.as_str()) {
                continue;
            }
            // An import that does not resolve stays in the file, so the
            // expander refuses that one name; the others are unaffected.
            if let Some(text) = self.exported_definition(file, name) {
                walk.out.push(text);
                if by_use {
                    path_imports.push(name.clone());
                }
                blanked_prefixes.extend(calls.crate_prefixes.iter().copied());
            }
        }
        if walk.refused {
            return ImportedMacros::default();
        }
        let mut seen = BTreeSet::new();
        walk.out.retain(|text| seen.insert(text.clone()));
        let definitions = walk.out.join(" ");
        if definitions.len() > MAX_IMPORTED_BYTES {
            return ImportedMacros::default();
        }
        ImportedMacros {
            definitions,
            path_imports,
            blanked_prefixes,
        }
    }

    #[cfg(test)]
    fn imported_definitions(&self, file: &Path) -> String {
        self.imported_macros(file).definitions
    }

    /// The flattened definition `crate::name` names in `file`'s crate, when
    /// exactly one macro of that name exists there and it is a plain
    /// `#[macro_export]`. A file whose crate cannot be determined, or that
    /// might belong to it, refuses.
    fn exported_definition(&self, file: &Path, name: &str) -> Option<String> {
        let root = self.crate_root(file)?;
        let index = self.export_index();
        let same_crate = |candidate: &Path| {
            candidate == file || self.crate_root(candidate).is_none_or(|other| other == root)
        };
        if index.opaque.iter().any(|candidate| same_crate(candidate)) {
            return None;
        }
        let mut here = index
            .definitions
            .get(name)?
            .iter()
            .filter(|(candidate, _)| same_crate(candidate));
        let (definer, definition) = here.next()?;
        if here.next().is_some() {
            return None;
        }
        // The definer must be known to belong to the crate: a `cfg`'d, `path`
        // or undeclared module may not be compiled at all.
        (self.crate_root(definer).as_ref() == Some(&root))
            .then(|| definition.clone())
            .flatten()
    }

    /// The crate root file `file` belongs to, following unique `mod`
    /// declarations upward.
    fn crate_root(&self, file: &Path) -> Option<PathBuf> {
        let mut current = file.to_path_buf();
        for _ in 0..=CRATE_ROOT_DEPTH {
            // A declaration decides first: `src/commands/build.rs` declared
            // by `mod build;` is a module, not a crate root.
            match self.parent(&current) {
                Some((parent, _)) => current = parent,
                None if self.is_crate_root(&current) => return Some(current),
                None => return None,
            }
        }
        None
    }

    /// A root file nothing declares: `lib.rs`, `main.rs`, `build.rs` (not
    /// inside a `src` tree, where it is a module), or a file directly in
    /// `tests`, `benches` or `examples`.
    fn is_crate_root(&self, file: &Path) -> bool {
        match file.file_name().and_then(|name| name.to_str()) {
            // `src/lib.rs`, `src/main.rs` and `src/bin/<name>/main.rs`; a
            // `lib.rs` deeper in a tree is a module that `mod lib;` names.
            Some("lib.rs" | "main.rs")
                if file.parent().is_some_and(|directory| {
                    directory.file_name().is_some_and(|name| name == "src")
                        || directory
                            .parent()
                            .and_then(Path::file_name)
                            .is_some_and(|name| name == "bin")
                }) =>
            {
                true
            }
            Some("build.rs") => !file
                .ancestors()
                .skip(1)
                .any(|directory| directory.file_name().is_some_and(|name| name == "src")),
            _ => file.parent().is_some_and(is_root_directory),
        }
    }

    fn export_index(&self) -> &ExportIndex {
        self.exports.get_or_init(|| {
            let mut index = ExportIndex::default();
            for path in self.sources.keys() {
                if !self
                    .sources
                    .get(path)
                    .is_some_and(|source| source.contains("macro_rules"))
                {
                    continue;
                }
                let Some(scope) = self.scope(path) else {
                    continue;
                };
                if scope.export_opaque {
                    index.opaque.insert(path.clone());
                }
                for scoped in scope
                    .macros
                    .iter()
                    .filter(|scoped| self.generators.contains(&scoped.name))
                {
                    index
                        .definitions
                        .entry(scoped.name.clone())
                        .or_default()
                        .push((path.clone(), scoped.exported.then(|| scoped.text.clone())));
                }
            }
            index
        })
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

    const EXPORTED: &str =
        "#[macro_export]\nmacro_rules! t { ($n:ident) => { #[test] fn $n() {} }; }\n";

    fn path_imported(files: &[(&str, &str)], file: &str) -> (String, Vec<String>) {
        let imported = visibility(files).imported_macros(Path::new(file));
        (imported.definitions, imported.path_imports)
    }

    #[test]
    fn an_exported_macro_named_by_path_resolves_in_its_crate() {
        let files = [
            ("src/lib.rs", "mod macros;\nmod cases;\n"),
            ("src/macros.rs", EXPORTED),
            ("src/cases.rs", "use crate::t;\nt!(a);\n"),
        ];
        let (definitions, path_imports) = path_imported(&files, "src/cases.rs");
        assert!(defines_t(&definitions) || definitions.contains("macro_rules! t "));
        assert_eq!(path_imports, vec!["t".to_string()]);
        // The defining file names nothing by path.
        assert_eq!(
            path_imported(&files, "src/macros.rs").1,
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_crate_path_call_resolves_without_an_import_but_a_bare_call_does_not() {
        let lib = ("src/lib.rs", "mod macros;\nmod cases;\n");
        let files = [
            lib,
            ("src/macros.rs", EXPORTED),
            ("src/cases.rs", "crate::t!(a);\ncrate :: t!(b);\n"),
        ];
        let imported = visibility(&files).imported_macros(Path::new("src/cases.rs"));
        assert!(imported.definitions.contains("macro_rules! t "));
        assert_eq!(imported.blanked_prefixes.len(), 2);
        assert!(imported.path_imports.is_empty());
        let bare = [
            lib,
            ("src/macros.rs", EXPORTED),
            ("src/cases.rs", "crate::t!(a);\nt!(b);\n"),
        ];
        assert_eq!(
            visibility(&bare).imported_macros(Path::new("src/cases.rs")),
            ImportedMacros::default()
        );
        let nested = [
            lib,
            ("src/macros.rs", EXPORTED),
            ("src/cases.rs", "fn f() { crate::t!(a); }\n"),
        ];
        assert_eq!(
            visibility(&nested).imported_macros(Path::new("src/cases.rs")),
            ImportedMacros::default()
        );
    }

    #[test]
    fn an_exported_macro_of_another_crate_is_not_a_second_definition() {
        let files = [
            ("a/src/lib.rs", "mod macros;\nmod cases;\n"),
            ("a/src/macros.rs", EXPORTED),
            ("a/src/cases.rs", "use crate::t;\nt!(a);\n"),
            ("b/src/lib.rs", "mod macros;\n"),
            ("b/src/macros.rs", EXPORTED),
        ];
        assert_eq!(
            path_imported(&files, "a/src/cases.rs").1,
            vec!["t".to_string()]
        );
    }

    #[test]
    fn a_path_import_that_is_not_exactly_one_plain_export_refuses() {
        let cases = "use crate::t;\nt!(a);\n";
        let lib = "mod macros;\nmod other;\nmod cases;\n";
        let refused = |macros: &str, other: &str| {
            let files = [
                ("src/lib.rs", lib),
                ("src/macros.rs", macros),
                ("src/other.rs", other),
                ("src/cases.rs", cases),
            ];
            path_imported(&files, "src/cases.rs")
        };
        let none = (String::new(), Vec::<String>::new());
        // A second macro of the name anywhere in the crate.
        assert_eq!(refused(EXPORTED, GENERATOR), none, "second definition");
        assert_eq!(refused(EXPORTED, EXPORTED), none, "second export");
        // Not exported: `use crate::t` cannot name it.
        assert_eq!(refused(GENERATOR, "fn f() {}\n"), none, "not exported");
        // An export the scope cannot list can define the name too.
        let hidden = "mod inner {\n    #[macro_export]\n    macro_rules! t { ($n:ident) => { #[test] fn $n() {} }; }\n}\n";
        assert_eq!(refused(EXPORTED, hidden), none, "export inside a module");
        let gated = "#[cfg(feature = \"x\")]\n#[macro_export]\nmacro_rules! t { () => {} }\n";
        assert_eq!(refused(EXPORTED, gated), none, "cfg'd export");
        let local = "#[macro_export(local_inner_macros)]\nmacro_rules! t { () => {} }\n";
        assert_eq!(refused(local, "fn f() {}\n"), none, "export with arguments");
    }

    #[test]
    fn a_path_import_needs_top_level_calls_and_a_declared_definer() {
        let lib = "mod macros;\nmod cases;\n";
        let nested = "use crate::t;\nmod tests { t!(a); }\n";
        let files = [
            ("src/lib.rs", lib),
            ("src/macros.rs", EXPORTED),
            ("src/cases.rs", nested),
        ];
        assert_eq!(
            path_imported(&files, "src/cases.rs"),
            (String::new(), Vec::new()),
            "a child module does not see the import"
        );
        let cases = "use crate::t;\nt!(a);\n";
        for declaration in [
            "#[cfg(feature = \"x\")]\nmod macros;\nmod cases;\n",
            "#[path = \"macros.rs\"]\nmod macros;\nmod cases;\n",
            "mod cases;\n",
        ] {
            let files = [
                ("src/lib.rs", declaration),
                ("src/macros.rs", EXPORTED),
                ("src/cases.rs", cases),
            ];
            assert_eq!(
                path_imported(&files, "src/cases.rs"),
                (String::new(), Vec::new()),
                "{declaration}"
            );
        }
        // `#[cfg(test)]` leaves the module plain.
        let files = [
            ("src/lib.rs", "#[cfg(test)]\nmod macros;\nmod cases;\n"),
            ("src/macros.rs", EXPORTED),
            ("src/cases.rs", cases),
        ];
        assert_eq!(
            path_imported(&files, "src/cases.rs").1,
            vec!["t".to_string()]
        );
    }

    #[test]
    fn a_nested_lib_rs_is_a_module_not_a_crate_root() {
        // `src/a/lib.rs` is declared by `mod lib;` in `src/a.rs`; `module_identity`
        // cannot name it, so the exported macro in it is not attributed to a
        // crate, and a sibling is refused rather than credited from a wrong root.
        let files = [
            ("src/lib.rs", "mod a;\n"),
            ("src/a.rs", "mod lib;\nmod cases;\n"),
            ("src/a/lib.rs", EXPORTED),
            ("src/a/cases.rs", "use crate::t;\nt!(boundary);\n"),
        ];
        let visibility = visibility(&files);
        assert!(!visibility.is_crate_root(Path::new("src/a/lib.rs")));
        assert!(visibility.is_crate_root(Path::new("src/lib.rs")));
        assert!(visibility.is_crate_root(Path::new("crates/x/src/main.rs")));
        assert!(visibility.is_crate_root(Path::new("src/bin/tool/main.rs")));
        assert_eq!(
            visibility
                .imported_macros(Path::new("src/a/cases.rs"))
                .path_imports,
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_module_file_named_like_a_root_is_not_taken_for_a_crate() {
        // `module_identity` cannot name a `build.rs` module, so its parent is
        // unknown; it must not become a crate of its own either.
        let files = [
            ("src/lib.rs", "mod commands;\nmod macros;\nmod other;\n"),
            ("src/commands/mod.rs", "mod build;\n"),
            ("src/commands/build.rs", "use crate::t;\nt!(a);\n"),
            ("src/macros.rs", EXPORTED),
            ("src/other.rs", GENERATOR),
        ];
        assert_eq!(
            path_imported(&files, "src/commands/build.rs"),
            (String::new(), Vec::new())
        );
        // A package-root build script is still a root.
        assert!(!visibility(&files).is_crate_root(Path::new("src/commands/build.rs")));
        assert!(visibility(&files).is_crate_root(Path::new("crates/x/build.rs")));
    }

    #[test]
    fn an_unresolved_path_import_does_not_withdraw_a_resolved_one() {
        let two = "#[macro_export]\nmacro_rules! u { ($n:ident) => { #[test] fn $n() {} }; }\n";
        let macros = format!("{EXPORTED}{two}");
        let other = "macro_rules! u { () => {} }\n";
        let files = [
            ("src/lib.rs", "mod macros;\nmod other;\nmod cases;\n"),
            ("src/macros.rs", macros.as_str()),
            ("src/other.rs", other),
            (
                "src/cases.rs",
                "use crate::t;\nuse crate::u;\nt!(a);\nu!(b);\n",
            ),
        ];
        // `u` has a second definition, `t` does not.
        assert_eq!(
            path_imported(&files, "src/cases.rs").1,
            vec!["t".to_string()]
        );
    }

    #[test]
    fn a_renamed_grouped_or_orphan_path_import_resolves_nothing() {
        let lib = "mod macros;\nmod cases;\n";
        for cases in [
            "use crate::t as u;\nt!(a);\n",
            "use crate::{t};\nt!(a);\n",
            "use crate::*;\nt!(a);\n",
            "#[cfg(unix)]\nuse crate::t;\nt!(a);\n",
        ] {
            let files = [
                ("src/lib.rs", lib),
                ("src/macros.rs", EXPORTED),
                ("src/cases.rs", cases),
            ];
            assert_eq!(
                path_imported(&files, "src/cases.rs"),
                (String::new(), Vec::new()),
                "{cases}"
            );
        }
        // `cases.rs` that no `mod` declaration reaches has no known crate.
        let files = [
            ("src/lib.rs", "mod macros;\n"),
            ("src/macros.rs", EXPORTED),
            ("src/cases.rs", "use crate::t;\nt!(a);\n"),
        ];
        assert_eq!(
            path_imported(&files, "src/cases.rs"),
            (String::new(), Vec::new())
        );
    }

    #[test]
    fn an_integration_test_crate_resolves_its_own_exported_macro() {
        let files = [
            ("tests/main.rs", "mod support;\nmod cases;\n"),
            ("tests/support/mod.rs", EXPORTED),
            ("tests/cases.rs", "use crate::t;\nt!(a);\n"),
            ("tests/other.rs", EXPORTED),
        ];
        // `tests/other.rs` is another test crate with its own `t`.
        assert_eq!(
            path_imported(&files, "tests/cases.rs").1,
            vec!["t".to_string()]
        );
        // A second test crate that exports the name is a different crate; a
        // module of the same crate that does is a second definition.
        let twice = [
            ("tests/main.rs", "mod support;\nmod cases;\n"),
            ("tests/support/mod.rs", EXPORTED),
            ("tests/cases.rs", "use crate::t;\nt!(a);\n"),
            ("tests/support/extra.rs", EXPORTED),
        ];
        assert_eq!(
            path_imported(&twice, "tests/cases.rs").1,
            Vec::<String>::new()
        );
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
