//! #9437 / RIPR-SPEC-0246 rule 5: a changed line in a `catch` arm keeps
//! `exposed` only when a related test can make the guarded call throw.

use super::*;

const LOAD: &str = "export async function load(read: () => Promise<number>, fallback: number): Promise<number> {\n  try {\n    return await read();\n  } catch {\n    return fallback + 1;\n  }\n}\n";
const HEAD: &str =
    "import { expect, it } from \"vitest\";\nimport { load } from \"../src/load\";\n";

fn run(
    label: &str,
    source: &str,
    line: usize,
    removed: &str,
    added: &str,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/load.ts"), source)?;
    ts_write_file(&root.join("tests/load.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/load.ts"),
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

fn catch_edit(label: &str, test_body: &str) -> Result<Finding, String> {
    let test = format!("{HEAD}\nit(\"x\", async () => {{\n{test_body}\n}});\n");
    run(
        label,
        LOAD,
        5,
        "    return fallback;",
        "    return fallback + 1;",
        &test,
    )
}

#[test]
fn an_unreached_catch_arm_withdraws_exposed() -> Result<(), String> {
    let finding = catch_edit(
        "catch-unreached",
        "  await expect(load(async () => 42, 0)).resolves.toBe(42);",
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.missing
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("catch arm is never exercised")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_rejecting_test_keeps_exposed() -> Result<(), String> {
    let finding = catch_edit(
        "catch-reached",
        "  await expect(load(async () => { throw new Error('x'); }, 7)).resolves.toBe(7);",
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let rejected = catch_edit(
        "catch-rejected",
        "  await expect(load(() => Promise.reject(1), 7)).resolves.toBe(7);",
    )?;
    assert_eq!(
        rejected.class,
        ExposureClass::Exposed,
        "{:?}",
        rejected.missing
    );
    Ok(())
}

#[test]
fn an_edit_outside_the_catch_arm_is_unchanged() -> Result<(), String> {
    let source = LOAD.replace("await read()", "(await read()) + 1");
    let test = format!(
        "{HEAD}\nit(\"x\", async () => {{\n  await expect(load(async () => 42, 0)).resolves.toBe(43);\n}});\n"
    );
    let finding = run(
        "catch-outside",
        &source,
        3,
        "    return await read();",
        "    return (await read()) + 1;",
        &test,
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

const HAPPY: &str = "  await expect(load(async () => 42, 0)).resolves.toBe(42);";

fn edit_with_test(label: &str, owner: &str, line: usize, test: &str) -> Result<Finding, String> {
    run(
        label,
        owner,
        line,
        "    return fallback;",
        "    return fallback + 1;",
        test,
    )
}

fn plain_test(body: &str) -> String {
    format!("{HEAD}\nit(\"x\", async () => {{\n{body}\n}});\n")
}

#[test]
fn a_guarded_call_that_can_fail_by_itself_keeps_exposed() -> Result<(), String> {
    let parse = "export function load(s: string, fallback: number): number {\n  try {\n    return JSON.parse(s);\n  } catch {\n    return fallback + 1;\n  }\n}\n";
    let test = format!("{HEAD}\nit(\"x\", () => {{\n  expect(load(\"7\", 0)).toBe(7);\n}});\n");
    let finding = edit_with_test("catch-json-parse", parse, 5, &test)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    // Bad input reaches the catch with no throw keyword in the test.
    let test = format!("{HEAD}\nit(\"x\", () => {{\n  expect(load(\"nope\", 0)).toBe(1);\n}});\n");
    let finding = edit_with_test("catch-json-parse-bad-input", parse, 5, &test)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let deref = "export function load(o: { v: number } | null, fallback: number): number {\n  try {\n    return o.v;\n  } catch {\n    return fallback + 1;\n  }\n}\n";
    let test =
        format!("{HEAD}\nit(\"x\", () => {{\n  expect(load({{ v: 7 }}, 0)).toBe(7);\n}});\n");
    let finding = edit_with_test("catch-null-deref", deref, 5, &test)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_dot_catch_handler_is_a_catch_arm() -> Result<(), String> {
    let owner = "export function load(read: () => Promise<number>, fallback: number): Promise<number> {\n  return read().catch(() => {\n    return fallback + 1;\n  });\n}\n";
    let weak = edit_with_test("dotcatch-unreached", owner, 3, &plain_test(HAPPY))?;
    assert_eq!(
        weak.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        weak.missing
    );
    let reached = edit_with_test(
        "dotcatch-reached",
        owner,
        3,
        &plain_test("  await expect(load(() => Promise.reject(1), 7)).resolves.toBe(7);"),
    )?;
    assert_eq!(
        reached.class,
        ExposureClass::Exposed,
        "{:?}",
        reached.missing
    );
    Ok(())
}

#[test]
fn a_signal_in_a_comment_keeps_exposed() -> Result<(), String> {
    // The scan is deliberately generous: comments and strings count.
    let body = format!("  // the reader could reject here\n{HAPPY}");
    let finding = catch_edit("catch-comment-signal", &body)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_module_mock_keeps_exposed() -> Result<(), String> {
    let test = format!(
        "{HEAD}vi.mock(\"../src/dep\", () => ({{}}));\nit(\"x\", async () => {{\n{HAPPY}\n}});\n"
    );
    let finding = edit_with_test("catch-mocked", LOAD, 5, &test)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn every_related_test_must_lack_a_signal() -> Result<(), String> {
    let both_quiet = format!(
        "{HEAD}\nit(\"a\", async () => {{\n{HAPPY}\n}});\nit(\"b\", async () => {{\n  await expect(load(async () => 5, 0)).resolves.toBe(5);\n}});\n"
    );
    let weak = edit_with_test("catch-two-quiet", LOAD, 5, &both_quiet)?;
    assert_eq!(
        weak.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        weak.missing
    );
    let one_throws = format!(
        "{HEAD}\nit(\"a\", async () => {{\n{HAPPY}\n}});\nit(\"b\", async () => {{\n  await expect(load(async () => {{ throw 1; }}, 5)).resolves.toBe(5);\n}});\n"
    );
    let kept = edit_with_test("catch-one-throws", LOAD, 5, &one_throws)?;
    assert_eq!(kept.class, ExposureClass::Exposed, "{:?}", kept.missing);
    Ok(())
}

#[test]
fn an_operator_on_the_call_result_keeps_exposed() -> Result<(), String> {
    // `0 in 42` throws, so the catch runs with no throw-capable word in the test.
    let owner = "export function load(read: () => number, fallback: number): boolean {\n  try {\n    return 0 in read();\n  } catch {\n    return fallback + 1;\n  }\n}\n";
    let test = format!("{HEAD}\nit(\"x\", () => {{\n  expect(load(() => 42, 0)).toBe(1);\n}});\n");
    let finding = edit_with_test("catch-in-operator", owner, 5, &test)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}
