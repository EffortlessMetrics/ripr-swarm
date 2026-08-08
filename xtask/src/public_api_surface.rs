//! Transitive public-API surface collection for `check-public-api` (#3052).
//!
//! The previous collector matched two line prefixes in
//! `crates/ripr/src/lib.rs`, so every `pub` item reachable through an
//! allowlisted `pub mod` was invisible to the gate. This module walks the
//! crate's module tree with `ra_ap_syntax` — the same parser and
//! parse-and-walk shape already used by `no_panic::collect_semantic_panic_findings`
//! — and records module-level items whose visibility is a bare `pub`.
//!
//! Scope boundary, stated so the gate does not overclaim again: this collects
//! module-level items only. Public struct fields, enum variants, trait items,
//! and associated functions in `impl` blocks are semver-relevant but are not
//! collected here.
//!
//! Resolution boundary: this is a syntax walk, not a name resolution pass. A
//! `pub use` records the name it binds in its own module, which is the
//! nameable public path, without resolving what that name refers to. A glob
//! re-export cannot be expanded and is recorded as a glob entry so it must be
//! recorded deliberately rather than passing silently.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ra_ap_syntax::ast::{self, HasAttrs, HasName, HasVisibility};
use ra_ap_syntax::{AstNode, Edition, SourceFile, SyntaxKind, SyntaxNode};

use crate::read_text_lossy;

/// Collect the public API surface reachable from `root_file`, naming every
/// entry under `crate_name`.
///
/// Entries are `<kind> <path>` (for example `pub mod ripr::domain`,
/// `pub const ripr::output::start_here_state::START_HERE_CLEAN`), sorted and
/// deduplicated. Anything the walk cannot resolve is an error rather than a
/// silent omission.
pub(crate) fn public_api_surface(
    root_file: &Path,
    crate_name: &str,
) -> Result<Vec<String>, String> {
    let mut entries = BTreeSet::new();
    let mut visited = BTreeSet::new();
    collect_module_file(root_file, crate_name, &mut entries, &mut visited)?;
    Ok(entries.into_iter().collect())
}

fn collect_module_file(
    file: &Path,
    module_path: &str,
    entries: &mut BTreeSet<String>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    if !visited.insert(file.to_path_buf()) {
        return Ok(());
    }
    let text = read_text_lossy(file)?;
    let parse = SourceFile::parse(&text, Edition::Edition2024);
    if !parse.errors().is_empty() {
        return Err(format!(
            "{}: the file does not parse, so its public surface cannot be collected",
            file.display()
        ));
    }
    let dir = child_module_dir(file)?;
    collect_items(
        parse.tree().syntax(),
        &dir,
        module_path,
        file,
        entries,
        visited,
    )
}

fn collect_items(
    node: &SyntaxNode,
    dir: &Path,
    module_path: &str,
    owner: &Path,
    entries: &mut BTreeSet<String>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    for child in node.children() {
        let Some(item) = ast::Item::cast(child) else {
            continue;
        };
        if is_cfg_test(&item) {
            continue;
        }
        match item {
            ast::Item::Module(module) => {
                collect_module(&module, dir, module_path, owner, entries, visited)?;
            }
            ast::Item::Use(use_item) if is_bare_pub(&use_item) => {
                collect_use(&use_item, module_path, entries);
            }
            // `#[macro_export]` puts the macro at the crate root regardless of
            // where it is declared, so its own visibility does not apply.
            ast::Item::MacroRules(macro_rules) if has_macro_export(&macro_rules) => {
                let name = item_name(macro_rules.name());
                entries.insert(format!("pub macro {}::{name}", crate_root(module_path)));
            }
            ast::Item::Fn(item) => insert_named(entries, &item, "fn", module_path, item.name()),
            ast::Item::Struct(item) => {
                insert_named(entries, &item, "struct", module_path, item.name());
            }
            ast::Item::Enum(item) => insert_named(entries, &item, "enum", module_path, item.name()),
            ast::Item::Union(item) => {
                insert_named(entries, &item, "union", module_path, item.name());
            }
            ast::Item::Trait(item) => {
                insert_named(entries, &item, "trait", module_path, item.name());
            }
            ast::Item::TypeAlias(item) => {
                insert_named(entries, &item, "type", module_path, item.name());
            }
            ast::Item::Const(item) => {
                insert_named(entries, &item, "const", module_path, item.name());
            }
            ast::Item::Static(item) => {
                insert_named(entries, &item, "static", module_path, item.name());
            }
            ast::Item::ExternCrate(item) if is_bare_pub(&item) => {
                let name = match item.rename().and_then(|rename| rename.name()) {
                    Some(name) => name.text().to_string(),
                    None => match item.name_ref() {
                        Some(name_ref) => name_ref.text().to_string(),
                        None => "<unnamed>".to_string(),
                    },
                };
                entries.insert(format!("pub extern crate {module_path}::{name}"));
            }
            ast::Item::MacroDef(item) => {
                insert_named(entries, &item, "macro", module_path, item.name());
            }
            // Items that are not bare `pub`, plus `impl` associated items,
            // `extern` blocks, and macro invocations, which are outside the
            // stated module-level scope of this collector.
            _ => {}
        }
    }
    Ok(())
}

