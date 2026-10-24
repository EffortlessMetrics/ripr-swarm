//! RIPR-SPEC-0246 rule 8: a counted-loop header change read only through a
//! mock call count is not a gap the preview adapter can establish.
//!
//! `for (let i = 0; i < attempts; i++)` becoming `i < attempts - 1` changes
//! how many times the loop body runs. A test that counts the calls of a mock
//! the body invokes (`fn.mock.callCount()`, `fn.mock.calls.length`) can notice
//! that, but only when the count it asserts equals the old iteration count
//! under the inputs it passes, and the adapter does not model iteration
//! counts. The predicate-boundary witness the ladder asks for does not apply
//! to a loop header, so the ladder's gap would be a guess. The finding reads
//! `static_unknown`; nothing is credited.

use super::*;

/// The three clauses of a counted `for (init; test; update)` header, or
/// `None` when the line is not one.
fn for_header_clauses(line: &str) -> Option<[&str; 3]> {
    let rest = line.trim().strip_prefix("for")?.trim_start();
    let inner = rest.strip_prefix('(')?;
    let mut depth = 0usize;
    let mut end = None;
    for (at, ch) in inner.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                end = Some(at);
                break;
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let header = &inner[..end?];
    // `for (const x of xs)` and `for (k in o)` have no clauses to count.
    let mut clauses = header.split(';');
    let init = clauses.next()?.trim();
    let test = clauses.next()?.trim();
    let update = clauses.next()?.trim();
    if clauses.next().is_some() || test.is_empty() {
        return None;
    }
    Some([init, test, update])
}

/// The clauses with all whitespace removed, so a spacing-only edit is no change.
fn squeezed(clauses: [&str; 3]) -> [String; 3] {
    clauses.map(|clause| clause.chars().filter(|c| !c.is_whitespace()).collect())
}

/// Both the old and the new line are counted-loop headers and they differ.
pub(crate) fn ts_loop_header_changed(old_line: &str, new_line: &str) -> bool {
    match (for_header_clauses(old_line), for_header_clauses(new_line)) {
        (Some(old), Some(new)) => squeezed(old) != squeezed(new),
        _ => false,
    }
}

/// The mock a call-count expression reads: `fn` in `fn.mock.callCount()` or
/// `fn.mock.calls.length`, when it is a plain identifier.
fn counted_mock(observed: &str) -> Option<&str> {
    let at = observed
        .find(".mock.callCount(")
        .or_else(|| observed.find(".mock.calls.length"))?;
    let receiver = observed[..at].trim();
    let name = receiver
        .rsplit(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '$')))
        .next()?;
    (!name.is_empty() && name.len() == receiver.len()).then_some(name)
}

/// `owner(` appears in `body` with `name` as a word inside its argument list.
fn owner_call_passes(body: &str, owner_name: &str, name: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '$');
    body.match_indices(&format!("{owner_name}("))
        .any(|(at, _)| {
            let args = &body[at + owner_name.len() + 1..];
            let mut depth = 0usize;
            let mut end = args.len();
            for (i, ch) in args.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' if depth == 0 => {
                        end = i;
                        break;
                    }
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            args[..end].match_indices(name).any(|(i, _)| {
                !args[..i].chars().next_back().is_some_and(word)
                    && !args[i + name.len()..].chars().next().is_some_and(word)
            })
        })
}

/// Some oracle-eligible related test asserts the call count of a mock it
/// passes to the owner.
fn counts_mock_calls(candidates: &[TypeScriptRelatedCandidate<'_>], owner_name: &str) -> bool {
    candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            candidate
                .test
                .assertions
                .iter()
                .filter_map(|assertion| assertion.observed_expression.as_deref())
                .filter_map(counted_mock)
                .any(|name| owner_call_passes(&candidate.test.body_text, owner_name, name))
        })
}

/// The change rewrites a counted-loop header and a related test observes it
/// only through the call count of a mock it hands the owner, which the
/// adapter cannot tie to the iteration count.
pub(crate) fn ts_loop_count_unmodeled(
    old_line: Option<&str>,
    new_line: &str,
    owner_name: &str,
    candidates: &[TypeScriptRelatedCandidate<'_>],
) -> bool {
    old_line.is_some_and(|old| ts_loop_header_changed(old, new_line))
        && counts_mock_calls(candidates, owner_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_bound_in_a_counted_header_is_a_loop_header_change() {
        assert!(ts_loop_header_changed(
            "  for (let i = 0; i < attempts; i++) {",
            "  for (let i = 0; i < attempts - 1; i++) {"
        ));
        assert!(ts_loop_header_changed(
            "for (let i = 0; i < n; i++) {",
            "for (let i = 1; i < n; i++) {"
        ));
    }

    #[test]
    fn an_unchanged_header_or_a_non_counted_loop_is_not() {
        assert!(!ts_loop_header_changed(
            "for (let i = 0; i < n; i++) {",
            "for (let i = 0; i < n; i++) { // note"
        ));
        assert!(!ts_loop_header_changed(
            "for (const x of xs) {",
            "for (const y of ys) {"
        ));
        assert!(!ts_loop_header_changed(
            "if (i < attempts) {",
            "if (i < attempts - 1) {"
        ));
        assert!(!ts_loop_header_changed(
            "while (i < attempts) {",
            "while (i < attempts - 1) {"
        ));
    }

    #[test]
    fn a_spacing_only_edit_is_no_header_change() {
        assert!(!ts_loop_header_changed(
            "for (let i = 0; i<n; i++) {",
            "for (let i = 0; i < n; i++) {"
        ));
    }

    #[test]
    fn the_counted_mock_is_a_plain_receiver() {
        assert_eq!(counted_mock("fn.mock.callCount()"), Some("fn"));
        assert_eq!(counted_mock("send.mock.calls.length"), Some("send"));
        assert_eq!(counted_mock("syscalls.length"), None);
        assert_eq!(counted_mock("getCallCount()"), None);
        assert_eq!(counted_mock("a.b.mock.callCount()"), None);
    }

    #[test]
    fn the_owner_call_must_pass_the_mock() {
        assert!(owner_call_passes(
            "await withRetry(fn, 2);",
            "withRetry",
            "fn"
        ));
        assert!(!owner_call_passes(
            "withRetry(other, 2); log(fn);",
            "withRetry",
            "fn"
        ));
        assert!(!owner_call_passes("withRetry(fnx, 2);", "withRetry", "fn"));
    }

    #[test]
    fn nested_parentheses_in_the_test_clause_do_not_end_the_header() {
        assert_eq!(
            for_header_clauses("for (let i = 0; i < f(a, (b)); i++) {"),
            Some(["let i = 0", "i < f(a, (b))", "i++"])
        );
    }
}
