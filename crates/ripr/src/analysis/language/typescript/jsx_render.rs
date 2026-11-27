//! JSX render reach (#15295, RIPR-SPEC-0243): a `<Name .../>` element in a
//! `.tsx`/`.jsx` test is a call of the imported `Name` component. Kept apart
//! from `related_tests.rs` so the scan stays a small, separately classified
//! unit.

use super::call_lexer::inside_block_comment;
use super::related_tests::{
    is_javascript_identifier_char, line_prefix_looks_like_comment_or_string,
};
use std::path::Path;

/// A JSX element `<Name ...>` / `<Name/>` rendered in a `.tsx`/`.jsx` test is
/// the call of a React component (#15295). Only a capitalized bare tag counts
/// (`<div>` is an intrinsic element and `<Ns.Item>` a member tag), and a `<`
/// that follows an identifier or `)` is a type argument or comparison. A `<`
/// right after `>` opens a nested or arrow-returned element
/// (`<Wrapper><Name/></Wrapper>`, `() =><Name/>`), so it counts. A generic
/// type parameter list `<Name extends X>` or `<Name = X>` is not a render.
/// The render proves reach only; assertion strength is judged separately.
pub(crate) fn contains_jsx_element_name(test_file: &Path, body_text: &str, name: &str) -> bool {
    let is_jsx_file = test_file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "tsx" | "jsx"));
    if !is_jsx_file || !name.starts_with(|ch: char| ch.is_ascii_uppercase()) {
        return false;
    }
    let needle = format!("<{name}");
    body_text.match_indices(&needle).any(|(idx, _)| {
        let rest = &body_text[idx + needle.len()..];
        let tag_ends = rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_whitespace() || ch == '/' || ch == '>');
        // `<Name extends X>(..) => ..` and `<Name = X>` open a generic type
        // parameter list, not an element: an element name is never followed
        // by `extends` or `=`.
        let after_name = rest.trim_start();
        let type_parameter = after_name.starts_with('=')
            || after_name
                .strip_prefix("extends")
                .is_some_and(|tail| tail.starts_with(|ch: char| ch.is_whitespace()));
        let before = body_text[..idx].chars().next_back();
        let opens_element =
            before.is_none_or(|ch| !(is_javascript_identifier_char(ch) || matches!(ch, ')' | '<')));
        tag_ends
            && !type_parameter
            && opens_element
            && !line_prefix_looks_like_comment_or_string(body_text, idx)
            && !inside_block_comment(body_text, idx)
    })
}
