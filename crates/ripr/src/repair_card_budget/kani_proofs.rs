//! Kani bounded-model-checking proofs for the pure cores of the repair-card
//! budget engine. Compiled only under `cargo kani`; normal builds never see
//! this file.
//!
//! `apply_repair_card_budget` itself serializes JSON and hashes with SHA-256,
//! and `reject_root_specific_route` formats its refusal with `format!`; both
//! exhaust CBMC (the route-predicate harness ran past 12 minutes and 4.9 GB
//! without finishing), so neither is covered here and they remain guarded by
//! the unit tests. What is proved is the family ordering key the engine sorts
//! and deduplicates by. The budget limits' own arithmetic lives in
//! `RepairCardBudget::validate` (proved in `domain/repair_card/kani_proofs.rs`).

use super::*;

fn any_family() -> RepairCardDetailFamily {
    let index: u8 = kani::any();
    kani::assume(index < 9);
    match index {
        0 => RepairCardDetailFamily::FixInstruction,
        1 => RepairCardDetailFamily::WitnessStageEvidence,
        2 => RepairCardDetailFamily::RelatedTestCandidates,
        3 => RepairCardDetailFamily::LimitationDetail,
        4 => RepairCardDetailFamily::CanonicalPacket,
        5 => RepairCardDetailFamily::RepairAttemptStatus,
        6 => RepairCardDetailFamily::FocusedProofReceipt,
        7 => RepairCardDetailFamily::StaticMovement,
        _ => RepairCardDetailFamily::MutationCalibration,
    }
}

/// The family ordering/dedup key is injective: two distinct families can
/// never share a sort key, so key-sorted order is a total order on families
/// and the adjacent-duplicate check (`window[0].family == window[1].family`)
/// catches every duplicate. Covers all 9 families pairwise.
#[kani::proof]
#[kani::unwind(40)]
fn family_key_is_injective() {
    let a = any_family();
    let b = any_family();
    if family_key(a) == family_key(b) {
        assert!(a == b);
    }
    kani::cover!(a != b);
    kani::cover!(a == b);
}
