//! Registration provenance must precede oracle credit (#8959, SPEC-0243).
use super::*;

const OWNER: &str = "export function sumAll(values: number[]): number {\n  return values.reduce((a, b) => a + b, 0);\n}\n";
const IMPORT: &str = "import { sumAll } from '../src/sum';\n";
const TEST: &str = "test('adds', () => { expect(sumAll([2, 3, 4])).toBe(9); });";
const NOOP: &str = "(_name: string, _callback: () => void) => {}";

fn registration_finding(label: &str, source: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/sum.ts"), OWNER)?;
    ts_write_file(
        &root.join("tests/sum.test.ts"),
        &format!("{IMPORT}{source}"),
    )?;
    let owners = extract_owners(Path::new("src/sum.ts"), OWNER);
    assert_eq!(owners.len(), 1);
    assert_eq!(owners[0].name, "sumAll");
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(
            "src/sum.ts",
            &[(2, "  return values.reduce((a, b) => a + b, 0);")],
        )],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let findings: Vec<_> = result
        .findings
        .into_iter()
        .filter(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("sumAll"))
        })
        .collect();
    let [finding] = findings.as_slice() else {
        return Err(format!(
            "{label}: expected exactly one sumAll finding: {findings:?}"
        ));
    };
    Ok(finding.clone())
}

#[test]
fn registration_bindings_keep_imported_and_global_controls() -> Result<(), String> {
    for prelude in [
        "import { describe, test, expect } from 'vitest';",
        "import { describe, it as test, expect } from '@jest/globals';",
        "const { describe, test } = require('node:test');",
        "",
    ] {
        let source = format!("{prelude}\ndescribe('sum', () => {{ {TEST} }});");
        let finding = registration_finding("registration-positive", &source)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{source}");
        assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
    }
    Ok(())
}

#[test]
fn registration_bindings_reject_local_noop_callbacks() -> Result<(), String> {
    for source in [
        format!("const describe = {NOOP}; describe('sum', () => {{ {TEST} }});"),
        format!(
            "import {{ describe }} from 'vitest'; const test = {NOOP}; describe('sum', () => {{ {TEST} }});"
        ),
        format!(
            "import {{ describe, test }} from 'vitest'; describe('sum', () => {{ const test = {NOOP}; {TEST} }});"
        ),
        format!("describe('sum', (test) => {{ {TEST} }});"),
        format!("{{ var test = {NOOP}; }} describe('sum', () => {{ {TEST} }});"),
        format!(
            "describe('sum', () => {{ {{ var test = {NOOP}; }} describe('inner', () => {{ {TEST} }}); }});"
        ),
        format!("import {{ describe as test }} from 'vitest'; {TEST}"),
        format!("import {{ test }} from './local-helper'; {TEST}"),
        format!("enum test {{ A }}; {TEST}"),
        format!("export enum test {{ A }}; {TEST}"),
        format!("export namespace test {{ export const value = 1; }} {TEST}"),
        format!("namespace test {{ export const value = 1; }} {TEST}"),
        format!("describe('setup', () => {{ test = {NOOP}; }}); await {TEST}"),
        format!("describe('setup', () => {{ test = {NOOP}; }}); {TEST}"),
    ] {
        assert_eq!(
            registration_finding("registration-noop", &source)?.class,
            ExposureClass::NoStaticPath,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn registration_bindings_preserve_unrelated_valid_evidence() -> Result<(), String> {
    for source in [
        format!(
            "describe('shadow', () => {{ const test = {NOOP}; {TEST} }}); describe('real', () => {{ {TEST} }});"
        ),
        format!("{{ const test = {NOOP}; }} describe('real', () => {{ {TEST} }});"),
        format!("{{ function test() {{}} }} describe('real', () => {{ {TEST} }});"),
        format!(
            "describe('shadow', () => {{ {{ let test; test = {NOOP}; }} }}); describe('real', () => {{ {TEST} }});"
        ),
        format!(
            "describe('shadow', () => {{ {{ var test; }} test = {NOOP}; }}); describe('real', () => {{ {TEST} }});"
        ),
        format!("[1].forEach(() => {{ {TEST} }});"),
        format!("describe('real', () => {{ [1].forEach(() => {{ {TEST} }}); }});"),
    ] {
        let tests = extract_tests(Path::new("tests/sum.test.ts"), &format!("{IMPORT}{source}"));
        assert_eq!(tests.len(), 1, "only the genuine test survives: {source}");
        assert_eq!(tests[0].assertions.len(), 1);
        let finding = registration_finding("registration-independent", &source)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{source}");
        assert_eq!(finding.related_tests.len(), 1);
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
    }
    Ok(())
}

#[test]
fn registration_bindings_do_not_execute_uncalled_closures_or_factories() -> Result<(), String> {
    for source in [
        format!("describe('sum', () => {{ const unused = () => {{ {TEST} }}; }});"),
        format!("function register() {{ describe('sum', () => {{ {TEST} }}); }}"),
        format!(
            "const describe = {NOOP}; function register() {{ describe('sum', () => {{ {TEST} }}); }} register();"
        ),
    ] {
        assert_eq!(
            registration_finding("registration-uncalled", &source)?.class,
            ExposureClass::NoStaticPath,
            "{source}"
        );
    }
    Ok(())
}

/// Owner reach currently scans a genuine test's body text. The unused
/// closure supplies no oracle, but correcting that reach is separate from
/// registration provenance (#8959).
#[test]
fn registration_bindings_uncalled_test_closure_has_no_oracle() -> Result<(), String> {
    let source =
        "test('uncalled', () => { const unused = () => { expect(sumAll([2, 3, 4])).toBe(9); }; });";
    let tests = extract_tests(Path::new("tests/sum.test.ts"), source);
    assert_eq!(tests.len(), 1);
    assert!(tests[0].assertions.is_empty());
    let finding = registration_finding("registration-unused-oracle", source)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::Unknown);
    Ok(())
}

#[test]
fn registration_bindings_distinguish_script_hoisting_from_strict_blocks() {
    for (directive, count) in [("", 0), ("'use strict';", 1)] {
        let source =
            format!("{directive} {{ function test() {{}} }} describe('sum', () => {{ {TEST} }});");
        let tests = extract_tests(Path::new("tests/sum.test.cjs"), &source);
        assert_eq!(tests.len(), count, "{source}");
    }
}
