//! #11738: the memoized scalar exits give every changed line the gate answer
//! the per-call loop gave, and one owner body is read once per scope however
//! many of its `return`s change.

use super::super::{
    ReturnPathGate, body_block_open, collapse_whitespace, evaluates_conditionally,
    mask_comments_and_strings, matching_close, scalar_early_return_exit, top_level_tail_starts,
    whole_word_offsets,
};
use super::SCALAR_EXIT_READS;
use crate::analysis::classify::boundary_pairing::{is_whole_decimal_literal, same_scalar_literal};
use crate::analysis::syntax::{returns_leave_reads, returns_leave_the_function, run_memo};

/// `scalar_early_return_gate` as it was before #11738, verbatim.
fn reference_gate(
    body: &str,
    inner: &str,
    inner_text: &str,
    changed: &str,
    spans_changed_line: &dyn Fn(usize, usize) -> bool,
) -> Option<ReturnPathGate> {
    if !is_whole_decimal_literal(changed)
        || evaluates_conditionally(&mask_comments_and_strings(changed))
        || inner.contains('?')
        || !returns_leave_the_function(body)
    {
        return None;
    }
    let mut changed_returns = 0usize;
    for offset in whole_word_offsets(inner, "return") {
        let value_start = offset + "return".len();
        let value_end = value_start + inner[value_start..].find(';')?;
        let value = collapse_whitespace(&inner_text[value_start..value_end]);
        let value = value.trim();
        if !is_whole_decimal_literal(value) {
            return None;
        }
        if same_scalar_literal(value, changed) {
            if !spans_changed_line(offset, value_end) {
                return None;
            }
            changed_returns += 1;
        }
    }
    let (after_semicolon, after_brace) = top_level_tail_starts(inner);
    let tail = collapse_whitespace(&inner_text[after_semicolon.max(after_brace)..]);
    let tail = tail.trim();
    (changed_returns == 1 && is_whole_decimal_literal(tail) && !same_scalar_literal(tail, changed))
        .then(|| ReturnPathGate::EarlyScalar {
            literal: changed.to_string(),
        })
}

/// `scalar_early_return_exit` with the reference gate, verbatim otherwise.
fn reference_exit(body: &str, expression: &str, changed_line: usize) -> Option<String> {
    let masked = mask_comments_and_strings(body);
    let open = body_block_open(&masked)?;
    let close = matching_close(&masked, open, b'{', b'}')?;
    let inner = &masked[open + 1..close];
    let inner_text = &body[open + 1..close];
    let changed = collapse_whitespace(expression.trim().trim_end_matches(';'));
    let changed = changed
        .strip_prefix("return ")
        .map_or(changed.as_str(), str::trim);
    let spans_changed_line = |start: usize, end: usize| {
        let line_of = |offset: usize| body[..open + 1 + offset].matches('\n').count();
        let text = &inner_text[start..end];
        let first = start + (text.len() - text.trim_start().len());
        let last = start + text.trim_end().len();
        (line_of(first)..=line_of(last)).contains(&changed_line)
    };
    match reference_gate(body, inner, inner_text, changed, &spans_changed_line)? {
        ReturnPathGate::EarlyScalar { literal } => Some(literal),
        _ => None,
    }
}

const BODIES: &[&str] = &[
    // The one source of `1`, of `2`, beside a distinct tail.
    "fn f(a: i32) -> i32 {\n    if a > 0 { return 1; }\n    if a > 5 { return 2; }\n    0\n}\n",
    // A second `return 1;` is a second source.
    "fn f(a: i32) -> i32 {\n    if a > 0 { return 1; }\n    if a > 5 { return 1; }\n    0\n}\n",
    // Spellings of one value: `1_000`, `1000`, `1000u32`.
    "fn f(a: u32) -> u32 {\n    if a > 0 { return 1_000; }\n    if a > 5 { return 1000u32; }\n    7\n}\n",
    "fn f(a: i32) -> i32 {\n    if a > 0 { return -1; }\n    if a > 5 { return 1i32; }\n    0\n}\n",
    // A parameter exit may alias any literal.
    "fn f(a: i32) -> i32 {\n    if a > 0 { return 1; }\n    if a > 5 { return a; }\n    0\n}\n",
    // A computed tail, and a tail of the changed literal itself.
    "fn f(a: i32) -> i32 {\n    if a > 0 { return 1; }\n    a\n}\n",
    "fn f(a: i32) -> i32 {\n    if a > 0 { return 1; }\n    1\n}\n",
    // A `?` and a closure `return`.
    "fn f(a: i32) -> Option<i32> {\n    if a > 0 { return 1; }\n    g(a)?;\n    0\n}\n",
    "fn f(a: i32) -> i32 {\n    let c = || { return 3; };\n    if a > 0 { return 1; }\n    0\n}\n",
    // A `return` with no `;` after it.
    "fn f(a: i32) -> i32 {\n    if a > 0 { return 1 }\n    0\n}\n",
    // A value spanning lines, and returns hidden in a comment and a string.
    "fn f(a: i32) -> i32 {\n    if a > 0 {\n        return\n            1;\n    }\n    0\n}\n",
    "fn f(a: i32) -> i32 {\n    // return 1;\n    let s = \"return 1;\";\n    if a > 0 { return 1; }\n    0\n}\n",
    // A tail after a closing brace.
    "fn f(a: i32) -> i32 {\n    if a > 0 { return 2; }\n    if a > 9 { 3 } else { 4 }\n}\n",
    "fn f(a: i32) -> i32 {\n    if a > 0 { return 2; }\n    match a { _ => {} }\n    5\n}\n",
    // No return at all.
    "fn f(a: i32) -> i32 {\n    0\n}\n",
];

