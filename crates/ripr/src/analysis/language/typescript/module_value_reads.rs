//! Read-only use of a module-level non-function `const` by a same-module
//! function (#13592 cause B2).
//!
//! A changed regex, symbol or lookup object such as `const SLASH_RE = /x/g;`
//! is not callable, so no test calls it; a test reaches it only through an
//! exported function that reads it. This module decides whether a function's
//! text *reads* such a binding in a form that provably does not rebind or
//! mutate it. The module-entry graph (`module_entries`) then adds an edge from
//! the reading function to the value, so the value is reached through the same
//! bounded, weaker-than-direct, never-`exposed` entry relation a helper
//! function gets.
//!
//! Fail-closed allowlist. A mention of the name counts as a read only when it
//! is one of:
//!
//! - the receiver of a non-mutating member access (`X.test(...)`, `X.source`,
//!   `X.key`, `X[i]`), provided no assignment, update or mutating method call
//!   ends the access chain;
//! - the first argument of a non-mutating string/array method
//!   (`s.replace(X, ...)`, `s.match(X)`, `s.split(X)`, `a.includes(X)`);
//! - the computed key of an index access (`obj[X]`).
//!
//! Every other mention in the enclosing function (a parameter, a local
//! declaration, an assignment, a value passed along, a spread, an object key,
//! a shorthand property) leaves the function with no edge, and a module-wide
//! assignment or mutating-call shape anywhere in the file disables the binding
//! entirely. Member accesses on another receiver (`other.X`) are not mentions.

use super::related_tests::is_javascript_identifier_char;

/// Member methods that never mutate their receiver.
const READ_METHODS: &[&str] = &[
    "test",
    "exec",
    "includes",
    "indexOf",
    "lastIndexOf",
    "has",
    "get",
    "join",
    "slice",
    "map",
    "filter",
    "some",
    "every",
    "find",
    "findIndex",
    "forEach",
    "reduce",
    "keys",
    "values",
    "entries",
    "concat",
    "toString",
    "startsWith",
    "endsWith",
];

/// Methods whose first argument is only read.
const READ_ARGUMENT_METHODS: &[&str] = &[
    "replace",
    "replaceAll",
    "match",
    "matchAll",
    "split",
    "search",
    "includes",
    "startsWith",
    "endsWith",
    "indexOf",
    "lastIndexOf",
    "concat",
];

/// Methods that mutate their receiver or an argument; any such use in the
/// module disables the binding.
const MUTATING_METHODS: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
    "set",
    "add",
    "delete",
    "clear",
    "assign",
    "defineProperty",
    "defineProperties",
    "setPrototypeOf",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mention {
    /// Not a mention of the binding (longer identifier, or `other.name`).
    Absent,
    Read,
    Other,
}

/// `true` when `masked_text` (the calling code with comments and literals
/// masked) contains at least one read of `name` and every mention of `name` in
/// the raw `scope` (the whole enclosing function) is a read.
pub(super) fn reads_unshadowed_value(masked_text: &str, scope: &str, name: &str) -> bool {
    mentions(masked_text, name).any(|mention| mention == Mention::Read)
        && mentions(scope, name).all(|mention| mention != Mention::Other)
}

/// `true` when the file never assigns, updates or mutates `name` through any
/// of the shapes this module can see.
pub(super) fn never_mutated_in_module(source: &str, name: &str) -> bool {
    source.match_indices(name).all(|(idx, _)| {
        if !bounded(source, idx, name) {
            return true;
        }
        let before = source[..idx].trim_end();
        if before.ends_with('.') && !before.ends_with("...") {
            return true;
        }
        // The binding's own declaration (`const NAME = ...`) is not a write.
        if before
            .strip_suffix("const")
            .is_some_and(|rest| !rest.ends_with(is_javascript_identifier_char))
        {
            return true;
        }
        let before_op = before.ends_with("++")
            || before.ends_with("--")
            || before
                .strip_suffix("delete")
                .is_some_and(|rest| !rest.ends_with(is_javascript_identifier_char));
        // `Object.assign(NAME, ...)`-style mutation of the binding's value.
        let mutating_argument = before.strip_suffix('(').is_some_and(|callee| {
            MUTATING_METHODS
                .iter()
                .any(|method| callee.ends_with(method))
        });
        !(before_op || mutating_argument || chain_ends_in_write(&source[idx + name.len()..]))
    })
}

fn mentions<'t>(text: &'t str, name: &'t str) -> impl Iterator<Item = Mention> + 't {
    text.match_indices(name)
        .map(move |(idx, _)| classify(text, idx, name))
}

