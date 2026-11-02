//! Versioned cross-language edge registry contract (#6481 / #5559).
//!
//! Defines typed, producer-owned cross-language relation edges without
//! migrating Bun producers or inferring edges from names, paths, or
//! co-location.

mod digest;
mod model;
mod validate;

pub use digest::cross_language_edge_registry_semantic_digest;
pub use model::*;
pub use validate::subjects_related_by_registered_edge;

#[cfg(test)]
mod tests;
