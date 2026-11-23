//! Barrels a test reaches the owner through without an `export ... from`:
//!
//! - import-then-export (#15922): `import { f } from './x'; export { f };`, a
//!   renamed export list entry and `export const g = f`. An import binding
//!   cannot be reassigned, so an export list entry is the imported module's
//!   binding as `export { f } from './x'` is, and `export const g = f` is a
//!   `const` initialized from it. Both resolve through the same bounded
//!   chain.
//! - a namespace import of a barrel (#15923): `import * as lib from '../src';
//!   lib.f()`. The member name is the exported name, so it resolves like a
//!   named import, but only when the whole test file uses the namespace as a
//!   member receiver it never writes.
//!
//! Every case runs `TypeScriptAdapter::analyze_diff` over a real workspace:
//! the positive rows must reach `exposed` through `re_export_chain_followed`
//! and every shape that could make the called binding something else must
//! stay `no_static_path` with no related test.

use super::*;
use crate::domain::RelationReason;

const CHANGED: &str = "    if (amount > threshold) {";

const OWNER: &str = "export function applyDiscount(amount: number, threshold: number): number {\n    if (amount > threshold) {\n        return amount - 10;\n    }\n    return amount;\n}\n";

const IMPORT_THEN_EXPORT: &str =
    "import { applyDiscount } from './discount';\nexport { applyDiscount };\n";

const STAR_BARREL: &str = "export * from './discount';\n";

/// One fixture row: label, workspace files, test file.
type Row<'a> = (&'a str, &'a [(&'a str, &'a str)], String);

/// A vitest file with `imports`, file-level `setup` and one test whose body
/// is `body`.
fn test_file(imports: &str, setup: &str, body: &str) -> String {
    format!(
        "import {{ describe, it, expect, vi, beforeEach }} from 'vitest';\n{imports}\n{setup}\ndescribe('discount', () => {{\n  it('keeps amount at threshold', () => {{\n    {body}\n  }});\n}});\n"
    )
}

fn named_test() -> String {
    test_file(
        "import { applyDiscount } from '../src';",
        "",
        "expect(applyDiscount(100, 100)).toBe(100);",
    )
}

fn namespace_test(setup: &str, body: &str) -> String {
    test_file("import * as lib from '../src';", setup, body)
}

const NAMESPACE_CALL: &str = "expect(lib.applyDiscount(100, 100)).toBe(100);";

