//! Crate-local `test` attribute aliases (#9001).
//!
//! Some workspaces re-bind the built-in `test` name to a test-wrapping
//! attribute macro of their own and then write `#[crate::test]`:
//!
//! ```text
//! // lib.rs
//! use app_macros::wrapped_test as test;
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
//!   must agree), or a manifest-confirmed direct automatic binary root
//!   (`src/bin/<name>.rs`), for `crate::test`; the test's own file for `self::test`
//!   has exactly one top-level `use ... as test;` that is ungated or whose `cfg`
//!   attributes require a test build per `cfg_predicates` (`cfg(test)`,
//!   `cfg(all(test, <anything>))`, stacked gates); the extra `all(..)`
//!   conjuncts are accepted by design, since a `#[crate::test]` user
//!   implies the alias is compiled;
//! - the import path starts at a workspace member crate (`crate`, `self`
//!   and `super` stay local), so a foreign crate's namesake macro is never
//!   mistaken for a workspace one;
//! - the aliased name resolves to exactly one `#[proc_macro_attribute]` fn
//!   in the analyzed index; and
//! - source of that macro's package names a known test attribute
//!   (`tokio::test`, `async_std::test` or `#[test]`) in code tokens, not in
//!   comments or string literals.
//!
//! The target is matched by its last path segment, so a workspace macro of
//! the same name in a different member crate is not told apart from the
//! named one; two candidates refuse. The test attribute is searched in the
//! macro's package, not traced through its expansion. Anything unresolved
//! (an external crate, a non-test-gated or repeated import, a macro that
//! never names a test attribute) credits nothing. Arbitrary `#[x::test]`
//! paths are untouched. Draft and Fast scans index the changed package and
//! its dependents, so a wrapper macro in an unchanged dependency is not
//! seen there and nothing is credited (#10141).

use super::index::{FactArena, IndexedFileFacts};
use super::member_crates::MemberCrates;
use super::model::FunctionFact;
use crate::analysis::syntax::{
    name_scope_for_file, parse_clean_source_file, rust_module_tree_scan,
};
use ra_ap_syntax::AstNode;
use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};
use ra_ap_syntax::{NodeOrToken, SyntaxKind};
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
        workspace_crates: &BTreeSet<String>,
        member_crates: &MemberCrates,
    ) -> Self {
        let mut aliased = BTreeSet::new();
        let mut proc_macros: Option<BTreeMap<String, Vec<PathBuf>>> = None;
        for (path, facts) in files {
            let Some(target) = test_alias_target(&facts.source) else {
                continue;
            };
            if !target.names_workspace_crate(workspace_crates) {
                continue;
            }
            let name = target.name;
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
                .map(|path| (path.clone(), crate_roots(files, path, member_crates)))
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

/// What a `use ... as test` import names: the leading and last path segments.
struct AliasTarget {
    first: String,
    name: String,
}

impl AliasTarget {
    /// `crate::`, `self::` and `super::` paths stay inside the importing
    /// crate; any other leading segment must be one of the workspace's own
    /// crates, so an external crate's namesake macro earns no credit.
    fn names_workspace_crate(&self, workspace_crates: &BTreeSet<String>) -> bool {
        matches!(self.first.as_str(), "crate" | "self" | "super")
            || workspace_crates.contains(&self.first)
    }
}

/// Whether `as` is followed, across any whitespace, by `test` or `r#test`.
fn spells_test_alias(source: &str) -> bool {
    source.match_indices("as").any(|(at, _)| {
        let before = source[..at].chars().next_back();
        if before.is_some_and(|ch| ch.is_alphanumeric() || ch == '_') {
            return false;
        }
        let rest = source[at + 2..].trim_start();
        let rest = rest.strip_prefix("r#").unwrap_or(rest);
        rest.strip_prefix("test").is_some_and(|after| {
            !after
                .chars()
                .next()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        })
    })
}

/// The single top-level `use ... as test`, ungated or whose cfg attributes
/// require a test build (extra `all(test, ..)` conjuncts are accepted by
/// design).
fn test_alias_target(source: &str) -> Option<AliasTarget> {
    if !spells_test_alias(source) {
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
        // An attribute that holds in every test build (`cfg(test)`,
        // `cfg(all(test, ..))`) leaves the alias active wherever tests run;
        // any other attribute (feature, `not(test)`, ..) keeps it unproven.
        let attrs: Vec<String> = item
            .attrs()
            .map(|attr| attr.syntax().text().to_string())
            .collect();
        let gated = !attrs.is_empty() && !super::cfg_predicates::attributes_require_test(&attrs);
        for tree in tree.syntax().descendants().filter_map(ast::UseTree::cast) {
            let Some(alias) = tree.rename().and_then(|rename| rename.name()) else {
                continue;
            };
            if alias.text().trim_start_matches("r#") != "test" {
                continue;
            }
            let target = tree.path().and_then(|path| {
                let name = path.segment()?.name_ref()?.text().to_string();
                let first = use_prefix(&tree).or_else(|| {
                    path.first_segment()?
                        .name_ref()
                        .map(|n| n.text().to_string())
                })?;
                Some(AliasTarget { first, name })
            });
            found.push((gated, target));
        }
    }
    match found.as_slice() {
        [(false, Some(target))] if !target.name.is_empty() => Some(AliasTarget {
            first: target.first.clone(),
            name: target.name.clone(),
        }),
        _ => None,
    }
}

/// The leading path segment contributed by enclosing use-tree prefixes, for
/// grouped imports such as `use macros::{wrapped_test as test};`.
fn use_prefix(tree: &ast::UseTree) -> Option<String> {
    let outer = tree
        .syntax()
        .ancestors()
        .skip(1)
        .filter_map(ast::UseTree::cast)
        .last()?;
    let path = outer.path()?;
    Some(path.first_segment()?.name_ref()?.text().to_string())
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
            let code = code_tokens(&facts.source);
            TEST_ATTRIBUTE_NEEDLES
                .iter()
                .any(|needle| code.contains(needle))
        })
}

