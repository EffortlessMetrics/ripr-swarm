//! RIPR-SPEC-0243 rule 4: an error payload that passes on both versions of
//! the changed line is not an exact discriminator.
//!
//! The oracle extractor reads `toThrow("...")`, `toThrow(SomeError)`, chai
//! `to.throw("...")` and `node:assert` `throws` / `rejects` payloads as
//! `exact_error_variant` without seeing the change. This module reads the
//! changed line's old and new text and decides whether that payload can tell
//! the two versions apart; when it cannot, the classifier reads the
//! assertion as `broad_error` / weak.
//!
//! On a message-only change a payload credits when it matches exactly one
//! of the old and new messages. RIPR-SPEC-0243 words this as "excludes the
//! old message", which assumes the test passes on the new side; the corpus
//! (#6686) also scores tests that pin the old message and fail on the new
//! one, and those discriminate the change just as well.

use super::*;

/// One lexical token of a changed line, as far as rule 4 needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LineToken {
    /// A string literal or an interpolation-free template literal, by value.
    Literal(String),
    /// A template literal with interpolations: its literal text pieces and
    /// the raw interpolation sources.
    Template {
        pieces: Vec<String>,
        interpolations: Vec<String>,
    },
    /// Any other run of non-space characters, or one punctuation character.
    Other(String),
}

/// Split `text` into [`LineToken`]s. Returns `None` for an unterminated
/// literal, so a line ripr cannot read never counts as message-only.
fn tokenize(text: &str) -> Option<Vec<LineToken>> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        // A trailing `// comment` is not code; an apostrophe in it must not
        // open a literal.
        if ch == '/' && chars.get(i + 1) == Some(&'/') {
            break;
        }
        if ch == '"' || ch == '\'' {
            let mut value = String::new();
            i += 1;
            loop {
                let c = *chars.get(i)?;
                i += 1;
                if c == ch {
                    break;
                }
                if c == '\\' {
                    value.push(unescape(*chars.get(i)?)?);
                    i += 1;
                } else {
                    value.push(c);
                }
            }
            tokens.push(LineToken::Literal(value));
            continue;
        }
        if ch == '`' {
            let mut pieces = vec![String::new()];
            let mut interpolations = Vec::new();
            i += 1;
            loop {
                let c = *chars.get(i)?;
                i += 1;
                if c == '`' {
                    break;
                }
                if c == '\\' {
                    let escaped = unescape(*chars.get(i)?)?;
                    i += 1;
                    if let Some(piece) = pieces.last_mut() {
                        piece.push(escaped);
                    }
                } else if c == '$' && chars.get(i) == Some(&'{') {
                    i += 1;
                    let mut depth = 1usize;
                    let mut raw = String::new();
                    loop {
                        let c = *chars.get(i)?;
                        i += 1;
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        raw.push(c);
                    }
                    interpolations.push(raw.split_whitespace().collect::<Vec<_>>().join(" "));
                    pieces.push(String::new());
                } else if let Some(piece) = pieces.last_mut() {
                    piece.push(c);
                }
            }
            if interpolations.is_empty() {
                tokens.push(LineToken::Literal(pieces.concat()));
            } else {
                tokens.push(LineToken::Template {
                    pieces,
                    interpolations,
                });
            }
            continue;
        }
        if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '.' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric()
                    || chars[i] == '_'
                    || chars[i] == '$'
                    || chars[i] == '.')
            {
                i += 1;
            }
            tokens.push(LineToken::Other(chars[start..i].iter().collect()));
            continue;
        }
        tokens.push(LineToken::Other(ch.to_string()));
        i += 1;
    }
    Some(tokens)
}

/// The value of a one-character escape (`\n` is a newline). `None` for
/// `\x`, `\u` and a line continuation, which ripr does not decode, so the
/// literal stays unreadable rather than misread.
fn unescape(escaped: char) -> Option<char> {
    match escaped {
        'n' => Some('\n'),
        't' => Some('\t'),
        'r' => Some('\r'),
        'b' => Some('\u{8}'),
        'f' => Some('\u{c}'),
        'v' => Some('\u{b}'),
        '0' => Some('\0'),
        'x' | 'u' | '\n' | '\r' => None,
        other => Some(other),
    }
}

/// Whether a `throw ...` token run throws a primitive literal.
fn throws_primitive(throw: &[LineToken]) -> bool {
    matches!(throw, [LineToken::Other(keyword), operand, ..]
        if keyword == "throw"
            && (matches!(operand, LineToken::Literal(_) | LineToken::Template { .. })
                || matches!(operand, LineToken::Other(word)
                    if word.chars().next().is_some_and(|c| c.is_ascii_digit()))))
}