/// The finding for the changed `applyDiscount` boundary in `src/discount.ts`.
fn finding(label: &str, files: &[(&str, &str)], test: &str) -> Result<Finding, String> {
    let owner = files
        .iter()
        .find(|(path, _)| *path == "src/discount.ts")
        .map(|(_, source)| *source)
        .ok_or_else(|| format!("{label}: the fixture has no src/discount.ts"))?;
    let line = owner
        .lines()
        .position(|line| line == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    for (path, source) in files {
        ts_write_file(&root.join(path), source)?;
    }
    ts_write_file(&root.join("test/d.test.ts"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/discount.ts", &[(line, CHANGED)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("applyDiscount"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `applyDiscount`"))
}

fn assert_chain_reached(
    label: &str,
    files: &[(&str, &str)],
    test: &str,
    class: ExposureClass,
) -> Result<(), String> {
    let finding = finding(label, files, test)?;
    assert_eq!(
        finding
            .related_tests
            .iter()
            .map(|related| related.relation_reason)
            .collect::<Vec<_>>(),
        vec![Some(RelationReason::ReExportChainFollowed)],
        "{label}: the test reaches the owner through the barrel"
    );
    assert_eq!(finding.class, class, "{label}");
    Ok(())
}

fn assert_no_static_path(label: &str, files: &[(&str, &str)], test: &str) -> Result<(), String> {
    let finding = finding(label, files, test)?;
    assert!(
        finding.related_tests.is_empty(),
        "{label}: no path is established, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

// ── import-then-export (#15922) ─────────────────────────────────────────────

#[test]
fn import_then_export_barrel_reaches_the_owner() -> Result<(), String> {
    assert_chain_reached(
        "ie-same-name",
        &[
            ("src/discount.ts", OWNER),
            ("src/index.ts", IMPORT_THEN_EXPORT),
        ],
        &named_test(),
        ExposureClass::Exposed,
    )
}

#[test]
fn import_then_export_through_two_barrels_reaches_the_owner() -> Result<(), String> {
    assert_chain_reached(
        "ie-two-hop",
        &[
            ("src/discount.ts", OWNER),
            ("src/mid.ts", IMPORT_THEN_EXPORT),
            (
                "src/index.ts",
                "import { applyDiscount } from './mid';\nexport { applyDiscount };\n",
            ),
        ],
        &named_test(),
        ExposureClass::Exposed,
    )
}

/// A renamed barrel export reads `weakly_exposed` on main for `export { f as
/// g } from` too: the classifier does not read a call of the renamed local as
/// the owner call. The relation is what these rows pin.
#[test]
fn renamed_import_then_export_relates_like_a_renamed_reexport() -> Result<(), String> {
    let renamed_call = test_file(
        "import { discount } from '../src';",
        "",
        "expect(discount(100, 100)).toBe(100);",
    );
    for (label, index) in [
        (
            "ie-explicit-rename-baseline",
            "export { applyDiscount as discount } from './discount';\n",
        ),
        (
            "ie-renamed-list",
            "import { applyDiscount as inner } from './discount';\nexport { inner as discount };\n",
        ),
        (
            "ie-const-alias",
            "import { applyDiscount as inner } from './discount';\nexport const discount = inner;\n",
        ),
    ] {
        assert_chain_reached(
            label,
            &[("src/discount.ts", OWNER), ("src/index.ts", index)],
            &renamed_call,
            ExposureClass::WeaklyExposed,
        )?;
    }
    Ok(())
}

#[test]
fn exported_namespace_import_reaches_the_owner_as_a_member_call() -> Result<(), String> {
    assert_chain_reached(
        "ie-namespace",
        &[
            ("src/discount.ts", OWNER),
            (
                "src/index.ts",
                "import * as pricing from './discount';\nexport { pricing };\n",
            ),
        ],
        &test_file(
            "import { pricing } from '../src';",
            "",
            "expect(pricing.applyDiscount(100, 100)).toBe(100);",
        ),
        ExposureClass::Exposed,
    )
}

#[test]
fn import_then_export_of_something_else_stays_unreached() -> Result<(), String> {
    let other = "export function applyDiscount(amount: number, threshold: number): number {\n    return amount;\n}\n";
    let renamed_call = test_file(
        "import { discount } from '../src';",
        "",
        "expect(discount(100, 100)).toBe(100);",
    );
    let rows: [Row<'_>; 6] = [
        (
            "ie-neg-type-only",
            &[(
                "src/index.ts",
                "import type { applyDiscount } from './discount';\nexport type { applyDiscount };\nexport function other(): number { return 1; }\n",
            )],
            named_test(),
        ),
        (
            "ie-neg-other-module",
            &[
                ("src/other.ts", other),
                (
                    "src/index.ts",
                    "import { applyDiscount } from './other';\nexport { applyDiscount };\n",
                ),
            ],
            named_test(),
        ),
        (
            "ie-neg-own-declaration",
            &[(
                "src/index.ts",
                "import { applyDiscount as real } from './discount';\nexport function applyDiscount(a: number, t: number) { return a; }\nvoid real;\n",
            )],
            named_test(),
        ),
        (
            "ie-neg-let-alias",
            &[(
                "src/index.ts",
                "import { applyDiscount as inner } from './discount';\nexport let discount = inner;\ndiscount = (a: number, t: number) => a;\n",
            )],
            renamed_call.clone(),
        ),
        (
            "ie-neg-wrapped-alias",
            &[(
                "src/index.ts",
                "import { applyDiscount as inner } from './discount';\nexport const discount = wrap(inner);\nfunction wrap(f: unknown) { return (a: number, t: number) => a; }\n",
            )],
            renamed_call,
        ),
        (
            "ie-neg-cycle",
            &[
                (
                    "src/a.ts",
                    "import { applyDiscount } from './index';\nexport { applyDiscount };\n",
                ),
                (
                    "src/index.ts",
                    "import { applyDiscount } from './a';\nexport { applyDiscount };\n",
                ),
            ],
            named_test(),
        ),
    ];
    for (label, extra, test) in rows {
        let mut files = vec![("src/discount.ts", OWNER)];
        files.extend_from_slice(extra);
        assert_no_static_path(label, &files, &test)?;
    }
    Ok(())
}

#[test]
fn import_then_export_chain_past_the_hop_bound_stays_unreached() -> Result<(), String> {
    // index -> h1 -> h2 -> h3 -> h4 -> discount is five hops; the bound is 4.
    let hop = |from: &str| {
        format!("import {{ applyDiscount }} from './{from}';\nexport {{ applyDiscount }};\n")
    };
    let (h4, h3, h2, h1, index) = (hop("discount"), hop("h4"), hop("h3"), hop("h2"), hop("h1"));
    assert_no_static_path(
        "ie-neg-five-hops",
        &[
            ("src/discount.ts", OWNER),
            ("src/h4.ts", &h4),
            ("src/h3.ts", &h3),
            ("src/h2.ts", &h2),
            ("src/h1.ts", &h1),
            ("src/index.ts", &index),
        ],
        &named_test(),
    )?;
    // One hop fewer is inside the bound.
    assert_chain_reached(
        "ie-four-hops",
        &[
            ("src/discount.ts", OWNER),
            ("src/h3.ts", &h4),
            ("src/h2.ts", &h2),
            ("src/h1.ts", &h1),
            ("src/index.ts", &index),
        ],
        &named_test(),
        ExposureClass::Exposed,
    )
}

#[test]
fn mocked_module_on_an_import_then_export_chain_stays_unreached() -> Result<(), String> {
    for (label, mock) in [
        ("ie-neg-mock-barrel", "vi.mock('../src');"),
        ("ie-neg-mock-owner", "vi.mock('../src/discount');"),
    ] {
        assert_no_static_path(
            label,
            &[
                ("src/discount.ts", OWNER),
                ("src/index.ts", IMPORT_THEN_EXPORT),
            ],
            &test_file(
                "import { applyDiscount } from '../src';",
                mock,
                "expect(applyDiscount(100, 100)).toBe(100);",
            ),
        )?;
    }
    Ok(())
}

// ── namespace import of a barrel (#15923) ───────────────────────────────────

#[test]
fn namespace_import_of_a_barrel_reaches_the_owner() -> Result<(), String> {
    for (label, index) in [
        ("ns-star", STAR_BARREL),
        ("ns-named", "export { applyDiscount } from './discount';\n"),
        ("ns-import-then-export", IMPORT_THEN_EXPORT),
    ] {
        assert_chain_reached(
            label,
            &[("src/discount.ts", OWNER), ("src/index.ts", index)],
            &namespace_test("", NAMESPACE_CALL),
            ExposureClass::Exposed,
        )?;
    }
    Ok(())
}

/// The usual shape: one namespace of the package root, members of several
/// modules called and read in the same file.
#[test]
fn namespace_members_of_several_modules_resolve_one_by_one() -> Result<(), String> {
    assert_chain_reached(
        "ns-two-modules",
        &[
            ("src/discount.ts", OWNER),
            (
                "src/tax.ts",
                "export function tax(a: number): number { return a * 2; }\nexport const RATE = 2;\n",
            ),
            (
                "src/index.ts",
                "export * from './discount';\nexport * from './tax';\n",
            ),
        ],
        &namespace_test(
            "",
            "expect(lib.applyDiscount(100, 100)).toBe(100);\n    expect(lib.tax(1)).toBe(lib.RATE);",
        ),
        ExposureClass::Exposed,
    )
}

#[test]
fn namespace_whose_member_is_another_binding_stays_unreached() -> Result<(), String> {
    let shadowing_barrel = "export * from './discount';\nexport function applyDiscount(a: number, t: number) { return a; }\n";
    let other = "export function applyDiscount(a: number, t: number): number { return a; }\n";
    let rows: [Row<'_>; 4] = [
        (
            "ns-neg-barrel-own-declaration",
            &[("src/index.ts", shadowing_barrel)],
            namespace_test("", NAMESPACE_CALL),
        ),
        (
            "ns-neg-ambiguous-star",
            &[
                ("src/other.ts", other),
                (
                    "src/index.ts",
                    "export * from './discount';\nexport * from './other';\n",
                ),
            ],
            namespace_test("", NAMESPACE_CALL),
        ),
        (
            "ns-neg-not-exported-by-barrel",
            &[
                (
                    "src/other.ts",
                    "export function tax(a: number): number { return a; }\n",
                ),
                ("src/index.ts", "export * from './other';\n"),
            ],
            namespace_test("", NAMESPACE_CALL),
        ),
        // The member the test calls must be spelled like the owner; a
        // renamed export through a namespace is left unresolved.
        (
            "ns-neg-renamed-member",
            &[(
                "src/index.ts",
                "export { applyDiscount as discount } from './discount';\n",
            )],
            namespace_test("", "expect(lib.discount(100, 100)).toBe(100);"),
        ),
    ];
    for (label, extra, test) in rows {
        let mut files = vec![("src/discount.ts", OWNER)];
        files.extend_from_slice(extra);
        assert_no_static_path(label, &files, &test)?;
    }
    Ok(())
}

/// Any use of the namespace local that is not a never-written member access
/// withholds every member of it, wherever the use sits in the file.
#[test]
fn namespace_used_as_a_value_or_written_stays_unreached() -> Result<(), String> {
    let rows = [
        (
            "ns-neg-spy-in-body",
            "",
            "vi.spyOn(lib, 'applyDiscount').mockReturnValue(100);\n    expect(lib.applyDiscount(100, 100)).toBe(100);",
        ),
        (
            "ns-neg-spy-in-hook",
            "beforeEach(() => { vi.spyOn(lib, 'applyDiscount').mockReturnValue(100); });",
            NAMESPACE_CALL,
        ),
        // A second binding of the same barrel object carries the spy.
        (
            "ns-neg-spy-through-second-namespace",
            "import * as other from '../src/index';\nbeforeEach(() => { vi.spyOn(other, 'applyDiscount').mockReturnValue(100); });",
            NAMESPACE_CALL,
        ),
        (
            "ns-neg-member-assigned",
            "beforeEach(() => { lib.applyDiscount = () => 100; });",
            NAMESPACE_CALL,
        ),
        (
            "ns-neg-cast-assigned",
            "beforeEach(() => { (lib as any).applyDiscount = () => 100; });",
            NAMESPACE_CALL,
        ),
        (
            "ns-neg-index-assigned",
            "beforeEach(() => { lib['applyDiscount'] = () => 100; });",
            NAMESPACE_CALL,
        ),
        (
            "ns-neg-passed-to-stub",
            "beforeEach(() => { stub(lib, 'applyDiscount'); });\nfunction stub(target: any, name: string) { target[name] = () => 100; }",
            NAMESPACE_CALL,
        ),
        (
            "ns-neg-deleted-member",
            "beforeEach(() => { delete lib.applyDiscount; });",
            NAMESPACE_CALL,
        ),
        (
            "ns-neg-body-shadow",
            "",
            "const lib = { applyDiscount: (a: number, t: number) => a };\n    expect(lib.applyDiscount(100, 100)).toBe(100);",
        ),
        (
            "ns-neg-parameter-shadow",
            "",
            "[{ applyDiscount: (a: number, t: number) => a }].forEach((lib) => {\n      expect(lib.applyDiscount(100, 100)).toBe(100);\n    });",
        ),
        ("ns-neg-mock-barrel", "vi.mock('../src');", NAMESPACE_CALL),
        (
            "ns-neg-mock-owner",
            "vi.mock('../src/discount');",
            NAMESPACE_CALL,
        ),
    ];
    for (label, setup, body) in rows {
        assert_no_static_path(
            label,
            &[("src/discount.ts", OWNER), ("src/index.ts", STAR_BARREL)],
            &namespace_test(setup, body),
        )?;
    }
    Ok(())
}

/// The recorded pairs are the file's called members; a member read records
/// nothing and a write or bare use records nothing for that local at all.
#[test]
fn namespace_member_calls_are_recorded_only_for_a_member_only_local() {
    let pairs = |source: &str| -> Vec<(String, String)> {
        extract_tests(Path::new("test/d.test.ts"), source)
            .first()
            .map(|test| test.namespace_member_calls_in_file().cloned().collect())
            .unwrap_or_default()
    };
    let pair = |member: &str| ("lib".to_string(), member.to_string());
    assert_eq!(
        pairs(
            "import * as lib from '../src';\ntest('t', () => {\n  expect(lib.a(1)).toBe(lib.RATE);\n  lib?.b();\n  new lib.Cart();\n});\n"
        ),
        vec![pair("a"), pair("b"), pair("Cart")],
        "calls are recorded, reads are allowed and record nothing"
    );
    for (label, line) in [
        ("bare argument", "spy(lib);"),
        ("spread", "const copy = { ...lib };"),
        ("destructure", "const { a } = lib;"),
        ("index access", "lib['a'](1);"),
        ("member assignment", "lib.a = () => 1;"),
        ("compound assignment", "lib.count += 1;"),
        ("postfix update", "lib.count++;"),
        ("prefix update", "++lib.count;"),
        ("delete", "delete lib.a;"),
        ("redeclaration", "const lib = other;"),
        ("a member of another object", "host.lib.a(1);"),
    ] {
        let source = format!(
            "import * as lib from '../src';\ntest('t', () => {{\n  expect(lib.a(1)).toBe(1);\n  {line}\n}});\n"
        );
        assert_eq!(pairs(&source), Vec::new(), "{label}");
    }
    // A default import keeps its own stricter rule and is never listed here.
    assert_eq!(
        pairs("import lib from '../src';\ntest('t', () => {\n  expect(lib.a(1)).toBe(1);\n});\n"),
        Vec::new()
    );
}
