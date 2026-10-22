//! RIPR-SPEC-0248 rule 2: a rounding-function swap keeps `exposed` only when
//! a tested input makes the two functions differ.

use super::*;

fn swap(old: &str, new: &str) -> Option<(&'static str, &'static str, String)> {
    ts_rounding_swap(old, new).map(|s| (s.old_function, s.new_function, s.operand))
}

#[test]
fn rounding_swap_reads_one_function_replaced_by_another() {
    assert_eq!(
        swap(
            "    return new Temperature(Math.round(((f - 32) * 5) / 9));",
            "    return new Temperature(Math.floor(((f - 32) * 5) / 9));"
        ),
        Some(("round", "floor", "((f - 32) * 5) / 9".to_string()))
    );
    assert_eq!(
        swap("return Math.trunc(x / 2);", "return Math.ceil(x / 2);"),
        Some(("trunc", "ceil", "x / 2".to_string()))
    );
    for (old, new) in [
        ("return Math.round(x);", "return Math.round(x);"),
        ("return Math.round(x);", "return Math.abs(x);"),
        ("return Math.round(x);", "return Math.floor(x + 1);"),
        ("return Math.round(x);", "return Math.floor(y);"),
        ("return Math.round(x);", "return x;"),
        (
            "return Math.round(a) + Math.round(b);",
            "return Math.floor(a) + Math.floor(b);",
        ),
        ("return Math.round(a, b);", "return Math.floor(a, b);"),
        ("return xMath.round(x);", "return xMath.floor(x);"),
    ] {
        assert_eq!(swap(old, new), None, "{old} -> {new}");
    }
}

const TEMPERATURE: &str = "export class Temperature {\n  constructor(readonly celsius: number) {}\n\n  static fromFahrenheit(f: number): Temperature {\n    return new Temperature(Math.round(((f - 32) * 5) / 9));\n  }\n}\n";
const TEMPERATURE_OLD: &str = "    return new Temperature(Math.round(((f - 32) * 5) / 9));";

const SCALE: &str = "export function scale(x: number): number {\n  return Math.round(x / 2);\n}\n";
const SCALE_OLD: &str = "  return Math.round(x / 2);";

fn rounding_finding(
    label: &str,
    source: &str,
    old_line: &str,
    new_line: &str,
    line: usize,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let module = source.replace(old_line, new_line);
    ts_write_file(&root.join("src/subject.ts"), &module)?;
    ts_write_file(&root.join("tests/subject.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/subject.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: new_line.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: old_line.to_string(),
            new_side_line: line,
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
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn temperature_test(calls: &[(&str, &str)]) -> String {
    let mut text = String::from(
        "import { expect, it } from \"vitest\";\nimport { Temperature } from \"../src/subject\";\n",
    );
    for (index, (argument, expected)) in calls.iter().enumerate() {
        text.push_str(&format!(
            "\nit(\"converts {index}\", () => {{\n  expect(Temperature.fromFahrenheit({argument}).celsius).toBe({expected});\n}});\n"
        ));
    }
    text
}

fn temperature(label: &str, new_fn: &str, calls: &[(&str, &str)]) -> Result<Finding, String> {
    rounding_finding(
        label,
        TEMPERATURE,
        TEMPERATURE_OLD,
        &TEMPERATURE_OLD.replace("Math.round", &format!("Math.{new_fn}")),
        5,
        &temperature_test(calls),
    )
}

#[test]
fn whole_pre_rounding_values_leave_the_rounding_mode_unreached() -> Result<(), String> {
    let finding = temperature("round-whole", "floor", &[("212", "100"), ("32", "0")])?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("pre-rounding value is whole")
                && line.contains("`Math.round` for `Math.floor`")),
        "{:?}",
        finding.missing
    );
    let summary = &finding.ripr.reveal.discriminate.summary;
    assert!(
        summary.contains("withdrew the discrimination credit"),
        "{summary}"
    );
    Ok(())
}

#[test]
fn a_fractional_input_where_the_functions_differ_keeps_the_credit() -> Result<(), String> {
    // (100 - 32) * 5 / 9 = 37.77..: round is 38, floor is 37.
    let finding = temperature(
        "round-fractional",
        "floor",
        &[("212", "100"), ("100", "38")],
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // The same input does not tell round from ceil apart.
    let finding = temperature("round-ceil-same", "ceil", &[("212", "100"), ("100", "38")])?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("rounds the same way under both functions")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_fractional_input_both_functions_round_alike_stays_unreached() -> Result<(), String> {
    // (34 - 32) * 5 / 9 = 1.11..: round and floor are both 1.
    let finding = temperature("round-alike", "floor", &[("34", "1")])?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn an_unreadable_call_keeps_the_credit() -> Result<(), String> {
    let finding = temperature(
        "round-variable",
        "floor",
        &[("212", "100"), ("boiling", "100")],
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    let finding = temperature("round-expression", "floor", &[("100 + 0", "38")])?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

fn scale_test(calls: &[(&str, &str)]) -> String {
    let mut text = String::from(
        "import { expect, it } from \"vitest\";\nimport { scale } from \"../src/subject\";\n",
    );
    for (index, (argument, expected)) in calls.iter().enumerate() {
        text.push_str(&format!(
            "\nit(\"scales {index}\", () => {{\n  expect(scale({argument})).toBe({expected});\n}});\n"
        ));
    }
    text
}

fn scale(
    label: &str,
    old_fn: &str,
    new_fn: &str,
    calls: &[(&str, &str)],
) -> Result<Finding, String> {
    let source = SCALE.replace("Math.round", &format!("Math.{old_fn}"));
    let old_line = SCALE_OLD.replace("Math.round", &format!("Math.{old_fn}"));
    rounding_finding(
        label,
        &source,
        &old_line,
        &old_line.replace(&format!("Math.{old_fn}"), &format!("Math.{new_fn}")),
        2,
        &scale_test(calls),
    )
}

#[test]
fn negative_halves_follow_each_functions_direction() -> Result<(), String> {
    // -3 / 2 = -1.5: round gives -1 (halves go up), floor -2, ceil -1, trunc -1.
    let finding = scale("neg-round-floor", "round", "floor", &[("-3", "-1")])?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    let finding = scale("neg-round-ceil", "round", "ceil", &[("-3", "-1")])?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    let finding = scale("neg-trunc-floor", "trunc", "floor", &[("-3", "-1")])?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // 3 / 2 = 1.5: trunc and floor agree, round and trunc do not.
    let finding = scale("pos-trunc-floor", "trunc", "floor", &[("3", "1")])?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    let finding = scale("pos-round-trunc", "round", "trunc", &[("3", "2")])?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // Even inputs are whole.
    let finding = scale(
        "even-round-floor",
        "round",
        "floor",
        &[("4", "2"), ("-8", "-4")],
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_module_function_with_a_non_literal_input_keeps_the_credit() -> Result<(), String> {
    let finding = scale(
        "fn-variable",
        "round",
        "floor",
        &[("4", "2"), ("half", "2")],
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}