/// Whether a `throw ...` token run throws a built-in error class
/// (`new TypeError(...)` or `Error(...)`), which is an `Error`.
fn throws_builtin_error(throw: &[LineToken]) -> bool {
    let rest = match throw {
        [LineToken::Other(keyword), LineToken::Other(new), rest @ ..]
            if keyword == "throw" && new == "new" =>
        {
            rest
        }
        [LineToken::Other(keyword), rest @ ..] if keyword == "throw" => rest,
        _ => return false,
    };
    matches!(rest, [LineToken::Other(class), LineToken::Other(open), ..]
        if open == "(" && BUILTIN_ERROR_CLASSES.contains(&class.as_str()))
}

/// Drop a statement-ending `;`, which adds no behavior.
fn without_trailing_semicolon(tokens: &[LineToken]) -> &[LineToken] {
    match tokens.split_last() {
        Some((LineToken::Other(last), rest)) if last == ";" => rest,
        _ => tokens,
    }
}

/// The tokens before and from the `throw` keyword or the `Promise.reject`
/// callee, or `None` when the line has neither.
fn split_at_throw(tokens: &[LineToken]) -> Option<(&[LineToken], &[LineToken])> {
    let index = tokens.iter().position(|token| {
        matches!(token, LineToken::Other(word) if word == "throw" || word == "Promise.reject")
    })?;
    Some(tokens.split_at(index))
}

/// The text of a literal operand, `None` for anything else.
fn literal_value(token: &LineToken) -> Option<&str> {
    match token {
        LineToken::Literal(value) => Some(value),
        _ => None,
    }
}

/// The facts rule 4 reads from one changed line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ErrorChangeFacts {
    /// Every token that differs between the old and new line lies inside the
    /// message argument of the thrown or rejected error (or the thrown
    /// operand itself), and the argument changed: a literal's value, or a
    /// whole operand such as `+ max` (amended 2026-10-14). A respelling that
    /// reads the same message on both sides is not message-only.
    pub(crate) message_only: bool,
    /// The old thrown message with adjacent `+` literals joined; `None` when
    /// an operand is not a literal or the message cannot be located.
    pub(crate) old_message: Option<String>,
    /// The new thrown message, read the same way.
    pub(crate) new_message: Option<String>,
    /// Both sides throw under the same guard, one a primitive literal and
    /// the other a built-in error (`throw "bad"` to `throw new Error("bad")`,
    /// or back): a bare `Error` class check passes on one side only.
    pub(crate) primitive_throw_flips: bool,
    /// `String(err)` of the old thrown value when ripr knows it: the message
    /// of a thrown primitive, or `Name: message` for a built-in error class
    /// (`Error: blank`). A user class may set any `name`, so it stays `None`.
    pub(crate) old_string_form: Option<String>,
    /// The new side's `String(err)`, read the same way.
    pub(crate) new_string_form: Option<String>,
}

impl ErrorChangeFacts {
    /// Read the facts for a changed line. With no old line (a pure
    /// addition, or a preview that reads only the current bytes) the old
    /// side stays unread, so `message_only` and `separates` remain
    /// unestablished while the new side's thrown message is still known.
    pub(crate) fn read(old_line: Option<&str>, new_line: &str) -> Self {
        let old_tokens = old_line.and_then(tokenize);
        let old_split = old_tokens.as_deref().and_then(split_at_throw);
        let old_message = old_split.and_then(|(_, throw)| thrown_message(throw));
        let old_string_form = old_split.and_then(|(_, throw)| thrown_string_form(throw));
        let new_tokens = tokenize(new_line);
        let new_split = new_tokens.as_deref().and_then(split_at_throw);
        let new_message = new_split.and_then(|(_, throw)| thrown_message(throw));
        let new_string_form = new_split.and_then(|(_, throw)| thrown_string_form(throw));
        let message_only = match (old_split, new_split) {
            (Some((old_prefix, old_throw)), Some((new_prefix, new_throw))) => {
                let (old_throw, new_throw) = (
                    without_trailing_semicolon(old_throw),
                    without_trailing_semicolon(new_throw),
                );
                old_prefix == new_prefix
                    && (literals_alone_differ(old_throw, new_throw)
                        || (message_argument_alone_differs(old_throw, new_throw)
                            && !same_readable_message(
                                old_message.as_deref(),
                                new_message.as_deref(),
                            )))
            }
            _ => false,
        };
        let primitive_throw_flips = match (old_split, new_split) {
            (Some((old_prefix, old_throw)), Some((new_prefix, new_throw))) => {
                old_prefix == new_prefix
                    && ((throws_primitive(old_throw) && throws_builtin_error(new_throw))
                        || (throws_builtin_error(old_throw) && throws_primitive(new_throw)))
            }
            _ => false,
        };
        Self {
            message_only,
            old_message,
            new_message,
            primitive_throw_flips,
            old_string_form,
            new_string_form,
        }
    }

