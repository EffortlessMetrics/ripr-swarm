//! Bounded Verus dialect admission inside `verus! { .. }` regions (#10747).
//!
//! ripr does not expand `verus!` or run the Verus verifier. This module applies
//! a pinned line-oriented lexer to admit owner-bound guard facts for a small
//! supported grammar and leaves everything else on the RIPR-SPEC-0369 floor.

mod dialect;

pub(crate) use dialect::{
    VerusGuardFact, VerusItemMode, VerusTrustBoundary, guard_fact_at_line, verus_guard_facts,
};
