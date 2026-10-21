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

/// What the whole-file pipeline sees of `source` at `tests/sum.test.ts`: the
/// extracted tests, the partial-extraction gap, and the committed-snapshot
/// promotion (`mod.rs` runs them in this order on one extraction).
struct Pipeline {
    tests: Vec<TypeScriptTest>,
    gap: Option<TypeScriptTestExtractionGap>,
}

fn pipeline(label: &str, source: &str, snapshots: &[(&str, &str)]) -> Result<Pipeline, String> {
    let root = ts_unique_tempdir(label)?;
    let relative = Path::new("tests/sum.test.ts");
    let written = ts_write_file(&root.join(".git/HEAD"), "x\n").and_then(|()| {
        let header = "// Jest Snapshot v1, https://jestjs.io/docs/snapshot-testing\n\n";
        let body: String = snapshots
            .iter()
            .map(|(key, value)| format!("exports[`{key}`] = `{value}`;\n\n"))
            .collect();
        ts_write_file(
            &root.join("tests/__snapshots__/sum.test.ts.snap"),
            &format!("{header}{body}"),
        )
    });
    let (mut tests, _, expansion) = extract_tests_with_expansion(relative, source);
    if written.is_ok() {
        let counted = expansion.as_ref().map_or_else(
            || source.to_string(),
            SuiteFactoryExpansion::snapshot_source,
        );
        promote_committed_snapshots(&root, relative, &counted, &mut tests);
    }
    let gap = detect_partial_test_extraction_expanded(relative, source, &tests, expansion.as_ref());
    let _ = std::fs::remove_dir_all(&root);
    written?;
    Ok(Pipeline { tests, gap })
}

fn snapshot_strengths(pipeline: &Pipeline) -> Vec<OracleStrength> {
    pipeline
        .tests
        .iter()
        .flat_map(|test| &test.assertions)
        .filter(|assertion| assertion.matcher == "toMatchSnapshot")
        .map(|assertion| assertion.oracle_strength.clone())
        .collect()
}

const SNAPSHOT_FACTORY: &str = "import { sumAll } from '../src/sum';\n\nfunction runSuite(name: string, impl: (values: number[]) => number) {\n  describe(name, () => {\n    test('adds', () => {\n      expect(impl([2, 3, 4])).toMatchSnapshot();\n    });\n  });\n}\n";

#[test]
fn each_call_of_a_suite_factory_earns_its_own_committed_snapshot() -> Result<(), String> {
    let snapshots = [("a adds 1", "9"), ("b adds 1", "9")];
    let two = format!("{SNAPSHOT_FACTORY}runSuite('a', sumAll);\nrunSuite('b', sumAll);\n");
    let got = pipeline("snap-two-calls", &two, &snapshots)?;
    assert_eq!(
        snapshot_strengths(&got),
        vec![OracleStrength::Strong, OracleStrength::Strong]
    );
    // Control: one call was already credited before call provenance.
    let one = format!("{SNAPSHOT_FACTORY}runSuite('a', sumAll);\n");
    let got = pipeline("snap-one-call", &one, &snapshots)?;
    assert_eq!(snapshot_strengths(&got), vec![OracleStrength::Strong]);
    Ok(())
}

#[test]
fn factory_calls_that_share_a_snapshot_key_stay_medium() -> Result<(), String> {
    // Both calls record under `a adds`, so the one entry belongs to neither.
    let two = format!("{SNAPSHOT_FACTORY}runSuite('a', sumAll);\nrunSuite('a', sumAll);\n");
    let got = pipeline("snap-same-key", &two, &[("a adds 1", "9")])?;
    assert_eq!(
        snapshot_strengths(&got),
        vec![OracleStrength::Medium, OracleStrength::Medium]
    );
    // A snapshot call outside every test body still refuses credit.
    let outside = format!(
        "{SNAPSHOT_FACTORY}runSuite('a', sumAll);\nrunSuite('b', sumAll);\nexpect(sumAll([1])).toMatchSnapshot();\n"
    );
    let got = pipeline(
        "snap-outside-body",
        &outside,
        &[("a adds 1", "9"), ("b adds 1", "9")],
    )?;
    assert!(
        !snapshot_strengths(&got).contains(&OracleStrength::Strong),
        "{:?}",
        snapshot_strengths(&got)
    );
    Ok(())
}

