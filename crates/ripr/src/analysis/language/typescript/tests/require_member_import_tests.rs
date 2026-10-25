//! A member of a literal `require()` is a named import (#9188). `const f =
//! require("./lib").f` picks one export out of a CommonJS module, so it
//! records the import `const { f } = require("./lib")` records and a call
//! through `f` reaches the owner. A call on the member, a computed key that is
//! not a string literal and a dynamic `require` argument stay unread.

use super::*;

fn imports_of(source: &str) -> Vec<(String, Option<String>, String, bool)> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    extract_imports_from_statements(&parsed.program.body)
        .into_iter()
        .map(|import| {
            (
                import.source,
                import.imported,
                import.local,
                import.namespace,
            )
        })
        .collect()
}

fn named(source: &str, imported: &str, local: &str) -> (String, Option<String>, String, bool) {
    (
        source.to_string(),
        Some(imported.to_string()),
        local.to_string(),
        false,
    )
}

#[test]
fn member_of_a_literal_require_records_a_named_import() {
    assert_eq!(
        imports_of("const stringify = require('./lib').stringify;\n"),
        vec![named("./lib", "stringify", "stringify")]
    );
    assert_eq!(
        imports_of("var s = require('./lib').stringify;\n"),
        vec![named("./lib", "stringify", "s")],
        "a renamed local keeps the exported name"
    );
    assert_eq!(
        imports_of("const s = require('./lib')['stringify'];\n"),
        vec![named("./lib", "stringify", "s")],
        "a string-literal key is the same member"
    );
}

#[test]
fn only_a_plain_member_of_a_literal_require_is_read() {
    for source in [
        "const s = require('./lib').make();\n",
        "const s = require('./lib')[key];\n",
        "const s = require('./lib')?.stringify;\n",
        "const s = require(name).stringify;\n",
        "const s = require('./lib').a.b;\n",
        "const s = other('./lib').stringify;\n",
    ] {
        assert_eq!(imports_of(source), Vec::new(), "{source}");
    }
}

#[test]
fn destructured_and_whole_module_requires_are_unchanged() {
    assert_eq!(
        imports_of("const { stringify } = require('./lib');\n"),
        vec![named("./lib", "stringify", "stringify")]
    );
    assert_eq!(
        imports_of("const lib = require('./lib');\n"),
        vec![(
            "./lib".to_string(),
            Some("default".to_string()),
            "lib".to_string(),
            true
        )]
    );
}

const OWNER: &str = "exports.stringify = function stringify(obj, options) {\n  if (typeof options === 'undefined') {\n    return JSON.stringify(obj);\n  }\n  return JSON.stringify(obj, null, 2);\n};\n";

fn class_for(label: &str, binding: &str, call: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/lib.js"), OWNER)?;
    ts_write_file(
        &root.join("test/lib.test.js"),
        &format!(
            "{binding}\n\ntest('plain', () => {{\n  expect({call}).toBe('{{\"a\":1}}');\n}});\n"
        ),
    )?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(
            "src/lib.js",
            &[(2, "  if (typeof options === 'undefined') {")],
        )],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .next()
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding"))
}

#[test]
fn a_call_through_the_member_binding_reaches_the_owner() -> Result<(), String> {
    // A renamed local, so the owner's own name cannot relate the call.
    let member = class_for(
        "req-member",
        "const s = require('../src/lib').stringify;",
        "s({ a: 1 })",
    )?;
    let destructured = class_for(
        "req-destructured",
        "const { stringify: s } = require('../src/lib');",
        "s({ a: 1 })",
    )?;
    assert_ne!(member, ExposureClass::NoStaticPath, "member form relates");
    assert_eq!(member, destructured, "same program, same reading");
    Ok(())
}

#[test]
fn a_member_of_another_module_or_another_name_does_not_reach_the_owner() -> Result<(), String> {
    let other_module = class_for(
        "req-other-module",
        "const stringify = require('../src/other').stringify;",
        "stringify({ a: 1 })",
    )?;
    assert_eq!(other_module, ExposureClass::NoStaticPath);
    let other_name = class_for(
        "req-other-name",
        "const stringify = require('../src/lib').pretty;",
        "stringify({ a: 1 })",
    )?;
    assert_eq!(other_name, ExposureClass::NoStaticPath);
    Ok(())
}
