//! RIPR-SPEC-0247 rule 1: a literal mock-call payload on a test-local mock
//! that the changed line calls reads `mock_expectation` / strong.
//!
//! `expect(send).toHaveBeenCalledWith("order 7 shipped")` compares every
//! argument of a recorded call with recursive equality, so when `send` is a
//! `jest.fn()` / `vi.fn()` made in the test and the changed line calls it,
//! the payload pins the changed call exactly as `toEqual` pins a value.
//! Anything else (a dynamic payload, a mock from a module factory or
//! `spyOn`, a mock the changed line does not call) keeps the RIPR-SPEC-0243
//! table's `mock_expectation` / medium.

use super::*;

/// The facts rule 1 reads from one changed line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MockCallFacts {
    owner_name: String,
    /// Calls on the changed line made through an owner parameter that the
    /// owner body mentions nowhere else.
    calls: Vec<ParameterCall>,
}

/// A call `p(...)` or `p.m(...)` through the owner parameter `p` at
/// `index`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ParameterCall {
    parameter: String,
    index: usize,
    /// `None` for `p(...)`, `Some(m)` for `p.m(...)`.
    method: Option<String>,
}

impl MockCallFacts {
    /// `owner_body` is the owner's source after the change; `None` (not
    /// readable) keeps no call, so rule 1 credits nothing.
    ///
    /// `toHaveBeenCalledWith` passes when ANY recorded call matches, so the
    /// changed call must be the only way the owner reaches the parameter:
    /// the parameter appears exactly twice in the owner (its declaration and
    /// this call). A second call, `p.call(...)`, an alias, passing `p` on, or
    /// an inner parameter shadowing it all add a mention and refuse. A loop or
    /// recursion could run the changed call more than once (or never), so
    /// an owner body with either credits nothing. A nested function (arrow,
    /// `function`, method shorthand or class member) refuses too, unless
    /// the changed line is a whole top-level statement of the body
    /// (RIPR-SPEC-0256 rule 1), where no nested function can run it.
    pub(crate) fn read(
        line_text: &str,
        owner_body: Option<&str>,
        owner_name: &str,
        owner_params: &[String],
    ) -> Self {
        let Some(owner_body) =
            owner_body.filter(|body| runs_changed_line_once(body, owner_name, line_text))
        else {
            return Self::default();
        };
        let calls = calls_in(line_text)
            .into_iter()
            .filter_map(|call| {
                let method = match call.depth {
                    0 if call.name == call.root => None,
                    1 => Some(call.name),
                    _ => return None,
                };
                let index = owner_params.iter().position(|param| *param == call.root)?;
                // `arguments[i]` reaches a parameter without naming it.
                (identifier_mentions(owner_body, &call.root) == 2
                    && identifier_mentions(owner_body, "arguments") == 0)
                    .then_some(ParameterCall {
                        parameter: call.root,
                        index,
                        method,
                    })
            })
            .collect();
        Self {
            owner_name: owner_name.to_string(),
            calls,
        }
    }

    /// `true` when `payload` is a primitive-literal `toHaveBeenCalledWith`
    /// on a mock made in `test_body` under the parameter's own name, passed
    /// to the owner's one call in the test at the parameter's position, and
    /// mentioned nowhere else but `expect(...)`.
    #[cfg(test)]
    pub(crate) fn credits(&self, payload: &TypeScriptMockPayload, test_body: &str) -> bool {
        self.credits_with_factories(payload, test_body, &[])
    }

    /// [`Self::credits`], also reading `const p = f();` through the test
    /// file's mock factories (`TypeScriptTest::mock_factories_in_file`).
    pub(crate) fn credits_with_factories(
        &self,
        payload: &TypeScriptMockPayload,
        test_body: &str,
        factories: &[(String, String)],
    ) -> bool {
        // Jest's recursive equality treats a property set to `undefined` as
        // absent, so an object payload does not pin every field.
        if payload.kind != TypeScriptMockPayloadKind::CalledWith
            || ts_mask_literals_and_comments(&payload.expected).contains('{')
        {
            return false;
        }
        let segments: Vec<&str> = payload.target.split('.').collect();
        if let [local] = segments.as_slice()
            && self.credits_object_argument(local, test_body)
        {
            return true;
        }
        let Some(call) = self
            .calls
            .iter()
            .find(|call| match (segments.as_slice(), &call.method) {
                ([name], None) => *name == call.parameter,
                ([object, property], Some(method)) => {
                    *object == call.parameter && property == method
                }
                _ => false,
            })
        else {
            return false;
        };
        let local = call.parameter.as_str();
        // The mock is declared at the test callback's top level, so the
        // name passed to the owner and asserted on is that binding and not
        // an outer one a nested block shadows.
        let masked_body = ts_mask_literals_and_comments(test_body);
        let Some(scope) = callback_scope(&masked_body) else {
            return false;
        };
        let made_in_test = match &call.method {
            None => test_local_mock_binding(scope, local),
            Some(method) => test_local_mock_property(scope, local, method, factories),
        };
        made_in_test
            && identifier_mentions_outside_expect(test_body, local) == 2
            && owner_call_passes_at(test_body, &self.owner_name, local, call.index)
    }
}

impl MockCallFacts {
    /// RIPR-SPEC-0256 rule 2: the asserted `local` is a test-local mock the
    /// test passes as the method of an inline object literal at the
    /// parameter's position (`checkout("ann", lines, { send })` for a
    /// changed `notifier.send(...)`), and mentions nowhere else but
    /// `expect(local)`.
    fn credits_object_argument(&self, local: &str, test_body: &str) -> bool {
        let masked_body = ts_mask_literals_and_comments(test_body);
        let Some(scope) = callback_scope(&masked_body) else {
            return false;
        };
        if !test_local_mock_binding(scope, local)
            || identifier_mentions_outside_expect(test_body, local) != 2
        {
            return false;
        }
        self.calls.iter().any(|call| {
            call.method.as_deref().is_some_and(|method| {
                owner_call_argument_at(test_body, &self.owner_name, call.index)
                    .is_some_and(|argument| object_literal_binds(&argument, method, local))
            })
        })
    }
}

