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
/// `None` when the line is not one. A label (`outer: for (...)`) is skipped,
/// and a `;` or `)` inside a string or template literal does not end a clause.
fn for_header_clauses(line: &str) -> Option<[&str; 3]> {
    let mut rest = line.trim();
    if let Some((label, after)) = rest.split_once(':')
        && !label.is_empty()
        && label.chars().all(is_word)
        && after.trim_start().starts_with("for")
    {
        rest = after.trim_start();
    }
    let rest = rest.strip_prefix("for")?.trim_start();
    let inner = rest.strip_prefix('(')?;
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut end = None;
    let mut semicolons = Vec::new();
    for (at, ch) in inner.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                end = Some(at);
                break;
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => semicolons.push(at),
            _ => {}
        }
    }
    let header = &inner[..end?];
    // `for (const x of xs)` and `for (k in o)` have no clauses to count.
    let [first, second] = semicolons[..] else {
        return None;
    };
    let init = header[..first].trim();
    let test = header[first + 1..second].trim();
    let update = header[second + 1..].trim();
    if test.is_empty() {
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

/// Words in a receiver expression that name no mock.
const NOT_A_MOCK: [&str; 8] = [
    "as", "any", "await", "expect", "this", "not", "mock", "unknown",
];

/// Where the receiver expression ending at `end` begins: identifiers, member
/// dots and bracketed groups (`fns[0]`, `(fn as any)`), back to the nearest
/// character that cannot continue one.
fn receiver_start(text: &str, end: usize) -> usize {
    let mut depth = 0usize;
    let mut start = end;
    for (at, ch) in text[..end].char_indices().rev() {
        match ch {
            ')' | ']' => depth += 1,
            '(' | '[' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            c if depth == 0 && !(is_word(c) || matches!(c, '.' | '?' | '!')) => break,
            _ => {}
        }
        start = at;
    }
    start
}

/// Every identifier named in the receiver of a call-count read in `masked`:
/// `fn.mock.callCount()`, `fn.mock.calls.length`, `m.callCount()` after
/// `const m = fn.mock`, `(fn as any).mock.callCount()`, `fns[0].mock...`,
/// `expect(fn).toHaveBeenCalledTimes(n)`. Naming every identifier in the
/// receiver over-collects on purpose: a name that is no mock only matters
/// when the owner call also mentions it.
fn counted_mocks(masked: &str) -> Vec<&str> {
    let mut mocks = Vec::new();
    for needle in [
        ".callCount(",
        ".mock.calls",
        ".calls.length",
        "toHaveBeenCalledTimes(",
        "toBeCalledTimes(",
    ] {
        for (at, _) in masked.match_indices(needle) {
            let end = if needle.starts_with('.') {
                at
            } else {
                // `expect(fn).toHaveBeenCalledTimes(`: the dot before the matcher.
                match masked[..at].trim_end().strip_suffix('.') {
                    Some(receiver) => receiver.len(),
                    None => continue,
                }
            };
            let receiver = &masked[receiver_start(masked, end)..end];
            for word in receiver.split(|c: char| !is_word(c)) {
                let numeric = word.chars().next().is_none_or(|c| c.is_ascii_digit());
                if !numeric && !NOT_A_MOCK.contains(&word) && !mocks.contains(&word) {
                    mocks.push(word);
                }
            }
        }
    }
    mocks
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
/// member dot precedes (`other.fn` is another value). With `skip_keys`, a
/// word followed by `:` is an object key, not a value.
fn mentions_value(text: &str, name: &str, skip_keys: bool) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let (before, after) = (&text[..at], &text[at + name.len()..]);
        let member = before.trim_end().ends_with('.') && !before.ends_with("...");
        let key = skip_keys && after.trim_start().starts_with(':');
        !before.chars().next_back().is_some_and(is_word)
            && !after.chars().next().is_some_and(is_word)
            && !member
            && !key
    })
}

fn mentions_free(text: &str, name: &str) -> bool {
    mentions_value(text, name, false)
}

/// A declaration, assignment or function whose name can carry what its body
/// or right-hand side mentions.
struct Binding<'a> {
    names: Vec<&'a str>,
    rhs: &'a str,
    /// Calling it can run what it mentions (a function, an arrow, an alias).
    callable: bool,
}

/// The end of the declaration that starts `rest`: the first `;` or newline
/// outside any bracket, or an unmatched closer.
fn declaration_end(rest: &str) -> usize {
    let mut depth = 0usize;
    for (at, ch) in rest.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return at,
            ')' | ']' | '}' => depth -= 1,
            ';' | '\n' if depth == 0 => return at,
            _ => {}
        }
    }
    rest.len()
}

