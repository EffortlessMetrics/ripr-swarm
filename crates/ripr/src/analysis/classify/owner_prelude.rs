//! A flat owner prelude before a boundary local (#12771).
//!
//! A local boundary operand (`let unit = if si { KIB } else { KB };`) reads a
//! flag from the caller's row and its arms from the file's constants. The
//! owner may rebind either before the local: `let si = !si;` flips the flag,
//! and a function-local `const KB: u64 = 2_000;` replaces the file's `KB`.
//! Read without those, the caller's literal flag or the file constant names a
//! boundary the comparison never sees (RIPR-SPEC-0158 examples 9 and 11).
//!
//! This module reads only one exact shape: every statement between the
//! owner's opening brace and the local's `let` is either a negated flag
//! rebinding `let NAME = !NAME;` or an integer constant item
//! `const NAME: <integer type> = <decimal literal>;`, with no braces, calls,
//! macros or other statements, and every other mention of each rebound name in
//! the owner is inside the local's own initializer. Anything else returns
//! `None`, and the caller keeps its fail-closed refusal.

use crate::analysis::facts::FunctionSummary;

/// The integer types a function-local constant may declare.
const INTEGER_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// What the prelude rebinds before the local.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct OwnerPrelude {
    /// Parameters rebound by `let NAME = !NAME;`.
    pub(super) negated_flags: Vec<String>,
    /// Function-local constants and their decimal literals.
    pub(super) constants: Vec<(String, String)>,
}

impl OwnerPrelude {
    /// Whether the prelude rebinds `name`, so a rebinding check on it is
    /// already answered here.
    pub(super) fn binds(&self, name: &str) -> bool {
        self.negated_flags.iter().any(|flag| flag == name)
            || self.constants.iter().any(|(constant, _)| constant == name)
    }

    /// The initializer with every negated flag's condition flipped:
    /// `if si {..}` reads `if !si {..}` and `if !si {..}` reads `if si {..}`.
    /// A negated flag anywhere else in the initializer returns `None`.
    pub(super) fn rewrite_initializer(&self, initializer: &str) -> Option<String> {
        let mut text = initializer.trim().to_string();
        for flag in &self.negated_flags {
            let rest = text.strip_prefix("if ")?.trim_start();
            let (negated, after) = match rest.strip_prefix('!') {
                Some(after) => (true, after.trim_start()),
                None => (false, rest),
            };
            let after = after.strip_prefix(flag.as_str())?;
            if !after.starts_with([' ', '{']) {
                return None;
            }
            // The arms must not read the flag again.
            if token_count(after, flag) != 0 {
                return None;
            }
            text = if negated {
                format!("if {flag}{after}")
            } else {
                format!("if !{flag}{after}")
            };
        }
        Some(text)
    }
}

/// The flat prelude before `let operand = initializer;`, or `None` when the
/// owner holds anything this module does not read exactly. A prelude that
/// rebinds nothing is also `None`.
pub(super) fn owner_prelude(
    owner: &FunctionSummary,
    operand: &str,
    initializer: &str,
    parameters: &[String],
) -> Option<OwnerPrelude> {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(
        &crate::analysis::language::mask_rust_char_literals(&owner.body),
    );
    let open = masked.find('{')?;
    let initializer = initializer.trim();
    if initializer.is_empty() || !is_identifier(operand) {
        return None;
    }
    // The local is declared exactly once, as `let operand = initializer;`.
    let declaration = format!("let {operand} = ");
    let mut declarations = masked.match_indices(&declaration).filter(|(start, _)| {
        !masked[..*start]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    });
    let (let_at, _) = declarations.next()?;
    if declarations.next().is_some() || let_at <= open {
        return None;
    }
    let at = let_at + declaration.len();
    let rest = owner.body.get(at..)?;
    if !rest.starts_with(initializer) || !rest[initializer.len()..].trim_start().starts_with(';') {
        return None;
    }
    let prelude = masked.get(open + 1..let_at)?;
    if prelude.contains(['{', '}', '(', ')', '[', ']', '|', '#', '"', '\'']) {
        return None;
    }
    let mut found = OwnerPrelude::default();
    let mut statements = prelude.split(';').collect::<Vec<_>>();
    // The text after the last `;` is the whitespace before the local's `let`.
    if !statements.pop()?.trim().is_empty() {
        return None;
    }
    for statement in statements {
        let words = statement.split_whitespace().collect::<Vec<_>>();
        match words.as_slice() {
            ["let", name, "=", negated] if negated.strip_prefix('!') == Some(*name) => {
                if !parameters.iter().any(|parameter| parameter == name) || found.binds(name) {
                    return None;
                }
                found.negated_flags.push((*name).to_string());
            }
            ["const", named, ty, "=", literal] => {
                let name = named.strip_suffix(':')?;
                // A parameter of the same name would win in the row inputs,
                // although Rust reads the block's item, so it declines.
                if !super::activation::is_constant_like(name)
                    || matches!(name, "true" | "false")
                    || parameters.iter().any(|parameter| parameter == name)
                    || !INTEGER_TYPES.contains(ty)
                    || !is_decimal_literal(literal)
                    || found.binds(name)
                {
                    return None;
                }
                found
                    .constants
                    .push((name.to_string(), (*literal).to_string()));
            }
            _ => return None,
        }
    }
    if found.negated_flags.is_empty() && found.constants.is_empty() {
        return None;
    }
    // Every other mention of a rebound name sits in the local's initializer:
    // a negated flag is named twice by its rebinding, a constant once by its
    // item. A later shadow, a nested item, a closure or a second read would
    // add a mention, so the prelude declines.
    let body = masked.get(open + 1..)?;
    let initializer_masked = masked.get(at..at + initializer.len())?;
    let accounted = |name: &str, own: usize| {
        token_count(body, name) == own + token_count(initializer_masked, name)
    };
    if !found.negated_flags.iter().all(|flag| accounted(flag, 2))
        || !found.constants.iter().all(|(name, _)| accounted(name, 1))
    {
        return None;
    }
    // A macro's expansion is not read, so it could rebind a name unseen.
    if has_macro_call(body) {
        return None;
    }
    Some(found)
}

fn is_identifier(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_decimal_literal(text: &str) -> bool {
    text.starts_with(|ch: char| ch.is_ascii_digit())
        && !text.ends_with('_')
        && text.chars().all(|ch| ch.is_ascii_digit() || ch == '_')
}

/// How many times `name` occurs in `text` as a whole identifier token.
fn token_count(text: &str, name: &str) -> usize {
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    text.match_indices(name)
        .filter(|(start, _)| {
            !text[..*start].chars().next_back().is_some_and(is_word)
                && !text[start + name.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_word)
        })
        .count()
}

/// Whether `text` holds a macro call: an identifier, optionally followed by
/// whitespace (`rebind !()` is legal), then `!` and an opening delimiter.
/// A keyword before a negated group (`return !(x)`) also matches, which only
/// declines.
fn has_macro_call(text: &str) -> bool {
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    text.match_indices('!').any(|(at, _)| {
        text[..at]
            .trim_end()
            .chars()
            .next_back()
            .is_some_and(is_word)
            && text[at + 1..].trim_start().starts_with(['(', '[', '{'])
    })
}

#[cfg(test)]
#[path = "owner_prelude_tests.rs"]
mod tests;
