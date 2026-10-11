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
    /// an inner parameter shadowing it all add a mention and refuse.
    pub(crate) fn read(
        line_text: &str,
        owner_body: Option<&str>,
        owner_name: &str,
        owner_params: &[String],
    ) -> Self {
        let Some(owner_body) = owner_body else {
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
    pub(crate) fn credits(&self, payload: &TypeScriptMockPayload, test_body: &str) -> bool {
        // Jest's recursive equality treats a property set to `undefined` as
        // absent, so an object payload does not pin every field.
        if payload.kind != TypeScriptMockPayloadKind::CalledWith
            || ts_mask_literals_and_comments(&payload.expected).contains('{')
        {
            return false;
        }
        let segments: Vec<&str> = payload.target.split('.').collect();
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
        let scope = test_callback_block(test_body).unwrap_or(test_body);
        let made_in_test = match &call.method {
            None => test_local_mock_binding(scope, local),
            Some(method) => test_local_mock_property(scope, local, method),
        };
        made_in_test
            && identifier_mentions_outside_expect(test_body, local) == 2
            && owner_call_passes_at(test_body, &self.owner_name, local, call.index)
    }
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
            depth += 1;
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
fn identifier_offsets(masked: &str, name: &str) -> Vec<usize> {
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
    let masked = ts_mask_literals_and_comments(test_body);
    let mut calls = masked.match_indices(owner_name).filter_map(|(at, _)| {
        if !(word_boundary_before(&masked, at) || masked[..at].ends_with('.')) {
            return None;
        }
        let rest = masked[at + owner_name.len()..].trim_start();
        top_level_arguments(rest.strip_prefix('(')?)
    });
    let (Some(arguments), None) = (calls.next(), calls.next()) else {
        return false;
    };
    arguments
        .iter()
        .position(|argument| argument.trim() == local)
        .is_some_and(|at| {
            at == index
                && !arguments[..at]
                    .iter()
                    .any(|argument| argument.trim_start().starts_with("..."))
        })
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

fn is_identifier_part(c: char) -> bool {
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
            starts_with_mock_factory(rest.trim_start())
                && word_boundary_before(&masked, at)
                && at_top_level(&masked, at)
        })
    })
}

/// `const db = { write: jest.fn() }`: `object` is bound to an object literal
/// in the test whose `property` is a mock factory call. A spread, a second
/// `property` key or a key that is not a plain identifier could replace the
/// factory, so any of them refuses.
fn test_local_mock_property(body: &str, object: &str, property: &str) -> bool {
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
            let Some(literal) = rest.trim_start().strip_prefix('{') else {
                return false;
            };
            let Some(literal) = literal_body(literal) else {
                return false;
            };
            // Every entry is `key: value` with a plain key, the property is
            // keyed once, and no spread or other key can replace it.
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
                (Some((_, value)), None) if starts_with_mock_factory(value)
            )
        })
    })
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

fn starts_with_mock_factory(text: &str) -> bool {
    MOCK_FACTORIES.iter().any(|factory| {
        text.strip_prefix(factory)
            .is_some_and(|rest| rest.starts_with('(') || rest.starts_with('<'))
    })
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