/// `true` when `argument` is a flat object literal whose entries are all
/// `key: value` or shorthand `key` with plain keys, `method` is keyed once,
/// and its value is exactly `local`. A spread, a method or accessor, a
/// quoted or computed key, or a second `method` key refuses.
fn object_literal_binds(argument: &str, method: &str, local: &str) -> bool {
    let Some(rest) = argument.trim().strip_prefix('{') else {
        return false;
    };
    let Some(literal) = literal_body(rest) else {
        return false;
    };
    if rest
        .get(literal.len() + 1..)
        .is_none_or(|tail| !tail.trim().is_empty())
    {
        return false;
    }
    let mut bound = Vec::new();
    for entry in top_level_entries(literal) {
        let (key, value) = entry
            .split_once(':')
            .map_or((entry, entry), |(key, value)| (key.trim(), value.trim()));
        if !key.starts_with(is_identifier_start) || !key.chars().all(is_identifier_part) {
            return false;
        }
        bound.push((key, value));
    }
    let mut keyed = bound.iter().filter(|(key, _)| *key == method);
    matches!((keyed.next(), keyed.next()), (Some((_, value)), None) if *value == local)
}

/// One call outside literals and comments: the called name, the first
/// identifier of its member chain (`notifier` for `notifier.send(`), and
/// the number of member steps (`1` there, `0` for `send(`).
struct Call {
    name: String,
    root: String,
    depth: usize,
}

/// Every identifier followed by `(` or `?.(`, outside literals and comments.
fn calls_in(text: &str) -> Vec<Call> {
    let masked = ts_mask_literals_and_comments(text);
    let chars: Vec<char> = masked.chars().collect();
    let mut calls = Vec::new();
    let mut chain_root = String::new();
    let mut depth = 0;
    let mut index = 0;
    while index < chars.len() {
        if !is_identifier_start(chars[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && is_identifier_part(chars[index]) {
            index += 1;
        }
        let name: String = chars[start..index].iter().collect();
        let mut before = start;
        while before > 0 && chars[before - 1].is_whitespace() {
            before -= 1;
        }
        if before > 0 && chars[before - 1] == '.' {
            // Only an identifier before the dot continues the chain; after
            // `)` or `]` (`getTable(db).write(`) the receiver is a computed
            // value, so the call has no parameter root.
            let mut receiver = before - 1;
            if receiver > 0 && chars[receiver - 1] == '?' {
                receiver -= 1;
            }
            while receiver > 0 && chars[receiver - 1].is_whitespace() {
                receiver -= 1;
            }
            if receiver > 0 && is_identifier_part(chars[receiver - 1]) {
                depth += 1;
            } else {
                chain_root.clear();
                depth = 0;
            }
        } else {
            chain_root.clone_from(&name);
            depth = 0;
        }
        let mut next = index;
        while next < chars.len() && chars[next].is_whitespace() {
            next += 1;
        }
        let called = chars.get(next) == Some(&'(')
            || (chars.get(next) == Some(&'?')
                && chars.get(next + 1) == Some(&'.')
                && chars.get(next + 2) == Some(&'('));
        if called && !CALL_LIKE_KEYWORDS.contains(&name.as_str()) {
            calls.push(Call {
                name,
                root: chain_root.clone(),
                depth,
            });
        }
    }
    calls
}

/// Byte offsets of `name` as a whole identifier that is not a member name
/// (`x.name`), outside literals and comments.
pub(super) fn identifier_offsets(masked: &str, name: &str) -> Vec<usize> {
    masked
        .match_indices(name)
        .filter(|(at, _)| {
            word_boundary_before(masked, *at)
                && !masked[at + name.len()..].starts_with(is_identifier_part)
                && !masked[..*at].trim_end().ends_with('.')
        })
        .map(|(at, _)| at)
        .collect()
}

fn identifier_mentions(text: &str, name: &str) -> usize {
    identifier_offsets(&ts_mask_literals_and_comments(text), name).len()
}

/// Mentions of `name` other than as the whole subject of `expect(name)` /
/// `expect(name.member)`. `expect(name(...))` calls the mock, so it counts.
fn identifier_mentions_outside_expect(text: &str, name: &str) -> usize {
    let masked = ts_mask_literals_and_comments(text);
    identifier_offsets(&masked, name)
        .into_iter()
        .filter(|at| {
            let before = masked[..*at].trim_end();
            let opens_expect = before.strip_suffix('(').is_some_and(|head| {
                let head = head.trim_end();
                head.strip_suffix("expect").is_some_and(|prefix| {
                    prefix
                        .chars()
                        .next_back()
                        .is_none_or(|c| !is_identifier_part(c) && c != '.')
                })
            });
            !(opens_expect && closes_expect_subject(&masked[at + name.len()..]))
        })
        .count()
}

/// `true` when `rest` (the text after the subject name) is `)` or one
/// `.member` then `)`.
fn closes_expect_subject(rest: &str) -> bool {
    let rest = rest.trim_start();
    let rest = match rest.strip_prefix('.') {
        Some(member) => member
            .trim_start()
            .trim_start_matches(is_identifier_part)
            .trim_start(),
        None => rest,
    };
    rest.starts_with(')')
}

/// `true` when the test calls the owner exactly once (bare or as a method)
/// and that call passes `local` as a whole argument at `index`, with no
/// spread before it.
fn owner_call_passes_at(test_body: &str, owner_name: &str, local: &str, index: usize) -> bool {
    owner_call_argument_at(test_body, owner_name, index)
        .is_some_and(|argument| argument.trim() == local)
}

/// The masked argument at `index` of the test's one owner call (bare or as
/// a method), or `None` when the test calls the owner other than exactly
/// once, passes fewer arguments, or spreads an argument at or before
/// `index`.
fn owner_call_argument_at(test_body: &str, owner_name: &str, index: usize) -> Option<String> {
    let masked = ts_mask_literals_and_comments(test_body);
    let mut calls = masked.match_indices(owner_name).filter_map(|(at, _)| {
        if !(word_boundary_before(&masked, at) || masked[..at].ends_with('.')) {
            return None;
        }
        let rest = masked[at + owner_name.len()..].trim_start();
        Some((at, top_level_arguments(rest.strip_prefix('(')?)?))
    });
    let (Some((at, arguments)), None) = (calls.next(), calls.next()) else {
        return None;
    };
    // One call site can still run several times: inside a loop, or inside a
    // callback, method, getter or class (`ids.forEach((id) => notify({ send },
    // id))`), and `toHaveBeenCalledWith` passes when any recorded call
    // matches. The test's own block holds no loop, and nothing before the
    // call in it opens a function that could hold the call.
    let range = callback_scope_range(&masked)?;
    if ["for", "while", "do"]
        .iter()
        .any(|word| !identifier_offsets(&masked[range.clone()], word).is_empty())
        || !range.contains(&at)
        || opens_function_before_call(&masked[range.start..at])
        || inside_parameter_list(&masked[range.clone()], at - range.start)
    {
        return None;
    }
    if arguments
        .iter()
        .take(index + 1)
        .any(|argument| argument.trim_start().starts_with("..."))
    {
        return None;
    }
    arguments.get(index).map(|argument| (*argument).to_string())
}

/// The comma-separated top-level arguments of a call whose `(` was just
/// consumed; `None` when the list never closes.
fn top_level_arguments(text: &str) -> Option<Vec<&str>> {
    let mut depth = 0usize;
    let mut start = 0;
    let mut arguments = Vec::new();
    for (index, c) in text.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                arguments.push(&text[start..index]);
                return Some(arguments);
            }
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                arguments.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    None
}

const CALL_LIKE_KEYWORDS: [&str; 10] = [
    "if", "for", "while", "switch", "catch", "return", "typeof", "await", "new", "function",
];

const MOCK_FACTORIES: [&str; 2] = ["jest.fn", "vi.fn"];

fn is_identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

pub(super) fn is_identifier_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// `const send = jest.fn()` (or `let`, `var`, `vi.fn`, a type argument).
fn test_local_mock_binding(body: &str, name: &str) -> bool {
    let masked = ts_mask_literals_and_comments(body);
    ["const", "let", "var"].iter().any(|keyword| {
        masked.match_indices(keyword).any(|(at, _)| {
            let rest = &masked[at + keyword.len()..];
            let Some(rest) = strip_required_space(rest) else {
                return false;
            };
            let Some(rest) = rest.strip_prefix(name) else {
                return false;
            };
            let rest = rest.trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                return false;
            };
            closed_mock_call(rest.trim_start()).is_some_and(ends_statement)
                && word_boundary_before(&masked, at)
                && at_top_level(&masked, at)
        })
    })
}

