//! Exact-snapshot RIPR provider contracts for external proof orchestration (#3298).
//!
//! The contract exposes RIPR-owned static evidence. It does not run project
//! tests, mutation tools, proof packs, builds, network clients, or source edits.
//!
//! # Consuming these DTOs
//!
//! Deserialization performs **no** validation: every invariant (snapshot
//! digest binding, completeness rules, closed vocabularies) lives in
//! [`validate`], and a consumer that skips it has no contract. A document
//! that parses can still be wrong — an uppercase git object id in a
//! `snapshot_id` deserializes cleanly and is rejected only by
//! `validate()`. Always deserialize **and** validate before acting on a
//! receipt, capability set, or request.
//!
//! Forward compatibility is fail-closed by design (see `docs/SEMVER.md`):
//! `deny_unknown_fields` means an added field breaks old readers, so any
//! additive change mints a new `ripr_*_v1` schema version. Old readers
//! reject new writers loudly rather than misreading them.

mod model;
mod validate;

pub use model::*;

#[cfg(test)]
mod tests;
