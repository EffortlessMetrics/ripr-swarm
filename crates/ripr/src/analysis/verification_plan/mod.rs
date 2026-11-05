//! Change-verification planning projections (#5162 programme).
//!
//! This tree holds pure, process-free verification-plan models. Snapshot
//! acquisition, materialization, and provider execution remain external
//! authorities (cargo-allow / cargo-proof).

// Internal projection helpers; no production caller until the #5171 plan
// integration slice, so compiled for tests only to keep dead code out of builds.
#[cfg(test)]
pub(crate) mod resources;
pub(crate) mod snapshot_binding;
