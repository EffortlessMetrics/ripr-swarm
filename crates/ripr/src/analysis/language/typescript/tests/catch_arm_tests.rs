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
