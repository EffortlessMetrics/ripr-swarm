//! Literal message checks in chai and `node:assert` (RIPR-SPEC-0243 rule 10).
//!
//! chai `expect(fn).to.throw("msg")` and `node:assert` `throws` / `rejects`
//! with an anchored regex or a `{ message: "..." }` object pin the thrown
//! error's message, so they read `exact_error_variant` / strong. On a
//! message-only change (rule 4's guard), the payload credits only when it
//! tells the old message from the new one: a payload that also matches the
//! old message passes on both versions.

use super::*;

/// The rule 10 payload of a `node:assert` `throws` / `rejects` call: its
/// second argument is an anchored regex literal or an all-literal object
/// whose `message` is a string literal. chai's `assert` has another
/// signature, and `doesNotThrow` / `doesNotReject` never pin a message.
pub(crate) fn node_assert_message_payload(
    method: &str,
    flavor: AssertFlavor,
    call: &oxc_ast::ast::CallExpression<'_>,
    source: &str,
) -> Option<TypeScriptErrorPayload> {
    if flavor == AssertFlavor::Chai {
        return None;
    }
    let rejects = match method {
        "throws" => false,
        "rejects" => true,
        _ => return None,
    };
    let arg = call.arguments.get(1)?;
    if let Some((expected, pattern)) = anchored_regex_argument(arg, source) {
        let kind = if rejects {
            TypeScriptErrorPayloadKind::AssertRejectsRegex
        } else {
            TypeScriptErrorPayloadKind::AssertThrowsRegex
        };
        return Some(TypeScriptErrorPayload {
            expected,
            kind,
            message_check: Some(pattern),
        });
    }
    let Argument::ObjectExpression(object) = arg else {
        return None;
    };
    let expected = safe_error_object_payload_text(arg, source)?;
    let message = object_string_property(object, "message")?;
    let kind = if rejects {
        TypeScriptErrorPayloadKind::AssertRejectsObject
    } else {
        TypeScriptErrorPayloadKind::AssertThrowsObject
    };
    Some(TypeScriptErrorPayload {
        expected,
        kind,
        message_check: Some(message),
    })
}

/// The rule 10 payload of chai `expect(fn).to.throw("msg")`: a `throw`,
/// `throws` or `Throw` terminal, not under `not`, called with exactly one
/// string literal. A class, a regex or a second argument stays broad.
pub(crate) fn chai_throw_message_payload(
    terminal: &str,
    negated: bool,
    call: Option<&oxc_ast::ast::CallExpression<'_>>,
    source: &str,
) -> Option<TypeScriptErrorPayload> {
    if negated || !matches!(terminal, "throw" | "throws" | "Throw") {
        return None;
    }
    let call = call?;
    let [Argument::StringLiteral(literal)] = call.arguments.as_slice() else {
        return None;
    };
    Some(TypeScriptErrorPayload {
        expected: source_text_for_argument(call.arguments.first()?, source)?,
        kind: TypeScriptErrorPayloadKind::ChaiThrowLiteral,
        message_check: Some(literal.value.to_string()),
    })
}

/// `(source text, pattern)` for a regex literal anchored with `^` and `$`
/// that has no unescaped `|` outside a character class. Anything else
/// (unanchored, an alternation that escapes the anchors) is not a pin.
fn anchored_regex_argument(arg: &Argument<'_>, source: &str) -> Option<(String, String)> {
    if !matches!(arg, Argument::RegExpLiteral(_)) {
        return None;
    }
    let text = source_text_for_argument(arg, source)?;
    let body = text.strip_prefix('/')?;
    let pattern = &body[..body.rfind('/')?];
    let inner = pattern.strip_prefix('^')?;
    let inner = inner.strip_suffix('$')?;
    if inner.ends_with('\\') && !inner.ends_with("\\\\") {
        // `\$` escapes the final dollar: not an end anchor.
        return None;
    }
    (!has_top_level_alternation(pattern)).then(|| (text.clone(), pattern.to_string()))
}

/// Whether a regex pattern has an unescaped `|` outside a character class.
fn has_top_level_alternation(pattern: &str) -> bool {
    let mut in_class = false;
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                chars.next();
            }
            '[' => in_class = true,
            ']' => in_class = false,
            '|' if !in_class => return true,
            _ => {}
        }
    }
    false
}

/// The string value of a static `key: "literal"` property.
fn object_string_property(
    object: &oxc_ast::ast::ObjectExpression<'_>,
    key: &str,
) -> Option<String> {
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        let name = match &property.key {
            PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
            PropertyKey::StringLiteral(literal) => literal.value.as_str(),
            _ => return None,
        };
        match &property.value {
            Expression::StringLiteral(value) if name == key => Some(value.value.to_string()),
            _ => None,
        }
    })
}

/// A message-only change (RIPR-SPEC-0243 rule 4): the changed line throws
/// or rejects on both sides, and every token that differs lies inside a
/// string literal of the thrown error's message argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MessageOnlyChange {
    /// The old message: the old side's message argument with the literals
    /// of a `+` chain joined. `None` when an operand is not a literal, in
    /// which case no message check can be shown to tell the sides apart.
    pub(crate) old_message: Option<String>,
    /// The new message, assembled the same way.
    pub(crate) new_message: Option<String>,
}

