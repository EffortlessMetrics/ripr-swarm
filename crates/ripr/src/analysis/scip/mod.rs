//! Optional read-only SCIP index admission (#8115).
//!
//! SCIP types remain behind the generated protobuf module; production code
//! projects admitted facts into portable identities for diagnostic/parity use
//! without changing classification in the first slice.

pub(crate) mod admission;
pub(crate) mod generated {
    #[allow(
        dead_code,
        clippy::all,
        reason = "policy:8115-scip-generated: prost output from pinned scip.proto stays behind the adapter"
    )]
    pub mod scip {
        include!(concat!(env!("OUT_DIR"), "/scip.rs"));
    }
}
