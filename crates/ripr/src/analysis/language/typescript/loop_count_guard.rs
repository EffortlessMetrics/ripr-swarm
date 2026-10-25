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

/// The root identifier of the receiver of every call-count read in `masked`:
/// `fn` in `fn.mock.callCount()`, `fn.mock.calls.length`, `m.callCount()`,
/// `(fn as any).mock.callCount()`; `fns` in `fns[0].mock...`; `other` in
/// `other.fn.mock.callCount()`, since a property mock is not the local `fn`.
fn counter_roots(masked: &str) -> Vec<&str> {
    let mut roots = Vec::new();
    for needle in [
        ".callCount",
        ".calledOnce",
        ".calledTwice",
        ".calledThrice",
        ".getCalls(",
        ".mock.calls",
        ".mock.results",
        ".calls.length",
    ] {
        for (at, _) in masked.match_indices(needle) {
            let receiver = &masked[receiver_start(masked, at)..at];
            let root = receiver.split(|c: char| !is_word(c)).find(|word| {
                !word.is_empty()
                    && !word.chars().all(|c| c.is_ascii_digit())
                    && !NOT_A_MOCK.contains(word)
            });
            if let Some(root) = root
                && !roots.contains(&root)
            {
                roots.push(root);
            }
        }
    }
    roots
}

/// The mocks a related test asserts the call count of: a count read inside an
/// assertion's observed or expected expression, a local that holds one
/// (`const n = fn.mock.callCount(); assert.equal(n, 2)`), a local that
/// aliases the mock's `.mock` (`const m = fn.mock`) mapped back to the mock,
/// and `expect(fn).toHaveBeenCalledTimes(n)`. A count that no assertion reads
/// cannot fail the test, so it asserts nothing.
fn asserted_counted_mocks<'a>(
    masked: &'a str,
    bindings: &[Binding<'a>],
    assertions: &[TypeScriptAssertion],
) -> Vec<&'a str> {
    let mut mocks: Vec<&str> = Vec::new();
    fn add<'a>(mocks: &mut Vec<&'a str>, root: &'a str) {
        if !mocks.contains(&root) {
            mocks.push(root);
        }
    }
    for assertion in assertions {
        let texts: Vec<String> = [
            assertion.observed_expression.as_deref(),
            assertion.dynamic_expected_text.as_deref(),
            assertion.expected_value_or_variant.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(ts_mask_literals_and_comments)
        .collect();
        let times_matcher = ["toHaveBeenCalledTimes", "toBeCalledTimes"]
            .iter()
            .any(|m| assertion.matcher.contains(m));
        for text in &texts {
            // The mask preserves length, so the roots below index `masked`
            // only through the bindings; read them from the expression itself
            // by locating the same names in the body.
            let roots: Vec<String> = counter_roots(text).into_iter().map(String::from).collect();
            for root in roots {
                if let Some(found) = find_word(masked, &root) {
                    add(&mut mocks, found);
                }
            }
            for binding in bindings {
                let uses_name = binding
                    .names
                    .iter()
                    .any(|name| mentions_value(text, name, false));
                // `const { calls } = fn.mock; calls.length`: a local holding
                // the mock's `.mock` stands for the mock.
                let rhs = binding.rhs.trim();
                if uses_name
                    && (rhs.ends_with(".mock") || rhs.contains(".mock."))
                    && let Some(root) = rhs
                        .split(|c: char| !is_word(c))
                        .find(|w| !w.is_empty() && !NOT_A_MOCK.contains(w))
                {
                    add(&mut mocks, root);
                }
                let holds_count = !counter_roots(binding.rhs).is_empty();
                if holds_count
                    && binding
                        .names
                        .iter()
                        .any(|name| mentions_value(text, name, false))
                {
                    for root in counter_roots(binding.rhs) {
                        add(&mut mocks, root);
                    }
                }
            }
            if times_matcher {
                let root = text
                    .split(|c: char| !is_word(c))
                    .find(|w| !w.is_empty() && !NOT_A_MOCK.contains(w));
                if let Some(found) = root.and_then(|root| find_word(masked, root)) {
                    add(&mut mocks, found);
                }
            }
        }
    }
    // `const spy = fn; spy.mock.calls.length`, `const fns = [fn]`, `{ fn }`:
    // a local built from plain references stands for what it mentions.
    loop {
        let before = mocks.len();
        for binding in bindings {
            let mut rhs = binding.rhs.trim();
            // `(fn)`, `((fn as any))`: grouping parentheses are not a call.
            while let Some(inner) = rhs.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
                rhs = inner.trim();
            }
            if rhs.contains('(') || !binding.names.iter().any(|n| mocks.contains(n)) {
                continue;
            }
            for word in rhs.split(|c: char| !is_word(c)) {
                let first = word.chars().next();
                if !word.is_empty()
                    && !first.is_some_and(|c| c.is_ascii_digit())
                    && !NOT_A_MOCK.contains(&word)
                    && !mention_positions(rhs, 0, rhs.len(), word).is_empty()
                {
                    add(&mut mocks, word);
                }
            }
        }
        if mocks.len() == before {
            break;
        }
    }
    // `m.callCount()` after `const m = fn.mock`: the mock is the root of the
    // aliased `.mock` expression.
    for binding in bindings {
        let rhs = binding.rhs.trim();
        if let Some(receiver) = rhs.strip_suffix(".mock")
            && binding.names.iter().any(|name| mocks.contains(name))
            && let Some(root) = receiver
                .split(|c: char| !is_word(c))
                .find(|w| !w.is_empty() && !NOT_A_MOCK.contains(w))
        {
            add(&mut mocks, root);
        }
    }
    mocks
}

/// The first whole-word occurrence of `word` in `text`, as a slice of it.
fn find_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    text.match_indices(word).find_map(|(at, _)| {
        let whole = !text[..at].chars().next_back().is_some_and(is_word)
            && !text[at + word.len()..].chars().next().is_some_and(is_word);
        whole.then(|| &text[at..at + word.len()])
    })
}

