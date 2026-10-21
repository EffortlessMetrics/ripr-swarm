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
//! - a method body that declares a nested `function` or `class` records no
//!   edges, because `this` may be rebound there;
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
pub(crate) fn assign_class_entries(owners: &mut [TypeScriptOwner]) {
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
        assignments.extend(class_entries_for(indices, &members));
    }
    for (index, entries) in assignments {
        owners[index].module_entries = entries;
    }
}

fn class_entries_for(
    indices: &[usize],
    members: &[&TypeScriptOwner],
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
        if declares_nested_this_scope(source) {
            continue;
        }
        for &callee in &callable {
            if callee != node.name && contains_member_call_name(source, "this", callee) {
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

/// A nested `function` or `class` may rebind `this`, so the body is not read.
fn declares_nested_this_scope(source: &str) -> bool {
    ["function", "class"]
        .iter()
        .any(|keyword| contains_word(source, keyword))
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
