//! Bounded `.map(owner)` iterator chains for reach call facts (#9478).

use super::MAX_CALL_FACTS_PER_LINE;
use crate::analysis::facts::CallFact;

/// Scan `masked` for bounded `literal.iter().map(path).collect::<Vec..>()` chains
/// and append synthetic owner call facts (`path(first_literal)`) for reach only.
pub(super) fn add_path_value_map_calls(
    masked: &str,
    _body: &str,
    start_line: usize,
    calls: &mut Vec<CallFact>,
) {
    let mut added = Vec::new();
    for (offset, masked_line) in masked.split_inclusive('\n').enumerate() {
        if !masked_line.contains(".map(") || !masked_line.contains(".collect::<Vec") {
            continue;
        }
        let line = start_line + offset;
        let on_line = calls.iter().filter(|call| call.line == line).count();
        if on_line >= MAX_CALL_FACTS_PER_LINE {
            continue;
        }
        let mut line_names: std::collections::BTreeSet<String> = calls
            .iter()
            .filter(|call| call.line == line)
            .map(|call| call.name.clone())
            .collect();
        for (target, call) in synthetic_calls_in_line(masked_line) {
            if line_names.len() >= MAX_CALL_FACTS_PER_LINE {
                break;
            }
            if !line_names.insert(target.clone()) {
                continue;
            }
            added.push(CallFact {
                line,
                name: target,
                text: call.into(),
            });
        }
    }
    for fact in added {
        let on_line = calls.iter().filter(|call| call.line == fact.line).count();
        if on_line < MAX_CALL_FACTS_PER_LINE {
            calls.push(fact);
        }
    }
}

/// `(owner leaf name, representative call text)` for each bounded chain on the line.
pub(in crate::analysis::extract) fn synthetic_calls_in_line(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let needle = ".map(";
    let mut search = 0;
    while let Some(at) = line[search..].find(needle) {
        let map_at = search + at;
        search = map_at + needle.len();
        let chain = match parse_chain_at(line, map_at) {
            Some(chain) => chain,
            None => continue,
        };
        let Some(first) = chain.inputs.first() else {
            continue;
        };
        let Some(input) = integer_element(first) else {
            continue;
        };
        let leaf = chain
            .owner_path
            .rsplit("::")
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(chain.owner_path.as_str());
        out.push((leaf.to_string(), format!("{}({input})", leaf)));
    }
    out
}

/// Parse a bounded iterator chain when `.map(` starts at `map_at`.
pub(in crate::analysis) fn parse_chain_at(line: &str, map_at: usize) -> Option<IteratorChain> {
    let after_map = &line[map_at + ".map(".len()..];
    let close = after_map.find(')')?;
    let owner_path = after_map[..close].trim();
    if !simple_value_path(owner_path) {
        return None;
    }
    let rest = after_map[close + 1..].trim();
    if !collect_suffix(rest) {
        return None;
    }
    let prefix = line[..map_at].trim_end();
    let walk = [".into_iter()", ".iter()", ".iter_mut()"]
        .into_iter()
        .find(|suffix| prefix.ends_with(suffix))?;
    let before_walk = prefix.strip_suffix(walk)?.trim_end();
    let receiver_start = before_walk
        .rfind("vec!")
        .or_else(|| before_walk.rfind('['))
        .unwrap_or(0);
    let receiver = before_walk[receiver_start..].trim();
    let inputs = literal_sequence(receiver)?;
    Some(IteratorChain {
        inputs,
        owner_path: owner_path.to_string(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct IteratorChain {
    pub inputs: Vec<String>,
    pub owner_path: String,
}

pub(in crate::analysis) fn parse_iterator_chain(text: &str) -> Option<IteratorChain> {
    let text = collapse_spaces(text.trim());
    let needle = ".map(";
    let at = text.find(needle)?;
    parse_chain_at(&text, at)
}

fn collect_suffix(text: &str) -> bool {
    let text = text.trim();
    if !text.starts_with(".collect::<Vec") {
        return false;
    }
    let Some(open) = text.find('(') else {
        return false;
    };
    let mut depth = 0i32;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let after = text[open + offset + 1..].trim_start();
                    return after.is_empty() || after.starts_with([',', ')', ';']);
                }
            }
            _ => {}
        }
    }
    false
}

