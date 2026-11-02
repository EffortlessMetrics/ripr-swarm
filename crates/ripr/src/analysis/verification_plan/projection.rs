//! Pure projection from producer-owned facts into `ChangeVerificationPlanV1`.

use crate::analysis::seam_classification::ClassifiedSeam;
use crate::analysis::test_grip_evidence::RelatedTestGrip;
use crate::analysis::witness::{
    AdapterInput, BehaviorEvidenceWitnessV1, InputCurrentness, from_classified_seam_with_input,
    from_finding_with_input,
};
use crate::change_verification_plan_digest::{
    change_verification_plan_semantic_digest, normalize_portable_path, normalize_test_identity,
};
use crate::domain::{
    CHANGE_VERIFICATION_PLAN_CLAIM_BOUNDARY, CHANGE_VERIFICATION_PLAN_SCHEMA_VERSION,
    ChangeVerificationPlanChangedSubject, ChangeVerificationPlanCommand,
    ChangeVerificationPlanCompleteness, ChangeVerificationPlanImpactedTest,
    ChangeVerificationPlanStageStates, ChangeVerificationPlanSubjectIdentity,
    ChangeVerificationPlanTestIdentity, ChangeVerificationPlanUncoveredSubject,
    ChangeVerificationPlanV1, ChangeVerificationPlanValidatingTest,
    ChangeVerificationPlanWidenedObligation, CommandSpec, Finding, OracleStrength,
    PlanRelationEvidenceClass, RelationReason, VerificationCommandPurpose,
    VerificationObligationKind, WidenedObligationScope,
};

pub(crate) struct ChangeVerificationPlanInput<'a> {
    pub(crate) subject_identity: ChangeVerificationPlanSubjectIdentity,
    pub(crate) adapter_input: AdapterInput,
    pub(crate) commands: Vec<&'a CommandSpec>,
}

pub(crate) fn project_plan_from_finding(
    finding: &Finding,
    input: &ChangeVerificationPlanInput<'_>,
) -> Result<ChangeVerificationPlanV1, String> {
    let witness = from_finding_with_input(finding, input.adapter_input);
    project_plan_from_witness_and_tests(
        input,
        witness,
        changed_subject_from_finding(finding),
        finding_related_tests(finding),
        &finding
            .probe
            .expected_sinks
            .first()
            .cloned()
            .unwrap_or_default(),
    )
}

pub(crate) fn project_plan_from_classified_seam(
    seam: &ClassifiedSeam,
    input: &ChangeVerificationPlanInput<'_>,
) -> Result<ChangeVerificationPlanV1, String> {
    let witness = from_classified_seam_with_input(seam, input.adapter_input);
    let tests: Vec<(
        ChangeVerificationPlanTestIdentity,
        RelationReason,
        OracleStrength,
        bool,
    )> = seam
        .evidence
        .related_tests
        .iter()
        .map(|grip| {
            (
                grip_test_identity(grip),
                grip.relation_reason,
                grip.oracle_strength.clone(),
                grip.test_target.is_some(),
            )
        })
        .collect();
    project_plan_from_witness_and_tests(
        input,
        witness,
        changed_subject_from_seam(seam),
        tests,
        &seam.seam.expected_sink().as_str().to_string(),
    )
}

