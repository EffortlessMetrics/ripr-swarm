//! #10822: write-shaped text inside a plain string literal (`"?test=1"`) is
//! data. The hook-write scan used to read it as an assignment to the runner
//! and withhold every registration in the file.

use super::*;

fn registered(source: &str) -> usize {
    extract_tests(Path::new("tests/a.test.ts"), source).len()
}

const IMPORT: &str = "import { expect, test } from \"vitest\";\n";

#[test]
fn a_string_literal_containing_a_runner_assignment_does_not_withhold_the_test() {
    let plain = format!("{IMPORT}test(\"a\", () => {{\n  expect(1).toBe(\"x?foo=1\");\n}});\n");
    assert_eq!(registered(&plain), 1, "the control registers");
    for literal in ["\"x?test=1\"", "'x?test=1'", "\"test = 1\""] {
        let source = format!("{IMPORT}test(\"a\", () => {{\n  expect(1).toBe({literal});\n}});\n");
        assert_eq!(registered(&source), 1, "{literal}");
    }
    let declared = format!(
        "{IMPORT}test(\"a\", () => {{\n  const url = \"http://e.com?test=123\";\n  expect(url).toBe(1);\n}});\n"
    );
    assert_eq!(registered(&declared), 1, "a declaration initializer string");
}

#[test]
fn a_real_write_to_the_runner_still_withholds_registration() {
    let template =
        format!("{IMPORT}test(\"a\", () => {{\n  expect(1).toBe(`${{test = 1}}`);\n}});\n");
    let direct = format!("{IMPORT}test(\"a\", () => {{\n  test = 1;\n  expect(1).toBe(1);\n}});\n");
    for source in [template, direct] {
        assert_eq!(registered(&source), 0, "{source}");
    }
}

#[test]
fn a_string_handed_to_eval_or_function_stays_visible() {
    for call in ["eval(\"test = 1\")", "new Function(\"test = 1\")()"] {
        let source =
            format!("{IMPORT}test(\"a\", () => {{\n  {call};\n  expect(1).toBe(1);\n}});\n");
        assert_eq!(registered(&source), 0, "{call}");
    }
}
