//! RIPR-SPEC-0243: `test(title, name)` runs the same-file function `name`, so
//! that function's assertions are the test's (mined from jest's
//! `function sumsEveryValue() {...}; test("...", sumsEveryValue)` shape). The
//! corpus case `jest-mined-test-fn-reference` pins the credited reading;
//! these pin the references that must stay unresolved.

use super::*;

const OWNER: &str = "export function sumAll(values: number[]): number {\n  let total = 0;\n  for (const value of values) {\n    total += value;\n  }\n  return total;\n}\n";
const CHANGED_LINE: (usize, &str) = (4, "    total += value;");
const HEADER: &str =
    "import { test, expect, describe } from 'vitest';\nimport { sumAll } from '../src/sum';\n\n";
const DECLARATION: &str = "function sumsEveryValue() {\n  expect(sumAll([2, 3, 4])).toBe(9);\n}\n";

fn class_of(label: &str, test_source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/sum.ts"), OWNER)?;
    ts_write_file(&root.join("tests/sum.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/sum.ts", &[CHANGED_LINE])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    Ok(result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("::sumAll"))
        })
        .map_or(ExposureClass::NoStaticPath, |finding| finding.class))
}

/// What the same assertion written as an inline callback earns: the class a
/// resolved reference must match, and an unresolved one must not.
fn inline_class() -> Result<ExposureClass, String> {
    let inline =
        file("test('sum adds every value', () => {\n  expect(sumAll([2, 3, 4])).toBe(9);\n});\n");
    let class = class_of("inline", &inline)?;
    assert_ne!(class, ExposureClass::NoStaticPath, "inline fixture");
    Ok(class)
}

fn file(body: &str) -> String {
    format!("{HEADER}{body}")
}

#[test]
fn a_named_function_registered_as_the_callback_credits_the_owner() -> Result<(), String> {
    let inline = inline_class()?;
    let registration = "test('sum adds every value', sumsEveryValue);\n";
    for (label, source) in [
        (
            "declared-first",
            file(&format!("{DECLARATION}{registration}")),
        ),
        ("hoisted", file(&format!("{registration}{DECLARATION}"))),
        (
            "async",
            file(&format!(
                "{}{registration}",
                DECLARATION.replace("function", "async function")
            )),
        ),
        (
            "only",
            file(&format!(
                "{DECLARATION}{}",
                registration.replace("test(", "test.only(")
            )),
        ),
        (
            "inside-describe",
            file(&format!(
                "{DECLARATION}describe('sum', () => {{\n  {registration}}});\n"
            )),
        ),
    ] {
        assert_eq!(class_of(label, &source)?, inline, "{label}");
    }
    Ok(())
}

#[test]
fn a_reference_that_could_name_another_function_credits_nothing() -> Result<(), String> {
    let inline = inline_class()?;
    let registration = "test('sum adds every value', sumsEveryValue);\n";
    for (label, extra) in [
        ("called-elsewhere", "sumsEveryValue();\n"),
        ("reassigned", "sumsEveryValue = () => {};\n"),
        (
            "second-declaration",
            "function sumsEveryValue() {\n  return 1;\n}\n",
        ),
        ("exported", "export { sumsEveryValue };\n"),
        (
            "same-name-parameter",
            "function other(x, sumsEveryValue) {\n  return x;\n}\n",
        ),
        ("named-in-a-title", "test('sumsEveryValue', () => {});\n"),
    ] {
        let source = file(&format!("{DECLARATION}{registration}{extra}"));
        assert_ne!(class_of(label, &source)?, inline, "{label}");
    }
    Ok(())
}

#[test]
fn a_function_the_runner_cannot_call_bare_credits_nothing() -> Result<(), String> {
    let inline = inline_class()?;
    let registration = "test('sum adds every value', sumsEveryValue);\n";
    for (label, declaration) in [
        (
            "takes-a-parameter",
            DECLARATION.replace("sumsEveryValue()", "sumsEveryValue(done)"),
        ),
        ("generator", DECLARATION.replace("function ", "function* ")),
        ("exported-declaration", format!("export {DECLARATION}")),
        (
            "arrow-variable",
            "const sumsEveryValue = () => {\n  expect(sumAll([2, 3, 4])).toBe(9);\n};\n"
                .to_string(),
        ),
    ] {
        let source = file(&format!("{declaration}{registration}"));
        assert_ne!(class_of(label, &source)?, inline, "{label}");
    }
    Ok(())
}

#[test]
fn only_a_test_registration_resolves_the_reference() -> Result<(), String> {
    let inline = inline_class()?;
    for (label, registration) in [
        ("describe", "describe('sum', sumsEveryValue);\n"),
        ("skip", "test.skip('sum', sumsEveryValue);\n"),
        ("member-callee", "suite.test('sum', sumsEveryValue);\n"),
        ("each", "test.each([1])('sum', sumsEveryValue);\n"),
        ("untitled", "test(sumsEveryValue);\n"),
    ] {
        let source = file(&format!("{DECLARATION}{registration}"));
        assert_ne!(class_of(label, &source)?, inline, "{label}");
    }
    Ok(())
}

#[test]
fn a_shadowed_runner_inside_the_function_is_judged_like_the_inline_form() -> Result<(), String> {
    let shadow = "  const expect = (value: number) => ({ toBe: (_: number) => value });\n  expect(sumAll([2, 3, 4])).toBe(9);\n";
    let by_reference = file(&format!(
        "function sumsEveryValue() {{\n{shadow}}}\ntest('sum adds every value', sumsEveryValue);\n"
    ));
    let inline = file(&format!(
        "test('sum adds every value', () => {{\n{shadow}}});\n"
    ));
    assert_eq!(
        class_of("shadow-by-reference", &by_reference)?,
        class_of("shadow-inline", &inline)?
    );
    Ok(())
}
