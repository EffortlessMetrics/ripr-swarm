//! Owner-result binding for snapshot oracles on return-value seams.
//!
//! A snapshot observes only the value it names (#8120/#8121). The seam-kind
//! match that admits `OracleKind::Snapshot` for a `ReturnValue` seam cannot
//! tell a snapshot of the owner's result from a snapshot of some other value
//! in the same test, so the snapshot discriminates the seam only when it
//! names the owner call or a plain local bound to a direct owner call.
//! Everything else stays unconfirmed: under-credit, never over-credit.

use super::call_text_contains_named_call;
use super::related_tests::strip_comments_and_strings;
use crate::analysis::facts::{OracleFact, TestSummary};

/// Whether `oracle` (a snapshot) observes the result of `owner_name`.
pub(super) fn snapshot_observes_owner_result(
    owner_name: &str,
    oracle: &OracleFact,
    test: &TestSummary,
) -> bool {
    if owner_name.is_empty() {
        return false;
    }
    let snapshot = strip_oracle_text(&oracle.text);
    if call_text_contains_named_call(&snapshot, owner_name) {
        return true;
    }
    free_identifiers(&snapshot)
        .iter()
        .any(|name| name_is_direct_owner_result(name, owner_name, oracle.line, test))
}

fn strip_oracle_text(text: &str) -> String {
    text.lines()
        .map(strip_comments_and_strings)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Identifiers read as plain names: not a field/method (`.x`), a path
/// segment (`a::x`), a macro or a callee.
fn free_identifiers(code: &str) -> Vec<String> {
    let bytes = code.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !is_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_identifier_byte(bytes[index]) {
            index += 1;
        }
        let before = code[..start].trim_end().chars().next_back();
        let after = code[index..].trim_start().chars().next();
        let qualified = matches!(before, Some('.') | Some(':'));
        let callee_or_macro = matches!(after, Some('(') | Some('!') | Some(':'));
        if !qualified && !callee_or_macro {
            names.push(code[start..index].to_string());
        }
    }
    names
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The latest statement before `oracle_line` that introduces or reassigns
/// `name` must be a plain `let name = <expr naming the owner call>;`.
/// A `mut` binding, a destructuring pattern, a compound assignment, or a
/// rebinding to anything else withdraws the credit.
fn name_is_direct_owner_result(
    name: &str,
    owner_name: &str,
    oracle_line: usize,
    test: &TestSummary,
) -> bool {
    let mut bound_to_owner = false;
    let mut statement = String::new();
    for (offset, line) in test.body.lines().enumerate() {
        if test.start_line + offset >= oracle_line {
            break;
        }
        let code = strip_comments_and_strings(line);
        statement.push_str(&code);
        statement.push(' ');
        let trimmed = code.trim_end();
        if !(trimmed.ends_with(';') || trimmed.ends_with('{') || trimmed.ends_with('}')) {
            continue;
        }
        if let Some(verdict) = statement_binding_verdict(&statement, name, owner_name) {
            bound_to_owner = verdict;
        }
        statement.clear();
    }
    bound_to_owner
}

/// `Some(true)` for a plain owner binding of `name`, `Some(false)` for any
/// other statement that (re)binds or reassigns it, `None` when the
/// statement does not touch `name` as a binding.
fn statement_binding_verdict(statement: &str, name: &str, owner_name: &str) -> Option<bool> {
    let trimmed = statement.trim();
    if let Some(rest) = trimmed.strip_prefix("let ") {
        let (pattern, initializer) = match rest.split_once('=') {
            Some((pattern, initializer)) => (pattern.trim(), Some(initializer)),
            None => (rest.trim_end_matches(';').trim(), None),
        };
        if !contains_word(pattern, name) {
            return None;
        }
        let plain = pattern
            .split_once(':')
            .map_or(pattern, |(pattern, _)| pattern.trim())
            == name;
        return Some(
            plain
                && initializer.is_some_and(|initializer| {
                    call_text_contains_named_call(initializer, owner_name)
                }),
        );
    }
    let lhs = trimmed.split_once('=').map(|(lhs, _)| {
        lhs.trim_end_matches(['+', '-', '*', '/', '%', '&', '|', '^', '<', '>', '!'])
            .trim()
    });
    match lhs {
        Some(lhs) if contains_word(lhs, name) && !trimmed.starts_with("assert") => Some(false),
        _ => None,
    }
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + word.len()..].chars().next();
        !before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !after.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}
