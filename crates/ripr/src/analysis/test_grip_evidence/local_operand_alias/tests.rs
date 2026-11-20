//! The memoized alias answer equals the scan for every body, parameter
//! list and operand, including when bodies alternate on one thread, and a
//! long owner answers all of its seams in linear time (#15189).

use super::{boundary_local_operand_parameter_index, scan_local_operand_parameter_index};

/// Body lines a generated owner is built from: every alias spelling the
/// scanners read, near misses, comments that hide them, and long chains.
const LINES: &[&str] = &[
    "if let Some(v) = amount {",
    "if let Ok(v) = years {",
    "if let Some(w) = amount { // if let Some(v) = years",
    "/* if let Ok(v) = amount */",
    "match amount {",
    "match  years {",
    "// match amount {",
    "Some(v) => v,",
    "Ok(w) => w,",
    "None => 0,",
    "let v = amount;",
    "let w: u64 = years;",
    "let v = amount + 1;",
    "let v=years",
    "if a == 1 { 1 } else if a == 2 { 2 } else { a }",
    "x",
    "}",
    "",
];
const OPERANDS: &[&str] = &["v", "w", "a", "amount", "1", "", "vv"];
const PARAMETERS: &[&[&str]] = &[&["amount", "years"], &["years"], &[], &["years", "amount"]];

fn body_from(choices: &[u8]) -> String {
    choices
        .iter()
        .map(|choice| LINES[usize::from(*choice) % LINES.len()])
        .collect::<Vec<_>>()
        .join("\n")
}

fn owned(parameters: &[&str]) -> Vec<String> {
    parameters.iter().map(|name| (*name).to_string()).collect()
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        cases: 2000,
        ..proptest::test_runner::Config::default()
    })]

    #[test]
    fn memoized_answers_equal_the_scan_across_alternating_bodies(
        first in proptest::collection::vec(proptest::num::u8::ANY, 0..24),
        second in proptest::collection::vec(proptest::num::u8::ANY, 0..24),
        asks in proptest::collection::vec(
            (proptest::bool::ANY, 0..OPERANDS.len(), 0..PARAMETERS.len()),
            1..40,
        ),
    ) {
        let bodies = [body_from(&first), body_from(&second)];
        for (use_second, operand, parameters) in asks {
            let body = &bodies[usize::from(use_second)];
            let parameters = owned(PARAMETERS[parameters]);
            let operand = OPERANDS[operand];
            proptest::prop_assert_eq!(
                boundary_local_operand_parameter_index(body, &parameters, operand),
                scan_local_operand_parameter_index(body, &parameters, operand),
                "`{}` with {:?} in\n{}",
                operand,
                parameters,
                body
            );
        }
    }
}

#[test]
fn each_alias_spelling_names_its_parameter_and_a_cached_body_does_not_leak() -> Result<(), String> {
    let parameters = owned(&["amount", "years"]);
    let cases: &[(&str, &str, Option<usize>)] = &[
        ("if let Some(v) = years {\n}", "v", Some(1)),
        ("if let Ok(v) = amount {\n}", "v", Some(0)),
        ("match years {\n    Some(v) => v,\n}", "v", Some(1)),
        ("let v: u64 = amount;", "v", Some(0)),
        // A comment hides the alias, and a computed right side is no alias.
        ("// if let Some(v) = years {", "v", None),
        ("let v = amount + 1;", "v", None),
        // Same body as the first case, asked again after other bodies.
        ("if let Some(v) = years {\n}", "v", Some(1)),
        ("if let Some(v) = years {\n}", "w", None),
        ("", "v", None),
    ];
    for (body, operand, expected) in cases {
        let got = boundary_local_operand_parameter_index(body, &parameters, operand);
        if got != *expected {
            return Err(format!(
                "`{operand}` in {body:?}: expected {expected:?}, got {got:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn every_seam_of_a_long_owner_is_answered_in_linear_time() -> Result<(), String> {
    const ARMS: usize = 20_000;
    let mut body = String::from("{\n    let a = amount;\n    if a == 0 { 0 }\n");
    for arm in 1..ARMS {
        body.push_str(&format!("    else if a == {arm} {{ {arm} }}\n"));
    }
    body.push_str("    else { a }\n}\n");
    let parameters = owned(&["amount", "years"]);
    let started = std::time::Instant::now();
    for arm in 0..ARMS {
        // Each `a == N` seam resolves its left operand, a local of `amount`,
        // and stops there.
        if boundary_local_operand_parameter_index(&body, &parameters, "a") != Some(0) {
            return Err(format!("seam {arm}: `a` must alias `amount`"));
        }
    }
    if boundary_local_operand_parameter_index(&body, &parameters, "1").is_some() {
        return Err("a literal aliases nothing".to_string());
    }
    let elapsed = started.elapsed();
    // Scanning per seam reads the 20,000-line body 20,000 times (minutes);
    // the memo reads it once and then compares the body text per seam.
    if elapsed > std::time::Duration::from_secs(20) {
        return Err(format!("{ARMS} seams took {elapsed:?}"));
    }
    Ok(())
}