/// `const db = { write: jest.fn() }`: `object` is bound to an object literal
/// in the test whose `property` is a mock factory call. A spread, a second
/// `property` key or a key that is not a plain identifier could replace the
/// factory, so any of them refuses. `const db = makeDb();` reads the literal
/// a test-file mock factory returns (`factories`, RIPR-SPEC-0247 rule 1,
/// amended 2026-10-15): the call is the whole initializer and every call
/// builds a fresh object.
fn test_local_mock_property(
    body: &str,
    object: &str,
    property: &str,
    factories: &[(String, String)],
) -> bool {
    let masked = ts_mask_literals_and_comments(body);
    ["const", "let", "var"].iter().any(|keyword| {
        masked.match_indices(keyword).any(|(at, _)| {
            if !word_boundary_before(&masked, at) || !at_top_level(&masked, at) {
                return false;
            }
            let rest = &masked[at + keyword.len()..];
            let Some(rest) = strip_required_space(rest) else {
                return false;
            };
            let Some(rest) = rest.strip_prefix(object) else {
                return false;
            };
            let Some(rest) = rest.trim_start().strip_prefix('=') else {
                return false;
            };
            let rest = rest.trim_start();
            if let Some(literal) = rest.strip_prefix('{') {
                // `{ ... }.inner` or `{ ... } && other` binds something else.
                return literal_body(literal).is_some_and(|body| {
                    ends_statement(&literal[body.len() + 1..])
                        && keys_mock_once(body, property, is_closed_mock_call)
                });
            }
            factories.iter().any(|(name, returned)| {
                factory_call_is_initializer(rest, name)
                    && ts_mask_literals_and_comments(returned)
                        .strip_prefix('{')
                        .and_then(literal_body)
                        .is_some_and(|literal| {
                            keys_mock_once(literal, property, is_bare_mock_factory)
                        })
            })
        })
    })
}

/// `true` when `rest` opens with `name()` and an explicit `;` (or the
/// block's end) closes the statement there. A line break alone does not:
/// a next line opening with `.`, `[`, `&&` or `?` continues the
/// expression, so `db` would not be the factory's fresh object.
fn factory_call_is_initializer(rest: &str, name: &str) -> bool {
    rest.strip_prefix(name)
        .and_then(|rest| rest.strip_prefix("()"))
        .is_some_and(|rest| {
            let rest = rest.trim_start();
            rest.is_empty() || rest.starts_with([';', '}'])
        })
}

/// `true` when a factory literal's value is exactly a bare `jest.fn()` /
/// `vi.fn()`: an implementation argument can call the mock through `this`
/// and a trailing operator (`jest.fn() && shared`) can hand back another
/// mock, so the factory path admits neither. A space before `()` is the
/// same call.
fn is_bare_mock_factory(value: &str) -> bool {
    MOCK_FACTORIES.iter().any(|factory| {
        value
            .trim_end()
            .strip_prefix(factory)
            .is_some_and(|rest| rest.trim_start() == "()")
    })
}

