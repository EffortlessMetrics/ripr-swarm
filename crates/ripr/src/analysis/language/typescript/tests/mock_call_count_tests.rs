//! A mock call count is not the argument the mock received (#9684): for a
//! changed side-effect call it reads `mock_expectation` / weak under any
//! matcher, while a pin on the recorded arguments keeps its reading.

use super::*;

const OWNER: &str = "export function notify(id: number, sender: { send: (value: number) => void }): void {\n  sender.send(id * 2);\n}\n";
const OWNER_LINE: (usize, &str) = (2, "  sender.send(id * 2);");

fn node_test(assertion: &str) -> String {
    format!(
        "import {{ test, mock }} from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ notify }} from \"../src/notify\";\n\ntest(\"notify\", () => {{\n  const fn = mock.fn();\n  notify(4, {{ send: fn }});\n  {assertion}\n}});\n"
    )
}

fn jest_test(assertion: &str) -> String {
    format!(
        "import {{ notify }} from \"../src/notify\";\n\ntest(\"notify\", () => {{\n  const fn = jest.fn();\n  notify(4, {{ send: fn }});\n  {assertion}\n}});\n"
    )
}

fn notify_class(label: &str, source: &str) -> Result<ExposureClass, String> {
    notify_class_for(label, source, Some("  sender.send(id + 1);"))
}

/// `old_line` is the removed counterpart; `None` makes the changed line a
/// pure addition.
fn notify_class_for(
    label: &str,
    source: &str,
    old_line: Option<&str>,
) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/notify.ts"), OWNER)?;
    ts_write_file(&root.join("tests/notify.test.ts"), source)?;
    let mut changed = changed_with_lines("src/notify.ts", &[OWNER_LINE]);
    changed.removed_lines = old_line
        .map(|text| crate::analysis::diff::ChangedLine {
            line: OWNER_LINE.0,
            text: text.to_string(),
            new_side_line: OWNER_LINE.0,
        })
        .into_iter()
        .collect();
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == OWNER_LINE.0)
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

