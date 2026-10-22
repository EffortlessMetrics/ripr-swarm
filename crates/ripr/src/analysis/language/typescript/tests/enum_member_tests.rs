//! Rule 15 compares canonical enum bindings, never the local path spelling.

use super::*;

const ENUM: &str = "export enum Level { Low = 1, High = 2 }\nexport const Other = { High: 1 };\nexport function weight(level: number): number {\n  return level === Level.High ? 11 : 10;\n}\n";
const OLD: &str = "  return level === Level.High ? 10 : 10;";

fn classify_enum(
    label: &str,
    source: &str,
    imports: &str,
    setup: &str,
    body: &str,
    extra: &[(&str, &str)],
) -> Result<Finding, String> {
    classify_enum_source(
        label,
        source,
        &format!(
            "import {{ expect, it, describe }} from \"vitest\";\n{imports}\n{setup}\nit(\"high\", () => {{\n{body}\n}});\n"
        ),
        extra,
    )
}

fn classify_enum_source(
    label: &str,
    source: &str,
    test_source: &str,
    extra: &[(&str, &str)],
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/level.ts"), source)?;
    for (path, source) in extra {
        ts_write_file(&root.join(path), source)?;
    }
    ts_write_file(&root.join("test/level.test.ts"), test_source)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/level.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: source
                .lines()
                .nth(3)
                .ok_or("missing changed line")?
                .to_string(),
            new_side_line: 4,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: OLD.to_string(),
            new_side_line: 4,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    if result.findings.len() != 1 {
        return Err(format!(
            "{label}: expected one finding, got {}",
            result.findings.len()
        ));
    }
    let finding = result
        .findings
        .into_iter()
        .next()
        .ok_or("missing finding")?;
    assert_eq!(finding.probe.location.line, 4, "{label}");
    assert_eq!(
        finding.related_tests.len(),
        1,
        "{label}: retain real owner relation"
    );
    assert_eq!(
        finding.related_tests[0].oracle_strength,
        OracleStrength::Strong,
        "{label}"
    );
    Ok(finding)
}

#[test]
fn canonical_enum_imports_aliases_and_reexports_select_the_edited_arm() -> Result<(), String> {
    let extra = [
        ("src/barrel.ts", "export { Level } from './level';\n"),
        ("src/alias.ts", "export { Level as Rank } from './level';\n"),
        ("src/star.ts", "export * from './level';\n"),
        (
            "src/imported.ts",
            "import { Level as Rank } from './level';\nexport { Rank as Level };\n",
        ),
    ];
    for (label, imports, argument) in [
        (
            "exact",
            "import { weight, Level } from '../src/level';",
            "Level.High",
        ),
        (
            "alias",
            "import { weight, Level as Rank } from '../src/level';",
            "Rank.High",
        ),
        (
            "barrel",
            "import { weight } from '../src/level'; import { Level } from '../src/barrel';",
            "Level.High",
        ),
        (
            "renamed-barrel",
            "import { weight } from '../src/level'; import { Rank as Level } from '../src/alias';",
            "Level.High",
        ),
        (
            "star",
            "import { weight } from '../src/level'; import { Level } from '../src/star';",
            "Level.High",
        ),
        (
            "import-export",
            "import { weight } from '../src/level'; import { Level } from '../src/imported';",
            "Level.High",
        ),
        (
            "namespace",
            "import { weight } from '../src/level'; import * as levels from '../src/level';",
            "levels.Level.High",
        ),
    ] {
        let finding = classify_enum(
            label,
            ENUM,
            imports,
            "",
            &format!("expect(weight({argument})).toBe(10);"),
            &extra,
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
        assert_eq!(finding.probe.family, ProbeFamily::ReturnValue, "{label}");
    }
    Ok(())
}

#[test]
fn an_owner_imported_enum_keeps_its_canonical_identity() -> Result<(), String> {
    for (owner_binding, condition) in [("Level", "Level.High"), ("Level as Rank", "Rank.High")] {
        let source = format!(
            "import {{ {owner_binding} }} from './constants';\n\nexport function weight(level: number): number {{\n  return level === {condition} ? 11 : 10;\n}}\n"
        );
        let root = ts_unique_tempdir("enum-owner-import")?;
        ts_write_file(&root.join("src/level.ts"), &source)?;
        ts_write_file(
            &root.join("src/constants.ts"),
            "export enum Level { Low = 1, High = 2 }\n",
        )?;
        ts_write_file(
            &root.join("test/level.test.ts"),
            "import { expect, it } from 'vitest';\nimport { weight } from '../src/level';\nimport { Level } from '../src/constants';\nit('high', () => { expect(weight(Level.High)).toBe(10); });\n",
        )?;
        let changed = ChangedFile {
            path: PathBuf::from("src/level.ts"),
            added_lines: vec![crate::analysis::diff::ChangedLine {
                line: 4,
                text: format!("  return level === {condition} ? 11 : 10;"),
                new_side_line: 4,
            }],
            removed_lines: vec![crate::analysis::diff::ChangedLine {
                line: 4,
                text: format!("  return level === {condition} ? 10 : 10;"),
                new_side_line: 4,
            }],
        };
        let result = TypeScriptAdapter.analyze_diff(
            &ts_analysis_options(root.clone()),
            &OraclePolicy::default(),
            &[changed],
        );
        let _ = std::fs::remove_dir_all(&root);
        let findings = result?.findings;
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].class, ExposureClass::Exposed, "{owner_binding}");
    }
    Ok(())
}

