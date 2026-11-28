//! Inline `new C(...)` receivers in an asserted value (#16482).
//!
//! The predicate boundary witness reads owner calls as `name(...)` or
//! `receiver.name(...)`, where the receiver is one identifier. A method call on
//! a fresh instance (`expect(new Cart().discount(100))`) has no identifier
//! receiver, so the witness rewrites each `new C(...)` member receiver, for a
//! class binding `C` the relation already resolved, to one placeholder
//! identifier and lists that placeholder as a receiver. The method's own
//! argument list is left as written.

use std::borrow::Cow;

use super::classifier::balanced_close_offset;
use super::related_tests::is_javascript_identifier_char;

/// The identifier that stands for an inline `new C(...)` receiver. `$` keeps
/// it out of reach of any name a test can bind by accident.
pub(super) const INLINE_NEW_RECEIVER: &str = "$riprNew$";

/// `observed` with every `new C(<args>).` or `new C(<args>)?.` for a
/// constructor `C` in `constructors` replaced by `INLINE_NEW_RECEIVER.`.
/// Unchanged when none occurs. A `new` that is part of a longer identifier or
/// a member name (`x.new`), a constructor named only as a prefix of a longer
/// name, unbalanced arguments, or a construction that is not followed by a
/// member access are left as written, which can only withhold a witness.
pub(super) fn rewrite_inline_new_receivers<'a>(
    observed: &'a str,
    constructors: &[String],
) -> Cow<'a, str> {
    if constructors.is_empty() || !observed.contains("new") {
        return Cow::Borrowed(observed);
    }
    let code = code_bytes(observed);
    let mut out = String::with_capacity(observed.len());
    let mut copied = 0;
    let mut search = 0;
    while let Some(found) = observed[search..].find("new") {
        let at = search + found;
        search = at + "new".len();
        // A `new` inside a string or a comment is text, not a receiver: an
        // argument such as `"new Cart().x"` stays as written.
        if !code.get(at).copied().unwrap_or(false) {
            continue;
        }
        if let Some(end) = inline_new_receiver_end(observed, at, constructors) {
            out.push_str(&observed[copied..at]);
            out.push_str(INLINE_NEW_RECEIVER);
            out.push('.');
            copied = end;
            search = end;
        }
    }
    if copied == 0 {
        return Cow::Borrowed(observed);
    }
    out.push_str(&observed[copied..]);
    Cow::Owned(out)
}

/// For each byte of `text`, `true` when it is code: outside a quoted string,
/// a template literal, a regex literal and a comment. A byte counts as code
/// only when both shared masks agree, the regex-aware one and the one that
/// reads every `/` as division, so a regex holding a quote (`/"/`) cannot
/// expose a string argument to the rewrite. Text a mask does not map byte
/// for byte reads as all text, which can only withhold a rewrite.
fn code_bytes(text: &str) -> Vec<bool> {
    let lexical = super::classifier::ts_mask_literals_and_comments(text);
    let division = super::classifier::ts_mask_literals_and_comments_division_only(text);
    if lexical.len() != text.len() || division.len() != text.len() {
        return vec![false; text.len()];
    }
    text.bytes()
        .zip(lexical.bytes().zip(division.bytes()))
        .map(|(byte, (left, right))| byte == left && byte == right)
        .collect()
}

/// The byte offset just past the member dot of an inline `new C(...)`
/// receiver that starts at `at`, or `None` when the text there is not one.
fn inline_new_receiver_end(observed: &str, at: usize, constructors: &[String]) -> Option<usize> {
    let starts_word = observed[..at]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_javascript_identifier_char(ch) && ch != '.');
    if !starts_word {
        return None;
    }
    let after_new = &observed[at + "new".len()..];
    let name_text = after_new.trim_start();
    if name_text.len() == after_new.len() {
        return None;
    }
    let constructor = constructors.iter().find(|constructor| {
        name_text
            .strip_prefix(constructor.as_str())
            .is_some_and(|rest| {
                rest.chars()
                    .next()
                    .is_none_or(|ch| !is_javascript_identifier_char(ch))
            })
    })?;
    let after_name = name_text[constructor.len()..].trim_start();
    let arguments = after_name.strip_prefix('(')?;
    let close = balanced_close_offset(arguments)?;
    let rest = &arguments[close + 1..];
    let trimmed = rest.trim_start();
    let member = trimmed
        .strip_prefix("?.")
        .or_else(|| trimmed.strip_prefix('.'))?;
    Some(observed.len() - member.len())
}
