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

/// Which `receiver.owner(` spellings count as the owner call.
#[derive(Clone, Copy)]
enum Receivers<'a> {
    /// Only the bare name: another receiver's same-named member is another
    /// binding (`other.withRetry(fn)`).
    Bare,
    /// A method owner, which is always called through some receiver.
    AnyMember,
    /// Namespace imports of the owner's module (`ns.withRetry(fn)`).
    Namespaces(&'a [String]),
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// Whether `text` mentions `name` as a free identifier: a whole word that no
/// member dot precedes (`other.fn` is another value).
fn mentions_free(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(is_word)
            && !text[at + name.len()..].chars().next().is_some_and(is_word)
            && !text[..at].trim_end().ends_with('.')
    })
}

/// `mock` and every local assigned from an expression that mentions it
/// (`const task = fn;`, `const run = () => fn();`), closed under repetition.
fn mock_carriers<'a>(masked: &'a str, mock: &'a str) -> Vec<&'a str> {
    let mut carriers = vec![mock];
    loop {
        let mut grew = false;
        for statement in masked.split([';', '\n']) {
            let Some(rest) = ["const ", "let ", "var "]
                .iter()
                .find_map(|keyword| statement.trim_start().strip_prefix(keyword))
            else {
                continue;
            };
            let Some((name, rhs)) = rest.split_once('=') else {
                continue;
            };
            let name = name.trim();
            // An object or array literal holds the mock under a key or slot
            // (`{ fn: mock.fn() }`); reading it back is a member access.
            if rhs.trim_start().starts_with(['{', '['])
                || name.is_empty()
                || !name.chars().all(is_word)
                || carriers.contains(&name)
                || !carriers.iter().any(|carrier| mentions_free(rhs, carrier))
            {
                continue;
            }
            carriers.push(name);
            grew = true;
        }
        if !grew {
            return carriers;
        }
    }
}

/// Some call of one of `call_names` in `masked` (comments and literals
/// blanked) hands the counted mock to the owner: the mock, or a local
/// carrying it, is mentioned as a free identifier inside an argument
/// (`owner(fn)`, `owner((fn))`, `owner(fn as any)`, `owner(fn.bind(null))`,
/// `owner(() => fn())`, `owner(task)` after `const task = fn`). A member
/// argument (`other.fn`) is another value, and a member call
/// (`other.withRetry(fn)`) is another binding unless `receivers` allows it.
fn owner_call_passes(
    masked: &str,
    call_names: &[&str],
    receivers: Receivers<'_>,
    mock: &str,
) -> bool {
    let carriers = mock_carriers(masked, mock);
    call_names.iter().any(|name| {
        [format!("{name}("), format!("{name}?.(")]
            .iter()
            .any(|needle| {
                masked.match_indices(needle.as_str()).any(|(at, _)| {
                    // The nearest code character, past whitespace and the blanks a
                    // masked comment leaves: `other. withRetry(` and
                    // `other./* c */withRetry(` are member calls too.
                    let prefix = &masked[..at];
                    if prefix.chars().next_back().is_some_and(is_word) {
                        return false;
                    }
                    let before = prefix.trim_end();
                    let callee_ok = match before.strip_suffix('.') {
                        None => true,
                        Some(receiver) => {
                            let receiver =
                                receiver.strip_suffix('?').unwrap_or(receiver).trim_end();
                            let chain: String = receiver
                                .chars()
                                .rev()
                                .take_while(|c| is_word(*c) || *c == '.')
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect();
                            match receivers {
                                Receivers::Bare => false,
                                Receivers::AnyMember => !chain.is_empty(),
                                Receivers::Namespaces(names) => names.contains(&chain),
                            }
                        }
                    };
                    callee_ok
                        && call_arguments(&masked[at + needle.len()..])
                            .iter()
                            .any(|arg| carriers.iter().any(|carrier| mentions_free(arg, carrier)))
                })
            })
    })
}

