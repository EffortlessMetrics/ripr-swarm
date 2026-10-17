//! #6578: a pattern assertion that pins a constructed variant field.
//!
//! `assert_matches!(parse_num("x1"), Err(ParseError::BadDigit { pos: 0 }))`
//! matches only when the owner's result holds that variant with `pos == 0`,
//! so a change to the `pos` initializer of the owner's one
//! `ParseError::BadDigit { pos: .. }` literal is observed. A literal pattern
//! compares by value without `PartialEq`, so no derive gate is needed. The
//! rule refuses a wildcard or binding (`pos: _`, `pos: p`), a guard, an
//! alternative (`|`), a scrutinee other than a direct owner call (which
//! must also settle on the owner among same-named rivals), a variant
//! path that differs from the constructed one, a range pattern, and a
//! variant name that more than one production `Name {` head spells (a
//! helper, a `From` impl or a rival enum might build the value the test
//! observes).

use crate::analysis::extract::mask_comments_and_strings;

const PATTERN_MACROS: [&str; 3] = [
    "assert_matches!(",
    "debug_assert_matches!(",
    "assert!(matches!(",
];

/// The changed initializer: the owner's body and first line, the owner's
/// name, and the probe's line and expression.
pub(super) struct ConstructedField<'a> {
    pub(super) owner_body: &'a str,
    pub(super) owner_start_line: usize,
    pub(super) owner_name: &'a str,
    pub(super) probe_line: usize,
    pub(super) probe_expression: &'a str,
}

/// Whether `assertion`, in a test whose body is `test_body`, is a pattern
/// assertion on a direct call of the owner that pins the field the probe
/// initializes, in the struct-variant literal on the probe line, to a
/// literal.
pub(super) fn pattern_pins_constructed_field(
    changed: &ConstructedField<'_>,
    test_body: &str,
    assertion: &str,
    names_owner: &dyn Fn(&[String]) -> bool,
    production_heads: &dyn Fn(&str) -> usize,
) -> bool {
    let ConstructedField {
        owner_body,
        owner_start_line,
        owner_name,
        probe_line,
        probe_expression,
    } = *changed;
    let Some(field) = initialized_field(probe_expression) else {
        return false;
    };
    let Some(variant) = probe_line
        .checked_sub(owner_start_line)
        .and_then(|offset| owner_body.lines().nth(offset))
        .and_then(|line| enclosing_literal_path(line, probe_expression))
    else {
        return false;
    };
    let Some(last) = variant.last() else {
        return false;
    };
    let Some((scrutinee, pattern, pattern_masked)) = macro_operands(assertion) else {
        return false;
    };
    let Some(qualifier) = direct_owner_call_qualifier(scrutinee, owner_name) else {
        return false;
    };
    if !names_owner(&qualifier) || pattern_masked.contains('|') || has_guard(&pattern_masked) {
        return false;
    }
    let Some(group) = single_variant_group(pattern, &pattern_masked, &variant) else {
        return false;
    };
    // Last, because it scans every production body: the owner's literal
    // must be the only `Last {` head the workspace's production code spells.
    field_pinned_to_literal(group, field)
        && !builds_outside_patterns(test_body, last)
        && production_heads(last) == 1
}

/// Whether `test_body` spells a `last {` head outside the patterns of its
/// pattern assertions: a value the test builds might reach the owner
/// through shared state, so the pattern no longer reads only the owner's
/// own literal.
fn builds_outside_patterns(test_body: &str, last: &str) -> bool {
    // Search the masked body, so a pattern assertion written inside a
    // comment or string never offsets a real build.
    let masked = mask_comments_and_strings(test_body);
    let total = literal_heads(&masked, last);
    let in_patterns: usize = PATTERN_MACROS
        .iter()
        .flat_map(|name| masked.match_indices(name))
        // `debug_assert_matches!(` also holds `assert_matches!(`; count
        // its pattern once.
        .filter(|(at, name)| !(*name == "assert_matches!(" && masked[..*at].ends_with("debug_")))
        .filter_map(|(at, _)| macro_operands(&masked[at..]))
        .map(|(_, _, pattern_masked)| literal_heads(&pattern_masked, last))
        .sum();
    total > in_patterns
}

