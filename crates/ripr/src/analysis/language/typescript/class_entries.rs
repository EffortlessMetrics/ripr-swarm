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
//! - an arrow function is read as run only when it is an argument of a
//!   synchronous array callback method (`items.forEach((x) => ...)`, `map`,
//!   `filter`, `reduce`, `some`, `every`, `find`, `sort` and their kin); every
//!   other arrow (`const later = () => this.helper()`, an array element, a
//!   logical operand, a stored or deferred callback such as `setTimeout`, an
//!   event listener, `subscribe` or `then`) is created or run later, so its
//!   body records no edges (weak reach, never `exposed`);
//! - a nested `function` expression or declaration binds its own `this`, so
//!   its body records no edges, while a call the method makes outside it still
//!   counts; a function whose parameters are followed by a return type
//!   annotation, or a nested `class`, drops the whole method;
//! - a generator method (`*items()`, `async *items()`) only creates an
//!   iterator, so it is neither a caller nor a callee;
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
    repeated_names: &BTreeSet<String>,
) {
    let mut by_class: BTreeMap<(PathBuf, String), Vec<usize>> = BTreeMap::new();
    for (index, owner) in owners.iter().enumerate() {
        // Owners are keyed by `(file, class name)`, so two classes of one name
        // in different scopes cannot be told apart: neither gets entries.
        if owner.owner_kind == OwnerKind::Method
            && let Some(class_name) = owner.class_name.as_ref()
            && !repeated_names.contains(class_name)
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
                && member.source_text.as_deref().is_some_and(is_plain_method)
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
            if callee != node.name
                && (contains_member_call_name(&executed, "this", callee)
                    || runs_this_method_reference(&executed, callee))
            {
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
                instance_factory: false,
                instance_kind: TypeScriptInstanceKind::default(),
                hops: 0,
                cross_module: None,
                closure: false,
                commonjs_whole: false,
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

/// The method source with comments, strings and templates masked, the body of
/// every nested function blanked and every arrow body that is not a
/// synchronous array callback blanked, or `None` when the body declares a scope
/// that may rebind `this` in a way the blanking cannot isolate (a nested
/// `class`, an object/class method shorthand such as `{ run() { ... } }`, or a
/// function whose return type annotation hides its body).
pub(super) fn executed_this_source(source: &str) -> Option<String> {
    let mut code = super::tests_extract::byte_preserving_code_mask(source)?;
    // The method's own `name(params) {` header is not a nested method.
    let body_start = first_top_level_brace(&code)?;
    if !blank_function_bodies(&mut code, body_start) {
        return None;
    }
    if contains_word(&code, "class") || has_method_shorthand(&code, body_start) {
        return None;
    }
    if !blank_unexecuted_arrows(&mut code) {
        return None;
    }
    Some(code)
}

/// `false` for a generator: its `*` sits before the parameter list, outside
/// every bracket, and calling it only creates an iterator.
fn is_plain_method(source: &str) -> bool {
    let Some(code) = super::tests_extract::byte_preserving_code_mask(source) else {
        return false;
    };
    let Some(body_start) = first_top_level_brace(&code) else {
        return false;
    };
    let mut depth = 0usize;
    for ch in code[..body_start].chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '*' if depth == 0 => return false,
            _ => {}
        }
    }
    true
}

/// Blank every nested `function` (parameters and body) after the method header. A nested
/// function binds its own `this`, so what it calls says nothing about the
/// method. Returns `false` when a function cannot be isolated: `function` in
/// the header, generic parameters, or anything but `{` after the parameters.
fn blank_function_bodies(code: &mut String, body_start: usize) -> bool {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for (index, _) in code.match_indices("function") {
        let before = code[..index].chars().next_back();
        let after = code[index + "function".len()..].chars().next();
        if before.is_some_and(is_javascript_identifier_char)
            || after.is_some_and(is_javascript_identifier_char)
        {
            continue;
        }
        if index < body_start {
            return false;
        }
        let mut rest = code[index + "function".len()..].trim_start();
        rest = rest.strip_prefix('*').unwrap_or(rest).trim_start();
        rest = rest
            .trim_start_matches(is_javascript_identifier_char)
            .trim_start();
        if !rest.starts_with('(') {
            return false;
        }
        let open = code.len() - rest.len();
        let params_end = balanced_end(code, open, '(', ')');
        let tail = code[params_end..].trim_start();
        if !tail.starts_with('{') {
            return false;
        }
        let brace = code.len() - tail.len();
        // The parameter defaults run only when the function is called, so the
        // whole function is blanked, keyword and parameters included.
        ranges.push(index..balanced_end(code, brace, '{', '}'));
    }
    blank_ranges(code, ranges);
    true
}

fn blank_ranges(code: &mut String, ranges: Vec<std::ops::Range<usize>>) {
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

/// Blank every arrow function, parameters and body, that is not an
/// unannotated, non-async synchronous array callback. Creating such an arrow
/// (`const later = (x = this.helper()) => 0`) runs neither its parameter
/// defaults nor its body; an unannotated arrow passed to a synchronous array
/// method (`items.forEach((x) => ...)`) is read as invoked. Returns `false`
/// when an arrow's return type annotation cannot be bounded.
fn blank_unexecuted_arrows(code: &mut String) -> bool {
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
        let head = code[..params_start].trim_end();
        // An async callback runs only up to its first `await` before the
        // array method returns; what follows runs later, so it is never read
        // as a synchronous callback.
        let is_async = head
            .strip_suffix("async")
            .is_some_and(|rest| !rest.ends_with(is_javascript_identifier_char));
        let Some(annotated_params) = return_annotation_params(before) else {
            return false;
        };
        // An annotated arrow is never credited as a synchronous callback, so
        // the annotation cannot widen what runs.
        if !is_async
            && annotated_params.is_none()
            && head.ends_with('(')
            && callback_runs_synchronously(head)
        {
            continue;
        }
        let start = annotated_params.unwrap_or(params_start);
        let after = arrow + 2;
        let body = code[after..].trim_start();
        let body_start = code.len() - body.len();
        let end = if body.starts_with('{') {
            balanced_end(code, body_start, '{', '}')
        } else {
            expression_end(code, body_start)
        };
        ranges.push(start..end);
    }
    blank_ranges(code, ranges);
    true
}

/// For the text `before` an arrow's `=>`, `Some(Some(open))` with the `(` of
/// the parameter list when a return type annotation (`(x = d): T =>`) sits
/// between them, `Some(None)` when there is none, and `None` when the scan
/// cannot place the annotation: a nested `=>` (a function type), a `?` after
/// a `:` (a conditional type or a ternary), or an unmatched `)`. Reading a
/// ternary branch or an object key that follows `)` as an annotation only
/// blanks more code.
fn return_annotation_params(before: &str) -> Option<Option<usize>> {
    let bytes = before.as_bytes();
    let mut depth = 0usize;
    let mut seen_colon = false;
    let mut index = bytes.len();
    while index > 0 {
        index -= 1;
        match bytes[index] {
            b'>' if index > 0 && bytes[index - 1] == b'=' => {
                if depth == 0 {
                    return None;
                }
                index -= 1;
            }
            b')' | b']' | b'}' | b'>' => depth += 1,
            b'(' | b'[' | b'{' | b'<' => match depth.checked_sub(1) {
                Some(next) => depth = next,
                None => return Some(None),
            },
            b':' if depth == 0 => {
                let head = before[..index].trim_end();
                if head.ends_with(')') {
                    return matching_open_paren(head, head.len() - 1).map(Some);
                }
                seen_colon = true;
            }
            b'?' if depth == 0 && seen_colon => return None,
            b';' | b',' | b'=' | b'?' if depth == 0 => return Some(None),
            _ => {}
        }
    }
    Some(None)
}

/// Methods that call a callback argument before they return.
const SYNC_CALLBACK_METHODS: [&str; 14] = [
    "forEach",
    "map",
    "filter",
    "reduce",
    "reduceRight",
    "some",
    "every",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "flatMap",
    "sort",
    "toSorted",
];

/// `true` when masked method source passes a `this.callee` reference that runs
/// synchronously: as the first argument of a synchronous array callback, or
/// through `.call(`, `.apply(`, or an immediately invoked `.bind(this)(`.
pub(super) fn runs_this_method_reference(executed: &str, callee: &str) -> bool {
    if !is_safe_javascript_identifier(callee) {
        return false;
    }
    let needle = format!("this.{callee}");
    executed.match_indices(&needle).any(|(idx, _)| {
        if !this_method_reference_boundary(executed, idx, &needle)
            || line_prefix_looks_like_comment_or_string(executed, idx)
            || inside_block_comment(executed, idx)
        {
            return false;
        }
        let after = &executed[idx + needle.len()..];
        if after.starts_with(".call(")
            || after.starts_with(".apply(")
            || after.starts_with(".bind(this)(")
        {
            return true;
        }
        sync_callback_first_method_reference(executed, idx, &needle)
    })
}

fn this_method_reference_boundary(executed: &str, idx: usize, needle: &str) -> bool {
    if !has_member_call_boundary(executed, idx) {
        return false;
    }
    let after_name = idx + needle.len();
    if executed[after_name..]
        .chars()
        .next()
        .is_some_and(is_javascript_identifier_char)
    {
        return false;
    }
    let private_prefix = idx + "this.".len();
    executed
        .get(private_prefix..)
        .is_some_and(|rest| !rest.starts_with('#'))
}

fn sync_callback_first_method_reference(executed: &str, ref_start: usize, needle: &str) -> bool {
    let after_needle = ref_start + needle.len();
    let arg_end = if executed[after_needle..].starts_with(".bind(this)") {
        after_needle + ".bind(this)".len()
    } else {
        after_needle
    };
    let rest = executed[arg_end..].trim_start();
    if !rest.starts_with(',') && !rest.starts_with(')') {
        return false;
    }
    let mut scan = ref_start;
    while scan > 0 {
        let Some(open) = executed[..scan].rfind('(') else {
            break;
        };
        if !executed[open + 1..ref_start]
            .chars()
            .all(|ch| ch.is_whitespace())
        {
            scan = open;
            continue;
        }
        let head = &executed[..open + 1];
        if callback_runs_synchronously(head) {
            return true;
        }
        scan = open;
    }
    false
}

/// `true` when `head` (the code before an arrow, ending in `(`) puts the arrow
/// first in the argument list of a call to one of [`SYNC_CALLBACK_METHODS`] on a
/// receiver; a later argument (`map`'s `thisArg`) is a stored value. An array element, a parenthesized operand, a stored callback and a
/// deferred callee (`setTimeout`, `on`, `subscribe`, `then`) do not qualify.
pub(super) fn callback_runs_synchronously(head: &str) -> bool {
    let mut depth = 0usize;
    let mut open = None;
    for (index, ch) in head.char_indices().rev() {
        match ch {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth > 0 => depth -= 1,
            '(' => {
                open = Some(index);
                break;
            }
            '[' | '{' => return false,
            _ => {}
        }
    }
    let Some(open) = open else {
        return false;
    };
    let callee = before_type_arguments(head[..open].trim_end());
    let name_len: usize = callee
        .chars()
        .rev()
        .take_while(|ch| is_javascript_identifier_char(*ch))
        .map(char::len_utf8)
        .sum();
    let name = &callee[callee.len() - name_len..];
    callee[..callee.len() - name_len].ends_with('.') && SYNC_CALLBACK_METHODS.contains(&name)
}

/// `callee` without a trailing explicit type argument list (`map<string>`),
/// which is erased at runtime. An arrow (`=>`) or a comparison is left as is.
fn before_type_arguments(callee: &str) -> &str {
    if !callee.ends_with('>') {
        return callee;
    }
    let mut depth = 0usize;
    for (index, ch) in callee.char_indices().rev() {
        match ch {
            '>' => depth += 1,
            '<' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return callee[..index].trim_end();
                }
            }
            '(' | ')' | ';' | '{' | '}' | '=' => return callee,
            _ => {}
        }
    }
    callee
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

