//! Display-only old condition for a predicate probe whose `before` is the
//! whole old line (#15420).
//!
//! The probe producer cuts `before` down to the changed shape only when the
//! edit stays inside that shape. When the same line also changed around the
//! condition (`if total >= 100 { total - 10 } ..` to
//! `if total > 200 { total - 25 } ..`), `before` stays the whole old line on
//! purpose: classifiers rebuild the old owner from it. Display surfaces then
//! pair a whole line with a bare condition. This module reads the old
//! condition out of that line so they can show both sides at one
//! granularity, without changing `probe.before`.

use crate::domain::{Finding, LanguageId, ProbeFamily};

/// Whether a Rust-shaped predicate finding pairs a whole old line that opens
/// a block with an `after` that is only a condition.
pub(crate) fn before_is_whole_line_over_condition(finding: &Finding) -> bool {
    whole_line_and_condition(finding).is_some()
}

/// The `if`/`while` condition of the old line, when `before` is a whole old
/// line over a condition-only `after` and the line has exactly one head whose
/// condition ends at its block. `None` when that is not established: a
/// second head on the line, an `if let` or `while let` (a struct pattern
/// brace is not the block), a match guard, or a block expression inside the
/// condition.
pub(crate) fn old_condition_of_whole_line(finding: &Finding) -> Option<String> {
    if !matches!(finding.language, None | Some(LanguageId::Rust)) {
        return None;
    }
    let (before, _) = whole_line_and_condition(finding)?;
    condition_of_single_head(before).map(str::to_string)
}

fn whole_line_and_condition(finding: &Finding) -> Option<(&str, &str)> {
    if finding.probe.family != ProbeFamily::Predicate {
        return None;
    }
    let before = finding.probe.before.as_deref()?.trim();
    let after = finding.probe.after.as_deref()?.trim();
    if before == after || crate::analysis::mask_comments_and_strings(after).contains('{') {
        return None;
    }
    // A block opens at bracket depth zero; a `{` inside parentheses or
    // brackets is a closure body or struct literal within a cut condition
    // (`v.iter().any(|x| { x > 1 })`), not a whole line's block.
    let mut depth = 0_usize;
    crate::analysis::mask_comments_and_strings(before)
        .bytes()
        .any(|byte| match byte {
            b'(' | b'[' => {
                depth += 1;
                false
            }
            b')' | b']' => {
                depth = depth.saturating_sub(1);
                false
            }
            b'{' => depth == 0,
            _ => false,
        })
        .then_some((before, after))
}

/// Words that, at bracket depth zero inside a condition, mean the next `{`
/// may not be the head's block, or that the head is not a plain condition.
const NOT_A_PLAIN_CONDITION: [&str; 9] = [
    "let", "match", "unsafe", "loop", "async", "const", "for", "move", "else",
];

fn condition_of_single_head(line: &str) -> Option<&str> {
    let masked = crate::analysis::mask_comments_and_strings(line);
    if masked.len() != line.len() {
        return None;
    }
    let bytes = masked.as_bytes();
    let mut depth = 0_usize;
    let mut heads = 0_usize;
    let mut start = None;
    let mut end = None;
    let mut plain = true;
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if is_word_byte(byte) {
            let word_end = bytes[at..]
                .iter()
                .position(|byte| !is_word_byte(*byte))
                .map_or(bytes.len(), |len| at + len);
            let word = masked.get(at..word_end)?;
            if depth == 0 && matches!(word, "if" | "while") {
                heads += 1;
                if start.is_none() {
                    start = Some(word_end);
                }
            } else if depth == 0
                && start.is_some()
                && end.is_none()
                && NOT_A_PLAIN_CONDITION.contains(&word)
            {
                plain = false;
            }
            at = word_end;
            continue;
        }
        match byte {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'{' if depth == 0 && start.is_some() && end.is_none() => end = Some(at),
            b';' if start.is_some() && end.is_none() => plain = false,
            _ => {}
        }
        at += 1;
    }
    if heads != 1 || !plain {
        return None;
    }
    let condition = line.get(start?..end?)?.trim();
    let masked_condition = masked.get(start?..end?)?.trim();
    // A match guard (`x if a > 1 => S { .. }`) or an operator right before
    // the brace (`if a == { b } { .. }`) means the brace found is not this
    // head's block.
    if condition.is_empty()
        || masked_condition.contains("=>")
        || masked_condition.ends_with(|last: char| "=<>!&|+-*/%^,.:".contains(last))
    {
        return None;
    }
    Some(condition)
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::condition_of_single_head;

    #[test]
    fn reads_the_condition_of_the_only_head_on_the_line() {
        for (line, condition) in [
            (
                "if total >= 100 { total - 10 } else { total }",
                "total >= 100",
            ),
            ("if total >= 100 {", "total >= 100"),
            ("} else if n >= 1 { a }", "n >= 1"),
            ("'outer: while n >= 1 { n -= 1 }", "n >= 1"),
            (
                "let fee = if amount > limit(1, 2) { 3 } else { 4 };",
                "amount > limit(1, 2)",
            ),
            (
                "if v.iter().any(|x| { *x > 1 }) { a }",
                "v.iter().any(|x| { *x > 1 })",
            ),
            ("if s == \"{\" { a } // if b {", "s == \"{\""),
            ("if é >= ü { a }", "é >= ü"),
            ("if ready()? { a }", "ready()?"),
        ] {
            assert_eq!(condition_of_single_head(line), Some(condition), "{line}");
        }
    }

    #[test]
    fn refuses_a_line_whose_condition_is_not_established() {
        for line in [
            // Two heads: which one changed is not established.
            "if a > 1 { 1 } else if b > 2 { 2 } else { 3 }",
            // A struct pattern or let chain: the first brace may be a pattern.
            "if let Shape { side } = shape { side }",
            "while let Some(x) = it.next() { x }",
            "if a > 1 && let Some(b) = c { b }",
            // A match guard is not a block head.
            "x if a > 1 => S { f: 2 },",
            "x if a > 1 => { 2 }",
            // A block expression inside the condition.
            "if unsafe { ready() } { a }",
            "if match k { 1 => true, _ => false } { a }",
            "if a == { b } { c }",
            // No head, or a head only inside brackets.
            "total >= 100",
            "call(if a > 1 { 1 } else { 2 })",
            "S { a: 1 }",
        ] {
            assert_eq!(condition_of_single_head(line), None, "{line}");
        }
    }
}
