//! Same-class methods that reach a changed instance method.
//!
//! A test cannot call an underscore-private or internal helper method on its
//! own, and it often calls only the public method of an instance it built:
//!
//! ```ts
//! class Command {
//!   _collect(value, previous) { previous.push(value); return previous; }
//!   add(value) { this.items = this._collect(value, this.items); return this; }
//! }
//! ```
//!
//! A test calling `new Command().add('a')` runs `_collect` through `add`.
//! This module records, per instance-method owner, the sibling methods of the
//! same class whose code reaches it through a bounded `this.method(...)` call
//! graph, so the relation layer can relate tests of those entries to the
//! owner. The graph is syntax-only and fail-closed:
//!
//! - an edge is a `this.name(` call (plain, `?.` or `!.`) in a method,
//!   getter or constructor body, to an ordinary sibling method;
//! - a method body that declares a nested `function` or `class`, or an object
//!   or class method shorthand (`name(...) {`), records no edges, because
//!   `this` may be rebound there; keywords inside strings, templates and
//!   comments are masked first and do not count;
//! - an arrow function that is not an argument of a call (`const later = () =>
//!   this.helper()`) is created, not run, so its body records no edges; an
//!   arrow passed to a call is read as invoked (weak reach, never `exposed`);
//! - a name an instance field of the class defines (`helper = () => 2`)
//!   replaces the prototype method on every instance and is not a callee;
//! - a name that appears twice in the file's class (overloads, getter and
//!   setter pairs) is not a node, and a name some method assigns on `this`
//!   (`this.name = ...`) or defines on `this` is shadowed on the instance and
//!   is not a callee;
//! - static methods are out of scope.
//!
//! Reach through an entry is weaker than a direct owner call, exactly like a
//! same-module entry: it is admitted only when no test calls the owner itself
//! and the classifier never promotes it to `exposed`.

use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Maximum `this.method(...)` edges followed from an entry to the owner.
/// Matches the module-entry bound.
const CLASS_ENTRY_MAX_HOPS: usize = 3;

/// Maximum entries recorded per owner. Dropping entries only withholds
/// reach, so the bound is fail-closed.
const CLASS_ENTRY_MAX_PER_OWNER: usize = 64;

/// Record, on each ordinary instance-method owner, the sibling methods of its
/// class that reach it. Owners of other shapes are left untouched.
pub(crate) fn assign_class_entries(
    owners: &mut [TypeScriptOwner],
    instance_fields: &BTreeMap<String, BTreeSet<String>>,
) {
    let mut by_class: BTreeMap<(PathBuf, String), Vec<usize>> = BTreeMap::new();
    for (index, owner) in owners.iter().enumerate() {
        if owner.owner_kind == OwnerKind::Method
            && let Some(class_name) = owner.class_name.as_ref()
        {
            by_class
                .entry((owner.file.clone(), class_name.clone()))
                .or_default()
                .push(index);
        }
    }
    let mut assignments: Vec<(usize, Vec<TypeScriptModuleEntry>)> = Vec::new();
    for indices in by_class.values() {
        let members: Vec<&TypeScriptOwner> = indices.iter().map(|&index| &owners[index]).collect();
        let fields = members
            .first()
            .and_then(|member| member.class_name.as_ref())
            .and_then(|class_name| instance_fields.get(class_name));
        assignments.extend(class_entries_for(indices, &members, fields));
    }
    for (index, entries) in assignments {
        owners[index].module_entries = entries;
    }
}