fn first_assignment(text: &str) -> Option<usize> {
    let bytes: Vec<char> = text.chars().collect();
    let mut at = 0usize;
    let mut offset = 0usize;
    while at < bytes.len() {
        let c = bytes[at];
        if c == '='
            && bytes.get(at + 1).is_none_or(|n| !matches!(n, '=' | '>'))
            && at
                .checked_sub(1)
                .and_then(|p| bytes.get(p))
                .is_none_or(|p| !matches!(p, '=' | '!' | '<' | '>'))
        {
            return Some(offset);
        }
        offset += c.len_utf8();
        at += 1;
    }
    None
}

fn is_callable(rhs: &str) -> bool {
    let rhs = rhs.trim_start();
    rhs.contains("=>")
        || rhs.starts_with("function")
        || rhs.starts_with("async")
        || rhs.contains(".bind(")
        || !rhs.contains('(')
}

fn bindings(masked: &str) -> Vec<Binding<'_>> {
    let mut found = Vec::new();
    for keyword in ["const", "let", "var"] {
        for (at, _) in masked.match_indices(keyword) {
            let after = &masked[at + keyword.len()..];
            if masked[..at].chars().next_back().is_some_and(is_word)
                || !after.chars().next().is_some_and(char::is_whitespace)
            {
                continue;
            }
            let declaration = &after[..declaration_end(after)];
            let Some(eq) = first_assignment(declaration) else {
                continue;
            };
            let (lhs, rhs) = (declaration[..eq].trim(), &declaration[eq + 1..]);
            let mut words = lhs.split(|c: char| !is_word(c)).filter(|w| !w.is_empty());
            let names: Vec<&str> = if lhs.starts_with(['{', '[']) {
                words.collect()
            } else {
                words.next().into_iter().collect()
            };
            found.push(Binding {
                names,
                rhs,
                callable: is_callable(rhs),
            });
        }
    }
    for (at, _) in masked.match_indices("function") {
        let after = &masked[at + "function".len()..];
        if masked[..at].chars().next_back().is_some_and(is_word) {
            continue;
        }
        let trimmed = after.trim_start_matches(|c: char| c.is_whitespace() || c == '*');
        let name_len = trimmed.find(|c: char| !is_word(c)).unwrap_or(trimmed.len());
        if name_len == 0 {
            continue;
        }
        let body = &trimmed[name_len..];
        let end = body.find('{').map_or(body.len(), |open| {
            let mut depth = 0usize;
            for (at, ch) in body[open..].char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return open + at + 1;
                        }
                    }
                    _ => {}
                }
            }
            body.len()
        });
        found.push(Binding {
            names: vec![&trimmed[..name_len]],
            rhs: &body[..end],
            callable: true,
        });
    }
    for statement in statements(masked) {
        let statement = statement.trim();
        let name_len = statement
            .find(|c: char| !is_word(c))
            .unwrap_or(statement.len());
        let rest = statement[name_len..].trim_start();
        if name_len > 0
            && rest.starts_with('=')
            && !rest.starts_with("==")
            && !rest.starts_with("=>")
        {
            found.push(Binding {
                names: vec![&statement[..name_len]],
                rhs: &rest[1..],
                callable: false,
            });
        }
    }
    found
}

/// Whether the `{` that ends `before` opens a statement block (`=> {`,
/// `) {`, `try {`) rather than an object literal.
fn opens_block(before: &str) -> bool {
    let before = before.trim_end();
    before.ends_with('>')
        || before.ends_with(')')
        || ["try", "else", "finally", "do"].iter().any(|keyword| {
            before.ends_with(keyword)
                && !before[..before.len() - keyword.len()]
                    .chars()
                    .next_back()
                    .is_some_and(is_word)
        })
}

/// The statements of `masked`: split at `;` and newlines outside
/// parentheses, brackets and object literals, so a multi-line argument list
/// stays whole, and at the edges of every statement block, so the body of a
/// test callback splits like top-level code. A newline beside a member dot
/// continues the chain.
fn statements(masked: &str) -> Vec<&str> {
    // `Some(depth)` is a block that saved the enclosing nesting depth.
    let mut open: Vec<Option<usize>> = Vec::new();
    let mut found = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (at, ch) in masked.char_indices() {
        match ch {
            '{' if opens_block(&masked[..at]) => {
                found.push(&masked[start..at]);
                start = at + 1;
                open.push(Some(depth));
                depth = 0;
            }
            '(' | '[' | '{' => {
                open.push(None);
                depth += 1;
            }
            ')' | ']' | '}' => match open.pop() {
                Some(Some(saved)) => {
                    found.push(&masked[start..at]);
                    start = at + 1;
                    depth = saved;
                }
                Some(None) => depth = depth.saturating_sub(1),
                None => {}
            },
            ';' if depth == 0 => {
                found.push(&masked[start..at]);
                start = at + 1;
            }
            '\n' if depth == 0 => {
                let continues = masked[start..at].trim_end().ends_with('.')
                    || masked[at..].trim_start().starts_with('.');
                if !continues {
                    found.push(&masked[start..at]);
                    start = at + 1;
                }
            }
            _ => {}
        }
    }
    found.push(&masked[start..]);
    found
}

