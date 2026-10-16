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
            "one point zero",
            "  assert.equal(lineCount([\"a\"]), 1.0);",
            true,
        ),
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
            // The literal itself is the recorded expected value.
            assert!(
                test.assertions[0].expected_value_or_variant.is_some(),
                "{label}"
            );
            assert!(!test.assertions[0].has_dynamic_matcher_arg, "{label}");
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
    // chai's TDD `assert.equal` is loose too, so it is a candidate.
    let chai_assert = "import { assert } from \"chai\";\nimport { lineCount } from \"../src/linecount\";\n\nit(\"counts\", () => {\n  assert.equal(lineCount([\"a\"]), 1);\n});\n";
    let tests = extract_tests(Path::new("tests/linecount.test.ts"), chai_assert);
    let [test] = tests.as_slice() else {
        return Err(format!(
            "chai assert: expected one test, got {}",
            tests.len()
        ));
    };
    assert_eq!(test.assertions.len(), 1);
    assert!(test.assertions[0].loose_number_expected);
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
        ("typed-negative-zero", "  assert.equal(lineCount([]), -0);"),
        (
            "typed-parenthesized-as",
            "  assert.equal(lineCount([\"a\"]), (1 as number));",
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
            "  assert.equal(lineCount([\"a\"]) > 0, 1);",
        ),
        (
            "typed-member",
            LINE_COUNT,
            "  assert.equal(lineCount([\"a\"]).toFixed(0), 1);",
        ),
    ] {
        let finding = line_count_finding(label, owner, &line_count_test(body))?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
        {
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

/// A test-local function that shadows the owner and a method owner earn no
/// credit although each assertion is marked. The method's test relates, so
/// only the guard's owner-kind check refuses it.
#[test]
fn loose_number_facts_refuse_a_shadowed_owner_and_a_method_owner() -> Result<(), String> {
    let read = |label: &str, owner_file: &str, owner_source: &str, owner_name: &str, test: &str| {
        let root = ts_unique_tempdir(label)?;
        ts_write_file(&root.join(owner_file), owner_source)?;
        ts_write_file(&root.join("tests/linecount.test.ts"), test)?;
        let owner = extract_owners(Path::new(owner_file), owner_source)
            .into_iter()
            .find(|owner| owner.name == owner_name)
            .ok_or_else(|| format!("{label}: owner {owner_name} not extracted"))?;
        let tests = extract_tests(Path::new("tests/linecount.test.ts"), test);
        let candidates =
            related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
        let marked = tests
            .iter()
            .flat_map(|test| &test.assertions)
            .filter(|assertion| assertion.loose_number_expected)
            .count();
        let facts = ts_typed_loose_number_facts(&owner, &candidates, None, Some(&root));
        let credited = tests
            .iter()
            .flat_map(|test| test.assertions.iter().map(move |a| (test, a)))
            .filter(|(test, assertion)| facts.credits(&test.file, assertion))
            .count();
        let _ = std::fs::remove_dir_all(&root);
        Ok::<_, String>((candidates.len(), marked, credited))
    };
    let plain = read(
        "typed-facts-plain",
        "src/linecount.ts",
        LINE_COUNT,
        "lineCount",
        &line_count_test("  assert.equal(lineCount([\"a\"]), 1);"),
    )?;
    assert_eq!(plain, (1, 1, 1), "plain");
    let shadowed = read(
        "typed-facts-shadowed",
        "src/linecount.ts",
        LINE_COUNT,
        "lineCount",
        &line_count_test(
            "  assert.equal(lineCount([\"a\"]), 1);\n  {\n    const lineCount = (items: string[]) => items.length > 0;\n    assert.equal(lineCount([\"a\"]), 1);\n  }",
        ),
    )?;
    // Relation already drops a test that declares its own `lineCount`;
    // the guard's own shadow check is a second line behind it.
    assert_eq!(shadowed.0, 0, "shadowed: the test does not relate");
    assert_eq!(shadowed.1, 2, "shadowed: both assertions are marked");
    assert_eq!(shadowed.2, 0, "shadowed");
    // The same related test and marked assertion, read against a method
    // owner of the same name: only the owner-kind check differs.
    let root = ts_unique_tempdir("typed-facts-method")?;
    ts_write_file(&root.join("src/linecount.ts"), LINE_COUNT)?;
    let test = line_count_test("  assert.equal(lineCount([\"a\"]), 1);");
    ts_write_file(&root.join("tests/linecount.test.ts"), &test)?;
    let function = extract_owners(Path::new("src/linecount.ts"), LINE_COUNT)
        .into_iter()
        .find(|owner| owner.name == "lineCount")
        .ok_or("owner lineCount not extracted")?;
    let method_source = "export class Counter {\n  lineCount(items: string[]): number {\n    return items.length;\n  }\n}\n";
    let method = extract_owners(Path::new("src/linecount.ts"), method_source)
        .into_iter()
        .find(|owner| owner.name == "lineCount")
        .ok_or("method lineCount not extracted")?;
    let tests = extract_tests(Path::new("tests/linecount.test.ts"), &test);
    let candidates = related_test_candidates(
        &function,
        &tests,
        Some(&root),
        &ReExportIndex::empty(),
        None,
    );
    let credited = |owner: &TypeScriptOwner| {
        let facts = ts_typed_loose_number_facts(owner, &candidates, None, Some(&root));
        tests[0]
            .assertions
            .iter()
            .filter(|assertion| facts.credits(&tests[0].file, assertion))
            .count()
    };
    let (by_function, by_method) = (credited(&function), credited(&method));
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        matches!(
            method.owner_kind,
            OwnerKind::Method | OwnerKind::ClassMethod
        ),
        "{:?}",
        method.owner_kind
    );
    assert!(
        method
            .source_text
            .as_deref()
            .is_some_and(ts_owner_declares_number_return)
    );
    assert_eq!(by_function, 1);
    assert_eq!(by_method, 0);
    Ok(())
}

/// Credits for `test` against the `lineCount` function owner: related
/// candidates, marked assertions and credited assertions.
fn line_count_facts(label: &str, test: &str) -> Result<(usize, usize, usize), String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/linecount.ts"), LINE_COUNT)?;
    ts_write_file(
        &root.join("src/other.ts"),
        "export function lineCount(items: string[]): boolean {\n  return items.length > 0;\n}\n",
    )?;
    ts_write_file(&root.join("tests/linecount.test.ts"), test)?;
    let owner = extract_owners(Path::new("src/linecount.ts"), LINE_COUNT)
        .into_iter()
        .find(|owner| owner.name == "lineCount")
        .ok_or("owner lineCount not extracted")?;
    let tests = extract_tests(Path::new("tests/linecount.test.ts"), test);
    let candidates =
        related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
    let facts = ts_typed_loose_number_facts(&owner, &candidates, None, Some(&root));
    let marked = tests
        .iter()
        .flat_map(|test| &test.assertions)
        .filter(|assertion| assertion.loose_number_expected)
        .count();
    let credited = tests
        .iter()
        .flat_map(|test| test.assertions.iter().map(move |a| (test, a)))
        .filter(|(test, assertion)| facts.credits(&test.file, assertion))
        .count();
    let _ = std::fs::remove_dir_all(&root);
    Ok((candidates.len(), marked, credited))
}