/// Which `receiver.owner(` spellings count as the owner call.
#[derive(Clone, Copy)]
enum Receivers<'a> {
    /// Only the bare name: another receiver's same-named member is another
    /// binding (`other.withRetry(fn)`).
    Bare,
    /// Receivers the test authenticates as the owner's: namespace imports of
    /// its module (`ns.withRetry(fn)`), or the locals built from the owner's
    /// class (`svc.retry(fn)`). An inline `new Cls().retry(fn)` also counts.
    Namespaces(&'a [String]),
    /// A method owner (named by the second field): only a call through one
    /// of these receivers is the owner, never its bare name (`retry(fn)`
    /// beside `svc.retry(real)`) unless the test binds that name to the
    /// method (`const retry = svc.retry.bind(svc)`).
    Members(&'a [String], &'a str),
}

/// The class of an inline receiver whose expression ends the text: `new Cls(…)`
/// or `(new Cls(…))`. A `new` nested in the constructor arguments, or a call
/// that merely receives a constructed value (`wrap(new Cls())`), is not it.
fn inline_constructed_class(receiver: &str) -> Option<&str> {
    inline_class_within(receiver, 16)
}

fn inline_class_within(receiver: &str, groups_left: usize) -> Option<&str> {
    let groups_left = groups_left.checked_sub(1)?;
    // `new Cls() as Alias` and `new Cls()!` are the same receiver.
    let mut receiver = receiver.trim_end();
    loop {
        let trimmed = receiver.strip_suffix('!').map(str::trim_end).or_else(|| {
            let at = receiver.rfind(" as ")?;
            let ty = &receiver[at + 4..];
            (!ty.contains(['(', ')'])).then(|| receiver[..at].trim_end())
        });
        match trimmed {
            Some(shorter) => receiver = shorter,
            None => break,
        }
    }
    let args_end = receiver.strip_suffix(')')?;
    let mut depth = 1usize;
    let open = args_end.char_indices().rev().find_map(|(i, ch)| match ch {
        ')' => {
            depth += 1;
            None
        }
        '(' => {
            depth -= 1;
            (depth == 0).then_some(i)
        }
        _ => None,
    })?;
    let mut head = args_end[..open].trim_end();
    // `new Cls<T>(…)`: the type arguments sit between the class and its `(`.
    if head.ends_with('>') {
        let mut depth = 0usize;
        let lt = head.char_indices().rev().find_map(|(i, ch)| match ch {
            '>' => {
                depth += 1;
                None
            }
            '<' => {
                depth = depth.saturating_sub(1);
                (depth == 0).then_some(i)
            }
            _ => None,
        });
        if let Some(lt) = lt {
            head = head[..lt].trim_end();
        }
    }
    let start = head
        .char_indices()
        .rev()
        .find(|(_, c)| !is_word(*c))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let class = &head[start..];
    let before = head[..start].trim_end();
    if !class.is_empty() && !matches!(class, "await" | "return") {
        let constructed = before
            .strip_suffix("new")
            .is_some_and(|rest| !rest.chars().next_back().is_some_and(is_word));
        return constructed.then_some(class);
    }
    // A bare group `(new Cls())`, possibly after `await`.
    let lead = ["await", "return"]
        .iter()
        .find_map(|word| {
            head.strip_suffix(word)
                .filter(|rest| !rest.chars().next_back().is_some_and(is_word))
        })
        .map_or(head, str::trim_end);
    let grouping =
        lead.is_empty() || lead.ends_with(['(', ',', '=', ';', '{', '[', ':', '?', '&', '|']);
    grouping
        .then(|| inline_class_within(&args_end[open + 1..], groups_left))
        .flatten()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// Whether `text` mentions `name` as a free identifier: a whole word that no
/// member dot precedes (`other.fn` is another value). With `skip_keys`, a
/// word followed by `:` is an object key, not a value.
fn mentions_value(text: &str, name: &str, _skip_keys: bool) -> bool {
    !mention_positions(text, 0, text.len(), name).is_empty()
}

/// Where `full[lo..hi]` mentions `name` as a free identifier, judged in the
/// context of `full`: a callback that binds `name` and spans several
/// statements still scopes it, and `{ name: x }` / `, name: x` is a key.
fn mention_positions(full: &str, lo: usize, hi: usize, name: &str) -> Vec<usize> {
    full[lo..hi]
        .match_indices(name)
        .map(|(rel, _)| lo + rel)
        .filter(|&at| {
            let (before, after) = (&full[..at], &full[at + name.len()..]);
            let member = before.trim_end().ends_with('.') && !before.ends_with("...");
            let before_key = before.trim_end();
            let key = after.trim_start().starts_with(':')
                && (before_key.ends_with('{') || before_key.ends_with(','));
            !before.chars().next_back().is_some_and(is_word)
                && !after.chars().next().is_some_and(is_word)
                && !member
                && !key
                && !parameter_scoped(full, at, name)
        })
        .collect()
}

/// The identifiers a parameter list binds, with default values and
/// annotations left out (`fn = async () => "ok"`, `{ a, b }`, `...rest`).
fn parameter_names(params: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (at, ch) in params.char_indices().chain([(params.len(), ',')]) {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let piece = params[start..at]
                    .trim()
                    .trim_start_matches("...")
                    .trim_start();
                if piece.starts_with(['{', '[']) {
                    let end = piece.find('=').unwrap_or(piece.len());
                    names.extend(
                        piece[..end]
                            .split(|c: char| !is_word(c))
                            .filter(|w| !w.is_empty()),
                    );
                } else if let Some(word) = piece.split(|c: char| !is_word(c)).next() {
                    names.push(word);
                }
                start = at + 1;
            }
            _ => {}
        }
    }
    names
}

