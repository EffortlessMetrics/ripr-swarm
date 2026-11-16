//! JSON-equivalence and response-read helpers for the route-response
//! credit (#13869). A child of `sink_alignment` so its private helpers stay
//! shared.

use super::*;

/// The unquoted keys of a dict-literal change whose old and new values
/// a Python comparison of the JSON body may not tell apart (#13869): one
/// string in either quote style, numbers or numeric expressions of one value
/// (`5`, `5.0`, `1_0` and `10`, `1e1`, `0` and `-0.0`, `1` and `True`, `(1)`,
/// `10**2` and `100`), lists of such values, and any value whose form
/// [`json_normal_form`] cannot establish (a tuple, a dict, a name, a prefixed
/// or adjacent string), which fails closed.
pub(super) fn json_equivalent_keys(old_line: Option<&str>, new_line: &str) -> Vec<String> {
    let (Some(old), Some(new)) = (
        old_line.and_then(parse_dict_literal_fields),
        parse_dict_literal_fields(new_line),
    ) else {
        return Vec::new();
    };
    new.iter()
        .filter_map(|(key, new_value)| {
            let (_, old_value) = old.iter().find(|(k, _)| k == key)?;
            let (old_form, new_form) = (json_normal_form(old_value), json_normal_form(new_value));
            (old_value != new_value
                && (old_form.is_none() || new_form.is_none() || old_form == new_form))
                .then(|| key.trim_matches(|c| c == '"' || c == '\'').to_string())
        })
        .collect()
}

/// A comparison-level normal form of a Python literal's JSON body (#13869),
/// or `None` when the value is not established (the caller then treats the
/// key as unchanged, failing closed).
///
/// A value made only of numbers, `+ - * / // % **` and parentheses is
/// evaluated (`200 - 10` reads 190, `(1)` reads 1, `10**2` reads 100). Any
/// other value is read as a literal: whitespace is dropped, a trailing comma
/// before `]` is dropped, each number token is replaced by its value
/// (`True`/`False` as 1/0, underscores ignored, exponents, hex/octal/binary
/// integers within `i64`) and each string is rewritten in double quotes. A
/// string with a prefix or an escape, adjacent strings, a
/// tuple or dict, a name or any other operator is not established.
fn json_normal_form(value: &str) -> Option<String> {
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    // A plain integer is compared exactly (`literal_scalar`), not through the
    // `f64` arithmetic below, which merges integers past 2^53.
    // A bounded length bounds the evaluator's recursion; a longer value is
    // not established.
    if compact.chars().count() > 256 {
        return None;
    }
    let unsigned = compact.strip_prefix(['-', '+']).unwrap_or(&compact);
    if unsigned.starts_with(|ch: char| ch.is_ascii_digit())
        && unsigned.chars().all(|ch| ch.is_ascii_digit() || ch == '_')
    {
        return literal_scalar(&compact);
    }
    if !compact.is_empty()
        && compact
            .chars()
            .all(|ch| ch.is_ascii_digit() || "._+-*/%()eE".contains(ch))
    {
        let chars: Vec<char> = compact.chars().collect();
        let mut at = 0;
        let number = arithmetic_sum(&chars, &mut at)?;
        if at != chars.len() {
            return None;
        }
        return canonical_number(number);
    }
    let mut normal = String::new();
    let mut token = String::new();
    // The source text, not `compact`: whitespace inside a string is part
    // of its value, and is dropped only between tokens.
    let mut chars = value.trim().chars();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '+' | '-') {
            token.push(ch);
            continue;
        }
        if !token.is_empty() {
            // A token right before a quote is a string prefix (`r`, `f`, `b`).
            if matches!(ch, '"' | '\'') {
                return None;
            }
            normal.push_str(&literal_scalar(&token)?);
            token.clear();
        }
        match ch {
            '"' | '\'' => {
                if normal.ends_with('"') {
                    return None;
                }
                let mut closed = false;
                normal.push('"');
                for inner in chars.by_ref() {
                    if inner == ch {
                        closed = true;
                        break;
                    }
                    // An escape is not established; the other quote is plain
                    // content, escaped in the double-quoted form.
                    match inner {
                        '\\' => return None,
                        '"' => normal.push_str("\\\""),
                        _ => normal.push(inner),
                    }
                }
                if !closed {
                    return None;
                }
                normal.push('"');
            }
            '[' | ',' => normal.push(ch),
            ']' => {
                // A string always ends in its closing quote, so a comma at
                // the end here is a structural trailing comma.
                if normal.ends_with(',') {
                    normal.pop();
                }
                normal.push(']');
            }
            ch if ch.is_whitespace() => {}
            _ => return None,
        }
    }
    if !token.is_empty() {
        normal.push_str(&literal_scalar(&token)?);
    }
    Some(normal)
}

