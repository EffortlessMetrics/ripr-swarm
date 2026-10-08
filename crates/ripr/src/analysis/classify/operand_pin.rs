//! #7077: a constructed field pinned only where it equals one of its operands.
//!
//! `total_cents: subtotal + shipping` is credited by
//! `assert_eq!(q.total_cents, 9_000)`. When the same test also pins
//! `q.subtotal_cents` (initialized from `subtotal`) to `9_000`, the field
//! equals that operand for that input, so replacing the field with the
//! operand alone passes the test. Both operands must be established primitive,
//! so the dropped operator is a built-in one: a custom type can overload
//! it, and an overloaded operator may carry side effects that assertions
//! on other fields observe (#7084 review). When every test that pins the
//! field is paired this way, the exact oracle does not discriminate the
//! change.

use super::super::rust_index::{RustIndex, TestSummary};
use crate::domain::{OracleKind, RelationReason};

/// Token carried in the discriminate summary when [`operand_only_pin`] holds.
pub(in crate::analysis) const FIELD_PINNED_EQUAL_TO_OPERAND: &str = "field_pinned_equal_to_operand";

/// The field whose pin cannot tell the changed initializer from one of its
/// operands, the sibling field that pins that operand, and the other operand
/// the pinning tests never vary.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::analysis) struct OperandOnlyPin {
    pub field: String,
    pub sibling: String,
    pub unseen_operand: String,
}

impl OperandOnlyPin {
    pub(in crate::analysis) fn summary(&self) -> String {
        format!(
            "Discriminator unconfirmed: every test that pins `{field}` also pins `{sibling}` to the same value ({FIELD_PINNED_EQUAL_TO_OPERAND}), so `{field}` equals that operand there and dropping `{unseen}` passes; pin `{field}` for an input where `{unseen}` changes the result",
            field = self.field,
            sibling = self.sibling,
            unseen = self.unseen_operand,
        )
    }
}

/// Whether the changed field initializer `expression` (`total_cents:
/// subtotal + shipping`) is pinned only where it equals one operand: some
/// sibling field of the same struct literal in `owner_body` is bound to that
/// operand, and every exact pin on the field in every related test sits
/// beside a pin of that sibling, on the same receiver, to the same literal.
/// Each receiver must be bound once, straight from a call to `owner_name`.
/// At least one related test must pin the field. Both operands must be
/// established primitive ([`operand_established_primitive`]): an accepted operator on
/// an operand that is not established primitive may be overloaded, and dropping an overloaded
/// operator call could skip side effects other assertions observe. Any
/// mention of the field that is not such a pin (a custom message, another
/// assertion macro, a binding read out of the result) returns `None`, as
/// does anything else ripr cannot read, which leaves the finding as it was.
pub(in crate::analysis) fn operand_only_pin(
    expression: &str,
    owner_name: &str,
    owner_body: &str,
    tests: &[(&TestSummary, RelationReason)],
    index: &RustIndex,
) -> Option<OperandOnlyPin> {
    let (field, left, right) = binary_field_initializer(expression)?;
    if !operand_established_primitive(left, owner_body, index)
        || !operand_established_primitive(right, owner_body, index)
    {
        return None;
    }
    let siblings = sibling_initializers(owner_body, expression)?;
    let mut tested = Vec::new();
    for (test, reason) in tests {
        let pins = exact_field_pins(test);
        let field_pins = pins
            .iter()
            .filter(|(_, pinned_field, _)| pinned_field == field)
            .collect::<Vec<_>>();
        let unread_mention = test.assertions.iter().any(|assertion| {
            whole_word_count(&assertion.text, field) > 0
                && !(assertion.kind == OracleKind::ExactValue
                    && exact_field_pins_of(&assertion.text)
                        .is_some_and(|(_, pinned_field, _)| pinned_field == field))
        });
        if unread_mention || whole_word_count(test.body.as_str(), field) != field_pins.len() {
            return None;
        }
        if field_pins.is_empty() {
            // A test that runs the owner and asserts without naming the field
            // may still observe it: whole-struct equality, a snapshot, or a
            // helper (`assert_quote(&q, 1_499)`), including a helper that
            // calls the owner for it (`assert_eq!(make_quote(), expected)`).
            let reaches_owner = matches!(
                reason,
                RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall
            ) || whole_word_count(test.body.as_str(), owner_name) > 0
                || test
                    .assertions
                    .iter()
                    .any(|assertion| whole_word_count(&assertion.text, owner_name) > 0);
            // With no assertion of its own it may still hand the result to a
            // helper that asserts (`check_quote(quote(2_500, 4))`).
            if reaches_owner {
                return None;
            }
            continue;
        }
        // A pin under a condition, a loop, a closure or after an early exit
        // may never run (`if false { assert_eq!(q.subtotal_cents, 9_000) }`),
        // so a pinning test with any such construct is not read.
        if has_control_flow(test.body.as_str()) {
            return None;
        }
        if field_pins.iter().any(|(receiver, _, _)| {
            !bound_once_from_owner_call(test.body.as_str(), receiver, owner_name)
        }) {
            return None;
        }
        // The same test may also observe the whole result beside its pins
        // (`assert_eq!(q, expected_quote())`, `check_quote(&q)`, a second
        // `let r = quote(..)`), or gate a pin (`#[cfg(any())]`), so every
        // statement must be a receiver binding or an assertion that reads
        // receivers only through plain fields (`assert_eq!(q.tier, Tier::Gold)`).
        let receivers = field_pins
            .iter()
            .map(|(receiver, _, _)| receiver.as_str())
            .collect::<Vec<_>>();
        let whole_result_check =
            !only_bindings_and_field_reads(test.body.as_str(), &receivers, owner_name);
        if whole_result_check {
            return None;
        }
        tested.push((
            pins.clone(),
            field_pins.into_iter().cloned().collect::<Vec<_>>(),
        ));
    }
    if tested.is_empty() {
        return None;
    }
    siblings.iter().find_map(|(sibling, operand)| {
        let unseen = if *operand == left {
            right
        } else if *operand == right {
            left
        } else {
            return None;
        };
        tested
            .iter()
            .all(|(pins, field_pins)| {
                field_pins.iter().all(|(receiver, _, value)| {
                    pins.iter()
                        .any(|(other_receiver, other_field, other_value)| {
                            other_receiver == receiver
                                && other_field == sibling
                                && other_value == value
                        })
                })
            })
            .then(|| OperandOnlyPin {
                field: field.to_string(),
                sibling: sibling.to_string(),
                unseen_operand: unseen.to_string(),
            })
    })
}

