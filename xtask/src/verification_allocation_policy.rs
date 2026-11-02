//! Verification-allocation policy vocabulary consumed by the Stage B adapter
//! (#4704 contract surface; #4707 consumes admitted generations only).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION: &str =
    "verification_allocation_policy.v1";

pub(crate) const VERIFICATION_ALLOCATION_DECISION_SCHEMA_VERSION: &str =
    "verification_allocation_decision.v1";

pub(crate) const VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY: &str = "Policy and readiness vocabulary only: defines conservative allocation \
 recommendations and prerequisites; it does not execute verification, reduce \
 required checks, or authorize operational skip.";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationAllocationActionV1 {
    FullRequired,
    TargetedPlusAudit,
    DeferToNightlyOrRelease,
    UnsupportedNoRecommendation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuleAdmissionStatusV1 {
    AdmittedForAdvisoryDesignReview,
    Rejected,
    InsufficientEvidence,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationAllocationLaterOwnerV1 {
    pub lane_id: String,
    pub command: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationAllocationRuleV1 {
    pub rule_id: String,
    pub admission: RuleAdmissionStatusV1,
    pub behavior_families: Vec<String>,
    pub targeted_command: String,
    pub full_audit_command: String,
    /// Deterministic audit selection uses this stable token with subject identity.
    pub audit_selection_token: String,
    pub later_owner: Option<VerificationAllocationLaterOwnerV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationAllocationPolicyV1 {
    pub schema_version: String,
    pub generation: u64,
    pub semantic_digest: String,
    pub rules: Vec<VerificationAllocationRuleV1>,
    pub limitations: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceStrengthV1 {
    Complete,
    Limited,
    Missing,
    Unsupported,
    Stale,
    Contradictory,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationAllocationSubjectEvidenceV1 {
    pub subject_id: String,
    pub behavior_family: String,
    pub evidence_strength: EvidenceStrengthV1,
    pub ripr_input_identity: String,
    pub route_state_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationAllocationDecisionV1 {
    pub schema_version: String,
    pub policy_generation: u64,
    pub policy_semantic_digest: String,
    pub subject_id: String,
    pub recommended_action: VerificationAllocationActionV1,
    pub rule_id: Option<String>,
    pub readiness: String,
    pub currentness: String,
    pub non_claims: Vec<String>,
}

pub(crate) fn policy_semantic_digest(policy: &VerificationAllocationPolicyV1) -> String {
    let mut clone = policy.clone();
    clone.semantic_digest.clear();
    clone.schema_version = VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION.to_string();
    let bytes = serde_json::to_vec(&clone).unwrap_or_default();
    sha256_hex(&bytes)
}

pub(crate) fn validate_policy(policy: &VerificationAllocationPolicyV1) -> Result<(), String> {
    if policy.schema_version != VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION {
        return Err(format!(
            "policy schema_version must be {VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION}"
        ));
    }
    if policy.rules.is_empty() {
        return Err("policy must contain at least one rule".to_string());
    }
    let expected = policy_semantic_digest(policy);
    if policy.semantic_digest != expected {
        return Err("policy semantic_digest does not match canonical digest".to_string());
    }
    for rule in &policy.rules {
        if rule.rule_id.trim().is_empty() {
            return Err("rule_id must be non-empty".to_string());
        }
        if rule.targeted_command.trim().is_empty() || rule.full_audit_command.trim().is_empty() {
            return Err(format!(
                "rule `{}` must declare targeted and full audit commands",
                rule.rule_id
            ));
        }
        if rule.audit_selection_token.trim().is_empty() {
            return Err(format!(
                "rule `{}` must declare audit_selection_token",
                rule.rule_id
            ));
        }
        if rule.admission == RuleAdmissionStatusV1::AdmittedForAdvisoryDesignReview
            && rule.behavior_families.is_empty()
        {
            return Err(format!(
                "admitted rule `{}` must bound behavior_families",
                rule.rule_id
            ));
        }
    }
    Ok(())
}

pub(crate) fn recommend_action(
    policy: &VerificationAllocationPolicyV1,
    subject: &VerificationAllocationSubjectEvidenceV1,
) -> VerificationAllocationDecisionV1 {
    let fallback =
        |action: VerificationAllocationActionV1, rule_id: Option<String>, reason: &str| {
            VerificationAllocationDecisionV1 {
                schema_version: VERIFICATION_ALLOCATION_DECISION_SCHEMA_VERSION.to_string(),
                policy_generation: policy.generation,
                policy_semantic_digest: policy.semantic_digest.clone(),
                subject_id: subject.subject_id.clone(),
                recommended_action: action,
                rule_id,
                readiness: reason.to_string(),
                currentness: "evaluated_at_request".to_string(),
                non_claims: vec![VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY.to_string()],
            }
        };

    if subject.evidence_strength != EvidenceStrengthV1::Complete {
        return fallback(
            VerificationAllocationActionV1::FullRequired,
            None,
            "evidence_not_complete",
        );
    }

    let Some(rule) = policy.rules.iter().find(|rule| {
        rule.admission == RuleAdmissionStatusV1::AdmittedForAdvisoryDesignReview
            && rule
                .behavior_families
                .iter()
                .any(|family| family == &subject.behavior_family)
    }) else {
        return fallback(
            VerificationAllocationActionV1::UnsupportedNoRecommendation,
            None,
            "no_admitted_rule_for_family",
        );
    };

    VerificationAllocationDecisionV1 {
        schema_version: VERIFICATION_ALLOCATION_DECISION_SCHEMA_VERSION.to_string(),
        policy_generation: policy.generation,
        policy_semantic_digest: policy.semantic_digest.clone(),
        subject_id: subject.subject_id.clone(),
        recommended_action: VerificationAllocationActionV1::TargetedPlusAudit,
        rule_id: Some(rule.rule_id.clone()),
        readiness: "ready_for_advisory_targeted".to_string(),
        currentness: "evaluated_at_request".to_string(),
        non_claims: vec![VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY.to_string()],
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod policy_semantics {
    use super::*;

    fn sample_policy() -> VerificationAllocationPolicyV1 {
        let mut policy = VerificationAllocationPolicyV1 {
            schema_version: VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION.to_string(),
            generation: 1,
            semantic_digest: String::new(),
            rules: vec![VerificationAllocationRuleV1 {
                rule_id: "rust-focused-clippy".to_string(),
                admission: RuleAdmissionStatusV1::AdmittedForAdvisoryDesignReview,
                behavior_families: vec!["rust_analysis".to_string()],
                targeted_command: "cargo clippy -p ripr".to_string(),
                full_audit_command: "cargo clippy --workspace --all-targets".to_string(),
                audit_selection_token: "audit-token-a".to_string(),
                later_owner: Some(VerificationAllocationLaterOwnerV1 {
                    lane_id: "nightly-full".to_string(),
                    command: "cargo xtask ci-full".to_string(),
                }),
            }],
            limitations: vec!["fixture_only".to_string()],
            non_claims: vec![VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY.to_string()],
        };
        policy.semantic_digest = policy_semantic_digest(&policy);
        policy
    }

    #[test]
    fn verification_allocation_policy_stale_digest_rejects() -> Result<(), String> {
        let mut policy = sample_policy();
        policy.semantic_digest = "deadbeef".to_string();
        if validate_policy(&policy).is_ok() {
            return Err("expected stale digest rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn verification_allocation_policy_limited_evidence_falls_back_to_full() -> Result<(), String> {
        let policy = sample_policy();
        let subject = VerificationAllocationSubjectEvidenceV1 {
            subject_id: "subject-1".to_string(),
            behavior_family: "rust_analysis".to_string(),
            evidence_strength: EvidenceStrengthV1::Limited,
            ripr_input_identity: "sha256:abc".to_string(),
            route_state_digest: "sha256:route".to_string(),
        };
        let decision = recommend_action(&policy, &subject);
        assert_eq!(
            decision.recommended_action,
            VerificationAllocationActionV1::FullRequired
        );
        Ok(())
    }
}
