//! RIPR-SPEC-0243 rule 1: a CommonJS runner bound with `var` or `let` is as
//! immutable as a `const` one when the file never writes the binding after its
//! initializer. Pre-ES2015 packages (qs, minimist, fastq, object-inspect)
//! write `var test = require('tape');` and read `no_static_path` for every
//! owner their own suites fail on (#9182).

use super::*;

const OWNER: &str = "exports.triple = function triple(x) {\n  return x * 3;\n};\n";
const CHANGED: (usize, &str) = (2, "  return x * 3;");

fn class_for(label: &str, test_source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/k.js"), OWNER)?;
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

fn tape_test(prelude: &str, tail: &str) -> String {
    format!(
        "{prelude}\nconst {{ triple }} = require('../src/k.js');\n\ntest('triples', function (t) {{\n  t.equal(triple(2), 6);\n  t.end();\n}});\n{tail}"
    )
}

#[test]
fn unwritten_var_and_let_runner_bindings_register() -> Result<(), String> {
    for prelude in [
        "const test = require('tape');",
        "var test = require('tape');",
        "let test = require('tape');",
        "var other = 1, test = require('tape');",
    ] {
        let class = class_for("var-runner", &tape_test(prelude, ""))?;
        assert_eq!(class, ExposureClass::Exposed, "{prelude}");
    }
    Ok(())
}

#[test]
fn written_var_runner_bindings_stay_uncredited() -> Result<(), String> {
    for (prelude, tail) in [
        ("var test = require('tape');", "test = function () {};"),
        ("var test = require('tape');", "test = require('other');"),
        ("var test = require('tape');", "var test = function () {};"),
        (
            "var test = require('tape');",
            "function swap() { test = null; }",
        ),
        ("let test = require('tape');", "test++;"),
        ("var test = require('tape');", "for (test of []) {}"),
    ] {
        let class = class_for("var-runner-written", &tape_test(prelude, tail))?;
        assert_ne!(class, ExposureClass::Exposed, "{prelude} {tail}");
    }
    Ok(())
}

#[test]
fn var_binding_of_a_non_runner_never_registers() -> Result<(), String> {
    let class = class_for(
        "var-not-runner",
        &tape_test("var test = require('./helper');", ""),
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    let local = class_for(
        "var-local-noop",
        &tape_test("var test = function (_name, _body) {};", ""),
    )?;
    assert_ne!(local, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn opaque_files_keep_var_runner_bindings_uncredited() -> Result<(), String> {
    let class = class_for(
        "var-runner-eval",
        &tape_test("var test = require('tape');", "eval('test = null');"),
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}
