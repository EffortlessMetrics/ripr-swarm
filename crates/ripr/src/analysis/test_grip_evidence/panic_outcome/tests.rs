use super::*;
use crate::analysis::rust_index::RustIndex;
use crate::analysis::seams::{ExpectedSink, RequiredDiscriminator, SeamKind};
use crate::domain::{Confidence, MissingDiscriminatorFact};

fn seam() -> RepoSeam {
    RepoSeam::new(
        "src/lib.rs",
        "div",
        SeamKind::PredicateBoundary,
        2,
        7,
        "b == 1",
        RequiredDiscriminator::BoundaryValue {
            description: "b == 1".to_string(),
        },
        ExpectedSink::ReturnValue,
    )
}

fn stage(state: StageState) -> StageEvidence {
    StageEvidence::new(state, Confidence::Medium, "plain stage")
}

fn evidence(states: [StageState; 5]) -> TestGripEvidence {
    TestGripEvidence {
        seam_id: seam().id().clone(),
        related_tests: Vec::new(),
        reach: stage(states[0].clone()),
        activate: stage(states[1].clone()),
        propagate: stage(states[2].clone()),
        observe: stage(states[3].clone()),
        discriminate: stage(states[4].clone()),
        observed_values: Vec::new(),
        missing_discriminators: vec![MissingDiscriminatorFact {
            value: "b == 1".to_string(),
            reason: "not observed".to_string(),
            flow_sink: None,
        }],
        statically_contradicted_related_tests: 0,
        new_test_target: None,
    }
}

fn labels() -> Vec<String> {
    vec!["`t` (tests/t.rs:4)".to_string()]
}

#[test]
fn no_owner_means_the_limit_does_not_apply() {
    let index = RustIndex::default();
    let none = deciding_tests([("t", Path::new("tests/t.rs"), 4)], None, 2, &index);
    assert!(none.is_empty());
}

#[test]
fn an_unrevealed_seam_is_withheld_under_the_typed_limit_once() {
    use StageState::{No, Yes};
    let seam = seam();
    let mut gap = evidence([Yes, Yes, Yes, No, No]);
    assert_eq!(
        classify_seam(&seam, &gap),
        SeamGripClass::ReachableUnrevealed
    );
    assert!(withhold(&seam, &mut gap, &labels()));
    assert!(gap.names_static_limit(LIMIT));
    assert_eq!(gap.discriminate.state, StageState::Unknown);
    assert!(gap.missing_discriminators.is_empty());
    // The seam no longer reads as a gap, so a second pass changes nothing.
    let settled = gap.discriminate.summary.clone();
    assert!(!withhold(&seam, &mut gap, &labels()));
    assert_eq!(gap.discriminate.summary, settled);
}

#[test]
fn a_weak_asserting_test_keeps_the_gap_and_is_not_withheld() {
    use StageState::{Weak, Yes};
    let seam = seam();
    let mut gap = evidence([Yes, Yes, Yes, Yes, Weak]);
    assert_eq!(classify_seam(&seam, &gap), SeamGripClass::WeaklyGripped);
    assert!(!withhold(&seam, &mut gap, &labels()));
    assert!(!gap.names_static_limit(LIMIT));
    assert_eq!(gap.discriminate.state, Weak);
    assert_eq!(gap.missing_discriminators.len(), 1);
}

#[test]
fn no_deciding_test_and_an_unreached_seam_are_not_withheld() {
    use StageState::{No, Yes};
    let seam = seam();
    let mut gap = evidence([Yes, Yes, Yes, No, No]);
    assert!(!withhold(&seam, &mut gap, &[]));
    assert!(!gap.names_static_limit(LIMIT));
    let mut unreached = evidence([No, No, No, No, No]);
    assert!(!withhold(&seam, &mut unreached, &labels()));
    let mut opaque = evidence([Yes, Yes, Yes, StageState::Opaque, No]);
    assert!(!withhold(&seam, &mut opaque, &labels()));
    assert_eq!(opaque.observe.state, StageState::Opaque);
}
