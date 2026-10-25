//! A top-level test's call to a same-file assertion helper reads as the
//! helper's assertions, credit-or-unknown only (#9636).

use super::*;

const OWNER: &str = "export function passes(score: number): boolean {\n  return 50 <= score;\n}\n";
const IMPORTS: &str = "import assert from \"node:assert/strict\";\nimport { test } from \"node:test\";\nimport { passes } from \"../src/lib\";\n\n";
const HELPER: &str = "function checkPass(score: number, want: boolean): void {\n  assert.equal(passes(score), want);\n}\n\n";

fn source(helper: &str, body: &str) -> String {
    format!("{IMPORTS}{helper}test(\"pass mark\", () => {{\n{body}}});\n")
}

fn assertion_count(source: &str) -> usize {
    extract_tests(Path::new("tests/lib.test.ts"), source)
        .iter()
        .find(|test| test.local_name == "pass mark")
        .map_or(0, |test| test.assertions.len())
}

/// 1 when the helper-inline pass itself rewrites the source, else 0. The
/// extraction path also runs the same-file assertion-helper pre-pass
/// (RIPR-SPEC-0243 decision 19), which credits some shapes this pass refuses,
/// so the refusal tests measure this pass directly.
fn inline_rewrites(source: &str) -> usize {
    let allocator = oxc_allocator::Allocator::default();
    let file = Path::new("tests/lib.test.ts");
    let ret = oxc_parser::Parser::new(&allocator, source, source_type_for(file)).parse();
    usize::from(
        ret.errors.is_empty()
            && super::helper_inline::inline_test_helpers(&ret.program.body, source).is_some(),
    )
}

fn finding_for(source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/lib.ts"), OWNER);
    let tests = extract_tests(Path::new("tests/lib.test.ts"), source);
    classify_change(
        Path::new("src/lib.ts"),
        2,
        "  return 50 <= score;",
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "expected a finding for the changed line".to_string())
}

#[test]
fn ts_same_file_helper_called_with_literals_is_credited() -> Result<(), String> {
    let text = source(HELPER, "  checkPass(50, true);\n  checkPass(49, false);\n");
    assert_eq!(assertion_count(&text), 2);
    let finding = finding_for(&text)?;
    assert!(
        matches!(
            finding.class,
            ExposureClass::Exposed | ExposureClass::WeaklyExposed
        ),
        "{:?} {:?}",
        finding.class,
        finding.evidence
    );
    Ok(())
}

#[test]
fn ts_inlining_keeps_every_line_number() -> Result<(), String> {
    let text = source(HELPER, "  checkPass(50, true);\n  checkPass(49, false);\n");
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &text);
    let test = tests
        .iter()
        .find(|test| test.local_name == "pass mark")
        .ok_or_else(|| "the test is extracted".to_string())?;
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines
            .get(test.line - 1)
            .is_some_and(|line| line.starts_with("test(\"pass mark\"")),
        "{}",
        test.line
    );
    Ok(())
}

#[test]
fn ts_negative_a_negative_literal_is_parenthesised() -> Result<(), String> {
    let text = source(HELPER, "  checkPass(-1, false);\n");
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &text);
    let test = tests
        .iter()
        .find(|test| test.local_name == "pass mark")
        .ok_or_else(|| "the test is extracted".to_string())?;
    assert!(
        test.body_text.contains("passes((-1))"),
        "{}",
        test.body_text
    );
    Ok(())
}

