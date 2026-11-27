//! #16369: a CommonJS module's private helper, reached only through the
//! function the module exports.
//!
//! `export function other(n) { return total(n); }` relates a test of `other`
//! to the changed helper `total` through the same-module entry graph. Before,
//! only ES export statements were entries, so each CommonJS twin read
//! `no_static_path`. The ESM row is the reference verdict for every twin.

use super::*;

const TOTAL: &str =
    "function total(n) {\n  if (n > 50) {\n    return n - 5;\n  }\n  return n;\n}\n";
const OTHER: &str = "function other(n) {\n  return total(n);\n}\n";
const NOOP: &str = "function noop() {}\n";
const CHANGED: &str = "  if (n > 50) {";
const CALL: &str = "    expect(other(50)).toBe(45);";

/// A jest test file: `preamble`, then one `test` whose body is `body`.
fn test_file(preamble: &str, body: &str) -> String {
    format!(
        "{preamble}\n\ndescribe('cart', () => {{\n  test('total', () => {{\n{body}\n  }});\n}});\n"
    )
}

/// The finding for the changed line of the private helper `total` in
/// `src/cart.<ext>`.
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
    );
    let _ = std::fs::remove_dir_all(&root);
    let finding = result?
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
    // The fixture really is the changed helper, not its caller.
    assert!(
        finding.evidence.iter().any(|line| line == "owner: total"),
        "{label}: {:?}",
        finding.evidence
    );
    Ok(finding)
}

fn assert_reached(label: &str, ext: &str, owner: &str, test_source: &str) -> Result<(), String> {
    let finding = finding(label, ext, owner, test_source)?;
    assert!(
        finding.related_tests.iter().any(
            |test| test.relation_reason == Some(crate::domain::RelationReason::HelperOwnerCall)
        ),
        "{label}: {:?}",
        finding.related_tests
    );
    // Reach through an entry is never promoted past unknown.
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{label}");
    Ok(())
}

