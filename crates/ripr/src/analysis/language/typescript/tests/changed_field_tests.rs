//! RIPR-SPEC-0243 rule 17: the changed field of a single-line returned
//! object literal is the one a test must read from the owner's result.

use super::playwright_unit_tests::finding_for_edit;
use super::*;

const DIMS: &str = "export function dims(w: number, h: number): { area: number; perimeter: number } {\n  return { area: w * h, perimeter: 2 * w + h };\n}\n";
const DIMS_OLD: &str = "  return { area: w * h, perimeter: 2 * (w + h) };";

fn dims_finding(label: &str, body: &str) -> Result<Finding, String> {
    let test = format!(
        "import {{ expect, test }} from \"@playwright/test\";\nimport {{ dims }} from \"../src/dims\";\n\ntest(\"dims\", () => {{\n{body}\n}});\n"
    );
    finding_for_edit(label, "dims", DIMS, 2, DIMS_OLD, &test)
}

#[test]
fn reading_the_changed_field_of_an_owner_result_is_an_observation() -> Result<(), String> {
    for (label, body) in [
        (
            "alias",
            "  const d = dims(3, 4);\n  expect(d.area).toBe(12);\n  expect(d.perimeter).toBe(14);",
        ),
        ("direct", "  expect(dims(3, 4).perimeter).toBe(14);"),
        (
            "soft",
            "  const d = dims(3, 4);\n  expect.soft(d.area).toBe(12);\n  expect.soft(d.perimeter).toBe(14);",
        ),
    ] {
        let finding = dims_finding(label, body)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn reading_only_an_unchanged_field_or_a_foreign_object_is_not() -> Result<(), String> {
    for (label, body) in [
        (
            "unchanged field",
            "  const d = dims(3, 4);\n  expect(d.area).toBe(12);",
        ),
        (
            "foreign object",
            "  dims(3, 4);\n  const d = { area: 12, perimeter: 14 };\n  expect(d.perimeter).toBe(14);",
        ),
        (
            "other owner",
            "  const d = other(3, 4);\n  expect(d.perimeter).toBe(14);",
        ),
    ] {
        let finding = dims_finding(label, body)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn the_changed_field_needs_matching_keys_and_exactly_one_new_value() {
    let field = |old: &str, new: &str| ts_changed_object_field(Some(old), new);
    let changed = field("return { a: 1, b: 2 };", "return { a: 1, b: 3 };");
    assert_eq!(
        changed,
        Some(TypeScriptChangedField {
            key: "b".to_string(),
            value: "3".to_string(),
        })
    );
    // Parenthesized, nested commas and quoted commas stay in their value.
    assert_eq!(
        field(
            "return ({ a: f(1, 2), b: \"x,y\" });",
            "return ({ a: f(1, 2), b: \"x,z\" });"
        )
        .map(|f| f.key),
        Some("b".to_string())
    );
    for (old, new) in [
        ("return { a: 1, b: 2 };", "return { a: 2, b: 3 };"),
        ("return { a: 1, b: 2 };", "return { a: 1, c: 2 };"),
        ("return { a: 1, b: 2 };", "return { a: 1, b: 2, c: 3 };"),
        ("return { a: 1, b };", "return { a: 1, b: 2 };"),
        ("return { ...a, b: 1 };", "return { ...a, b: 2 };"),
        ("return { a: 1, b: 2 };", "return { a: 1, b: 2 };"),
        ("return { a: 1 };", "return { a: 2 };"),
    ] {
        assert_eq!(field(old, new), None, "{old} -> {new}");
    }
    assert_eq!(
        ts_changed_object_field(None, "return { a: 1, b: 2 };"),
        None
    );
}
