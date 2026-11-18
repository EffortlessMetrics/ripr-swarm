//! #8361: a concrete receiver for a method stub.
//!
//! A method stub used to open with `let subject: T = todo!("build the T")`
//! even when the owner's own impl shows how to build one. This module picks
//! a construction the test scope is already known to accept, in this order:
//!
//! 1. the first call in the stub's uniqueness scope (the inline test module
//!    the stub is written into) to a constructor of `T` whose arguments are
//!    all literals, copied with the same arguments — an existing `#[test]`
//!    (not `should_panic`, not a ripr stub) has already shown that this exact
//!    call compiles and runs there;
//! 2. a no-argument constructor named `new`, `empty` or `default`, in that
//!    order;
//! 3. `<T as ::core::default::Default>::default()` when `T`, spelled without
//!    type arguments, has `#[derive(Default)]` and the file declares no
//!    namesake `Default` item.
//!
//! A constructor is an associated function of an inherent impl of `T`
//! (sibling of the owner's impl, same module) that takes no `self`, has no
//! type or const generics, is not `async`, `unsafe` or cfg-gated, and returns
//! `Self` or `T`. Anything else keeps the labelled `todo!()`.
//!
//! The receiver is a setup choice, not evidence: the expected value stays a
//! `todo!()`, and the line says where the construction came from so the
//! developer changes it when the case needs another state.

use super::StubScope;
use crate::analysis::syntax::fn_signature::local_type_derives;
use crate::analysis::syntax::{has_test_attribute, parse_clean_source_file};
use ra_ap_syntax::ast::{self, HasArgList, HasAttrs, HasGenericParams, HasName};
use ra_ap_syntax::{AstNode, TextSize};

/// How the stub builds its receiver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReceiverConstruction {
    /// The constructor call, spelled for the test scope (`TokenBucket::new(5)`).
    pub(super) expression: String,
    /// Where the construction came from, for the line comment and the CLI.
    pub(super) origin: ReceiverOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ReceiverOrigin {
    /// Copied from this existing function in the test scope.
    ExistingTest(String),
    /// A constructor that takes no arguments.
    NoArgumentConstructor,
    /// `Default::default()`.
    Default,
}

impl ReceiverConstruction {
    pub(super) fn comment(&self) -> String {
        match &self.origin {
            ReceiverOrigin::ExistingTest(name) => format!(
                "ripr: built like the existing test `{name}`; change it if this case needs another state"
            ),
            ReceiverOrigin::NoArgumentConstructor | ReceiverOrigin::Default => {
                "ripr: built with its no-argument constructor; change it if this case needs another state"
                    .to_string()
            }
        }
    }

    pub(super) fn origin_text(&self) -> String {
        match &self.origin {
            ReceiverOrigin::ExistingTest(name) => format!("as in the existing test `{name}`"),
            ReceiverOrigin::NoArgumentConstructor => "its no-argument constructor".to_string(),
            ReceiverOrigin::Default => "its derived `Default`".to_string(),
        }
    }
}

struct Constructor {
    name: String,
    params: usize,
}

