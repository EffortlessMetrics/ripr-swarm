//! RIPR-SPEC-0234 rule 4: an error payload that passes on both versions of
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
//! of the old and new messages. RIPR-SPEC-0234 words this as "excludes the
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
                    value.push(*chars.get(i)?);
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
                    let escaped = *chars.get(i)?;
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
    /// Every token that differs between the old and new line lies inside a
    /// string literal (or the literal text of a template) of the thrown or
    /// rejected expression, and at least one literal's value changed.
    pub(crate) message_only: bool,
    /// The old thrown message with adjacent `+` literals joined; `None` when
    /// an operand is not a literal or the message cannot be located.
    pub(crate) old_message: Option<String>,
    /// The new thrown message, read the same way.
    pub(crate) new_message: Option<String>,
    /// The old side throws a primitive literal (`throw "bad"`).
    pub(crate) old_throws_primitive: bool,
}

impl ErrorChangeFacts {
    /// Read the facts for a changed line. With no old line (a pure
    /// addition) nothing is message-only and no old message exists.
    pub(crate) fn read(old_line: Option<&str>, new_line: &str) -> Self {
        let Some(old_tokens) = old_line.and_then(tokenize) else {
            return Self::default();
        };
        let old_split = split_at_throw(&old_tokens);
        let old_throws_primitive = old_split.is_some_and(|(_, throw)| {
            matches!(throw, [LineToken::Other(keyword), operand, ..]
                if keyword == "throw"
                    && (matches!(operand, LineToken::Literal(_) | LineToken::Template { .. })
                        || matches!(operand, LineToken::Other(word)
                            if word.chars().next().is_some_and(|c| c.is_ascii_digit()))))
        });
        let old_message = old_split.and_then(|(_, throw)| thrown_message(throw));
        let new_tokens = tokenize(new_line);
        let new_split = new_tokens.as_deref().and_then(split_at_throw);
        let new_message = new_split.and_then(|(_, throw)| thrown_message(throw));
        let message_only = match (old_split, new_split) {
            (Some((old_prefix, old_throw)), Some((new_prefix, new_throw))) => {
                old_prefix == new_prefix && literals_alone_differ(old_throw, new_throw)
            }
            _ => false,
        };
        Self {
            message_only,
            old_message,
            new_message,
            old_throws_primitive,
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
            // Every thrown error is an `Error`; only a primitive old throw
            // fails that class check.
            if payload.expected.trim() == "Error" {
                return facts.old_throws_primitive;
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
            let literal = strip_error_name_prefix(&literal);
            facts.separates(|candidate| candidate == literal)
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
    let position = tokens.windows(2).position(|pair| {
        matches!(&pair[0], LineToken::Other(key) if key == "message")
            && matches!(&pair[1], LineToken::Other(colon) if colon == ":")
    })?;
    literal_value(tokens.get(position + 2)?).map(str::to_string)
}

/// The literal text an anchored regex matches when its pattern between `^`
/// and `$` holds no metacharacters other than escapes.
fn plain_anchored_regex_text(text: &str) -> Option<String> {
    let (pattern, _) = split_regex_literal(text)?;
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

/// `node:assert` tests a regex against `String(err)`, such as
/// `Error: blank`; drop a leading `Identifier: ` to compare the message.
fn strip_error_name_prefix(text: &str) -> String {
    if let Some((name, message)) = text.split_once(": ")
        && !name.is_empty()
        && is_safe_javascript_identifier(name)
    {
        return message.to_string();
    }
    text.to_string()
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

        // A `+` split adds an operator token: not message-only (rule 4 text).
        let split = ErrorChangeFacts::read(
            Some(r#"    throw new Error("unsupported currency");"#),
            r#"    throw new Error("unsupported " + "currency");"#,
        );
        assert!(!split.message_only);
        assert_eq!(split.old_message.as_deref(), Some("unsupported currency"));

        // A changed condition on the same line is not message-only.
        let condition = ErrorChangeFacts::read(
            Some(r#"if (s === "") throw new Error("empty");"#),
            r#"if (s !== "") throw new Error("empty");"#,
        );
        assert!(!condition.message_only);

        // A changed interpolation is not message-only.
        let interpolation = ErrorChangeFacts::read(
            Some("throw new Error(`bad ${a}`);"),
            "throw new Error(`bad ${b}`);",
        );
        assert!(!interpolation.message_only);
        assert_eq!(interpolation.old_message, None);
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
    fn rule_4_string_payload_must_exclude_the_old_message() {
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
        // Not a message-only change: anchored regexes keep their credit.
        let split = ErrorChangeFacts::read(
            Some(r#"throw new Error("unsupported currency");"#),
            r#"throw new Error("unsupported " + "currency");"#,
        );
        assert!(error_payload_credits(
            &payload(Kind::AssertThrowsRegex, "/^Error: unsupported currency$/"),
            &split
        ));
    }
}
