//! Calls of an owner's name that method resolution sends elsewhere (#15273).
//!
//! A trait default method runs only for types whose impl does not override
//! it: `Unit.advance()` runs `impl Counter for Unit { fn advance }` when that
//! impl defines one. A trait impl method is not what `Square(3).area()` runs
//! when `Square` also has an inherent `area(&self)`, because inherent methods
//! take method-call syntax first. An inherent `area(self)` takes it too when
//! the receiver is an owned `Square`: the by-value step comes before the `&`
//! step that finds the trait impl (#15428).
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
    impl_self_type_name, impl_trait_name_from_path, live_let_statement_before,
    mask_comments_and_strings, matching_open, method_call_sites, owner_trait_name,
};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName, HasVisibility},
};

/// Whether every call of a trait default method's name in `test` resolves
/// to a type whose impl of that trait overrides the default, so none of them
/// runs `owner`. `same_name_ids` are the indexed symbol ids sharing the
/// owner's name. `false` whenever any part of that is not established.
pub(super) fn every_call_runs_an_override(
    test: &TestSummary,
    owner: &FunctionSummary,
    same_name_ids: &[&str],
    index: &RustIndex,
) -> bool {
    let FunctionContainer::Trait { trait_name } = &owner.item.container else {
        return false;
    };
    let trait_name = impl_trait_name_from_path(trait_name);
    let overriding: Vec<String> = same_name_ids
        .iter()
        .filter(|id| **id != owner.id.0.as_str())
        .filter(|id| owner_trait_name(id).as_deref() == Some(trait_name))
        // A generic trait (`impl Counter<u8> for Fast`) or self type
        // (`impl Counter for Fast<u8>`) does not say which instantiations
        // override: `Counter<u16>` or `Fast<u16>` may keep the default. A
        // reference self type (`impl Counter for &Fast`) is not `Fast`.
        .filter(|id| {
            id.rsplit_once(" for ").is_some_and(|(head, ty)| {
                !head.contains('<') && !ty.contains('<') && !ty.trim_start().starts_with('&')
            })
        })
        .filter_map(|id| impl_self_type_name(id))
        .collect();
    if overriding.is_empty() {
        return false;
    }
    // Every same-named candidate must be compiled in and each overriding
    // type must be the only type of that name, so a cfg-gated override or a
    // namesake in another module cannot stand for the type the test calls.
    if same_name_ids
        .iter()
        .filter(|id| **id != owner.id.0.as_str())
        .any(|id| candidate_cfg_gated(id, index))
        || !overriding.iter().all(|ty| declared_once(ty, index))
    {
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
    // A generic self type's inherent impl may carry bounds the receiver
    // does not meet (`impl<T: Copy> Square<T>`), so it is never read here.
    if !owner.item.has_self_param || self_ty.contains('<') {
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
    // turn, inherent before trait only within one step. A twin with the
    // owner's receiver kind is found at the owner's step and takes every
    // call. A twin with an earlier kind takes the call only when probing
    // starts at the type itself, so only for a receiver that is an owned
    // value (#15428): `(&Square(3)).area()` starts at `&Square`, where a
    // trait `&self` method is a by-value match and comes first. A twin with
    // a later kind never takes it. Its self type must be the same spelling,
    // so `impl Square<u8>` never stands for `Square<u16>`.
    let Some(owner_receiver) = receiver_kind(&owner.body) else {
        return false;
    };
    let twin_receivers: Vec<ReceiverKind> = index
        .functions()
        .iter()
        .filter(|function| {
            twin_ids.contains(&function.id.0.as_str())
                && function.item.has_self_param
                && !is_cfg_gated(function, index)
                && declared_pub(function, index)
                && matches!(
                    &function.item.container,
                    FunctionContainer::Inherent { self_ty: other }
                        if compact_spelling(other) == compact_spelling(self_ty)
                )
        })
        .filter_map(|function| receiver_kind(&function.body))
        .collect();
    let at_the_owner_step = twin_receivers.contains(&owner_receiver);
    let at_an_earlier_step = twin_receivers.iter().any(|twin| *twin < owner_receiver);
    if !(at_the_owner_step || at_an_earlier_step) || !declared_once(&base, index) {
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
            && (at_the_owner_step
                || receiver_is_an_owned_value(&masked, at, &base, &test.body, index))
    })
}

/// Whether the call at byte `at` of the masked test body `text`, already
/// resolved to the type `base`, is a path call (`Square::area(..)`, where an
/// inherent method is found before any trait's) or has a receiver that is an
/// owned `base`: the unit value, a tuple-struct constructor or struct literal
/// written in place, a call of an associated function whose one indexed
/// definition returns `Self`, or a `let` binding of one of those or one
/// annotated with the type. A parenthesised receiver, a borrowed one, a call
/// whose return type is not read as owned (`fn new() -> &'static Square`)
/// and a binding this cannot read are not established as owned.
fn receiver_is_an_owned_value(
    text: &str,
    at: usize,
    base: &str,
    scope_body: &str,
    index: &RustIndex,
) -> bool {
    let bytes = text.as_bytes();
    if at >= 2 && bytes[at - 2] == b':' && bytes[at - 1] == b':' {
        return true;
    }
    if at == 0 || bytes[at - 1] != b'.' {
        return false;
    }
    if let Some(receiver) = ident_ending_at(text, at - 1) {
        if receiver == base {
            return true;
        }
        let lets = text
            .get(..at)
            .and_then(|before| before.rfind(';').map(|end| &before[..=end]))
            .unwrap_or("");
        return live_let_statement_before(lets, text, scope_body, receiver, at)
            .and_then(|statement| statement.split_once('='))
            .is_some_and(|(pattern, initializer)| {
                if pattern.contains('&') {
                    return false;
                }
                if pattern
                    .split_once(':')
                    .is_some_and(|(_, annotation)| annotation.trim() == base)
                {
                    return true;
                }
                let initializer = initializer.trim().trim_end_matches(';').trim_end();
                initializer == base || ends_in_owned_construction(initializer, base, index)
            });
    }
    ends_in_owned_construction(&text[..at - 1], base, index)
}

/// Whether `text` ends in an expression that builds an owned `base`:
/// `Square(3)` or `Square { side: 3 }` right after the type's path, or
/// `Square::with_side(3)` where the type's one indexed `with_side` returns
/// `Self` or the type by value.
fn ends_in_owned_construction(text: &str, base: &str, index: &RustIndex) -> bool {
    let bytes = text.as_bytes();
    let mut close = text.len();
    while close > 0 && bytes[close - 1].is_ascii_whitespace() {
        close -= 1;
    }
    let Some(close) = close.checked_sub(1) else {
        return false;
    };
    let open_byte = match bytes[close] {
        b')' => b'(',
        b'}' => b'{',
        _ => return false,
    };
    let Some(open) = matching_open(bytes, close, open_byte, bytes[close]) else {
        return false;
    };
    // The callee path ends right before the opening bracket; a
    // parenthesised or block expression has none.
    let before = text[..open].trim_end();
    let path_start = before
        .char_indices()
        .rev()
        .find(|&(_, ch)| !(ch.is_alphanumeric() || ch == '_' || ch == ':'))
        .map_or(0, |(at, ch)| at + ch.len_utf8());
    let mut segments = before[path_start..].rsplit("::");
    let Some(last) = segments.next().filter(|segment| !segment.is_empty()) else {
        return false;
    };
    if last == base {
        return true;
    }
    open_byte == b'('
        && segments.next() == Some(base)
        && super::super::owner_pin::constructor_returns_self(base, last, false, index)
}

/// Whether a function or its `impl` block carries a `cfg`-family attribute,
/// so it may be compiled out of the test build.
fn is_cfg_gated(function: &FunctionSummary, index: &RustIndex) -> bool {
    function.attrs.iter().any(|attr| attr.contains("cfg"))
        || function.impl_attrs.iter().any(|attr| attr.contains("cfg"))
        || enclosed_by_cfg(function, index)
}

/// Whether any item enclosing `function` in its file's parse (its `impl`, a
/// `mod`, or the file through an inner `#![cfg]`) carries a `cfg`-family
/// attribute. A function this cannot find in a clean parse counts as
/// enclosed: nothing establishes it is compiled in.
fn enclosed_by_cfg(function: &FunctionSummary, index: &RustIndex) -> bool {
    parsed_fn(function, index).is_none_or(|found| {
        found.syntax().ancestors().any(|node| {
            node.children()
                .filter_map(ast::Attr::cast)
                .any(|attr| attr.syntax().text().to_string().contains("cfg"))
        })
    })
}

/// Whether `function` declares a visibility (`pub`, `pub(crate)`, ..), so
/// method probing from a test outside its module can see it. A private
/// inherent twin is skipped by such callers, and the trait impl runs.
fn declared_pub(function: &FunctionSummary, index: &RustIndex) -> bool {
    parsed_fn(function, index).is_some_and(|found| found.visibility().is_some())
}

/// The innermost `fn` named like `function` whose span covers its start
/// line in a clean parse of its file. `function.body` starts at the `fn`
/// token, so attributes and visibility are read from this parse.
fn parsed_fn(function: &FunctionSummary, index: &RustIndex) -> Option<ast::Fn> {
    let source = index
        .files()
        .get(&function.file)
        .map(|facts| facts.data().source.clone())?;
    let parse = parse_clean_source_file(&source)?;
    let line_of = |at: usize| {
        source
            .get(..at)
            .map_or(0, |prefix| prefix.matches('\n').count() + 1)
    };
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|candidate| {
            candidate
                .name()
                .is_some_and(|name| name.text() == function.name.as_str())
        })
        .filter(|candidate| {
            let range = candidate.syntax().text_range();
            line_of(usize::from(range.start())) <= function.start_line
                && function.start_line <= line_of(usize::from(range.end()))
        })
        .min_by_key(|candidate| candidate.syntax().text_range().len())
}

