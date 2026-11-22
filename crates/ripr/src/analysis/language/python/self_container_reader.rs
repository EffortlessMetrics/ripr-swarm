//! Reader evidence for a private container a method records into (#16010).
//!
//! `self._seen.append(n + 1)` in an ordinary method changes what the object
//! holds, not what the method returns. A test that calls the method and pins
//! its return value reaches the owner without observing the recorded value,
//! so a strong oracle on that result must not read as discrimination of the
//! edit. The state is observed only by a test that reads the container or
//! calls something that reads it.
//!
//! The gap is reported only when static evidence can list every reader from
//! this file. Each condition that cannot be established keeps the existing
//! verdict:
//!
//! - the changed line and the line it replaced both record into the same
//!   single-underscore `self._name`, so readers are expected in this file,
//!   and neither recorded argument calls, assigns, awaits or yields, so the
//!   edit cannot change other state the result may carry;
//! - the owner is an undecorated, non-dunder method of a class with no bases,
//!   keywords or decorators, so no inherited method reads the container;
//! - the container is seeded in `__init__` by a fresh empty collection and
//!   nowhere outside a function body, so no caller holds an alias of it;
//! - the owner names the container on the changed line only, never hands
//!   `self` on, and is not itself reached from a reader, so its result does
//!   not carry the state;
//! - no dunder method reads the container, since Python invokes those through
//!   syntax, and the file uses no dynamic attribute access;
//! - no oracle-eligible related test uses a fixture, a setup receiver or a
//!   value helper, inspects attributes by name (`vars(c)`), imports a reader
//!   under another name, hands a local it binds to a call (`dump(c)`, which
//!   may read the container from another module), calls a method of its own
//!   test class other than an assertion, or names the container or a reader.
//!
//! A test that calls any reader keeps the existing verdict, even a reader
//! that projects only the container's size. The gate does not judge whether
//! that read distinguishes the edit, so its advice asks for the recorded
//! values, not for any reader call.
//!
//! Readers are found by whole-word name over the file's functions and methods
//! and closed over same-file callers. That over-approximates, which only
//! keeps the existing verdict more often.

use super::PythonOwner;
use super::effect_alignment::self_attribute_mutation;
use super::module_constants::mentions_identifier;
use super::related_tests::PythonRelatedCandidate;
use super::static_limits::is_simple_python_identifier;
use crate::domain::OwnerKind;

/// Why a recorded-container edit is not discriminated.
pub(super) struct SelfContainerGap {
    pub(super) discriminator: String,
    pub(super) reason: String,
    /// The assertion to add, as one sentence.
    pub(super) advice: String,
}

/// Fresh empty collections: a seed no caller can alias.
const FRESH_SEEDS: [&str; 7] = [
    "[]",
    "{}",
    "set()",
    "list()",
    "dict()",
    "deque()",
    "collections.deque()",
];

/// Spellings that read or write attributes by name, which a word search for
/// the container cannot follow.
const DYNAMIC_ACCESS: [&str; 5] = ["__dict__", "getattr", "setattr", "vars", "__getattr__"];

