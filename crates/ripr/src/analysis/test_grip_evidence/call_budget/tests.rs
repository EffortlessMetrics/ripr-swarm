use super::*;
use crate::analysis::extract::extract_call_facts;
use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
use crate::domain::{MissingDiscriminatorFact, ValueFact};
use std::path::PathBuf;

fn test_with_line_of(distinct: usize) -> TestSummary {
    let body: String = (0..distinct).map(|i| format!("h{i}();")).collect();
    TestSummary {
        name: "wide".to_string(),
        file: PathBuf::from("tests/wide.rs"),
        start_line: 1,
        end_line: 3,
        body: body.as_str().into(),
        calls: extract_call_facts(&body, 1),
        assertions: Vec::new(),
        literals: Vec::new(),
        attrs: Vec::new(),
        nested_fn_names: Vec::new(),
        let_bindings: Vec::new(),
    }
}

fn stage(state: StageState) -> StageEvidence {
    StageEvidence::new(state, Confidence::Medium, "plain stage")
}

fn evidence(states: [StageState; 5], missing: usize) -> TestGripEvidence {
    let seam = RepoSeam::new(
        "src/lib.rs",
        "row",
        SeamKind::PredicateBoundary,
        2,
        7,
        "index >= 100",
        RequiredDiscriminator::BoundaryValue {
            description: "index >= 100".to_string(),
        },
        ExpectedSink::ReturnValue,
    );
    TestGripEvidence {
        seam_id: seam.id().clone(),
        related_tests: Vec::new(),
        reach: stage(states[0].clone()),
        activate: stage(states[1].clone()),
        propagate: stage(states[2].clone()),
        observe: stage(states[3].clone()),
        discriminate: stage(states[4].clone()),
        observed_values: Vec::<ValueFact>::new(),
        missing_discriminators: (0..missing)
            .map(|_| MissingDiscriminatorFact {
                value: "index == 100".to_string(),
                reason: "not observed".to_string(),
                flow_sink: None,
            })
            .collect(),
        statically_contradicted_related_tests: 0,
        new_test_target: None,
    }
}

fn names_limit(stage: &StageEvidence) -> bool {
    stage.state == StageState::Unknown && stage.summary.contains(LIMIT.as_str())
}

#[test]
fn saturation_is_read_at_exactly_the_shared_budget() {
    let under = test_with_line_of(MAX_CALL_FACTS_PER_LINE - 1);
    let at = test_with_line_of(MAX_CALL_FACTS_PER_LINE);
    assert_eq!(under.calls.len(), MAX_CALL_FACTS_PER_LINE - 1, "premise");
    assert_eq!(at.calls.len(), MAX_CALL_FACTS_PER_LINE, "premise");
    assert!(!any_saturated([&under]));
    assert!(any_saturated([&under, &at]));
}

#[test]
fn absence_stages_and_missing_values_become_unknown_under_the_typed_reason() {
    use StageState::{No, Yes};
    let mut gap = evidence([Yes, Yes, Yes, No, No], 1);
    assert!(withhold(&mut gap, true));
    assert_eq!(gap.reach.state, Yes);
    assert_eq!(gap.activate.state, Yes);
    assert_eq!(gap.propagate.state, Yes);
    assert!(names_limit(&gap.observe), "{:?}", gap.observe);
    assert!(names_limit(&gap.discriminate), "{:?}", gap.discriminate);
    assert!(gap.missing_discriminators.is_empty());
}

#[test]
fn an_unsaturated_evidence_is_never_changed() {
    use StageState::{No, Yes};
    let mut gap = evidence([Yes, Yes, Yes, No, No], 1);
    assert!(!withhold(&mut gap, false));
    assert_eq!(gap.observe.state, No);
    assert_eq!(gap.discriminate.state, No);
    assert_eq!(gap.missing_discriminators.len(), 1);
}

#[test]
fn a_missing_value_beside_established_stages_withholds_the_discriminator() {
    use StageState::Yes;
    let mut extension = evidence([Yes, Yes, Yes, Yes, Yes], 1);
    assert!(withhold(&mut extension, true));
    assert!(names_limit(&extension.discriminate));
    assert!(extension.missing_discriminators.is_empty());
    assert_eq!(extension.observe.state, Yes, "an established stage stays");
}

#[test]
fn positives_ungripped_and_opaque_reach_are_left_alone() {
    use StageState::{No, Opaque, Unknown, Yes};
    let mut positive = evidence([Yes, Yes, Yes, Yes, Yes], 0);
    assert!(!withhold(&mut positive, true));
    assert_eq!(positive.discriminate.summary, "plain stage");
    let mut ungripped = evidence([No, No, No, No, No], 1);
    assert!(!withhold(&mut ungripped, true));
    assert_eq!(ungripped.observe.state, No);
    assert_eq!(ungripped.missing_discriminators.len(), 1);
    let mut opaque = evidence([Opaque, Yes, Unknown, No, No], 1);
    assert!(!withhold(&mut opaque, true));
    assert_eq!(opaque.observe.state, No);
}

#[test]
fn an_already_unknown_stage_gains_the_reason_once() {
    use StageState::{Unknown, Yes};
    let mut unknown = evidence([Yes, Yes, Unknown, Yes, Yes], 0);
    assert!(withhold(&mut unknown, true));
    assert_eq!(unknown.propagate.state, Unknown);
    assert!(unknown.propagate.summary.starts_with("plain stage; "));
    assert!(unknown.propagate.summary.contains(LIMIT.as_str()));
    assert_eq!(unknown.discriminate.summary, "plain stage");
    let once = unknown.propagate.summary.clone();
    assert!(!withhold(&mut unknown, true), "idempotent");
    assert_eq!(unknown.propagate.summary, once);
}

#[test]
fn clearing_a_missing_value_still_names_the_reason_on_an_unknown_stage() {
    use StageState::{Unknown, Yes};
    // Activation already unknown beside a missing value: the value is
    // cleared and the existing unknown stage gains the typed reason.
    let mut seam = evidence([Yes, Unknown, Yes, Yes, Unknown], 1);
    assert!(withhold(&mut seam, true));
    assert!(seam.missing_discriminators.is_empty());
    assert_eq!(seam.activate.state, Unknown);
    assert!(seam.activate.summary.starts_with("plain stage; "));
    assert!(seam.activate.summary.contains(LIMIT.as_str()));
    assert!(!withhold(&mut seam, true), "idempotent");
}