/// How many `last {` heads the indexed source spells outside executable
/// test bodies and enum declarations: production and test-helper `fn`
/// bodies (a helper might feed the owner), and every `const`, `static` and
/// `macro_rules!` body, test modules included (a fail-closed overcount).
pub(super) fn production_literal_heads(
    index: &crate::analysis::rust_index::RustIndex,
    last: &str,
) -> usize {
    index
        .files()
        .iter()
        .filter(|(_, view)| view.data().source.contains(last))
        .map(|(_, view)| {
            let masked = mask_comments_and_strings(&view.data().source);
            let tests: Vec<(usize, usize)> = view
                .functions
                .iter()
                .filter(|function| function.source_role.registers_executable_test())
                .map(|function| (function.start_line, function.end_line))
                .collect();
            let declarations = enum_bodies(&masked);
            literal_head_offsets(&masked, last)
                .filter(|at| !declarations.iter().any(|body| body.contains(at)))
                .filter(|at| {
                    let line = masked[..*at].matches('\n').count() + 1;
                    !tests
                        .iter()
                        .any(|(start, end)| (*start..=*end).contains(&line))
                })
                .count()
        })
        .sum()
}

/// The field a `name: value` initializer sets; a shorthand `name` reads a
/// local, not a literal, and is left out.
fn initialized_field(expression: &str) -> Option<&str> {
    let (name, value) = expression.trim().split_once(':')?;
    let name = name.trim();
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    // `a::b` is a path, not `name: value`; a path inside the value
    // (`pos: usize::from(i)`) still initializes `pos`.
    (valid && !value.starts_with(':')).then_some(name)
}

/// The path segments before the `{` that opens the struct literal holding
/// `expression` on `line` (`ParseError::BadDigit`), read on masked text.
fn enclosing_literal_path(line: &str, expression: &str) -> Option<Vec<String>> {
    let at = line.find(expression.trim())?;
    let masked = mask_comments_and_strings(line);
    let mut depth = 0usize;
    let mut open = None;
    for (index, byte) in masked.as_bytes()[..at].iter().enumerate().rev() {
        match byte {
            b'}' => depth += 1,
            b'{' if depth == 0 => {
                open = Some(index);
                break;
            }
            b'{' => depth -= 1,
            _ => {}
        }
    }
    path_before(&masked[..open?])
}

/// The `a::B::C` path ending right before `end` (whitespace skipped), as
/// segments; `None` unless the last segment starts uppercase.
fn path_before(text: &str) -> Option<Vec<String>> {
    let text = text.trim_end();
    let start = text
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .map_or(0, |index| index + 1);
    let segments: Vec<String> = text[start..].split("::").map(str::to_string).collect();
    let last = segments.last()?;
    (!segments.iter().any(String::is_empty)
        && last
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase()))
    .then_some(segments)
}

/// How many `Last {` struct-literal or pattern heads `masked` holds.
fn literal_heads(masked: &str, last: &str) -> usize {
    literal_head_offsets(masked, last).count()
}

