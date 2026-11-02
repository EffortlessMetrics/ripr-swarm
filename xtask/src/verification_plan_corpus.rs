//! Change-to-verification plan corpus contract (#5173, RIPR-SPEC-0377 PR A).
//!
//! Typed, versioned DTOs for one governed verification-plan corpus row and the
//! fail-closed validator that decides whether a committed row may enter the
//! mechanics or real denominators. Synthetic mechanics rows prove selection
//! law, identity binding, and judgment boundaries only; they never count as
//! real-repository safety evidence.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const VERIFICATION_PLAN_CASE_SCHEMA_VERSION: &str = "verification_plan_case.v1";
pub(crate) const VERIFICATION_PLAN_CORPUS_SCHEMA_VERSION: &str = "verification_plan_corpus.v1";
pub(crate) const VERIFICATION_PLAN_FIXTURE_CORPUS_SCHEMA_VERSION: &str =
    "verification_plan_fixture_corpus.v1";
pub(crate) const VERIFICATION_PLAN_CORPUS_REPORT_SCHEMA_VERSION: &str =
    "verification_plan_corpus_report.v1";
pub(crate) const VERIFICATION_PLAN_CLAIM_BOUNDARY: &str = "Static verification-plan corpus \
 contract: a counted row defines auditable change-to-verification evidence only; it claims no \
 production-safe omission, no statistical accuracy, and no admitted CI policy.";

pub(crate) const DEFAULT_VERIFICATION_PLAN_CORPUS_PATH: &str =
    "fixtures/verification-plan-corpus/corpus.json";

/// Closed case-class vocabulary (#5173). Synthetic mechanics stay separate from
/// real evidence strata in every projection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationPlanCaseClassV1 {
    SyntheticMechanics,
    RetainedRepositoryRegression,
    HistoricalFailure,
    HistoricalCleanControl,
    MutationGeneratedChange,
    CompileOrApiChange,
    ConfigurationOrBuildChange,
    ResourceOnlyChange,
    IsolationOrOrderDependence,
    UnsupportedOrOpaqueBoundary,
    InfrastructureOrInconclusive,
}

impl VerificationPlanCaseClassV1 {
    pub(crate) fn is_synthetic_mechanics(self) -> bool {
        matches!(self, Self::SyntheticMechanics)
    }
}

