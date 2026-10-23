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

#[test]
fn comments_and_postfix_division_do_not_hide_a_write() -> Result<(), String> {
    for tail in [
        "let n = 0;\nn++ / 2; // Swap the runner out.\ntest = function () {};",
        "// Swap the runner out.\ntest = function () {};",
        "test /* c */ = function () {};",
        "test // c\n= function () {};",
        "function /* c */ test() {}",
        "let n = 0; n-- / 2; test = null; n / 3;",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        assert_eq!(
            class_for("var-runner-comment", &source)?,
            ExposureClass::NoStaticPath,
            "{tail}"
        );
    }
    Ok(())
}

#[test]
fn balanced_as_clauses_and_byte_order_marks_are_still_writes() -> Result<(), String> {
    for tail in [
        "(test as () => void) = function () {};",
        "(test as ((a: number) => void)) = function () {};",
        "(test as typeof import('tape')) = null;",
        "test\u{feff}= null;",
        "function\u{feff}test() {}",
        "test\u{feff}++;",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        let class = class_for_file("var-runner-bom", "test/k.test.ts", &source)?;
        assert_eq!(class, ExposureClass::NoStaticPath, "{tail}");
    }
    Ok(())
}

#[test]
fn non_ascii_names_are_whole_identifiers_in_the_write_scan() {
    use crate::analysis::language::typescript::tests_extract::{
        identifier_written_in, strict_identifier_writes,
    };
    let identifier_written_strictly = |text: &str, name: &str| {
        strict_identifier_writes(text)
            .iter()
            .any(|(written, _)| written == name)
    };
    for (write, name) in [
        ("café = null;", "café"),
        ("té++;", "té"),
        ("(café) = 1;", "café"),
        ("for ([café] of xs);", "café"),
    ] {
        assert!(identifier_written_strictly(write, name), "{write}");
    }
    // A longer name sharing an ASCII prefix is a different identifier.
    assert!(!identifier_written_in("café = null;", "caf"));
    assert!(!identifier_written_strictly(
        "const café = 1; read(caf);",
        "caf"
    ));
}

/// Writes that must stay visible after each shape. Every entry is
/// `(label, file, tail, expected)`; a no-write control pairs each shape.
fn assert_tails(file: &str, tails: &[&str], expected: ExposureClass) -> Result<(), String> {
    let mut wrong = Vec::new();
    for tail in tails {
        let source = tape_test(
            "var test = require('tape');\nconst n = 8, s = 'sample';",
            tail,
        );
        if class_for_file("masker-9936", file, &source)? != expected {
            wrong.push(*tail);
        }
    }
    assert!(wrong.is_empty(), "{expected:?} expected for {wrong:?}");
    Ok(())
}

/// #9936 shape 1: inside an open JSX element a closing tag `</div>` is not a
/// regex start, so a write after it is still seen.
#[test]
fn a_write_after_a_jsx_closing_tag_is_still_seen() -> Result<(), String> {
    assert_tails(
        "test/k.test.tsx",
        &[
            "const v = <p>x</p>; test = null;",
            "const v = <></>; test = null;",
            "render(<i>x</i>); test = null;",
            "const v = <br/>; const w = <p>x</p>; test = null;",
        ],
        ExposureClass::NoStaticPath,
    )?;
    // Negative control: the same shapes with no write keep the binding.
    assert_tails(
        "test/k.test.tsx",
        &["const v = <div>a</div>; const w = <p>x</p>; const u = <></>;"],
        ExposureClass::Exposed,
    )
}

/// A `<` comparison against a regex is not a closing tag: outside an open JSX
/// element the parent's operand-position regex reading is kept.
#[test]
fn a_comparison_against_a_regex_is_not_a_jsx_closing_tag() -> Result<(), String> {
    assert_tails(
        "test/k.test.ts",
        &[
            "const q = n</`/.source.length; test = null;",
            "const b = f(n)</`/.test(s);\ntest = null;",
        ],
        ExposureClass::NoStaticPath,
    )?;
    assert_tails(
        "test/k.test.ts",
        &["const q = n</x test = null/.source.length;"],
        ExposureClass::Exposed,
    )
}

