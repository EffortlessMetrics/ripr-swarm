//! A class method that a field of the same key shadows (#16306,
//! RIPR-SPEC-0243 rule 28).
//!
//! `class Cart { total = (n) => n; total(n) {...} }` defines `total` on every
//! instance when it is constructed, after the prototype holds the method, so
//! `new Cart().total()` calls the field and never the method body. Before
//! #16306 the method was related to that test. Now a change to the method is
//! still reported but relates to no test, and its guidance names the field.
//! Source order does not matter, a bare `total;` field shadows too, and a
//! static field shadows a static method.
//!
//! Controls that keep their relation: a field of another name, a static field
//! beside an instance method, a `#private` field beside a public method, a
//! TypeScript `declare` field (no runtime definition), and a field initialized
//! from the method itself (`total = this.total.bind(this)`), which captures
//! the method before the field is installed.
//!
//! Each test body asserts what the code does at runtime: a shadowed case
//! asserts the field's value (or the throw of calling a bare field), a control
//! asserts the method's value.
//!
//! Every case runs `TypeScriptAdapter::analyze_diff` on a generated jest
//! repository and reads the finding on the changed line.

use super::*;

const JEST_PACKAGE: &str =
    "{\"name\":\"demo\",\"version\":\"1.0.0\",\"devDependencies\":{\"jest\":\"^29.0.0\"}}\n";

/// The method whose `if` line is the changed line.
const METHOD: &str =
    "  total(n) {\n    if (n > 50) {\n      return n - 5;\n    }\n    return n;\n  }\n";
const CHANGED: &str = "    if (n > 50) {";

const NAMED_REQUIRE: &str = "const { Cart } = require('../src/cart');";
/// Calls the method: the body subtracts 5 above 50.
const CALL: &str = "    expect(new Cart().total(100)).toBe(95);\n";
/// Calls the field `total = (n) => n`, which returns its input.
const FIELD_CALL: &str = "    expect(new Cart().total(100)).toBe(100);\n";
/// Calls a bare `total;` field, which holds `undefined`.
const BARE_CALL: &str = "    expect(() => new Cart().total(100)).toThrow(TypeError);\n";

/// The finding `analyze_diff` reports on the line of `owner_path` that reads
/// `CHANGED`, with `test` as the test file at `test_path`.
fn diff_finding(
    label: &str,
    owner_path: &str,
    owner_text: &str,
    test_path: &str,
    test: &str,
) -> Result<Finding, String> {
    let line = owner_text
        .lines()
        .position(|text| text == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("package.json"), JEST_PACKAGE)?;
    ts_write_file(&root.join(owner_path), owner_text)?;
    ts_write_file(&root.join(test_path), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(owner_path, &[(line, CHANGED)])],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: the changed line yields no finding"))
}

fn jest_test(preamble: &str, body: &str) -> String {
    format!("{preamble}\n\ndescribe('Cart', () => {{\n  test('it', () => {{\n{body}  }});\n}});\n")
}

fn expect_related(label: &str, finding: &Finding, related: bool) -> Result<(), String> {
    if finding.related_tests.is_empty() == related {
        return Err(format!(
            "{label}: expected {} related test, got {:?}",
            if related { "a" } else { "no" },
            finding.related_tests
        ));
    }
    Ok(())
}

/// `class Cart { <members> }` exported as a named CommonJS export.
fn commonjs_class(members: &str) -> String {
    format!("class Cart {{\n{members}}}\nmodule.exports = {{ Cart }};\n")
}

fn js_finding(label: &str, owner_text: &str, body: &str) -> Result<Finding, String> {
    diff_finding(
        label,
        "src/cart.js",
        owner_text,
        "tests/cart.test.js",
        &jest_test(NAMED_REQUIRE, body),
    )
}

#[test]
fn a_method_shadowed_by_an_instance_field_relates_to_no_test() -> Result<(), String> {
    for (label, members, body) in [
        (
            "field-before",
            format!("  total = (n) => n;\n{METHOD}"),
            FIELD_CALL,
        ),
        (
            "field-after",
            format!("{METHOD}  total = (n) => n;\n"),
            FIELD_CALL,
        ),
        ("bare-field", format!("  total;\n{METHOD}"), BARE_CALL),
        (
            "quoted-field",
            format!("  'total' = (n) => n;\n{METHOD}"),
            FIELD_CALL,
        ),
        (
            "bound-other-method",
            format!("  total = this.other.bind(this);\n  other(n) {{ return n; }}\n{METHOD}"),
            FIELD_CALL,
        ),
    ] {
        let finding = js_finding(label, &commonjs_class(&members), body)?;
        expect_related(label, &finding, false)?;
    }
    Ok(())
}

