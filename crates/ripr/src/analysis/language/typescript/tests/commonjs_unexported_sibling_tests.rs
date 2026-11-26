//! #16366: a function beside a CommonJS module's one-identifier whole export.
//!
//! After `module.exports = other;` (or TypeScript `export = other;`),
//! `require()` hands back `other` itself, so `const { total } =
//! require(...)`, `require(...).total` and `lib.total` read a property of
//! `other`, which is `undefined` unless the file adds it. Before, such a
//! binding of the unexported sibling `total` related as `exposed`. A file that
//! may add the property (`module.exports.total = total`, `other.total =
//! total`, `Object.assign(other, { total })`) keeps the relation, as does an
//! object whole export.

use super::*;

const TOTAL: &str =
    "function total(n) {\n  if (n > 50) {\n    return n - 5;\n  }\n  return n;\n}\n";
const OTHER: &str = "function other(n) {\n  return n;\n}\n";
const CHANGED: &str = "  if (n > 50) {";
const CALL: &str = "    expect(total(50)).toBe(45);";

/// A jest test file: `preamble`, then one `test` whose body is `body`.
fn test_file(preamble: &str, body: &str) -> String {
    format!(
        "{preamble}\n\ndescribe('cart', () => {{\n  test('total', () => {{\n{body}\n  }});\n}});\n"
    )
}

/// The finding for the changed line of `total` in `src/cart.<ext>`.
fn finding(label: &str, ext: &str, owner: &str, test_source: &str) -> Result<Finding, String> {
    let line = owner
        .lines()
        .position(|line| line == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let owner_path = format!("src/cart.{ext}");
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join(&owner_path), owner)?;
    ts_write_file(&root.join(format!("tests/cart.test.{ext}")), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(&owner_path, &[(line, CHANGED)])],
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
    // The fixture really is the changed `total`, not the whole export.
    assert!(
        finding.evidence.iter().any(|line| line == "owner: total"),
        "{label}: {:?}",
        finding.evidence
    );
    Ok(finding)
}

#[test]
fn a_binding_of_a_sibling_of_the_whole_export_stays_unrelated() -> Result<(), String> {
    let function_export = format!("{TOTAL}{OTHER}module.exports = other;\n");
    let class_export = format!("{TOTAL}class Cart {{}}\nmodule.exports = Cart;\n");
    for (label, ext, owner, preamble, body) in [
        (
            "sibling-named-require",
            "js",
            function_export.clone(),
            "const { total } = require('../src/cart');",
            CALL,
        ),
        (
            "sibling-member-require",
            "js",
            function_export.clone(),
            "const total = require('../src/cart').total;",
            CALL,
        ),
        (
            "sibling-namespace-member",
            "js",
            function_export.clone(),
            "const lib = require('../src/cart');",
            "    expect(lib.total(50)).toBe(45);",
        ),
        (
            "sibling-of-a-class-export",
            "js",
            class_export,
            "const { total } = require('../src/cart');",
            CALL,
        ),
        (
            "sibling-of-ts-export-equals",
            "ts",
            format!("{TOTAL}{OTHER}export = other;\n").replace("(n)", "(n: number)"),
            "const { total } = require('../src/cart');",
            CALL,
        ),
    ] {
        let finding = finding(label, ext, &owner, &test_file(preamble, body))?;
        assert!(
            finding.related_tests.is_empty(),
            "{label}: related: {:?}",
            finding.related_tests
        );
        assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    }
    Ok(())
}

#[test]
fn a_sibling_the_file_adds_as_a_property_keeps_the_relation() -> Result<(), String> {
    for (label, owner) in [
        (
            "property-on-module-exports",
            format!("{TOTAL}{OTHER}module.exports = other;\nmodule.exports.total = total;\n"),
        ),
        (
            "property-on-the-export",
            format!("{TOTAL}{OTHER}other.total = total;\nmodule.exports = other;\n"),
        ),
        (
            "object-assign-onto-the-export",
            format!("{TOTAL}{OTHER}Object.assign(other, {{ total }});\nmodule.exports = other;\n"),
        ),
        (
            "object-whole-export",
            format!("{TOTAL}{OTHER}module.exports = {{ total, other }};\n"),
        ),
    ] {
        let finding = finding(
            label,
            "js",
            &owner,
            &test_file("const { total } = require('../src/cart');", CALL),
        )?;
        assert!(
            !finding.related_tests.is_empty(),
            "{label}: the binding reads the added property"
        );
        assert_eq!(finding.ripr.reach.state, StageState::Yes, "{label}");
        assert_ne!(finding.class, ExposureClass::NoStaticPath, "{label}");
    }
    Ok(())
}

#[test]
fn only_siblings_of_a_one_identifier_whole_export_are_marked() {
    let marked = |source: &str| -> Vec<String> {
        extract_owners(Path::new("src/cart.js"), source)
            .into_iter()
            .filter(|owner| owner.commonjs_whole_function)
            .map(|owner| owner.name)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    // The export itself and its sibling.
    assert_eq!(
        marked(&format!("{TOTAL}{OTHER}module.exports = other;\n")),
        ["other", "total"]
    );
    // A merged second declaration of the export name, a named export or an
    // ES export statement leaves the sibling unmarked.
    assert_eq!(
        marked(&format!(
            "{TOTAL}{OTHER}exports.total = total;\nmodule.exports = other;\n"
        )),
        ["other"]
    );
    assert!(marked(&format!("{TOTAL}{OTHER}module.exports = {{ other }};\n")).is_empty());
    assert_eq!(
        marked(&format!("export {TOTAL}{OTHER}module.exports = other;\n")),
        ["other"]
    );
}