/// `true` when an inline literal's value is exactly one mock factory call
/// with no implementation or an arrow one, and nothing after it.
fn is_closed_mock_call(value: &str) -> bool {
    closed_mock_call(value).is_some_and(|after| after.trim().is_empty())
}

/// `true` when every entry of a flat literal body is `key: value` with a
/// plain key and `property` is keyed exactly once, to a mock factory call.
fn keys_mock_once(literal: &str, property: &str, is_mock: fn(&str) -> bool) -> bool {
    let mut entries = Vec::new();
    for entry in top_level_entries(literal) {
        let Some((key, value)) = entry.split_once(':') else {
            return false;
        };
        let key = key.trim();
        if !key.starts_with(is_identifier_start) || !key.chars().all(is_identifier_part) {
            return false;
        }
        entries.push((key, value.trim_start()));
    }
    let mut keyed = entries.iter().filter(|(key, _)| *key == property);
    matches!(
        (keyed.next(), keyed.next()),
        (Some((_, value)), None) if is_mock(value)
    )
}

/// The text after a mock factory call `jest.fn(...)` / `vi.fn<T>(...)` that
/// opens `text`, or `None` when `text` is not one whose implementation (if
/// any) is an arrow function. A named or `function` implementation can be
/// called with the mock object as `this` and record the asserted call on
/// itself (#8138); an arrow's `this` is the test's. The argument must
/// itself be the arrow: a wrapper that merely contains one (a call such as
/// `(() => recordTwo)()`, a conditional such as
/// `ready ? (v) => v : recordTwo`, a grouping) can evaluate to a named
/// implementation, so it refuses too (#8148 review). A space between the
/// factory or its type arguments and `(` is the same call, as in
/// [`is_bare_mock_factory`]. String contents arrive masked (see
/// `ts_mask_literals_and_comments`), so a `)` inside a string in the
/// implementation never ends the call.
fn closed_mock_call(text: &str) -> Option<&str> {
    let rest = MOCK_FACTORIES
        .iter()
        .find_map(|factory| text.strip_prefix(factory))?;
    let rest = rest.trim_start();
    let rest = if rest.starts_with('<') {
        &rest[closing_offset(rest, '<', '>')?..]
    } else {
        rest
    };
    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let end = closing_offset(rest, '(', ')')?;
    let argument = rest[1..end - 1].trim();
    let arrow = is_arrow_expression(argument)
        && identifier_mentions(argument, "function") == 0
        && identifier_mentions(argument, "this") == 0;
    (argument.is_empty() || arrow).then(|| &rest[end..])
}

/// `true` when `text` is itself an arrow function expression: an optional
/// `async`, then a parenthesized parameter list or a single identifier,
/// followed by `=>`. A call (`(() => recordTwo)()`), a conditional
/// (`ready ? (v) => v : recordTwo`) or any other wrapper can evaluate to a
/// named or `function` implementation that records the asserted call
/// (#8148 review), so a merely contained arrow refuses.
fn is_arrow_expression(text: &str) -> bool {
    let mut rest = text.trim_start();
    if rest
        .strip_prefix("async")
        .is_some_and(|after| !after.starts_with(is_identifier_part))
    {
        rest = rest["async".len()..].trim_start();
    }
    if let Some(parameter_end) = rest
        .starts_with('(')
        .then(|| closing_offset(rest, '(', ')'))
        .flatten()
    {
        return rest[parameter_end..].trim_start().starts_with("=>");
    }
    let word_end = rest
        .find(|c: char| !is_identifier_part(c))
        .unwrap_or(rest.len());
    word_end > 0 && rest[word_end..].trim_start().starts_with("=>")
}

/// `true` when only whitespace stands before an explicit `;`, the block's
/// `}` or the end of the text, or before a line break whose next line opens
/// a new statement with a word. A line break does not end a statement whose
/// next line opens with `.`, `[`, `(`, `&&`, `?` or a word operator such as
/// `in` (#8138).
fn ends_statement(after: &str) -> bool {
    let next = after.trim_start();
    if next.is_empty() || next.starts_with([';', '}']) {
        return true;
    }
    let word: String = next
        .chars()
        .take_while(|c| is_identifier_part(*c))
        .collect();
    after[..after.len() - next.len()].contains('\n')
        && next.starts_with(is_identifier_start)
        && !matches!(word.as_str(), "in" | "instanceof" | "as" | "satisfies")
}

