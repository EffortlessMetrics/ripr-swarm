//! Refusal premises for rule 7's primitive *type*, not value, namespace.
//! This does not resolve arbitrary aliases, re-exports or qualified types.

use super::{FunctionSummary, RustIndex, primitive_type_token};
use crate::analysis::syntax::{
    TRUSTED_MACRO_NAMES, UseBinding, name_scope_for_items, parse_clean_source_file,
    standard_derive_names, trusted_macro_binding_sites,
};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasName},
};
use std::collections::BTreeSet;

fn same_name(left: &str, right: &str) -> bool {
    left.trim_start_matches("r#") == right.trim_start_matches("r#")
}

pub(super) enum TypeSite {
    Signature,
    Annotation(usize),
    Cast(usize),
}

fn type_name(item: &ast::Item) -> Option<ast::Name> {
    match item {
        ast::Item::Struct(item) => item.name(),
        ast::Item::Enum(item) => item.name(),
        ast::Item::Union(item) => item.name(),
        ast::Item::Trait(item) => item.name(),
        ast::Item::TypeAlias(item) => item.name(),
        ast::Item::Module(item) => item.name(),
        // Functions, constants and statics do not bind the type namespace.
        _ => None,
    }
}

fn function(owner: &FunctionSummary, index: &RustIndex) -> Option<ast::Fn> {
    let facts = index.files().get(&owner.file)?;
    if facts.used_lexical_fallback {
        return None;
    }
    let parse = parse_clean_source_file(&facts.source)?;
    let mut matches = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| {
            function
                .name()
                .is_some_and(|name| same_name(name.text(), &owner.name))
                && function.fn_token().is_some_and(|token| {
                    facts.source[..usize::from(token.text_range().start())]
                        .matches('\n')
                        .count()
                        + 1
                        == owner.start_line
                })
        })
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches.remove(0))
}

fn enclosing_scopes(site: &SyntaxNode) -> Vec<SyntaxNode> {
    site.ancestors()
        .filter(|node| {
            matches!(
                node.kind(),
                SyntaxKind::SOURCE_FILE | SyntaxKind::ITEM_LIST | SyntaxKind::STMT_LIST
            )
        })
        .collect()
}

/// Preserve the separate whole-owner opaque-expansion refusal. Type binding
/// identity below uses only ancestors of the actual annotation/cast instead.
fn context_scopes(function: &ast::Fn) -> Vec<SyntaxNode> {
    let mut scopes = enclosing_scopes(function.syntax());
    if let Some(body) = function.body() {
        scopes.extend(body.syntax().descendants().filter(|node| {
            ast::StmtList::can_cast(node.kind())
                && node.ancestors().find_map(ast::Fn::cast).as_ref() == Some(function)
                && node
                    .ancestors()
                    .take_while(|ancestor| ancestor != function.syntax())
                    .all(|ancestor| !ast::Module::can_cast(ancestor.kind()))
        }));
    }
    scopes
}

fn type_site(function: &ast::Fn, ty: &str, site: TypeSite) -> Option<SyntaxNode> {
    let (offset, annotation) = match site {
        TypeSite::Signature => return Some(function.syntax().clone()),
        TypeSite::Annotation(offset) => (offset, true),
        TypeSite::Cast(offset) => (offset, false),
    };
    let absolute = usize::from(function.fn_token()?.text_range().start()).checked_add(offset)?;
    let node = function.syntax().descendants().find_map(|node| {
        let actual = if annotation {
            let binding = ast::LetStmt::cast(node)?;
            if usize::from(binding.initializer()?.syntax().text_range().start()) != absolute {
                return None;
            }
            binding.ty()?
        } else {
            let cast = ast::CastExpr::cast(node)?;
            let actual = cast.ty()?;
            if usize::from(actual.syntax().text_range().start()) != absolute {
                return None;
            }
            actual
        };
        (actual.syntax().text().to_string().trim() == ty.trim()).then(|| actual.syntax().clone())
    })?;
    // A lexical let scan must not borrow a nested function's type context.
    (node.ancestors().find_map(ast::Fn::cast).as_ref() == Some(function)).then_some(node)
}

fn items(scope: &SyntaxNode) -> Vec<ast::Item> {
    scope.children().filter_map(ast::Item::cast).collect()
}

/// The shared macro authority supplies scoped imports/definitions, opaque
/// macro preludes and expansion bindings; derive spelling alone proves nothing.
fn standard_derive_binding_holds(attr: &ast::Attr) -> bool {
    let Some(names) = standard_derive_names(attr) else {
        return false;
    };
    // The macro scan deliberately leaves crate/self/super globs to its caller.
    // This bounded type proof does not resolve names imported by those globs.
    if enclosing_scopes(attr.syntax())
        .iter()
        .any(|scope| !name_scope_for_items(&items(scope)).globs.is_empty())
    {
        return false;
    }
    let Some(file) = attr.syntax().ancestors().last() else {
        return false;
    };
    let source = file.text().to_string();
    let Some(prefix) = source.get(..usize::from(attr.syntax().text_range().start())) else {
        return false;
    };
    let line = prefix.matches('\n').count() + 1;
    let trusted = TRUSTED_MACRO_NAMES
        .iter()
        .copied()
        .chain(["derive"])
        .chain(names.iter().map(String::as_str))
        .collect::<Vec<_>>();
    !trusted_macro_binding_sites(&source, &BTreeSet::new(), &trusted, &|_, _| false, &|_| {
        false
    })
    .iter()
    .any(|(name, site)| {
        (name == "derive" || names.contains(name))
            && site
                .scope
                .is_none_or(|(start, end)| start <= line && line <= end)
    })
}

