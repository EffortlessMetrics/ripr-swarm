//! Lexical name scopes around one function (#6292, #6537, #6544): which
//! `use` items, globs, module declarations and item definitions could bind
//! a name at that function, innermost scope first.
//!
//! This is syntax, not name resolution. The caller decides what the facts
//! mean and fails closed when they do not settle a single definition.
//!
//! A function body records only the items directly in its statement list.
//! An item inside a nested block (`{ use crate::b::render; .. }`, a closure
//! body, an `if` arm) binds its name only in that block; the scan does not
//! track which calls sit in that block, so it records the name as
//! block-bound and the caller treats every call of it in the function as
//! unsettled. An item inside a nested `fn` (or a module nested in the body)
//! is block-bound the same way.

#[cfg(test)]
use super::nesting::parse_clean_source_file;
use super::ra::LineIndex;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasGenericArgs, HasGenericParams, HasModuleItem, HasName},
};
use std::collections::BTreeSet;

/// The scopes around one function, innermost first: the function body (and
/// any enclosing function bodies), then each enclosing inline module, then
/// the file.
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
    /// `use` and `extern crate` leaves in this scope, by the name each binds.
    pub(crate) imports: Vec<UseImport>,
    /// Path prefixes of glob imports (`use a::b::*` records `[a, b]`);
    /// `None` for a prefix the scan cannot read.
    pub(crate) globs: Vec<Option<Vec<String>>>,
    /// Names of non-module items declared directly in this scope (`fn`,
    /// `const`, `static`, types, traits).
    pub(crate) defines: BTreeSet<String>,
    /// Names of `mod` items declared directly in this scope.
    pub(crate) child_modules: BTreeSet<String>,
    /// Names an item inside a nested block of this function body binds.
    pub(crate) block_bindings: BTreeSet<String>,
    /// A nested block of this function body holds a glob import, which
    /// may bind any name there.
    pub(crate) block_globs: bool,
    /// Original names a renaming `use` inside a nested block of this
    /// function body imports under another name (`use a::render as fast;`
    /// records `render`).
    pub(crate) block_rename_targets: BTreeSet<String>,
}

impl NameScope {
    /// The `use` bindings of `name` in this scope.
    pub(crate) fn imports_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a UseBinding> {
        self.imports
            .iter()
            .filter(move |import| import.bound == name)
            .map(|import| &import.binding)
    }

