//! Closed constructor domains for the actionability, gate and repair proofs.
//!
//! One declaration owns the input slice, exhaustive compiler check and native
//! constructor witnesses. Adding or omitting a variant fails ordinary test and
//! Kani compilation; duplicate rows fail the same generated match.

use crate::domain::{EvidenceState, FixInstructionState};
use crate::output::gate::GateMode;

macro_rules! exhaustive_domain {
    ($name:ident: $ty:ty, $cover:ident; $($variant:path),+ $(,)?) => {
        pub(crate) const $name: &[$ty] = &[$($variant),+];
        const _: fn($ty) = |value| {
            #[deny(unreachable_patterns)]
            match value { $($variant => (),)+ }
        };
        #[cfg(kani)]
        pub(crate) fn $cover(value: &$ty) {
            $(kani::cover!(*value == $variant);)+
        }
    };
}

exhaustive_domain!(EVIDENCE_STATES: EvidenceState, cover_evidence_states;
    EvidenceState::Actionable, EvidenceState::AlreadyObserved,
    EvidenceState::InternalOnly, EvidenceState::StaticLimitation, EvidenceState::Unknown);
exhaustive_domain!(GATE_MODES: GateMode, cover_gate_modes;
    GateMode::VisibleOnly, GateMode::Acknowledgeable,
    GateMode::BaselineCheck, GateMode::CalibratedGate);
exhaustive_domain!(FIX_INSTRUCTION_STATES: FixInstructionState, cover_fix_instruction_states;
    FixInstructionState::FixSiteReady, FixInstructionState::StaticLimitation,
    FixInstructionState::Stale, FixInstructionState::Unavailable, FixInstructionState::InspectOnly);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::repair_card_route_exposable;

    #[test]
    fn constructor_domains_are_nonempty_and_unique() {
        fn check<T: PartialEq + std::fmt::Debug>(values: &[T]) {
            assert!(!values.is_empty());
            for (index, value) in values.iter().enumerate() {
                assert!(!values[..index].contains(value), "duplicate {value:?}");
            }
        }
        check(EVIDENCE_STATES);
        check(GATE_MODES);
        check(FIX_INSTRUCTION_STATES);
    }

    #[test]
    fn actionability_matches_the_literal_constructor_predicate() {
        for &state in EVIDENCE_STATES {
            assert_eq!(state.is_actionable(), state == EvidenceState::Actionable);
        }
    }

    #[test]
    fn repair_route_requires_ready_instruction_and_readiness() {
        for &instruction in FIX_INSTRUCTION_STATES {
            for repair_ready in [false, true] {
                assert_eq!(
                    repair_card_route_exposable(instruction, repair_ready),
                    instruction == FixInstructionState::FixSiteReady && repair_ready
                );
            }
        }
    }
}
