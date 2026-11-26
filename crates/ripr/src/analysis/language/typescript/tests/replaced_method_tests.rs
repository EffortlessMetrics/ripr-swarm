//! A class method that an assignment replaces (#16314, RIPR-SPEC-0243 rule
//! 29).
//!
//! `this.total = (n) => n;` in the constructor defines `total` on every
//! instance, and a top-level `Cart.prototype.total = ...` (or, for a static
//! method, `Cart.total = ...`) overwrites the method itself, so
//! `new Cart().total()` runs the assigned value and never the method body. A
//! change to the method is still reported but relates to no test, and its
//! guidance names the assignment.
//!
//! Controls that keep their relation: an assignment that reads the same
//! member (`this.total = this.total.bind(this)`, `Cart.prototype.total =
//! wrap(Cart.prototype.total)`), a conditional constructor assignment, an
//! assignment of another name, and an assignment of the other placement.
//!
//! Each test body asserts what the code does at runtime: a replaced case
//! asserts the assigned value, a control asserts the method's value.

use super::*;

const JEST_PACKAGE: &str =
    "{\"name\":\"demo\",\"version\":\"1.0.0\",\"devDependencies\":{\"jest\":\"^29.0.0\"}}\n";

/// The method whose `if` line is the changed line.
const METHOD: &str =
    "  total(n) {\n    if (n > 50) {\n      return n - 5;\n    }\n    return n;\n  }\n";
const CHANGED: &str = "    if (n > 50) {";

const NAMED_REQUIRE: &str = "const { Cart } = require('../src/cart');";
/// Runs the method: the body subtracts 5 above 50.
const METHOD_CALL: &str = "    expect(new Cart().total(100)).toBe(95);\n";
/// Runs an assigned `(n) => n`, which returns its input.
const ASSIGNED_CALL: &str = "    expect(new Cart().total(100)).toBe(100);\n";
const STATIC_METHOD_CALL: &str = "    expect(Cart.total(100)).toBe(95);\n";
const STATIC_ASSIGNED_CALL: &str = "    expect(Cart.total(100)).toBe(100);\n";

/// The finding `analyze_diff` reports on the line of `src/cart.js` that reads
/// `CHANGED`, with `test` as the jest test file.
fn diff_finding(label: &str, owner_text: &str, test: &str) -> Result<Finding, String> {
    diff_finding_at(label, "src/cart.js", owner_text, "tests/cart.test.js", test)
}