fn project_plan_from_witness_and_tests(
    input: &ChangeVerificationPlanInput<'_>,
    witness: BehaviorEvidenceWitnessV1,
    changed_subjects: Vec<ChangeVerificationPlanChangedSubject>,
    tests: Vec<(
        ChangeVerificationPlanTestIdentity,
        RelationReason,
        OracleStrength,
        bool,
    )>,
    expected_sink: &str,
) -> Result<ChangeVerificationPlanV1, String> {
    let limitations = witness.limitations.clone();
    let mut non_claims = witness.non_claims.clone();
    non_claims.push("plan_does_not_recompute_witness_meaning".to_string());
    non_claims.push("affinity_candidate_never_authorizes_exclusion".to_string());

    let stages = ChangeVerificationPlanStageStates {
        reach: witness.reach.state.clone(),
        activate: witness.activation.state.clone(),
        propagate: witness.propagation.state.clone(),
        observe: witness.observation.state.clone(),
        discriminate: witness.discrimination.state.clone(),
    };

    let mut impacted_tests = Vec::new();
    let mut validating_tests = Vec::new();

    for (test, reason, strength, has_target) in tests {
        let evidence_class = classify_relation_evidence(reason);
        if evidence_class.may_authorize_exclusion() {
            return Err("plan projection attempted to authorize exclusion".to_string());
        }
        if evidence_class.may_include_in_impacted_set() {
            impacted_tests.push(ChangeVerificationPlanImpactedTest {
                test: test.clone(),
                evidence_class,
                relation_basis: reason.as_str().to_string(),
                path_identities: Vec::new(),
                base_presence: "unknown".to_string(),
                head_presence: if has_target {
                    "selected_existing".to_string()
                } else {
                    "typed_absence".to_string()
                },
                target_state: if has_target {
                    "selected_existing".to_string()
                } else {
                    "typed_absence".to_string()
                },
                currentness: input_currentness_label(input.adapter_input.currentness),
                applicability: applicability_for_class(evidence_class),
            });
        } else if evidence_class == PlanRelationEvidenceClass::AffinityCandidate {
            impacted_tests.push(ChangeVerificationPlanImpactedTest {
                test: test.clone(),
                evidence_class,
                relation_basis: reason.as_str().to_string(),
                path_identities: Vec::new(),
                base_presence: "unknown".to_string(),
                head_presence: "present".to_string(),
                target_state: "inspect_only".to_string(),
                currentness: input_currentness_label(input.adapter_input.currentness),
                applicability: "inspect_only".to_string(),
            });
        }

        if qualifies_as_validating(reason, &strength, expected_sink, &stages) {
            validating_tests.push(ChangeVerificationPlanValidatingTest {
                test,
                validated_subjects: changed_subjects
                    .iter()
                    .map(|subject| subject.subject_identity.clone())
                    .collect(),
                stages: stages.clone(),
                oracle_kind: reason.as_str().to_string(),
                oracle_strength: strength.as_str().to_string(),
                oracle_alignment: oracle_alignment_label(reason, expected_sink),
                first_unresolved_edge: witness
                    .earliest_unresolved_edge
                    .as_ref()
                    .map(|edge| format!("{}:{}", edge.stage, edge.detail)),
            });
        }
    }

    sort_impacted(&mut impacted_tests);
    sort_validating(&mut validating_tests);

    let mut widened_obligations = Vec::new();
    for limitation in &limitations {
        if limitation.starts_with("static_limit:")
            || limitation.contains("opaque")
            || limitation.contains("macro_limit")
            || limitation.contains("dynamic_dispatch")
        {
            widened_obligations.push(ChangeVerificationPlanWidenedObligation {
                scope: WidenedObligationScope::Workspace,
                obligation_kind: VerificationObligationKind::Test,
                widening_reason: format!("opaque_boundary:{limitation}"),
                originating_edge: witness
                    .earliest_unresolved_edge
                    .as_ref()
                    .map(|edge| format!("{}:{}", edge.stage, edge.detail)),
            });
        }
    }
    if limitations
        .iter()
        .any(|item| item.contains("cross_language"))
    {
        widened_obligations.push(ChangeVerificationPlanWidenedObligation {
            scope: WidenedObligationScope::Package,
            obligation_kind: VerificationObligationKind::Build,
            widening_reason: "opaque_boundary:cross_language_limit".to_string(),
            originating_edge: None,
        });
    }

    let uncovered_changed_subjects =
        uncovered_subjects(&changed_subjects, &validating_tests, &stages);
    let completeness = derive_completeness(
        input.adapter_input.currentness,
        &limitations,
        &uncovered_changed_subjects,
        &widened_obligations,
    );

    let commands = input
        .commands
        .iter()
        .map(|spec| ChangeVerificationPlanCommand {
            command_id: spec.command_id.clone(),
            purpose: VerificationCommandPurpose::FastValidation,
            state: "referenced".to_string(),
        })
        .collect();

    let mut plan = ChangeVerificationPlanV1 {
        schema_version: CHANGE_VERIFICATION_PLAN_SCHEMA_VERSION.to_string(),
        plan_id: String::new(),
        semantic_digest: String::new(),
        subject_identity: input.subject_identity.clone(),
        changed_subjects,
        impacted_tests,
        validating_tests,
        uncovered_changed_subjects,
        widened_obligations,
        commands,
        completeness,
        limitations,
        non_claims,
        claim_boundary: CHANGE_VERIFICATION_PLAN_CLAIM_BOUNDARY.to_string(),
    };
    finalize_plan(&mut plan)?;
    Ok(plan)
}

pub(crate) fn classify_relation_evidence(reason: RelationReason) -> PlanRelationEvidenceClass {
    match reason {
        RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall => {
            PlanRelationEvidenceClass::ResolvedMayDepend
        }
        RelationReason::SeamCalleeCall
        | RelationReason::ImportPathAffinity
        | RelationReason::ReExportChainFollowed => PlanRelationEvidenceClass::ConservativeMayDepend,
        RelationReason::WeakTokenSubstring
        | RelationReason::SameTestFile
        | RelationReason::SameModule
        | RelationReason::OwnerNamedTest
        | RelationReason::FixtureOwnerAffinity
        | RelationReason::AssertionTargetAffinity => PlanRelationEvidenceClass::AffinityCandidate,
    }
}

