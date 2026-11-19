//! #15306: a changed literal return of one arm of a literal-return lookup.

use super::*;

fn owner(source: &str) -> TypeScriptOwner {
    TypeScriptOwner {
        name: "dir".to_string(),
        file: PathBuf::from("src/dir.ts"),
        start_line: 10,
        end_line: 10 + source.lines().count(),
        source_text: Some(source.to_string()),
        ..test_owner("dir", "src/dir.ts")
    }
}

const SWITCH: &str = "export function dir(n: number): string {\n  switch (n) {\n    case 1:\n      return 'uno';\n    case 2:\n      return 'two';\n    default:\n      return 'other';\n  }\n}\n";

#[test]
fn a_switch_arm_literal_return_is_located_with_its_old_value() {
    let change = literal_arm_return(
        &owner(SWITCH),
        13,
        "      return 'uno';",
        Some("      return 'one';"),
    );
    assert_eq!(
        change,
        Some(LiteralArmReturn {
            old_value: ArmConstant::Text("one".to_string()),
            old_text: "'one'".to_string(),
        })
    );
}

#[test]
fn an_old_literal_another_arm_also_returns_is_refused() {
    let source = SWITCH.replace("'other'", "'one'");
    assert_eq!(
        literal_arm_return(
            &owner(&source),
            13,
            "      return 'uno';",
            Some("      return 'one';")
        ),
        None
    );
}

#[test]
fn a_non_literal_return_anywhere_in_the_owner_is_refused() {
    let source = SWITCH.replace("return 'other';", "return String(n);");
    assert_eq!(
        literal_arm_return(
            &owner(&source),
            13,
            "      return 'uno';",
            Some("      return 'one';")
        ),
        None
    );
}

#[test]
fn a_single_return_owner_and_a_line_mismatch_are_refused() {
    let single = "export function dir(n: number): string {\n  return 'uno';\n}\n";
    assert_eq!(
        literal_arm_return(
            &owner(single),
            11,
            "  return 'uno';",
            Some("  return 'one';")
        ),
        None
    );
    // The changed line must be the return the parse finds there.
    assert_eq!(
        literal_arm_return(
            &owner(SWITCH),
            12,
            "      return 'uno';",
            Some("      return 'one';")
        ),
        None
    );
}

#[test]
fn nested_function_returns_are_not_the_owners_and_semicolons_are_optional() {
    let source = "export const dir = (n: number) => {\n  const f = () => compute(n)\n  if (n > 0) return 'pos'\n  if (n < 0) return 'neg'\n  return 'zero'\n}\n";
    let change = literal_arm_return(
        &owner(source),
        13,
        "  if (n < 0) return 'neg'",
        Some("  if (n < 0) return 'negative'"),
    );
    // `if (...) return` on one line is not a plain return line.
    assert_eq!(change, None);
    let change = literal_arm_return(&owner(source), 14, "  return 'z'", Some("  return 'zero'"));
    assert_eq!(
        change.map(|change| change.old_value),
        Some(ArmConstant::Text("zero".to_string()))
    );
}

#[test]
fn a_generator_or_concise_arrow_owner_is_refused() {
    let generator =
        "export function* dir(n: number) {\n  if (n) {\n    return 1;\n  }\n  return 2;\n}\n";
    assert_eq!(
        literal_arm_return(
            &owner(generator),
            12,
            "    return 3;",
            Some("    return 1;")
        ),
        None
    );
    let concise = "export const dir = (n: number) => n ? 1 : 2;\n";
    assert_eq!(
        literal_arm_return(&owner(concise), 10, "return 3;", Some("return 1;")),
        None
    );
}

#[test]
fn a_class_method_owner_parses_inside_a_class_wrapper() {
    let method = "dir(n: number): number {\n    if (n === 1) {\n      return 10;\n    }\n    return 20;\n  }\n";
    let change = literal_arm_return(
        &owner(method),
        12,
        "      return 11;",
        Some("      return 10;"),
    );
    assert_eq!(
        change.map(|change| change.old_value),
        Some(ArmConstant::Number(10.0))
    );
}

const DIR: &str = "export function dir(n: number): string {\n  switch (n) {\n    case 1:\n      return 'one';\n    case 2:\n      return 'two';\n    default:\n      return 'other';\n  }\n}\n";

