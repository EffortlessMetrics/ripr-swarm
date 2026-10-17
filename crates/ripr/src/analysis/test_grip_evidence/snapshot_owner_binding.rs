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
    // Bindings in scope, innermost last, each with the brace depth it was
    // introduced at. A block's closing brace drops its inner bindings so a
    // shadow in a finished block never masks the outer value.
    let mut scope: Vec<(usize, bool)> = Vec::new();
    let mut depth = 0usize;
    let mut statement_depth = 0usize;
    let mut statement = String::new();
    for (offset, line) in test.body.lines().enumerate() {
        if test.start_line + offset >= oracle_line {
            break;
        }
        let code = strip_comments_and_strings(line);
        if statement.is_empty() {
            statement_depth = depth;
        }
        statement.push_str(&code);
        statement.push(' ');
        for ch in code.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    scope.retain(|(bound_at, _)| *bound_at <= depth);
                }
                _ => {}
            }
        }
        let trimmed = code.trim_end();
        if !(trimmed.ends_with(';') || trimmed.ends_with('{') || trimmed.ends_with('}')) {
            continue;
        }
        for piece in statement.split_inclusive(';') {
            match statement_binding_verdict(piece, name, owner_name) {
                Some(BindingVerdict::Scoped(owner_bound)) => {
                    scope.retain(|(bound_at, _)| *bound_at < statement_depth);
                    scope.push((statement_depth, owner_bound));
                }
                // A reassignment or a pattern binder we do not scope-track may
                // leave any enclosing binding changed after its block ends.
                Some(BindingVerdict::Withdrawn) => {
                    for (_, owner_bound) in &mut scope {
                        *owner_bound = false;
                    }
                    scope.push((statement_depth, false));
                }
                None => {}
            }
        }
        statement.clear();
    }
    scope.last().is_some_and(|(_, owner_bound)| *owner_bound)
}

enum BindingVerdict {
    /// A `let` of `name`: scoped to its block.
    Scoped(bool),
    /// A reassignment or other binder of `name`: withdraws all credit.
    Withdrawn,
}

/// `Scoped(true)` for a plain owner binding of `name`, `Scoped(false)` for a
/// `let` that shadows it with anything else, `Withdrawn` for a reassignment
/// or a pattern binder (`if let`, `while let`, `for`, match arm, closure
/// parameter), `None` when the statement does not bind `name`.
fn statement_binding_verdict(
    statement: &str,
    name: &str,
    owner_name: &str,
) -> Option<BindingVerdict> {
    let trimmed = statement.trim();
    if let Some(rest) = trimmed.strip_prefix("let ") {
        let (pattern, initializer) = match rest.split_once('=') {
            Some((pattern, initializer)) => (pattern.trim(), Some(initializer)),
            None => (rest.trim_end_matches(';').trim(), None),
        };
        if !contains_word(pattern, name) {
            return binds_by_pattern(rest, name).then_some(BindingVerdict::Withdrawn);
        }
        let plain = pattern
            .split_once(':')
            .map_or(pattern, |(pattern, _)| pattern.trim())
            == name;
        return Some(BindingVerdict::Scoped(
            plain
                && initializer
                    .is_some_and(|initializer| initializer_is_owner_call(initializer, owner_name)),
        ));
    }
    if binds_by_pattern(trimmed, name) {
        return Some(BindingVerdict::Withdrawn);
    }
    match assignment_target(trimmed) {
        Some(target) if contains_word(target, name) => Some(BindingVerdict::Withdrawn),
        _ => None,
    }
}

/// `if let` / `while let` / `for` headers, match arms and closure
/// parameters that name `name`. Over-matching only withdraws credit.
fn binds_by_pattern(statement: &str, name: &str) -> bool {
    if !contains_word(statement, name) {
        return false;
    }
    let header = statement.trim_start_matches('}').trim_start();
    let header = header.strip_prefix("else ").unwrap_or(header);
    if header.starts_with("if let ")
        || header.starts_with("while let ")
        || header.starts_with("for ")
    {
        return true;
    }
    if statement.contains("=>") {
        let mut arms: Vec<&str> = statement.split("=>").collect();
        arms.pop();
        return arms.iter().any(|pattern| {
            let arm = pattern
                .rsplit_once([',', '{'])
                .map_or(*pattern, |(_, arm)| arm);
            contains_word(arm, name)
        });
    }
    statement.contains('|') && !statement.contains("||")
}

