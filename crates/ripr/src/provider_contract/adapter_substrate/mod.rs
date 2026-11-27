//! Versioned adapter-substrate qualification receipt contract (#6382).
//!
//! Records how a language adapter's parser, compiler service, helper, packet
//! producer, or native library was qualified and which semantic claims it may
//! support. Ecosystem ADRs remain authoritative; this receipt references them.

mod digest;
mod model;
mod validate;

pub use digest::adapter_substrate_qualification_semantic_digest;
pub use model::*;
pub use validate::substrate_classes_compatible;

#[cfg(test)]
mod tests;
