//! Same-module bare-call facts for Python function and method owners.
//!
//! Records the names an owner's body calls as bare `name(` calls, so a test
//! that calls a same-module caller of a changed helper can be related to it
//! with a weak `helper_owner_call` relation (#4568). Attribute calls, names the
//! body rebinds, parameters, calls inside comments or strings, and nested
//! definitions' own names are not edges. Facts are textual and advisory: a
//! missing edge only withholds a weak relation, never grants an oracle.
//!
//! The text is the owner's whole span, so a call inside a nested `def` or
//! lambda counts as an edge of the enclosing owner (an over-approximation;
//! the relation it feeds stays weak). Call-site shape is shared with
//! `contains_call_name` through `is_bare_call_site`.

use super::PythonParameter;
use super::related_tests::{is_bare_call_site, is_python_identifier_char, text_binds_name};

/// Most distinct callee names kept per owner, so a generated body cannot make
/// the fact unbounded.
const MAX_CALLEES: usize = 64;

const NON_CALL_KEYWORDS: &[&str] = &[
    "if", "elif", "while", "for", "return", "and", "or", "not", "in", "is", "assert", "yield",
    "await", "lambda", "with", "except", "raise", "else", "del", "print", "match", "case",
];

/// Sorted, de-duplicated names `owner_text` calls as bare `name(` calls.
pub(super) fn bare_callees(owner_text: &str, parameters: &[PythonParameter]) -> Vec<String> {
    let mut callees: Vec<String> = Vec::new();
    for (open, _) in owner_text.match_indices('(') {
        let prefix = &owner_text[..open];
        let Some(start) = prefix
            .char_indices()
            .rev()
            .take_while(|(_, ch)| is_python_identifier_char(*ch))
            .last()
            .map(|(index, _)| index)
        else {
            continue;
        };
        let name = &prefix[start..];
        if name.chars().next().is_none_or(|ch| ch.is_ascii_digit())
            || NON_CALL_KEYWORDS.contains(&name)
            || callees.iter().any(|existing| existing == name)
            || parameters.iter().any(|parameter| parameter.name == name)
            || !is_bare_call_site(owner_text, start)
            || text_binds_name(owner_text, name)
        {
            continue;
        }
        callees.push(name.to_string());
        if callees.len() >= MAX_CALLEES {
            break;
        }
    }
    callees.sort();
    callees
}

/// Whether `owner_text` calls `name` anywhere other than its own `def name(`
/// header: `name(`, `self.name(` or `mod.name(`. Textual, so a mention in a
/// string or comment also counts; the claim it guards fails closed.
pub(super) fn calls_own_name(owner_text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    owner_text.match_indices(name).any(|(start, _)| {
        let before = owner_text[..start].chars().next_back();
        let rest = &owner_text[start + name.len()..];
        let boundary_before = !before.is_some_and(is_python_identifier_char);
        let boundary_after = !rest.chars().next().is_some_and(is_python_identifier_char);
        boundary_before
            && boundary_after
            && rest.trim_start().starts_with('(')
            && !owner_text[..start].trim_end().ends_with("def")
    })
}