/// Negative controls: each shape leaves the helper call unread, so the test
/// keeps its pre-change reading (no assertion, no related test).
#[test]
fn ts_shapes_that_are_not_provably_straight_line_stay_unknown() {
    let plain = "function checkPass(score: number, want: boolean): void {\n  assert.equal(passes(score), want);\n}\n\n";
    let cases: [(&str, String, &str); 12] = [
        (
            "conditional call",
            HELPER.to_string(),
            "  if (true) {\n    checkPass(50, true);\n  }\n",
        ),
        (
            "call inside a loop",
            HELPER.to_string(),
            "  for (let i = 0; i < 1; i++) {\n    checkPass(50, true);\n  }\n",
        ),
        (
            "non-literal argument",
            HELPER.to_string(),
            "  const n = 50;\n  checkPass(n, true);\n",
        ),
        (
            "identifier argument",
            HELPER.to_string(),
            "  checkPass(passes, true);\n",
        ),
        (
            "argument count mismatch",
            HELPER.to_string(),
            "  checkPass(50);\n",
        ),
        (
            "helper with a branch",
            "function checkPass(score: number, want: boolean): void {\n  if (score) {\n    assert.equal(passes(score), want);\n  }\n}\n\n"
                .to_string(),
            "  checkPass(50, true);\n",
        ),
        (
            "helper with a default",
            "function checkPass(score: number, want = true): void {\n  assert.equal(passes(score), want);\n}\n\n"
                .to_string(),
            "  checkPass(50, true);\n",
        ),
        (
            "helper that assigns a parameter",
            "function checkPass(score: number, want: boolean): void {\n  score = 1;\n  assert.equal(passes(score), want);\n}\n\n"
                .to_string(),
            "  checkPass(50, true);\n",
        ),
        (
            "helper used as a value",
            format!("{plain}const alias = checkPass;\n\n"),
            "  checkPass(50, true);\n",
        ),
        (
            "helper declared twice",
            format!(
                "{plain}function checkPass(score: number, want: boolean): void {{\n  assert.notEqual(passes(score), want);\n}}\n\n"
            ),
            "  checkPass(50, true);\n",
        ),
        (
            "parameter used as a member name",
            "function checkPass(score: number, want: boolean): void {\n  assert.equal(passes(1), want.score);\n}\n\n"
                .to_string(),
            "  checkPass(50, true);\n",
        ),
        (
            "parameter named inside a string",
            "function checkPass(score: number, want: boolean): void {\n  assert.equal(passes(50), want, \"score\");\n}\n\n"
                .to_string(),
            "  checkPass(50, true);\n",
        ),
    ];
    for (label, helper, body) in cases {
        let text = source(&helper, body);
        assert_eq!(inline_rewrites(&text), 0, "{label}: {text}");
    }
}

#[test]
fn ts_exported_helper_is_not_inlined() {
    let helper = "export function checkPass(score: number, want: boolean): void {\n  assert.equal(passes(score), want);\n}\n\n";
    let text = source(helper, "  checkPass(50, true);\n");
    assert_eq!(assertion_count(&text), 0);
}

