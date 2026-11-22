//! Display-only old condition for a predicate probe whose `before` is the
//! whole old line (#15420).
//!
//! The probe producer cuts `before` down to the changed shape only when the
//! edit stays inside that shape. When the same line also changed around the
//! condition (`if total >= 100 { total - 10 } ..` to
//! `if total > 200 { total - 25 } ..`), `before` stays the whole old line on
//! purpose: classifiers rebuild the old owner from it. Display surfaces then
//! pair a whole line with a bare condition. The producer calls this module
//! with both lines to read the old condition (`probe.before_condition`), so
//! display surfaces can show both sides at one granularity without changing
//! `probe.before`.

use crate::domain::{Finding, LanguageId, ProbeFamily};

/// Whether a Rust-shaped predicate finding pairs a whole old line that opens
/// a block with an `after` that is only a condition.
pub(crate) fn before_is_whole_line_over_condition(finding: &Finding) -> bool {
    finding.probe.family == ProbeFamily::Predicate
        && finding
            .probe
            .before
            .as_deref()
            .zip(finding.probe.after.as_deref())
            .is_some_and(|(before, after)| whole_line_and_condition(before, after).is_some())
}

/// The old condition the producer paired with `after` (`probe.before_condition`),
/// for a Rust predicate finding whose `before` is still the whole old line.
/// Display surfaces read it here and derive nothing themselves: the new line
/// is needed to establish the pairing and a finding does not carry it.
pub(crate) fn old_condition_of_whole_line(finding: &Finding) -> Option<String> {
    if !matches!(finding.language, None | Some(LanguageId::Rust))
        || !before_is_whole_line_over_condition(finding)
    {
        return None;
    }
    finding.probe.before_condition.clone()
}

/// The `if`/`while` condition of the old line, for a predicate whose `after`
/// is the condition of the one head on the new line while `before` is the
/// whole old line. `None` when that is not established on either line: a
/// second head anywhere on the line (also nested in a call argument), an
/// `if let` or `while let` (a struct pattern brace is not the block), a
/// match guard, a block expression inside the condition, a comparison or
/// boolean operator outside the head's condition, a new line whose head
/// condition is not `after` (the changed predicate is then somewhere else on
/// the line, such as a comparison the branch body gained), an old condition
/// equal to `after` (the condition did not change), or an old condition that
/// does not start with the identifier `after` starts with.
pub(super) fn old_condition_for_edit(new_line: &str, after: &str, before: &str) -> Option<String> {
    let (before, after) = whole_line_and_condition(before, after)?;
    let old = condition_of_single_head(before)?;
    let new = condition_of_single_head(new_line.trim())?;
    (new == after && old != after && same_leading_identifier(old, after)).then(|| old.to_string())
}

fn whole_line_and_condition<'a>(before: &'a str, after: &'a str) -> Option<(&'a str, &'a str)> {
    let before = before.trim();
    let after = after.trim();
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
            if matches!(word, "if" | "while") {
                // A head nested in a call argument or index counts too:
                // which head's condition changed is then not established.
                heads += 1;
                if depth == 0 && start.is_none() {
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
            // `[0; 4]` holds a `;` inside brackets; only a statement end counts.
            b';' if depth == 0 && start.is_some() && end.is_none() => plain = false,
            _ => {}
        }
        at += 1;
    }
    if heads != 1 || !plain {
        return None;
    }
    // A predicate probe also covers a comparison in a branch body
    // (`if enabled { total > 100 } ..`). When the line holds one outside the
    // head's condition, the head's condition may not be what changed.
    if holds_predicate_operator(masked.get(..start?)?)
        || holds_predicate_operator(masked.get(end?..)?)
    {
        return None;
    }
    let condition = line.get(start?..end?)?.trim();
    let masked_condition = masked.get(start?..end?)?.trim();
    // Masking blanks a literal's quotes too, so a condition that ends in a
    // string or character literal (`s == "{"`) is shorter once masked. Its
    // last token is then the literal, not an operator. Any other masked
    // tail is a comment before the brace: not established.
    let ends_in_literal = masked_condition.len() < condition.len();
    if ends_in_literal && !condition.ends_with(['"', '\'']) {
        return None;
    }
    // A match guard (`x if a > 1 => S { .. }`) or an operator right before
    // the brace (`if a == { b } { .. }`) means the brace found is not this
    // head's block.
    if condition.is_empty()
        || masked_condition.contains("=>")
        || (!ends_in_literal
            && masked_condition.ends_with(|last: char| "=<>!&|+-*/%^,.:".contains(last)))
    {
        return None;
    }
    Some(condition)
}