pub(crate) fn qualifies_as_validating(
    reason: RelationReason,
    strength: &OracleStrength,
    expected_sink: &str,
    stages: &ChangeVerificationPlanStageStates,
) -> bool {
    if !matches!(
        reason,
        RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall
    ) {
        return false;
    }
    if !matches!(strength, OracleStrength::Strong | OracleStrength::Medium) {
        return false;
    }
    if oracle_alignment_label(reason, expected_sink) != "observes_expected_sink" {
        return false;
    }
    stages.reach == "yes" && stages.observe == "yes" && stages.discriminate != "no"
}

pub(crate) fn oracle_alignment_label(reason: RelationReason, expected_sink: &str) -> String {
    if expected_sink.trim().is_empty() {
        return "sink_unspecified".to_string();
    }
    if matches!(
        reason,
        RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall
    ) {
        "observes_expected_sink".to_string()
    } else {
        format!(
            "impacted_at_most:wrong_or_weak_relation:{}",
            reason.as_str()
        )
    }
}

fn applicability_for_class(class: PlanRelationEvidenceClass) -> String {
    match class {
        PlanRelationEvidenceClass::ResolvedMayDepend => "may_depend".to_string(),
        PlanRelationEvidenceClass::ConservativeMayDepend => "conservative_may_depend".to_string(),
        PlanRelationEvidenceClass::ValidationObligation => "validation_obligation".to_string(),
        PlanRelationEvidenceClass::ObservedExecution => "corroboration_only".to_string(),
        PlanRelationEvidenceClass::AffinityCandidate => "inspect_only".to_string(),
        PlanRelationEvidenceClass::OpaqueBoundary => "widen".to_string(),
    }
}

fn input_currentness_label(currentness: InputCurrentness) -> String {
    match currentness {
        InputCurrentness::Current => "current".to_string(),
        InputCurrentness::Stale => "stale".to_string(),
        InputCurrentness::Wrong => "wrong".to_string(),
        InputCurrentness::Unspecified => "unspecified".to_string(),
    }
}

fn derive_completeness(
    currentness: InputCurrentness,
    limitations: &[String],
    uncovered: &[ChangeVerificationPlanUncoveredSubject],
    widened: &[ChangeVerificationPlanWidenedObligation],
) -> ChangeVerificationPlanCompleteness {
    let stale = !matches!(currentness, InputCurrentness::Current);
    let unsupported = limitations.iter().any(|item| item.contains("unsupported"));
    let limited = !uncovered.is_empty() || !widened.is_empty() || stale;
    let not_proven = stale || unsupported || limited;
    ChangeVerificationPlanCompleteness {
        complete_for_declared_model: false,
        conservative_with_fallback: !widened.is_empty(),
        limited,
        stale,
        unsupported,
        not_proven,
        declared_model: CHANGE_VERIFICATION_PLAN_SCHEMA_VERSION.to_string(),
        unsupported_families: limitations
            .iter()
            .filter(|item| item.contains("unsupported") || item.contains("static_limit"))
            .cloned()
            .collect(),
    }
}

fn uncovered_subjects(
    changed: &[ChangeVerificationPlanChangedSubject],
    validating: &[ChangeVerificationPlanValidatingTest],
    stages: &ChangeVerificationPlanStageStates,
) -> Vec<ChangeVerificationPlanUncoveredSubject> {
    if validating.is_empty() {
        return changed
            .iter()
            .map(|subject| ChangeVerificationPlanUncoveredSubject {
                subject_identity: subject.subject_identity.clone(),
                earliest_missing_stage: earliest_missing_stage(stages),
                missing_detail: "no_validating_test".to_string(),
            })
            .collect();
    }
    Vec::new()
}

fn earliest_missing_stage(stages: &ChangeVerificationPlanStageStates) -> String {
    for (name, value) in [
        ("reach", &stages.reach),
        ("activate", &stages.activate),
        ("propagate", &stages.propagate),
        ("observe", &stages.observe),
        ("discriminate", &stages.discriminate),
    ] {
        if value != "yes" {
            return name.to_string();
        }
    }
    "discriminate".to_string()
}

fn changed_subject_from_finding(finding: &Finding) -> Vec<ChangeVerificationPlanChangedSubject> {
    let subject_identity = finding
        .canonical_gap
        .as_ref()
        .map(|gap| gap.id.clone())
        .unwrap_or_else(|| finding.id.clone());
    vec![ChangeVerificationPlanChangedSubject {
        subject_identity,
        change_kind: finding.probe.delta.as_str().to_string(),
        behavior_family: finding.probe.family.as_str().to_string(),
        base_source_identity: finding.probe.before.clone(),
        head_source_identity: finding.probe.after.clone(),
        source_locations: vec![normalize_portable_path(
            finding.probe.location.file.to_string_lossy().as_ref(),
        )],
    }]
}

