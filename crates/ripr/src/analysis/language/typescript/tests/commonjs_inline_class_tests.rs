//! A class expression assigned as a CommonJS module's whole export.
//!
//! `module.exports = class Cart { ... }` is the inline twin of `class Cart {}`
//! followed by `module.exports = Cart;` (RIPR-SPEC-0243 rule 19, #15092).
//! Before #16252 the class's methods were no owners at all, so a changed
//! method produced no finding. They are now owners carrying the same
//! whole-export markers, and a whole-module `require` under any local name
//! relates a method and the constructor.
//!
//! * The markers hold only while the file-level `module.exports` gate holds
//!   and the file adds no named export beside the class. Otherwise the members
//!   stay owners (a changed line is still reported) but lose the class name:
//!   the expression's own name binds only inside its body, so no named import
//!   reaches them.
//! * A default-named binding (`const { default: Cart } = require(...)`,
//!   `import Cart from`) and a named `require` stay unrelated, as for the
//!   declared class.
//! * An anonymous `module.exports = class { ... }` is the whole export too:
//!   its members are named `default` and relate through a whole-module
//!   `require`, like `export default class {}` through a default import
//!   (#16349).
//!
//! * A static method relates through the whole-module `require` binding
//!   (`Basket.make()`), not through a named binding, which reads a static
//!   property of the class.
//! * An instance field shadows the prototype method a sibling calls.
//!
//! The relation cases run `find_related_tests` after asserting the parsed
//! subjects; the end-to-end cases run `TypeScriptAdapter::analyze_diff` and
//! read the finding on the changed line.

use super::*;
use crate::domain::RelationReason;

const BODY: &str = " {\n  constructor() {\n    this.items = [];\n  }\n  add(price) {\n    this.items.push(price);\n  }\n  total() {\n    let sum = 0;\n    for (const p of this.items) sum += p;\n    if (sum >= 100) {\n      return sum - 10;\n    }\n    return sum;\n  }\n};\n";
const OTHER: &str = "class Cart {\n  constructor() {\n    this.items = [];\n  }\n  add() {}\n  total() {\n    return 110;\n  }\n}\nmodule.exports = Cart;\n";
const STUB_CLASS: &str = "class { add() {} total() { return 110; } }";

const REQUIRE: &str = "const Cart = require('../src/cart');";
const NAMED_REQUIRE: &str = "const { Cart } = require('../src/cart');";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subject {
    Total,
    Constructor,
}

const BOTH: [Subject; 2] = [Subject::Total, Subject::Constructor];

/// `module.exports = class Cart {...};` (or anonymous) followed by `extra`.
fn owner_source(class_head: &str, extra: &str) -> String {
    format!("module.exports = {class_head}{BODY}{extra}")
}

fn test_source(preamble: &str, class_local: &str) -> String {
    format!(
        "{preamble}\n\ndescribe('Cart', () => {{\n  test('total applies the bulk rebate', () => {{\n    const cart = new {class_local}();\n    cart.add(60);\n    cart.add(60);\n    expect(cart.total()).toBe(110);\n  }});\n}});\n"
    )
}