#[test]
fn enum_spelling_without_the_same_declaration_never_credits() -> Result<(), String> {
    let extra = [
        ("src/other.ts", "export enum Level { High = 1 }\n"),
        ("src/dangling.ts", "export { Level } from './missing';\n"),
        (
            "src/ambiguous.ts",
            "export * from './level'; export * from './other';\n",
        ),
    ];
    for (label, imports, setup, body) in [
        (
            "foreign",
            "import { weight } from '../src/level'; import { Level } from '../src/other';",
            "",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "wrong-export",
            "import { weight, Other as Level } from '../src/level';",
            "",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "missing",
            "import { weight } from '../src/level';",
            "",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "file-local",
            "import { weight } from '../src/level';",
            "const Level = { High: 1 };",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "body-local",
            "import { weight } from '../src/level';",
            "",
            "const Level = { High: 1 }; expect(weight(Level.High)).toBe(10);",
        ),
        (
            "dangling",
            "import { weight } from '../src/level'; import { Level } from '../src/dangling';",
            "",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "ambiguous",
            "import { weight } from '../src/level'; import { Level } from '../src/ambiguous';",
            "",
            "expect(weight(Level.High)).toBe(10);",
        ),
    ] {
        let finding = classify_enum(label, ENUM, imports, setup, body, &extra)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
        assert_eq!(finding.probe.family, ProbeFamily::Predicate, "{label}");
    }
    Ok(())
}

