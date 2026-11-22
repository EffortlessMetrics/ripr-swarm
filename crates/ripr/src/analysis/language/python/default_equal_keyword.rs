//! An added keyword argument that restates the callee's default (#16011).
//!
//! `current_time.shift(**changes)` to `current_time.shift(check_imaginary=True,
//! **changes)`, where every workspace method `shift` declares
//! `check_imaginary=True`, binds the same value as before for every call. The
//! only difference is a duplicate-keyword `TypeError` when `**changes` holds
//! that name, which a value assertion on an ordinary call does not see.
//!
//! The lines are compared token by token as spelled, so whitespace inside a
//! string stays an edit and a call spelled inside a string is not a call.

use super::PythonOwner;
use crate::domain::OwnerKind;
use rustpython_parser::{Mode, Tok, lexer::lex};
use std::path::Path;

/// The keyword argument the new line adds to one call, when removing it gives
/// back the old line and its literal value equals the declared default of
/// every workspace definition the call can reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DefaultEqualKeyword {
    pub(super) callee: String,
    pub(super) argument: String,
}

type Token = (Tok, String);

pub(super) fn default_equal_keyword(
    old_line: &str,
    new_line: &str,
    file: &Path,
    owners: &[PythonOwner],
) -> Option<DefaultEqualKeyword> {
    let old = spellings(old_line)?;
    let new = spellings(new_line)?;
    for open in 1..new.len() {
        let (Tok::Name { .. }, callee) = &new[open - 1] else {
            continue;
        };
        if new[open].0 != Tok::Lpar {
            continue;
        }
        let attribute_call = open >= 2 && new[open - 2].0 == Tok::Dot;
        let Some(args) = call_arguments(&new, open) else {
            continue;
        };
        if args.iter().any(|&(start, _)| new[start].0 == Tok::Star) {
            continue; // `*rows` may bind any positional parameter
        }
        let positional = args
            .iter()
            .filter(|&&(start, end)| {
                !is_keyword(&new, start, end) && new[start].0 != Tok::DoubleStar
            })
            .count();
        for &(start, end) in &args {
            if !is_keyword(&new, start, end) || !removal_gives(&new, start, end, &old) {
                continue;
            }
            let value = &new[start + 2..end];
            if !is_plain_literal(value) {
                continue;
            }
            let call = Call {
                callee,
                attribute_call,
                positional,
                file,
            };
            if every_definition_defaults(owners, &call, &new[start].1, value) {
                return Some(DefaultEqualKeyword {
                    callee: callee.clone(),
                    argument: new[start..end]
                        .iter()
                        .map(|(_, text)| text.as_str())
                        .collect(),
                });
            }
        }
    }
    None
}

/// Each token of one trimmed line with its source spelling; None when the
/// line does not lex on its own or ends in a comment.
fn spellings(line: &str) -> Option<Vec<Token>> {
    let text = line.trim();
    let mut tokens = Vec::new();
    let mut code_end = 0usize;
    for result in lex(text, Mode::Module) {
        let (tok, range) = result.ok()?;
        if tok == Tok::Newline {
            continue;
        }
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        code_end = code_end.max(end);
        tokens.push((tok, text.get(start..end)?.to_string()));
    }
    text.get(code_end..)
        .is_some_and(|rest| rest.trim().is_empty())
        .then_some(tokens)
}

/// The `(start, end)` token spans of the top-level arguments of the call
/// whose `(` is at `open`; None when the bracket does not close on the line.
fn call_arguments(tokens: &[Token], open: usize) -> Option<Vec<(usize, usize)>> {
    let mut depth = 0usize;
    let mut args = Vec::new();
    let mut start = open + 1;
    for (index, (tok, _)) in tokens.iter().enumerate().skip(open) {
        match tok {
            Tok::Lpar | Tok::Lsqb | Tok::Lbrace => depth += 1,
            Tok::Rpar | Tok::Rsqb | Tok::Rbrace => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if start < index {
                        args.push((start, index));
                    }
                    return Some(args);
                }
            }
            Tok::Comma if depth == 1 => {
                args.push((start, index));
                start = index + 1;
            }
            _ => {}
        }
    }
    None
}

fn is_keyword(tokens: &[Token], start: usize, end: usize) -> bool {
    end > start + 2
        && matches!(tokens[start].0, Tok::Name { .. })
        && tokens[start + 1].0 == Tok::Equal
}

