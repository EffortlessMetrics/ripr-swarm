//! #12922: the TypeScript adapter never emits `propagation_unknown`. It emits
//! `reachable_unrevealed` and `infection_unknown` (pinned in
//! `infection_tests`), but no code path assigns `PropagationUnknown`. An
//! input whose assertion cannot fail its test (a floating promise matcher)
//! must read `static_unknown`, never `exposed`, and never the unemitted
//! class. If the adapter later starts emitting `propagation_unknown`, the
//! sweep below fails and this waiver must be replaced by a TypeScript verdict
//! corpus `[[axis]]` with a floor.

use super::nested_callback_tests::settings_finding_in;
use super::*;

const SOURCE: &str = "export function f(n: number): boolean {\n  return n > 100;\n}\n";
const LINE: (usize, &str) = (2, "  return n > 100;");
const REMOVED: &str = "  return n >= 100;";
const HEAD: &str = "import { f } from \"../src/settings\";\n";

fn finding_of(label: &str, body: &str) -> Result<Finding, String> {
    let source = format!("{HEAD}{body}\n");
    let tests = extract_tests(Path::new("tests/settings.test.ts"), &source);
    assert_eq!(tests.len(), 1, "{label}: fixture must register its test");
    settings_finding_in(label, SOURCE, LINE, &source, REMOVED)
}

const AWAITED: &str =
    "test('t', async () => { await expect(Promise.resolve(f(100))).resolves.toBe(false); });";

#[test]
fn unknown_stage_inputs_read_static_unknown_never_exposed_or_propagation_unknown()
-> Result<(), String> {
    for (label, body) in [
        (
            "floating-resolves",
            "test('t', async () => { expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
        (
            "floating-rejects",
            "test('t', async () => { expect(Promise.reject(f(100))).rejects.toThrow(); });",
        ),
        (
            "void-discarded",
            "test('t', async () => { void expect(Promise.resolve(f(100))).resolves.toBe(false); });",
        ),
    ] {
        let finding = finding_of(label, body)?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{label}");
        assert_ne!(finding.class, ExposureClass::Exposed, "{label}");
        assert_ne!(finding.class, ExposureClass::PropagationUnknown, "{label}");
    }
    Ok(())
}

#[test]
fn the_awaited_control_is_exposed_so_the_sweep_can_tell_the_classes_apart() -> Result<(), String> {
    let finding = finding_of("awaited-control", AWAITED)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}
