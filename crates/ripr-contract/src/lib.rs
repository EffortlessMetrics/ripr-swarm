#![forbid(unsafe_code)]
//! Contract types shared by the `ripr` facade and its internal support
//! crates (ADR 0027, #8382/#12726).
//!
//! This crate owns the language-neutral domain DTOs and value-origin shells
//! that language adapters and the facade exchange. It is an implementation
//! package published in lockstep with `ripr`; its types are re-exported at
//! their established `ripr::domain` paths and are not a separate public API.

pub mod analysis;
pub mod config;
pub mod core_error;
pub mod domain;
pub mod path_identity;
mod path_unicode;
pub mod value_origins;
// Cargo and Nextest provide the test package's runtime manifest directory.
// Nextest remaps this value when consuming a producer's binary elsewhere.
#[cfg(test)]
fn test_manifest_dir() -> std::path::PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR").map_or_else(
        || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        std::path::PathBuf::from,
    )
}
