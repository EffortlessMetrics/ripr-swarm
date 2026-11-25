//! A class method that a later same-key member overwrites (#16261,
//! RIPR-SPEC-0243 rule 26).
//!
//! `class Cart { total() {...} total() {...} }` leaves only the second
//! `total` on the prototype, so a test calling `new Cart().total()` never runs
//! the first body. Before #16261 both bodies were related to that test. Now a
//! change to the earlier body is still reported but relates to no test, and
//! the later body keeps its relation, for a declared class exported through
//! `module.exports = Cart;`, an ES default class and an inline
//! `module.exports = class Cart`.
//!
//! A later quoted or bracketed string key (`"total"()`, `['total']()`) names
//! the same property and overwrites too. The finding on an overwritten body
//! says a later definition replaces it, instead of the usual advice to add a
//! direct receiver call, which would run the later body.
//!
//! Controls that keep their credit: a getter and setter of one key (one
//! accessor property), a static and an instance member of one name
//! (different objects), and TypeScript overload signatures (no runtime
//! definition).
//!
//! Every case runs `TypeScriptAdapter::analyze_diff` on a generated jest
//! repository and reads the finding on the changed line.

use super::*;

const JEST_PACKAGE: &str =
    "{\"name\":\"demo\",\"version\":\"1.0.0\",\"devDependencies\":{\"jest\":\"^29.0.0\"}}\n";

/// The earlier, overwritten `total` (its `if` line is the changed line).
const EARLIER: &str =
    "  total(n) {\n    if (n > 50) {\n      return n - 5;\n    }\n    return n;\n  }\n";
const EARLIER_LINE: &str = "    if (n > 50) {";
/// The later, live `total`.
const LATER: &str =
    "  total(n) {\n    if (n >= 100) {\n      return n - 10;\n    }\n    return n;\n  }\n";
const LATER_LINE: &str = "    if (n >= 100) {";

const REQUIRE: &str = "const Cart = require('../src/cart');";
const IMPORT: &str = "import Cart from '../src/cart';";
const CALL: &str = "    expect(new Cart().total(100)).toBe(90);\n";

