use std::path::PathBuf;

use crate::analysis::verification_plan::{
    ChangeVerificationPlanInput, classify_relation_evidence, project_plan_from_finding,
    qualifies_as_validating, test_only_plan_with_promoted_affinity,
    test_only_plan_without_limitations,
};
use crate::analysis::witness::{AdapterInput, InputCurrentness, from_finding_with_input};
use crate::change_verification_plan_digest::change_verification_plan_semantic_digest;
use crate::domain::{
    ActivationEvidence, AnalysisScope, ChangeVerificationPlanStageStates,
    ChangeVerificationPlanSubjectIdentity, ExposureClass, Finding, FindingCanonicalGap, LanguageId,
    LanguageStatus, OracleKind, OracleStrength, PlanRelationEvidenceClass, Probe, ProbeFamily,
    ProbeId, RelatedTest, RelationReason, RevealEvidence, RiprEvidence, SourceCurrentness,
    SourceLocation, StageEvidence, StageState, StaticLimitKind, StopReason, SymbolId,
};

fn ok_plan(
    result: Result<crate::domain::ChangeVerificationPlanV1, String>,
) -> crate::domain::ChangeVerificationPlanV1 {
    assert!(result.is_ok(), "{:?}", result.err());
    if let Ok(plan) = result { plan } else { loop {} }
}

fn ok_digest(result: Result<String, String>) -> String {
    assert!(result.is_ok(), "{:?}", result.err());
    if let Ok(digest) = result {
        digest
    } else {
        loop {}
    }
}

fn stage(state: StageState) -> StageEvidence {
    StageEvidence::new(state, crate::domain::Confidence::Medium, "producer")
}

fn ripr(
    reach: StageState,
    infect: StageState,
    propagate: StageState,
    observe: StageState,
    discriminate: StageState,
) -> RiprEvidence {
    RiprEvidence {
        reach: stage(reach),
        infect: stage(infect),
        propagate: stage(propagate),
        reveal: RevealEvidence {
            observe: stage(observe),
            discriminate: stage(discriminate),
        },
    }
}

fn plan_input(
    subject: ChangeVerificationPlanSubjectIdentity,
) -> ChangeVerificationPlanInput<'static> {
    ChangeVerificationPlanInput {
        subject_identity: subject,
        adapter_input: AdapterInput::diff_current(),
        commands: Vec::new(),
    }
}

fn subject_identity(base: &str, head: &str) -> ChangeVerificationPlanSubjectIdentity {
    ChangeVerificationPlanSubjectIdentity {
        repository: "demo".to_string(),
        base_revision: Some(base.to_string()),
        head_revision: Some(head.to_string()),
        worktree_identity: None,
        analyzer_generation: Some("analysis:v1".to_string()),
        schema_generation: Some("schema:v1".to_string()),
        language: Some("rust".to_string()),
        config_profile: None,
        features: Vec::new(),
        manifest_lock_inputs: Vec::new(),
    }
}

fn finding_with_related(related: Vec<RelatedTest>, ripr_evidence: RiprEvidence) -> Finding {
    Finding {
        analysis_scope: AnalysisScope::Diff,
        confidence_basis: Vec::new(),
        id: "finding:pricing".to_string(),
        canonical_gap: Some(FindingCanonicalGap {
            id: "gap:pricing:boundary".to_string(),
            language: "rust".to_string(),
            file: "src/pricing.rs".to_string(),
            owner: "pricing::discounted_total".to_string(),
            behavior_kind: "predicate_boundary".to_string(),
            probe_kind: "predicate_boundary".to_string(),
            normalized_discriminator: "amount >= threshold".to_string(),
        }),
        probe: Probe {
            id: ProbeId("probe:pricing".to_string()),
            location: SourceLocation::new("src/pricing.rs", 12, 5),
            owner: Some(SymbolId("pricing::discounted_total".to_string())),
            family: ProbeFamily::Predicate,
            delta: crate::domain::DeltaKind::Control,
            before: Some("amount >= threshold".to_string()),
            after: Some("amount > threshold".to_string()),
            expression: "amount >= threshold".to_string(),
            expected_sinks: vec!["return_value".to_string()],
            required_oracles: vec!["amount >= threshold".to_string()],
        },
        class: ExposureClass::Exposed,
        ripr: ripr_evidence,
        confidence: 0.9,
        evidence: Vec::new(),
        missing: Vec::new(),
        flow_sinks: Vec::new(),
        activation: ActivationEvidence {
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        },
        stop_reasons: Vec::new(),
        related_tests_matched_total: None,
        related_tests: related,
        recommended_next_step: None,
        language: Some(LanguageId::Rust),
        language_status: Some(LanguageStatus::Stable),
        owner_kind: None,
        static_limit_kind: None,
        changed_sink: Some("return_value".to_string()),
        observed_sink: None,
        oracle_alignment: Some("direct".to_string()),
        alignment_reason: None,
        source_currentness: SourceCurrentness::CandidateCurrent,
    }
}

