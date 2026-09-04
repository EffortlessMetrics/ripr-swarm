//! Shared comment/string masking for the lexical extractors.
//!
//! `extract_call_facts` and `extract_literal_facts` scan raw source text,
//! so commented-out calls and numbers would otherwise become live
//! evidence. `mask_comments_and_strings` erases line comments, block
//! comments (nested, as Rust permits), plain string literals, character
//! literals, and raw strings (any hash count, with the `r`, `br`, `cr`
//! prefixes) from the text while preserving every newline and the total
//! byte layout, so line attribution in the extractors stays exact.
//! Byte/C string prefixes (`b"`, `c"`) open plain strings and are masked
//! the same way. Character literals need their own state because a
//! literal like `'"'` contains a double quote: without it, that quote
//! would open a string that never closes on the same line and cascade
//! masking to the end of the body. The opening quote is recognized with
//! a bounded lookahead for its closing quote, which keeps lifetime
//! apostrophes (`'static`, `'a`) in code; plain strings and character
//! literals additionally close defensively at a raw newline so a
//! malformed line cannot cascade past its own line, while raw strings
//! legitimately span lines and keep masking across them.

/// The masking states. `RawStr` carries the opening hash count.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MaskState {
    Code,
    LineComment,
    BlockComment,
    Str,
    RawStr,
}

/// Position of the closing quote for the character literal opening at
/// `quote_index`, within a bounded lookahead, or `None` when no closing
/// quote appears nearby (the apostrophe is then a lifetime marker or an
/// unpaired character and stays code).
fn char_literal_close(bytes: &[u8], quote_index: usize) -> Option<usize> {
    let mut cursor = quote_index + 1;
    let limit = quote_index + 13;
    while cursor < bytes.len() && cursor <= limit {
        match bytes[cursor] {
            b'\'' => return Some(cursor),
            b'\\' => cursor += 2,
            b'\n' => return None,
            _ => cursor += 1,
        }
    }
    None
}

/// Hash count of the raw-string prefix ending just before the quote at
/// `quote_index` (`r"`, `r#"`, `cr##"` ...), or `None` when the quote
/// does not close a raw-string prefix. The prefix must begin at an
/// identifier boundary, so a trailing `r` inside a longer identifier
/// (`timer"..."`) never opens a raw string.
fn raw_hashes_before_quote(bytes: &[u8], quote_index: usize) -> Option<usize> {
    let mut back = quote_index;
    let mut hashes = 0usize;
    while back > 0 && bytes[back - 1] == b'#' {
        back -= 1;
        hashes += 1;
    }
    if back == 0 || bytes[back - 1] != b'r' {
        return None;
    }
    // The char literal `'r'` ends in the same shape as a raw prefix; a
    // quote before the `r` proves it is a character literal.
    if back >= 2 && bytes[back - 2] == b'\'' {
        return None;
    }
    if back >= 2 {
        let before = bytes[back - 2];
        // `br` and `cr` are raw prefixes; any other identifier character
        // means the `r` merely ends a longer identifier.
        if before.is_ascii_alphanumeric() && before != b'b' && before != b'c' {
            return None;
        }
    }
    Some(hashes)
}

