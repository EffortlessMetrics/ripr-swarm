//! RIPR-SPEC-0231 rules 2 and 3: a `matches!` or `assert_matches!` assertion
//! is only as strong as its pattern. A binding, a wildcard payload, a range or
//! a side-only constructor does not pin a value, so it never reads as an exact
//! oracle.
//!
//! The reading is syntactic. A path the line cannot resolve (a variant, a
//! unit struct, a constant) counts as pinning a value, which keeps today's
//! reading; only patterns that are shown not to pin a value are weakened.

use crate::analysis::extract::mask_comments_and_strings;
use crate::domain::{OracleKind, OracleStrength};

use super::arguments::{complete_macro_arguments, parenthesized_contents};
use super::classify::OracleClassification;

/// The `Option` and `Result` constructors, which only say which side a value
/// is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Some,
    None,
    Ok,
    Err,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reading {
    /// Accepts every value of its type.
    Irrefutable,
    /// Checks only which side of an `Option` or `Result` the value is on.
    SideOnly(Side),
    /// A range, or a constructor whose payload is a range.
    Range { err: bool },
    /// Fixes a shape (a slice length, a tuple of side checks) but no value.
    PinsNothing { err: bool },
    /// Contains a literal, a constant or an unresolved variant.
    Pins,
}

/// The classification a whole-pattern assertion earns from its pattern, or
/// `None` when the pattern pins a value and the ordinary chain decides.
pub(super) fn pattern_assertion_classification(line: &str) -> Option<OracleClassification> {
    let pattern = asserted_pattern(line)?;
    let (pattern, guard) = split_guard(&pattern);
    // A guard that compares with `==` pins a value itself
    // (`_ if value == 2`, RIPR-SPEC-0108's runtime-controlled fixtures), so
    // the ordinary chain keeps its reading (decision 6).
    if guard.is_some_and(guard_pins_value) {
        return None;
    }
    let guarded = guard.is_some();
    let alternatives = split_top_level(pattern, '|');
    if alternatives.len() > 1 {
        return or_pattern_classification(&alternatives, guarded);
    }
    reading_classification(read_pattern(pattern), guarded)
}

/// The pattern operand of an `assert_matches!` / `debug_assert_matches!`, or
/// of a `matches!` that is the whole condition of `assert!`, `debug_assert!`
/// or `ensure!`. A compound condition is not a pattern assertion.
fn asserted_pattern(line: &str) -> Option<String> {
    if let Some(arguments) = ["assert_matches!", "debug_assert_matches!"]
        .into_iter()
        .find_map(|name| complete_macro_arguments(line, name))
    {
        return arguments.get(1).cloned();
    }
    let condition = ["assert!", "debug_assert!", "ensure!"]
        .into_iter()
        .find_map(|name| complete_macro_arguments(line, name)?.into_iter().next())?;
    let mut expression = condition.trim();
    while let Some(inner) = parenthesized_contents(expression) {
        expression = inner.trim();
    }
    let arguments = complete_macro_arguments(expression, "matches!")?;
    if arguments.len() == 2 {
        arguments.get(1).cloned()
    } else {
        None
    }
}

fn guard_pins_value(guard: &str) -> bool {
    mask_comments_and_strings(guard).contains("==")
}

/// Split `pattern if guard` at its top-level `if`.
fn split_guard(pattern: &str) -> (&str, Option<&str>) {
    let masked = mask_comments_and_strings(pattern);
    let mut depth = 0usize;
    let bytes = masked.as_bytes();
    for (index, ch) in masked.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            'i' if depth == 0
                && masked[index..].starts_with("if")
                && (index == 0 || !is_identifier_byte(bytes[index - 1]))
                && !bytes
                    .get(index + 2)
                    .copied()
                    .is_some_and(is_identifier_byte) =>
            {
                return (pattern[..index].trim(), Some(pattern[index + 2..].trim()));
            }
            _ => {}
        }
    }
    (pattern.trim(), None)
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Split on a separator outside delimiters, comments and literals. `|` is
/// never split inside `||`.
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in masked.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if ch == separator && depth == 0 => {
                let doubled = separator == '|'
                    && (bytes.get(index + 1) == Some(&b'|')
                        || (index > 0 && bytes[index - 1] == b'|'));
                if !doubled {
                    parts.push(text[start..index].trim());
                    start = index + 1;
                }
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    // A leading `|` in an or-pattern leaves an empty first alternative.
    if parts.len() > 1 && parts.first().is_some_and(|part| part.is_empty()) {
        parts.remove(0);
    }
    parts
}

