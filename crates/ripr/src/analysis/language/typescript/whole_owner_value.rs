//! The asserted value must be the owner call itself (#16496).
//!
//! A predicate boundary witness reads the arguments of an owner call found in
//! the asserted text. That says the boundary input reached the owner, not that
//! the assertion can tell the two sides of the boundary apart: a comparison
//! (`discount(100) > 0`), arithmetic, a ternary or a negation can map both
//! sides to one passing value. Renamed imports (#16452) and method receivers
//! (#16482) already witness only through the whole call; this module gives a
//! free function or a namespace member call the same rule.
//!
//! A call written only inside a comment is not a call, so comments are removed
//! from the asserted text before any owner call is read from it.

use std::borrow::Cow;

use super::classifier::{balanced_close_offset, observed_is_one_owner_call_spelling};

/// `observed` without `//` and `/* */` comments outside quotes. Unchanged
/// when it holds no comment opener.
pub(super) fn observed_without_comments(observed: &str) -> Cow<'_, str> {
    if !observed.contains("//") && !observed.contains("/*") {
        return Cow::Borrowed(observed);
    }
    Cow::Owned(super::guard_entry_guard::strip_comments(observed))
}

/// Whether the asserted value is one whole owner call, for any of
/// `call_spellings` (a name and the receivers it may be called through),
/// once value-preserving forms are unwrapped:
///
/// - balanced parentheses, `(owner(100))`;
/// - `await owner(100)`;
/// - `Promise.resolve(owner(100))` and `Promise.reject(owner(100))`, which a
///   `.resolves` or `.rejects` matcher reads back unchanged;
/// - a verified test-local identity helper, `wrap(owner(100))`;
/// - a thunk for a throw matcher, `() => owner(100)` or
///   `() => { owner(100); }`.
///
/// Anything else around the call, an operator, a member read, a template or
/// another call, is not modeled, so it is not the whole owner value.
pub(super) fn observed_asserts_whole_owner_value(
    observed: &str,
    call_spellings: &[(&str, &[String])],
    identity_helpers: &[&str],
) -> bool {
    let mut value = observed.trim();
    // Each step strips at least one byte, so the loop ends.
    loop {
        if call_spellings
            .iter()
            .any(|(name, receivers)| observed_is_one_owner_call_spelling(value, name, receivers))
        {
            return true;
        }
        let Some(inner) = unwrap_value_preserving(value, identity_helpers) else {
            return false;
        };
        value = inner.trim();
    }
}

/// One value-preserving layer around `value`, removed, or `None` when
/// `value` has none.
fn unwrap_value_preserving<'a>(value: &'a str, identity_helpers: &[&str]) -> Option<&'a str> {
    if let Some(inner) = whole_parenthesized(value) {
        return Some(inner);
    }
    if let Some(rest) = value.strip_prefix("await")
        && rest.starts_with(|c: char| c.is_whitespace() || c == '(')
    {
        return Some(rest);
    }
    if let Some(body) = thunk_body(value) {
        return Some(body);
    }
    let promise = ["Promise.resolve", "Promise.reject"]
        .into_iter()
        .find_map(|wrapper| single_argument_of(value, wrapper));
    if promise.is_some() {
        return promise;
    }
    identity_helpers
        .iter()
        .find_map(|helper| single_argument_of(value, helper))
}

/// The text inside `( ... )` when the parentheses enclose all of `value`.
fn whole_parenthesized(value: &str) -> Option<&str> {
    let after_open = value.strip_prefix('(')?;
    let close = balanced_close_offset(after_open)?;
    after_open[close + 1..]
        .trim()
        .is_empty()
        .then(|| &after_open[..close])
}

/// The one argument of `callee(<argument>)` when that call is all of
/// `value`. A second top-level argument is refused: it is not the identity
/// form the wrapper is read as.
fn single_argument_of<'a>(value: &'a str, callee: &str) -> Option<&'a str> {
    let rest = value.strip_prefix(callee)?.trim_start();
    let inner = whole_parenthesized(rest)?;
    (!has_top_level_comma(inner)).then_some(inner)
}

/// The body of `() => <expression>`, `async () => <expression>` or
/// `() => { <expression>; }` (with or without `return`).
fn thunk_body(value: &str) -> Option<&str> {
    let value = value
        .strip_prefix("async")
        .filter(|rest| rest.starts_with(|c: char| c.is_whitespace() || c == '('))
        .map_or(value, str::trim_start);
    let rest = value.strip_prefix('(')?.trim_start().strip_prefix(')')?;
    let body = rest.trim_start().strip_prefix("=>")?.trim();
    let Some(block) = body.strip_prefix('{') else {
        return Some(body);
    };
    let block = block.strip_suffix('}')?.trim();
    let statement = block.strip_suffix(';').unwrap_or(block).trim();
    if statement.contains(';') {
        return None;
    }
    Some(
        statement
            .strip_prefix("return")
            .filter(|rest| rest.starts_with(char::is_whitespace))
            .map_or(statement, str::trim),
    )
}

/// Whether `text` has a comma outside every bracket pair and string.
fn has_top_level_comma(text: &str) -> bool {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in text.chars() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}
