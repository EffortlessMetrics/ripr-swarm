//! #16365: TypeScript's CommonJS module syntax. `export = Cart;` compiles to
//! `module.exports = Cart;` and `import Cart = require('./cart')` to `const
//! Cart = require('./cart')`, so each reads like its CommonJS twin. Before,
//! the owner side never took `export =` as the whole export and the test side
//! recorded no import for `import x = require(...)`, so a test calling the
//! changed code read `no_static_path`.

use super::*;

const CLASS_BODY: &str = "class Cart {\n  total(n: number): number {\n    if (n > 50) {\n      return n - 5;\n    }\n    return n;\n  }\n}\n";
const FUNCTION_BODY: &str = "function total(n: number): number {\n  if (n > 50) {\n    return n - 5;\n  }\n  return n;\n}\n";
const CHANGED: &str = "if (n > 50) {";

const CLASS_CALL: &str = "    expect(new Cart().total(50)).toBe(45);";
const FUNCTION_CALL: &str = "    expect(total(50)).toBe(45);";

/// A jest test file: `preamble`, then one `test` whose body is `body`.
fn test_file(preamble: &str, body: &str) -> String {
    format!(
        "{preamble}\n\ndescribe('cart', () => {{\n  test('total', () => {{\n{body}\n  }});\n}});\n"
    )
}

/// The finding for the changed `if` line of `src/cart.ts`.
fn finding(label: &str, owner: &str, test_source: &str) -> Result<Finding, String> {
    let line = owner
        .lines()
        .position(|line| line.trim_start() == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let text = owner
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("{label}: no line {line}"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cart.ts"), owner)?;
    ts_write_file(&root.join("tests/cart.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.ts", &[(line, text)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let finding = result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("total"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `total`"))?;
    // The fixture really is the changed `total`, not another owner.
    assert!(
        finding.evidence.iter().any(|line| line == "owner: total"),
        "{label}: {:?}",
        finding.evidence
    );
    Ok(finding)
}

fn assert_related(label: &str, owner: &str, test_source: &str) -> Result<(), String> {
    let finding = finding(label, owner, test_source)?;
    assert!(
        !finding.related_tests.is_empty(),
        "{label}: the test calls the changed code"
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{label}");
    assert_ne!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

fn assert_unrelated(label: &str, owner: &str, test_source: &str) -> Result<(), String> {
    let finding = finding(label, owner, test_source)?;
    assert!(
        finding.related_tests.is_empty(),
        "{label}: related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

#[test]
fn an_export_equals_class_relates_through_either_require_form() -> Result<(), String> {
    let owner = format!("{CLASS_BODY}export = Cart;\n");
    for (label, preamble) in [
        (
            "eq-class-import-require",
            "import Cart = require('../src/cart');",
        ),
        (
            "eq-class-const-require",
            "const Cart = require('../src/cart');",
        ),
    ] {
        assert_related(label, &owner, &test_file(preamble, CLASS_CALL))?;
    }
    // An ES default import of the class stays unrelated, as for its
    // `module.exports = Cart;` twin (RIPR-SPEC-0243 rule 19).
    assert_unrelated(
        "eq-class-default-import",
        &owner,
        &test_file("import Cart from '../src/cart';", CLASS_CALL),
    )
}

#[test]
fn an_export_equals_function_relates_through_either_import_form() -> Result<(), String> {
    let owner = format!("{FUNCTION_BODY}export = total;\n");
    for (label, preamble) in [
        (
            "eq-fn-import-require",
            "import total = require('../src/cart');",
        ),
        (
            "eq-fn-const-require",
            "const total = require('../src/cart');",
        ),
        ("eq-fn-default-import", "import total from '../src/cart';"),
    ] {
        assert_related(label, &owner, &test_file(preamble, FUNCTION_CALL))?;
    }
    Ok(())
}

#[test]
fn an_import_equals_require_is_a_namespace_binding() -> Result<(), String> {
    // An object whole export, read off the namespace.
    assert_related(
        "eq-object-member",
        &format!("{FUNCTION_BODY}export = {{ total }};\n"),
        &test_file(
            "import lib = require('../src/cart');",
            "    expect(lib.total(50)).toBe(45);",
        ),
    )?;
    // A named ES export, read off the namespace.
    assert_related(
        "import-require-named-class",
        &format!("export {CLASS_BODY}"),
        &test_file(
            "import m = require('../src/cart');",
            "    expect(new m.Cart().total(50)).toBe(45);",
        ),
    )
}

#[test]
fn other_export_equals_shapes_stay_unrelated() -> Result<(), String> {
    let other = "function other(n: number): number {\n  return n;\n}\n";
    for (label, owner, preamble, body) in [
        // Another function is the whole export.
        (
            "eq-other-function",
            format!("{FUNCTION_BODY}{other}export = other;\n"),
            "import other = require('../src/cart');",
            "    expect(other(50)).toBe(50);",
        ),
        // A `module.exports` write beside `export =` leaves the export unknown.
        (
            "eq-with-module-exports",
            format!("{FUNCTION_BODY}{other}export = total;\nmodule.exports = other;\n"),
            "import total = require('../src/cart');",
            FUNCTION_CALL,
        ),
        // The name is written after the declaration.
        (
            "eq-class-rewritten",
            format!(
                "{CLASS_BODY}export = Cart;\n// @ts-ignore\nCart = class {{ total() {{ return 45; }} }};\n"
            ),
            "import Cart = require('../src/cart');",
            CLASS_CALL,
        ),
        // A type-only import loads nothing at runtime.
        (
            "eq-type-only-import",
            format!("{CLASS_BODY}export = Cart;\n"),
            "import type Cart = require('../src/cart');",
            "    const cart: Cart | undefined = undefined;\n    expect(cart).toBeUndefined();",
        ),
        // The text only looks like the import.
        (
            "eq-import-in-comment",
            format!("{FUNCTION_BODY}export = total;\n"),
            "// import total = require('../src/cart');",
            "    expect(1).toBe(1);",
        ),
    ] {
        assert_unrelated(label, &owner, &test_file(preamble, body))?;
    }
    Ok(())
}

#[test]
fn export_equals_target_reads_only_one_identifier_export() {
    let parse = |source: &str| -> Option<String> {
        let allocator = oxc_allocator::Allocator::default();
        let parsed = oxc_parser::Parser::new(
            &allocator,
            source,
            oxc_span::SourceType::ts().with_module(true),
        )
        .parse();
        super::super::export_equals::export_equals_target(&parsed.program).map(str::to_string)
    };
    assert_eq!(
        parse("class Cart {}\nexport = Cart;\n").as_deref(),
        Some("Cart")
    );
    // An inline expression, two assignments, or an ES export beside it.
    assert_eq!(parse("export = class Cart {};\n"), None);
    assert_eq!(
        parse("const a = 1;\nconst b = 2;\nexport = a;\nexport = b;\n"),
        None
    );
    assert_eq!(
        parse("const a = 1;\nexport const b = 2;\nexport = a;\n"),
        None
    );
    assert_eq!(parse("const a = 1;\nexport default a;\n"), None);
}