fn reading_classification(reading: Reading, guarded: bool) -> Option<OracleClassification> {
    let (kind, strength) = match reading {
        Reading::Irrefutable => (OracleKind::RelationalCheck, OracleStrength::Weak),
        // Rule 2: an `Err` side check is a broad error, guard or not.
        Reading::SideOnly(Side::Err) | Reading::Range { err: true } => {
            (OracleKind::BroadError, OracleStrength::Weak)
        }
        Reading::PinsNothing { err: true } => (OracleKind::BroadError, OracleStrength::Weak),
        Reading::SideOnly(_) if guarded => (OracleKind::RelationalCheck, OracleStrength::Weak),
        Reading::SideOnly(_) => (OracleKind::SmokeOnly, OracleStrength::Smoke),
        Reading::Range { err: false } | Reading::PinsNothing { err: false } => {
            (OracleKind::RelationalCheck, OracleStrength::Weak)
        }
        Reading::Pins => return None,
    };
    Some(OracleClassification { kind, strength })
}

fn or_pattern_classification(alternatives: &[&str], guarded: bool) -> Option<OracleClassification> {
    let readings = alternatives
        .iter()
        .map(|alternative| read_pattern(alternative))
        .collect::<Vec<_>>();
    let covers = |a: Side, b: Side| {
        readings.contains(&Reading::SideOnly(a)) && readings.contains(&Reading::SideOnly(b))
    };
    if readings.contains(&Reading::Irrefutable)
        || covers(Side::Some, Side::None)
        || covers(Side::Ok, Side::Err)
    {
        return reading_classification(Reading::Irrefutable, guarded);
    }
    // Alternatives on both sides do not observe the side
    // (`Ok(_) | Err(E::X)`), so the assertion is only a relation.
    let sides = alternatives
        .iter()
        .filter_map(|alternative| top_side(alternative))
        .collect::<Vec<_>>();
    let both = |a: Side, b: Side| sides.contains(&a) && sides.contains(&b);
    if both(Side::Some, Side::None) || both(Side::Ok, Side::Err) {
        return Some(OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        });
    }
    // Otherwise the or-pattern reads as its weakest alternative. An
    // alternative that pins a value keeps the ordinary chain's (strong)
    // reading, so only when every alternative pins does the chain decide.
    readings
        .into_iter()
        .filter_map(|reading| reading_classification(reading, guarded))
        .min_by_key(|classification| classification.strength.rank())
}

/// The `Option`/`Result` constructor at the top of a pattern, if any.
fn top_side(pattern: &str) -> Option<Side> {
    let pattern = strip_wrappers(pattern);
    let (path, _) = constructor_parts(pattern);
    side_of_path(path)
}

fn side_of_path(path: &str) -> Option<Side> {
    let last = path.rsplit("::").next()?.trim();
    match last {
        "Some" => Some(Side::Some),
        "None" => Some(Side::None),
        "Ok" => Some(Side::Ok),
        "Err" => Some(Side::Err),
        _ => None,
    }
}

