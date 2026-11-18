//! Wording for a pilot whose ranking is empty because unchanged Rust paths
//! could not be read (#13624).

/// Why nothing on the current change ranks when unreadable skipped paths may
/// hold the owner or test that would explain it. Replaces the "already
/// gripped, intentional or suppressed" reading, which unreadable evidence
/// cannot support.
pub(super) fn unreadable_unranked_reason(skipped: usize) -> String {
    format!(
        "Pilot ranks no gap for this change because {skipped} unchanged Rust path(s) could not be read (permissions) and were skipped, so tests and owners in them are not considered; restore read access and re-run"
    )
}
