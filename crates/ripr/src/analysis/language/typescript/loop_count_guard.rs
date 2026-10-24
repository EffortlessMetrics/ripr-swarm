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

/// Characters whose neighbours' whitespace carries no meaning: an operator
/// or bracket that cannot fuse with the next token. `+` and `-` are absent on
/// purpose (`n + ++j` and `n++ + j` stay different), as are quotes.
fn spacing_is_trivia(c: char) -> bool {
    matches!(
        c,
        '(' | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | ','
            | ';'
            | '<'
            | '>'
            | '='
            | '!'
            | '&'
            | '|'
            | '?'
            | ':'
            | '*'
            | '/'
            | '%'
    )
}

/// A clause with each whitespace run outside a string or template literal
/// reduced to one space, and dropped when it touches a [`spacing_is_trivia`]
/// character. Literal contents (`counts["a b"]`) and `+`/`-` spacing keep
/// their meaning, so a changed key or operator target is never normalized
/// away. Any difference this keeps reads as a changed header, which only
/// keeps today's gap.
fn normalized_clause(clause: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut pending_space = false;
    for c in clause.trim().chars() {
        if let Some(open) = quote {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space
            && !out.chars().next_back().is_some_and(spacing_is_trivia)
            && !spacing_is_trivia(c)
        {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
        if matches!(c, '"' | '\'' | '`') {
            quote = Some(c);
        }
    }
    out
}

/// A `/` may open a regex literal, whose spaces and brackets mean something,
/// and the adapter does not tell it from division here, so any header holding
/// one is compared as written and an unproven normalization is a change.
fn normalized(clauses: [&str; 3]) -> [String; 3] {
    if clauses.iter().any(|clause| clause.contains('/')) {
        clauses.map(|clause| clause.trim().to_string())
    } else {
        clauses.map(normalized_clause)
    }
}

/// Both the old and the new line are counted-loop headers and they differ.
pub(crate) fn ts_loop_header_changed(old_line: &str, new_line: &str) -> bool {
    match (for_header_clauses(old_line), for_header_clauses(new_line)) {
        (Some(old), Some(new)) => normalized(old) != normalized(new),
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

/// The top-level arguments of the call whose `(` ends just before `after`.
fn call_arguments(after: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (at, ch) in after.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                args.push(after[start..at].trim());
                return args;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(after[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    // An unclosed call is unreadable.
    Vec::new()
}

/// Some call of one of `call_names` in `masked` (comments and literals
/// blanked) hands the plain identifier `mock` over as a whole argument. A
/// member call (`other.withRetry(fn)`) is another binding, and a member
/// argument (`other.fn`) is another value.
fn owner_call_passes(masked: &str, call_names: &[&str], mock: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '$');
    call_names.iter().any(|name| {
        [format!("{name}("), format!("{name}?.(")]
            .iter()
            .any(|needle| {
                masked.match_indices(needle.as_str()).any(|(at, _)| {
                    // The nearest code character, past whitespace and the blanks a
                    // masked comment leaves: `other. withRetry(` and
                    // `other./* c */withRetry(` are member calls too.
                    let prefix = &masked[..at];
                    let before = prefix.trim_end();
                    let immediate = prefix.chars().next_back();
                    !immediate.is_some_and(word)
                        && !before.ends_with('.')
                        && call_arguments(&masked[at + needle.len()..]).contains(&mock)
                })
            })
    })
}

/// Some related test that calls the owner directly, by import, or through a
/// renamed import asserts the call count of a mock it passes to that call.
fn counts_mock_calls(
    candidates: &[TypeScriptRelatedCandidate<'_>],
    owner: &TypeScriptOwner,
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.relation,
                TypeScriptRelationKind::DirectOwnerCall
                    | TypeScriptRelationKind::ImportedOwnerCall
                    | TypeScriptRelationKind::ImportAliasOwnerCall
            )
        })
        .any(|candidate| {
            let names =
                super::classifier::ts_owner_call_names(candidate, owner, alias_map, workspace_root);
            let masked = ts_mask_literals_and_comments(&candidate.test.body_text);
            candidate
                .test
                .assertions
                .iter()
                .filter_map(|assertion| assertion.observed_expression.as_deref())
                .filter_map(counted_mock)
                .any(|mock| owner_call_passes(&masked, &names, mock))
        })
}

/// The change rewrites a counted-loop header and a related test observes it
/// only through the call count of a mock it hands the owner, which the
/// adapter cannot tie to the iteration count.
pub(crate) fn ts_loop_count_unmodeled(
    old_line: Option<&str>,
    new_line: &str,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    old_line.is_some_and(|old| ts_loop_header_changed(old, new_line))
        && counts_mock_calls(candidates, owner, alias_map, workspace_root)
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
    fn formatting_trivia_is_no_header_change_but_meaning_is() {
        assert!(!ts_loop_header_changed(
            "for (let i = 0; i<n; i++) {",
            "for (let i = 0; i < n; i++) {"
        ));
        // A different key, or a different incremented variable, is a change.
        assert!(ts_loop_header_changed(
            "for (let i = 0; i < counts[\"a b\"]; i++) {",
            "for (let i = 0; i < counts[\"ab\"]; i++) {"
        ));
        assert!(ts_loop_header_changed(
            "for (let i = 0; i < n + ++j; i++) {",
            "for (let i = 0; i < n++ + j; i++) {"
        ));
        assert!(ts_loop_header_changed(
            "for (let i = 0; i < n - 1; i++) {",
            "for (let i = 0; i < n -1; i++) {"
        ));
    }

    #[test]
    fn a_header_holding_a_slash_is_compared_as_written() {
        // Regex literal bytes (spaces, brackets) change the iteration count.
        assert!(ts_loop_header_changed(
            "for (let i = 0; i < /a  b/.source.length; i++) {",
            "for (let i = 0; i < /a b/.source.length; i++) {"
        ));
        assert!(ts_loop_header_changed(
            "for (let i = 0; i < /a [b]/.source.length; i++) {",
            "for (let i = 0; i < /a[b]/.source.length; i++) {"
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

    fn passes(body: &str, names: &[&str], mock: &str) -> bool {
        owner_call_passes(&ts_mask_literals_and_comments(body), names, mock)
    }

    #[test]
    fn the_owner_call_must_pass_the_mock_as_a_whole_argument() {
        let names = ["withRetry"];
        assert!(passes("await withRetry(fn, 2);", &names, "fn"));
        assert!(passes("await withRetry?.(fn, 2);", &names, "fn"));
        assert!(!passes("withRetry(other, 2); log(fn);", &names, "fn"));
        assert!(!passes("withRetry(fnx, 2);", &names, "fn"));
        assert!(!passes("rewithRetry(fn, 2);", &names, "fn"));
        // Another value, another binding, and text that is not code.
        assert!(!passes("await withRetry(other.fn, 3);", &names, "fn"));
        assert!(!passes(
            "withRetry(real, 3); other.withRetry(fn, 2);",
            &names,
            "fn"
        ));
        assert!(!passes(
            "withRetry(real, 3); // withRetry(fn, 2)",
            &names,
            "fn"
        ));
        assert!(!passes(
            "withRetry(real, 3); log(\"withRetry(fn, 2)\");",
            &names,
            "fn"
        ));
        // Trivia between a member dot and its callee is still a member call.
        assert!(!passes("other.withRetry(fn, 2);", &names, "fn"));
        assert!(!passes("other. withRetry(fn, 2);", &names, "fn"));
        assert!(!passes("other.\nwithRetry(fn, 2);", &names, "fn"));
        assert!(!passes("other./* member */withRetry(fn, 2);", &names, "fn"));
        assert!(!passes("other?.withRetry(fn, 2);", &names, "fn"));
        // A renamed import is the owner under its local name.
        assert!(passes("await retry(fn, 2);", &["retry"], "fn"));
        assert!(!passes("await retry(fn, 2);", &names, "fn"));
    }

    #[test]
    fn nested_parentheses_in_the_test_clause_do_not_end_the_header() {
        assert_eq!(
            for_header_clauses("for (let i = 0; i < f(a, (b)); i++) {"),
            Some(["let i = 0", "i < f(a, (b))", "i++"])
        );
    }
}
