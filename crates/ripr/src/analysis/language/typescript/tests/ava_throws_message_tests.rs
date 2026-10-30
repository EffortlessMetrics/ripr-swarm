//! RIPR-SPEC-0243 rule 10 (AVA): `t.throws` / `t.throwsAsync` with an
//! all-literal expectation object whose `message` is a string compares the
//! message by equality, so it pins the error as `node:assert` does.
//!
//! The subject is rule 4's `throw new Error("empty")` becoming
//! `throw new Error("blank")`. Each end-to-end test first checks the parsed
//! assertion, so a fixture slip cannot pass on nothing.

use super::*;

const OWNER: &str = "export function parse(s: string): string {\n  if (s === \"\") {\n    throw new Error(\"blank\");\n  }\n  return s;\n}\n";
const ADDED: (usize, &str) = (3, "    throw new Error(\"blank\");");
const REMOVED: &str = "    throw new Error(\"empty\");";

fn test_source(call: &str) -> String {
    test_source_importing("import test from \"ava\";", call)
}

fn test_source_importing(runner_import: &str, call: &str) -> String {
    format!(
        "{runner_import}\nimport {{ parse }} from \"../src/lib\";\n\ntest(\"parse\", async t => {{\n  {call};\n}});\n"
    )
}

fn only_assertion(label: &str, call: &str) -> Result<TypeScriptAssertion, String> {
    only_assertion_in(label, test_source(call))
}

fn only_assertion_in(label: &str, test: String) -> Result<TypeScriptAssertion, String> {
    let tests = extract_tests(Path::new("tests/parse.test.ts"), &test);
    let [parsed] = tests.as_slice() else {
        return Err(format!("{label}: expected one test, got {tests:?}"));
    };
    let [assertion] = parsed.assertions.as_slice() else {
        return Err(format!(
            "{label}: expected one assertion, got {:?}",
            parsed.assertions
        ));
    };
    Ok(assertion.clone())
}

fn class_for(label: &str, call: &str) -> Result<(ExposureClass, String), String> {
    let test = test_source(call);
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/lib.ts"), OWNER)?;
    ts_write_file(&root.join("tests/parse.test.ts"), &test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/lib.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: ADDED.0,
            text: ADDED.1.to_string(),
            new_side_line: ADDED.0,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: ADDED.0,
            text: REMOVED.to_string(),
            new_side_line: ADDED.0,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    let finding = result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == ADDED.0)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))?;
    Ok((finding.class, finding.ripr.reveal.observe.summary))
}

#[test]
fn an_ava_message_object_reads_as_an_exact_error() -> Result<(), String> {
    for (label, call, kind) in [
        (
            "throws",
            "t.throws(() => parse(\"\"), { message: \"blank\" })",
            TypeScriptErrorPayloadKind::AvaThrowsObject,
        ),
        (
            "throws-async",
            "await t.throwsAsync(async () => parse(\"\"), { message: \"blank\" })",
            TypeScriptErrorPayloadKind::AvaThrowsAsyncObject,
        ),
    ] {
        let assertion = only_assertion(label, call)?;
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::ExactErrorVariant,
            "{label}"
        );
        assert_eq!(assertion.oracle_strength, OracleStrength::Strong, "{label}");
        let payload = assertion
            .error_payload
            .ok_or_else(|| format!("{label}: expected an error payload"))?;
        assert_eq!(payload.kind, kind, "{label}");
    }
    Ok(())
}

#[test]
fn an_ava_message_object_credits_only_when_it_separates_the_messages() -> Result<(), String> {
    let (new_message, summary) = class_for(
        "ava-new-message",
        "t.throws(() => parse(\"\"), { message: \"blank\" })",
    )?;
    assert_eq!(new_message, ExposureClass::Exposed, "{summary}");
    assert!(
        summary.contains("oracle kind: `exact_error_variant`"),
        "{summary}"
    );

    let (old_message, _) = class_for(
        "ava-old-message",
        "t.throws(() => parse(\"\"), { message: \"empty\" })",
    )?;
    assert_eq!(old_message, ExposureClass::Exposed);

    // Neither message: the assertion passes on neither version, so the
    // reading stays broad.
    let (neither, summary) = class_for(
        "ava-neither",
        "t.throws(() => parse(\"\"), { message: \"x\" })",
    )?;
    assert_eq!(neither, ExposureClass::WeaklyExposed, "{summary}");
    assert!(summary.contains("oracle kind: `broad_error`"), "{summary}");
    Ok(())
}

