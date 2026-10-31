//! Kani harnesses for executed-control result-state predicates.
//! Run: `cargo kani -p ripr --harness <name>`.

use super::*;

fn any_result_state() -> ResultState {
    match kani::any::<u8>() % 6 {
        0 => ResultState::Passed,
        1 => ResultState::Failed,
        2 => ResultState::NotRun,
        3 => ResultState::NotProven,
        4 => ResultState::Substituted,
        _ => ResultState::InstrumentFailure,
    }
}

fn any_outcome() -> ObservedOutcome {
    match kani::any::<u8>() % 8 {
        0 => ObservedOutcome::FailedBeforePassedAfter,
        1 => ObservedOutcome::RejectedWrongImplementation,
        2 => ObservedOutcome::CommandSucceededWithoutExercisingSubject,
        3 => ObservedOutcome::OrdinaryPositiveTestsPassed,
        4 => ObservedOutcome::ReviewArgumentOnly,
        5 => ObservedOutcome::StructuralDiscriminationOnly,
        6 => ObservedOutcome::InstrumentUnavailable,
        _ => ObservedOutcome::NotExecuted,
    }
}

/// Only `passed` and `substituted` claim satisfaction; every non-pass state
/// (failed, not_run, not_proven, instrument_failure) stays non-satisfying.
#[kani::proof]
fn only_pass_and_substitute_claim_satisfaction() {
    let state = any_result_state();
    let expected = state == ResultState::Passed || state == ResultState::Substituted;
    assert_eq!(state.claims_satisfaction(), expected);
    if matches!(
        state,
        ResultState::Failed
            | ResultState::NotRun
            | ResultState::NotProven
            | ResultState::InstrumentFailure
    ) {
        assert!(!state.claims_satisfaction());
    }
    kani::cover!(state.claims_satisfaction());
    kani::cover!(!state.claims_satisfaction());
}

/// An outcome that records a real discriminating observation (failed-before /
/// rejected-wrong-implementation) or an explicit instrument gap is never
/// classified as an unexecuted substitute, while ordinary-positive,
/// review-only, structural-only, subject-not-exercised and not-executed are.
#[kani::proof]
fn substitute_outcome_unexecuted_partition() {
    let outcome = any_outcome();
    let real = matches!(
        outcome,
        ObservedOutcome::FailedBeforePassedAfter
            | ObservedOutcome::RejectedWrongImplementation
            | ObservedOutcome::InstrumentUnavailable
    );
    assert_eq!(substitute_outcome_is_unexecuted(outcome), !real);
    kani::cover!(substitute_outcome_is_unexecuted(outcome));
    kani::cover!(real);
}
