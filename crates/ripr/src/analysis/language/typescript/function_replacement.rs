//! A test file that replaces the changed owner function through its module
//! object (#15939).
//!
//! `sinon.stub(ns, 'owner')`, `sinon.replace(ns, 'owner', fake)`,
//! `mock.method(ns, 'owner', fake)`, `jest.replaceProperty(ns, 'owner', fake)`,
//! `ns.owner = fake` and a `spyOn(ns, 'owner')` whose value is fabricated in a
//! hook all swap the function a later `ns.owner(...)` (or, under a CommonJS
//! transpile, a bare named-import call) runs. The assertion then observes the
//! substitute, not the changed code, so no owner-call relation may be
//! credited through that file.
//!
//! The fact is file-wide because the replacement usually sits in a
//! `beforeEach` or at the top level, outside the test body the relation
//! reads. It is text-level and fail-closed: an entry can only withhold a
//! relation. A call-through spy (`vi.spyOn(ns, 'owner')` with no fabrication,
//! `sinon.spy(ns, 'owner')`, a two-argument `mock.method` in a file with no
//! `mockImplementation`) records nothing and still relates.
//!
//! Not modeled: `Object.assign(ns, { owner: fake })`, a replacement made by
//! a helper in another file, and a restore that returns the real function
//! before a sibling test calls it (that sibling loses its relation).

use super::related_tests::{
    has_member_call_boundary, is_javascript_identifier_char, owner_spy_call_fabricated,
};
use super::{TypeScriptOwner, TypeScriptTest};

/// Entry in [`replaced_function_member_names`]: every member of every object
/// may be replaced (a whole-object stub or a computed member name on a target
/// that is not a plain identifier).
pub(crate) const FUNCTION_REPLACEMENT_ANY: &str = "*";

/// Argument lists longer than this are not scanned; the call then replaces
/// every name.
const MAX_ARGUMENT_BYTES: usize = 8192;

/// Members the test file replaces with a substitute implementation.
///
/// Entries are one of:
///
/// - `local.name`: the member `name` is replaced on the object reached
///   through the identifier `local` (a plain identifier, the root of a dotted
///   path, or the identifier inside a cast such as `(local as any)`), by a
///   replacement call naming it with a plain string literal or by an
///   assignment `local.name = ...`;
/// - `name`: the same, on a target that is not rooted at an identifier (a
///   call result, an index);
/// - `local.*`: the object reached through `local` is stubbed whole
///   (`sinon.stub(local)`) or through a member name that is not a plain string
///   literal;
/// - [`FUNCTION_REPLACEMENT_ANY`]: the same, on a target that is not rooted
///   at an identifier.
///
/// The recognised replacement calls are `stub`, a three-argument `replace`
/// (a two-argument `replace` is `String.prototype.replace`), `replaceProperty`,
/// `defineProperty`, `mock.method` with an implementation argument (or with
/// none when the file mentions `mockImplementation`), and a `spyOn` whose
/// value is fabricated anywhere in the file.
pub(crate) fn replaced_function_member_names(source: &str) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    let file_reimplements_mocks = source.contains("mockImplementation");
    for word in [
        "stub",
        "replace",
        "replaceProperty",
        "defineProperty",
        "method",
        "spyOn",
    ] {
        for (idx, _) in source.match_indices(word) {
            let before = source[..idx].chars().next_back();
            let rest = &source[idx + word.len()..];
            if before.is_some_and(is_javascript_identifier_char)
                || rest
                    .chars()
                    .next()
                    .is_some_and(is_javascript_identifier_char)
            {
                continue;
            }
            if word == "method" && !source[..idx].ends_with("mock.") {
                continue;
            }
            let Some(inner) = rest.trim_start().strip_prefix('(') else {
                continue;
            };
            let Some(end) = super::classifier::balanced_close_offset(inner) else {
                continue;
            };
            if end > MAX_ARGUMENT_BYTES {
                names.insert(FUNCTION_REPLACEMENT_ANY.to_string());
                continue;
            }
            let arguments = top_level_arguments(&inner[..end]);
            let replaces = match word {
                "stub" => !arguments.is_empty(),
                "replace" | "replaceProperty" | "defineProperty" => arguments.len() >= 3,
                "method" => {
                    arguments.len() >= 3 || (arguments.len() == 2 && file_reimplements_mocks)
                }
                _ => arguments.len() >= 2 && owner_spy_call_fabricated(source, idx, inner),
            };
            if !replaces {
                continue;
            }
            let target = arguments.first().copied().and_then(root_identifier);
            let member = arguments.get(1).copied().and_then(plain_string_literal);
            names.insert(replacement_entry(target, member));
        }
    }
    for (idx, _) in source.match_indices('.') {
        let rest = &source[idx + 1..];
        let len: usize = rest
            .chars()
            .take_while(|ch| is_javascript_identifier_char(*ch))
            .map(char::len_utf8)
            .sum();
        if len == 0 || !assignment_follows(&rest[len..]) {
            continue;
        }
        names.insert(replacement_entry(
            receiver_root_identifier(&source[..idx]),
            Some(&rest[..len]),
        ));
    }
    names.into_iter().collect()
}

