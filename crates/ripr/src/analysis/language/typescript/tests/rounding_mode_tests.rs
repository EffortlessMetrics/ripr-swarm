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

fn loop_scale_test(table: &str, extra: &str) -> String {
    format!(
        "import {{ expect, it }} from \"vitest\";\nimport {{ scale }} from \"../src/subject\";\n\nconst rows: Array<[number, number]> = [{table}];\n{extra}\nfor (const [input, expected] of rows) {{\n  it(`scales ${{input}}`, () => {{\n    expect(scale(input)).toBe(expected);\n  }});\n}}\n"
    )
}

fn loop_scale(label: &str, test: &str) -> Result<Finding, String> {
    rounding_finding(
        label,
        SCALE,
        SCALE_OLD,
        &SCALE_OLD.replace("Math.round", "Math.floor"),
        2,
        test,
    )
}

#[test]
fn loop_registered_whole_rows_leave_the_rounding_mode_unreached() -> Result<(), String> {
    let finding = loop_scale("loop-whole", &loop_scale_test("[4, 2], [-8, -4]", ""))?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // 3 / 2 = 1.5: round gives 2, floor 1.
    let finding = loop_scale("loop-fraction", &loop_scale_test("[4, 2], [3, 2]", ""))?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_loop_table_that_is_touched_or_computed_keeps_the_credit() -> Result<(), String> {
    let touched = loop_scale_test("[4, 2]", "rows.push([3, 2]);");
    assert_eq!(
        loop_scale("loop-pushed", &touched)?.class,
        ExposureClass::Exposed
    );
    let computed = loop_scale_test("[4, 2], [half, 2]", "");
    assert_eq!(
        loop_scale("loop-computed", &computed)?.class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn loop_rows_resolve_only_a_single_untouched_literal_table() {
    use super::super::loop_rows::ts_enclosing_loop_rows;
    let body = "it(\"x\", () => {\n  expect(f(a)).toBe(b);\n});";
    let file = |header: &str, table: &str| format!("{table}\n{header} {{\n  {body}\n}}\n");
    let table = "const rows = [[1, 100], [2.5, 250],];";
    let rows = ts_enclosing_loop_rows(&file("for (const [a, b] of rows)", table), body);
    assert_eq!(
        rows.as_ref().map(|r| r.expand(&["a".to_string()])),
        Some(vec![vec!["1".to_string()], vec!["2.5".to_string()]])
    );
    for (header, table) in [
        ("for (const [a, b] of rows)", "let rows = [[1, 100]];"),
        ("for (const [a, b] of other)", table),
        ("for (const {a, b} of rows)", table),
        ("for (let [a, b] of rows)", table),
        ("for (const [a, b] of rows.slice())", table),
        ("for (const [a = 1, b] of rows)", table),
        (
            "for (const [a, b] of rows)",
            "const rows = [[1, 100], row];",
        ),
        ("for (const [a, b] of rows)", "const rows = [[1]];"),
    ] {
        assert_eq!(
            ts_enclosing_loop_rows(&file(header, table), body),
            None,
            "{header} / {table}"
        );
    }
    assert_eq!(
        ts_enclosing_loop_rows(&format!("{table}\n{body}\n"), body),
        None
    );
}

const HALF: &str =
    "export function half(a: number, b: number): number {\n  return Math.round(b / 2);\n}\n";
const HALF_OLD: &str = "  return Math.round(b / 2);";

fn half(label: &str, call: &str) -> Result<Finding, String> {
    rounding_finding(
        label,
        HALF,
        HALF_OLD,
        &HALF_OLD.replace("Math.round", "Math.floor"),
        2,
        &format!(
            "import {{ expect, it }} from \"vitest\";\nimport {{ half }} from \"../src/subject\";\n\nit(\"halves\", () => {{\n  expect({call}).toBe(3);\n}});\n"
        ),
    )
}

#[test]
fn a_spread_argument_leaves_later_positions_unread() -> Result<(), String> {
    // Without the spread, `half(4, 5)` binds b = 5: 2.5 rounds to 3 and floors
    // to 2, so the call tells the functions apart.
    let plain = half("round-plain", "half(4, 5)")?;
    assert_eq!(plain.class, ExposureClass::Exposed);
    // An empty spread shifts `4` into `a` and `5` into `b`, but by position
    // alone `4` would bind `b` (2, whole) and withdraw the credit.
    let spread = half("round-spread", "half(...[], 4, 5)")?;
    assert_eq!(spread.class, ExposureClass::Exposed, "{:?}", spread.missing);
    Ok(())
}

#[test]
fn every_positional_reader_marks_positions_after_a_spread_unreadable() {
    // `owner_call_arguments`, the whole-call reader and the member-call reader
    // feed the rounding, dropped-term, boundary, boolean-pin, lookup-table and
    // object-argument guards (#9645).
    let unreadable_after = |prefix: &[&str]| {
        let mut arguments: Vec<String> = prefix.iter().map(|a| a.to_string()).collect();
        let start = arguments.len();
        arguments.extend((start..16).map(|position| format!("...#{position}")));
        arguments
    };
    let calls = owner_call_arguments(
        "half(1, 2) + half(...xs, 4, 5) + half(1, ...xs, 5)",
        "half",
        &[],
    );
    assert_eq!(
        calls,
        vec![
            vec!["1".to_string(), "2".to_string()],
            unreadable_after(&[]),
            unreadable_after(&["1"]),
        ]
    );
    assert_eq!(
        positional_prefix(vec![
            "1".to_string(),
            " ...rest".to_string(),
            "3".to_string()
        ]),
        unreadable_after(&["1"])
    );
    assert_eq!(
        positional_prefix(vec!["a".to_string(), "b".to_string()]),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn unreadable_spread_positions_never_witness_equal_arguments() {
    // `isAllowed(...xs)` may pass any values, so no two of its positions are
    // known to be equal (#10476 review); a real duplicate before the spread is.
    let params = vec!["count".to_string(), "limit".to_string()];
    let spread = positional_prefix(vec!["...xs".to_string()]);
    assert!(!call_has_identical_arguments(&spread, &params));
    let after = positional_prefix(vec!["5".to_string(), "...xs".to_string()]);
    assert!(!call_has_identical_arguments(&after, &params));
    let before = positional_prefix(vec!["5".to_string(), "5".to_string(), "...xs".to_string()]);
    assert!(call_has_identical_arguments(&before, &params));
}