/// The construction for the receiver of the owner at `owner_offset` in
/// `source`, or `None` when no constructor is established. `type_path` is
/// the self type as the test spells it, without generic arguments.
pub(super) fn receiver_construction(
    source: &str,
    owner_offset: usize,
    type_path: &str,
    scope: &StubScope<'_>,
) -> Option<ReceiverConstruction> {
    let parse = parse_clean_source_file(source)?;
    let offset = TextSize::try_from(owner_offset).ok()?;
    let owner = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.syntax().text_range().contains_inclusive(offset))
        .max_by_key(|function| function.syntax().text_range().start())?;
    let owner_impl = owner
        .syntax()
        .parent()
        .and_then(ast::AssocItemList::cast)?
        .syntax()
        .parent()
        .and_then(ast::Impl::cast)?;
    let self_ty = normalized(&owner_impl.self_ty()?.syntax().text().to_string());
    let base_name = self_ty.split('<').next().unwrap_or(&self_ty).to_string();
    if base_name.is_empty() || base_name.contains("::") || base_name.contains('&') {
        return None;
    }
    let container = owner_impl.syntax().parent()?;
    let constructors: Vec<Constructor> = container
        .children()
        .filter_map(ast::Impl::cast)
        .filter(|candidate| {
            candidate.trait_().is_none()
                && lifetime_generics_only(candidate)
                && !has_cfg(candidate.attrs())
                && candidate
                    .self_ty()
                    .is_some_and(|ty| normalized(&ty.syntax().text().to_string()) == self_ty)
        })
        .filter_map(|candidate| candidate.assoc_item_list())
        .flat_map(|items| items.assoc_items())
        .filter_map(|item| match item {
            ast::AssocItem::Fn(function) => Some(function),
            _ => None,
        })
        .filter_map(|function| constructor(&function, &self_ty, &base_name))
        .collect();

    // A scope that declares or imports another item named like the type
    // binds that name in the test module, so neither a copied call nor a
    // fallback would reach the owner's type (#14825 review).
    if scope_shadows(scope, &base_name) {
        return None;
    }
    if let Some(found) = reused_call(scope, &base_name, &constructors) {
        return Some(found.with_path(type_path));
    }
    let no_argument = constructors
        .iter()
        .filter(|candidate| {
            candidate.params == 0 && NEUTRAL_CONSTRUCTORS.contains(&candidate.name.as_str())
        })
        .min_by_key(|candidate| {
            NEUTRAL_CONSTRUCTORS
                .iter()
                .position(|name| *name == candidate.name)
        });
    if let Some(candidate) = no_argument {
        return Some(ReceiverConstruction {
            expression: format!("{type_path}::{}()", candidate.name),
            origin: ReceiverOrigin::NoArgumentConstructor,
        });
    }
    // A generic type's derived `Default` carries bounds on its arguments the
    // stub cannot check, so only a type spelled without arguments qualifies.
    // Only a `#[derive(Default)]` counts, in a file that declares or imports
    // no item of its own named `Default` (a namesake trait has no
    // `default()`). The call names the standard trait in full, so an
    // inherent `default(..)` taking arguments cannot capture it (#14825).
    let has_default = self_ty == base_name
        && !declares_default_namesake(&parse)
        && local_type_derives(source, &base_name)
            .is_some_and(|traits| traits.iter().any(|name| name == "Default"));
    has_default.then(|| ReceiverConstruction {
        expression: format!("<{type_path} as ::core::default::Default>::default()"),
        origin: ReceiverOrigin::Default,
    })
}

/// No-argument constructors whose names promise a neutral value, in
/// preference order. `connect()` or `from_env()` are not neutral setup.
const NEUTRAL_CONSTRUCTORS: [&str; 3] = ["new", "empty", "default"];

struct Reused {
    constructor: String,
    arguments: Vec<String>,
    test: String,
}

impl Reused {
    fn with_path(self, type_path: &str) -> ReceiverConstruction {
        ReceiverConstruction {
            expression: format!(
                "{type_path}::{}({})",
                self.constructor,
                self.arguments.join(", ")
            ),
            origin: ReceiverOrigin::ExistingTest(self.test),
        }
    }
}

