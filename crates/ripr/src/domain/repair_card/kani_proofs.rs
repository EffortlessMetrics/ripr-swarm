//! Kani bounded-model-checking proofs for the repair-card actionability
//! gates. Compiled only under `cargo kani`; normal builds never see this file.
//!
//! Each harness calls the production function (not a copy) and ranges over
//! every instruction state and every `usize` budget value.

use super::*;

/// Every `FixInstructionState` variant, selected by a symbolic index so the
/// harness covers the whole closed vocabulary. The index is assumed `< 5`,
/// the number of variants.
fn any_instruction_state() -> FixInstructionState {
    let index: u8 = kani::any();
    kani::assume(index < 5);
    match index {
        0 => FixInstructionState::FixSiteReady,
        1 => FixInstructionState::StaticLimitation,
        2 => FixInstructionState::Stale,
        3 => FixInstructionState::Unavailable,
        _ => FixInstructionState::InspectOnly,
    }
}

/// The route is exposable exactly when the instruction is `FixSiteReady` and
/// the readiness authority's flip is true; every other combination
/// (stale, limited, unavailable, inspect-only, or not ready) is closed.
#[kani::proof]
fn route_exposable_iff_fix_site_ready_and_repair_ready() {
    let instruction = any_instruction_state();
    let repair_ready: bool = kani::any();
    let exposable = repair_card_route_exposable(instruction, repair_ready);

    assert_eq!(
        exposable,
        instruction == FixInstructionState::FixSiteReady && repair_ready
    );
    if instruction != FixInstructionState::FixSiteReady {
        assert!(!exposable);
    }
    if !repair_ready {
        assert!(!exposable);
    }
    // Non-vacuity: both the open and a closed outcome are reachable.
    kani::cover!(exposable);
    kani::cover!(!exposable && instruction == FixInstructionState::FixSiteReady);
    kani::cover!(!exposable && instruction == FixInstructionState::Stale);
}

/// `RepairCardBudget::validate` accepts exactly the budgets whose three
/// limits are all non-zero, for every `usize` value (no assumptions).
#[kani::proof]
fn budget_validate_accepts_exactly_nonzero_limits() {
    let budget = RepairCardBudget {
        max_detail_items: kani::any(),
        max_serialized_bytes: kani::any(),
        max_inline_detail_bytes: kani::any(),
    };
    let accepted = budget.validate().is_ok();
    assert_eq!(
        accepted,
        budget.max_detail_items != 0
            && budget.max_serialized_bytes != 0
            && budget.max_inline_detail_bytes != 0
    );
    kani::cover!(accepted);
    kani::cover!(!accepted);
}

/// The shipped default budget is itself valid, so the default path never
/// refuses on the budget's own shape.
#[kani::proof]
fn default_budget_is_valid() {
    assert!(RepairCardBudget::default().validate().is_ok());
}
