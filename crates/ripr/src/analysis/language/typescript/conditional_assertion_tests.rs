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

/// The conditional-credit walker mirrors the collector's constant-condition
/// withholding (review on #8714): a nested condition inside a branch the
/// collector discarded is never extracted, so the walker must not discover
/// it either — otherwise `if (false) { if (ok) { expect(...) } }` earns a
/// false `typescript_conditional_assertion_credit` limitation whenever any
/// other assertion in the file was credited.
#[test]
fn ts_nested_condition_in_constant_dead_branch_is_silent() -> Result<(), String> {
    let dead_consequent = "  expect(price(150)).toBe(129);\n  if (false) {\n    if (ok) {\n      expect(price(150)).toBe(130);\n    }\n  }";
    assert_eq!(
        extracted_assertions(dead_consequent).len(),
        1,
        "only the live assertion is extracted"
    );
    assert!(
        credit_gap_for_body(dead_consequent).is_none(),
        "walker must not disclose the dead consequent's nested condition"
    );
    let dead_alternate = "  if (true) {\n    expect(price(150)).toBe(131);\n  } else {\n    if (ok) {\n      expect(price(150)).toBe(130);\n    }\n  }";
    assert_eq!(
        extracted_assertions(dead_alternate).len(),
        1,
        "consequent of if (true) keeps its assertion"
    );
    assert!(
        credit_gap_for_body(dead_alternate).is_none(),
        "walker must not disclose the dead alternate's nested condition"
    );
    // Live branch of a constant `if` still recurses: the nested unevaluated
    // condition with a collected assertion earns the disclosure.
    let live_nested =
        "  if (true) {\n    if (ok) {\n      expect(price(150)).toBe(130);\n    }\n  }";
    let gap = credit_gap_for_body(live_nested)
        .ok_or_else(|| "live nested condition under constant if must still disclose".to_string())?;
    assert_eq!(
        gap.condition, "ok",
        "gap must name the nested condition: {gap:?}"
    );
    Ok(())
}