/// The initializer's top-level expression is the owner call itself, with an
/// optional `?` / `.unwrap()`: `make_count(2)`, `svc.make_count(2)?`. A call
/// wrapped in another call, comparison or method chain is another value.
fn initializer_is_owner_call(initializer: &str, owner_name: &str) -> bool {
    let text = initializer.trim().trim_end_matches(';').trim();
    let mut search = 0;
    while let Some(found) = text[search..].find(owner_name) {
        let start = search + found;
        search = start + owner_name.len();
        let before_ok = text[..start]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '.' | '&'));
        let boundary_ok = !text[..start]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !before_ok || !boundary_ok || !text[search..].trim_start().starts_with('(') {
            continue;
        }
        let args = &text[search..].trim_start();
        let mut depth = 0usize;
        for (at, ch) in args.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        let rest = args[at + 1..].trim();
                        return matches!(rest, "" | "?" | ".unwrap()");
                    }
                }
                _ => {}
            }
        }
    }
    false
}

/// The place expression assigned by `statement`, when it is an assignment
/// (`x = ..`, `x += ..`, `*x.y <<= ..`). Comparisons (`==`, `!=`, `<=`,
/// `>=`), match arms (`=>`) and any left-hand side that is not a plain
/// place (`if x == 0 {`, `debug_assert!(x == 3);`) are not assignments.
fn assignment_target(statement: &str) -> Option<&str> {
    let bytes = statement.as_bytes();
    let at = (0..bytes.len()).find(|&at| {
        bytes[at] == b'='
            && !matches!(bytes.get(at + 1), Some(b'=') | Some(b'>'))
            && !matches!(
                at.checked_sub(1).map(|prev| bytes[prev]),
                Some(b'=') | Some(b'!')
            )
            && !is_comparison_angle(bytes, at)
    })?;
    let target = statement[..at]
        .trim_end_matches(['+', '-', '*', '/', '%', '&', '|', '^', '<', '>'])
        .trim();
    let plain_place = !target.is_empty()
        && target
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '*'));
    plain_place.then_some(target)
}

/// `<=` / `>=` compare; `<<=` / `>>=` assign.
fn is_comparison_angle(bytes: &[u8], at: usize) -> bool {
    match at.checked_sub(1).map(|prev| bytes[prev]) {
        Some(angle @ (b'<' | b'>')) => at.checked_sub(2).map(|prev| bytes[prev]) != Some(angle),
        _ => false,
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

#[cfg(test)]
mod tests {
    use super::{assignment_target, initializer_is_owner_call, statement_binding_verdict};

    #[test]
    fn reads_and_comparisons_are_not_assignments() {
        for read in [
            "if count == 0 {",
            "debug_assert!(count == 3);",
            "while count >= 1 {",
            "if count <= 1 {",
            "assert!(count != 4);",
            "match count { 1 => {",
        ] {
            assert_eq!(assignment_target(read), None, "{read}");
            assert!(
                statement_binding_verdict(read, "count", "make_count").is_none(),
                "{read}"
            );
        }
    }

    #[test]
    fn plain_and_compound_assignments_are_assignments() {
        for (write, target) in [
            ("count = 5;", "count"),
            ("count += 1;", "count"),
            ("count <<= 1;", "count"),
            ("*count = 2;", "*count"),
            ("record.count -= 1;", "record.count"),
        ] {
            assert_eq!(assignment_target(write), Some(target), "{write}");
        }
        assert!(matches!(
            statement_binding_verdict("count = 5;", "count", "make_count"),
            Some(super::BindingVerdict::Withdrawn)
        ));
    }

    #[test]
    fn pattern_binders_withdraw_and_wrapped_calls_are_not_the_owner_result() {
        for binder in [
            "if let Some(count) = pick() {",
            "for count in items {",
            "Some(count) => {",
            "let f = |count| count + 1;",
        ] {
            let verdict = statement_binding_verdict(binder, "count", "make_count");
            assert!(
                matches!(
                    verdict,
                    Some(super::BindingVerdict::Withdrawn | super::BindingVerdict::Scoped(false))
                ),
                "{binder}"
            );
        }
        assert!(initializer_is_owner_call(" make_count(2);", "make_count"));
        assert!(initializer_is_owner_call(
            " svc.make_count(2)?;",
            "make_count"
        ));
        for other in [
            " make_count(2) == 3;",
            " unrelated(make_count(2));",
            " make_count(2).to_string();",
        ] {
            assert!(!initializer_is_owner_call(other, "make_count"), "{other}");
        }
    }
}
