//! Whether a Python source patches, mocks or monkeypatches a given name (#10391).
//!
//! The bare-bool credit needs the test's callee to stay bound to the function
//! it imported. A file-wide `mock` / `patch` substring disqualified nearly
//! every real pytest file, so this pass reads the token stream and keeps the
//! old veto for everything except a small closed allow-list:
//!
//! - `monkeypatch.setenv` / `delenv` / `chdir` (environment, not a name);
//! - mock constructors (`Mock`, `MagicMock`, ..) and `assert_*` / `reset_mock`;
//! - imports of patch-like names, never aliased with `as`.
//!
//! Any other call on a patch-like name (`patch`, `patch.object`, `setattr`,
//! `delattr`, `setitem`, `patch.dict`, `monkeypatch.context()`, `mocker.spy`, a
//! helper containing the marker) disqualifies, as does any alias or
//! reference to such an API (`swap = mock.patch.object`, `partial(patch.object)`),
//! and any allowed call whose arguments name the callee or the owner, contain a
//! string with a backslash (an escape can spell the name), an f-string,
//! adjacent literals, `*args` / `**kwargs` or `autospec`.
//!
//! Accepted residuals: a collaborator patched by name (`patch("src.rules.helper")`
//! disqualifies, but only the veto, not a credit), conftest autouse fixtures,
//! and a direct `rules.f = fake` assignment (counted by the binding scan, not
//! here).

use rustpython_parser::{Mode, Tok, lexer::lex};

const PATCH_MARKERS: [&str; 5] = ["patch", "mock", "Mock", "setattr", "delattr"];

/// Names that rebind or patch when called, aliased or passed along.
const PATCH_API_NAMES: [&str; 12] = [
    "patch", "object", "multiple", "dict", "setattr", "delattr", "setitem", "delitem", "context",
    "spy", "partial", "stopall",
];

/// Names that build or inspect a mock object without patching anything.
const MOCK_OBJECT_NAMES: [&str; 9] = [
    "Mock",
    "MagicMock",
    "AsyncMock",
    "PropertyMock",
    "NonCallableMock",
    "NonCallableMagicMock",
    "mock_open",
    "ANY",
    "DEFAULT",
];

/// The only calls on a patch-like name that keep the credit: they change the
/// process environment or working directory, never a Python name.
const ENVIRONMENT_VERBS: [&str; 3] = ["setenv", "delenv", "chdir"];

fn has_marker(text: &str) -> bool {
    PATCH_MARKERS.iter().any(|marker| text.contains(marker))
}

/// Whether `text` holds `name` as a whole identifier.
fn mentions_identifier(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(idx, _)| {
        let word = |ch: char| ch.is_alphanumeric() || ch == '_';
        text[..idx].chars().next_back().is_none_or(|ch| !word(ch))
            && text[idx + name.len()..]
                .chars()
                .next()
                .is_none_or(|ch| !word(ch))
    })
}

/// Whether `source` may patch, mock or rebind any of `names`, or does so in a
/// way this reading cannot follow. A source that does not lex is not trusted.
pub(super) fn may_patch_names(source: &str, names: &[&str]) -> bool {
    let mut tokens: Vec<Tok> = Vec::new();
    for token in lex(source, Mode::Module) {
        match token {
            Ok((token, _)) => tokens.push(token),
            Err(_) => return true,
        }
    }
    let regions = Regions::of(&tokens);
    // A formatted string hides its expressions from the token stream.
    if tokens.iter().any(|token| {
        matches!(token, Tok::String { kind, value, .. }
            if kind.is_any_fstring() && (has_marker(value) || names.iter().any(|name| value.contains(name))))
    }) {
        return true;
    }
    for index in 0..tokens.len() {
        if import_alias_hides_marker(&tokens, &regions, index) {
            return true;
        }
        let Some(chain) = Chain::at(&tokens, index) else {
            continue;
        };
        if !chain.segments.iter().any(|segment| has_marker(segment)) {
            continue;
        }
        if chain_may_patch(&tokens, &regions, index, &chain, names) {
            return true;
        }
    }
    false
}

/// Token positions that are parameter lists or import statements.
struct Regions {
    parameters: Vec<bool>,
    imports: Vec<bool>,
    declarations: Vec<bool>,
}

impl Regions {
    fn of(tokens: &[Tok]) -> Self {
        let mut regions = Self {
            parameters: vec![false; tokens.len()],
            imports: vec![false; tokens.len()],
            declarations: vec![false; tokens.len()],
        };
        for (index, token) in tokens.iter().enumerate() {
            match token {
                Tok::Def | Tok::Class => {
                    if let Some(slot) = regions.declarations.get_mut(index + 1) {
                        *slot = true;
                    }
                    if matches!(token, Tok::Def)
                        && matches!(tokens.get(index + 2), Some(Tok::Lpar))
                        && let Some(close) = matching_close(tokens, index + 2)
                    {
                        regions.parameters[index + 3..close].fill(true);
                    }
                }
                Tok::Lambda => {
                    let end = tokens[index..]
                        .iter()
                        .position(|token| matches!(token, Tok::Colon))
                        .map_or(tokens.len(), |offset| index + offset);
                    regions.parameters[index + 1..end].fill(true);
                }
                Tok::Import | Tok::From => {
                    let end = tokens[index..]
                        .iter()
                        .position(|token| matches!(token, Tok::Newline | Tok::Semi))
                        .map_or(tokens.len(), |offset| index + offset);
                    regions.imports[index..end].fill(true);
                }
                _ => {}
            }
        }
        regions
    }
}

