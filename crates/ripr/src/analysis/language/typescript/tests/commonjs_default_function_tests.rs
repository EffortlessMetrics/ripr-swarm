//! RIPR-SPEC-0243 rule 15: a CommonJS module whose whole export is a function
//! (`module.exports = function ...` or `module.exports = name;`) IS that
//! function for every test that `require()`s it, so a call through the test's
//! binding reaches the owner, as the ES `export default` twin already does.
//! Real-package origin: minimist (`module.exports = function (args, opts)`),
//! rfdc and fast-safe-stringify (`module.exports = name`), all read
//! `no_static_path` while their own suites failed every mutant of the line.

use super::*;

const CHANGED: (usize, &str) = (3, "  return x * 3;");

/// Analyze `src/k.js` (`owner`) with line 3 changed, against one test file;
/// returns the class of the finding on the changed line.
fn class_for(label: &str, owner: &str, test_source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/k.js"), owner)?;
    ts_write_file(&root.join("test/k.test.js"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/k.js", &[CHANGED])],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .next()
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding"))
}

const ANONYMOUS: &str = "module.exports = function (x) {\n  // triple\n  return x * 3;\n};\n";
const NAMED_FUNCTION: &str =
    "module.exports = function triple(x) {\n  // triple\n  return x * 3;\n};\n";
const DECLARED: &str =
    "function triple(x) {\n  // triple\n  return x * 3;\n}\n\nmodule.exports = triple;\n";

fn test_with(binding: &str, call: &str) -> String {
    format!(
        "const {binding} = require('../src/k.js');\n\ntest('triples', () => {{\n  expect({call}).toBe(6);\n}});\n"
    )
}

fn marks(source: &str) -> Vec<(String, bool, bool)> {
    extract_owners(Path::new("src/k.js"), source)
        .into_iter()
        .map(|owner| {
            (
                owner.name,
                owner.exported_as_default,
                owner.commonjs_default_export,
            )
        })
        .collect()
}

#[test]
fn anonymous_function_export_is_called_through_any_require_binding() -> Result<(), String> {
    let class = class_for("cjs-anon", ANONYMOUS, &test_with("triple", "triple(2)"))?;
    assert_eq!(class, ExposureClass::Exposed);
    let renamed = class_for("cjs-anon-rename", ANONYMOUS, &test_with("t", "t(2)"))?;
    assert_eq!(renamed, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn named_function_export_is_called_through_any_require_binding() -> Result<(), String> {
    let class = class_for(
        "cjs-named",
        NAMED_FUNCTION,
        &test_with("triple", "triple(2)"),
    )?;
    assert_eq!(class, ExposureClass::Exposed);
    let renamed = class_for("cjs-named-rename", NAMED_FUNCTION, &test_with("t", "t(2)"))?;
    assert_eq!(renamed, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn declared_function_assigned_to_module_exports_is_the_default_export() -> Result<(), String> {
    assert_eq!(
        marks(DECLARED),
        vec![("triple".to_string(), true, true)],
        "module.exports = triple marks the declaration"
    );
    let class = class_for("cjs-declared", DECLARED, &test_with("t", "t(2)"))?;
    assert_eq!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn es_default_export_required_from_commonjs_is_a_namespace_not_a_call() -> Result<(), String> {
    // `require()` of an ES module returns its namespace object: `t(2)` throws
    // at runtime, so the call must not reach the owner.
    let es_owner = "export default function (x) {\n  // triple\n  return x * 3;\n}\n";
    let class = class_for("cjs-es-owner", es_owner, &test_with("t", "t(2)"))?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn second_module_exports_assignment_leaves_the_declaration_unmarked() {
    let source = "function triple(x) {\n  return x * 3;\n}\nfunction other(x) {\n  return x;\n}\n\nmodule.exports = triple;\nmodule.exports = other;\n";
    assert_eq!(
        marks(source),
        vec![
            ("triple".to_string(), false, false),
            ("other".to_string(), false, false),
        ],
        "two assignments make the live value ambiguous"
    );
    let conditional = "function triple(x) {\n  return x * 3;\n}\nif (process.env.X) {\n  module.exports = triple;\n}\n";
    assert_eq!(
        marks(conditional),
        vec![("triple".to_string(), false, false)],
        "a nested assignment is not a top-level default export"
    );
}

#[test]
fn rebound_function_declaration_is_not_the_default_export() {
    let source = "function triple(x) {\n  return x * 3;\n}\ntriple = wrap(triple);\nmodule.exports = triple;\n";
    assert!(
        marks(source).iter().all(|(_, _, commonjs)| !commonjs),
        "a later write changes what module.exports holds"
    );
    let twice = "function triple(x) {\n  return x * 3;\n}\nfunction triple(x) {\n  return x * 4;\n}\nmodule.exports = triple;\n";
    assert!(
        marks(twice).iter().all(|(_, _, commonjs)| !commonjs),
        "a second declaration of the name is ambiguous"
    );
}

#[test]
fn property_assignments_on_the_exported_function_keep_the_default_export() {
    let source = "function triple(x) {\n  return x * 3;\n}\ntriple.default = triple;\ntriple.stable = triple;\nmodule.exports = triple;\n";
    assert_eq!(marks(source), vec![("triple".to_string(), true, true)]);
}

#[test]
fn body_local_shadow_of_the_require_binding_is_not_credited() -> Result<(), String> {
    let test = "const t = require('../src/k.js');\n\ntest('triples', () => {\n  const t = (x) => x * 3;\n  expect(t(2)).toBe(6);\n});\n";
    let class = class_for("cjs-shadow", ANONYMOUS, test)?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn require_of_a_different_module_is_not_credited() -> Result<(), String> {
    let test = "const t = require('../src/other.js');\n\ntest('triples', () => {\n  expect(t(2)).toBe(6);\n});\n";
    let class = class_for("cjs-other", ANONYMOUS, test)?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn assertion_on_an_unrelated_value_is_not_credited() -> Result<(), String> {
    // The changed result is computed but never observed.
    let test = "const t = require('../src/k.js');\n\ntest('triples', () => {\n  t(2);\n  expect(1 + 1).toBe(2);\n});\n";
    let class = class_for("cjs-unobserved", ANONYMOUS, test)?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}
