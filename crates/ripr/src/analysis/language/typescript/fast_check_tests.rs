//! fast-check runner-to-predicate controls (#10751).

use super::tests::{ts_analysis_options, ts_unique_tempdir, ts_write_file};
use super::*;

const IDENTITY_SRC: &str = "export function identity(n: number): number {\n  return n;\n}\n";
const IDENTITY_OLD: &str = "  return n + 1;\n";

fn assertion_count(source: &str) -> usize {
    extract_tests(Path::new("tests/identity.test.ts"), source)
        .iter()
        .map(|test| test.assertions.len())
        .sum()
}

fn fast_check_test(body: &str) -> String {
    format!(
        "import fc from \"fast-check\";\nimport {{ identity }} from \"../src/identity\";\n\ntest(\"identity property\", () => {{\n  {body}\n}});\n"
    )
}

fn finding_class(body: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir("fc-class")?;
    ts_write_file(&root.join("src/identity.ts"), IDENTITY_SRC)?;
    let test = fast_check_test(body);
    ts_write_file(&root.join("tests/identity.test.ts"), &test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/identity.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: "  return n;".to_string(),
            new_side_line: 2,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: IDENTITY_OLD.to_string(),
            new_side_line: 2,
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
        .find(|finding| finding.probe.location.line == 2)
        .ok_or_else(|| "expected finding on changed line".to_string())?;
    Ok(finding.class)
}

#[test]
fn concise_arrow_test_body_credits_assert_property() -> Result<(), String> {
    let source = "import fc from \"fast-check\";\nimport { identity } from \"../src/identity\";\ntest(\"identity property\", () => fc.assert(fc.property(fc.integer(), (n) => identity(n) === n)));\n";
    assert_eq!(assertion_count(source), 1);
    Ok(())
}

#[test]
fn awaited_async_property_is_credited() -> Result<(), String> {
    let source = "import fc from \"fast-check\";\nimport { identity } from \"../src/identity\";\ntest(\"identity property\", async () => { await fc.assert(fc.asyncProperty(fc.integer(), async (n) => identity(n) === n)); });\n";
    assert_eq!(assertion_count(source), 1);
    Ok(())
}

#[test]
fn assert_property_with_returned_equality_is_credited() -> Result<(), String> {
    let body = "fc.assert(fc.property(fc.integer(), (n) => identity(n) === n));";
    let source = fast_check_test(body);
    let tests = extract_tests(Path::new("tests/identity.test.ts"), &source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions.len(), 1);
    assert_eq!(tests[0].assertions[0].matcher, "fast-check:property-return");
    assert_eq!(
        tests[0].assertions[0].oracle_kind,
        OracleKind::RelationalCheck
    );
    Ok(())
}

#[test]
fn property_without_assert_is_not_credited() -> Result<(), String> {
    let body = "fc.property(fc.integer(), (n) => identity(n) === n);";
    assert_eq!(assertion_count(&fast_check_test(body)), 0);
    assert_ne!(finding_class(body)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn ignored_check_result_is_not_credited() -> Result<(), String> {
    let body = "fc.check(fc.property(fc.integer(), (n) => identity(n) === n));";
    assert_eq!(assertion_count(&fast_check_test(body)), 0);
    Ok(())
}

#[test]
fn block_body_without_return_is_not_credited() -> Result<(), String> {
    let body = "fc.assert(fc.property(fc.integer(), (n) => { identity(n) === n; }));";
    assert_eq!(assertion_count(&fast_check_test(body)), 0);
    Ok(())
}

#[test]
fn shadowed_assert_import_is_not_credited() -> Result<(), String> {
    let source = "import { assert, property, integer } from \"fast-check\";\nimport { identity } from \"../src/identity\";\ntest(\"x\", () => {\n  const assert = () => {};\n  assert(property(integer(), (n: number) => identity(n) === n));\n});\n".to_string();
    assert_eq!(assertion_count(&source), 0);
    Ok(())
}

#[test]
fn local_framework_lookalike_without_import_is_not_credited() -> Result<(), String> {
    let source = "import { identity } from \"../src/identity\";\nconst fc = { assert: (p: unknown) => p, property: (_a: unknown, pred: (n: number) => boolean) => pred, integer: () => 0 };\ntest(\"x\", () => { fc.assert(fc.property(fc.integer(), (n) => identity(n) === n)); });\n".to_string();
    assert_eq!(assertion_count(&source), 0);
    Ok(())
}