/// [`is_cfg_gated`] for the indexed function with symbol id `id`; an id with
/// no indexed function counts as gated, since nothing establishes it.
fn candidate_cfg_gated(id: &str, index: &RustIndex) -> bool {
    index
        .functions()
        .iter()
        .find(|function| function.id.0 == id)
        .is_none_or(|function| is_cfg_gated(function, index))
}

/// Whether exactly one `struct`, `enum` or `union` named `name` is declared
/// across the indexed files (comments and strings masked), so the name
/// identifies one type.
fn declared_once(name: &str, index: &RustIndex) -> bool {
    let mut count = 0usize;
    for facts in index.files().values() {
        if !facts.source.contains(name) {
            continue;
        }
        let masked = mask_comments_and_strings(&facts.source);
        for keyword in ["struct", "enum", "union"] {
            for (at, _) in masked.match_indices(keyword) {
                if !ident_boundary(&masked, at, at + keyword.len()) {
                    continue;
                }
                let rest = masked[at + keyword.len()..].trim_start();
                if rest.strip_prefix(name).is_some_and(|tail| {
                    !tail.starts_with(|ch: char| ch.is_alphanumeric() || ch == '_')
                }) {
                    count += 1;
                }
            }
        }
    }
    count == 1
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

/// Ordered as method probing tries the steps for an owned receiver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