/// The byte spans of the `enum Name { .. }` bodies in `masked`, where a
/// `Last {` head declares a variant rather than building one.
fn enum_bodies(masked: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = masked.as_bytes();
    let mut bodies = Vec::new();
    for (at, _) in masked.match_indices("enum") {
        let word_start =
            at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        let word_end = bytes
            .get(at + 4)
            .is_some_and(|byte| byte.is_ascii_whitespace());
        let Some(open) = masked[at..].find('{').map(|offset| at + offset) else {
            continue;
        };
        // Only an item head (`enum Name<..> where .. {`) reaches the brace.
        if !word_start || !word_end || masked[at..open].contains([';', '}', '(', '=']) {
            continue;
        }
        let mut depth = 0usize;
        for (index, byte) in bytes.iter().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        bodies.push(open..index);
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    bodies
}

/// The byte offsets of the `Last {` heads in `masked`.
fn literal_head_offsets<'m>(masked: &'m str, last: &'m str) -> impl Iterator<Item = usize> + 'm {
    masked
        .match_indices(last)
        .map(|(at, _)| at)
        .filter(move |at| {
            let before_ok = masked[..*at]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'));
            let after = masked[at + last.len()..].trim_start();
            before_ok && (after.starts_with('{') || after.starts_with("::<"))
        })
}

/// The scrutinee and pattern operands of a pattern assertion, with the
/// pattern's masked text.
fn macro_operands(assertion: &str) -> Option<(&str, &str, String)> {
    let trimmed = assertion.trim_start();
    let offset = assertion.len() - trimmed.len();
    let open = PATTERN_MACROS
        .iter()
        .find(|name| trimmed.starts_with(*name))
        .map(|name| offset + name.len())?;
    let masked = mask_comments_and_strings(assertion);
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut comma = None;
    let mut close = None;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth == 0 => {
                close = Some(index);
                break;
            }
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => match comma {
                None => comma = Some(index),
                // A message argument after the pattern.
                Some(_) => {
                    close = Some(index);
                    break;
                }
            },
            _ => {}
        }
    }
    let (comma, close) = (comma?, close?);
    // `assert!(matches!(..) || ..)`: the `matches!` call must be the whole
    // condition, followed only by the `assert!` close or its message.
    if trimmed.starts_with("assert!(matches!(") {
        let after = masked[close..].strip_prefix(')')?.trim_start();
        if !(after.starts_with(')') || after.starts_with(',')) {
            return None;
        }
    }
    Some((
        assertion[open..comma].trim(),
        assertion[comma + 1..close].trim(),
        masked[comma + 1..close].trim().to_string(),
    ))
}

/// The spelled path before `owner` when the whole scrutinee is `owner(..)`,
/// `&owner(..)` or `path::owner(..)` (empty for a bare call).
fn direct_owner_call_qualifier(scrutinee: &str, owner: &str) -> Option<Vec<String>> {
    let scrutinee = scrutinee.trim_start_matches('&').trim();
    let head = format!("{owner}(");
    let at = scrutinee.find(&head)?;
    let prefix = &scrutinee[..at];
    let path_ok = prefix.is_empty()
        || prefix.ends_with("::")
            && prefix
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == ':');
    if !path_ok
        || !closes_at_end(&scrutinee[at + head.len() - 1..])
        || !literal_arguments(&scrutinee[at + head.len()..scrutinee.len() - 1])
    {
        return None;
    }
    let segments: Vec<String> = prefix
        .trim_end_matches("::")
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect();
    Some(segments)
}

/// Whether every call argument is a literal (optionally borrowed), so the
/// test cannot hand the owner a variant value it built itself.
fn literal_arguments(arguments: &str) -> bool {
    let masked = mask_comments_and_strings(arguments);
    let mut depth = 0usize;
    let mut start = 0;
    let mut ranges = Vec::new();
    for (index, byte) in masked.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                ranges.push(start..index);
                start = index + 1;
            }
            _ => {}
        }
    }
    ranges.push(start..arguments.len());
    ranges.into_iter().all(|range| {
        let argument = arguments[range].trim().trim_start_matches('&').trim();
        argument.is_empty() || is_value_literal(argument)
    })
}

/// Whether the parenthesis opening `text` closes at its last byte.
fn closes_at_end(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    let mut depth = 0usize;
    for (index, byte) in masked.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + 1 == masked.trim_end().len();
                }
            }
            _ => {}
        }
    }
    false
}

fn has_guard(pattern_masked: &str) -> bool {
    pattern_masked
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .any(|word| word == "if")
}

