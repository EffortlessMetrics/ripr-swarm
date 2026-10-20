//! RIPR-SPEC-0243 rule 14: a bare `describe` / `test` registration whose callee
//! the file declares as its own function is that function's call, so its
//! callback donates no assertions (#8959). Each control is compared with the
//! genuine registration of the same body and must read `NoStaticPath`
//! exactly, not merely differ from the credited class.

use super::*;

const OWNER: &str = "export function sumAll(values: number[]): number {\n  let total = 0;\n  for (const value of values) {\n    total += value;\n  }\n  return total;\n}\n";
const CHANGED_LINE: (usize, &str) = (4, "    total += value;");
const IMPORTS: &str = "import { sumAll } from '../src/sum';\n";
const SUITE_BODY: &str =
    "  test('adds every value', () => {\n    expect(sumAll([2, 3, 4])).toBe(9);\n  });\n";

fn class_of(label: &str, test_source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/sum.ts"), OWNER)?;
    ts_write_file(&root.join("tests/sum.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/sum.ts", &[CHANGED_LINE])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    Ok(result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("::sumAll"))
        })
        .map_or(ExposureClass::NoStaticPath, |finding| finding.class))
}

fn genuine_class() -> Result<ExposureClass, String> {
    let class = class_of(
        "genuine",
        &format!(
            "import {{ describe, test, expect }} from 'vitest';\n{IMPORTS}\ndescribe('sum', () => {{\n{SUITE_BODY}}});\n"
        ),
    )?;
    assert_ne!(class, ExposureClass::NoStaticPath, "genuine registration");
    Ok(class)
}

#[test]
fn a_locally_declared_registration_function_donates_no_assertions() -> Result<(), String> {
    genuine_class()?;
    let header = "import { expect } from 'vitest';\n";
    for (label, source) in [
        (
            "local-noop-describe",
            format!(
                "{header}{IMPORTS}\nconst describe = (_name: string, _body: () => void) => {{}};\ndescribe('sum', () => {{\n{SUITE_BODY}}});\n"
            ),
        ),
        (
            "local-noop-describe-declaration",
            format!(
                "{header}{IMPORTS}\nfunction describe(_name: string, _body: () => void) {{}}\ndescribe('sum', () => {{\n{SUITE_BODY}}});\n"
            ),
        ),
        (
            "local-noop-test-at-top",
            format!(
                "{header}{IMPORTS}\nconst test = (_name: string, _body: () => void) => {{}};\ntest('adds every value', () => {{\n  expect(sumAll([2, 3, 4])).toBe(9);\n}});\n"
            ),
        ),
        (
            "local-noop-test-in-genuine-describe",
            format!(
                "import {{ describe, expect }} from 'vitest';\n{IMPORTS}\ndescribe('sum', () => {{\n  const test = (_name: string, _body: () => void) => {{}};\n{SUITE_BODY}}});\n"
            ),
        ),
        (
            "local-noop-it-alias-function-expression",
            format!(
                "{header}{IMPORTS}\nconst it = function (_name: string, _body: () => void) {{}};\nit('adds every value', () => {{\n  expect(sumAll([2, 3, 4])).toBe(9);\n}});\n"
            ),
        ),
        (
            "exported-noop-describe",
            format!(
                "{header}{IMPORTS}\nexport const describe = (_name: string, _body: () => void) => {{}};\ndescribe('sum', () => {{\n{SUITE_BODY}}});\n"
            ),
        ),
        (
            "exported-noop-describe-declaration",
            format!(
                "{header}{IMPORTS}\nexport function describe(_name: string, _body: () => void) {{}}\ndescribe('sum', () => {{\n{SUITE_BODY}}});\n"
            ),
        ),
        (
            "asserted-noop-describe",
            format!(
                "{header}{IMPORTS}\nconst describe = ((_name: string, _body: () => void) => {{}}) as typeof globalThis.describe;\ndescribe('sum', () => {{\n{SUITE_BODY}}});\n"
            ),
        ),
        (
            "noop-describe-only-form",
            format!(
                "{header}{IMPORTS}\nfunction describe(_name: string, _body: () => void) {{}}\ndescribe.only('sum', () => {{\n{SUITE_BODY}}});\n"
            ),
        ),
        (
            "noop-describe-each-form",
            format!(
                "{header}{IMPORTS}\nfunction describe(_name: string, _body: () => void) {{}}\ndescribe.each([1])('sum', () => {{\n{SUITE_BODY}}});\n"
            ),
        ),
    ] {
        assert_eq!(
            class_of(label, &source)?,
            ExposureClass::NoStaticPath,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn an_owner_assertion_only_in_an_uncalled_closure_credits_nothing() -> Result<(), String> {
    genuine_class()?;
    let source = format!(
        "import {{ describe, expect }} from 'vitest';\n{IMPORTS}\ndescribe('sum', () => {{\n  function check() {{\n    expect(sumAll([2, 3, 4])).toBe(9);\n  }}\n}});\n"
    );
    assert_eq!(
        class_of("uncalled-closure", &source)?,
        ExposureClass::NoStaticPath
    );
    Ok(())
}

#[test]
fn a_registration_helper_the_file_does_not_redeclare_keeps_its_credit() -> Result<(), String> {
    // A sibling function with another name leaves `test` and `describe` the
    // runner's, so the genuine reading must survive.
    let genuine = genuine_class()?;
    let source = format!(
        "import {{ describe, test, expect }} from 'vitest';\n{IMPORTS}\nfunction helper() {{}}\nconst unrelated = () => {{}};\ndescribe('sum', () => {{\n{SUITE_BODY}}});\n"
    );
    assert_eq!(class_of("sibling-functions", &source)?, genuine);
    Ok(())
}

#[test]
fn a_registration_name_bound_to_a_non_function_keeps_its_credit() -> Result<(), String> {
    // Only a declared function is a shadow; a name bound to a call result
    // keeps the name-based reading, and this also shows the fixtures above
    // parse and score, so their `NoStaticPath` is the gate's doing.
    let genuine = genuine_class()?;
    let source = format!(
        "import {{ expect }} from 'vitest';\nimport {{ makeDescribe }} from './helpers';\n{IMPORTS}\nconst describe = makeDescribe();\ndescribe('sum', () => {{\n{SUITE_BODY}}});\n"
    );
    assert_eq!(class_of("call-result", &source)?, genuine);
    Ok(())
}
