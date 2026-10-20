//! RIPR-SPEC-0243 rule 13: a top-level suite factory is expanded at each
//! call, so `runSuite("sum", sumAll)` reads like the suite written out with
//! the owner in place of the parameter (mined from jest's shared-suite
//! helpers). The corpus case `jest-mined-suite-factory` pins the credited
//! reading; these pin the calls and bodies that must stay unexpanded.

use super::*;

const OWNER: &str = "export function sumAll(values: number[]): number {\n  let total = 0;\n  for (const value of values) {\n    total += value;\n  }\n  return total;\n}\n";
const CHANGED_LINE: (usize, &str) = (4, "    total += value;");
const HEADER: &str = "import { test, expect, describe, beforeEach } from 'vitest';\nimport { sumAll } from '../src/sum';\n\n";
const FACTORY: &str = "function runSuite(name: string, impl: (values: number[]) => number) {\n  describe(name, () => {\n    test('adds every value', () => {\n      expect(impl([2, 3, 4])).toBe(9);\n    });\n  });\n}\n";
const CALL: &str = "runSuite('sum', sumAll);\n";

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

fn file(body: &str) -> String {
    format!("{HEADER}{body}")
}

/// What the suite written out with the owner in place earns: the class an
/// expanded call must match, and an unexpanded one must not.
fn written_out_class() -> Result<ExposureClass, String> {
    let written_out = file(
        "describe('sum', () => {\n  test('adds every value', () => {\n    expect(sumAll([2, 3, 4])).toBe(9);\n  });\n});\n",
    );
    let class = class_of("written-out", &written_out)?;
    assert_ne!(class, ExposureClass::NoStaticPath, "written-out fixture");
    Ok(class)
}

#[test]
fn a_called_suite_factory_reads_like_the_written_out_suite() -> Result<(), String> {
    let written_out = written_out_class()?;
    for (label, source) in [
        ("call-after", file(&format!("{FACTORY}{CALL}"))),
        ("call-before-declaration", file(&format!("{CALL}{FACTORY}"))),
        (
            "two-calls",
            file(&format!("{FACTORY}{CALL}runSuite('again', sumAll);\n")),
        ),
        (
            "hook-in-factory",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace(
                    "  describe(name",
                    "  beforeEach(() => {});\n  describe(name"
                )
            )),
        ),
    ] {
        assert_eq!(class_of(label, &source)?, written_out, "{label}");
    }
    Ok(())
}

#[test]
fn a_factory_the_calls_cannot_rename_into_credits_nothing() -> Result<(), String> {
    let written_out = written_out_class()?;
    for (label, source) in [
        (
            "param-rebound-in-body",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace(
                    "      expect(impl",
                    "      const impl = (values: number[]) => values.length;\n      expect(impl"
                )
            )),
        ),
        (
            "param-used-as-a-value",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace("expect(impl([2, 3, 4]))", "expect(impl)")
            )),
        ),
        (
            "param-used-as-a-member",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace("expect(impl(", "expect(config.impl(")
            )),
        ),
        (
            "title-from-a-parameter",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace("'adds every value'", "`${name} adds every value`")
            )),
        ),
        (
            "argument-name-already-in-body",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace(
                    "expect(impl([2, 3, 4]))",
                    "expect(impl([2, 3, 4]) + sumAll([]))"
                )
            )),
        ),
        (
            "factory-body-declares",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace("  describe(name", "  const extra = 1;\n  describe(name")
            )),
        ),
        (
            "async-factory",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace("function", "async function")
            )),
        ),
        (
            "default-parameter",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace("name: string", "name: string = 'x'")
            )),
        ),
        (
            "rest-parameter",
            file(&format!(
                "{}{CALL}",
                FACTORY.replace("name: string", "...name: string[]")
            )),
        ),
        (
            "escaped-spelling-in-the-file",
            file(&format!("{FACTORY}{CALL}const key = '\\u0061';\n")),
        ),
    ] {
        assert_ne!(class_of(label, &source)?, written_out, "{label}");
    }
    Ok(())
}

#[test]
fn a_factory_mentioned_beyond_its_calls_credits_nothing() -> Result<(), String> {
    let written_out = written_out_class()?;
    for (label, extra) in [
        ("exported", "export { runSuite };\n"),
        ("reassigned", "runSuite = () => {};\n"),
        ("aliased", "const alias = runSuite;\n"),
        (
            "called-in-a-function",
            "function wrap() {\n  runSuite('x', sumAll);\n}\n",
        ),
        ("method-call", "runSuite.call(null, 'x', sumAll);\n"),
        ("second-declaration", "function runSuite() {}\n"),
    ] {
        let source = file(&format!("{FACTORY}{CALL}{extra}"));
        assert_ne!(class_of(label, &source)?, written_out, "{label}");
    }
    Ok(())
}

#[test]
fn a_call_with_a_computed_argument_credits_nothing() -> Result<(), String> {
    let written_out = written_out_class()?;
    for (label, call) in [
        (
            "arrow-argument",
            "runSuite('sum', (values) => sumAll(values));\n",
        ),
        ("bound-argument", "runSuite('sum', sumAll.bind(null));\n"),
        ("member-argument", "runSuite('sum', math.sumAll);\n"),
        ("template-title", "runSuite(`sum`, sumAll);\n"),
        ("missing-argument", "runSuite('sum');\n"),
        ("extra-argument", "runSuite('sum', sumAll, 1);\n"),
        ("spread-argument", "runSuite(...args);\n"),
        ("type-arguments", "runSuite<number>('sum', sumAll);\n"),
        ("awaited", "await runSuite('sum', sumAll);\n"),
        (
            "inside-describe",
            "describe('outer', () => {\n  runSuite('sum', sumAll);\n});\n",
        ),
    ] {
        let source = file(&format!("{FACTORY}{call}"));
        assert_ne!(class_of(label, &source)?, written_out, "{label}");
    }
    Ok(())
}

#[test]
fn a_substitution_that_breaks_the_parse_keeps_the_unexpanded_tests() -> Result<(), String> {
    // `name` inside the title string is rewritten to `'sum'`, leaving
    // `'f('sum')'`: the expanded source no longer parses, and the plain test
    // beside the factory must keep its credit.
    let written_out = written_out_class()?;
    let factory = "function runSuite(name: string, impl: (values: number[]) => number) {\n  describe(name, () => {\n    test('f(name)', () => {\n      expect(impl([2, 3, 4])).toBe(9);\n    });\n  });\n}\n";
    let plain = "test('direct', () => {\n  expect(sumAll([2, 3, 4])).toBe(9);\n});\n";
    let class = class_of("broken-parse", &file(&format!("{plain}{factory}{CALL}")))?;
    assert_eq!(class, written_out);
    Ok(())
}

#[test]
fn a_parameter_spelling_inside_a_string_credits_nothing() -> Result<(), String> {
    // The expected value `'prefix impl('` must not be rewritten to the
    // owner's name: the expansion would assert a different value.
    let factory = "function runSuite(name: string, impl: (values: number[]) => number) {\n  describe(name, () => {\n    test('adds', () => {\n      expect(String(impl([2, 3, 4]))).toBe('prefix impl(');\n    });\n  });\n}\n";
    let class = class_of("string-spelling", &file(&format!("{factory}{CALL}")))?;
    assert_eq!(class, ExposureClass::NoStaticPath);
    Ok(())
}