    /// On a message-only change, whether a payload that `matches` a message
    /// tells the two versions apart: it matches exactly one of the old and
    /// new messages. A payload matching both passes on both versions; one
    /// matching neither, or an unreadable message, is not established.
    fn separates(&self, matches: impl Fn(&str) -> bool) -> bool {
        match (self.old_message.as_deref(), self.new_message.as_deref()) {
            (Some(old), Some(new)) => matches(old) != matches(new),
            _ => false,
        }
    }

    /// [`Self::separates`] over `String(err)` instead of the message.
    fn separates_string_form(&self, matches: impl Fn(&str) -> bool) -> bool {
        match (
            self.old_string_form.as_deref(),
            self.new_string_form.as_deref(),
        ) {
            (Some(old), Some(new)) => matches(old) != matches(new),
            _ => false,
        }
    }
}

/// Whether a literal rejection equality's pinned reason still pins this
/// changed line's thrown message (#7315). The promoted
/// `expect(...).rejects.<equality>(<literal>)` assertion carries its pinned
/// reason in `expected_value_or_variant` while `error_payload` stays
/// `None`, so the rule-4 payload guard cannot read it. On a message-only
/// change the pinned reason must separate the two versions exactly as a
/// thrown-literal payload does; without a readable old side it must match
/// the changed line's own thrown message, because a reason this line never
/// produces observes a different throw site, not this change.
pub(crate) fn literal_rejection_credits(pinned: Option<&str>, facts: &ErrorChangeFacts) -> bool {
    let Some(value) = pinned.and_then(string_literal_value) else {
        return false;
    };
    if facts.message_only {
        return facts.separates(|message| message.contains(&value));
    }
    facts
        .new_message
        .as_deref()
        .is_some_and(|message| message.contains(&value))
}

/// Whether the two thrown expressions differ only in literal values, and in
/// at least one of them. A literal respelled with the same value (other
/// quotes, a template without interpolation) is not a difference.
fn literals_alone_differ(old: &[LineToken], new: &[LineToken]) -> bool {
    if old.len() != new.len() {
        return false;
    }
    let mut changed = false;
    for (before, after) in old.iter().zip(new) {
        match (before, after) {
            (LineToken::Other(a), LineToken::Other(b)) if a == b => {}
            (LineToken::Literal(a), LineToken::Literal(b)) => changed |= a != b,
            (
                LineToken::Template {
                    pieces: a,
                    interpolations: ia,
                },
                LineToken::Template {
                    pieces: b,
                    interpolations: ib,
                },
            ) if ia == ib => changed |= a != b,
            _ => return false,
        }
    }
    changed
}

/// The tokens before the message argument of a `throw ...` /
/// `Promise.reject(...)` run, the argument itself, and the tokens after it,
/// split where [`thrown_message`] reads the message.
fn split_message_argument(throw: &[LineToken]) -> Option<(usize, usize)> {
    let mut start = 1;
    if let Some(LineToken::Other(word)) = throw.get(start)
        && word == "("
    {
        start += 1;
        // `throw (log(e), e)`: a top-level comma inside the parentheses is
        // the sequence operator, and the thrown value is its last operand.
        let mut depth = 0usize;
        for token in &throw[start..] {
            match token {
                LineToken::Other(open) if open == "(" || open == "[" || open == "{" => depth += 1,
                LineToken::Other(close) if close == ")" || close == "]" || close == "}" => {
                    let Some(next) = depth.checked_sub(1) else {
                        break;
                    };
                    depth = next;
                }
                LineToken::Other(comma) if comma == "," && depth == 0 => return None,
                _ => {}
            }
        }
    }
    let mut called = false;
    let constructed = matches!(throw.get(start), Some(LineToken::Other(word)) if word == "new");
    if constructed {
        start += 1;
    }
    if let (Some(LineToken::Other(callee)), Some(LineToken::Other(open))) =
        (throw.get(start), throw.get(start + 1))
        && open == "("
        && callee
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
    {
        if callee.contains('.') {
            return None;
        }
        start += 2;
        called = true;
    } else if constructed
        || matches!(
            (throw.get(start), throw.get(start + 1)),
            (Some(LineToken::Other(callee)), Some(LineToken::Other(open)))
                if (open == "(" || open == "<")
                    && callee
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        )
    {
        // `new errors.ParseError(...)` or `errors.make(...)` (the tokenizer
        // keeps the dotted path as one word), or `new X<T>(...)`: the class
        // or factory is not one name, so the split is refused.
        return None;
    }
    let mut depth = 0usize;
    let mut end = start;
    while let Some(token) = throw.get(end) {
        match token {
            LineToken::Other(open) if open == "(" || open == "[" || open == "{" => depth += 1,
            LineToken::Other(close) if close == ")" || close == "]" || close == "}" => {
                let Some(next) = depth.checked_sub(1) else {
                    break;
                };
                depth = next;
            }
            // Outside a call, a top-level comma is the sequence operator
            // (`throw (log(e), e)`): the thrown value is its last operand.
            LineToken::Other(comma) if comma == "," && depth == 0 => {
                if !called {
                    return None;
                }
                break;
            }
            _ => {}
        }
        end += 1;
    }
    (depth == 0 && end > start).then_some((start, end))
}