/// The finding `analyze_diff` reports on the line of `owner_path` that reads
/// `changed`, with `test` as `tests/cart.test.js`.
fn diff_finding(
    label: &str,
    owner_path: &str,
    owner_text: &str,
    changed: &str,
    test: &str,
) -> Result<Finding, String> {
    let line = owner_text
        .lines()
        .position(|text| text == changed)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("package.json"), JEST_PACKAGE)?;
    ts_write_file(&root.join(owner_path), owner_text)?;
    ts_write_file(&root.join("tests/cart.test.js"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(owner_path, &[(line, changed)])],
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

/// The earlier body relates to no test and the later one keeps its relation.
fn assert_only_later_related(
    label: &str,
    owner_path: &str,
    owner_text: &str,
    preamble: &str,
) -> Result<(), String> {
    let test = jest_test(preamble, CALL);
    let earlier = diff_finding(
        &format!("{label}-earlier"),
        owner_path,
        owner_text,
        EARLIER_LINE,
        &test,
    )?;
    expect_related(&format!("{label}: overwritten body"), &earlier, false)?;
    let later = diff_finding(
        &format!("{label}-later"),
        owner_path,
        owner_text,
        LATER_LINE,
        &test,
    )?;
    expect_related(&format!("{label}: live body"), &later, true)
}

#[test]
fn an_overwritten_method_of_a_declared_class_relates_to_no_test() -> Result<(), String> {
    assert_only_later_related(
        "overwritten-declared",
        "src/cart.js",
        &format!("class Cart {{\n{EARLIER}{LATER}}}\nmodule.exports = Cart;\n"),
        REQUIRE,
    )
}

#[test]
fn an_overwritten_method_of_an_es_default_class_relates_to_no_test() -> Result<(), String> {
    assert_only_later_related(
        "overwritten-esm-default",
        "src/cart.js",
        &format!("export default class Cart {{\n{EARLIER}{LATER}}}\n"),
        IMPORT,
    )
}

#[test]
fn an_overwritten_method_of_an_inline_commonjs_class_relates_to_no_test() -> Result<(), String> {
    assert_only_later_related(
        "overwritten-inline",
        "src/cart.js",
        &format!("module.exports = class Cart {{\n{EARLIER}{LATER}}};\n"),
        REQUIRE,
    )
}

#[test]
fn an_overwritten_body_is_still_an_owner_marked_overwritten() -> Result<(), String> {
    let text = format!("class Cart {{\n{EARLIER}{LATER}}}\nmodule.exports = Cart;\n");
    let owners = extract_owners(Path::new("src/cart.js"), &text);
    let kinds: Vec<TypeScriptMethodKind> = owners
        .iter()
        .filter(|owner| owner.name == "total")
        .map(|owner| owner.method_kind)
        .collect();
    if kinds
        != [
            TypeScriptMethodKind::Overwritten,
            TypeScriptMethodKind::Ordinary,
        ]
    {
        return Err(format!(
            "expected the earlier total overwritten, got {kinds:?}"
        ));
    }
    Ok(())
}

#[test]
fn a_getter_and_setter_of_one_key_keep_the_getter_relation() -> Result<(), String> {
    let text = "export class Cart {\n  constructor() {\n    this.n = 100;\n  }\n  get total() {\n    if (this.n >= 100) {\n      return this.n - 10;\n    }\n    return this.n;\n  }\n  set total(value) {\n    this.n = value;\n  }\n}\n";
    let finding = diff_finding(
        "accessor-pair",
        "src/cart.js",
        text,
        "    if (this.n >= 100) {",
        &jest_test(
            "import { Cart } from '../src/cart';",
            "    const cart = new Cart();\n    expect(cart.total).toBe(90);\n",
        ),
    )?;
    expect_related("getter beside its setter", &finding, true)
}

#[test]
fn a_static_member_does_not_overwrite_the_instance_method() -> Result<(), String> {
    let text = format!(
        "class Cart {{\n{LATER}  static total() {{\n    return 0;\n  }}\n}}\nmodule.exports = Cart;\n"
    );
    let finding = diff_finding(
        "static-beside-instance",
        "src/cart.js",
        &text,
        LATER_LINE,
        &jest_test(REQUIRE, CALL),
    )?;
    expect_related("instance total beside a static total", &finding, true)
}

#[test]
fn overload_signatures_do_not_overwrite_the_implementation() -> Result<(), String> {
    let text = "export default class Cart {\n  total(n: number): number;\n  total(n: string): number;\n  total(n: any): number {\n    if (n >= 100) {\n      return n - 10;\n    }\n    return n;\n  }\n}\n";
    let finding = diff_finding(
        "overloads",
        "src/cart.ts",
        text,
        LATER_LINE,
        &jest_test(IMPORT, CALL),
    )?;
    expect_related("implementation after overload signatures", &finding, true)
}

#[test]
fn a_quoted_later_key_overwrites_the_earlier_method() -> Result<(), String> {
    // `"total"()` and `['total']()` define the same property as `total()`.
    for (label, later_head) in [
        ("overwritten-quoted", "  \"total\"(n) {"),
        ("overwritten-bracketed", "  ['total'](n) {"),
    ] {
        let later = LATER.replacen("  total(n) {", later_head, 1);
        let text = format!("class Cart {{\n{EARLIER}{later}}}\nmodule.exports = Cart;\n");
        let earlier = diff_finding(
            label,
            "src/cart.js",
            &text,
            EARLIER_LINE,
            &jest_test(REQUIRE, CALL),
        )?;
        expect_related(&format!("{label}: overwritten body"), &earlier, false)?;
    }
    Ok(())
}

#[test]
fn an_overwritten_body_points_at_the_definition_that_remains() -> Result<(), String> {
    let finding = diff_finding(
        "overwritten-guidance",
        "src/cart.js",
        &format!("class Cart {{\n{EARLIER}{LATER}}}\nmodule.exports = Cart;\n"),
        EARLIER_LINE,
        &jest_test(REQUIRE, CALL),
    )?;
    if finding.class != ExposureClass::NoStaticPath {
        return Err(format!("expected no_static_path, got {:?}", finding.class));
    }
    if !finding
        .missing
        .iter()
        .any(|line| line.contains("defines `total` again and replaces this body"))
    {
        return Err(format!("missing names no overwrite: {:?}", finding.missing));
    }
    let step = finding.recommended_next_step.as_deref().unwrap_or_default();
    if !step.contains("a later definition of `total` in the same class replaces this one")
        || step.contains("new ClassName")
    {
        return Err(format!(
            "recommendation does not name the overwrite: {step:?}"
        ));
    }
    Ok(())
}