fn class_entries_for(
    indices: &[usize],
    members: &[&TypeScriptOwner],
    instance_fields: Option<&BTreeSet<String>>,
) -> Vec<(usize, Vec<TypeScriptModuleEntry>)> {
    // A name that occurs twice in the class is ambiguous, so it is no node.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for member in members {
        *counts.entry(member.name.as_str()).or_default() += 1;
    }
    let nodes: Vec<(usize, &TypeScriptOwner)> = indices
        .iter()
        .copied()
        .zip(members.iter().copied())
        .filter(|(_, member)| {
            member.source_text.is_some()
                && matches!(
                    member.method_kind,
                    TypeScriptMethodKind::Ordinary
                        | TypeScriptMethodKind::Getter
                        | TypeScriptMethodKind::Constructor
                )
                && counts.get(member.name.as_str()) == Some(&1)
        })
        .collect();
    // Source of every method of the class (including the ones that are not
    // nodes), so an instance-level rebinding anywhere shadows the callee.
    let class_sources: Vec<&str> = members
        .iter()
        .filter_map(|member| member.source_text.as_deref())
        .collect();
    let callable: BTreeSet<&str> = nodes
        .iter()
        .map(|(_, node)| *node)
        .filter(|node| node.method_kind == TypeScriptMethodKind::Ordinary)
        .map(|node| node.name.as_str())
        .filter(|name| is_safe_javascript_identifier(name))
        // An instance field of the same name (or a computed field, `*`) replaces
        // the prototype method on every instance.
        .filter(|name| {
            instance_fields.is_none_or(|fields| !fields.contains(*name) && !fields.contains("*"))
        })
        .filter(|name| {
            !class_sources
                .iter()
                .any(|source| rebinds_this_member(source, name))
        })
        .collect();

    // Reverse edges: callee name -> names of the nodes that call it.
    let mut callers: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (_, node) in &nodes {
        let Some(source) = node.source_text.as_deref() else {
            continue;
        };
        let Some(executed) = executed_this_source(source) else {
            continue;
        };
        for &callee in &callable {
            if callee != node.name && contains_member_call_name(&executed, "this", callee) {
                callers
                    .entry(callee)
                    .or_default()
                    .insert(node.name.as_str());
            }
        }
    }
    let by_name: BTreeMap<&str, &TypeScriptOwner> = nodes
        .iter()
        .map(|(_, node)| (node.name.as_str(), *node))
        .collect();
    let mut out = Vec::new();
    for (index, node) in &nodes {
        if node.method_kind != TypeScriptMethodKind::Ordinary {
            continue;
        }
        let target = node.name.as_str();
        let mut seen: BTreeSet<&str> = BTreeSet::from([target]);
        let mut queue: VecDeque<(&str, usize)> = VecDeque::from([(target, 0)]);
        while let Some((name, hops)) = queue.pop_front() {
            if hops == CLASS_ENTRY_MAX_HOPS {
                continue;
            }
            for &caller in callers.get(name).into_iter().flatten() {
                if seen.insert(caller) {
                    queue.push_back((caller, hops + 1));
                }
            }
        }
        let mut entries: Vec<TypeScriptModuleEntry> = seen
            .into_iter()
            .filter(|&name| name != target)
            .filter_map(|name| by_name.get(name))
            .map(|entry| TypeScriptModuleEntry {
                name: entry.name.clone(),
                exported_as_default: false,
                class_member: Some(entry.method_kind),
                decorated: None,
            })
            .collect();
        entries.sort();
        entries.truncate(CLASS_ENTRY_MAX_PER_OWNER);
        if !entries.is_empty() {
            out.push((*index, entries));
        }
    }
    out
}

/// The method source with comments, strings and templates masked and every
/// arrow body that is not a call argument blanked, or `None` when the body
/// declares a scope that may rebind `this` (a nested `function`, a `class`, or
/// an object/class method shorthand such as `{ run() { ... } }`).
fn executed_this_source(source: &str) -> Option<String> {
    let mut code = super::tests_extract::byte_preserving_code_mask(source)?;
    // The method's own `name(params) {` header is not a nested method.
    let body_start = first_top_level_brace(&code)?;
    if ["function", "class"]
        .iter()
        .any(|keyword| contains_word(&code, keyword))
        || has_method_shorthand(&code, body_start)
    {
        return None;
    }
    blank_unexecuted_arrows(&mut code);
    Some(code)
}