/// Constant-falsy loops: the walker must not disclose a nested condition
/// inside a body the collector withheld (review on #8714).
#[test]
fn ts_nested_condition_in_dead_loop_body_is_silent() {
    let while_dead = "  expect(price(150)).toBe(129);\n  while (false) {\n    if (ok) {\n      expect(price(150)).toBe(130);\n    }\n  }";
    assert_eq!(
        extracted_assertions(while_dead).len(),
        1,
        "only the live assertion is extracted"
    );
    assert!(
        credit_gap_for_body(while_dead).is_none(),
        "walker must not disclose the dead while body's nested condition"
    );
    let for_dead = "  expect(price(150)).toBe(129);\n  for (;false;) {\n    if (ok) {\n      expect(price(150)).toBe(130);\n    }\n  }";
    assert_eq!(
        extracted_assertions(for_dead).len(),
        1,
        "only the live assertion is extracted"
    );
    assert!(
        credit_gap_for_body(for_dead).is_none(),
        "walker must not disclose the dead for body's nested condition"
    );
    // Live loop bodies still recurse.
    let while_live =
        "  while (true) {\n    if (ok) {\n      expect(price(150)).toBe(130);\n    }\n  }";
    assert!(
        credit_gap_for_body(while_live).is_some(),
        "live while body must still disclose its nested condition"
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

/// BigInt zero is falsy in every radix, so `0x0n` / `0o0n` / `0b0n` branches
/// are dead and their assertions are withheld just like `0n` (review on
/// #8714); non-zero spellings of every radix stay truthy and collected.
#[test]
fn ts_bigint_zero_is_falsy_in_every_radix() {
    for falsy in ["0n", "0x0n", "0X0n", "0o0n", "0O0n", "0b0n", "0B0n"] {
        let assertions = extracted_assertions(&format!(
            "  if ({falsy}) {{\n    expect(price(150)).toBe(130);\n  }}"
        ));
        assert!(
            assertions.is_empty(),
            "constant-falsy BigInt {falsy} must withhold its branch: {assertions:?}"
        );
    }
    for truthy in ["1n", "0x1n", "0o1n", "0b1n", "10n", "0xffn"] {
        let assertions = extracted_assertions(&format!(
            "  if ({truthy}) {{\n    expect(price(150)).toBe(130);\n  }}"
        ));
        assert_eq!(
            assertions.len(),
            1,
            "constant-truthy BigInt {truthy} must keep its assertion: {assertions:?}"
        );
    }
}

/// Plain hex/octal/binary numeric literals without the `n` suffix parse to
/// their value, so `0x0` / `0o0` / `0b0` are constant-falsy too (review on
/// #8714).
#[test]
fn ts_radix_numeric_zero_is_falsy() {
    for falsy in ["0x0", "0o0", "0b0", "0", "0.0", "0e3"] {
        let assertions = extracted_assertions(&format!(
            "  if ({falsy}) {{\n    expect(price(150)).toBe(130);\n  }}"
        ));
        assert!(
            assertions.is_empty(),
            "constant-falsy numeric {falsy} must withhold its branch: {assertions:?}"
        );
    }
}

/// A template literal with no interpolations is truthy exactly when its
/// cooked text is non-empty (review on #8714): `if (`abc`)` always runs its
/// consequent, `if (``) ` never does.
#[test]
fn ts_template_literal_truthiness_not_inverted() {
    let truthy = extracted_assertions("  if (`abc`) {\n    expect(price(150)).toBe(130);\n  }");
    assert_eq!(
        truthy.len(),
        1,
        "non-empty template must keep its assertion: {truthy:?}"
    );
    let falsy = extracted_assertions("  if (``) {\n    expect(price(150)).toBe(130);\n  }");
    assert!(
        falsy.is_empty(),
        "empty template must withhold its branch: {falsy:?}"
    );
}

/// Consulted conditional-credit gaps are scoped to the test files this
/// diff's findings actually credited (review on #8714): the credit
/// consumption happens in the test file holding the related test, which the
/// diff usually does not touch; a gap in a test file no finding relates to
/// must not mark this analysis partial.
#[test]
fn ts_credit_gaps_scoped_to_credited_test_files() {
    let gap = |file: &str| TypeScriptConditionalCreditGap {
        file: PathBuf::from(file),
        sample_line: 4,
        condition: "flag".to_string(),
    };
    let gaps = vec![
        gap("tests/credited.test.ts"),
        gap("tests/unrelated.test.ts"),
    ];
    let consulted: std::collections::BTreeSet<String> = ["tests/credited.test.ts"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let scoped = consulted_credit_gaps(&gaps, &consulted);
    assert_eq!(
        scoped.len(),
        1,
        "only the credited file's gap may be consulted: {scoped:?}"
    );
    assert_eq!(
        scoped[0].file,
        PathBuf::from("tests/credited.test.ts"),
        "scoped gap must be the credited file's: {scoped:?}"
    );
    // No credited overlap: nothing consulted, and the summary stays silent.
    let consulted: std::collections::BTreeSet<String> =
        ["src/other.ts".to_string()].into_iter().collect();
    let scoped = consulted_credit_gaps(&gaps, &consulted);
    assert!(scoped.is_empty(), "no overlap means no consulted gaps");
    let summary = conditional_assertion_credit_summary(&scoped);
    assert!(
        matches!(summary, Ok(None)),
        "empty scoped set must produce no limitation: {summary:?}"
    );
}

/// The conditional-credit limitation rides its own kind. The changed scope
/// was fully analyzed; the caveat is conditional credit, not unanalyzed
/// files (review on #8714): mapping it to `language_scope_unsupported` made
/// both JSON and human output claim files were not analyzed and drop the
/// unevaluated condition.
#[test]
fn ts_conditional_credit_limitation_kind_is_unevaluated_condition() -> Result<(), String> {
    let gap = TypeScriptConditionalCreditGap {
        file: PathBuf::from("tests/lib.test.ts"),
        sample_line: 5,
        condition: "ok".to_string(),
    };
    let summary = conditional_assertion_credit_summary(&[&gap])?
        .ok_or_else(|| "a single consulted gap must produce a limitation".to_string())?;
    assert!(
        matches!(
            summary.kind,
            crate::analysis_outcome::AnalysisLimitationKind::UnevaluatedCondition
        ),
        "conditional credit must not ride language_scope_unsupported: {summary:?}"
    );
    Ok(())
}

/// The one-parse extraction entry point discloses the same gap as the
/// standalone detector (review on #8714): production consumes the fold, so
/// the fold is what must stay honest — live conditions disclose, dead
/// branches stay silent, both through the same walk as `extract_tests`.
#[test]
fn ts_combined_extraction_discloses_same_conditional_credit_gap() -> Result<(), String> {
    let source = test_file("  if (ok) {\n    expect(price(150)).toBe(130);\n  }");
    let (tests, gap) =
        extract_tests_and_conditional_credit_gap(Path::new("tests/lib.test.ts"), &source);
    assert_eq!(tests.len(), 1, "fixture must register its test");
    let gap = gap.ok_or_else(|| "combined extraction must disclose the gap".to_string())?;
    assert_eq!(gap.condition, "ok", "gap must name the condition: {gap:?}");
    assert_eq!(
        gap.file,
        PathBuf::from("tests/lib.test.ts"),
        "gap must name the test file: {gap:?}"
    );
    let dead = test_file(
        "  expect(price(150)).toBe(129);\n  if (false) {\n    if (ok) {\n      expect(price(150)).toBe(130);\n    }\n  }",
    );
    let (_, gap) = extract_tests_and_conditional_credit_gap(Path::new("tests/lib.test.ts"), &dead);
    assert!(
        gap.is_none(),
        "dead branch must stay silent through the fold too: {gap:?}"
    );
    Ok(())
}

/// An empty branch and the assertion after it on the SAME line must not earn
/// conditional credit: the probe reads the branch body's real content, not
/// the first/last line of its span (#8714 review).
#[test]
fn ts_empty_branch_sharing_a_line_with_the_assertion_is_benign() {
    let body = "  if (flag) {} expect(price(150)).toBe(130);";
    let assertions = extracted_assertions(body);
    assert_eq!(
        assertions.len(),
        1,
        "the assertion after the empty branch is collected and unconditional: {assertions:?}"
    );
    assert!(
        credit_gap_for_body(body).is_none(),
        "an empty branch must not earn conditional credit: {assertions:?}"
    );
}

/// Positive control for the same-line probe: a single-line branch whose body
/// HOLDS the assertion still earns the disclosure (#8714).
#[test]
fn ts_single_line_branch_holding_the_assertion_discloses() -> Result<(), String> {
    let body = "  if (flag) { expect(price(150)).toBe(130); }";
    let gap = credit_gap_for_body(body)
        .ok_or_else(|| "a one-line branch holding the assertion must disclose".to_string())?;
    assert_eq!(
        gap.condition, "flag",
        "gap must name the condition: {gap:?}"
    );
    Ok(())
}

/// Statements after an unconditional `return`/`throw`/`break`/`continue` are
/// dead: their assertions never run, so collection withholds them and the
/// finding cannot reach `exposed` on them. Everything before the exit stays
/// credited.
#[test]
fn ts_assertion_after_unconditional_exit_is_withheld() {
    for exit in ["return;", "throw new Error(\"x\");", "{ return; }"] {
        let body =
            format!("  expect(price(150)).toBe(130);\n  {exit}\n  expect(price(150)).toBe(130);");
        let assertions = extracted_assertions(&body);
        assert_eq!(
            assertions.len(),
            1,
            "only the assertion before `{exit}` may be credited: {assertions:?}"
        );
    }
    for exit in ["break;", "continue;"] {
        let body = format!(
            "  for (const n of [1]) {{\n    {exit}\n    expect(price(150)).toBe(130);\n  }}"
        );
        assert!(
            extracted_assertions(&body).is_empty(),
            "assertion after `{exit}` is dead and must be withheld"
        );
    }
}

/// A conditional exit is not decidable, so the later assertion keeps its
/// credit; and `return expect(..)` keeps its own returned assertion.
#[test]
fn ts_assertion_after_conditional_exit_stays_credited() {
    let guarded =
        extracted_assertions("  if (skip) {\n    return;\n  }\n  expect(price(150)).toBe(130);");
    assert_eq!(
        guarded.len(),
        1,
        "conditional exit keeps credit: {guarded:?}"
    );
    let returned = extracted_assertions("  return expect(price(150)).resolves.toBe(130);");
    assert_eq!(
        returned.len(),
        1,
        "returned assertion is credited: {returned:?}"
    );
}

/// Decidable `if` exits end the list too; an undecidable one-sided exit does
/// not (#9003 review).
#[test]
fn ts_decidable_if_exit_withholds_following_assertion() {
    for exit in [
        "if (true) { return; }",
        "if (true) return;",
        "if (false) { x(); } else { return; }",
        "if (flag) { return; } else { throw new Error(\"x\"); }",
    ] {
        let body = format!("  {exit}\n  expect(price(150)).toBe(130);");
        assert!(
            extracted_assertions(&body).is_empty(),
            "assertion after `{exit}` is dead and must be withheld"
        );
    }
    for live in [
        "if (flag) { return; }",
        "if (false) { return; }",
        "if (true) { x(); }",
        "if (flag) { return; } else { x(); }",
    ] {
        let body = format!("  {live}\n  expect(price(150)).toBe(130);");
        assert_eq!(
            extracted_assertions(&body).len(),
            1,
            "assertion after `{live}` stays credited"
        );
    }
}

/// A `try` that always exits makes the statements after it dead (#9138): the
/// finalizer replaces the completion when it exits, and without a finalizer
/// override the block must exit and, when a catch exists, the catch must too.
#[test]
fn ts_assertion_after_always_exiting_try_is_withheld() {
    for dead in [
        "try { return; } finally { cleanup(); }",
        "try { throw new Error(\"x\"); } finally {}",
        "try { return; } catch (e) { return; }",
        "try { x(); } catch (e) { return; } finally { return; }",
        "try { x(); } finally { throw new Error(\"x\"); }",
        "try { return; } catch (e) { throw e; } finally { cleanup(); }",
        // A bare non-throwing exit first: the catch can never run (#9150).
        "try { return; } catch (e) {}",
        "try { return; } catch (e) { log(e); }",
        "try { return 1; } catch (e) { if (y) { return; } }",
        "try { return \"ok\"; } catch (e) {}",
        "try { break; } catch (e) {}",
        "try { continue; } catch (e) {}",
        "try { return; } catch (e) {} finally { cleanup(); }",
    ] {
        let body = format!("  {dead}\n  expect(price(150)).toBe(130);");
        assert!(
            extracted_assertions(&body).is_empty(),
            "assertion after `{dead}` is dead and must be withheld"
        );
    }
}

/// Undecidable or live-completing `try` shapes keep the later assertion
/// credited: a conditional exit, a catch that falls through, a block that can
/// complete normally, a conditional finalizer exit.
#[test]
fn ts_assertion_after_try_that_can_complete_stays_credited() {
    for live in [
        "try { if (x) { return; } } finally { cleanup(); }",
        "try { throw new Error(\"x\"); } catch (e) {}",
        "try { throw new Error(\"x\"); } catch (e) { log(e); }",
        // Something can throw into the catch before the exit completes.
        "try { work(); return; } catch (e) {}",
        "try { return f(); } catch (e) {}",
        "try { return a.b; } catch (e) {}",
        "try { if (x) { return; } } catch (e) {}",
        "try { { return; } } catch (e) {}",
        "try { x(); } catch (e) { return; }",
        "try { x(); } finally { cleanup(); }",
        "try { x(); } finally { if (y) { return; } }",
        "try { work(); return; } catch (e) { if (y) { return; } }",
    ] {
        let body = format!("  {live}\n  expect(price(150)).toBe(130);");
        assert_eq!(
            extracted_assertions(&body).len(),
            1,
            "assertion after `{live}` stays credited"
        );
    }
}

/// The finalizer's own assertions and ones before the `try` stay credited
/// when the `try` always exits; only what follows is dead.
#[test]
fn ts_always_exiting_try_keeps_earlier_and_finalizer_credit() {
    let body = "  expect(price(150)).toBe(130);\n  try { return; } finally { expect(price(150)).toBe(130); }\n  expect(price(150)).toBe(130);";
    assert_eq!(
        extracted_assertions(body).len(),
        2,
        "before-try and finalizer assertions stay; the trailing one is dead"
    );
}

/// The shared predicate also stops the conditional-credit probe and walker, so
/// a dead `if (flag)` after an always-exiting `try` discloses nothing, while
/// the same shape after a completing `try` still does.
#[test]
fn ts_dead_conditional_after_always_exiting_try_yields_no_credit_gap() {
    let dead =
        "  try { return; } finally {}\n  if (flag) {\n    expect(price(150)).toBe(130);\n  }";
    assert!(
        credit_gap_for_body(dead).is_none(),
        "dead `if (flag)` after an exiting try must not disclose"
    );
    let live = "  try { x(); } finally {}\n  if (flag) {\n    expect(price(150)).toBe(130);\n  }";
    assert!(
        credit_gap_for_body(live).is_some(),
        "live conditional after a completing try still discloses"
    );
}

/// End to end: the only strong oracle sits after an always-exiting `try`, so
/// the changed predicate must not classify `exposed`; a completing `try`
/// control keeps the credit.
#[test]
fn ts_dead_assertion_after_try_finally_return_does_not_expose() -> Result<(), String> {
    let got = finding_for_body(
        "  try { return; } finally { cleanup(); }\n  expect(price(150)).toBe(130);",
    )?;
    assert_ne!(
        got.class,
        ExposureClass::Exposed,
        "an unreachable assertion must not expose the change: {got:?}"
    );
    // A bare non-throwing exit first makes an empty catch unreachable (#9150).
    let bare = finding_for_body("  try { return; } catch (e) {}\n  expect(price(150)).toBe(130);")?;
    assert_ne!(
        bare.class,
        ExposureClass::Exposed,
        "the catch cannot run, so the assertion is dead: {bare:?}"
    );
    // Live control: work() can throw into the catch, so the assertion may run.
    let live = finding_for_body(
        "  try { work(); return; } catch (e) {}\n  expect(price(150)).toBe(130);",
    )?;
    assert_eq!(
        live.class,
        ExposureClass::Exposed,
        "a throwing-capable try keeps the credited assertion: {live:?}"
    );
    let control =
        finding_for_body("  try { x(); } finally { cleanup(); }\n  expect(price(150)).toBe(130);")?;
    assert_eq!(
        control.class,
        ExposureClass::Exposed,
        "a completing try keeps the credited assertion: {control:?}"
    );
    Ok(())
}

/// An assertion inside a provably unreachable `catch` (the `try` block opens
/// with a non-throwing exit) never runs, so it earns no credit and raises no
/// conditional-credit gap (#9150); a live catch keeps both.
#[test]
fn ts_assertion_inside_unreachable_catch_is_withheld() -> Result<(), String> {
    let assertion = "expect(price(150)).toBe(130);";
    for dead in ["return;", "return 1;", "break;", "continue;"] {
        let body = format!("  try {{ {dead} }} catch (e) {{ {assertion} }}");
        assert!(
            extracted_assertions(&body).is_empty(),
            "assertion in the catch after `{dead}` is dead and must be withheld"
        );
    }
    let gap = "  try { return; } catch (e) {\n    if (flag) {\n      expect(price(150)).toBe(130);\n    }\n  }";
    assert!(
        credit_gap_for_body(gap).is_none(),
        "a conditional inside an unreachable catch must not disclose"
    );
    for live in ["work(); return;", "return f();", "throw e;"] {
        let body = format!("  try {{ {live} }} catch (e) {{ {assertion} }}");
        assert_eq!(
            extracted_assertions(&body).len(),
            1,
            "catch after `{live}` can run and stays credited"
        );
    }
    let dead = finding_for_body(&format!("  try {{ return; }} catch (e) {{ {assertion} }}"))?;
    assert_ne!(
        dead.class,
        ExposureClass::Exposed,
        "an exact assertion solely in a dead catch must not expose: {dead:?}"
    );
    let live = finding_for_body(&format!(
        "  try {{ work(); return; }} catch (e) {{ {assertion} }}"
    ))?;
    assert_eq!(
        live.class,
        ExposureClass::Exposed,
        "a reachable catch keeps its credited assertion: {live:?}"
    );
    Ok(())
}

/// A dead conditional assertion after `return;` must not produce a
/// conditional-credit limitation; the live assertion before it is credited.
#[test]
fn ts_dead_conditional_after_return_yields_no_credit_gap() {
    let body = "  expect(price(150)).toBe(130);\n  return;\n  if (flag) {\n    expect(price(150)).toBe(130);\n  }";
    let source = test_file(body);
    let (tests, gap) =
        extract_tests_and_conditional_credit_gap(Path::new("tests/lib.test.ts"), &source);
    assert_eq!(tests.len(), 1, "fixture must register its test");
    assert_eq!(
        tests[0].assertions.len(),
        1,
        "only the live assertion counts"
    );
    assert!(gap.is_none(), "dead `if (flag)` must not disclose: {gap:?}");
    assert!(
        credit_gap_for_body(body).is_none(),
        "standalone detector must agree"
    );
    let control =
        "  expect(price(150)).toBe(130);\n  if (flag) {\n    expect(price(150)).toBe(130);\n  }";
    assert!(
        credit_gap_for_body(control).is_some(),
        "live conditional control still discloses"
    );
}

/// End to end: the only strong oracle sits after `return;`, so the changed
/// predicate must not classify `exposed`.
#[test]
fn ts_dead_assertion_after_return_does_not_expose() -> Result<(), String> {
    let got = finding_for_body("  return;\n  expect(price(150)).toBe(130);")?;
    assert_ne!(
        got.class,
        ExposureClass::Exposed,
        "an unreachable assertion must not expose the change: {got:?}"
    );
    Ok(())
}
