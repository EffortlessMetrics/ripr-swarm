//! Change-verification plan projection (#5163, VP1).
//!
//! Additive pure projection over witness and grip producer facts. Does not
//! change public classification, CLI output, or cache behavior.

mod projection;

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