/// `&p`, `&mut p`, `box p` and `name @ p` read through to `p`.
fn strip_wrappers(pattern: &str) -> &str {
    let mut pattern = pattern.trim();
    loop {
        if let Some(rest) = pattern.strip_prefix('&') {
            pattern = rest.trim_start();
            if let Some(rest) = pattern.strip_prefix("mut ") {
                pattern = rest.trim_start();
            }
        } else if let Some(rest) = pattern.strip_prefix("box ") {
            pattern = rest.trim_start();
        } else if let Some(sub) = binding_subpattern(pattern) {
            pattern = sub;
        } else {
            return pattern;
        }
    }
}

/// The sub-pattern of `name @ p` (with optional `ref` / `mut`).
fn binding_subpattern(pattern: &str) -> Option<&str> {
    let parts = split_top_level(pattern, '@');
    let [binding, sub] = parts.as_slice() else {
        return None;
    };
    is_binding(binding).then_some(*sub)
}

/// A catch-all binding: a lowercase-initial identifier other than `true`
/// and `false`, optionally with `ref`, `mut` or `ref mut`.
fn is_binding(text: &str) -> bool {
    let mut words = text.split_whitespace().collect::<Vec<_>>();
    let Some(name) = words.pop() else {
        return false;
    };
    let modifiers_ok = matches!(words.as_slice(), [] | ["ref"] | ["mut"] | ["ref", "mut"]);
    modifiers_ok
        && name != "true"
        && name != "false"
        && name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// `(path, delimited payload)` for `Path(..)`, `Path { .. }` or a bare path.
fn constructor_parts(pattern: &str) -> (&str, Option<(char, &str)>) {
    let masked = mask_comments_and_strings(pattern);
    let Some(open) = masked.find(['(', '{', '[']) else {
        return (pattern.trim(), None);
    };
    let delimiter = masked[open..].chars().next().unwrap_or('(');
    let close = match delimiter {
        '(' => ')',
        '{' => '}',
        _ => ']',
    };
    if !masked.trim_end().ends_with(close) {
        return (pattern.trim(), None);
    }
    let end = masked.trim_end().len() - 1;
    (
        pattern[..open].trim(),
        Some((delimiter, pattern.get(open + 1..end).unwrap_or_default())),
    )
}

fn read_pattern(pattern: &str) -> Reading {
    let pattern = strip_wrappers(pattern);
    if pattern == "_" || is_binding(pattern) {
        return Reading::Irrefutable;
    }
    if is_range(pattern) {
        return Reading::Range { err: false };
    }
    if is_literal(pattern) {
        return Reading::Pins;
    }
    let (path, payload) = constructor_parts(pattern);
    match payload {
        Some(('(', inner)) if path.is_empty() => tuple_reading(inner),
        Some(('[', inner)) if path.is_empty() => slice_reading(inner),
        Some(('(', inner)) => match side_of_path(path) {
            Some(side) => side_payload_reading(side, inner),
            // A tuple-struct or variant the line cannot resolve.
            None => Reading::Pins,
        },
        // A struct-like path is read as a variant unless it can be shown to
        // name a struct, which needs type information this reader lacks.
        Some(_) => Reading::Pins,
        None => match side_of_path(path) {
            Some(Side::None) => Reading::SideOnly(Side::None),
            _ => Reading::Pins,
        },
    }
}

fn side_payload_reading(side: Side, inner: &str) -> Reading {
    let err = side == Side::Err;
    let elements = split_top_level(inner, ',');
    let elements = elements
        .iter()
        .filter(|element| !element.is_empty())
        .collect::<Vec<_>>();
    let readings = elements
        .iter()
        .map(|element| {
            if element.trim() == ".." {
                Reading::Irrefutable
            } else {
                read_pattern(element)
            }
        })
        .collect::<Vec<_>>();
    if readings.contains(&Reading::Pins) {
        Reading::Pins
    } else if readings
        .iter()
        .any(|reading| matches!(reading, Reading::Range { .. }))
    {
        Reading::Range { err }
    } else if readings
        .iter()
        .any(|reading| matches!(reading, Reading::PinsNothing { .. }))
    {
        Reading::PinsNothing { err }
    } else {
        Reading::SideOnly(side)
    }
}

fn combine(readings: &[Reading], all_irrefutable: Reading) -> Reading {
    if readings.contains(&Reading::Pins) {
        Reading::Pins
    } else if readings
        .iter()
        .any(|reading| matches!(reading, Reading::Range { .. }))
    {
        Reading::Range { err: false }
    } else if readings
        .iter()
        .all(|reading| *reading == Reading::Irrefutable)
    {
        all_irrefutable
    } else {
        Reading::PinsNothing { err: false }
    }
}

fn tuple_reading(inner: &str) -> Reading {
    let elements = split_top_level(inner, ',');
    // `(p)` is a parenthesized pattern, not a one-tuple.
    if elements.len() == 1 {
        return read_pattern(inner);
    }
    let readings = elements
        .iter()
        .filter(|element| !element.is_empty())
        .map(|element| {
            if *element == ".." {
                Reading::Irrefutable
            } else {
                read_pattern(element)
            }
        })
        .collect::<Vec<_>>();
    combine(&readings, Reading::Irrefutable)
}

fn slice_reading(inner: &str) -> Reading {
    let elements = split_top_level(inner, ',')
        .into_iter()
        .filter(|element| !element.is_empty())
        .collect::<Vec<_>>();
    let is_rest = |element: &str| {
        element == ".."
            || split_top_level(element, '@')
                .as_slice()
                .get(1)
                .is_some_and(|rest| *rest == "..")
    };
    // `[..]` and `[name @ ..]` accept every slice; any other slice pattern
    // checks a length.
    if let [only] = elements.as_slice()
        && is_rest(only)
    {
        return Reading::Irrefutable;
    }
    let readings = elements
        .iter()
        .map(|element| {
            if is_rest(element) {
                Reading::Irrefutable
            } else {
                read_pattern(element)
            }
        })
        .collect::<Vec<_>>();
    combine(&readings, Reading::PinsNothing { err: false })
}

fn is_range(pattern: &str) -> bool {
    let masked = mask_comments_and_strings(pattern);
    let mut depth = 0usize;
    let bytes = masked.as_bytes();
    for (index, ch) in masked.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '.' if depth == 0 && bytes.get(index + 1) == Some(&b'.') => {
                return masked.trim() != "..";
            }
            _ => {}
        }
    }
    false
}