/// The source's tokens with whitespace removed, leaving out comments and
/// string literals so prose or a message that merely mentions a test
/// attribute is not read as one. An unparseable file contributes nothing.
fn code_tokens(source: &str) -> String {
    let Some(parse) = parse_clean_source_file(source) else {
        return String::new();
    };
    parse
        .tree()
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| match element {
            NodeOrToken::Token(token) => Some(token),
            NodeOrToken::Node(_) => None,
        })
        .filter(|token| {
            !matches!(
                token.kind(),
                SyntaxKind::COMMENT
                    | SyntaxKind::STRING
                    | SyntaxKind::BYTE_STRING
                    | SyntaxKind::C_STRING
            )
        })
        .map(|token| compact(token.text()))
        .collect()
}

/// Indexed library/main roots, or the file itself for an admitted direct
/// automatic binary. Unresolved binary helpers and files outside `src`
/// receive no crate-root credit.
fn crate_roots(
    files: &BTreeMap<PathBuf, IndexedFileFacts>,
    file: &Path,
    member_crates: &MemberCrates,
) -> Vec<PathBuf> {
    let Some(src) = src_dir(file) else {
        return Vec::new();
    };
    if file.starts_with(src.join("bin")) {
        return if member_crates.is_default_binary_root(file)
            && binary_alias_root_is_unambiguous(files, file, src)
        {
            vec![file.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    ["lib.rs", "main.rs"]
        .iter()
        .map(|name| src.join(name))
        .filter(|root| files.contains_key(root))
        .collect()
}

/// The new binary route refuses unresolved namespace/alternate module
/// readings. Do not guess through reexports, globs or item-producing macros.
fn binary_alias_root_is_unambiguous(
    files: &BTreeMap<PathBuf, IndexedFileFacts>,
    file: &Path,
    src: &Path,
) -> bool {
    let Some(facts) = files.get(file) else {
        return false;
    };
    let Some(target) = test_alias_target(&facts.source) else {
        return false;
    };
    if matches!(target.first.as_str(), "crate" | "self" | "super") {
        return false;
    }
    let Some(parse) = parse_clean_source_file(&facts.source) else {
        return false;
    };
    let scope = name_scope_for_file(&parse.tree());
    let shadows =
        |name: &str| name.trim_start_matches("r#") == target.first.trim_start_matches("r#");
    if scope.child_modules.iter().any(|name| shadows(name))
        || scope.defines.iter().any(|name| shadows(name))
        || scope.imports.iter().any(|import| shadows(&import.bound))
        || !scope.globs.is_empty()
    {
        return false;
    }
    // Until module composition supplies target-specific contexts here,
    // keep packages with any indexed module/include edge unresolved.
    files
        .iter()
        .filter(|(path, _)| src_dir(path) == Some(src))
        .all(|(_, facts)| {
            let scan = rust_module_tree_scan(&facts.source);
            scan.complete && scan.edges.is_empty()
        })
}