/// The non-empty comma-separated entries of a flat literal body, split only
/// outside parentheses and brackets.
fn top_level_entries(literal: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, c) in literal.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(literal[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    entries.push(literal[start..].trim());
    entries.retain(|entry| !entry.is_empty());
    entries
}

/// The test callback's block in a masked `it(...)` call: the first `{`
/// directly inside the call's argument list (paren depth 1, so a
/// destructured parameter or a nested call's block is skipped) up to its
/// matching `}`. A body with no such brace is read whole. `None` when the
/// block never closes, so an unreadable shape earns nothing.
fn callback_scope(masked: &str) -> Option<&str> {
    callback_scope_range(masked).map(|range| &masked[range])
}

/// The byte range of [`callback_scope`] within `masked`.
fn callback_scope_range(masked: &str) -> Option<std::ops::Range<usize>> {
    let mut parens = 0usize;
    for (index, c) in masked.char_indices() {
        match c {
            '(' => parens += 1,
            ')' => parens = parens.saturating_sub(1),
            '{' if parens == 1 => {
                let start = index + 1;
                let mut depth = 0usize;
                for (offset, c) in masked[start..].char_indices() {
                    match c {
                        '{' => depth += 1,
                        '}' if depth == 0 => return Some(start..start + offset),
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                return None;
            }
            _ => {}
        }
    }
    Some(0..masked.len())
}

/// `true` when the masked test text `before` the owner call opens an arrow
/// or a method-like block (see [`has_method_like_block`]: a `function`,
/// class, method or getter body), any of which may hold the call and run it more
/// than once. A mock's own type arguments and implementation
/// (`vi.fn<(m: string) => void>()`, `jest.fn((a, b) => a)`) are skipped when
/// they close before the call; any other arrow refuses, even one that ends
/// before the call.
fn opens_function_before_call(before: &str) -> bool {
    let before = without_closed_mock_factories(before);
    // A `function`, class, method or getter body opens with a `{` that the
    // allowlist reads as method-like (`function (id) {`, `class H {`).
    before.contains("=>") || has_method_like_block(&before)
}

/// `true` when the owner call at byte `at` of the masked test `scope` sits
/// in a parameter list: an enclosing `(` whose matching `)` is followed by
/// `=>`, `{` or a `:` return type (`(r = owner(...)) => r`,
/// `function run(r = owner(...)) {`). A default runs on every call that
/// omits the argument. An enclosing `(` that never closes also refuses; an
/// `if`, `switch` or `with` condition does not.
fn inside_parameter_list(scope: &str, at: usize) -> bool {
    let mut open = Vec::new();
    for (offset, c) in scope[..at].char_indices() {
        match c {
            '(' => open.push(offset),
            ')' => {
                open.pop();
            }
            _ => {}
        }
    }
    open.into_iter().any(|start| {
        // An `if`, `switch` or `with` condition runs once, though its `)` is
        // followed by `{`.
        let head = scope[..start].trim_end();
        let word = trailing_identifier(head);
        if ["if", "switch", "with"].contains(&word) && !is_property_name(head, word) {
            return false;
        }
        let Some(end) = closing_offset(&scope[start..], '(', ')') else {
            return true;
        };
        let after = scope[start + end..].trim_start();
        after.starts_with("=>") || after.starts_with('{') || after.starts_with(':')
    })
}

/// `text` with each `vi.fn` / `jest.fn` mock factory's type arguments and
/// arguments removed (`vi.fn<...>(...)`, `jest.fn(impl)`). Type arguments
/// count only when a `(` follows them and they read as types (see
/// [`type_like`]). A group that does not close inside
/// `text` is kept, so a call inside a mock implementation still reads as
/// nested. An `=>` inside the type arguments does not close the `<`.
fn without_closed_mock_factories(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(".fn") {
        let after = start + ".fn".len();
        let factory = ["vi", "jest"].iter().any(|receiver| {
            rest[..start].ends_with(receiver) && word_boundary_before(rest, start - receiver.len())
        });
        kept.push_str(&rest[..after]);
        rest = &rest[after..];
        if !factory || rest.starts_with(is_identifier_part) {
            continue;
        }
        let mut end = 0;
        if rest.starts_with('<') {
            match closing_offset(rest, '<', '>') {
                Some(close)
                    if rest[close..].trim_start().starts_with('(')
                        && type_like(&rest[1..close - 1]) =>
                {
                    end = close
                }
                _ => continue,
            }
        }
        // A space between the factory or its type arguments and `(` is the
        // same call, as in [`is_bare_mock_factory`] and
        // [`closed_mock_call`].
        let tail = &rest[end..];
        let whitespace = tail.len() - tail.trim_start().len();
        if tail[whitespace..].starts_with('(') {
            let Some(close) = closing_offset(&tail[whitespace..], '(', ')') else {
                kept.push_str(rest);
                return kept;
            };
            end += whitespace + close;
        }
        rest = &rest[end..];
    }
    kept.push_str(rest);
    kept
}

/// `true` when the text inside a factory's `<...>` reads as type arguments:
/// no `;`, `{` or `}`, and balanced parentheses. A comparison such as
/// `vi.fn<3; xs.forEach((w) => w>(0) && owner(...))` that only looks closed
/// keeps its text, so the callback in it still refuses.
fn type_like(text: &str) -> bool {
    !text.contains([';', '{', '}']) && text.matches('(').count() == text.matches(')').count()
}

/// The byte offset just past the `close` matching the `open` that starts
/// `text`, skipping `=>`, or `None` when it never closes.
fn closing_offset(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        if c == '=' && chars.peek().is_some_and(|(_, next)| *next == '>') {
            chars.next();
        } else if c == open {
            depth += 1;
        } else if c == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(offset + c.len_utf8());
            }
        }
    }
    None
}

/// The owner's masked body: the text after its header (the parameter list up
/// to the first top-level `{` or `=>`), or `None` when no body opens.
pub(super) fn masked_owner_body(owner_source: &str) -> Option<String> {
    let masked = ts_mask_literals_and_comments(owner_source);
    let mut parens = 0usize;
    for (index, c) in masked.char_indices() {
        match c {
            '(' => parens += 1,
            ')' => parens = parens.saturating_sub(1),
            '{' if parens == 0 => return Some(masked[index + 1..].to_string()),
            '=' if parens == 0 && masked[index..].starts_with("=>") => {
                return Some(masked[index + 2..].to_string());
            }
            _ => {}
        }
    }
    None
}

/// `true` when the changed line runs at most once per owner call: the
/// owner's body has no loop and no call back into the owner, and either it
/// has no nested function at all (no arrow, `function`, method shorthand or
/// class member), or (RIPR-SPEC-0256 rule 1) the changed
/// line is a whole top-level statement of the body with no function on it,
/// so a nested function elsewhere (`lines.reduce((sum, l) => ...)`) cannot
/// run it.
fn runs_changed_line_once(owner_source: &str, owner_name: &str, line_text: &str) -> bool {
    let Some(body) = masked_owner_body(owner_source) else {
        return false;
    };
    if !["for", "while", "do", owner_name]
        .iter()
        .all(|word| identifier_offsets(&body, word).is_empty())
    {
        return false;
    }
    let no_nested_function = !body.contains("=>")
        && identifier_offsets(&body, "function").is_empty()
        && !has_method_like_block(&body);
    no_nested_function || changed_line_is_top_level_statement(&body, line_text)
}

