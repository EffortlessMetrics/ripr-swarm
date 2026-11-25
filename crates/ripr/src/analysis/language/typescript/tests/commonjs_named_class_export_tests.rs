//! A class expression assigned to a CommonJS named export (#16275,
//! RIPR-SPEC-0243 rule 27).
//!
//! `exports.Cart = class { ... }` and `module.exports.Cart = class ...` are
//! the inline twins of `class Cart {}` followed by `exports.Cart = Cart;`.
//! Before #16275 the class's methods were no owners at all, so a changed
//! method produced no finding and the run reported no behavioral candidate.
//! The members are now owners named by the export property (`Cart`), whatever
//! the expression's own id says: that id binds only inside the class body.
//!
//! * A named `require` of the property relates a changed method; a test that
//!   destructures the inner id (`{ Basket }`) or another export stays
//!   unrelated.
//! * The members of one class share the one export slot `Cart`. When a later
//!   top-level statement assigns `Cart` again (another class, a class with no
//!   method, or any value) the earlier class is dead and yields no owner, the
//!   same fail-closed rule as a reassigned function export (#4638); a
//!   reassigned function export now also counts value assignments that are
//!   not functions.
//! * Instance fields of the class and a declared class of the same name read
//!   under the property name, so field shadowing and repeated-name guards
//!   see them.
//!
//! The end-to-end cases run `TypeScriptAdapter::analyze_diff` on a generated
//! jest repository and read the finding on the changed line.

use super::*;

const JEST_PACKAGE: &str =
    "{\"name\":\"demo\",\"version\":\"1.0.0\",\"devDependencies\":{\"jest\":\"^29.0.0\"}}\n";

/// A class body whose `if` line is the changed line.
const BODY: &str =
    "{\n  total(n) {\n    if (n > 50) {\n      return n - 5;\n    }\n    return n;\n  }\n}";
const CHANGED: &str = "    if (n > 50) {";
const OTHER_BODY: &str = "{\n  total(n) {\n    return n - 5;\n  }\n}";

const NAMED_REQUIRE: &str = "const { Cart } = require('../src/cart');";
const CALL: &str = "    expect(new Cart().total(50)).toBe(45);\n";

