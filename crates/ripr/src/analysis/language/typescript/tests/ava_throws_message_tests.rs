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
    format!(
        "import test from \"ava\";\nimport {{ parse }} from \"../src/lib\";\n\ntest(\"parse\", async t => {{\n  {call};\n}});\n"
    )
}

fn only_assertion(label: &str, call: &str) -> Result<TypeScriptAssertion, String> {
    let test = test_source(call);
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
