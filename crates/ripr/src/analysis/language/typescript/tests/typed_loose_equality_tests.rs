//! RIPR-SPEC-0247 rule 2, typed amendment: loose `==` equality against a
//! number literal (`0` and `1` included) reads exact on the whole result of
//! an owner declared to return `number`.

use super::*;

const LINE_COUNT: &str =
    "export function lineCount(items: string[]): number {\n  return items.length;\n}\n";
const RETURN_OLD: &str = "  return items.length;";
const RETURN_NEW: &str = "  return items.length * 2;";

#[test]
fn number_return_annotation_reads_only_an_exact_number() {
    for (label, owner) in [
        ("function", LINE_COUNT),
        (
            "parameter type with parentheses",
            "function f(cb: (x: number) => void, n = (1 + 2)): number {\n  return n;\n}",
        ),
        ("arrow", "(items: string[]): number => items.length"),
        (
            "arrow with a block",
            "(items: string[]):number=>{\n  return items.length;\n}",
        ),
        ("method", "count(): number {\n  return 1;\n}"),
        (
            "a parenthesis in a comment or string",
            "function f(/* ) */ s = \")\"): number {\n  return 1;\n}",
        ),
    ] {
        assert!(ts_owner_declares_number_return(owner), "{label}");
    }
    for (label, owner) in [
        ("no annotation", "function f(x: number) {\n  return x;\n}"),
        (
            "string",
            "function f(x: number): string {\n  return \"\";\n}",
        ),
        (
            "union",
            "function f(x: number): number | string {\n  return x;\n}",
        ),
        ("array", "function f(): number[] {\n  return [];\n}"),
        (
            "promise",
            "async function f(): Promise<number> {\n  return 1;\n}",
        ),
        ("async", "async function f(): number {\n  return 1;\n}"),
        ("async arrow", "async (): number => 1"),
        ("generator", "function* f(): number {\n  return 1;\n}"),
        ("generic", "function f<T>(x: T): number {\n  return 1;\n}"),
        (
            "number only in a parameter",
            "function f(x: number) {\n  return x;\n}",
        ),
        ("bare arrow", "x => x"),
        (
            "predicate",
            "function f(x: unknown): x is number {\n  return true;\n}",
        ),
    ] {
        assert!(!ts_owner_declares_number_return(owner), "{label}");
    }
}

fn line_count_test(body: &str) -> String {
    format!(
        "import test from \"node:test\";\nimport assert from \"node:assert\";\nimport {{ lineCount }} from \"../src/linecount\";\n\ntest(\"line count counts one item\", () => {{\n{body}\n}});\n"
    )
}

