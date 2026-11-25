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
//! constructor, its name is declared once and every other mention of it in the
//! module is a construction `new Class(...)`, and the calling scope mentions the name
//! only as a construction, and the class uses `this` only as a plain access
//! (`this.name`, `this?.name`, `this.#name`) to a member it declares or a field
//! it assigns as `this.name = ...`, calls `this.name(...)` only for an instance
//! method it declares that nothing in the module reassigns, has no static
//! member or block and no member or parameter decorator (no `@` at all), and
//! never uses `super` or `new.target`, and every `new Class(...)` in the
//! module is the receiver of a call to one of its methods. A construction
//! edge comes only from code that runs when its function runs, in a function
//! whose header is `(params) [: Type] [=>] {`. A method is a node only when its name is the only
//! member of that name in the class (quoted keys included), nothing in the
//! module assigns a member of that name, and no literal bracket key of that
//! name (`["name"]`) appears in the module. An instance kept in a variable (`const s = new Class();
//! s.method()`) refuses the whole class.

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
    /// Identifier names of the members the class body declares.
    members: BTreeSet<&'s str>,
    /// Names of the methods the class body declares, with `#` for a private
    /// one: the only members `this.NAME(...)` may call.
    callable: BTreeSet<String>,
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
        // A member or parameter decorator receives the prototype (or adds an
        // initializer bound to the instance) and may replace any method
        // without naming it in the class text. Any `@` in code (not in a
        // comment or literal, so a JSDoc `@param` stays) refuses the class.
        || ts_mask_literals_and_comments(span_text(source, class.span))
            .contains('@')
    {
        return None;
    }
    // Every member name, so a method that shares its name with a field, an
    // accessor, a static or an overload implementation is no node.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    // Names of members keyed by a string or number: never a node
    // themselves, but a quoted `"write" = ...` field still replaces `write`
    // on every instance.
    let mut other_keys: Vec<String> = Vec::new();
    let mut methods = Vec::new();
    let mut callable = BTreeSet::new();
    for element in &class.body.body {
        // A static member or block runs with `this` bound to the class, so
        // the text cannot tell its `this.NAME` from an instance's: a class
        // with any static code is not followed.
        let (key, computed) = match element {
            ClassElement::MethodDefinition(method) if !method.r#static => {
                (&method.key, method.computed)
            }
            ClassElement::PropertyDefinition(property) if !property.r#static => {
                (&property.key, property.computed)
            }
            ClassElement::AccessorProperty(property) if !property.r#static => {
                (&property.key, property.computed)
            }
            ClassElement::TSIndexSignature(_) => continue,
            _ => return None,
        };
        // A computed member may define any name.
        if computed {
            return None;
        }
        // A constructor that returns hands `new` another object, whether it
        // is spelled `constructor` or `"constructor"`.
        if let ClassElement::MethodDefinition(method) = element
            && method.kind == MethodDefinitionKind::Constructor
            && span_text(source, method.span).contains("return")
        {
            return None;
        }
        if let (ClassElement::MethodDefinition(method), PropertyKey::PrivateIdentifier(name)) =
            (element, key)
            && method.kind == MethodDefinitionKind::Method
            && method.value.body.is_some()
        {
            callable.insert(format!("#{}", name.name));
        }
        let PropertyKey::StaticIdentifier(name) = key else {
            if let Some(name) = key.static_name() {
                other_keys.push(name.to_string());
            }
            continue;
        };
        let name = name.name.as_str();
        if let ClassElement::MethodDefinition(method) = element {
            // An overload signature carries no body and is not a member.
            if method.value.body.is_none() {
                continue;
            }
            if method.kind == MethodDefinitionKind::Method {
                callable.insert(name.to_string());
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
    methods.retain(|(name, _)| {
        counts.get(name) == Some(&1) && !other_keys.iter().any(|other| other == name)
    });
    // A field, accessor or quoted key of the same name replaces the method on
    // every instance, so `this.name(...)` may call anything.
    callable.retain(|name: &String| {
        name.starts_with('#')
            || (counts.get(name.as_str()) == Some(&1) && !other_keys.contains(name))
    });
    Some(ModuleClass {
        name: id.name.as_str(),
        decl_start: id.span.start as usize,
        text: span_text(source, class.span),
        methods,
        members: counts.into_keys().collect(),
        callable,
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
        // A method the module may replace (`this.write = ...` in the
        // constructor, `o.write = ...`, `["write"]`) may be anything when
        // called, so it is no safe call target either.
        let callable: BTreeSet<String> = class
            .callable
            .iter()
            .filter(|name| {
                name.starts_with('#')
                    || !(rebinds_this_member(class.text, name)
                        || super::class_entries::assigns_member_named(masked, name)
                        || mentions_literal_key(source, name))
            })
            .cloned()
            .collect();
        let declared_once = classes
            .iter()
            .filter(|other| other.name == class.name)
            .count()
            == 1;
        if !declared_once
            || taken(class.name)
            || !constructs(masked, class.name)
            || !only_constructed(masked, class.name, class.decl_start)
            || !every_construction_calls_a_method(masked, class.name, &callable)
            || !never_mutated_in_module(source, masked, class.name, Some(class.decl_start), None)
            || this_escapes(class.text, &class.members, &callable)
        {
            continue;
        }
        for &(method, text) in &class.methods {
            // An instance or prototype member of this name assigned anywhere
            // may replace the method.
            if rebinds_this_member(class.text, method)
                || super::class_entries::assigns_member_named(masked, method)
                || mentions_literal_key(source, method)
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

/// Whether the class text may reach its instance or prototype other than
/// through a plain access to its own members. `this` counts as plain only as
/// `this.NAME` / `this?.NAME` / `this.#name`, where NAME is a member the class
/// body declares or a field the class assigns as `this.NAME = ...`. Any other
/// use (`Object.assign(this, o)`, `this[key] = f`, `const self = this`,
/// `(this as any).NAME`, `helper(this)`) or an inherited member
/// (`this.constructor`, `this.__proto__`, `this.valueOf()`, a static block's
/// `this.prototype`, `this.__defineGetter__`) may replace a method on the very
/// instance the entry calls, so no method of the class is a node. So may
/// `super` and `new.target`, which reach the same objects without `this`, and
/// a call `this.NAME(...)` of anything but a method the class declares: a
/// field or getter may hold `Object.prototype.valueOf`, which returns the
/// instance when called through it.
fn this_escapes(text: &str, members: &BTreeSet<&str>, callable: &BTreeSet<String>) -> bool {
    if word_at_any(text, "super") || text.contains("new.target") {
        return true;
    }
    let accesses: Vec<Option<(&str, ThisAccess<'_>)>> = text
        .match_indices("this")
        .filter(|&(idx, _)| bounded(text, idx, "this") && !text[..idx].ends_with('.'))
        .map(|(idx, _)| {
            this_member_access(&text[idx + 4..]).map(|access| (text[..idx].trim_end(), access))
        })
        .collect();
    let assigned: BTreeSet<&str> = accesses
        .iter()
        .flatten()
        .filter(|(_, access)| access.assigns)
        .map(|(_, access)| access.name)
        .collect();
    accesses.iter().any(|access| {
        !access.as_ref().is_some_and(|(before, access)| {
            let name = access.name;
            // `for (this.m of xs)` / `for (this.m in o)` assigns the member.
            let loop_target = ["of", "in"].iter().any(|word| {
                access
                    .next
                    .strip_prefix(word)
                    .is_some_and(|rest| !rest.starts_with(is_javascript_identifier_char))
            });
            if loop_target {
                return false;
            }
            // A method is only called directly (`this.m(`, `this.m?.(`,
            // `this.m<T>(`, a tagged template) or passed as a call argument
            // (`.sort(this.m)`). Any other spelling may be an assignment
            // target that replaces it: `[this.m] = ...`, `({ k: this.m } =
            // ...)`, `for (this.m of ...)`, `(this.m as T) = f`,
            // `this.m! = f`, `(this.m) = f`.
            if callable.contains(name) {
                let next = access.next;
                if next.starts_with(['(', '<', '`']) || next.starts_with("?.(") {
                    return access.calls;
                }
                // The `(` opens a call's argument list: it follows a callee
                // name, never a keyword (`if (c) (this.m) &&= f`) or a `)`.
                let argument = before.strip_suffix('(').is_some_and(|callee| {
                    let callee = callee.trim_end();
                    let start = callee
                        .rfind(|ch: char| !is_javascript_identifier_char(ch))
                        .map_or(0, |offset| offset + 1);
                    let word = &callee[start..];
                    !word.is_empty()
                        && !word.starts_with(|ch: char| ch.is_ascii_digit())
                        && !matches!(
                            word,
                            "if" | "while"
                                | "for"
                                | "switch"
                                | "catch"
                                | "with"
                                | "return"
                                | "throw"
                                | "typeof"
                                | "void"
                                | "delete"
                                | "await"
                                | "yield"
                                | "else"
                                | "do"
                                | "case"
                                | "in"
                                | "of"
                                | "instanceof"
                                | "new"
                                | "as"
                                | "satisfies"
                        )
                });
                return argument && next.starts_with([')', ',']) && !followed_by_assignment(next);
            }
            if access.calls {
                return false;
            }
            name.starts_with('#')
                || (!matches!(name, "constructor" | "__proto__" | "prototype")
                    && (members.contains(name) || assigned.contains(name)))
        })
    })
}

/// Whether the text after the `)` or `,` that ends an argument starts an
/// assignment (`(this.m) = f`).
fn followed_by_assignment(next: &str) -> bool {
    let rest = next.trim_start_matches(|ch: char| ch == ')' || ch.is_whitespace());
    // `=`, any compound or logical assignment, a non-null `!`, or a comment
    // that may hide one.
    rest.starts_with(['!', '/'])
        || rest.find('=').is_some_and(|at| {
            at <= 4
                && rest[..at].chars().all(|ch| "+-*/%&|^<>?".contains(ch))
                && !rest[at + 1..].starts_with(['=', '>'])
                && !matches!(&rest[..at], "<" | ">" | "!")
        })
}

/// A plain member access after a `this` keyword.
struct ThisAccess<'t> {
    name: &'t str,
    /// The target of a plain `=`.
    assigns: bool,
    /// Called: `(`, `?.(`, a tagged template, or type arguments then `(`.
    calls: bool,
    /// The code after the name.
    next: &'t str,
}

/// The access when `after` (the text after a `this` keyword) is a plain
/// member access `.NAME` / `?.NAME` / `.#name`. `None` for any other use, and
/// for a name spelled with a unicode escape.
fn this_member_access(after: &str) -> Option<ThisAccess<'_>> {
    let rest = after.trim_start();
    let tail = rest
        .strip_prefix("?.")
        .or_else(|| rest.strip_prefix('.'))?
        .trim_start();
    let name_end = tail
        .char_indices()
        .find(|&(offset, ch)| !(is_javascript_identifier_char(ch) || (offset == 0 && ch == '#')))
        .map_or(tail.len(), |(offset, _)| offset);
    let name = &tail[..name_end];
    let next = tail[name_end..].trim_start();
    if name.is_empty()
        || name == "#"
        || name.starts_with(|ch: char| ch.is_ascii_digit())
        || tail[name_end..].starts_with('\\')
    {
        return None;
    }
    let assigns = next.starts_with('=') && !next.starts_with("==") && !next.starts_with("=>");
    // A new statement on the next line (no semicolon) starts with a word.
    let next_line_word = tail[name_end..]
        .split_once(|ch: char| !ch.is_whitespace())
        .is_some_and(|(space, _)| space.contains('\n'))
        && next.starts_with(|ch: char| is_javascript_identifier_char(ch));
    let calls = !(next_line_word || ends_without_call(next));
    Some(ThisAccess {
        name,
        assigns,
        calls,
        next,
    })
}

/// Whether `next`, the text after `this.NAME`, ends the access without
/// calling it. Only a known separator, operator or member access counts; a
/// call through parentheses (`(this.f)()`), a comment, a TS `!` or type
/// arguments (`this.f!()`, `this.f<T>()`) or a tagged template may call the
/// member with `this` as its receiver.
fn ends_without_call(next: &str) -> bool {
    let after_parens = next.trim_start_matches(|ch: char| ch == ')' || ch.is_whitespace());
    if after_parens.len() != next.len() && !after_parens.is_empty() {
        let skipped = &next[..next.len() - after_parens.len()];
        return (skipped.contains('\n') && after_parens.starts_with(is_javascript_identifier_char))
            || ends_without_call_token(after_parens);
    }
    ends_without_call_token(next)
}

fn ends_without_call_token(next: &str) -> bool {
    let Some(first) = next.chars().next() else {
        return true;
    };
    if first == '?' {
        return !next
            .strip_prefix("?.")
            .is_some_and(|rest| rest.trim_start().starts_with('('));
    }
    if first == '!' {
        return next.starts_with("!=");
    }
    matches!(
        first,
        '.' | ';'
            | ','
            | '['
            | ']'
            | '}'
            | ':'
            | '+'
            | '-'
            | '*'
            | '%'
            | '&'
            | '|'
            | '^'
            | '='
            | '>'
            | ')'
    )
}

fn word_at_any(text: &str, word: &str) -> bool {
    text.match_indices(word)
        .any(|(idx, _)| bounded(text, idx, word))
}

/// `["NAME"]`, `['NAME']` or `` [`NAME`] `` anywhere in the module: a
/// bracket access with the method's name as a literal key can write the
/// member (`this["write"] = ...`), and masked text cannot tell a read from a
/// write there, so any such mention refuses the method.
fn mentions_literal_key(source: &str, name: &str) -> bool {
    ['"', '\'', '`'].iter().any(|quote| {
        source
            .match_indices(&format!("{quote}{name}{quote}"))
            .any(|(idx, _)| {
                source[..idx].trim_end().ends_with('[')
                    && source[idx + name.len() + 2..].trim_start().starts_with(']')
            })
    })
}

/// Whether every mention of `name` in `masked` other than its declaration is
/// a construction `new NAME(`. A value read such as `NAME.prototype` can be
/// aliased (`const p = NAME.prototype; Object.assign(p, o)`) or passed to a
/// mutating call the value-read allowlist cannot see, and a type position
/// shares the spelling, so any other mention refuses the class. A mention
/// after `.` is refused too: text cannot tell a property `o.NAME` from the
/// binding after a numeric literal and a line break (`1.\nNAME.prototype`).
fn only_constructed(masked: &str, name: &str, decl_start: usize) -> bool {
    masked.match_indices(name).all(|(idx, _)| {
        idx == decl_start || !bounded(masked, idx, name) || is_construction_at(masked, idx, name)
    })
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

/// Keys of the methods `calls` runs synchronously as `new Class(...).method(`. `scope` is
/// the whole enclosing function: when it mentions the class name in any other
/// way (a parameter, a local, an alias) the name may be rebound there, so the
/// class contributes nothing.
pub(super) fn constructed_method_calls<'n>(
    calls: &str,
    scope: &str,
    nodes: &'n [MethodNode<'_>],
) -> BTreeSet<&'n str> {
    let mut found = BTreeSet::new();
    // Only code that runs when `calls` runs: a nested function, a deferred or
    // stored arrow (`const later = () => new C().m()`, `setTimeout(...)`) is
    // blanked. A body the blanking cannot isolate gives no edge.
    let mut masked: Option<Option<String>> = None;
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
        let Some(code) = masked.get_or_insert_with(|| executed_body(calls)) else {
            return found;
        };
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

/// Whether every `new NAME(...)` in `masked` is the direct receiver of a call
/// to one of the class's own methods, `new NAME(...).method(`. Any other use
/// hands the instance on, and `Object.getPrototypeOf(new NAME())` or a kept
/// instance can replace a method on the prototype every later instance uses.
fn every_construction_calls_a_method(
    masked: &str,
    name: &str,
    callable: &BTreeSet<String>,
) -> bool {
    masked.match_indices(name).all(|(idx, _)| {
        if !is_construction_at(masked, idx, name) {
            return true;
        }
        let rest = past_type_arguments(&masked[idx + name.len()..]);
        let Some(arguments_end) = balanced_paren_end(rest) else {
            return false;
        };
        let Some(member) = rest[arguments_end..].trim_start().strip_prefix('.') else {
            return false;
        };
        let member = member.trim_start();
        let method_end = member
            .find(|ch: char| !is_javascript_identifier_char(ch))
            .unwrap_or(member.len());
        callable.contains(&member[..method_end])
            && past_type_arguments(&member[method_end..]).starts_with('(')
    })
}

/// The code of the function `text` that runs when it is called, through
/// `executed_this_source` on its body alone (its own `function` keyword or
/// arrow header would read as a nested function). `None` without a block
/// body (an expression arrow) or when the blanking cannot isolate a scope.
fn executed_body(text: &str) -> Option<String> {
    let code = super::tests_extract::byte_preserving_code_mask(text)?;
    let brace = body_brace(&code)?;
    executed_this_source(&format!("body(){}", text.get(brace..)?))
}

/// Offset of the `{` that opens the body of the function `code` starts with:
/// `[async] [function[*]] [name] (params) [: Type] [=>] {` or `x => {`.
/// `None` for any other header, so an expression arrow whose first brace
/// belongs to a nested function (`(x) => x ? () => { ... } : 0`) gives no
/// body.
fn body_brace(code: &str) -> Option<usize> {
    let trimmed = code.trim_start();
    let mut rest = trimmed;
    for keyword in ["async", "function"] {
        if let Some(after) = rest.strip_prefix(keyword)
            && !after.starts_with(is_javascript_identifier_char)
        {
            rest = after.trim_start();
        }
    }
    rest = rest.strip_prefix('*').unwrap_or(rest).trim_start();
    let name_end = rest
        .find(|ch: char| !is_javascript_identifier_char(ch))
        .unwrap_or(rest.len());
    let after_name = rest[name_end..].trim_start();
    let tail = if after_name.starts_with('(') {
        let close = balanced_paren_end(after_name)?;
        let after_params = &after_name[close..];
        let header_end = after_params.find('{')?;
        let header = &after_params[..header_end];
        let annotation = match header.split_once("=>") {
            Some((annotation, between)) if between.trim().is_empty() => annotation,
            Some(_) => return None,
            None => header,
        }
        .trim();
        let plain_type = annotation.is_empty()
            || annotation.strip_prefix(':').is_some_and(|ty| {
                ty.chars().all(|ch| {
                    is_javascript_identifier_char(ch)
                        || ch.is_whitespace()
                        || matches!(ch, '<' | '>' | '[' | ']' | '.' | '|' | ',')
                })
            });
        if !plain_type {
            return None;
        }
        &after_params[header_end..]
    } else {
        // `x => {`: one bare parameter.
        if name_end == 0 {
            return None;
        }
        after_name.strip_prefix("=>")?.trim_start()
    };
    if !tail.starts_with('{') {
        return None;
    }
    Some(code.len() - tail.len())
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
