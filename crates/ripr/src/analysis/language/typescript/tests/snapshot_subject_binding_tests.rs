//! RIPR-SPEC-0258 rule 3 (#12465): on an effect seam a snapshot confirms only
//! when its asserted subject is the whole result of the owner call, or a local
//! bound once to it. A snapshot of any other value serializes nothing the
//! owner produced, so it earns no credit.

use super::*;
use std::path::Path;

const LIB_FILE: &str = "src/lib.ts";
const TEST_FILE: &str = "tests/lib.test.ts";
const CHANGED_TEXT: &str = "    console.log(\"shipped \" + id);";
const LIB: &str =
    "export function notify(id: number): void {\n    console.log(\"shipped \" + id);\n}\n";

fn class_for(test_source: &str) -> Result<ExposureClass, String> {
    let owners = extract_owners(Path::new(LIB_FILE), LIB);
    let tests = extract_tests(Path::new(TEST_FILE), test_source);
    // Fixture construction: one test whose one assertion is a strong snapshot,
    // so a weak reading below is the subject guard and not a missing oracle.
    let [test] = tests.as_slice() else {
        return Err(format!("expected one test, got {}", tests.len()));
    };
    let [assertion] = test.assertions.as_slice() else {
        return Err(format!("expected one assertion: {test:?}"));
    };
    if assertion.oracle_kind != OracleKind::Snapshot
        || assertion.oracle_strength != OracleStrength::Strong
    {
        return Err(format!("expected a strong snapshot: {assertion:?}"));
    }
    let line = LIB
        .lines()
        .position(|line| line == CHANGED_TEXT)
        .map(|index| index + 1)
        .ok_or_else(|| "lib is missing the changed line".to_string())?;
    classify_change(
        Path::new(LIB_FILE),
        line,
        CHANGED_TEXT,
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "expected a finding".to_string())
}

fn test_with(setup_and_assert: &str) -> String {
    format!("import {{ notify }} from '../src/lib';\ntest('n', () => {{\n{setup_and_assert}}});\n")
}

#[test]
fn snapshot_of_the_owner_result_confirms_the_effect_seam() -> Result<(), String> {
    for body in [
        "  expect(notify(7)).toMatchInlineSnapshot(`undefined`);\n",
        "  const result = notify(7);\n  expect(result).toMatchInlineSnapshot(`undefined`);\n",
    ] {
        let class = class_for(&test_with(body))?;
        if !matches!(class, ExposureClass::Exposed) {
            return Err(format!("{body}: expected exposed, got {class:?}"));
        }
    }
    Ok(())
}

#[test]
fn snapshot_of_an_unrelated_subject_does_not_confirm_the_effect_seam() -> Result<(), String> {
    for body in [
        // The #12465 repro: the owner runs, an unrelated value is snapshotted.
        "  notify(7);\n  const unrelated = 'x';\n  expect(unrelated).toMatchInlineSnapshot(`\"x\"`);\n",
        "  notify(7);\n  expect('x').toMatchInlineSnapshot(`\"x\"`);\n",
        // A local bound to something other than the owner call.
        "  notify(7);\n  const other = 'y';\n  expect(other).toMatchInlineSnapshot(`\"y\"`);\n",
        // A member read on the result is not the whole result: not on the allowlist.
        "  expect(notify(7).toString()).toMatchInlineSnapshot(`\"undefined\"`);\n",
        // A local the test rewrites after binding the owner call.
        "  let result = notify(7);\n  result = 'x';\n  expect(result).toMatchInlineSnapshot(`\"x\"`);\n",
    ] {
        let class = class_for(&test_with(body))?;
        if matches!(class, ExposureClass::Exposed) {
            return Err(format!("{body}: unrelated subject must not read exposed"));
        }
    }
    Ok(())
}
