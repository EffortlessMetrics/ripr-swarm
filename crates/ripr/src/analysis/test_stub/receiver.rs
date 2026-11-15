//! #8361: a concrete receiver for a method stub.
//!
//! A method stub used to open with `let subject: T = todo!("build the T")`
//! even when the owner's own impl shows how to build one. This module picks
//! a construction the test scope is already known to accept, in this order:
//!
//! 1. the first call in the stub's uniqueness scope (the inline test module
//!    the stub is written into) to a constructor of `T` whose arguments are
//!    all literals, copied with the same arguments — an existing test has
//!    already shown that this exact call compiles and runs there;
//! 2. a constructor of `T` that takes no arguments (`new` first);
//! 3. `T::default()` when `T` derives or implements `Default`.
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
use crate::analysis::syntax::fn_signature::local_type_traits;
use crate::analysis::syntax::parse_clean_source_file;
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
            ReceiverOrigin::Default => "its `Default` implementation".to_string(),
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

    if let Some(found) = reused_call(scope, &base_name, &constructors) {
        return Some(found.with_path(type_path));
    }
    let no_argument = constructors
        .iter()
        .filter(|candidate| candidate.params == 0)
        .min_by_key(|candidate| candidate.name != "new");
    if let Some(candidate) = no_argument {
        return Some(ReceiverConstruction {
            expression: format!("{type_path}::{}()", candidate.name),
            origin: ReceiverOrigin::NoArgumentConstructor,
        });
    }
    let has_default = local_type_traits(source, &base_name)
        .is_some_and(|traits| traits.iter().any(|name| name == "Default"));
    has_default.then(|| ReceiverConstruction {
        expression: format!("{type_path}::default()"),
        origin: ReceiverOrigin::Default,
    })
}

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
/// constructor's arity, inside a named function that is not `should_panic`.
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
            let function = call.syntax().ancestors().find_map(ast::Fn::cast)?;
            if function
                .attrs()
                .any(|attr| attr.syntax().text().to_string().contains("should_panic"))
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

/// A literal argument (`5`, `"w"`, `true`, `-3`, `1.5`) as written.
fn literal_argument(arg: &ast::Expr) -> Option<String> {
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
    attrs.any(|attr| {
        let text: String = attr
            .syntax()
            .text()
            .to_string()
            .split_whitespace()
            .collect();
        text.starts_with("#[cfg(") || text.starts_with("#[cfg_attr(")
    })
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect()
}
