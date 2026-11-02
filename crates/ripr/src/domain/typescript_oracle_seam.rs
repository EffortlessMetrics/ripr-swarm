//! TypeScript oracle-kind ↔ probe-family seam table (RIPR-SPEC-0104).
//!
//! Shared by the TypeScript classifier, assertion selection, and preview
//! related-test projection so output and analysis agree without crossing
//! module boundaries.

use super::{OracleKind, ProbeFamily};

/// Whether an assertion oracle kind can observe a changed probe family.
pub(crate) fn ts_oracle_kind_matches_seam(
    oracle_kind: &OracleKind,
    probe_family: &ProbeFamily,
) -> bool {
    match probe_family {
        ProbeFamily::ErrorPath => !matches!(
            oracle_kind,
            OracleKind::ExactValue | OracleKind::RelationalCheck | OracleKind::MockExpectation
        ),
        ProbeFamily::ReturnValue
        | ProbeFamily::Predicate
        | ProbeFamily::FieldConstruction
        | ProbeFamily::MatchArm => !matches!(
            oracle_kind,
            OracleKind::ExactErrorVariant | OracleKind::BroadError | OracleKind::MockExpectation
        ),
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => true,
        ProbeFamily::StaticUnknown => true,
    }
}