#[test]
fn an_es_exported_class_method_shadowed_by_a_field_relates_to_no_test() -> Result<(), String> {
    let finding = diff_finding(
        "field-esm",
        "src/cart.js",
        &format!("export class Cart {{\n  total = (n) => n;\n{METHOD}}}\n"),
        "tests/cart.test.js",
        &jest_test("import { Cart } from '../src/cart';", FIELD_CALL),
    )?;
    expect_related("ES export", &finding, false)
}

#[test]
fn a_static_method_shadowed_by_a_static_field_relates_to_no_test() -> Result<(), String> {
    let members = format!(
        "  static total = (n) => n;\n{}",
        METHOD.replace("  total(n)", "  static total(n)")
    );
    let finding = js_finding(
        "static-field",
        &commonjs_class(&members),
        "    expect(Cart.total(100)).toBe(100);\n",
    )?;
    expect_related("static field", &finding, false)
}

#[test]
fn fields_that_do_not_shadow_keep_the_method_relation() -> Result<(), String> {
    for (label, members) in [
        ("no-field", METHOD.to_string()),
        ("other-field", format!("  items = [];\n{METHOD}")),
        (
            "static-field-instance-method",
            format!("  static total = 0;\n{METHOD}"),
        ),
        ("private-field", format!("  #total = 0;\n{METHOD}")),
    ] {
        let finding = js_finding(label, &commonjs_class(&members), CALL)?;
        expect_related(label, &finding, true)?;
    }
    Ok(())
}

#[test]
fn a_field_bound_from_the_method_itself_keeps_the_method_relation() -> Result<(), String> {
    for (label, members, body) in [
        (
            "self-bind",
            format!("  total = this.total.bind(this);\n{METHOD}"),
            CALL,
        ),
        (
            "self-read",
            format!("  total = this.total;\n{METHOD}"),
            CALL,
        ),
        (
            "static-self-bind",
            format!(
                "  static total = this.total.bind(this);\n{}",
                METHOD.replace("  total(n)", "  static total(n)")
            ),
            "    expect(Cart.total(100)).toBe(95);\n",
        ),
    ] {
        let finding = js_finding(label, &commonjs_class(&members), body)?;
        expect_related(label, &finding, true)?;
    }
    Ok(())
}

#[test]
fn a_declare_field_defines_nothing_and_keeps_the_method_relation() -> Result<(), String> {
    let method = METHOD.replace("total(n) {", "total(n: number): number {");
    let finding = diff_finding(
        "declare-field",
        "src/cart.ts",
        &format!("export class Cart {{\n  declare total: (n: number) => number;\n{method}}}\n"),
        "tests/cart.test.ts",
        &jest_test("import { Cart } from '../src/cart';", CALL),
    )?;
    expect_related("declare field", &finding, true)
}

#[test]
fn a_shadowed_method_is_an_owner_marked_field_shadowed() -> Result<(), String> {
    let owners = extract_owners(
        Path::new("src/cart.js"),
        &commonjs_class(&format!("  total = (n) => n;\n{METHOD}")),
    );
    let [owner] = owners.as_slice() else {
        return Err(format!("expected one owner, got {owners:?}"));
    };
    if owner.name != "total" || owner.method_kind != TypeScriptMethodKind::FieldShadowed {
        return Err(format!("unexpected owner {owner:?}"));
    }
    Ok(())
}

#[test]
fn a_shadowed_method_points_at_the_field() -> Result<(), String> {
    let finding = js_finding(
        "field-guidance",
        &commonjs_class(&format!("  total = (n) => n;\n{METHOD}")),
        FIELD_CALL,
    )?;
    expect_field_guidance(&finding)
}

/// A test that also imports an unrelated package symbol named like the method
/// does not turn the field's guidance into an unresolved-alias disclosure.
#[test]
fn a_same_named_package_import_keeps_the_field_guidance() -> Result<(), String> {
    let finding = diff_finding(
        "field-guidance-package-import",
        "src/cart.js",
        &format!("export class Cart {{\n  total = (n) => n;\n{METHOD}}}\n"),
        "tests/cart.test.js",
        &jest_test(
            "import { Cart } from '../src/cart';\nimport { total } from 'lodash-es';",
            &format!("{FIELD_CALL}    expect(total).toBeDefined();\n"),
        ),
    )?;
    expect_field_guidance(&finding)
}

fn expect_field_guidance(finding: &Finding) -> Result<(), String> {
    if finding.class != ExposureClass::NoStaticPath {
        return Err(format!("expected no_static_path, got {:?}", finding.class));
    }
    if !finding
        .missing
        .iter()
        .any(|line| line.contains("also named `total` and shadows this body"))
    {
        return Err(format!("missing names no field: {:?}", finding.missing));
    }
    let step = finding.recommended_next_step.as_deref().unwrap_or_default();
    if !step.contains("the field `total` of this class shadows the method")
        || step.contains("new ClassName")
    {
        return Err(format!("recommendation does not name the field: {step:?}"));
    }
    Ok(())
}
