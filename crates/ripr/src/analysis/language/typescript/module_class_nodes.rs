//! Methods of a module-private class as nodes of the module-entry graph
//! (#15928).
//!
//! ```ts
//! export function serialize(input) {
//!   return new Serializer().write(input);   // entry -> Serializer.write
//! }
//! class Serializer {
//!   write(value) { return this.order(value); }   // -> Serializer.order
//!   order(value) { return compareKeys(value); }  // -> compareKeys
//! }
//! function compareKeys(value) { /* changed */ }
//! ```
//!
//! A test cannot construct `Serializer`, so the only reach to its methods, and
//! to a function only they call, is through the exported entry. This module
//! names each plain method `Class.method` and finds the two edges the bare-call
//! graph cannot see: `new Class(...).method(` from any function or method, and
//! `this.method(` between methods of the class.
//!
//! Fail-closed. A class is followed only when it is a top-level declaration
//! with no `extends`, no decorator, no computed member and no `return` in its
//! constructor, its name is declared once and never written, aliased, exported or used as anything but
//! `new Class(...)` in the module, and the calling scope mentions the name
//! only as a construction. A method is a node only when its name is the only
//! member of that name in the class and nothing in the module assigns a member
//! of that name. An instance kept in a variable (`const s = new Class();
//! s.method()`) is not followed.

use super::class_entries::{executed_this_source, rebinds_this_member, runs_this_method_reference};
use super::module_value_reads::{never_mutated_in_module, past_type_arguments};
use super::*;
use oxc_ast::ast::{Class, ClassElement, MethodDefinitionKind, PropertyKey};
use std::collections::{BTreeMap, BTreeSet};

/// A top-level class declaration and its plain instance methods.
pub(super) struct ModuleClass<'s> {
    pub(super) name: &'s str,
    /// Parsed offset of the class name in its declaration.
    decl_start: usize,
    pub(super) text: &'s str,
    /// `(name, method text)` of each method that may be a node.
    methods: Vec<(&'s str, &'s str)>,
}

/// One followable method: `key` is `Class.method`.
pub(super) struct MethodNode<'s> {
    pub(super) key: String,
    pub(super) class: &'s str,
    pub(super) method: &'s str,
    pub(super) text: &'s str,
}

/// The class as a graph candidate, or `None` when its shape is not followed.
pub(super) fn module_class<'a>(class: &'a Class<'a>, source: &'a str) -> Option<ModuleClass<'a>> {
    let id = class.id.as_ref()?;
    if class.super_class.is_some()
        || class.r#abstract
        || class.declare
        || !class.decorators.is_empty()
    {
        return None;
    }
    // Every member name, so a method that shares its name with a field, an
    // accessor, a static or an overload implementation is no node.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut methods = Vec::new();
    for element in &class.body.body {
        let (key, computed) = match element {
            ClassElement::MethodDefinition(method) => (&method.key, method.computed),
            ClassElement::PropertyDefinition(property) => (&property.key, property.computed),
            ClassElement::AccessorProperty(property) => (&property.key, property.computed),
            ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => continue,
        };
        // A computed member may define any name.
        if computed {
            return None;
        }
        let PropertyKey::StaticIdentifier(name) = key else {
            continue;
        };
        let name = name.name.as_str();
        if let ClassElement::MethodDefinition(method) = element {
            // An overload signature carries no body and is not a member.
            if method.value.body.is_none() {
                continue;
            }
            // A constructor that returns hands `new` another object.
            if method.kind == MethodDefinitionKind::Constructor
                && span_text(source, method.span).contains("return")
            {
                return None;
            }
            if method.kind == MethodDefinitionKind::Method
                && !method.r#static
                && !method.value.generator
                && method.decorators.is_empty()
            {
                methods.push((name, span_text(source, method.span)));
            }
        }
        *counts.entry(name).or_default() += 1;
    }
    methods.retain(|(name, _)| counts.get(name) == Some(&1));
    Some(ModuleClass {
        name: id.name.as_str(),
        decl_start: id.span.start as usize,
        text: span_text(source, class.span),
        methods,
    })
}

