//! Contract types shared by the `ripr` facade and its internal support
//! crates (ADR 0026, #8382/#12726).
//!
//! This crate owns the language-neutral domain DTOs and value-origin shells
//! that language adapters and the facade exchange. It is an implementation
//! package published in lockstep with `ripr`; its types are re-exported at
//! their established `ripr::domain` paths and are not a separate public API.

pub mod domain;
pub mod value_origins;
