//! A class that is a CommonJS module's whole export.
//!
//! `class Cart {}` followed by `module.exports = Cart;` makes `require()`
//! hand back the class itself, so `const Cart = require('../src/cart')` binds
//! the owner's class under any local name and a method called on `new Cart()`
//! relates. This is the class twin of the function rule (RIPR-SPEC-0243 rule
//! 19) and the CommonJS twin of `export default Cart;`.
//!
//! * A whole-module `require` (`const`, `let`, `var`, any local name) relates
//!   a method and the constructor. A default-named binding of the module
//!   (`const { default: Cart } = require(...)`, `import Cart from`) does not.
//! * The class is marked only while the name provably still holds it: one
//!   file-level declaration, never written, exactly one `module.exports`
//!   assignment, no other use of the module object, no named export beside it.
//! * A `require` of an ES module's default class is a namespace object, not
//!   the class, and stays unrelated.
//!
//! Every case relates through `find_related_tests`, the production path, after
//! asserting the parsed subjects, and every negative has a credited twin that
//! differs only in the export or import line.

use super::*;
use crate::domain::RelationReason;

const CLASS: &str = "class Cart {\n  constructor() {\n    this.items = [];\n  }\n  add(price) {\n    this.items.push(price);\n  }\n  total() {\n    let sum = 0;\n    for (const p of this.items) sum += p;\n    if (sum >= 100) {\n      return sum - 10;\n    }\n    return sum;\n  }\n}\n";
const OTHER: &str = "class Cart {\n  constructor() {\n    this.items = [];\n  }\n  add() {}\n  total() {\n    return 110;\n  }\n}\nmodule.exports = Cart;\n";
const LOCAL_CLASS: &str = "class { add() {} total() { return 110; } }";

const WHOLE_EXPORT: &str = "module.exports = Cart;";
const REQUIRE: &str = "const Cart = require('../src/cart');";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subject {
    Total,
    Constructor,
}

const BOTH: [Subject; 2] = [Subject::Total, Subject::Constructor];

fn owner_source(export_lines: &str) -> String {
    format!("{CLASS}{export_lines}\n")
}

fn test_source(preamble: &str, class_local: &str) -> String {
    format!(
        "{preamble}\n\ndescribe('Cart', () => {{\n  test('total applies the bulk rebate', () => {{\n    const cart = new {class_local}();\n    cart.add(60);\n    cart.add(60);\n    expect(cart.total()).toBe(110);\n  }});\n}});\n"
    )
}

fn relation(
    owner_text: &str,
    test_text: &str,
    subject: Subject,
) -> Result<Option<RelationReason>, String> {
    let files = [("src/cart.js", owner_text), ("src/other.js", OTHER)];
    let test_file = Path::new("tests/cart.test.js");
    let mut workspace_files: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    workspace_files.push(test_file.to_path_buf());
    let mut sources = std::collections::HashMap::new();
    let mut owners = Vec::new();
    for (path, text) in files {
        sources.insert(PathBuf::from(path), text.to_string());
        owners.extend(extract_owners(Path::new(path), text));
    }
    sources.insert(test_file.to_path_buf(), test_text.to_string());
    let index = ReExportIndex::build(
        &workspace_files,
        &sources,
        Path::new(""),
        None,
        |path: &Path| path.starts_with("tests"),
    );
    let wanted: Vec<&TypeScriptOwner> = owners
        .iter()
        .filter(|owner| {
            owner.file == Path::new("src/cart.js")
                && owner.class_name.as_deref() == Some("Cart")
                && match subject {
                    Subject::Total => owner.name == "total",
                    Subject::Constructor => owner.method_kind == TypeScriptMethodKind::Constructor,
                }
        })
        .collect();
    if wanted.len() != 1 {
        return Err(format!(
            "expected one {subject:?} owner of Cart in src/cart.js, got {}",
            wanted.len()
        ));
    }
    let tests = extract_tests(test_file, test_text);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    if !tests[0].body_text.contains("cart.total()") {
        return Err(format!("test body lost the call: {:?}", tests[0].body_text));
    }
    let related = find_related_tests(wanted[0], &tests, None, &index, None);
    Ok(related.first().and_then(|test| test.relation_reason))
}

fn assert_credited(export_lines: &str, preamble: &str, local: &str) -> Result<(), String> {
    for subject in BOTH {
        let related = relation(
            &owner_source(export_lines),
            &test_source(preamble, local),
            subject,
        )?;
        if related.is_none() {
            return Err(format!(
                "{export_lines:?} / {preamble:?} / {subject:?}: expected a relation, got none"
            ));
        }
    }
    Ok(())
}

fn assert_withheld(export_lines: &str, preamble: &str, local: &str) -> Result<(), String> {
    for subject in BOTH {
        let related = relation(
            &owner_source(export_lines),
            &test_source(preamble, local),
            subject,
        )?;
        if related.is_some() {
            return Err(format!(
                "{export_lines:?} / {preamble:?} / {subject:?}: expected no relation, got {related:?}"
            ));
        }
    }
    Ok(())
}