/// `name: left <op> right` with plain identifier operands and one binary
/// operator, written with spaces around it.
fn binary_field_initializer(expression: &str) -> Option<(&str, &str, &str)> {
    let trimmed = expression.trim().trim_end_matches(',').trim();
    let (name, value) = trimmed.split_once(':')?;
    if value.starts_with(':') {
        return None;
    }
    let name = name.trim();
    let parts = value.split_whitespace().collect::<Vec<_>>();
    let [left, operator, right] = parts.as_slice() else {
        return None;
    };
    let binary = matches!(*operator, "+" | "-" | "*" | "/" | "%" | "|" | "^" | "&");
    (binary && is_identifier(name) && is_identifier(left) && is_identifier(right) && left != right)
        .then_some((name, *left, *right))
}

/// Whether `operand` is established to hold a built-in-operator value, so the
/// dropped `<op>` of the initializer is a primitive operator with no call
/// to skip. A custom type can overload any accepted operator (review of
/// #7084), and an overloaded operator may carry side effects that
/// assertions on other fields observe, so an operand that cannot be established
/// primitive keeps the finding. Proof runs through the owner's own text: a
/// parameter of a primitive type, or a `let` bound to a numeric or bool
/// literal, to another established name, to an arithmetic combination of those,
/// or to a call whose every same-named function in the index declares one
/// bare primitive return. A string, char literal or block comment anywhere
/// in the owner body, or anything else unreadable, proves nothing.
fn operand_established_primitive(operand: &str, owner_body: &str, index: &RustIndex) -> bool {
    // Comments are dropped first. A string, char literal or block comment
    // could hide a binding or a type this proof would misread, so such an
    // owner is not read at all.
    let text = strip_line_comments(owner_body);
    if text.contains(['"', '\'']) || text.contains("/*") {
        return false;
    }
    let bindings = let_binding_expressions(&text);
    let mut established: Vec<String> = owner_param_types(&text)
        .into_iter()
        .filter(|(_, ty)| primitive_type_token(ty))
        .map(|(name, _)| name)
        .collect();
    loop {
        let mut changed = false;
        for (name, _, _) in &bindings {
            if established.iter().any(|seen| seen == name) {
                continue;
            }
            // Shadowing: a name is established only when every binding of it
            // proves, so the value at the literal is primitive whichever
            // binding produced it. The annotation owns the type when
            // present: a primitive annotation proves the binding outright,
            // a non-primitive one refuses it even when the initializer
            // looks like a literal (`let s: Money = 499;`).
            let all = bindings.iter().filter(|(bound, _, _)| bound == name).all(
                |(_, bound_expr, annotation)| match annotation {
                    Some(ty) => primitive_type_token(ty),
                    None => expression_established(bound_expr, &established, index),
                },
            );
            if all {
                established.push(name.clone());
                changed = true;
            }
        }
        if !changed {
            return established.iter().any(|seen| seen == operand);
        }
    }
}

/// Whether one binding initializer proves its name primitive. A typed
/// binding with a primitive annotation proves it outright; the value forms
/// are read by [`expression_established`].
fn expression_established(expr: &str, established: &[String], index: &RustIndex) -> bool {
    let expr = expr.trim();
    if is_identifier(expr) {
        return expr == "true" || expr == "false" || established.iter().any(|name| name == expr);
    }
    if let Some(callee) = call_callee(expr) {
        return call_established_primitive(callee, index);
    }
    arithmetic_established(expr, established)
}

/// The last segment of a plain free call (`name(..)`, `a::b::name(..)`),
/// when the whole expression is exactly that call. A method call, a
/// turbofish, a chained transform or a type segment in the path (`Other::
/// quote`) is not read.
fn call_callee(expr: &str) -> Option<&str> {
    let expr = expr.trim();
    if expr.contains("::<") {
        return None;
    }
    let open = expr.find('(')?;
    if matching_paren(expr, open) != Some(expr.len() - 1) {
        return None;
    }
    let callee = expr[..open].trim();
    let name = match callee.rsplit_once("::") {
        Some((path, name))
            if path.split("::").all(|segment| {
                segment
                    .trim()
                    .starts_with(|ch: char| ch.is_ascii_lowercase())
            }) =>
        {
            name
        }
        Some(_) => return None,
        None => callee,
    };
    is_identifier(name).then_some(name)
}

/// Whether every function named `callee` in the index declares one bare
/// primitive return (optionally behind one reference). No such function,
/// disagreeing returns, or any non-primitive return proves nothing: a call
/// to a function outside the index (another crate) could return a type
/// that overloads the operator.
fn call_established_primitive(callee: &str, index: &RustIndex) -> bool {
    let mut seen: Option<String> = None;
    for function in index.functions().iter() {
        if function.name != callee {
            continue;
        }
        let Some(returns) = declared_return_type(function.body.as_str()) else {
            return false;
        };
        match &seen {
            Some(previous) if previous != returns => return false,
            _ => seen = Some(returns.to_string()),
        }
    }
    seen.is_some_and(|token| primitive_type_token(&token))
}

