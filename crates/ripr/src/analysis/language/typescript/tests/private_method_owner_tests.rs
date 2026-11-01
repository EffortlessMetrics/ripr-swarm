//! Private member ownership and truthful reach boundaries (RIPR-SPEC-0243).

use super::*;

#[test]
fn private_method_variants_keep_unknown_reach() -> Result<(), String> {
    let test = "import { Ledger } from '../src/ledger';\ntest('entry', () => { const ledger = new Ledger(); expect(ledger.entry(60)).toBe(55); });\n";
    let masked = "test('text only', () => { const text = 'ledger.#adjust(60)'; /* ledger.#adjust(60) */ expect(text).toBe('ledger.#adjust(60)'); });\n";
    for (key, modifier) in [("#adjust", ""), ("#adjust", "static "), ("#调整", "")] {
        let src = format!(
            "export class Ledger {{\n  {modifier}{key}(amount: number): number {{\n    if (amount >= 50) return amount - 5;\n    return amount;\n  }}\n  adjust(amount: number) {{ return amount; }}\n  entry(amount: number) {{ return {key_call}.{key}(amount); }}\n}}\n",
            key_call = if modifier.is_empty() {
                "this"
            } else {
                "Ledger"
            }
        );
        for (label, test_source) in [("none", ""), ("public", test), ("masked", masked)] {
            let root = ts_unique_tempdir("private-variants")?;
            ts_write_file(&root.join("src/ledger.ts"), &src)?;
            if !test_source.is_empty() {
                ts_write_file(&root.join("tests/ledger.test.ts"), test_source)?;
            }
            let result = TypeScriptAdapter.analyze_diff(
                &ts_analysis_options(root.clone()),
                &OraclePolicy::default(),
                &[changed_with_lines(
                    "src/ledger.ts",
                    &[(3, "    if (amount >= 50) return amount - 5;")],
                )],
            )?;
            assert_eq!(result.findings.len(), 1, "{key} {modifier} {label}");
            let finding = &result.findings[0];
            assert_eq!(finding.class, ExposureClass::StaticUnknown);
            assert_eq!(finding.ripr.reach.state, StageState::Unknown);
            assert_eq!(finding.ripr.reveal.observe.state, StageState::Unknown);
            assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
            assert!(
                finding.related_tests.is_empty(),
                "{key} {label}: {:?}",
                finding.related_tests
            );
            assert!(
                finding
                    .evidence
                    .iter()
                    .any(|line| line == &format!("owner: {key}"))
            );
            if label == "public" {
                let entry_line = src
                    .lines()
                    .nth(6)
                    .ok_or_else(|| "missing public entry".to_string())?;
                let public = TypeScriptAdapter.analyze_diff(
                    &ts_analysis_options(root),
                    &OraclePolicy::default(),
                    &[changed_with_lines("src/ledger.ts", &[(7, entry_line)])],
                )?;
                assert_eq!(public.findings.len(), 1);
                assert!(
                    !public.findings[0].related_tests.is_empty(),
                    "the same public-caller test must be extracted and related"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn private_method_unsupported_keys_keep_file_partial_counts() -> Result<(), String> {
    for header in [
        "get #adjust()",
        "set #adjust(amount: number)",
        "'adjust'(amount: number)",
        // A string key spelled like a private name stays a public-style key:
        // `#` prefixes originate only from the `PrivateIdentifier` arm of
        // `method_key_name`, so this must not become a private-unknown owner.
        "'#adjust'(amount: number)",
        "12(amount: number)",
        "['adjust'](amount: number)",
        "[adjust](amount: number)",
        "#adjust = (amount: number) =>",
    ] {
        let root = ts_unique_tempdir("private-key-gaps")?;
        let src = format!(
            "export class Ledger {{\n  {header} {{\n    const value = 55;\n    return value;\n  }}\n}}\n"
        );
        ts_write_file(&root.join("src/ledger.ts"), &src)?;
        let result = TypeScriptAdapter.analyze_diff(
            &ts_analysis_options(root),
            &OraclePolicy::default(),
            &[changed_with_lines(
                "src/ledger.ts",
                &[(3, "    const value = 55;"), (4, "    return value;")],
            )],
        )?;
        assert!(
            result.findings.is_empty(),
            "{header}: {:?}",
            result.findings
        );
        let gaps: Vec<_> = result
            .limitations
            .iter()
            .filter(|limitation| {
                limitation
                    .bounded_detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("typescript_owner_extraction_partial"))
            })
            .collect();
        assert_eq!(gaps.len(), 1, "{header}: {:?}", result.limitations);
        assert_eq!(
            gaps[0].affected_items,
            Some(1),
            "one affected file, not two changed lines"
        );
    }
    Ok(())
}

#[test]
fn private_method_nested_boundaries_refuse_enclosing_public_credit() -> Result<(), String> {
    for class_open in ["class Inner {", "const Inner = class {"] {
        for method in [
            "#adjust(x: number)",
            "get #adjust()",
            "set #adjust(x: number)",
        ] {
            let root = ts_unique_tempdir("private-nested")?;
            let src = format!(
                "export class Ledger {{\n  entry(n: number) {{\n    {class_open}\n      {method} {{\n        if (n >= 50) return n - 5;\n        return n;\n      }}\n    }};\n    return n;\n  }}\n  visible(n: number) {{ return n * 2; }}\n}}\n"
            );
            ts_write_file(&root.join("src/ledger.ts"), &src)?;
            ts_write_file(
                &root.join("tests/ledger.test.ts"),
                "import { Ledger } from '../src/ledger';\ntest('entry', () => { const ledger = new Ledger(); expect(ledger.entry(60)).toBe(60); expect(ledger.visible(3)).toBe(6); });\n",
            )?;
            let result = TypeScriptAdapter.analyze_diff(
                &ts_analysis_options(root.clone()),
                &OraclePolicy::default(),
                &[changed_with_lines(
                    "src/ledger.ts",
                    &[
                        (5, "        if (n >= 50) return n - 5;"),
                        (6, "        return n;"),
                        (11, "  visible(n: number) { return n * 2; }"),
                    ],
                )],
            )?;
            assert_eq!(
                result.findings.len(),
                1,
                "{class_open} {method}: {:?}",
                result.findings
            );
            let finding = &result.findings[0];
            assert!(finding.evidence.iter().any(|line| line == "owner: visible"));
            assert!(
                !finding.related_tests.is_empty(),
                "positive public control must resolve"
            );
            let gaps: Vec<_> = result
                .limitations
                .iter()
                .filter(|limitation| {
                    limitation.bounded_detail.as_deref().is_some_and(|detail| {
                        detail.contains("typescript_owner_extraction_partial")
                    })
                })
                .collect();
            assert_eq!(gaps.len(), 1);
            assert_eq!(gaps[0].affected_items, Some(1));
            ts_write_file(
                &root.join("src/ledger.ts"),
                &src.replace("        if (n >= 50) return n - 5;\n", ""),
            )?;
            let deleted = ChangedFile {
                path: PathBuf::from("src/ledger.ts"),
                added_lines: Vec::new(),
                removed_lines: vec![crate::analysis::diff::ChangedLine {
                    line: 5,
                    text: "        if (n >= 50) return n - 5;".to_string(),
                    new_side_line: 5,
                }],
            };
            let result = TypeScriptAdapter.analyze_diff(
                &ts_analysis_options(root),
                &OraclePolicy::default(),
                &[deleted],
            )?;
            assert!(
                result.findings.is_empty(),
                "deleted nested body must not borrow entry: {:?}",
                result.findings
            );
            assert!(result.limitations.iter().any(|limitation| {
                limitation
                    .bounded_detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("typescript_owner_extraction_partial"))
            }));
        }
    }
    Ok(())
}

#[test]
fn private_method_replacement_and_removed_body_keep_unknown_reach() -> Result<(), String> {
    let root = ts_unique_tempdir("private-edit-kinds")?;
    let src = "export class Ledger {\n  #adjust(n: number) {\n    if (n >= 51) return n - 5;\n    return n;\n  }\n}\n";
    ts_write_file(&root.join("src/ledger.ts"), src)?;
    let mut changed = changed_with_lines("src/ledger.ts", &[(3, "    if (n >= 51) return n - 5;")]);
    changed
        .removed_lines
        .push(crate::analysis::diff::ChangedLine {
            line: 3,
            new_side_line: 3,
            text: "    if (n >= 50) return n - 5;".to_string(),
        });
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed.clone()],
    )?;
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].class, ExposureClass::StaticUnknown);
    assert!(result.findings[0].related_tests.is_empty());
    ts_write_file(
        &root.join("src/ledger.ts"),
        &src.replace("    if (n >= 51) return n - 5;\n", ""),
    )?;
    changed.added_lines.clear();
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root),
        &OraclePolicy::default(),
        &[changed],
    )?;
    assert_eq!(result.findings.len(), 1);
    let finding = &result.findings[0];
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    assert_eq!(
        finding.source_currentness,
        crate::domain::SourceCurrentness::BaseDeleted
    );
    assert!(finding.related_tests.is_empty());
    Ok(())
}