/// Whether `old` → `new` is a message-only change of a throw or reject line.
pub(crate) fn message_only_change(old: &str, new: &str) -> Option<MessageOnlyChange> {
    let old_tokens = lex_line(old)?;
    let new_tokens = lex_line(new)?;
    if old_tokens.len() != new_tokens.len() {
        return None;
    }
    let old_range = message_argument_range(&old_tokens)?;
    let new_range = message_argument_range(&new_tokens)?;
    if old_range != new_range {
        return None;
    }
    let mut changed = false;
    for (index, (before, after)) in old_tokens.iter().zip(&new_tokens).enumerate() {
        match (before, after) {
            (Token::Code(a), Token::Code(b)) if a == b => {}
            (Token::Str(a), Token::Str(b)) => {
                if a != b {
                    if !old_range.contains(&index) {
                        return None;
                    }
                    changed = true;
                }
            }
            (Token::Template(a), Token::Template(b)) => {
                if a.len() != b.len() {
                    return None;
                }
                for (part_a, part_b) in a.iter().zip(b) {
                    match (part_a, part_b) {
                        (TemplatePart::Text(x), TemplatePart::Text(y)) => {
                            if x != y {
                                if !old_range.contains(&index) {
                                    return None;
                                }
                                changed = true;
                            }
                        }
                        (TemplatePart::Code(x), TemplatePart::Code(y)) if x == y => {}
                        _ => return None,
                    }
                }
            }
            _ => return None,
        }
    }
    changed.then(|| MessageOnlyChange {
        old_message: joined_literal_message(&old_tokens[old_range.clone()]),
        new_message: joined_literal_message(&new_tokens[new_range]),
    })
}

/// Whether a rule 10 payload still credits under a message-only change: it
/// must pass on exactly one of the two messages. A check that matches both
/// (chai `"blank"` against `"not blank"` and `"blank"`) or neither passes
/// or fails on both versions alike. Payloads outside rule 10 are not judged
/// here.
///
/// RIPR-SPEC-0243 rule 4 words the guard for a payload written against the
/// new message ("must not match the old message"). Reading both sides
/// keeps that case and also credits a test still pinned to the old
/// message, which fails on the new one (#6686 `mocha-spec14-chai-throw-string`).
pub(crate) fn message_check_tells_change_apart(
    payload: &TypeScriptErrorPayload,
    change: &MessageOnlyChange,
) -> bool {
    let Some(check) = payload.message_check.as_deref() else {
        return true;
    };
    let (Some(old), Some(new)) = (change.old_message.as_deref(), change.new_message.as_deref())
    else {
        return false;
    };
    let matches = |message: &str| -> Option<bool> {
        match payload.kind {
            TypeScriptErrorPayloadKind::ChaiThrowLiteral => Some(message.contains(check)),
            TypeScriptErrorPayloadKind::AssertThrowsObject
            | TypeScriptErrorPayloadKind::AssertRejectsObject => Some(check == message),
            TypeScriptErrorPayloadKind::AssertThrowsRegex
            | TypeScriptErrorPayloadKind::AssertRejectsRegex => {
                plain_anchored_regex_text(check).map(|text| text == message)
            }
            _ => None,
        }
    };
    match (matches(old), matches(new)) {
        (Some(on_old), Some(on_new)) => on_old != on_new,
        _ => false,
    }
}

/// The literal text an anchored regex matches when it has no metacharacter
/// between its anchors other than an escaped punctuation character, with a
/// leading `Identifier: ` (`node:assert` tests `String(err)`, such as
/// `Error: blank`) removed.
fn plain_anchored_regex_text(pattern: &str) -> Option<String> {
    let inner = pattern.strip_prefix('^')?.strip_suffix('$')?;
    let mut text = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let escaped = chars.next()?;
                if escaped.is_ascii_alphanumeric() {
                    // `\d`, `\s`, `A`: a class or code, not a literal.
                    return None;
                }
                text.push(escaped);
            }
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' => {
                return None;
            }
            _ => text.push(ch),
        }
    }
    let without_prefix = text
        .split_once(": ")
        .filter(|(name, _)| is_safe_javascript_identifier(name))
        .map_or(text.as_str(), |(_, message)| message);
    Some(without_prefix.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Code(String),
    /// A quoted string's raw text between its quotes.
    Str(String),
    Template(Vec<TemplatePart>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TemplatePart {
    Text(String),
    Code(String),
}

/// Split one source line into code tokens (whitespace dropped) and string
/// and template literals. `None` for a line the lexer cannot close (an
/// unterminated literal, a nested template in an interpolation).
fn lex_line(line: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
        } else if ch == '/' && chars.get(index + 1) == Some(&'/') {
            break;
        } else if ch == '"' || ch == '\'' {
            let (text, next) = read_quoted(&chars, index + 1, ch)?;
            tokens.push(Token::Str(text));
            index = next;
        } else if ch == '`' {
            let (parts, next) = read_template(&chars, index + 1)?;
            tokens.push(Token::Template(parts));
            index = next;
        } else if ch == '_' || ch == '$' || ch.is_alphanumeric() {
            let start = index;
            while index < chars.len()
                && (chars[index] == '_' || chars[index] == '$' || chars[index].is_alphanumeric())
            {
                index += 1;
            }
            tokens.push(Token::Code(chars[start..index].iter().collect()));
        } else {
            tokens.push(Token::Code(ch.to_string()));
            index += 1;
        }
    }
    Some(tokens)
}

