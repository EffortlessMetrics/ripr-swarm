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
    /// Names called on the changed line through an owner parameter (`send`
    /// in `send(x)` for a parameter `send`, `write` in `db.write(x)` for a
    /// parameter `db`) that the owner body calls exactly once. Keywords are
    /// not calls. `toHaveBeenCalledWith` passes when ANY recorded call
    /// matches, so a second call to the same name could still match after
    /// the changed call is mutated; such a name is not kept. A call through
    /// anything but a parameter may reach code other than the test's mock.
    callees: Vec<String>,
}

impl MockCallFacts {
    /// `owner_body` is the owner's source after the change; `None` (not
    /// readable) keeps no callee, so rule 1 credits nothing.
    pub(crate) fn read(line_text: &str, owner_body: Option<&str>, owner_params: &[String]) -> Self {
        let Some(owner_body) = owner_body else {
            return Self::default();
        };
        let owner_calls = calls_in(owner_body);
        let callees = calls_in(line_text)
            .into_iter()
            .filter(|call| {
                owner_params.contains(&call.root)
                    && !body_redeclares(owner_body, &call.root)
                    && owner_calls
                        .iter()
                        .filter(|other| other.name == call.name)
                        .count()
                        == 1
            })
            .map(|call| call.name)
            .collect();
        Self { callees }
    }

    /// `true` when `payload` is an all-literal `toHaveBeenCalledWith` on a
    /// mock made in `test_body`, the test never calls it itself, and the
    /// changed line calls it.
    pub(crate) fn credits(&self, payload: &TypeScriptMockPayload, test_body: &str) -> bool {
        if payload.kind != TypeScriptMockPayloadKind::CalledWith {
            return false;
        }
        let segments: Vec<&str> = payload.target.split('.').collect();
        let Some(last) = segments.last() else {
            return false;
        };
        if !self.callees.iter().any(|callee| callee == last)
            || calls_in(test_body).iter().any(|call| call.name == *last)
        {
            return false;
        }
        match segments.as_slice() {
            [name] => test_local_mock_binding(test_body, name),
            [object, property] => test_local_mock_property(test_body, object, property),
            _ => false,
        }
    }
}

/// One call outside literals and comments: the called name and the first
/// identifier of its member chain (`notifier` for `notifier.send(`).
struct Call {
    name: String,
    root: String,
}

/// Every identifier followed by `(` or `?.(`, outside literals and comments.
fn calls_in(text: &str) -> Vec<Call> {
    let masked = ts_mask_literals_and_comments(text);
    let chars: Vec<char> = masked.chars().collect();
    let mut calls = Vec::new();
    let mut chain_root = String::new();
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
        let member = before > 0 && chars[before - 1] == '.';
        if !member {
            chain_root.clone_from(&name);
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
            });
        }
    }
    calls
}

/// `true` when the owner body declares `name` again, shadowing the
/// parameter.
fn body_redeclares(owner_body: &str, name: &str) -> bool {
    let masked = ts_mask_literals_and_comments(owner_body);
    ["const", "let", "var", "function"].iter().any(|keyword| {
        masked.match_indices(keyword).any(|(at, _)| {
            word_boundary_before(&masked, at)
                && strip_required_space(&masked[at + keyword.len()..])
                    .and_then(|rest| rest.strip_prefix(name))
                    .is_some_and(|rest| !rest.starts_with(is_identifier_part))
        })
    })
}

/// The owner's source lines after the change, read from the workspace;
/// `None` without a workspace root or a readable owner file.
pub(crate) fn ts_owner_body_text(
    owner: &TypeScriptOwner,
    workspace_root: Option<&Path>,
) -> Option<String> {
    let source = std::fs::read_to_string(workspace_root?.join(&owner.file)).ok()?;
    let lines: Vec<&str> = source
        .lines()
        .skip(owner.start_line.saturating_sub(1))
        .take(
            owner
                .end_line
                .saturating_add(1)
                .saturating_sub(owner.start_line),
        )
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
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
            starts_with_mock_factory(rest.trim_start()) && word_boundary_before(&masked, at)
        })
    })
}

/// `const db = { write: jest.fn() }`: `object` is bound to an object literal
/// in the test whose `property` is a mock factory call.
fn test_local_mock_property(body: &str, object: &str, property: &str) -> bool {
    let masked = ts_mask_literals_and_comments(body);
    ["const", "let", "var"].iter().any(|keyword| {
        masked.match_indices(keyword).any(|(at, _)| {
            if !word_boundary_before(&masked, at) {
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
            literal.split(',').any(|entry| {
                entry
                    .trim()
                    .strip_prefix(property)
                    .and_then(|value| value.trim_start().strip_prefix(':'))
                    .is_some_and(|value| starts_with_mock_factory(value.trim_start()))
            })
        })
    })
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