/// The findings `analyze_diff` reports on the line of `src/cart.js` that
/// reads `CHANGED`, with `test` as `tests/cart.test.js`.
fn diff_findings(label: &str, owner_text: &str, test: &str) -> Result<Vec<Finding>, String> {
    let line = owner_text
        .lines()
        .position(|text| text == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("package.json"), JEST_PACKAGE)?;
    ts_write_file(&root.join("src/cart.js"), owner_text)?;
    ts_write_file(&root.join("tests/cart.test.js"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.js", &[(line, CHANGED)])],
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(result?
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect())
}

fn diff_finding(label: &str, owner_text: &str, test: &str) -> Result<Finding, String> {
    diff_findings(label, owner_text, test)?
        .into_iter()
        .next()
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

#[test]
fn a_named_export_class_expression_yields_owners_named_by_the_property() -> Result<(), String> {
    for (label, source) in [
        ("exports", format!("exports.Cart = class Basket {BODY};\n")),
        (
            "module.exports",
            format!("module.exports.Cart = class {BODY};\n"),
        ),
    ] {
        let owners = extract_owners(Path::new("src/cart.js"), &source);
        let [owner] = owners.as_slice() else {
            return Err(format!("{label}: expected one owner, got {owners:?}"));
        };
        if owner.name != "total"
            || owner.class_name.as_deref() != Some("Cart")
            || owner.owner_kind != OwnerKind::Method
            || owner.class_default_export
            || owner.commonjs_default_export
        {
            return Err(format!("{label}: unexpected owner {owner:?}"));
        }
    }
    Ok(())
}

#[test]
fn a_changed_method_of_a_named_export_class_relates_to_a_named_require() -> Result<(), String> {
    for (label, source) in [
        (
            "named-class-exports",
            format!("exports.Cart = class {BODY};\n"),
        ),
        (
            "named-class-module-exports",
            format!("module.exports.Cart = class {BODY};\n"),
        ),
        (
            "named-class-inner-id",
            format!("exports.Cart = class Basket {BODY};\n"),
        ),
    ] {
        let finding = diff_finding(label, &source, &jest_test(NAMED_REQUIRE, CALL))?;
        expect_related(label, &finding, true)?;
    }
    Ok(())
}

#[test]
fn the_inner_class_id_and_a_sibling_export_stay_unrelated() -> Result<(), String> {
    let inner = diff_finding(
        "named-class-inner-import",
        &format!("exports.Cart = class Basket {BODY};\n"),
        &jest_test(
            "const { Basket } = require('../src/cart');",
            "    expect(new Basket().total(50)).toBe(45);\n",
        ),
    )?;
    expect_related("inner id import", &inner, false)?;
    let sibling = diff_finding(
        "named-class-sibling",
        &format!("exports.Cart = class {BODY};\nexports.Other = class {OTHER_BODY};\n"),
        &jest_test(
            "const { Other } = require('../src/cart');",
            "    expect(new Other().total(50)).toBe(45);\n",
        ),
    )?;
    expect_related("sibling export", &sibling, false)
}

#[test]
fn two_named_export_classes_with_one_method_name_keep_their_owners() -> Result<(), String> {
    let finding = diff_finding(
        "named-class-two-classes",
        &format!("exports.Cart = class {BODY};\nexports.Other = class {OTHER_BODY};\n"),
        &jest_test(NAMED_REQUIRE, CALL),
    )?;
    expect_related("two classes", &finding, true)
}

#[test]
fn a_reassigned_named_export_class_yields_no_owner() -> Result<(), String> {
    for (label, later) in [
        (
            "named-class-then-class",
            format!("exports.Cart = class {OTHER_BODY};\n"),
        ),
        (
            "named-class-then-empty",
            "exports.Cart = class {};\n".to_string(),
        ),
        (
            "named-class-then-null",
            "module.exports.Cart = null;\n".to_string(),
        ),
    ] {
        let source = format!("exports.Cart = class {BODY};\n{later}");
        let owners = extract_owners(Path::new("src/cart.js"), &source);
        if owners
            .iter()
            .any(|owner| owner.start_line == 2 && owner.name == "total")
        {
            return Err(format!("{label}: the dead class kept an owner {owners:?}"));
        }
        let findings = diff_findings(label, &source, &jest_test(NAMED_REQUIRE, CALL))?;
        if findings
            .iter()
            .any(|finding| !finding.related_tests.is_empty())
        {
            return Err(format!(
                "{label}: a dead class body was related {findings:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_function_export_reassigned_to_a_value_yields_no_owner() -> Result<(), String> {
    let live = "exports.total = function (n) {\n  return n;\n};\n";
    let owners = extract_owners(Path::new("src/cart.js"), live);
    if !owners.iter().any(|owner| owner.name == "total") {
        return Err(format!(
            "control: the live function export has no owner {owners:?}"
        ));
    }
    let dead = format!("{live}exports.total = null;\n");
    let owners = extract_owners(Path::new("src/cart.js"), &dead);
    if owners.iter().any(|owner| owner.name == "total") {
        return Err(format!(
            "the reassigned function export kept an owner {owners:?}"
        ));
    }
    Ok(())
}

#[test]
fn a_named_export_class_reads_under_its_property_for_fields_and_repeats() -> Result<(), String> {
    let source = "class Cart {\n  total() {}\n}\nexports.Cart = class Basket {\n  total = () => 0;\n  total() {}\n};\nexports.Order = class Order {};\n";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let repeated = repeated_class_names(&parsed.program);
    if !repeated.contains("Cart") || repeated.contains("Basket") || repeated.contains("Order") {
        return Err(format!("unexpected repeated class names {repeated:?}"));
    }
    let fields = class_instance_field_names(&parsed.program);
    if !fields
        .get("Cart")
        .is_some_and(|names| names.contains("total"))
    {
        return Err(format!(
            "the export's field is not keyed by Cart: {fields:?}"
        ));
    }
    if fields.contains_key("Basket") {
        return Err(format!("the inner id keyed fields: {fields:?}"));
    }
    Ok(())
}
