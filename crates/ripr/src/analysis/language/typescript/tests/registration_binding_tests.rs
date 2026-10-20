//! Registration provenance must precede oracle credit (#8959, SPEC-0243).
use super::*;

const OWNER: &str = "export function sumAll(values: number[]): number {\n  return values.reduce((a, b) => a + b, 0);\n}\n";
const IMPORT: &str = "import { sumAll } from '../src/sum';\n";
const TEST: &str = "test('adds', () => { expect(sumAll([2, 3, 4])).toBe(9); });";
const NOOP: &str = "(_name: string, _callback: () => void) => {}";

fn registration_finding(label: &str, source: &str) -> Result<Finding, String> {
    let allocator = oxc_allocator::Allocator::default();
    let input = format!("{IMPORT}{source}");
    let parsed = Parser::new(
        &allocator,
        &input,
        source_type_for(Path::new("tests/sum.test.ts")),
    )
    .parse();
    assert!(parsed.errors.is_empty(), "{label}: {:?}", parsed.errors);
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
        "const describe = require('node:test').describe; const test = require('node:test').test;",
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
        format!("import test = require('./helper'); {TEST}"),
        format!("import test = helpers.noop; {TEST}"),
        format!("export import test = helpers.noop; {TEST}"),
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

/// A function value that is never referenced does not execute its owner call.
/// Matching titles must not recreate a relation through heuristic fallback.
#[test]
fn registration_bindings_uncalled_test_closure_has_no_oracle() -> Result<(), String> {
    for closure in [
        "() => { expect(sumAll([2, 3, 4])).toBe(9); }",
        "() => expect(sumAll([2, 3, 4])).toBe(9)",
        "function () { expect(sumAll([2, 3, 4])).toBe(9); }",
    ] {
        for body in [
            format!("const unused = {closure};"),
            format!("if (true) {{ const unused = {closure}; }}"),
        ] {
            let source =
                format!("describe('sumAll', () => {{ test('sumAll', () => {{ {body} }}); }});");
            let tests = extract_tests(Path::new("tests/sum.test.ts"), &source);
            assert_eq!(tests.len(), 1);
            assert!(tests[0].assertions.is_empty());
            assert!(tests[0].body_text.contains("sumAll([2, 3, 4])"));
            let finding = registration_finding("registration-unused-owner", &source)?;
            assert_eq!(finding.class, ExposureClass::NoStaticPath, "{source}");
            assert!(finding.related_tests.is_empty(), "{finding:?}");
        }
    }
    Ok(())
}

/// WeaklyExposed/Unknown is correct when the owner executes but no live
/// oracle observes its result. Do not turn this distinct case into NoStaticPath.
#[test]
fn registration_bindings_reached_owner_without_live_oracle_stays_weak() -> Result<(), String> {
    for body in [
        "const value = sumAll([2, 3, 4]); const unused = () => { expect(value).toBe(9); };",
        "const run = () => sumAll([2, 3, 4]); run();",
        "const run = () => sumAll([2, 3, 4]); const alias = run; alias();",
        "const consume = (fn) => fn(); const run = () => sumAll([2, 3, 4]); consume(run);",
    ] {
        for title in ["called", "éééééééééééééééééééééééééééééééé"]
        {
            let source = format!("test('{title}', () => {{ {body} }});");
            let finding = registration_finding("registration-live-owner", &source)?;
            assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{source}");
            assert_eq!(finding.related_tests.len(), 1);
            assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::Unknown);
            assert_eq!(
                finding.related_tests[0].oracle_strength,
                OracleStrength::Unknown
            );
        }
    }
    Ok(())
}

#[test]
fn registration_bindings_unused_closure_preserves_live_owner_oracle() -> Result<(), String> {
    for body in [
        "const unused = () => sumAll([0]); expect(sumAll([2, 3, 4])).toBe(9);",
        "const unused = () => sumAll([0]), value = sumAll([2, 3, 4]); expect(value).toBe(9);",
    ] {
        let source = format!("test('sumAll', () => {{ {body} }});");
        let finding = registration_finding("registration-live-oracle", &source)?;
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
fn registration_bindings_distinguish_script_hoisting_from_strict_blocks() {
    for (directive, count) in [("", 0), ("'use strict';", 1)] {
        let source =
            format!("{directive} {{ function test() {{}} }} describe('sum', () => {{ {TEST} }});");
        let tests = extract_tests(Path::new("tests/sum.test.cjs"), &source);
        assert_eq!(tests.len(), count, "{source}");
    }
}

#[test]
fn registration_bindings_keep_verified_runner_extensions() -> Result<(), String> {
    for runner in ["vitest", "@playwright/test"] {
        let source = format!(
            "import {{ test as base, expect }} from '{runner}'; const test = base.extend({{}}); {TEST}"
        );
        let finding = registration_finding("registration-runner-extend", &source)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{source}");
        assert_eq!(finding.related_tests.len(), 1);
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            OracleStrength::Strong
        );
    }
    for prelude in [
        format!("const base = {{ extend: () => {NOOP} }}; const test = base.extend({{}});"),
        "import { test as base } from './helper'; const test = base.extend({});".to_string(),
        "import { describe as base } from 'vitest'; const test = base.extend({});".to_string(),
        "import { test as base } from 'vitest'; let test = base.extend({});".to_string(),
        format!(
            "import {{ test as base }} from 'vitest'; base.extend = () => {NOOP}; const test = base.extend({{}});"
        ),
        format!(
            "import {{ test as base }} from 'vitest'; base['extend'] = () => {NOOP}; const test = base.extend({{}});"
        ),
        format!(
            "import {{ test as base }} from 'vitest'; const test = base.extend({{}}); test = {NOOP};"
        ),
    ] {
        let source = format!("{prelude} {TEST}");
        let finding = registration_finding("registration-unverified-extend", &source)?;
        assert_eq!(finding.class, ExposureClass::NoStaticPath, "{source}");
        assert!(finding.related_tests.is_empty());
    }
    Ok(())
}
