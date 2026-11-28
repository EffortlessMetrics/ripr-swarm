//! A named import of a class its module does not export under that name
//! (#16493).
//!
//! `class Cart {}` with only `exports.Basket = Cart` leaves `const { Cart } =
//! require(...)` bound to `undefined`, so `new Cart()` throws and the test
//! cannot tell any edit of `Cart.total` apart. Such a test is not related to
//! the class. A module whose exports name the class, and every module whose
//! export facts are not fully known, keep the relation.
//!
//! Every case runs `TypeScriptAdapter::analyze_diff` over a real Jest
//! workspace, so the import pass runs exactly as in production.

use super::*;

const CHANGED: &str = "    if (n > 50) {";
const CLASS: &str = "class Cart {\n  total(n) {\n    if (n > 50) {\n      return n - 5;\n    }\n    return n;\n  }\n}\n";
const PACKAGE: &str =
    "{\"name\":\"demo\",\"version\":\"1.0.0\",\"devDependencies\":{\"jest\":\"^29.0.0\"}}\n";

fn test_file(binding: &str, class: &str) -> String {
    format!(
        "{binding}\n\ndescribe('Cart', () => {{\n  test('it', () => {{\n    expect(new {class}().total(50)).toBe(45);\n  }});\n}});\n"
    )
}

/// The finding for the changed `total` boundary of `src/cart.js`, whose
/// source is `CLASS` followed by `exports`.
fn finding(label: &str, exports: &str, test: &str) -> Result<Finding, String> {
    let owner = format!("{CLASS}{exports}");
    let line = owner
        .lines()
        .position(|line| line == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("package.json"), PACKAGE)?;
    ts_write_file(&root.join("src/cart.js"), &owner)?;
    ts_write_file(&root.join("tests/cart.test.js"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.js", &[(line, CHANGED)])],
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
                .is_some_and(|owner| owner.0.ends_with("::total"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `total`"))
}

#[test]
fn a_require_of_a_class_name_the_module_does_not_export_is_not_related() -> Result<(), String> {
    for (label, exports) in [
        ("exports-other-name", "exports.Basket = Cart;\n"),
        ("object-other-name", "module.exports = { Basket: Cart };\n"),
    ] {
        let finding = finding(
            label,
            exports,
            &test_file("const { Cart } = require('../src/cart');", "Cart"),
        )?;
        assert!(
            finding.related_tests.is_empty(),
            "{label}: `Cart` is undefined in the test, related: {:?}",
            finding.related_tests
        );
        assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    }
    Ok(())
}

#[test]
fn a_require_of_an_exported_class_name_stays_related() -> Result<(), String> {
    for (label, exports) in [
        ("exports-dot", "exports.Cart = Cart;\n"),
        ("module-exports-dot", "module.exports.Cart = Cart;\n"),
        ("object-shorthand", "module.exports = { Cart };\n"),
    ] {
        let finding = finding(
            label,
            exports,
            &test_file("const { Cart } = require('../src/cart');", "Cart"),
        )?;
        assert_eq!(
            finding.related_tests.len(),
            1,
            "{label}: the test is related"
        );
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
    }
    let whole = finding(
        "whole-export",
        "module.exports = Cart;\n",
        &test_file("const Cart = require('../src/cart');", "Cart"),
    )?;
    assert_eq!(
        whole.related_tests.len(),
        1,
        "whole export: the test is related"
    );
    assert_eq!(whole.class, ExposureClass::Exposed, "whole export");
    Ok(())
}

/// When the module's own exports cannot be read in full, its absence of
/// `Cart` is not established, so the relation stays as it was.
#[test]
fn a_module_with_unreadable_exports_keeps_the_relation() -> Result<(), String> {
    for (label, exports) in [
        (
            "computed-key",
            "const key = 'Cart';\nexports[key] = Cart;\n",
        ),
        (
            "object-assign",
            "Object.assign(module.exports, { Cart });\n",
        ),
        ("no-export-facts", ""),
    ] {
        let finding = finding(
            label,
            exports,
            &test_file("const { Cart } = require('../src/cart');", "Cart"),
        )?;
        assert_eq!(
            finding.related_tests.len(),
            1,
            "{label}: an unproven absence keeps the relation"
        );
    }
    Ok(())
}