#[test]
fn private_method_nested_runtime_path_and_auto_accessor_stay_partial() -> Result<(), String> {
    for (member, run) in [
        (
            "#adjust(x: number) { if (x >= 50) return x - 5; return x; }",
            "return this.#adjust(n);",
        ),
        ("accessor #value = 55;", "return this.#value;"),
    ] {
        let root = ts_unique_tempdir("private-nested-runtime")?;
        let src = format!(
            "export class Ledger {{\n  entry(n: number) {{\n    class Inner {{\n      {member}\n      run(n: number) {{ {run} }}\n    }}\n    return new Inner().run(n);\n  }}\n}}\n"
        );
        ts_write_file(&root.join("src/ledger.ts"), &src)?;
        ts_write_file(
            &root.join("tests/ledger.test.ts"),
            "import { Ledger } from '../src/ledger';\ntest('entry', () => { const ledger = new Ledger(); expect(ledger.entry(60)).toBe(55); });\n",
        )?;
        let result = TypeScriptAdapter.analyze_diff(
            &ts_analysis_options(root),
            &OraclePolicy::default(),
            &[changed_with_lines("src/ledger.ts", &[(4, member)])],
        )?;
        assert!(
            result.findings.is_empty(),
            "nested reach must not be credited: {:?}",
            result.findings
        );
        assert!(result.limitations.iter().any(|limitation| {
            limitation
                .bounded_detail
                .as_deref()
                .is_some_and(|detail| detail.contains("typescript_owner_extraction_partial"))
        }));
    }
    Ok(())
}

#[test]
fn deleting_a_whole_private_method_is_a_silent_gap_until_base_ranges_exist() -> Result<(), String> {
    // Non-claim (RIPR-SPEC-0243 Rule 21a): boundaries come from candidate-side
    // source, so a member removed entirely leaves no owner and no disclosure.
    let root = ts_unique_tempdir("private-whole-delete")?;
    ts_write_file(
        &root.join("src/ledger.ts"),
        "export class Ledger {\n  entry(amount: number) { return amount; }\n}\n",
    )?;
    let deleted = ChangedFile {
        path: PathBuf::from("src/ledger.ts"),
        added_lines: Vec::new(),
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: "  #adjust(amount: number) { return amount - 5; }".to_string(),
            new_side_line: 2,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root),
        &OraclePolicy::default(),
        &[deleted],
    )?;
    assert!(result.findings.is_empty(), "{:?}", result.findings);
    assert!(
        !result.limitations.iter().any(|limitation| limitation
            .bounded_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("typescript_owner_extraction_partial"))),
        "{:?}",
        result.limitations
    );
    Ok(())
}