/// The byte index of the first `{` outside parentheses: where the method body
/// (or a type literal in its return type) begins.
fn first_top_level_brace(code: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in code.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            '{' if depth == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

/// `true` when `code` holds `name(...) {` with a name that is not a control
/// keyword: a method shorthand (or getter, or computed method) that binds its
/// own `this`.
fn has_method_shorthand(code: &str, body_start: usize) -> bool {
    const CONTROL: [&str; 6] = ["if", "for", "while", "switch", "catch", "with"];
    let bytes = code.as_bytes();
    bytes.iter().enumerate().any(|(close, &byte)| {
        if byte != b')' || close < body_start {
            return false;
        }
        let rest = code[close + 1..].trim_start();
        if !rest.starts_with('{') {
            return false;
        }
        let Some(open) = matching_open_paren(code, close) else {
            return true;
        };
        let head = code[..open].trim_end();
        let word: String = head
            .chars()
            .rev()
            .take_while(|ch| is_javascript_identifier_char(*ch))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        // `foo(...) {` with a plain name, or `]` of a computed key, is a method;
        // a control keyword is a statement block.
        !CONTROL.contains(&word.as_str())
    })
}

fn matching_open_paren(code: &str, close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, ch) in code[..=close].char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Blank the body of every arrow function that is not an argument of a call.
/// Creating such an arrow (`const later = () => this.helper()`) does not run
/// it; an arrow in argument position (`items.forEach((x) => ...)`) is read as
/// invoked.
fn blank_unexecuted_arrows(code: &mut String) {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for (arrow, _) in code.match_indices("=>") {
        let before = code[..arrow].trim_end();
        let params_start = if before.ends_with(')') {
            match matching_open_paren(before, before.len() - 1) {
                Some(open) => open,
                None => continue,
            }
        } else {
            before
                .char_indices()
                .rev()
                .take_while(|(_, ch)| is_javascript_identifier_char(*ch))
                .last()
                .map_or(before.len(), |(index, _)| index)
        };
        let mut head = code[..params_start].trim_end();
        if let Some(rest) = head.strip_suffix("async")
            && !rest.ends_with(is_javascript_identifier_char)
        {
            head = rest.trim_end();
        }
        if head.ends_with(['(', ',']) {
            continue;
        }
        let after = arrow + 2;
        let body = code[after..].trim_start();
        let body_start = code.len() - body.len();
        let end = if body.starts_with('{') {
            balanced_end(code, body_start, '{', '}')
        } else {
            expression_end(code, body_start)
        };
        ranges.push(after..end);
    }
    let mut bytes = std::mem::take(code).into_bytes();
    for range in ranges {
        for byte in bytes.get_mut(range).into_iter().flatten() {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    *code = String::from_utf8(bytes).unwrap_or_default();
}

fn balanced_end(code: &str, start: usize, open: char, close: char) -> usize {
    let mut depth = 0usize;
    for (offset, ch) in code[start..].char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return start + offset + ch.len_utf8();
            }
        }
    }
    code.len()
}

/// The end of an arrow's expression body: the first `,` `;` or closing
/// bracket at nesting depth zero.
fn expression_end(code: &str, start: usize) -> usize {
    let mut depth = 0usize;
    for (offset, ch) in code[start..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    return start + offset;
                }
                depth -= 1;
            }
            ',' | ';' if depth == 0 => return start + offset,
            _ => {}
        }
    }
    code.len()
}

/// Names the instance fields of each class in `program` define, keyed by class
/// name; `*` marks a computed field key, which may define any name.
pub(crate) fn class_instance_field_names(
    program: &oxc_ast::ast::Program<'_>,
) -> BTreeMap<String, BTreeSet<String>> {
    use oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind};
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut visit = |class: &Class<'_>| {
        let Some(name) = class.id.as_ref().map(|id| id.name.as_str().to_string()) else {
            return;
        };
        let fields = out.entry(name).or_default();
        for element in &class.body.body {
            let (is_static, computed, key) = match element {
                ClassElement::PropertyDefinition(field) => {
                    (field.r#static, field.computed, Some(&field.key))
                }
                ClassElement::AccessorProperty(field) => {
                    (field.r#static, field.computed, Some(&field.key))
                }
                _ => continue,
            };
            if is_static {
                continue;
            }
            match key.and_then(property_key_name) {
                Some(name) if !computed => {
                    fields.insert(name);
                }
                _ => {
                    fields.insert("*".to_string());
                }
            }
        }
    };
    for statement in &program.body {
        match statement {
            Statement::ClassDeclaration(class) => visit(class),
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::ClassDeclaration(class)) = export.declaration.as_ref() {
                    visit(class);
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    visit(class);
                }
            }
            _ => {}
        }
    }
    out
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(idx, _)| {
        let before = text[..idx].chars().next_back();
        let after = text[idx + word.len()..].chars().next();
        before.is_none_or(|ch| !is_javascript_identifier_char(ch))
            && after.is_none_or(|ch| !is_javascript_identifier_char(ch))
    })
}

/// `true` when `source` assigns, deletes or defines `this.name`, so the
/// instance may carry its own `name` in place of the prototype method.
fn rebinds_this_member(source: &str, name: &str) -> bool {
    const COMPOUND: [&str; 14] = [
        "+=", "-=", "*=", "/=", "%=", "**=", "||=", "&&=", "??=", "<<=", ">>=", "&=", "|=", "^=",
    ];
    let needle = format!("this.{name}");
    source.match_indices(&needle).any(|(idx, _)| {
        let before_ok = source[..idx]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_javascript_identifier_char(ch) && ch != '.');
        let after = &source[idx + needle.len()..];
        if !before_ok
            || after
                .chars()
                .next()
                .is_some_and(is_javascript_identifier_char)
        {
            return false;
        }
        let rest = after.trim_start();
        let assigned = rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>");
        let compound = COMPOUND.iter().any(|op| rest.starts_with(op));
        let deleted = source[..idx].trim_end().ends_with("delete");
        assigned || compound || deleted
    }) || source.contains("defineProperty(this")
        || source.contains("defineProperties(this")
        || source.contains("setPrototypeOf(this")
}
