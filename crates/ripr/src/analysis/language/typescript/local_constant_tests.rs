//! Single-immutable-binding test constants (#10707, #10706): the credited
//! shapes and the shapes that must stay uncredited.

use super::local_constant::{BodyConstant, test_body_integer_constant, test_file_integer_constant};
use super::tests::{
    boundary_witness_owner, classify_boundary_line_for_owner, exact_value_test, ts_unique_tempdir,
    ts_write_file,
};
use super::*;

fn body(source: &str, name: &str) -> BodyConstant {
    test_body_integer_constant(Path::new("tests/pricing.test.ts"), source, name)
}

fn resolved(source: &str, name: &str) -> Option<String> {
    match body(source, name) {
        BodyConstant::Resolved(value) => Some(value),
        _ => None,
    }
}

fn is_refused(source: &str, name: &str) -> bool {
    matches!(body(source, name), BodyConstant::Refused)
}

fn is_undeclared(source: &str, name: &str) -> bool {
    matches!(body(source, name), BodyConstant::Undeclared)
}

fn call(inner: &str) -> String {
    format!("test('t', () => {{\n{inner}\n}});")
}

#[test]
fn single_const_in_callback_block_resolves() {
    for inner in [
        "const edge = 50;\nexpect(f(edge)).toBe(1);",
        "const edge = 50\nexpect(f(edge)).toBe(1)",
        "const edge: number = 50;\nexpect(f(edge)).toBe(1);",
        "const a = 1, edge = 50;\nexpect(f(edge)).toBe(1);",
        "const edge = 5_0;\nexpect(f(edge)).toBe(1);",
    ] {
        assert_eq!(
            resolved(&call(inner), "edge").as_deref(),
            Some("50"),
            "{inner}"
        );
    }
    // The statement-list shape and a function callback parse too.
    assert_eq!(
        resolved("const edge = 50;\nexpect(f(edge)).toBe(1);", "edge").as_deref(),
        Some("50")
    );
    assert_eq!(
        resolved(
            "test('t', async function () {\nconst edge = 50;\nexpect(f(edge)).toBe(1);\n});",
            "edge"
        )
        .as_deref(),
        Some("50")
    );
}

#[test]
fn rebound_shadowed_or_computed_local_never_resolves() {
    let refused = [
        "let edge = 50;\nexpect(f(edge)).toBe(1);",
        "var edge = 50;\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\nedge = 49;\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\nedge++;\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\nedge += 1;\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\n[1].forEach((edge) => expect(f(edge)).toBe(1));",
        "const edge = 50;\n[1].forEach(function (edge) { expect(f(edge)).toBe(1); });",
        "const edge = 50;\n[1].forEach(edge => expect(f(edge)).toBe(1));",
        "const edge = 50;\ntry { g(); } catch (edge) { expect(f(edge)).toBe(1); }",
        "const edge = 50;\n{ const edge = 60; expect(f(edge)).toBe(1); }",
        "const edge = 50;\nconst edge = 60;\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\nfunction inner(edge) { return edge; }\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\nconst { edge: other } = cfg;\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\nconst { edge } = cfg;\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\nconst cfg = { edge };\nexpect(f(edge)).toBe(1);",
        "const edge = 25 * 2;\nexpect(f(edge)).toBe(1);",
        "const edge = 50.5;\nexpect(f(edge)).toBe(1);",
        "const edge = -50;\nexpect(f(edge)).toBe(1);",
        "const edge = 0x32;\nexpect(f(edge)).toBe(1);",
        "const edge = 50 as number;\nexpect(f(edge)).toBe(1);",
        "const edge = compute();\nexpect(f(edge)).toBe(1);",
        "expect(f(edge)).toBe(1);\nconst edge = 50;",
        "if (cond) { const edge = 50; }\nexpect(f(edge)).toBe(1);",
        "for (const edge = 50; ;) { break; }\nexpect(f(edge)).toBe(1);",
        "const edge = 50;\neval('edge = 49');\nexpect(f(edge)).toBe(1);",
    ];
    for inner in refused {
        let source = call(inner);
        assert_eq!(resolved(&source, "edge"), None, "must not resolve: {inner}");
    }
}

#[test]
fn nested_only_or_unparsed_binding_is_refused_not_rescued_by_outer_scope() {
    assert!(is_refused(
        &call("if (cond) { const edge = 50; }\nexpect(f(edge)).toBe(1);"),
        "edge"
    ));
    assert!(is_refused(
        &call("let edge = 50;\nexpect(f(edge)).toBe(1);"),
        "edge"
    ));
    assert!(is_refused("this is not ( code edge const", "edge"));
}

#[test]
fn name_the_callback_only_reads_is_left_to_the_outer_scope() {
    assert!(is_undeclared(&call("expect(f(edge)).toBe(1);"), "edge"));
    assert!(is_undeclared(&call("expect(f(other)).toBe(1);"), "edge"));
    // A member of the same spelling is not the binding.
    assert!(is_undeclared(
        &call("expect(f(cfg.edge)).toBe(1);\nconst z = 1;"),
        "edge"
    ));
}

