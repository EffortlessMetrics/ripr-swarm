//! RIPR-SPEC-0246 rule 11: a ternary-condition comparison edit keeps `exposed`
//! only when a tested input makes the two comparisons differ.

use super::*;

fn edit(old: &str, new: &str) -> Option<(String, String, String)> {
    ts_comparison_edit(old, new).map(|e| (e.parameter, e.old_text, e.new_text))
}

#[test]
fn comparison_edit_reads_one_condition_changed_in_a_template_or_ternary() {
    assert_eq!(
        edit(
            "  return `${n} item${n === 1 ? \"\" : \"s\"}`;",
            "  return `${n} item${n <= 1 ? \"\" : \"s\"}`;"
        ),
        Some(("n".to_string(), "n === 1".to_string(), "n <= 1".to_string()))
    );
    assert_eq!(
        edit(
            "return count > 10 ? \"many\" : \"few\";",
            "return count > 100 ? \"many\" : \"few\";"
        ),
        Some((
            "count".to_string(),
            "count > 10".to_string(),
            "count > 100".to_string()
        ))
    );
    assert_eq!(
        edit(
            "const label = n !== -1 ? a : b;",
            "const label = n > -1 ? a : b;"
        ),
        Some((
            "n".to_string(),
            "n !== -1".to_string(),
            "n > -1".to_string()
        ))
    );
}

#[test]
fn comparison_edit_refuses_everything_but_one_bare_condition() {
    for (old, new) in [
        // Unchanged, or another part of the line changed.
        ("return n === 1 ? a : b;", "return n == 1 ? a : b;"),
        ("return n === 1 ? a : b;", "return n === 1 ? a : b;"),
        ("return n === 1 ? a : b;", "return n === 1 ? a : c;"),
        // Two comparisons change.
        (
            "return n === 1 ? (m === 2 ? a : b) : c;",
            "return n <= 1 ? (m <= 2 ? a : b) : c;",
        ),
        // Another identifier.
        ("return n === 1 ? a : b;", "return m <= 1 ? a : b;"),
        // The comparison is not the whole condition.
        ("return k + n === 1 ? a : b;", "return k + n <= 1 ? a : b;"),
        (
            "return k && n === 1 ? a : b;",
            "return k && n <= 1 ? a : b;",
        ),
        (
            "return k === n === 1 ? a : b;",
            "return k === n <= 1 ? a : b;",
        ),
        // No ternary after it, or a nullish or optional chain instead.
        ("return n === 1;", "return n <= 1;"),
        ("return n === 1 ?? a;", "return n <= 1 ?? a;"),
        ("return n === 1 ?.a : b;", "return n <= 1 ?.a : b;"),
        // A non-literal or non-numeric right side.
        ("return n === m ? a : b;", "return n <= m ? a : b;"),
        ("return n === \"1\" ? a : b;", "return n <= \"1\" ? a : b;"),
        ("return n === 0x1 ? a : b;", "return n <= 0x1 ? a : b;"),
        // The text sits inside a string, or after a line comment.
        (
            "return \"(n === 1 ? a : b)\";",
            "return \"(n <= 1 ? a : b)\";",
        ),
        (
            "return k; // n === 1 ? a : b",
            "return k; // n <= 1 ? a : b",
        ),
        // An arrow or shift is not a comparison.
        ("return (n) => 1 ? a : b;", "return (n) => 2 ? a : b;"),
        ("return n << 1 ? a : b;", "return n << 2 ? a : b;"),
    ] {
        assert_eq!(edit(old, new), None, "{old} -> {new}");
    }
}

const PLURAL: &str = "export function plural(n: number): string {\n  return `${n} item${n <= 1 ? \"\" : \"s\"}`;\n}\n";
const PLURAL_OLD: &str = "  return `${n} item${n === 1 ? \"\" : \"s\"}`;";
const PLURAL_NEW: &str = "  return `${n} item${n <= 1 ? \"\" : \"s\"}`;";