/// The first call in the scope to `Type::ctor(literal, ...)` with the
/// constructor's arity, inside a `#[test]` function that is not
/// `should_panic` and is not a ripr stub (#14825 review: reusing a stub's own
/// receiver would change its comment on the next run and break the
/// already-written check), and not under `cfg`.
fn reused_call(
    scope: &StubScope<'_>,
    base_name: &str,
    constructors: &[Constructor],
) -> Option<Reused> {
    if constructors.is_empty() || scope.range.is_empty() {
        return None;
    }
    let parse = parse_clean_source_file(scope.parse_unit)?;
    let start = TextSize::try_from(scope.range.start).ok()?;
    let end = TextSize::try_from(scope.range.end).ok()?;
    let in_scope = |node: &ra_ap_syntax::SyntaxNode| {
        let range = node.text_range();
        range.start() >= start && range.end() <= end
    };
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::CallExpr::cast)
        .filter(|call| {
            let range = call.syntax().text_range();
            range.start() >= start && range.end() <= end
        })
        .find_map(|call| {
            let ast::Expr::PathExpr(callee) = call.expr()? else {
                return None;
            };
            let path = callee.path()?;
            let qualifier = path.qualifier()?;
            if qualifier.qualifier().is_some()
                || normalized(&qualifier.syntax().text().to_string()) != base_name
            {
                return None;
            }
            let name = path.segment()?.name_ref()?.text().to_string();
            let arguments: Vec<String> = call
                .arg_list()?
                .args()
                .map(|arg| literal_argument(&arg))
                .collect::<Option<_>>()?;
            constructors
                .iter()
                .find(|candidate| candidate.name == name && candidate.params == arguments.len())?;
            // A call inside a closure may be one a `catch_unwind` or a lazy
            // combinator never runs to completion; only a call in the test's
            // own body is known to have returned.
            if call
                .syntax()
                .ancestors()
                .take_while(|node| !ast::Fn::can_cast(node.kind()))
                .any(|node| {
                    ast::ClosureExpr::can_cast(node.kind())
                        || ast::BlockExpr::cast(node)
                            .is_some_and(|block| block.async_token().is_some())
                })
            {
                return None;
            }
            // A call under `cfg` (on the test, a statement, or a nested
            // module) may never be compiled, so it proves nothing about the
            // constructor's argument types (#14825 review).
            if call
                .syntax()
                .ancestors()
                .take_while(|node| in_scope(node))
                .filter_map(ast::AnyHasAttrs::cast)
                .any(|owner| has_cfg(owner.attrs()))
            {
                return None;
            }
            let function = call.syntax().ancestors().find_map(ast::Fn::cast)?;
            if !has_test_attribute(&function)
                || function
                    .attrs()
                    .any(|attr| attr_named(&attr, &["should_panic"]))
                || is_ripr_stub(&function)
            {
                return None;
            }
            Some(Reused {
                constructor: name,
                arguments,
                test: function.name()?.text().to_string(),
            })
        })
}

/// Whether the stub scope declares an item, or imports a name, that could
/// bind `base_name` to something other than the owner's type, including
/// through a glob. Only `use super::Type;` and `use super::*;` outside any
/// nested module (the owner sits in that module's parent, since an owner
/// in a nested module is refused earlier) are known to name the owner.
/// An unparsed scope counts as shadowing.
fn scope_shadows(scope: &StubScope<'_>, base_name: &str) -> bool {
    if scope.range.is_empty() {
        return false;
    }
    let Some(parse) = parse_clean_source_file(scope.parse_unit) else {
        return true;
    };
    let (Ok(start), Ok(end)) = (
        TextSize::try_from(scope.range.start),
        TextSize::try_from(scope.range.end),
    ) else {
        return true;
    };
    parse
        .tree()
        .syntax()
        .descendants()
        .filter(|node| {
            let range = node.text_range();
            range.start() >= start && range.end() <= end
        })
        .any(|node| {
            if let Some(rename) = ast::Rename::cast(node.clone()) {
                return rename.name().is_some_and(|name| name.text() == base_name);
            }
            if let Some(tree) = ast::UseTree::cast(node.clone()) {
                let names_base = tree
                    .path()
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|name| name.text() == base_name);
                let glob = tree.star_token().is_some();
                if tree.use_tree_list().is_some() || !(names_base || glob) {
                    return false;
                }
                // A glob can import the name too. Only `use super::*;` or
                // `use super::Type;` in the test module (or a function body
                // in it, as a written stub has) reaches the owner's module;
                // inside a nested module `super` means something else
                // (#14825 review).
                let owner_path = if glob {
                    "super".to_string()
                } else {
                    format!("super::{base_name}")
                };
                let top_level = !tree
                    .syntax()
                    .ancestors()
                    .take_while(|ancestor| {
                        let range = ancestor.text_range();
                        range.start() >= start && range.end() <= end
                    })
                    .any(|ancestor| ast::Module::can_cast(ancestor.kind()));
                return !(tree.rename().is_none()
                    && top_level
                    && full_use_path(&tree) == owner_path);
            }
            ast::Item::cast(node).is_some_and(|item| {
                let name = match &item {
                    ast::Item::Module(it) => it.name(),
                    ast::Item::Struct(it) => it.name(),
                    ast::Item::Enum(it) => it.name(),
                    ast::Item::Union(it) => it.name(),
                    ast::Item::TypeAlias(it) => it.name(),
                    ast::Item::Trait(it) => it.name(),
                    _ => None,
                };
                name.is_some_and(|name| name.text() == base_name)
            })
        })
}

/// A function ripr wrote: its body still holds a `ripr:` fill-in or the
/// receiver comment ripr writes.
fn is_ripr_stub(function: &ast::Fn) -> bool {
    let text = function.syntax().text().to_string();
    text.contains("todo!(\"ripr:") || text.contains("// ripr: built ")
}