fn derives_have_root_context(owner: &FunctionSummary, index: &RustIndex) -> bool {
    // File-local macro facts do not establish a prelude inherited from another
    // source file, or a Cargo-rebound std/core root.
    super::super::owner_pin::target_root(&owner.file, index, &Default::default()).as_ref()
        == Some(&owner.file)
        && ["std", "core"].iter().all(|root| {
            index
                .drop_in_manifests
                .unbound_standard_root(&owner.file, root)
        })
}

fn closed_scope(scope: &SyntaxNode, derives: bool) -> bool {
    let preserves = |attr: ast::Attr| {
        let text = attr.syntax().text().to_string();
        crate::analysis::facts::cfg_predicates::attribute_preserves_source_items(&text)
            || crate::analysis::facts::cfg_predicates::attribute_is_plain_test_gate(&text)
            || derives && standard_derive_binding_holds(&attr)
    };
    scope.children().all(|node| {
        if let Some(attr) = ast::Attr::cast(node.clone()) {
            return preserves(attr);
        }
        if let Some(item) = ast::Item::cast(node.clone()) {
            return !matches!(
                item,
                ast::Item::MacroRules(_) | ast::Item::MacroDef(_) | ast::Item::MacroCall(_)
            ) && item
                .syntax()
                .children()
                .filter_map(ast::Attr::cast)
                .all(preserves);
        }
        // A statement macro may introduce a local type or value binding.
        !node
            .descendants()
            .any(|node| ast::MacroCall::can_cast(node.kind()))
    })
}

pub(super) fn context_is_closed(owner: &FunctionSummary, index: &RustIndex) -> bool {
    let derives = derives_have_root_context(owner, index);
    function(owner, index).is_some_and(|function| {
        context_scopes(&function)
            .iter()
            .all(|scope| closed_scope(scope, derives))
    })
}

/// A bare primitive spelling is insufficient when a type declaration,
/// generic binder, import, glob or opaque scope may take its type name.
pub(super) fn type_is_primitive(
    ty: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    site: TypeSite,
) -> bool {
    if !primitive_type_token(ty) {
        return false;
    }
    let declared = ty;
    let ty = ty.trim().strip_prefix("&mut ").unwrap_or(ty.trim());
    let ty = ty.strip_prefix('&').unwrap_or(ty).trim();
    let Some(function) = function(owner, index) else {
        return false;
    };
    let Some(site) = type_site(&function, declared, site) else {
        return false;
    };
    // Fn/impl/trait type parameters bind the type namespace. Const generics
    // and value parameters of the same name do not.
    if function.syntax().ancestors().any(|ancestor| {
        ancestor
            .children()
            .filter_map(ast::GenericParamList::cast)
            .any(|list| {
                list.syntax()
                    .children()
                    .filter_map(ast::TypeParam::cast)
                    .any(|parameter| {
                        parameter
                            .name()
                            .is_some_and(|name| same_name(name.text(), ty))
                    })
            })
    }) {
        return false;
    }
    let scopes = enclosing_scopes(&site);
    let derives = derives_have_root_context(owner, index);
    if scopes.iter().any(|scope| !closed_scope(scope, derives)) {
        return false;
    }
    let scope_items = scopes.iter().map(items).collect::<Vec<_>>();
    if scope_items
        .iter()
        .flatten()
        .any(|item| type_name(item).is_some_and(|name| same_name(name.text(), ty)))
    {
        // Even an alias whose terminal happens to be primitive is unestablished
        // here. No alias resolver is inferred from a primitive-looking name.
        return false;
    }
    let standard_primitive = |path: &[String], glob: bool| {
        let [root, namespace, rest @ ..] = path else {
            return false;
        };
        if !matches!(root.as_str(), "std" | "core")
            || namespace != "primitive"
            || if glob {
                !rest.is_empty()
            } else {
                !matches!(rest, [name] if primitive_type_token(name))
            }
            || !index
                .drop_in_manifests
                .unbound_standard_root(&owner.file, root)
            || super::super::owner_pin::target_root(&owner.file, index, &Default::default())
                .as_ref()
                != Some(&owner.file)
        {
            return false;
        }
        // This is the existing closed manifest AND target-root proof plus
        // source binding refusal, not a spelling exception. Out-of-line
        // files cannot establish a Rust 2015 crate-root import's bindings.
        // Absolute/opaque paths refuse too.
        !scope_items.iter().any(|items| {
            let scope = name_scope_for_items(items);
            items.iter().any(|item| type_name(item).is_some_and(|name| same_name(name.text(), root)))
                || scope.imports.iter().any(|import| same_name(&import.bound, root))
                || scope.globs.iter().any(|glob| glob.as_ref().is_none_or(|path| {
                    !matches!(path.as_slice(), [r, p] if matches!(r.as_str(), "std" | "core") && p == "primitive")
                }))
        })
    };
    scope_items.iter().all(|items| {
        let scope = name_scope_for_items(items);
        scope
            .imports
            .iter()
            .filter(|import| same_name(&import.bound, ty))
            .all(|import| match &import.binding {
                UseBinding::Path(path) | UseBinding::Renamed(path) => {
                    standard_primitive(path, false)
                }
                UseBinding::Opaque => false,
            })
            && scope.globs.iter().all(|glob| {
                glob.as_ref()
                    .is_some_and(|path| standard_primitive(path, true))
            })
    })
}
