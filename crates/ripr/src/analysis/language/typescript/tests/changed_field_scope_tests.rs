//! #14856 / #14858: a read must name the changed value to witness it. A local
//! alias of the owner call that is asserted through a sibling field, and an
//! unchanged child of the changed field, both observe a value the change does
//! not alter and must not read `exposed`. The changed field (or the changed
//! child) through the same path must stay `exposed`.

use super::*;

const OWNER: &str = "export interface C { retries: number | { n: number }; verbose: boolean }\nexport function f(): C {\n  return { retries: 3, verbose: false };\n}\n";

fn class_for(new_line: &str, old_line: &str, test_body: &str) -> Result<ExposureClass, String> {
    let test_src =
        format!("import {{ f }} from \"./owner\";\ntest(\"reads\", () => {{ {test_body} }});\n");
    let owners = extract_owners(Path::new("owner.ts"), OWNER);
    let tests = extract_tests(Path::new("owner.test.ts"), &test_src);
    assert_eq!(owners.len(), 1, "expected one owner from live extractor");
    assert_eq!(tests.len(), 1, "expected one test from live extractor");
    let finding = classify_change_with_alias_state(
        Path::new("owner.ts"),
        3,
        new_line,
        Some(old_line),
        &owners,
        &tests,
        None,
        &ReExportIndex::empty(),
        None,
        None,
    )
    .ok_or_else(|| "expected a finding".to_string())?;
    Ok(finding.class)
}

const OLD: &str = "  return { retries: 3, verbose: false };";
const NEW_SCALAR: &str = "  return { retries: 5, verbose: false };";
const NEW_OBJECT: &str = "  return { retries: { n: 5 }, verbose: false };";

fn assert_class(got: ExposureClass, want_exposed: bool, what: &str) -> Result<(), String> {
    let is_exposed = matches!(got, ExposureClass::Exposed);
    let is_weak = matches!(got, ExposureClass::WeaklyExposed);
    if want_exposed && !is_exposed {
        return Err(format!("{what}: expected exposed, got {got:?}"));
    }
    if !want_exposed && !is_weak {
        return Err(format!("{what}: expected weakly_exposed, got {got:?}"));
    }
    Ok(())
}

/// #14856: the alias form of the sibling read downgrades.
#[test]
fn alias_local_sibling_field_read_downgrades() -> Result<(), String> {
    let got = class_for(
        NEW_SCALAR,
        OLD,
        "const cfg = f(); expect(cfg.verbose).toBe(false);",
    )?;
    assert_class(got, false, "alias sibling read")
}

/// #14856 control: the changed field through the same alias stays exposed.
#[test]
fn alias_local_changed_field_read_stays_exposed() -> Result<(), String> {
    let got = class_for(
        NEW_SCALAR,
        OLD,
        "const cfg = f(); expect(cfg.retries).toBe(5);",
    )?;
    assert_class(got, true, "alias changed-field read")
}

/// #14856 control: a local that does not alias the owner is not owner-rooted,
/// so the guard does not claim it; the bracket form of the alias also
/// downgrades like the dotted form.
#[test]
fn alias_local_bracket_sibling_field_read_downgrades() -> Result<(), String> {
    let got = class_for(
        NEW_SCALAR,
        OLD,
        "const cfg = f(); expect(cfg[\"verbose\"]).toBe(false);",
    )?;
    assert_class(got, false, "alias bracket sibling read")
}

/// #14858: an unchanged child of the changed field downgrades.
#[test]
fn unchanged_child_of_changed_field_downgrades() -> Result<(), String> {
    let got = class_for(NEW_OBJECT, OLD, "expect(f().retries.other).toBe(1);")?;
    assert_class(got, false, "unchanged child (direct)")
}

/// #14858 control: the changed child through the same path stays exposed.
#[test]
fn changed_child_of_changed_field_stays_exposed() -> Result<(), String> {
    let got = class_for(NEW_OBJECT, OLD, "expect(f().retries.n).toBe(5);")?;
    assert_class(got, true, "changed child (direct)")
}

/// #14858 through an alias local: the unchanged child never reads exposed.
#[test]
fn alias_child_reads_split_changed_from_unchanged() -> Result<(), String> {
    let other = class_for(
        NEW_OBJECT,
        OLD,
        "const cfg = f(); expect(cfg.retries.other).toBe(1);",
    )?;
    assert_class(other, false, "unchanged child (alias)")?;
    Ok(())
}

/// #14858: when both versions are object literals, a child whose value did not
/// move is unchanged and a child whose value moved is changed.
#[test]
fn object_to_object_change_splits_children_by_value() -> Result<(), String> {
    let old = "  return { retries: { n: 3, other: 1 }, verbose: false };";
    let new = "  return { retries: { n: 5, other: 1 }, verbose: false };";
    let unchanged = class_for(new, old, "expect(f().retries.other).toBe(1);")?;
    assert_class(unchanged, false, "object->object unchanged child")?;
    let changed = class_for(new, old, "expect(f().retries.n).toBe(5);")?;
    assert_class(changed, true, "object->object changed child")
}

/// Must-not-move: a scalar changed field read through a child (`.length`) or
/// the whole changed field keeps the existing verdict, since the new value is
/// not an object literal the guard can enumerate.
#[test]
fn non_object_changed_value_child_read_keeps_credit() -> Result<(), String> {
    let got = class_for(
        NEW_SCALAR,
        OLD,
        "expect(f().retries.toFixed()).toBe(\"5\");",
    )?;
    assert_class(got, true, "method on scalar changed field")?;
    let whole = class_for(NEW_OBJECT, OLD, "expect(f().retries).toEqual({ n: 5 });")?;
    assert_class(whole, true, "whole changed object")
}

/// Review of #15644: a read of a member the OLD primitive answered through its
/// prototype (`3 .toFixed`) changes when the field becomes an object, so it
/// keeps its credit. An old value whose members the guard does not model (an
/// array's or a string's `.length`, `null`) keeps the existing credit too.
#[test]
fn non_object_old_value_member_read_keeps_credit() -> Result<(), String> {
    let prototype = class_for(
        NEW_OBJECT,
        OLD,
        "expect(typeof f().retries.toFixed).toBe(\"undefined\");",
    )?;
    assert_class(prototype, true, "old number's prototype member")?;
    for old in [
        "  return { retries: [1, 2], verbose: false };",
        "  return { retries: \"ab\", verbose: false };",
        "  return { retries: null, verbose: false };",
    ] {
        let got = class_for(NEW_OBJECT, old, "expect(f().retries.length).toBe(2);")?;
        assert_class(got, true, old)?;
    }
    Ok(())
}