/// Whether the two thrown expressions differ only inside their message
/// argument (`new RangeError("limit " + max)` against
/// `new RangeError("limit ")`). The message is then unreadable on the side
/// with a non-literal operand, so a payload cannot be shown to separate the
/// versions.
fn message_argument_alone_differs(old: &[LineToken], new: &[LineToken]) -> bool {
    let (Some((old_start, old_end)), Some((new_start, new_end))) =
        (split_message_argument(old), split_message_argument(new))
    else {
        return false;
    };
    old.get(..old_start) == new.get(..new_start)
        && old.get(old_end..) == new.get(new_end..)
        && old.get(old_start..old_end) != new.get(new_start..new_end)
}

/// Whether both sides read the same message (`"unsupported currency"`
/// respelled as `"unsupported " + "currency"`): a rewrite, not a message
/// change, so the ladder's reading stands.
fn same_readable_message(old: Option<&str>, new: Option<&str>) -> bool {
    matches!((old, new), (Some(old), Some(new)) if old == new)
}

/// The thrown message of a `throw ...` / `Promise.reject(...)` token run: the
/// first argument of `new X(...)` / `X(...)`, or the thrown operand itself,
/// with adjacent `+` literals joined. `None` when any operand is not a
/// literal.
fn thrown_message(throw: &[LineToken]) -> Option<String> {
    let mut rest = throw.get(1..)?;
    if let Some(LineToken::Other(word)) = rest.first()
        && word == "("
    {
        rest = rest.get(1..)?;
    }
    if let [LineToken::Other(word), ..] = rest
        && word == "new"
    {
        rest = rest.get(1..)?;
    }
    // `Ident(` opens a constructor or factory call: read its first argument.
    if let [LineToken::Other(callee), LineToken::Other(open), ..] = rest
        && open == "("
        && callee
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
    {
        rest = rest.get(2..)?;
    }
    let mut message = String::new();
    let mut expect_operand = true;
    for token in rest {
        if expect_operand {
            message.push_str(literal_value(token)?);
            expect_operand = false;
            continue;
        }
        match token {
            LineToken::Other(op) if op == "+" => expect_operand = true,
            LineToken::Other(end) if end == ")" || end == ";" || end == "," => break,
            _ => return None,
        }
    }
    (!expect_operand).then_some(message)
}

/// Built-in error classes whose `name` is their class name.
const BUILTIN_ERROR_CLASSES: &[&str] = &[
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
];

/// `String(err)` of a `throw ...` / `Promise.reject(...)` token run when
/// ripr knows it: a thrown primitive literal reads as its value, and
/// `new Error("m")` / `TypeError("m")` reads as `Error: m` / `TypeError: m`.
/// A user error class may set any `name`, and an empty message renders
/// without `: `, so both stay `None`.
fn thrown_string_form(throw: &[LineToken]) -> Option<String> {
    let message = thrown_message(throw)?;
    let mut rest = throw.get(1..)?;
    if let Some(LineToken::Other(word)) = rest.first()
        && word == "("
    {
        rest = rest.get(1..)?;
    }
    if let [LineToken::Other(word), ..] = rest
        && word == "new"
    {
        rest = rest.get(1..)?;
    }
    match rest {
        [LineToken::Literal(_), ..] => Some(message),
        [LineToken::Other(class), LineToken::Other(open), ..]
            if open == "(" && BUILTIN_ERROR_CLASSES.contains(&class.as_str()) =>
        {
            (!message.is_empty()).then(|| format!("{class}: {message}"))
        }
        _ => None,
    }
}

