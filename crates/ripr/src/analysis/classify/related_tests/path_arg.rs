//! An inherent `Type::method` path handed to a helper that runs it (#11199).
//!
//! ```text
//! fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {
//!     assert_eq!(f(&Pricer, amount), want);
//! }
//! #[test] fn discount_cases() { case(Pricer::discount, 150, 140); }
//! ```
//!
//! The test never writes `discount(`, so no call fact or body scan names the
//! owner, yet the helper runs it. This is a separate reach source from a call:
//! it grants the helper-reach relation (`helper_owner_call`), never
//! `direct_owner_call`, because the test does not call the owner itself, and
//! the owner is run by the helper only through the function-typed argument.
//!
//! Every link must be established from syntax, and each gap fails closed (no
//! relation, never a guessed one):
//!
//! - the owner is a method of an inherent `impl` whose self type is a plain
//!   identifier, and no other definition of the same name could be the target
//!   of that type path (a trait default method or a trait impl method is not
//!   named by this rule: `Type::method` would resolve through the trait);
//! - the test's file holds the owner's `impl` as a root item, nothing in the
//!   file rebinds the type name (`use`, glob, `type`), the test module does not
//!   shadow the type, and the test body declares no item and no generic
//!   parameter that could rebind it;
//! - the path is spelled exactly `Type::method` (no extra qualifier, no
//!   turbofish), as a direct argument of a single-segment call in the test;
//! - that callee is one same-file, workspace-unique free function the test
//!   does not shadow, resolved by the established same-file helper identity;
//! - the helper unconditionally invokes the parameter at that argument
//!   position (see `unconditionally_invoked_param_positions`): not under a
//!   branch, loop, closure or `async`, and the parameter is a plain immutable
//!   binding nothing rebinds. A helper that only stores or forwards the
//!   function grants nothing.
//!
//! An unrelated type's same-named method, an unresolvable type head, a bound
//! generic or `impl Trait` argument, and a path handed to a callee in another
//! file or module all stay unrelated. The relation grants no oracle: whether
//! anything observes the result is judged exactly as for any other related
//! test.

use super::super::owner_pin::function_declaration_is_async;
use super::{
    FunctionSummary, RustIndex, TestSummary, file_rebinds_type, impl_identity_is_shadowed,
    impl_self_type_name, is_ident_byte, node_executes_in, owner_impl_is_root_item_of,
};
use crate::analysis::facts::{FunctionContainer, FunctionImplContext};
use ra_ap_syntax::{AstNode, ast, ast::HasArgList};

/// An owner that a `Type::method` path value can name, established once per
/// probe.
pub(super) struct PathArgOwner<'o> {
    owner: &'o FunctionSummary,
    impl_type: String,
    /// `Type::method` without whitespace.
    path: String,
}

impl<'o> PathArgOwner<'o> {
    /// `None` unless the owner is a method of an inherent `impl` of a plain
    /// named type and no other same-named definition could be the target of
    /// `Type::method`.
    pub(super) fn new(
        owner: &'o FunctionSummary,
        same_name_definitions: &[&FunctionSummary],
    ) -> Option<Self> {
        if !matches!(owner.item.container, FunctionContainer::Inherent { .. }) {
            return None;
        }
        let impl_type = impl_self_type_name(&owner.id.0)?;
        if impl_type.is_empty() || !impl_type.bytes().all(is_ident_byte) {
            return None;
        }
        match &owner.impl_context {
            FunctionImplContext::Impl { self_type } if *self_type == impl_type => {}
            _ => return None,
        }
        // A second definition `Type::method` could name (another impl block,
        // a same-named type in another module, or any definition whose
        // context is not established) makes the path ambiguous.
        if same_name_definitions.iter().any(|definition| {
            definition.id != owner.id
                && definition
                    .impl_context
                    .may_be_target_of_type_path(&impl_type)
        }) {
            return None;
        }
        let path = format!("{impl_type}::{}", owner.name);
        Some(Self {
            owner,
            impl_type,
            path,
        })
    }

    /// Whether `test` passes the owner's `Type::method` path to a helper that
    /// provably runs that argument.
    pub(super) fn test_hands_path_to_invoking_helper(
        &self,
        test: &TestSummary,
        index: &RustIndex,
    ) -> bool {
        // Cheap prefilter: the compact body must spell the path at all.
        let compact: String = test.body.chars().filter(|ch| !ch.is_whitespace()).collect();
        if !compact.contains(&self.path) {
            return false;
        }
        // The owner's impl must be a root item of the test's own file and
        // nothing may rebind the type there.
        let test_source = index
            .files()
            .get(&test.file)
            .map(|facts| facts.data().source.as_ref());
        if !owner_impl_is_root_item_of(index, self.owner, &test.file)
            || file_rebinds_type(index, &test.file, &self.impl_type)
            || impl_identity_is_shadowed(test, self.owner, &self.impl_type, test_source, index)
        {
            return false;
        }
        let Some(parse) = index.source_syntax.parse_fragment(&test.body) else {
            return false;
        };
        let syntax = parse.tree().syntax().clone();
        let Some(function) = syntax.children().find_map(ast::Fn::cast) else {
            return false;
        };
        // A generic parameter of the test, or an item declared inside it, can
        // hide the nominal type or the helper.
        if crate::analysis::syntax::function_type_param_names(&function).contains(&self.impl_type)
            || syntax
                .descendants()
                .any(|node| ast::Item::can_cast(node.kind()) && node != *function.syntax())
        {
            return false;
        }
        syntax
            .descendants()
            .filter(|node| node_executes_in(node, function.syntax()))
            .filter_map(ast::CallExpr::cast)
            .any(|call| self.call_runs_path_through_helper(&call, test, index))
    }

