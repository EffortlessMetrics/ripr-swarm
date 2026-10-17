//! Same-module bare-call facts for Python function and method owners.
//!
//! Records the names an owner's body calls as bare `name(` calls, so a test
//! that calls a same-module caller of a changed helper can be related to it
//! with a weak `helper_owner_call` relation (#4568). Attribute calls, names the
//! body rebinds, parameters, calls inside comments or strings, and nested
//! definitions' own names are not edges. Facts are textual and advisory: a
//! missing edge only withholds a weak relation, never grants an oracle.

use super::related_tests::{
    is_definition_name, is_python_identifier_char, line_prefix_looks_like_comment_or_string,
    python_text_hides_code, text_binds_name,
};
use super::{PythonParameter, python_callee_start_has_boundary};

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
            || !python_callee_start_has_boundary(owner_text, start)
            || line_prefix_looks_like_comment_or_string(owner_text, start)
            || python_text_hides_code(owner_text, start)
            || is_definition_name(owner_text, start)
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
