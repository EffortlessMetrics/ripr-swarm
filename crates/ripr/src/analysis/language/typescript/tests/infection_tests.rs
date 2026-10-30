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
        (
            "comment",
            "  return n;",
            "  return n; // fallback",
            "comment_only",
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