/// The `{ .. }` body of the one `Path { .. }` in the pattern whose path
/// agrees with `variant`: the same last segment and, when both name one,
/// the same enclosing segment.
fn single_variant_group<'p>(pattern: &'p str, masked: &str, variant: &[String]) -> Option<&'p str> {
    let last = variant.last()?;
    if literal_heads(masked, last) != 1 {
        return None;
    }
    let (at, _) = masked.match_indices(last.as_str()).find(|(at, _)| {
        masked[at + last.len()..].trim_start().starts_with('{')
            && masked[..*at]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    })?;
    let written = path_before(&masked[..at + last.len()])?;
    if written.len() >= 2
        && variant.len() >= 2
        && written[written.len() - 2] != variant[variant.len() - 2]
    {
        return None;
    }
    let open = at + last.len() + masked[at + last.len()..].find('{')?;
    let mut depth = 0usize;
    for (index, byte) in masked.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return pattern.get(open + 1..index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the field pattern list binds `field` to a literal.
fn field_pinned_to_literal(group: &str, field: &str) -> bool {
    let masked = mask_comments_and_strings(group);
    let mut depth = 0usize;
    let mut start = 0;
    let mut entries = Vec::new();
    for (index, byte) in masked.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                entries.push(start..index);
                start = index + 1;
            }
            _ => {}
        }
    }
    entries.push(start..group.len());
    entries.into_iter().any(|range| {
        let entry = &group[range];
        entry
            .split_once(':')
            .filter(|(name, _)| name.trim() == field)
            .is_some_and(|(_, value)| is_literal(value.trim()))
    })
}

/// A literal pattern value. A range pattern (`0..5`, `0..`, `'a'..='z'`)
/// admits more than one value; a string literal holding `..` is refused
/// with it.
fn is_literal(value: &str) -> bool {
    !value.contains("..") && is_value_literal(value)
}

