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

/// #9936 shape 1: in `.tsx` a closing tag `</div>` is not a regex start, so a
/// write on the same line (or after it) is still seen.
#[test]
fn a_write_after_a_jsx_closing_tag_is_still_seen() -> Result<(), String> {
    for tail in [
        "const v = <p>x</p>; test = null;",
        "const v = <div>a</div>;\ntest = function () {};",
        "const v = <div><b>a</b></div>; test = null;",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        assert_eq!(
            class_for_file("var-runner-jsx-close", "test/k.test.tsx", &source)?,
            ExposureClass::NoStaticPath,
            "{tail}"
        );
    }
    // Negative control: the same shapes with no write keep the binding.
    let control = tape_test(
        "var test = require('tape');",
        "const v = <div>a</div>; const w = <p>x</p>;",
    );
    assert_eq!(
        class_for_file("var-runner-jsx-close-control", "test/k.test.tsx", &control)?,
        ExposureClass::Exposed
    );
    Ok(())
}

/// #9936 shape 2: a backtick inside a regex literal that follows `)` or `}`
/// (where `/` reads as division) does not open a template string.
#[test]
fn a_write_after_a_backtick_in_a_regex_literal_is_still_seen() -> Result<(), String> {
    for tail in [
        "if (n) /`/.test(s);\ntest = null;",
        "if (n) /[`'\"]/.test(s);\ntest = null;",
        "{}\n/`/.test(s);\ntest = function () {};",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        assert_eq!(
            class_for("var-runner-regex-backtick", &source)?,
            ExposureClass::NoStaticPath,
            "{tail}"
        );
    }
    // Negative controls: no write keeps the binding, and a real division by
    // a template literal still masks the template text.
    for tail in [
        "if (n) /`/.test(s);\nconst s2 = 1;",
        "const q = n /`test = null`;",
    ] {
        let source = tape_test("var test = require('tape');", tail);
        assert_eq!(
            class_for("var-runner-regex-backtick-control", &source)?,
            ExposureClass::Exposed,
            "{tail}"
        );
    }
    Ok(())
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

#[test]
fn review_9936_genuine_division_and_comparison_controls() -> Result<(), String> {
    for (tail, expected) in [
        (
            "const q = n /`2`.length; test = null; const r = 8 / 2;",
            ExposureClass::NoStaticPath,
        ),
        (
            "const q = n /`2`.length; test = null; // done",
            ExposureClass::NoStaticPath,
        ),
        (
            "const q = n /[+`2`][0]; test = null; const r = 8 / 2;",
            ExposureClass::NoStaticPath,
        ),
        (
            "const q = n /`${(test = null, 2)}`.length; const r = 8 / 2;",
            ExposureClass::NoStaticPath,
        ),
        (
            "const q = n /`x/ test = null`.length;",
            ExposureClass::Exposed,
        ),
        (
            "const q = n</`/.source.length; test = null;",
            ExposureClass::NoStaticPath,
        ),
        (
            "const q = n</x test = null/.source.length;",
            ExposureClass::Exposed,
        ),
        ("const q = n /`test = null`.length;", ExposureClass::Exposed),
        (
            "const q = n++ /[+`${(test = null, 2)}`][0] / 2;",
            ExposureClass::NoStaticPath,
        ),
        (
            "const obj = { if(x: number) { return x; } }; const q = obj.if(n) /[+`${(test = null, 2)}`][0] / 2;",
            ExposureClass::NoStaticPath,
        ),
    ] {
        let source = tape_test(
            "var test = require('tape');",
            &format!("let n = 8; const s = 'sample'; {tail}"),
        );
        assert_eq!(
            class_for_file("masker-division-review-control", "test/k.test.ts", &source)?,
            expected,
            "{tail}"
        );
    }
    Ok(())
}

#[test]
fn review_9936_control_head_regex_masks_escaped_and_later_backticks() -> Result<(), String> {
    for head in [
        "if ((n))",
        "if /* comment */ (n)",
        "while (false)",
        "for (; false;)",
    ] {
        for regex in ["/a`/", "/[\\`]/", "/test = null`/", "/[/'\"`]/"] {
            for written in [false, true] {
                let tail = format!(
                    "const n = true; const s = ''; {head} {regex}.test(s);{}",
                    if written { " test = null;" } else { "" }
                );
                let source = tape_test("var test = require('tape');", &tail);
                assert_eq!(
                    class_for_file("masker-control-head-review", "test/k.test.ts", &source)?,
                    if written {
                        ExposureClass::NoStaticPath
                    } else {
                        ExposureClass::Exposed
                    },
                    "{tail}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn review_9936_nested_jsx_and_expression_braces_preserve_writes() -> Result<(), String> {
    for jsx in [
        "<div data-x={1 < 2 ? 1 : 0}>text<span /></div>",
        "<><div /><p>text</p></>",
        "<div>{true ? <span>text</span> : <b />}</div>",
    ] {
        for written in [false, true] {
            let source = tape_test(
                "var test = require('tape');",
                &format!(
                    "const node = {jsx};{}",
                    if written { " test = null;" } else { "" }
                ),
            );
            assert_eq!(
                class_for_file("masker-nested-jsx-review", "test/k.test.tsx", &source)?,
                if written {
                    ExposureClass::NoStaticPath
                } else {
                    ExposureClass::Exposed
                },
                "{jsx}; written={written}"
            );
        }
    }
    Ok(())
}

#[test]
fn review_9936_type_assertions_do_not_open_jsx_tags() -> Result<(), String> {
    for (tail, expected) in [
        (
            "const b = n</T>`/.source.length; test = null;",
            ExposureClass::NoStaticPath,
        ),
        (
            "const b = n</T>test = null/.source.length;",
            ExposureClass::Exposed,
        ),
    ] {
        let tail = format!("type T = number; const n = 8; const typed = <T>n; {tail}");
        let source = tape_test("var test = require('tape');", &tail);
        assert_eq!(
            class_for_file("masker-type-assertion-review", "test/k.test.ts", &source)?,
            expected,
            "{tail}"
        );
    }
    Ok(())
}

#[test]
fn review_9936_jsx_text_does_not_open_code_literals_or_comments() -> Result<(), String> {
    for text in [
        "if (n) /a",
        "// text",
        "/* text",
        "`text",
        "\"text",
        "'text",
    ] {
        let source = tape_test(
            "var test = require('tape');",
            &format!("const node = <div>{text} {{test = null}}</div>;"),
        );
        assert_eq!(
            class_for_file("masker-jsx-text-review", "test/k.test.tsx", &source)?,
            ExposureClass::NoStaticPath,
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn review_9936_private_control_keyword_method_is_a_call() -> Result<(), String> {
    for (operand, expected) in [
        ("+`${(test = null, 2)}`", ExposureClass::NoStaticPath),
        ("+`test = null`", ExposureClass::Exposed),
    ] {
        let tail = format!(
            "class C {{ #if(x: number) {{ return x; }} f() {{ return this.#if(8) /({operand}) / 2; }} }} new C().f();"
        );
        let source = tape_test("var test = require('tape');", &tail);
        assert_eq!(
            class_for_file("masker-private-control-review", "test/k.test.ts", &source)?,
            expected,
            "{tail}"
        );
    }
    Ok(())
}

#[test]
fn review_9936_identical_source_uses_the_native_file_syntax() -> Result<(), String> {
    let tail = "const a = 8, g = 2; const x: any = 0; type div = any; const v = <div>x</div>; test = null; const y = a /g;";
    let source = tape_test("var test = require('tape');", tail);
    for (file, expected) in [
        ("test/k.test.ts", ExposureClass::Exposed),
        ("test/k.test.tsx", ExposureClass::NoStaticPath),
    ] {
        assert_eq!(
            class_for_file("masker-native-syntax-review", file, &source)?,
            expected,
            "{file}"
        );
    }
    Ok(())
}

#[test]
fn review_9936_jsx_text_is_not_an_executable_write() -> Result<(), String> {
    let source = tape_test(
        "var test = require('tape');",
        "const node = <div>test = null</div>;",
    );
    assert_eq!(
        class_for_file("masker-jsx-text-negative", "test/k.test.tsx", &source)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn review_9936_for_await_body_regex_keeps_real_and_fake_writes_separate() -> Result<(), String> {
    for written in [false, true] {
        let tail = format!(
            "async function f() {{ const s = ''; for await (const x of []) /test = null`/.test(s); }}{}",
            if written { " test = null;" } else { "" }
        );
        let source = tape_test("var test = require('tape');", &tail);
        assert_eq!(
            class_for_file("masker-for-await-review", "test/k.test.ts", &source)?,
            if written {
                ExposureClass::NoStaticPath
            } else {
                ExposureClass::Exposed
            }
        );
    }
    Ok(())
}

#[test]
fn review_9936_generic_jsx_and_attributes_preserve_only_executable_writes() -> Result<(), String> {
    for (expression, written) in [
        ("<Component<number> />", false),
        (
            "<Component<() => number>><Component<string> /></Component>",
            false,
        ),
        ("<div test=\"text\" />", false),
        ("<div test={test} />", false),
        ("<div test={test = null} />", true),
        ("<div data-x={<span>test = null `é</span>} />", false),
        ("<div data-x={<span>{test = null}</span>} />", true),
        ("<div data-x=<>test = null</> />", false),
        ("<div data-x=<>{test = null}</> />", true),
        ("<div {...{value: <span>test = null</span>}} />", false),
        ("<div {...{value: <span>{test = null}</span>}} />", true),
    ] {
        for trailing_write in [false, true] {
            let source = tape_test(
                "var test = require('tape');",
                &format!(
                    "declare function Component<T>(props: any): any; const node = {expression};{}",
                    if trailing_write { " test = null;" } else { "" }
                ),
            );
            assert_eq!(
                class_for_file("masker-jsx-attribute-review", "test/k.test.tsx", &source)?,
                if written || trailing_write {
                    ExposureClass::NoStaticPath
                } else {
                    ExposureClass::Exposed
                },
                "{expression}; trailing_write={trailing_write}"
            );
        }
    }
    Ok(())
}

#[test]
fn review_9936_tsx_comparison_and_generic_arrow_are_code() -> Result<(), String> {
    for expression in ["a <<b > c", "<T extends unknown>(x: T) => x"] {
        for written in [false, true] {
            let source = tape_test(
                "var test = require('tape');",
                &format!(
                    "const a = 8, b = 1, c = 0; const q = {expression};{}",
                    if written { " test = null;" } else { "" }
                ),
            );
            assert_eq!(
                class_for_file("masker-tsx-code-review", "test/k.test.tsx", &source)?,
                if written {
                    ExposureClass::NoStaticPath
                } else {
                    ExposureClass::Exposed
                },
                "{expression}; written={written}"
            );
        }
    }
    Ok(())
}

#[test]
fn review_9936_for_member_then_await_is_division() -> Result<(), String> {
    for (operand, expected) in [
        ("+`${(test = null, 2)}`", ExposureClass::NoStaticPath),
        ("+`test = null`", ExposureClass::Exposed),
    ] {
        let source = tape_test(
            "var test = require('tape');",
            &format!(
                "async function f() {{ const obj = {{ for: 0 }}; obj.for\n await(8) /({operand}) /2; }} f();"
            ),
        );
        assert_eq!(
            class_for_file("masker-for-member-await-review", "test/k.test.ts", &source)?,
            expected,
            "{operand}"
        );
    }
    Ok(())
}

#[test]
fn review_9936_comparison_regex_after_angle_shapes_keeps_the_write_visible() -> Result<(), String> {
    let regex = "const q = n</a`/.test(s); test = null;";
    let cases = [
        (
            "test/k.test.ts",
            format!("const n = 1, s = ''; const a = <any>s; {regex}"),
        ),
        (
            "test/k.test.ts",
            format!("const n = 1, s = ''; 1<<n; {regex}"),
        ),
        (
            "test/k.test.tsx",
            format!("const n = 1, s = ''; const a = <Foo a={{1}} />; {regex}"),
        ),
        (
            "test/k.test.tsx",
            format!(
                "const n = 1, s = ''; function f() {{ return <p>x</p>; test = null; }} {regex}"
            ),
        ),
        (
            "test/k.test.ts",
            "async function f() { const s = ''; for await (const x of []) /a`/.test(s); test = null; }"
                .to_string(),
        ),
    ];
    for (index, (file, tail)) in cases.iter().enumerate() {
        let source = tape_test("var test = require('tape');", tail);
        assert_eq!(
            class_for_file("masker-angle-shapes-review", file, &source)?,
            ExposureClass::NoStaticPath,
            "case {index}: {tail}"
        );
    }
    Ok(())
}
