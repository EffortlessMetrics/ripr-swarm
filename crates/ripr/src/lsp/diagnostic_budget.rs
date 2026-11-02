//! Shared transport-neutral diagnostic delivery budgeting lives in
//! [`crate::app::diagnostic_budget`]; LSP keeps this path for wire/schema
//! compatibility and existing call sites (#5193).

pub use crate::app::diagnostic_budget::*;
