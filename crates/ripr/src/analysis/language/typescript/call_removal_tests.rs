//! #10157 receiver-identity controls from #9667 comment 6371144187.

use super::*;

/// Runs the adapter over a unified diff of `src_path` in a package holding
/// `files` and returns the finding on `line`.
fn finding_for(
    label: &str,
    runner: &str,
    files: &[(&str, &str)],
    diff: &str,
    line: usize,
) -> Result<Finding, String> {
    let root = super::tests::ts_unique_tempdir(label)?;
    let write =
        |path: &str, contents: &str| super::tests::ts_write_file(&root.join(path), contents);
    write("package.json", runner)?;
    for (path, contents) in files {
        write(path, contents)?;
    }
    let changed_files = crate::analysis::diff::parse_unified_diff(diff);
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let result = TypeScriptAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("no finding on line {line}"))
}

const NODE_TEST: &str =
    r#"{"name":"pkg","type":"module","scripts":{"test":"node --test test/*.test.ts"}}"#;

fn control(
    label: &str,
    source: &str,
    body: &str,
    imports: &str,
    exposed: bool,
) -> Result<(), String> {
    control_with_removed_call(label, source, body, imports, exposed, "sink.write(event);")
}

fn control_with_removed_call(
    label: &str,
    source: &str,
    body: &str,
    imports: &str,
    exposed: bool,
    removed_call: &str,
) -> Result<(), String> {
    let test = format!(
        "import {{ mock, test }} from 'node:test';\nimport assert from 'node:assert/strict';\n{imports}\ntest('writes once', () => {{\n{body}\n}});\n"
    );
    let line = source
        .lines()
        .position(|line| line.trim() == "void event;")
        .ok_or("missing discard")?
        + 1;
    let diff = format!(
        "diff --git a/src/audit.ts b/src/audit.ts\n--- a/src/audit.ts\n+++ b/src/audit.ts\n@@ -{line},1 +{line},1 @@\n-  {removed_call}\n+  void event;\n"
    );
    let finding = finding_for(
        label,
        NODE_TEST,
        &[
            ("src/audit.ts", source),
            ("test/audit.test.ts", &test),
            (
                "other/audit.ts",
                "export const sink = { write(event: string) { void event; } };",
            ),
        ],
        &diff,
        line,
    )?;
    assert_eq!(
        finding.class == ExposureClass::Exposed,
        exposed,
        "{label}: {finding:?}"
    );
    Ok(())
}

const MODULE: &str = "export const sink = { write(event: string) { void event; } };\nexport function record(event: string): void {\n  void event;\n}\n";
const PARAM: &str = "export function emit(sink: { write(event: string): void }, event: string): void {\n  void event;\n}\n";
const OBJECT: &str = "{ write(event: string) { void event; } }";
const PIN: &str = "assert.equal(write.mock.callCount(), 1);";
const SPY: &str = "const write = mock.method(sink, 'write');";
const MODULE_IMPORT: &str = "import { record, sink } from '../src/audit.ts';";
const PARAM_IMPORT: &str = "import { emit } from '../src/audit.ts';";

