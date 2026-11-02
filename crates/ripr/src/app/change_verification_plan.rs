//! App-side entry for change-verification plan projection (#5163).

#[expect(
    unused_imports,
    reason = "VP2+ routes will call the builder through this seam"
)]
pub(crate) use crate::analysis::verification_plan::{
    ChangeVerificationPlanInput, project_plan_from_classified_seam, project_plan_from_finding,
};
