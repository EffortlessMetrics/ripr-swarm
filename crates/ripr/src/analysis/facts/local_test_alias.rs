//! Crate-local `test` attribute aliases (#9001).
//!
//! Some workspaces re-bind the built-in `test` name to a test-wrapping
//! attribute macro of their own and then write `#[crate::test]`:
//!
//! ```text
//! // lib.rs
//! use axum_macros::__private_axum_test as test;
//! // anywhere in the crate
//! #[crate::test]
//! async fn handles_request() { .. }
//! ```
//!
//! `crate::test` is not a built-in test attribute path, so these tests were
//! invisible. This module credits `#[crate::test]` and `#[self::test]` only
//! when the alias is established by source the index holds:
//!
//! - the crate root (`src/lib.rs`, else `src/main.rs`; every existing root
//!   must agree) for `crate::test`, or the test's own file for `self::test`,
//!   has exactly one top-level, ungated `use ... as test;`;
//! - the aliased name resolves to exactly one `#[proc_macro_attribute]` fn
//!   in the analyzed index; and
//! - source of that macro's package names a known test attribute
//!   (`tokio::test`, `async_std::test` or `#[test]`) outside line comments.
//!
//! The target is matched by its last path segment, so a same-named macro in
//! another crate is not told apart; two candidates refuse. Anything
//! unresolved (an external crate, a gated or repeated import, a macro that
//! never names a test attribute) credits nothing. Arbitrary `#[x::test]`
//! paths are untouched.

use super::index::{FactArena, IndexedFileFacts};
use super::model::FunctionFact;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::AstNode;
use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const TEST_ATTRIBUTE_NEEDLES: &[&str] = &["tokio::test", "async_std::test", "#[test]"];

/// Which files may spell their test attribute `#[crate::test]` or
/// `#[self::test]`.
#[derive(Debug, Default)]
pub(super) struct LocalTestAliases {
    aliased: BTreeSet<PathBuf>,
    roots: BTreeMap<PathBuf, Vec<PathBuf>>,
}

/// The two path spellings one file is allowed.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FileAliasCredit {
    crate_path: bool,
    self_path: bool,
}

impl FileAliasCredit {
    pub(super) fn credits(self, normalized_path: &str) -> bool {
        match normalized_path {
            "crate::test" => self.crate_path,
            "self::test" => self.self_path,
            _ => false,
        }
    }
}

impl LocalTestAliases {
    pub(super) fn collect(
        files: &BTreeMap<PathBuf, IndexedFileFacts>,
        function_facts: &FactArena<FunctionFact>,
    ) -> Self {
        let mut aliased = BTreeSet::new();
        let mut proc_macros: Option<BTreeMap<String, Vec<PathBuf>>> = None;
        for (path, facts) in files {
            let Some(name) = test_alias_target(&facts.source) else {
                continue;
            };
            let macros = proc_macros.get_or_insert_with(|| proc_macro_files(files, function_facts));
            let Some([macro_file]) = macros.get(&name).map(Vec::as_slice) else {
                continue;
            };
            if package_names_test_attribute(files, macro_file) {
                aliased.insert(path.clone());
            }
        }
        let roots = if aliased.is_empty() {
            BTreeMap::new()
        } else {
            files
                .keys()
                .map(|path| (path.clone(), crate_roots(files, path)))
                .collect()
        };
        Self { aliased, roots }
    }

    pub(super) fn credit_for(&self, file: &Path) -> FileAliasCredit {
        if self.aliased.is_empty() {
            return FileAliasCredit::default();
        }
        let roots = self.roots.get(file).map(Vec::as_slice).unwrap_or(&[]);
        FileAliasCredit {
            crate_path: !roots.is_empty() && roots.iter().all(|root| self.aliased.contains(root)),
            self_path: self.aliased.contains(file),
        }
    }
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn proc_macro_files(
    files: &BTreeMap<PathBuf, IndexedFileFacts>,
    function_facts: &FactArena<FunctionFact>,
) -> BTreeMap<String, Vec<PathBuf>> {
    let mut proc_macros: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (path, facts) in files {
        for &id in &facts.functions {
            let function = &function_facts[id];
            if function
                .attrs
                .iter()
                .any(|attr| compact(attr) == "#[proc_macro_attribute]")
            {
                proc_macros
                    .entry(function.name.clone())
                    .or_default()
                    .push(path.clone());
            }
        }
    }
    proc_macros
}

/// The last path segment of the single top-level, ungated `use ... as test`.
fn test_alias_target(source: &str) -> Option<String> {
    if !source.contains("as test") && !source.contains("as r#test") {
        return None;
    }
    let parse = parse_clean_source_file(source)?;
    let mut found = Vec::new();
    for item in parse.tree().items() {
        let ast::Item::Use(item) = item else {
            continue;
        };
        let Some(tree) = item.use_tree() else {
            continue;
        };
        let gated = item.attrs().next().is_some();
        for tree in tree.syntax().descendants().filter_map(ast::UseTree::cast) {
            let Some(alias) = tree.rename().and_then(|rename| rename.name()) else {
                continue;
            };
            if alias.text().trim_start_matches("r#") != "test" {
                continue;
            }
            let target = tree
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .map(|name| name.text().to_string());
            found.push((gated, target));
        }
    }
    match found.as_slice() {
        [(false, Some(target))] if !target.is_empty() => Some(target.clone()),
        _ => None,
    }
}

fn src_dir(file: &Path) -> Option<&Path> {
    file.ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "src"))
}

fn package_names_test_attribute(
    files: &BTreeMap<PathBuf, IndexedFileFacts>,
    macro_file: &Path,
) -> bool {
    let Some(src) = src_dir(macro_file) else {
        return false;
    };
    files
        .iter()
        .filter(|(path, _)| src_dir(path) == Some(src))
        .any(|(_, facts)| {
            let code = facts
                .source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .map(compact)
                .collect::<String>();
            TEST_ATTRIBUTE_NEEDLES
                .iter()
                .any(|needle| code.contains(needle))
        })
}

/// The crate-root files `file` could belong to: `src/lib.rs` and
/// `src/main.rs` of its package that the index holds. Empty when the file
/// is not under a `src` directory (an integration test is its own crate).
fn crate_roots(files: &BTreeMap<PathBuf, IndexedFileFacts>, file: &Path) -> Vec<PathBuf> {
    let Some(src) = src_dir(file) else {
        return Vec::new();
    };
    ["lib.rs", "main.rs"]
        .iter()
        .map(|name| src.join(name))
        .filter(|root| files.contains_key(root))
        .collect()
}
