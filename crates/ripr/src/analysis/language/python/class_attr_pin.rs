//! Class-attribute string constant pin evidence for the Python preview
//! adapter (#15294).
//!
//! A changed class-body `name = "old text"` -> `name = "new text"` is a value
//! the class carries to whatever reads it later (a message formatted into an
//! exception, a label rendered into output). Tests that only construct the
//! class and compare an unrelated return value reach the owner without ever
//! observing the text, so a strong oracle on any such test must not read as
//! discrimination of the edit.
//!
//! The change is discriminated only when a related test's assertion carries a
//! plain string literal that the OLD text accepts and the NEW text rejects
//! (`pytest.raises(E, match="...")` naming the old message, or an equality on
//! the formatted message). `{placeholders}` in the text are wildcards. A
//! literal that fits both texts (an unchanged prefix) pins neither, and a
//! literal with regex syntax or an f-string brace is unreadable; neither
//! credits the edit. The gate engages only on a message-only edit of a
//! string-literal assignment directly owned by a class; every other shape
//! keeps the existing verdict.

use super::PythonOwner;
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

/// Why a class-attribute text edit is not discriminated.
pub(super) struct ClassAttrPinGap {
    pub(super) discriminator: String,
    pub(super) reason: String,
}

pub(super) fn python_class_attr_pin_gap(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    owner_source: Option<&str>,
    line: usize,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<ClassAttrPinGap> {
    let (name, new_text) = assigned_string(line_text)?;
    let (old_name, old_text) = assigned_string(old_line_text?)?;
    if name != old_name || old_text == new_text {
        return None;
    }
    let lines: Vec<&str> = owner_source?.lines().collect();
    let start = owner.start_line.checked_sub(1)?;
    let changed = line.checked_sub(1)?;
    if !lines.get(start)?.trim_start().starts_with("class ")
        || changed <= start
        || changed >= owner.end_line.min(lines.len())
    {
        return None;
    }
    let old = tokens(&old_text);
    let new = tokens(&new_text);
    let pinned = related_candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .flat_map(|candidate| candidate.test.assertions.iter())
        .flat_map(|assertion| string_literals(&assertion.gate_text))
        .any(|literal| {
            let literal: Vec<char> = literal.chars().collect();
            literal.len() >= 4 && infix_matches(&old, &literal) && !infix_matches(&new, &literal)
        });
    if pinned {
        return None;
    }
    Some(ClassAttrPinGap {
        discriminator: format!("class attribute `{name}` text {new_text:?}"),
        reason: format!(
            "no related assertion carries a literal that the old `{name}` text {old_text:?} accepts and the new text {new_text:?} rejects, so the tests reach `{}` without observing the changed text",
            owner.name
        ),
    })
}

/// The name and literal of a line that is exactly `name = "text"` or
/// `name: T = "text"`.
fn assigned_string(line: &str) -> Option<(String, String)> {
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("<class-attr>"), line.trim())
    else {
        return None;
    };
    let (target, value) = match module.body.as_slice() {
        [Stmt::Assign(assign)] => match assign.targets.as_slice() {
            [target] => (target, assign.value.as_ref()),
            _ => return None,
        },
        [Stmt::AnnAssign(assign)] => (assign.target.as_ref(), assign.value.as_deref()?),
        _ => return None,
    };
    let Expr::Name(name) = target else {
        return None;
    };
    let Expr::Constant(constant) = value else {
        return None;
    };
    let ast::Constant::Str(text) = &constant.value else {
        return None;
    };
    Some((name.id.to_string(), text.clone()))
}

#[derive(Clone, Copy)]
enum Token {
    Char(char),
    Wildcard,
}

/// The text as characters with each `{...}` placeholder as one wildcard.
fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
            }
            out.push(Token::Wildcard);
        } else {
            out.push(Token::Char(ch));
        }
    }
    out
}

/// Whether some instantiation of the wildcards contains `literal` as a
/// substring.
fn infix_matches(tokens: &[Token], literal: &[char]) -> bool {
    (0..=tokens.len()).any(|begin| {
        let mut states = vec![(begin, 0usize, 0usize)];
        let mut seen = std::collections::BTreeSet::new();
        while let Some((i, j, fixed)) = states.pop() {
            if !seen.insert((i, j, fixed)) {
                continue;
            }
            // At least half the literal must be fixed text of the constant, so
            // a placeholder cannot absorb the whole literal.
            if j == literal.len() && fixed * 2 >= literal.len() {
                return true;
            }
            match tokens.get(i) {
                Some(Token::Char(ch)) if literal.get(j) == Some(ch) => {
                    states.push((i + 1, j + 1, fixed + 1))
                }
                Some(Token::Wildcard) => {
                    states.push((i + 1, j, fixed));
                    if j < literal.len() {
                        states.push((i, j + 1, fixed));
                    }
                }
                _ => {}
            }
        }
        false
    })
}

/// Plain quoted literals of the source text; one with regex syntax, an
/// escape or a brace is unreadable and dropped.
fn string_literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' && ch != '\'' {
            continue;
        }
        let mut literal = String::new();
        let mut closed = false;
        for inner in chars.by_ref() {
            if inner == ch {
                closed = true;
                break;
            }
            literal.push(inner);
        }
        if closed && !literal.chars().any(|c| "\\^$*+?()[]{}|\n".contains(c)) {
            out.push(literal);
        }
    }
    out
}