/// The declared return type text of a function whose `body` starts at its
/// signature: the tokens between `->` and the body's `{` or `;`, with a
/// `where` clause cut off. `None` when the signature shape is unreadable.
fn declared_return_type(body: &str) -> Option<&str> {
    let at = body.find("fn ")?;
    let open = body[at..].find('(')? + at;
    let close = matching_paren(body, open)?;
    let rest = body[close + 1..].trim_start();
    let rest = rest.strip_prefix("->")?.trim_start();
    let end = rest.find(['{', ';']).unwrap_or(rest.len());
    let token = rest[..end].trim();
    let token = match token.find(" where ") {
        Some(at) => token[..at].trim(),
        None => token,
    };
    (!token.is_empty()).then_some(token)
}

/// Whether `text` is one bare primitive arithmetic type, optionally behind
/// one reference. A generic parameter, a path or a compound type is not.
fn primitive_type_token(text: &str) -> bool {
    let text = text.trim();
    let text = text.strip_prefix("&mut ").unwrap_or(text);
    let text = text.strip_prefix('&').unwrap_or(text).trim();
    matches!(
        text,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
    )
}

/// `(name, initializer, annotation)` for each plain `let name = ..` of
/// `text`, in order. The signature wrapper is cut first, so a `let` after
/// the body's opening brace is read. A typed binding carries its
/// annotation text; patterns other than a plain name, and
/// initializer-less declarations, are not read.
fn let_binding_expressions(text: &str) -> Vec<(String, String, Option<String>)> {
    let inner = match text.find('{') {
        Some(open) if text[..open].contains("fn ") => text[open + 1..]
            .trim_end()
            .strip_suffix('}')
            .unwrap_or(&text[open + 1..]),
        _ => text,
    };
    inner
        .split(';')
        .filter_map(|statement| {
            let rest = statement.trim().strip_prefix("let ")?;
            let (name, value) = rest.split_once('=')?;
            let name = name.trim();
            let name = match name.strip_prefix("mut ") {
                Some(name) => name.trim(),
                None => name,
            };
            let (name, annotation) = match name.split_once(':') {
                Some((name, ty)) => (name.trim(), Some(ty.trim().to_string())),
                None => (name, None),
            };
            is_identifier(name).then(|| (name.to_string(), value.trim().to_string(), annotation))
        })
        .collect()
}

/// `(name, type)` for each plain `name: Type` parameter of the function
/// whose body text this is. `self`, patterns and unparsable items
/// contribute nothing.
fn owner_param_types(text: &str) -> Vec<(String, String)> {
    let Some(at) = text.find("fn ") else {
        return Vec::new();
    };
    let Some(open) = text[at..].find('(') else {
        return Vec::new();
    };
    let open = open + at;
    let Some(close) = matching_paren(text, open) else {
        return Vec::new();
    };
    top_level_items(&text[open + 1..close])
        .into_iter()
        .filter_map(|item| {
            let (name, ty) = item.split_once(':')?;
            let name = name.trim();
            let name = match name.strip_prefix("mut ") {
                Some(name) => name.trim(),
                None => name,
            };
            is_identifier(name).then(|| (name.to_string(), ty.trim().to_string()))
        })
        .collect()
}

/// `text` without the rest of any line holding a `//` comment.
fn strip_line_comments(text: &str) -> String {
    text.lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `expr` is an arithmetic combination of established names, numeric or
/// bool literals, and `as` casts to primitive types, using only the
/// accepted operators (with unary `-`, `!`, `&`). A word directly followed
/// by `(` is a call and proves nothing; any other character shape — a
/// stray `.`, a path, an index, a macro — refuses.
fn arithmetic_established(expr: &str, established: &[String]) -> bool {
    let bytes = expr.as_bytes();
    if !expr.is_ascii() {
        return false;
    }
    let mut depth = 0i32;
    let mut at = 0usize;
    let mut expecting_cast = false;
    while at < bytes.len() {
        match bytes[at] {
            byte if byte.is_ascii_whitespace() => at += 1,
            b'0'..=b'9' => {
                if expecting_cast {
                    return false;
                }
                expecting_cast = false;
                at += 1;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                // One fractional dot between digits is part of a float
                // literal; any other `.` refuses.
                if at < bytes.len() && bytes[at] == b'.' {
                    if at + 1 < bytes.len()
                        && bytes[at + 1].is_ascii_digit()
                        && bytes[at - 1].is_ascii_digit()
                    {
                        at += 1;
                        while at < bytes.len()
                            && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_')
                        {
                            at += 1;
                        }
                    } else {
                        return false;
                    }
                }
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = at;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                let word = &expr[start..at];
                if expecting_cast {
                    if !primitive_type_token(word) {
                        return false;
                    }
                    expecting_cast = false;
                } else if word == "as" {
                    expecting_cast = true;
                } else if word != "true"
                    && word != "false"
                    && !established.iter().any(|name| name == word)
                {
                    return false;
                }
                if expr[at..].trim_start().starts_with('(') {
                    return false;
                }
            }
            b'(' => {
                depth += 1;
                expecting_cast = false;
                at += 1;
            }
            b')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
                at += 1;
            }
            b'+' | b'-' | b'*' | b'/' | b'%' | b'|' | b'^' | b'&' | b'!' => {
                expecting_cast = false;
                at += 1;
            }
            _ => return false,
        }
    }
    depth == 0 && !expecting_cast
}