/// The end of the bracket group opened at `open` (`(`, `[` or `{`).
fn group_end(text: &str, open: usize) -> usize {
    let mut depth = 0usize;
    for (at, ch) in text[open..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return open + at + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// Where the arrow body starting at `start` ends: its block, or the
/// expression up to the next top-level `,`, `;` or unmatched closer.
fn arrow_body_end(text: &str, start: usize) -> usize {
    let lead = text[start..].len() - text[start..].trim_start().len();
    let body = start + lead;
    if text[body..].starts_with('{') {
        return group_end(text, body);
    }
    let mut depth = 0usize;
    for (at, ch) in text[body..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return body + at,
            ')' | ']' | '}' => depth -= 1,
            ',' | ';' if depth == 0 => return body + at,
            _ => {}
        }
    }
    text.len()
}

/// Whether the mention of `name` at `at` sits in the parameters or body of
/// an arrow function or function expression that binds `name` itself: a
/// callback's own `fn` is not the outer mock.
fn function_scopes(text: &str) -> Vec<(usize, usize, &str)> {
    let arrow_scopes = text.match_indices("=>").filter_map(|(arrow, _)| {
        let before = text[..arrow].trim_end();
        let (start, params) = if before.ends_with(')') {
            let mut depth = 0usize;
            let open = before.char_indices().rev().find_map(|(i, ch)| match ch {
                ')' => {
                    depth += 1;
                    None
                }
                '(' => {
                    depth -= 1;
                    (depth == 0).then_some(i)
                }
                _ => None,
            })?;
            (open, &before[open + 1..before.len() - 1])
        } else {
            let word = before
                .rsplit(|c: char| !is_word(c))
                .next()
                .filter(|w| !w.is_empty())?;
            (before.len() - word.len(), word)
        };
        Some((start, arrow_body_end(text, arrow + 2), params))
    });
    let function_scopes = text.match_indices("function").filter_map(|(keyword, _)| {
        let rest = &text[keyword + "function".len()..];
        let open = keyword + "function".len() + rest.find('(')?;
        let close = group_end(text, open);
        let brace = close + text[close..].find('{')?;
        Some((open, group_end(text, brace), &text[open + 1..close - 1]))
    });
    arrow_scopes.chain(function_scopes).collect()
}

/// Whether the mention of `name` at `at` sits in the parameters or body of
/// an arrow function or function expression that binds `name` itself: a
/// callback's own `fn` is not the outer mock.
fn parameter_scoped(text: &str, at: usize, name: &str) -> bool {
    function_scopes(text)
        .into_iter()
        .any(|(start, end, params)| {
            start <= at
                && at < end
                && (parameter_names(params).contains(&name)
                    || (declares_local(&text[start..end], name)
                        && !is_test_callback(text, start)
                        // A mock declared inside this callback is the counted
                        // mock, not a shadow of one declared outside it.
                        && first_declaration(text, name).is_none_or(|at| at < start || end <= at)))
        })
}

/// Where `name` is first declared with `const`/`let`/`var` in `text`.
fn first_declaration(text: &str, name: &str) -> Option<usize> {
    ["const", "let", "var"]
        .iter()
        .flat_map(|keyword| text.match_indices(keyword))
        .filter(|&(at, keyword)| {
            !text[..at].chars().next_back().is_some_and(is_word)
                && text[at + keyword.len()..].starts_with(char::is_whitespace)
                && binding_names(&text[at + keyword.len()..]).contains(&name)
        })
        .map(|(at, _)| at)
        .min()
}

/// Every name a `const`/`let`/`var` statement binds: `a = 1, fn = f` has two
/// declarators, split at top-level commas before the statement ends.
fn declarator_names(after_keyword: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let (mut depth, mut from) = (0usize, 0usize);
    let mut end = after_keyword.len();
    for (at, ch) in after_keyword.char_indices() {
        match ch {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                names.extend(binding_names(&after_keyword[from..at]));
                from = at + 1;
            }
            ';' | '\n' if depth == 0 => {
                end = at;
                break;
            }
            _ => {}
        }
    }
    names.extend(binding_names(&after_keyword[from..end]));
    names
}

/// The names a `const`/`let`/`var` pattern binds: `a`, `{ a, b: c = d }`
/// (`c`, `a`; defaults and keys are not bindings), `[a, b]`.
fn binding_names(after_keyword: &str) -> Vec<&str> {
    let pattern = after_keyword.trim_start();
    if !pattern.starts_with(['{', '[']) {
        let end = pattern
            .find([':', '=', ';', ',', '\n'])
            .unwrap_or(pattern.len());
        return pattern[..end]
            .split(|c: char| !is_word(c))
            .filter(|w| !w.is_empty())
            .collect();
    }
    let group = &pattern[..group_end(pattern, 0)];
    let inner = group.strip_prefix(['{', '[']).unwrap_or(group);
    let inner = inner.strip_suffix(['}', ']']).unwrap_or(inner);
    let (mut depth, mut from) = (0usize, 0usize);
    let mut names = Vec::new();
    for (at, ch) in inner.char_indices() {
        match ch {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                push_binding(&mut names, &inner[from..at]);
                from = at + 1;
            }
            _ => {}
        }
    }
    push_binding(&mut names, &inner[from..]);
    names
}

/// One element of a destructuring pattern: the target side of `key: target`,
/// without its `= default`.
fn push_binding<'a>(names: &mut Vec<&'a str>, piece: &'a str) {
    let piece = piece.split('=').next().unwrap_or("");
    let target = piece.rsplit(':').next().unwrap_or("");
    names.extend(
        target
            .split(|c: char| !is_word(c))
            .filter(|w| !w.is_empty()),
    );
}

/// Whether the function `body` (parameters and block) declares `name` itself
/// at its own top level (`const name = …`, `let { name } = …`): inside the
/// closure that is a new binding, not the outer value. A declaration in a
/// nested block or function, a loop head (`for (const name of …)`) and a type
/// annotation are not.
fn declares_local(function: &str, name: &str) -> bool {
    // Only the function's own block holds its top-level declarations: an
    // expression body has none, and a block inside it belongs to a nested
    // function.
    let params_end = if function.starts_with('(') {
        group_end(function, 0)
    } else {
        function
            .find(|c: char| !is_word(c))
            .unwrap_or(function.len())
    };
    let rest = &function[params_end..];
    let body = match (rest.find("=>"), rest.find('{')) {
        (Some(arrow), Some(brace)) if brace > arrow => rest[arrow + 2..].trim_start(),
        (Some(arrow), None) => rest[arrow + 2..].trim_start(),
        (_, Some(brace)) => &rest[brace..],
        (None, None) => "",
    };
    if !body.starts_with('{') {
        return false;
    }
    let (mut braces, mut parens) = (0usize, 0usize);
    for (at, ch) in body.char_indices() {
        match ch {
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            '(' | '[' => parens += 1,
            ')' | ']' => parens = parens.saturating_sub(1),
            _ => {}
        }
        if braces != 1 || parens != 0 || !is_word(ch) {
            continue;
        }
        let rest = &body[at..];
        let Some(keyword) = ["const", "let", "var"].into_iter().find(|keyword| {
            rest.starts_with(keyword) && rest[keyword.len()..].starts_with(char::is_whitespace)
        }) else {
            continue;
        };
        let before = &body[..at];
        let starts_statement = before.chars().next_back().is_some_and(|c| !is_word(c))
            && (before.trim_end().ends_with(['{', '}', ';'])
                || before[before.trim_end().len()..].contains('\n'));
        if !starts_statement {
            continue;
        }
        if declarator_names(&rest[keyword.len()..]).contains(&name) {
            return true;
        }
    }
    false
}

