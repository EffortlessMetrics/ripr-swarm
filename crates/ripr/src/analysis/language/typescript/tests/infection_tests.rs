//! #10704: infection the TypeScript preview adapter states for a change.
//! An unmodeled shape reads `infection_unknown` and never a delegatable
//! packet; a changed line under an arm no test input enters reads
//! `reachable_unrevealed` with infection `no`; an entered arm keeps
//! `weakly_exposed`.

use super::*;

const PRICE: &str = "export function price(qty: number, coupon?: string): number {\n  let total = qty * 10;\n  if (coupon === 'HALF') {\n    total = total * 3;\n  }\n  return total;\n}\n";

fn finding_for(
    label: &str,
    source: &str,
    line: usize,
    removed: &str,
    added: &str,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/price.ts"), source)?;
    ts_write_file(&root.join("tests/price.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/price.ts"),
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

fn weak_test(call: &str) -> String {
    format!(
        "import {{ price }} from \"../src/price\";\ntest(\"price\", () => {{\n  expect({call}).toBeGreaterThan(0);\n}});\n"
    )
}

fn arm_change(label: &str, call: &str) -> Result<Finding, String> {
    let source = PRICE.replace("total * 3", "total * qty");
    finding_for(
        label,
        &source,
        4,
        "    total = total * 3;",
        "    total = total * qty;",
        &weak_test(call),
    )
}

fn has_evidence(finding: &Finding, line: &str) -> bool {
    finding.evidence.iter().any(|entry| entry == line)
}

#[test]
fn an_arm_no_test_input_enters_reads_infection_no_and_is_not_weakly_exposed() -> Result<(), String>
{
    let finding = arm_change("infect-arm-untaken", "price(2)")?;
    assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
    assert_eq!(finding.ripr.infect.state, StageState::No);
    // `reachable_unrevealed` does not require a stop reason: the finding is a
    // concrete gap (the arm is never entered), not an unknown.
    assert!(finding.stop_reasons.is_empty());
    assert!(
        finding.ripr.infect.summary.contains("coupon === 'HALF'"),
        "{}",
        finding.ripr.infect.summary
    );
    assert!(has_evidence(
        &finding,
        "typescript_infection: no (branch_not_entered)"
    ));
    // No "assertion too weak" row and no delegatable packet.
    assert!(finding.related_tests.iter().all(|row| row.miss.is_none()));
    assert!(finding.activation.missing_discriminators.is_empty());
    assert!(
        finding
            .recommended_next_step
            .as_deref()
            .is_some_and(|step| step.contains("no related test call enters"))
    );
    Ok(())
}

#[test]
fn an_arm_some_test_input_enters_stays_weakly_exposed() -> Result<(), String> {
    let finding = arm_change("infect-arm-entered", "price(3, 'HALF')")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        !finding
            .evidence
            .iter()
            .any(|e| e.starts_with("typescript_infection: "))
    );
    Ok(())
}

fn shape_change(label: &str, old: &str, new: &str) -> Result<Finding, String> {
    let source =
        format!("export function price(flag: boolean, n?: number): number {{\n{new}\n}}\n");
    finding_for(label, &source, 2, old, new, &weak_test("price(true, 1)"))
}

#[test]
fn unmodeled_shapes_read_infection_unknown_and_never_a_packet() -> Result<(), String> {
    for (label, old, new, shape) in [
        (
            "literal-return",
            "  return 5;",
            "  return 6;",
            "literal_return",
        ),
        (
            "fallback",
            "  return n ?? 10;",
            "  return n ?? 20;",
            "fallback_literal",
        ),
        ("noop", "  return n;", "  return n + 0;", "arithmetic_noop"),
    ] {
        let finding = shape_change(label, old, new)?;
        assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{label}");
        assert_eq!(finding.ripr.infect.state, StageState::Unknown, "{label}");
        assert!(
            has_evidence(
                &finding,
                &format!("typescript_infection: unknown ({shape})")
            ),
            "{label}: {:?}",
            finding.evidence
        );
        assert!(
            finding
                .stop_reasons
                .contains(&StopReason::InfectionEvidenceUnknown),
            "{label}"
        );
        assert!(
            finding.activation.missing_discriminators.is_empty(),
            "{label}"
        );
        assert!(
            finding.related_tests.iter().all(|row| row.miss.is_none()),
            "{label}"
        );
        assert!(
            !has_evidence(&finding, "actionability_category: complete_repair_packet"),
            "{label}"
        );
    }
    Ok(())
}

fn literal_arm(label: &str, call: &str) -> Result<Finding, String> {
    let source = "export function price(qty: number, coupon?: string): number {\n  if (coupon === 'HALF') {\n    return 7;\n  }\n  return qty;\n}\n";
    finding_for(
        label,
        source,
        3,
        "    return 6;",
        "    return 7;",
        &weak_test(call),
    )
}

fn switch_return(label: &str, test: &str) -> Result<Finding, String> {
    let source = "export function price(kind: string): number {\n  switch (kind) {\n    case 'gold':\n      return 10;\n    case 'silver':\n      return 7;\n    default:\n      return 0;\n  }\n}\n";
    finding_for(label, source, 6, "      return 5;", "      return 7;", test)
}

#[test]
fn selected_switch_literal_return_has_weak_oracle_gap() -> Result<(), String> {
    let test = "import { test, expect } from 'vitest';\nimport { price } from '../src/price';\ntest('price', () => { const result = price('silver'); expect(result).toBeLessThan(100); });\n";
    let finding = switch_return("switch-selected", test)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn switch_literal_return_controls() -> Result<(), String> {
    let exact = switch_return(
        "switch-exact",
        "import { price } from '../src/price';\ntest('price', () => { expect(price('silver')).toBe(7); });\n",
    )?;
    assert_eq!(exact.class, ExposureClass::Exposed);
    let other = switch_return("switch-other", &weak_test("price('gold')"))?;
    assert_ne!(other.class, ExposureClass::Exposed);
    let unknown = switch_return("switch-unknown", &weak_test("price(pick())"))?;
    assert_eq!(unknown.class, ExposureClass::InfectionUnknown);
    let unrelated = "import { test, expect } from 'vitest';\nimport { price } from '../src/price';\ntest('price', () => { const result = price('silver'); expect(0).toBeLessThan(100); });\n";
    let finding = switch_return("switch-unrelated", unrelated)?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    let discriminating = "import { test, expect } from 'vitest';\nimport { price } from '../src/price';\ntest('price', () => { const result = price('silver'); expect(result).toBeLessThan(6); });\n";
    let finding = switch_return("switch-discriminating", discriminating)?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    let failing = "import { test, expect } from 'vitest';\nimport { price } from '../src/price';\ntest('price', () => { const result = price('silver'); expect(result).toBeLessThan(1); });\n";
    let finding = switch_return("switch-failing", failing)?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    let shadowed = "import { test, expect } from 'vitest';\nimport { price } from '../src/price';\ntest('price', () => { const expect = price('silver'); expect(expect).toBeLessThan(100); });\n";
    let finding = switch_return("switch-shadowed-expect", shadowed)?;
    assert_ne!(finding.class, ExposureClass::WeaklyExposed);
    let source = "export function price(kind: string): number {\n  switch (kind) {\n    case 'silver':\n      return 9007199254740993;\n  }\n}\n";
    let test = "import { test, expect } from 'vitest';\nimport { price } from '../src/price';\ntest('price', () => { const result = price('silver'); expect(result).toBeLessThan(100); });\n";
    let unsafe_number = finding_for(
        "switch-unsafe-number",
        source,
        4,
        "      return 9007199254740992;",
        "      return 9007199254740993;",
        test,
    )?;
    assert_eq!(unsafe_number.class, ExposureClass::InfectionUnknown);
    Ok(())
}

#[test]
fn switch_return_direct_selector_controls() -> Result<(), String> {
    let test_for = |argument: &str| {
        format!(
            "import {{ test, expect }} from 'vitest';\nimport {{ price }} from '../src/price';\ntest('price', () => {{ const result = price({argument}); expect(result).toBeLessThan(100); }});\n"
        )
    };
    let other = switch_return("switch-direct-other", &test_for("'gold'"))?;
    assert_ne!(other.class, ExposureClass::WeaklyExposed);
    assert_ne!(other.class, ExposureClass::Exposed);
    let computed = switch_return("switch-direct-computed", &test_for("pick()"))?;
    assert_eq!(computed.class, ExposureClass::InfectionUnknown);
    let escaped_match = switch_return("switch-escaped-match", &test_for("'\\x73ilver'"))?;
    assert_eq!(escaped_match.class, ExposureClass::WeaklyExposed);
    let escaped_other = switch_return("switch-escaped-other", &test_for("'\\x67old'"))?;
    assert_eq!(escaped_other.class, ExposureClass::InfectionUnknown);
    Ok(())
}

#[test]
fn switch_return_with_another_changed_line_stays_unknown() -> Result<(), String> {
    let root = ts_unique_tempdir("switch-changed-flow")?;
    let source = "export function price(kind: string): number {\n  switch (kind) {\n    case 'gold':\n      return 10;\n    case 'silver':\n      return 7;\n    default:\n      return 0;\n  }\n}\n";
    let test = "import { test, expect } from 'vitest';\nimport { price } from '../src/price';\ntest('price', () => { const result = price('silver'); expect(result).toBeLessThan(100); });\n";
    ts_write_file(&root.join("src/price.ts"), source)?;
    ts_write_file(&root.join("tests/price.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/price.ts"),
        added_lines: vec![
            crate::analysis::diff::ChangedLine {
                line: 5,
                new_side_line: 5,
                text: "    case 'silver':".into(),
            },
            crate::analysis::diff::ChangedLine {
                line: 6,
                new_side_line: 6,
                text: "      return 7;".into(),
            },
        ],
        removed_lines: vec![
            crate::analysis::diff::ChangedLine {
                line: 5,
                new_side_line: 5,
                text: "    case 'bronze':".into(),
            },
            crate::analysis::diff::ChangedLine {
                line: 6,
                new_side_line: 5,
                text: "      return 5;".into(),
            },
        ],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    let finding = result?
        .findings
        .into_iter()
        .find(|f| f.probe.location.line == 6)
        .ok_or("missing return finding")?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    Ok(())
}

#[test]
fn switch_return_with_a_competing_test_stays_unknown() -> Result<(), String> {
    let root = ts_unique_tempdir("switch-competing")?;
    let source = "export function price(kind: string): number {\n  switch (kind) {\n    case 'gold':\n      return 10;\n    case 'silver':\n      return 7;\n    default:\n      return 0;\n  }\n}\n";
    let test = "import { test, expect } from 'vitest';\nimport { price } from '../src/price';\ntest('price', () => { const result = price('silver'); expect(result).toBeLessThan(100); });\n";
    ts_write_file(&root.join("src/price.ts"), source)?;
    ts_write_file(&root.join("tests/price.test.ts"), test)?;
    ts_write_file(
        &root.join("tests/other.test.ts"),
        &weak_test("price('gold')"),
    )?;
    let changed = ChangedFile {
        path: PathBuf::from("src/price.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 6,
            new_side_line: 6,
            text: "      return 7;".into(),
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 6,
            new_side_line: 6,
            text: "      return 5;".into(),
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    let finding = result?
        .findings
        .into_iter()
        .find(|f| f.probe.location.line == 6)
        .ok_or("missing return finding")?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    Ok(())
}

#[test]
fn an_unmodeled_shape_in_an_untaken_arm_reads_the_stronger_no_fact() -> Result<(), String> {
    let untaken = literal_arm("infect-literal-untaken", "price(2)")?;
    assert_eq!(untaken.class, ExposureClass::ReachableUnrevealed);
    assert!(has_evidence(
        &untaken,
        "typescript_infection: no (branch_not_entered)"
    ));
    // Discriminating control: an input that enters the arm leaves the shape
    // unknown.
    let entered = literal_arm("infect-literal-entered", "price(3, 'HALF')")?;
    assert_eq!(entered.class, ExposureClass::InfectionUnknown);
    assert!(has_evidence(
        &entered,
        "typescript_infection: unknown (literal_return)"
    ));
    Ok(())
}

#[test]
fn a_dropped_semicolon_before_a_comment_is_never_a_neutral_comment_edit() -> Result<(), String> {
    // Production path: `return f // note` followed by `(1);` returns `f(1)`.
    let source = "export function price(n: number): number {\n  const f = (x: number) => x + n;\n  return f // note\n  (1);\n}\n";
    let finding = finding_for(
        "infect-asi",
        source,
        3,
        "  return f;",
        "  return f // note",
        &weak_test("price(2)"),
    )?;
    assert!(
        !has_evidence(&finding, "typescript_infection: unknown (comment_only)"),
        "{:?}",
        finding.evidence
    );
    // Control: the same edit with the semicolon kept is a comment-only edit,
    // which emits no probe at all (#10706).
    let kept = finding_for(
        "infect-comment-kept",
        "export function price(n: number): number {\n  const f = (x: number) => x + n;\n  return f; // note\n}\n",
        3,
        "  return f;",
        "  return f; // note",
        &weak_test("price(2)"),
    );
    assert!(kept.is_err(), "{kept:?}");
    Ok(())
}

#[test]
fn a_trailing_comment_or_whitespace_edit_emits_no_probe_but_a_changed_token_keeps_it() {
    let test = weak_test("price(2)");
    let source_with =
        |line: &str| format!("export function price(n: number): number {{\n{line}\n}}\n");
    for (label, new) in [
        ("noop-comment", "  return n; // fallback"),
        ("noop-comment-edit", "  return n; // other"),
        ("noop-space", "  return n;   "),
    ] {
        let found = finding_for(label, &source_with(new), 2, "  return n;", new, &test);
        assert!(found.is_err(), "{label}: {found:?}");
    }
    // Controls through the same production path: a changed token (with or
    // without a comment) still reads as an edit.
    for (label, new) in [
        ("edit-token", "  return n + 1;"),
        ("edit-token-comment", "  return n + 1; // fallback"),
    ] {
        let found = finding_for(label, &source_with(new), 2, "  return n;", new, &test);
        assert!(found.is_ok(), "{label}: {found:?}");
    }
}

#[test]
fn a_paired_old_line_inside_a_template_keeps_its_probe_when_the_new_line_is_code()
-> Result<(), String> {
    // The real diff pairing sends new line 3 (`return 7;` plus trailing
    // spaces) to old line 3 (`return 7;`, template content). Equal trimmed
    // text is not a no-op: the old line was string content.
    let new_source = "export function price(): number {\n  const s = `old`;\n  return 7;   \n  // formerly the rest of a template\n}\n";
    let patch = "--- a/src/price.ts\n+++ b/src/price.ts\n@@ -1,5 +1,5 @@\n export function price(): number {\n-  const s = `old\n-  return 7;\n-`; return 0;\n+  const s = `old`;\n+  return 7;   \n+  // formerly the rest of a template\n }\n";
    let changed = crate::analysis::diff::parse_unified_diff(patch);
    let file = changed.first().ok_or("no changed file")?;
    let pairs = file.aligned_counterparts();
    assert_eq!(
        pairs.get(&3).map(|removed| removed.text.as_str()),
        Some("  return 7;"),
        "new line 3 must pair with old `  return 7;`"
    );
    let root = ts_unique_tempdir("infect-old-template")?;
    ts_write_file(&root.join("src/price.ts"), new_source)?;
    ts_write_file(&root.join("tests/price.test.ts"), &weak_test("price()"))?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &changed,
    );
    let _ = std::fs::remove_dir_all(&root);
    let findings = result?.findings;
    assert!(
        findings.iter().any(|f| f.probe.location.line == 3),
        "line 3 must stay reported: {:?}",
        findings
            .iter()
            .map(|f| f.probe.location.line)
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn old_template_content_is_found_through_a_two_hunk_diff() -> Result<(), String> {
    // Hunk one edits line 1; hunk two (far below) is the template counterexample.
    let pad: String = (1..=9).map(|n| format!("const pad{n} = {n};\n")).collect();
    let new_source = format!(
        "// v2\n{pad}export function price(): number {{\n  const s = `old`;\n  return 7;   \n  // rest\n}}\n"
    );
    let ctx_top: String = (1..=3).map(|n| format!(" const pad{n} = {n};\n")).collect();
    let ctx_mid: String = (7..=9).map(|n| format!(" const pad{n} = {n};\n")).collect();
    let patch = format!(
        "--- a/src/price.ts\n+++ b/src/price.ts\n@@ -1,4 +1,4 @@\n-// v1\n+// v2\n{ctx_top}@@ -8,9 +8,9 @@\n{ctx_mid} export function price(): number {{\n-  const s = `old\n-  return 7;\n-`; return 0;\n+  const s = `old`;\n+  return 7;   \n+  // rest\n }}\n"
    );
    let changed = crate::analysis::diff::parse_unified_diff(&patch);
    let file = changed.first().ok_or("no changed file")?;
    assert_eq!(
        file.aligned_counterparts()
            .get(&13)
            .map(|removed| removed.text.as_str()),
        Some("  return 7;")
    );
    let root = ts_unique_tempdir("infect-old-template-2")?;
    ts_write_file(&root.join("src/price.ts"), &new_source)?;
    ts_write_file(&root.join("tests/price.test.ts"), &weak_test("price()"))?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &changed,
    );
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        result?.findings.iter().any(|f| f.probe.location.line == 13),
        "line 13 must stay reported"
    );
    Ok(())
}