/// `true` when some `{` in the masked `body` may open a function body that
/// is not an arrow or `function`: method shorthand or a class member
/// (`run(x) {`, `get total() {`), a typed method (`run(): string {`,
/// `run(): { n: number } {`) or a class body. Only an allowlist of shapes
/// reads as a plain block or object literal: a `{` at the start of the body,
/// after `= ( , [ : ? & | ! ; {`, after a control keyword's `(...)` where
/// the keyword stands in statement position (see [`brace_context_at`]),
/// after `else`, `try`, `finally`, `do` or `return`, or opening a template
/// literal's `${`. Any other `{` counts, so an unfamiliar shape refuses, and
/// so does an object method named like a keyword (`{ catch(e) {...} }`).
///
/// Matching `()` pairs are indexed in one forward pass, and each brace
/// prefix is classified once, so nested control blocks are near-linear in
/// depth instead of walking every enclosing `{` again for every nested `{`.
fn has_method_like_block(body: &str) -> bool {
    const BLOCK_WORDS: [&str; 5] = ["else", "try", "finally", "do", "return"];
    let open_of = matching_parens(body);
    let mut memo = BraceMemo::new(body.len());
    body.char_indices()
        .filter(|(_, c)| *c == '{')
        .any(|(at, _)| {
            count_brace_work(1);
            // A template literal's `${` opens an expression, not a block.
            if body[..at].ends_with('$') {
                return false;
            }
            let before = body[..at].trim_end();
            let Some(last) = before.chars().next_back() else {
                return false;
            };
            if last == ')' {
                let Some(open) = open_paren_ending(before, &open_of) else {
                    return true;
                };
                return !brace_context_at(
                    body,
                    before[..open].trim_end().len(),
                    false,
                    &open_of,
                    &mut memo,
                );
            }
            if is_identifier_part(last) {
                // `cfg.try {` ends a property access (a return type such as
                // `typeof cfg.try`), not a `try` statement.
                let word = trailing_identifier(before);
                return !BLOCK_WORDS.contains(&word) || is_property_name(before, word);
            }
            !matches!(
                last,
                '=' | '(' | ',' | '[' | ':' | '?' | '&' | '|' | '!' | ';' | '{'
            )
        })
}

/// `open_of[close] = Some(open)` when `body[close]` is a `)` whose matching
/// `(` sits at `open`. Unmatched closers stay `None`.
fn matching_parens(body: &str) -> Vec<Option<usize>> {
    let mut open_of = vec![None; body.len()];
    let mut stack = Vec::new();
    for (at, c) in body.char_indices() {
        count_brace_work(1);
        match c {
            '(' => stack.push(at),
            ')' => {
                if let Some(open) = stack.pop()
                    && let Some(slot) = open_of.get_mut(at)
                {
                    *slot = Some(open);
                }
            }
            _ => {}
        }
    }
    open_of
}

/// The byte offset of the `(` matching the `)` that ends `prefix`, using a
/// [`matching_parens`] index of the whole body `prefix` belongs to.
fn open_paren_ending(prefix: &str, open_of: &[Option<usize>]) -> Option<usize> {
    let close = prefix.len().checked_sub(')'.len_utf8())?;
    open_of.get(close).copied().flatten()
}

/// Cached [`brace_context_at`] answers: 0 unknown, 1 false, 2 true.
struct BraceMemo {
    keyword: Vec<u8>,
    block: Vec<u8>,
}

impl BraceMemo {
    fn new(len: usize) -> Self {
        Self {
            keyword: vec![0; len.saturating_add(1)],
            block: vec![0; len.saturating_add(1)],
        }
    }

    fn get(&self, end: usize, block: bool) -> Option<bool> {
        let slot = if block { &self.block } else { &self.keyword };
        match slot.get(end).copied() {
            Some(1) => Some(false),
            Some(2) => Some(true),
            _ => None,
        }
    }

    fn set(&mut self, end: usize, block: bool, value: bool) {
        let slot = if block {
            &mut self.block
        } else {
            &mut self.keyword
        };
        if let Some(cell) = slot.get_mut(end) {
            *cell = if value { 2 } else { 1 };
        }
    }
}

