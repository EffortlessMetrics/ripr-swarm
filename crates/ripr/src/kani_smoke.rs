//! Kani smoke harness: the minimal proof that keeps the `cargo xtask kani`
//! lane honest (RIPR-SPEC-0371).
//!
//! Compiled only under `cargo kani` (`cfg(kani)`); ordinary builds never see
//! this module. It proves one tiny property of production code so the lane
//! has a real, non-vacuous harness to run before any other proof lands.

use crate::domain::EvidenceState;
use crate::proof_domains::{EVIDENCE_STATES, cover_evidence_states};

/// Every `EvidenceState` variant, selected by a symbolic index so the proof
/// covers all of them without deriving `kani::Arbitrary` on production types.
fn any_state() -> EvidenceState {
    let index: usize = kani::any();
    // Bound: the compiler-checked domain contains every production constructor.
    kani::assume(index < EVIDENCE_STATES.len());
    let state = EVIDENCE_STATES[index];
    cover_evidence_states(&state);
    state
}

/// The actionability flip is fail-closed: `is_actionable` holds for exactly
/// the `Actionable` variant and for no other evidence state.
#[kani::proof]
fn evidence_state_actionable_only_for_actionable() {
    let state = any_state();
    let actionable = state.is_actionable();
    assert_eq!(actionable, state == EvidenceState::Actionable);
    // Non-vacuity: both outcomes of the property are reachable.
    kani::cover!(actionable, "an actionable state is explored");
    kani::cover!(!actionable, "a non-actionable state is explored");
}
