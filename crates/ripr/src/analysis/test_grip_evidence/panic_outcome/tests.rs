use super::*;
use crate::analysis::rust_index::RustIndex;
use crate::analysis::seams::{ExpectedSink, RequiredDiscriminator, SeamKind};
use crate::domain::{Confidence, MissingDiscriminatorFact, StageEvidence, StageState};

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
fn an_unrevealed_seam_names_the_typed_limit_once_without_reclassifying() {
    use StageState::{No, Yes};
    let seam = seam();
    let mut gap = evidence([Yes, Yes, Yes, No, No]);
    assert_eq!(
        classify_seam(&seam, &gap),
        SeamGripClass::ReachableUnrevealed
    );
    assert!(withhold(&seam, &mut gap, &labels()));
    assert!(gap.names_static_limit(LIMIT));
    // Classification inputs stay intact; consumers also honor the named limit.
    assert_eq!(gap.discriminate.state, StageState::No);
    assert_eq!(gap.missing_discriminators.len(), 1);
    assert_eq!(
        classify_seam(&seam, &gap),
        SeamGripClass::ReachableUnrevealed
    );
    // A second pass changes nothing.
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

/// The named limit is authoritative independently of stage uncertainty.
/// Unlike the parser fixture, these producer inputs have concrete states;
/// retaining only the old Unknown/Opaque filter must fail this control.
#[test]
fn named_panic_limit_with_concrete_stages_withholds_pilot_and_brief() {
    use crate::analysis::ClassifiedSeam;
    use crate::app::agent_brief::{
        AgentBriefPolicy, AgentBriefResolvedWorkingSet, select_agent_brief_seams,
    };
    use crate::config::RiprConfig;
    use crate::output::evidence_record::{is_static_limited, static_limitations_for};
    use crate::output::pilot::top_actionable_seams;
    use StageState::{No, Yes};

    let seam = seam();
    let ordinary = evidence([Yes, Yes, Yes, No, No]);
    let mut limited = ordinary.clone();
    assert!(withhold(&seam, &mut limited, &labels()));
    let config = RiprConfig::default();
    let working_set = AgentBriefResolvedWorkingSet::files(vec!["src/lib.rs".into()]);
    for (evidence, withheld) in [(ordinary, false), (limited, true)] {
        let class = classify_seam(&seam, &evidence);
        assert_eq!(class, SeamGripClass::ReachableUnrevealed);
        let entry = ClassifiedSeam {
            seam: seam.clone(),
            evidence,
            class,
        };
        assert_eq!(is_static_limited(&entry), withheld);
        let limits = static_limitations_for(&entry);
        if withheld {
            assert_eq!(limits.len(), 1);
            assert_eq!(limits[0].category, LIMIT.as_str());
            assert_eq!(limits[0].state, "no");
            assert_eq!(limits[0].reason, entry.evidence.discriminate.summary);
        }
        let entries = [entry];
        assert_eq!(
            top_actionable_seams(&entries, 5, None).len(),
            usize::from(!withheld)
        );
        let brief = select_agent_brief_seams(
            &entries,
            &working_set,
            3,
            AgentBriefPolicy::from_config(&config),
        );
        assert_eq!(brief.top_seams.len(), usize::from(!withheld));
        assert_eq!(brief.omitted_by_policy, usize::from(withheld));
    }
}