/// Erase comment and string contents from `text`: every masked byte
/// becomes a space, newlines (wherever they occur) survive, and total
/// length is preserved, so downstream line-based extraction keeps exact
/// line attribution.
pub(crate) fn mask_comments_and_strings(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len());
    let mut state = MaskState::Code;
    let mut block_depth = 0usize;
    let mut raw_hashes = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        match state {
            MaskState::Code => {
                if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
                    state = MaskState::LineComment;
                    out.push(b' ');
                    out.push(b' ');
                    index += 2;
                    continue;
                }
                if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
                    state = MaskState::BlockComment;
                    block_depth = 1;
                    out.push(b' ');
                    out.push(b' ');
                    index += 2;
                    continue;
                }
                if byte == b'"' {
                    if let Some(hashes) = raw_hashes_before_quote(bytes, index) {
                        raw_hashes = hashes;
                        state = MaskState::RawStr;
                    } else {
                        state = MaskState::Str;
                    }
                    out.push(b' ');
                    index += 1;
                    continue;
                }
                if byte == b'\''
                    && let Some(close) = char_literal_close(bytes, index)
                {
                    // Mask the whole character literal, delimiters
                    // included, so its contents never become evidence.
                    while index <= close {
                        out.push(b' ');
                        index += 1;
                    }
                    continue;
                }
                out.push(byte);
                index += 1;
            }
            MaskState::LineComment => {
                if byte == b'\n' {
                    state = MaskState::Code;
                    out.push(b'\n');
                } else {
                    out.push(b' ');
                }
                index += 1;
            }
            MaskState::BlockComment => {
                if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
                    block_depth += 1;
                    out.push(b' ');
                    out.push(b' ');
                    index += 2;
                    continue;
                }
                if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    block_depth = block_depth.saturating_sub(1);
                    out.push(b' ');
                    out.push(b' ');
                    index += 2;
                    if block_depth == 0 {
                        state = MaskState::Code;
                    }
                    continue;
                }
                if byte == b'\n' {
                    out.push(b'\n');
                } else {
                    out.push(b' ');
                }
                index += 1;
            }
            MaskState::Str => {
                if byte == b'\\' {
                    out.push(b' ');
                    if let Some(&next) = bytes.get(index + 1) {
                        out.push(if next == b'\n' { b'\n' } else { b' ' });
                        index += 2;
                    } else {
                        index += 1;
                    }
                    continue;
                }
                if byte == b'"' {
                    state = MaskState::Code;
                    out.push(b' ');
                    index += 1;
                    continue;
                }
                if byte == b'\n' {
                    // Valid plain strings never contain a raw newline;
                    // closing here keeps a malformed line from cascading.
                    state = MaskState::Code;
                    out.push(b'\n');
                } else {
                    out.push(b' ');
                }
                index += 1;
            }
            MaskState::RawStr => {
                if byte == b'"' {
                    let mut closing = 0usize;
                    while let Some(&b'#') = bytes.get(index + 1 + closing) {
                        closing += 1;
                    }
                    if closing == raw_hashes {
                        // Consume the closing quote and its hashes
                        // (closing + 1 bytes).
                        out.extend(std::iter::repeat_n(b' ', closing + 1));
                        index += closing + 1;
                        state = MaskState::Code;
                        continue;
                    }
                }
                if byte == b'\n' {
                    out.push(b'\n');
                } else {
                    out.push(b' ');
                }
                index += 1;
            }
        }
    }
    // Masking only replaces whole characters with spaces (span edges are
    // character boundaries), so the output is always valid UTF-8; the
    // fallback keeps the input rather than panicking on a defect.
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_comments_and_strings_are_masked_with_lines_preserved() {
        let text = "fn check() {\n    /* other_call(9); */\n    let note = \"value 7\";\n    live_call(3); // trailing 5\n}";
        let masked = mask_comments_and_strings(text);
        assert!(!masked.contains("other_call"));
        assert!(!masked.contains("value 7"));
        assert!(!masked.contains("trailing"));
        assert!(masked.contains("live_call(3);"));
        assert_eq!(masked.lines().count(), text.lines().count());
    }

    #[test]
    fn char_literal_double_quote_does_not_cascade() {
        // `'"'` contains a double quote; without the char-literal state
        // the whole-body masker opened an unclosed string there and
        // swallowed every following line. Both calls stay live.
        let text = "fn check() {\n    match ch {\n        '\"' => first(),\n        _ => second(),\n    }\n}";
        let masked = mask_comments_and_strings(text);
        assert!(masked.contains("first(),"), "{masked}");
        assert!(masked.contains("second(),"), "{masked}");
    }

    #[test]
    fn char_literal_r_does_not_open_a_raw_string() {
        let text = "fn check(delim: char) {\n    let d = 'r';\n    live(d);\n}";
        let masked = mask_comments_and_strings(text);
        assert!(masked.contains("live(d);"), "{masked}");
    }

    #[test]
    fn raw_strings_are_masked_across_lines_and_close_exactly() {
        let text = "fn check() {\n    let raw = r#\"line1(7)\nline2(8)\"#;\n    after(9);\n}";
        let masked = mask_comments_and_strings(text);
        assert!(!masked.contains("line1"), "{masked}");
        assert!(!masked.contains("line2"), "{masked}");
        assert!(masked.contains("after(9);"), "{masked}");
    }

    #[test]
    fn lifetime_apostrophes_stay_code() {
        let text = "fn check<'a>(value: &'a str) {\n    keep(value);\n}";
        let masked = mask_comments_and_strings(text);
        assert!(masked.contains("keep(value);"), "{masked}");
    }
}
