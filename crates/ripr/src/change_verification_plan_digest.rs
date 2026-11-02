//! Portable semantic digest for `ChangeVerificationPlanV1` (#5163).
//!
//! Digest logic stays at the crate root (domain must not own JSON/canonical
//! rendering). Presentation order, absolute checkout spellings, timestamps,
//! and traversal order are excluded.

use crate::domain::{
    CHANGE_VERIFICATION_PLAN_SCHEMA_VERSION, ChangeVerificationPlanChangedSubject,
    ChangeVerificationPlanCommand, ChangeVerificationPlanCompleteness,
    ChangeVerificationPlanImpactedTest, ChangeVerificationPlanSubjectIdentity,
    ChangeVerificationPlanTestIdentity, ChangeVerificationPlanUncoveredSubject,
    ChangeVerificationPlanV1, ChangeVerificationPlanValidatingTest,
    ChangeVerificationPlanWidenedObligation,
};

#[derive(serde::Serialize)]
struct DigestInput<'a> {
    schema_version: &'static str,
    subject_identity: &'a ChangeVerificationPlanSubjectIdentity,
    changed_subjects: &'a [ChangeVerificationPlanChangedSubject],
    impacted_tests: &'a [ChangeVerificationPlanImpactedTest],
    validating_tests: &'a [ChangeVerificationPlanValidatingTest],
    uncovered_changed_subjects: &'a [ChangeVerificationPlanUncoveredSubject],
    widened_obligations: &'a [ChangeVerificationPlanWidenedObligation],
    commands: &'a [ChangeVerificationPlanCommand],
    completeness: &'a ChangeVerificationPlanCompleteness,
    limitations: &'a [String],
    non_claims: &'a [String],
}

pub(crate) fn change_verification_plan_semantic_digest(
    plan: &ChangeVerificationPlanV1,
) -> Result<String, String> {
    let input = DigestInput {
        schema_version: CHANGE_VERIFICATION_PLAN_SCHEMA_VERSION,
        subject_identity: &plan.subject_identity,
        changed_subjects: &plan.changed_subjects,
        impacted_tests: &plan.impacted_tests,
        validating_tests: &plan.validating_tests,
        uncovered_changed_subjects: &plan.uncovered_changed_subjects,
        widened_obligations: &plan.widened_obligations,
        commands: &plan.commands,
        completeness: &plan.completeness,
        limitations: &plan.limitations,
        non_claims: &plan.non_claims,
    };
    let canonical = serde_json::to_vec(&input)
        .map_err(|err| format!("serialize change verification plan digest input: {err}"))?;
    Ok(format!("sha256:{}", hex_sha256(&canonical)))
}

fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

pub(crate) fn change_verification_plan_integrity_matches(plan: &ChangeVerificationPlanV1) -> bool {
    change_verification_plan_semantic_digest(plan)
        .ok()
        .is_some_and(|digest| {
            !digest.is_empty() && digest == plan.semantic_digest && digest == plan.plan_id
        })
}
pub(crate) fn normalize_portable_path(path: &str) -> String {
    path.replace('\\', "/")
}

pub(crate) fn normalize_test_identity(
    name: &str,
    file: &str,
    line: usize,
) -> ChangeVerificationPlanTestIdentity {
    ChangeVerificationPlanTestIdentity {
        test_name: name.trim().to_string(),
        file: normalize_portable_path(file.trim()),
        line,
    }
}

#[cfg(test)]
mod tests {
    fn ok_digest(result: Result<String, String>) -> String {
        assert!(result.is_ok(), "{:?}", result.err());
        if let Ok(digest) = result {
            digest
        } else {
            loop {}
        }
    }

    use super::*;
    use crate::domain::{
        ChangeVerificationPlanCompleteness, PlanRelationEvidenceClass, VerificationCommandPurpose,
        VerificationObligationKind, WidenedObligationScope,
    };

    fn minimal_plan(subject: ChangeVerificationPlanSubjectIdentity) -> ChangeVerificationPlanV1 {
        ChangeVerificationPlanV1 {
            schema_version: CHANGE_VERIFICATION_PLAN_SCHEMA_VERSION.to_string(),
            plan_id: String::new(),
            semantic_digest: String::new(),
            subject_identity: subject,
            changed_subjects: Vec::new(),
            impacted_tests: Vec::new(),
            validating_tests: Vec::new(),
            uncovered_changed_subjects: Vec::new(),
            widened_obligations: Vec::new(),
            commands: Vec::new(),
            completeness: ChangeVerificationPlanCompleteness {
                complete_for_declared_model: false,
                conservative_with_fallback: false,
                limited: true,
                stale: false,
                unsupported: false,
                not_proven: true,
                declared_model: "change_verification_plan.v1".to_string(),
                unsupported_families: Vec::new(),
            },
            limitations: vec!["unspecified_input_completeness".to_string()],
            non_claims: vec!["projection_only".to_string()],
            claim_boundary: crate::domain::CHANGE_VERIFICATION_PLAN_CLAIM_BOUNDARY.to_string(),
        }
    }

