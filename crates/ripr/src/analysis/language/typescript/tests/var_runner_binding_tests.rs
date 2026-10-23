//! RIPR-SPEC-0243 rule 1: a CommonJS runner bound with `var` or `let` is as
//! immutable as a `const` one when the file never writes the binding after its
//! initializer. Pre-ES2015 packages (qs, minimist, fastq, object-inspect)
//! write `var test = require('tape');` and read `no_static_path` for every
//! owner their own suites fail on (#9182).

use super::*;

const OWNER: &str = "exports.triple = function triple(x) {\n  return x * 3;\n};\n";
const CHANGED: (usize, &str) = (2, "  return x * 3;");

fn class_for(label: &str, test_source: &str) -> Result<ExposureClass, String> {
    class_for_file(label, "test/k.test.js", test_source)
}

fn class_for_file(label: &str, file: &str, test_source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/k.js"), OWNER)?;
    ts_write_file(&root.join(file), test_source)?;
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
        (
            "var test = require('tape');",
            "for ([test] of [[function () {}]]) {}",
        ),
        (
            "var test = require('tape');",
            "for ({ test } of [{ test: function () {} }]) {}",
        ),
        ("var test = require('tape');", "for ([test] in {}) {}"),
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

#[test]
fn a_written_unrelated_declarator_keeps_the_runner_declarator_registered() -> Result<(), String> {
    let class = class_for(
        "var-runner-sibling-written",
        &tape_test("var counter = 0, test = require('tape');", "counter++;"),
    )?;
    assert_eq!(class, ExposureClass::Exposed);
    let written_runner = class_for(
        "var-runner-sibling-runner-written",
        &tape_test("var counter = 0, test = require('tape');", "test = null;"),
    )?;
    assert_ne!(written_runner, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn grouped_and_typescript_wrapped_targets_refuse_the_binding() -> Result<(), String> {
    for tail in [
        "(test) = function () {};",
        "(test)++;",
        "++(test);",
        "((test)) = null;",
        "for ((test) of []) {}",
        "test! = function () {};",
        "(test as any) = function () {};",
        "(test satisfies unknown) = null;",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        let class = class_for_file("var-runner-wrapped", "test/k.test.ts", &source)?;
        assert_eq!(class, ExposureClass::NoStaticPath, "{tail}");
    }
    let control = tape_test(
        "var test = require('tape');",
        "(test);\ntest!.skip('x', () => {});",
    );
    let class = class_for_file("var-runner-wrapped-control", "test/k.test.ts", &control)?;
    assert_eq!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn block_level_function_declarations_rebind_the_var_binding() -> Result<(), String> {
    for tail in [
        "if (process.env.C) {\n  function test() {}\n}",
        "{ function* test() {} }",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        let class = class_for("var-runner-annex-b", &source)?;
        assert_eq!(class, ExposureClass::NoStaticPath, "{tail}");
    }
    Ok(())
}

#[test]
fn a_write_after_a_postfix_update_and_division_is_still_seen() -> Result<(), String> {
    let source = tape_test(
        "var test = require('tape'), n = 1, m = 2;",
        "n++ / 2;\ntest = null;\nm / 3;",
    );
    assert_eq!(
        class_for("var-runner-postfix-div", &source)?,
        ExposureClass::NoStaticPath
    );
    Ok(())
}

#[test]
fn template_writes_count_while_strings_and_comments_do_not() -> Result<(), String> {
    let template = tape_test("var test = require('tape');", "const s = `${test = null}`;");
    assert_eq!(
        class_for("var-runner-template", &template)?,
        ExposureClass::NoStaticPath
    );
    for tail in ["// test = null\nconst s = 'test = 1';", "test.x = 1;"] {
        let source = tape_test("var test = require('tape');", tail);
        let class = class_for("var-runner-inert", &source)?;
        assert_eq!(class, ExposureClass::Exposed, "{tail}");
    }
    Ok(())
}

#[test]
fn crlf_and_non_ascii_sources_keep_the_binding() -> Result<(), String> {
    let crlf = tape_test("var test = require('tape');", "").replace('\n', "\r\n");
    assert_eq!(class_for("var-runner-crlf", &crlf)?, ExposureClass::Exposed);
    let unicode = tape_test("// é ü\nvar café = 1, test = require('tape');", "");
    assert_eq!(
        class_for("var-runner-unicode", &unicode)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_nested_scope_with_its_own_binding_is_refused_conservatively() -> Result<(), String> {
    // The scan is textual, so a shadowing `test = ...` in an inner scope
    // withholds credit; fail-closed, documented in RIPR-SPEC-0243 rule 1.
    let source = tape_test("var test = require('tape');", "{ const test = 1; }");
    assert_eq!(
        class_for("var-runner-shadow", &source)?,
        ExposureClass::NoStaticPath
    );
    Ok(())
}

#[test]
fn global_object_member_writes_refuse_the_binding() -> Result<(), String> {
    for tail in [
        "globalThis.test = function () {};",
        "this.test = null;",
        "window.test = null;",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        assert_eq!(
            class_for("var-runner-global", &source)?,
            ExposureClass::NoStaticPath,
            "{tail}"
        );
    }
    let read = tape_test("var test = require('tape');", "const g = globalThis.test;");
    assert_eq!(
        class_for("var-runner-global-read", &read)?,
        ExposureClass::Exposed
    );
    Ok(())
}