#[cfg(test)]
thread_local! {
    static BRACE_WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn count_brace_work(amount: usize) {
    #[cfg(test)]
    BRACE_WORK.with(|work| work.set(work.get().saturating_add(amount)));
    #[cfg(not(test))]
    let _ = amount;
}

#[cfg(test)]
fn take_brace_work() -> usize {
    BRACE_WORK.with(|work| work.replace(0))
}

/// Walk for a control keyword before `(...)`. With `block == false`,
/// `body[..end]` ends with the word before a `(...)`; with `block == true`,
/// that prefix precedes a `{` that must open a statement block, not an
/// object literal: one at the start of the body, after `;`, `}`, a control
/// keyword's `(...)`, `else`, `try`, `finally` or `do`, or after a `{` that
/// itself opens a block (`return {`, `= {` and `({` open object literals).
/// `catch` must follow a `}`; after `,`, `:`, `.` or an object literal's `{`
/// the word names a method or property, so it is not control.
///
/// The walk is iterative and memoizes each `(end, block)` prefix, so deep
/// nesting cannot overflow the stack and does not rescan every enclosing
/// brace for every nested `{`.
fn brace_context_at(
    body: &str,
    mut end: usize,
    mut block: bool,
    open_of: &[Option<usize>],
    memo: &mut BraceMemo,
) -> bool {
    const CONTROL: [&str; 5] = ["if", "for", "while", "switch", "with"];
    const BLOCK_WORDS: [&str; 4] = ["else", "try", "finally", "do"];
    let mut path = Vec::new();
    let result = loop {
        count_brace_work(1);
        if let Some(hit) = memo.get(end, block) {
            break hit;
        }
        path.push((end, block));
        let text = if end <= body.len() { &body[..end] } else { "" };
        if !block {
            // `text` ends with the word before a `(...)`.
            let word = trailing_identifier(text);
            if is_property_name(text, word) {
                break false;
            }
            let before = text[..text.len() - word.len()].trim_end();
            if word == "catch" {
                break before.ends_with('}');
            }
            if !CONTROL.contains(&word) {
                break false;
            }
            // The keyword must start a statement.
            match before.chars().next_back() {
                None | Some(';' | '}' | ')') => break true,
                Some('{') => {
                    end = before.len().saturating_sub('{'.len_utf8());
                    block = true;
                }
                Some(_) => {
                    let word = trailing_identifier(before);
                    break word == "else" && !is_property_name(before, word);
                }
            }
        } else {
            // `text` precedes a `{` that must open a block.
            let before = text.trim_end();
            match before.chars().next_back() {
                None | Some(';' | '}') => break true,
                Some('{') => end = before.len().saturating_sub('{'.len_utf8()),
                Some(')') => {
                    let Some(open) = open_paren_ending(before, open_of) else {
                        break false;
                    };
                    end = before[..open].trim_end().len();
                    block = false;
                }
                Some(c) if is_identifier_part(c) => {
                    let word = trailing_identifier(before);
                    break BLOCK_WORDS.contains(&word) && !is_property_name(before, word);
                }
                Some(_) => break false,
            }
        }
    };
    for (cached_end, cached_block) in path {
        memo.set(cached_end, cached_block, result);
    }
    result
}

/// `true` when `word`, the identifier `text` ends with, is a property name
/// (`cfg.try`, `a?.if`) rather than a keyword.
fn is_property_name(text: &str, word: &str) -> bool {
    text[..text.len() - word.len()].trim_end().ends_with('.')
}

/// The byte offset of the `(` matching the `)` that ends `text`.
fn matching_open_paren(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (at, c) in text.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The identifier `text` ends with, or `""`.
fn trailing_identifier(text: &str) -> &str {
    let start = text
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_identifier_part(*c))
        .last()
        .map_or(text.len(), |(at, _)| at);
    &text[start..]
}

/// RIPR-SPEC-0256 rule 1: the masked changed line occurs once in the masked
/// owner `body`, outside every brace, parenthesis and bracket, right after
/// the start of the body, a `;`, a `}` or the head of a top-level braceless
/// `if (...)` / `else` (see [`follows_braceless_if_or_else`]), and holds no
/// `=>`, `function` or method-like block (see [`has_method_like_block`]).
/// A line after `=>` (an expression-bodied arrow) or inside a callback
/// refuses.
fn changed_line_is_top_level_statement(body: &str, line_text: &str) -> bool {
    let masked_line = ts_mask_literals_and_comments(line_text);
    let statement = masked_line.trim();
    if statement.is_empty()
        || statement.contains("=>")
        || !identifier_offsets(statement, "function").is_empty()
        || has_method_like_block(statement)
        || !starts_new_statement(statement)
    {
        return false;
    }
    let mut found = body.match_indices(statement);
    let (Some((at, _)), None) = (found.next(), found.next()) else {
        return false;
    };
    let mut depth = 0usize;
    for c in body[..at].chars() {
        match c {
            '{' | '(' | '[' => depth += 1,
            // An unmatched closer means the masking or the body boundary
            // is off; refuse rather than guess the statement's depth.
            '}' | ')' | ']' => match depth.checked_sub(1) {
                Some(next) => depth = next,
                None => return false,
            },
            _ => {}
        }
    }
    let before = body[..at].trim_end();
    depth == 0
        && (before
            .chars()
            .next_back()
            .is_none_or(|c| matches!(c, ';' | '}'))
            || follows_braceless_if_or_else(before))
}

/// RIPR-SPEC-0256 rule 1 (2026-10-15): `true` when `before`, the masked
/// body text before a top-level changed line with its trailing space
/// trimmed, ends with the head of a braceless branch that runs the line at
/// most once: `if (...)` whose `if` starts a statement (after the start of
/// the body, a `;` or a `}`), or `else` after a `;` or a `}`. That position
/// rule also refuses a word that ends a property access (`cfg.if (x)`,
/// `cfg.else`), since a `.` precedes it. The caller has
/// already placed the line outside every brace, parenthesis and bracket, so
/// the `if` / `else` sits directly in the owner body, not in a nested
/// function. A nested `if (a) if (b)`, an `else if (...)`, a labelled `if`,
/// an `else` right after a `)` and any other word before `(...)` refuse,
/// which under-credits rather than guess.
fn follows_braceless_if_or_else(before: &str) -> bool {
    if before.ends_with(')') {
        let Some(open) = matching_open_paren(before) else {
            return false;
        };
        let head = before[..open].trim_end();
        let word = trailing_identifier(head);
        if word != "if" {
            return false;
        }
        return head[..head.len() - word.len()]
            .trim_end()
            .chars()
            .next_back()
            .is_none_or(|c| matches!(c, ';' | '}'));
    }
    let word = trailing_identifier(before);
    word == "else"
        && before[..before.len() - word.len()]
            .trim_end()
            .chars()
            .next_back()
            .is_some_and(|c| matches!(c, ';' | '}'))
}

/// `true` when `statement` cannot continue the expression on the line before
/// it: it opens with an identifier that is not an operator or clause keyword.
/// A line opening with `:`, `?`, `&&`, `.`, `(` and the like continues an
/// expression (`const each = (id) => id < 0 ? {}` then `: p.m(id);`), so the
/// `}` before it closes an object literal inside a nested function, not a
/// block.
fn starts_new_statement(statement: &str) -> bool {
    const CONTINUATIONS: [&str; 9] = [
        "in",
        "instanceof",
        "as",
        "satisfies",
        "of",
        "else",
        "catch",
        "finally",
        "while",
    ];
    let word = statement
        .find(|c: char| !is_identifier_part(c))
        .map_or(statement, |end| &statement[..end]);
    statement.starts_with(is_identifier_start) && !CONTINUATIONS.contains(&word)
}

/// `true` when `at` sits outside every brace block of `masked`.
fn at_top_level(masked: &str, at: usize) -> bool {
    let mut depth = 0usize;
    for c in masked[..at].chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth == 0
}

fn strip_required_space(text: &str) -> Option<&str> {
    let trimmed = text.trim_start();
    (trimmed.len() < text.len()).then_some(trimmed)
}

fn word_boundary_before(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !is_identifier_part(c) && c != '.')
}

