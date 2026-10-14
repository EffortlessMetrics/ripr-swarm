//! RIPR-SPEC-0231 rule 1 for boolean-valued operands (#7598): a comparison,
//! disjunction or `matches!` that an assertion only reads through a boolean
//! literal, a negation or an operator the classifier does not read pins no
//! value, so it never earns an exact kind.

use super::arguments::{
    AssertionComparison, assertion_comparison, boolean_assertion_macro_condition,
};
use super::patterns::{
    boolean_literal_value, has_top_level_disjunction, has_top_level_exact_equality,
    pattern_conjunct_assertions, pattern_invocation_spans, pattern_operand_negations,
    split_top_level_conjuncts, strip_not_call, top_level_comparison_operator, unwrap_parentheses,
};
use crate::analysis::extract::mask_comments_and_strings;

/// How an assertion that compares a boolean-valued operand with a boolean
/// literal reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BoolOperandReading {
    /// The assertion only states a relation, so it reads no stronger than
    /// the inequality ceiling: the operand is asserted false
    /// (`assert_eq!(r == Err(E::Bad), false)`), holds a deciding `||`, or
    /// compares with `!=`, `<`, `>`, `<=` or `>=`.
    Capped,
    /// The operand is an `&&` chain with a `matches!` conjunct and no pinning
    /// `==` conjunct asserted true, which reads like the same chain in
    /// `assert!` (rule 3): this assertion text.
    Chain(String),
}

/// The reading of an equality macro with one boolean-literal operand
/// (`assert_eq!(X, true)`, `assert_ne!(false, X)` and the `debug_` and
/// `prop_` spellings), or a boolean assertion whose whole condition is such a
/// comparison (`ensure!((a == b) == false)`), when `X` is boolean-valued: an
/// `==`, `!=`, `<`, `>`, `<=` or `>=` comparison, an `||` or an `&&` at its
/// top level, through any `!` or `.not()`. `None` leaves the assertion to
/// the other rules: `X` is a bare value or call (`assert_eq!(is_valid(x),
/// true)` pins a bool result), a whole `matches!` (its polarity rule reads
/// it), or a comparison asserted true that may pin (`assert_eq!(a == b,
/// true)`).
pub(super) fn bool_literal_comparison_reading(line: &str) -> Option<BoolOperandReading> {
    let (operand, mut positive) = compared_with_bool_literal(line)?;
    if pattern_operand_negations(&operand).is_some() {
        return None;
    }
    let mut core = unwrap_parentheses(operand.trim());
    loop {
        if let Some(rest) = core.strip_prefix('!') {
            core = unwrap_parentheses(rest.trim());
        } else if let Some(receiver) = strip_not_call(core) {
            core = unwrap_parentheses(receiver.trim());
        } else {
            break;
        }
        positive = !positive;
    }
    let disjunction = has_top_level_disjunction(core);
    let conjunction = split_top_level_conjuncts(core).len() > 1;
    let comparison =
        top_level_comparison_operator(core).and_then(|(at, width)| core.get(at..at + width));
    let relational = has_top_level_relational_operator(core);
    if !(disjunction || conjunction || comparison.is_some() || relational) {
        return None;
    }
    if !positive || disjunction {
        return Some(BoolOperandReading::Capped);
    }
    if conjunction {
        let chain = format!("assert!({core}\n);");
        return (pattern_conjunct_assertions(&chain).is_some()
            && !has_top_level_exact_equality(core))
        .then_some(BoolOperandReading::Chain(chain));
    }
    (comparison == Some("!=") || relational).then_some(BoolOperandReading::Capped)
}

/// The non-literal operand of a comparison with a boolean literal, and
/// whether the assertion states that operand is true.
fn compared_with_bool_literal(line: &str) -> Option<(String, bool)> {
    let literal = |side: &str| boolean_literal_value(mask_comments_and_strings(side).trim());
    let pick = |left: &str, right: &str, not_equal: bool| match (literal(left), literal(right)) {
        (None, Some(value)) => Some((left.to_string(), value != not_equal)),
        (Some(value), None) => Some((right.to_string(), value != not_equal)),
        _ => None,
    };
    match assertion_comparison(line)? {
        AssertionComparison::Operands {
            not_equal,
            left,
            right,
        } => pick(&left, &right, not_equal),
        AssertionComparison::Condition(condition) => {
            let condition = unwrap_parentheses(condition.trim());
            // `a && b == true` compares only `b`; the chain rules read it.
            if has_top_level_disjunction(condition)
                || split_top_level_conjuncts(condition).len() > 1
            {
                return None;
            }
            let (at, width) = top_level_comparison_operator(condition)?;
            let not_equal = condition.get(at..at + width) == Some("!=");
            pick(
                condition.get(..at)?,
                condition.get(at + width..)?,
                not_equal,
            )
        }
    }
}

/// Whether a spaced `<`, `>`, `<=` or `>=` sits outside every delimiter,
/// string and comment. Unspaced angle brackets are generics, `->` or `=>`.
fn has_top_level_relational_operator(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'<' | b'>' if depth == 0 => {
                let before = index
                    .checked_sub(1)
                    .and_then(|at| bytes.get(at))
                    .is_some_and(u8::is_ascii_whitespace);
                let next = bytes.get(index + 1);
                let after = next.is_some_and(u8::is_ascii_whitespace)
                    || (next == Some(&b'=')
                        && bytes.get(index + 2).is_some_and(u8::is_ascii_whitespace));
                if before && after {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Whether a boolean assertion's condition uses a `matches!` as an operand
/// the classifier does not read: of `^`, `<`, `>`, `<=` or `>=`
/// (`matches!(..) ^ true`), as a call argument (`Not::not(matches!(..))`,
/// `check(matches!(..))`) or as an `if`, `while` or `match` operand. Each can
/// invert or discard the pattern, so the caller caps the reading. A
/// `matches!` in a closure body, compared with `==` or `!=`, negated, or
/// joined by `&&` or `||` is read by the other rules.
pub(super) fn contains_unread_pattern_operand(line: &str) -> bool {
    let Some((_, condition)) = boolean_assertion_macro_condition(line) else {
        return false;
    };
    let masked = mask_comments_and_strings(&condition);
    pattern_invocation_spans(&condition)
        .into_iter()
        .any(|(start, end)| {
            let before = masked.get(..start).unwrap_or_default().trim_end();
            let after = masked
                .get(end..)
                .unwrap_or_default()
                .trim_start_matches(|ch: char| ch.is_whitespace() || ch == ')');
            unread_operator_before(before)
                || after.starts_with(['^', '<', '>'])
                || call_or_keyword_before(before)
        })
}

fn unread_operator_before(before: &str) -> bool {
    match before.as_bytes().last() {
        Some(b'^' | b'<') => true,
        Some(b'>') => !(before.ends_with("->") || before.ends_with("=>")),
        Some(b'=') => before.ends_with("<=") || before.ends_with(">="),
        _ => false,
    }
}

/// Whether the text before a `matches!` opens a call argument list or an
/// `if`, `while` or `match` operand, through any parentheses.
fn call_or_keyword_before(before: &str) -> bool {
    let unopened = before.trim_end_matches(|ch: char| ch.is_whitespace() || ch == '(');
    let opened = before
        .get(unopened.len()..)
        .is_some_and(|tail| tail.contains('('));
    let word_start = unopened
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map_or(0, |at| at + 1);
    let word = unopened.get(word_start..).unwrap_or_default();
    if matches!(word, "if" | "while" | "match") {
        return true;
    }
    opened && !word.is_empty()
}
