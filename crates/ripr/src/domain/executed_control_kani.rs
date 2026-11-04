//! Kani harnesses for executed-control result-state predicates.
//! Run: `cargo kani -p ripr --harness <name>`.

use super::*;

use crate::domain::proof_domains::{
    OUTCOMES, RESULT_STATES, cover_outcomes, cover_result_states, expected_satisfaction,
    expected_unexecuted,
};

fn any_result_state() -> ResultState {
    let index: usize = kani::any();
    kani::assume(index < RESULT_STATES.len());
    let value = RESULT_STATES[index];
    cover_result_states(&value);
    value
}

fn any_outcome() -> ObservedOutcome {
    let index: usize = kani::any();
    kani::assume(index < OUTCOMES.len());
    let value = OUTCOMES[index];
    cover_outcomes(&value);
    value
}

/// Every state agrees with the SPEC-0183 claim table in both directions.
/// Each state has its own reachability witness, including both positive states.
#[kani::proof]
fn result_state_satisfaction_matches_contract() {
    let state = any_result_state();
    assert_eq!(state.claims_satisfaction(), expected_satisfaction(state));
}

/// SPEC-0183's complete executed/unexecuted partition, with a witness for
/// every outcome. Instrument unavailability remains distinct from execution.
#[kani::proof]
fn substitute_outcome_execution_partition_matches_contract() {
    let outcome = any_outcome();
    assert_eq!(
        substitute_outcome_is_unexecuted(outcome),
        expected_unexecuted(outcome)
    );
}
