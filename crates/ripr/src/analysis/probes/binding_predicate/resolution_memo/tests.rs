//! The memoized relation equals the scan for every question, including
//! when long bodies alternate on one thread, and every seam of a long
//! owner shares one scan of its body (#15189).

use super::super::scan_changed_binding_uses;
use super::{Question, resolved};
use std::cell::Cell;

/// Body lines covering uses, shadows, reassignments, closures, macros and
/// nested items, plus a long chain that pushes a body past the memo floor.
const LINES: &[&str] = &[
    "    let a = amount + 1;",
    "    let a = years;",
    "    a = 3;",
    "    if a == 1 { 1 } else if a == 2 { 2 } else { a }",
    "    if a > limit {",
    "    let f = |a| a > 2;",
    "    assert!(a > 0);",
    "    fn inner(a: u64) -> bool { a > 1 }",
    "    // if a == 9 {",
    "    }",
    "    {",
    "    a",
];

fn body_from(choices: &[u8], chain_arms: usize) -> String {
    let mut chain = String::from("    if a == 0 { 0 }");
    for arm in 1..chain_arms {
        chain.push_str(&format!(" else if a == {arm} {{ {arm} }}"));
    }
    let mut body = String::from("pub fn price(amount: u64, years: u32) -> u64 {\n");
    for choice in choices {
        body.push_str(LINES[usize::from(*choice) % LINES.len()]);
        body.push('\n');
    }
    body.push_str(&chain);
    body.push_str("\n}\n");
    body
}

fn ask<'a>(body: &'a str, binding: &'a str, changed_line: usize) -> Question<'a> {
    Question {
        binding,
        initializer: "amount + 1",
        body,
        body_start_line: 1,
        changed_line,
    }
}

fn direct_scan(question: Question<'_>) -> super::BindingPredicateResolution {
    scan_changed_binding_uses(
        question.binding,
        question.initializer,
        question.body,
        question.body_start_line,
        question.changed_line,
    )
}

thread_local! {
    static SCANS: Cell<usize> = const { Cell::new(0) };
}

fn counting_scan(
    binding: &str,
    initializer: &str,
    body: &str,
    body_start_line: usize,
    changed_line: usize,
) -> super::BindingPredicateResolution {
    SCANS.with(|scans| scans.set(scans.get() + 1));
    scan_changed_binding_uses(binding, initializer, body, body_start_line, changed_line)
}

#[test]
fn memoized_relation_equals_the_scan_across_alternating_bodies() -> Result<(), String> {
    let mut bodies = Vec::new();
    for seed in 0u8..16 {
        let choices: Vec<u8> = (0..6)
            .map(|step| seed.wrapping_mul(5).wrapping_add(step * 7))
            .collect();
        // Short bodies bypass the memo; long ones enter it.
        bodies.push(body_from(&choices, 2));
        bodies.push(body_from(&choices, 120));
    }
    for round in 0..2 {
        for body in &bodies {
            for binding in ["a", "amount", "f"] {
                for changed_line in 1..9 {
                    let question = ask(body, binding, changed_line);
                    let expected = direct_scan(question);
                    let answer = resolved(question, scan_changed_binding_uses);
                    if answer != expected {
                        return Err(format!(
                            "round {round}: memo differs for `{binding}` changed on line {changed_line}:\n{body}"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn every_seam_of_a_long_owner_shares_one_scan_of_its_body() -> Result<(), String> {
    const SEAMS: usize = 1_900;
    let body = body_from(&[0], SEAMS);
    SCANS.with(|scans| scans.set(0));
    let first = resolved(ask(&body, "a", 2), counting_scan);
    for _seam in 1..SEAMS {
        if resolved(ask(&body, "a", 2), counting_scan) != first {
            return Err("a repeat question changed its answer".into());
        }
    }
    let scans = SCANS.with(Cell::get);
    if scans != 1 {
        return Err(format!(
            "{SEAMS} seams on one body scanned it {scans} times, not once"
        ));
    }
    // A different question on the same body is its own scan.
    let _ = resolved(ask(&body, "a", 3), counting_scan);
    let scans = SCANS.with(Cell::get);
    if scans != 2 {
        return Err(format!(
            "a new changed line reused another answer ({scans} scans)"
        ));
    }
    Ok(())
}