/// The method nodes of every class the graph may follow. `taken` answers
/// whether a name is already a top-level function, value or import binding.
/// `masked` is `source` with comments and literals masked, in source byte
/// coordinates.
pub(super) fn method_nodes<'s>(
    classes: &[ModuleClass<'s>],
    source: &str,
    masked: &str,
    taken: impl Fn(&str) -> bool,
) -> Vec<MethodNode<'s>> {
    let mut nodes = Vec::new();
    for class in classes {
        let declared_once = classes
            .iter()
            .filter(|other| other.name == class.name)
            .count()
            == 1;
        if !declared_once
            || taken(class.name)
            || !constructs(masked, class.name)
            || !never_mutated_in_module(source, masked, class.name, Some(class.decl_start), None)
        {
            continue;
        }
        for &(method, text) in &class.methods {
            // An instance or prototype member of this name assigned anywhere
            // may replace the method.
            if rebinds_this_member(class.text, method)
                || super::class_entries::assigns_member_named(masked, method)
            {
                continue;
            }
            nodes.push(MethodNode {
                key: format!("{}.{method}", class.name),
                class: class.name,
                method,
                text,
            });
        }
    }
    nodes
}

/// `new NAME(` somewhere in `masked`.
fn constructs(masked: &str, name: &str) -> bool {
    masked
        .match_indices(name)
        .any(|(idx, _)| is_construction_at(masked, idx, name))
}

fn bounded(text: &str, idx: usize, name: &str) -> bool {
    text[..idx]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_javascript_identifier_char(ch))
        && text[idx + name.len()..]
            .chars()
            .next()
            .is_none_or(|ch| !is_javascript_identifier_char(ch))
}

fn is_construction_at(text: &str, idx: usize, name: &str) -> bool {
    bounded(text, idx, name)
        && past_type_arguments(&text[idx + name.len()..]).starts_with('(')
        && text[..idx]
            .trim_end()
            .strip_suffix("new")
            .is_some_and(|rest| {
                !rest
                    .chars()
                    .next_back()
                    .is_some_and(|ch| is_javascript_identifier_char(ch) || ch == '.')
            })
}

/// Keys of the methods `calls` runs as `new Class(...).method(`. `scope` is
/// the whole enclosing function: when it mentions the class name in any other
/// way (a parameter, a local, an alias) the name may be rebound there, so the
/// class contributes nothing.
pub(super) fn constructed_method_calls<'n>(
    calls: &str,
    scope: &str,
    nodes: &'n [MethodNode<'_>],
) -> BTreeSet<&'n str> {
    let mut found = BTreeSet::new();
    let mut masked: Option<String> = None;
    let mut refused: BTreeSet<&str> = BTreeSet::new();
    for node in nodes {
        if refused.contains(node.class) || !calls.contains(node.class) {
            continue;
        }
        let every_mention_constructs = scope.match_indices(node.class).all(|(idx, _)| {
            !bounded(scope, idx, node.class)
                || scope[..idx].trim_end().ends_with('.')
                || is_construction_at(scope, idx, node.class)
        });
        if !every_mention_constructs {
            refused.insert(node.class);
            continue;
        }
        let code = masked.get_or_insert_with(|| ts_mask_literals_and_comments(calls));
        let called = code.match_indices(node.class).any(|(idx, _)| {
            if !is_construction_at(code, idx, node.class) {
                return false;
            }
            let rest = past_type_arguments(&code[idx + node.class.len()..]);
            let Some(arguments_end) = balanced_paren_end(rest) else {
                return false;
            };
            rest[arguments_end..]
                .trim_start()
                .strip_prefix('.')
                .and_then(|member| member.trim_start().strip_prefix(node.method))
                .is_some_and(|tail| tail.starts_with('('))
        });
        if called {
            found.insert(node.key.as_str());
        }
    }
    found
}

/// Byte offset just past the `)` that closes the `(` `text` starts with.
fn balanced_paren_end(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Keys of the sibling methods `node` runs as `this.method(`, or hands by
/// reference to a synchronous array callback.
pub(super) fn this_method_calls<'n>(
    node: &MethodNode<'_>,
    nodes: &'n [MethodNode<'_>],
) -> BTreeSet<&'n str> {
    let Some(executed) = executed_this_source(node.text) else {
        return BTreeSet::new();
    };
    nodes
        .iter()
        .filter(|sibling| sibling.class == node.class && sibling.method != node.method)
        .filter(|sibling| {
            contains_member_call_name(&executed, "this", sibling.method)
                || runs_this_method_reference(&executed, sibling.method)
        })
        .map(|sibling| sibling.key.as_str())
        .collect()
}

fn span_text(source: &str, span: oxc_span::Span) -> &str {
    source
        .get(span.start as usize..span.end as usize)
        .unwrap_or_default()
}
