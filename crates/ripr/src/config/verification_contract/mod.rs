//! Repository verification contract compiler (VP29A / #5223, #6428).
//!
//! Passive compilation joins admitted verification declaration sources into one
//! deterministic, provenance-rich contract. It does not execute Cargo, tests,
//! providers, shell, network, or credential operations.

mod compiler;
mod inventory;
mod model;

pub(crate) use compiler::{
    VerificationContractCompileInput, compile_repository_verification_contract,
};
pub(crate) use model::RepositoryVerificationContractDecisionV1;