/// [`diff_finding`] with the owner at `owner_path` and the test at
/// `test_path`.
fn diff_finding_at(
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

/// `class Cart { constructor() { <ctor> } <method> } <after>` exported as a
/// named CommonJS export. An empty `ctor` leaves the constructor out.
fn cart(ctor: &str, method: &str, after: &str) -> String {
    let constructor = if ctor.is_empty() {
        String::new()
    } else {
        format!("  constructor() {{\n{ctor}  }}\n")
    };
    format!("class Cart {{\n{constructor}{method}}}\n{after}module.exports = {{ Cart }};\n")
}

fn static_method() -> String {
    METHOD.replace("  total(n)", "  static total(n)")
}

fn case(label: &str, owner_text: &str, body: &str, related: bool) -> Result<(), String> {
    let finding = diff_finding(label, owner_text, &jest_test(NAMED_REQUIRE, body))?;
    expect_related(label, &finding, related)
}

#[test]
fn a_method_replaced_by_an_assignment_relates_to_no_test() -> Result<(), String> {
    case(
        "constructor",
        &cart("    this.total = (n) => n;\n", METHOD, ""),
        ASSIGNED_CALL,
        false,
    )?;
    case(
        "constructor-reads-another-object",
        &cart(
            "    this.total = helper.total;\n",
            METHOD,
            "const helper = { total: (n) => n };\n",
        ),
        ASSIGNED_CALL,
        false,
    )?;
    case(
        "prototype",
        &cart(
            "",
            METHOD,
            "Cart.prototype.total = function (n) { return n; };\n",
        ),
        ASSIGNED_CALL,
        false,
    )?;
    case(
        "static",
        &cart("", &static_method(), "Cart.total = (n) => n;\n"),
        STATIC_ASSIGNED_CALL,
        false,
    )?;
    // A wrapper around a different saved method still cuts `total` off.
    case(
        "prototype-wraps-another-method",
        &cart(
            "",
            &format!("{METHOD}  size(n) {{ return n; }}\n"),
            "const original = Cart.prototype.size;\nCart.prototype.total = function (n) { return original.call(this, n); };\n",
        ),
        ASSIGNED_CALL,
        false,
    )
}

#[test]
fn an_es_exported_class_method_replaced_in_the_constructor_relates_to_no_test() -> Result<(), String>
{
    let finding = diff_finding(
        "constructor-esm",
        &format!(
            "export class Cart {{\n  constructor() {{\n    this.total = (n) => n;\n  }}\n{METHOD}}}\n"
        ),
        &jest_test("import { Cart } from '../src/cart';", ASSIGNED_CALL),
    )?;
    expect_related("ES export", &finding, false)
}

/// A TypeScript cast on the receiver (`(Cart.prototype as any).total = ...`,
/// `(this as any).total = ...`) is the same replacement.
#[test]
fn a_replacement_through_a_typescript_cast_relates_to_no_test() -> Result<(), String> {
    let method = METHOD.replace("total(n) {", "total(n: number): number {");
    let static_method = static_method().replace("total(n) {", "total(n: number): number {");
    for (label, owner_text, body) in [
        (
            "ts-constructor-cast",
            format!(
                "export class Cart {{\n  constructor() {{\n    (this as any).total = (n: number) => n;\n  }}\n{method}}}\n"
            ),
            ASSIGNED_CALL,
        ),
        (
            "ts-prototype-cast",
            format!(
                "export class Cart {{\n{method}}}\n(Cart.prototype as any).total = (n: number) => n;\n"
            ),
            ASSIGNED_CALL,
        ),
        (
            "ts-static-cast",
            format!(
                "export class Cart {{\n{static_method}}}\n(Cart as any).total = (n: number) => n;\n"
            ),
            STATIC_ASSIGNED_CALL,
        ),
    ] {
        let finding = diff_finding_at(
            label,
            "src/cart.ts",
            &owner_text,
            "tests/cart.test.ts",
            &jest_test("import { Cart } from '../src/cart';", body),
        )?;
        expect_related(label, &finding, false)?;
    }
    Ok(())
}

#[test]
fn assignments_that_keep_the_method_keep_its_relation() -> Result<(), String> {
    for (label, owner_text, body) in [
        ("no-assignment", cart("", METHOD, ""), METHOD_CALL),
        (
            "constructor-self-bind",
            cart("    this.total = this.total.bind(this);\n", METHOD, ""),
            METHOD_CALL,
        ),
        (
            "prototype-wrap",
            cart(
                "",
                METHOD,
                "Cart.prototype.total = wrap(Cart.prototype.total);\nfunction wrap(f) { return f; }\n",
            ),
            METHOD_CALL,
        ),
        (
            "static-wrap",
            cart(
                "",
                &static_method(),
                "Cart.total = wrap(Cart.total);\nfunction wrap(f) { return f; }\n",
            ),
            STATIC_METHOD_CALL,
        ),
        (
            "constructor-conditional",
            cart(
                "    if (globalThis.FLAG) {\n      this.total = (n) => n;\n    }\n",
                METHOD,
                "",
            ),
            METHOD_CALL,
        ),
        (
            "constructor-bracket-bind",
            cart("    this.total = this['total'].bind(this);\n", METHOD, ""),
            METHOD_CALL,
        ),
        (
            "constructor-saved-original",
            cart(
                "    const original = this.total;\n    this.total = (n) => original.call(this, n);\n",
                METHOD,
                "",
            ),
            METHOD_CALL,
        ),
        (
            "constructor-after-early-return",
            cart(
                "    if (!globalThis.FLAG) {\n      return;\n    }\n    this.total = (n) => n;\n",
                METHOD,
                "",
            ),
            METHOD_CALL,
        ),
        (
            "prototype-saved-original",
            cart(
                "",
                METHOD,
                "const original = Cart.prototype.total;\nCart.prototype.total = function (n) { return original.call(this, n); };\n",
            ),
            METHOD_CALL,
        ),
        (
            "constructor-parameter-saves-original",
            format!(
                "class Cart {{\n  constructor(original = this.total) {{\n    this.total = (n) => original.call(this, n);\n  }}\n{METHOD}}}\nmodule.exports = {{ Cart }};\n"
            ),
            METHOD_CALL,
        ),
        (
            "constructor-other-name",
            cart("    this.items = [];\n", METHOD, ""),
            METHOD_CALL,
        ),
        (
            "prototype-other-name",
            cart(
                "",
                METHOD,
                "Cart.prototype.size = function () { return 0; };\n",
            ),
            METHOD_CALL,
        ),
        (
            "static-assignment-beside-instance-method",
            cart("", METHOD, "Cart.total = 0;\n"),
            METHOD_CALL,
        ),
        (
            "prototype-assignment-beside-static-method",
            cart("", &static_method(), "Cart.prototype.total = (n) => n;\n"),
            STATIC_METHOD_CALL,
        ),
    ] {
        case(label, &owner_text, body, true)?;
    }
    Ok(())
}

/// An inline `exports.Cart = class {}` binds no module-scope `Cart`, so a
/// top-level write to a local `Cart` does not replace the class's method.
#[test]
fn a_write_to_a_local_named_like_an_inline_export_class_keeps_the_relation() -> Result<(), String> {
    let owner_text = format!(
        "const Cart = {{ total: (n) => n }};\nexports.Cart = class {{\n{}}};\nCart.total = (n) => n * 2;\n",
        static_method()
    );
    case(
        "inline-export-local-write",
        &owner_text,
        STATIC_METHOD_CALL,
        true,
    )
}

/// A declared `class Cart` beside an inline `exports.Cart = class {}`: a
/// top-level `Cart.total = ...` replaces the declared class's method, not the
/// exported class's.
#[test]
fn a_write_to_a_declared_class_leaves_an_inline_export_class_of_that_name() -> Result<(), String> {
    let owner_text = format!(
        "class Cart {{\n  static total(n) {{ return 0; }}\n}}\nexports.Cart = class {{\n{}}};\nCart.total = (n) => n * 2;\n",
        static_method()
    );
    let owners = extract_owners(Path::new("src/cart.js"), &owner_text);
    let kinds: Vec<_> = owners
        .iter()
        .filter(|owner| owner.name == "total")
        .map(|owner| owner.method_kind)
        .collect();
    if kinds
        != [
            TypeScriptMethodKind::ReplacedByAssignment,
            TypeScriptMethodKind::Ordinary,
        ]
    {
        return Err(format!("unexpected owners {owners:?}"));
    }
    // Both classes on one line: the owner's own declaration decides, not
    // its line.
    let one_line = "class Cart { static total() { return 1; } } exports.Cart = class { static total() { return 80; } };\nCart.total = () => 0;\n";
    let one_line_owners = extract_owners(Path::new("src/cart.js"), one_line);
    let one_line_kinds: Vec<_> = one_line_owners
        .iter()
        .filter(|owner| owner.name == "total")
        .map(|owner| owner.method_kind)
        .collect();
    if one_line_kinds
        != [
            TypeScriptMethodKind::ReplacedByAssignment,
            TypeScriptMethodKind::Ordinary,
        ]
    {
        return Err(format!("unexpected one-line owners {one_line_owners:?}"));
    }
    case(
        "declared-and-inline-export",
        &owner_text,
        STATIC_METHOD_CALL,
        true,
    )
}

#[test]
fn a_replaced_method_is_an_owner_marked_replaced_and_an_accessor_is_not() -> Result<(), String> {
    let source = format!(
        "{}class Box {{\n  constructor() {{\n    this.size = 1;\n  }}\n  set size(v) {{ this._s = v; }}\n  get size() {{ return this._s; }}\n}}\n",
        cart("    this.total = (n) => n;\n", METHOD, "")
    );
    let owners = extract_owners(Path::new("src/cart.js"), &source);
    let kinds = |name: &str, class: &str| {
        owners
            .iter()
            .filter(|owner| owner.name == name && owner.class_name.as_deref() == Some(class))
            .map(|owner| owner.method_kind)
            .collect::<Vec<_>>()
    };
    if kinds("total", "Cart") != [TypeScriptMethodKind::ReplacedInConstructor] {
        return Err(format!("unexpected owners {owners:?}"));
    }
    // Assigning `this.size` in the constructor calls the setter; it does not
    // replace the accessor.
    if kinds("size", "Box") != [TypeScriptMethodKind::Setter, TypeScriptMethodKind::Getter] {
        return Err(format!("accessors must keep their kind: {owners:?}"));
    }
    Ok(())
}

#[test]
fn a_replaced_method_points_at_the_assignment() -> Result<(), String> {
    let finding = diff_finding(
        "replaced-guidance",
        &cart(
            "",
            METHOD,
            "Cart.prototype.total = function (n) { return n; };\n",
        ),
        &jest_test(NAMED_REQUIRE, ASSIGNED_CALL),
    )?;
    if finding.class != ExposureClass::NoStaticPath {
        return Err(format!("expected no_static_path, got {:?}", finding.class));
    }
    if !finding
        .missing
        .iter()
        .any(|line| line.contains("`Cart.prototype.total = ...`) replaces this body"))
    {
        return Err(format!(
            "missing names no assignment: {:?}",
            finding.missing
        ));
    }
    let step = finding.recommended_next_step.as_deref().unwrap_or_default();
    if !step.contains("an assignment to `total` in this file replaces the method")
        || !step.contains("`Cart.prototype.total = wrap(Cart.prototype.total)`")
        || step.contains("this.total")
    {
        return Err(format!(
            "recommendation does not name the assignment and its receiver: {step:?}"
        ));
    }
    let constructor = diff_finding(
        "replaced-guidance-constructor",
        &cart("    this.total = (n) => n;\n", METHOD, ""),
        &jest_test(NAMED_REQUIRE, ASSIGNED_CALL),
    )?;
    if !constructor.missing.iter().any(|line| {
        line.contains("(`this.total = ...` in the constructor) replaces this body")
            && !line.contains("prototype")
    }) {
        return Err(format!(
            "constructor guidance names another site: {:?}",
            constructor.missing
        ));
    }
    let step = constructor
        .recommended_next_step
        .as_deref()
        .unwrap_or_default();
    if !step.contains("`this.total = this.total.bind(this)`") || step.contains("prototype") {
        return Err(format!(
            "constructor recommendation names another receiver: {step:?}"
        ));
    }
    Ok(())
}
