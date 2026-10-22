use super::*;
use crate::reports::verdict_corpus::SubjectOrigin;

fn row(
    case: &str,
    subject: &str,
    family: &str,
    truth: TruthState,
    observed: Verdict,
    outcome: Outcome,
) -> CaseRow {
    CaseRow {
        case_id: case.to_string(),
        subject_id: subject.to_string(),
        origin: SubjectOrigin::Authored,
        anchor: "src/lib.rs:1".to_string(),
        behavior_family: family.to_string(),
        test_shape: "shape".to_string(),
        hard_case: false,
        truth,
        ideal_verdict: truth_ideal(truth),
        observed_verdict: observed,
        followed_retarget: false,
        changed_since_labeling: false,
        observed_classifications: Vec::new(),
        outcome,
        contradictions: Vec::new(),
        findings_scored: 0,
        findings_contradicted: 0,
        contradiction_counts: BTreeMap::new(),
        packet_provenance: None,
        oracle: None,
    }
}

fn truth_ideal(truth: TruthState) -> Verdict {
    if truth == TruthState::Discriminated {
        Verdict::Credited
    } else {
        Verdict::Gap
    }
}

fn gate(cell_floor: usize, trap_floor: usize, ceiling: usize) -> GridGate {
    GridGate {
        cell_floor,
        trap_floor,
        false_credit_ceiling: ceiling,
        waived: Vec::new(),
    }
}

#[test]
fn every_operator_family_maps_to_one_operator() {
    assert_eq!(operator_of("predicate_boundary"), Some("boundary"));
    assert_eq!(operator_of("predicate_equality"), Some("equality"));
    assert_eq!(operator_of("predicate_boolean"), Some("logical"));
    assert_eq!(operator_of("return_value_arithmetic"), Some("arithmetic"));
    assert_eq!(operator_of("arithmetic_constant"), Some("constant"));
    assert_eq!(operator_of("return_value"), Some("return_value"));
    assert_eq!(operator_of("side_effect_field_write"), Some("deletion"));
    assert_eq!(operator_of("match_arm"), None);
}

#[test]
fn cells_count_by_truth_and_score() {
    let rows = vec![
        row(
            "a",
            "s",
            "predicate_boundary",
            TruthState::Discriminated,
            Verdict::Credited,
            Outcome::Ideal,
        ),
        row(
            "b",
            "s",
            "predicate_boundary",
            TruthState::NotDiscriminated,
            Verdict::Credited,
            Outcome::FalseExposed,
        ),
        row(
            "c",
            "s",
            "predicate_boundary",
            TruthState::PartiallyDiscriminated,
            Verdict::Limited,
            Outcome::Abstained,
        ),
    ];
    let report = grid_report(&rows, &gate(3, 0, 0), "ledger".to_string(), false);
    assert_eq!(report.covered_cells, 3);
    assert_eq!(report.denominator, 21);
    let gap = report
        .cells
        .iter()
        .find(|c| c.operator == "boundary" && c.state == "gap")
        .map(|c| (c.cases, c.false_verdicts));
    assert_eq!(gap, Some((1, 1)));
    assert!(report.uncovered.contains(&"equality/credited".to_string()));
    assert_eq!(grid_gate(&report).ok(), Some(None));
}

#[test]
fn traps_and_controls_split_by_truth() {
    let rows = vec![
        row(
            "t1",
            "authored-trap-kit",
            "return_value",
            TruthState::NotDiscriminated,
            Verdict::Credited,
            Outcome::FalseExposed,
        ),
        row(
            "t2",
            "authored-trap-kit",
            "return_value",
            TruthState::NotDiscriminated,
            Verdict::Gap,
            Outcome::Ideal,
        ),
        row(
            "c1",
            "authored-trap-kit",
            "return_value",
            TruthState::Discriminated,
            Verdict::Gap,
            Outcome::FalseActionable,
        ),
        row(
            "g1",
            "authored-grid",
            "return_value",
            TruthState::NotDiscriminated,
            Verdict::Gap,
            Outcome::Ideal,
        ),
    ];
    let report = grid_report(&rows, &gate(0, 0, 1), "ledger".to_string(), false);
    assert_eq!(report.traps.traps, 2);
    assert_eq!(report.traps.credited, 1);
    assert_eq!(report.traps.gapped, 1);
    assert_eq!(report.traps.controls, 1);
    assert_eq!(report.traps.controls_gapped, 1);
}

#[test]
fn gate_fails_below_floors_and_above_the_ceiling() {
    let rows = vec![row(
        "t1",
        "authored-trap-kit",
        "return_value",
        TruthState::NotDiscriminated,
        Verdict::Credited,
        Outcome::FalseExposed,
    )];
    let report = grid_report(&rows, &gate(2, 2, 0), "ledger".to_string(), false);
    let failure = grid_gate(&report).err().unwrap_or_default();
    assert!(
        failure.contains("grid cells covered fell to 1/21 below the floor 2"),
        "{failure}"
    );
    assert!(
        failure.contains("trap cases fell to 1 below the floor 2"),
        "{failure}"
    );
    assert!(
        failure.contains("false-credited traps rose to 1/1 above the ceiling 0"),
        "{failure}"
    );
}

#[test]
fn gate_notes_improvement_and_subset_runs_are_not_gated() {
    let rows = vec![row(
        "g1",
        "authored-grid",
        "return_value",
        TruthState::NotDiscriminated,
        Verdict::Gap,
        Outcome::Ideal,
    )];
    let report = grid_report(&rows, &gate(0, 0, 3), "ledger".to_string(), false);
    let note = grid_gate(&report).ok().flatten().unwrap_or_default();
    assert!(note.contains("raise `cell_floor`"), "{note}");
    assert!(note.contains("lower `false_credit_ceiling`"), "{note}");
    let subset = grid_report(&rows, &gate(99, 99, 0), "ledger".to_string(), true);
    assert_eq!(grid_gate(&subset).ok(), Some(None));
}

#[test]
fn waivers_remove_a_cell_and_stale_waivers_fail() {
    let rows = vec![row(
        "g1",
        "authored-grid",
        "return_value",
        TruthState::NotDiscriminated,
        Verdict::Gap,
        Outcome::Ideal,
    )];
    let mut waiving = gate(0, 0, 0);
    waiving.waived = vec![
        GridWaiver {
            operator: "logical".to_string(),
            state: "partial".to_string(),
            reason: "no logical mutant can pass".to_string(),
        },
        GridWaiver {
            operator: "return_value".to_string(),
            state: "gap".to_string(),
            reason: "covered after all".to_string(),
        },
        GridWaiver {
            operator: "nonsense".to_string(),
            state: "gap".to_string(),
            reason: "typo".to_string(),
        },
    ];
    let report = grid_report(&rows, &waiving, "ledger".to_string(), false);
    assert_eq!(report.stale_waivers.len(), 2, "{:?}", report.stale_waivers);
    assert!(!report.uncovered.contains(&"logical/partial".to_string()));
    assert!(
        grid_gate(&report)
            .err()
            .unwrap_or_default()
            .contains("stale grid waivers")
    );
}
