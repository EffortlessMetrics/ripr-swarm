//! App-side entry for change-verification plan projection (#5163).

use crate::analysis::verification_plan::{
    ChangeVerificationPlanInput, project_plan_from_classified_seam, project_plan_from_finding,
};
use crate::analysis::seam_classification::ClassifiedSeam;
use crate::domain::{ChangeVerificationPlanV1, Finding};

pub(crate) fn build_change_verification_plan_from_finding(
    finding: &Finding,
    input: &ChangeVerificationPlanInput<'_>,
) -> Result<ChangeVerificationPlanV1, String> {
    project_plan_from_finding(finding, input)
}

pub(crate) fn build_change_verification_plan_from_classified_seam(
    seam: &ClassifiedSeam,
    input: &ChangeVerificationPlanInput<'_>,
) -> Result<ChangeVerificationPlanV1, String> {
    project_plan_from_classified_seam(seam, input)
}