/// The class name the members of a default-exported class carry: its own id,
/// or `default` when it is anonymous (#16349, `name_anonymous_default_class`).
fn default_class_name<'c>(class: &'c Class<'_>) -> &'c str {
    class.id.as_ref().map_or("default", |id| id.name.as_str())
}

/// Names of classes the program declares more than once, in any scope.
pub(crate) fn repeated_class_names(program: &oxc_ast::ast::Program<'_>) -> BTreeSet<String> {
    use oxc_ast_visit::{Visit, walk};
    struct Counter {
        seen: BTreeMap<String, usize>,
    }
    impl<'a> Visit<'a> for Counter {
        fn visit_class(&mut self, class: &Class<'a>) {
            if let Some(id) = class.id.as_ref() {
                *self.seen.entry(id.name.as_str().to_string()).or_default() += 1;
            }
            walk::walk_class(self, class);
        }
    }
    let mut counter = Counter {
        seen: BTreeMap::new(),
    };
    counter.visit_program(program);
    // `exports.Cart = class Basket { ... }` is reachable as `Cart`, never as
    // `Basket` (that id binds only inside its body), so it is counted under
    // the property instead of its id: a declared `class Cart` beside it reads
    // as repeated, and `exports.A = class X {}; exports.B = class X {}` does
    // not repeat `X`.
    for statement in &program.body {
        // An anonymous default-exported class is named `default` (#16349), so
        // two of them in one file (an ES and a CommonJS one, or two
        // `module.exports = class {}`) repeat that name.
        let anonymous_default = match statement {
            Statement::ExportDefaultDeclaration(export) => matches!(
                &export.declaration,
                oxc_ast::ast::ExportDefaultDeclarationKind::ClassDeclaration(class)
                    if class.id.is_none()
            ),
            _ => super::owners::commonjs_whole_class_export(statement)
                .is_some_and(|class| class.id.is_none()),
        };
        if anonymous_default {
            *counter.seen.entry("default".to_string()).or_default() += 1;
        }
        if let Some((property, class)) = super::owners::commonjs_named_class_export(statement) {
            if let Some(id) = class.id.as_ref() {
                if id.name.as_str() == property {
                    continue;
                }
                if let Some(count) = counter.seen.get_mut(id.name.as_str()) {
                    *count = count.saturating_sub(1);
                }
            }
            *counter.seen.entry(property.to_string()).or_default() += 1;
        }
    }
    counter
        .seen
        .into_iter()
        .filter_map(|(name, count)| (count > 1).then_some(name))
        .collect()
}