fn line_count_finding(label: &str, owner: &str, test: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(
        &root.join("src/linecount.ts"),
        &owner.replace(RETURN_OLD, RETURN_NEW),
    )?;
    ts_write_file(&root.join("tests/linecount.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/linecount.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: RETURN_NEW.to_string(),
            new_side_line: 2,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: RETURN_OLD.to_string(),
            new_side_line: 2,
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
        .find(|finding| finding.probe.location.line == 2)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

#[test]
fn loose_number_literal_marks_only_weak_loose_equality() -> Result<(), String> {
    for (label, body, marked) in [
        ("equal 1", "  assert.equal(lineCount([\"a\"]), 1);", true),
        ("equal 0", "  assert.equal(lineCount([]), 0);", true),
        ("deepEqual 0", "  assert.deepEqual(lineCount([]), 0);", true),
        ("negative", "  assert.equal(lineCount([]), -0);", true),
        (
            "parenthesized as",
            "  assert.equal(lineCount([\"a\"]), (1 as number));",
            true,
        ),
        // Rule 2 already reads these strong.
        (
            "equal 2",
            "  assert.equal(lineCount([\"a\", \"b\"]), 2);",
            false,
        ),
        (
            "strictEqual",
            "  assert.strictEqual(lineCount([\"a\"]), 1);",
            false,
        ),
        (
            "boolean",
            "  assert.equal(lineCount([\"a\"]), true);",
            false,
        ),
        (
            "numeric string",
            "  assert.equal(lineCount([\"a\"]), \"1\");",
            false,
        ),
        (
            "notEqual",
            "  assert.notEqual(lineCount([\"a\"]), 1);",
            false,
        ),
        (
            "variable",
            "  const one = 1;\n  assert.equal(lineCount([\"a\"]), one);",
            false,
        ),
        (
            "Infinity",
            "  assert.equal(lineCount([\"a\"]), Infinity);",
            false,
        ),
    ] {
        let tests = extract_tests(Path::new("tests/linecount.test.ts"), &line_count_test(body));
        let [test] = tests.as_slice() else {
            return Err(format!("{label}: expected one test, got {}", tests.len()));
        };
        assert_eq!(test.assertions.len(), 1, "{label}");
        assert_eq!(test.assertions[0].loose_number_expected, marked, "{label}");
        if marked {
            assert_eq!(
                test.assertions[0].oracle_kind,
                OracleKind::RelationalCheck,
                "{label}"
            );
        }
    }
    // chai's negated `expect(...).not.to.equal(1)` reads relational / weak
    // under the same matcher name and must not be marked.
    let chai = "import { expect } from \"chai\";\nimport { lineCount } from \"../src/linecount\";\n\nit(\"counts\", () => {\n  expect(lineCount([\"a\"])).not.to.equal(1);\n});\n";
    let tests = extract_tests(Path::new("tests/linecount.test.ts"), chai);
    let [test] = tests.as_slice() else {
        return Err(format!("chai: expected one test, got {}", tests.len()));
    };
    assert_eq!(test.assertions.len(), 1);
    assert_eq!(test.assertions[0].matcher, "equal");
    assert_eq!(test.assertions[0].oracle_kind, OracleKind::RelationalCheck);
    assert!(!test.assertions[0].loose_number_expected);
    Ok(())
}

#[test]
fn loose_number_pin_exposes_a_number_owner() -> Result<(), String> {
    for (label, body) in [
        ("typed-equal-one", "  assert.equal(lineCount([\"a\"]), 1);"),
        ("typed-equal-zero", "  assert.equal(lineCount([]), 0);"),
        (
            "typed-deep-equal-one",
            "  assert.deepEqual(lineCount([\"a\"]), 1);",
        ),
        (
            "typed-local",
            "  const n = lineCount([\"a\"]);\n  assert.equal(n, 1);",
        ),
    ] {
        let finding = line_count_finding(label, LINE_COUNT, &line_count_test(body))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
        let row = finding
            .related_tests
            .first()
            .ok_or_else(|| format!("{label}: expected a related test row"))?;
        assert_eq!(row.oracle_kind, OracleKind::ExactValue, "{label}");
        assert_eq!(row.oracle_strength, OracleStrength::Strong, "{label}");
    }
    Ok(())
}

#[test]
fn loose_number_pin_stays_weak_without_a_number_owner_or_whole_result() -> Result<(), String> {
    let untyped = LINE_COUNT.replace("): number {", ") {");
    let union = LINE_COUNT.replace("): number {", "): number | boolean {");
    for (label, owner, body) in [
        (
            "typed-untyped-owner",
            untyped.as_str(),
            "  assert.equal(lineCount([\"a\"]), 1);",
        ),
        (
            "typed-union-owner",
            union.as_str(),
            "  assert.equal(lineCount([\"a\"]), 1);",
        ),
        (
            "typed-wrapped",
            LINE_COUNT,
            "  assert.equal(Math.min(lineCount([\"a\"]), 5), 1);",
        ),
        (
            "typed-compared",
            LINE_COUNT,
            "  assert.equal(lineCount([\"a\"]) > 0, true);",
        ),
        (
            "typed-shadowed",
            LINE_COUNT,
            "  const lineCount = (items: string[]) => items.length > 0;\n  assert.equal(lineCount([\"a\"]), 1);",
        ),
    ] {
        let finding = line_count_finding(label, owner, &line_count_test(body))?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
        if label != "typed-shadowed" {
            let row = finding
                .related_tests
                .first()
                .ok_or_else(|| format!("{label}: expected a related test row"))?;
            assert_ne!(
                (row.oracle_kind.clone(), row.oracle_strength.clone()),
                (OracleKind::ExactValue, OracleStrength::Strong),
                "{label}"
            );
        }
    }
    Ok(())
}
