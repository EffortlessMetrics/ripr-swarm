//! RIPR-SPEC-0320: a truthiness assertion on the whole result of an owner
//! declared to return `boolean` pins that result.

use super::*;

const IS_ADULT: &str = "export function isAdult(age: number): boolean {\n  return age >= 18;\n}\n";

#[test]
fn boolean_return_annotation_reads_only_an_exact_boolean() {
    for (label, owner) in [
        ("function", IS_ADULT),
        ("arrow", "(age: number): boolean => age >= 18"),
        (
            "arrow with a block",
            "(age: number):boolean=>{\n  return age >= 18;\n}",
        ),
    ] {
        assert!(ts_owner_declares_return_type(owner, "boolean"), "{label}");
    }
    for (label, owner) in [
        (
            "no annotation",
            "function f(age: number) {\n  return age >= 18;\n}",
        ),
        (
            "number",
            "function f(age: number): number {\n  return age;\n}",
        ),
        (
            "nullable",
            "function f(age: number): boolean | undefined {\n  return undefined;\n}",
        ),
        (
            "promise",
            "async function f(): Promise<boolean> {\n  return true;\n}",
        ),
        (
            "type predicate",
            "function f(x: unknown): x is string {\n  return true;\n}",
        ),
    ] {
        assert!(!ts_owner_declares_return_type(owner, "boolean"), "{label}");
    }
}

/// Related candidates and credited assertions for `test` against the
/// `isAdult` function owner declared by `owner_source`.
fn is_adult_facts(label: &str, owner_source: &str, test: &str) -> Result<(usize, usize), String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/adult.ts"), owner_source)?;
    ts_write_file(&root.join("tests/adult.test.ts"), test)?;
    let owner = extract_owners(Path::new("src/adult.ts"), owner_source)
        .into_iter()
        .find(|owner| owner.name == "isAdult")
        .ok_or("owner isAdult not extracted")?;
    let tests = extract_tests(Path::new("tests/adult.test.ts"), test);
    let candidates =
        related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
    let facts = ts_typed_boolean_assert_facts(&owner, &candidates, None, Some(&root));
    let credited = tests
        .iter()
        .flat_map(|test| test.assertions.iter().map(move |a| (test, a)))
        .filter(|(test, assertion)| facts.credits(&test.file, assertion))
        .count();
    let _ = std::fs::remove_dir_all(&root);
    Ok((candidates.len(), credited))
}

fn adult_test(body: &str) -> String {
    format!(
        "import test from \"node:test\";\nimport assert from \"node:assert\";\nimport {{ isAdult }} from \"../src/adult\";\n\ntest(\"adult\", () => {{\n{body}\n}});\n"
    )
}

#[test]
fn truthiness_on_a_boolean_owner_with_literal_arguments_is_credited() -> Result<(), String> {
    for (label, body, credited) in [
        ("ok true side", "  assert.ok(isAdult(18));", 1),
        ("ok false side", "  assert.ok(!isAdult(17));", 1),
        (
            "both sides",
            "  assert.ok(isAdult(18));\n  assert.ok(!isAdult(17));",
            2,
        ),
        ("with a message", "  assert.ok(isAdult(18), \"adult\");", 1),
        // A variable can hold every call on one side of the boundary.
        (
            "loop variable",
            "  for (let age = 20; age < 30; age++) {\n    assert.ok(isAdult(age));\n  }",
            0,
        ),
        ("expression argument", "  assert.ok(isAdult(17 + 1));", 0),
        // Rust's float parser reads these, TypeScript reads identifiers.
        (
            "identifier spelled like a float",
            "  const inf = 20;\n  assert.ok(isAdult(inf));",
            0,
        ),
        ("NaN", "  assert.ok(!isAdult(NaN));", 0),
        ("negative number", "  assert.ok(!isAdult(-1));", 1),
        ("exponent", "  assert.ok(isAdult(1e2));", 1),
        ("string", "  assert.ok(isAdult(\"18\"));", 1),
        // `!!` is two negations, not the owner call.
        ("double negation", "  assert.ok(!!isAdult(18));", 0),
        // The result is wrapped, so the assertion no longer pins it.
        ("wrapped in a call", "  assert.ok(String(isAdult(18)));", 0),
    ] {
        let found = is_adult_facts(
            &format!("typed-bool-{}", label.replace(' ', "-")),
            IS_ADULT,
            &adult_test(body),
        )?;
        assert_eq!(found, (1, credited), "{label}");
    }
    Ok(())
}

