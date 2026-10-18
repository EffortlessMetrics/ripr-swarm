/// Charge terminal elapsed, or keep the admission reservation when completion
/// is unknown. Do not refund task slots, cap overruns, or invent zero-cost work.
pub(crate) fn focused_proof_lease_charged_seconds(lease: &ResourceLease) -> u64 {
    match lease.settled_elapsed_ms {
        Some(millis) => u64::try_from(millis.div_ceil(1000))
            .unwrap_or(u64::MAX)
            .max(1),
        None => lease.timeout_sec,
    }
}