/// Runs the TypeScript adapter on one changed line of `src/dir.ts` with one
/// test file, returning the finding on that line.
fn finding_at(
    label: &str,
    source: &str,
    line: usize,
    removed: &str,
    added: &str,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let changed_source = source.replacen(removed.trim(), added.trim(), 1);
    ts_write_file(&root.join("src/dir.ts"), &changed_source)?;
    ts_write_file(&root.join("tests/dir.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/dir.ts"),
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

fn vitest(body: &str) -> String {
    format!(
        "import {{ describe, it, expect }} from 'vitest';\nimport {{ dir }} from '../src/dir';\n\ndescribe('dir', () => {{\n  it('maps', () => {{\n{body}  }});\n}});\n"
    )
}

/// geolib `getCompassDirection`: the only exact assertion expects another
/// arm's result, so changing the `case 1` arm passes the suite.
#[test]
fn an_arm_no_assertion_expects_is_not_exposed_and_names_the_arm() -> Result<(), String> {
    for (label, line, removed, added) in [
        ("arm-case", 4, "      return 'one';", "      return 'uno';"),
        (
            "arm-default",
            8,
            "      return 'other';",
            "      return 'o';",
        ),
    ] {
        let finding = finding_at(
            label,
            DIR,
            line,
            removed,
            added,
            &vitest("    expect(dir(2)).toBe('two');\n"),
        )?;
        assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{label}");
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains("expects another arm's result (`'two'`)")),
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// The discriminating positive: the assertion that expects the arm's old
/// result keeps `exposed`, written with either quote.
#[test]
fn an_assertion_expecting_the_arms_old_result_keeps_exposed() -> Result<(), String> {
    for (label, body) in [
        ("arm-pinned", "    expect(dir(1)).toBe('one');\n"),
        (
            "arm-pinned-double",
            "    expect(dir(2)).toBe('two');\n    expect(dir(1)).toEqual(\"one\");\n",
        ),
        // A derived local still names the old value: kept, never ruled out.
        (
            "arm-pinned-local",
            "    const label = dir(1);\n    expect(label).toBe('one');\n",
        ),
    ] {
        let finding = finding_at(
            label,
            DIR,
            4,
            "      return 'one';",
            "      return 'uno';",
            &vitest(body),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// Unreadable expected sides keep the existing verdict: an `it.each` row
/// value, a snapshot, and a structural matcher on the owner's result.
#[test]
fn an_unreadable_expected_side_keeps_the_verdict() -> Result<(), String> {
    let each = "import { it, expect } from 'vitest';\nimport { dir } from '../src/dir';\n\nit.each([[2, 'two'], [3, 'other']])('dir(%i)', (n, want) => {\n  expect(dir(n)).toBe(want);\n});\n";
    let baseline = finding_at(
        "arm-each-baseline",
        DIR,
        4,
        "      return 'one';",
        "      return 'uno';",
        each,
    )?;
    let unpinned = baseline
        .missing
        .iter()
        .any(|line| line.contains("expects another arm's result"));
    assert!(!unpinned, "it.each: {:?}", baseline.missing);
    let snapshot = finding_at(
        "arm-snapshot",
        DIR,
        4,
        "      return 'one';",
        "      return 'uno';",
        &vitest("    expect(dir(2)).toMatchInlineSnapshot(`\"two\"`);\n"),
    )?;
    assert!(
        !snapshot
            .missing
            .iter()
            .any(|line| line.contains("expects another arm's result")),
        "snapshot: {:?}",
        snapshot.missing
    );
    Ok(())
}

/// A duplicated old literal or a computed return elsewhere means another
/// statement can yield the expected value: the rule does not apply.
#[test]
fn a_lookup_that_is_not_all_distinct_literals_keeps_exposed() -> Result<(), String> {
    for (label, source) in [
        (
            "arm-duplicate",
            DIR.replace("return 'other';", "return 'one';"),
        ),
        (
            "arm-computed",
            DIR.replace("return 'other';", "return String(n);"),
        ),
    ] {
        let finding = finding_at(
            label,
            &source,
            4,
            "      return 'one';",
            "      return 'uno';",
            &vitest("    expect(dir(2)).toBe('two');\n"),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}
