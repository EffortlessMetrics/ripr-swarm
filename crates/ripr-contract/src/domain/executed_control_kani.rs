//! Kani harnesses for executed-control result-state predicates.
//! Run: `cargo kani -p ripr --harness <name>`.

use super::*;

const RESULT_STATES: [ResultState; 6] = [
    ResultState::Passed,
    ResultState::Failed,
    ResultState::NotRun,
    ResultState::NotProven,
    ResultState::Substituted,
    ResultState::InstrumentFailure,
];

const OUTCOMES: [ObservedOutcome; 8] = [
    ObservedOutcome::FailedBeforePassedAfter,
    ObservedOutcome::RejectedWrongImplementation,
    ObservedOutcome::CommandSucceededWithoutExercisingSubject,
    ObservedOutcome::OrdinaryPositiveTestsPassed,
    ObservedOutcome::ReviewArgumentOnly,
    ObservedOutcome::StructuralDiscriminationOnly,
    ObservedOutcome::InstrumentUnavailable,
    ObservedOutcome::NotExecuted,
];

// Wildcard-free matches: a new variant fails to compile under `cfg(kani)` until
// the symbolic domains above are extended.
fn result_state_domain_is_exhaustive(state: ResultState) {
    match state {
        ResultState::Passed
        | ResultState::Failed
        | ResultState::NotRun
        | ResultState::NotProven
        | ResultState::Substituted
        | ResultState::InstrumentFailure => {}
    }
}

fn outcome_domain_is_exhaustive(outcome: ObservedOutcome) {
    match outcome {
        ObservedOutcome::FailedBeforePassedAfter
        | ObservedOutcome::RejectedWrongImplementation
        | ObservedOutcome::CommandSucceededWithoutExercisingSubject
        | ObservedOutcome::OrdinaryPositiveTestsPassed
        | ObservedOutcome::ReviewArgumentOnly
        | ObservedOutcome::StructuralDiscriminationOnly
        | ObservedOutcome::InstrumentUnavailable
        | ObservedOutcome::NotExecuted => {}
    }
}

fn any_result_state() -> ResultState {
    let index: usize = kani::any();
    kani::assume(index < RESULT_STATES.len());
    let value = RESULT_STATES[index];
    result_state_domain_is_exhaustive(value);
    value
}

fn any_outcome() -> ObservedOutcome {
    let index: usize = kani::any();
    kani::assume(index < OUTCOMES.len());
    let value = OUTCOMES[index];
    outcome_domain_is_exhaustive(value);
    value
}

/// Satisfaction is never claimed by a failure-like state. The exact
/// membership table is deliberately not restated here: a harness that rebuilds
/// the production `match` only checks a copy, so the table stays covered by
/// unit tests and this harness verifies the safety direction only.
#[kani::proof]
fn failure_like_states_never_claim_satisfaction() {
    let state = any_result_state();
    if matches!(
        state,
        ResultState::Failed
            | ResultState::NotRun
            | ResultState::NotProven
            | ResultState::InstrumentFailure
    ) {
        kani::assert(
            !state.claims_satisfaction(),
            "a failure-like state claims satisfaction",
        );
    }
    kani::cover!(state.claims_satisfaction());
    kani::cover!(!state.claims_satisfaction());
}

/// An outcome that records a real discriminating observation (failed-before /
/// rejected-wrong-implementation) or an explicit instrument gap is never
/// classified as an unexecuted substitute, and an outcome that never executed
/// the subject always is. The full partition table is not restated (see above).
#[kani::proof]
fn real_observations_are_never_unexecuted() {
    let outcome = any_outcome();
    if matches!(
        outcome,
        ObservedOutcome::FailedBeforePassedAfter
            | ObservedOutcome::RejectedWrongImplementation
            | ObservedOutcome::InstrumentUnavailable
    ) {
        kani::assert(
            !substitute_outcome_is_unexecuted(outcome),
            "a real observation is classified as unexecuted",
        );
    }
    if outcome == ObservedOutcome::NotExecuted {
        kani::assert(
            substitute_outcome_is_unexecuted(outcome),
            "a never-executed outcome is not classified as unexecuted",
        );
    }
    kani::cover!(substitute_outcome_is_unexecuted(outcome));
    kani::cover!(!substitute_outcome_is_unexecuted(outcome));
}