#[test]
fn an_unexpanded_factory_still_counts_its_snapshot_call() -> Result<(), String> {
    // The call is refused (a computed argument), so the factory body's
    // snapshot call stays in the file but in no test: the counts disagree.
    let source = format!(
        "{SNAPSHOT_FACTORY}runSuite('a', sumAll);\nrunSuite('b', (v) => sumAll(v));\nrunSuite('c', sumAll);\n"
    );
    let got = pipeline(
        "snap-unexpanded",
        &source,
        &[("a adds 1", "9"), ("c adds 1", "9")],
    )?;
    assert!(
        !snapshot_strengths(&got).contains(&OracleStrength::Strong),
        "{:?}",
        snapshot_strengths(&got)
    );
    Ok(())
}

#[test]
fn an_expanded_factory_is_not_disclosed_as_partial_extraction() -> Result<(), String> {
    let source = file(&format!("{FACTORY}{CALL}"));
    let got = pipeline("partial-silent", &source, &[])?;
    assert_eq!(got.tests.len(), 1);
    assert!(got.gap.is_none(), "{:?}", got.gap);
    // Control: without the expansion the factory's own registration, which
    // reads `impl(...)`, is the unextracted shape the detector exists to find.
    assert!(
        detect_partial_test_extraction(Path::new("tests/sum.test.ts"), &source, &got.tests)
            .is_some()
    );
    Ok(())
}

#[test]
fn a_registration_the_expansion_drops_is_still_disclosed_at_its_call() -> Result<(), String> {
    // The tagged-template `.each` is never extracted, expanded or not, and the
    // appended copy sits on a line the file does not have: the disclosure
    // reports the call's line.
    let factory = "function runSuite(name: string, impl: (values: number[]) => number) {\n  describe(name, () => {\n    test('adds', () => {\n      expect(impl([2, 3, 4])).toBe(9);\n    });\n    test.each`\n      a\n      ${1}\n    `('row', () => {\n      expect(impl([1])).toBe(1);\n    });\n  });\n}\n";
    let source = file(&format!("{factory}{CALL}"));
    let got = pipeline("partial-dropped", &source, &[])?;
    let gap = got
        .gap
        .ok_or_else(|| "a dropped registration must stay disclosed".to_string())?;
    assert_eq!(gap.shape, "tagged-template .each");
    let call_line = source
        .lines()
        .position(|line| line == CALL.trim_end())
        .map(|index| index + 1)
        .ok_or_else(|| "call line".to_string())?;
    assert_eq!(gap.sample_line, call_line);
    Ok(())
}

#[test]
fn an_expanded_test_points_its_evidence_at_the_call() -> Result<(), String> {
    let source = file(&format!(
        "{FACTORY}runSuite('one', sumAll);\nrunSuite('two', sumAll);\n"
    ));
    let got = pipeline("evidence-line", &source, &[])?;
    let call_lines: Vec<usize> = source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.starts_with("runSuite("))
        .map(|(index, _)| index + 1)
        .collect();
    assert_eq!(call_lines.len(), 2);
    let pointed: Vec<usize> = got
        .tests
        .iter()
        .flat_map(|test| {
            test.assertions
                .iter()
                .map(|assertion| test.evidence_line(assertion))
        })
        .collect();
    assert_eq!(pointed, call_lines);
    // Control: a test written out keeps the assertion's own line.
    let plain = file("test('direct', () => {\n  expect(sumAll([2, 3, 4])).toBe(9);\n});\n");
    let got = pipeline("evidence-plain", &plain, &[])?;
    let assertion = got
        .tests
        .first()
        .and_then(|test| test.assertions.first().map(|assertion| (test, assertion)))
        .ok_or_else(|| "plain assertion".to_string())?;
    assert_eq!(assertion.0.evidence_line(assertion.1), assertion.1.line);
    assert_eq!(assertion.1.line, 5);
    Ok(())
}