/// Whether every member of `Cart` carries both markers; an `Err` when the
/// members disagree or the class did not parse.
fn marked_whole_export(export_lines: &str) -> Result<bool, String> {
    let text = owner_source(export_lines);
    let owners = extract_owners(Path::new("src/cart.js"), &text);
    let members: Vec<&TypeScriptOwner> = owners
        .iter()
        .filter(|owner| owner.class_name.as_deref() == Some("Cart"))
        .collect();
    if members.len() < 3 {
        return Err(format!(
            "expected the constructor, add and total of Cart, got {}",
            members.len()
        ));
    }
    let marked = members
        .iter()
        .filter(|owner| owner.class_default_export && owner.commonjs_default_export)
        .count();
    let half = members
        .iter()
        .filter(|owner| owner.class_default_export != owner.commonjs_default_export)
        .count();
    if half != 0 || (marked != 0 && marked != members.len()) {
        return Err(format!(
            "{export_lines:?}: {marked} of {} Cart members marked, {half} half-marked",
            members.len()
        ));
    }
    Ok(marked != 0)
}

#[test]
fn a_whole_module_require_of_the_class_relates_under_any_binding_kind() -> Result<(), String> {
    if !marked_whole_export(WHOLE_EXPORT)? {
        return Err("module.exports = Cart did not mark the class".to_string());
    }
    for preamble in [
        REQUIRE,
        "let Cart = require('../src/cart');",
        "var Cart = require('../src/cart');",
    ] {
        assert_credited(WHOLE_EXPORT, preamble, "Cart")?;
    }
    Ok(())
}

#[test]
fn the_require_binding_may_choose_any_local_name() -> Result<(), String> {
    assert_credited(
        WHOLE_EXPORT,
        "const Basket = require('../src/cart');",
        "Basket",
    )
}

#[test]
fn a_default_named_binding_of_the_commonjs_class_stays_unrelated() -> Result<(), String> {
    // `const { default: Cart } = require(...)` reads a property the class
    // does not have, and its record is the same as an ES `import Cart from`;
    // both are refused, as for a whole-export function (rule 19).
    for preamble in [
        "const { default: Cart } = require('../src/cart');",
        "import Cart from '../src/cart';",
        "import { Cart } from '../src/cart';",
    ] {
        assert_withheld(WHOLE_EXPORT, preamble, "Cart")?;
    }
    Ok(())
}

#[test]
fn a_require_of_another_module_does_not_lend_its_class() -> Result<(), String> {
    assert_withheld(
        WHOLE_EXPORT,
        "const Cart = require('../src/other');",
        "Cart",
    )
}

#[test]
fn a_named_require_of_a_whole_export_class_stays_unrelated() -> Result<(), String> {
    // The class has no property named `Cart`; the destructure binds nothing.
    assert_withheld(
        WHOLE_EXPORT,
        "const { Cart } = require('../src/cart');",
        "Cart",
    )
}

#[test]
fn a_class_with_a_named_export_too_keeps_its_named_require_reading() -> Result<(), String> {
    for export_lines in [
        "module.exports = Cart;\nmodule.exports.Cart = Cart;",
        "module.exports = { Cart };",
    ] {
        if marked_whole_export(export_lines)? {
            return Err(format!(
                "{export_lines:?}: marker would withdraw the named require"
            ));
        }
        assert_credited(
            export_lines,
            "const { Cart } = require('../src/cart');",
            "Cart",
        )?;
    }
    Ok(())
}

#[test]
fn a_static_property_naming_the_class_keeps_its_named_require_reading() -> Result<(), String> {
    for export_lines in [
        "Cart.Cart = Cart;\nmodule.exports = Cart;",
        "module.exports = Cart;\nCart.Cart = Cart;",
        "Cart['Cart'] = Cart;\nmodule.exports = Cart;",
        "Object.assign(Cart, { Cart });\nmodule.exports = Cart;",
        "Object.defineProperty(Cart, 'Cart', { value: Cart });\nmodule.exports = Cart;",
    ] {
        if marked_whole_export(export_lines)? {
            return Err(format!(
                "{export_lines:?}: marker would drop the named require"
            ));
        }
        assert_credited(
            export_lines,
            "const { Cart } = require('../src/cart');",
            "Cart",
        )?;
    }
    // Reading a member of the class is not an export: the class stays marked.
    for export_lines in [
        "const limit = Cart.length === 0 ? 1 : 2;\nmodule.exports = Cart;",
        "if (Cart.name == 'Cart') console.log(limit => limit);\nmodule.exports = Cart;",
    ] {
        if !marked_whole_export(export_lines)? {
            return Err(format!(
                "{export_lines:?}: a member read unmarked the class"
            ));
        }
        assert_credited(export_lines, REQUIRE, "Cart")?;
    }
    Ok(())
}