/// Whether `at` sits in a function body other than a test registration
/// callback: a closure that can be handed on, so a call inside it runs only
/// when the closure does.
fn inside_nested_function(text: &str, at: usize) -> bool {
    function_scopes(text)
        .into_iter()
        .any(|(start, end, _)| start <= at && at < end && !is_test_callback(text, start))
}

/// Whether the function starting at `start` is the argument of a test
/// registration call: its nearest enclosing bracket is that call's `(`.
fn is_test_callback(text: &str, start: usize) -> bool {
    let mut depth = 0usize;
    for (i, ch) in text[..start].char_indices().rev() {
        match ch {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth > 0 => depth -= 1,
            '(' => {
                let callee = callee_chain(text[..i].trim_end());
                return TEST_WRAPPERS.contains(&callee.split('.').next().unwrap_or(""));
            }
            '[' | '{' => return false,
            _ => {}
        }
    }
    false
}

/// A declaration, assignment or function whose name can carry what its body
/// or right-hand side mentions.
struct Binding<'a> {
    names: Vec<&'a str>,
    rhs: &'a str,
}

/// Whether the newline at `at` continues the statement: the line so far ends
/// in an operator that wants an operand, or the next line starts with one.
fn newline_continues(text: &str, start: usize, at: usize) -> bool {
    let before = text[start..at].trim_end();
    let after = text[at..].trim_start();
    before.ends_with([
        '.', '=', '?', ':', ',', '+', '-', '*', '/', '&', '|', '<', '(', '[',
    ]) || before.ends_with("=>")
        || after.starts_with(['.', '?', ':', '+', '*', '/', '|', ')', ']'])
        || after.starts_with("&&")
        || after.starts_with("=>")
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
            ';' if depth == 0 => return at,
            '\n' if depth == 0 && !newline_continues(rest, 0, at) => return at,
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

/// The first `,` outside brackets, which separates declarators.
fn first_top_level_comma(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(at),
            _ => {}
        }
    }
    None
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
            // `const a = 1, b = f(a)` declares two bindings.
            let mut declaration = &after[..declaration_end(after)];
            while let Some(eq) = first_assignment(declaration) {
                let (lhs, tail) = (declaration[..eq].trim(), &declaration[eq + 1..]);
                let next = first_top_level_comma(tail);
                let rhs = &tail[..next.unwrap_or(tail.len())];
                let mut words = lhs.split(|c: char| !is_word(c)).filter(|w| !w.is_empty());
                let names: Vec<&str> = if lhs.starts_with(['{', '[']) {
                    words.collect()
                } else {
                    words.next().into_iter().collect()
                };
                found.push(Binding { names, rhs });
                match next {
                    Some(comma) => declaration = &tail[comma + 1..],
                    None => break,
                }
            }
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
        // The body opens after the parameter list closes, not at a `{` in a
        // parameter type or default.
        let params_end = body.find('(').map_or(0, |open| group_end(body, open));
        let end = body[params_end..].find('{').map_or(body.len(), |open| {
            let open = params_end + open;
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
        });
    }
    for (at, _) in masked.match_indices("class") {
        let after = &masked[at + "class".len()..];
        if masked[..at].chars().next_back().is_some_and(is_word)
            || !after.chars().next().is_some_and(char::is_whitespace)
        {
            continue;
        }
        let trimmed = after.trim_start();
        let name_len = trimmed.find(|c: char| !is_word(c)).unwrap_or(trimmed.len());
        if name_len == 0 {
            continue;
        }
        let body = &trimmed[name_len..];
        if let Some(open) = body.find('{') {
            found.push(Binding {
                names: vec![&trimmed[..name_len]],
                rhs: &body[..group_end(body, open)],
            });
        }
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
            '\n' if depth == 0 && !newline_continues(masked, start, at) => {
                found.push(&masked[start..at]);
                start = at + 1;
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
            if !counter_roots(rhs).is_empty() {
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
        for binding in bindings {
            if !owners.iter().any(|o| {
                let skip_keys = binding.rhs.trim_start().starts_with('{');
                mentions_owner(binding.rhs, o, receivers, skip_keys)
            }) {
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
fn mentions_owner(statement: &str, name: &str, receivers: Receivers<'_>, skip_keys: bool) -> bool {
    statement.match_indices(name).any(|(at, _)| {
        // Under an object literal `owner:` is a key, not a use of the owner.
        if skip_keys && statement[at + name.len()..].trim_start().starts_with(':') {
            return false;
        }
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
            None => {
                !matches!(receivers, Receivers::Members(_, method) if method == name)
                    && !declares_method(statement, at, name.len())
            }
            Some(receiver) => {
                let receiver = receiver.strip_suffix('?').unwrap_or(receiver).trim_end();
                let unwrapped = without_receiver_casts(receiver);
                let receiver = unwrapped.as_str();
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
                    Receivers::Namespaces(names) | Receivers::Members(names, _) => {
                        names.contains(&chain)
                            || (chain.is_empty()
                                && inline_constructed_class(receiver)
                                    .is_some_and(|class| names.contains(&format!("new {class}"))))
                    }
                }
            }
        }
    })
}

/// `name(...) {` or `name(...): T {` at a class-body position declares a
/// method; it is not a call of the owner.
fn declares_method(statement: &str, at: usize, len: usize) -> bool {
    let before = statement[..at].trim_end();
    let before = [
        "async",
        "static",
        "public",
        "private",
        "protected",
        "override",
    ]
    .iter()
    .fold(before, |text, word| {
        text.strip_suffix(word).map_or(text, str::trim_end)
    });
    if !(before.is_empty() || before.ends_with(['{', '}', ';'])) {
        return false;
    }
    let after = statement[at + len..].trim_start();
    let Some(open) = after
        .find('(')
        .filter(|open| after[..*open].chars().all(|c| c != ';'))
    else {
        return false;
    };
    let close = group_end(after, open);
    after[close..].trim_start().starts_with(['{', ':'])
}

/// `(svc as any)`, `svc!`, `(svc as Svc)!` name the receiver `svc`.
fn without_receiver_casts(receiver: &str) -> String {
    let mut text = receiver.trim_end().to_string();
    for _ in 0..8 {
        let trimmed = text.trim_end();
        if let Some(rest) = trimmed.strip_suffix('!') {
            text = rest.to_string();
            continue;
        }
        let Some(end) = trimmed.strip_suffix(')') else {
            break;
        };
        let mut depth = 1usize;
        let open = end.char_indices().rev().find_map(|(i, ch)| match ch {
            ')' => {
                depth += 1;
                None
            }
            '(' => {
                depth -= 1;
                (depth == 0).then_some(i)
            }
            _ => None,
        });
        let Some(open) = open else { break };
        let inner = &end[open + 1..];
        // Only a bare grouping paren is a cast group; `Svc(opts as any)` is an
        // argument list and `.retry` still belongs to the constructed `Svc`.
        let before = end[..open].trim_end();
        let word = before
            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .next()
            .unwrap_or("");
        let call_like = before.ends_with(['.', '>', ')', ']'])
            || (!word.is_empty()
                && !matches!(word, "await" | "new" | "return" | "typeof" | "void"));
        if call_like {
            break;
        }
        match inner.find(" as ") {
            Some(at) if !inner[at..].contains(['(', ')']) => {
                text = format!("{}{}", &end[..open], &inner[..at]);
            }
            _ => break,
        }
    }
    text
}

/// Receivers whose member calls only observe their arguments.
const INERT_RECEIVERS: [&str; 5] = ["console", "assert", "expect", "JSON", "Math"];

/// Bare callees that only observe or schedule their arguments.
const INERT_CALLEES: [&str; 6] = [
    "assert",
    "String",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "queueMicrotask",
];

/// Test registration callees: the test body is their argument, not a use.
const TEST_WRAPPERS: [&str; 10] = [
    "it",
    "test",
    "describe",
    "suite",
    "before",
    "after",
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
];

/// Suffixes that read a mock's state without passing the mock on.
const STATE_READS: [&str; 9] = [
    ".mock",
    ".callCount",
    ".calledOnce",
    ".calledTwice",
    ".calledThrice",
    ".called",
    ".getCalls(",
    ".calls",
    ".results",
];

/// Whether `rhs` is a function or arrow literal (or a declared function's
/// parameters and body), not a call whose result is a function.
fn defines_a_function(rhs: &str) -> bool {
    let rhs = rhs.trim_start();
    if rhs.starts_with('(') || rhs.starts_with("function") || rhs.starts_with("async") {
        return true;
    }
    let word = rhs.find(|c: char| !is_word(c)).unwrap_or(rhs.len());
    word > 0 && rhs[word..].trim_start().starts_with("=>")
}

/// Whether `statement` declares or assigns a plain local (`const t = fn`,
/// `t = fn`, `const { a } = fn`): the flow is followed through
/// [`mock_carriers`], so the statement itself sends the mock nowhere new.
fn is_simple_binding(statement: &str) -> bool {
    let trimmed = statement.trim_start();
    let (declared, rest) = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))
        .map_or((false, trimmed), |rest| (true, rest.trim_start()));
    let name_len = rest.find(|c: char| !is_word(c)).unwrap_or(rest.len());
    let after = rest[name_len..].trim_start();
    let assigns = first_assignment(after).is_some();
    (name_len > 0 && assigns && (after.starts_with('=') || after.starts_with(':') && declared))
        || (declared && rest.starts_with(['{', '[']) && first_assignment(rest).is_some())
}

/// Where the member chain that ends `callee` starts, whitespace around its
/// dots included (`other. withRetry` starts at `other`).
fn callee_start(callee: &str) -> usize {
    let mut rest = callee.trim_end();
    loop {
        let word_start = rest
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_word(*c) || matches!(c, '?' | '!'))
            .last()
            .map_or(rest.len(), |(at, _)| at);
        match rest[..word_start].trim_end().strip_suffix('.') {
            Some(receiver) => rest = receiver.trim_end(),
            None => return word_start,
        }
    }
}