pub(super) fn python_self_container_reader_gap(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    owners: &[PythonOwner],
    owner_source: Option<&str>,
    line: usize,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<SelfContainerGap> {
    let new_line = line_text.trim();
    let old_line = old_line_text?.trim();
    let attr = self_attribute_mutation(new_line)?;
    if self_attribute_mutation(old_line) != Some(attr)
        || new_line == old_line
        || !has_inert_argument(new_line)
        || !has_inert_argument(old_line)
        || !attr.starts_with('_')
        || attr.starts_with("__")
        || !matches!(owner.owner_kind, Some(OwnerKind::Method))
        || !owner.decorators.is_empty()
        || is_dunder(&owner.name)
    {
        return None;
    }
    let lines: Vec<&str> = owner_source?.lines().collect();
    let changed = line.checked_sub(1)?;
    if lines.get(changed).map(|text| text.trim()) != Some(new_line) {
        return None;
    }
    // The innermost class around the owner, and nothing between them.
    let class = owners
        .iter()
        .filter(|candidate| {
            candidate.file == owner.file
                && candidate.owner_kind.is_none()
                && !candidate.is_module_owner()
                && candidate.start_line < owner.start_line
                && candidate.end_line >= owner.end_line
        })
        .max_by_key(|candidate| candidate.start_line)?;
    if !class.decorators.is_empty()
        || !is_baseless_class(lines.get(class.start_line.checked_sub(1)?)?.trim())
    {
        return None;
    }
    if lines.iter().any(|text| {
        DYNAMIC_ACCESS
            .iter()
            .any(|word| mentions_identifier(text, word))
    }) {
        return None;
    }
    let functions: Vec<&PythonOwner> = owners
        .iter()
        .filter(|candidate| {
            candidate.file == owner.file
                && candidate.owner_kind.is_some()
                && !candidate.is_module_owner()
        })
        .collect();
    // The innermost function a line sits in.
    let function_at = |index: usize| {
        functions
            .iter()
            .filter(|function| index + 1 >= function.start_line && index < function.end_line)
            .max_by_key(|function| function.start_line)
            .copied()
    };
    let mut seeded = false;
    let mut readers = std::collections::BTreeSet::new();
    for (index, text) in lines.iter().enumerate() {
        if index == changed || !mentions_identifier(text, attr) {
            continue;
        }
        let function = function_at(index)?;
        if function.start_line == owner.start_line {
            // Another read in the owner can reach its result.
            return None;
        }
        if function.name == "__init__"
            && function.start_line > class.start_line
            && function.end_line <= class.end_line
            && is_fresh_seed(text, attr)
        {
            seeded = true;
            continue;
        }
        if is_dunder(&function.name) {
            return None;
        }
        let _ = readers.insert(function.name.as_str());
    }
    if !seeded {
        return None;
    }
    let body = |function: &PythonOwner| {
        lines
            .iter()
            .skip(function.start_line)
            .take(function.end_line.saturating_sub(function.start_line))
    };
    // `self` handed on (`return self`, `notify(self)`) lets a caller read
    // the container without a method of this class.
    if body(owner).any(|text| names_bare_self(text)) {
        return None;
    }
    loop {
        let more: Vec<&str> = functions
            .iter()
            .filter(|function| !readers.contains(function.name.as_str()))
            .filter(|function| {
                readers
                    .iter()
                    .any(|reader| body(function).any(|text| mentions_identifier(text, reader)))
            })
            .map(|function| function.name.as_str())
            .collect();
        if more.is_empty() {
            break;
        }
        readers.extend(more);
    }
    if readers.contains(owner.name.as_str()) || readers.iter().any(|reader| is_dunder(reader)) {
        return None;
    }
    let mut eligible = related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .peekable();
    eligible.peek()?;
    for candidate in eligible {
        let test = candidate.test;
        if !test.fixtures.is_empty()
            || !test.setup_receivers.is_empty()
            || !test.value_helpers.is_empty()
            || DYNAMIC_ACCESS
                .iter()
                .any(|word| mentions_identifier(&test.body_text, word))
            || test.imports.iter().any(|import| {
                import.alias != import.imported
                    && readers.contains(
                        import
                            .imported
                            .rsplit('.')
                            .next()
                            .unwrap_or(import.imported.as_str()),
                    )
            })
            || hands_local_to_call(&test.body_text)
            || calls_own_helper_method(&test.body_text)
            || mentions_identifier(&test.body_text, attr)
            || readers
                .iter()
                .any(|reader| mentions_identifier(&test.body_text, reader))
        {
            return None;
        }
    }
    let read_by = if readers.is_empty() {
        "nothing in this file reads it".to_string()
    } else {
        format!(
            "it is read through {}",
            readers
                .iter()
                .map(|reader| format!("`{reader}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Some(SelfContainerGap {
        discriminator: format!("the values `{}` records in `self.{attr}`", owner.name),
        reason: format!(
            "no related test reads `self.{attr}` or calls a function that reads it ({read_by}), so the tests pin the result of `{}` without observing what it records",
            owner.name
        ),
        advice: format!(
            "After calling `{}`, assert the values `self.{attr}` holds, not only its size.",
            owner.name
        ),
    })
}

/// The recorded argument only computes a value: no call, assignment
/// expression, `await` or `yield` that could change other state.
fn has_inert_argument(line: &str) -> bool {
    let argument = line
        .split_once('(')
        .and_then(|(_, rest)| rest.rsplit_once(')'))
        .map_or("", |(argument, _)| argument);
    !argument.contains('(')
        && !argument.contains(":=")
        && !mentions_identifier(argument, "await")
        && !mentions_identifier(argument, "yield")
}

/// Whether the test passes a name it binds (`c = Clock(1)`, `with .. as c`)
/// as a call argument, so a function this file cannot list may read it.
fn hands_local_to_call(body: &str) -> bool {
    let locals: Vec<&str> = body
        .lines()
        .filter_map(|line| {
            let code = line.split('#').next().unwrap_or(line).trim();
            if let Some((_, target)) = code.rsplit_once(" as ") {
                return Some(target.trim_end_matches(':').trim());
            }
            let (target, value) = code.split_once('=')?;
            let target = target.split(':').next().unwrap_or(target).trim();
            (!value.starts_with('=')
                && !target.ends_with(['!', '<', '>', '+', '-', '*', '/', '%', '|', '&', '^']))
            .then_some(target)
        })
        .filter(|name| is_simple_python_identifier(name))
        .collect();
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    locals.iter().any(|name| {
        body.match_indices(name).any(|(start, _)| {
            let before = body[..start].trim_end().chars().next_back();
            let after = body[start + name.len()..].trim_start().chars().next();
            let whole = !body[..start].chars().next_back().is_some_and(is_word)
                && !body[start + name.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_word);
            whole
                && matches!(before, Some('(' | ',' | '=' | '*'))
                && matches!(after, Some(')' | ','))
        })
    })
}

/// Whether the test calls `self.name(..)` for a name that is not a
/// `unittest` assertion; such a helper may call a reader.
fn calls_own_helper_method(body: &str) -> bool {
    body.match_indices("self.").any(|(start, _)| {
        let rest = &body[start + "self.".len()..];
        let name_len = rest
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..name_len];
        !name.is_empty()
            && !name.starts_with("assert")
            && !name.starts_with("fail")
            && rest[name_len..].starts_with('(')
    })
}

fn is_dunder(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

/// `self._name = []` or `self._name: T = []`, with nothing after it but a
/// comment.
fn is_fresh_seed(line: &str, attr: &str) -> bool {
    let code = line.split('#').next().unwrap_or(line).trim();
    let Some(rest) = code
        .strip_prefix("self.")
        .and_then(|rest| rest.strip_prefix(attr))
    else {
        return false;
    };
    let rest = rest.trim_start();
    let value = match rest.strip_prefix(':') {
        Some(annotated) => annotated.split_once('=').map(|(_, value)| value),
        None => rest.strip_prefix('='),
    };
    value.is_some_and(|value| FRESH_SEEDS.contains(&value.trim()))
}

/// Whether `line` names `self` other than as the receiver of an attribute.
fn names_bare_self(line: &str) -> bool {
    let code = line.split('#').next().unwrap_or(line);
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    code.match_indices("self").any(|(start, _)| {
        let before = code[..start].chars().next_back();
        let after = code[start + 4..].chars().next();
        !before.is_some_and(|ch| is_word(ch) || ch == '.')
            && !after.is_some_and(is_word)
            && after != Some('.')
    })
}

/// `class Name:`, `class Name():` or `class Name(object):`.
fn is_baseless_class(header: &str) -> bool {
    let Some(rest) = header.strip_prefix("class ") else {
        return false;
    };
    let rest = rest.split('#').next().unwrap_or(rest).trim();
    let Some(rest) = rest.strip_suffix(':') else {
        return false;
    };
    match rest.split_once('(') {
        None => is_simple_python_identifier(rest.trim()),
        Some((class, bases)) => {
            is_simple_python_identifier(class.trim())
                && matches!(
                    bases.trim().strip_suffix(')').map(str::trim),
                    Some("" | "object")
                )
        }
    }
}