fn read_quoted(chars: &[char], mut index: usize, quote: char) -> Option<(String, usize)> {
    let mut text = String::new();
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\\' {
            text.push(ch);
            text.push(*chars.get(index + 1)?);
            index += 2;
        } else if ch == quote {
            return Some((text, index + 1));
        } else {
            text.push(ch);
            index += 1;
        }
    }
    None
}

fn read_template(chars: &[char], mut index: usize) -> Option<(Vec<TemplatePart>, usize)> {
    let mut parts = Vec::new();
    let mut text = String::new();
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\\' {
            text.push(ch);
            text.push(*chars.get(index + 1)?);
            index += 2;
        } else if ch == '`' {
            parts.push(TemplatePart::Text(text));
            return Some((parts, index + 1));
        } else if ch == '$' && chars.get(index + 1) == Some(&'{') {
            parts.push(TemplatePart::Text(std::mem::take(&mut text)));
            let mut depth = 1usize;
            let mut code = String::new();
            index += 2;
            while depth > 0 {
                let next = *chars.get(index)?;
                match next {
                    '`' => return None,
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                if depth > 0 && !next.is_whitespace() {
                    code.push(next);
                }
                index += 1;
            }
            parts.push(TemplatePart::Code(code));
        } else {
            text.push(ch);
            index += 1;
        }
    }
    None
}

fn is_code(token: Option<&Token>, text: &str) -> bool {
    matches!(token, Some(Token::Code(code)) if code == text)
}

/// The token range of the thrown or rejected error's message argument:
/// after `throw` or `Promise.reject(`, either the first argument of an
/// error constructor call (`new Error(msg)`, `Error(msg)`,
/// `new errors.Parse(msg)`) or a thrown primitive up to the end of the
/// expression.
fn message_argument_range(tokens: &[Token]) -> Option<std::ops::Range<usize>> {
    let (mut cursor, in_reject_call) = tokens.iter().enumerate().find_map(|(index, _)| {
        if is_code(tokens.get(index), "throw") {
            Some((index + 1, false))
        } else if is_code(tokens.get(index), "Promise")
            && is_code(tokens.get(index + 1), ".")
            && is_code(tokens.get(index + 2), "reject")
            && is_code(tokens.get(index + 3), "(")
        {
            Some((index + 4, true))
        } else {
            None
        }
    })?;
    if is_code(tokens.get(cursor), "new") {
        cursor += 1;
    }
    let mut path_end = cursor;
    while matches!(tokens.get(path_end), Some(Token::Code(code)) if is_safe_javascript_identifier(code))
    {
        if is_code(tokens.get(path_end + 1), ".") {
            path_end += 2;
        } else {
            path_end += 1;
            break;
        }
    }
    if path_end > cursor && is_code(tokens.get(path_end), "(") {
        let start = path_end + 1;
        return Some(start..argument_end(tokens, start));
    }
    // A thrown or rejected primitive: up to `;` (or the reject call's `)`).
    let end = if in_reject_call {
        argument_end(tokens, cursor)
    } else {
        tokens[cursor..]
            .iter()
            .position(|token| matches!(token, Token::Code(code) if code == ";"))
            .map_or(tokens.len(), |offset| cursor + offset)
    };
    (end > cursor).then_some(cursor..end)
}

/// The index of the `,` or `)` that ends the argument starting at `start`.
fn argument_end(tokens: &[Token], start: usize) -> usize {
    let mut depth = 0usize;
    for (offset, token) in tokens[start..].iter().enumerate() {
        if let Token::Code(code) = token {
            match code.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" if depth == 0 => return start + offset,
                ")" | "]" | "}" => depth -= 1,
                "," if depth == 0 => return start + offset,
                _ => {}
            }
        }
    }
    tokens.len()
}

/// The literals of a `"a" + "b"` chain joined, or `None` when any operand
/// is not a string literal (or a template without interpolation).
fn joined_literal_message(tokens: &[Token]) -> Option<String> {
    let mut message = String::new();
    let mut expect_literal = true;
    for token in tokens {
        match (token, expect_literal) {
            (Token::Str(text), true) => message.push_str(&unescape(text)),
            (Token::Template(parts), true) => match parts.as_slice() {
                [TemplatePart::Text(text)] => message.push_str(&unescape(text)),
                _ => return None,
            },
            (Token::Code(code), false) if code == "+" => {}
            _ => return None,
        }
        expect_literal = !expect_literal;
    }
    (!expect_literal).then_some(message)
}

/// Resolve the simple escapes a message literal uses (`\"`, `\'`, `\\`,
/// `\n`, `\t`); any other escape keeps its character.
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
