//! The `typeof` kind of a statically literal call argument (#11196).
//!
//! A changed `typeof input === "function"` guard does not compare `input`
//! with the string `"function"`; it asks what kind of value `input` is. The
//! discriminating test input is therefore a value of that kind (a function),
//! never the type-name string itself. This module reads the kind a call
//! argument is written with, and nothing it cannot read from the argument
//! text alone.

/// The `typeof` result of an argument written as a literal or an inline
/// function, or `None` when the text does not fix the kind (an identifier, a
/// call, a member read, any other expression). A trailing type assertion
/// (`x as any`, `x as unknown as T`) does not change the runtime value, so
/// it is read through.
pub(crate) fn literal_argument_typeof_kind(argument: &str) -> Option<&'static str> {
    let mut text = argument.trim();
    // Only a top-level ` as ` cast is stripped: the head must be balanced so
    // an arrow body (`() => x as T`) is not cut inside its own expression.
    while let Some((head, _)) = text.rsplit_once(" as ") {
        let head = head.trim_end();
        if !is_balanced(head) {
            break;
        }
        text = head;
    }
    let mut text = text.trim();
    // `(() => 42)`: parentheses around the whole argument carry no kind.
    while wraps_whole(text, '(', ')') {
        text = text.get(1..text.len() - 1).unwrap_or("").trim();
    }
    if text.is_empty() {
        return None;
    }
    if is_function_expression(text) {
        return Some("function");
    }
    if let Some(quote) = text
        .chars()
        .next()
        .filter(|ch| matches!(ch, '"' | '\'' | '`'))
    {
        let inner = text.get(1..text.len().checked_sub(1)?)?;
        let closed = text.len() >= 2 && text.ends_with(quote);
        let has_interpolation = quote == '`' && inner.contains("${");
        let plain = !inner.contains(quote) && !has_interpolation;
        return (closed && plain && !inner.contains('\\')).then_some("string");
    }
    if matches!(text, "true" | "false") {
        return Some("boolean");
    }
    if text == "null" {
        return Some("object");
    }
    if text == "undefined" {
        return Some("undefined");
    }
    let unsigned = text.strip_prefix('-').unwrap_or(text).trim_start();
    if super::classifier::numeric_literal_value(unsigned).is_some() {
        return Some(if text.trim_end().ends_with('n') {
            "bigint"
        } else {
            "number"
        });
    }
    if wraps_whole(text, '{', '}') || wraps_whole(text, '[', ']') {
        return Some("object");
    }
    None
}

/// `function (...) {...}`, `async function ...`, or an arrow whose whole text
/// is `<params> => <body>`.
fn is_function_expression(text: &str) -> bool {
    let text = text.strip_prefix("async ").unwrap_or(text).trim_start();
    if let Some(rest) = text.strip_prefix("function") {
        return rest.starts_with(|ch: char| ch.is_whitespace() || matches!(ch, '(' | '*'));
    }
    let Some(arrow) = text.find("=>") else {
        return false;
    };
    let params = text.get(..arrow).unwrap_or("").trim();
    if params.contains(['"', '\'', '`']) {
        return false;
    }
    let bare_identifier = !params.is_empty()
        && params
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '$'));
    bare_identifier
        || (wraps_whole(params, '(', ')') && !text.get(arrow + 2..).unwrap_or("").trim().is_empty())
}

fn is_balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// The text opens with `open` and its matching `close` is the last character.
fn wraps_whole(text: &str, open: char, close: char) -> bool {
    if !text.starts_with(open) || !text.ends_with(close) {
        return false;
    }
    let mut depth = 0i32;
    let last = text.len().saturating_sub(close.len_utf8());
    for (idx, ch) in text.char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return idx == last;
            }
        }
    }
    false
}
