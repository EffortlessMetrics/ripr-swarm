//! RIPR-SPEC-0243 rule 27: `toBeCloseTo(<number>)` on a literal-argument call
//! pins a changed `return` expression when the old value passes and the new
//! value falls outside the default tolerance (0.005).

use super::*;

const NEW_OWNER: &str = "export function cmToIn(cm: number): number {\n  return cm / 2.5;\n}\n";
const OLD_LINE: &str = "  return cm / 2.54;";
const NEW_LINE: &str = "  return cm / 2.5;";

fn test_source(body: &str) -> String {
    format!(
        "import {{ expect, it }} from \"vitest\";\nimport {{ cmToIn }} from \"../src/units\";\n\nit(\"converts\", () => {{\n{body}\n}});\n"
    )
}

/// Credited assertions for the edit from `old_line` to `new_line` of
/// `owner_source`, read against `test_source`.
fn close_to_credits(
    label: &str,
    owner_source: &str,
    old_line: &str,
    new_line: &str,
    test_source: &str,
) -> Result<usize, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/units.ts"), owner_source)?;
    ts_write_file(&root.join("tests/units.test.ts"), test_source)?;
    let owner = extract_owners(Path::new("src/units.ts"), owner_source)
        .into_iter()
        .find(|owner| owner.name == "cmToIn")
        .ok_or_else(|| format!("{label}: owner cmToIn not extracted"))?;
    let line = owner_source
        .lines()
        .position(|text| text == new_line.trim_end() || text == new_line)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: changed line not found"))?;
    let tests = extract_tests(Path::new("tests/units.test.ts"), test_source);
    let candidates = related_test_candidates(
        &owner,
        &tests,
        Some(&root),
        &ReExportIndex::empty(),
        None,
        &RelationIndex::empty(),
    );
    let facts = ts_close_to_pin_facts(
        &ProbeFamily::ReturnValue,
        line,
        new_line,
        Some(old_line),
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
    Ok(credited)
}

fn credits(label: &str, body: &str) -> Result<usize, String> {
    close_to_credits(label, NEW_OWNER, OLD_LINE, NEW_LINE, &test_source(body))
}

#[test]
fn a_change_outside_the_tolerance_is_pinned() -> Result<(), String> {
    // 2.54 / 2.54 = 1 passes; 2.54 / 2.5 = 1.016 is 0.016 away.
    assert_eq!(
        credits("close-pin", "  expect(cmToIn(2.54)).toBeCloseTo(1);")?,
        1
    );
    Ok(())
}

#[test]
fn a_change_inside_the_tolerance_is_not_pinned() -> Result<(), String> {
    // 100 / 2.54 = 39.370 and 100 / 2.5 = 40; asserting 39.37 passes the old
    // value, and the new one is 0.63 away: pinned. Asserting 39.99 fails the
    // old value too, so nothing is credited.
    assert_eq!(
        credits("close-far", "  expect(cmToIn(100)).toBeCloseTo(39.37);")?,
        1
    );
    assert_eq!(
        credits(
            "close-old-fails",
            "  expect(cmToIn(100)).toBeCloseTo(39.99);"
        )?,
        0
    );
    // A change smaller than 0.005 on the asserted input does not fail.
    let owner = "export function cmToIn(cm: number): number {\n  return cm / 2.5399;\n}\n";
    assert_eq!(
        close_to_credits(
            "close-small",
            owner,
            OLD_LINE,
            "  return cm / 2.5399;",
            &test_source("  expect(cmToIn(2.54)).toBeCloseTo(1);"),
        )?,
        0
    );
    Ok(())
}

#[test]
fn an_explicit_precision_a_negation_or_a_wrapped_call_earns_nothing() -> Result<(), String> {
    for (label, body) in [
        ("digits", "  expect(cmToIn(2.54)).toBeCloseTo(1, 3);"),
        ("negated", "  expect(cmToIn(2.54)).not.toBeCloseTo(1);"),
        ("wrapped", "  expect(cmToIn(2.54) + 1).toBeCloseTo(2);"),
        (
            "variable",
            "  const out = 1;\n  expect(cmToIn(2.54)).toBeCloseTo(out);",
        ),
        (
            "non-literal argument",
            "  const cm = 2.54;\n  expect(cmToIn(cm)).toBeCloseTo(1);",
        ),
        (
            "other matcher",
            "  expect(cmToIn(2.54)).toBeGreaterThan(0);",
        ),
    ] {
        assert_eq!(credits(label, body)?, 0, "{label}");
    }
    Ok(())
}

#[test]
fn a_shadowed_owner_or_a_multi_line_change_earns_nothing() -> Result<(), String> {
    let shadowed = "  const cmToIn = (cm: number) => cm;\n  expect(cmToIn(2.54)).toBeCloseTo(1);";
    assert_eq!(credits("shadow", shadowed)?, 0);
    // A parameter rebound before the changed return could change what it reads.
    let rebinding =
        "export function cmToIn(cm: number): number {\n  cm = cm + 1;\n  return cm / 2.5;\n}\n";
    assert_eq!(
        close_to_credits(
            "rebind",
            rebinding,
            OLD_LINE,
            NEW_LINE,
            &test_source("  expect(cmToIn(2.54)).toBeCloseTo(1);"),
        )?,
        0
    );
    Ok(())
}
