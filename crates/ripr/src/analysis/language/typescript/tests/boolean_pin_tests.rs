//! RIPR-SPEC-0247 rule 5: a truthiness assertion on a literal-argument call
//! to an owner declared to return `boolean` is an exact pin.

use super::*;

const ADULT: &str = "export function isAdult(age: number): boolean {\n  return age >= 18;\n}\n";

fn node_test(body: &str) -> String {
    format!(
        "import {{ test }} from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ isAdult }} from \"../src/adult\";\n\ntest(\"adult\", () => {{\n{body}\n}});\n"
    )
}

/// Related candidates and credited assertions for `isAdult` as declared by
/// `owner_source`, read against `test_source`.
fn boolean_pin_credits(
    label: &str,
    owner_source: &str,
    test_source: &str,
) -> Result<(usize, usize), String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/adult.ts"), owner_source)?;
    ts_write_file(&root.join("tests/adult.test.ts"), test_source)?;
    let owner = extract_owners(Path::new("src/adult.ts"), owner_source)
        .into_iter()
        .find(|owner| owner.name == "isAdult")
        .ok_or_else(|| format!("{label}: owner isAdult not extracted"))?;
    let tests = extract_tests(Path::new("tests/adult.test.ts"), test_source);
    let candidates =
        related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
    let facts = ts_boolean_pin_facts(
        &ProbeFamily::Predicate,
        &owner,
        &candidates,
        None,
        Some(&root),
    );
    let credited = tests
        .iter()
        .flat_map(|test| test.assertions.iter().map(move |a| (test, a)))
        .filter(|(test, assertion)| facts.credits(&test.file, assertion))
        .count();
    let _ = std::fs::remove_dir_all(&root);
    Ok((candidates.len(), credited))
}

#[test]
fn boolean_return_annotation_reads_only_an_exact_boolean() {
    for owner in [
        ADULT,
        "(age: number): boolean => age >= 18",
        "function f(cb: (x: number) => void): boolean {\n  return true;\n}",
    ] {
        assert!(ts_owner_declares_boolean_return(owner), "{owner}");
    }
    for owner in [
        "function f(age: number) {\n  return age >= 18;\n}",
        "function f(age: number): boolean | undefined {\n  return undefined;\n}",
        "function f(age: number): Promise<boolean> {\n  return Promise.resolve(true);\n}",
        "async function f(age: number): boolean {\n  return true;\n}",
        "function f(age: number): number {\n  return 1;\n}",
        "function f<T>(age: T): boolean {\n  return true;\n}",
    ] {
        assert!(!ts_owner_declares_boolean_return(owner), "{owner}");
    }
}

#[test]
fn truthiness_assertions_on_literal_calls_are_pins() -> Result<(), String> {
    let test = node_test("  assert.ok(isAdult(18));\n  assert.ok(!isAdult(17));");
    assert_eq!(boolean_pin_credits("bool-pin-node", ADULT, &test)?, (1, 2));
    let jest = "import { isAdult } from \"../src/adult\";\n\nit(\"adult\", () => {\n  expect(isAdult(18)).toBeTruthy();\n  expect(isAdult(17)).toBeFalsy();\n});\n";
    assert_eq!(boolean_pin_credits("bool-pin-jest", ADULT, jest)?, (1, 2));
    Ok(())
}

#[test]
fn a_non_boolean_owner_or_a_wrapped_call_is_not_a_pin() -> Result<(), String> {
    let test = node_test("  assert.ok(isAdult(18));\n  assert.ok(!isAdult(17));");
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
            "union",
            "export function isAdult(age: number): boolean | null {\n  return age >= 18;\n}\n",
        ),
    ] {
        assert_eq!(boolean_pin_credits(label, owner, &test)?, (1, 0), "{label}");
    }
    for (label, body) in [
        (
            "dynamic argument",
            "  const age = 20;\n  assert.ok(isAdult(age));",
        ),
        ("conjunction", "  assert.ok(isAdult(18) && true);"),
        ("member", "  assert.ok(isAdult(18).valueOf());"),
        ("wrapper call", "  assert.ok(Boolean(isAdult(18)));"),
        (
            "shadowed",
            "  const isAdult = (age: number) => age;\n  assert.ok(isAdult(18));",
        ),
    ] {
        let test = node_test(body);
        assert_eq!(boolean_pin_credits(label, ADULT, &test)?.1, 0, "{label}");
    }
    Ok(())
}

#[test]
fn a_same_named_function_from_another_module_is_not_a_pin() -> Result<(), String> {
    let test = node_test("  assert.ok(isAdult(18));").replace("../src/adult", "../src/other-adult");
    assert_eq!(boolean_pin_credits("bool-pin-foreign", ADULT, &test)?.1, 0);
    Ok(())
}
