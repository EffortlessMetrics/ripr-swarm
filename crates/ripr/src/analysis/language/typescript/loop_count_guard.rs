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

/// Both the old and the new line are counted-loop headers and they differ.
pub(crate) fn ts_loop_header_changed(old_line: &str, new_line: &str) -> bool {
    match (for_header_clauses(old_line), for_header_clauses(new_line)) {
        (Some(old), Some(new)) => old != new,
        _ => false,
    }
}

/// Some oracle-eligible related test asserts a mock's call count.
fn counts_mock_calls(candidates: &[TypeScriptRelatedCandidate<'_>]) -> bool {
    candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .flat_map(|candidate| &candidate.test.assertions)
        .filter_map(|assertion| assertion.observed_expression.as_deref())
        .any(|observed| observed.contains("callCount(") || observed.contains("calls.length"))
}

/// The change rewrites a counted-loop header and a related test observes it
/// only through a mock call count the adapter cannot tie to the iteration
/// count.
pub(crate) fn ts_loop_count_unmodeled(
    old_line: Option<&str>,
    new_line: &str,
    candidates: &[TypeScriptRelatedCandidate<'_>],
) -> bool {
    old_line.is_some_and(|old| ts_loop_header_changed(old, new_line))
        && counts_mock_calls(candidates)
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
    fn nested_parentheses_in_the_test_clause_do_not_end_the_header() {
        assert_eq!(
            for_header_clauses("for (let i = 0; i < f(a, (b)); i++) {"),
            Some(["let i = 0", "i < f(a, (b))", "i++"])
        );
    }
}