#[test]
fn a_rewritten_or_ambiguous_whole_export_is_not_marked() -> Result<(), String> {
    for export_lines in [
        // The class binding is assignable.
        "Cart = class { add() {} total() { return 110; } };\nmodule.exports = Cart;",
        "function swap() {\n  Cart = null;\n}\nmodule.exports = Cart;",
        // Two assignments: which one stands is not read.
        "module.exports = Cart;\nif (process.env.X) module.exports = class { add() {} total() { return 110; } };",
        // The module object escapes.
        "const m = module;\nm.exports = class { add() {} total() { return 110; } };\nmodule.exports = Cart;",
        // Another value is the whole export.
        "class Other { add() {} total() { return 110; } }\nmodule.exports = Other;",
    ] {
        if marked_whole_export(export_lines)? {
            return Err(format!("{export_lines:?}: class marked"));
        }
        assert_withheld(export_lines, REQUIRE, "Cart")?;
    }
    Ok(())
}

#[test]
fn a_rebound_require_local_stays_unrelated() -> Result<(), String> {
    for preamble in [
        format!("let Cart = require('../src/cart');\nCart = {LOCAL_CLASS};"),
        format!("var Cart = require('../src/cart');\nvar Cart = {LOCAL_CLASS};"),
    ] {
        assert_withheld(WHOLE_EXPORT, &preamble, "Cart")?;
    }
    Ok(())
}

#[test]
fn a_describe_level_class_of_the_same_name_shadows_the_require() -> Result<(), String> {
    let source = test_source(REQUIRE, "Cart").replace(
        "describe('Cart', () => {\n",
        &format!("describe('Cart', () => {{\n  const Cart = {LOCAL_CLASS};\n"),
    );
    if !source.contains("  const Cart = class") {
        return Err("fixture lost the describe-level class".to_string());
    }
    for subject in BOTH {
        let related = relation(&owner_source(WHOLE_EXPORT), &source, subject)?;
        if related.is_some() {
            return Err(format!(
                "describe-level shadow / {subject:?}: expected no relation, got {related:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_class_declared_in_the_test_body_shadows_the_require() -> Result<(), String> {
    for (preamble, local) in [
        (REQUIRE, "Cart"),
        ("const Basket = require('../src/cart');", "Basket"),
    ] {
        assert_credited(WHOLE_EXPORT, preamble, local)?;
        let source = test_source(preamble, local).replace(
            &format!("    const cart = new {local}();\n"),
            &format!(
                "    class {local} {{ add() {{}} total() {{ return 110; }} }}\n    const cart = new {local}();\n"
            ),
        );
        if !source.contains(&format!("    class {local} {{ add()")) {
            return Err("fixture lost the body-local class".to_string());
        }
        for subject in BOTH {
            let related = relation(&owner_source(WHOLE_EXPORT), &source, subject)?;
            if related.is_some() {
                return Err(format!(
                    "body-local class {local} / {subject:?}: expected no relation, got {related:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn a_require_of_an_es_default_class_stays_unrelated() -> Result<(), String> {
    // `require()` of an ES module is its namespace object; the class is
    // `.default`, so `new Cart()` does not construct it.
    for export_lines in ["export default Cart;", "export { Cart as default };"] {
        assert_withheld(export_lines, REQUIRE, "Cart")?;
    }
    let inline = CLASS.replacen("class Cart", "export default class Cart", 1);
    for subject in BOTH {
        if relation(&inline, &test_source(REQUIRE, "Cart"), subject)?.is_some() {
            return Err(format!(
                "inline ES default class / {subject:?}: a require related"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_whole_export_function_keeps_its_direct_call_reading() -> Result<(), String> {
    // The function rule (rule 19) is untouched by the class marker.
    let owner_text = "function price(amount) {\n  if (amount >= 100) {\n    return amount - 10;\n  }\n  return amount;\n}\nmodule.exports = price;\n";
    let test_text = "const price = require('../src/cart');\n\ntest('rebate', () => {\n  expect(price(120)).toBe(110);\n});\n";
    let owners = extract_owners(Path::new("src/cart.js"), owner_text);
    let wanted: Vec<&TypeScriptOwner> = owners
        .iter()
        .filter(|owner| owner.name == "price")
        .collect();
    if wanted.len() != 1 || !wanted[0].commonjs_default_export || wanted[0].class_default_export {
        return Err("the whole-export function owner lost its markers".to_string());
    }
    let test_file = Path::new("tests/cart.test.js");
    let workspace_files = vec![PathBuf::from("src/cart.js"), test_file.to_path_buf()];
    let mut sources = std::collections::HashMap::new();
    sources.insert(PathBuf::from("src/cart.js"), owner_text.to_string());
    sources.insert(test_file.to_path_buf(), test_text.to_string());
    let index = ReExportIndex::build(
        &workspace_files,
        &sources,
        Path::new(""),
        None,
        |path: &Path| path.starts_with("tests"),
    );
    let tests = extract_tests(test_file, test_text);
    if tests.len() != 1 {
        return Err(format!("expected one parsed test, got {}", tests.len()));
    }
    if find_related_tests(wanted[0], &tests, None, &index, None).is_empty() {
        return Err("the whole-export function lost its test".to_string());
    }
    Ok(())
}