#[test]
fn a_call_count_does_not_see_the_arguments_of_a_changed_call() -> Result<(), String> {
    for (label, source) in [
        (
            "node-callcount",
            node_test("assert.equal(fn.mock.callCount(), 1);"),
        ),
        (
            "node-calls-length",
            node_test("assert.strictEqual(fn.mock.calls.length, 1);"),
        ),
        (
            "jest-calls-length",
            jest_test("expect(fn.mock.calls.length).toBe(1);"),
        ),
    ] {
        assert_eq!(
            notify_class(label, &source)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn an_aliased_mock_call_count_is_not_an_argument_pin() -> Result<(), String> {
    let source = node_test("const calls = fn.mock.calls; assert.equal(calls.length, 1);");
    assert_eq!(
        notify_class("count-alias", &source)?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn an_optional_mock_call_count_is_not_an_argument_pin() -> Result<(), String> {
    let source = node_test("assert.equal(fn?.mock.calls.length, 1);");
    assert_eq!(
        notify_class("count-optional", &source)?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn a_computed_mock_call_count_is_not_an_argument_pin() -> Result<(), String> {
    let source = node_test("assert.equal(fn[\"mock\"][\"calls\"][\"length\"], 1);");
    assert_eq!(
        notify_class("count-computed", &source)?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn an_added_call_keeps_the_count_because_the_count_sees_it() -> Result<(), String> {
    let source = node_test("assert.equal(fn.mock.callCount(), 1);");
    assert_eq!(
        notify_class_for("added-call", &source, None)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_pin_on_the_recorded_arguments_still_exposes_the_change() -> Result<(), String> {
    let source = node_test("assert.deepEqual(fn.mock.calls[0].arguments, [8]);");
    assert_eq!(notify_class("args-pin", &source)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn only_the_classifier_demotes_a_count_so_other_families_keep_it() {
    let tests = extract_tests(
        Path::new("tests/notify.test.ts"),
        &node_test("assert.equal(fn.mock.callCount(), 2);"),
    );
    let assertions: Vec<_> = tests
        .iter()
        .flat_map(|test| test.assertions.iter())
        .collect();
    assert_eq!(assertions.len(), 1);
    assert_eq!(assertions[0].oracle_kind, OracleKind::ExactValue);
    assert_eq!(assertions[0].oracle_strength, OracleStrength::Strong);
    assert!(mock_call_count_assertion(assertions[0], &tests[0]));
}

#[test]
fn an_expression_that_is_not_a_plain_mock_count_is_left_alone() {
    for assertion in [
        "assert.equal(counter.callCount(), 1);",
        "assert.equal(fn.callCount(), 1);",
        "assert.equal(fn.mock.callCount() + 1, 2);",
        "assert.equal(fn.mock.callCount[\"()\"], 1);",
        "assert.equal(fn.mock.calls[0].arguments.length, 1);",
    ] {
        let tests = extract_tests(Path::new("tests/notify.test.ts"), &node_test(assertion));
        let found: Vec<_> = tests
            .iter()
            .flat_map(|test| test.assertions.iter())
            .collect();
        assert_eq!(found.len(), 1, "{assertion}");
        assert!(
            !mock_call_count_assertion(found[0], &tests[0]),
            "{assertion}"
        );
    }
}

#[test]
fn more_mock_call_count_paths_and_aliases_are_not_argument_pins() -> Result<(), String> {
    for (index, assertion) in [
        "const m = fn.mock; const calls = m.calls; const n = calls.length; assert.equal(n, 1);",
        "const m = fn[\"mock\"]; assert.equal(m.callCount(), 1);",
        "assert.equal(fn.mock?.callCount?.(), 1);",
        "assert.equal(fn?.[\"mock\"]?.calls?.length, 1);",
        "assert.equal(fn[\"mo\\u0063k\"].calls.length, 1);",
        "const calls = fn.mock.calls; { assert.equal(calls.length, 1); }",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            notify_class(&format!("count-path-{index}"), &node_test(assertion))?,
            ExposureClass::WeaklyExposed,
            "{assertion}"
        );
    }
    Ok(())
}

#[test]
fn new_mock_call_count_spellings_preserve_added_call_credit() -> Result<(), String> {
    for assertion in [
        "const calls = fn.mock.calls; assert.equal(calls.length, 1);",
        "assert.equal(fn?.mock.calls.length, 1);",
        "assert.equal(fn[\"mock\"].calls.length, 1);",
    ] {
        assert_eq!(
            notify_class_for("added-count-path", &node_test(assertion), None)?,
            ExposureClass::Exposed,
            "{assertion}"
        );
    }
    Ok(())
}

#[test]
fn mock_call_count_aliases_do_not_absorb_argument_pins_or_other_values() -> Result<(), String> {
    let source =
        node_test("const calls = fn[\"mock\"].calls; assert.deepEqual(calls[0].arguments, [8]);");
    assert_eq!(
        notify_class("alias-argument-pin", &source)?,
        ExposureClass::Exposed
    );
    for assertion in [
        "const calls = [8]; assert.equal(calls.length, 1);",
        "const calls = \"fn.mock.calls\"; assert.equal(calls.length, 13);",
        "const calls = fn.mock.calls; { const calls = [8]; assert.equal(calls.length, 1); }",
        "const key = \"arguments\"; assert.equal(fn.mock.calls[0][key].length, 1);",
    ] {
        let tests = extract_tests(Path::new("tests/notify.test.ts"), &node_test(assertion));
        assert_eq!(tests.len(), 1, "{assertion}");
        assert_eq!(tests[0].assertions.len(), 1, "{assertion}");
        assert!(
            !mock_call_count_assertion(&tests[0].assertions[0], &tests[0]),
            "{assertion}"
        );
    }
    Ok(())
}

#[test]
fn called_times_matchers_already_keep_mock_call_count_evidence_weak() -> Result<(), String> {
    assert_eq!(
        notify_class(
            "called-times",
            &jest_test("expect(fn).toHaveBeenCalledTimes(1);")
        )?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn named_callback_mock_call_count_aliases_use_body_order() -> Result<(), String> {
    let header = "import { test, mock } from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport { notify } from \"../src/notify\";\n";
    let callback = "function notifyCallback() {\n const fn = mock.fn();\n notify(4, { send: fn });\n const calls = fn.mock.calls;\n assert.equal(calls.length, 1);\n}\n";
    let registration = "test(\"notify\", notifyCallback);\n";
    for source in [
        format!("{header}{callback}{registration}"),
        format!("{header}{registration}{callback}"),
    ] {
        assert_eq!(
            notify_class("named-count-alias", &source)?,
            ExposureClass::WeaklyExposed
        );
    }
    Ok(())
}

#[test]
fn mock_call_count_late_constants_do_not_supply_alias_identity() {
    let source = node_test("assert.equal(calls.length, 1); const calls = fn.mock.calls;");
    let tests = extract_tests(Path::new("tests/notify.test.ts"), &source);
    assert_eq!(tests.len(), 1);
    assert_eq!(mock_call_count_assertions(&tests[0]), vec![false]);
}

#[test]
fn mock_call_count_batch_preserves_many_ordinary_argument_pins() {
    let pins = "assert.equal(fn.mock.calls[0].arguments[0], 8);\n".repeat(1000);
    let source = node_test(&pins);
    let tests = extract_tests(Path::new("tests/notify.test.ts"), &source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions.len(), 1000);
    assert_eq!(mock_call_count_assertions(&tests[0]), vec![false; 1000]);
}

#[test]
fn a_long_receiver_does_not_resurrect_mock_call_count_credit() {
    let assertion = format!(
        "assert.equal(fn.{}mock.calls.length, 1);",
        "nested.".repeat(100)
    );
    let tests = extract_tests(Path::new("tests/notify.test.ts"), &node_test(&assertion));
    assert_eq!(tests.len(), 1);
    assert_eq!(mock_call_count_assertions(&tests[0]), vec![true]);
}

#[test]
fn shadowed_argument_pin_keeps_credit_beside_an_unused_mock_call_count() -> Result<(), String> {
    let source = node_test(
        "const count = fn.mock.calls.length; { const count = fn.mock.calls[0].arguments[0]; assert.equal(count, 8); }",
    );
    assert_eq!(
        notify_class("shadowed-argument-pin", &source)?,
        ExposureClass::Exposed
    );
    Ok(())
}