#[test]
fn enum_reexports_require_a_real_export_at_every_module() -> Result<(), String> {
    let private = ENUM.replacen("export enum Level", "enum Level", 1);
    let finding = classify_enum(
        "private-enum-reexport",
        &private,
        "import { weight } from '../src/level'; import { Level } from '../src/barrel';",
        "",
        "expect(weight(Level.High)).toBe(10);",
        &[("src/barrel.ts", "export { Level } from './level';\n")],
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn enum_imported_local_reexports_share_the_existing_hop_budget() -> Result<(), String> {
    for hops in [4, 5] {
        let modules: Vec<_> = (1..=hops)
            .map(|index| {
                let target = if index == hops {
                    "level".to_string()
                } else {
                    format!("barrel{}", index + 1)
                };
                let source = if index % 2 == 0 {
                    format!("import {{ Level as Rank }} from './{target}'; export {{ Rank as Level }};\n")
                } else {
                    format!("export {{ Level }} from './{target}';\n")
                };
                (format!("src/barrel{index}.ts"), source)
            })
            .collect();
        let extra: Vec<_> = modules
            .iter()
            .map(|(path, source)| (path.as_str(), source.as_str()))
            .collect();
        let finding = classify_enum(
            "mixed-enum-hop-budget",
            ENUM,
            "import { weight } from '../src/level'; import { Level } from '../src/barrel1';",
            "",
            "expect(weight(Level.High)).toBe(10);",
            &extra,
        )?;
        let expected = if hops == 4 {
            ExposureClass::Exposed
        } else {
            ExposureClass::WeaklyExposed
        };
        assert_eq!(finding.class, expected, "{hops} module hops");
    }
    Ok(())
}

#[test]
fn mutated_or_escaped_enum_roots_never_credit() -> Result<(), String> {
    for (label, setup, body) in [
        (
            "setup-write",
            "Level.High = 1;",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "body-write",
            "",
            "Level.High = 1; expect(weight(Level.High)).toBe(10);",
        ),
        (
            "computed-write",
            "Level['High'] = 1;",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "alias-escape",
            "const other = Level; other.High = 1;",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "call-escape",
            "Object.assign(Level, { High: 1 });",
            "expect(weight(Level.High)).toBe(10);",
        ),
        (
            "delete",
            "delete Level.High;",
            "expect(weight(Level.High)).toBe(10);",
        ),
    ] {
        let finding = classify_enum(
            label,
            ENUM,
            "import { weight, Level } from '../src/level';",
            setup,
            body,
            &[],
        )?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    }
    Ok(())
}

#[test]
fn an_enclosing_scope_enum_shadow_never_credits() -> Result<(), String> {
    let finding = classify_enum_source(
        "enclosing-shadow",
        ENUM,
        "import { expect, it, describe } from 'vitest';\nimport { weight, Level } from '../src/level';\ndescribe('scope', () => { const Level = { High: 1 }; it('high', () => { expect(weight(Level.High)).toBe(10); }); });\n",
        &[],
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn owner_enum_shadows_and_opaque_declarations_never_credit() -> Result<(), String> {
    for (label, source) in [
        (
            "body-shadow",
            ENUM.replace("export const Other = { High: 1 };\n", "")
                .replace("  return", "  const Level = { High: 1 };\n  return"),
        ),
        (
            "parameter-shadow",
            ENUM.replace(
                "weight(level: number)",
                "weight(level: number, Level = { High: 1 })",
            ),
        ),
        (
            "ambient",
            ENUM.replace("export enum", "export declare enum"),
        ),
        (
            "non-enum",
            ENUM.replace(
                "export enum Level { Low = 1, High = 2 }",
                "export const Level = { Low: 1, High: 2 };",
            ),
        ),
    ] {
        let finding = classify_enum(
            label,
            &source,
            "import { weight, Level } from '../src/level';",
            "",
            "expect(weight(Level.High)).toBe(10);",
            &[],
        )?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    }
    Ok(())
}

#[test]
fn scalar_edited_arm_witnesses_are_unchanged() -> Result<(), String> {
    let source =
        "\n\nexport function weight(level: number): number {\n  return level === 2 ? 11 : 10;\n}\n";
    let root = ts_unique_tempdir("scalar-edited-arm")?;
    ts_write_file(&root.join("src/level.ts"), source)?;
    ts_write_file(
        &root.join("test/level.test.ts"),
        "import { expect, it } from 'vitest'; import { weight } from '../src/level'; it('high', () => { expect(weight(2)).toBe(10); });",
    )?;
    let changed = ChangedFile {
        path: PathBuf::from("src/level.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: "  return level === 2 ? 11 : 10;".to_string(),
            new_side_line: 4,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: "  return level === 2 ? 10 : 10;".to_string(),
            new_side_line: 4,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    let findings = result?.findings;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].class, ExposureClass::Exposed);
    Ok(())
}