/// `true` when the test's file replaces the owner function: an entry names
/// the owner (or `default` for a default-exported owner) through a local the
/// file imports, or on a target that is not rooted at an identifier. A
/// global or body-local target (`console`, `Date`, a fixture object) is not
/// the owner's module. The import's source is not compared with the owner's
/// module, so a barrel namespace (`import * as lib from '../src'`) is covered
/// and a wrong tie only withholds.
pub(crate) fn owner_function_replaced_in_file(
    test: &TypeScriptTest,
    owner: &TypeScriptOwner,
) -> bool {
    let names_owner =
        |member: &str| member == owner.name || (owner.exported_as_default && member == "default");
    test.function_replacements_in_file.iter().any(|entry| {
        if entry == FUNCTION_REPLACEMENT_ANY {
            return true;
        }
        match entry.split_once('.') {
            None => names_owner(entry),
            Some((local, member)) => {
                (member == "*" || names_owner(member))
                    && test
                        .imports_in_file
                        .iter()
                        .any(|import| import.local == local)
            }
        }
    })
}

/// The entry for a replacement of `member` (every member when `None`) on
/// the object rooted at `target` (any object when `None`).
fn replacement_entry(target: Option<&str>, member: Option<&str>) -> String {
    match (target, member) {
        (Some(local), Some(member)) => format!("{local}.{member}"),
        (Some(local), None) => format!("{local}.*"),
        (None, Some(member)) => member.to_string(),
        (None, None) => FUNCTION_REPLACEMENT_ANY.to_string(),
    }
}

/// The identifier a target expression is rooted at once wrapping
/// parentheses, an `as` cast and a non-null `!` are removed: `d` for `d`,
/// `(d as any)` and `d!`, `a` for `a.b`. `None` for a call result, an index
/// or any other expression.
fn root_identifier(argument: &str) -> Option<&str> {
    let mut text = argument.trim();
    while let Some(inner) = text.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        text = inner.trim();
    }
    let text = text.split(" as ").next().unwrap_or(text).trim();
    let text = text.strip_suffix('!').unwrap_or(text);
    let path = !text.is_empty()
        && text
            .chars()
            .all(|ch| is_javascript_identifier_char(ch) || ch == '.');
    let root = text.split('.').next().unwrap_or_default();
    (path && !root.is_empty()).then_some(root)
}

/// The root identifier of the receiver expression ending `before` (the text
/// up to a member `.`): `d` for `d`, `a` for `a.b`, `d` for `(d as any)`.
/// `None` for a call result, an index or any other receiver.
fn receiver_root_identifier(before: &str) -> Option<&str> {
    let before = before.trim_end();
    if before.ends_with(')') {
        let mut depth = 0usize;
        for (at, ch) in before.char_indices().rev() {
            match ch {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        // A call (`f(x).name = ...`) has a callee before the
                        // parenthesis; a cast or grouping does not.
                        if !has_member_call_boundary(before, at) {
                            return None;
                        }
                        return root_identifier(&before[at..]);
                    }
                }
                _ => {}
            }
        }
        return None;
    }
    let chain_len: usize = before
        .chars()
        .rev()
        .take_while(|ch| is_javascript_identifier_char(*ch) || *ch == '.')
        .map(char::len_utf8)
        .sum();
    let chain = &before[before.len() - chain_len..];
    let root = chain.split('.').next().unwrap_or_default();
    let anchored = before[..before.len() - chain_len]
        .chars()
        .next_back()
        .is_none_or(|ch| !matches!(ch, ')' | ']' | '?'));
    (anchored && !root.is_empty() && !chain.starts_with('.')).then_some(root)
}

/// `true` when `after` starts a plain or compound assignment rather than a
/// comparison or an arrow.
fn assignment_follows(after: &str) -> bool {
    let after = after.trim_start();
    let compound = ["||=", "&&=", "??="];
    (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"))
        || compound.iter().any(|operator| after.starts_with(operator))
}

/// The top-level arguments of a call's argument text, string-aware. A
/// trailing comma adds no argument.
fn top_level_arguments(arguments: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0usize;
    let mut parts = Vec::new();
    for (offset, ch) in arguments.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(arguments[start..offset].trim());
                start = offset + 1;
            }
            _ => {}
        }
    }
    let last = arguments[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// The content of `argument` when it is a plain string literal with no
/// escape and no template interpolation, else `None`.
fn plain_string_literal(argument: &str) -> Option<&str> {
    let quote = argument
        .chars()
        .next()
        .filter(|ch| matches!(ch, '"' | '\'' | '`'))?;
    let inner = argument.strip_prefix(quote)?.strip_suffix(quote)?;
    let interpolated = quote == '`' && inner.contains("${");
    (!inner.is_empty() && !inner.contains(quote) && !inner.contains('\\') && !interpolated)
        .then_some(inner)
}
