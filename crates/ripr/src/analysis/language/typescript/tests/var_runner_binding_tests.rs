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
        ("var test = require('tape');", "(test) = function () {};"),
        ("var test = require('tape');", "(test)++;"),
        ("var test = require('tape');", "[test] = [function () {}];"),
        (
            "var test = require('tape');",
            "({ test } = { test: function () {} });",
        ),
        (
            "var test = require('tape');",
            "for ([test] of [[function () {}]]) {}",
        ),
        ("var test = require('tape');", "for ({ test } of xs) {}"),
        ("var test = require('tape');", "for ([test] in o) {}"),
        ("var test = require('tape');", "const s = `${test = null}`;"),
        ("var test = require('tape');", "{ function test() {} }"),
        (
            "var test = require('tape');",
            "globalThis.test = function () {};",
        ),
        ("var test = require('tape');", "this.test = function () {};"),
        (
            "var test = require('tape');",
            "function f(test) { return test; }",
        ),
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
fn reads_strings_comments_members_and_sibling_writes_keep_the_binding() -> Result<(), String> {
    for (prelude, tail) in [
        (
            "var test = require('tape');",
            "// test = null\nconst s = 'test = 1';",
        ),
        ("var test = require('tape');", "test.x = 1;"),
        (
            "var test = require('tape');",
            "test.skip('later', function () {});",
        ),
        ("var test = require('tape'), n = 1;", "n = 2;"),
        ("var n = 1, test = require('tape');", "n++;"),
        ("var test = require('tape');", "\r\n/* é */ var é = 1;"),
    ] {
        let class = class_for("var-runner-reads", &tape_test(prelude, tail))?;
        assert_eq!(class, ExposureClass::Exposed, "{prelude} {tail}");
    }
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
fn typescript_assignment_targets_refuse_the_binding() -> Result<(), String> {
    for tail in ["test! = function () {};", "(test as any) = function () {};"] {
        let source = tape_test("var test = require('tape');", tail);
        let class = class_for_file("var-runner-ts-target", "test/k.test.ts", &source)?;
        assert_ne!(class, ExposureClass::Exposed, "{tail}");
    }
    let control = tape_test("var test = require('tape');", "");
    let class = class_for_file("var-runner-ts-control", "test/k.test.ts", &control)?;
    assert_eq!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn each_declarator_is_judged_by_its_own_binding() -> Result<(), String> {
    let kept = tape_test(
        "var helper = require('./helper.js'), test = require('tape');",
        "helper = null;",
    );
    assert_eq!(
        class_for("var-runner-sibling-require", &kept)?,
        ExposureClass::Exposed
    );
    let counted = tape_test("var counter = 0, test = require('tape');", "counter++;");
    assert_eq!(
        class_for("var-runner-sibling-counter", &counted)?,
        ExposureClass::Exposed
    );
    let refused = tape_test(
        "var helper = require('./helper.js'), test = require('tape');",
        "test = function () {};",
    );
    assert_ne!(
        class_for("var-runner-own-write", &refused)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn loop_head_destructuring_targets_refuse_the_binding() -> Result<(), String> {
    for tail in [
        "for ([test] of [[function () {}]]) {}",
        "for ({ test } of xs) {}",
        "for ([test] in o) {}",
        "for ((test) of xs) {}",
        "(test) = function () {};",
        "(test)++;",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        let class = class_for("var-runner-loop-target", &source)?;
        assert_ne!(class, ExposureClass::Exposed, "{tail}");
    }
    Ok(())
}
