//! Run snapshot binding and requested/prepared subject compatibility (#6347).
//!
//! Pure decision surface: references accepted shared snapshot identities,
//! performs no Git/Cargo/provider/shell/network work, and grants no execution
//! authority.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "policy:6347: passive VP25A binding surface before cargo-proof integration consumes it"
    )
)]

mod decide;
mod model;

#[cfg(test)]
mod tests;

#[expect(
    unused_imports,
    reason = "policy:6347: VP25 binding surface for follow-on slices"
)]
pub(crate) use decide::{
    decide_snapshot_binding, portable_run_binding_identity, requested_prepared_compatibility,
};
#[expect(
    unused_imports,
    reason = "policy:6347: VP25 binding surface for follow-on slices"
)]
pub(crate) use model::{
    BindingCompleteness, CompatibilityMismatchAxis, PreparedRootMetadataV1,
    PreparedVerificationSubjectRefV1, RepositorySnapshotEnvelopeRefV2,
    RequestedVerificationSubjectV1, SharedAcquisitionReceiptRefV1, SubjectCapturePolicyV1,
    SubjectCompleteness, VERIFICATION_RUN_SNAPSHOT_BINDING_CLAIM_BOUNDARY,
    VERIFICATION_RUN_SNAPSHOT_BINDING_SCHEMA_VERSION, VerificationLiveSubjectRelationV1,
    VerificationPlanBindingRefV1, VerificationRunSnapshotBindingV1,
    VerificationSnapshotBindingDecisionV1, VerificationSnapshotCompatibilityV1,
    VerificationSourceSubjectKindV2, VerifiedContentDigest,
};