/// The other `field: operand` initializers of the struct literal that holds
/// `expression` in `body`, each with a plain identifier value. Shorthand
/// `operand,` names a field of the same name.
fn sibling_initializers<'a>(body: &'a str, expression: &str) -> Option<Vec<(&'a str, &'a str)>> {
    let at = body.find(expression.trim())?;
    if body[at + 1..].contains(expression.trim()) {
        return None;
    }
    let open = enclosing_open_brace(body, at)?;
    let close = matching_close_brace(body, open)?;
    let inner = &body[open + 1..close];
    // Commas inside a string or comment (`note: "x, subtotal_cents: subtotal"`)
    // would split into fabricated initializers, so such a literal is not read.
    if inner.contains(['"', '\'']) || inner.contains("//") || inner.contains("/*") {
        return None;
    }
    let mut siblings = Vec::new();
    for item in top_level_items(inner) {
        let (name, value) = match item.split_once(':') {
            Some((name, value)) => (name.trim(), value.trim()),
            None => (item, item),
        };
        if is_identifier(name) && is_identifier(value) {
            siblings.push((name, value));
        }
    }
    Some(siblings)
}

/// The `{` that opens the innermost brace group containing `at`.
fn enclosing_open_brace(body: &str, at: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in body.as_bytes()[..at].iter().enumerate().rev() {
        match byte {
            b'}' => depth += 1,
            b'{' if depth == 0 => return Some(offset),
            b'{' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn matching_close_brace(body: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in body.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Comma-separated items of `text` at bracket depth zero, trimmed and
/// non-empty.
fn top_level_items(text: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (offset, byte) in text.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                items.push(text[start..offset].trim());
                start = offset + 1;
            }
            _ => {}
        }
    }
    items.push(text[start..].trim());
    items.into_iter().filter(|item| !item.is_empty()).collect()
}

/// `(receiver, field, expected)` for each exact `assert_eq!(receiver.field,
/// expected)` in `test`, either argument order.
fn exact_field_pins(test: &TestSummary) -> Vec<(String, String, String)> {
    test.assertions
        .iter()
        .filter(|assertion| assertion.kind == OracleKind::ExactValue)
        .filter_map(|assertion| exact_field_pins_of(&assertion.text))
        .collect()
}

/// One `assert_eq!(receiver.field, expected)` with exactly two arguments
/// and a literal expected side: an integer (digit separators removed), a
/// string, a char or a bool. A non-literal expected side (`it.next()`,
/// `expected`) may differ between two pins with the same text.
fn exact_field_pins_of(text: &str) -> Option<(String, String, String)> {
    let inner = text
        .trim()
        .strip_prefix("assert_eq!(")?
        .trim_end_matches(';')
        .strip_suffix(')')?;
    let items = top_level_items(inner);
    let [first, second] = items.as_slice() else {
        return None;
    };
    let (receiver, field, expected) = field_read(first)
        .map(|(receiver, field)| (receiver, field, *second))
        .or_else(|| field_read(second).map(|(receiver, field)| (receiver, field, *first)))?;
    let expected = literal_value(expected)?;
    Some((receiver.to_string(), field.to_string(), expected))
}

fn literal_value(text: &str) -> Option<String> {
    let text = text.trim();
    let digits = text.strip_prefix('-').unwrap_or(text);
    if !digits.is_empty()
        && digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_')
    {
        return Some(text.replace('_', ""));
    }
    let quoted = text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')));
    (quoted || text == "true" || text == "false").then(|| text.to_string())
}

/// Occurrences of `word` in `text` not joined to an identifier character on
/// either side.
fn whole_word_count(text: &str, word: &str) -> usize {
    let is_ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    text.match_indices(word)
        .filter(|(at, _)| {
            !text[..*at].chars().next_back().is_some_and(is_ident)
                && !text[at + word.len()..].chars().next().is_some_and(is_ident)
        })
        .count()
}

/// Whether every statement of `body` is `let <receiver> = ..` for one of
/// `receivers`, or an assertion macro that names no `owner_name` and uses each
/// receiver only as a plain field read. Line comments are dropped; anything
/// else (a helper call, another binding, an attribute) returns `false`.
fn only_bindings_and_field_reads(body: &str, receivers: &[&str], owner_name: &str) -> bool {
    let text = body
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n");
    let inner = match text.find('{') {
        Some(open) if text[..open].contains("fn ") => text[open + 1..]
            .trim_end()
            .strip_suffix('}')
            .unwrap_or(&text[open + 1..]),
        _ => text.as_str(),
    };
    inner
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .all(|statement| {
            if let Some(rest) = statement.strip_prefix("let ") {
                // The whole statement, so a transform chained on the next
                // line (`let q = quote(2_500, 4)\n    .with_coupon(500)`)
                // is seen too.
                let Some((name, value)) = rest.split_once('=') else {
                    return false;
                };
                let name = name.split(':').next().unwrap_or_default().trim();
                return receivers.contains(&name) && is_bare_owner_call(value, owner_name);
            }
            // An assertion macro whose own parenthesis is the only bracket: a
            // helper inside it (`assert_eq!(make_quote(1_000, 1), ..)`,
            // `make_quote![..]`) or a helper named `assert_*` may run the
            // owner and see the operand. Only the standard assertion macros
            // are read; a custom `assert_quote_ok!(..)` may call the owner.
            let macro_name_len = statement
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .unwrap_or(statement.len());
            matches!(
                &statement[..macro_name_len],
                "assert"
                    | "assert_eq"
                    | "assert_ne"
                    | "debug_assert"
                    | "debug_assert_eq"
                    | "debug_assert_ne"
            ) && statement[macro_name_len..].starts_with("!(")
                && statement.matches(['(', '[', '{']).count() == 1
                && whole_word_count(statement, owner_name) == 0
                && !receivers
                    .iter()
                    .any(|receiver| uses_more_than_field_reads(statement, receiver))
        })
}

/// Whether `text` uses `receiver` other than as a plain field read
/// (`q.total_cents`): the whole value, a method call, or a reference.
fn uses_more_than_field_reads(text: &str, receiver: &str) -> bool {
    let is_ident = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    text.match_indices(receiver).any(|(at, _)| {
        if text[..at].chars().next_back().is_some_and(is_ident) {
            return false;
        }
        let rest = &text[at + receiver.len()..];
        if rest.chars().next().is_some_and(is_ident) {
            return false;
        }
        let Some(after_dot) = rest.strip_prefix('.') else {
            return true;
        };
        let name_len = after_dot
            .find(|ch: char| !is_ident(ch))
            .unwrap_or(after_dot.len());
        name_len == 0 || after_dot[name_len..].trim_start().starts_with(['(', ':'])
    })
}

/// `q.total_cents` -> `("q", "total_cents")`.
fn field_read(text: &str) -> Option<(&str, &str)> {
    let (receiver, field) = text.trim().split_once('.')?;
    (is_identifier(receiver) && is_identifier(field)).then_some((receiver, field))
}

/// Whether `body` binds `receiver` exactly once, straight from a call to
/// `owner_name`: `let q = quote(..)` or `let q = crate::quote(..)`. A second
/// `let q` may shadow the owner's value (`let q = q.with_coupon(..)`).
fn bound_once_from_owner_call(body: &str, receiver: &str, owner_name: &str) -> bool {
    let bindings = body
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("let ")?;
            let (name, value) = rest.split_once('=')?;
            let name = name.trim();
            let name = name.split(':').next().unwrap_or(name).trim();
            (name == receiver).then_some(value)
        })
        .collect::<Vec<_>>();
    let [value] = bindings.as_slice() else {
        return false;
    };
    let value = value.split_once("//").map_or(*value, |(code, _)| code);
    is_bare_owner_call(value, owner_name)
}