/// `mock` and every local assigned from an expression that mentions it
/// (`const task = fn`, `const run = () => fn()`, `const args = [fn, 2]`,
/// `const { run } = { run: fn }`, `let t; t = fn`), closed under repetition.
/// A count read from the mock (`const n = fn.mock.callCount()`) is no carrier.
fn mock_carriers(bindings: &[Binding<'_>], mock: &str) -> Vec<String> {
    let mut carriers = vec![mock.to_string()];
    loop {
        let mut grew = false;
        for binding in bindings {
            let rhs = binding.rhs;
            if rhs.contains(".callCount(") || rhs.contains(".mock.calls") {
                continue;
            }
            // Under an object literal `fn:` is a key, not a use of `fn`.
            let skip_keys = rhs.trim_start().starts_with('{');
            if !carriers.iter().any(|c| mentions_value(rhs, c, skip_keys)) {
                continue;
            }
            for name in &binding.names {
                if !carriers.iter().any(|c| c == name) {
                    carriers.push((*name).to_string());
                    grew = true;
                }
            }
        }
        if !grew {
            return carriers;
        }
    }
}

/// The owner's call names plus every callable local that runs one
/// (`const r = withRetry`, `const go = (f) => withRetry(f, 2)`,
/// `function go(f) { return withRetry(f, 2); }`).
fn owner_names(
    bindings: &[Binding<'_>],
    call_names: &[&str],
    receivers: Receivers<'_>,
) -> Vec<String> {
    let mut owners: Vec<String> = call_names.iter().map(|n| (*n).to_string()).collect();
    loop {
        let mut grew = false;
        for binding in bindings.iter().filter(|b| b.callable) {
            if !owners
                .iter()
                .any(|o| mentions_owner(binding.rhs, o, receivers))
            {
                continue;
            }
            for name in &binding.names {
                if !owners.iter().any(|o| o == name) {
                    owners.push((*name).to_string());
                    grew = true;
                }
            }
        }
        if !grew {
            return owners;
        }
    }
}

/// `statement` mentions the owner as a callee or value: the name as a whole
/// word, bare or through a receiver `receivers` allows, in any spelling
/// (`name(`, `name<T>(`, `name!(`, `(0, name)(`, `name.call(`).
fn mentions_owner(statement: &str, name: &str, receivers: Receivers<'_>) -> bool {
    statement.match_indices(name).any(|(at, _)| {
        // The nearest code character, past whitespace and the blanks a
        // masked comment leaves: `other. withRetry(` and
        // `other./* c */withRetry(` are member calls too.
        let prefix = &statement[..at];
        if prefix.chars().next_back().is_some_and(is_word)
            || statement[at + name.len()..]
                .chars()
                .next()
                .is_some_and(is_word)
        {
            return false;
        }
        match prefix.trim_end().strip_suffix('.') {
            None => true,
            Some(receiver) => {
                let receiver = receiver.strip_suffix('?').unwrap_or(receiver).trim_end();
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
        }
    })
}

/// Some statement in `masked` (comments and literals blanked) runs the owner
/// and mentions the counted mock, or a local carrying it, in the same
/// statement. This is the explicit conservative policy: it does not parse
/// argument positions, so a handoff in any spelling (`owner(fn)`,
/// `owner<T>(fn)`, `owner.call(null, fn)`, `owner(() => fn())`,
/// `owner(...args)` after `const args = [fn, 2]`, an aliased or wrapped
/// owner) abstains, and so does a mock that merely shares a statement with
/// the owner. A member value (`other.fn`), another receiver's same-named
/// member and text in comments or strings do not count.
fn hands_mock_to_owner(
    masked: &str,
    call_names: &[&str],
    receivers: Receivers<'_>,
    mock: &str,
) -> bool {
    let bindings = bindings(masked);
    let carriers = mock_carriers(&bindings, mock);
    let owners = owner_names(&bindings, call_names, receivers);
    statements(masked).iter().any(|statement| {
        owners
            .iter()
            .any(|owner| mentions_owner(statement, owner, receivers))
            && carriers.iter().any(|c| mentions_free(statement, c))
    })
}

/// Some related test, however the link was found, that runs the owner
/// (directly, by import, through a renamed or namespace import, as a method,
/// with type arguments) and asserts the call count of a
/// mock it hands to that call.
fn counts_mock_calls(
    candidates: &[TypeScriptRelatedCandidate<'_>],
    owner: &TypeScriptOwner,
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    let owner_is_method = matches!(owner.owner_kind, OwnerKind::Method | OwnerKind::ClassMethod);
    candidates.iter().any(|candidate| {
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
        counted_mocks(&masked)
            .into_iter()
            .any(|mock| hands_mock_to_owner(&masked, &names, receivers, mock))
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
    fn counted_mocks_are_read_from_every_count_spelling() {
        let mocks = |text: &str| -> Vec<String> {
            let masked = ts_mask_literals_and_comments(text);
            counted_mocks(&masked)
                .into_iter()
                .map(String::from)
                .collect()
        };
        assert_eq!(mocks("fn.mock.callCount()"), ["fn"]);
        assert_eq!(mocks("send.mock.calls.length"), ["send"]);
        assert_eq!(mocks("(fn as any).mock.callCount()"), ["fn"]);
        assert_eq!(mocks("fns[0].mock.callCount()"), ["fns"]);
        assert_eq!(mocks("a.b.mock.callCount()"), ["a", "b"]);
        assert_eq!(mocks("const m = fn.mock; m.callCount()"), ["m"]);
        assert_eq!(mocks("const n = fn.mock.calls; n.length"), ["fn"]);
        assert_eq!(mocks("expect(fn).toHaveBeenCalledTimes(2)"), ["fn"]);
        assert!(mocks("syscalls.length").is_empty());
        assert!(mocks("getCallCount()").is_empty());
        assert!(mocks("// fn.mock.callCount()").is_empty());
    }

    fn passes(body: &str, names: &[&str], mock: &str) -> bool {
        hands_mock_to_owner(
            &ts_mask_literals_and_comments(body),
            names,
            Receivers::Bare,
            mock,
        )
    }

    #[test]
    fn the_owner_call_must_mention_the_mock_as_a_free_identifier() {
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
            "withRetry(c ? fn : g, 2);",
            "withRetry(...[fn, 2]);",
            "const task = fn; withRetry(task, 2);",
            "const run = () => fn(); const again = run; withRetry(again, 2);",
            "const args = [fn, 2] as const; withRetry(...args);",
            "const { run } = { run: fn }; withRetry(run, 2);",
            "const [t] = [fn]; withRetry(t, 2);",
            "const h = { fn }; withRetry(h.fn, 2);",
            "const opts = { task: fn }; withRetry(opts.task, 2);",
            "let t; t = fn; withRetry(t, 2);",
            "withRetry<number>(fn, 2);",
            "withRetry!(fn, 2);",
            "(withRetry)(fn, 2);",
            "(0, withRetry)(fn, 2);",
            "withRetry.call(null, fn, 2);",
            "withRetry.apply(null, [fn, 2]);",
            "withRetry.bind(null, fn, 2)();",
            "const r = withRetry; r(fn, 2);",
            "const go = (f) => withRetry(f, 2); go(fn);",
            "function go(f) {\n  return withRetry(f, 2);\n}\ngo(fn);",
            "withRetry(\n  fn,\n  2,\n);",
        ] {
            assert!(passes(call, &names, "fn"), "{call}");
        }
        // A local that does not carry the mock, or a member value, does not.
        for call in [
            "const task = other; withRetry(task, 2);",
            "const task = other.fn; withRetry(task, 2);",
            "withRetry(() => other.fn(), 2);",
            "const other = { fn: g }; withRetry(other.fn, 2);",
            "const n = fn.mock.callCount(); withRetry(real, n);",
            "const go = (f) => withRetry(f, 2); go(real); log(fn);",
        ] {
            assert!(!passes(call, &names, "fn"), "{call}");
        }
    }

    #[test]
    fn a_labelled_or_quoted_header_is_still_a_counted_header() {
        assert!(ts_loop_header_changed(
            "outer: for (let i = 0; i < attempts; i++) {",
            "outer: for (let i = 0; i < attempts - 1; i++) {"
        ));
        assert_eq!(
            for_header_clauses("for (let i = 0; i < \"a;b)\".length; i++) {"),
            Some(["let i = 0", "i < \"a;b)\".length", "i++"])
        );
        assert!(ts_loop_header_changed(
            "for (let i = 0; i < \"a;b\".length; i++) {",
            "for (let i = 0; i < \"a;bc\".length; i++) {"
        ));
    }

    #[test]
    fn member_calls_count_only_where_the_owner_is_called_through_a_receiver() {
        let call = |receivers| {
            hands_mock_to_owner(
                "svc.retry(fn, 2); ns.retry(fn, 2);",
                &["retry"],
                receivers,
                "fn",
            )
        };
        assert!(call(Receivers::AnyMember));
        assert!(!call(Receivers::Bare));
        assert!(hands_mock_to_owner(
            "ns.retry(fn, 2);",
            &["retry"],
            Receivers::Namespaces(&["ns".to_string()]),
            "fn"
        ));
        assert!(!hands_mock_to_owner(
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