#[test]
fn other_ava_error_expectations_stay_broad() -> Result<(), String> {
    for (label, call) in [
        ("no-expectation", "t.throws(() => parse(\"\"))"),
        (
            "class-only",
            "t.throws(() => parse(\"\"), { instanceOf: TypeError })",
        ),
        (
            "regex-message",
            "t.throws(() => parse(\"\"), { message: /blank/ })",
        ),
        (
            "non-literal-message",
            "t.throws(() => parse(\"\"), { message: expected })",
        ),
        (
            "spread",
            "t.throws(() => parse(\"\"), { ...base, message: \"blank\" })",
        ),
        (
            "literal-and-class",
            "t.throws(() => parse(\"\"), { message: \"blank\", instanceOf: TypeError })",
        ),
        (
            "duplicate-message",
            "t.throws(() => parse(\"\"), { message: \"blank\", message: \"x\" })",
        ),
        (
            "duplicate-message-string-key",
            "t.throws(() => parse(\"\"), { message: \"blank\", \"message\": \"x\" })",
        ),
        (
            "not-throws",
            "t.notThrows(() => parse(\"\"), { message: \"blank\" })",
        ),
        (
            "not-throws-async",
            "await t.notThrowsAsync(async () => parse(\"\"), { message: \"blank\" })",
        ),
    ] {
        let assertion = only_assertion(label, call)?;
        assert_eq!(assertion.oracle_kind, OracleKind::BroadError, "{label}");
        assert!(assertion.error_payload.is_none(), "{label}");
    }
    Ok(())
}

