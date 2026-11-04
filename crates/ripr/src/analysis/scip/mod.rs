//! Optional read-only SCIP index admission (#8115).
//!
//! SCIP types remain behind checked-in prost output from `vendor/scip.proto`
//! (regenerate with `prost-build` and `.disable_comments` when the pin changes).

pub(crate) mod admission;
pub(crate) mod generated {
    #[allow(
        dead_code,
        clippy::all,
        reason = "policy:8115-scip-generated: prost output from pinned scip.proto stays behind the adapter"
    )]
    pub mod scip {
        include!("generated/scip.rs");
    }
}