#[test]
fn direct_owner_and_strong_oracle_split_impacted_and_validating_rows() {
    let finding = finding_with_related(
        vec![RelatedTest {
            name: "checks_boundary".to_string(),
            file: PathBuf::from("tests/pricing.rs"),
            line: 10,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_confidence: Some(RelationReason::DirectOwnerCall.confidence()),
            relation_reason: Some(RelationReason::DirectOwnerCall),
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }],
        ripr(
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
        ),
    );
    let plan = ok_plan(project_plan_from_finding(
        &finding,
        &plan_input(subject_identity("base-a", "head-a")),
    ));
    assert_eq!(plan.impacted_tests.len(), 1);
    assert_eq!(plan.validating_tests.len(), 1);
    assert_eq!(
        plan.impacted_tests[0].evidence_class,
        PlanRelationEvidenceClass::ResolvedMayDepend
    );
    assert!(plan.digest_matches());
}

#[test]
fn weak_affinity_is_never_resolved_may_depend() {
    assert_eq!(
        classify_relation_evidence(RelationReason::WeakTokenSubstring).as_str(),
        "affinity_candidate"
    );
    assert_eq!(
        classify_relation_evidence(RelationReason::WeakTokenSubstring),
        PlanRelationEvidenceClass::AffinityCandidate
    );
    let finding = finding_with_related(
        vec![RelatedTest {
            name: "tokenish".to_string(),
            file: PathBuf::from("tests/other.rs"),
            line: 3,
            oracle: None,
            oracle_kind: OracleKind::BroadError,
            oracle_strength: OracleStrength::Weak,
            relation_confidence: Some(RelationReason::WeakTokenSubstring.confidence()),
            relation_reason: Some(RelationReason::WeakTokenSubstring),
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }],
        ripr(
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Weak,
        ),
    );
    let plan = ok_plan(project_plan_from_finding(
        &finding,
        &plan_input(subject_identity("base", "head")),
    ));
    assert!(
        plan.impacted_tests
            .iter()
            .all(|row| row.evidence_class != PlanRelationEvidenceClass::ResolvedMayDepend)
    );
    assert!(plan.validating_tests.is_empty());
}

#[test]
fn candidate_relation_does_not_upgrade_witness_reach_by_rendering() {
    let finding = finding_with_related(
        vec![RelatedTest {
            name: "same_file".to_string(),
            file: PathBuf::from("tests/pricing.rs"),
            line: 4,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_confidence: Some(RelationReason::SameTestFile.confidence()),
            relation_reason: Some(RelationReason::SameTestFile),
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }],
        ripr(
            StageState::No,
            StageState::No,
            StageState::No,
            StageState::No,
            StageState::No,
        ),
    );
    let witness = from_finding_with_input(&finding, AdapterInput::diff_current());
    assert_ne!(witness.reach.state, "yes");
    let plan = ok_plan(project_plan_from_finding(
        &finding,
        &plan_input(subject_identity("base", "head")),
    ));
    assert!(plan.validating_tests.is_empty());
}

#[test]
fn wrong_sink_oracle_stays_impacted_at_most() {
    let stages = ChangeVerificationPlanStageStates {
        reach: "yes".to_string(),
        activate: "yes".to_string(),
        propagate: "yes".to_string(),
        observe: "yes".to_string(),
        discriminate: "yes".to_string(),
    };
    assert!(!qualifies_as_validating(
        RelationReason::DirectOwnerCall,
        &OracleStrength::Strong,
        Some("orthogonal"),
        &stages
    ));
}

#[test]
fn opaque_boundary_adds_widening_obligation() {
    let mut finding = finding_with_related(
        Vec::new(),
        ripr(
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Weak,
        ),
    );
    finding.stop_reasons.push(StopReason::ProcMacroOpaque);
    finding.static_limit_kind = Some(StaticLimitKind::RustMacroReachUnresolved);
    let plan = ok_plan(project_plan_from_finding(
        &finding,
        &plan_input(subject_identity("base", "head")),
    ));
    assert!(!plan.widened_obligations.is_empty());
    assert!(
        plan.widened_obligations
            .iter()
            .any(|row| row.widening_reason.contains("opaque_boundary"))
    );
}

#[test]
fn stale_input_is_not_empty_clean() {
    let finding = finding_with_related(
        Vec::new(),
        ripr(
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Weak,
        ),
    );
    let input = ChangeVerificationPlanInput {
        subject_identity: subject_identity("base", "head"),
        adapter_input: AdapterInput {
            subject_set: crate::analysis::witness::SubjectSet::DiffOnly,
            currentness: InputCurrentness::Stale,
        },
        commands: Vec::new(),
    };
    let plan = ok_plan(project_plan_from_finding(&finding, &input));
    assert!(plan.completeness.stale);
    assert!(plan.completeness.not_proven);
    assert!(!plan.limitations.is_empty());
}

