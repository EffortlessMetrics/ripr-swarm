//! Tests that replace a function around the changed owner, and tests that
//! replace the changed owner itself (#13632).
//!
//! `mocked_module` withholds credit from any oracle-eligible test that uses
//! patch syntax, because a stand-in may replace the very behavior the test
//! pins. Two narrower readings live here, each fail-closed:
//!
//! - [`callee_only_patch`]: the test replaces only a *module-level callee* of
//!   the owner with a literal stand-in, and the edit neither touches nor feeds
//!   that callee, so the exact assertion still reads the edit. The patched name
//!   is never the owner itself.
//! - [`owner_patched_by_every_test`]: every related test replaces the owner
//!   (the changed function itself), so none runs the changed code. It reads as
//!   reached-but-unrevealed, never as a credited discriminator.
//!
//! A non-substituting related test beside mock-patched siblings (#13633) is
//! read independently of them: [`drop_substituting_siblings`] sets the siblings
//! aside, and the caller keeps the original limit unless the independent tests
//! alone earn `exposed`.

use super::literal_patch::{literal_patch_calls, literal_patch_targets, mentions_substitution};
use super::related_tests::{PythonRelatedCandidate, is_python_identifier_char, owner_module_paths};
use super::static_limits::test_has_mocked_module;
use super::{PythonOwner, PythonTest};

/// Identifier-like tokens of `text`, in order, with the byte offset of each.
fn tokens(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if is_python_identifier_char(ch) {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            out.push((begin, &text[begin..index]));
        }
    }
    if let Some(begin) = start {
        out.push((begin, &text[begin..]));
    }
    out
}

const KEYWORDS: &[&str] = &[
    "return", "if", "else", "elif", "not", "and", "or", "in", "is", "None", "True", "False", "for",
    "while", "lambda", "yield", "await", "raise", "assert", "pass", "import", "from", "as", "with",
    "try", "except", "finally", "class", "def", "del", "global", "nonlocal", "break", "continue",
    "async",
];

/// Builtins a changed line may call without reaching a patched name.
const SAFE_CALLS: &[&str] = &[
    "int", "str", "len", "float", "bool", "list", "tuple", "dict", "set", "sorted", "round", "abs",
    "sum", "min", "max", "repr",
];

fn is_plain_name(token: &str) -> bool {
    !token.starts_with(|ch: char| ch.is_ascii_digit()) && !KEYWORDS.contains(&token)
}

/// The names `line` reads or writes, minus keywords, numbers and safe builtins.
fn data_names(line: &str) -> Vec<&str> {
    tokens(line)
        .into_iter()
        .map(|(_, token)| token)
        .filter(|token| is_plain_name(token) && !SAFE_CALLS.contains(token))
        .collect()
}

/// Whether `line` calls anything but a safe builtin, or reads an attribute.
fn line_reaches_beyond_names(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (start, token) in tokens(line) {
        if !is_plain_name(token) {
            continue;
        }
        let after = line[start + token.len()..].trim_start();
        let called = after.starts_with('(');
        if called && !SAFE_CALLS.contains(&token) {
            return true;
        }
    }
    line.char_indices().any(|(index, ch)| {
        if ch != '.' {
            return false;
        }
        let Some(before) = index.checked_sub(1).and_then(|prev| bytes.get(prev)) else {
            return false;
        };
        let before = char::from(*before);
        if matches!(before, ')' | ']') {
            return true;
        }
        if !is_python_identifier_char(before) {
            return false;
        }
        // `1.5` is a number, `a.b` an attribute read.
        let head = tokens(&line[..index]).pop().map(|(_, token)| token);
        head.is_some_and(is_plain_name)
    })
}

