//! Bounded local function aliases shared by call facts and owner pins.

use super::MAX_CALL_FACTS_PER_LINE;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::CallFact;
use crate::analysis::syntax::{parse_clean_source_file, source_binds_value_name};
use ra_ap_syntax::{AstNode, ast};

/// Most `let` aliases considered per body; each costs a body scan, so the
/// count is bounded. Aliases past the cap are ignored (less reach evidence).
const MAX_LOCAL_ALIASES_PER_BODY: usize = 64;

/// A `let alias = path::to::function;` binding found in a masked body. The
/// path is matched by its last segment name only; it is never resolved to a
/// definition.
struct LocalFnAlias<'a> {
    alias: &'a str,
    /// The right-hand side exactly as written (`Type::function`).
    path: &'a str,
    target: &'a str,
    line_offset: usize,
    pointer: bool,
}

/// Credit a call through a simple local function alias to the aliased
/// function (`let from_isoywd = NaiveDate::from_isoywd_opt;` then
/// `from_isoywd(..)` also reads as a call to `from_isoywd_opt`). Reach
/// evidence only: it adds a name-only call fact beside the alias-named one
/// and never strengthens an oracle or argument reading.
///
/// Fail-closed: the alias must be bound exactly once by an immutable
/// `let NAME = <path>;` or a safe Rust `fn(..)` annotation (no `mut`, no closure or
/// call on the right) and every other mention of `NAME` in the body must be
/// a direct `NAME(` call after the binding. Any other mention — a second
/// `let`, a reassignment, a pattern binding, a closure parameter, a nested
/// `fn`, passing the alias along — leaves the alias unresolved and adds
/// nothing.
pub(super) fn add_local_alias_calls(
    masked: &str,
    body: &str,
    start_line: usize,
    calls: &mut Vec<CallFact>,
) {
    let mut added = Vec::new();
    for alias in local_fn_aliases(masked, body) {
        if !alias_is_unambiguous(masked, &alias) {
            continue;
        }
        let bytes = masked.as_bytes();
        let mut search = alias.line_offset;
        while let Some(found) = masked[search..].find(alias.alias) {
            let at = search + found;
            let end = at + alias.alias.len();
            search = end;
            if (at > 0 && is_ident_byte(bytes[at - 1]))
                || bytes.get(end).is_some_and(|b| is_ident_byte(*b))
            {
                continue;
            }
            let after = masked[end..].trim_start();
            if !after.starts_with('(') {
                continue;
            }
            let open = masked.len() - after.len();
            let Some(close) = matching_paren_on_one_line(masked, open) else {
                continue;
            };
            // The fact reads as if the test had spelled the aliased path
            // itself: `Type::function(args)`. Only the call's own bytes are
            // kept, so a target-looking substring elsewhere on the line
            // (a string, a comment) can never lend arguments or reach.
            let text = format!("{}{}", alias.path, &body[open..=close]);
            added.push(CallFact {
                line: start_line + masked[..at].matches('\n').count(),
                name: alias.target.to_string(),
                text: text.into(),
            });
        }
    }
    for fact in added {
        let on_line = calls.iter().filter(|call| call.line == fact.line).count();
        if on_line < MAX_CALL_FACTS_PER_LINE {
            calls.push(fact);
        }
    }
}

/// The index of the `)` closing the `(` at `open`, when it sits on the same
/// line. `masked` has strings and comments erased, so only real parentheses
/// count.
fn matching_paren_on_one_line(masked: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in masked.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'\n' => return None,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `offset` sits inside a macro invocation's delimiters
/// (`name!(..)`, `name![..]`, `name!{..}`), whose tokens need not be Rust
/// statements (`stringify!(let f = T::g; f(1))`).
fn inside_macro_group(masked: &str, offset: usize) -> bool {
    let bytes = masked.as_bytes();
    let mut stack: Vec<bool> = Vec::new();
    for (index, byte) in bytes[..offset].iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => {
                let before = masked[..index].trim_end().as_bytes();
                stack.push(before.last() == Some(&b'!'));
            }
            b')' | b']' | b'}' => {
                stack.pop();
            }
            _ => {}
        }
    }
    stack.into_iter().any(|is_macro| is_macro)
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Immutable aliases with a plain path ending in a snake-case segment.
/// A bare target additionally requires a safe `fn(..)` annotation and no
/// body-local binding of the target, which may otherwise be a closure or parameter.
fn local_fn_aliases<'a>(masked: &'a str, source: &str) -> Vec<LocalFnAlias<'a>> {
    let bytes = masked.as_bytes();
    let mut aliases = Vec::new();
    let mut search = 0;
    while aliases.len() < MAX_LOCAL_ALIASES_PER_BODY
        && let Some(found) = masked[search..].find("let")
    {
        let at = search + found;
        search = at + 3;
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            continue;
        }
        let rest = &masked[at + 3..];
        let trimmed = rest.trim_start();
        if trimmed.len() == rest.len() {
            continue;
        }
        let name_len = trimmed
            .bytes()
            .take_while(|byte| is_ident_byte(*byte))
            .count();
        let name = &trimmed[..name_len];
        let after_name = trimmed[name_len..].trim_start();
        let (rhs, pointer) = if let Some(annotated) = after_name.strip_prefix(':') {
            let Some((annotation, rhs)) = annotated.split_once('=') else {
                continue;
            };
            if !safe_fn_pointer_type(annotation) {
                continue;
            }
            (rhs, true)
        } else if let Some(rhs) = after_name.strip_prefix('=') {
            (rhs, false)
        } else {
            continue;
        };
        if rhs.starts_with('=') || matches!(name, "" | "_" | "mut" | "ref") {
            continue;
        }
        if name.starts_with(|c: char| c.is_ascii_digit() || c.is_ascii_uppercase()) {
            continue;
        }
        let Some(end) = rhs.find(';') else {
            continue;
        };
        let path = rhs[..end].trim();
        let Some(target) = simple_fn_path_target(path) else {
            continue;
        };
        if !path.contains("::") && (!pointer || source_binds_value_name(source, target)) {
            continue;
        }
        if inside_macro_group(masked, at) {
            continue;
        }
        if target == name {
            continue;
        }
        aliases.push(LocalFnAlias {
            alias: name,
            path,
            target,
            line_offset: at,
            pointer,
        });
    }
    aliases
}

