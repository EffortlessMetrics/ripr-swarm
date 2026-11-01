//! A floating `expect(p).resolves...` / `.rejects...` statement (no `await`,
//! no `return`) cannot fail its test, so it supplies no discriminator and the
//! test reads `static_unknown`, like a floating `.then`. Awaited, returned and
//! expression-bodied forms stay credited. Each test pins the parsed
//! assertions before the class.

use super::nested_callback_tests::settings_finding_in;
use super::*;

const SOURCE: &str = "export function f(n: number): boolean {\n  return n > 100;\n}\n";
const LINE: (usize, &str) = (2, "  return n > 100;");
const REMOVED: &str = "  return n >= 100;";
const HEAD: &str = "import { f } from \"../src/settings\";\n";

fn test_file(body: &str) -> String {
    format!("{HEAD}{body}\n")
}

fn assertion_count(source: &str) -> usize {
    extract_tests(Path::new("tests/settings.test.ts"), source)
        .iter()
        .map(|test| test.assertions.len())
        .sum()
}

fn class_of(label: &str, body: &str) -> Result<ExposureClass, String> {
    settings_finding_in(label, SOURCE, LINE, &test_file(body), REMOVED).map(|finding| finding.class)
}

#[test]
fn awaited_returned_and_expression_bodied_promise_assertions_stay_exposed() -> Result<(), String> {
    for (label, body) in [
        (
            "awaited",
            "test('t', async () => { await expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
        (
            "awaited-rejects",
            "test('t', async () => { await expect(Promise.reject(f(100))).rejects.toBe(false); });",
        ),
        (
            "returned",
            "test('t', () => { return expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
        (
            "expression-body",
            "test('t', () => expect(Promise.resolve(f(100))).resolves.toBe(false));",
        ),
        (
            "async-expression-body",
            "test('t', async () => expect(Promise.resolve(f(100))).resolves.toBe(false));",
        ),
        (
            "awaited-then-expression-callback",
            "test('t', async () => { await Promise.resolve(1).then(() => expect(Promise.resolve(f(100))).resolves.toBe(false)); });",
        ),
        (
            "awaited-then-floating-other",
            "test('t', async () => {\n  await expect(Promise.resolve(f(100))).resolves.toBe(false);\n  expect(Promise.resolve(1)).resolves.toBe(1);\n});",
        ),
    ] {
        let source = test_file(body);
        assert!(assertion_count(&source) >= 1, "{label}: parsed assertions");
        assert_eq!(class_of(label, body)?, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn floating_promise_assertions_supply_no_discriminator() -> Result<(), String> {
    for (label, body) in [
        (
            "async-floating",
            "test('t', async () => { expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
        (
            "async-floating-rejects",
            "test('t', async () => { expect(Promise.reject(f(100))).rejects.toThrow(); });",
        ),
        (
            "sync-callback-floating",
            "test('t', () => { expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
        (
            "void-discarded",
            "test('t', async () => { void expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
        (
            "not-resolves-floating",
            "test('t', async () => { expect(Promise.resolve(f(100))).not.resolves.toBe(true); });",
        ),
        (
            "soft-floating",
            "test('t', async () => { expect.soft(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
        (
            "non-final-statement",
            "test('t', async () => {\n  expect(Promise.resolve(f(100))).resolves.toBe(false);\n  await Promise.resolve();\n});",
        ),
        (
            "block-body-last-statement",
            "test('t', async () => {\n  expect(Promise.resolve(f(100))).resolves.toBe(false)\n});",
        ),
    ] {
        let source = test_file(body);
        assert_eq!(
            assertion_count(&source),
            0,
            "{label}: no credited assertion"
        );
        assert_eq!(
            class_of(label, body)?,
            ExposureClass::StaticUnknown,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn promise_all_wrapping_is_never_newly_credited() -> Result<(), String> {
    // The wrapped assertions are not read by the extractor; the control pins
    // only that nothing is credited as `exposed`.
    let body = "test('t', async () => { await Promise.all([expect(Promise.resolve(f(100))).resolves.toBe(false)]); });";
    assert_eq!(assertion_count(&test_file(body)), 0);
    assert_eq!(class_of("promise-all", body)?, ExposureClass::StaticUnknown);
    Ok(())
}

#[test]
fn floating_owner_assertion_beside_an_awaited_unrelated_one_stays_a_gap() -> Result<(), String> {
    let body = "test('t', async () => {\n  expect(Promise.resolve(f(100))).resolves.toBe(false);\n  await expect(Promise.resolve(1)).resolves.toBe(1);\n});";
    assert_eq!(assertion_count(&test_file(body)), 1, "only the awaited one");
    assert_eq!(
        class_of("mixed", body)?,
        ExposureClass::WeaklyExposed,
        "mixed"
    );
    Ok(())
}

#[test]
fn a_for_each_expression_body_assertion_floats() {
    // `forEach` discards its callback's result, so the expression-bodied
    // assertion cannot fail the test and earns no credit.
    let body =
        "test('t', () => { [100].forEach(v => expect(Promise.resolve(f(v))).resolves.toBe(false)); });";
    assert_eq!(assertion_count(&test_file(body)), 0);
}
