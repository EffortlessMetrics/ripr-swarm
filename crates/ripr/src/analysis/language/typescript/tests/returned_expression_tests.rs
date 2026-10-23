//! #9497 (RIPR-SPEC-0243 rule 18): a changed `return` expression whose old
//! and new values agree on every literal input the strong assertions use is
//! unexercised, so its finding is `weakly_exposed`; any input that separates
//! the two, or anything the evaluator cannot decide, keeps the verdict.

use super::*;

const CLAMP: &str = "export function clamp(n: number, lo: number, hi: number): number {\n  return Math.min(hi, Math.max(lo + 1, n));\n}\n";
const CLAMP_OLD: &str = "  return Math.min(hi, Math.max(lo, n));";
const RANK: &str = "export function rank(score: number): string {\n  return score >= 90 ? \"gold\" : \"bronze\";\n}\n";
const RANK_OLD: &str = "  return score >= 90 ? \"gold\" : \"silver\";";

fn finding_for(label: &str, source: &str, removed: &str, test: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/owner.ts"), source)?;
    ts_write_file(&root.join("tests/owner.test.ts"), test)?;
    let added = source
        .lines()
        .nth(1)
        .ok_or_else(|| "fixture line absent".to_string())?
        .to_string();
    let changed = ChangedFile {
        path: PathBuf::from("src/owner.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: added,
            new_side_line: 2,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: removed.to_string(),
            new_side_line: 2,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == 2)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

/// The verdict may still be weak for another reason; what must not appear is
/// this rule's own limitation.
fn assert_not_decided_by_evaluation(finding: &Finding) {
    assert!(
        !finding
            .missing
            .iter()
            .any(|text| text.contains("evaluate to the same value")),
        "{:?}",
        finding.missing
    );
}

fn clamp_test(calls: &[(&str, &str)]) -> String {
    let mut body = String::new();
    for (call, expected) in calls {
        body.push_str(&format!("  expect({call}).toBe({expected});\n"));
    }
    format!("import {{ clamp }} from \"../src/owner\";\ntest(\"clamp\", () => {{\n{body}}});\n")
}

#[test]
fn clamp_inputs_that_miss_the_changed_lower_arm_withdraw_exposed() -> Result<(), String> {
    let test = clamp_test(&[("clamp(15, 0, 10)", "10"), ("clamp(5, 0, 10)", "5")]);
    let finding = finding_for("clamp-unexercised", CLAMP, CLAMP_OLD, &test)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|text| text.contains("evaluate to the same value")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn an_input_that_reaches_the_changed_lower_arm_keeps_exposed() -> Result<(), String> {
    let test = clamp_test(&[("clamp(5, 0, 10)", "5"), ("clamp(-3, 0, 10)", "0")]);
    let finding = finding_for("clamp-exercised", CLAMP, CLAMP_OLD, &test)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_ternary_changed_on_the_arm_no_input_selects_withdraws_exposed() -> Result<(), String> {
    let test = "import { rank } from \"../src/owner\";\ntest(\"rank\", () => {\n  expect(rank(90)).toBe(\"gold\");\n});\n";
    let finding = finding_for("rank-unexercised", RANK, RANK_OLD, test)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    let test = "import { rank } from \"../src/owner\";\ntest(\"rank\", () => {\n  expect(rank(90)).toBe(\"gold\");\n  expect(rank(89)).toBe(\"silver\");\n});\n";
    let finding = finding_for("rank-exercised", RANK, RANK_OLD, test)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_input_the_evaluator_cannot_decide_keeps_the_existing_verdict() -> Result<(), String> {
    // A computed argument, a second assertion the evaluator cannot read, and
    // a free identifier in the expression each leave the verdict alone.
    let computed = clamp_test(&[("clamp(pick(), 0, 10)", "5")]);
    let finding = finding_for("clamp-computed", CLAMP, CLAMP_OLD, &computed)?;
    assert_not_decided_by_evaluation(&finding);

    let free = "export function clamp(n: number, lo: number, hi: number): number {\n  return Math.min(hi, Math.max(lo + OFFSET, n));\n}\n";
    let test = clamp_test(&[("clamp(5, 0, 10)", "5")]);
    let finding = finding_for("clamp-free-identifier", free, CLAMP_OLD, &test)?;
    assert_not_decided_by_evaluation(&finding);
    Ok(())
}

#[test]
fn a_parameter_rebound_before_the_return_keeps_the_existing_verdict() -> Result<(), String> {
    let rebound = "export function clamp(n: number, lo: number, hi: number): number {\n  return Math.min(hi, Math.max(lo + 1, n));\n  lo = lo - 5;\n}\n";
    let test = clamp_test(&[("clamp(5, 0, 10)", "5")]);
    let finding = finding_for("clamp-rebound", rebound, CLAMP_OLD, &test)?;
    assert_not_decided_by_evaluation(&finding);
    Ok(())
}

#[test]
fn a_return_inside_a_nested_function_that_shadows_a_parameter_keeps_the_verdict()
-> Result<(), String> {
    let nested = "export function f(n: number) { return [1, 2].map(function (n) {\n  return n >= 1 ? \"a\" : \"b\";\n}); }\n";
    let old = "  return n > 1 ? \"a\" : \"b\";";
    let test = "import { f } from \"../src/owner\";\ntest(\"f\", () => {\n  expect(f(5)).toEqual([\"b\", \"a\"]);\n});\n";
    let finding = finding_for("nested-shadow", nested, old, test)?;
    assert_not_decided_by_evaluation(&finding);
    Ok(())
}

#[test]
fn an_indirect_owner_call_in_the_observed_value_keeps_the_verdict() -> Result<(), String> {
    let test = clamp_test(&[("clamp(5, 0, 10) + wrap(2)", "7")]);
    let finding = finding_for("clamp-indirect", CLAMP, CLAMP_OLD, &test)?;
    assert_not_decided_by_evaluation(&finding);
    Ok(())
}

#[test]
fn one_agreeing_and_one_differing_call_list_in_one_assertion_keeps_exposed() -> Result<(), String> {
    // `clamp(5, 0, 10)` agrees on both versions; `clamp(-3, 0, 10)` reaches the
    // changed lower arm, so the assertion discriminates and is not withdrawn.
    let test = clamp_test(&[("clamp(5, 0, 10) + clamp(-3, 0, 10)", "5")]);
    let finding = finding_for("clamp-mixed-lists", CLAMP, CLAMP_OLD, &test)?;
    assert_not_decided_by_evaluation(&finding);
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}