fn safe_fn_pointer_type(annotation: &str) -> bool {
    // Only annotations pay for a type parse; ordinary aliases keep their lexical path.
    let Some(parse) = parse_clean_source_file(&format!("type RiprAlias = {annotation};")) else {
        return false;
    };
    let Some(ast::Type::FnPtrType(pointer)) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::TypeAlias::cast)
        .and_then(|alias| alias.ty())
    else {
        return false;
    };
    pointer.unsafe_token().is_none()
        && pointer.abi().is_none()
        && pointer.async_token().is_none()
        && pointer.const_token().is_none()
}

/// Resolve a complete directly asserted pointer call with the very same
/// binding/use rules that produce its call fact. The owner pin still resolves
/// the returned path to its actual definition and admits the original assertion.
pub(crate) fn local_fn_pointer_call(body: &str, operand: &str) -> Option<String> {
    if !body.contains(':') {
        return None;
    }
    let operand = operand.trim();
    let masked_operand = mask_comments_and_strings(operand);
    let open = masked_operand.find('(')?;
    let name = masked_operand[..open].trim_end();
    if name.is_empty() || !name.bytes().all(is_ident_byte) {
        return None;
    }
    let close = matching_paren_on_one_line(&masked_operand, open)?;
    if !masked_operand[close + 1..].trim().is_empty() {
        return None;
    }
    let masked = mask_comments_and_strings(body);
    let alias = local_fn_aliases(&masked, body).into_iter().find(|alias| {
        alias.pointer && alias.alias == name && alias_is_unambiguous(&masked, alias)
    })?;
    Some(format!("{}{}", alias.path, &operand[open..=close]))
}

/// The last segment of `path` when it is `ident(::ident)*` with a
/// snake-case final segment; anything else (calls, generics, references,
/// closures, literals) is not a simple function path.
fn simple_fn_path_target(path: &str) -> Option<&str> {
    let mut last = "";
    for segment in path.split("::") {
        if segment.is_empty()
            || !segment.bytes().all(is_ident_byte)
            || segment.starts_with(|c: char| c.is_ascii_digit())
        {
            return None;
        }
        last = segment;
    }
    last.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        .then_some(last)
}

/// The alias is bound exactly once and every other mention is a direct call
/// after the binding.
///
/// Limitation: block scope is not modeled. The decision is body-wide, so a
/// same-named mention in any other block (a shadowing parameter, pattern or
/// loop binding, a macro, a field) disables the alias for the whole body
/// instead of being scoped out. That only drops reach evidence; it never
/// credits a call the alias does not make.
fn alias_is_unambiguous(masked: &str, alias: &LocalFnAlias<'_>) -> bool {
    let bytes = masked.as_bytes();
    let name = alias.alias;
    let mut search = 0;
    let mut definition_seen = false;
    while let Some(found) = masked[search..].find(name) {
        let at = search + found;
        let end = at + name.len();
        search = end;
        if (at > 0 && is_ident_byte(bytes[at - 1]))
            || bytes.get(end).is_some_and(|b| is_ident_byte(*b))
        {
            continue;
        }
        // The binding itself: `let NAME` at the recorded offset.
        if !definition_seen
            && at > alias.line_offset
            && masked[alias.line_offset + 3..at].trim().is_empty()
        {
            definition_seen = true;
            continue;
        }
        if !definition_seen {
            return false;
        }
        let before = masked[..at].trim_end();
        let after = masked[end..].trim_start();
        if before.ends_with('.') || before.ends_with("::") || !after.starts_with('(') {
            return false;
        }
        // `fn NAME(` declares a nested function that shadows the alias.
        if let Some(prefix) = before.strip_suffix("fn")
            && !prefix.bytes().next_back().is_some_and(is_ident_byte)
        {
            return false;
        }
    }
    definition_seen
}