#[test]
fn truthiness_stays_smoke_unless_the_owner_declares_boolean() -> Result<(), String> {
    for (label, owner) in [
        (
            "number",
            "export function isAdult(age: number): number {\n  return age >= 18 ? 1 : 0;\n}\n",
        ),
        (
            "unannotated",
            "export function isAdult(age: number) {\n  return age >= 18;\n}\n",
        ),
        (
            "nullable",
            "export function isAdult(age: number): boolean | undefined {\n  return age >= 18;\n}\n",
        ),
        (
            "any",
            "export function isAdult(age: number): any {\n  return age >= 18 && \"yes\";\n}\n",
        ),
    ] {
        let found = is_adult_facts(
            &format!("typed-bool-owner-{label}"),
            owner,
            &adult_test("  assert.ok(isAdult(18));"),
        )?;
        assert_eq!(found, (1, 0), "{label}");
    }
    Ok(())
}

/// A comma inside a string literal is not an argument separator.
#[test]
fn a_string_argument_with_a_comma_stays_one_literal() -> Result<(), String> {
    let owner = "export function isAdult(role: string, extra: number): boolean {\n  return role === \"admin,root\" && extra > 0;\n}\n";
    for (label, body, credited) in [
        ("one string", "  assert.ok(isAdult(\"admin,root\", 1));", 1),
        (
            "nested call keeps its commas",
            "  assert.ok(isAdult(String(\"admin\", \"root\"), 1));",
            0,
        ),
    ] {
        let found = is_adult_facts(
            &format!("typed-bool-comma-{}", label.replace(' ', "-")),
            owner,
            &adult_test(body),
        )?;
        assert_eq!(found, (1, credited), "{label}");
    }
    Ok(())
}

/// The finding for the boundary flip `age >= 18` to `age > 18` in `isAdult`
/// against `test`, read end to end through the adapter.
fn adult_boundary_finding(label: &str, test: &str) -> Result<Finding, String> {
    const OLD: &str = "  return age >= 18;";
    const NEW: &str = "  return age > 18;";
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/adult.ts"), &IS_ADULT.replace(OLD, NEW))?;
    ts_write_file(&root.join("tests/adult.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/adult.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: NEW.to_string(),
            new_side_line: 2,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: OLD.to_string(),
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

/// Jest, chai and `node:assert` truthiness checks on both sides of the
/// boundary read exposed; the same checks on one side only stay weak.
#[test]
fn truthiness_on_both_sides_of_the_boundary_reads_exposed() -> Result<(), String> {
    let jest = "import { isAdult } from \"../src/adult\";\n\ntest(\"adult\", () => {\n  expect(isAdult(18)).toBeTruthy();\n  expect(isAdult(17)).toBeFalsy();\n});\n";
    let chai = "import { assert } from \"chai\";\nimport { isAdult } from \"../src/adult\";\n\ndescribe(\"adult\", () => {\n  it(\"pins both sides\", () => {\n    assert.isTrue(isAdult(18));\n    assert.isFalse(isAdult(17));\n  });\n});\n";
    let one_side = "import { isAdult } from \"../src/adult\";\n\ntest(\"adult\", () => {\n  expect(isAdult(30)).toBeTruthy();\n});\n";
    for (label, test, class) in [
        ("jest", jest, ExposureClass::Exposed),
        ("chai", chai, ExposureClass::Exposed),
        ("one side", one_side, ExposureClass::WeaklyExposed),
    ] {
        let finding = adult_boundary_finding(&format!("typed-bool-finding-{label}"), test)?;
        assert_eq!(finding.class, class, "{label}");
    }
    Ok(())
}
