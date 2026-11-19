//! Calls of an owner's name that method resolution sends elsewhere (#15273).
//!
//! A trait default method runs only for types whose impl does not override
//! it: `Unit.advance()` runs `impl Counter for Unit { fn advance }` when that
//! impl defines one. A trait impl method is not what `Square(3).area()` runs
//! when `Square` also has an inherent `area(&self)`, because inherent methods
//! take method-call syntax first.
//!
//! These readings only refuse, and only on positive evidence. Every call of
//! the name in the test has to resolve, by the same receiver typing the
//! owner-call relation already uses, to a type that sends it elsewhere. A
//! call ripr cannot type, a call on a type that runs the owner, and (for the
//! inherent case) any mention of the trait in the test keep the relation the
//! caller already had. An unreadable call never turns into a new gap.

use super::{
    FunctionContainer, FunctionSummary, RustIndex, TestSummary, call_site_resolves_to_type,
    compact_impl_type_name, first_argument_span, ident_boundary, ident_ending_at,
    impl_self_type_name, impl_trait_name_from_path, mask_comments_and_strings, method_call_sites,
    owner_trait_name,
};

/// Whether every call of a trait default method's name in `test` resolves
/// to a type whose impl of that trait overrides the default, so none of them
/// runs `owner`. `same_name_ids` are the indexed symbol ids sharing the
/// owner's name. `false` whenever any part of that is not established.
pub(super) fn every_call_runs_an_override(
    test: &TestSummary,
    owner: &FunctionSummary,
    same_name_ids: &[&str],
) -> bool {
    let FunctionContainer::Trait { trait_name } = &owner.item.container else {
        return false;
    };
    let trait_name = impl_trait_name_from_path(trait_name);
    let overriding: Vec<String> = same_name_ids
        .iter()
        .filter(|id| **id != owner.id.0.as_str())
        .filter(|id| owner_trait_name(id).as_deref() == Some(trait_name))
        .filter_map(|id| impl_self_type_name(id))
        .collect();
    if overriding.is_empty() {
        return false;
    }
    let masked = mask_comments_and_strings(&test.body);
    let name = owner.name.as_str();
    if !every_mention_is_a_call(&masked, name) {
        return false;
    }
    let mut sites = method_call_sites(&masked, name).peekable();
    if sites.peek().is_none() {
        return false;
    }
    sites.all(|at| {
        overriding.iter().any(|ty| {
            call_site_resolves_to_type(
                &masked,
                at,
                name,
                ty,
                Some(trait_name),
                &masked,
                &test.body,
                true,
            ) || trait_call_on_unit_value(&masked, at, name, trait_name, ty)
        })
    })
}

/// `Counter::advance(&Fast)`: a trait-qualified call whose first argument is
/// the unit value of `ty`, spelled by the type's own name exactly as the
/// method-call receiver `Fast.advance()` is.
fn trait_call_on_unit_value(text: &str, at: usize, name: &str, trait_name: &str, ty: &str) -> bool {
    if at < 2
        || text.get(at - 2..at) != Some("::")
        || ident_ending_at(text, at - 2) != Some(trait_name)
    {
        return false;
    }
    let Some((start, end)) = first_argument_span(text, at + name.len()) else {
        return false;
    };
    let arg = text[start..end].trim();
    let arg = arg.strip_prefix('&').map_or(arg, str::trim_start);
    arg == ty
}