/// #9936 shape 2, statement-regex context only: a regex literal right after
/// the `)` of an `if`/`while`/`for` header is a regex, never a division, so a
/// backtick inside it does not open a template. Other backtick-in-regex
/// positions (after `}` or a bare statement start) stay a documented residual.
#[test]
fn a_write_after_a_regex_following_a_control_header_is_still_seen() -> Result<(), String> {
    assert_tails(
        "test/k.test.ts",
        &[
            "if (n) /`/.test(s);\ntest = null;",
            "if (n) /a`/.test(s);\ntest = null;",
            "if (n) /[`'\"]/.test(s);\ntest = null;",
            "if (n) /[\\`]/.test(s);\ntest = null;",
            "while (n) /`é/.test(s);\ntest = null;",
            "if (n) /`/g.test(s); test = null;",
        ],
        ExposureClass::NoStaticPath,
    )?;
    assert_tails(
        "test/k.test.ts",
        &["if (n) /`/.test(s);\nconst s2 = 1;"],
        ExposureClass::Exposed,
    )
}

/// A division by a template or array keeps the parent's reading, so a write
/// in or after it stays visible and a template's text stays masked.
#[test]
fn a_write_in_or_after_a_division_by_a_template_is_still_seen() -> Result<(), String> {
    assert_tails(
        "test/k.test.ts",
        &[
            "const q = n /`2`.length; test = null; const r = 8 / 2;",
            "const q = n /`2`.length; test = null; // done",
            "const q = n /[+`2`][0]; test = null; const r = 8 / 2;",
            "const q = n /`${(test = null, 2)}`.length; const r = 8 / 2;",
            "const q = n /[+`${(test = null, 2)}`][0] / 2;",
            "let i = 0; const q = i++ /[+`${(test = null, 2)}`][0] / 2;",
            "const q = n /`a/b`; test = null;",
            "const q = n /`x`; // c/d `\ntest = null;",
            "const q = n /`${n}` / 2; test = null;",
        ],
        ExposureClass::NoStaticPath,
    )?;
    assert_tails(
        "test/k.test.ts",
        &[
            "const q = n /`test = null`.length;",
            "const q = n /`x/ test = null`.length;",
            "const q = n /`a/b`;",
        ],
        ExposureClass::Exposed,
    )
}

/// A native TypeScript number-array operand can contain executable template
/// interpolation. A following division slash does not turn it into a regex.
#[test]
fn review_9936_division_template_interpolation_write_is_seen() -> Result<(), String> {
    let tail = "const n: number = 4; const q = n /[+`${(test = null, 2)}`][0] / 2;";
    let source = tape_test("var test = require('tape');", tail);
    assert_eq!(
        class_for_file("masker-division-interpolation", "test/k.test.ts", &source)?,
        ExposureClass::NoStaticPath,
        "{tail}"
    );
    Ok(())
}

#[test]
fn review_9936_later_regex_backtick_write_is_seen() -> Result<(), String> {
    let tail = "const n = true; const s = ''; if (n) /a`/.test(s);\ntest = null;";
    let source = tape_test("var test = require('tape');", tail);
    assert_eq!(
        class_for_file("masker-later-regex-backtick", "test/k.test.ts", &source)?,
        ExposureClass::NoStaticPath,
        "{tail}"
    );
    Ok(())
}

#[test]
fn review_9936_comparison_regex_write_is_seen() -> Result<(), String> {
    let tail = "const n: any = 4; const s = ''; const b = n</`/.test(s);\ntest = null;";
    let source = tape_test("var test = require('tape');", tail);
    assert_eq!(
        class_for_file("masker-comparison-regex", "test/k.test.ts", &source)?,
        ExposureClass::NoStaticPath,
        "{tail}"
    );
    Ok(())
}

#[test]
fn review_9936_no_write_controls_retain_runner_credit() -> Result<(), String> {
    for tail in [
        "const n = 4; const q = n /[+`${2}`][0] / 2;",
        "const n = true; const s = ''; if (n) /a`/.test(s);",
        "const n: any = 4; const s = ''; const b = n</`/.test(s);",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        assert_eq!(
            class_for_file("masker-no-write-review-control", "test/k.test.ts", &source)?,
            ExposureClass::Exposed,
            "{tail}"
        );
    }
    Ok(())
}

/// The classifier mask preserves characters; parser consumers must use its
/// byte-preserving adapter when comparing the mask with source spans.
#[test]
fn review_9936_mask_adapter_preserves_unicode_source_offsets() -> Result<(), String> {
    for source in [
        "if (n) /`é/.test(s);\ntest = null;",
        "const s = 'é'; test = null;",
        "const s = `é${2}`; test = null;",
        "/* é */ test = null;",
    ] {
        let code = super::super::tests_extract::byte_preserving_code_mask(source)
            .ok_or_else(|| format!("could not mask {source}"))?;
        assert_eq!(code.len(), source.len(), "{source}");
        assert_eq!(code.find("test = null"), source.find("test = null"));
    }
    Ok(())
}