/// `import patch as p` / `from unittest import mock as m` hides a patch-like
/// name behind an alias this reading would not recognize later.
fn import_alias_hides_marker(tokens: &[Tok], regions: &Regions, index: usize) -> bool {
    matches!(tokens[index], Tok::As)
        && regions.imports[index]
        && index > 0
        && matches!(&tokens[index - 1], Tok::Name { name } if has_marker(name))
}

/// A dotted name `a.b.c`, with the index of its last token.
struct Chain<'a> {
    segments: Vec<&'a str>,
    end: usize,
}

impl<'a> Chain<'a> {
    /// The dotted name starting at `index`, when a name starts there. A name
    /// after a dot starts a chain only when the dot follows a non-name (`f().mock`).
    fn at(tokens: &'a [Tok], index: usize) -> Option<Self> {
        let Tok::Name { name } = &tokens[index] else {
            return None;
        };
        if index >= 1 && matches!(tokens[index - 1], Tok::Dot) {
            let after_name = index >= 2 && matches!(tokens[index - 2], Tok::Name { .. });
            if after_name {
                return None;
            }
        }
        let mut segments = vec![name.as_str()];
        let mut end = index;
        while matches!(tokens.get(end + 1), Some(Tok::Dot))
            && let Some(Tok::Name { name }) = tokens.get(end + 2)
        {
            segments.push(name.as_str());
            end += 2;
        }
        Some(Self { segments, end })
    }
}

fn chain_may_patch(
    tokens: &[Tok],
    regions: &Regions,
    start: usize,
    chain: &Chain<'_>,
    names: &[&str],
) -> bool {
    if regions.declarations[start] {
        return false;
    }
    let last = chain.segments.last().copied().unwrap_or("");
    let next = tokens.get(chain.end + 1);
    if matches!(next, Some(Tok::Lpar)) {
        return call_may_patch(tokens, chain, chain.end + 1, names);
    }
    // A reference, not a call. A name that is not itself patch-like (the
    // `return_value` of `mock_get.return_value`) cannot alias a patcher.
    if regions.imports[start] {
        return false;
    }
    // `swap = mock.patch.object`, `partial(patch.object, ..)`: any segment.
    if chain
        .segments
        .iter()
        .any(|segment| PATCH_API_NAMES.contains(segment))
    {
        return true;
    }
    if !has_marker(last) || MOCK_OBJECT_NAMES.contains(&last) {
        return false;
    }
    let previous = start.checked_sub(1).and_then(|before| tokens.get(before));
    // A keyword name (`f(mock=True)`) binds nothing.
    if matches!(next, Some(Tok::Equal))
        && matches!(previous, Some(Tok::Lpar | Tok::Comma))
        && !regions.parameters[start]
    {
        return false;
    }
    // A declared parameter or its annotation (`mocker: MockerFixture`), not a
    // default value that could alias a patcher.
    if regions.parameters[start] {
        return matches!(previous, Some(Tok::Equal));
    }
    true
}

fn call_may_patch(tokens: &[Tok], chain: &Chain<'_>, open: usize, names: &[&str]) -> bool {
    let Some(close) = matching_close(tokens, open) else {
        return true;
    };
    if args_name_a_target_or_hide_one(&tokens[open + 1..close], names) {
        return true;
    }
    let verb = chain.segments.last().copied().unwrap_or("");
    let safe = ENVIRONMENT_VERBS.contains(&verb)
        || MOCK_OBJECT_NAMES.contains(&verb)
        || verb.starts_with("assert")
        || verb == "reset_mock";
    !safe
}

/// Any identifier or string in the arguments that names a target, or a form
/// (`*args`, f-string, adjacent literals, `autospec`) whose target is unreadable.
fn args_name_a_target_or_hide_one(args: &[Tok], names: &[&str]) -> bool {
    let mut previous_was_string = false;
    for token in args {
        let is_string = matches!(token, Tok::String { .. });
        let hides = match token {
            Tok::Name { name } => {
                name.contains("autospec") || names.iter().any(|callee| name == callee)
            }
            Tok::String { value, kind, .. } => {
                kind.is_any_fstring()
                    // An escape or line continuation can spell the name unreadably.
                    || value.contains('\\')
                    || previous_was_string
                    || names
                        .iter()
                        .any(|callee| mentions_identifier(value, callee))
            }
            Tok::Star | Tok::DoubleStar => true,
            _ => false,
        };
        if hides {
            return true;
        }
        previous_was_string = is_string;
    }
    false
}

/// The index of the bracket that closes the one at `open`.
fn matching_close(tokens: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, token) in tokens.get(open..)?.iter().enumerate() {
        match token {
            Tok::Lpar | Tok::Lsqb | Tok::Lbrace => depth += 1,
            Tok::Rpar | Tok::Rsqb | Tok::Rbrace => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}
