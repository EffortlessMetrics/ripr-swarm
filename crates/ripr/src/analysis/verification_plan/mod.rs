//! Change-verification planning projections (#5162 programme).
//!
//! This tree holds pure, process-free verification-plan models. Snapshot
//! acquisition, materialization, and provider execution remain external
//! authorities (cargo-allow / cargo-proof).

pub(crate) mod snapshot_binding;
