use super::{
    tests::{expected, observed, subject},
    *,
};
use crate::product_gate_plan::composites::{BASE_RECOVERY, CompositeResult};

type TestResult = Result<(), String>;

#[test]
fn composite_receipts_cannot_collapse_to_generic_success() -> TestResult {
    let s = subject();
    let rows = expected();
    assert_eq!(rows.iter().filter(|row| row.composite.is_some()).count(), 3);
    for producer in [
        CompositeProducer::BaseUserText,
        CompositeProducer::PrEvidence,
        CompositeProducer::SchemaSweep,
    ] {
        let mut results = observed(&rows, &s);
        let row = results
            .iter_mut()
            .find(|row| row.row.composite == Some(producer))
            .ok_or("composite fixture")?;
        row.composite = None;
        let report = inspect(&s, &rows, &results);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.starts_with("missing composite observation:"))
        );
    }
    Ok(())
}

#[test]
fn matcher_pin_is_never_cross_tree_execution() -> TestResult {
    let s = subject();
    let rows = expected();
    for mutation in 0..7 {
        let mut results = observed(&rows, &s);
        let observation = results
            .iter_mut()
            .find(|row| row.row.composite == Some(CompositeProducer::BaseUserText))
            .and_then(|row| row.composite.as_mut())
            .ok_or("user-text fixture")?;
        match mutation {
            0 => observation.matcher_sha = Some(s.head_sha.clone()),
            1 => observation.supports_cross_tree = Some(false), // Relabel legacy as executed.
            2 => {
                observation.result = CompositeResult::MatcherPinOnly;
                observation.supports_cross_tree = Some(false);
                observation.commands.clear();
            }
            3 => observation.completed_steps.swap(0, 1),
            4 => {
                observation.recovery = Some(crate::product_gate_plan::composites::PinnedRecovery {
                    after_blob: "different",
                    ..BASE_RECOVERY
                })
            }
            5 => observation.target_sha = s.head_sha.clone(),
            _ => observation.base_sha = s.head_sha.clone(),
        }
        let report = inspect(&s, &rows, &results);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert!(!report.findings.is_empty(), "mutation {mutation}");
    }
    let mut results = observed(&rows, &s);
    results
        .iter_mut()
        .find(|row| row.row.composite == Some(CompositeProducer::BaseUserText))
        .and_then(|row| row.composite.as_mut())
        .ok_or("user-text fixture")?
        .recovery = Some(BASE_RECOVERY);
    assert!(inspect(&s, &rows, &results).findings.is_empty());
    Ok(())
}

#[test]
fn ordered_commands_current_artifacts_and_one_owner_are_required() -> TestResult {
    let s = subject();
    let rows = expected();
    for producer in [
        CompositeProducer::PrEvidence,
        CompositeProducer::SchemaSweep,
    ] {
        for mutation in 0..11 {
            let mut results = observed(&rows, &s);
            let observation = results
                .iter_mut()
                .find(|row| row.row.composite == Some(producer))
                .and_then(|row| row.composite.as_mut())
                .ok_or("composite fixture")?;
            match mutation {
                0 => observation.commands.swap(0, 1),
                1 => {
                    observation.commands.pop();
                }
                2 => observation.owner.id = Some("individual-command"),
                3 => {
                    observation.artifacts.pop();
                }
                4 => observation.artifacts[0].run_id += 1,
                5 => observation.artifacts[0].run_attempt += 1,
                6 => observation.artifacts[0].subject_sha = s.head_sha.clone(),
                7 => observation.artifacts[0].edge.produced_by += 1,
                8 => observation.artifacts[0].edge.path = "stale/pr/check.json",
                9 => observation.target_sha = s.head_sha.clone(),
                _ => observation.commands[0].push_str(" || true"),
            }
            let report = inspect(&s, &rows, &results);
            assert_eq!(report.authority, Authority::NotEstablished);
            assert!(
                !report.findings.is_empty(),
                "{producer:?} mutation {mutation}"
            );
        }
    }
    Ok(())
}