/// Whether `value` is exactly one call to `owner_name` (`quote(..)`,
/// `crate::quote(..)`), with nothing chained after it.
fn is_bare_owner_call(value: &str, owner_name: &str) -> bool {
    let value = value.trim().trim_end_matches(';').trim_end();
    let Some(open) = value.find('(') else {
        return false;
    };
    // The call must be the whole initializer: a chained transform
    // (`quote(100, 1).with_coupon(50)`) can make the pins agree without the
    // owner's own result doing so.
    if matching_paren(value, open) != Some(value.len() - 1) {
        return false;
    }
    let callee = value[..open].trim();
    // A path through modules only (`crate::quote`, `pricing::quote`); a type
    // segment (`Other::quote`) names a different function.
    callee
        .rsplit_once("::")
        .map_or(callee == owner_name, |(path, name)| {
            name == owner_name
                && path.split("::").all(|segment| {
                    segment
                        .trim()
                        .starts_with(|ch: char| ch.is_ascii_lowercase())
                })
        })
}

/// The byte index of the `)` closing the `(` at `open`.
fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + at);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `body` holds a construct that may skip or repeat a statement:
/// a condition, a match, a loop, a closure, an early exit, `?` or a
/// short-circuit `&&`/`||`. Comments and strings count too, which only
/// refuses more.
fn has_control_flow(body: &str) -> bool {
    [
        "if", "match", "for", "while", "loop", "return", "break", "continue",
    ]
    .iter()
    .any(|word| whole_word_count(body, word) > 0)
        || body.contains(['?', '|'])
        || body.contains("&&")
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::{FunctionSourceRole, OracleFact, OwnedRustIndex};
    use crate::analysis::rust_index::{FileFacts, FunctionFact};
    use crate::domain::{OracleStrength, SymbolId};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const OWNER: &str = "pub fn quote(unit: u64, quantity: u64) -> Quote {
    let subtotal = unit * quantity;
    let shipping = shipping_cents(subtotal);
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";

    /// An index holding one file whose functions carry `bodies` verbatim,
    /// the way the parser producer records signatures in `body`.
    fn index_with(bodies: &[&str]) -> RustIndex {
        let path = PathBuf::from("src/lib.rs");
        let functions: Vec<FunctionFact> = bodies
            .iter()
            .enumerate()
            .map(|(offset, body)| FunctionFact {
                id: SymbolId(format!("lib::{}::{offset}", fn_name_of(body))),
                name: fn_name_of(body).to_string(),
                file: path.clone(),
                start_line: 1,
                end_line: 1,
                body: (*body).into(),
                calls: Vec::new(),
                returns: Vec::new(),
                literals: Vec::new(),
                source_role: FunctionSourceRole::Production,
                attrs: Vec::new(),
                impl_attrs: Vec::new(),
                nested_fn_names: Vec::new(),
                let_bindings: Vec::new(),
                item: Default::default(),
                impl_context: Default::default(),
            })
            .collect();
        RustIndex::from_owned(OwnedRustIndex {
            files: BTreeMap::from([(
                path.clone(),
                FileFacts {
                    path: path.clone(),
                    functions: functions.clone(),
                    source: "".into(),
                    ..FileFacts::default()
                },
            )]),
            functions,
            ..Default::default()
        })
    }

    fn fn_name_of(body: &str) -> &str {
        let at = body.find("fn ").unwrap_or(0);
        let rest = &body[at + 3..];
        let end = rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(rest.len());
        &rest[..end]
    }

    fn test_with(body_lines: &[&str]) -> TestSummary {
        TestSummary {
            name: "quote_test".to_string(),
            body: body_lines.join("\n").into(),
            assertions: body_lines
                .iter()
                .filter(|line| line.trim_start().starts_with("assert"))
                .map(|line| OracleFact {
                    line: 1,
                    text: line.trim().to_string(),
                    kind: OracleKind::ExactValue,
                    strength: OracleStrength::Strong,
                    observed_tokens: Vec::new(),
                    ok_value_observed: None,
                })
                .collect(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: body_lines.len(),
            calls: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn pin_with(index: &RustIndex, tests: &[TestSummary]) -> Option<OperandOnlyPin> {
        let refs = tests
            .iter()
            .map(|test| (test, RelationReason::DirectOwnerCall))
            .collect::<Vec<_>>();
        operand_only_pin(
            "total_cents: subtotal + shipping",
            "quote",
            OWNER,
            &refs,
            index,
        )
    }

    fn pin(tests: &[TestSummary]) -> Option<OperandOnlyPin> {
        let index = index_with(&["fn shipping_cents(cents: u64) -> u64 { 499 }"]);
        pin_with(&index, tests)
    }

    #[test]
    fn a_total_pinned_equal_to_its_subtotal_names_the_unseen_operand() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9000);",
        ])]);

        assert_eq!(
            found,
            Some(OperandOnlyPin {
                field: "total_cents".to_string(),
                sibling: "subtotal_cents".to_string(),
                unseen_operand: "shipping".to_string(),
            })
        );
    }

    #[test]
    fn different_pinned_values_discriminate_the_operand() {
        let found = pin(&[test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.subtotal_cents, 1_000);",
            "assert_eq!(q.total_cents, 1_499);",
        ])]);

        assert_eq!(found, None);
    }

    #[test]
    fn one_unpaired_pinning_test_keeps_the_credit() {
        let found = pin(&[
            test_with(&[
                "let q = quote(2_500, 4);",
                "assert_eq!(q.subtotal_cents, 9_000);",
                "assert_eq!(q.total_cents, 9_000);",
            ]),
            test_with(&[
                "let q = quote(1_000, 1);",
                "assert_eq!(q.total_cents, 1_499);",
            ]),
        ]);

        assert_eq!(found, None);
    }

    #[test]
    fn a_receiver_not_bound_from_the_owner_is_not_read() {
        let found = pin(&[test_with(&[
            "let q = other(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);

        assert_eq!(found, None);
    }

    #[test]
    fn no_test_pinning_the_field_is_not_this_rule() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
        ])]);

        assert_eq!(found, None);
    }

    /// Review of #7077: a second receiver in the same test that pins the
    /// field to a different value discriminates the dropped operand.
    #[test]
    fn a_second_receiver_pinning_the_field_keeps_the_credit() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "let r = quote(1_000, 1);",
            "assert_eq!(r.total_cents, 1_499);",
        ])]);

        assert_eq!(found, None);
    }

    /// Review of #7077: a pin ripr cannot parse (a custom message) still
    /// mentions the field, so the rule fails closed.
    #[test]
    fn an_unread_pin_on_the_field_keeps_the_credit() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let with_message = test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.total_cents, 1_499, \"shipping added\");",
        ]);
        let read_out = test_with(&[
            "let total = quote(1_000, 1).total_cents;",
            "assert_eq!(total, 1_499);",
        ]);

        assert_eq!(pin(&[paired.clone(), with_message]), None);
        assert_eq!(pin(&[paired, read_out]), None);
    }

    #[test]
    fn a_shadowed_receiver_is_not_read() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "let q = q.with_coupon(5);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);

        assert_eq!(found, None);
    }

    #[test]
    fn a_non_literal_expected_side_is_not_read() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, expected);",
            "assert_eq!(q.total_cents, expected);",
        ])]);

        assert_eq!(found, None);
    }

    /// One test sees the total equal the subtotal and another sees it equal
    /// the shipping: each dropped operand is caught by one of them.
    #[test]
    fn tests_pairing_different_operands_keep_the_credit() {
        let found = pin(&[
            test_with(&[
                "let q = quote(2_500, 4);",
                "assert_eq!(q.subtotal_cents, 9_000);",
                "assert_eq!(q.total_cents, 9_000);",
            ]),
            test_with(&[
                "let q = quote(0, 4);",
                "assert_eq!(499, q.shipping_cents);",
                "assert_eq!(499, q.total_cents);",
            ]),
        ]);

        assert_eq!(found, None);
    }

    /// Re-review of #7077: a test that runs the owner and observes the
    /// whole result never names the field, yet may see the dropped operand.
    #[test]
    fn a_whole_result_observer_keeps_the_credit() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let whole = test_with(&["assert_eq!(quote(1_000, 1), expected_quote());"]);
        let unrelated = test_with(&["assert_eq!(discounted(10_000), 9_000);"]);

        assert_eq!(pin(&[paired.clone(), whole]), None);
        let index = index_with(&["fn shipping_cents(cents: u64) -> u64 { 499 }"]);
        assert!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                OWNER,
                &[
                    (&paired, RelationReason::DirectOwnerCall),
                    (&unrelated, RelationReason::SameModule),
                ],
                &index,
            )
            .is_some()
        );
    }

    /// Codex review of #7084: a sibling pin that may not run cannot show
    /// the field equals the operand.
    #[test]
    fn a_conditional_sibling_pin_keeps_the_credit() {
        let mut gated = test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.total_cents, 1_499);",
            "if false { assert_eq!(q.subtotal_cents, 1_499); }",
        ]);
        let mut cfg_gated = test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.total_cents, 1_499);",
            "#[cfg(any())] assert_eq!(q.subtotal_cents, 1_499);",
        ]);
        let mut short_circuit = test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.total_cents, 1_499);",
            "let _ = false && { assert_eq!(q.subtotal_cents, 1_499); true };",
        ]);
        // The parser records the gated pin as an assertion; `test_with`
        // only collects lines that start with `assert`.
        for test in [&mut gated, &mut cfg_gated, &mut short_circuit] {
            let mut fact = test.assertions[0].clone();
            fact.text = "assert_eq!(q.subtotal_cents, 1_499);".to_string();
            test.assertions.push(fact);
        }
        let ungated = test_with(&[
            "let q = quote(1_000, 1);",
            "assert_eq!(q.total_cents, 1_499);",
            "assert_eq!(q.subtotal_cents, 1_499);",
        ]);

        assert!(pin(&[ungated]).is_some());
        assert_eq!(pin(&[gated]), None);
        assert_eq!(pin(&[cfg_gated]), None);
        assert_eq!(pin(&[short_circuit]), None);
    }

    /// Codex review of #7084: a test related through a helper that calls
    /// the owner may check the whole result without naming the owner; one
    /// related only through proximity does not block the rule.
    #[test]
    fn a_helper_related_whole_result_check_keeps_the_credit() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let through_helper = test_with(&["assert_eq!(make_quote(), expected());"]);
        let index = index_with(&["fn shipping_cents(cents: u64) -> u64 { 499 }"]);
        let check = |reason| {
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                OWNER,
                &[
                    (&paired, RelationReason::DirectOwnerCall),
                    (&through_helper, reason),
                ],
                &index,
            )
        };

        let hands_to_helper = test_with(&["check_quote(quote(2_500, 4));"]);
        let without_assertions = operand_only_pin(
            "total_cents: subtotal + shipping",
            "quote",
            OWNER,
            &[
                (&paired, RelationReason::DirectOwnerCall),
                (&hands_to_helper, RelationReason::DirectOwnerCall),
            ],
            &index,
        );

        assert_eq!(check(RelationReason::HelperOwnerCall), None);
        assert!(check(RelationReason::SameModule).is_some());
        assert_eq!(without_assertions, None);
    }

    /// CodeRabbit review of #7084: the pinning test itself may also check
    /// the whole result, which sees the operand the pins leave out.
    #[test]
    fn a_pinning_test_that_also_checks_the_whole_result_keeps_the_credit() {
        let found = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert_eq!(q, expected_quote());",
        ])]);
        let method = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert!(q.is_consistent());",
        ])]);
        let other_field = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.tier, Tier::Gold);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);

        let helper = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "check_quote(&q);",
        ])]);
        let second_result = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "let r = quote(1_000, 1);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert_eq!(r, expected_r());",
        ])]);
        let in_fn = pin(&[test_with(&[
            "fn quote_test() {",
            "    let q = quote(2_500, 4); // gold",
            "    assert_eq!(q.subtotal_cents, 9_000);",
            "    assert_eq!(q.total_cents, 9_000);",
            "}",
        ])]);

        assert_eq!(found, None);
        assert_eq!(method, None);
        let helper_in_assertion = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert_eq!(make_quote(1_000, 1), expected_quote());",
        ])]);
        let owner_in_assertion = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert_eq!(quote(1_000, 1).tier, Tier::Standard);",
        ])]);
        let transformed = pin(&[test_with(&[
            "let q = quote(2_500, 4).with_coupon(500);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);
        let transformed_next_line = pin(&[test_with(&[
            "let q = quote(2_500, 4)",
            "    .with_coupon(500);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);
        let assert_named_helper = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert_quote_total(1_000, 1);",
        ])]);
        let custom_macro = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert_quote_ok!(1_000, 1);",
        ])]);
        let other_type = pin(&[test_with(&[
            "let q = Other::quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);
        let module_path = pin(&[test_with(&[
            "let q = crate::quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ])]);
        let bracket_macro = pin(&[test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
            "assert_eq!(make_quote![1_000, 1].tier, Tier::Standard);",
        ])]);

        assert_eq!(helper, None);
        assert_eq!(helper_in_assertion, None);
        assert_eq!(owner_in_assertion, None);
        assert_eq!(transformed, None);
        assert_eq!(transformed_next_line, None);
        assert_eq!(assert_named_helper, None);
        assert_eq!(bracket_macro, None);
        assert_eq!(custom_macro, None);
        assert_eq!(other_type, None);
        assert!(module_path.is_some());
        assert_eq!(second_result, None);
        assert!(other_field.is_some());
        assert!(in_fn.is_some());
    }

    /// Codex review of #7084: text inside a string or comment in the struct
    /// literal is not an initializer.
    #[test]
    fn a_string_or_comment_in_the_literal_is_not_read() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let owner_with = |extra: &str| {
            format!(
                "pub fn quote(unit: u64, quantity: u64) -> Quote {{
    let subtotal = unit * quantity;
    let shipping = 499;
    Quote {{
        subtotal_cents: displayed,{extra}
        total_cents: subtotal + shipping,
    }}
}}"
            )
        };
        let index = index_with(&["fn shipping_cents(cents: u64) -> u64 { 499 }"]);
        let check = |owner: &str| {
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                owner,
                &[(&paired, RelationReason::DirectOwnerCall)],
                &index,
            )
        };

        assert_eq!(
            check(&owner_with(" note: \"x, subtotal_cents: subtotal, y\",")),
            None
        );
        assert_eq!(
            check(&owner_with(" // , subtotal_cents: subtotal,\n")),
            None
        );
        assert!(check(OWNER).is_some());
    }

    /// Codex review of #7084: an operand of a custom type can overload the
    /// operator, and an overloaded operator may carry side effects that
    /// assertions on other fields observe, so its binding must prove a
    /// primitive type before the downgrade.
    #[test]
    fn an_overloaded_operator_operand_keeps_the_credit() {
        let owner = "pub fn quote(unit: u64, quantity: u64) -> Quote {
    let subtotal = unit * quantity;
    let shipping = shipping_money(subtotal);
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let run = |index: &RustIndex| {
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                owner,
                &refs,
                index,
            )
        };

        let custom = index_with(&["fn shipping_money(cents: u64) -> Money { Money(499) }"]);
        assert_eq!(run(&custom), None);
        let absent = RustIndex::default();
        assert_eq!(run(&absent), None);
        let disagreeing = index_with(&[
            "fn shipping_money(cents: u64) -> u64 { 499 }",
            "fn shipping_money(cents: u64) -> u32 { 499 }",
        ]);
        assert_eq!(run(&disagreeing), None);
        let agreeing = index_with(&[
            "fn shipping_money(cents: u64) -> u64 { 499 }",
            "fn shipping_money(cents: u64) -> u64 { 500 }",
        ]);
        assert!(run(&agreeing).is_some());
    }

    /// A parameter of a custom type overloads the operator just the same.
    #[test]
    fn a_custom_typed_parameter_keeps_the_credit() {
        let owner = "pub fn quote(subtotal: Money, shipping: Money) -> Quote {
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";
        let paired = test_with(&[
            "let q = quote(Money(2_500), Money(4));",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let index = RustIndex::default();

        assert_eq!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                owner,
                &refs,
                &index,
            ),
            None
        );
    }

    #[test]
    fn a_non_primitive_callee_return_keeps_the_credit() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let tier = index_with(&["fn shipping_cents(cents: u64) -> Tier { Tier::Gold }"]);
        let result = index_with(&["fn shipping_cents(cents: u64) -> Result<u64, E> { Ok(499) }"]);

        assert_eq!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                OWNER,
                &refs,
                &tier,
            ),
            None
        );
        assert_eq!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                OWNER,
                &refs,
                &result,
            ),
            None
        );
    }

    #[test]
    fn a_literal_and_param_chain_proves_without_calls() {
        let owner = "pub fn quote(unit: u64, quantity: u64) -> Quote {
    let subtotal = unit * quantity;
    let shipping = 499;
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let index = RustIndex::default();

        assert!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                owner,
                &refs,
                &index,
            )
            .is_some()
        );
    }

    #[test]
    fn a_primitive_annotation_proves_the_binding() {
        let owner = "pub fn quote(unit: u64, quantity: u64) -> Quote {
    let subtotal = unit * quantity;
    let shipping: u64 = opaque();
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let index = RustIndex::default();

        assert!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                owner,
                &refs,
                &index,
            )
            .is_some()
        );
    }

    /// The annotation owns the type when present: `let s: Money = 499;`
    /// binds a custom value even though the initializer reads as a literal.
    #[test]
    fn a_non_primitive_annotation_keeps_the_credit() {
        let owner = "pub fn quote(unit: u64, quantity: u64) -> Quote {
    let subtotal = unit * quantity;
    let shipping: Money = 499;
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let index = RustIndex::default();

        assert_eq!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                owner,
                &refs,
                &index,
            ),
            None
        );
    }

    #[test]
    fn a_reference_primitive_return_proves_the_operand() {
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let reference = index_with(&["fn shipping_cents(cents: u64) -> &u64 { &499 }"]);

        assert!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                OWNER,
                &refs,
                &reference,
            )
            .is_some()
        );
    }

    #[test]
    fn an_arithmetic_mix_with_a_call_keeps_the_credit() {
        let owner = "pub fn quote(unit: u64, quantity: u64) -> Quote {
    let subtotal = unit * quantity;
    let shipping = 1 + shipping_cents(subtotal);
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let index = index_with(&["fn shipping_cents(cents: u64) -> u64 { 499 }"]);

        assert_eq!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                owner,
                &refs,
                &index,
            ),
            None
        );
    }

    /// A string anywhere in the owner body could hide a binding or a type
    /// this proof would misread, so such an owner keeps the credit.
    #[test]
    fn a_string_anywhere_in_the_owner_keeps_the_credit() {
        let owner = "pub fn quote(unit: u64, quantity: u64) -> Quote {
    let note = format!(\"{}\", 499);
    let subtotal = unit * quantity;
    let shipping = 499;
    Quote {
        subtotal_cents: subtotal,
        shipping_cents: shipping,
        total_cents: subtotal + shipping,
    }
}";
        let paired = test_with(&[
            "let q = quote(2_500, 4);",
            "assert_eq!(q.subtotal_cents, 9_000);",
            "assert_eq!(q.total_cents, 9_000);",
        ]);
        let refs = vec![(&paired, RelationReason::DirectOwnerCall)];
        let index = RustIndex::default();

        assert_eq!(
            operand_only_pin(
                "total_cents: subtotal + shipping",
                "quote",
                owner,
                &refs,
                &index,
            ),
            None
        );
    }

    #[test]
    fn only_a_plain_binary_initializer_is_read() {
        assert_eq!(
            binary_field_initializer("total: a + b"),
            Some(("total", "a", "b"))
        );
        assert_eq!(binary_field_initializer("total: a + b + c"), None);
        assert_eq!(binary_field_initializer("total: f(a) + b"), None);
        assert_eq!(binary_field_initializer("total: a + a"), None);
        assert_eq!(binary_field_initializer("total: a::b + c"), None);
    }
}
