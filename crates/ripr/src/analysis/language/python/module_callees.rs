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

/// Sorted, de-duplicated names the owner's own scope calls as bare `name(`
/// calls: [`bare_callees`] over [`own_scope_text`]. The route-helper relation
/// (#14150) reads this, because it claims the handler itself calls the
/// helper; the relations that fed [`bare_callees`] before keep that reading.
pub(super) fn own_scope_callees(owner_text: &str, parameters: &[PythonParameter]) -> Vec<String> {
    bare_callees(&own_scope_text(owner_text), parameters)
}

/// `owner_text` with the bodies of nested `def` and `async def` lines, and
/// everything from a `lambda` on its line, removed. Textual, like the rest of
/// this module. A `def ` opening a line at any indentation ends at the next
/// line indented no deeper. A `lambda` outside strings and comments cuts its
/// line; when the line still has an open bracket, the lines that close it are
/// dropped too, because a lambda body may continue inside them. A helper the
/// handler calls only through a nested function is therefore not credited
/// even when the handler invokes that function. Both rules only withhold a
/// weak relation, never grant one.
fn own_scope_text(owner_text: &str) -> String {
    let lines: Vec<&str> = owner_text.split('\n').collect();
    // The owner text starts at its first statement, so the first line has no
    // indentation; its real one is the shallowest the following lines show.
    let base_indent = lines
        .iter()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    let mut kept: Vec<&str> = Vec::new();
    let mut nested_def_indent: Option<usize> = None;
    let mut open_brackets: i64 = 0;
    for (index, line) in lines.iter().enumerate() {
        let body = line.trim_start_matches([' ', '\t']);
        let indent = if index == 0 {
            base_indent
        } else {
            line.len() - body.len()
        };
        if open_brackets > 0 {
            open_brackets += bracket_delta(line);
            continue;
        }
        if let Some(def_indent) = nested_def_indent {
            if body.is_empty() || indent > def_indent {
                continue;
            }
            nested_def_indent = None;
        }
        if body.starts_with("def ") || body.starts_with("async def ") {
            nested_def_indent = Some(indent);
            continue;
        }
        match lambda_at(line) {
            Some(at) => {
                open_brackets = bracket_delta(line).max(0);
                kept.push(&line[..at]);
            }
            None => kept.push(line),
        }
    }
    kept.join("\n")
}

/// Byte index of the `lambda` keyword on the line, outside strings, comments
/// and docstrings, or `None`.
fn lambda_at(line: &str) -> Option<usize> {
    line.match_indices("lambda").find_map(|(at, _)| {
        let after = line[at + "lambda".len()..].chars().next();
        (super::static_limits::python_callee_start_has_boundary(line, at)
            && !code_is_hidden(line, at)
            && after.is_none_or(|ch| !is_python_identifier_char(ch)))
        .then_some(at)
    })
}

/// Open minus closed brackets on the line, outside strings and comments.
fn bracket_delta(text: &str) -> i64 {
    text.char_indices()
        .filter(|(at, _)| !code_is_hidden(text, *at))
        .map(|(_, ch)| match ch {
            '(' | '[' | '{' => 1,
            ')' | ']' | '}' => -1,
            _ => 0,
        })
        .sum()
}

fn code_is_hidden(text: &str, at: usize) -> bool {
    super::related_tests::line_prefix_looks_like_comment_or_string(text, at)
        || super::related_tests::python_text_hides_code(text, at)
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