/// Whether deleting the argument at `start..end` with one adjoining comma, or
/// alone between the call's brackets, leaves exactly the old tokens.
fn removal_gives(new: &[Token], start: usize, end: usize, old: &[Token]) -> bool {
    let without = |from: usize, to: usize| -> bool {
        new[..from].iter().chain(new[to..].iter()).eq(old.iter())
    };
    let next = new.get(end).map(|(tok, _)| tok);
    let previous = start
        .checked_sub(1)
        .and_then(|index| new.get(index))
        .map(|(tok, _)| tok);
    (next == Some(&Tok::Comma) && without(start, end + 1))
        || (previous == Some(&Tok::Comma) && without(start - 1, end))
        || (previous == Some(&Tok::Lpar) && next == Some(&Tok::Rpar) && without(start, end))
}

/// A number, `True`, `False`, `None` or one plain (non f-) string, or a
/// signed number.
fn is_plain_literal(value: &[Token]) -> bool {
    match value {
        [(Tok::Int { .. } | Tok::Float { .. } | Tok::Complex { .. }, _)]
        | [(Tok::True | Tok::False | Tok::None, _)]
        | [(Tok::String { .. }, _)] => true,
        [
            (Tok::Minus | Tok::Plus, _),
            (Tok::Int { .. } | Tok::Float { .. } | Tok::Complex { .. }, _),
        ] => true,
        _ => false,
    }
}

struct Call<'a> {
    callee: &'a str,
    attribute_call: bool,
    positional: usize,
    file: &'a Path,
}

/// Method names of the built-in strings and containers. An attribute call
/// spelled with one of them may reach the built-in, whatever the workspace
/// defines under that name.
const BUILTIN_METHODS: &[&str] = &[
    "append",
    "capitalize",
    "center",
    "clear",
    "copy",
    "count",
    "decode",
    "encode",
    "endswith",
    "expandtabs",
    "extend",
    "find",
    "format",
    "get",
    "index",
    "insert",
    "items",
    "join",
    "keys",
    "ljust",
    "lower",
    "lstrip",
    "partition",
    "pop",
    "remove",
    "replace",
    "reverse",
    "rfind",
    "rindex",
    "rjust",
    "rpartition",
    "rsplit",
    "rstrip",
    "setdefault",
    "sort",
    "split",
    "splitlines",
    "startswith",
    "strip",
    "title",
    "translate",
    "update",
    "upper",
    "values",
    "zfill",
];

/// The call can reach at least one workspace definition, and every definition
/// it could reach declares `name` as a keyword-bindable parameter, past the
/// positional arguments, whose default is spelled as `value`. An attribute
/// call reaches methods, never under a built-in method name; a bare call
/// reaches functions, one of them in the changed file. A class of the
/// callee's name (its constructor) refuses.
fn every_definition_defaults(
    owners: &[PythonOwner],
    call: &Call<'_>,
    name: &str,
    value: &[Token],
) -> bool {
    if call.attribute_call && BUILTIN_METHODS.contains(&call.callee) {
        return false;
    }
    let named: Vec<&PythonOwner> = owners
        .iter()
        .filter(|owner| owner.name == call.callee)
        .collect();
    if named.iter().any(|owner| owner.owner_kind.is_none()) {
        return false;
    }
    let reachable: Vec<&PythonOwner> = named
        .into_iter()
        .filter(|owner| {
            if call.attribute_call {
                matches!(
                    owner.owner_kind,
                    Some(OwnerKind::Method | OwnerKind::ClassMethod)
                )
            } else {
                matches!(
                    owner.owner_kind,
                    Some(OwnerKind::Function | OwnerKind::ModuleFunction)
                )
            }
        })
        .collect();
    if reachable.is_empty()
        || (!call.attribute_call && !reachable.iter().any(|owner| owner.file == call.file))
    {
        return false;
    }
    reachable.iter().all(|owner| {
        // A method called through its receiver binds `self` / `cls` itself.
        let bound = usize::from(
            call.attribute_call
                && !owner
                    .decorators
                    .iter()
                    .any(|decorator| decorator == "staticmethod"),
        );
        owner
            .parameters
            .iter()
            .enumerate()
            .any(|(index, parameter)| {
                parameter.name == name
                    && !parameter.positional_only
                    && (parameter.keyword_only || index >= bound + call.positional)
                    && parameter
                        .default
                        .as_deref()
                        .and_then(spellings)
                        .is_some_and(|default| default.as_slice() == value)
            })
    })
}
