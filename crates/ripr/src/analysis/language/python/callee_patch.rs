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
    // The edit's taint, closed over every name bound from it, never reaches a
    // call argument, a nested scope or a rebinding the owner declares.
    if edit_reaches_owner_calls(owner, owner_source, &edit_names) {
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

/// One logical statement of the owner: physical lines joined while a bracket
/// is open or a backslash continues.
struct Statement {
    text: String,
    indent: usize,
}

fn owner_statements(owner: &PythonOwner, owner_source: &str) -> Vec<Statement> {
    let mut out = Vec::new();
    let mut current: Option<Statement> = None;
    let mut depth = 0i32;
    for (index, line) in owner_source.lines().enumerate() {
        let number = index + 1;
        if number < owner.start_line || number > owner.end_line {
            continue;
        }
        if current.is_none() && line.trim().is_empty() {
            continue;
        }
        let statement = current.get_or_insert_with(|| Statement {
            text: String::new(),
            indent: line.len() - line.trim_start().len(),
        });
        statement.text.push(' ');
        statement.text.push_str(line.trim());
        for ch in line.chars() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = (depth - 1).max(0),
                _ => {}
            }
        }
        if depth == 0
            && !line.trim_end().ends_with('\\')
            && let Some(done) = current.take()
        {
            out.push(done);
        }
    }
    out.extend(current);
    out
}

/// `(target text, value text, augmented)` of the last top-level `=` of
/// `text`, ignoring comparisons, walrus and keyword arguments.
fn assignment_split(text: &str) -> Option<(&str, &str, bool)> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut last = None;
    let mut skip = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if skip {
            skip = false;
            continue;
        }
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = (depth - 1).max(0),
            b'=' if depth == 0 => {
                if bytes.get(index + 1) == Some(&b'=') {
                    skip = true;
                    continue;
                }
                let prev = index.checked_sub(1).map(|at| bytes[at]);
                let shift = matches!(prev, Some(b'<' | b'>'))
                    && index.checked_sub(2).map(|at| bytes[at]) == prev;
                if matches!(prev, Some(b'=' | b'!' | b':'))
                    || (!shift && matches!(prev, Some(b'<' | b'>')))
                {
                    continue;
                }
                let augmented = shift
                    || matches!(
                        prev,
                        Some(b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'@')
                    );
                last = Some((index, augmented));
            }
            _ => {}
        }
    }
    last.map(|(index, augmented)| (&text[..index], &text[index + 1..], augmented))
}

/// Names a statement binds when it reads a tainted name: assignment targets,
/// `for` and `as` targets, walrus targets, and the receiver of an attribute or
/// subscript (a method may mutate it).
fn bound_names(text: &str) -> Vec<&str> {
    let mut bound = Vec::new();
    if let Some((target, _, _)) = assignment_split(text) {
        bound.extend(data_names(target));
    }
    let all = tokens(text);
    for (position, (start, token)) in all.iter().enumerate() {
        match *token {
            "for" => bound.extend(
                all[position + 1..]
                    .iter()
                    .map(|(_, name)| *name)
                    .take_while(|name| *name != "in")
                    .filter(|name| is_plain_name(name)),
            ),
            "as" => bound.extend(
                all.get(position + 1)
                    .map(|(_, name)| *name)
                    .filter(|name| is_plain_name(name)),
            ),
            name if is_plain_name(name) => {
                let after = text[start + name.len()..].trim_start();
                if after.starts_with('.') || after.starts_with('[') || after.starts_with(":=") {
                    bound.push(name);
                }
            }
            _ => {}
        }
    }
    bound
}

fn is_function_header(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("def ") || text.starts_with("async def ") || text.starts_with("class ")
}

/// Whether the names the edit touches, closed over every name bound on a
/// statement that reads one, reach a call argument, a nested scope, a lambda
/// or a `global`/`nonlocal` declaration of the owner. Unsure reads true.
fn edit_reaches_owner_calls(owner: &PythonOwner, owner_source: &str, edit_names: &[&str]) -> bool {
    let statements = owner_statements(owner, owner_source);
    let header = statements.iter().position(|s| is_function_header(&s.text));
    let body: Vec<(usize, &Statement)> = statements
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != header)
        .collect();
    let mut taint: Vec<&str> = edit_names.to_vec();
    loop {
        let before = taint.len();
        for (_, statement) in &body {
            let names = data_names(&statement.text);
            let reads = match assignment_split(&statement.text) {
                Some((target, value, augmented)) => {
                    let mut reads = data_names(value);
                    if augmented {
                        reads.extend(data_names(target));
                    }
                    let other_binders = statement.text.contains(" for ")
                        || statement.text.contains(" as ")
                        || statement.text.contains(":=");
                    if other_binders {
                        reads = names.clone();
                    }
                    reads
                }
                None => names.clone(),
            };
            if reads.iter().any(|name| taint.contains(name)) {
                for name in bound_names(&statement.text) {
                    if !taint.contains(&name) {
                        taint.push(name);
                    }
                }
            }
        }
        if taint.len() == before {
            break;
        }
    }
    let tainted = |text: &str| data_names(text).iter().any(|name| taint.contains(name));
    let mut nested_indent: Option<usize> = None;
    for (_, statement) in &body {
        let text = statement.text.trim_start();
        if nested_indent.is_some_and(|indent| statement.indent > indent) {
            if tainted(text) {
                return true;
            }
        } else {
            nested_indent = None;
        }
        if text.starts_with("global ") || text.starts_with("nonlocal ") {
            return true;
        }
        if is_function_header(text) {
            if tainted(text) {
                return true;
            }
            nested_indent = Some(statement.indent);
        }
        if tokens(text).iter().any(|(_, token)| *token == "lambda") && tainted(text) {
            return true;
        }
        if call_arguments(text)
            .iter()
            .any(|arguments| tainted(arguments))
        {
            return true;
        }
    }
    false
}

/// The text inside every call parenthesis of `text`, whole across lines, other
/// than a safe builtin's. A parenthesis after a name, `)` or `]` is a call.
fn call_arguments(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    for (open, _) in text.match_indices('(') {
        let before = text[..open].trim_end();
        let callee = tokens(before)
            .pop()
            .filter(|(start, token)| start + token.len() == before.len());
        let called = match callee {
            Some((_, token)) => is_plain_name(token) && !SAFE_CALLS.contains(&token),
            None => before.ends_with(')') || before.ends_with(']'),
        };
        if !called {
            continue;
        }
        let mut depth = 0i32;
        let mut close = bytes.len();
        for (offset, &byte) in bytes[open..].iter().enumerate() {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        spans.push(&text[open + 1..close]);
    }
    spans
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