/// The members of the inline class: the first owners of `src/cart.js` (the
/// class opens the file), whatever their class name reads.
fn inline_members(owner_text: &str) -> Vec<TypeScriptOwner> {
    let owners = extract_owners(Path::new("src/cart.js"), owner_text);
    let body_end = owner_text
        .find("};\n")
        .map_or(0, |at| owner_text[..at].lines().count() + 1);
    owners
        .into_iter()
        .filter(|owner| owner.start_line <= body_end)
        .collect()
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
    for (path, text) in files {
        sources.insert(PathBuf::from(path), text.to_string());
    }
    sources.insert(test_file.to_path_buf(), test_text.to_string());
    let index = ReExportIndex::build(
        &workspace_files,
        &sources,
        Path::new(""),
        None,
        |path: &Path| path.starts_with("tests"),
    );
    let members = inline_members(owner_text);
    let wanted: Vec<&TypeScriptOwner> = members
        .iter()
        .filter(|owner| match subject {
            Subject::Total => owner.name == "total",
            Subject::Constructor => owner.method_kind == TypeScriptMethodKind::Constructor,
        })
        .collect();
    if wanted.len() != 1 {
        return Err(format!(
            "expected one {subject:?} owner of the inline class, got {}",
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

fn assert_credited(owner_text: &str, preamble: &str, local: &str) -> Result<(), String> {
    for subject in BOTH {
        if relation(owner_text, &test_source(preamble, local), subject)?.is_none() {
            return Err(format!(
                "{owner_text:?} / {preamble:?} / {subject:?}: expected a relation, got none"
            ));
        }
    }
    Ok(())
}

fn assert_withheld(owner_text: &str, preamble: &str, local: &str) -> Result<(), String> {
    for subject in BOTH {
        let related = relation(owner_text, &test_source(preamble, local), subject)?;
        if related.is_some() {
            return Err(format!(
                "{owner_text:?} / {preamble:?} / {subject:?}: expected no relation, got {related:?}"
            ));
        }
    }
    Ok(())
}

/// `Some(true)` when every inline member carries both whole-export markers
/// and the class name `Cart`, `Some(false)` when none carries a marker or the
/// name; an `Err` on a mix or a missing member.
fn marked_whole_export(owner_text: &str) -> Result<bool, String> {
    let members = inline_members(owner_text);
    if members.len() != 3 {
        return Err(format!(
            "expected the constructor, add and total of the inline class, got {}",
            members.len()
        ));
    }
    let marked = members
        .iter()
        .filter(|owner| {
            owner.class_default_export
                && owner.commonjs_default_export
                && owner.class_name.as_deref() == Some("Cart")
        })
        .count();
    let unmarked = members
        .iter()
        .filter(|owner| {
            !owner.class_default_export
                && !owner.commonjs_default_export
                && owner.class_name.is_none()
        })
        .count();
    match (marked, unmarked) {
        (3, 0) => Ok(true),
        (0, 3) => Ok(false),
        _ => Err(format!(
            "{owner_text:?}: {marked} marked and {unmarked} unmarked of 3 members"
        )),
    }
}

#[test]
fn every_method_of_an_inline_whole_export_class_is_an_owner() -> Result<(), String> {
    let text = owner_source("class Cart", "");
    let members = inline_members(&text);
    let names: Vec<&str> = members.iter().map(|owner| owner.name.as_str()).collect();
    if names.len() != 3 || !names.contains(&"add") || !names.contains(&"total") {
        return Err(format!(
            "expected constructor, add and total, got {names:?}"
        ));
    }
    if !marked_whole_export(&text)? {
        return Err("inline whole-export class not marked".to_string());
    }
    Ok(())
}

#[test]
fn a_whole_module_require_of_the_inline_class_relates_under_any_local_name() -> Result<(), String> {
    let text = owner_source("class Cart", "");
    for preamble in [
        REQUIRE,
        "let Cart = require('../src/cart');",
        "var Cart = require('../src/cart');",
    ] {
        assert_credited(&text, preamble, "Cart")?;
    }
    assert_credited(&text, "const Basket = require('../src/cart');", "Basket")
}

#[test]
fn a_default_named_or_named_binding_of_the_inline_class_stays_unrelated() -> Result<(), String> {
    let text = owner_source("class Cart", "");
    for preamble in [
        NAMED_REQUIRE,
        "const { default: Cart } = require('../src/cart');",
        "import Cart from '../src/cart';",
        "import { Cart } from '../src/cart';",
    ] {
        assert_withheld(&text, preamble, "Cart")?;
    }
    Ok(())
}

#[test]
fn a_require_of_another_module_does_not_lend_its_class() -> Result<(), String> {
    assert_withheld(
        &owner_source("class Cart", ""),
        "const Cart = require('../src/other');",
        "Cart",
    )
}

#[test]
fn a_named_export_beside_the_inline_class_unmarks_it_and_drops_its_name() -> Result<(), String> {
    for extra in [
        // The named property is another class: a named require reads it.
        format!("module.exports.Cart = {STUB_CLASS};\n"),
        "module.exports.helper = () => 1;\n".to_string(),
        "exports.helper = () => 1;\n".to_string(),
    ] {
        let text = owner_source("class Cart", &extra);
        if marked_whole_export(&text)? {
            return Err(format!("{extra:?}: inline class still marked"));
        }
        assert_withheld(&text, REQUIRE, "Cart")?;
        assert_withheld(&text, NAMED_REQUIRE, "Cart")?;
    }
    Ok(())
}

#[test]
fn an_ambiguous_whole_export_unmarks_the_inline_class() -> Result<(), String> {
    for extra in [
        // Two assignments: which one stands is not read.
        format!("if (process.env.X) module.exports = {STUB_CLASS};\n"),
        // The module object escapes.
        format!("const m = module;\nm.exports = {STUB_CLASS};\n"),
    ] {
        let text = owner_source("class Cart", &extra);
        if marked_whole_export(&text)? {
            return Err(format!("{extra:?}: inline class still marked"));
        }
        assert_withheld(&text, REQUIRE, "Cart")?;
        assert_withheld(&text, NAMED_REQUIRE, "Cart")?;
    }
    Ok(())
}

#[test]
fn a_rebound_or_shadowed_require_local_stays_unrelated() -> Result<(), String> {
    let text = owner_source("class Cart", "");
    for preamble in [
        format!("let Cart = require('../src/cart');\nCart = {STUB_CLASS};"),
        format!("var Cart = require('../src/cart');\nvar Cart = {STUB_CLASS};"),
    ] {
        assert_withheld(&text, &preamble, "Cart")?;
    }
    let shadowed = test_source(REQUIRE, "Cart").replace(
        "    const cart = new Cart();\n",
        "    class Cart { add() {} total() { return 110; } }\n    const cart = new Cart();\n",
    );
    if !shadowed.contains("    class Cart { add()") {
        return Err("fixture lost the body-local class".to_string());
    }
    for subject in BOTH {
        let related = relation(&text, &shadowed, subject)?;
        if related.is_some() {
            return Err(format!(
                "body-local class / {subject:?}: expected no relation, got {related:?}"
            ));
        }
    }
    Ok(())
}

/// An anonymous inline class is the whole export as well: its members are
/// named `default` and a whole-module `require` relates them (#16349); a
/// named `require` still reads a property the class does not have.
#[test]
fn an_anonymous_inline_class_relates_through_a_whole_module_require() -> Result<(), String> {
    let text = owner_source("class", "");
    let members = inline_members(&text);
    if members.len() != 3
        || members
            .iter()
            .any(|owner| owner.class_name.as_deref() != Some("default"))
    {
        return Err(format!(
            "expected three members named `default`, got {:?}",
            members
                .iter()
                .map(|owner| (&owner.name, &owner.class_name))
                .collect::<Vec<_>>()
        ));
    }
    assert_credited(&text, REQUIRE, "Cart")?;
    assert_withheld(&text, NAMED_REQUIRE, "Cart")
}

#[test]
fn accessor_pairs_of_the_inline_class_are_not_reassigned_exports() {
    let text = "module.exports = class Cart {\n  get size() {\n    return this.n;\n  }\n  set size(value) {\n    this.n = value;\n  }\n};\n";
    let owners = extract_owners(Path::new("src/cart.js"), text);
    let sizes = owners.iter().filter(|owner| owner.name == "size").count();
    assert_eq!(sizes, 2, "getter and setter both stay owners: {owners:?}");
}

// ── End to end through `TypeScriptAdapter::analyze_diff` ────────────────────

const JEST_PACKAGE: &str =
    "{\"name\":\"demo\",\"version\":\"1.0.0\",\"devDependencies\":{\"jest\":\"^29.0.0\"}}\n";

/// The finding `analyze_diff` reports on the line of `src/cart.js` that reads
/// `changed`, with `test` as `tests/cart.test.js`; `Ok(None)` when the
/// changed line yields no finding at all.
fn diff_finding(
    label: &str,
    owner_text: &str,
    changed: &str,
    test: &str,
) -> Result<Option<Finding>, String> {
    let line = owner_text
        .lines()
        .position(|text| text == changed)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("package.json"), JEST_PACKAGE)?;
    ts_write_file(&root.join("src/cart.js"), owner_text)?;
    ts_write_file(&root.join("tests/cart.test.js"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.js", &[(line, changed)])],
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line))
}

fn jest_test(preamble: &str, body: &str) -> String {
    format!("{preamble}\n\ndescribe('Cart', () => {{\n  test('it', () => {{\n{body}  }});\n}});\n")
}

const CHANGED_TOTAL: &str = "    if (sum >= 100) {";

#[test]
fn a_changed_method_of_the_inline_class_is_reported_and_related() -> Result<(), String> {
    let test = test_source(REQUIRE, "Cart");
    let named = diff_finding(
        "inline-class-finding",
        &owner_source("class Cart", ""),
        CHANGED_TOTAL,
        &test,
    )?
    .ok_or("the changed method of the inline class yields no finding")?;
    if named.related_tests.is_empty() {
        return Err(format!("expected a related test, got {named:?}"));
    }
    let anonymous = diff_finding(
        "inline-anonymous-class-finding",
        &owner_source("class", ""),
        CHANGED_TOTAL,
        &test,
    )?
    .ok_or("the changed method of the anonymous inline class yields no finding")?;
    // Since #16349 the anonymous class is the module's whole export too, so the
    // whole-module `require` relates it like its named twin.
    if anonymous.related_tests.is_empty() {
        return Err(format!(
            "anonymous inline class: expected a related test, got {anonymous:?}"
        ));
    }
    Ok(())
}

const STATIC_MAKE: &str =
    "  static make(n) {\n    if (n >= 100) {\n      return n - 10;\n    }\n    return n;\n  }\n";
const CHANGED_MAKE: &str = "    if (n >= 100) {";

#[test]
fn a_static_method_relates_through_the_whole_module_require_only() -> Result<(), String> {
    let source = format!("module.exports = class Cart {{\n{STATIC_MAKE}}};\n");
    let whole = diff_finding(
        "inline-static-whole-require",
        &source,
        CHANGED_MAKE,
        &jest_test(
            "const Basket = require('../src/cart');",
            "    expect(Basket.make(100)).toBe(90);\n",
        ),
    )?
    .ok_or("the changed static method yields no finding")?;
    if whole.related_tests.is_empty() {
        return Err(format!(
            "whole-module require: expected a related test, got {whole:?}"
        ));
    }
    // `const { Cart }` reads the static property `Cart`, another class.
    let with_property = format!(
        "module.exports = class Cart {{\n{STATIC_MAKE}  static Cart = class {{ static make() {{ return 90; }} }};\n}};\n"
    );
    let named = diff_finding(
        "inline-static-named-require",
        &with_property,
        CHANGED_MAKE,
        &jest_test(NAMED_REQUIRE, "    expect(Cart.make(100)).toBe(90);\n"),
    )?
    .ok_or("the changed static method yields no finding")?;
    if !named.related_tests.is_empty() {
        return Err(format!(
            "named require of a static property: expected no related test, got {:?}",
            named.related_tests
        ));
    }
    Ok(())
}

#[test]
fn an_instance_field_shadows_the_prototype_method_a_sibling_calls() -> Result<(), String> {
    let sibling = "  read() {\n    return this.total(100);\n  }\n  total(n) {\n    if (n >= 100) {\n      return n - 10;\n    }\n    return n;\n  }\n";
    let changed = "    if (n >= 100) {";
    let reached = diff_finding(
        "inline-sibling-reached",
        &format!("module.exports = class Cart {{\n{sibling}}};\n"),
        changed,
        &jest_test(REQUIRE, "    expect(new Cart().read()).toBe(100);\n"),
    )?
    .ok_or("the changed method yields no finding")?;
    if reached.related_tests.is_empty() {
        return Err(format!(
            "sibling call without a field: expected a related test, got {reached:?}"
        ));
    }
    let shadowed = diff_finding(
        "inline-sibling-field-shadow",
        &format!("module.exports = class Cart {{\n  total = () => 1;\n{sibling}}};\n"),
        changed,
        &jest_test(REQUIRE, "    expect(new Cart().read()).toBe(1);\n"),
    )?
    .ok_or("the changed method yields no finding")?;
    if !shadowed.related_tests.is_empty() {
        return Err(format!(
            "an instance field replaces the method: expected no related test, got {:?}",
            shadowed.related_tests
        ));
    }
    Ok(())
}

#[test]
fn a_reassigned_binding_sharing_a_method_name_keeps_the_method_owner() -> Result<(), String> {
    // The named export unmarks the class and clears its name; the method is
    // still a method, so the name-only reassignment filter must not drop it.
    let source = "let total = () => 0;\ntotal = () => 1;\nmodule.exports = class Cart {\n  total(n) {\n    if (n >= 100) {\n      return n - 10;\n    }\n    return n;\n  }\n};\nmodule.exports.helper = total;\n";
    let owners = extract_owners(Path::new("src/cart.js"), source);
    if !owners
        .iter()
        .any(|owner| owner.name == "total" && owner.owner_kind == OwnerKind::Method)
    {
        return Err(format!("the method owner `total` was dropped: {owners:?}"));
    }
    diff_finding(
        "inline-reassigned-helper",
        source,
        "    if (n >= 100) {",
        &jest_test(REQUIRE, "    expect(new Cart().total(100)).toBe(100);\n"),
    )?
    .ok_or("the changed method yields no finding")?;
    Ok(())
}
