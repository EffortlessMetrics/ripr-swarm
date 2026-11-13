//! Shared, non-executing formal evidence contracts for repository tooling.
#![forbid(unsafe_code)]

#[cfg(feature = "formal-evidence-audit")]
pub mod formal_evidence;

#[cfg(feature = "formal-assurance")]
pub mod formal_registry;
