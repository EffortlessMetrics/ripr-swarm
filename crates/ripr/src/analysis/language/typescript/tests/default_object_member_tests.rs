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
            "path-qualified",
            "  const source = { utils: { merge: () => ({ a: 1, b: 2 }) } };\n  expect(source.utils.merge({ a: 1 }, { b: 2 })).toEqual({ a: 1, b: 2 });",
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

// The whole-file use check refuses a replacement or use anywhere in the file,
// setup hooks included.
#[test]
fn a_file_level_or_setup_replacement_of_the_member_credits_nothing() -> Result<(), String> {
    let header = "import { it, expect, vi, beforeEach } from 'vitest';\nimport utils from '../src/utils';\n\n";
    let test = format!("it('merges', () => {{\n{CALL}\n}});\n");
    for (label, setup) in [
        (
            "file-level-assignment",
            "utils.merge = () => ({ a: 1, b: 5, c: 4 });\n",
        ),
        (
            "file-level-spy",
            "vi.spyOn(utils, 'merge').mockReturnValue({ a: 1, b: 5, c: 4 });\n",
        ),
        (
            "setup-hook-assignment",
            "beforeEach(() => {\n  utils.merge = () => ({ a: 1, b: 5, c: 4 });\n});\n",
        ),
    ] {
        assert_ne!(
            class_of(label, OWNER, &format!("{header}{setup}{test}"))?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    // A sibling member's direct call no longer withholds (#6653, amended rule 5):
    // a direct call cannot replace `merge`, and the owner already runs with
    // `this === utils`, so a sibling call adds no new way to lose the credit.
    assert_eq!(
        class_of(
            "other-member-call",
            OWNER,
            &format!("{header}utils.other(1);\n{test}")
        )?,
        ExposureClass::Exposed
    );
    // Control: the same file without a setup use still credits.
    assert_eq!(
        class_of("control", OWNER, &format!("{header}{test}"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_const_arrow_member_is_reached_like_a_function() -> Result<(), String> {
    let owner = "const merge = (base: any, extra: any): any => {\n  const out = { ...base };\n  for (const key of Object.keys(extra)) {\n    out[key] = (out[key] ?? 0) + extra[key];\n  }\n  return out;\n};\n\nexport default { merge };\n";
    assert_eq!(
        class_of("arrow-member", owner, &test_with(CALL))?,
        ExposureClass::Exposed
    );
    // A `let` arrow reassigned before the export is not the member the
    // object holds, so the owner is dropped.
    let reassignable = owner
        .replacen("const merge", "let merge", 1)
        .replace("\nexport default", "\nmerge = () => ({});\nexport default");
    // A `let` arrow rebound inside a nested function is just as unreliable.
    let nested = owner.replacen("const merge", "let merge", 1).replace(
        "\nexport default",
        "\nfunction patch() {\n  merge = () => ({});\n}\nexport default",
    );
    assert!(
        class_of("nested-write-arrow-member", &nested, &test_with(CALL))
            .map_or(true, |class| class != ExposureClass::Exposed)
    );
    // No finding for the dropped owner counts as uncredited.
    assert!(
        class_of("let-arrow-member", &reassignable, &test_with(CALL))
            .map_or(true, |class| class != ExposureClass::Exposed)
    );
    Ok(())
}

#[test]
fn a_title_or_comment_naming_the_local_does_not_veto_the_call() -> Result<(), String> {
    let test = format!(
        "import {{ it, expect }} from 'vitest';\nimport utils from '../src/utils';\n\nit('utils merges', () => {{\n  // utils.merge is the member under test\n{CALL}\n}});\n"
    );
    assert_eq!(
        class_of("title-mention", OWNER, &test)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_named_import_call_cannot_launder_a_replaced_default_member() -> Result<(), String> {
    let owner = OWNER.replace("function merge", "export function merge");
    let test = "import { it, expect } from 'vitest';\nimport { merge } from '../src/utils';\nimport utils from '../src/utils';\n\nit('merges', () => {\n  utils.merge = () => ({ a: 1, b: 5, c: 4 });\n  merge({ a: 1, b: 2 }, { b: 3, c: 4 });\n  expect(utils.merge({ a: 1, b: 2 }, { b: 3, c: 4 })).toEqual({ a: 1, b: 5, c: 4 });\n});\n";
    assert_ne!(
        class_of("named-plus-replaced-default", &owner, test)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn calls_of_several_members_still_credit_each_called_member() -> Result<(), String> {
    // axios `utils.merge` beside `utils.extend` (#6653): every use is still a
    // direct member call, so the sibling member no longer withholds `merge`.
    let two = test_with(&format!(
        "{CALL}\n  expect(utils.extend({{ a: 1 }}, {{ b: 2 }})).toEqual({{ a: 1, b: 2 }});"
    ));
    assert_eq!(
        class_of("two-members", OWNER, &two)?,
        ExposureClass::Exposed
    );
    // The real axios shape: the default object holds both members.
    let both = OWNER.replace(
        "export default { merge };",
        "function extend(a: any, b: any): any {\n  return { ...a, ...b };\n}\n\nexport default { merge, extend };",
    );
    assert_eq!(
        class_of("two-members-both-exported", &both, &two)?,
        ExposureClass::Exposed
    );
    // A sibling use that is not a direct call still withholds the credit.
    let leaked = test_with(&format!(
        "{CALL}\n  const other = utils.extend;\n  other({{}}, {{}});"
    ));
    assert_ne!(
        class_of("sibling-alias", OWNER, &leaked)?,
        ExposureClass::Exposed
    );
    // Only the member the test calls is credited, not an uncalled sibling.
    let unrelated =
        test_with("  expect(utils.extend({ a: 1 }, { b: 2 })).toEqual({ a: 1, b: 2 });");
    assert_ne!(
        class_of("sibling-only", OWNER, &unrelated)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_call_beside_a_non_direct_use_of_the_local_credits_nothing() -> Result<(), String> {
    // Each form sits next to a plain `utils.merge(...)` call; any use that is
    // not a direct `local.member(` call withholds every member (rule 5).
    for (label, extra) in [
        ("optional-call", "  utils.merge?.({}, {});"),
        ("computed-call", "  utils['merge']({}, {});"),
        (
            "destructure",
            "  const { merge } = utils;\n  merge({}, {});",
        ),
        ("value-pass", "  [1].map(() => utils);"),
    ] {
        let test = test_with(&format!("{CALL}\n{extra}"));
        assert_ne!(
            class_of(label, OWNER, &test)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn a_sibling_that_can_replace_the_member_through_this_credits_nothing() -> Result<(), String> {
    // `utils.extend()` runs with `this === utils` and can stub `merge` before
    // the asserted call; no test-file fact sees that write (#13694 review).
    let test = test_with(&format!("  utils.extend();\n{CALL}"));
    for (label, extend) in [
        (
            "function-this-write",
            "function extend(): void {\n  this.merge = () => ({ a: 1, b: 5, c: 4 });\n}",
        ),
        (
            "nested-this-escape",
            "function extend(): void {\n  register(() => this);\n}",
        ),
        (
            "arrow-this",
            "const extend = (): void => {\n  this.merge = () => ({ a: 1, b: 5, c: 4 });\n};",
        ),
    ] {
        let owner = OWNER.replace(
            "export default { merge };",
            &format!("{extend}\n\nexport default {{ merge, extend }};"),
        );
        assert_ne!(
            class_of(label, &owner, &test)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    // Control: a sibling with no `this` still lets `merge` be credited.
    let owner = OWNER.replace(
        "export default { merge };",
        "function extend(): void {\n  return;\n}\n\nexport default { merge, extend };",
    );
    assert_eq!(
        class_of("sibling-without-this", &owner, &test)?,
        ExposureClass::Exposed
    );
    Ok(())
}
