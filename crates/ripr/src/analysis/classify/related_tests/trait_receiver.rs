//! A trait impl method called on a receiver the index types.
//!
//! `impl BufMut for BytesMut { fn advance_mut }` shares its method name with
//! the trait declaration and every other impl, so a call only relates to it
//! once the receiver is known to be a `BytesMut`. The text receiver rule
//! knows three constructor names (`new`, `default`, `from`), so
//! `BytesMut::with_capacity(8).advance_mut(1)` and a binding built by any
//! other associated function stayed a name-only match. An inherent method of
//! the same type already resolves those receivers through the indexed
//! declaration of the constructor.
//!
//! This module lends that receiver authority to a trait impl method, and adds
//! the checks a trait method needs that an inherent one does not: with a
//! receiver of the impl's type, method-call syntax still runs something else
//! when the type has another method of that name, or when an impl the index
//! does not list could define one. Each of those keeps the name-only
//! relation. Nothing here refuses a relation another rule grants.

use super::{
    FunctionContainer, FunctionSummary, RustIndex, TestSummary, compact_impl_type_name,
    impl_trait_name_from_path, inherent_receiver_call_resolves_with,
};

/// Whether `test` calls `owner`, a trait impl method taking `self`, on a
/// receiver the index types as the impl's self type, where that type has no
/// other method of the name that could take the call. `false` whenever any
/// part of that is not established.
pub(super) fn typed_receiver_runs_trait_impl(
    test: &TestSummary,
    owner: &FunctionSummary,
    impl_type: &str,
    index: &RustIndex,
    trust_unplaced_standard_root: bool,
) -> bool {
    let FunctionContainer::TraitImpl {
        trait_path,
        self_ty,
    } = &owner.item.container
    else {
        return false;
    };
    if !owner.item.has_self_param
        || !self_type_is_a_named_type(self_ty)
        || compact_impl_type_name(self_ty).as_deref() != Some(impl_type)
    {
        return false;
    }
    // The receiver is typed by the type's name alone, so the name must
    // declare one type.
    if super::super::owner_pin::workspace_type_name_count(impl_type, index) != 1 {
        return false;
    }
    let owner_trait = impl_trait_name_from_path(trait_path);
    if another_method_may_take_the_call(owner, owner_trait, impl_type, index)
        || super::super::implicit_call::unindexed_impl_may_take_method_call(
            owner,
            owner_trait,
            index,
        )
    {
        return false;
    }
    inherent_receiver_call_resolves_with(
        test,
        owner.name.as_str(),
        impl_type,
        Some(index),
        Some(owner),
        trust_unplaced_standard_root,
        None,
    )
}

/// A self type written as a path (`Chain<T, U>`, `crate::Limit`). A
/// reference, pointer, slice, array, tuple or trait object is implemented
/// for another type than the one a constructor returns.
fn self_type_is_a_named_type(self_ty: &str) -> bool {
    let ty = self_ty.trim();
    !ty.is_empty()
        && !ty.starts_with(['&', '*', '[', '(', '<', '!'])
        && !["dyn ", "impl ", "fn(", "fn ", "unsafe ", "extern "]
            .iter()
            .any(|prefix| ty.starts_with(prefix))
}

/// Whether an indexed method named like `owner` could take a method call on
/// a value of `impl_type` instead of `owner`.
///
/// Another impl of the owner's own trait never does: one trait method
/// selects its impl by the receiver's type, and the declaration's default is
/// what the owner overrides. Every other same-named method is read by where
/// it sits. One on the same type takes or contests the call (an inherent
/// twin, another trait's method, a sibling instantiation). A default body of
/// another trait reaches the type through an impl block that lists no
/// method. A container the parser did not establish is not ruled out.
fn another_method_may_take_the_call(
    owner: &FunctionSummary,
    owner_trait: &str,
    impl_type: &str,
    index: &RustIndex,
) -> bool {
    let functions = index.functions();
    let name = owner.name.strip_prefix("r#").unwrap_or(&owner.name);
    index
        .call_identity_memos
        .function_positions_named(index, name)
        .iter()
        .filter_map(|&position| functions.get(position))
        .filter(|function| function.id != owner.id)
        .any(|function| match &function.item.container {
            FunctionContainer::Free | FunctionContainer::Local => false,
            FunctionContainer::Unknown => true,
            FunctionContainer::Inherent { self_ty } => on_the_same_type(self_ty, impl_type),
            FunctionContainer::TraitImpl { self_ty, .. } => on_the_same_type(self_ty, impl_type),
            FunctionContainer::Trait { trait_name } => {
                impl_trait_name_from_path(trait_name) != owner_trait && function.item.has_body
            }
        })
}

/// Whether a rival's self type is, or cannot be told from, `impl_type`.
fn on_the_same_type(self_ty: &str, impl_type: &str) -> bool {
    compact_impl_type_name(self_ty).is_none_or(|rival| rival == impl_type)
}

#[cfg(test)]
mod tests;