#[test]
fn counted_removal_receiver_identity_controls() -> Result<(), String> {
    control(
        "passed-receiver",
        PARAM,
        &format!("const sink = {OBJECT}; {SPY} emit(sink, 'a'); {PIN}"),
        PARAM_IMPORT,
        true,
    )?;
    control(
        "imported-receiver",
        MODULE,
        &format!("{SPY} record('a'); {PIN}"),
        MODULE_IMPORT,
        true,
    )?;
    control(
        "sibling-js-module-same-stem",
        MODULE,
        &format!("{SPY} record('a'); {PIN}"),
        "import { record, sink } from '../src/audit.js';",
        false,
    )?;
    control(
        "different-module",
        MODULE,
        &format!("{SPY} sink.write('test-side'); record('a'); {PIN}"),
        "import { record } from '../src/audit.ts'; import { sink } from '../other/audit.ts';",
        false,
    )?;
    control(
        "reassignment",
        PARAM,
        &format!(
            "let sink = {OBJECT}; {SPY} sink.write('test-side'); sink = {OBJECT}; emit(sink, 'a'); {PIN}"
        ),
        PARAM_IMPORT,
        false,
    )?;
    control(
        "whitespace-shadow",
        MODULE,
        &format!("const\n sink = {OBJECT}; {SPY} sink.write('test-side'); record('a'); {PIN}"),
        MODULE_IMPORT,
        false,
    )?;
    control(
        "different-module-identity-only",
        MODULE,
        &format!("{SPY} record('a'); {PIN}"),
        "import { record } from '../src/audit.ts'; import { sink } from '../other/audit.ts';",
        false,
    )?;
    control(
        "whitespace-shadow-identity-only",
        MODULE,
        &format!("const\n sink = {OBJECT}; {SPY} record('a'); {PIN}"),
        MODULE_IMPORT,
        false,
    )?;
    control(
        "late-registration",
        MODULE,
        &format!("record('a'); {SPY} sink.write('test-side'); {PIN}"),
        MODULE_IMPORT,
        false,
    )?;
    Ok(())
}

#[test]
fn counted_removal_refuses_unresolved_execution() -> Result<(), String> {
    for (label, body) in [
        (
            "wrong-argument",
            format!("const sink = {OBJECT}; const other = {OBJECT}; {SPY} emit(other, 'a'); {PIN}"),
        ),
        (
            "reset",
            format!("const sink = {OBJECT}; {SPY} emit(sink, 'a'); write.mock.resetCalls(); {PIN}"),
        ),
        (
            "conditional-registration",
            format!("const sink = {OBJECT}; if (true) {{ {SPY} }} emit(sink, 'a'); {PIN}"),
        ),
        (
            "zero-count",
            format!(
                "const sink = {OBJECT}; {SPY} emit(sink, 'a'); assert.equal(write.mock.callCount(), 0);"
            ),
        ),
    ] {
        control(label, PARAM, &body, PARAM_IMPORT, false)?;
    }
    Ok(())
}

