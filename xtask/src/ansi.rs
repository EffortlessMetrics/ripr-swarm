//! Shared terminal text normalization for command and lane diagnostics.

/// Remove ANSI SGR escape sequences from one line.
///
/// CI sets `CARGO_TERM_COLOR: always`, so cargo's own progress lines arrive as
/// `\x1b[1m\x1b[92m     Running\x1b[0m unittests src\lib.rs (...)`. Matching a
/// prefix like `Running ` against that fails, which is how the first real lane
/// run reported no targets while parsing every failure correctly.
pub(crate) fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        // Peek rather than consume: an ESC that does not introduce a CSI
        // sequence must not swallow the character after it. Consuming
        // unconditionally silently deleted real text — an ESC followed by `X`
        // lost both.
        if chars.peek() != Some(&'[') {
            continue;
        }
        let _ = chars.next();
        for next in chars.by_ref() {
            if next.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}