/// Planner disposition vocabulary (#5173 judgment boundary).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationPlanPlannerDispositionV1 {
    PlanSelectedMaterialOutcome,
    PlanDelayedMaterialOutcome,
    WouldMissMaterialOutcome,
    OverWidenBeyondJustifiedScope,
    CorrectFallbackOrRefusal,
    OutcomeNotMeasurable,
    InfrastructureLimited,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanProvenanceV1 {
    pub source: String,
    pub license: String,
    pub retention_class: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanChangeIdentityV1 {
    pub repository: String,
    pub base: String,
    pub head: String,
    pub tree: String,
    pub patch_identity: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanCommitmentV1 {
    pub plan_identity: String,
    pub plan_digest: String,
    pub committed_before_outcomes: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanRouteIdentitiesV1 {
    pub fast_command_identity: String,
    pub complete_command_identity: String,
    pub full_command_identity: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanExecutionObservationV1 {
    pub selected_subjects: Vec<String>,
    pub discovered_subjects: Vec<String>,
    pub executed_subjects: Vec<String>,
    pub targeted_observation_succeeded: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanJudgmentV1 {
    pub judgment_generation: String,
    pub independent_reviewer: String,
    pub material_outcome: String,
    pub disposition: VerificationPlanPlannerDispositionV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanCaseV1 {
    pub schema_version: String,
    pub case_id: String,
    pub generation: String,
    pub case_class: VerificationPlanCaseClassV1,
    pub synthetic: bool,
    pub selection_stratum: String,
    pub selection_rationale: String,
    pub selection_frozen_before_outcomes: bool,
    pub provenance: VerificationPlanProvenanceV1,
    pub change: VerificationPlanChangeIdentityV1,
    pub changed_subject_denominator: Vec<String>,
    pub expected_obligation_families: Vec<String>,
    pub plan: VerificationPlanCommitmentV1,
    pub routes: VerificationPlanRouteIdentitiesV1,
    pub execution: VerificationPlanExecutionObservationV1,
    pub judgment: VerificationPlanJudgmentV1,
    pub evidence_digests: Vec<String>,
    pub non_claims: Vec<String>,
    #[serde(default)]
    pub limitations: Vec<String>,
    #[serde(default)]
    pub removed_from_manifest: bool,
    #[serde(default)]
    pub successor_of: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanFixtureExpectationV1 {
    pub accepted: bool,
    pub disposition: VerificationPlanPlannerDispositionV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanFixtureScenarioV1 {
    pub id: String,
    pub expected: VerificationPlanFixtureExpectationV1,
    pub case: VerificationPlanCaseV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanFixtureCorpusV1 {
    pub schema_version: String,
    pub spec: String,
    pub scenarios: Vec<VerificationPlanFixtureScenarioV1>,
}

#[derive(Clone, Debug)]
pub(crate) struct VerificationPlanRowAssessmentV1 {
    pub case_id: String,
    pub synthetic: bool,
    pub case_class: VerificationPlanCaseClassV1,
    pub accepted: bool,
    pub disposition: VerificationPlanPlannerDispositionV1,
    pub reasons: Vec<String>,
}

/// PR A synthetic falsification matrix: every control in #5173 required controls
/// has at least one discriminating mechanics row.
pub(crate) const REQUIRED_VERIFICATION_PLAN_MECHANICS_SCENARIO_IDS: [&str; 16] = [
    "mechanics_plan_committed_before_outcomes",
    "mechanics_would_miss_remains_visible",
    "mechanics_would_delay_not_miss",
    "mechanics_correct_fallback",
    "mechanics_over_widen",
    "mechanics_zero_subjects_not_success",
    "mechanics_infrastructure_inconclusive",
    "mechanics_removed_row_stays_denominator",
    "mechanics_successor_preserves_miss",
    "mechanics_missing_artifact_incomplete",
    "mechanics_equivalent_not_selector_miss",
    "mechanics_wrong_plan_identity_rejected",
    "mechanics_synthetic_not_real_evidence",
    "mechanics_deterministic_identity",
    "mechanics_judgment_generation_bound",
    "mechanics_stale_plan_identity_rejected",
];

pub(crate) fn missing_verification_plan_required_scenarios<'a>(
    present: impl IntoIterator<Item = &'a str>,
) -> Vec<&'static str> {
    let present: BTreeSet<&str> = present.into_iter().collect();
    REQUIRED_VERIFICATION_PLAN_MECHANICS_SCENARIO_IDS
        .iter()
        .filter(|required| !present.contains(**required))
        .copied()
        .collect()
}

pub(crate) fn load_verification_plan_fixture_corpus(
    body: &str,
) -> Result<VerificationPlanFixtureCorpusV1, String> {
    let corpus: VerificationPlanFixtureCorpusV1 = serde_json::from_str(body)
        .map_err(|error| format!("parse verification plan fixture corpus: {error}"))?;
    if corpus.schema_version != VERIFICATION_PLAN_FIXTURE_CORPUS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported verification plan fixture corpus schema `{}`",
            corpus.schema_version
        ));
    }
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if !ids.insert(scenario.id.as_str()) {
            return Err(format!(
                "duplicate verification plan fixture scenario id `{}`",
                scenario.id
            ));
        }
        if scenario.id != scenario.case.case_id {
            return Err(format!(
                "scenario id `{}` must match case.case_id `{}`",
                scenario.id, scenario.case.case_id
            ));
        }
    }
    Ok(corpus)
}

fn digest_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

pub(crate) fn verification_plan_row_digest(
    case: &VerificationPlanCaseV1,
) -> Result<String, String> {
    if case.schema_version != VERIFICATION_PLAN_CASE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported verification plan case schema `{}`",
            case.schema_version
        ));
    }
    let body = serde_json::to_string(case)
        .map_err(|error| format!("serialize verification plan case for digest: {error}"))?;
    Ok(digest_hex(&body))
}