/// Whether masked code holds a comparison or boolean operator: `<`, `>`
/// (so also `<=`, `>=`, `=>`, `->` and generics, refused as not
/// established), `==`, `!=`, `&&`, `||`, or a prefix `!`. A macro's `!`
/// (`println!(..)`) follows a word and is not one.
fn holds_predicate_operator(masked: &str) -> bool {
    let bytes = masked.as_bytes();
    bytes.iter().enumerate().any(|(at, byte)| {
        let next = bytes.get(at + 1).copied();
        match byte {
            b'<' | b'>' => true,
            b'=' => next == Some(b'='),
            b'&' => next == Some(b'&'),
            b'|' => next == Some(b'|'),
            b'!' => {
                next == Some(b'=')
                    || !at
                        .checked_sub(1)
                        .is_some_and(|prev| is_word_byte(bytes[prev]))
            }
            _ => false,
        }
    })
}

/// Whether two code fragments start with the same identifier (`total` in
/// `total >= 100` and `total > 200`; `ready` in `!ready` and `ready`).
/// Literals and comments are masked first.
fn same_leading_identifier(left: &str, right: &str) -> bool {
    fn leading(text: &str) -> Option<String> {
        crate::analysis::mask_comments_and_strings(text)
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .find(|word| {
                word.chars()
                    .next()
                    .is_some_and(|first| first.is_alphabetic() || first == '_')
            })
            .map(str::to_string)
    }
    leading(left).is_some_and(|name| Some(name) == leading(right))
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::{condition_of_single_head, old_condition_for_edit, same_leading_identifier};

    /// The pairing needs both lines: the new line's one head must hold
    /// `after` as its condition.
    #[test]
    fn pairs_the_old_condition_only_when_the_new_head_holds_the_changed_predicate() {
        let old = "if total >= 100 { total - 10 } else { total }";
        assert_eq!(
            old_condition_for_edit(
                "if total > 200 { total - 25 } else { total }",
                "total > 200",
                old
            )
            .as_deref(),
            Some("total >= 100")
        );
        for (new_line, after, before) in [
            // The branch body gained the comparison; the guard is unchanged.
            (
                "if total > 5 { (total > 100) as i32 } else { 1 }",
                "total > 100",
                "if total > 5 { total - 10 } else { 0 }",
            ),
            // Only the arms changed: the condition is not its own old side.
            (
                "if a > b { b } else { a }",
                "a > b",
                "if a > b { a } else { b }",
            ),
            // A nested head on either line.
            (
                "if s.outer_ready() { consume(if s.inner() { 1 } else { 2 }) } else { 0 }",
                "s.outer_ready()",
                "if s.outer() { consume(if s.inner() { 1 } else { 3 }) } else { 0 }",
            ),
            // `before` already cut, or `after` a whole line.
            ("if total > 200 { a }", "total > 200", "total >= 100"),
            (
                "if total > 200 { total - 25 } else { total }",
                "if total > 200 { total - 25 } else { total }",
                old,
            ),
            // The new condition names something else first.
            (
                "if limit > 200 { total - 25 } else { total }",
                "limit > 200",
                old,
            ),
        ] {
            assert_eq!(
                old_condition_for_edit(new_line, after, before),
                None,
                "{before} -> {new_line}"
            );
        }
    }

    #[test]
    fn pairs_only_conditions_that_start_with_the_same_identifier() {
        assert!(same_leading_identifier("total >= 100", "total > 200"));
        assert!(same_leading_identifier("!ready", "ready"));
        assert!(!same_leading_identifier("enabled", "total >= 200"));
        // A literal is not a name.
        assert!(!same_leading_identifier("\"total\" == a", "total > 1"));
        assert!(!same_leading_identifier("1 > 0", "2 > 0"));
    }

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
            ("if c == '{' { a }", "c == '{'"),
            ("if key == [0; 4] { a }", "key == [0; 4]"),
            ("if ready { println!(\"go\") }", "ready"),
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
            // A comparison in a branch body or before the head may be the
            // probe instead of the head's condition.
            "if enabled { total > 100 } else { false }",
            "if total > 5 { (total > 100) as u32 } else { 0 }",
            "if enabled { !done } else { false }",
            "if enabled { a == b } else { false }",
            "if n > 1 { v.retain(|x| x.ok) } else { v.retain(|x| x.ok || x.new) }",
            "let ok = a && if b >= 1 { c } else { d };",
            // A comment between the condition and the brace.
            "if a == /* b */ { c } { d }",
            // No head, or a head only inside brackets.
            "total >= 100",
            "call(if a > 1 { 1 } else { 2 })",
            // A second head nested in a call argument of the branch body.
            "if s.outer() { consume(if s.inner() { 1 } else { 2 }) } else { 0 }",
            "if s.outer() { v[if s.inner() { 1 } else { 2 }] } else { 0 }",
            "S { a: 1 }",
        ] {
            assert_eq!(condition_of_single_head(line), None, "{line}");
        }
    }
}
