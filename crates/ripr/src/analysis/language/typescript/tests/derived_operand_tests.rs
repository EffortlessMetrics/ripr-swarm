//! #11256: a predicate on a local the owner computes from a call that holds no
//! owner parameter reads a value no test call can set, so no repair start.

use super::*;

const NAMED: &str = "typescript_boundary_operand_derived_unresolved";

fn derived_finding(label: &str, source: &str, changed: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/parse.ts"), source)?;
    ts_write_file(
        &root.join("tests/parse.test.ts"),
        "import { parse } from \"../src/parse\";\ntest(\"parses\", () => {\n  expect(parse(\"a\", 1)).toBe(\"x\");\n});\n",
    )?;
    let line = source
        .lines()
        .position(|text| text == changed)
        .ok_or("changed line not in source")?
        + 1;
    let old = changed.replace("!==", "===");
    let file = ChangedFile {
        path: PathBuf::from("src/parse.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: changed.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: old,
            new_side_line: line,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[file],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn names_limitation(finding: &Finding) -> bool {
    finding
        .evidence
        .iter()
        .any(|line| line == &format!("typescript_limitation: {NAMED}"))
}

#[test]
fn a_call_derived_local_operand_names_its_limitation() -> Result<(), String> {
    let source = "declare function kindOf(key: string): string;\nexport function parse(name: string, count: number) {\n  const key = \"k\";\n  const kind = kindOf(key);\n  if (kind !== \"flag\") {\n    return name;\n  }\n  return String(count);\n}\n";
    let finding = derived_finding("derived-operand", source, "  if (kind !== \"flag\") {")?;
    assert!(names_limitation(&finding), "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn parameters_and_input_steered_locals_keep_their_reading() -> Result<(), String> {
    // A parameter operand is an input a test call sets.
    let parameter = "export function parse(name: string, count: number) {\n  if (name !== \"flag\") {\n    return name;\n  }\n  return String(count);\n}\n";
    // A call over a parameter is steered by the test's input.
    let steered = "declare function kindOf(key: string): string;\nexport function parse(name: string, count: number) {\n  const kind = kindOf(name);\n  if (kind !== \"flag\") {\n    return name;\n  }\n  return String(count);\n}\n";
    // A non-call initializer is not a derived-from-call value.
    let plain = "export function parse(name: string, count: number) {\n  const kind = name + \"x\";\n  if (kind !== \"flag\") {\n    return name;\n  }\n  return String(count);\n}\n";
    for (label, source, changed) in [
        ("operand-parameter", parameter, "  if (name !== \"flag\") {"),
        ("operand-steered", steered, "  if (kind !== \"flag\") {"),
        ("operand-plain", plain, "  if (kind !== \"flag\") {"),
    ] {
        let finding = derived_finding(label, source, changed)?;
        assert!(
            !names_limitation(&finding),
            "{label}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}