/// The text of a flat object literal up to its closing brace; `None` when it
/// nests braces or never closes, so a property is only read at top level.
fn literal_body(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (index, c) in text.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.checked_sub(1)?,
            '{' => return None,
            '}' if depth == 0 => return Some(&text[..index]),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn called_with(target: &str, expected: &str) -> TypeScriptMockPayload {
        TypeScriptMockPayload {
            target: target.to_string(),
            expected: expected.to_string(),
            kind: TypeScriptMockPayloadKind::CalledWith,
        }
    }

    fn params(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    fn nested_if_owner(depth: usize, inner: &str) -> String {
        let mut src = String::from("function announce(id, send) {\n");
        for _ in 0..depth {
            src.push_str("  if (a) {\n");
        }
        src.push_str(inner);
        src.push('\n');
        for _ in 0..depth {
            src.push_str("  }\n");
        }
        src.push('}');
        src
    }

    fn announce_facts(owner: &str, line: &str) -> MockCallFacts {
        MockCallFacts::read(line, Some(owner), "announce", &params(&["id", "send"]))
    }

    const SEND_LINE: &str = "  send(\"order \" + id);";
    const SEND_TEST: &str = "const send = jest.fn(); announce(7, send); expect(send).toHaveBeenCalledWith(\"order 7\");";

    fn measure_nested_if(depth: usize) -> Result<(bool, bool, usize), String> {
        let owner = nested_if_owner(depth, SEND_LINE);
        let body = masked_owner_body(&owner)
            .ok_or_else(|| format!("nested-if owner of depth {depth} must open a body"))?;
        let _ = take_brace_work();
        let facts = announce_facts(&owner, SEND_LINE);
        let work = take_brace_work();
        let credits = facts.credits(&called_with("send", "\"order 7\""), SEND_TEST);
        Ok((credits, has_method_like_block(&body), work))
    }

    #[test]
    fn method_like_block_allowlist_matches_known_shapes() {
        assert!(!has_method_like_block("  if (a) {\n  send(id);\n  }"));
        assert!(!has_method_like_block("  else {\n  send(id);\n  }"));
        assert!(!has_method_like_block(
            "  try {\n  send(id);\n  } finally {\n  }"
        ));
        assert!(!has_method_like_block("  const o = { a: 1, b: [2] };"));
        assert!(!has_method_like_block("  return { a: 1 };"));
        assert!(!has_method_like_block("  log(`${id}`);"));
        assert!(has_method_like_block(
            "  const h = { run(x) {\n  send(id);\n  } };"
        ));
        assert!(has_method_like_block(
            "  const h = { catch(e) {\n  send(id);\n  } };"
        ));
        assert!(has_method_like_block(
            "  const h = { run(x): typeof cfg.try {\n  send(id);\n  } };"
        ));
        assert!(!has_method_like_block(
            &"  if (a) {\n".repeat(8) + "  send(id);\n" + &"  }\n".repeat(8)
        ));
    }

    #[test]
    fn nested_if_owner_still_credits_a_literal_mock_payload() -> Result<(), String> {
        let (credits, method_like, _) = measure_nested_if(8)?;
        if method_like {
            return Err("nested if blocks must not read as methods".into());
        }
        if !credits {
            return Err("a once-run nested if owner must still credit".into());
        }
        Ok(())
    }

    #[test]
    fn nested_method_shorthand_still_refuses_credit() -> Result<(), String> {
        let owner = nested_if_owner(
            8,
            "  const h = { run(x) {\n  send(\"order \" + id);\n  } };",
        );
        let body = masked_owner_body(&owner).ok_or("method-shorthand owner must open a body")?;
        let facts = announce_facts(&owner, SEND_LINE);
        if !has_method_like_block(&body) {
            return Err("method shorthand inside nested ifs must stay method-like".into());
        }
        if facts.credits(&called_with("send", "\"order 7\""), SEND_TEST) {
            return Err("a method shorthand that may rerun the call must not credit".into());
        }
        Ok(())
    }

    #[test]
    fn nested_if_method_like_scan_is_near_linear_in_depth() -> Result<(), String> {
        let (credits_small, method_like_small, work_small) = measure_nested_if(500)?;
        let (credits_large, method_like_large, work_large) = measure_nested_if(2000)?;
        if method_like_small || method_like_large {
            return Err("nested if blocks at large depth must not read as methods".into());
        }
        if !credits_small || !credits_large {
            return Err("a once-run nested if owner must still credit at large depth".into());
        }
        if work_small == 0 || work_large <= work_small {
            return Err(format!(
                "the scan must record work at both depths (small={work_small}, large={work_large})"
            ));
        }
        // 4× depth would be ~16× work if each `{` still walked every
        // enclosing brace. A 6× cap leaves room for linear scan of a
        // longer body without accepting the quadratic walk.
        if work_large > work_small.saturating_mul(6) {
            return Err(format!(
                "work must stay near-linear: 500-deep={work_small}, 2000-deep={work_large}"
            ));
        }
        Ok(())
    }
}
