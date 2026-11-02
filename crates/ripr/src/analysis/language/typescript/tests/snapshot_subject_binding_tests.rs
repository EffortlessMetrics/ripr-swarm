//! RIPR-SPEC-0258 rule 3 (#12465): on an effect seam a snapshot confirms only
//! when its asserted subject is the whole result of the owner call (a free
//! function, or a method on a receiver the test constructs from the owner's
//! class), or a local bound once to it. A snapshot of any other value
//! serializes nothing the owner produced, so the seam reads weak.

use super::*;
use std::path::Path;

const LIB_FILE: &str = "src/lib.ts";
const TEST_FILE: &str = "tests/lib.test.ts";

const FN_LIB: &str = "export function enqueue(queue: string[], id: number): string[] {\n    queue.push(\"shipped \" + id);\n    return queue;\n}\n";
const FN_CHANGED: &str = "    queue.push(\"shipped \" + id);";
const FN_IMPORT: &str = "import { enqueue } from '../src/lib';\n";

const CLASS_LIB: &str = "export class Outbox {\n    private lines: string[] = [];\n\n    send(id: number): string[] {\n        this.lines.push(\"shipped \" + id);\n        return this.lines;\n    }\n}\n";
const CLASS_CHANGED: &str = "        this.lines.push(\"shipped \" + id);";
const CLASS_IMPORT: &str = "import { Outbox } from '../src/lib';\n";

const SNAPSHOT: &str = "toMatchInlineSnapshot(`[\"shipped 7\"]`)";

fn class_for(
    lib: &str,
    changed: &str,
    import: &str,
    body: &str,
) -> Result<ExposureClass, String> {
    let test_source = format!("{import}test('n', () => {{\n{body}}});\n");
    let owners = extract_owners(Path::new(LIB_FILE), lib);
    let tests = extract_tests(Path::new(TEST_FILE), &test_source);
    // Fixture construction: one test whose snapshot assertion is strong, so a
    // weak reading below is the subject guard and not a missing oracle.
    let [test] = tests.as_slice() else {
        return Err(format!("expected one test, got {}", tests.len()));
    };
    if !test.assertions.iter().any(|assertion| {
        assertion.oracle_kind == OracleKind::Snapshot
            && assertion.oracle_strength == OracleStrength::Strong
    }) {
        return Err(format!("expected a strong snapshot: {test:?}"));
    }
    let line = lib
        .lines()
        .position(|line| line == changed)
        .map(|index| index + 1)
        .ok_or_else(|| "lib is missing the changed line".to_string())?;
    classify_change(
        Path::new(LIB_FILE),
        line,
        changed,
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "expected a finding".to_string())
}

fn expect_class(
    lib: &str,
    changed: &str,
    import: &str,
    bodies: &[String],
    want_exposed: bool,
) -> Result<(), String> {
    for body in bodies {
        let class = class_for(lib, changed, import, body)?;
        let ok = if want_exposed {
            matches!(class, ExposureClass::Exposed)
        } else {
            matches!(class, ExposureClass::WeaklyExposed)
        };
        if !ok {
            return Err(format!("{body}: unexpected class {class:?}"));
        }
    }
    Ok(())
}

#[test]
fn snapshot_of_the_function_owner_result_confirms_the_effect_seam() -> Result<(), String> {
    expect_class(
        FN_LIB,
        FN_CHANGED,
        FN_IMPORT,
        &[
            format!("  expect(enqueue([], 7)).{SNAPSHOT};\n"),
            format!("  const result = enqueue([], 7);\n  expect(result).{SNAPSHOT};\n"),
        ],
        true,
    )
}

#[test]
fn snapshot_of_the_method_owner_result_confirms_the_effect_seam() -> Result<(), String> {
    expect_class(
        CLASS_LIB,
        CLASS_CHANGED,
        CLASS_IMPORT,
        &[
            format!("  const box = new Outbox();\n  expect(box.send(7)).{SNAPSHOT};\n"),
            format!(
                "  const box = new Outbox();\n  const result = box.send(7);\n  expect(result).{SNAPSHOT};\n"
            ),
        ],
        true,
    )
}

#[test]
fn snapshot_of_an_unrelated_function_subject_reads_weak() -> Result<(), String> {
    expect_class(
        FN_LIB,
        FN_CHANGED,
        FN_IMPORT,
        &[
            // The #12465 repro: the owner runs, an unrelated value is snapshotted.
            format!("  enqueue([], 7);\n  const u = ['shipped 7'];\n  expect(u).{SNAPSHOT};\n"),
            format!("  enqueue([], 7);\n  expect(['shipped 7']).{SNAPSHOT};\n"),
            // A member read on the result is not the whole result.
            "  expect(enqueue([], 7).length).toMatchInlineSnapshot(`1`);\n".to_string(),
            // A spy's recorded calls are not on the allowlist (weak by design).
            "  const spy = jest.spyOn(console, 'log');\n  enqueue([], 7);\n  expect(spy.mock.calls).toMatchInlineSnapshot(`[]`);\n"
                .to_string(),
        ],
        false,
    )
}

#[test]
fn a_bound_local_that_is_rewritten_or_shadowed_reads_weak() -> Result<(), String> {
    expect_class(
        FN_LIB,
        FN_CHANGED,
        FN_IMPORT,
        &[
            format!("  let r = enqueue([], 7);\n  r = ['shipped 7'];\n  expect(r).{SNAPSHOT};\n"),
            format!(
                "  let r = enqueue([], 7);\n  ({{ r }} = {{ r: ['shipped 7'] }});\n  expect(r).{SNAPSHOT};\n"
            ),
            format!(
                "  let r = enqueue([], 7);\n  [r] = [['shipped 7']];\n  expect(r).{SNAPSHOT};\n"
            ),
            format!(
                "  const r = enqueue([], 7);\n  for (const r of [['shipped 7']]) {{\n    expect(r).{SNAPSHOT};\n  }}\n"
            ),
            format!(
                "  let r = enqueue([], 7);\n  ({{ ...r }} = {{ a: 1 }});\n  expect(r).{SNAPSHOT};\n"
            ),
        ],
        false,
    )
}

#[test]
fn snapshot_of_an_unrelated_method_subject_reads_weak() -> Result<(), String> {
    expect_class(
        CLASS_LIB,
        CLASS_CHANGED,
        CLASS_IMPORT,
        &[
            format!(
                "  const box = new Outbox();\n  box.send(7);\n  const u = ['shipped 7'];\n  expect(u).{SNAPSHOT};\n"
            ),
            // The receiver is rebound to something other than the owner's class.
            format!(
                "  let box = new Outbox();\n  box = {{ send: (n) => ['shipped 7'] }};\n  expect(box.send(7)).{SNAPSHOT};\n"
            ),
            // The method is replaced on the receiver.
            format!(
                "  const box = new Outbox();\n  box.send = (n) => ['shipped 7'];\n  expect(box.send(7)).{SNAPSHOT};\n"
            ),
        ],
        false,
    )
}