/// Whether every substitution in `test` replaces a module-level callee of
/// `owner` with a literal stand-in, and the edit from `old_line` to `new_line`
/// neither names that callee nor feeds a value into any call the owner makes.
/// Fails closed: any shape not recognized returns false.
pub(super) fn callee_only_patch(
    test: &PythonTest,
    owner: &PythonOwner,
    owner_source: &str,
    old_line: Option<&str>,
    new_line: &str,
) -> bool {
    let Some(old_line) = old_line else {
        return false;
    };
    if owner_source.is_empty()
        || test.constant_rebinding.fixture_substitution
        || test
            .decorators
            .iter()
            .any(|decorator| mentions_substitution(decorator))
    {
        return false;
    }
    let Some(targets) = literal_patch_targets(test, owner, true) else {
        return false;
    };
    if targets.is_empty() {
        return false;
    }
    let owner_segments: Vec<&str> = owner.qualified_name.split('.').collect();
    let module_paths = owner_module_paths(&owner.file);
    let mut patched = Vec::new();
    for target in &targets {
        let segments: Vec<&str> = target.split('.').collect();
        let Some((name, prefix)) = segments.split_last() else {
            return false;
        };
        if prefix.is_empty()
            || segments
                .iter()
                .any(|segment| !is_plain_name(segment) || segment.is_empty())
            || segments
                .iter()
                .any(|segment| owner_segments.contains(segment))
            || !module_paths
                .iter()
                .any(|module| *module == prefix.join("."))
        {
            return false;
        }
        patched.push(*name);
    }
    // The edit is not at, and not an argument of, a patched call.
    let edit_names: Vec<&str> = data_names(old_line)
        .into_iter()
        .chain(data_names(new_line))
        .collect();
    if line_reaches_beyond_names(old_line)
        || line_reaches_beyond_names(new_line)
        || edit_names.iter().any(|name| patched.contains(name))
    {
        return false;
    }
    // No other call in the owner receives a value the edit touches, directly
    // or through a name it binds. A call spread over lines is not read.
    let edited_text = [old_line.trim(), new_line.trim()];
    for line in owner_source.lines() {
        let text = line.trim();
        if edited_text.contains(&text)
            || text.starts_with("def ")
            || text.starts_with("async def ")
            || text.starts_with("class ")
        {
            continue;
        }
        for (start, token) in tokens(line) {
            if !is_plain_name(token) || SAFE_CALLS.contains(&token) {
                continue;
            }
            let rest = &line[start + token.len()..];
            let Some(arguments) = rest.trim_start().strip_prefix('(') else {
                continue;
            };
            let opens = arguments.matches('(').count() + 1;
            if opens != arguments.matches(')').count() {
                return false;
            }
            if data_names(arguments)
                .iter()
                .any(|name| edit_names.contains(name))
            {
                return false;
            }
        }
    }
    true
}

/// `"a.b.c"` string literals that follow a `setattr(` or `patch(` opener in
/// `body`. Read by text: it only decides how an already-uncredited finding is
/// worded, never lifts a limit.
fn patch_string_targets(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    for opener in ["setattr(", "patch(", "patch.object("] {
        for (index, _) in body.match_indices(opener) {
            let rest = body[index + opener.len()..].trim_start();
            let Some(quote) = rest.chars().next().filter(|ch| matches!(ch, '"' | '\'')) else {
                continue;
            };
            let inner = &rest[1..];
            if let Some(end) = inner.find(quote) {
                found.push(inner[..end].to_string());
            }
        }
    }
    found
}

/// Whether `test` replaces `owner` itself: a patch target whose last segment is
/// the owner's name under one of the owner's module paths.
fn test_patches_owner(test: &PythonTest, owner: &PythonOwner) -> bool {
    if owner.is_module_owner() || !test_has_mocked_module(test) {
        return false;
    }
    let module_paths = owner_module_paths(&owner.file);
    patch_string_targets(&test.body_text).iter().any(|target| {
        target.rsplit_once('.').is_some_and(|(prefix, name)| {
            name == owner.name && module_paths.iter().any(|module| module == prefix)
        })
    })
}

/// Whether there is at least one related test and every one of them replaces
/// the owner itself.
pub(super) fn owner_patched_by_every_test(
    owner: &PythonOwner,
    candidates: &[PythonRelatedCandidate<'_>],
) -> bool {
    !candidates.is_empty()
        && candidates
            .iter()
            .all(|candidate| test_patches_owner(candidate.test, owner))
}

/// Set aside the oracle-eligible related tests that substitute behavior when
/// an oracle-eligible test with no substitution at all also reaches the owner.
/// Returns whether anything was dropped. A sibling that leaves a patch running
/// (`.start(`) is never set aside, since the patch may outlive its test.
pub(super) fn drop_substituting_siblings(
    candidates: &mut Vec<PythonRelatedCandidate<'_>>,
    owner: &PythonOwner,
    owner_source: &str,
    old_line: Option<&str>,
    new_line: &str,
) -> bool {
    let substitutes = |test: &PythonTest| {
        test_has_mocked_module(test) || test.constant_rebinding.fixture_substitution
    };
    let independent = candidates
        .iter()
        .any(|candidate| candidate.relation.uses_oracle() && !substitutes(candidate.test));
    if !independent {
        return false;
    }
    let droppable = |candidate: &PythonRelatedCandidate<'_>| {
        candidate.relation.uses_oracle()
            && substitutes(candidate.test)
            && literal_patch_calls(candidate.test, owner, owner_source, old_line, new_line)
                .is_none()
    };
    if candidates
        .iter()
        .any(|candidate| droppable(candidate) && candidate.test.body_text.contains(".start("))
    {
        return false;
    }
    let before = candidates.len();
    candidates.retain(|candidate| !droppable(candidate));
    candidates.len() != before
}
