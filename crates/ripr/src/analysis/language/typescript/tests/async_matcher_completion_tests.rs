//! Runner-completion admission for direct `.resolves` / `.rejects` matchers
//! (#8933). A matcher promise must be awaited, returned to the runner, or be
//! the expression-bodied test value; a `return` inside a discarded callback
//! (for example `forEach`) does not propagate.

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

fn admission(source: &str) -> TypeScriptAssertionAdmission {
    extract_tests(Path::new("tests/settings.test.ts"), source)
        .first()
        .map(|test| test.assertion_admission)
        .unwrap_or(TypeScriptAssertionAdmission::Unresolved)
}

#[test]
fn foreach_returned_async_matchers_supply_no_discriminator() {
    for (label, body) in [
        (
            "resolves",
            "test('t', () => { [100].forEach((v) => { return expect(Promise.resolve(f(v))).resolves.toBe(false); }); });",
        ),
        (
            "rejects",
            "test('t', () => { [100].forEach((v) => { return expect(Promise.reject(f(v))).rejects.toBe(false); }); });",
        ),
        (
            "resolves-not",
            "test('t', () => { [100].forEach((v) => { return expect(Promise.resolve(f(v))).not.resolves.toBe(true); }); });",
        ),
    ] {
        let source = test_file(body);
        assert_eq!(
            assertion_count(&source),
            0,
            "{label}: no credited assertion"
        );
        assert_eq!(
            admission(&source),
            TypeScriptAssertionAdmission::Unresolved,
            "{label}: admission"
        );
    }
}

#[test]
fn foreach_returned_synchronous_matchers_stay_credited() {
    let body =
        "test('t', () => { [100, 200].forEach((v) => { return expect(f(v)).toBe(false); }); });";
    let source = test_file(body);
    assert_eq!(assertion_count(&source), 2, "one case per element");
}

#[test]
fn runner_returned_and_awaited_async_matchers_stay_credited() -> Result<(), String> {
    for (label, body) in [
        (
            "top-level-return",
            "test('t', () => { return expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
        (
            "awaited",
            "test('t', async () => { await expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
    ] {
        let source = test_file(body);
        assert!(assertion_count(&source) >= 1, "{label}");
        let finding = settings_finding_in(label, SOURCE, LINE, &source, REMOVED)?;
        assert_ne!(finding.class, ExposureClass::StaticUnknown, "{label}");
    }
    Ok(())
}

#[test]
fn foreach_returned_async_matcher_beside_awaited_sibling_credits_only_awaited() -> Result<(), String>
{
    let body = "test('t', async () => {\n  [100].forEach((v) => { return expect(Promise.resolve(f(v))).resolves.toBe(false); });\n  await expect(Promise.resolve(1)).resolves.toBe(1);\n});";
    let source = test_file(body);
    assert_eq!(assertion_count(&source), 1, "only the awaited matcher");
    let finding = settings_finding_in("mixed", SOURCE, LINE, &source, REMOVED)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}