fn bounded(text: &str, idx: usize, name: &str) -> bool {
    let before_ok = text[..idx]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_javascript_identifier_char(ch));
    let after_ok = text[idx + name.len()..]
        .chars()
        .next()
        .is_none_or(|ch| !is_javascript_identifier_char(ch));
    before_ok && after_ok
}

fn classify(text: &str, idx: usize, name: &str) -> Mention {
    if !bounded(text, idx, name) {
        return Mention::Absent;
    }
    let raw_before = &text[..idx];
    let before = raw_before.trim_end();
    if before.ends_with("...") {
        return Mention::Other;
    }
    if before.ends_with('.') {
        return Mention::Absent;
    }
    let after = &text[idx + name.len()..];
    if is_read_receiver(after)
        || is_read_argument(before, after)
        || is_computed_key(raw_before, after)
    {
        Mention::Read
    } else {
        Mention::Other
    }
}

/// `NAME.member...` / `NAME[index]...` whose access chain is read-only.
fn is_read_receiver(after: &str) -> bool {
    let rest = after.trim_start();
    if !(rest.starts_with('.') || rest.starts_with("?.") || rest.starts_with('[')) {
        return false;
    }
    !chain_ends_in_write(after) && !chain_calls_mutating_or_unknown(after)
}

/// `recv.replace(NAME, ...)`: the name is the first argument of a method that
/// only reads it.
fn is_read_argument(before: &str, after: &str) -> bool {
    let Some(callee) = before.strip_suffix('(') else {
        return false;
    };
    let callee = callee.trim_end();
    let ident_start = callee
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_javascript_identifier_char(*ch))
        .last()
        .map_or(callee.len(), |(i, _)| i);
    let method = &callee[ident_start..];
    let member = callee[..ident_start].trim_end().ends_with('.');
    let next = after.trim_start();
    member
        && READ_ARGUMENT_METHODS.contains(&method)
        && (next.starts_with(',') || next.starts_with(')'))
}

/// `obj[NAME]`: an index access, not an array or computed-key literal.
fn is_computed_key(raw_before: &str, after: &str) -> bool {
    let Some(open) = raw_before.strip_suffix('[') else {
        return false;
    };
    after.trim_start().starts_with(']')
        && open
            .chars()
            .next_back()
            .is_some_and(|ch| is_javascript_identifier_char(ch) || ch == ')' || ch == ']')
}

/// Walk a `.member` / `?.member` / `[index]` chain starting at `after`.
/// Returns the text following the chain and the member names that were called.
fn walk_chain(after: &str) -> (&str, Vec<(&str, bool)>) {
    let mut rest = after;
    let mut members = Vec::new();
    loop {
        let trimmed = rest.trim_start();
        let step = trimmed
            .strip_prefix("?.")
            .or_else(|| trimmed.strip_prefix('.'));
        if let Some(step) = step {
            let step = step.trim_start();
            let end = step
                .find(|ch: char| !is_javascript_identifier_char(ch))
                .unwrap_or(step.len());
            if end == 0 {
                return (trimmed, members);
            }
            let ident = &step[..end];
            let after_ident = &step[end..];
            let called = after_ident.trim_start().starts_with('(');
            members.push((ident, called));
            if called {
                return (after_ident, members);
            }
            rest = after_ident;
            continue;
        }
        if let Some(inner) = trimmed.strip_prefix('[') {
            let mut depth = 1usize;
            let mut close = None;
            for (offset, ch) in inner.char_indices() {
                match ch {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(offset);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close) = close else {
                return (trimmed, members);
            };
            rest = &inner[close + 1..];
            continue;
        }
        return (trimmed, members);
    }
}

fn chain_calls_mutating_or_unknown(after: &str) -> bool {
    let (_, members) = walk_chain(after);
    members.iter().any(|(ident, called)| {
        *called && !READ_METHODS.contains(ident) && !READ_ARGUMENT_METHODS.contains(ident)
    })
}

/// Whether the access chain after a name is assigned, updated or deleted.
fn chain_ends_in_write(after: &str) -> bool {
    let (rest, members) = walk_chain(after);
    if members
        .iter()
        .any(|(ident, called)| *called && MUTATING_METHODS.contains(ident))
    {
        return true;
    }
    let rest = rest.trim_start();
    if rest.starts_with("++") || rest.starts_with("--") {
        return true;
    }
    // `=` (not `==`, `=>`) and every compound `op=` form.
    let compound = [
        "+", "-", "*", "/", "%", "&", "|", "^", "??", "||", "&&", "<<", ">>", ">>>", "**",
    ];
    if let Some(after_eq) = rest.strip_prefix('=') {
        return !(after_eq.starts_with('=') || after_eq.starts_with('>'));
    }
    compound
        .iter()
        .any(|op| rest.strip_prefix(op).is_some_and(|r| r.starts_with('=')))
}