#[test]
fn reordered_related_tests_preserve_digest() {
    let a = RelatedTest {
        name: "a".to_string(),
        file: PathBuf::from("tests/a.rs"),
        line: 1,
        oracle: None,
        oracle_kind: OracleKind::ExactValue,
        oracle_strength: OracleStrength::Strong,
        relation_confidence: Some(RelationReason::DirectOwnerCall.confidence()),
        relation_reason: Some(RelationReason::DirectOwnerCall),
        miss: None,
        miss_support: None,
        role: crate::domain::RelatedTestRole::Contributing,
    };
    let b = RelatedTest {
        name: "b".to_string(),
        file: PathBuf::from("tests/b.rs"),
        line: 2,
        oracle: None,
        oracle_kind: OracleKind::ExactValue,
        oracle_strength: OracleStrength::Medium,
        relation_confidence: Some(RelationReason::HelperOwnerCall.confidence()),
        relation_reason: Some(RelationReason::HelperOwnerCall),
        miss: None,
        miss_support: None,
        role: crate::domain::RelatedTestRole::Contributing,
    };
    let forward = ok_plan(project_plan_from_finding(
        &finding_with_related(
            vec![a.clone(), b.clone()],
            ripr(
                StageState::Yes,
                StageState::Yes,
                StageState::Yes,
                StageState::Yes,
                StageState::Yes,
            ),
        ),
        &plan_input(subject_identity("base", "head")),
    ));
    let reverse = ok_plan(project_plan_from_finding(
        &finding_with_related(
            vec![b, a],
            ripr(
                StageState::Yes,
                StageState::Yes,
                StageState::Yes,
                StageState::Yes,
                StageState::Yes,
            ),
        ),
        &plan_input(subject_identity("base", "head")),
    ));
    assert_eq!(forward.semantic_digest, reverse.semantic_digest);
}

#[test]
fn promoting_affinity_must_change_digest_negative_control() {
    let finding = finding_with_related(
        vec![RelatedTest {
            name: "tokenish".to_string(),
            file: PathBuf::from("tests/other.rs"),
            line: 3,
            oracle: None,
            oracle_kind: OracleKind::BroadError,
            oracle_strength: OracleStrength::Weak,
            relation_confidence: Some(RelationReason::WeakTokenSubstring.confidence()),
            relation_reason: Some(RelationReason::WeakTokenSubstring),
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }],
        ripr(
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Weak,
        ),
    );
    let mut plan = ok_plan(project_plan_from_finding(
        &finding,
        &plan_input(subject_identity("base", "head")),
    ));
    let before = plan.semantic_digest.clone();
    assert!(test_only_plan_with_promoted_affinity(&mut plan).is_ok());
    assert_ne!(before, plan.semantic_digest);
}

#[test]
fn removing_limitations_changes_digest_negative_control() {
    let finding = finding_with_related(
        Vec::new(),
        ripr(
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Weak,
        ),
    );
    let mut plan = ok_plan(project_plan_from_finding(
        &finding,
        &plan_input(subject_identity("base", "head")),
    ));
    let before = ok_digest(change_verification_plan_semantic_digest(&plan));
    test_only_plan_without_limitations(&mut plan);
    let after = ok_digest(change_verification_plan_semantic_digest(&plan));
    assert_ne!(before, after);
}

#[test]
fn diff_only_limitation_keeps_plan_limited() {
    let finding = finding_with_related(
        vec![RelatedTest {
            name: "checks_boundary".to_string(),
            file: PathBuf::from("tests/pricing.rs"),
            line: 10,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_confidence: Some(RelationReason::DirectOwnerCall.confidence()),
            relation_reason: Some(RelationReason::DirectOwnerCall),
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }],
        ripr(
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
        ),
    );
    let plan = ok_plan(project_plan_from_finding(
        &finding,
        &plan_input(subject_identity("base", "head")),
    ));
    assert!(plan.completeness.limited);
    assert!(plan.completeness.not_proven);
    assert!(
        plan.limitations
            .iter()
            .any(|item| item.contains("diff_only"))
    );
}

#[test]
fn production_builder_rejects_stronger_test_only_state() {
    let finding = finding_with_related(
        vec![RelatedTest {
            name: "tokenish".to_string(),
            file: PathBuf::from("tests/other.rs"),
            line: 3,
            oracle: None,
            oracle_kind: OracleKind::BroadError,
            oracle_strength: OracleStrength::Weak,
            relation_confidence: Some(RelationReason::WeakTokenSubstring.confidence()),
            relation_reason: Some(RelationReason::WeakTokenSubstring),
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }],
        ripr(
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Yes,
            StageState::Weak,
        ),
    );
    let production = ok_plan(project_plan_from_finding(
        &finding,
        &plan_input(subject_identity("base", "head")),
    ));
    let mut promoted = production.clone();
    assert!(test_only_plan_with_promoted_affinity(&mut promoted).is_ok());
    assert_ne!(
        production.impacted_tests[0].evidence_class,
        promoted.impacted_tests[0].evidence_class
    );
}