#[test]
fn a_message_object_outside_an_ava_test_stays_broad() -> Result<(), String> {
    // tape and uvu give `t.throws` a different expectation contract, and a
    // file with no runner import registers nothing: only AVA's own `test`
    // import credits the object.
    let call = "t.throws(() => parse(\"\"), { message: \"blank\" })";
    for (label, runner_import) in [
        ("tape", "import test from \"tape\";"),
        ("uvu", "import { test } from \"uvu\";"),
        ("ava-type-only", "import type test from \"ava\";"),
    ] {
        let assertion = only_assertion_in(label, test_source_importing(runner_import, call))?;
        assert_eq!(assertion.oracle_kind, OracleKind::BroadError, "{label}");
        assert!(assertion.error_payload.is_none(), "{label}");
    }
    // A file with no runner import registers no test, so nothing credits.
    let unregistered = test_source_importing("const test = globalThis.test;", call);
    assert!(
        extract_tests(Path::new("tests/parse.test.ts"), &unregistered).is_empty(),
        "an unimported `test` must register nothing"
    );
    // The positive control: AVA's own import, and the named `test` export.
    for (label, runner_import) in [
        ("ava-default", "import test from \"ava\";"),
        ("ava-named", "import { test } from \"ava\";"),
    ] {
        let assertion = only_assertion_in(label, test_source_importing(runner_import, call))?;
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::ExactErrorVariant,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn an_ava_expectation_with_any_other_property_stays_broad() -> Result<(), String> {
    for (label, props) in [
        ("extra-foo", "message: \"blank\", foo: 1"),
        (
            "instance-of-string",
            "message: \"blank\", instanceOf: \"Error\"",
        ),
        ("is-null", "message: \"blank\", is: null"),
        ("name-number", "message: \"blank\", name: 123"),
        ("name-string", "message: \"blank\", name: \"TypeError\""),
        ("computed-key", "[\"message\"]: \"blank\""),
        ("shorthand", "message"),
        ("method", "message() { return \"blank\"; }"),
        ("empty", ""),
    ] {
        let call = format!("t.throws(() => parse(\"\"), {{ {props} }})");
        let assertion = only_assertion(label, &call)?;
        assert_eq!(assertion.oracle_kind, OracleKind::BroadError, "{label}");
        assert!(assertion.error_payload.is_none(), "{label}");
    }
    Ok(())
}

#[test]
fn a_shadowed_or_replaced_ava_receiver_stays_broad() -> Result<(), String> {
    let throws = "t.throws(() => parse(\"\"), { message: \"blank\" })";
    for (label, call) in [
        (
            "shadowed",
            format!("const t = {{ throws: () => {{}} }};\n  {throws}"),
        ),
        ("replaced", format!("t.throws = () => {{}};\n  {throws}")),
    ] {
        let assertion = only_assertion(label, &call)?;
        assert_eq!(assertion.oracle_kind, OracleKind::BroadError, "{label}");
        assert!(assertion.error_payload.is_none(), "{label}");
    }
    Ok(())
}

#[test]
fn an_ava_thrower_must_match_the_matcher() -> Result<(), String> {
    let message = "{ message: \"blank\" }";
    for (label, call, credited) in [
        (
            "sync-arrow",
            format!("t.throws(() => parse(\"\"), {message})"),
            true,
        ),
        (
            "sync-function",
            format!("t.throws(function () {{ parse(\"\"); }}, {message})"),
            true,
        ),
        (
            "async-throws-async",
            format!("await t.throwsAsync(async () => parse(\"\"), {message})"),
            true,
        ),
        (
            "async-with-throws",
            format!("t.throws(async () => parse(\"\"), {message})"),
            false,
        ),
        (
            "generator",
            format!("t.throws(function* () {{ parse(\"\"); }}, {message})"),
            false,
        ),
        (
            "promise-argument",
            format!("await t.throwsAsync(parse(\"\"), {message})"),
            false,
        ),
        (
            "sync-with-throws-async",
            format!("await t.throwsAsync(() => parse(\"\"), {message})"),
            false,
        ),
        (
            "resolved-error",
            format!("await t.throwsAsync(Promise.resolve(new Error(\"blank\")), {message})"),
            false,
        ),
    ] {
        let assertion = only_assertion(label, &call)?;
        let expected = if credited {
            OracleKind::ExactErrorVariant
        } else {
            OracleKind::BroadError
        };
        assert_eq!(assertion.oracle_kind, expected, "{label}");
    }
    Ok(())
}

#[test]
fn an_ava_duplicate_message_stays_broad_in_javascript() -> Result<(), String> {
    for (label, props) in [
        ("duplicate", "message: \"blank\", message: \"x\""),
        ("string-key", "message: \"blank\", \"message\": \"x\""),
    ] {
        let test = format!(
            "import test from \"ava\";\nimport {{ parse }} from \"../src/lib\";\n\ntest(\"parse\", t => {{\n  t.throws(() => parse(\"\"), {{ {props} }});\n}});\n"
        );
        let tests = extract_tests(Path::new("tests/parse.test.js"), &test);
        let [parsed] = tests.as_slice() else {
            return Err(format!("{label}: expected one test, got {tests:?}"));
        };
        let [assertion] = parsed.assertions.as_slice() else {
            return Err(format!("{label}: expected one assertion"));
        };
        assert_eq!(assertion.oracle_kind, OracleKind::BroadError, "{label}");
    }
    Ok(())
}

#[test]
fn a_bare_ava_throws_async_outlives_the_test_so_it_stays_broad() -> Result<(), String> {
    let delayed = "async () => {\n    await new Promise(resolve => setTimeout(resolve, 30));\n    parse(\"\");\n  }";
    for (label, call, credited) in [
        (
            "bare",
            format!("t.throwsAsync({delayed}, {{ message: \"blank\" }})"),
            false,
        ),
        (
            "awaited",
            format!("await t.throwsAsync({delayed}, {{ message: \"blank\" }})"),
            true,
        ),
        (
            "returned",
            format!("return t.throwsAsync({delayed}, {{ message: \"blank\" }})"),
            true,
        ),
    ] {
        let assertion = only_assertion(label, &call)?;
        let expected = if credited {
            OracleKind::ExactErrorVariant
        } else {
            OracleKind::BroadError
        };
        assert_eq!(assertion.oracle_kind, expected, "{label}");
        let (class, summary) = class_for(label, &call)?;
        let expected_class = if credited {
            ExposureClass::Exposed
        } else {
            ExposureClass::WeaklyExposed
        };
        assert_eq!(class, expected_class, "{label}: {summary}");
    }
    Ok(())
}

#[test]
fn ava_runner_edge_forms_credit_only_a_real_ava_registration() -> Result<(), String> {
    let call = "t.throws(() => parse(\"\"), { message: \"blank\" })";
    let aliased = only_assertion_in(
        "aliased",
        test_source_importing("import { test as it } from \"ava\";", call)
            .replace("test(\"parse\"", "it(\"parse\""),
    )?;
    assert_eq!(aliased.oracle_kind, OracleKind::ExactErrorVariant);

    let reexport = test_source_importing("export { test } from \"ava\";", call);
    assert!(
        extract_tests(Path::new("tests/parse.test.ts"), &reexport).is_empty(),
        "a re-exported `test` must not register an AVA test"
    );

    // A callback parameter named `t` is a second binding of the receiver
    // name; the text-level receiver check cannot tell the two apart, so it
    // fails closed and the outer `t.throws` stays broad.
    let nested = test_source(
        "[1].forEach(t => t.is(1, 1));\n  t.throws(() => parse(\"\"), { message: \"blank\" })",
    );
    let tests = extract_tests(Path::new("tests/parse.test.ts"), &nested);
    let throws: Vec<_> = tests
        .iter()
        .flat_map(|parsed| &parsed.assertions)
        .filter(|assertion| assertion.matcher == "throws")
        .collect();
    assert!(!throws.is_empty(), "nested-param: expected a `t.throws`");
    for assertion in throws {
        assert_eq!(
            assertion.oracle_kind,
            OracleKind::BroadError,
            "nested-param"
        );
    }
    Ok(())
}

#[test]
fn a_comment_inside_an_ava_expectation_never_supplies_the_message() -> Result<(), String> {
    // The real property is `message: "x"`, which matches neither version.
    let (class, summary) = class_for(
        "ava-comment-message",
        "t.throws(() => parse(\"\"), { /* message: \"blank\" */ message: \"x\" })",
    )?;
    assert_eq!(class, ExposureClass::WeaklyExposed, "{summary}");
    let (line_comment, summary) = class_for(
        "ava-line-comment-message",
        "t.throws(() => parse(\"\"), {\n    // message: \"blank\"\n    message: \"x\"\n  })",
    )?;
    assert_eq!(line_comment, ExposureClass::WeaklyExposed, "{summary}");
    // An ordinary comment beside the real message still credits it.
    let (commented, summary) = class_for(
        "ava-commented-message",
        "t.throws(() => parse(\"\"), { /* the new text */ message: \"blank\" })",
    )?;
    assert_eq!(commented, ExposureClass::Exposed, "{summary}");
    Ok(())
}
