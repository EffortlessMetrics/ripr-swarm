//! Versioned cross-language edge registry contract (#6481 / #5559).
//!
//! Defines typed, producer-owned cross-language relation edges without
//! migrating Bun producers or inferring edges from names, paths, or
//! co-location.

mod currentness;
mod digest;
mod load;
mod model;
mod movement;
mod validate;

pub use currentness::{
    analyze_cross_language_edge_registry_currentness, subjects_related_by_effective_registered_edge,
};
pub use digest::{
    cross_language_edge_content_digest, cross_language_edge_registry_semantic_digest,
    cross_language_edge_semantic_key,
};
pub use load::load_cross_language_edge_registry_from_json;
pub use model::*;
pub use movement::compare_cross_language_edge_registries;
pub use validate::{edge_connects_subjects, subjects_related_by_registered_edge};

#[cfg(test)]
mod tests;
