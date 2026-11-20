//! #15393: a changed default parameter value no related call lets apply.

use super::super::unused_default_guard::{
    argument_is_definitely_defined, changed_default_parameter,
};
use super::*;

const PICK: &str = "export function pick(\n  name: string,\n  opts: { level: string } = { level: 'AA' },\n): string {\n  return name + opts.level;\n}\n";
const SCALE: &str =
    "export function scale(a: number, b: number = 2): number {\n  return a * b;\n}\n";

fn finding_at(
    label: &str,
    source: &str,
    line: usize,
    removed: &str,
    added: &str,
    files: &[(&str, &str)],
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(
        &root.join("src/def.ts"),
        &source.replacen(removed, added, 1),
    )?;
    for (path, text) in files {
        ts_write_file(&root.join(path), text)?;
    }
    let changed = ChangedFile {
        path: PathBuf::from("src/def.ts"),
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

const PICK_OLD: &str = "  opts: { level: string } = { level: 'AA' },";
const PICK_NEW: &str = "  opts: { level: string } = { level: 'AAA' },";

fn pick_test(call: &str, expected: &str) -> String {
    format!(
        "import {{ it, expect }} from 'vitest';\nimport {{ pick }} from '../src/def';\n\nit('picks', () => {{\n  expect({call}).toBe({expected});\n}});\n"
    )
}

/// tinycolor `isReadable`: every call passes the defaulted argument, so the
/// changed default on its own signature line never applies.
#[test]
fn a_default_every_call_fills_is_not_exposed() -> Result<(), String> {
    let finding = finding_at(
        "default-filled",
        PICK,
        3,
        PICK_OLD,
        PICK_NEW,
        &[(
            "tests/def.test.ts",
            &pick_test("pick('x', { level: 'B' })", "'xB'"),
        )],
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{:?}",
        finding.missing
    );
    assert!(
        finding.missing.iter().any(
            |line| line.contains("only moves the default value of `opts`")
                && line.contains("`(\'x\', { level: \'B\' })`")
        ),
        "{:?}",
        finding.missing
    );
    // The one-line signature form reads the same way.
    let scale_test = "import { it, expect } from 'vitest';\nimport { scale } from '../src/def';\n\nit('scales', () => {\n  expect(scale(3, 4)).toBe(12);\n});\n";
    let finding = finding_at(
        "default-filled-one-line",
        SCALE,
        1,
        "export function scale(a: number, b: number = 2): number {",
        "export function scale(a: number, b: number = 3): number {",
        &[("tests/def.test.ts", scale_test)],
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// The discriminating negatives: a call that omits the argument, passes
/// `undefined` or an identifier, or another module calling the owner, can
/// let the default apply, so the verdict stands.
#[test]
fn a_call_that_may_use_the_default_keeps_the_verdict() -> Result<(), String> {
    for (label, call) in [
        ("default-omitted", "pick('x')"),
        ("default-undefined", "pick('x', undefined)"),
    ] {
        let finding = finding_at(
            label,
            PICK,
            3,
            PICK_OLD,
            PICK_NEW,
            &[("tests/def.test.ts", &pick_test(call, "'xAA'"))],
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    let finding = finding_at(
        "default-other-caller",
        PICK,
        3,
        PICK_OLD,
        PICK_NEW,
        &[
            (
                "tests/def.test.ts",
                &pick_test("pick('x', { level: 'B' })", "'xB'"),
            ),
            (
                "src/other.ts",
                "import { pick } from './def';\nexport const usePick = () => pick('y');\n",
            ),
        ],
    )?;
    assert_ne!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "another caller may omit the argument: {:?}",
        finding.missing
    );
    Ok(())
}

/// One scan case: its label, the owner source, and the extra files.
type Case<'a> = (&'a str, &'a str, Vec<(&'a str, &'a str)>);

/// Callers the scan cannot name or skip: a recursive call in the owner's
/// own body, a sibling test without an assertion, an aliased re-export, a
/// default export, and a computed-key reference each keep the verdict.
#[test]
fn a_caller_the_scan_cannot_resolve_keeps_the_verdict() -> Result<(), String> {
    let filled = pick_test("pick('x', { level: 'B' })", "'xB'");
    let recursive = PICK.replacen(
        "  return name + opts.level;",
        "  return name ? pick('') + opts.level : opts.level;",
        1,
    );
    let sibling = format!("{filled}\nit('smoke', () => {{\n  pick('y');\n}});\n");
    let default_export = format!("{PICK}export default pick;\n");
    let computed = "import { it, expect } from 'vitest';\nimport * as m from '../src/def';\n\nit('picks', () => {\n  expect(m.pick('x', { level: 'B' })).toBe('xB');\n  m['pick']('y');\n});\n";
    let cases: [Case<'_>; 5] = [
        (
            "default-recursive",
            &recursive,
            vec![("tests/def.test.ts", &filled)],
        ),
        (
            "default-sibling-test",
            PICK,
            vec![("tests/def.test.ts", &sibling)],
        ),
        (
            "default-aliased-reexport",
            PICK,
            vec![
                ("tests/def.test.ts", &filled),
                ("src/index.ts", "export { pick as choose } from './def';\n"),
                (
                    "src/use.ts",
                    "import { choose } from './index';\nexport const run = () => choose('y');\n",
                ),
            ],
        ),
        (
            "default-default-export",
            &default_export,
            vec![("tests/def.test.ts", &filled)],
        ),
        (
            "default-computed-key",
            PICK,
            vec![("tests/def.test.ts", computed)],
        ),
    ];
    for (label, source, files) in cases {
        let finding = finding_at(label, source, 3, PICK_OLD, PICK_NEW, &files)?;
        assert_ne!(
            finding.class,
            ExposureClass::ReachableUnrevealed,
            "{label}: {:?}",
            finding.missing
        );
        assert!(
            !finding
                .missing
                .iter()
                .any(|line| line.contains("only moves the default value")),
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

#[test]
fn only_a_lone_default_value_edit_is_located() {
    let file = Path::new("src/def.ts");
    let source = PICK.replacen(PICK_OLD, PICK_NEW, 1);
    assert_eq!(
        changed_default_parameter(file, &source, 2, PICK_NEW, PICK_OLD).map(|change| change.index),
        Some(1)
    );
    // A renamed parameter, a body line, or an added default is not one.
    assert_eq!(
        changed_default_parameter(
            file,
            &source,
            2,
            PICK_NEW,
            "  options: { level: string } = { level: 'AA' },"
        ),
        None
    );
    assert_eq!(
        changed_default_parameter(
            file,
            &source,
            4,
            "  return name + opts.level;",
            "  return opts.level;"
        ),
        None
    );
    assert_eq!(
        changed_default_parameter(file, &source, 2, PICK_NEW, "  opts: { level: string },"),
        None
    );
}

#[test]
fn only_arguments_that_cannot_be_undefined_fill_a_default() {
    for argument in [
        "{ level: 'B' }",
        "{}",
        "[1, 2]",
        "'x'",
        "`t`",
        "0",
        "-1.5",
        "true",
        "null",
        "() => 1",
        "function () {}",
        "new Map()",
        "{ level: 'ZZZ' } as any",
        "{} satisfies Opts",
        "[1] as number[]",
    ] {
        assert!(argument_is_definitely_defined(argument), "{argument}");
    }
    for argument in [
        "undefined",
        "void 0",
        "opts",
        "opts.level",
        "make()",
        "...rest",
        "a ?? b",
        "{ a: 1 }.b",
        "new Foo().bar",
        "opts as Opts",
        "undefined!",
        "make() as Opts",
        "new Foo().get()",
        "new Foo() && undefined",
        "new Foo() ? undefined : 1",
        "",
    ] {
        assert!(!argument_is_definitely_defined(argument), "{argument}");
    }
}

/// The common layout: the test sits in a `describe` titled with the owner's
/// name, and that title is not a caller.
#[test]
fn a_describe_titled_with_the_owner_name_is_not_a_caller() -> Result<(), String> {
    let test = "import { describe, it, expect } from 'vitest';\nimport { pick } from '../src/def';\n\ndescribe('pick', () => {\n  it('joins the level', () => {\n    expect(pick('x', { level: 'B' })).toBe('xB');\n  });\n});\n";
    let finding = finding_at(
        "default-describe",
        PICK,
        3,
        PICK_OLD,
        PICK_NEW,
        &[("tests/def.test.ts", test)],
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// tinycolor's layout: a multi-line import list and a cast argument. An
/// import that renames a binding keeps the verdict.
#[test]
fn a_multi_line_import_is_not_a_caller_but_a_renaming_one_refuses() -> Result<(), String> {
    let test = |import: &str| {
        format!(
            "import {{ describe, expect, it }} from 'vitest';\n{import}\n\ndescribe('pick', () => {{\n  it('joins', () => {{\n    expect(pick('x', {{ level: 'B' }} as any)).toBe('xB');\n  }});\n}});\n"
        )
    };
    let plain = test("import {\n  other,\n  pick,\n} from '../src/def';");
    let finding = finding_at(
        "default-multiline-import",
        PICK,
        3,
        PICK_OLD,
        PICK_NEW,
        &[("tests/def.test.ts", &plain)],
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{:?}",
        finding.missing
    );
    let renamed = test(
        "import {\n  pick,\n  pick as choose,\n} from '../src/def';\nconst run = () => choose('y');",
    );
    let finding = finding_at(
        "default-renaming-import",
        PICK,
        3,
        PICK_OLD,
        PICK_NEW,
        &[("tests/def.test.ts", &renamed)],
    )?;
    assert_ne!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// tinycolor's other half: the test imports the owner's own name through an
/// `export *` barrel and is titled with that name. A barrel that renames the
/// owner keeps the verdict.
#[test]
fn a_barrel_import_under_the_owner_name_is_followed() -> Result<(), String> {
    let test = "import { it, expect } from 'vitest';\nimport { pick } from '../src/index';\n\nit('pick', () => {\n  expect(pick('x', { level: 'B' })).toBe('xB');\n});\n";
    let finding = finding_at(
        "default-barrel",
        PICK,
        3,
        PICK_OLD,
        PICK_NEW,
        &[
            ("tests/def.test.ts", test),
            ("src/index.ts", "export * from './def';\n"),
        ],
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::ReachableUnrevealed,
        "{:?}",
        finding.missing
    );
    Ok(())
}
