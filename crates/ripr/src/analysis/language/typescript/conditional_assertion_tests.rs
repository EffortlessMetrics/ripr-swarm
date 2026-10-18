//! Assertions under constant-falsy branches are withheld at collection, and
//! assertions under unevaluated conditions keep their credit with an additive
//! disclosure (#8714).

use super::*;

const OWNER: &str = "export function price(amount: number): number {\n  if (amount > 100) {\n    return amount - 10;\n  }\n  return amount;\n}\n";

fn test_file(body: &str) -> String {
    format!("import {{ price }} from \"../src/lib\";\n\ntest(\"price\", () => {{\n{body}\n}});\n")
}

fn finding_for_body(body: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/lib.ts"), OWNER);
    assert_eq!(owners.len(), 1, "fixture must contain its exported owner");
    let source = test_file(body);
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    assert_eq!(tests.len(), 1, "fixture must register its test");
    let line_text = OWNER
        .lines()
        .nth(2)
        .ok_or_else(|| "fixture line absent".to_string())?;
    classify_change(
        Path::new("src/lib.ts"),
        3,
        line_text,
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "fixture must produce a finding".to_string())
}

fn extracted_assertions(body: &str) -> Vec<TypeScriptAssertion> {
    let source = test_file(body);
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    assert_eq!(tests.len(), 1, "fixture must register its test");
    tests[0].assertions.clone()
}

fn credit_gap_for_body(body: &str) -> Option<TypeScriptConditionalCreditGap> {
    let source = test_file(body);
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    detect_conditional_assertion_credit(Path::new("tests/lib.test.ts"), &source, &tests)
}

/// `if (false)` never runs: the assertion inside it is withheld at
/// collection, fail closed like a swallowed try body (#8714).
#[test]
fn ts_constant_falsy_if_withholds_assertion() {
    let assertions = extracted_assertions("  if (false) {\n    expect(price(150)).toBe(130);\n  }");
    assert!(
        assertions.is_empty(),
        "constant-falsy branch assertions must be withheld: {assertions:?}"
    );
}

/// The alternate of `if (false)` IS the running branch, so collection keeps
/// assertions there (#8714).
#[test]
fn ts_constant_falsy_if_keeps_else_collection() {
    let body = "  if (false) {\n    expect(price(150)).toBe(999);\n  } else {\n    expect(price(150)).toBe(130);\n  }";
    let assertions = extracted_assertions(body);
    assert_eq!(
        assertions.len(),
        1,
        "else branch of a constant-falsy if must keep its assertion: {assertions:?}"
    );
}

/// `if (true)` always runs its consequent, so collection is unchanged and no
/// disclosure is produced: the condition is decidable, not unevaluated (#8714).
#[test]
fn ts_constant_truthy_if_stays_silent() {
    let body = "  if (true) {\n    expect(price(150)).toBe(130);\n  }";
    let assertions = extracted_assertions(body);
    assert_eq!(
        assertions.len(),
        1,
        "constant-truthy if must keep its assertion: {assertions:?}"
    );
    assert!(
        credit_gap_for_body(body).is_none(),
        "a decidable condition must not produce a credit gap"
    );
}

/// An honestly unevaluated condition (`ok`) with a collected assertion inside
/// produces the disclosure, naming the condition and the branch line (#8714).
#[test]
fn ts_unevaluated_if_condition_disclosed() -> Result<(), String> {
    let body = "  if (ok) {\n    expect(price(150)).toBe(130);\n  }";
    let gap = credit_gap_for_body(body)
        .ok_or_else(|| "unevaluated condition must produce a credit gap".to_string())?;
    assert_eq!(gap.condition, "ok", "gap must name the condition: {gap:?}");
    assert_eq!(
        gap.sample_line, 4,
        "sample line is the branch line in the test file: {gap:?}"
    );
    Ok(())
}

/// Constant conditions never produce the disclosure: `while (false)` withholds
/// its body and `for (;false;)` likewise; a do-while body always runs once so
/// it keeps collection (#8714).
#[test]
fn ts_constant_loop_conditions_never_disclose() {
    let while_body = "  while (false) {\n    expect(price(150)).toBe(130);\n  }";
    assert!(
        extracted_assertions(while_body).is_empty(),
        "constant-falsy while must withhold its body"
    );
    let for_body = "  for (;false;) {\n    expect(price(150)).toBe(130);\n  }";
    assert!(
        extracted_assertions(for_body).is_empty(),
        "constant-falsy for must withhold its body"
    );
    let do_body = "  do {\n    expect(price(150)).toBe(130);\n  } while (false);";
    assert_eq!(
        extracted_assertions(do_body).len(),
        1,
        "do-while body runs at least once and keeps its assertion"
    );
    assert!(
        credit_gap_for_body(do_body).is_none(),
        "a do-while never earns the disclosure"
    );
}

/// `for (;;)` has no test: the body always runs, so it is benign and silent
/// (#8714).
#[test]
fn ts_infinite_for_stays_silent() {
    let body = "  for (;;) {\n    expect(price(150)).toBe(130);\n  }";
    assert_eq!(
        extracted_assertions(body).len(),
        1,
        "testless for must keep its assertion"
    );
    assert!(
        credit_gap_for_body(body).is_none(),
        "testless for must not produce a credit gap"
    );
}

/// A present, non-constant `for` test is unevaluated like an `if` (#8714).
#[test]
fn ts_unevaluated_for_condition_disclosed() -> Result<(), String> {
    let body = "  for (;ok;) {\n    expect(price(150)).toBe(130);\n  }";
    let gap = credit_gap_for_body(body)
        .ok_or_else(|| "unevaluated for test must produce a credit gap".to_string())?;
    assert_eq!(gap.condition, "ok", "gap must name the for test: {gap:?}");
    Ok(())
}

/// The disclosure is classification-neutral: the credited oracle under
/// `if (ok)` keeps the same `exposed` class it earned unconditionally
/// (RIPR-SPEC-0243 example 11 pins the class across #8714).
#[test]
fn ts_conditional_assertion_keeps_exposed_class() -> Result<(), String> {
    let got = finding_for_body("  if (ok) {\n    expect(price(150)).toBe(130);\n  }")?;
    assert_eq!(
        got.class,
        ExposureClass::Exposed,
        "conditional credit must not change the class: {got:?}"
    );
    Ok(())
}

/// A test file without extracted assertions never produces the gap (#8714).
#[test]
fn ts_no_assertions_no_credit_gap() {
    let source = test_file("  expect;");
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &source);
    assert!(
        tests[0].assertions.is_empty(),
        "fixture must hold no assertions"
    );
    assert!(
        detect_conditional_assertion_credit(Path::new("tests/lib.test.ts"), &source, &tests)
            .is_none(),
        "no collected assertions means no conditional-credit gap"
    );
}
