//! A module-level scalar constant edit (`export const LIMIT = 11;`) is a
//! return-value change the tests reach by name. It used to be an
//! unrecognised shape, and a non-ASCII constant name was never matched in the
//! tests at all (hostile-unicode-emoji-literal read `no_static_path`).

use super::*;

fn const_finding(
    label: &str,
    owner_source: &str,
    name: &str,
    line: &str,
) -> Result<Finding, String> {
    const_finding_expecting(label, owner_source, name, line, "'x'")
}

fn const_finding_expecting(
    label: &str,
    owner_source: &str,
    name: &str,
    line: &str,
    expected: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/consts.ts"), owner_source)?;
    let test = format!(
        "import {{ expect, it }} from 'vitest';\nimport {{ {name} }} from '../src/consts';\n\nit('keeps it', () => {{\n  expect({name}).toBe({expected});\n}});\n"
    );
    ts_write_file(&root.join("tests/consts.test.ts"), &test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/consts.ts", &[(1, line)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .next()
        .ok_or_else(|| format!("{label}: expected a finding"))
}

#[test]
fn exported_scalar_constants_read_as_a_return_value_change() -> Result<(), String> {
    for (label, line) in [
        ("num", "export const LIMIT = 11;"),
        ("neg", "export const LIMIT = -1.5;"),
        ("str", "export const LIMIT = \"ready\";"),
        ("typed", "export const LIMIT: number = 11;"),
        ("plain", "const LIMIT = 'a b';"),
        ("bool", "export const LIMIT = true;"),
    ] {
        let finding = const_finding(label, &format!("{line}\n"), "LIMIT", line)?;
        assert_eq!(finding.probe.family, ProbeFamily::ReturnValue, "{label}");
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn non_ascii_constant_name_is_matched_in_the_tests() -> Result<(), String> {
    let line = "export const bannière = \"🚀 ready\";";
    let finding = const_finding("unicode", &format!("{line}\n"), "bannière", line)?;
    assert_eq!(finding.probe.family, ProbeFamily::ReturnValue);
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.reach.state, StageState::Yes);
    Ok(())
}

#[test]
fn a_neighbouring_non_ascii_letter_is_not_a_match() -> Result<(), String> {
    // The tests mention `bannièreX`, a different identifier, not `bannière`.
    let root = ts_unique_tempdir("unicode-neighbour")?;
    let line = "export const bannière = \"🚀 ready\";";
    ts_write_file(&root.join("src/consts.ts"), &format!("{line}\n"))?;
    ts_write_file(
        &root.join("tests/consts.test.ts"),
        "import { expect, it } from 'vitest';\nconst bannièreX = 'x';\n\nit('other', () => {\n  expect(bannièreX).toBe('x');\n});\n",
    )?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/consts.ts", &[(1, line)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let finding = result
        .findings
        .into_iter()
        .next()
        .ok_or("expected a finding")?;
    assert_ne!(finding.ripr.reach.state, StageState::Yes);
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn computed_or_structured_initializers_are_not_scalar_constants() {
    use super::super::probe_shape::is_module_scalar_constant;
    for line in [
        "export const LIMIT = compute(1);",
        "export const LIMIT = 1 + 2;",
        "export const LIMIT = `a${b}`;",
        "export const LIMIT = [1, 2];",
        "export const LIMIT = { a: 1 };",
        "export const A = 1, B = 2;",
        "  const LIMIT = 11;",
        "export let LIMIT = 11;",
        "export const LIMIT: Record<string, number> = 1;",
        "export const LIMIT = 'a' + 'b';",
        "export const LIMIT = undefined;",
        "export const LIMIT = 1abc;",
        "export const LIMIT = 1e;",
        "export const LIMIT = 1..2;",
        "export const LIMIT = 1.2.3;",
        "export const LIMIT = 1_;",
        "export const 1LIMIT = 1;",
        "export const LI-MIT = 1;",
    ] {
        assert!(!is_module_scalar_constant(line), "{line}");
    }
}

#[test]
fn a_test_expecting_the_edited_literal_does_not_credit_the_edit() -> Result<(), String> {
    for (label, line, expected) in [
        ("same-num", "export const LIMIT = 11;", "11"),
        ("same-str", "export const LIMIT = \"ready\";", "'ready'"),
        ("same-bool", "export const LIMIT = true;", "true"),
    ] {
        let finding =
            const_finding_expecting(label, &format!("{line}\n"), "LIMIT", line, expected)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    }
    // The previous value is pinned instead: the edit is caught.
    let line = "export const LIMIT = 11;";
    let finding = const_finding_expecting("old", &format!("{line}\n"), "LIMIT", line, "10")?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn numeric_literal_spellings_are_scalar_constants() {
    use super::super::probe_shape::is_module_scalar_constant;
    for literal in [
        "0", "10", "1_000", "1.5", "5.", "1e3", "1E-3", "2.5e+10", "0x1F", "0b101", "0o17", "10n",
        "-1.5",
    ] {
        let line = format!("export const LIMIT = {literal};");
        assert!(is_module_scalar_constant(&line), "{line}");
    }
}