fn simple_value_path(path: &str) -> bool {
    let path = path.trim();
    if path.is_empty()
        || path.contains(['(', ')', '{', '}', '|', '<', '>'])
        || path.starts_with('&')
    {
        return false;
    }
    path.split("::").all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !segment.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// A whole element read as a decimal integer literal: digits and `_`
/// separators, an optional leading `-`, and an optional integer type suffix.
/// Anything else (`0x10`, `1e3`, `1.5`, `2 * 3`) is not read, so the owner is
/// never folded on an input the test did not write.
pub(in crate::analysis) fn integer_element(text: &str) -> Option<i128> {
    const SUFFIXES: [&str; 12] = [
        "i128", "u128", "isize", "usize", "i16", "i32", "i64", "u16", "u32", "u64", "i8", "u8",
    ];
    let text = text.trim();
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, text),
    };
    let numeric = SUFFIXES
        .iter()
        .find_map(|suffix| body.strip_suffix(suffix))
        .unwrap_or(body);
    if !numeric.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    if !numeric.chars().all(|ch| ch.is_ascii_digit() || ch == '_') {
        return None;
    }
    let digits: String = numeric.chars().filter(|ch| *ch != '_').collect();
    let value: i128 = digits.parse().ok()?;
    Some(if negative { -value } else { value })
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join("")
}

fn literal_sequence(argument: &str) -> Option<Vec<String>> {
    let argument = argument.trim();
    if argument.starts_with("vec!") {
        let open = argument.find('[')?;
        let close = matching_bracket(argument, open, b'[')?;
        if !argument[close + 1..].trim().is_empty() {
            return None;
        }
        return split_top_level_commas(&argument[open + 1..close]);
    }
    if argument.starts_with('[') {
        let close = matching_bracket(argument, 0, b'[')?;
        if !argument[close + 1..].trim().is_empty() {
            return None;
        }
        return split_top_level_commas(&argument[1..close]);
    }
    None
}

fn matching_bracket(text: &str, open: usize, open_byte: u8) -> Option<usize> {
    let close_byte = match open_byte {
        b'[' => b']',
        _ => return None,
    };
    let mut depth = 0i32;
    for (offset, byte) in text.as_bytes()[open..].iter().enumerate() {
        match *byte {
            b if b == open_byte => depth += 1,
            b if b == close_byte => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level_commas(text: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (offset, ch) in text.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(text[start..offset].trim().to_string());
                start = offset + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    // A trailing comma (`vec![1, 2,]`) leaves an empty last part; it is not
    // an element.
    let last = text[start..].trim();
    if !last.is_empty() {
        parts.push(last.to_string());
    }
    (!parts.is_empty() && parts.iter().all(|part| !part.is_empty())).then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_map_collect_chain() {
        let line = "assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);";
        let calls = synthetic_calls_in_line(line);
        assert_eq!(calls, vec![("inc".to_string(), "inc(1)".to_string())]);
    }

    #[test]
    fn a_trailing_comma_is_not_an_element_but_a_wrapped_literal_is_refused() {
        assert_eq!(
            literal_sequence("vec![1, 2,]"),
            Some(vec!["1".to_string(), "2".to_string()])
        );
        assert_eq!(literal_sequence("wrap(vec![1, 2])"), None);
        assert_eq!(literal_sequence("vec![1, 2].len()"), None);
        assert_eq!(
            literal_sequence("[1, 2] "),
            Some(vec!["1".to_string(), "2".to_string()])
        );
    }

    #[test]
    fn integer_elements_are_whole_decimal_literals_only() {
        assert_eq!(integer_element("1_000"), Some(1000));
        assert_eq!(integer_element("-7"), Some(-7));
        assert_eq!(integer_element("1u32"), Some(1));
        for unread in [
            "0x10", "1e3", "1.5", "2 * 3", "1 << 3", "1u3", "", "-", "u8",
        ] {
            assert_eq!(integer_element(unread), None, "{unread}");
        }
    }

    #[test]
    fn refuses_unbounded_paths() {
        let line = "v.iter().map(|x| inc(x)).collect::<Vec<_>>();";
        assert!(synthetic_calls_in_line(line).is_empty());
    }
}
