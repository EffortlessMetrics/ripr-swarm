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
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/consts.ts"), owner_source)?;
    let test = format!(
        "import {{ expect, it }} from 'vitest';\nimport {{ {name} }} from '../src/consts';\n\nit('keeps it', () => {{\n  expect({name}).toBe('x');\n}});\n"
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
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.reach.state, StageState::Yes);
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
    ] {
        assert!(!is_module_scalar_constant(line), "{line}");
    }
}
