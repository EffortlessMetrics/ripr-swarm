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