pub(crate) fn verification_plan_corpus_identity(
    cases: &[VerificationPlanCaseV1],
) -> Result<String, String> {
    let mut digests = Vec::with_capacity(cases.len());
    for case in cases {
        digests.push(verification_plan_row_digest(case)?);
    }
    digests.sort();
    Ok(digest_hex(&digests.join("\n")))
}

fn expected_plan_digest(plan_identity: &str) -> String {
    digest_hex(&format!("verification-plan:{plan_identity}"))
}

/// Fail-closed row validator (#5173 controls 1–15 for committed mechanics rows).
pub(crate) fn assess_verification_plan_case(
    case: &VerificationPlanCaseV1,
) -> VerificationPlanRowAssessmentV1 {
    let mut reasons = Vec::new();
    let mut accepted = true;

    let push = |reasons: &mut Vec<String>, accepted: &mut bool, reason: String| {
        *accepted = false;
        reasons.push(reason);
    };

    if case.schema_version != VERIFICATION_PLAN_CASE_SCHEMA_VERSION {
        push(
            &mut reasons,
            &mut accepted,
            format!("unsupported case schema `{}`", case.schema_version),
        );
    }
    if case.case_id.trim().is_empty() {
        push(&mut reasons, &mut accepted, "missing case_id".to_string());
    }
    if case.synthetic != case.case_class.is_synthetic_mechanics() {
        push(
            &mut reasons,
            &mut accepted,
            "synthetic flag must agree with synthetic_mechanics case_class".to_string(),
        );
    }
    if !case.selection_frozen_before_outcomes {
        push(
            &mut reasons,
            &mut accepted,
            "prospective mechanics rows must freeze selection before outcomes".to_string(),
        );
    }
    if !case.plan.committed_before_outcomes {
        push(
            &mut reasons,
            &mut accepted,
            "verification plan must be committed before outcomes for mechanics rows".to_string(),
        );
    }
    if expected_plan_digest(&case.plan.plan_identity) != case.plan.plan_digest {
        push(
            &mut reasons,
            &mut accepted,
            "plan_digest must match plan_identity (anti-hardcode control)".to_string(),
        );
    }
    if case.execution.targeted_observation_succeeded && case.execution.executed_subjects.is_empty()
    {
        push(
            &mut reasons,
            &mut accepted,
            "zero executed subjects cannot be a successful targeted observation".to_string(),
        );
    }
    if case.evidence_digests.is_empty() && !case.removed_from_manifest {
        push(
            &mut reasons,
            &mut accepted,
            "missing evidence digests make the row incomplete".to_string(),
        );
    }
    if case.judgment.judgment_generation.trim().is_empty() {
        push(
            &mut reasons,
            &mut accepted,
            "judgment_generation is required".to_string(),
        );
    }
    if case.judgment.independent_reviewer.trim().is_empty() {
        push(
            &mut reasons,
            &mut accepted,
            "independent reviewer identity is required".to_string(),
        );
    }
    if case.removed_from_manifest && case.successor_of.is_none() {
        push(
            &mut reasons,
            &mut accepted,
            "removed manifest rows must retain successor linkage or stay explicitly incomplete"
                .to_string(),
        );
    }

    VerificationPlanRowAssessmentV1 {
        case_id: case.case_id.clone(),
        synthetic: case.synthetic,
        case_class: case.case_class,
        accepted,
        disposition: case.judgment.disposition,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex64(ch: char) -> String {
        ch.to_string().repeat(64)
    }

    fn sample_case(disposition: VerificationPlanPlannerDispositionV1) -> VerificationPlanCaseV1 {
        let plan_identity = "plan-mechanics-sample";
        VerificationPlanCaseV1 {
            schema_version: VERIFICATION_PLAN_CASE_SCHEMA_VERSION.to_string(),
            case_id: "mechanics_sample".to_string(),
            generation: "gen-1".to_string(),
            case_class: VerificationPlanCaseClassV1::SyntheticMechanics,
            synthetic: true,
            selection_stratum: "synthetic_mechanics".to_string(),
            selection_rationale: "fixture".to_string(),
            selection_frozen_before_outcomes: true,
            provenance: VerificationPlanProvenanceV1 {
                source: "fixtures/verification-plan-corpus".to_string(),
                license: "MIT".to_string(),
                retention_class: "committed_fixture".to_string(),
            },
            change: VerificationPlanChangeIdentityV1 {
                repository: "EffortlessMetrics/ripr-swarm".to_string(),
                base: hex64('a'),
                head: hex64('b'),
                tree: hex64('c'),
                patch_identity: "patch:mechanics-sample".to_string(),
            },
            changed_subject_denominator: vec!["subject:sample".to_string()],
            expected_obligation_families: vec!["test_execution".to_string()],
            plan: VerificationPlanCommitmentV1 {
                plan_identity: plan_identity.to_string(),
                plan_digest: expected_plan_digest(plan_identity),
                committed_before_outcomes: true,
            },
            routes: VerificationPlanRouteIdentitiesV1 {
                fast_command_identity: "cmd:fast:sample".to_string(),
                complete_command_identity: "cmd:complete:sample".to_string(),
                full_command_identity: "cmd:full:sample".to_string(),
            },
            execution: VerificationPlanExecutionObservationV1 {
                selected_subjects: vec!["test::sample".to_string()],
                discovered_subjects: vec!["test::sample".to_string()],
                executed_subjects: vec!["test::sample".to_string()],
                targeted_observation_succeeded: true,
            },
            judgment: VerificationPlanJudgmentV1 {
                judgment_generation: "judgment-gen-1".to_string(),
                independent_reviewer: "reviewer:fixture".to_string(),
                material_outcome: "observed".to_string(),
                disposition,
            },
            evidence_digests: vec!["evidence:sha256:".to_string() + &hex64('d')],
            non_claims: vec!["Not real repository safety evidence.".to_string()],
            limitations: Vec::new(),
            removed_from_manifest: false,
            successor_of: None,
        }
    }

    #[test]
    fn verification_plan_row_digest_is_stable() -> Result<(), String> {
        let case = sample_case(VerificationPlanPlannerDispositionV1::CorrectFallbackOrRefusal);
        let first = verification_plan_row_digest(&case)?;
        let second = verification_plan_row_digest(&case)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn verification_plan_validator_rejects_wrong_plan_digest() {
        let mut case = sample_case(VerificationPlanPlannerDispositionV1::WouldMissMaterialOutcome);
        case.plan.plan_digest = "sha256:".to_string() + &hex64('0');
        let assessment = assess_verification_plan_case(&case);
        assert!(!assessment.accepted);
        assert!(
            assessment
                .reasons
                .iter()
                .any(|reason| reason.contains("plan_digest"))
        );
    }

    #[test]
    fn verification_plan_validator_rejects_zero_subject_success() {
        let mut case = sample_case(VerificationPlanPlannerDispositionV1::WouldMissMaterialOutcome);
        case.execution.executed_subjects.clear();
        case.execution.targeted_observation_succeeded = true;
        let assessment = assess_verification_plan_case(&case);
        assert!(!assessment.accepted);
    }

    #[test]
    fn committed_fixture_corpus_covers_required_mechanics_scenarios() -> Result<(), String> {
        let body = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join(DEFAULT_VERIFICATION_PLAN_CORPUS_PATH),
        )
        .map_err(|error| format!("read committed verification plan corpus: {error}"))?;
        let corpus = load_verification_plan_fixture_corpus(&body)?;
        let missing = missing_verification_plan_required_scenarios(
            corpus.scenarios.iter().map(|scenario| scenario.id.as_str()),
        );
        assert!(
            missing.is_empty(),
            "missing required mechanics scenarios: {missing:?}"
        );
        Ok(())
    }
}
