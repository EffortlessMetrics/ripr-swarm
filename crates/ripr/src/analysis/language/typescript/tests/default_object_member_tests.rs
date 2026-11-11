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

#[test]
fn a_sibling_that_can_reach_the_object_without_this_credits_nothing() -> Result<(), String> {
    // Routes to the anonymous default object that need no `this` (#13694
    // review): CommonJS globals, a self-import, a dynamic import, or a
    // helper that does one of those.
    let test = test_with(&format!("  utils.extend();\n{CALL}"));
    let stub = "() => ({ a: 1, b: 5, c: 4 })";
    for (label, prelude, extend) in [
        (
            "require-self",
            "",
            format!("function extend(): void {{\n  require('./utils').default.merge = {stub};\n}}"),
        ),
        (
            "exports-default",
            "",
            format!("function extend(): void {{\n  exports.default.merge = {stub};\n}}"),
        ),
        (
            "self-import",
            "import self from './utils';\n",
            format!("function extend(): void {{\n  self.merge = {stub};\n}}"),
        ),
        (
            "dynamic-import",
            "",
            format!(
                "async function extend(): Promise<void> {{\n  (await import('./utils')).default.merge = {stub};\n}}"
            ),
        ),
        (
            "transitive-helper",
            "",
            format!(
                "function patch(): void {{\n  module.exports.default.merge = {stub};\n}}\n\nfunction extend(): void {{\n  patch();\n}}"
            ),
        ),
        // An imported member's body is invisible here (#13694 review).
        (
            "relative-imported-member",
            "import { extend } from './extend';\n",
            String::new(),
        ),
        (
            "package-imported-member",
            "import { extend } from 'lodash';\n",
            String::new(),
        ),
        // Bindings outside the modeled top-level declarations.
        (
            "import-equals-self",
            "import me = require('./utils');\n",
            format!("function extend(): void {{\n  me.default.merge = {stub};\n}}"),
        ),
        (
            "namespace-helper",
            "",
            format!(
                "namespace N {{\n  export function go(): void {{\n    require('./utils').default.merge = {stub};\n  }}\n}}\n\nfunction extend(): void {{\n  N.go();\n}}"
            ),
        ),
        (
            "block-hoisted-var",
            "",
            format!(
                "try {{\n  var h = require('./utils');\n}} catch {{}}\n\nfunction extend(): void {{\n  h.default.merge = {stub};\n}}"
            ),
        ),
        // A declared member bound to an opaque package import.
        (
            "package-import-alias",
            "import extendImpl from 'extend';\n",
            "const extend = extendImpl;".to_string(),
        ),
        (
            "package-import-destructured",
            "import $ from 'jquery';\n",
            "const { extend } = $;".to_string(),
        ),
        // A top-level reassignment the declaration scan does not see.
        (
            "destructured-reassignment",
            "",
            format!(
                "function extend(): void {{}}\nfunction bad(this: any): void {{\n  this.merge = {stub};\n}}\n({{ extend }} = {{ extend: bad }});"
            ),
        ),
        (
            "destructured-reassignment-in-init",
            "",
            format!(
                "function extend(): void {{}}\nfunction bad(this: any): void {{\n  this.merge = {stub};\n}}\nfunction init(): void {{\n  ({{ extend }} = {{ extend: bad }});\n}}\ninit();"
            ),
        ),
        (
            "reassigned-member",
            "",
            format!(
                "let extend = (): void => {{}};\nextend = function (this: any): void {{\n  this.merge = {stub};\n}};"
            ),
        ),
    ] {
        let owner = format!(
            "{prelude}{}",
            OWNER.replace(
                "export default { merge };",
                &format!("{extend}\n\nexport default {{ merge, extend }};"),
            )
        );
        assert_ne!(
            class_of(label, &owner, &test)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    // The unsafe sibling withholds only when the test calls it: a test that
    // calls `merge` alone keeps the single-member credit.
    let owner = OWNER.replace(
        "export default { merge };",
        &format!("function extend(): void {{\n  exports.default.merge = {stub};\n}}\n\nexport default {{ merge, extend }};"),
    );
    assert_eq!(
        class_of("unsafe-sibling-not-called", &owner, &test_with(CALL))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_member_name_declared_twice_credits_nothing() -> Result<(), String> {
    let owner = OWNER.replace(
        "export default { merge };",
        "var merge = (base: any, extra: any): any => ({ ...base, ...extra });\n\nexport default { merge };",
    );
    assert_ne!(
        class_of("duplicate-name", &owner, &test_with(CALL))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_second_binding_of_the_module_withholds_when_a_sibling_is_unsafe() -> Result<(), String> {
    // #13694 review / #13750: the unsafe sibling is reached through another
    // binding of the same module, outside the `utils.member(` calls.
    let unsafe_owner = OWNER.replace(
        "export default { merge };",
        "export function extend(): void {\n  exports.default.merge = () => ({ a: 1, b: 5, c: 4 });\n}\n\nexport default { merge, extend };",
    );
    let header = "import { it, expect, vi } from 'vitest';\nimport utils from '../src/utils';\n";
    for (label, binding, call) in [
        (
            "named-import",
            "import { extend } from '../src/utils';\n",
            "extend();",
        ),
        (
            "second-default",
            "import u2 from '../src/utils';\n",
            "u2.extend();",
        ),
        (
            "namespace",
            "import * as ns from '../src/utils';\n",
            "ns.extend();",
        ),
    ] {
        let test = format!("{header}{binding}\nit('merges', () => {{\n  {call}\n{CALL}\n}});\n");
        assert_ne!(
            class_of(label, &unsafe_owner, &test)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    // Control: with a `this`-free sibling the second binding is harmless.
    let safe_owner = OWNER.replace(
        "export default { merge };",
        "export function extend(): void {\n  return;\n}\n\nexport default { merge, extend };",
    );
    let test = format!(
        "{header}import {{ extend }} from '../src/utils';\n\nit('merges', () => {{\n  extend();\n{CALL}\n}});\n"
    );
    assert_eq!(
        class_of("named-import-safe-sibling", &safe_owner, &test)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn every_second_binding_route_withholds_beside_a_credited_control() -> Result<(), String> {
    // #13750: one negative per route, each beside a positive control whose
    // sibling is safe (same test shape, same binding, nothing reachable).
    let unsafe_owner = OWNER.replace(
        "export default { merge };",
        "export function extend(): void {\n  require('./utils').default.merge = () => ({ a: 1, b: 5, c: 4 });\n}\n\nexport default { merge, extend };",
    );
    let safe_owner = OWNER.replace(
        "export default { merge };",
        "export function extend(): void {\n  return;\n}\n\nexport default { merge, extend };",
    );
    let header = "import { it, expect, vi } from 'vitest';\nimport utils from '../src/utils';\n";
    for (label, binding, call) in [
        (
            "named-import",
            "import { extend } from '../src/utils';",
            "extend();",
        ),
        (
            "second-default-local",
            "import u2 from '../src/utils';",
            "u2.extend();",
        ),
        (
            "default-as",
            "import { default as u2 } from '../src/utils';",
            "u2.extend();",
        ),
        (
            "namespace-default",
            "import * as ns from '../src/utils';",
            "ns.default.extend();",
        ),
        (
            "require",
            "const m = require('../src/utils');",
            "m.default.extend();",
        ),
    ] {
        let test = format!("{header}{binding}\n\nit('merges', () => {{\n  {call}\n{CALL}\n}});\n");
        assert_ne!(
            class_of(&format!("{label}-unsafe"), &unsafe_owner, &test)?,
            ExposureClass::Exposed,
            "{label}"
        );
        // A `require()` of the owner module is not a credited shape even with
        // a safe sibling (it stays withheld, fail closed), so it has no control.
        if label != "require" {
            assert_eq!(
                class_of(&format!("{label}-safe"), &safe_owner, &test)?,
                ExposureClass::Exposed,
                "{label} control"
            );
        }
    }
    Ok(())
}

#[test]
fn a_dynamic_load_of_the_module_in_the_test_body_withholds() -> Result<(), String> {
    // #13750 review: no static import record exists for these bindings.
    let unsafe_owner = OWNER.replace(
        "export default { merge };",
        "export function extend(): void {\n  require('./utils').default.merge = () => ({ a: 1, b: 5, c: 4 });\n}\n\nexport default { merge, extend };",
    );
    for (label, load) in [
        (
            "dynamic-const",
            "const m = await import('../src/utils');\n  m.default.extend();",
        ),
        (
            "dynamic-inline",
            "(await import('../src/utils')).default.extend();",
        ),
        (
            "require-inline",
            "require('../src/utils').default.extend();",
        ),
        (
            "require-spaced",
            "require ('../src/utils').default.extend();",
        ),
        (
            "import-spaced",
            "(await import\n  ('../src/utils')).default.extend();",
        ),
        (
            "require-actual",
            "vi.importActual('../src/utils');\n  jest.requireActual('../src/utils').default.extend();",
        ),
    ] {
        let test = test_with(&format!("  {load}\n{CALL}")).replace("() => {", "async () => {");
        assert_ne!(
            class_of(&format!("{label}-unsafe"), &unsafe_owner, &test)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    // The whole file counts: a load in a hook, a describe body, or at file
    // level (outside the test body) withholds too.
    for (label, test) in [
        (
            "before-each",
            format!(
                "import {{ it, expect, beforeEach }} from 'vitest';\nimport utils from '../src/utils';\n\nbeforeEach(async () => {{\n  (await import('../src/utils')).default.extend();\n}});\n\nit('merges', () => {{\n{CALL}\n}});\n"
            ),
        ),
        (
            "describe-body",
            format!(
                "import {{ it, expect, describe }} from 'vitest';\nimport utils from '../src/utils';\n\ndescribe('d', () => {{\n  require('../src/utils').default.extend();\n  it('merges', () => {{\n{CALL}\n  }});\n}});\n"
            ),
        ),
        (
            "file-level",
            format!(
                "import {{ it, expect }} from 'vitest';\nimport utils from '../src/utils';\n\nconst m = await import('../src/utils');\nm.default.extend();\n\nit('merges', () => {{\n{CALL}\n}});\n"
            ),
        ),
        (
            "sibling-test",
            format!(
                "import {{ it, expect }} from 'vitest';\nimport utils from '../src/utils';\n\nit('other', async () => {{\n  (await import('../src/utils')).default.extend();\n}});\n\nit('merges', () => {{\n{CALL}\n}});\n"
            ),
        ),
    ] {
        assert_ne!(
            class_of(&format!("{label}-unsafe"), &unsafe_owner, &test)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    // Controls: the same test with no second load stays credited, and a load
    // of an unrelated module does not withhold.
    let plain = test_with(CALL);
    assert_eq!(
        class_of("no-load", &unsafe_owner, &plain)?,
        ExposureClass::Exposed
    );
    let other = test_with(&format!("  require('../src/other');\n{CALL}"));
    assert_eq!(
        class_of("other-module", &unsafe_owner, &other)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn node_process_globals_in_a_sibling_credit_nothing() -> Result<(), String> {
    // #13750 comment: `process`, `__filename` and `__dirname` reach the module
    // cache without naming anything else on the escape list.
    let test = test_with(&format!("  utils.extend();\n{CALL}"));
    for (label, body) in [
        (
            "process-main-module",
            "(process as any).mainModule.require('./utils').default.merge = () => ({ a: 1, b: 5, c: 4 });",
        ),
        ("filename", "void [__filename.length];"),
        ("dirname", "void [__dirname.length];"),
    ] {
        let owner = OWNER.replace(
            "export default { merge };",
            &format!(
                "function extend(): void {{\n  {body}\n}}\n\nexport default {{ merge, extend }};"
            ),
        );
        assert_ne!(
            class_of(label, &owner, &test)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    // Control: the same sibling shape without a Node global stays credited
    // (the control test does call `utils.extend()`, which is safe here).
    let owner = OWNER.replace(
        "export default { merge };",
        "function extend(): void {\n  void 0;\n}\n\nexport default { merge, extend };",
    );
    assert_eq!(
        class_of("no-node-global", &owner, &test)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn overload_signatures_do_not_count_as_a_second_declaration() -> Result<(), String> {
    // Bodiless overloads share the implementation's name (#13694 review).
    let owner = format!("function merge(base: object, extra: object): object;\n{OWNER}");
    assert_eq!(
        class_of("overloads", &owner, &test_with(CALL))?,
        ExposureClass::Exposed
    );
    Ok(())
}