/// Whether an `exact_error_variant` assertion still pins the change under
/// rule 4 (and rule 10's message guard). `false` means it reads
/// `broad_error` / weak for this change.
pub(crate) fn error_payload_credits(
    payload: &TypeScriptErrorPayload,
    facts: &ErrorChangeFacts,
) -> bool {
    use TypeScriptErrorPayloadKind as Kind;
    match payload.kind {
        Kind::ThrowsClass | Kind::RejectsThrowClass => {
            // Every thrown error is an `Error`, so this payload is a bare
            // `toThrow()` (RIPR-SPEC-0243 decision 6); only a primitive throw
            // on exactly one side, under the same guard, fails it there.
            if payload.expected.trim() == "Error" {
                return facts.primitive_throw_flips;
            }
            // The same class is thrown on both versions of a message change.
            !facts.message_only
        }
        Kind::ThrowsLiteral | Kind::RejectsThrowLiteral | Kind::ChaiThrowLiteral => {
            if !facts.message_only {
                return true;
            }
            // Jest, Vitest and chai match a string payload as a substring.
            let Some(expected) = string_literal_value(&payload.expected) else {
                return false;
            };
            facts.separates(|message| message.contains(&expected))
        }
        Kind::ThrowsObject | Kind::RejectsThrowObject | Kind::RejectsMatchObject => true,
        Kind::AssertThrowsObject | Kind::AssertRejectsObject => {
            if !facts.message_only {
                return true;
            }
            let Some(message) = object_message_value(&payload.expected) else {
                return false;
            };
            facts.separates(|candidate| candidate == message)
        }
        Kind::AssertThrowsRegex | Kind::AssertRejectsRegex => {
            if !facts.message_only {
                return true;
            }
            let Some(literal) = plain_anchored_regex_text(&payload.expected) else {
                return false;
            };
            // `node:assert` tests the regex against `String(err)`, so
            // `/^blank$/` never matches `Error: blank`.
            facts.separates_string_form(|candidate| candidate == literal)
        }
    }
}

/// The value of a single string-literal source text.
fn string_literal_value(text: &str) -> Option<String> {
    match tokenize(text)?.as_slice() {
        [LineToken::Literal(value)] => Some(value.clone()),
        _ => None,
    }
}

/// The string value of the `message` property of an all-literal object.
fn object_message_value(text: &str) -> Option<String> {
    let tokens = tokenize(text)?;
    // Only a top-level key counts: `{ cause: { message: "x" } }` does not.
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token {
            LineToken::Other(open) if open == "{" || open == "[" || open == "(" => depth += 1,
            LineToken::Other(close) if close == "}" || close == "]" || close == ")" => {
                depth = depth.checked_sub(1)?;
            }
            // A key may be bare or quoted (`{ "message": "..." }`).
            LineToken::Other(key) | LineToken::Literal(key)
                if depth == 1
                    && key == "message"
                    && matches!(tokens.get(index + 1), Some(LineToken::Other(colon)) if colon == ":") =>
            {
                return literal_value(tokens.get(index + 2)?).map(str::to_string);
            }
            _ => {}
        }
    }
    None
}

