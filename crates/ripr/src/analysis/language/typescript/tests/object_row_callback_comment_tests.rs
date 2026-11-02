//! #12494: a line or block comment inside an inline `test.each` object-row
//! callback must not demote the row to `weakly_exposed`.

use super::*;

const QUOTED_TITLE: &str = "export function quotedTitle(input: number): string {\n  if (1000 <= input) {\n    return \"gold\";\n  }\n  return \"silver\";\n}\n";
const QUOTED_TITLE_LINE: (usize, &str) = (2, "  if (1000 <= input) {");

fn object_each_source(callback: &str) -> String {
    format!(
        "import {{ quotedTitle }} from \"../src/quoted\";\n\ntest.each([\n  {{ input: 999, out: \"silver\" }},\n  {{ input: 1000, out: \"gold\" }},\n])(\"%s\", {callback});\n"
    )
}

fn observed_and_expected(source: &str) -> Vec<(Option<String>, Option<String>, bool, usize)> {
    extract_tests(Path::new("tests/quoted.test.ts"), source)
        .iter()
        .flat_map(|test| test.assertions.iter())
        .map(|assertion| {
            (
                assertion.observed_expression.clone(),
                assertion.expected_value_or_variant.clone(),
                assertion.has_dynamic_matcher_arg,
                assertion.line,
            )
        })
        .collect()
}

fn quoted_title_finding(label: &str, source: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/quoted.ts"), QUOTED_TITLE)?;
    ts_write_file(&root.join("tests/quoted.test.ts"), source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/quoted.ts", &[QUOTED_TITLE_LINE])],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == QUOTED_TITLE_LINE.0)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

const UNCOMMENTED: &str = "(t) => {\n  expect(quotedTitle(t.input)).toBe(t.out);\n}";
const LINE_COMMENT: &str =
    "(t) => {\n  // a plain note\n  expect(quotedTitle(t.input)).toBe(t.out);\n}";
const BLOCK_COMMENT: &str =
    "(t) => {\n  /* a plain note */\n  expect(quotedTitle(t.input)).toBe(t.out);\n}";

fn expected_rows(line: usize) -> Vec<(Option<String>, Option<String>, bool, usize)> {
    vec![
        (
            Some("quotedTitle(999)".to_string()),
            Some("\"silver\"".to_string()),
            false,
            line,
        ),
        (
            Some("quotedTitle(1000)".to_string()),
            Some("\"gold\"".to_string()),
            false,
            line,
        ),
    ]
}

#[test]
fn object_each_callback_comment_pair_stays_exposed() -> Result<(), String> {
    for (label, callback, line) in [
        ("uncommented", UNCOMMENTED, 7),
        ("line-comment", LINE_COMMENT, 8),
        ("block-comment", BLOCK_COMMENT, 8),
    ] {
        let source = object_each_source(callback);
        assert_eq!(
            observed_and_expected(&source),
            expected_rows(line),
            "{label}"
        );
        let finding = quoted_title_finding(label, &source)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
        assert_evidence_lacks(&finding, "typescript_table_case_unresolved");
    }
    Ok(())
}
