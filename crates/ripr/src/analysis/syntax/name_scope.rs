//! Lexical name scopes around one function (#6292, #6537, #6544): which
//! `use` items, globs and item definitions could bind a bare name at that
//! function, innermost scope first.
//!
//! This is syntax, not name resolution. The caller decides what the facts
//! mean and fails closed when they do not settle a single definition.

use super::nesting::parse_clean_source_file;
use super::ra::LineIndex;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasModuleItem, HasName},
};

/// The scopes that could bind `name` inside one function, innermost first:
/// the function body, then each enclosing inline module, then the file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NameScopes {
    pub(crate) scopes: Vec<NameScope>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NameScope {
    /// Inline module names from the file root down to this scope.
    pub(crate) modules: Vec<String>,
    /// A function body rather than a module.
    pub(crate) fn_body: bool,
    /// `use` leaves in this scope whose binding is the name.
    pub(crate) imports: Vec<UseBinding>,
    /// Path prefixes of glob imports (`use a::b::*` records `[a, b]`).
    pub(crate) globs: Vec<Vec<String>>,
    /// An item named the name is declared directly in this scope.
    pub(crate) defines: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UseBinding {
    /// The imported path, its last segment the name itself.
    Path(Vec<String>),
    /// `use x::other as name;`: the binding is not spelled by its path.
    Renamed,
}

/// The name scopes of the function named `fn_name` whose `fn` keyword sits
/// on `fn_line`. `None` when the source does not parse cleanly or holds no
/// such function, so a caller fails closed.
pub(crate) fn name_scopes_for_fn(
    source: &str,
    fn_line: usize,
    fn_name: &str,
    name: &str,
) -> Option<NameScopes> {
    let parse = parse_clean_source_file(source)?;
    let line_index = LineIndex::new(source);
    let tree = parse.tree();
    let function = tree.syntax().descendants().find_map(|node| {
        let function = ast::Fn::cast(node)?;
        let token = function.fn_token()?;
        let named = function.name().is_some_and(|ident| ident.text() == fn_name);
        (named && line_index.line(token.text_range().start()) == fn_line).then_some(function)
    })?;
    let modules_of = |node: &SyntaxNode| {
        let mut modules: Vec<String> = node
            .ancestors()
            .filter_map(ast::Module::cast)
            .filter_map(|module| module.name().map(|ident| ident.text().to_string()))
            .collect();
        modules.reverse();
        modules
    };
    let mut scopes = Vec::new();
    for ancestor in function.syntax().ancestors() {
        if let Some(enclosing_fn) = ast::Fn::cast(ancestor.clone()) {
            let mut scope = NameScope {
                modules: modules_of(&ancestor),
                fn_body: true,
                ..NameScope::default()
            };
            if let Some(body) = enclosing_fn.body() {
                for node in body.syntax().descendants() {
                    if nearest_item_scope(&node).as_ref() != Some(&ancestor) {
                        continue;
                    }
                    if let Some(item) = ast::Item::cast(node) {
                        record_item(&item, name, &mut scope);
                    }
                }
            }
            scopes.push(scope);
        } else if let Some(module) = ast::Module::cast(ancestor.clone()) {
            let mut scope = NameScope {
                modules: modules_of(&ancestor),
                ..NameScope::default()
            };
            // `modules_of` walks from the node itself, so the module's own
            // name is already the last segment.
            if let Some(items) = module.item_list() {
                for item in items.items() {
                    record_item(&item, name, &mut scope);
                }
            }
            scopes.push(scope);
        } else if let Some(file) = ast::SourceFile::cast(ancestor.clone()) {
            let mut scope = NameScope::default();
            for item in file.items() {
                record_item(&item, name, &mut scope);
            }
            scopes.push(scope);
        }
        // An `impl` or `trait` binds nothing a bare call could name; an
        // associated fn's scope continues at its enclosing module.
    }
    Some(NameScopes { scopes })
}

/// The nearest enclosing fn or module of `node`, excluding `node` itself.
fn nearest_item_scope(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.ancestors().skip(1).find(|ancestor| {
        ast::Fn::can_cast(ancestor.kind()) || ast::Module::can_cast(ancestor.kind())
    })
}

fn record_item(item: &ast::Item, name: &str, scope: &mut NameScope) {
    match item {
        ast::Item::Use(item_use) => {
            if let Some(tree) = item_use.use_tree() {
                flatten_use_tree(&tree, &[], name, scope);
            }
        }
        ast::Item::Fn(function) => {
            if function.name().is_some_and(|ident| ident.text() == name) {
                scope.defines = true;
            }
        }
        ast::Item::Const(item) => {
            if item.name().is_some_and(|ident| ident.text() == name) {
                scope.defines = true;
            }
        }
        ast::Item::Static(item) => {
            if item.name().is_some_and(|ident| ident.text() == name) {
                scope.defines = true;
            }
        }
        ast::Item::MacroRules(_) | ast::Item::MacroCall(_) => {
            // A macro may expand to an item of any name; the caller sees
            // only spelled bindings. Item-producing macros are a documented
            // residual of this bounded scan.
        }
        _ => {}
    }
}

fn path_segments(path: &ast::Path) -> Vec<String> {
    path.syntax()
        .text()
        .to_string()
        .split("::")
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn flatten_use_tree(tree: &ast::UseTree, prefix: &[String], name: &str, scope: &mut NameScope) {
    let mut full = prefix.to_vec();
    if let Some(path) = tree.path() {
        full.extend(path_segments(&path));
    }
    if let Some(list) = tree.use_tree_list() {
        for child in list.use_trees() {
            flatten_use_tree(&child, &full, name, scope);
        }
        return;
    }
    if tree.star_token().is_some() {
        scope.globs.push(full);
        return;
    }
    // `a::{self}` binds `a`.
    if full.last().is_some_and(|segment| segment == "self") {
        full.pop();
    }
    let Some(last) = full.last().cloned() else {
        return;
    };
    match tree.rename() {
        Some(rename) => {
            let bound = rename.name().map(|ident| ident.text().to_string());
            if bound.as_deref() == Some(name) {
                if last == name {
                    scope.imports.push(UseBinding::Path(full));
                } else {
                    scope.imports.push(UseBinding::Renamed);
                }
            }
        }
        None => {
            if last == name {
                scope.imports.push(UseBinding::Path(full));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(source: &str, fn_line: usize, fn_name: &str, name: &str) -> NameScopes {
        name_scopes_for_fn(source, fn_line, fn_name, name).unwrap_or_default()
    }

    #[test]
    fn records_scopes_innermost_first_with_imports_globs_and_definitions() {
        let source = "\
pub mod celsius { pub fn snap(v: u32) -> u32 { v } }
#[cfg(test)]
mod tests {
    use super::*;
    use super::celsius::snap;
    use crate::{a::{self}, b::render as draw};
    #[test]
    fn rounds() {
        use super::other::snap as snap;
        assert_eq!(snap(1), 1);
    }
}
";
        let found = scopes(source, 8, "rounds", "snap");
        assert_eq!(found.scopes.len(), 3, "fn body, mod tests, file root");
        assert!(found.scopes[0].fn_body);
        assert_eq!(found.scopes[0].modules, vec!["tests".to_string()]);
        assert_eq!(
            found.scopes[0].imports,
            vec![UseBinding::Path(vec![
                "super".to_string(),
                "other".to_string(),
                "snap".to_string()
            ])]
        );
        assert!(!found.scopes[1].fn_body);
        assert_eq!(found.scopes[1].modules, vec!["tests".to_string()]);
        assert_eq!(
            found.scopes[1].imports,
            vec![UseBinding::Path(vec![
                "super".to_string(),
                "celsius".to_string(),
                "snap".to_string()
            ])]
        );
        assert_eq!(found.scopes[1].globs, vec![vec!["super".to_string()]]);
        assert!(!found.scopes[1].defines);
        assert!(found.scopes[2].modules.is_empty());
        assert!(!found.scopes[2].defines, "the root declares no fn snap");

        let renamed = scopes(source, 8, "rounds", "draw");
        assert_eq!(renamed.scopes[1].imports, vec![UseBinding::Renamed]);
        let self_import = scopes(source, 8, "rounds", "a");
        assert_eq!(
            self_import.scopes[1].imports,
            vec![UseBinding::Path(vec!["crate".to_string(), "a".to_string()])]
        );
    }

    #[test]
    fn a_module_item_of_the_name_is_a_definition_of_that_scope() {
        let source = "\
pub fn delay() -> u32 { 5 }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warm() { assert_eq!(delay(), 5); }
}
";
        let found = scopes(source, 6, "warm", "delay");
        assert_eq!(found.scopes.len(), 3);
        assert!(!found.scopes[1].defines);
        assert!(found.scopes[2].defines);
    }

    #[test]
    fn unparseable_or_missing_function_yields_none() {
        assert!(name_scopes_for_fn("fn broken( {", 1, "broken", "x").is_none());
        assert!(name_scopes_for_fn("fn present() {}", 1, "absent", "x").is_none());
        assert!(name_scopes_for_fn("fn present() {}", 2, "present", "x").is_none());
    }
}