/// The member chain that ends `callee`, whitespace removed
/// (`other. withRetry` is `other.withRetry`).
fn callee_chain(callee: &str) -> String {
    let callee = callee.trim_end();
    callee[callee_start(callee)..]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// The call whose argument list holds position `at`: its callee text, or
/// `None` when no call encloses it.
fn enclosing_callee(text: &str, at: usize) -> Option<String> {
    enclosing_call(text, at, true)
}

/// [`enclosing_callee`], optionally stopping at a test registration callee.
fn enclosing_call(text: &str, at: usize, skip_wrappers: bool) -> Option<String> {
    let mut depth = 0usize;
    for (i, ch) in text[..at].char_indices().rev() {
        match ch {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' => {
                if depth > 0 {
                    depth -= 1;
                } else if ch == '(' {
                    let callee = callee_chain(text[..i].trim_end());
                    let root = callee.split('.').next().unwrap_or("");
                    if !skip_wrappers || !TEST_WRAPPERS.contains(&root) {
                        return Some(callee);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the use of `name` at `at` demonstrably sends the mock nowhere: a
/// direct invocation, a state read or configuration of the mock itself
/// (`.mock…`, `.callCount`), an argument of a call on an observing receiver
/// (`console.log(m)`, `assert.ok(m)`), an argument of a member that shares the
/// owner's name on another receiver (`other.owner(m)` is another binding), or
/// an argument of a body-local function that does not reach the owner (a
/// shadowed canonical name). Every other callee may forward the mock.
fn use_is_inert(
    masked: &str,
    at: usize,
    name: &str,
    call_names: &[&str],
    owners: &[String],
    carriers: &[String],
    locals: &[&str],
) -> bool {
    let after = &masked[at + name.len()..];
    if after.starts_with(".mock") {
        return true;
    }
    if STATE_READS.iter().any(|read| {
        after.starts_with(read) && !after[read.len()..].chars().next().is_some_and(is_word)
    }) {
        return true;
    }
    if after.trim_start().starts_with('(') && !inside_nested_function(masked, at) {
        return true;
    }
    if masked[..at].trim_end().ends_with("expect(") {
        return true;
    }
    let Some(callee) = enclosing_callee(masked, at) else {
        return false;
    };
    if callee.is_empty() {
        return false;
    }
    let segments: Vec<&str> = callee.split('.').collect();
    let root = segments[0].trim_end_matches(['?', '!']);
    let last = segments[segments.len() - 1].trim_end_matches(['?', '!']);
    if owners.iter().any(|o| o == root) || carriers.iter().any(|c| c == root) {
        return false;
    }
    if segments.len() > 1 {
        INERT_RECEIVERS.contains(&root)
            || (root == "t" && matches!(segments[1], "assert" | "diagnostic"))
            || call_names.contains(&last)
    } else {
        INERT_CALLEES.contains(&root) || locals.contains(&root)
    }
}

/// The span of the outermost call whose argument list holds `at`, test
/// registration callees excluded: a callback body split into several
/// statements still belongs to the call that received the mock.
fn outermost_call_span(text: &str, at: usize) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut span = None;
    for (i, ch) in text[..at].char_indices().rev() {
        match ch {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' => {
                if depth > 0 {
                    depth -= 1;
                } else if ch == '(' {
                    let callee = callee_chain(text[..i].trim_end());
                    let root = callee.split('.').next().unwrap_or("");
                    if !TEST_WRAPPERS.contains(&root) {
                        let start = callee_start(text[..i].trim_end());
                        let mut end = group_end(text, i);
                        // `.then(async (f) => {…})` chained on the call.
                        while text[end..].trim_start().starts_with('.') {
                            let rest = &text[end..];
                            let Some(open) = rest.find('(') else { break };
                            end += group_end(rest, open);
                        }
                        span = Some((start, end));
                    }
                }
            }
            _ => {}
        }
    }
    span
}

/// Some statement in `masked` (comments and literals blanked) sends the
/// counted mock, or a local carrying it, somewhere the rule cannot rule out
/// as the owner. This is the explicit conservative policy for unresolved
/// flow: a statement that runs the owner (in any spelling, through an alias
/// or a local wrapper) and mentions the mock abstains, and so does a use
/// that is not a plain local binding, a state read, a direct invocation or an
/// argument of a known-unrelated call. A member value (`other.fn`), an
/// object key, a parameter that rebinds the name and text in comments or
/// strings are not uses of the mock.
fn hands_mock_to_owner(
    masked: &str,
    call_names: &[&str],
    receivers: Receivers<'_>,
    mock: &str,
) -> bool {
    let bindings = bindings(masked);
    let carriers = mock_carriers(&bindings, mock);
    let receivers = match receivers {
        Receivers::Members(names, method)
            if bindings.iter().any(|binding| {
                binding.names.contains(&method)
                    && mentions_owner(binding.rhs, method, receivers, false)
            }) =>
        {
            Receivers::Namespaces(names)
        }
        other => other,
    };
    let owners = owner_names(&bindings, call_names, receivers);
    // Only a function or arrow defined in the body is known not to forward the
    // mock; a name bound from a call, a destructuring or an import is unknown.
    let locals: Vec<&str> = bindings
        .iter()
        .filter(|binding| defines_a_function(binding.rhs))
        .flat_map(|binding| binding.names.iter().copied())
        .collect();
    // A bare call reaches a method owner only through a name bound to it.
    let bare_owners: Vec<String> = owners
        .iter()
        .filter(
            |owner| !matches!(receivers, Receivers::Members(_, method) if method == owner.as_str()),
        )
        .cloned()
        .collect();
    let runs_owner = |text: &str| {
        owners
            .iter()
            .any(|owner| mentions_owner(text, owner, receivers, false))
    };
    statements(masked).iter().any(|statement| {
        let lo = statement.as_ptr() as usize - masked.as_ptr() as usize;
        let hi = lo + statement.len();
        let uses: Vec<(usize, &str)> = carriers
            .iter()
            .flat_map(|c| {
                mention_positions(masked, lo, hi, c)
                    .into_iter()
                    .map(move |at| (at, c.as_str()))
            })
            .collect();
        if uses.is_empty() {
            return false;
        }
        if runs_owner(statement) {
            return true;
        }
        let binding = is_simple_binding(statement);
        uses.iter().any(|&(at, name)| {
            // A call that received the mock and spans several statements
            // (a callback body) may run the owner in a later one.
            if let Some((start, end)) = outermost_call_span(masked, at)
                && runs_owner(&masked[start..end])
            {
                return true;
            }
            // A plain binding follows the flow through its carrier; a call
            // inside it still has to be shown unrelated.
            if binding && enclosing_callee(masked, at).is_none() {
                return false;
            }
            !use_is_inert(
                masked,
                at,
                name,
                call_names,
                &bare_owners,
                &carriers,
                &locals,
            )
        })
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
        // A method is a property: a local of the same name cannot shadow it.
        let canonical_is_the_binding = owner_is_method
            || (candidate.relation != TypeScriptRelationKind::ImportAliasOwnerCall
                && !super::related_tests::local_identifier_declared_in_test_body(
                    &candidate.test.body_text,
                    &owner.name,
                )
                && !super::related_tests::enclosing_scope_shadows(candidate.test, &owner.name));
        let names: Vec<&str> =
            super::classifier::ts_owner_call_names(candidate, owner, alias_map, workspace_root)
                .into_iter()
                .filter(|name| *name != owner.name || canonical_is_the_binding)
                .collect();
        let namespaces = if owner_is_method {
            super::related_tests::owner_method_receiver_names(
                candidate.test,
                owner,
                alias_map,
                workspace_root,
            )
        } else {
            super::classifier::unshadowed_owner_namespace_receivers(
                candidate.test,
                owner,
                alias_map,
                workspace_root,
            )
        };
        let receivers = if owner_is_method {
            Receivers::Members(&namespaces, &owner.name)
        } else if namespaces.is_empty() {
            Receivers::Bare
        } else {
            Receivers::Namespaces(&namespaces)
        };
        let masked = ts_mask_literals_and_comments(&candidate.test.body_text);
        let bindings = bindings(&masked);
        asserted_counted_mocks(&masked, &bindings, &candidate.test.assertions)
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
    fn counter_roots_are_the_receiver_roots_of_count_reads() {
        let roots = |text: &str| -> Vec<String> {
            let masked = ts_mask_literals_and_comments(text);
            counter_roots(&masked)
                .into_iter()
                .map(String::from)
                .collect()
        };
        assert_eq!(roots("fn.mock.callCount()"), ["fn"]);
        assert_eq!(roots("send.mock.calls.length"), ["send"]);
        assert_eq!(roots("(fn as any).mock.callCount()"), ["fn"]);
        assert_eq!(roots("fns[0].mock.callCount()"), ["fns"]);
        assert_eq!(roots("other.fn.mock.callCount()"), ["other"]);
        assert_eq!(roots("m.callCount()"), ["m"]);
        assert_eq!(roots("fn.callCount"), ["fn"]);
        assert_eq!(roots("fn.calledTwice"), ["fn"]);
        assert_eq!(roots("fn.getCalls().length"), ["fn"]);
        assert!(roots("syscalls.length").is_empty());
        assert!(roots("getCallCount()").is_empty());
        assert!(roots("// fn.mock.callCount()").is_empty());
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
    fn an_inline_receiver_names_the_class_it_ends_with() {
        for (receiver, class) in [
            ("new Svc()", Some("Svc")),
            ("await assert.rejects(new Svc(1, new Date())", Some("Svc")),
            ("new Svc(new Map())", Some("Svc")),
            ("(new Svc(new Map()))", Some("Svc")),
            ("await (new Svc())", Some("Svc")),
            ("new Svc<Map<string, number>>(1)", Some("Svc")),
            ("(new Svc() as any)", Some("Svc")),
            ("(new Svc())!", Some("Svc")),
            ("wrap(new Svc())", None),
            ("new Other(new Svc()).inner()", None),
            ("make<T>(new Svc())", None),
            ("svc", None),
            ("new", None),
            ("é(", None),
        ] {
            assert_eq!(inline_constructed_class(receiver), class, "{receiver}");
        }
    }

    #[test]
    fn the_owner_call_must_mention_the_mock_as_a_free_identifier() {
        let names = ["withRetry"];
        assert!(passes("await withRetry(fn, 2);", &names, "fn"));
        assert!(passes("await withRetry?.(fn, 2);", &names, "fn"));
        assert!(!passes(
            "withRetry(other, 2); console.log(fn);",
            &names,
            "fn"
        ));
        // An unknown callee may forward the mock anywhere: unresolved flow abstains.
        assert!(passes("withRetry(other, 2); log(fn);", &names, "fn"));
        assert!(!passes("withRetry(fnx, 2);", &names, "fn"));
        assert!(!passes(
            "const rewithRetry = (f, n) => n; rewithRetry(fn, 2);",
            &names,
            "fn"
        ));
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
        // Under the canonical name alone `retry` is an unknown callee, so the
        // unresolved flow abstains.
        assert!(passes("await retry(fn, 2);", &names, "fn"));
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
            "const task =\n  fn;\nwithRetry(task, 2);",
            "const task = async () =>\n  fn();\nwithRetry(task, 2);",
            "const task = true\n  ? fn\n  : null;\nwithRetry(task as any, 2);",
            "let t: any; t ??= fn; withRetry(t, 2);",
            "const o: any = {}; o.task = fn; withRetry(o.task, 2);",
            "const arr: any[] = []; arr.push(fn); withRetry(arr[0], 2);",
            "function go(f: { x?: number } | any) {\n  return withRetry(f, 2);\n}\ngo(fn);",
            "const go = once(withRetry); go(fn, 2);",
            "let go: any; go = (f) => withRetry(f, 2); go(fn);",
            "const h = { go(f) { return withRetry(f, 2); } }; h.go(fn);",
            "class H { go(f) { return withRetry(f, 2); } }\nnew H().go(fn);",
            "withRetry((f = fn) => f(), 2);",
            "let task = real; task = fn; withRetry(task, 2);",
            "const task: () => Promise<string> = fn; withRetry(task, 2);",
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
            "withRetry(async (fn = async () => 1) => fn(), 3);",
            "withRetry(async function (fn) { return fn(); }, 3);",
            "const cb = (fn) => fn(); withRetry(cb, 3);",
            "const task = other; withRetry(task, 2);",
            "const task = other.fn; withRetry(task, 2);",
            "withRetry(() => other.fn(), 2);",
            "const other = { fn: g }; withRetry(other.fn, 2);",
            "const n = fn.mock.callCount(); withRetry(real, n);",
            "const go = (f) => withRetry(f, 2); go(real); console.log(fn);",
            "withRetry(real, 3, { fn: 1 });",
            "const cfg = make({ fn: 1 }); console.log(cfg);",
            "const local = (f, n) => n; local(fn, 2);",
        ] {
            assert!(!passes(call, &names, "fn"), "{call}");
        }
    }

    fn wrapped(body: &str, mock: &str) -> bool {
        let text = format!("it(\"x\", async () => {{\n{body}\n}});");
        hands_mock_to_owner(
            &ts_mask_literals_and_comments(&text),
            &["withRetry"],
            Receivers::Bare,
            mock,
        )
    }

    #[test]
    fn unresolved_flow_abstains_and_unrelated_use_does_not() {
        for body in [
            "const h: any[] = [];\nconst n = h.push(fn);\nawait withRetry(h[0], 2);",
            "const outer = vi.fn();\nouter.mockImplementation(fn);\nawait withRetry(outer, 2);",
            "const reg: any = { list: [] as any[], register(f: any) { this.list.push(f); } };\nreg.register(fn);\nawait withRetry(reg.list[0], 2);",
            "await Promise.all([fn].map(async (f) => {\n  await withRetry(f, 2);\n}));",
            "await Promise.resolve(fn).then(async (f) => {\n  await withRetry(f, 2);\n});",
            "h.use(fn);",
            "emitter.on('x', fn);",
            "mock.method(deps, 'load', () => fn());\nawait withRetry(() => deps.load(), 2);",
            "stash.push(() => fn());\nawait withRetry(stash[0], 2);",
            "cbs.a = () => fn();\nawait withRetry(cbs.a, 2);",
            "Object.assign(deps, { load: () => fn() });",
            "reg.fns.push(async () => { await fn(); });",
            "const run = makeRunner();\nawait run(fn);",
            "const { run } = makeRunner();\nawait run(fn);",
            "const { withRetry: r } = await import('../src/retry');\nawait r(fn, 2);",
            "t.mock.method(obj, 'x', fn);",
        ] {
            assert!(wrapped(body, "fn"), "{body}");
        }
        for body in [
            "const lg = vi.fn();\nlg('x');",
            "const lg = vi.fn();\nawait lg();",
            "const lg = vi.fn();\nlg.mockReturnValue(1);\nlg.mockClear();",
            "const lg = vi.fn();\nsetTimeout(lg, 0);",
            "const lg = vi.fn();\nassert(lg);\nt.assert.ok(lg);",
            "const lg = vi.fn();\nconst helper = (x) => x;\nhelper(lg);",
            "const lg = vi.fn();\nconsole.log(lg);\nassert.ok(lg);",
            "const lg = vi.fn();\nother.withRetry(lg, 2);",
        ] {
            assert!(
                !wrapped(&format!("{body}\nawait withRetry(real, 3);"), "lg"),
                "{body}"
            );
        }
    }

    #[test]
    fn malformed_text_never_panics() {
        for text in [
            "}}} )) ]] withRetry(fn",
            "function",
            "function (",
            "function f(a) {",
            "=> => fn",
            "(fn) =>",
            "const = fn; let x; var",
            "withRetry(fn, \u{e9}\u{4e2d}) => { fn",
            "{ { fn withRetry } ",
            ".mock.calls .callCount( toHaveBeenCalledTimes(",
        ] {
            let masked = ts_mask_literals_and_comments(text);
            let _ = counter_roots(&masked);
            let _ = hands_mock_to_owner(&masked, &["withRetry"], Receivers::Bare, "fn");
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
        let svc = ["svc".to_string()];
        let call = |receivers| {
            hands_mock_to_owner(
                "svc.retry(fn, 2); ns.retry(fn, 2);",
                &["retry"],
                receivers,
                "fn",
            )
        };
        assert!(call(Receivers::Namespaces(&svc)));
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
    fn a_method_owner_is_called_bare_only_through_a_name_bound_to_it() {
        let svc = ["svc".to_string()];
        let members = Receivers::Members(&svc, "retry");
        assert!(hands_mock_to_owner(
            "svc.retry(fn, 2);",
            &["retry"],
            members,
            "fn"
        ));
        // A same-named local function is not the method.
        assert!(!hands_mock_to_owner(
            "const retry = (f) => f();\nretry(fn);",
            &["retry"],
            members,
            "fn"
        ));
        // A name bound to the method is.
        assert!(hands_mock_to_owner(
            "const retry = svc.retry.bind(svc);\nretry(fn, 2);",
            &["retry"],
            members,
            "fn"
        ));
        assert!(hands_mock_to_owner(
            "const go = (f) => svc.retry(f);\ngo(fn);",
            &["retry"],
            members,
            "fn"
        ));
    }

    #[test]
    fn a_mock_declared_inside_a_subtest_callback_is_the_counted_mock() {
        let subtest = "await t.test('sub', async () => {\n  const fn = mock.fn();\n  await assert.rejects(withRetry(fn, 2), /down/);\n});";
        assert!(hands_mock_to_owner(
            subtest,
            &["withRetry"],
            Receivers::Bare,
            "fn"
        ));
        // A callback that redeclares a mock declared outside it shadows it.
        let shadow = "const fn = mock.fn();\nawait withRetry(async () => { const fn = () => 0; return fn(); }, 2);";
        assert!(!hands_mock_to_owner(
            shadow,
            &["withRetry"],
            Receivers::Bare,
            "fn"
        ));
    }

    #[test]
    fn a_declaration_scan_reads_every_declarator_and_only_the_functions_own_block() {
        assert_eq!(declarator_names(" a = 1, fn = f(x, y);\nz"), ["a", "fn"]);
        assert_eq!(declarator_names(" { a, b } = o, c = 2"), ["a", "b", "c"]);
        assert!(declares_local("() => { const a = 1, fn = 2; }", "fn"));
        assert!(!declares_local(
            "x => x ? fn : () => { const fn = 1; }",
            "fn"
        ));
        assert!(!declares_local("() => fn(() => { const fn = 1; })", "fn"));
        assert!(declares_local(
            "function (a) { const fn = 1; return a; }",
            "fn"
        ));
        assert!(declares_local(
            "function (a) { a.map(b => b); const fn = 1; }",
            "fn"
        ));
    }

    #[test]
    fn a_cast_receiver_is_still_the_receiver() {
        let svc = ["svc".to_string()];
        for call in [
            "(svc as any).retry(fn, 2);",
            "svc!.retry(fn, 2);",
            "(svc as unknown as Svc)!.retry(fn, 2);",
        ] {
            assert!(
                hands_mock_to_owner(call, &["retry"], Receivers::Members(&svc, "retry"), "fn"),
                "{call}"
            );
        }
        assert!(!hands_mock_to_owner(
            "(other as any).retry(fn, 2);",
            &["retry"],
            Receivers::Members(&svc, "retry"),
            "fn"
        ));
        // A constructor's or callee's argument list is not a cast group.
        assert_eq!(
            without_receiver_casts("new Svc(opts as any)"),
            "new Svc(opts as any)"
        );
        assert_eq!(
            without_receiver_casts("other (svc as any)"),
            "other (svc as any)"
        );
        assert_eq!(without_receiver_casts("(svc as any)"), "svc");
        assert_eq!(without_receiver_casts("await (svc as any)"), "await svc");
        assert!(!hands_mock_to_owner(
            "other (svc as any).retry(fn, 2);",
            &["retry"],
            Receivers::Members(&svc, "retry"),
            "fn"
        ));
        for text in ["{", "[", "const {", "const [é", "{ é"] {
            let _ = declares_local(&format!("() => {{ {text}"), "fn");
            let _ = binding_names(text);
        }
    }

    #[test]
    fn binding_names_skip_defaults_and_keys() {
        assert_eq!(binding_names(" a = fn;"), ["a"]);
        assert_eq!(binding_names(" { a = fn, b: c, d } = o"), ["a", "c", "d"]);
        assert_eq!(binding_names(" [a = fn, b] = o"), ["a", "b"]);
        assert_eq!(binding_names(" r: ReturnType<typeof fn> = x"), ["r"]);
        let deep = format!("{}new Svc(){}", "(".repeat(5000), ")".repeat(5000));
        assert_eq!(inline_constructed_class(&deep), None);
    }

    #[test]
    fn nested_parentheses_in_the_test_clause_do_not_end_the_header() {
        assert_eq!(
            for_header_clauses("for (let i = 0; i < f(a, (b)); i++) {"),
            Some(["let i = 0", "i < f(a, (b))", "i++"])
        );
    }
}
