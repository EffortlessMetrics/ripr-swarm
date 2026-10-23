//! RIPR-SPEC-0243 rule 19: a CommonJS module whose whole export is a function
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
    assert_eq!(class, ExposureClass::NoStaticPath);
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
    assert_eq!(class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn require_of_a_different_module_is_not_credited() -> Result<(), String> {
    let test = "const t = require('../src/other.js');\n\ntest('triples', () => {\n  expect(t(2)).toBe(6);\n});\n";
    let class = class_for("cjs-other", ANONYMOUS, test)?;
    assert_eq!(class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn assertion_on_an_unrelated_value_is_not_credited() -> Result<(), String> {
    // The changed result is computed but never observed.
    let test = "const t = require('../src/k.js');\n\ntest('triples', () => {\n  t(2);\n  expect(1 + 1).toBe(2);\n});\n";
    let class = class_for("cjs-unobserved", ANONYMOUS, test)?;
    assert_eq!(class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn other_export_shapes_are_not_default_function_exports() {
    for source in [
        "const obj = { a: 1 };\nmodule.exports = obj;\n",
        "function f(x) { return x; }\nmodule.exports = { f };\n",
        "exports.x = function (x) {\n  return x;\n};\n",
        "const triple = (x) => x * 3;\nmodule.exports = triple;\n",
        "class Triple {}\nmodule.exports = Triple;\n",
        "function f(x) { return x; }\nexport = f;\n",
    ] {
        assert!(
            marks(source).iter().all(|(_, _, commonjs)| !commonjs),
            "{source:?}"
        );
    }
}

#[test]
fn crlf_and_non_ascii_sources_keep_the_gate() -> Result<(), String> {
    let owner =
        "// é ü\r\nmodule.exports = function (x) {\r\n  // triple\r\n  return x * 3;\r\n};\r\n";
    let class = class_for("cjs-crlf", owner, &test_with("t", "t(2)"))?;
    assert_eq!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_rebound_let_require_binding_does_not_call_the_owner() -> Result<(), String> {
    let owner = "module.exports = function (x) {\n  // triple\n  return x * 3;\n};\n";
    for (label, prelude) in [
        (
            "cjs-rebound-let",
            "let f = require('../src/k.js');\nf = (x) => x * 3;\n",
        ),
        (
            "cjs-rebound-var",
            "var f = require('../src/k.js');\nf = (x) => x * 3;\n",
        ),
    ] {
        let test = format!("{prelude}test('x', () => {{\n  expect(f(2)).toBe(6);\n}});\n");
        let class = class_for(label, owner, &test)?;
        assert_eq!(class, ExposureClass::NoStaticPath, "{label}");
    }
    // An unwritten `let` binding still reaches the owner.
    let test = "let f = require('../src/k.js');\ntest('x', () => {\n  expect(f(2)).toBe(6);\n});\n";
    assert_eq!(
        class_for("cjs-unwritten-let", owner, test)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_later_whole_slot_overwrite_leaves_the_inline_function_unmarked() {
    for source in [
        "module.exports = function (x) {\n  // triple\n  return x * 3;\n};\nmodule.exports = 0;\n",
        "module.exports = function (x) {\n  // triple\n  return x * 3;\n};\nmodule . exports = 0;\n",
        "module.exports = function (x) {\n  // triple\n  return x * 3;\n};\nmodule\n  .exports = 0;\n",
    ] {
        assert!(
            marks(source).iter().all(|(_, _, marked)| !marked),
            "{source}"
        );
    }
}

#[test]
fn a_spaced_overwrite_leaves_the_declared_function_unmarked() {
    let source = "function triple(x) {\n  // triple\n  return x * 3;\n}\n\nmodule.exports = triple;\nmodule . exports = 0;\n";
    assert!(marks(source).iter().all(|(_, _, marked)| !marked));
}

#[test]
fn a_reassigned_require_binding_is_not_credited() -> Result<(), String> {
    for test_source in [
        "let f = require('../src/k.js');\nf = (x) => x * 3;\n\ntest('triples', () => {\n  expect(f(2)).toBe(6);\n});\n",
        "var f = require('../src/k.js');\nf++;\n\ntest('triples', () => {\n  expect(f(2)).toBe(6);\n});\n",
    ] {
        let class = class_for("cjs-reassigned-binding", ANONYMOUS, test_source)?;
        assert_eq!(class, ExposureClass::NoStaticPath, "{test_source}");
    }
    Ok(())
}

/// Tails that make `module.exports` unknowable; each is valid JavaScript on its
/// own, so a refusal cannot come from a parse failure.
const SHAPES: &[(&str, &str)] = &[
    (
        "second",
        "function other(x) { return x; }\nmodule.exports = other;\n",
    ),
    ("object", "module.exports = { a: 1 };\n"),
    ("zero", "module.exports = 0;\n"),
    (
        "nested",
        "function other(x) { return x; }\nif (process.env.C) { module.exports = other; }\n",
    ),
    ("eval", "eval('module.exports = other');\n"),
    ("with", "with (ctx) { run(); }\n"),
    (
        "newline",
        "function other(x) { return x; }\nmodule\n  .exports = other;\n",
    ),
    ("spaced", "module . exports = 0;\n"),
    ("optional-read", "const e = module?.exports;\n"),
    ("compound", "module.exports &&= null;\n"),
    ("plus", "module.exports += 1;\n"),
    ("update", "module.exports++;\n"),
    ("delete", "delete module.exports;\n"),
    ("alias", "const m = module;\nm.exports = null;\n"),
    ("assign", "Object.assign(module, { exports: null });\n"),
    (
        "define",
        "Object.defineProperty(module, 'exports', { value: null });\n",
    ),
    ("reflect", "Reflect.set(module, 'exports', 0);\n"),
    ("bracket", "module['exports'] = null;\n"),
    ("bracket-spaced", "module [ 'exports' ] = 0;\n"),
    ("bracket-newline", "module\n['exports'] = 0;\n"),
    ("vtab", "module\u{b}.exports = 0;\n"),
    ("nbsp", "module\u{a0}.exports = 0;\n"),
    ("line-sep", "module\u{2028}.exports = 0;\n"),
    ("bom", "module\u{feff}.exports = 0;\n"),
    ("paren", "(module.exports) = 0;\n"),
    ("array", "[module.exports] = [0];\n"),
    ("destructure", "({ a: module.exports } = { a: 0 });\n"),
    ("for-of", "for (module.exports of []);\n"),
    ("require-main", "require.main.exports = 0;\n"),
    ("require-cache", "require.cache[__filename].exports = 0;\n"),
    ("main-module", "process.mainModule.exports = 0;\n"),
    (
        "umd-guard",
        "if (typeof module !== 'undefined') { run(); }\n",
    ),
    ("main-check", "if (require.main === module) { run(); }\n"),
    (
        "postfix-division",
        "let i = 0; i++ / 2; module.exports = 0;\n",
    ),
    ("postfix-division-eval", "let i = 0; i-- / 2; eval('x');\n"),
    (
        "postfix-division-assign",
        "let i = 0; i++ / 2; Object.assign(module, { exports: 0 });\n",
    ),
];

fn exposed_with_control(label: &str, owner: &str) -> Result<(), String> {
    let call = test_with("t", "t(2)");
    let control = class_for(&format!("{label}-control"), owner, &call)?;
    assert_eq!(
        control,
        ExposureClass::Exposed,
        "control {label}: {owner:?}"
    );
    Ok(())
}

fn refused(label: &str, owner: &str) -> Result<(), String> {
    // A reassigned name drops the owner entirely (no finding at all), which
    // is also a refusal; anything else must read `no_static_path`.
    match class_for(label, owner, &test_with("t", "t(2)")) {
        Ok(class) => assert_eq!(class, ExposureClass::NoStaticPath, "{label}: {owner:?}"),
        Err(error) if error.contains("expected a finding") => {}
        Err(error) => return Err(error),
    }
    assert!(
        marks(owner).iter().all(|(_, _, commonjs)| !commonjs),
        "{label}: {owner:?}"
    );
    Ok(())
}

#[test]
fn both_forms_share_one_file_level_gate() -> Result<(), String> {
    for (form, head) in [("inline", ANONYMOUS), ("declared", DECLARED)] {
        exposed_with_control(form, head)?;
        for (name, tail) in SHAPES {
            refused(&format!("cjs-{form}-{name}"), &format!("{head}{tail}"))?;
        }
    }
    Ok(())
}

#[test]
fn every_gate_tail_is_valid_javascript() -> Result<(), String> {
    // A tail that fails to parse would make the refusal vacuous: the tail
    // alone, with a harmless owner, must still analyze as an owner file.
    for (name, tail) in SHAPES {
        let source = format!("function keep(x) {{ return x; }}\n{tail}");
        let owners = extract_owners(Path::new("src/k.js"), &source);
        assert!(
            owners.iter().any(|owner| owner.name == "keep"),
            "tail {name} does not parse: {tail:?}"
        );
    }
    Ok(())
}

#[test]
fn declared_name_writes_and_rebinds_refuse_the_declared_form() -> Result<(), String> {
    let non_ascii = "function café(x) {\n  // triple\n  return x * 3;\n}\nmodule.exports = café;\n";
    exposed_with_control("cjs-declared-non-ascii", non_ascii)?;
    refused(
        "cjs-declared-non-ascii-write",
        &format!("{non_ascii}café = null;\n"),
    )?;
    for (name, tail) in [
        ("for-of-array", "for ([triple] of [[0]]);\n"),
        ("for-of-object", "for ({ triple } of []);\n"),
        ("for-in-array", "for ([triple] in {});\n"),
        ("grouped", "(triple) = null;\n"),
        ("annex-b", "{ function triple() { return 4; } }\n"),
    ] {
        refused(
            &format!("cjs-declared-{name}"),
            &format!("{DECLARED}{tail}"),
        )?;
    }
    Ok(())
}

#[test]
fn a_string_or_comment_mentioning_module_keeps_the_gate_open() -> Result<(), String> {
    for tail in [
        "// module.exports = other\n",
        "const s = 'module.exports = other';\n",
        "/* eval(x) */\n",
        "module.exports.extra = 1;\n",
    ] {
        exposed_with_control("cjs-inert-tail", &format!("{ANONYMOUS}{tail}"))?;
        exposed_with_control("cjs-inert-tail-declared", &format!("{DECLARED}{tail}"))?;
    }
    Ok(())
}
