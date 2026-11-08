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
    "deleteProperty",
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

/// `true` when nothing in the file can write `name` through a shape this
/// module can see, and every mention of `name` outside its own declaration is
/// an allowlisted read. `masked` is `source` with comments and literals
/// masked. Fail-closed: an alias (`const a = NAME`), a value passed to another
/// function, a re-export (`export { NAME }`) or any other non-read mention
/// anywhere in the module disables the binding, since the value may then be
/// mutated through a path this text scan cannot follow.
pub(super) fn never_mutated_in_module(
    source: &str,
    masked: &str,
    name: &str,
    declaration: Option<usize>,
) -> bool {
    // Masking must preserve offsets for the parsed declaration offset to apply.
    let declaration = declaration.filter(|_| masked.len() == source.len());
    let only_reads = masked
        .match_indices(name)
        .all(|(idx, _)| classify(masked, idx, name) != Mention::Other || declaration == Some(idx));
    only_reads && no_write_shape(source, name, declaration)
}

fn no_write_shape(source: &str, name: &str, declaration: Option<usize>) -> bool {
    source.match_indices(name).all(|(idx, _)| {
        if !bounded(source, idx, name) {
            return true;
        }
        let before = source[..idx].trim_end();
        let after = &source[idx + name.len()..];
        if before.ends_with('.') && !before.ends_with("...") {
            // `other.NAME` is another binding.
            return true;
        }
        if declaration == Some(idx) {
            return true;
        }
        let before_op = before.ends_with("++")
            || before.ends_with("--")
            || before
                .strip_suffix("delete")
                .is_some_and(|rest| !rest.ends_with(is_javascript_identifier_char));
        // `Object.assign(NAME, ...)`, `Reflect.set(NAME, ...)`-style mutation.
        let mutating_argument = before.strip_suffix('(').is_some_and(|callee| {
            MUTATING_METHODS
                .iter()
                .any(|method| callee.ends_with(method))
        });
        !(before_op
            || mutating_argument
            || chain_ends_in_write(after)
            || chain_is_destructuring_target(after))
    })
}

/// `[NAME.a] = ...`, `({ k: NAME.a } = ...)`, `for (NAME.a of ...)`: the
/// access chain sits inside a pattern that is assigned to.
fn chain_is_destructuring_target(after: &str) -> bool {
    let (rest, _) = walk_chain(after);
    let trimmed = rest.trim_start();
    if trimmed.starts_with("of ") || trimmed.starts_with("in ") {
        return true;
    }
    let mut depth = 0usize;
    for (offset, ch) in rest.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    if ch == ')' {
                        return false;
                    }
                    let tail = rest[offset + 1..].trim_start();
                    return tail
                        .strip_prefix('=')
                        .is_some_and(|r| !(r.starts_with('=') || r.starts_with('>')));
                }
                depth -= 1;
            }
            ';' if depth == 0 => return false,
            _ => {}
        }
    }
    false
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
    if !after.trim_start().starts_with(']') {
        return false;
    }
    let Some(last) = open.chars().next_back() else {
        return false;
    };
    if last == ')' || last == ']' {
        return true;
    }
    if !is_javascript_identifier_char(last) {
        return false;
    }
    // `const[NAME] = arr`, `for (const[NAME] of arr)`: a keyword before `[`
    // opens a destructuring pattern, not an index access.
    let word_start = open
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_javascript_identifier_char(*ch))
        .last()
        .map_or(open.len(), |(i, _)| i);
    !NON_INDEX_KEYWORDS.contains(&&open[word_start..])
}

/// Words that may directly precede `[` without making it an index access.
const NON_INDEX_KEYWORDS: &[&str] = &[
    "const",
    "let",
    "var",
    "of",
    "in",
    "return",
    "typeof",
    "void",
    "new",
    "delete",
    "case",
    "else",
    "do",
    "yield",
    "await",
    "throw",
    "instanceof",
    "extends",
    "export",
    "default",
    "async",
];

/// Walk a `.member` / `?.member` / `[index]` chain starting at `after`.
/// Returns the text following the chain and the member names that were called.
/// `(` or `?.(` directly after a member: a call.
fn starts_call(after: &str) -> bool {
    let trimmed = after.trim_start();
    trimmed.starts_with('(') || trimmed.starts_with("?.(")
}

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
            let called = starts_call(after_ident);
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
            // A call on a computed member (`X["push"](1)`) names no method:
            // unknown, so never a read.
            if starts_call(rest) {
                members.push(("[computed]", true));
                return (rest, members);
            }
            continue;
        }
        return (trimmed, members);
    }
}

fn chain_calls_mutating_or_unknown(after: &str) -> bool {
    let (rest, members) = walk_chain(after);
    if members.iter().any(|(ident, called)| {
        *called && !READ_METHODS.contains(ident) && !READ_ARGUMENT_METHODS.contains(ident)
    }) {
        return true;
    }
    // A call whose result is then navigated (`NAME.get(k).prop = 1`,
    // `NAME.find(f)[0].x = 1`) may hand back an inner reference; only methods
    // that return a primitive or a fresh match array may be navigated.
    members.last().is_some_and(|(ident, called)| {
        *called && !FRESH_RESULT_METHODS.contains(ident) && call_result_is_navigated(rest)
    })
}

/// Methods whose result is a primitive or a fresh array, never an inner
/// reference of the receiver.
const FRESH_RESULT_METHODS: &[&str] = &[
    "test",
    "exec",
    "includes",
    "indexOf",
    "lastIndexOf",
    "has",
    "join",
    "toString",
    "startsWith",
    "endsWith",
];

/// `rest` starts at the `(` of the last call in a chain; true when the text
/// after the matching `)` continues with `.`, `?.`, `[` or an assignment.
fn call_result_is_navigated(rest: &str) -> bool {
    let trimmed_rest = rest.trim_start();
    let Some(inner) = trimmed_rest
        .strip_prefix("?.(")
        .or_else(|| trimmed_rest.strip_prefix('('))
    else {
        return false;
    };
    let mut depth = 1usize;
    for (offset, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let tail = inner[offset + 1..].trim_start();
                    return tail.starts_with('.')
                        || tail.starts_with("?.")
                        || tail.starts_with('[')
                        || chain_ends_in_write(tail);
                }
            }
            _ => {}
        }
    }
    true
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
