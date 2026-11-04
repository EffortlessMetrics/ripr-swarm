//! Change-verification planning projections (#5162 programme; VP1 projection #5163).
//!
//! Pure, process-free verification-plan models. Snapshot acquisition,
//! materialization, and provider execution remain external authorities
//! (cargo-allow / cargo-proof). The projection is additive over witness and grip
//! producer facts and does not change public classification, CLI output, or cache
//! behavior.

mod projection;
pub(crate) mod snapshot_binding;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use projection::{
    test_only_plan_with_promoted_affinity, test_only_plan_without_limitations,
};

pub(crate) use projection::{
    ChangeVerificationPlanInput, project_plan_from_finding, retain_classified_seam_plan_projection,
    retain_finding_plan_projection,
};

#[cfg(test)]
pub(crate) use projection::{classify_relation_evidence, qualifies_as_validating};