#[test]
fn ts_helper_from_another_file_is_not_inlined() -> Result<(), String> {
    let text = format!(
        "{IMPORTS}import {{ checkPass }} from \"./helpers\";\n\ntest(\"pass mark\", () => {{\n  checkPass(50, true);\n}});\n"
    );
    assert_eq!(assertion_count(&text), 0);
    assert_eq!(finding_for(&text)?.class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn ts_test_with_a_declaration_or_parameter_is_not_inlined() {
    let with_local = source(
        HELPER,
        "  const passes = () => true;\n  checkPass(50, true);\n",
    );
    assert_eq!(inline_rewrites(&with_local), 0);
    let with_parameter =
        format!("{IMPORTS}{HELPER}test(\"pass mark\", (t) => {{\n  checkPass(50, true);\n}});\n");
    assert_eq!(inline_rewrites(&with_parameter), 0);
}

#[test]
fn ts_helper_called_away_from_the_boundary_is_not_exposed() -> Result<(), String> {
    let text = source(HELPER, "  checkPass(60, true);\n");
    let finding = finding_for(&text)?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "an input away from the boundary reads weakly exposed, never exposed: {:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn ts_this_arguments_or_assertion_named_parameters_are_not_inlined() {
    let cases = [
        "function checkPass(score: number, want: boolean): void {\n  assert.equal(passes(this.x), want);\n}\n\n",
        "function checkPass(score: number, want: boolean): void {\n  assert.equal(passes(arguments[0]), want);\n}\n\n",
        "function checkPass(expect: number, want: boolean): void {\n  assert.equal(passes(expect), want);\n}\n\n",
        "function checkPass(assert: number, want: boolean): void {\n  assert.equal(passes(1), want);\n}\n\n",
    ];
    for helper in cases {
        let call = if helper.contains("(expect:") || helper.contains("(assert:") {
            "  checkPass(true, true);\n"
        } else {
            "  checkPass(50, true);\n"
        };
        let text = source(helper, call);
        assert_eq!(inline_rewrites(&text), 0, "{helper}");
    }
}

#[test]
fn ts_numeric_argument_used_as_a_member_object_is_not_inlined() {
    let helper = "function checkPass(score: number, want: boolean): void {\n  assert.equal(passes(Number(score.toFixed(1))), want);\n}\n\n";
    let text = source(helper, "  checkPass(5, true);\n");
    assert_eq!(assertion_count(&text), 0);
}

fn raw(test_head: &str, body: &str) -> String {
    format!("{IMPORTS}{HELPER}{test_head} {{\n{body}}});\n")
}

#[test]
fn ts_the_rewrite_is_exactly_the_substituted_assertions() -> Result<(), String> {
    let text = source(HELPER, "  checkPass(50, true);\n  checkPass(49, false);\n");
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &text);
    let test = tests
        .iter()
        .find(|test| test.local_name == "pass mark")
        .ok_or_else(|| "the test is extracted".to_string())?;
    assert!(
        test.body_text
            .contains("  assert.equal(passes(50), true);\n  assert.equal(passes(49), false);"),
        "{}",
        test.body_text
    );
    Ok(())
}

#[test]
fn ts_opaque_files_are_not_inlined() {
    for line in ["eval(\"1\");\n", "const s = \"\\u0041\";\n"] {
        let text = format!("{line}{}", source(HELPER, "  checkPass(50, true);\n"));
        assert_eq!(inline_rewrites(&text), 0, "{line}");
    }
}

#[test]
fn ts_callbacks_that_are_not_plain_sync_arrows_are_not_inlined() {
    for head in [
        "test(\"pass mark\", async () =>",
        "test(\"pass mark\", function* ()",
        "test(\"pass mark\", function named()",
        "test(\"pass mark\", (t) =>",
    ] {
        let text = raw(head, "  checkPass(50, true);\n");
        assert_eq!(inline_rewrites(&text), 0, "{head}");
    }
}

#[test]
fn ts_a_test_nested_in_describe_is_not_inlined() {
    let text = format!(
        "{IMPORTS}{HELPER}describe(\"suite\", () => {{\n  test(\"pass mark\", () => {{\n    checkPass(50, true);\n  }});\n}});\n"
    );
    assert_eq!(inline_rewrites(&text), 0);
}

#[test]
fn ts_a_call_that_does_not_own_its_lines_is_not_inlined() {
    for body in [
        "  checkPass(50, true); checkPass(49, false);\n",
        "  checkPass(50, true); // pin\n",
        "  checkPass(\n    50, true);\n",
    ] {
        let text = source(HELPER, body);
        assert_eq!(inline_rewrites(&text), 0, "{body}");
    }
}

#[test]
fn ts_an_assertion_the_substitution_cannot_prove_is_not_inlined() {
    for assertion in [
        "assert.ok([1].some((x) => x === score));",
        "assert.equal(passes(score), want, `${score}`);",
        "assert.equal(passes(score), want, \"a\\nb\");",
        "assert.deepEqual({ score }, want);",
        "assert.ok(score++);",
        "assert.ok((score = 1));",
        "assert.ok(function () { return score; });",
    ] {
        let helper = format!(
            "function checkPass(score: number, want: boolean): void {{\n  {assertion}\n}}\n\n"
        );
        let text = source(&helper, "  checkPass(50, true);\n");
        assert_eq!(inline_rewrites(&text), 0, "{assertion}");
    }
}

#[test]
fn ts_a_body_with_a_non_expression_statement_is_not_inlined() {
    for first in ["const x = 1;", "if (true) {}", "let y;"] {
        let text = source(HELPER, &format!("  {first}\n  checkPass(50, true);\n"));
        assert_eq!(inline_rewrites(&text), 0, "{first}");
    }
}

#[test]
fn ts_compound_shift_assignment_is_not_inlined() {
    for operator in ["<<=", ">>=", ">>>="] {
        let helper = format!(
            "function checkPass(score: number, want: boolean): void {{\n  assert.ok(score {operator} 1);\n}}\n\n"
        );
        let text = source(&helper, "  checkPass(50, true);\n");
        assert_eq!(assertion_count(&text), 0, "{operator}");
    }
}

#[test]
fn ts_a_rewrite_that_fails_to_parse_keeps_the_files_other_tests() {
    let text = format!(
        "// ripr-test-force-parse-failure\n{}\ntest(\"other\", () => {{\n  assert.equal(passes(50), true);\n}});\n",
        source(HELPER, "  checkPass(50, true);\n")
    );
    let tests = extract_tests(Path::new("tests/lib.test.ts"), &text);
    let other = tests.iter().find(|test| test.local_name == "other");
    assert_eq!(other.map(|test| test.assertions.len()), Some(1));
    assert_eq!(inline_rewrites(&text), 1);
}
