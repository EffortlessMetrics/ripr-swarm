//! Refuse grip credit when a test supplies the asserted outcome through a
//! value-supplying adapter (#12359, #12610). Logic mirrors
//! `extract::oracles::patterns` but lives here so repo seam discrimination
//! can apply the same refusal without duplicating product boundaries.

use super::related_tests::strip_comments_and_strings;

const VALUE_SUPPLYING_ADAPTERS: &[&str] = &[
    "or",
    "or_else",
    "and",
    "and_then",
    "ok_or",
    "ok_or_else",
    "unwrap_or",
    "unwrap_or_else",
    "map_err",
    "map_or",
    "map_or_else",
];

/// True when the assertion compares a value the test can supply independently
/// of the owner's changed behavior (adapter-fed `Err`, or an error read through
/// a replacing adapter such as `.or(Err(..)).unwrap_err()`).
pub(super) fn asserted_outcome_independent_of_owner(line: &str) -> bool {
    err_constructor_feeds_value_adapter(line) || inline_error_read_through_replacing_adapter(line)
}

fn err_constructor_feeds_value_adapter(line: &str) -> bool {
    let masked = strip_turbofish(&strip_comments_and_strings(line));
    masked.match_indices("Err").any(|(index, _)| {
        let is_token =
            |ch: Option<char>| ch.is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'));
        let before_ok = is_token(masked[..index].chars().next_back());
        let after = &masked[index + "Err".len()..];
        before_ok
            && after.trim_start().starts_with('(')
            && enclosing_callee(&masked[..index])
                .is_some_and(|name| VALUE_SUPPLYING_ADAPTERS.contains(&name))
    })
}

fn inline_error_read_through_replacing_adapter(line: &str) -> bool {
    let masked = strip_comments_and_strings(line);
    let Some((left, right)) = equality_operands(&masked) else {
        return false;
    };
    for operand in [left.as_str(), right.as_str()] {
        let mut operand = operand.trim();
        while operand.starts_with('(')
            && group_end(operand, 0) == Some(operand.len().saturating_sub(1))
        {
            operand = operand[1..operand.len() - 1].trim();
        }
        if (operand.contains(".unwrap_err(") || operand.contains(".expect_err("))
            && has_replacing_adapter_call(operand)
        {
            return true;
        }
    }
    false
}

fn equality_operands(line: &str) -> Option<(String, String)> {
    for mac in ["assert_eq!", "debug_assert_eq!"] {
        let needle = format!("{mac}(");
        let Some(start) = line.find(&needle).map(|index| index + needle.len()) else {
            continue;
        };
        let open_paren = start.saturating_sub(1);
        let Some(end) = group_end(line, open_paren) else {
            continue;
        };
        let inside = &line[start..end];
        let Some(comma) = split_top_level_comma(inside) else {
            continue;
        };
        return Some((comma.0.to_string(), comma.1.to_string()));
    }
    None
}

fn split_top_level_comma(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                return Some((text[..index].trim(), text[index + 1..].trim()));
            }
            _ => {}
        }
    }
    None
}

fn group_end(text: &str, open_index: usize) -> Option<usize> {
    let open = *text.as_bytes().get(open_index)?;
    let close = match open {
        b'(' => b')',
        _ => return None,
    };
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in text[open_index..].char_indices() {
        let index = open_index + index;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && ch as u8 == close {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn has_replacing_adapter_call(masked: &str) -> bool {
    const REPLACING: &[&str] = &["map_err", "and", "or", "ok_or", "ok_or_else", "or_else"];
    let stripped = strip_turbofish(masked);
    REPLACING
        .iter()
        .any(|name| has_named_call(stripped.as_str(), name))
}

fn has_named_call(masked: &str, name: &str) -> bool {
    masked.match_indices(name).any(|(index, _)| {
        let before_ok = masked[..index]
            .chars()
            .next_back()
            .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'));
        let after = masked[index + name.len()..].trim_start();
        before_ok && after.starts_with('(')
    })
}

fn enclosing_callee(prefix: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (offset, ch) in prefix.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' if depth > 0 => depth -= 1,
            '(' => {
                let head = prefix[..offset].trim_end();
                let start = head
                    .char_indices()
                    .rev()
                    .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
                    .map_or(0, |(i, c)| i + c.len_utf8());
                let name = &head[start..];
                if !name.is_empty() {
                    return Some(name);
                }
            }
            _ => {}
        }
    }
    None
}

fn strip_turbofish(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("::<") {
        out.push_str(&rest[..index]);
        let mut depth = 0usize;
        let mut end = None;
        for (offset, ch) in rest[index + 2..].char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end = Some(index + 2 + offset + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(end) => rest = &rest[end..],
            None => {
                out.push_str(&rest[index..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn or_err_constructor_in_assertion_is_adapter_fed() {
        assert!(asserted_outcome_independent_of_owner(
            "assert_eq!(parse(\"1\").or(Err(Fault::Odd)), Err(Fault::Odd));"
        ));
    }

    #[test]
    fn or_err_before_unwrap_err_is_adapter_fed() {
        assert!(asserted_outcome_independent_of_owner(
            "assert_eq!(parse_masked(1).or(Err(Fault::Odd)).unwrap_err(), Fault::Odd);"
        ));
    }

    #[test]
    fn direct_unwrap_err_pin_is_not_adapter_fed() {
        assert!(!asserted_outcome_independent_of_owner(
            "assert_eq!(parse(\"1\").unwrap_err(), Fault::Odd);"
        ));
    }
}