    /// Whether a nested block of this function body may bind `name`.
    pub(crate) fn block_binds(&self, name: &str) -> bool {
        self.block_globs || self.block_bindings.contains(name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UseImport {
    /// The name the leaf binds in its scope.
    pub(crate) bound: String,
    pub(crate) binding: UseBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UseBinding {
    /// The imported path, its last segment the bound name itself.
    Path(Vec<String>),
    /// `use x::other as name;`: the imported path, whose last segment is
    /// not the bound name.
    Renamed(Vec<String>),
    /// A binding whose path the scan cannot read (`extern crate`, a leading
    /// `::`, an unreadable segment).
    Opaque,
}

/// Declared type-parameter names share the type/module path namespace. This
/// records names only; it does not resolve their bounds or inferred arguments.
pub(crate) fn function_type_param_names(function: &ast::Fn) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(parameters) = function.generic_param_list() {
        for parameter in parameters.generic_params() {
            if let ast::GenericParam::TypeParam(parameter) = parameter
                && let Some(name) = parameter.name()
            {
                names.insert(name.text().trim_start_matches("r#").to_string());
            }
        }
    }
    names
}

/// The name scopes of the function named `fn_name` whose `fn` keyword sits
/// on `fn_line`. `None` when the source does not parse cleanly or holds no
/// such function, so a caller fails closed.
#[cfg(test)]
pub(crate) fn name_scopes_for_fn(
    source: &str,
    fn_line: usize,
    fn_name: &str,
) -> Option<NameScopes> {
    let parse = parse_clean_source_file(source)?;
    let line_index = LineIndex::new(source);
    let tree = parse.tree();
    name_scopes_in_tree(&tree, &line_index, fn_line, fn_name)
}

pub(super) fn name_scopes_in_tree(
    tree: &ast::SourceFile,
    line_index: &LineIndex,
    fn_line: usize,
    fn_name: &str,
) -> Option<NameScopes> {
    let mut functions = tree.syntax().descendants().filter_map(|node| {
        let function = ast::Fn::cast(node)?;
        let token = function.fn_token()?;
        let named = function.name().is_some_and(|ident| ident.text() == fn_name);
        (named && line_index.line(token.text_range().start()) == fn_line).then_some(function)
    });
    let function = functions.next()?;
    // A function summary's name/line cannot identify one of two same-named
    // declarations on that line. Never borrow the first declaration's scope.
    if functions.next().is_some() {
        return None;
    }
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
                defines: function_type_param_names(&enclosing_fn),
                ..NameScope::default()
            };
            if let Some(body) = enclosing_fn.body() {
                let top_level = body.stmt_list().map(|list| list.syntax().clone());
                let inner = (ancestor != *function.syntax()).then(|| function.syntax());
                for node in body.syntax().descendants() {
                    let Some(item) = ast::Item::cast(node.clone()) else {
                        continue;
                    };
                    if nearest_item_scope(&node).as_ref() == Some(&ancestor) {
                        if node.parent() == top_level {
                            record_item(&item, &mut scope);
                        } else {
                            record_block_item(&item, &mut scope);
                        }
                    } else if !inner
                        .is_some_and(|inner| inner.text_range().contains_range(node.text_range()))
                    {
                        // An item inside a nested fn (or a module nested in
                        // the body) binds only there, but the caller scans
                        // the whole body's calls: it is block-bound. The
                        // test fn's own items belong to its inner scope.
                        record_block_item(&item, &mut scope);
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
                    record_item(&item, &mut scope);
                }
            }
            scopes.push(scope);
        } else if let Some(file) = ast::SourceFile::cast(ancestor.clone()) {
            let mut scope = NameScope::default();
            for item in file.items() {
                record_item(&item, &mut scope);
            }
            scopes.push(scope);
        }
        // An `impl` or `trait` binds nothing a path call could name; an
        // associated fn's scope continues at its enclosing module.
    }
    Some(NameScopes { scopes })
}

/// Record an item that binds its name only in a nested block or nested fn
/// of the function body `scope`: block-bound, never the body's own import.
fn record_block_item(item: &ast::Item, scope: &mut NameScope) {
    let mut block = NameScope::default();
    record_item(item, &mut block);
    scope.block_globs |= !block.globs.is_empty();
    for import in block.imports {
        if let UseBinding::Renamed(path) = &import.binding
            && let Some(original) = path.last()
        {
            scope.block_rename_targets.insert(original.clone());
        }
        scope.block_bindings.insert(import.bound);
    }
    scope.block_bindings.extend(block.defines);
    scope.block_bindings.extend(block.child_modules);
}

/// The nearest enclosing fn or module of `node`, excluding `node` itself.
fn nearest_item_scope(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.ancestors().skip(1).find(|ancestor| {
        ast::Fn::can_cast(ancestor.kind()) || ast::Module::can_cast(ancestor.kind())
    })
}

fn record_item(item: &ast::Item, scope: &mut NameScope) {
    let mut define = |name: Option<ast::Name>| {
        if let Some(name) = name {
            scope.defines.insert(name.text().to_string());
        }
    };
    match item {
        ast::Item::Use(item_use) => {
            if let Some(tree) = item_use.use_tree() {
                flatten_use_tree(&tree, Some(Vec::new()), scope);
            }
        }
        ast::Item::ExternCrate(item) => {
            let bound = match item.rename() {
                Some(rename) => rename.name().map(|ident| ident.text().to_string()),
                None => item.name_ref().map(|name| name.text().to_string()),
            };
            if let Some(bound) = bound {
                scope.imports.push(UseImport {
                    bound,
                    binding: UseBinding::Opaque,
                });
            }
        }
        ast::Item::Module(module) => {
            if let Some(name) = module.name() {
                scope.child_modules.insert(name.text().to_string());
            }
        }
        ast::Item::Fn(item) => define(item.name()),
        ast::Item::Const(item) => define(item.name()),
        ast::Item::Static(item) => define(item.name()),
        ast::Item::Struct(item) => define(item.name()),
        ast::Item::Enum(item) => define(item.name()),
        ast::Item::Union(item) => define(item.name()),
        ast::Item::Trait(item) => define(item.name()),
        ast::Item::TypeAlias(item) => define(item.name()),
        ast::Item::MacroRules(_) | ast::Item::MacroCall(_) => {
            // A macro may expand to an item of any name; the caller sees
            // only spelled bindings. Item-producing macros are a documented
            // residual of this bounded scan.
        }
        _ => {}
    }
}

/// The path's segments through the syntax tree, outermost first. `None`
/// for a path the scan cannot read as plain module segments: a leading
/// `::`, a generic argument list, a `<T as Tr>` anchor, or a segment with
/// no name.
pub(crate) fn path_segments(path: &ast::Path) -> Option<Vec<String>> {
    let mut segments = Vec::new();
    for (position, segment) in path.segments().enumerate() {
        if (position == 0 && segment.coloncolon_token().is_some())
            || segment.generic_arg_list().is_some()
            || segment.type_anchor().is_some()
        {
            return None;
        }
        segments.push(segment.name_ref()?.text().to_string());
    }
    (!segments.is_empty()).then_some(segments)
}

/// `prefix` is `None` once an enclosing tree's path could not be read: its
/// leaves still bind their names, but opaquely.
fn flatten_use_tree(tree: &ast::UseTree, prefix: Option<Vec<String>>, scope: &mut NameScope) {
    let mut full = prefix;
    let mut last_text = None;
    if let Some(path) = tree.path() {
        last_text = path
            .segment()
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string());
        full = match (full, path_segments(&path)) {
            (Some(mut full), Some(segments)) => {
                full.extend(segments);
                Some(full)
            }
            _ => None,
        };
    }
    if let Some(list) = tree.use_tree_list() {
        for child in list.use_trees() {
            flatten_use_tree(&child, full.clone(), scope);
        }
        return;
    }
    if tree.star_token().is_some() {
        scope.globs.push(full);
        return;
    }
    // `a::{self}` binds `a`.
    if let Some(path) = full.as_mut()
        && path.last().is_some_and(|segment| segment == "self")
    {
        path.pop();
        last_text = path.last().cloned();
    }
    let renamed_to = tree
        .rename()
        .map(|rename| rename.name().map(|ident| ident.text().to_string()));
    let bound = match &renamed_to {
        // `use x as _;` binds no name.
        Some(name) => name.clone(),
        None => last_text.clone(),
    };
    let Some(bound) = bound else {
        return;
    };
    let binding = match full {
        Some(path) if path.last() == Some(&bound) => UseBinding::Path(path),
        Some(path) if !path.is_empty() => UseBinding::Renamed(path),
        _ => UseBinding::Opaque,
    };
    scope.imports.push(UseImport { bound, binding });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(source: &str, fn_line: usize, fn_name: &str) -> NameScopes {
        name_scopes_for_fn(source, fn_line, fn_name).unwrap_or_default()
    }

