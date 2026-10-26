//! Registration provenance must precede oracle credit (#8959, SPEC-0243).
use super::*;

const OWNER: &str = "export function sumAll(values: number[]): number {\n  return values.reduce((a, b) => a + b, 0);\n}\n";
const IMPORT: &str = "import { sumAll } from '../src/sum';\n";
const TEST: &str = "test('adds', () => { expect(sumAll([2, 3, 4])).toBe(9); });";
const NOOP: &str = "(_name: string, _callback: () => void) => {}";

fn registration_finding(label: &str, source: &str) -> Result<Finding, String> {
    registration_finding_at(label, "tests/sum.test.ts", &format!("{IMPORT}{source}"))
}

fn registration_finding_at(label: &str, file: &str, input: &str) -> Result<Finding, String> {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = Parser::new(&allocator, input, source_type_for(Path::new(file))).parse();
    assert!(parsed.errors.is_empty(), "{label}: {:?}", parsed.errors);
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/sum.ts"), OWNER)?;
    ts_write_file(&root.join(file), input)?;
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

/// #10477: a registration picked off a whole-module `require()` namespace
/// relates like the destructured form, across declarations.
#[test]
fn registration_picked_off_a_require_namespace_is_a_known_runner() -> Result<(), String> {
    for prelude in [
        "const runner = require('node:test'); const test = runner.test;",
        "const runner = require('node:test'); const { test } = runner;",
        "const runner = require('node:test'), test = runner.test;",
        "const runner = require('node:test'); const { describe, test } = runner;",
    ] {
        let source = format!("{prelude}\n{TEST}");
        let finding = registration_finding("registration-namespace-pick", &source)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{source}");
        assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
    }
    Ok(())
}

/// #10477: the namespace may be spelled like the export it picks, and the
/// registration then goes through the renamed local only.
#[test]
fn a_namespace_spelled_like_its_picked_export_still_registers_the_renamed_local()
-> Result<(), String> {
    let source = format!(
        "const test = require('node:test'); const {{ test: it }} = test;\n{}",
        TEST.replacen("test(", "it(", 1)
    );
    let finding = registration_finding("registration-namespace-keyname", &source)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{source}");
    assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
    Ok(())
}

/// #10477 negatives: a non-runner module, a namespace that is written, escaped
/// or rebound, and a `let` namespace never make the picked local a runner.
#[test]
fn registration_picked_off_an_unproven_require_namespace_stays_unrelated() -> Result<(), String> {
    for prelude in [
        "const helpers = require('./helpers'); const test = helpers.test;",
        "const runner = require('node:test'); const test = runner.test; runner.test = () => {};",
        "const runner = require('node:test'), test = runner.test; runner.test = () => {};",
        "const runner = require('node:test'); const test = runner.test; globalThis.r = runner;",
        "let runner = require('node:test'); const test = runner.test; runner = {};",
        "const runner = require('node:test'); const runner2 = runner; const test = runner2.test;",
        "const runner = require('node:test'); const test = runner?.test;",
        "const runner = require('node:test'); const { test = () => {} } = runner;",
        // `.default` is not a native CommonJS export (rule 22): no pick, any spelling.
        "const runner = require('node:test'); const test = runner.default;",
        "const runner = require('node:test'); const { default: test } = runner;",
        "const runner = require('node:test'); const test = runner['default'];",
        "const runner = require('node:test'); const { 'default': test } = runner;",
        "const runner = require('node:test'); const { ['default']: test } = runner;",
        // Nested patterns and members that are not runner exports.
        "const runner = require('node:test'); const { a: { test } } = runner;",
        "const runner = require('node:test'); const test = runner.mock;",
        "const runner = require('node:test'); const test = runner.test.skip;",
        // Guards the shared extractor also applies: delete, assign, define, call through.
        "const runner = require('node:test'); const test = runner.test; delete runner.test;",
        "const runner = require('node:test'); const test = runner.test; Object.assign(runner, {});",
        "const runner = require('node:test'); const test = runner.test; Object.defineProperty(runner, 'test', {});",
        "const runner = require('node:test'); const test = runner.test; runner.mock.reset();",
        "const runner = require('node:test'); const test = runner.test; runner.test.bind(runner);",
        // Only a `const` pick of a `const` namespace, declared in order, counts.
        "const runner = require('node:test'); var test = runner.test;",
        "const runner = require('node:test'); let test = runner.test;",
        "const test = runner.test; const runner = require('node:test');",
        "const runner = require('node:test'); const test = runner.test; function swap() { test = () => {}; }",
        "const runner = require('node:test'); const test = runner.test; ({ test } = { test: () => {} });",
        "const runner = require('node:test'); const test = runner.test; for (test of []) {}",
        // The namespace is used in a way the text scan cannot bound.
        "const runner = require('node:test'); const test = runner.test; runner`x`;",
        "const runner = require('node:test'); const test = runner.test; `${runner}`;",
        "const runner = require('node:test'); const test = runner.test; \\u0072unner.mock = () => {};",
        "const runner = require('node:test'); const test = runner.test; void typeof runner;",
        "const runner = require('node:test'); const test = runner.test; eval('runner.mock = 1');",
        "import runner from 'node:test'; const test = runner.test;",
        // A forward pick reads the namespace in its dead zone.
        "const early = runner.test; const runner = require('node:test'); const test = runner.test;",
        "const early = runner.test; const runner = require('node:test'), test = runner.test;",
        // Every write to the picked local, wrapped or not, withdraws the pick.
        "const runner = require('node:test'); const test = runner.test; (test) = () => {};",
        "const runner = require('node:test'); const test = runner.test; (test as any) = () => {};",
        "const runner = require('node:test'); const test = runner.test; [test] = [() => {}];",
        "const runner = require('node:test'); const test = runner.test; ({ a: test } = { a: () => {} });",
        "const runner = require('node:test'); const test = runner.test; test++;",
        "const runner = require('node:test'); const test = runner.test; test += 1;",
        "const runner = require('node:test'); const test = runner.test; for (test of []) {}",
        // Replacing, deleting or escaping the picked local's members or value.
        "const runner = require('node:test'); const test = runner.test; test.only = () => {};",
        "const runner = require('node:test'); const test = runner.test; test.x = 1;",
        "const runner = require('node:test'); const test = runner.test; delete test.only;",
        "const runner = require('node:test'); const test = runner.test; Object.assign(test, {});",
        "const runner = require('node:test'); const test = runner.test; const copy = test;",
    ] {
        let source = format!("{prelude}\n{TEST}");
        assert_eq!(
            registration_finding("registration-namespace-negative", &source)?.class,
            ExposureClass::NoStaticPath,
            "{source}"
        );
    }
    Ok(())
}

/// #10477: a registration that runs before the pick declares its local reads a
/// dead binding, so it earns nothing; a modifier call after the pick still does.
#[test]
fn a_namespace_pick_registers_only_calls_after_its_declaration() -> Result<(), String> {
    let before = format!("const runner = require('node:test');\n{TEST}\nconst test = runner.test;");
    assert_eq!(
        registration_finding("registration-namespace-order", &before)?.class,
        ExposureClass::NoStaticPath,
        "{before}"
    );
    let modifier = format!(
        "const runner = require('node:test'); const test = runner.test;\n{}",
        TEST.replacen("test(", "test.only(", 1)
    );
    let finding = registration_finding("registration-namespace-modifier", &modifier)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{modifier}");
    assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
    Ok(())
}

/// #10477: renaming an inactive runner member into an active-looking local
/// must not register (`xit`, `xdescribe`): the extraction guards read the local
/// spelling, so the pick refuses inactive members. Each pair keeps the active
/// twin credited.
#[test]
fn a_namespace_pick_never_renames_an_inactive_runner_member() -> Result<(), String> {
    let suite = "describe('pending', () => { test('adds', () => { expect(sumAll([2, 3, 4])).toBe(9); }); });";
    for (inactive, active, body) in [
        (
            "const runner = require('mocha'); const test = runner.xit;",
            "const runner = require('mocha'); const test = runner.it;",
            TEST,
        ),
        (
            "const runner = require('mocha'); const { xit: test } = runner;",
            "const runner = require('mocha'); const { it: test } = runner;",
            TEST,
        ),
        (
            "const runner = require('mocha'), test = runner.xit;",
            "const runner = require('mocha'), test = runner.it;",
            TEST,
        ),
        (
            "const runner = require('mocha'); const describe = runner.xdescribe; const test = runner.it;",
            "const runner = require('mocha'); const describe = runner.describe; const test = runner.it;",
            suite,
        ),
        (
            "const runner = require('mocha'); const { xdescribe: describe, it: test } = runner;",
            "const runner = require('mocha'); const { describe, it: test } = runner;",
            suite,
        ),
    ] {
        let inactive_source = format!("{inactive}\n{body}");
        let tests = extract_tests(
            Path::new("tests/sum.test.ts"),
            &format!("{IMPORT}{inactive_source}"),
        );
        assert!(tests.is_empty(), "{inactive_source}: {tests:?}");
        assert_eq!(
            registration_finding("registration-namespace-inactive", &inactive_source)?.class,
            ExposureClass::NoStaticPath,
            "{inactive_source}"
        );
        let active_source = format!("{active}\n{body}");
        let finding = registration_finding("registration-namespace-active", &active_source)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{active_source}");
        assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
        assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
    }
    // A file with an inactive and an active registration keeps only the active one.
    let mixed = format!(
        "const runner = require('mocha'); const test = runner.it; const skipped = runner.xit;\n{TEST}\n{}",
        TEST.replacen("test(", "skipped(", 1)
    );
    let finding = registration_finding("registration-namespace-mixed", &mixed)?;
    assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
    Ok(())
}

/// #10646: one namespace with 50 picks keeps every verdict. The picks mix the
/// member, destructured and `{ key: local }` spellings. The production path
/// also asserts each verdict against the pre-#10646 cut-and-rescan reference
/// under `cfg(test)`.
const PICKS: usize = 50;

fn pick_source(extra: &str) -> String {
    let mut source = String::from("const runner = require('node:test');\n");
    for index in 0..PICKS {
        match index % 3 {
            0 => source.push_str(&format!("const t{index} = runner.test;\n")),
            1 => source.push_str(&format!("const {{ test: t{index} }} = runner;\n")),
            _ => source.push_str(&format!("const {{ it: t{index} }} = runner;\n")),
        }
    }
    source.push_str(extra);
    for index in 0..PICKS {
        source.push_str(&format!("t{index}('adds', () => {{}});\n"));
    }
    source
}

fn picked(source: &str) -> Vec<String> {
    super::super::admission::namespace_pick_locals_for_test(source, Path::new("tests/sum.test.ts"))
}

#[test]
fn fifty_namespace_picks_keep_every_verdict() {
    let locals = picked(&pick_source(""));
    let expected: Vec<String> = (0..PICKS).map(|index| format!("t{index}")).collect();
    assert_eq!(locals, expected);
}

/// #10646: one written (or value-used) pick refuses alone; the other 49 stay.
#[test]
fn one_written_pick_among_fifty_refuses_alone() {
    for written in [0, 17, 49] {
        for extra in [
            format!("t{written} = () => {{}};\n"),
            format!("t{written}++;\n"),
            format!("const copy = t{written};\n"),
            format!("t{written}.only = 1;\n"),
        ] {
            let locals = picked(&pick_source(&extra));
            let expected: Vec<String> = (0..PICKS)
                .filter(|index| *index != written)
                .map(|index| format!("t{index}"))
                .collect();
            assert_eq!(locals, expected, "{extra}");
        }
    }
}

/// Removal control: the same file without the extra statement admits all 50,
/// so the refusal above comes from that one statement and not from the shape.
#[test]
fn removing_the_write_restores_the_pick() {
    assert_eq!(picked(&pick_source("")).len(), PICKS);
    assert_eq!(picked(&pick_source("t17.call(1);\n")).len(), PICKS);
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
        "const run = () => sumAll([2, 3, 4]); const value = `ééé ${run()}`;",
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
        "const unused = () => sumAll([0]), value = sumAll([2, 3, 4]); expect(sumAll([2, 3, 4])).toBe(9);",
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
    // The existing alias classifier cannot discriminate this multi-declarator
    // value, but the executed owner and extracted exact oracle must survive.
    let finding = registration_finding(
        "registration-multi-declarator",
        "test('sumAll', () => { const unused = () => sumAll([0]), value = sumAll([2, 3, 4]); expect(value).toBe(9); });",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
    assert_eq!(
        finding.related_tests[0].oracle_strength,
        OracleStrength::Strong
    );
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
        for sibling in ["", "base('unrelated', () => { expect(1).toBe(1); });"] {
            let source = format!(
                "import {{ test as base, expect }} from '{runner}'; const test = base.extend({{}}); {sibling} {TEST}"
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

/// Combined-source control only: #8950 remains independently owned.
#[test]
fn registration_bindings_called_factory_controls() -> Result<(), String> {
    for (prelude, called_class) in [
        (
            "import { describe, test, expect } from 'vitest';".to_string(),
            ExposureClass::Exposed,
        ),
        (String::new(), ExposureClass::Exposed),
        (
            format!("import {{ test, expect }} from 'vitest'; const describe = {NOOP};"),
            ExposureClass::NoStaticPath,
        ),
        (
            format!("import {{ describe, expect }} from 'vitest'; const test = {NOOP};"),
            ExposureClass::NoStaticPath,
        ),
    ] {
        for (call, expected) in [
            ("", ExposureClass::NoStaticPath),
            ("register();", called_class),
        ] {
            let source = format!(
                "{prelude} function register() {{ describe('sum', () => {{ {TEST} }}); }} {call}"
            );
            let finding = registration_finding("registration-factory-integration", &source)?;
            assert_eq!(finding.class, expected, "{source}");
            if expected == ExposureClass::Exposed {
                assert_eq!(finding.related_tests.len(), 1);
                assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
                assert_eq!(
                    finding.related_tests[0].oracle_strength,
                    OracleStrength::Strong
                );
            } else {
                assert!(finding.related_tests.is_empty(), "{finding:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn registration_bindings_named_callback_preserves_live_and_dead_reach() -> Result<(), String> {
    for (body, expected) in [
        (
            "const unused = () => { expect(sumAll([2, 3, 4])).toBe(9); };",
            ExposureClass::NoStaticPath,
        ),
        (
            "const unused = () => sumAll([0]); expect(sumAll([2, 3, 4])).toBe(9);",
            ExposureClass::Exposed,
        ),
    ] {
        let source = format!("function check() {{ {body} }} test('sumAll', check);");
        let finding = registration_finding("registration-named-callback", &source)?;
        assert_eq!(finding.class, expected, "{source}");
        if expected == ExposureClass::NoStaticPath {
            assert!(finding.related_tests.is_empty());
        } else {
            assert_eq!(finding.related_tests.len(), 1);
            assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
            assert_eq!(
                finding.related_tests[0].oracle_strength,
                OracleStrength::Strong
            );
        }
    }
    Ok(())
}

#[test]
fn registration_bindings_dead_closure_containers() -> Result<(), String> {
    for body in [
        "for (const x of [1]) { const unused = () => sumAll([2, 3, 4]); }",
        "for (const x in {a: 1}) { const unused = () => sumAll([2, 3, 4]); }",
        "for (let x = 0; x < 1; x++) { const unused = () => sumAll([2, 3, 4]); }",
        "for (const unused = () => sumAll([2, 3, 4]); false;) {}",
        "while (false) { const unused = () => sumAll([2, 3, 4]); }",
        "do { const unused = () => sumAll([2, 3, 4]); } while (false);",
        "switch (1) { case 1: { const unused = () => sumAll([2, 3, 4]); } }",
        "label: { const unused = () => sumAll([2, 3, 4]); }",
    ] {
        for (live, expected) in [
            ("", ExposureClass::NoStaticPath),
            ("expect(sumAll([2, 3, 4])).toBe(9);", ExposureClass::Exposed),
        ] {
            let source = format!("test('sumAll', () => {{ {body} {live} }});");
            let finding = registration_finding("dead-containers", &source)?;
            assert_eq!(finding.class, expected, "{source}");
            if live.is_empty() {
                assert!(finding.related_tests.is_empty(), "{finding:?}");
            } else {
                assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
                assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
                assert_eq!(
                    finding.related_tests[0].oracle_strength,
                    OracleStrength::Strong
                );
            }
        }
    }
    Ok(())
}

#[test]
fn registration_bindings_callback_strictness_is_local() -> Result<(), String> {
    for callback in ["function ()", "() =>"] {
        for (directive, expected) in [
            ("'use strict';", ExposureClass::Exposed),
            ("", ExposureClass::NoStaticPath),
        ] {
            for registration in ["describe('sum',", "[1].forEach("] {
                let source = format!(
                    "const {{ sumAll }} = require('../src/sum'); describe('first', function () {{ 'use strict'; }}); {registration} {callback} {{ {directive} {{ function test() {{}} }} {TEST} }});"
                );
                let finding =
                    registration_finding_at("callback-strict", "tests/sum.test.cjs", &source)?;
                assert_eq!(finding.class, expected, "{source}");
                assert_eq!(
                    finding.related_tests.len(),
                    usize::from(!directive.is_empty()),
                    "{finding:?}"
                );
                if !directive.is_empty() {
                    assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
                    assert_eq!(
                        finding.related_tests[0].oracle_strength,
                        OracleStrength::Strong
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn registration_bindings_line_comments_end_at_every_js_terminator() -> Result<(), String> {
    for end in ["\n", "\r", "\u{2028}", "\u{2029}"] {
        for (source, expected) in [
            (
                format!(
                    "test('sum', () => {{ const run = () => sumAll([2, 3, 4]); // comment{end}run(); }});"
                ),
                ExposureClass::WeaklyExposed,
            ),
            (
                format!(
                    "test('sum', () => {{ const run = () => sumAll([2, 3, 4]); // comment{end} }});"
                ),
                ExposureClass::NoStaticPath,
            ),
            (
                format!(
                    "import {{ test as base, expect }} from 'vitest'; // comment{end}base.extend = () => () => {{}};\nconst test = base.extend({{}}); {TEST}"
                ),
                ExposureClass::NoStaticPath,
            ),
            (
                format!(
                    "import {{ test as base, expect }} from 'vitest'; // comment{end}const test = base.extend({{}}); {TEST}"
                ),
                ExposureClass::Exposed,
            ),
        ] {
            let finding = registration_finding("comment-terminator", &source)?;
            assert_eq!(finding.class, expected, "{source:?}");
            if expected == ExposureClass::NoStaticPath {
                assert!(finding.related_tests.is_empty(), "{finding:?}");
            } else {
                assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
                if expected == ExposureClass::Exposed {
                    assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
                    assert_eq!(
                        finding.related_tests[0].oracle_strength,
                        OracleStrength::Strong
                    );
                }
            }
        }
    }
    Ok(())
}