#[test]
fn file_level_constant_resolves_only_when_single_and_immutable() -> Result<(), String> {
    let dir = ts_unique_tempdir("const-file")?;
    let cases: [(&str, &str, Option<&str>); 9] = [
        (
            "const edge = 50;\ntest('t', () => { expect(f(edge)).toBe(1); });\n",
            "edge",
            Some("50"),
        ),
        (
            "export const EDGE = 50;\ntest('t', () => { expect(f(EDGE)).toBe(1); });\n",
            "EDGE",
            Some("50"),
        ),
        (
            "let edge = 50;\ntest('t', () => { expect(f(edge)).toBe(1); });\n",
            "edge",
            None,
        ),
        (
            "const edge = 50;\nedge2();\ntest('t', () => { const edge = 49; expect(f(edge)).toBe(1); });\n",
            "edge",
            None,
        ),
        (
            "const edge = 50;\ntest('t', () => { [1].forEach((edge) => expect(f(edge)).toBe(1)); });\n",
            "edge",
            None,
        ),
        (
            "const edge = 50;\nconst edge2 = 1;\ndescribe('d', () => { let edge = 49; test('t', () => { expect(f(edge)).toBe(1); }); });\n",
            "edge",
            None,
        ),
        (
            "const edge = 25 * 2;\ntest('t', () => { expect(f(edge)).toBe(1); });\n",
            "edge",
            None,
        ),
        (
            "import { edge } from './other';\ntest('t', () => { expect(f(edge)).toBe(1); });\n",
            "edge",
            None,
        ),
        (
            "const edge = 50;\nbeforeEach(() => { edge = 49; });\ntest('t', () => { expect(f(edge)).toBe(1); });\n",
            "edge",
            None,
        ),
    ];
    for (index, (source, name, expected)) in cases.iter().enumerate() {
        let relative = PathBuf::from(format!("tests/case{index}.test.ts"));
        ts_write_file(&dir.join(&relative), source)?;
        let mut test = exact_value_test("f", &format!("f({name})"), "1");
        test.file = relative;
        for binding in file_level_bindings(source, name) {
            test.scope_bindings = vec![binding].into();
        }
        let got = test_file_integer_constant(&test, name, Some(&dir));
        assert_eq!(got.as_deref(), *expected, "case {index}: {source}");
    }
    Ok(())
}

/// The file-level scope binding the extractor records for a top-level
/// declaration of `name` (empty when the file has none).
fn file_level_bindings(source: &str, name: &str) -> Vec<TypeScriptScopeBinding> {
    if source.contains(&format!("const {name} =")) || source.contains(&format!("let {name} =")) {
        vec![TypeScriptScopeBinding {
            name: name.to_string(),
            constructed_by: None,
            file_level: true,
            parameter_end: None,
        }]
    } else {
        Vec::new()
    }
}

#[test]
fn non_file_level_scope_binding_blocks_the_module_constant() -> Result<(), String> {
    let dir = ts_unique_tempdir("const-file-scope")?;
    let relative = PathBuf::from("tests/scope.test.ts");
    ts_write_file(
        &dir.join(&relative),
        "const edge = 50;\ntest('t', () => { expect(f(edge)).toBe(1); });\n",
    )?;
    let mut test = exact_value_test("f", "f(edge)", "1");
    test.file = relative;
    let binding = |file_level| TypeScriptScopeBinding {
        name: "edge".to_string(),
        constructed_by: None,
        file_level,
        parameter_end: None,
    };
    test.scope_bindings = vec![binding(true)].into();
    assert_eq!(
        test_file_integer_constant(&test, "edge", Some(&dir)).as_deref(),
        Some("50")
    );
    test.scope_bindings = vec![binding(true), binding(false)].into();
    assert_eq!(test_file_integer_constant(&test, "edge", Some(&dir)), None);
    // No workspace root: nothing to read, nothing credited.
    test.scope_bindings = vec![binding(true)].into();
    assert_eq!(test_file_integer_constant(&test, "edge", None), None);
    Ok(())
}

fn classify_with_body(body: &str, observed: &str) -> Result<ExposureClass, String> {
    let owner = boundary_witness_owner();
    let mut test = exact_value_test("applyDiscount", observed, "90");
    test.body_text = body.to_string();
    Ok(classify_boundary_line_for_owner(&owner, "  if (total >= 100) {", &[test])?.class)
}

#[test]
fn lowercase_local_constant_witnesses_the_boundary() -> Result<(), String> {
    let class = classify_with_body(
        "test('t', () => {\n  const edge = 100;\n  expect(applyDiscount(edge)).toBe(90);\n});",
        "applyDiscount(edge)",
    )?;
    assert_eq!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn local_constant_that_is_rebound_or_off_value_does_not_witness() -> Result<(), String> {
    for body in [
        "test('t', () => {\n  let edge = 100;\n  expect(applyDiscount(edge)).toBe(90);\n});",
        "test('t', () => {\n  const edge = 100;\n  [1].forEach((edge) => expect(applyDiscount(edge)).toBe(90));\n});",
        "test('t', () => {\n  const edge = 150;\n  expect(applyDiscount(edge)).toBe(90);\n});",
        "test('t', () => {\n  const edge = 50 * 2;\n  expect(applyDiscount(edge)).toBe(90);\n});",
        "test('t', () => {\n  const cfg = { edge: 100 };\n  cfg.edge = 50;\n  expect(applyDiscount(cfg.edge)).toBe(90);\n});",
    ] {
        let observed = if body.contains("cfg.edge)") {
            "applyDiscount(cfg.edge)"
        } else {
            "applyDiscount(edge)"
        };
        assert_eq!(
            classify_with_body(body, observed)?,
            ExposureClass::WeaklyExposed,
            "{body}"
        );
    }
    Ok(())
}