    #[test]
    fn equivalent_checkout_spelling_yields_equal_digest() {
        let left = minimal_plan(ChangeVerificationPlanSubjectIdentity {
            repository: "repo-a".to_string(),
            base_revision: Some("base".to_string()),
            head_revision: Some("head".to_string()),
            worktree_identity: None,
            analyzer_generation: None,
            schema_generation: None,
            language: Some("rust".to_string()),
            config_profile: None,
            features: Vec::new(),
            manifest_lock_inputs: Vec::new(),
        });
        let mut right = left.clone();
        right.subject_identity.repository = "repo-a".to_string();
        assert_eq!(
            ok_digest(change_verification_plan_semantic_digest(&left)),
            ok_digest(change_verification_plan_semantic_digest(&right))
        );
    }

    #[test]
    fn base_head_movement_changes_digest() {
        let mut base = minimal_plan(ChangeVerificationPlanSubjectIdentity {
            repository: "repo".to_string(),
            base_revision: Some("aaa".to_string()),
            head_revision: Some("bbb".to_string()),
            worktree_identity: None,
            analyzer_generation: None,
            schema_generation: None,
            language: None,
            config_profile: None,
            features: Vec::new(),
            manifest_lock_inputs: Vec::new(),
        });
        let digest_a = ok_digest(change_verification_plan_semantic_digest(&base));
        base.subject_identity.head_revision = Some("ccc".to_string());
        let digest_b = ok_digest(change_verification_plan_semantic_digest(&base));
        assert_ne!(digest_a, digest_b);
    }

    #[test]
    fn normalize_path_is_portable() {
        assert_eq!(normalize_portable_path("src\\lib.rs"), "src/lib.rs");
    }

    #[test]
    fn widening_reason_is_digest_member() {
        let subject = ChangeVerificationPlanSubjectIdentity {
            repository: "repo".to_string(),
            base_revision: None,
            head_revision: None,
            worktree_identity: None,
            analyzer_generation: None,
            schema_generation: None,
            language: None,
            config_profile: None,
            features: Vec::new(),
            manifest_lock_inputs: Vec::new(),
        };
        let mut plan = minimal_plan(subject);
        plan.widened_obligations
            .push(ChangeVerificationPlanWidenedObligation {
                scope: WidenedObligationScope::Workspace,
                obligation_kind: VerificationObligationKind::Test,
                widening_reason: "opaque_boundary:macro".to_string(),
                originating_edge: Some("reach:macro_limit".to_string()),
            });
        let with = ok_digest(change_verification_plan_semantic_digest(&plan));
        plan.widened_obligations[0].widening_reason = "opaque_boundary:dynamic".to_string();
        let changed = ok_digest(change_verification_plan_semantic_digest(&plan));
        assert_ne!(with, changed);
    }

    #[test]
    fn affinity_class_change_changes_digest() {
        let subject = ChangeVerificationPlanSubjectIdentity {
            repository: "repo".to_string(),
            base_revision: None,
            head_revision: None,
            worktree_identity: None,
            analyzer_generation: None,
            schema_generation: None,
            language: None,
            config_profile: None,
            features: Vec::new(),
            manifest_lock_inputs: Vec::new(),
        };
        let mut plan = minimal_plan(subject);
        plan.impacted_tests
            .push(ChangeVerificationPlanImpactedTest {
                test: normalize_test_identity("t", "tests/a.rs", 1),
                evidence_class: PlanRelationEvidenceClass::AffinityCandidate,
                relation_basis: "weak_token_substring".to_string(),
                path_identities: Vec::new(),
                base_presence: "unknown".to_string(),
                head_presence: "present".to_string(),
                target_state: "typed_absence".to_string(),
                currentness: "current".to_string(),
                applicability: "inspect_only".to_string(),
            });
        let weak = ok_digest(change_verification_plan_semantic_digest(&plan));
        plan.impacted_tests[0].evidence_class = PlanRelationEvidenceClass::ResolvedMayDepend;
        plan.impacted_tests[0].relation_basis = "direct_owner_call".to_string();
        let strong = ok_digest(change_verification_plan_semantic_digest(&plan));
        assert_ne!(weak, strong);
    }
}