fn collect_module(
    module: &ast::Module,
    dir: &Path,
    module_path: &str,
    owner: &Path,
    entries: &mut BTreeSet<String>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    if !is_bare_pub(module) {
        // Items declared in a non-`pub` module are not nameable from outside
        // the crate. Anything re-exported out of it is recorded at the `pub use`
        // that binds the name.
        return Ok(());
    }
    let name = item_name(module.name());
    let child_path = format!("{module_path}::{name}");
    entries.insert(format!("pub mod {child_path}"));

    if let Some(list) = module.item_list() {
        return collect_items(
            list.syntax(),
            &dir.join(&name),
            &child_path,
            owner,
            entries,
            visited,
        );
    }
    let file = resolve_module_file(dir, &name, path_attribute(module).as_deref(), owner)?;
    collect_module_file(&file, &child_path, entries, visited)
}

fn collect_use(use_item: &ast::Use, module_path: &str, entries: &mut BTreeSet<String>) {
    let Some(tree) = use_item.use_tree() else {
        return;
    };
    let mut leaves = Vec::new();
    expand_use_tree(&tree, &[], &mut leaves);
    for leaf in leaves {
        match leaf {
            UseLeaf::Name(name) => {
                entries.insert(format!("pub use {module_path}::{name}"));
            }
            // A syntax walk cannot enumerate what a glob re-exports. Recording
            // the glob keeps it visible to the gate instead of silently
            // widening the surface.
            UseLeaf::Glob => {
                entries.insert(format!("pub use {module_path}::*"));
            }
        }
    }
}

enum UseLeaf {
    Name(String),
    Glob,
}

fn expand_use_tree(tree: &ast::UseTree, prefix: &[String], out: &mut Vec<UseLeaf>) {
    let mut segments = prefix.to_vec();
    if let Some(path) = tree.path() {
        for segment in path.segments() {
            segments.push(segment.syntax().text().to_string());
        }
    }
    if let Some(list) = tree.use_tree_list() {
        for child in list.use_trees() {
            expand_use_tree(&child, &segments, out);
        }
        return;
    }
    if tree.star_token().is_some() {
        out.push(UseLeaf::Glob);
        return;
    }
    if let Some(rename) = tree.rename() {
        let name = match rename.name() {
            Some(name) => name.text().to_string(),
            None => "_".to_string(),
        };
        out.push(UseLeaf::Name(name));
        return;
    }
    let mut reversed = segments.iter().rev();
    let Some(last) = reversed.next() else {
        return;
    };
    if last == "self" {
        // `use foo::bar::{self}` binds `bar`, not `self`.
        if let Some(parent) = reversed.next() {
            out.push(UseLeaf::Name(parent.clone()));
        }
        return;
    }
    out.push(UseLeaf::Name(last.clone()));
}