/// The observed call must reach the owner through this file's own import
/// of it: a namespace member or a renamed import counts, a bare name the
/// file imports from another module does not.
#[test]
fn loose_number_facts_bind_the_observed_call_to_the_owner_import() -> Result<(), String> {
    let test = |imports: &str, body: &str| {
        format!(
            "import test from \"node:test\";\nimport assert from \"node:assert\";\n{imports}\n\ntest(\"counts\", () => {{\n{body}\n}});\n"
        )
    };
    let namespace = line_count_facts(
        "typed-bind-namespace",
        &test(
            "import * as counts from \"../src/linecount\";",
            "  assert.equal(counts.lineCount([\"a\"]), 1);",
        ),
    )?;
    assert_eq!(namespace, (1, 1, 1), "namespace member");
    let renamed = line_count_facts(
        "typed-bind-renamed",
        &test(
            "import { lineCount as count } from \"../src/linecount\";",
            "  assert.equal(count([\"a\"]), 1);",
        ),
    )?;
    assert_eq!(renamed, (1, 1, 1), "renamed import");
    let foreign = line_count_facts(
        "typed-bind-foreign-bare",
        &test(
            "import * as counts from \"../src/linecount\";\nimport { lineCount } from \"../src/other\";",
            "  counts.lineCount([\"a\"]);\n  assert.equal(lineCount([\"a\"]), 1);",
        ),
    )?;
    assert_eq!(
        foreign.0, 1,
        "foreign bare: the namespace call relates the test"
    );
    assert_eq!(foreign.1, 1, "foreign bare: the assertion is marked");
    assert_eq!(foreign.2, 0, "foreign bare");
    Ok(())
}

/// A body-local `counts` shadows the namespace import, so
/// `counts.lineCount(...)` is not an owner call. A real `lineCount(...)`
/// smoke call keeps the test related; the loose-number pin must not credit.
#[test]
fn loose_number_facts_refuse_a_shadowed_namespace_receiver() -> Result<(), String> {
    let test = |imports: &str, body: &str| {
        format!(
            "import test from \"node:test\";\nimport assert from \"node:assert\";\n{imports}\n\ntest(\"counts\", () => {{\n{body}\n}});\n"
        )
    };
    let source = test(
        "import * as counts from \"../src/linecount\";\nimport { lineCount } from \"../src/linecount\";",
        "  const counts = fakeCounts();\n  lineCount([\"a\"]);\n  assert.equal(counts.lineCount([\"a\"]), 1);",
    );
    let tests = extract_tests(Path::new("tests/linecount.test.ts"), &source);
    let [parsed] = tests.as_slice() else {
        return Err(format!("expected one test, got {}", tests.len()));
    };
    assert_eq!(parsed.assertions.len(), 1);
    assert!(parsed.assertions[0].loose_number_expected);
    assert_eq!(
        parsed.assertions[0].observed_expression.as_deref(),
        Some("counts.lineCount([\"a\"])")
    );
    if !local_identifier_declared_in_test_body(&parsed.body_text, "counts") {
        return Err("body-local `counts` must read as a shadow".to_string());
    }
    let facts = line_count_facts("typed-bind-shadowed-namespace", &source)?;
    assert_eq!(facts.0, 1, "shadowed namespace: the smoke call relates");
    assert_eq!(facts.1, 1, "shadowed namespace: the assertion is marked");
    assert_eq!(facts.2, 0, "shadowed namespace: the pin must not credit");
    let owner = extract_owners(Path::new("src/linecount.ts"), LINE_COUNT)
        .into_iter()
        .find(|owner| owner.name == "lineCount")
        .ok_or("owner lineCount not extracted")?;
    if !related_test_candidates(&owner, &tests, None, &ReExportIndex::empty(), None)
        .iter()
        .any(|candidate| candidate.relation.uses_oracle())
    {
        return Err(format!(
            "real owner smoke call must keep a trusted relation: {:?}",
            related_test_candidates(&owner, &tests, None, &ReExportIndex::empty(), None)
                .iter()
                .map(|candidate| candidate.relation)
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}
