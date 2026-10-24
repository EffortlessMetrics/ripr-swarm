//! RIPR-SPEC-0246 rule 8: a counted-loop header change that a related test
//! reads only through a mock call count is `static_unknown`, not a gap.

use super::*;

const RETRY: &str = "export async function withRetry<T>(fn: () => Promise<T>, attempts: number): Promise<T> {\n  let lastError: unknown;\n  for (let i = 0; i < attempts; i++) {\n    try {\n      return await fn();\n    } catch (error) {\n      lastError = error;\n    }\n  }\n  throw lastError;\n}\n";
const HEAD: &str = "import { it, mock } from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport { withRetry } from \"../src/retry\";\n";
const OLD: &str = "  for (let i = 0; i < attempts; i++) {";
const NEW: &str = "  for (let i = 0; i < attempts - 1; i++) {";

fn run(
    label: &str,
    source: &str,
    removed: &str,
    added: &str,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/retry.ts"), source)?;
    ts_write_file(&root.join("test/retry.test.ts"), test)?;
    let line = 3;
    let changed = ChangedFile {
        path: PathBuf::from("src/retry.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: added.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: removed.to_string(),
            new_side_line: line,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn retry_test(body: &str) -> String {
    format!("{HEAD}\nit(\"x\", async () => {{\n{body}\n}});\n")
}

const COUNTED: &str = "  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  await assert.rejects(withRetry(fn, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);";

#[test]
fn a_call_count_over_a_changed_loop_bound_is_unknown() -> Result<(), String> {
    let finding = run("loop-count", RETRY, OLD, NEW, &retry_test(COUNTED))?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    assert_eq!(
        finding.stop_reasons,
        vec![StopReason::GapEvidenceUnresolved]
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("counted loop header")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_value_assertion_alone_is_not_made_unknown() -> Result<(), String> {
    let body = "  const fn = mock.fn(async () => \"ok\");\n  assert.equal(await withRetry(fn, 3), \"ok\");";
    let finding = run("loop-value", RETRY, OLD, NEW, &retry_test(body))?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_changed_condition_that_is_not_a_loop_header_is_not_made_unknown() -> Result<(), String> {
    let source = RETRY.replace(OLD, "  if (attempts > 0) {");
    let finding = run(
        "loop-if",
        &source,
        "  if (attempts > 0) {",
        "  if (attempts > 1) {",
        &retry_test(COUNTED),
    )?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_for_of_loop_header_change_is_not_made_unknown() -> Result<(), String> {
    let source = RETRY.replace(OLD, "  for (const step of steps) {");
    let finding = run(
        "loop-for-of",
        &source,
        "  for (const step of steps) {",
        "  for (const step of steps.slice(1)) {",
        &retry_test(COUNTED),
    )?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_call_count_on_a_mock_the_owner_never_receives_is_not_made_unknown() -> Result<(), String> {
    let body = "  const fn = mock.fn(async () => \"ok\");\n  const log = mock.fn();\n  assert.equal(await withRetry(fn, 3), \"ok\");\n  assert.equal(log.mock.callCount(), 0);";
    let finding = run("loop-unrelated-count", RETRY, OLD, NEW, &retry_test(body))?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

const REAL_VALUE: &str = "  const real = mock.fn(async () => \"ok\");\n  assert.equal(await withRetry(real, 3), \"ok\");";

fn assert_stays_a_gap(label: &str, body: &str) -> Result<(), String> {
    let finding = run(label, RETRY, OLD, NEW, &retry_test(body))?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{label}: {:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_member_argument_is_not_the_counted_mock() -> Result<(), String> {
    assert_stays_a_gap(
        "loop-member-arg",
        "  const fn = mock.fn(async () => \"ok\");\n  const other = { fn: mock.fn(async () => \"ok\") };\n  assert.equal(await withRetry(other.fn, 3), \"ok\");\n  assert.equal(fn.mock.callCount(), 0);",
    )
}

#[test]
fn an_unrelated_member_call_or_a_comment_is_not_the_owner_call() -> Result<(), String> {
    let counted =
        "  const fn = mock.fn(async () => \"ok\");\n  assert.equal(fn.mock.callCount(), 2);";
    assert_stays_a_gap(
        "loop-member-call",
        &format!(
            "{REAL_VALUE}\n  const other = {{ withRetry: (f: unknown, n: number) => n }};\n  other.withRetry(fn, 2);\n{counted}"
        ),
    )?;
    assert_stays_a_gap(
        "loop-comment",
        &format!("{REAL_VALUE}\n  // withRetry(fn, 2)\n{counted}"),
    )
}

#[test]
fn an_optional_call_passing_the_mock_is_unknown() -> Result<(), String> {
    let body = COUNTED.replace("withRetry(fn, 2)", "withRetry?.(fn, 2)");
    let finding = run("loop-optional-call", RETRY, OLD, NEW, &retry_test(&body))?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_renamed_import_passing_the_mock_is_unknown() -> Result<(), String> {
    let test = format!(
        "{}\nit(\"x\", async () => {{\n{}\n}});\n",
        HEAD.replace("{ withRetry }", "{ withRetry as retry }"),
        COUNTED.replace("withRetry(fn, 2)", "retry(fn, 2)")
    );
    let finding = run("loop-alias", RETRY, OLD, NEW, &test)?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}