    fn call_runs_path_through_helper(
        &self,
        call: &ast::CallExpr,
        test: &TestSummary,
        index: &RustIndex,
    ) -> bool {
        let Some(ast::Expr::PathExpr(callee)) = call.expr() else {
            return false;
        };
        let Some(path) = callee.path() else {
            return false;
        };
        if path.qualifier().is_some() {
            return false;
        }
        let Some(helper_name) = path
            .segment()
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
        else {
            return false;
        };
        let Some(arguments) = call.arg_list() else {
            return false;
        };
        let positions: Vec<usize> = arguments
            .args()
            .enumerate()
            .filter(|(_, argument)| self.names_owner_path(argument))
            .map(|(position, _)| position)
            .collect();
        if positions.is_empty() {
            return false;
        }
        let offset = u32::from(call.syntax().text_range().start()) as usize;
        let line = test.start_line
            + test.body.get(..offset).map_or(0, |before| {
                before.bytes().filter(|byte| *byte == b'\n').count()
            });
        // The callee must be the one workspace-unique function of that name
        // and the test must not shadow, rename or import it over.
        if super::super::helper_transfer::test_call_is_shadowed(index, test, &helper_name, line) {
            return false;
        }
        let mut matching = index
            .functions()
            .iter()
            .filter(|function| function.name == helper_name);
        let (Some(helper), None) = (matching.next(), matching.next()) else {
            return false;
        };
        // Indexed bodies start at `fn` and omit `async`: read the declaration.
        // A `cfg` anywhere in the helper may mean a disabled twin is the one
        // that was parsed.
        if helper.item.container != FunctionContainer::Free
            || helper.file != test.file
            || helper.attrs.iter().any(|attr| attr.contains("cfg"))
            || helper.body.contains("cfg")
            || function_declaration_is_async(helper, index) != Some(false)
        {
            return false;
        }
        let syntax = &index.call_identity_memos.receiver_pin_syntax;
        if !super::super::owner_pin::same_file_free_helper_call_reaches_owner(
            test, helper, None, index, syntax,
        ) {
            return false;
        }
        // The helper's own file must not rebind the assertion macros whose
        // operands the invocation reader looks through.
        if file_may_rebind_assert_macros(index, &helper.file) {
            return false;
        }
        let Some(invoked) =
            crate::analysis::syntax::unconditionally_invoked_param_positions(helper.body.as_ref())
        else {
            return false;
        };
        positions.iter().any(|position| invoked.contains(position))
    }

    /// Whether `argument` is exactly the owner's `Type::method` path value
    /// (not a call, not a closure, no turbofish or extra qualifier).
    fn names_owner_path(&self, argument: &ast::Expr) -> bool {
        let ast::Expr::PathExpr(expression) = argument else {
            return false;
        };
        let compact: String = expression
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        compact == self.path
    }
}

/// The standard assertion macros the helper-side invocation reader looks
/// through.
const ASSERT_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// Whether the file defines or imports a name an assertion macro could be
/// rebound to, or cannot be read at all. The helper-side reader treats the
/// standard assertion macros as the standard ones; a file that redefines or
/// imports one of those names is read as opaque.
fn file_may_rebind_assert_macros(index: &RustIndex, file: &std::path::Path) -> bool {
    use ra_ap_syntax::ast::HasName;
    let Some(tree) = index.source_syntax.get(index, file).and_then(|s| s.tree()) else {
        return true;
    };
    tree.syntax().descendants().any(|node| {
        ast::MacroRules::cast(node.clone())
            .and_then(|item| item.name())
            .is_some_and(|name| ASSERT_MACROS.iter().any(|known| name.text() == *known))
            || ast::MacroDef::cast(node.clone())
                .and_then(|item| item.name())
                .is_some_and(|name| ASSERT_MACROS.iter().any(|known| name.text() == *known))
            || ast::Use::cast(node).is_some_and(|item| {
                item.syntax().descendants().any(|inner| {
                    ast::NameRef::cast(inner.clone())
                        .is_some_and(|name| ASSERT_MACROS.iter().any(|known| name.text() == *known))
                        || ast::Name::cast(inner).is_some_and(|name| {
                            ASSERT_MACROS.iter().any(|known| name.text() == *known)
                        })
                })
            })
    })
}

#[cfg(test)]
mod tests;