#[test]
fn individual_command_rows_do_not_replace_the_composite() -> TestResult {
    let s = subject();
    let mut rows = expected();
    let index = rows
        .iter()
        .position(|row| row.composite == Some(CompositeProducer::PrEvidence))
        .ok_or("PR fixture")?;
    let composite = rows.remove(index);
    for (index, command) in crate::product_gate_plan::composites::PR_COMMANDS
        .into_iter()
        .enumerate()
    {
        let mut row = composite.clone();
        row.id = format!("pr.command.{index}");
        row.command = command.to_owned();
        row.composite = None;
        rows.push(row);
    }
    assert!(
        inspect(&s, &rows, &observed(&rows, &s))
            .findings
            .iter()
            .any(|finding| finding == "missing canonical Rust row: product.evidence.pr_evidence")
    );
    Ok(())
}

#[test]
fn event_scope_cannot_be_erased_by_success() {
    let mut s = subject();
    s.event = Event::Other;
    let rows = expected();
    let report = inspect(&s, &rows, &observed(&rows, &s));
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|finding| finding.starts_with("composite event guard excludes execution:"))
            .count(),
        2
    );
    assert_eq!(report.authority, Authority::NotEstablished);
}

#[test]
fn a_second_row_cannot_relabel_the_same_composite_owner() -> TestResult {
    let s = subject();
    let mut rows = expected();
    let mut duplicate = rows
        .iter()
        .find(|row| row.composite == Some(CompositeProducer::PrEvidence))
        .ok_or("PR fixture")?
        .clone();
    duplicate.id = "individual-command.receipt".to_owned();
    rows.push(duplicate);
    assert!(
        inspect(&s, &rows, &observed(&rows, &s))
            .findings
            .iter()
            .any(|finding| finding == "noncanonical composite owner: individual-command.receipt")
    );
    Ok(())
}

#[test]
fn retained_pr_comparison_is_b_to_h_while_doctor_stays_on_checkout() -> TestResult {
    let mut s = subject();
    s.event = Event::PullRequest;
    let rows = expected();
    let mut results = observed(&rows, &s);
    for row in &mut results {
        if let Some(observation) = &mut row.composite {
            if row.row.composite == Some(CompositeProducer::PrEvidence) {
                observation.target_sha = s.head_sha.clone();
            }
            for binding in &mut observation.artifacts {
                if binding.edge.producer == crate::product_gate_plan::ProductGateId::PrEvidence {
                    binding.subject_sha = s.head_sha.clone();
                }
            }
        }
    }
    let report = inspect(&s, &rows, &results);
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.authority, Authority::NotEstablished);
    let observation = results
        .iter_mut()
        .find(|row| row.row.composite == Some(CompositeProducer::PrEvidence))
        .and_then(|row| row.composite.as_mut())
        .ok_or("PR fixture")?;
    observation.target_sha = s.group_sha.clone();
    assert!(
        inspect(&s, &rows, &results)
            .findings
            .iter()
            .any(|finding| finding.starts_with("composite subject differs:"))
    );
    Ok(())
}

#[test]
fn oversized_composite_inputs_are_bounded_before_inspection() -> TestResult {
    let s = subject();
    let rows = expected();
    for mutation in 0..3 {
        let mut results = observed(&rows, &s);
        let observation = results
            .iter_mut()
            .find(|row| row.row.composite == Some(CompositeProducer::PrEvidence))
            .and_then(|row| row.composite.as_mut())
            .ok_or("PR fixture")?;
        match mutation {
            0 => observation.commands[0] = "x".repeat(4097),
            1 => observation.commands = vec![String::new(); 17],
            _ => observation.artifacts[0].subject_sha = "x".repeat(41),
        }
        let report = inspect(&s, &rows, &results);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert_eq!(
            report.findings,
            ["declared or observed input exceeds inspection bounds"]
        );
    }
    Ok(())
}