    fn strings(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|s| s.to_string()).collect()
    }

    fn imports_of(scope: &NameScope, name: &str) -> Vec<UseBinding> {
        scope.imports_of(name).cloned().collect()
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
        let found = scopes(source, 8, "rounds");
        assert_eq!(found.scopes.len(), 3, "fn body, mod tests, file root");
        assert!(found.scopes[0].fn_body);
        assert_eq!(found.scopes[0].modules, vec!["tests".to_string()]);
        assert_eq!(
            imports_of(&found.scopes[0], "snap"),
            vec![UseBinding::Path(strings(&["super", "other", "snap"]))]
        );
        assert!(!found.scopes[0].block_binds("snap"));
        assert!(!found.scopes[1].fn_body);
        assert_eq!(found.scopes[1].modules, vec!["tests".to_string()]);
        assert_eq!(
            imports_of(&found.scopes[1], "snap"),
            vec![UseBinding::Path(strings(&["super", "celsius", "snap"]))]
        );
        assert_eq!(found.scopes[1].globs, vec![Some(strings(&["super"]))]);
        assert!(!found.scopes[1].defines.contains("snap"));
        assert!(found.scopes[2].modules.is_empty());
        assert!(
            found.scopes[2].child_modules.contains("celsius"),
            "the root declares `mod celsius`"
        );
        assert!(!found.scopes[2].defines.contains("snap"));

        assert_eq!(
            imports_of(&found.scopes[1], "draw"),
            vec![UseBinding::Renamed(strings(&["crate", "b", "render"]))]
        );
        assert_eq!(
            imports_of(&found.scopes[1], "a"),
            vec![UseBinding::Path(strings(&["crate", "a"]))]
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
        let found = scopes(source, 6, "warm");
        assert_eq!(found.scopes.len(), 3);
        assert!(!found.scopes[1].defines.contains("delay"));
        assert!(found.scopes[2].defines.contains("delay"));
        assert!(found.scopes[1].child_modules.is_empty());
        assert!(found.scopes[2].child_modules.contains("tests"));
    }

    /// Review finding: a `use` inside a nested block binds its name only in
    /// that block, so it is recorded as block-bound, never as the function
    /// body's own import. The same name imported at the body's top level is
    /// an ordinary import (control).
    #[test]
    fn a_block_local_use_is_block_bound_not_function_wide() {
        let source = "\
#[test]
fn mixed() {
    use crate::a::render;
    {
        use crate::b::render;
        assert_eq!(render(2), 4);
    }
    let check = || {
        use crate::c::*;
    };
    assert_eq!(render(1), 2);
}
";
        let found = scopes(source, 2, "mixed");
        let body = &found.scopes[0];
        assert!(body.fn_body);
        assert_eq!(
            imports_of(body, "render"),
            vec![UseBinding::Path(strings(&["crate", "a", "render"]))],
            "only the top-level import is the body's own"
        );
        assert!(body.block_bindings.contains("render"));
        assert!(body.block_globs, "the closure's glob is block-bound");
        assert!(body.block_binds("render"));

        let top_only = "\
#[test]
fn top() {
    use crate::a::render;
    assert_eq!(render(1), 2);
}
";
        let found = scopes(top_only, 2, "top");
        assert!(!found.scopes[0].block_binds("render"));
        assert_eq!(
            imports_of(&found.scopes[0], "render"),
            vec![UseBinding::Path(strings(&["crate", "a", "render"]))]
        );
    }

    /// Review finding: a nested fn's own `use` binds only inside that fn,
    /// yet its calls are part of the test body's text, so it is
    /// block-bound; a block-local rename records the name it renames.
    #[test]
    fn a_nested_fn_item_and_a_block_rename_are_block_bound() {
        let source = "\
mod tests {
    use crate::b::render;
    #[test]
    fn t() {
        fn via_a(x: i32) -> i32 {
            use crate::a::render;
            render(x)
        }
        {
            use crate::a::draw as fast;
        }
        assert_eq!(via_a(1), 2);
    }
}
";
        let found = scopes(source, 4, "t");
        let body = &found.scopes[0];
        assert!(body.fn_body);
        assert!(body.defines.contains("via_a"));
        assert!(imports_of(body, "render").is_empty());
        assert!(
            body.block_binds("render"),
            "the nested fn's `use` is block-bound"
        );
        assert!(body.block_bindings.contains("fast"));
        assert!(body.block_rename_targets.contains("draw"));
        assert!(!body.block_rename_targets.contains("render"));

        // From the nested fn itself, its `use` is its own top-level import,
        // and the enclosing test body does not re-count it as block-bound.
        let inner = scopes(source, 5, "via_a");
        assert_eq!(
            imports_of(&inner.scopes[0], "render"),
            vec![UseBinding::Path(strings(&["crate", "a", "render"]))]
        );
        assert!(inner.scopes[1].fn_body);
        assert!(!inner.scopes[1].block_bindings.contains("render"));
    }

    /// Segments come from the syntax tree: spacing around `::` does not
    /// matter, and a leading `::` or `extern crate` binds opaquely.
    #[test]
    fn use_paths_are_read_through_the_syntax_tree() {
        let source = "\
mod tests {
    use crate :: b :: render;
    use ::other::render as other_render;
    extern crate dep as a;
    use ::glob::*;
    fn t() {}
}
";
        let found = scopes(source, 6, "t");
        let tests = &found.scopes[1];
        assert_eq!(
            imports_of(tests, "render"),
            vec![UseBinding::Path(strings(&["crate", "b", "render"]))]
        );
        assert_eq!(imports_of(tests, "other_render"), vec![UseBinding::Opaque]);
        assert_eq!(imports_of(tests, "a"), vec![UseBinding::Opaque]);
        assert_eq!(tests.globs, vec![None]);
    }

    #[test]
    fn unparseable_or_missing_function_yields_none() {
        assert!(name_scopes_for_fn("fn broken( {", 1, "broken").is_none());
        assert!(name_scopes_for_fn("fn present() {}", 1, "absent").is_none());
        assert!(name_scopes_for_fn("fn present() {}", 2, "present").is_none());
    }
    #[test]
    fn same_named_functions_on_one_line_have_no_unique_scope() {
        let source = "mod left { fn check() {} } mod right { fn check() {} }";
        assert!(name_scopes_for_fn(source, 1, "check").is_none());
    }
    #[test]
    fn function_type_parameters_refuse_module_name_borrowing() {
        let source = "fn check<'a, helpers, const N: usize>() {}";
        let scopes = name_scopes_for_fn(source, 1, "check");
        assert!(
            scopes.as_ref().is_some_and(|scopes| scopes
                .scopes
                .first()
                .is_some_and(
                    |scope| scope.defines.contains("helpers") && !scope.defines.contains("N")
                ))
        );
    }
}