/// A literal argument (`5`, `"w"`, `true`, `-3`, `1.5`) as written.
fn literal_argument(arg: &ast::Expr) -> Option<String> {
    // The copied text is printed to the terminal on the `Receiver:` line.
    if arg
        .syntax()
        .text()
        .to_string()
        .chars()
        .any(char::is_control)
    {
        return None;
    }
    match arg {
        ast::Expr::Literal(_) => Some(arg.syntax().text().to_string()),
        ast::Expr::PrefixExpr(prefix)
            if prefix.op_kind() == Some(ast::UnaryOp::Neg)
                && matches!(prefix.expr(), Some(ast::Expr::Literal(_))) =>
        {
            Some(normalized(&arg.syntax().text().to_string()))
        }
        _ => None,
    }
}

fn constructor(function: &ast::Fn, self_ty: &str, base_name: &str) -> Option<Constructor> {
    if function.async_token().is_some()
        || function.unsafe_token().is_some()
        || function.body().is_none()
        || function.generic_param_list().is_some()
        || has_cfg(function.attrs())
    {
        return None;
    }
    let params = function.param_list()?;
    if params.self_param().is_some() {
        return None;
    }
    let returns = normalized(&function.ret_type()?.ty()?.syntax().text().to_string());
    if returns != "Self" && returns != self_ty && returns != base_name {
        return None;
    }
    Some(Constructor {
        name: function.name()?.text().to_string(),
        params: params.params().count(),
    })
}

fn lifetime_generics_only(item: &ast::Impl) -> bool {
    item.generic_param_list().is_none_or(|list| {
        list.generic_params()
            .all(|param| matches!(param, ast::GenericParam::LifetimeParam(_)))
    })
}

fn has_cfg(mut attrs: ast::AstChildren<ast::Attr>) -> bool {
    attrs.any(|attr| attr_named(&attr, &["cfg", "cfg_attr"]))
}

/// Whether the file declares, or imports under that name, any item called
/// `Default`, which would make a derived `Default` ambiguous.
fn declares_default_namesake(parse: &ra_ap_syntax::Parse<ast::SourceFile>) -> bool {
    parse.tree().syntax().descendants().any(|node| {
        if let Some(rename) = ast::Rename::cast(node.clone()) {
            return rename.name().is_some_and(|name| name.text() == "Default");
        }
        if let Some(tree) = ast::UseTree::cast(node.clone()) {
            // `use other::Default;` (a last segment with no rename or glob).
            return tree.rename().is_none()
                && tree.use_tree_list().is_none()
                && tree.star_token().is_none()
                && tree
                    .path()
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|name| name.text() == "Default")
                && !matches!(
                    full_use_path(&tree).trim_start_matches("::"),
                    "std::default::Default" | "core::default::Default"
                );
        }
        ast::Item::cast(node).is_some_and(|item| {
            let name = match &item {
                ast::Item::Trait(it) => it.name(),
                ast::Item::Struct(it) => it.name(),
                ast::Item::Enum(it) => it.name(),
                ast::Item::Union(it) => it.name(),
                ast::Item::TypeAlias(it) => it.name(),
                ast::Item::MacroRules(it) => it.name(),
                ast::Item::Fn(it) => it.name(),
                _ => None,
            };
            name.is_some_and(|name| name.text() == "Default")
        })
    })
}

/// The whole path a use tree imports, joining the prefixes of the trees it
/// is nested in (`use std::{default::Default}` gives `std::default::Default`).
fn full_use_path(tree: &ast::UseTree) -> String {
    let mut segments = Vec::new();
    let mut current = Some(tree.clone());
    while let Some(node) = current {
        if let Some(path) = node.path() {
            segments.push(normalized(&path.syntax().text().to_string()));
        }
        current = node
            .syntax()
            .parent()
            .and_then(ast::UseTreeList::cast)
            .and_then(|list| list.syntax().parent())
            .and_then(ast::UseTree::cast);
    }
    segments.reverse();
    segments.join("::")
}

/// Whether the attribute's single-segment path is one of `names`.
fn attr_named(attr: &ast::Attr, names: &[&str]) -> bool {
    attr.meta()
        .and_then(|meta| meta.simple_name())
        .is_some_and(|name| names.contains(&name.as_str()))
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect()
}