fn is_value_literal(value: &str) -> bool {
    if matches!(value, "true" | "false") {
        return true;
    }
    if value.len() >= 2
        && (value.starts_with('"') && value.ends_with('"')
            || value.starts_with('\'') && value.ends_with('\''))
    {
        return true;
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    digits.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        && digits
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "pub fn parse_num(s: &str) -> Result<u32, ParseError> {
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut acc: u32 = 0;
    for (i, c) in s.chars().enumerate() {
        let d = c.to_digit(10).ok_or(ParseError::BadDigit { pos: 0 + i })?;
        acc = acc * 10 + d;
    }
    Ok(acc)
}";

    const CHANGED: ConstructedField<'static> = ConstructedField {
        owner_body: OWNER,
        owner_start_line: 10,
        owner_name: "parse_num",
        probe_line: 16,
        probe_expression: "pos: 0 + i",
    };

    fn pins(assertion: &str) -> bool {
        pattern_pins_constructed_field(&CHANGED, assertion, assertion, &|_| true, &|last| {
            literal_heads(&mask_comments_and_strings(OWNER), last)
        })
    }

    #[test]
    fn a_literal_variant_field_pattern_on_the_owner_result_pins_the_field() {
        for assertion in [
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));",
            "assert_matches!(parse_num(\"1x\"), Err(BadDigit { pos: 1 }));",
            "assert_matches!(crate::parse_num(\"1x\"), Err(ParseError::BadDigit { pos: 1, .. }));",
            "assert!(matches!(parse_num(\"x\"), Err(ParseError::BadDigit { pos: 0 })));",
            "assert!(matches!(parse_num(\"x\"), Err(ParseError::BadDigit { pos: 0 })), \"pos\");",
            "debug_assert_matches!(parse_num(\"x\"), Err(ParseError::BadDigit { pos: 0 }), \"pos\");",
        ] {
            assert!(pins(assertion), "{assertion}");
        }
    }

    #[test]
    fn a_pattern_that_leaves_the_field_open_or_reads_another_value_does_not_pin() {
        for assertion in [
            // The field is left open.
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { .. }));",
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: _ }));",
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: p }));",
            // A guard or an alternative decides instead of the literal.
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: p }) if p < 5);",
            // A range admits more than one value.
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0..5 }));",
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0.. }));",
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0..=1 }));",
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 'a'..='z' }));",
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 | 1 }));",
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }) | Err(ParseError::Empty));",
            // Another variant, or the same name under another enum.
            "assert_matches!(parse_num(\"x1\"), Err(ParseError::Empty));",
            "assert_matches!(parse_num(\"x1\"), Err(LexError::BadDigit { pos: 0 }));",
            // Not the owner's result.
            "assert_matches!(other(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));",
            "assert_matches!(parse_num(\"x1\").map(|v| v + 1), Err(ParseError::BadDigit { pos: 0 }));",
            "assert_matches!(r, Err(ParseError::BadDigit { pos: 0 }));",
            // The test hands the owner a value it built or holds.
            "assert_matches!(parse_num(input), Err(ParseError::BadDigit { pos: 0 }));",
            "assert_matches!(parse_num(&bad(0)), Err(ParseError::BadDigit { pos: 0 }));",
            "assert_matches!(parse_num(\"x\", Err(ParseError::BadDigit { pos: 0 })), Err(ParseError::BadDigit { pos: 0 }));",
            // `matches!` is only part of the asserted condition.
            "assert!(matches!(parse_num(\"x\"), Err(ParseError::BadDigit { pos: 0 })) || matches!(parse_num(\"x\"), Err(ParseError::BadDigit { pos: 1 })));",
            "assert!(matches!(parse_num(\"x\"), Err(ParseError::BadDigit { pos: 0 })) || cfg!(windows));",
            "assert!(matches!(parse_num(\"x\"), Err(ParseError::BadDigit { pos: 0 })) == false);",
            // Not a pattern assertion.
            "assert_eq!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));",
        ] {
            assert!(!pins(assertion), "{assertion}");
        }
    }

    #[test]
    fn a_variant_another_production_site_builds_is_not_pinned_through_one_pattern() {
        // A helper elsewhere builds the same variant, so the matched value
        // might come from it.
        let helper = "fn more() -> ParseError { ParseError::BadDigit { pos: 9 } }";
        let assertion = "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));";
        assert!(!pattern_pins_constructed_field(
            &CHANGED,
            assertion,
            assertion,
            &|_| true,
            &|last| {
                literal_heads(&mask_comments_and_strings(OWNER), last)
                    + literal_heads(&mask_comments_and_strings(helper), last)
            },
        ));
        // A call that settles on a same-named rival lends nothing, and the
        // spelled qualifier reaches the identity rule.
        let rival =
            "assert_matches!(other::parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));";
        assert!(!pattern_pins_constructed_field(
            &CHANGED,
            rival,
            rival,
            &|qualifier| qualifier.is_empty(),
            &|_| 1,
        ));
        // The test builds the variant itself (here into shared state the
        // owner might read), so the pattern may observe the test's value.
        let assertion = "assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit { pos: 0 }));";
        let body = format!(
            "FAULT.with(|f| f.set(Some(ParseError::BadDigit {{ pos: 0 }})));\n{assertion}\ndebug_assert_matches!(parse_num(\"x1\"), Err(ParseError::BadDigit {{ pos: 0 }}));"
        );
        assert!(!pattern_pins_constructed_field(
            &CHANGED,
            &body,
            assertion,
            &|_| true,
            &|_| 1,
        ));
        // A pattern assertion inside a comment or string offsets nothing.
        for hidden in [
            "// assert_matches!(parse_num(\"1\"), Err(ParseError::BadDigit { pos: 0 }));",
            "let note = \"assert_matches!(x, Err(ParseError::BadDigit { pos: 0 }))\";",
        ] {
            let body = format!(
                "{hidden}\nFAULT.with(|f| f.set(Some(ParseError::BadDigit {{ pos: 0 }})));\n{assertion}"
            );
            assert!(
                !pattern_pins_constructed_field(&CHANGED, &body, assertion, &|_| true, &|_| 1),
                "{hidden}"
            );
        }
        // Two pattern assertions alone build nothing.
        let body = format!(
            "{assertion}\ndebug_assert_matches!(parse_num(\"1\"), Err(ParseError::BadDigit {{ pos: 0 }}));\nassert!(matches!(parse_num(\"x\"), Err(ParseError::BadDigit {{ pos: 0 }})));"
        );
        assert!(pattern_pins_constructed_field(
            &CHANGED,
            &body,
            assertion,
            &|_| true,
            &|_| 1,
        ));
        // A shorthand initializer reads a local, never a literal.
        assert!(initialized_field("pos").is_none());
        assert!(initialized_field("crate::pos").is_none());
        assert_eq!(initialized_field("pos: usize::from(i)"), Some("pos"));
    }
}