fn insert_named<T: HasVisibility>(
    entries: &mut BTreeSet<String>,
    item: &T,
    kind: &str,
    module_path: &str,
    name: Option<ast::Name>,
) {
    if !is_bare_pub(item) {
        return;
    }
    entries.insert(format!("pub {kind} {module_path}::{}", item_name(name)));
}

/// A bare `pub` is the only visibility that reaches outside the crate.
/// `pub(crate)`, `pub(super)`, `pub(self)`, and `pub(in path)` all carry a
/// `VisibilityInner` and are rejected.
fn is_bare_pub<T: HasVisibility>(item: &T) -> bool {
    match item.visibility() {
        Some(visibility) => visibility.visibility_inner().is_none(),
        None => false,
    }
}

fn item_name(name: Option<ast::Name>) -> String {
    match name {
        Some(name) => name.text().to_string(),
        // Fail visible rather than silent: an unnamed item still surfaces as an
        // entry the allowlist must account for.
        None => "<unnamed>".to_string(),
    }
}

fn crate_root(module_path: &str) -> &str {
    match module_path.split_once("::") {
        Some((root, _)) => root,
        None => module_path,
    }
}

/// `#[cfg(test)]` items are not part of the published surface. An item whose
/// `cfg` mentions both `test` and `not` is kept, because `#[cfg(not(test))]`
/// items are present in a normal build.
fn is_cfg_test<T: HasAttrs>(item: &T) -> bool {
    for attr in item.attrs() {
        let Some(ast::Meta::CfgMeta(cfg)) = attr.meta() else {
            continue;
        };
        let Some(predicate) = cfg.cfg_predicate() else {
            continue;
        };
        let identifiers = bare_identifiers(predicate.syntax());
        if identifiers.contains("test") && !identifiers.contains("not") {
            return true;
        }
    }
    false
}

fn has_macro_export(item: &ast::MacroRules) -> bool {
    item.attrs().any(|attr| match attr.meta() {
        Some(ast::Meta::PathMeta(meta)) => meta
            .path()
            .is_some_and(|path| path.syntax().text().to_string().trim() == "macro_export"),
        _ => false,
    })
}

/// Identifier tokens only: text inside a string literal (`feature = "latest"`)
/// is a single `STRING` token and never contributes an identifier.
fn bare_identifiers(node: &SyntaxNode) -> BTreeSet<String> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| token.text().to_string())
        .collect()
}

fn path_attribute(module: &ast::Module) -> Option<String> {
    for attr in module.attrs() {
        let Some(ast::Meta::KeyValueMeta(meta)) = attr.meta() else {
            continue;
        };
        let is_path = meta
            .path()
            .is_some_and(|path| path.syntax().text().to_string().trim() == "path");
        if !is_path {
            continue;
        }
        let value = meta.expr()?.syntax().text().to_string();
        return Some(value.trim().trim_matches('"').to_string());
    }
    None
}

/// The directory that owns a file's child modules: `lib.rs`, `main.rs`, and
/// `mod.rs` own their own directory; `foo.rs` owns `foo/`.
fn child_module_dir(file: &Path) -> Result<PathBuf, String> {
    let parent = file
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", file.display()))?;
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("{} has no readable file stem", file.display()))?;
    if matches!(stem, "lib" | "main" | "mod") {
        return Ok(parent.to_path_buf());
    }
    Ok(parent.join(stem))
}

fn resolve_module_file(
    dir: &Path,
    name: &str,
    path_attribute: Option<&str>,
    owner: &Path,
) -> Result<PathBuf, String> {
    if let Some(relative) = path_attribute {
        let candidate = dir.join(relative);
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(format!(
            "{}: `pub mod {name};` has #[path = \"{relative}\"] which does not resolve to {}",
            owner.display(),
            candidate.display()
        ));
    }
    let flat = dir.join(format!("{name}.rs"));
    if flat.is_file() {
        return Ok(flat);
    }
    let nested = dir.join(name).join("mod.rs");
    if nested.is_file() {
        return Ok(nested);
    }
    Err(format!(
        "{}: `pub mod {name};` resolves to neither {} nor {}, so its public surface cannot be collected",
        owner.display(),
        flat.display(),
        nested.display()
    ))
}

#[cfg(test)]
mod tests;