fn is_literal(pattern: &str) -> bool {
    let pattern = pattern.strip_prefix('-').unwrap_or(pattern).trim_start();
    pattern == "true"
        || pattern == "false"
        || pattern.starts_with(|ch: char| ch.is_ascii_digit() || ch == '"' || ch == '\'')
        || pattern.starts_with("b\"")
        || pattern.starts_with("b'")
        || pattern.starts_with("r\"")
        || pattern.starts_with("r#")
}

#[cfg(test)]
mod tests {
    use super::pattern_assertion_classification;
    use crate::domain::{OracleKind, OracleStrength};

    #[test]
    fn spec_0231_rules_2_and_3_read_the_pattern() -> Result<(), String> {
        use OracleKind::{BroadError, RelationalCheck, SmokeOnly};
        use OracleStrength::{Smoke, Weak};
        let weakened = [
            // Acceptance examples 3, 4, 7, 17, 18, 21, 23, 24, 25 and 26.
            (
                "assert!(matches!(check(20), Err(e) if !e.is_empty()))",
                BroadError,
                Weak,
            ),
            ("assert!(matches!(check(20), Err(_)))", BroadError, Weak),
            ("assert!(matches!(check(5), Ok(_)))", SmokeOnly, Smoke),
            ("assert!(matches!(check(20), Err(_e)))", BroadError, Weak),
            ("ensure!(matches!(check(5), Ok(_)))", SmokeOnly, Smoke),
            (
                "assert!(matches!(check(20), Err(ref e) if e.len() > 1))",
                BroadError,
                Weak,
            ),
            ("assert!(matches!(check(20), Err(mut e)))", BroadError, Weak),
            ("assert!(matches!(check(20), Err(e @ _)))", BroadError, Weak),
            (
                "assert!(matches!(lookup(1), Some(ref x)))",
                SmokeOnly,
                Smoke,
            ),
            ("assert!(matches!(check(5), Ok(x @ _)))", SmokeOnly, Smoke),
            (
                "assert!(matches!(lookup(1), Some(_) | None))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(check(5), Ok(_) | Err(_)))",
                RelationalCheck,
                Weak,
            ),
            ("assert!(matches!(pair(), (_, _)))", RelationalCheck, Weak),
            (
                "assert!(matches!(lookup(1), Some(1..=5)))",
                RelationalCheck,
                Weak,
            ),
            ("assert!(matches!(parse(), Some(Ok(_))))", SmokeOnly, Smoke),
            ("assert!(matches!(lookup(1), &Some(_)))", SmokeOnly, Smoke),
            ("assert!(matches!(items(), [_, ..]))", RelationalCheck, Weak),
            (
                "assert!(matches!(check(20), Ok(_) | Err(E::Bad)))",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(3) | Some(_)))",
                SmokeOnly,
                Smoke,
            ),
            ("assert!(matches!(items(), [..]))", RelationalCheck, Weak),
            (
                "assert!(matches!(items(), [rest @ ..]))",
                RelationalCheck,
                Weak,
            ),
            // Other forms the rules name.
            ("assert!(matches!(check(20), Err(1..=5)))", BroadError, Weak),
            (
                "assert!(matches!(lookup(1), Some(x) if x > 3))",
                RelationalCheck,
                Weak,
            ),
            ("assert_matches!(check(5), Ok(_))", SmokeOnly, Smoke),
            ("debug_assert_matches!(lookup(1), None)", SmokeOnly, Smoke),
            ("assert_matches!(check(20), Err(_e))", BroadError, Weak),
            ("assert!((matches!(check(5), Ok(_))))", SmokeOnly, Smoke),
            ("assert!(matches!(check(5), Ok(..)))", SmokeOnly, Smoke),
            (
                "assert!(matches!(pair(), (Some(_), _)))",
                RelationalCheck,
                Weak,
            ),
        ];
        for (text, kind, strength) in weakened {
            let actual = pattern_assertion_classification(text);
            if actual.as_ref().map(|c| (&c.kind, &c.strength)) != Some((&kind, &strength)) {
                return Err(format!(
                    "{text}: expected {kind:?}/{strength:?}, got {actual:?}"
                ));
            }
        }
        // Patterns that pin a value keep the ordinary chain's reading, and
        // compound conditions are not pattern assertions.
        for text in [
            "assert!(matches!(check(5), Ok(5)))",
            "assert!(matches!(check(20), Err(E::Bad)))",
            "assert!(matches!(check(20), Err(e @ E::Bad)))",
            "assert!(matches!(check(20), Err(E::Bad(..))))",
            "assert!(matches!(check(20), Err(E::Bad { .. })))",
            "assert!(matches!(lookup(1), Some(x @ 3)))",
            "assert!(matches!(items(), [1, ..]))",
            "assert!(matches!(lookup(1), Some(\"a\")))",
            "assert!(matches!(make(), Only::Value))",
            "assert!(matches!(cfg(), Cfg { .. }))",
            "assert!(matches!(check(20), Err(E::A) | Err(E::B)))",
            "assert!(matches!(check(5), Ok(_)) && score(2) == 4)",
            "assert_eq!(check(20), Err(e))",
            // A guard that compares with `==` pins a value (decision 6).
            "assert!(matches!(value, _ if value == 2))",
            "assert_matches!(value, _ if value == 2)",
            "assert!(matches!(lookup(1), Some(x) if x == 3))",
        ] {
            let actual = pattern_assertion_classification(text);
            if actual.is_some() {
                return Err(format!(
                    "{text}: expected no pattern reading, got {actual:?}"
                ));
            }
        }
        Ok(())
    }
}
