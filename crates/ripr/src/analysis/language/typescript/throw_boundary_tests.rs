//! A throw test at the exact boundary input of a changed throw guard
//! (#13601, RIPR-SPEC-0097).
//!
//! `if (n <= 0) throw ...` changed to `n < 0` stops throwing for `n == 0`
//! only, so `expect(() => f(0)).toThrow()` is the check that notices. Each
//! case writes a real owner module and test file under a temporary root and
//! classifies the changed line through the production `classify_change`
//! path.

use super::tests::ts_unique_tempdir;
use super::*;
use crate::domain::ExposureClass;

const OWNER_FILE: &str = "src/lim.ts";
const TEST_FILE: &str = "tests/lim.test.ts";
const IMPORT: &str = "import { mustPositive } from '../src/lim';\n";
const GUARD: &str = "  if (n < 0) throw new RangeError(\"bad\");";

fn owner_source(guard: &str) -> String {
    format!("export function mustPositive(n: number): number {{\n{guard}\n  return n;\n}}\n")
}

/// The finding for the changed line 2 of `owner_source`, with the tests
/// written in `test_body` after the import.
fn classify(owner_source: &str, test_body: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir("throw-boundary")?;
    let test_source = format!("{IMPORT}{test_body}");
    let result = classify_in(&root, owner_source, &test_source);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn classify_in(root: &Path, owner_source: &str, test_source: &str) -> Result<Finding, String> {
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("mkdir src: {err}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|err| format!("mkdir tests: {err}"))?;
    std::fs::write(root.join(OWNER_FILE), owner_source)
        .map_err(|err| format!("write owner: {err}"))?;
    std::fs::write(root.join(TEST_FILE), test_source)
        .map_err(|err| format!("write test: {err}"))?;
    let line_text = owner_source
        .lines()
        .nth(1)
        .ok_or("owner has no second line")?;
    let owners = extract_owners(Path::new(OWNER_FILE), owner_source);
    let tests = extract_tests(Path::new(TEST_FILE), test_source);
    if tests.is_empty() {
        return Err(format!("fixture parsed to no tests: {test_source}"));
    }
    classify_change(
        Path::new(OWNER_FILE),
        2,
        line_text,
        &owners,
        &tests,
        Some(root),
        &ReExportIndex::empty(),
        None,
    )
    .ok_or_else(|| "fixture produced no finding".to_string())
}

fn class_of(owner: &str, test_body: &str) -> Result<ExposureClass, String> {
    Ok(classify(owner, test_body)?.class)
}

#[test]
fn throw_test_at_the_boundary_input_closes_the_gap() -> Result<(), String> {
    let owner = owner_source(GUARD);
    for body in [
        "test('throws at zero', () => {\n  expect(() => mustPositive(0)).toThrow();\n});\n",
        "test('throws at zero', () => {\n  expect(() => mustPositive(0)).toThrow(RangeError);\n});\n",
        "test('throws at zero', () => {\n  expect(() => mustPositive(0)).toThrowError(RangeError);\n});\n",
    ] {
        assert_eq!(class_of(&owner, body)?, ExposureClass::Exposed, "{body}");
    }
    // The control from the issue: another test at an input on one side does
    // not take the boundary test's place.
    let both = "test('returns', () => {\n  expect(mustPositive(5)).toBe(5);\n});\ntest('throws at zero', () => {\n  expect(() => mustPositive(0)).toThrow(RangeError);\n});\n";
    assert_eq!(class_of(&owner, both)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn rejects_to_throw_at_the_boundary_input_closes_the_gap() -> Result<(), String> {
    let owner = format!(
        "export async function mustPositive(n: number): Promise<number> {{\n{GUARD}\n  return n;\n}}\n"
    );
    let body = "test('rejects at zero', async () => {\n  await expect(mustPositive(0)).rejects.toThrow(RangeError);\n});\n";
    assert_eq!(class_of(&owner, body)?, ExposureClass::Exposed);
    let other = "test('rejects at five', async () => {\n  await expect(mustPositive(5)).rejects.toThrow(RangeError);\n});\n";
    assert_ne!(class_of(&owner, other)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn throw_test_at_another_input_does_not_close_the_gap() -> Result<(), String> {
    let owner = owner_source(GUARD);
    for input in ["-1", "1", "5"] {
        let body = format!(
            "test('throws', () => {{\n  expect(() => mustPositive({input})).toThrow(RangeError);\n}});\n"
        );
        assert_ne!(
            class_of(&owner, &body)?,
            ExposureClass::Exposed,
            "input {input}"
        );
    }
    Ok(())
}

#[test]
fn throw_test_that_does_not_name_the_guard_error_does_not_close_the_gap() -> Result<(), String> {
    let owner = owner_source(GUARD);
    for matcher in [
        "toThrow(TypeError)",
        "toThrow('bad')",
        "toThrow(/bad/)",
        "toThrow(new RangeError('bad'))",
    ] {
        let body =
            format!("test('throws', () => {{\n  expect(() => mustPositive(0)).{matcher};\n}});\n");
        assert_ne!(
            class_of(&owner, &body)?,
            ExposureClass::Exposed,
            "{matcher}"
        );
    }
    Ok(())
}

#[test]
fn negated_throw_in_the_test_does_not_close_the_gap() -> Result<(), String> {
    let owner = owner_source(GUARD);
    let body = "test('mixed', () => {\n  expect(() => mustPositive(0)).toThrow(RangeError);\n  expect(() => mustPositive(5)).not.toThrow();\n});\n";
    assert_ne!(class_of(&owner, body)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn computed_or_shadowed_call_does_not_close_the_gap() -> Result<(), String> {
    let owner = owner_source(GUARD);
    for body in [
        "test('computed', () => {\n  expect(() => mustPositive(Number('0'))).toThrow(RangeError);\n});\n",
        "test('local', () => {\n  const zero = 0;\n  expect(() => mustPositive(zero)).toThrow(RangeError);\n});\n",
        "test('shadow', () => {\n  const mustPositive = (n: number) => { throw new RangeError('x'); };\n  expect(() => mustPositive(0)).toThrow(RangeError);\n});\n",
        "test('block', () => {\n  expect(() => { mustPositive(0); }).toThrow(RangeError);\n});\n",
    ] {
        assert_ne!(class_of(&owner, body)?, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn guard_that_is_not_one_plain_throw_line_does_not_close_the_gap() -> Result<(), String> {
    let body = "test('throws at zero', () => {\n  expect(() => mustPositive(0)).toThrow(RangeError);\n});\n";
    // The throw is on a later line: the changed line alone is not a throw.
    let later = "export function mustPositive(n: number): number {\n  if (n < 0) {\n    throw new RangeError(\"bad\");\n  }\n  return n;\n}\n";
    assert_ne!(class_of(later, body)?, ExposureClass::Exposed);
    // The condition has a second part that can decide the throw.
    let joined = owner_source("  if (n < 0 && n !== -7) throw new RangeError(\"bad\");");
    assert_ne!(class_of(&joined, body)?, ExposureClass::Exposed);
    // Another throw in the owner can make the call throw on its own.
    let second = "export function mustPositive(n: number): number {\n  if (n < 0) throw new RangeError(\"bad\");\n  if (n === 0) throw new Error(\"zero\");\n  return n;\n}\n";
    assert_ne!(class_of(second, body)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn throw_test_does_not_close_a_guard_that_does_not_throw() -> Result<(), String> {
    let owner = owner_source("  if (n < 0) return 0;");
    let body = "test('throws at zero', () => {\n  expect(() => mustPositive(0)).toThrow();\n});\n";
    assert_ne!(class_of(&owner, body)?, ExposureClass::Exposed);
    Ok(())
}
