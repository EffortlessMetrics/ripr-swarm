//! #15947: a tool configuration script (`jest.config.ts`, `vitest.config.*`)
//! is loaded by its runner, never imported by a test, so its fields are not
//! finding subjects. Mined from vercel/ms, where a `jest.config.ts` edit read
//! as nine pieces of code with no test path.

use super::*;

/// The vercel/ms shape: a named, typed config object exported by default.
const CONFIG: &str = "import type { Config } from 'jest';\n\nconst config: Config = {\n  testEnvironment: 'node',\n  coverageThreshold: { global: { lines: 90 } },\n  collectCoverage: process.env.CI === 'true',\n};\n\nexport default config;\n";
const CHANGED_LINE: (usize, &str) = (6, "  collectCoverage: process.env.CI === 'true',");

fn run(label: &str, path: &str, handwritten: &[&str]) -> Result<LanguageDiffResult, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join(path), CONFIG)?;
    ts_write_file(
        &root.join("tests/ms.test.ts"),
        "import { expect, test } from '@jest/globals';\n\ntest('ok', () => {\n  expect(1 + 1).toBe(2);\n});\n",
    )?;
    let result = TypeScriptAdapter::analyze_diff_with_read_limits_and_handwritten(
        &ts_analysis_options(root.clone()),
        &[changed_with_lines(path, &[CHANGED_LINE])],
        ts_file_read_limit(),
        ts_workspace_read_budget(),
        &crate::config::TypescriptLanguageConfig {
            handwritten_files: handwritten.iter().map(|entry| entry.to_string()).collect(),
            ..crate::config::TypescriptLanguageConfig::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);
    result
}

#[test]
fn a_changed_tool_config_field_is_not_a_finding_subject() -> Result<(), String> {
    for path in [
        "jest.config.ts",
        "packages/core/vitest.config.ts",
        "vitest.config.e2e.ts",
    ] {
        let result = run("tool-config-refused", path, &[])?;
        if !result.findings.is_empty() || result.changed_files != 0 {
            return Err(format!(
                "{path}: expected no finding and no counted file, got {} finding(s), {} file(s)",
                result.findings.len(),
                result.changed_files
            ));
        }
    }
    Ok(())
}

#[test]
fn the_same_object_in_an_ordinary_module_is_still_reported() -> Result<(), String> {
    // The control that keeps the refusal honest: the stimulus above does
    // produce a finding when the file is product source, so the empty result
    // for `jest.config.ts` is the path rule and not an unprobed shape.
    for path in ["src/config.ts", "src/app.config.ts", "jest.setup.ts"] {
        let result = run("tool-config-near-miss", path, &[])?;
        if result.changed_files != 1 || result.findings.is_empty() {
            return Err(format!(
                "{path}: expected one counted file with a finding, got {} file(s), {} finding(s)",
                result.changed_files,
                result.findings.len()
            ));
        }
    }
    Ok(())
}

#[test]
fn a_declared_handwritten_tool_config_is_analyzed_again() -> Result<(), String> {
    let result = run(
        "tool-config-declared",
        "jest.config.ts",
        &["jest.config.ts"],
    )?;
    if result.changed_files != 1 || result.findings.is_empty() {
        return Err(format!(
            "expected the declared config to be counted and reported, got {} file(s), {} finding(s)",
            result.changed_files,
            result.findings.len()
        ));
    }
    Ok(())
}
