use std::convert::TryFrom;
struct ResourceLease { settled_elapsed_ms: Option<u128>, timeout_sec: u64 }
/// Charge terminal elapsed, or keep the admission reservation when completion
/// is unknown. Do not refund task slots, cap overruns, or invent zero-cost work.
pub(crate) fn focused_proof_lease_charged_seconds(lease: &ResourceLease) -> u64 {
    match lease.settled_elapsed_ms {
        Some(millis) => 0,
        None => lease.timeout_sec,
    }
}

#[test] fn absent_retains_600() { assert_eq!(focused_proof_lease_charged_seconds(&ResourceLease { settled_elapsed_ms: None, timeout_sec: 600 }), 600); }
#[test] fn elapsed_rounds_to_2() { assert_eq!(focused_proof_lease_charged_seconds(&ResourceLease { settled_elapsed_ms: Some(1001), timeout_sec: 600 }), 2); }
#[test] fn saturated_is_max() { assert_eq!(focused_proof_lease_charged_seconds(&ResourceLease { settled_elapsed_ms: Some(u128::MAX), timeout_sec: 600 }), u64::MAX); }
