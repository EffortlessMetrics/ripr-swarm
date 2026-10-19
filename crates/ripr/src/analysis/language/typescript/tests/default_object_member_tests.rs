//! RIPR-SPEC-0243 rule 5: a top-level function held by the module's
//! `export default { name }` object is reached by a default import as
//! `local.name(...)` (mined from axios `utils.merge`). The corpus case
//! `jest-mined-default-export-object` pins the credited reading; these pin
//! the shapes that must stay uncredited.

use super::*;

const OWNER: &str = "function merge(base: any, extra: any): any {\n  const out = { ...base };\n  for (const key of Object.keys(extra)) {\n    out[key] = (out[key] ?? 0) + extra[key];\n  }\n  return out;\n}\n\nexport default { merge };\n";
const CHANGED_LINE: (usize, &str) = (4, "    out[key] = (out[key] ?? 0) + extra[key];");

fn class_of(label: &str, owner_source: &str, test_source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/utils.ts"), owner_source)?;
    ts_write_file(&root.join("tests/utils.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/utils.ts", &[CHANGED_LINE])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("::merge"))
        })
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding for `merge`"))
}

fn test_with(body: &str) -> String {
    format!(
        "import {{ it, expect, vi }} from 'vitest';\nimport utils from '../src/utils';\n\nit('merges', () => {{\n{body}\n}});\n"
    )
}

const CALL: &str =
    "  expect(utils.merge({ a: 1, b: 2 }, { b: 3, c: 4 })).toEqual({ a: 1, b: 5, c: 4 });";

#[test]
fn default_import_member_call_credits_the_owner() -> Result<(), String> {
    for (label, owner) in [
        ("shorthand", OWNER.to_string()),
        (
            "same-name-pair",
            OWNER.replace("{ merge }", "{ merge: merge }"),
        ),
    ] {
        assert_eq!(
            class_of(label, &owner, &test_with(CALL))?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    // Any local name for the default import works.
    let renamed = test_with(CALL)
        .replace("import utils from", "import helpers from")
        .replace("utils.merge", "helpers.merge");
    assert_eq!(
        class_of("renamed-local", OWNER, &renamed)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_object_that_could_replace_the_member_credits_nothing() -> Result<(), String> {
    for (label, object) in [
        ("spread", "{ merge, ...extras }"),
        ("spread-first", "{ ...extras, merge }"),
        ("renamed-entry", "{ merge: other }"),
        ("method", "{ merge() { return {}; } }"),
        ("computed", "{ [name]: merge }"),
        ("duplicate", "{ merge, merge }"),
        ("getter", "{ get merge() { return other; } }"),
    ] {
        let owner = OWNER.replace("{ merge };", &format!("{object};"));
        assert_ne!(
            class_of(label, &owner, &test_with(CALL))?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn a_default_import_used_for_anything_else_credits_nothing() -> Result<(), String> {
    for (label, body) in [
        (
            "alias",
            "  const m = utils.merge;\n  expect(m({ a: 1 }, { b: 2 })).toEqual({ a: 1, b: 2 });",
        ),
        (
            "reassigned",
            "  utils.merge = vi.fn(() => ({ a: 1, b: 2 }));\n  expect(utils.merge({ a: 1 }, { b: 2 })).toEqual({ a: 1, b: 2 });",
        ),
        (
            "assigned-via-object",
            "  Object.assign(utils, { merge: () => ({}) });\n  expect(utils.merge({ a: 1 }, { b: 2 })).toEqual({ a: 1, b: 2 });",
        ),
        (
            "shadowed",
            "  const utils = { merge: () => ({ a: 1, b: 2 }) };\n  expect(utils.merge({ a: 1 }, { b: 2 })).toEqual({ a: 1, b: 2 });",
        ),
        (
            "other-member",
            "  expect(utils.other({ a: 1 }, { b: 2 })).toEqual({ a: 1, b: 2 });",
        ),
    ] {
        assert_ne!(
            class_of(label, OWNER, &test_with(body))?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn a_default_import_of_another_module_credits_nothing() -> Result<(), String> {
    let test = test_with(CALL).replace("../src/utils", "../src/other");
    assert_ne!(
        class_of("other-module", OWNER, &test)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_function_the_object_does_not_hold_is_not_a_member() -> Result<(), String> {
    let owner = OWNER.replace("{ merge };", "{ other };");
    let owner = format!("{owner}function other(): number {{\n  return 1;\n}}\n");
    assert_ne!(
        class_of("not-held", &owner, &test_with(CALL))?,
        ExposureClass::Exposed
    );
    Ok(())
}