const EXPRESSIONS: &[&str] = &[
    "1",
    "return 1;",
    "2",
    "0",
    "-1",
    "1i32",
    "1_000",
    "1000",
    "1000u32",
    "3",
    "5",
    "a",
    "",
];

fn compare_every_case() -> Result<(usize, usize), String> {
    let (mut gated, mut refused) = (0, 0);
    for body in BODIES {
        for changed_line in 0..body.lines().count() {
            for expression in EXPRESSIONS {
                let expected = reference_exit(body, expression, changed_line);
                let actual = scalar_early_return_exit(body, expression, changed_line);
                if actual != expected {
                    return Err(format!(
                        "line {changed_line} `{expression}`: {actual:?} != {expected:?}\n{body}"
                    ));
                }
                if expected.is_some() {
                    gated += 1;
                } else {
                    refused += 1;
                }
            }
        }
    }
    Ok((gated, refused))
}

#[test]
fn memoized_exits_give_the_per_call_answer_inside_and_outside_a_scope() -> Result<(), String> {
    let outside = compare_every_case()?;
    let _scope = run_memo::enter();
    let inside = compare_every_case()?;
    let again = compare_every_case()?;
    assert_eq!(outside, inside);
    assert_eq!(inside, again);
    // Premise: the corpus reaches both answers, so equality is not vacuous.
    let (gated, refused) = inside;
    assert!(gated >= 8, "only {gated} gated cases");
    assert!(refused > gated, "only {refused} refused cases");
    Ok(())
}

#[test]
fn the_gate_reads_on_the_changed_line_only() {
    let body = BODIES[0];
    assert_eq!(scalar_early_return_exit(body, "1", 1).as_deref(), Some("1"));
    assert_eq!(scalar_early_return_exit(body, "1", 2), None);
    assert_eq!(scalar_early_return_exit(body, "2", 2).as_deref(), Some("2"));
    assert_eq!(scalar_early_return_exit(BODIES[1], "1", 1), None);
}

fn long_owner(returns: usize) -> String {
    let mut body = String::from("fn big(a: i32) -> i32 {\n");
    for i in 1..=returns {
        body.push_str(&format!("    if a > {i} {{ return {i}; }}\n"));
    }
    body.push_str("    0\n}\n");
    body
}

fn reads() -> (usize, usize) {
    (
        SCALAR_EXIT_READS.with(std::cell::Cell::get),
        returns_leave_reads(),
    )
}

#[test]
fn one_long_owner_is_read_once_per_scope_however_many_returns_change() {
    let returns = 300;
    let body = long_owner(returns);
    let _scope = run_memo::enter();
    let before = reads();
    for line in 1..=returns {
        assert_eq!(
            scalar_early_return_exit(&body, &line.to_string(), line),
            Some(line.to_string()),
            "line {line} lost its gate"
        );
    }
    let after = reads();
    assert_eq!((after.0 - before.0, after.1 - before.1), (1, 1));
}

#[test]
fn outside_a_scope_every_call_reads_the_owner() {
    let body = long_owner(20);
    let before = reads();
    for line in 1..=20 {
        assert!(scalar_early_return_exit(&body, &line.to_string(), line).is_some());
    }
    let after = reads();
    assert_eq!((after.0 - before.0, after.1 - before.1), (20, 20));
}
