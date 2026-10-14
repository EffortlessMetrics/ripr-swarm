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

/// Whether a `throw ...` / `Promise.reject(...)` token run throws or
/// rejects a primitive: its operand opens with a literal or a number.
fn rejects_primitive(throw: &[LineToken]) -> bool {
    let operand = match throw.get(1) {
        Some(LineToken::Other(open)) if open == "(" => throw.get(2),
        other => other,
    };
    match operand {
        Some(LineToken::Literal(_) | LineToken::Template { .. }) => true,
        Some(LineToken::Other(word)) => word.chars().next().is_some_and(|c| c.is_ascii_digit()),
        None => false,
    }
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

/// Whether `token` is a `throw` keyword or a `Promise.reject` callee.
fn is_throw_site(token: &LineToken) -> bool {
    matches!(token, LineToken::Other(word) if word == "throw" || word == "Promise.reject")
}

/// How many `throw` / `Promise.reject` sites the line has. More than one
/// makes the first-site message untrustworthy (#7950). Template
/// interpolations are scanned too: a second site inside `` `${throw ...}` ``
/// is still a site on the line. An interpolation that cannot be tokenized
/// counts as one unread site so the first-site message is not trusted.
fn throw_site_count(tokens: &[LineToken]) -> usize {
    tokens.iter().map(throw_sites_in_token).sum()
}

fn throw_sites_in_token(token: &LineToken) -> usize {
    if is_throw_site(token) {
        return 1;
    }
    match token {
        LineToken::Template { interpolations, .. } => interpolations
            .iter()
            .map(|source| match tokenize(source) {
                Some(nested) => throw_site_count(&nested),
                None => 1,
            })
            .sum(),
        _ => 0,
    }
}

/// The tokens before and from the `throw` keyword or the `Promise.reject`
/// callee, or `None` when the line has neither.
fn split_at_throw(tokens: &[LineToken]) -> Option<(&[LineToken], &[LineToken])> {
    let index = tokens.iter().position(is_throw_site)?;
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
    /// The old side throws or rejects a primitive (`throw "bad"`,
    /// `Promise.reject("bad")`), so the rejected reason is the message
    /// itself rather than an `Error` carrying it.
    pub(crate) old_primitive: bool,
    /// The new side's [`Self::old_primitive`].
    pub(crate) new_primitive: bool,
}

impl ErrorChangeFacts {
    /// Read the facts for a changed line. With no old line (a pure
    /// addition, or a preview that reads only the current bytes) the old
    /// side stays unread, so `message_only` and `separates` remain
    /// unestablished while the new side's thrown message is still known.
    pub(crate) fn read(old_line: Option<&str>, new_line: &str) -> Self {
        let old_tokens = old_line.and_then(tokenize);
        let old_split = old_tokens.as_deref().and_then(split_at_throw);
        // Two sites on one line: the first-site message is not the thrown
        // value of every path, so it is unread. The split itself stays,
        // because refusing it would leave `message_only` false and restore
        // the ladder's credit (#7950, RIPR-SPEC-0243 rule 4).
        let old_ambiguous = old_tokens
            .as_deref()
            .is_some_and(|tokens| throw_site_count(tokens) > 1);
        let (old_message, old_string_form) = if old_ambiguous {
            (None, None)
        } else {
            (
                old_split.and_then(|(_, throw)| thrown_message(throw)),
                old_split.and_then(|(_, throw)| thrown_string_form(throw)),
            )
        };
        let new_tokens = tokenize(new_line);
        let new_split = new_tokens.as_deref().and_then(split_at_throw);
        let new_ambiguous = new_tokens
            .as_deref()
            .is_some_and(|tokens| throw_site_count(tokens) > 1);
        let (new_message, new_string_form) = if new_ambiguous {
            (None, None)
        } else {
            (
                new_split.and_then(|(_, throw)| thrown_message(throw)),
                new_split.and_then(|(_, throw)| thrown_string_form(throw)),
            )
        };
        let message_only = match (old_split, new_split) {
            (Some((old_prefix, old_throw)), Some((new_prefix, new_throw))) => {
                let (old_throw, new_throw) = (
                    without_trailing_semicolon(old_throw),
                    without_trailing_semicolon(new_throw),
                );
                // The same-message exception stays on the argument path:
                // on the literal path a changed later argument (`cause:
                // "x"` against `cause: "y"`) reads the same first-argument
                // message, and lifting `message_only` there would leave
                // the ladder's credit on a test that passes on both sides.
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
            old_primitive: old_split.is_some_and(|(_, throw)| rejects_primitive(throw)),
            new_primitive: new_split.is_some_and(|(_, throw)| rejects_primitive(throw)),
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
/// produces observes a different throw site, not this change. The matcher
/// is an equality (`toBe`, `toEqual`, `toStrictEqual`), so the reason must
/// equal a message: a substring of one passes on neither version.
///
/// An equality against a string can only pass for a primitive reason: a
/// rejected `Error` never equals the string, whatever its message, so a
/// side that throws an `Error` gives no credit (#7944).
pub(crate) fn literal_rejection_credits(pinned: Option<&str>, facts: &ErrorChangeFacts) -> bool {
    let Some(value) = pinned.and_then(string_literal_value) else {
        return false;
    };
    if facts.message_only {
        return facts.old_primitive
            && facts.new_primitive
            && facts.separates(|message| message == value);
    }
    facts.new_primitive && facts.new_message.as_deref() == Some(value.as_str())
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
///
/// The argument is the first argument of a call or construction whose
/// callee is a name or a dotted path, optionally with type arguments
/// (`new RangeError(...)`, `errors.make(...)`, `make<T>(...)`); the callee
/// stays in the head, so a changed class or factory is not message-only.
/// Any other operand (a ternary, a sequence, an unclosed line) is read
/// whole. The split falls back rather than refusing, because a refusal
/// leaves `message_only` false and the ladder's credit in place: a shape it
/// cannot read may only remove credit (RIPR-SPEC-0243 rule 4).
fn split_message_argument(throw: &[LineToken]) -> Option<(usize, usize)> {
    let mut start = 1;
    if let Some(LineToken::Other(word)) = throw.get(start)
        && word == "("
    {
        start += 1;
    }
    if matches!(throw.get(start), Some(LineToken::Other(word)) if word == "new") {
        start += 1;
    }
    let called = match call_argument_start(throw, start) {
        Some(argument) => {
            start = argument;
            true
        }
        None => false,
    };
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
            // In a call, a top-level comma ends the first argument; in a
            // whole operand it is the sequence operator and stays inside.
            LineToken::Other(comma) if comma == "," && depth == 0 && called => break,
            _ => {}
        }
        end += 1;
    }
    (end > start).then_some((start, end))
}

/// The index just past the `(` of a call at `at` whose callee is one name
/// or dotted path (the tokenizer keeps `errors.make` as one word), with an
/// optional balanced `<...>` type-argument list.
fn call_argument_start(throw: &[LineToken], at: usize) -> Option<usize> {
    let Some(LineToken::Other(callee)) = throw.get(at) else {
        return None;
    };
    if !callee
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
    {
        return None;
    }
    let mut next = at + 1;
    if matches!(throw.get(next), Some(LineToken::Other(open)) if open == "<") {
        let mut depth = 0usize;
        loop {
            match throw.get(next)? {
                LineToken::Other(open) if open == "<" => depth += 1,
                LineToken::Other(close) if close == ">" => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        next += 1;
                        break;
                    }
                }
                _ => {}
            }
            next += 1;
        }
    }
    matches!(throw.get(next), Some(LineToken::Other(open)) if open == "(").then_some(next + 1)
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
        // `throw (log("a"), e)`: a top-level comma in the parentheses is
        // the sequence operator; the thrown value is the last operand.
        if has_top_level_comma(rest) {
            return None;
        }
    }
    if let [LineToken::Other(word), ..] = rest
        && word == "new"
    {
        rest = rest.get(1..)?;
    }
    // `Ident(` (or `a.b<T>(`) opens a constructor or factory call: read its
    // first argument, located as `split_message_argument` locates it.
    let mut called = false;
    if let Some(argument) = call_argument_start(rest, 0) {
        rest = rest.get(argument..)?;
        called = true;
    }
    let mut message = String::new();
    let mut expect_operand = true;
    for (index, token) in rest.iter().enumerate() {
        if expect_operand {
            message.push_str(literal_value(token)?);
            expect_operand = false;
            continue;
        }
        match token {
            LineToken::Other(op) if op == "+" => expect_operand = true,
            // Only closers and `;` may follow: `throw ("a") + "b"` or
            // `new Error("a").stack` throws more than the literal read.
            LineToken::Other(end) if end == ")" => {
                // Up to the statement's `;` (a `}` after it closes a block,
                // not the thrown value).
                let tail = rest.get(index + 1..).unwrap_or_default();
                if !tail
                    .iter()
                    .take_while(|t| !matches!(t, LineToken::Other(c) if c == ";"))
                    .all(|t| matches!(t, LineToken::Other(c) if c == ")"))
                {
                    return None;
                }
                break;
            }
            LineToken::Other(end) if end == ";" => break,
            // Outside a call a comma is the sequence operator (`throw "x",
            // e`), and the thrown value is a later operand.
            LineToken::Other(comma) if comma == "," && called => break,
            _ => return None,
        }
    }
    (!expect_operand).then_some(message)
}

/// Whether a top-level `,` comes before the `)` that closes the group
/// `tokens` opens inside.
fn has_top_level_comma(tokens: &[LineToken]) -> bool {
    let mut depth = 0usize;
    for token in tokens {
        match token {
            LineToken::Other(open) if open == "(" || open == "[" || open == "{" => depth += 1,
            LineToken::Other(close) if close == ")" || close == "]" || close == "}" => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            LineToken::Other(comma) if comma == "," && depth == 0 => return true,
            _ => {}
        }
    }
    false
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
        // A changed literal in a later argument stays message-only, so a
        // payload on the unchanged first-argument message does not
        // separate the versions and gains no credit.
        let later = ErrorChangeFacts::read(
            Some(r#"throw new Error("bad", { cause: "x" });"#),
            r#"throw new Error("bad", { cause: "y" });"#,
        );
        assert!(later.message_only);
        assert!(!error_payload_credits(
            &payload(TypeScriptErrorPayloadKind::ThrowsLiteral, r#""bad""#),
            &later
        ));
        // A generic factory's message is read past its type arguments.
        let generic_read =
            ErrorChangeFacts::read(Some(r#"throw make<A>("a");"#), r#"throw make<A>("b");"#);
        assert!(generic_read.message_only);
        assert_eq!(generic_read.old_message.as_deref(), Some("a"));
        // An operand that continues past the message's closing `)` throws
        // more than the literal read, so no message is read.
        for (old, new) in [
            (r#"throw ("a" + "b") + "ab";"#, r#"throw ("cd") + "ab";"#),
            (r#"throw ("a") + "b";"#, r#"throw ("ab") + "b";"#),
            (r#"throw new Error("a").x;"#, r#"throw new Error("b").x;"#),
        ] {
            let facts = ErrorChangeFacts::read(Some(old), new);
            assert_eq!(
                (facts.old_message, facts.new_message),
                (None, None),
                "{old}"
            );
        }
        // A block that closes after the statement still reads its message.
        let block = ErrorChangeFacts::read(
            Some(r#"if (x) { throw new Error("a"); }"#),
            r#"if (x) { throw new Error("b"); }"#,
        );
        assert_eq!(block.old_message.as_deref(), Some("a"));
        assert_eq!(block.new_message.as_deref(), Some("b"));
        // A sequence throws its last operand, so no message is read from
        // an earlier one.
        for (old, new) in [
            (r#"throw (log("a"), e);"#, r#"throw (log("b"), e);"#),
            (r#"throw ("x", "b");"#, r#"throw ("y", "b");"#),
            (r#"throw "x", "b";"#, r#"throw "y", "b";"#),
        ] {
            let facts = ErrorChangeFacts::read(Some(old), new);
            assert_eq!(
                (facts.old_message, facts.new_message),
                (None, None),
                "{old}"
            );
        }
        // A dotted or generic callee that stays the same keeps the change
        // inside its first argument; a shape the split cannot name a call
        // in (a sequence, an unclosed line) is read whole. Either way the
        // change is message-only, which can only remove credit: refusing
        // the split would leave the ladder's credit in place.
        for (old, new) in [
            (
                r#"throw new errors.ParseError("bad " + x);"#,
                r#"throw new errors.ParseError("bad ");"#,
            ),
            (
                r#"throw errors.ParseError("bad " + x);"#,
                r#"throw errors.ParseError("bad ");"#,
            ),
            (r#"throw make<A>("bad " + x);"#, r#"throw make<A>("bad ");"#),
            (
                "throw errors.make(`bad ${a}`);",
                "throw errors.make(`bad ${b}`);",
            ),
            (r#"throw (log(a), e);"#, r#"throw (log(b), e);"#),
            (r#"throw x, e;"#, r#"throw y, e;"#),
            (r#"throw f([a"#, r#"throw f([b"#),
        ] {
            assert!(
                ErrorChangeFacts::read(Some(old), new).message_only,
                "{old} -> {new}"
            );
        }
        // A changed class or factory (single-name, dotted or generic), a
        // later argument that also changes, and an added argument are not.
        for (old, new) in [
            (r#"throw new Error("a");"#, r#"throw new TypeError("b");"#),
            (
                r#"throw new errors.A("bad");"#,
                r#"throw new errors.B("bad");"#,
            ),
            (r#"throw errors.A("bad");"#, r#"throw errors.B("bad");"#),
            (r#"throw makeErr("a");"#, r#"throw makeOther("b");"#),
            (
                r#"throw errors.make("bad " + x);"#,
                r#"throw errors.other("bad ");"#,
            ),
            (r#"throw make<A>("bad " + x);"#, r#"throw make<B>("bad ");"#),
            (
                r#"throw new Error("a", { cause: x });"#,
                r#"throw new Error("b", { cause: y });"#,
            ),
            (r#"throw new Error();"#, r#"throw new Error("x");"#),
        ] {
            assert!(
                !ErrorChangeFacts::read(Some(old), new).message_only,
                "{old} -> {new}"
            );
        }
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
    fn multiple_throw_sites_read_no_message() {
        use TypeScriptErrorPayloadKind as Kind;
        // Two throw sites: the first-site messages would "separate"
        // `toThrow("a")`, which credits a test that may only hit the
        // second throw (#7950).
        let two_throws = ErrorChangeFacts::read(
            Some(r#"if (a) throw new Error("a"); if (b) throw new Error("b");"#),
            r#"if (a) throw new Error("x"); if (b) throw new Error("b");"#,
        );
        assert_eq!(two_throws.old_message, None);
        assert_eq!(two_throws.new_message, None);
        // The split is kept, so a literal-only first-site change is still
        // message-only; unread messages then keep `separates` false.
        assert!(two_throws.message_only);
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""a""#),
            &two_throws
        ));
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""x""#),
            &two_throws
        ));
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""b""#),
            &two_throws
        ));

        let two_rejects = ErrorChangeFacts::read(
            Some(r#"c ? Promise.reject("a") : Promise.reject("b")"#),
            r#"c ? Promise.reject("x") : Promise.reject("b")"#,
        );
        assert_eq!(two_rejects.old_message, None);
        assert_eq!(two_rejects.new_message, None);
        assert!(!error_payload_credits(
            &payload(Kind::RejectsThrowLiteral, r#""a""#),
            &two_rejects
        ));

        let mixed = ErrorChangeFacts::read(
            Some(r#"throw new Error("a"); return Promise.reject("b");"#),
            r#"throw new Error("x"); return Promise.reject("b");"#,
        );
        assert_eq!(mixed.old_message, None);
        assert_eq!(mixed.new_message, None);

        // A throw inside a template interpolation is a second site.
        let nested = ErrorChangeFacts::read(
            Some(
                r#"if (a) throw new Error("a"); return `${(() => { throw new Error("b") })()}`; "#,
            ),
            r#"if (a) throw new Error("x"); return `${(() => { throw new Error("b") })()}`; "#,
        );
        assert_eq!(nested.old_message, None);
        assert_eq!(nested.new_message, None);
        assert!(!error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""a""#),
            &nested
        ));
        // An interpolation without a throw is not a second site.
        let template_value = ErrorChangeFacts::read(
            Some(r#"if (a) throw new Error("a"); return `${n}`; "#),
            r#"if (a) throw new Error("x"); return `${n}`; "#,
        );
        assert_eq!(template_value.old_message.as_deref(), Some("a"));
        assert_eq!(template_value.new_message.as_deref(), Some("x"));

        // A `throw` inside a string is not a second site.
        let literal_word = ErrorChangeFacts::read(
            Some(r#"throw new Error("throw a");"#),
            r#"throw new Error("throw x");"#,
        );
        assert_eq!(literal_word.old_message.as_deref(), Some("throw a"));
        assert_eq!(literal_word.new_message.as_deref(), Some("throw x"));

        // One throw site still reads the message (control).
        let one = ErrorChangeFacts::read(
            Some(r#"if (a) throw new Error("a");"#),
            r#"if (a) throw new Error("x");"#,
        );
        assert_eq!(one.old_message.as_deref(), Some("a"));
        assert_eq!(one.new_message.as_deref(), Some("x"));
        assert!(error_payload_credits(
            &payload(Kind::ThrowsLiteral, r#""a""#),
            &one
        ));
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
        // The same holds behind a dotted factory: the split falls back to
        // its first argument rather than refusing, so `toThrow("bad")`,
        // which passes on both versions, is not credited.
        let factory = ErrorChangeFacts::read(
            Some(r#"throw errors.make("bad " + x);"#),
            r#"throw errors.make("bad ");"#,
        );
        assert!(!error_payload_credits(
            &payload(TypeScriptErrorPayloadKind::ThrowsLiteral, r#""bad""#),
            &factory
        ));
        // A pinned rejection reason that only the old, readable message
        // holds separates the versions and keeps its credit.
        // The reason must equal a message: a substring of the old one
        // (`"input"`) passes on neither version.
        let shortened = ErrorChangeFacts::read(
            Some(r#"return Promise.reject("bad " + "input");"#),
            r#"return Promise.reject("bad ");"#,
        );
        assert!(shortened.message_only);
        assert!(literal_rejection_credits(
            Some(r#""bad input""#),
            &shortened
        ));
        assert!(literal_rejection_credits(Some(r#""bad ""#), &shortened));
        assert!(!literal_rejection_credits(Some(r#""input""#), &shortened));
        assert!(!literal_rejection_credits(Some(r#""bad""#), &shortened));
        // A rejected `Error` never equals a string, whatever its message
        // (#7944): no credit on either path.
        let error_reason = ErrorChangeFacts::read(
            Some(r#"throw new Error("a" + "b");"#),
            r#"throw new Error("a");"#,
        );
        assert!(error_reason.message_only);
        assert!(!literal_rejection_credits(Some(r#""ab""#), &error_reason));
        assert!(!literal_rejection_credits(Some(r#""a""#), &error_reason));
        let added_error = ErrorChangeFacts::read(None, r#"throw new Error("bad");"#);
        assert!(!literal_rejection_credits(Some(r#""bad""#), &added_error));
        let rejected_error = ErrorChangeFacts::read(
            Some(r#"return Promise.reject(new Error("a"));"#),
            r#"return Promise.reject(new Error("b"));"#,
        );
        assert!(!literal_rejection_credits(Some(r#""a""#), &rejected_error));
        // A caught rejection throws nothing this line reads.
        let caught = ErrorChangeFacts::read(
            Some(r#"return Promise.reject("a").catch(f);"#),
            r#"return Promise.reject("b").catch(f);"#,
        );
        assert!(!literal_rejection_credits(Some(r#""a""#), &caught));
        // Off the message-only path (here an added line) the reason must
        // equal the new message.
        let unrelated = ErrorChangeFacts::read(None, r#"return Promise.reject("bad input");"#);
        assert!(!unrelated.message_only);
        assert!(literal_rejection_credits(
            Some(r#""bad input""#),
            &unrelated
        ));
        assert!(!literal_rejection_credits(Some(r#""input""#), &unrelated));
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