#[test]
fn a_finding_points_its_oracle_evidence_at_the_factory_call() -> Result<(), String> {
    let source = file(&format!("{FACTORY}{CALL}"));
    let call_line = source
        .lines()
        .position(|line| line == CALL.trim_end())
        .map(|index| index + 1)
        .ok_or_else(|| "call line".to_string())?;
    let root = ts_unique_tempdir("evidence-ref")?;
    ts_write_file(&root.join("src/sum.ts"), OWNER)?;
    ts_write_file(&root.join("tests/sum.test.ts"), &source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/sum.ts", &[CHANGED_LINE])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let expected = format!("typescript_oracle_evidence_ref: tests/sum.test.ts:{call_line}");
    assert!(
        result
            .findings
            .iter()
            .any(|finding| finding.evidence.iter().any(|line| line == &expected)),
        "{:?}",
        result
            .findings
            .iter()
            .map(|finding| &finding.evidence)
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn a_call_the_expansion_refuses_keeps_the_factory_disclosed() -> Result<(), String> {
    // One call expands and one does not (a computed argument): the factory is
    // not fully expanded, so its own registration stays the disclosed shape.
    let factory = "function runSuite(name: string, impl: (values: number[]) => number) {\n  describe(name, () => {\n    it(`adds ${1}`, () => {\n      expect(impl([2, 3, 4])).toBe(9);\n    });\n  });\n}\n";
    let source = file(&format!(
        "{factory}runSuite('sum', sumAll);\nrunSuite('other', (v) => sumAll(v));\n"
    ));
    let got = pipeline("partial-refused-call", &source, &[])?;
    assert!(got.gap.is_some(), "a refused call must keep the disclosure");
    Ok(())
}

#[test]
fn a_test_the_expansion_leaves_unchanged_is_not_disclosed_as_dropped() -> Result<(), String> {
    // Only the suite title is a parameter, so the appended copy of the test is
    // textually the factory's own registration. The span recovery must match
    // the copy, not the covered declaration it equals.
    let factory = "function runSuite(name: string) {\n  describe(name, () => {\n    test('adds', () => {\n      expect(sumAll([2, 3, 4])).toBe(9);\n    });\n  });\n}\n";
    let source = file(&format!("{factory}runSuite('sum');\nrunSuite('again');\n"));
    let got = pipeline("partial-identical-body", &source, &[])?;
    assert_eq!(got.tests.len(), 2);
    assert!(got.gap.is_none(), "{:?}", got.gap);
    Ok(())
}

#[test]
fn a_conditional_credit_disclosure_names_a_line_of_the_file() -> Result<(), String> {
    let factory = "function runSuite(name: string, impl: (values: number[]) => number) {\n  describe(name, () => {\n    test('adds', () => {\n      if (ok) {\n        expect(impl([2, 3, 4])).toBe(9);\n      }\n    });\n  });\n}\n";
    let source = file(&format!("{factory}{CALL}"));
    let (tests, gap, _) = extract_tests_with_expansion(Path::new("tests/sum.test.ts"), &source);
    assert_eq!(tests.len(), 1);
    let gap = gap.ok_or_else(|| "the conditional credit must be disclosed".to_string())?;
    let call_line = source
        .lines()
        .position(|line| line == CALL.trim_end())
        .map(|index| index + 1)
        .ok_or_else(|| "call line".to_string())?;
    // The walk meets the factory's own `if (ok)` first, a real line of the
    // file; either way the line must exist.
    assert!(
        gap.sample_line >= 1 && gap.sample_line <= source.lines().count(),
        "{gap:?}"
    );
    assert!(
        gap.sample_line == 7 || gap.sample_line == call_line,
        "{gap:?}"
    );
    Ok(())
}