fn assert_unreached(label: &str, ext: &str, owner: &str, test_source: &str) -> Result<(), String> {
    let finding = finding(label, ext, owner, test_source)?;
    assert!(
        finding.related_tests.is_empty(),
        "{label}: related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

#[test]
fn a_commonjs_export_reaches_the_helper_it_calls_like_its_esm_twin() -> Result<(), String> {
    let named = test_file("const { other } = require('../src/cart');", CALL);
    let whole = test_file(
        "const calc = require('../src/cart');",
        "    expect(calc(50)).toBe(45);",
    );
    for (label, ext, owner, test) in [
        (
            "helper-esm-reference",
            "js",
            format!("{TOTAL}export {OTHER}"),
            test_file("import { other } from '../src/cart';", CALL),
        ),
        (
            "helper-object-export",
            "js",
            format!("{TOTAL}{OTHER}module.exports = {{ other }};\n"),
            named.clone(),
        ),
        (
            "helper-object-export-namespace",
            "js",
            format!("{TOTAL}{OTHER}module.exports = {{ other }};\n"),
            test_file(
                "const lib = require('../src/cart');",
                "    expect(lib.other(50)).toBe(45);",
            ),
        ),
        (
            "helper-renamed-key",
            "js",
            format!("{TOTAL}{OTHER}module.exports = {{ run: other }};\n"),
            test_file(
                "const { run } = require('../src/cart');",
                "    expect(run(50)).toBe(45);",
            ),
        ),
        (
            "helper-exports-property",
            "js",
            format!("{TOTAL}{OTHER}exports.other = other;\n"),
            named.clone(),
        ),
        (
            "helper-property-after-whole",
            "js",
            format!("{TOTAL}{OTHER}{NOOP}module.exports = noop;\nmodule.exports.other = other;\n"),
            named.clone(),
        ),
        (
            "helper-whole-declared",
            "js",
            format!("{TOTAL}{OTHER}module.exports = other;\n"),
            whole.clone(),
        ),
        (
            "helper-whole-inline-function",
            "js",
            format!("{TOTAL}module.exports = function other(n) {{\n  return total(n);\n}};\n"),
            whole.clone(),
        ),
        (
            "helper-whole-inline-arrow",
            "js",
            format!("{TOTAL}module.exports = (n) => total(n);\n"),
            whole,
        ),
        (
            "helper-ts-export-equals",
            "ts",
            format!("{TOTAL}{OTHER}export = other;\n").replace("(n)", "(n: number)"),
            test_file("import other = require('../src/cart');", CALL),
        ),
    ] {
        assert_reached(label, ext, &owner, &test)?;
    }
    Ok(())
}

#[test]
fn an_unknown_commonjs_export_shape_reaches_nothing() -> Result<(), String> {
    let named = test_file("const { other } = require('../src/cart');", CALL);
    for (label, owner, test) in [
        // The export never calls the changed helper.
        (
            "helper-not-called",
            format!(
                "{TOTAL}function other(n) {{\n  return n;\n}}\nmodule.exports = {{ other }};\n"
            ),
            test_file(
                "const { other } = require('../src/cart');",
                "    expect(other(50)).toBe(50);",
            ),
        ),
        // A local of the same name shadows the helper inside the export.
        (
            "helper-shadowed",
            format!(
                "{TOTAL}function other(n) {{\n  const total = (x) => x;\n  return total(n);\n}}\nmodule.exports = {{ other }};\n"
            ),
            named.clone(),
        ),
        // The object exports `run`, not `other`.
        (
            "helper-old-key-name",
            format!("{TOTAL}{OTHER}module.exports = {{ run: other }};\n"),
            named.clone(),
        ),
        // Two whole writes, or one the top level does not see.
        (
            "helper-two-whole-writes",
            format!(
                "{TOTAL}{OTHER}{NOOP}module.exports = {{ other }};\nmodule.exports = {{ noop }};\n"
            ),
            named.clone(),
        ),
        (
            "helper-nested-whole-write",
            format!(
                "{TOTAL}{OTHER}{NOOP}module.exports = {{ other }};\nif (process.env.X) {{\n  module.exports = {{ noop }};\n}}\n"
            ),
            named.clone(),
        ),
        // `exports` is detached once `module.exports` is replaced, and a
        // property written before the replacement is gone.
        (
            "helper-detached-exports",
            format!("{TOTAL}{OTHER}{NOOP}module.exports = {{ noop }};\nexports.other = other;\n"),
            named.clone(),
        ),
        (
            "helper-property-before-whole",
            format!(
                "{TOTAL}{OTHER}{NOOP}module.exports.other = other;\nmodule.exports = {{ noop }};\n"
            ),
            named.clone(),
        ),
        // The whole export is not provably the calling function.
        (
            "helper-whole-reassigned",
            format!(
                "{TOTAL}let other = function (n) {{\n  return total(n);\n}};\nmodule.exports = other;\nother = (n) => n;\n"
            ),
            test_file("const other = require('../src/cart');", CALL),
        ),
        // A named binding reads a property the whole function lacks.
        (
            "helper-named-on-whole-function",
            format!("{TOTAL}{OTHER}module.exports = other;\n"),
            named,
        ),
    ] {
        assert_unreached(label, "js", &owner, &test)?;
    }
    Ok(())
}

#[test]
fn commonjs_exports_reads_only_a_known_export_shape() {
    let read = |source: &str| -> Vec<(String, Option<String>)> {
        let allocator = oxc_allocator::Allocator::default();
        let parsed = oxc_parser::Parser::new(
            &allocator,
            source,
            oxc_span::SourceType::default().with_module(true),
        )
        .parse();
        super::super::commonjs_entries::commonjs_exports(&parsed.program.body, source)
            .0
            .into_iter()
            .map(|export| (export.local.to_string(), export.property))
            .collect()
    };
    let named = |local: &str, property: &str| (local.to_string(), Some(property.to_string()));
    assert_eq!(
        read("module.exports = { a, b: c, d() {}, [e]: f, ...g };\n"),
        [named("a", "a"), named("c", "b")]
    );
    assert_eq!(
        read("exports.a = a;\nmodule.exports.b = b;\n"),
        [named("a", "a"), named("b", "b")]
    );
    assert_eq!(read("module.exports = a;\n"), [("a".to_string(), None)]);
    // A write inside a string or a comment is not a write.
    assert_eq!(
        read("module.exports = { a };\n// module.exports = {};\nconst s = 'module.exports = b';\n"),
        [named("a", "a")]
    );
    // An ES export beside it, or a compound or nested write, leaves the
    // shape unknown.
    assert!(read("module.exports = { a };\nexport const b = 1;\n").is_empty());
    assert!(read("module.exports = { a };\nfunction f() { module.exports = {}; }\n").is_empty());
    assert!(read("module.exports = { a };\nmodule.exports = { b };\n").is_empty());
}
