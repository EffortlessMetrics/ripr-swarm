//! Shared, feature-scoped internal automation contracts.
//! Each command retains its existing public entrypoint.

#[cfg(feature = "formal-evidence-audit")]
pub mod formal_evidence;
