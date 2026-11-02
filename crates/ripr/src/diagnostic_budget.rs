//! Transport-neutral diagnostic delivery budgeting for non-LSP consumers (#5193).
//!
//! The canonical implementation lives under [`crate::lsp::diagnostic_budget`]
//! for wire compatibility; MCP and other adapters import this facade instead of
//! the LSP module directly.

pub use crate::lsp::diagnostic_budget::*;