/// Some related test that calls the owner directly, by import, through a
/// renamed or namespace import, or as a method asserts the call count of a
/// mock it hands to that call.
fn counts_mock_calls(
    candidates: &[TypeScriptRelatedCandidate<'_>],
    owner: &TypeScriptOwner,
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    let owner_is_method = matches!(owner.owner_kind, OwnerKind::Method | OwnerKind::ClassMethod);
    candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.relation,
                TypeScriptRelationKind::DirectOwnerCall
                    | TypeScriptRelationKind::ImportedOwnerCall
                    | TypeScriptRelationKind::ImportAliasOwnerCall
                    | TypeScriptRelationKind::ReceiverOwnerCall
                    | TypeScriptRelationKind::ClassMethodCall
            )
        })
        .any(|candidate| {
            // The shared union always lists the canonical name, but a test
            // that reaches the owner through a renamed import can declare
            // its own unrelated `owner` (or shadow it in an enclosing scope):
            // keep that spelling only where it is the binding that gave the
            // test its relation.
            let canonical_is_the_binding = candidate.relation
                != TypeScriptRelationKind::ImportAliasOwnerCall
                && !super::related_tests::local_identifier_declared_in_test_body(
                    &candidate.test.body_text,
                    &owner.name,
                )
                && !super::related_tests::enclosing_scope_shadows(candidate.test, &owner.name);
            let names: Vec<&str> =
                super::classifier::ts_owner_call_names(candidate, owner, alias_map, workspace_root)
                    .into_iter()
                    .filter(|name| *name != owner.name || canonical_is_the_binding)
                    .collect();
            let namespaces = if owner_is_method {
                Vec::new()
            } else {
                super::classifier::unshadowed_owner_namespace_receivers(
                    candidate.test,
                    owner,
                    alias_map,
                    workspace_root,
                )
            };
            let receivers = if owner_is_method {
                Receivers::AnyMember
            } else if namespaces.is_empty() {
                Receivers::Bare
            } else {
                Receivers::Namespaces(&namespaces)
            };
            let masked = ts_mask_literals_and_comments(&candidate.test.body_text);
            candidate
                .test
                .assertions
                .iter()
                .filter_map(|assertion| assertion.observed_expression.as_deref())
                .filter_map(counted_mock)
                .any(|mock| owner_call_passes(&masked, &names, receivers, mock))
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
        owner_call_passes(
            &ts_mask_literals_and_comments(body),
            names,
            Receivers::Bare,
            mock,
        )
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
    fn any_way_of_handing_the_mock_over_counts() {
        let names = ["withRetry"];
        for call in [
            "withRetry(() => fn(), 2);",
            "withRetry(fn.bind(null), 2);",
            "withRetry(fn as any, 2);",
            "withRetry((fn), 2);",
            "const task = fn; withRetry(task, 2);",
            "const run = () => fn(); const again = run; withRetry(again, 2);",
        ] {
            assert!(passes(call, &names, "fn"), "{call}");
        }
        // A local that does not carry the mock, or a member value, does not.
        assert!(!passes(
            "const task = other; withRetry(task, 2);",
            &names,
            "fn"
        ));
        assert!(!passes(
            "const task = other.fn; withRetry(task, 2);",
            &names,
            "fn"
        ));
        assert!(!passes("withRetry(() => other.fn(), 2);", &names, "fn"));
    }

    #[test]
    fn member_calls_count_only_where_the_owner_is_called_through_a_receiver() {
        let call = |receivers| {
            owner_call_passes(
                "svc.retry(fn, 2); ns.retry(fn, 2);",
                &["retry"],
                receivers,
                "fn",
            )
        };
        assert!(call(Receivers::AnyMember));
        assert!(!call(Receivers::Bare));
        assert!(owner_call_passes(
            "ns.retry(fn, 2);",
            &["retry"],
            Receivers::Namespaces(&["ns".to_string()]),
            "fn"
        ));
        assert!(!owner_call_passes(
            "other.retry(fn, 2);",
            &["retry"],
            Receivers::Namespaces(&["ns".to_string()]),
            "fn"
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