/// The literal text an anchored regex matches when its pattern between `^`
/// and `$` holds no metacharacters other than escapes.
fn plain_anchored_regex_text(text: &str) -> Option<String> {
    let (pattern, flags) = split_regex_literal(text)?;
    // `i` and `m` widen what a literal pattern matches; ripr compares it
    // as plain text only without them.
    if !flags.chars().all(|flag| flag == 'u') {
        return None;
    }
    let inner = pattern.strip_prefix('^')?.strip_suffix('$')?;
    let mut literal = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let escaped = chars.next()?;
                // `\d`, `\w`, `\s` and friends are classes, not literals.
                if escaped.is_ascii_alphanumeric() {
                    return None;
                }
                literal.push(escaped);
            }
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' => {
                return None;
            }
            _ => literal.push(ch),
        }
    }
    Some(literal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(kind: TypeScriptErrorPayloadKind, expected: &str) -> TypeScriptErrorPayload {
        TypeScriptErrorPayload {
            expected: expected.to_string(),
            kind,
        }
    }

    #[test]
    fn message_only_reads_literal_values_not_spelling() {
        let changed = ErrorChangeFacts::read(
            Some(r#"    throw new Error("empty");"#),
            r#"    throw new Error("blank");"#,
        );
        assert!(changed.message_only);
        assert_eq!(changed.old_message.as_deref(), Some("empty"));

        // Same value, other quotes: no message changed.
        let respelled = ErrorChangeFacts::read(
            Some(r#"    throw new Error("memo required");"#),
            "    throw new Error('memo required');",
        );
        assert!(!respelled.message_only);

        // A `+` split that reads the same message on both sides is a
        // rewrite, not a message change (rule 4 text).
        let split = ErrorChangeFacts::read(
            Some(r#"    throw new Error("unsupported currency");"#),
            r#"    throw new Error("unsupported " + "currency");"#,
        );
        assert!(!split.message_only);
        assert_eq!(split.new_message.as_deref(), Some("unsupported currency"));
        assert_eq!(split.old_message.as_deref(), Some("unsupported currency"));

        // A changed condition on the same line is not message-only.
        let condition = ErrorChangeFacts::read(
            Some(r#"if (s === "") throw new Error("empty");"#),
            r#"if (s !== "") throw new Error("empty");"#,
        );
        assert!(!condition.message_only);

        // A dropped `;` or a trailing comment adds no behavior.
        let semicolon = ErrorChangeFacts::read(
            Some(r#"    throw new Error("not blank")"#),
            r#"    throw new Error("blank");"#,
        );
        assert!(semicolon.message_only);
        let comment = ErrorChangeFacts::read(
            Some(r#"    throw new Error("not blank");"#),
            r#"    throw new Error("blank"); // don't trim"#,
        );
        assert!(comment.message_only);
        assert_eq!(comment.new_message.as_deref(), Some("blank"));

        // Escapes decode to their characters; `\u` stays unreadable.
        let escaped = ErrorChangeFacts::read(
            Some(r#"throw new Error("line\none");"#),
            r#"throw new Error("line one");"#,
        );
        assert_eq!(escaped.old_message.as_deref(), Some("line\none"));
        let unicode = ErrorChangeFacts::read(
            Some(r#"throw new Error("\u00e9");"#),
            r#"throw new Error("e");"#,
        );
        assert!(!unicode.message_only);
        assert_eq!(unicode.old_message, None);

        // A changed interpolation, or a dropped operand, changes only the
        // message argument; the side with a non-literal operand stays
        // unread.
        let interpolation = ErrorChangeFacts::read(
            Some("throw new Error(`bad ${a}`);"),
            "throw new Error(`bad ${b}`);",
        );
        assert!(interpolation.message_only);
        assert_eq!(interpolation.old_message, None);
        let dropped = ErrorChangeFacts::read(
            Some(r#"  throw new RangeError("limit " + max);"#),
            r#"  throw new RangeError("limit ");"#,
        );
        assert!(dropped.message_only);
        assert_eq!(dropped.old_message, None);
        assert_eq!(dropped.new_message.as_deref(), Some("limit "));
        let primitive =
            ErrorChangeFacts::read(Some(r#"throw "limit " + max;"#), r#"throw "limit ";"#);
        assert!(primitive.message_only);
        // A member-path class is not one name: the split is refused even
        // when the class is unchanged.
        let member_class = ErrorChangeFacts::read(
            Some(r#"throw new errors.ParseError("bad " + x);"#),
            r#"throw new errors.ParseError("bad ");"#,
        );
        assert!(!member_class.message_only);
        let member_changed = ErrorChangeFacts::read(
            Some(r#"throw new errors.A("bad");"#),
            r#"throw new errors.B("bad");"#,
        );
        assert!(!member_changed.message_only);
        // A class change outside the argument, a later argument that also
        // changes, an empty argument list and an unclosed line are not.
        for (old, new) in [
            (r#"throw new Error("a");"#, r#"throw new TypeError("b");"#),
            (
                r#"throw new Error("a", { cause: x });"#,
                r#"throw new Error("b", { cause: y });"#,
            ),
            (r#"throw new Error();"#, r#"throw new Error("x");"#),
            (r#"throw f([a"#, r#"throw f([b"#),
        ] {
            assert!(
                !ErrorChangeFacts::read(Some(old), new).message_only,
                "{old} -> {new}"
            );
        }
        // A renamed single-name factory, and a sequence operand, are not.
        for (old, new) in [
            (r#"throw makeErr("a");"#, r#"throw makeOther("b");"#),
            (r#"throw (log(a), e);"#, r#"throw (log(b), e);"#),
            (r#"throw x, e;"#, r#"throw y, e;"#),
        ] {
            assert!(
                !ErrorChangeFacts::read(Some(old), new).message_only,
                "{old} -> {new}"
            );
        }
        // So is a dotted or generic factory without `new`.
        let factory = ErrorChangeFacts::read(
            Some(r#"throw errors.make("bad " + x);"#),
            r#"throw errors.other("bad ");"#,
        );
        assert!(!factory.message_only);
        let generic = ErrorChangeFacts::read(
            Some(r#"throw make<A>("bad " + x);"#),
            r#"throw make<B>("bad ");"#,
        );
        assert!(!generic.message_only);
        let member_fn_changed = ErrorChangeFacts::read(
            Some(r#"throw errors.A("bad");"#),
            r#"throw errors.B("bad");"#,
        );
        assert!(!member_fn_changed.message_only);
        let member_fn = ErrorChangeFacts::read(
            Some(r#"throw errors.ParseError("bad " + x);"#),
            r#"throw errors.ParseError("bad ");"#,
        );
        assert!(!member_fn.message_only);
        // A change in another argument, or after the call, is not.
        let cause = ErrorChangeFacts::read(
            Some(r#"throw new Error("bad", { cause: a });"#),
            r#"throw new Error("bad", { cause: b });"#,
        );
        assert!(!cause.message_only);
        let member = ErrorChangeFacts::read(
            Some(r#"return Promise.reject(new Error("bad")).a;"#),
            r#"return Promise.reject(new Error("bad")).b;"#,
        );
        assert!(!member.message_only);
    }

    #[test]
    fn a_dropped_message_operand_leaves_no_payload_exact() {
        // RIPR-SPEC-0248 example 10: `toThrow("limit ")` matches both
        // `limit 5` and `limit `, and the old message is not a literal.
        let facts = ErrorChangeFacts::read(
            Some(r#"  throw new RangeError("limit " + max);"#),
            r#"  throw new RangeError("limit ");"#,
        );
        for (kind, expected) in [
            (TypeScriptErrorPayloadKind::ThrowsLiteral, r#""limit ""#),
            (TypeScriptErrorPayloadKind::ThrowsLiteral, r#""limit 5""#),
            (TypeScriptErrorPayloadKind::ThrowsClass, "RangeError"),
        ] {
            assert!(
                !error_payload_credits(&payload(kind, expected), &facts),
                "{expected}"
            );
        }
        // A pinned rejection reason that only the old, readable message
        // holds separates the versions and keeps its credit.
        let shortened = ErrorChangeFacts::read(
            Some(r#"return Promise.reject(new Error("bad " + "input"));"#),
            r#"return Promise.reject(new Error("bad "));"#,
        );
        assert!(shortened.message_only);
        assert!(literal_rejection_credits(Some(r#""input""#), &shortened));
        assert!(!literal_rejection_credits(Some(r#""bad""#), &shortened));
    }

    #[test]
    fn old_message_joins_literal_chains_and_refuses_non_literals() {
        let joined = ErrorChangeFacts::read(
            Some(r#"throw new Error("ba" + "d");"#),
            r#"throw new Error("good");"#,
        );
        assert_eq!(joined.old_message.as_deref(), Some("bad"));
        let dynamic = ErrorChangeFacts::read(
            Some(r#"throw new Error("bad " + name);"#),
            r#"throw new Error("good " + name);"#,
        );
        assert!(dynamic.message_only);
        assert_eq!(dynamic.old_message, None);
        let reject = ErrorChangeFacts::read(
            Some(r#"return Promise.reject(new Error("no"));"#),
            r#"return Promise.reject(new Error("nope"));"#,
        );
        assert!(reject.message_only);
        assert_eq!(reject.old_message.as_deref(), Some("no"));
    }

    #[test]
    fn rule_4_string_payload_must_match_exactly_one_message() {
        use TypeScriptErrorPayloadKind as Kind;
        let facts = ErrorChangeFacts::read(
            Some(r#"throw new Error("empty");"#),
            r#"throw new Error("blank");"#,
        );
        assert!(error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""blank""#),
            &facts
        ));
        // A test pinning the old message fails on the new one: it separates.
        assert!(error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""empty""#),
            &facts
        ));
        // An empty string passes on both versions.
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""""#),
            &facts
        ));
        assert!(error_payload_credits(
            &payload(Kind::ChaiThrowLiteral, r#""blank""#),
            &facts
        ));
        let not_blank = ErrorChangeFacts::read(
            Some(r#"throw new Error("not blank");"#),
            r#"throw new Error("blank");"#,
        );
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""blank""#),
            &not_blank
        ));
        assert!(!error_payload_credits(
            &payload(Kind::ChaiThrowLiteral, r#""blank""#),
            &not_blank
        ));
    }

    #[test]
    fn rule_4_class_payloads_on_message_and_other_changes() {
        use TypeScriptErrorPayloadKind as Kind;
        let message = ErrorChangeFacts::read(
            Some(r#"throw new TypeError("empty");"#),
            r#"throw new TypeError("blank");"#,
        );
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsClass, "TypeError"),
            &message
        ));
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsClass, "Error"),
            &message
        ));
        let class_swap = ErrorChangeFacts::read(
            Some(r#"throw new Error("empty");"#),
            r#"throw new TypeError("empty");"#,
        );
        assert!(error_payload_credits(
            &payload(Kind::ThrowsClass, "TypeError"),
            &class_swap
        ));
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsClass, "Error"),
            &class_swap
        ));
        let primitive =
            ErrorChangeFacts::read(Some(r#"throw "empty";"#), r#"throw new Error("empty");"#);
        assert!(error_payload_credits(
            &payload(Kind::ThrowsClass, "Error"),
            &primitive
        ));
        // The other direction: an `Error` becomes a primitive.
        let to_primitive =
            ErrorChangeFacts::read(Some(r#"throw new Error("empty");"#), r#"throw "empty";"#);
        assert!(error_payload_credits(
            &payload(Kind::ThrowsClass, "Error"),
            &to_primitive
        ));
        // A side ripr cannot type (a variable, an object) is not an `Error`.
        let variable = ErrorChangeFacts::read(Some("throw reason;"), r#"throw "bad";"#);
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsClass, "Error"),
            &variable
        ));
        let object = ErrorChangeFacts::read(Some(r#"throw { message: "x" };"#), r#"throw "x";"#);
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsClass, "Error"),
            &object
        ));
        // Primitives on both sides fail `toThrow(Error)` on both sides.
        let both_primitive = ErrorChangeFacts::read(Some(r#"throw "a";"#), r#"throw "b";"#);
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsClass, "Error"),
            &both_primitive
        ));
        // A changed guard decides which inputs throw: not established.
        let guard = ErrorChangeFacts::read(
            Some(r#"if (s === "") throw "empty";"#),
            r#"if (s !== "") throw new Error("empty");"#,
        );
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsClass, "Error"),
            &guard
        ));
        // Object payloads are unaffected for Jest.
        assert!(error_payload_credits(
            &payload(Kind::ThrowsObject, r#"{ message: "empty" }"#),
            &message
        ));
    }

    #[test]
    fn rule_10_node_assert_payloads_under_the_message_guard() {
        use TypeScriptErrorPayloadKind as Kind;
        let facts = ErrorChangeFacts::read(
            Some(r#"throw new Error("empty");"#),
            r#"throw new Error("blank");"#,
        );
        assert!(error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^Error: blank$/"),
            &facts
        ));
        // Pinning the old message also tells the versions apart.
        assert!(error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^Error: empty$/"),
            &facts
        ));
        // Matching neither message establishes nothing.
        assert!(!error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^Error: other$/"),
            &facts
        ));
        // Metacharacters between the anchors: ripr cannot compare it.
        assert!(!error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^Error: bl.nk$/"),
            &facts
        ));
        assert!(error_payload_credits(
            &payload(Kind::AssertRejectsObject, r#"{ message: "blank" }"#),
            &facts
        ));
        assert!(!error_payload_credits(
            &payload(Kind::AssertRejectsObject, r#"{ message: "other" }"#),
            &facts
        ));
        // A quoted `message` key reads the same as a bare one.
        assert!(error_payload_credits(
            &payload(Kind::AssertRejectsObject, r#"{ "message": "blank" }"#),
            &facts
        ));
        // `node:assert` tests the regex against `String(err)`: a regex
        // without the `Error: ` prefix matches neither version.
        assert!(!error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^blank$/"),
            &facts
        ));
        assert!(!error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^TypeError: blank$/"),
            &facts
        ));
        // `i` widens the match: `/^Error: blank$/i` also matches `BLANK`.
        let case = ErrorChangeFacts::read(
            Some(r#"throw new Error("BLANK");"#),
            r#"throw new Error("blank");"#,
        );
        assert!(!error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^Error: blank$/i"),
            &case
        ));
        // A user class may set any `name`: its `String(err)` is unknown.
        let user_class = ErrorChangeFacts::read(
            Some(r#"throw new ParseError("empty");"#),
            r#"throw new ParseError("blank");"#,
        );
        assert!(!error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^ParseError: blank$/"),
            &user_class
        ));
        // A thrown primitive renders as itself.
        let primitive = ErrorChangeFacts::read(Some(r#"throw "empty";"#), r#"throw "blank";"#);
        assert!(error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^blank$/"),
            &primitive
        ));
        // Only a top-level `message` key is the error's message.
        assert!(!error_payload_credits(
            &payload(
                Kind::AssertRejectsObject,
                r#"{ cause: { message: "blank" }, message: "other" }"#
            ),
            &facts
        ));
        // Not a message-only change: anchored regexes keep their credit.
        let class = ErrorChangeFacts::read(
            Some(r#"throw new Error("unsupported currency");"#),
            r#"throw new TypeError("unsupported currency");"#,
        );
        assert!(error_payload_credits(
            &payload(
                Kind::AssertThrowsRegex,
                "/^TypeError: unsupported currency$/"
            ),
            &class
        ));
        // A `+` split that keeps the message is a rewrite: anchored regexes
        // keep their credit.
        let split = ErrorChangeFacts::read(
            Some(r#"throw new Error("unsupported currency");"#),
            r#"throw new Error("unsupported " + "currency");"#,
        );
        assert!(error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^Error: unsupported currency$/"),
            &split
        ));
        // A readable dropped operand that changes the message is
        // message-only, and an exact payload for one side separates them.
        let shortened = ErrorChangeFacts::read(
            Some(r#"throw new Error("unsupported " + "currency");"#),
            r#"throw new Error("unsupported ");"#,
        );
        assert!(shortened.message_only);
        assert!(error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^Error: unsupported $/"),
            &shortened
        ));
    }
}