fn changed_subject_from_seam(seam: &ClassifiedSeam) -> Vec<ChangeVerificationPlanChangedSubject> {
    vec![ChangeVerificationPlanChangedSubject {
        subject_identity: seam.seam.id().as_str().to_string(),
        change_kind: "repo_seam".to_string(),
        behavior_family: seam.seam.kind().as_str().to_string(),
        base_source_identity: None,
        head_source_identity: None,
        source_locations: vec![normalize_portable_path(&seam.seam.file().to_string_lossy())],
    }]
}

fn finding_related_tests(
    finding: &Finding,
) -> Vec<(
    ChangeVerificationPlanTestIdentity,
    RelationReason,
    OracleStrength,
    bool,
)> {
    finding
        .related_tests
        .iter()
        .filter_map(|test| {
            let reason = test.relation_reason?;
            Some((
                related_test_identity(test),
                reason,
                test.oracle_strength.clone(),
                false,
            ))
        })
        .collect()
}

fn related_test_identity(test: &crate::domain::RelatedTest) -> ChangeVerificationPlanTestIdentity {
    normalize_test_identity(&test.name, test.file.to_string_lossy().as_ref(), test.line)
}

fn grip_test_identity(grip: &RelatedTestGrip) -> ChangeVerificationPlanTestIdentity {
    normalize_test_identity(
        &grip.test_name,
        grip.file.to_string_lossy().as_ref(),
        grip.line,
    )
}

fn sort_impacted(tests: &mut [ChangeVerificationPlanImpactedTest]) {
    tests.sort_by(|left, right| {
        left.evidence_class
            .cmp(&right.evidence_class)
            .then_with(|| left.test.file.cmp(&right.test.file))
            .then_with(|| left.test.test_name.cmp(&right.test.test_name))
            .then_with(|| left.test.line.cmp(&right.test.line))
    });
}

fn sort_validating(tests: &mut [ChangeVerificationPlanValidatingTest]) {
    tests.sort_by(|left, right| {
        left.test
            .file
            .cmp(&right.test.file)
            .then_with(|| left.test.test_name.cmp(&right.test.test_name))
            .then_with(|| left.test.line.cmp(&right.test.line))
    });
}

pub(crate) fn finalize_plan(plan: &mut ChangeVerificationPlanV1) -> Result<(), String> {
    plan.limitations.sort();
    plan.limitations.dedup();
    plan.non_claims.sort();
    plan.non_claims.dedup();
    plan.completeness.unsupported_families.sort();
    plan.completeness.unsupported_families.dedup();
    let digest = change_verification_plan_semantic_digest(plan)?;
    plan.semantic_digest = digest.clone();
    plan.plan_id = digest;
    Ok(())
}

/// Keep the finding adapter on the production path without changing public bytes.
pub(crate) fn retain_finding_plan_projection(finding: &Finding, input: AdapterInput) {
    let subject = ChangeVerificationPlanSubjectIdentity {
        repository: "unspecified".to_string(),
        base_revision: None,
        head_revision: None,
        worktree_identity: None,
        analyzer_generation: None,
        schema_generation: None,
        language: finding.language.map(|lang| lang.as_str().to_string()),
        config_profile: None,
        features: Vec::new(),
        manifest_lock_inputs: Vec::new(),
    };
    let plan_input = ChangeVerificationPlanInput {
        subject_identity: subject,
        adapter_input: input,
        commands: Vec::new(),
    };
    let plan = project_plan_from_finding(finding, &plan_input);
    debug_assert!(
        plan.as_ref().is_ok_and(|value| value.digest_matches()),
        "change verification plan digest must match canonical bytes"
    );
}

#[cfg(test)]
pub(crate) fn test_only_plan_with_promoted_affinity(
    plan: &mut ChangeVerificationPlanV1,
) -> Result<(), String> {
    if let Some(row) = plan.impacted_tests.first_mut() {
        row.evidence_class = PlanRelationEvidenceClass::ResolvedMayDepend;
        row.relation_basis = "direct_owner_call".to_string();
        row.applicability = "may_depend".to_string();
    }
    finalize_plan(plan)
}

#[cfg(test)]
pub(crate) fn test_only_plan_without_limitations(plan: &mut ChangeVerificationPlanV1) {
    plan.limitations.clear();
    plan.completeness.limited = false;
    plan.completeness.not_proven = false;
}
