//! Line-local TypeScript call lexer helpers behind `contains_call_name`
//! (#13592): block-comment detection and string/template state. Kept apart
//! from `related_tests` so edits here stay a small diff (#13868).

use super::*;

/// `true` when `idx` falls inside an unclosed `/* ... */` comment.
///
/// A `/*` written in a whole-line `//` comment or a plain string opens
/// nothing (`// a trailing /* after a segment`, `const glob = "/*";`), so it
/// must not hide the code after it (#13592 follow-up). The scan has no regex,
/// JSX or multi-line template awareness, so it fails closed: a `/*` still
/// opens a comment unless it follows a `//` that starts its line (after
/// indentation) or sits in a [`plain_string_end`] string. A regex
/// (`/^https?:\/\//`, `/[ //]/`), a JSX apostrophe or a template's closing
/// backtick can make any finer reading wrong.
pub(crate) fn inside_block_comment(body_text: &str, idx: usize) -> bool {
    let Some(prefix) = body_text.get(..idx) else {
        return false;
    };
    let bytes = prefix.as_bytes();
    let mut in_block = false;
    let mut line_blank = true;
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let byte = bytes[i];
        let next = bytes.get(i + 1).copied();
        if in_block {
            if byte == b'*' && next == Some(b'/') {
                in_block = false;
                line_blank = false;
                i += 2;
                continue;
            }
        } else if byte == b'\n' {
            line_blank = true;
            line_start = i + 1;
        } else if byte == b'/' && next == Some(b'/') && line_blank {
            // Whole-line comment: skip to the end of the line.
            match bytes[i..].iter().position(|&b| b == b'\n') {
                Some(offset) => {
                    i += offset;
                    continue;
                }
                None => return false,
            }
        } else if byte == b'\\' {
            // Outside strings a backslash belongs to a regex (`/^\/*/`) or an
            // identifier escape; the byte it escapes never opens a comment.
            line_blank = false;
            i += 2;
            continue;
        } else if byte == b'/' && next == Some(b'*') {
            in_block = true;
            i += 2;
            continue;
        } else {
            if let Some(end) = plain_string_end(bytes, line_start, i) {
                i = end;
            }
            if !byte.is_ascii_whitespace() {
                line_blank = false;
            }
        }
        i += 1;
    }
    in_block
}

/// The index of the closing quote when the quote at `at` opens a string this
/// line lexer can trust: nothing before it on the line is a quote, backtick or
/// `/` (no earlier string, regex or comment to misread), it follows `=`, `(`,
/// `,`, `[` or `:` (optionally spaced), its text holds no backslash, and its
/// first matching quote is followed (optionally spaced) by `;`, `,`, `)`, `]`
/// or the end of the line. A JSX apostrophe, a regex quote or a template's
/// closing backtick fails one of these.
fn plain_string_end(bytes: &[u8], line_start: usize, at: usize) -> Option<usize> {
    let open = *bytes.get(at)?;
    if !matches!(open, b'\'' | b'"' | b'`') {
        return None;
    }
    let before = bytes.get(line_start..at)?;
    if before
        .iter()
        .any(|b| matches!(b, b'\'' | b'"' | b'`' | b'/'))
    {
        return None;
    }
    let delimiter = before.iter().rev().find(|b| !b.is_ascii_whitespace())?;
    if !matches!(delimiter, b'=' | b'(' | b',' | b'[' | b':') {
        return None;
    }
    let mut end = at + 1;
    loop {
        match *bytes.get(end)? {
            b'\\' | b'\n' => return None,
            b'$' if open == b'`' => return None,
            byte if byte == open => break,
            _ => end += 1,
        }
    }
    let follower = bytes
        .get(end + 1..)?
        .iter()
        .find(|b| !matches!(b, b' ' | b'\t'));
    matches!(follower, None | Some(b';' | b',' | b')' | b']' | b'\n')).then_some(end)
}

/// `true` when `prefix` ends inside a string or template literal's text.
///
/// A template literal's `${...}` substitution is code, not text: a call
/// written there runs (`` `${render(value)}` ``), so the scan tracks nested
/// substitutions and their braces instead of treating everything after a
/// backtick as text (#13592 follow-up). Line-local, like its callers.
pub(super) fn has_unclosed_quote_or_template(prefix: &str) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        /// Code; the count is the open `{` depth inside a template
        /// substitution (unused at the outermost level).
        Code(usize),
        Single,
        Double,
        Template,
    }
    let mut stack: Vec<Mode> = vec![Mode::Code(0)];
    let mut escaped = false;
    // The last non-space character read as code, to tell a division from a
    // regex opener inside a substitution.
    let mut previous_code = ' ';
    let mut chars = prefix.chars().peekable();
    while let Some(ch) = chars.next() {
        let nested = stack.len() > 1;
        let Some(mode) = stack.last_mut() else {
            return false;
        };
        let after = previous_code;
        if matches!(*mode, Mode::Code(_)) && !ch.is_whitespace() {
            previous_code = ch;
        }
        match *mode {
            Mode::Single | Mode::Double | Mode::Template if escaped => escaped = false,
            Mode::Single | Mode::Double | Mode::Template if ch == '\\' => escaped = true,
            Mode::Single if ch == '\'' => {
                stack.pop();
            }
            Mode::Double if ch == '"' => {
                stack.pop();
            }
            Mode::Template if ch == '`' => {
                stack.pop();
            }
            Mode::Template if ch == '$' && chars.peek() == Some(&'{') => {
                chars.next();
                stack.push(Mode::Code(0));
            }
            Mode::Single | Mode::Double | Mode::Template => {}
            Mode::Code(depth) => match ch {
                '\'' => stack.push(Mode::Single),
                '"' => stack.push(Mode::Double),
                '`' => stack.push(Mode::Template),
                // Outside strings a backslash belongs to a regex (`/\/\//`)
                // or an identifier escape; the character it escapes neither
                // opens a string nor starts a comment.
                '\\' => {
                    chars.next();
                }
                // A `//` in code starts a trailing comment: the rest of the
                // line is text (`x = 1; // see render(x)`). A regex holding
                // `//` reads the same way, which only hides.
                '/' if chars.peek() == Some(&'/') => return true,
                // A `/` inside a substitution that is not a division may
                // open a regex whose braces or quotes this scan cannot
                // follow; fail closed and read the rest as text.
                '/' if nested
                    && !(is_javascript_identifier_char(after) || matches!(after, ')' | ']')) =>
                {
                    return true;
                }
                '{' => *mode = Mode::Code(depth + 1),
                '}' if depth > 0 => *mode = Mode::Code(depth - 1),
                // Closes a template substitution and resumes the template
                // text; a stray `}` at the outermost level stays code.
                '}' if stack.len() > 1 => {
                    stack.pop();
                }
                _ => {}
            },
        }
    }
    !matches!(stack.last(), Some(Mode::Code(_)))
}