/// A scalar literal token's canonical text: a number's value, or `None` /
/// `null` for Python's `None`; any other name is not established.
fn literal_scalar(token: &str) -> Option<String> {
    let digits = token.replace('_', "");
    let (negative, magnitude) = match digits.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, digits.strip_prefix('+').unwrap_or(&digits)),
    };
    let radix_integer = [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ]
    .iter()
    .find_map(|(prefix, radix)| {
        magnitude.strip_prefix(prefix).map(|rest| {
            rest.chars()
                .all(|ch| ch.is_ascii_alphanumeric())
                .then(|| i128::from_str_radix(rest, *radix).ok())
                .flatten()
        })
    });
    let integer = match token {
        "None" => return Some("null".to_string()),
        "True" => Some(1),
        "False" => Some(0),
        _ => match radix_integer {
            Some(parsed) => Some(parsed?),
            None if !magnitude.is_empty() && magnitude.chars().all(|ch| ch.is_ascii_digit()) => {
                Some(magnitude.parse::<i128>().ok()?)
            }
            None => None,
        },
    };
    if let Some(integer) = integer {
        let integer = if negative {
            integer.checked_neg()?
        } else {
            integer
        };
        // Python compares integers exactly; beyond 2^53 a float would merge
        // neighbours, so the digits themselves are the form.
        return if integer.unsigned_abs() < EXACT_FLOAT_INTEGER {
            canonical_number(integer as f64)
        } else {
            Some(integer.to_string())
        };
    }
    if !magnitude.starts_with(|ch: char| ch.is_ascii_digit() || ch == '.') {
        return None;
    }
    canonical_number(digits.parse::<f64>().ok()?)
}

/// 2^53: below it every integer has an exact `f64`.
const EXACT_FLOAT_INTEGER: u128 = 1 << 53;

/// A finite number's canonical text (signed zero reads `0`), or `None` at or
/// beyond 2^53, where distinct values may share one `f64` (fail closed).
fn canonical_number(number: f64) -> Option<String> {
    if !number.is_finite() || number.abs() >= EXACT_FLOAT_INTEGER as f64 {
        return None;
    }
    Some(if number == 0.0 {
        "0".to_string()
    } else {
        format!("{number}")
    })
}

/// Whether an arithmetic value is exact in `f64` (below 2^53), checked at
/// every step so an intermediate past it cannot round a result.
fn exact(value: f64) -> Option<f64> {
    (value.is_finite() && value.abs() < EXACT_FLOAT_INTEGER as f64).then_some(value)
}

/// `sum := product (('+' | '-') product)*` over a whitespace-free numeric
/// expression, with Python's precedence: `**` binds tighter than a unary
/// sign on its left (`-2**2` is -4) and looser on its right (`2**-1`).
fn arithmetic_sum(chars: &[char], at: &mut usize) -> Option<f64> {
    let mut value = arithmetic_product(chars, at)?;
    while let Some(&op) = chars.get(*at) {
        if !matches!(op, '+' | '-') {
            break;
        }
        *at += 1;
        let rhs = arithmetic_product(chars, at)?;
        value = exact(if op == '+' { value + rhs } else { value - rhs })?;
    }
    Some(value)
}

/// `product := unary (('*' | '/' | '//' | '%') unary)*`.
fn arithmetic_product(chars: &[char], at: &mut usize) -> Option<f64> {
    let mut value = arithmetic_unary(chars, at)?;
    loop {
        let op = match (chars.get(*at), chars.get(*at + 1)) {
            (Some('*'), Some('*')) => return None,
            (Some('/'), Some('/')) => "//",
            (Some('*'), _) => "*",
            (Some('/'), _) => "/",
            (Some('%'), _) => "%",
            _ => break,
        };
        *at += op.len();
        let rhs = arithmetic_unary(chars, at)?;
        if matches!(op, "/" | "//" | "%") && rhs == 0.0 {
            return None;
        }
        value = exact(match op {
            "*" => value * rhs,
            "/" => value / rhs,
            "//" => (value / rhs).floor(),
            _ => value - rhs * (value / rhs).floor(),
        })?;
    }
    Some(value)
}

/// `unary := ('+' | '-') unary | power`.
fn arithmetic_unary(chars: &[char], at: &mut usize) -> Option<f64> {
    match chars.get(*at) {
        Some('-') => {
            *at += 1;
            arithmetic_unary(chars, at).map(|value| -value)
        }
        Some('+') => {
            *at += 1;
            arithmetic_unary(chars, at)
        }
        _ => arithmetic_power(chars, at),
    }
}

/// `power := atom ('**' unary)?`.
fn arithmetic_power(chars: &[char], at: &mut usize) -> Option<f64> {
    let base = arithmetic_atom(chars, at)?;
    if chars.get(*at) == Some(&'*') && chars.get(*at + 1) == Some(&'*') {
        *at += 2;
        let exponent = arithmetic_unary(chars, at)?;
        return exact(base.powf(exponent));
    }
    Some(base)
}