fn comparison_finding(
    label: &str,
    source: &str,
    old_line: &str,
    new_line: &str,
    test: &str,
) -> Result<Finding, String> {
    let line = 2;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/subject.ts"), source)?;
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

fn plural_test(calls: &[(&str, &str)]) -> String {
    let mut text = String::from(
        "import { expect, it } from \"vitest\";\nimport { plural } from \"../src/subject\";\n",
    );
    for (index, (argument, expected)) in calls.iter().enumerate() {
        text.push_str(&format!(
            "\nit(\"counts {index}\", () => {{\n  expect(plural({argument})).toBe({expected});\n}});\n"
        ));
    }
    text
}

fn plural(label: &str, calls: &[(&str, &str)]) -> Result<Finding, String> {
    comparison_finding(label, PLURAL, PLURAL_OLD, PLURAL_NEW, &plural_test(calls))
}

#[test]
fn inputs_both_comparisons_treat_alike_leave_the_edit_unreached() -> Result<(), String> {
    let finding = plural(
        "plural-one-and-three",
        &[("1", "\"1 item\""), ("3", "\"3 items\"")],
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding.missing.iter().any(|line| {
            line.contains("`n === 1` into `n <= 1`") && line.contains("Add a case where `n` is `0`")
        }),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn an_input_where_the_comparisons_differ_keeps_the_credit() -> Result<(), String> {
    let finding = plural(
        "plural-with-zero",
        &[
            ("1", "\"1 item\""),
            ("3", "\"3 items\""),
            ("0", "\"0 items\""),
        ],
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    let negative = plural(
        "plural-with-negative",
        &[("1", "\"1 item\""), ("-2", "\"-2 items\"")],
    )?;
    assert_eq!(negative.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_unreadable_call_keeps_the_credit() -> Result<(), String> {
    for (label, calls) in [
        (
            "plural-variable",
            vec![("1", "\"1 item\""), ("count", "\"3 items\"")],
        ),
        ("plural-no-argument", vec![("", "\"undefined items\"")]),
    ] {
        let finding = plural(label, &calls)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn an_argument_only_the_unexercised_reader_folds_demotes_generically() -> Result<(), String> {
    // `1 + 1` is not a plain numeric literal, so the rule 11 reader refuses it
    // and the post-ladder hook stands down; the #9497 unexercised returned-
    // expression arm folds the same input and demotes at the ladder with the
    // generic changed-arm note, not the rule 11 wording.
    let finding = plural(
        "plural-expression",
        &[("1", "\"1 item\""), ("1 + 1", "\"2 items\"")],
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("the changed arm or bound is not exercised")),
        "{:?}",
        finding.missing
    );
    assert!(
        !finding
            .missing
            .iter()
            .any(|line| line.contains("`n === 1` into `n <= 1`")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_rebound_parameter_keeps_the_credit() -> Result<(), String> {
    let source = "export function plural(n: number): string {\n  return `${n} item${n <= 1 ? \"\" : \"s\"}`;\n}\n";
    let rebound = source.replace("{\n  return", "{\n  n = n - 1;\n  return");
    // The rebinding shifts the changed line down by one.
    let line_old = PLURAL_OLD;
    let line_new = PLURAL_NEW;
    let root = ts_unique_tempdir("plural-rebound")?;
    ts_write_file(&root.join("src/subject.ts"), &rebound)?;
    ts_write_file(
        &root.join("tests/subject.test.ts"),
        &plural_test(&[("1", "\"0 item\""), ("3", "\"2 items\"")]),
    )?;
    let changed = ChangedFile {
        path: PathBuf::from("src/subject.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 3,
            text: line_new.to_string(),
            new_side_line: 3,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 3,
            text: line_old.to_string(),
            new_side_line: 3,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    let finding = result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == 3)
        .ok_or("expected a finding on the changed line")?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_literal_beyond_i64_names_no_differing_input() -> Result<(), String> {
    let huge = ts_comparison_edit(
        "n === 99999999999999999999 ? 1 : 2",
        "n <= 99999999999999999999 ? 1 : 2",
    )
    .ok_or("the huge-literal edit should still read")?;
    assert_eq!(huge.differing_value(), None);
    let top = ts_comparison_edit(
        "n === 9223372036854775807 ? 1 : 2",
        "n <= 9223372036854775807 ? 1 : 2",
    )
    .ok_or("the i64::MAX edit should read")?;
    assert_eq!(top.differing_value(), None);
    Ok(())
}