/// Names the instance fields of each class in `program` define, keyed by class
/// name; `*` marks a computed field key, which may define any name.
pub(crate) fn class_instance_field_names(
    program: &oxc_ast::ast::Program<'_>,
) -> BTreeMap<String, BTreeSet<String>> {
    use oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind};
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut visit_named = |name: Option<&str>, class: &Class<'_>| {
        let Some(name) = name else {
            return;
        };
        let fields = out.entry(name.to_string()).or_default();
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
        // `exports.Cart = class { ... }`: its members are owners named by the
        // property, so its fields shadow them under that name.
        if let Some((property, class)) = super::owners::commonjs_named_class_export(statement) {
            visit_named(Some(property), class);
            continue;
        }
        match statement {
            Statement::ClassDeclaration(class) => {
                visit_named(class.id.as_ref().map(|id| id.name.as_str()), class)
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::ClassDeclaration(class)) = export.declaration.as_ref() {
                    visit_named(class.id.as_ref().map(|id| id.name.as_str()), class);
                }
            }
            // An anonymous `export default class {}` has members named
            // `default` (#16349), so its fields are keyed by that name too.
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::ClassDeclaration(class) = &export.declaration {
                    visit_named(Some(default_class_name(class)), class);
                }
            }
            // `module.exports = class Cart { ... }` (#16252): its methods are
            // owners, so its fields must shadow them here too. Anonymous, its
            // members are named `default` (#16349).
            Statement::ExpressionStatement(expression) => {
                if let Some(class) = super::owners::commonjs_whole_class_export(statement) {
                    visit_named(Some(default_class_name(class)), class);
                } else if let oxc_ast::ast::Expression::AssignmentExpression(assign) =
                    &expression.expression
                    && let oxc_ast::ast::Expression::ClassExpression(class) =
                        assign.right.get_inner_expression()
                {
                    visit_named(class.id.as_ref().map(|id| id.name.as_str()), class);
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
pub(crate) fn rebinds_this_member(source: &str, name: &str) -> bool {
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

/// `true` when `text` assigns, deletes from a prototype, or defines a member
/// called `name` on any receiver (`program._collect = fn`, `Command.prototype._collect = fn`,
/// `Object.defineProperty(program, '_collect', ...)`), so a call to it may run
/// the replacement and not the class method.
pub(crate) fn assigns_member_named(text: &str, name: &str) -> bool {
    const COMPOUND: [&str; 14] = [
        "+=", "-=", "*=", "/=", "%=", "**=", "||=", "&&=", "??=", "<<=", ">>=", "&=", "|=", "^=",
    ];
    let needle = format!(".{name}");
    let assigned = text.match_indices(&needle).any(|(idx, _)| {
        let after = &text[idx + needle.len()..];
        if after
            .chars()
            .next()
            .is_some_and(is_javascript_identifier_char)
        {
            return false;
        }
        let rest = after.trim_start();
        let plain = rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>");
        plain
            || COMPOUND.iter().any(|op| rest.starts_with(op))
            || deletes_prototype_member_at(text, idx)
    });
    assigned
        || ["defineProperty(", "defineProperties("].iter().any(|call| {
            text.match_indices(call).any(|(idx, _)| {
                let window: String = text[idx..].chars().take(240).collect();
                ['\'', '"', '`']
                    .iter()
                    .any(|quote| window.contains(&format!("{quote}{name}{quote}")))
                    || window.contains(&format!("{{ {name}:"))
            })
        })
}

/// `true` when the member expression that ends just before byte `dot` is the
/// operand of `delete` and names a prototype (`delete Command.prototype.x`).
/// `delete instance.x` removes only an own property, so a prototype method
/// stays callable and the instance form is not a replacement.
fn deletes_prototype_member_at(text: &str, dot: usize) -> bool {
    let chain: usize = text[..dot]
        .chars()
        .rev()
        .take_while(|ch| is_javascript_identifier_char(*ch) || *ch == '.')
        .map(char::len_utf8)
        .sum();
    text[..dot - chain].trim_end().ends_with("delete")
        && text[dot - chain..dot].ends_with(".prototype")
}
