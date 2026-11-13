//! Bind the real cancellation namespace for a focused formal target.
//!
//! Cancellation is the exact contract module reexported by `ripr::analysis`.
//! Callers reexport this namespace and import the original process-owner file
//! at their crate root, preserving its retained reexecuted test selectors.
//! This is source-module evidence, not a build of the full ripr/xtask facade.

pub mod analysis {
    pub(crate) use ripr_contract::analysis::cancellation;
}