#[test]
fn counted_removal_refuses_enclosing_mock_shadow() -> Result<(), String> {
    let test = format!(
        "import {{ mock, test, describe }} from 'node:test';\nimport assert from 'node:assert/strict';\n{PARAM_IMPORT}\ndescribe('shadow', () => {{ const mock = {{ method() {{ return {{ mock: {{ callCount() {{ return 1; }} }} }}; }} }}; test('writes once', () => {{ const sink = {OBJECT}; {SPY} emit(sink, 'a'); {PIN} }}); }});"
    );
    let finding = finding_for(
        "enclosing-mock-shadow",
        NODE_TEST,
        &[("src/audit.ts", PARAM), ("test/audit.test.ts", &test)],
        "diff --git a/src/audit.ts b/src/audit.ts\n--- a/src/audit.ts\n+++ b/src/audit.ts\n@@ -2,1 +2,1 @@\n-  sink.write(event);\n+  void event;\n",
        2,
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn counted_removal_refuses_early_return_and_default_parameters() -> Result<(), String> {
    for source in [
        "export function emit(sink: { write(event: string): void }, event: string): void {\n  return;\n  void event;\n}\n",
        "export function emit(sink = makeSink(), event: string): void {\n  void event;\n}\n",
    ] {
        control(
            "unsupported-owner",
            source,
            &format!("const sink = {OBJECT}; {SPY} emit(sink, 'a'); {PIN}"),
            PARAM_IMPORT,
            false,
        )?;
    }
    Ok(())
}

#[test]
fn counted_removal_retained_corpus_shape_is_exposed() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/typescript-verdict-corpus/subjects/authored-ts-nodetest-mocks");
    let source =
        std::fs::read_to_string(fixture.join("src/audit.ts")).map_err(|error| error.to_string())?;
    let test = std::fs::read_to_string(fixture.join("test/audit.test.ts"))
        .map_err(|error| error.to_string())?;
    assert_eq!(source.matches("  sink.write(line);").count(), 1);
    let source = source.replace("  sink.write(line);", "  void line;");
    let finding = finding_for(
        "retained-corpus",
        NODE_TEST,
        &[("src/audit.ts", &source), ("test/audit.test.ts", &test)],
        "diff --git a/src/audit.ts b/src/audit.ts\n--- a/src/audit.ts\n+++ b/src/audit.ts\n@@ -9,1 +9,1 @@\n-  sink.write(line);\n+  void line;\n",
        9,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn counted_removal_refuses_export_aliases_and_opaque_reads() -> Result<(), String> {
    for source in [
        "const sink = { write(event: string) { void event; } };\nconst other = { write(event: string) { void event; } };\nexport { other as sink };\nexport function record(event: string): void {\n  void event;\n}\n",
        "export const sink = { write(event: string) { void event; } };\nexport function record(event: string): void {\n  const value = opaqueGlobal;\n  void event;\n}\n",
    ] {
        control(
            "ambiguous-module-binding",
            source,
            &format!("{SPY} record('a'); {PIN}"),
            MODULE_IMPORT,
            false,
        )?;
    }
    control(
        "opaque-replacement",
        PARAM,
        &format!(
            "const sink = {OBJECT}; const write = mock.method(sink, 'write', () => opaque()); emit(sink, 'a'); {PIN}"
        ),
        PARAM_IMPORT,
        false,
    )?;
    Ok(())
}

#[test]
fn counted_removal_refuses_owner_export_alias_and_global_discard() -> Result<(), String> {
    control(
        "owner-export-alias",
        "function emit(sink: { write(event: string): void }, event: string): void {\n  void event;\n}\nfunction other(sink: { write(event: string): void }, event: string): void { sink.write(event); }\nexport { other as emit };\n",
        &format!("const sink = {OBJECT}; {SPY} emit(sink, 'a'); {PIN}"),
        PARAM_IMPORT,
        false,
    )?;
    control(
        "opaque-discard",
        "export function emit(sink: { write(event: string): void }, event: string): void {\n  void event;\n  void opaqueGlobal;\n}\n",
        &format!("const sink = {OBJECT}; {SPY} emit(sink, 'a'); {PIN}"),
        PARAM_IMPORT,
        false,
    )?;
    Ok(())
}

#[test]
fn counted_removal_refuses_mutated_mock_api() -> Result<(), String> {
    control(
        "mutated-mock-api",
        PARAM,
        &format!("const sink = {OBJECT}; {SPY} emit(sink, 'a'); {PIN}"),
        &format!(
            "{PARAM_IMPORT} mock.method = () => ({{ mock: {{ callCount() {{ return 1; }} }} }});"
        ),
        false,
    )
}

#[test]
fn counted_removal_refuses_registration_argument_effects() -> Result<(), String> {
    let test = format!(
        "import {{ mock, test }} from 'node:test'; import assert from 'node:assert/strict'; {PARAM_IMPORT} test('writes once', () => {{ const sink = {OBJECT}; {SPY} emit(sink, 'a'); {PIN} }}, mock.method = () => ({{ mock: {{ callCount() {{ return 1; }} }} }}));"
    );
    let finding = finding_for(
        "registration-side-effect",
        NODE_TEST,
        &[("src/audit.ts", PARAM), ("test/audit.test.ts", &test)],
        "diff --git a/src/audit.ts b/src/audit.ts\n--- a/src/audit.ts\n+++ b/src/audit.ts\n@@ -2,1 +2,1 @@\n-  sink.write(event);\n+  void event;\n",
        2,
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn counted_removal_refuses_arguments_that_can_throw() {
    let new = "void event;";
    let read = |old: &str| {
        super::call_removal_guard::removed_method_call(new, Some(old))
            .map(|removed| (removed.receiver, removed.method))
    };
    for old in [
        "sink.write(event);",
        "sink.write(event, 'a', 1, true, null);",
        "sink.write(`line`);",
        "sink.write();",
    ] {
        assert_eq!(
            read(old),
            Some(("sink".to_string(), "write".to_string())),
            "{old}"
        );
    }
    // Arguments are evaluated before the callee: a throw there stops the old
    // test before its count assertion, so neither version discriminates.
    for old in [
        "sink.write((() => { throw new Error('boom'); })());",
        "sink.write(load());",
        "sink.write(a.b);",
        "sink.write(...rest);",
        "sink.write(`line ${event}`);",
        "sink.write(await later);",
    ] {
        assert_eq!(read(old), None, "{old}");
    }
}

#[test]
fn counted_removal_refuses_unusable_target_methods() -> Result<(), String> {
    for (label, object) in [
        (
            "throwing-target",
            "{ write(event: string) { throw new Error('boom'); } }",
        ),
        ("missing-target", "{ other(event: string) { void event; } }"),
        (
            "duplicate-target",
            "{ write(event: string) { void event; }, write(event: string) { throw new Error('boom'); } }",
        ),
    ] {
        control(
            label,
            PARAM,
            &format!("const sink = {object}; {SPY} emit(sink, 'a'); {PIN}"),
            PARAM_IMPORT,
            false,
        )?;
    }
    // A replacement bypasses the original body, but cannot create a missing method.
    for (label, object, exposed) in [
        (
            "replaced-throwing-target",
            "{ write(event: string) { throw new Error('boom'); } }",
            true,
        ),
        (
            "replaced-missing-target",
            "{ other(event: string) { void event; } }",
            false,
        ),
    ] {
        control(
            label,
            PARAM,
            &format!(
                "const sink = {object}; const write = mock.method(sink, 'write', () => undefined); emit(sink, 'a'); {PIN}"
            ),
            PARAM_IMPORT,
            exposed,
        )?;
    }
    for (label, source, spy, exposed) in [
        (
            "imported-throwing-target",
            MODULE.replace(
                "write(event: string) { void event; }",
                "write(event: string) { throw new Error('boom'); }",
            ),
            SPY,
            false,
        ),
        (
            "imported-missing-target",
            MODULE.replace("write(event: string)", "other(event: string)"),
            SPY,
            false,
        ),
        (
            "imported-replaced-target",
            MODULE.replace(
                "write(event: string) { void event; }",
                "write(event: string) { throw new Error('boom'); }",
            ),
            "const write = mock.method(sink, 'write', () => undefined);",
            true,
        ),
    ] {
        control(
            label,
            &source,
            &format!("{spy} record('a'); {PIN}"),
            MODULE_IMPORT,
            exposed,
        )?;
    }
    Ok(())
}

#[test]
fn counted_removal_pins_removed_arguments_before_the_statement() -> Result<(), String> {
    let body = format!("const sink = {OBJECT}; {SPY} emit(sink, 'a'); {PIN}");
    control_with_removed_call(
        "unbound-removed-argument",
        PARAM,
        &body,
        PARAM_IMPORT,
        false,
        "sink.write(missing);",
    )?;
    let later = PARAM.replace("  void event;", "  void event;\n  const later = 'value';");
    control_with_removed_call(
        "later-removed-argument",
        &later,
        &body,
        PARAM_IMPORT,
        false,
        "sink.write(later);",
    )?;
    let earlier = PARAM.replace("  void event;", "  const line = event;\n  void event;");
    control_with_removed_call(
        "earlier-removed-argument",
        &earlier,
        &body,
        PARAM_IMPORT,
        true,
        "sink.write(line);",
    )?;
    Ok(())
}