/// Whether every call of a trait impl method's name in `test` is a method
/// or path call on the impl's own type that an inherent method of the same
/// name and a `self` receiver on that type takes first. A test that names
/// the trait anywhere (`Shape::area(&s)`, `<Square as Shape>::area`, a
/// `dyn Shape` or `impl Shape` binding, a `S: Shape` bound) keeps the
/// relation: those spellings can dispatch through the trait.
pub(super) fn inherent_twin_takes_every_call(
    test: &TestSummary,
    owner: &FunctionSummary,
    same_name_ids: &[&str],
    index: &RustIndex,
) -> bool {
    let FunctionContainer::TraitImpl {
        trait_path,
        self_ty,
    } = &owner.item.container
    else {
        return false;
    };
    if !owner.item.has_self_param {
        return false;
    }
    let (Some(base), trait_name) = (
        compact_impl_type_name(self_ty),
        impl_trait_name_from_path(trait_path),
    ) else {
        return false;
    };
    if trait_name.is_empty() {
        return false;
    }
    let twin_ids: Vec<&str> = same_name_ids
        .iter()
        .copied()
        .filter(|id| *id != owner.id.0.as_str())
        .filter(|id| id.contains("::impl ") && owner_trait_name(id).is_none())
        .filter(|id| impl_self_type_name(id).as_deref() == Some(base.as_str()))
        .collect();
    if twin_ids.is_empty() {
        return false;
    }
    // Method probing tries each receiver step (by value, `&`, `&mut`) in
    // turn, inherent before trait only within one step, so the twin takes the
    // call only when it has the same receiver kind. Its self type must be the
    // same spelling, so `impl Square<u8>` never stands for `Square<u16>`.
    let Some(owner_receiver) = receiver_kind(&owner.body) else {
        return false;
    };
    let twin_has_self_receiver = index.functions().iter().any(|function| {
        twin_ids.contains(&function.id.0.as_str())
            && function.item.has_self_param
            && receiver_kind(&function.body) == Some(owner_receiver)
            && matches!(
                &function.item.container,
                FunctionContainer::Inherent { self_ty: other }
                    if compact_spelling(other) == compact_spelling(self_ty)
            )
    });
    if !twin_has_self_receiver {
        return false;
    }
    let masked = mask_comments_and_strings(&test.body);
    if mentions_identifier(&masked, trait_name) {
        return false;
    }
    let name = owner.name.as_str();
    if !every_mention_is_a_call(&masked, name) {
        return false;
    }
    let mut sites = method_call_sites(&masked, name).peekable();
    if sites.peek().is_none() {
        return false;
    }
    sites.all(|at| {
        call_site_resolves_to_type(&masked, at, name, &base, None, &masked, &test.body, true)
    })
}

/// Whether `ident` appears in `text` as a whole identifier.
fn mentions_identifier(text: &str, ident: &str) -> bool {
    identifier_mentions(text, ident) > 0
}

/// How many times `ident` appears in `text` as a whole identifier.
fn identifier_mentions(text: &str, ident: &str) -> usize {
    text.match_indices(ident)
        .filter(|(at, _)| ident_boundary(text, *at, at + ident.len()))
        .count()
}

/// Whether every mention of `name` is a call: a path value such as
/// `.map(Counter::advance)` may run the owner through a receiver this
/// reading never types.
fn every_mention_is_a_call(text: &str, name: &str) -> bool {
    identifier_mentions(text, name) == method_call_sites(text, name).count()
}

/// The kind of a method's `self` receiver read from its `fn` text: by value
/// (`self`, `mut self`), shared (`&self`, `&'a self`) or exclusive
/// (`&mut self`). `None` for a typed receiver (`self: Box<Self>`) or a
/// signature this cannot read.
fn receiver_kind(body: &str) -> Option<ReceiverKind> {
    let open = body.find('(')?;
    let rest = &body[open + 1..];
    let end = rest.find([',', ')'])?;
    let receiver = rest[..end].split_whitespace().collect::<Vec<_>>().join(" ");
    if receiver == "self" || receiver == "mut self" {
        return Some(ReceiverKind::Value);
    }
    let after_ref = receiver.strip_prefix('&')?.trim_start();
    let after_lifetime = match after_ref.strip_prefix('\'') {
        Some(lifetime) => lifetime.split_once(' ').map_or("", |(_, tail)| tail),
        None => after_ref,
    };
    match after_lifetime.trim_start() {
        "self" => Some(ReceiverKind::Shared),
        "mut self" => Some(ReceiverKind::Exclusive),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReceiverKind {
    Value,
    Shared,
    Exclusive,
}

/// A type spelling with its whitespace removed.
fn compact_spelling(ty: &str) -> String {
    ty.split_whitespace().collect()
}

#[cfg(test)]
mod tests;