/// `atom := number | '(' sum ')'`; a number starts with a digit or `.`
/// (`_5` is a name).
fn arithmetic_atom(chars: &[char], at: &mut usize) -> Option<f64> {
    if chars.get(*at) == Some(&'(') {
        *at += 1;
        let value = arithmetic_sum(chars, at)?;
        (chars.get(*at) == Some(&')')).then_some(())?;
        *at += 1;
        return Some(value);
    }
    let start = *at;
    if !chars
        .get(start)
        .is_some_and(|&ch| ch.is_ascii_digit() || ch == '.')
    {
        return None;
    }
    while let Some(&ch) = chars.get(*at) {
        let exponent_sign =
            matches!(ch, '+' | '-') && *at > start && matches!(chars.get(*at - 1), Some('e' | 'E'));
        if ch.is_ascii_digit() || matches!(ch, '.' | '_' | 'e' | 'E') || exponent_sign {
            *at += 1;
        } else {
            break;
        }
    }
    let text: String = chars
        .get(start..*at)?
        .iter()
        .filter(|&&ch| ch != '_')
        .collect();
    exact(text.parse::<f64>().ok()?)
}

/// Whether `side` is a literal expected value: numbers, strings without
/// escapes, `True`/`False`/`None`, and lists, tuples or dicts of them. A
/// name, call or attribute may carry the route's own response (a binding,
/// helper or fixture that makes the same request), so it is not one.
fn literal_expected(side: &str) -> bool {
    let mut stripped = String::new();
    let mut chars = side.trim().chars();
    while let Some(ch) = chars.next() {
        if ch == '"' || ch == '\'' {
            loop {
                match chars.next() {
                    Some(close) if close == ch => break,
                    Some('\\') | None => return false,
                    Some(_) => {}
                }
            }
            stripped.push('0');
        } else {
            stripped.push(ch);
        }
    }
    // Each identifier must be a whole `True`, `False` or `None` token, so a
    // name such as `TrueFalse` or `None_` is not a literal.
    let mut literal = String::new();
    let mut word = String::new();
    for ch in stripped.chars().chain(std::iter::once(' ')) {
        if ch.is_alphanumeric() || ch == '_' {
            if word.is_empty() && ch.is_ascii_digit() {
                literal.push(ch);
            } else {
                word.push(ch);
            }
            continue;
        }
        if !word.is_empty() {
            if !matches!(word.as_str(), "True" | "False" | "None") {
                return false;
            }
            literal.push('0');
            word.clear();
        }
        literal.push(ch);
    }
    let stripped = literal;
    !stripped.trim().is_empty()
        && stripped
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch.is_whitespace() || "-+.[](){},:".contains(ch))
}

/// Whether an exact assertion compares one changed quoted key of `name.json()`
/// with `==` or `self.assertEqual`: `response.json()["total"] == 195`. Any other
/// comparison, the whole body, or a read of another attribute of the response
/// (`status_code`, `text`) is not it.
pub(super) fn reads_response_json(text: &str, name: &str, changed_keys: &[String]) -> bool {
    let text = text.trim();
    let (left, right) = if let Some(arguments) = text
        .strip_prefix("self.assertEqual(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let Some(pair) = two_plain_arguments(arguments) else {
            return false;
        };
        pair
    } else {
        let text = text.strip_prefix("assert ").unwrap_or(text).trim();
        let Some((left, right)) = text.split_once("==") else {
            return false;
        };
        if right.contains("==") || left.ends_with(['!', '<', '>', '=']) || right.starts_with('=') {
            return false;
        }
        (left, right)
    };
    let target = format!("{name}.json()");
    let reads = |side: &str| {
        let side = side.trim();
        // Only a read of one key: a whole-body comparison can match an error
        // body (a validation failure, a raised HTTP error) that never reaches
        // the changed `return`, and the changed-key gate accepts any whole
        // comparison.
        side.strip_prefix(target.as_str())
            .and_then(|rest| plain_key_subscript(rest.trim_start()))
            .is_some_and(|key| changed_keys.iter().any(|changed| changed == key))
    };
    // The expected side must be a literal: another response, an inline
    // request, the same read, or a name bound to a request compares the
    // handler with itself and passes on both versions.
    (reads(left) && literal_expected(right)) || (reads(right) && literal_expected(left))
}

/// The two arguments of `self.assertEqual(first, second)`, split at the one
/// top-level comma. A third argument, a keyword argument, an escaped string or
/// unbalanced brackets yield nothing.
fn two_plain_arguments(arguments: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    let mut split = None;
    let mut quote: Option<char> = None;
    for (idx, ch) in arguments.char_indices() {
        if let Some(open) = quote {
            if ch == '\\' {
                return None;
            }
            if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            // `first=...` or any other top-level `=` is not a plain argument.
            '=' if depth == 0 => return None,
            ',' if depth == 0 => {
                if split.is_some() {
                    return None;
                }
                split = Some(idx);
            }
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    let idx = split?;
    let (first, second) = (arguments.get(..idx)?, arguments.get(idx + 1..)?);
    Some((first, second))
}

/// The key of `text` when it is exactly one subscript by a quoted
/// identifier-like key (`["total"]`, `['total']`). Anything after it (`["a"] + x["b"]`,
/// a second subscript, a call) yields nothing.
fn plain_key_subscript(text: &str) -> Option<&str> {
    let inner = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))?
        .trim();
    let key = inner
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            inner
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        });
    key.filter(|key| {
        !key.is_empty()
            && key
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    })
}
