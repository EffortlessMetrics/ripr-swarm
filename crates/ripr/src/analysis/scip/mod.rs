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

/// SHA-256 of `vendor/scip.proto` that `generated/scip.rs` was produced from.
/// Update both together: regenerate, then set this to the new proto digest.
#[cfg(test)]
const PINNED_PROTO_SHA256: &str =
    "fd9dc475394a672b4e830c2cbb399e16e576b4402aa2fac19e2924dbf8dace46";

#[cfg(test)]
mod tests {
    use super::PINNED_PROTO_SHA256;
    use sha2::{Digest, Sha256};

    /// Drift gate: editing the vendored proto without regenerating and
    /// re-pinning fails here. It binds the pin, not the generator output, so
    /// regeneration itself stays a reviewed step.
    #[test]
    fn vendored_proto_matches_the_pin_the_generated_bindings_were_built_from() -> Result<(), String>
    {
        let proto = include_bytes!("../../../vendor/scip.proto");
        let actual: String = Sha256::digest(proto)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if actual != PINNED_PROTO_SHA256 {
            return Err(format!(
                "vendor/scip.proto digest {actual} differs from pinned {PINNED_PROTO_SHA256}; \
                 regenerate analysis/scip/generated/scip.rs and update the pin"
            ));
        }
        Ok(())
    }
}
