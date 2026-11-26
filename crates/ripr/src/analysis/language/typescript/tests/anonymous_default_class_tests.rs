//! Methods of an anonymous default-exported class (#16349, RIPR-SPEC-0243
//! rule 19 as amended).
//!
//! `export default class { ... }` and `module.exports = class { ... }` are
//! the module's default (or whole) export, reached through a default import
//! or a whole-module `require` under any local name, exactly as their named
//! twins are. Their members carry the class name `default` (the ES class's
//! runtime name; a reserved word no same-file construction can spell), so
//! the class-method relation credits that binding.
//!
//! Controls that stay unrelated: a named import or named `require` of the
//! module, a default-named `require` of the CommonJS form, and the CommonJS
//! form with a named export beside the class. Each test body asserts the
//! method's runtime value.

use super::*;

const JEST_PACKAGE: &str =
    "{\"name\":\"demo\",\"version\":\"1.0.0\",\"devDependencies\":{\"jest\":\"^29.0.0\"}}\n";

/// The class body whose `if` line is the changed line.
const BODY: &str =
    "{\n  total(n) {\n    if (n > 50) {\n      return n - 5;\n    }\n    return n;\n  }\n}";
const CHANGED: &str = "    if (n > 50) {";
const CALL: &str = "    expect(new Cart().total(100)).toBe(95);\n";

const ES_DEFAULT: &str = "import Cart from '../src/cart';";
const REQUIRE: &str = "const Cart = require('../src/cart');";

fn es_anonymous() -> String {
    format!("export default class {BODY}\n")
}

fn commonjs_anonymous(after: &str) -> String {
    format!("module.exports = class {BODY};\n{after}")
}

fn jest_test(preamble: &str, body: &str) -> String {
    format!("{preamble}\n\ndescribe('Cart', () => {{\n  test('it', () => {{\n{body}  }});\n}});\n")
}

/// The finding `analyze_diff` reports on the changed line of `src/cart.js`,
/// with `test` as the jest test file.
fn diff_finding(label: &str, owner_text: &str, test: &str) -> Result<Finding, String> {
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
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: the changed line yields no finding"))
}

fn case(label: &str, owner_text: &str, test: &str, related: bool) -> Result<(), String> {
    let finding = diff_finding(label, owner_text, test)?;
    if finding.related_tests.is_empty() == related {
        return Err(format!(
            "{label}: expected {} related test, got {:?} ({:?})",
            if related { "a" } else { "no" },
            finding.related_tests,
            finding.class
        ));
    }
    Ok(())
}

#[test]
fn an_anonymous_default_class_method_relates_through_the_default_binding() -> Result<(), String> {
    case(
        "es-default-import",
        &es_anonymous(),
        &jest_test(ES_DEFAULT, CALL),
        true,
    )?;
    case(
        "es-default-other-local",
        &es_anonymous(),
        &jest_test(
            "import Basket from '../src/cart';",
            &CALL.replace("Cart", "Basket"),
        ),
        true,
    )?;
    case(
        "es-default-extends",
        &format!("import Base from './base';\nexport default class extends Base {BODY}\n"),
        &jest_test(ES_DEFAULT, CALL),
        true,
    )?;
    case(
        "commonjs-whole-require",
        &commonjs_anonymous(""),
        &jest_test(REQUIRE, CALL),
        true,
    )
}

#[test]
fn other_bindings_of_an_anonymous_default_class_stay_unrelated() -> Result<(), String> {
    for (label, owner_text, preamble) in [
        (
            "es-named-import",
            es_anonymous(),
            "import { Cart } from '../src/cart';",
        ),
        (
            "commonjs-named-require",
            commonjs_anonymous(""),
            "const { Cart } = require('../src/cart');",
        ),
        (
            "commonjs-default-named-require",
            commonjs_anonymous(""),
            "const { default: Cart } = require('../src/cart');",
        ),
        (
            "commonjs-named-export-beside",
            commonjs_anonymous("module.exports.helper = () => 1;\n"),
            REQUIRE,
        ),
    ] {
        case(label, &owner_text, &jest_test(preamble, CALL), false)?;
    }
    Ok(())
}

/// The `default` name does not lift rule 29: a method the constructor
/// replaces (`this.total = (n) => n;`) stays unrelated through the very
/// default binding that relates the ordinary method above.
#[test]
fn a_constructor_replaced_method_of_an_anonymous_default_class_stays_unrelated()
-> Result<(), String> {
    let replaced = BODY.replacen(
        "{\n  total(n)",
        "{\n  constructor() {\n    this.total = (n) => n;\n  }\n  total(n)",
        1,
    );
    let assigned_call = "    expect(new Cart().total(100)).toBe(100);\n";
    case(
        "es-constructor-replaced",
        &format!("export default class {replaced}\n"),
        &jest_test(ES_DEFAULT, assigned_call),
        false,
    )?;
    case(
        "commonjs-constructor-replaced",
        &format!("module.exports = class {replaced};\n"),
        &jest_test(REQUIRE, assigned_call),
        false,
    )
}

#[test]
fn members_of_an_anonymous_default_class_are_named_default() -> Result<(), String> {
    for (label, text) in [("es", es_anonymous()), ("commonjs", commonjs_anonymous(""))] {
        let owners = extract_owners(Path::new("src/cart.js"), &text);
        let total: Vec<_> = owners
            .iter()
            .filter(|owner| owner.name == "total")
            .map(|owner| (owner.class_name.as_deref(), owner.class_default_export))
            .collect();
        if total != [(Some("default"), true)] {
            return Err(format!("{label}: unexpected owners {owners:?}"));
        }
    }
    // A named class keeps its own name, and an anonymous class that is not
    // the default export keeps none.
    let named = extract_owners(
        Path::new("src/cart.js"),
        &format!("export default class Cart {BODY}\n"),
    );
    let local = extract_owners(
        Path::new("src/cart.js"),
        &format!("export const make = () => class {BODY};\n"),
    );
    let named_total: Vec<_> = named
        .iter()
        .filter(|owner| owner.name == "total")
        .map(|owner| owner.class_name.as_deref())
        .collect();
    if named_total != [Some("Cart")] {
        return Err(format!(
            "expected the named class's `total` as `Cart`, got {named:?}"
        ));
    }
    // The factory itself is an owner, so the check below reads a non-empty
    // set; none of its owners may take the default class's name.
    if !local.iter().any(|owner| owner.name == "make")
        || local
            .iter()
            .any(|owner| owner.class_name.as_deref() == Some("default"))
    {
        return Err(format!(
            "expected the `make` owner and no `default` name, got {local:?}"
        ));
    }
    Ok(())
}
