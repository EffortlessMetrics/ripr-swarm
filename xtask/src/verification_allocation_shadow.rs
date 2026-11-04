//! Full-verification shadow runner (#4705, VA02).
//!
//! Commits a #4704 allocation recommendation for each governed manifest subject
//! before running the unchanged full verification command, then retains both in one
//! exact-subject receipt. Does not admit rules, reduce work, or emit savings claims.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "policy:4705: shadow runner surface lands before reporting leaf wiring"
    )
)]

use serde::{Deserialize, Serialize};

use crate::verification_allocation_policy::{
    VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY, VerificationAllocationActionV1,
    VerificationAllocationDecisionV1, VerificationAllocationPolicyV1,
    VerificationAllocationSubjectEvidenceV1, recommend_action, resolve_admitted_rule,
    validate_policy,
};

pub(crate) const VERIFICATION_ALLOCATION_SHADOW_SCHEMA_VERSION: &str =
    "verification_allocation_shadow.v1";

pub(crate) const VERIFICATION_ALLOCATION_SHADOW_CLAIM_BOUNDARY: &str = "Shadow execution only: \
 recommendation is committed before the unchanged full verification command runs; this slice \
 does not judge recommendations, admit policy rules, or claim verification savings.";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShadowTerminalStateV1 {
    Clean,
    ProductFailure,
    TestFailure,
    BuildFailure,
    Timeout,
    Cancelled,
    InfrastructureFailure,
    Inconclusive,
    Equivalent,
    SetupFailure,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShadowCleanupDispositionV1 {
    Clean,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShadowSubjectManifestEntryV1 {
    pub subject_id: String,
    pub full_verification_command: String,
    pub evidence: VerificationAllocationSubjectEvidenceV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationAllocationShadowManifestV1 {
    pub manifest_id: String,
    pub repository_identity: String,
    pub base_revision: String,
    pub head_revision: String,
    pub tree_identity: String,
    pub portable_root_identity: String,
    pub execution_root_identity: String,
    pub policy: VerificationAllocationPolicyV1,
    pub subjects: Vec<ShadowSubjectManifestEntryV1>,
    pub tool_identity: String,
    pub environment_class: String,
    pub working_directory: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommittedRecommendationV1 {
    pub sequence: u64,
    pub decision: VerificationAllocationDecisionV1,
    pub decision_digest: String,
    pub commitment_digest: String,
    pub committed_before_execution: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FullVerificationExecutionV1 {
    pub command_line: String,
    pub working_directory: String,
    pub environment_class: String,
    pub tool_identity: String,
    pub exit_code: Option<i32>,
    pub terminal_state: ShadowTerminalStateV1,
    pub detail: String,
    pub elapsed_ms: u64,
    pub retry_attempt: u32,
    pub first_attempt_retained: bool,
    pub first_attempt_exit_code: Option<i32>,
    pub first_attempt_detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShadowSubjectReceiptV1 {
    pub subject_id: String,
    pub recommendation: CommittedRecommendationV1,
    pub full_verification: FullVerificationExecutionV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShadowCleanupRecordV1 {
    pub disposition: ShadowCleanupDispositionV1,
    pub detail: String,
    pub owned_residue: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationAllocationShadowReceiptV1 {
    pub schema_version: String,
    pub manifest_id: String,
    pub manifest_digest: String,
    pub repository_identity: String,
    pub base_revision: String,
    pub head_revision: String,
    pub tree_identity: String,
    pub portable_root_identity: String,
    pub execution_root_identity: String,
    pub policy_generation: u64,
    pub policy_semantic_digest: String,
    pub subjects: Vec<ShadowSubjectReceiptV1>,
    pub cleanup: ShadowCleanupRecordV1,
    pub claim_boundary: String,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ShadowExecutionFailureKindV1 {
    Timeout,
    Cancelled,
    Infrastructure,
}

#[derive(Clone, Debug)]
pub(crate) struct ShadowCommandObservationV1 {
    pub exit_code: i32,
    pub detail: String,
    pub elapsed_ms: u64,
    pub failure_kind: Option<ShadowExecutionFailureKindV1>,
}

pub(crate) trait RecommendationCommitSink {
    fn commit_recommendation(
        &mut self,
        sequence: u64,
        decision: &VerificationAllocationDecisionV1,
        decision_digest: &str,
    ) -> Result<String, String>;
}

pub(crate) trait ShadowCleanupObserver {
    fn observe_cleanup(&self) -> ShadowCleanupRecordV1;
}

#[derive(Default)]
pub(crate) struct InMemoryRecommendationJournal {
    pub committed_digests: Vec<String>,
}

impl RecommendationCommitSink for InMemoryRecommendationJournal {
    fn commit_recommendation(
        &mut self,
        _sequence: u64,
        decision: &VerificationAllocationDecisionV1,
        decision_digest: &str,
    ) -> Result<String, String> {
        let material = format!("{decision_digest}|{}", decision.subject_id);
        let commitment_digest =
            crate::verification_allocation_policy::sha256_hex(material.as_bytes());
        self.committed_digests.push(commitment_digest.clone());
        Ok(commitment_digest)
    }
}

struct OptionsCleanupObserver<'a> {
    options: &'a VerificationAllocationShadowRunOptionsV1,
}

impl ShadowCleanupObserver for OptionsCleanupObserver<'_> {
    fn observe_cleanup(&self) -> ShadowCleanupRecordV1 {
        if self.options.cleanup_succeeds {
            ShadowCleanupRecordV1 {
                disposition: ShadowCleanupDispositionV1::Clean,
                detail: "cleanup_ok".to_string(),
                owned_residue: Vec::new(),
            }
        } else {
            ShadowCleanupRecordV1 {
                disposition: ShadowCleanupDispositionV1::Failed,
                detail: "cleanup_failed".to_string(),
                owned_residue: self.options.cleanup_residue.clone(),
            }
        }
    }
}

pub(crate) trait ShadowCommandAdapter {
    fn run(&self, command: &str, cwd: &str, tool_identity: &str) -> ShadowCommandObservationV1;
}

#[derive(Clone, Debug)]
pub(crate) struct VerificationAllocationShadowRunOptionsV1 {
    pub expected_tool_identity: String,
    pub cleanup_succeeds: bool,
    pub cleanup_residue: Vec<String>,
}

impl Default for VerificationAllocationShadowRunOptionsV1 {
    fn default() -> Self {
        Self {
            expected_tool_identity: String::new(),
            cleanup_succeeds: true,
            cleanup_residue: Vec::new(),
        }
    }
}

pub(crate) fn manifest_semantic_digest(
    manifest: &VerificationAllocationShadowManifestV1,
) -> String {
    let mut clone = manifest.clone();
    clone.policy.semantic_digest = String::new();
    let bytes = serde_json::to_vec(&clone).unwrap_or_default();
    crate::verification_allocation_policy::sha256_hex(&bytes)
}

pub(crate) fn decision_digest(decision: &VerificationAllocationDecisionV1) -> String {
    let bytes = serde_json::to_vec(decision).unwrap_or_default();
    crate::verification_allocation_policy::sha256_hex(&bytes)
}

pub(crate) fn classify_terminal_state(
    observation: &ShadowCommandObservationV1,
    exit_code: i32,
) -> ShadowTerminalStateV1 {
    if let Some(kind) = observation.failure_kind {
        return match kind {
            ShadowExecutionFailureKindV1::Timeout => ShadowTerminalStateV1::Timeout,
            ShadowExecutionFailureKindV1::Cancelled => ShadowTerminalStateV1::Cancelled,
            ShadowExecutionFailureKindV1::Infrastructure => {
                ShadowTerminalStateV1::InfrastructureFailure
            }
        };
    }
    if exit_code == 0 {
        if observation.detail.contains("equivalent") {
            return ShadowTerminalStateV1::Equivalent;
        }
        return ShadowTerminalStateV1::Clean;
    }
    if observation.detail.contains("build_failure") {
        ShadowTerminalStateV1::BuildFailure
    } else if observation.detail.contains("test_failure") {
        ShadowTerminalStateV1::TestFailure
    } else if observation.detail.contains("product_failure") {
        ShadowTerminalStateV1::ProductFailure
    } else if observation.detail.contains("inconclusive") {
        ShadowTerminalStateV1::Inconclusive
    } else if observation.detail.contains("setup_failure") {
        ShadowTerminalStateV1::SetupFailure
    } else {
        ShadowTerminalStateV1::ProductFailure
    }
}

pub(crate) fn authoritative_full_verification_command(
    policy: &VerificationAllocationPolicyV1,
    entry: &ShadowSubjectManifestEntryV1,
) -> Result<String, String> {
    let decision = recommend_action(policy, &entry.evidence);
    if let Some(rule) = resolve_admitted_rule(policy, &decision, &entry.evidence)
        && decision.recommended_action == VerificationAllocationActionV1::TargetedPlusAudit
    {
        return Ok(rule.full_audit_command.clone());
    }
    for rule in &policy.rules {
        if entry.full_verification_command.trim() == rule.targeted_command.trim()
            && entry.full_verification_command.trim() != rule.full_audit_command.trim()
        {
            return Err(format!(
                "subject `{}` declares targeted command as full verification",
                entry.subject_id
            ));
        }
    }
    Ok(entry.full_verification_command.clone())
}

fn validate_manifest(manifest: &VerificationAllocationShadowManifestV1) -> Result<(), String> {
    if manifest.manifest_id.trim().is_empty() {
        return Err("manifest_id must be non-empty".to_string());
    }
    if manifest.subjects.is_empty() {
        return Err("manifest must retain at least one governed subject".to_string());
    }
    validate_policy(&manifest.policy)?;
    let mut seen = std::collections::BTreeSet::new();
    for entry in &manifest.subjects {
        if entry.subject_id.trim().is_empty() {
            return Err("subject_id must be non-empty".to_string());
        }
        if entry.full_verification_command.trim().is_empty() {
            return Err(format!(
                "subject `{}` must declare a full verification command",
                entry.subject_id
            ));
        }
        if entry.evidence.subject_id != entry.subject_id {
            return Err(format!(
                "subject `{}` evidence identity does not match manifest row",
                entry.subject_id
            ));
        }
        if !seen.insert(entry.subject_id.clone()) {
            return Err(format!("duplicate manifest subject `{}`", entry.subject_id));
        }
    }
    if manifest.tool_identity.trim().is_empty() {
        return Err("tool_identity must be non-empty".to_string());
    }
    Ok(())
}

pub(crate) fn reject_recommendation_after_outcome(
    outcome_observed: bool,
    recommendation_committed: bool,
) -> Result<(), String> {
    if outcome_observed && !recommendation_committed {
        return Err(
            "actual outcome visible before recommendation commitment is forbidden".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn reject_subset_command(
    declared_full_command: &str,
    attempted_command: &str,
) -> Result<(), String> {
    if declared_full_command.trim() != attempted_command.trim() {
        return Err(
            "full verification command cannot be replaced by a targeted subset".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn validate_receipt_denominator(
    manifest: &VerificationAllocationShadowManifestV1,
    receipt: &VerificationAllocationShadowReceiptV1,
) -> Result<(), String> {
    if receipt.subjects.len() != manifest.subjects.len() {
        return Err("receipt omitted one or more manifest subjects".to_string());
    }
    let manifest_ids: Vec<_> = manifest
        .subjects
        .iter()
        .map(|row| row.subject_id.as_str())
        .collect();
    let receipt_ids: Vec<_> = receipt
        .subjects
        .iter()
        .map(|row| row.subject_id.as_str())
        .collect();
    if manifest_ids != receipt_ids {
        return Err("receipt subject order or identity does not match manifest".to_string());
    }
    Ok(())
}

pub(crate) fn validate_recommendation_precedes_execution(
    row: &ShadowSubjectReceiptV1,
) -> Result<(), String> {
    if !row.recommendation.committed_before_execution {
        return Err(format!(
            "subject `{}` recommendation was not committed before execution",
            row.subject_id
        ));
    }
    if row.recommendation.commitment_digest.is_empty() {
        return Err(format!(
            "subject `{}` missing durable recommendation commitment",
            row.subject_id
        ));
    }
    Ok(())
}

pub(crate) fn execute_verification_allocation_shadow_with_parts<
    A: ShadowCommandAdapter,
    C: ShadowCleanupObserver,
>(
    manifest: &VerificationAllocationShadowManifestV1,
    adapter: &A,
    commit_sink: &mut InMemoryRecommendationJournal,
    cleanup: &C,
    options: &VerificationAllocationShadowRunOptionsV1,
) -> Result<VerificationAllocationShadowReceiptV1, String> {
    validate_manifest(manifest)?;
    if !options.expected_tool_identity.is_empty()
        && options.expected_tool_identity != manifest.tool_identity
    {
        return Err("tool_identity mismatch rejects shadow execution".to_string());
    }

    let manifest_digest = manifest_semantic_digest(manifest);
    let mut subject_rows = Vec::with_capacity(manifest.subjects.len());
    let mut sequence = 0_u64;

    for entry in &manifest.subjects {
        sequence += 1;
        let authoritative = authoritative_full_verification_command(&manifest.policy, entry)?;
        reject_subset_command(&authoritative, &entry.full_verification_command)?;

        let decision = recommend_action(&manifest.policy, &entry.evidence);
        let digest = decision_digest(&decision);
        let commitment_digest = commit_sink.commit_recommendation(sequence, &decision, &digest)?;
        let recommendation = CommittedRecommendationV1 {
            sequence,
            decision,
            decision_digest: digest,
            commitment_digest,
            committed_before_execution: true,
        };

        let observation = adapter.run(
            &entry.full_verification_command,
            &manifest.working_directory,
            &manifest.tool_identity,
        );
        let terminal_state = classify_terminal_state(&observation, observation.exit_code);

        subject_rows.push(ShadowSubjectReceiptV1 {
            subject_id: entry.subject_id.clone(),
            recommendation,
            full_verification: FullVerificationExecutionV1 {
                command_line: entry.full_verification_command.clone(),
                working_directory: manifest.working_directory.clone(),
                environment_class: manifest.environment_class.clone(),
                tool_identity: manifest.tool_identity.clone(),
                exit_code: Some(observation.exit_code),
                terminal_state,
                detail: observation.detail.clone(),
                elapsed_ms: observation.elapsed_ms,
                retry_attempt: 0,
                first_attempt_retained: true,
                first_attempt_exit_code: None,
                first_attempt_detail: String::new(),
            },
        });
    }

    let receipt = VerificationAllocationShadowReceiptV1 {
        schema_version: VERIFICATION_ALLOCATION_SHADOW_SCHEMA_VERSION.to_string(),
        manifest_id: manifest.manifest_id.clone(),
        manifest_digest,
        repository_identity: manifest.repository_identity.clone(),
        base_revision: manifest.base_revision.clone(),
        head_revision: manifest.head_revision.clone(),
        tree_identity: manifest.tree_identity.clone(),
        portable_root_identity: manifest.portable_root_identity.clone(),
        execution_root_identity: manifest.execution_root_identity.clone(),
        policy_generation: manifest.policy.generation,
        policy_semantic_digest: manifest.policy.semantic_digest.clone(),
        subjects: subject_rows,
        cleanup: cleanup.observe_cleanup(),
        claim_boundary: VERIFICATION_ALLOCATION_SHADOW_CLAIM_BOUNDARY.to_string(),
        non_claims: vec![
            VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY.to_string(),
            "no_verification_savings_claim".to_string(),
            "no_policy_admission".to_string(),
        ],
    };

    validate_receipt_denominator(manifest, &receipt)?;
    for row in &receipt.subjects {
        validate_recommendation_precedes_execution(row)?;
    }
    if commit_sink.committed_digests.len() != manifest.subjects.len() {
        return Err("recommendation journal missing committed subjects".to_string());
    }
    Ok(receipt)
}

pub(crate) fn execute_verification_allocation_shadow<A: ShadowCommandAdapter>(
    manifest: &VerificationAllocationShadowManifestV1,
    adapter: &A,
    options: &VerificationAllocationShadowRunOptionsV1,
) -> Result<VerificationAllocationShadowReceiptV1, String> {
    let mut commit_sink = InMemoryRecommendationJournal::default();
    let cleanup = OptionsCleanupObserver { options };
    execute_verification_allocation_shadow_with_parts(
        manifest,
        adapter,
        &mut commit_sink,
        &cleanup,
        options,
    )
}

pub(crate) fn execute_verification_allocation_shadow_with_retry<A: ShadowCommandAdapter>(
    manifest: &VerificationAllocationShadowManifestV1,
    adapter: &A,
    options: &VerificationAllocationShadowRunOptionsV1,
    retry_same_command: bool,
) -> Result<VerificationAllocationShadowReceiptV1, String> {
    let mut commit_sink = InMemoryRecommendationJournal::default();
    let cleanup = OptionsCleanupObserver { options };
    let mut receipt = execute_verification_allocation_shadow_with_parts(
        manifest,
        adapter,
        &mut commit_sink,
        &cleanup,
        options,
    )?;
    if receipt.subjects.is_empty() {
        return Err("retry requires at least one subject".to_string());
    }
    let entry = manifest
        .subjects
        .first()
        .ok_or_else(|| "retry requires at least one subject".to_string())?;
    let first = receipt.subjects[0].full_verification.clone();
    if retry_same_command && first.exit_code != Some(0) {
        let retry_observation = adapter.run(
            &entry.full_verification_command,
            &manifest.working_directory,
            &manifest.tool_identity,
        );
        receipt.subjects[0].full_verification.first_attempt_retained = true;
        receipt.subjects[0]
            .full_verification
            .first_attempt_exit_code = first.exit_code;
        receipt.subjects[0].full_verification.first_attempt_detail = first.detail.clone();
        receipt.subjects[0].full_verification.retry_attempt = 1;
        receipt.subjects[0].full_verification.exit_code = Some(retry_observation.exit_code);
        receipt.subjects[0].full_verification.detail = retry_observation.detail.clone();
        receipt.subjects[0].full_verification.elapsed_ms = first
            .elapsed_ms
            .saturating_add(retry_observation.elapsed_ms);
        receipt.subjects[0].full_verification.terminal_state =
            classify_terminal_state(&retry_observation, retry_observation.exit_code);
    }
    for row in &receipt.subjects {
        validate_recommendation_precedes_execution(row)?;
    }
    Ok(receipt)
}

fn observation(exit_code: i32, detail: &str) -> ShadowCommandObservationV1 {
    ShadowCommandObservationV1 {
        exit_code,
        detail: detail.to_string(),
        elapsed_ms: 1,
        failure_kind: None,
    }
}

#[cfg(test)]
mod shadow_semantics {
    use super::*;
    use crate::verification_allocation_policy::{
        EvidenceStrengthV1, RuleAdmissionStatusV1, VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION,
        VerificationAllocationRuleV1, policy_semantic_digest,
    };

    struct ScriptShadowAdapter {
        outcomes: std::collections::BTreeMap<String, ShadowCommandObservationV1>,
    }

    struct SequentialShadowAdapter {
        outcomes: Vec<ShadowCommandObservationV1>,
        index: std::cell::Cell<usize>,
    }

    impl SequentialShadowAdapter {
        fn new(outcomes: &[ShadowCommandObservationV1]) -> Self {
            Self {
                outcomes: outcomes.to_vec(),
                index: std::cell::Cell::new(0),
            }
        }
    }

    impl ShadowCommandAdapter for SequentialShadowAdapter {
        fn run(
            &self,
            _command: &str,
            _cwd: &str,
            _tool_identity: &str,
        ) -> ShadowCommandObservationV1 {
            let idx = self.index.get();
            self.index.set(idx + 1);
            self.outcomes
                .get(idx)
                .cloned()
                .unwrap_or(ShadowCommandObservationV1 {
                    exit_code: 127,
                    detail: "missing_script".to_string(),
                    elapsed_ms: 0,
                    failure_kind: None,
                })
        }
    }

    impl ScriptShadowAdapter {
        fn new(outcomes: &[(&str, i32, &str)]) -> Self {
            let mut map = std::collections::BTreeMap::new();
            for (command, code, detail) in outcomes {
                map.insert(
                    (*command).to_string(),
                    ShadowCommandObservationV1 {
                        exit_code: *code,
                        detail: (*detail).to_string(),
                        elapsed_ms: 1,
                        failure_kind: None,
                    },
                );
            }
            Self { outcomes: map }
        }
    }

    impl ShadowCommandAdapter for ScriptShadowAdapter {
        fn run(
            &self,
            command: &str,
            _cwd: &str,
            _tool_identity: &str,
        ) -> ShadowCommandObservationV1 {
            self.outcomes
                .get(command)
                .cloned()
                .unwrap_or(ShadowCommandObservationV1 {
                    exit_code: 127,
                    detail: "missing_script".to_string(),
                    elapsed_ms: 0,
                    failure_kind: None,
                })
        }
    }

    fn full_audit_command() -> String {
        sample_rule().full_audit_command
    }

    fn limited_evidence(subject_id: &str) -> VerificationAllocationSubjectEvidenceV1 {
        let mut evidence = sample_evidence(subject_id);
        evidence.evidence_strength = EvidenceStrengthV1::Limited;
        evidence
    }

    fn entry_with_full_audit(subject_id: &str) -> ShadowSubjectManifestEntryV1 {
        ShadowSubjectManifestEntryV1 {
            subject_id: subject_id.to_string(),
            full_verification_command: full_audit_command(),
            evidence: sample_evidence(subject_id),
        }
    }

    fn entry_with_command(subject_id: &str, command: &str) -> ShadowSubjectManifestEntryV1 {
        ShadowSubjectManifestEntryV1 {
            subject_id: subject_id.to_string(),
            full_verification_command: command.to_string(),
            evidence: limited_evidence(subject_id),
        }
    }
    fn sample_rule() -> VerificationAllocationRuleV1 {
        VerificationAllocationRuleV1 {
            rule_id: "rust-focused-clippy".to_string(),
            admission: RuleAdmissionStatusV1::AdmittedForAdvisoryDesignReview,
            behavior_families: vec!["rust_analysis".to_string()],
            targeted_command: "cargo clippy -p ripr".to_string(),
            full_audit_command: "cargo clippy --workspace --all-targets".to_string(),
            audit_selection_token: "audit-token-a".to_string(),
            later_owner: None,
        }
    }

    fn sample_policy() -> VerificationAllocationPolicyV1 {
        let mut policy = VerificationAllocationPolicyV1 {
            schema_version: VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION.to_string(),
            generation: 1,
            semantic_digest: String::new(),
            rules: vec![sample_rule()],
            limitations: vec!["fixture_only".to_string()],
            non_claims: vec![VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY.to_string()],
        };
        policy.semantic_digest = policy_semantic_digest(&policy);
        policy
    }

    fn sample_evidence(subject_id: &str) -> VerificationAllocationSubjectEvidenceV1 {
        VerificationAllocationSubjectEvidenceV1 {
            subject_id: subject_id.to_string(),
            behavior_family: "rust_analysis".to_string(),
            evidence_strength: EvidenceStrengthV1::Complete,
            ripr_input_identity: format!("sha256:{subject_id}"),
            route_state_digest: "sha256:route".to_string(),
        }
    }

    fn sample_manifest(
        subjects: Vec<ShadowSubjectManifestEntryV1>,
    ) -> VerificationAllocationShadowManifestV1 {
        VerificationAllocationShadowManifestV1 {
            manifest_id: "fixture-manifest".to_string(),
            repository_identity: "github:EffortlessMetrics/ripr-swarm".to_string(),
            base_revision: "base-deadbeef".to_string(),
            head_revision: "head-cafebabe".to_string(),
            tree_identity: "tree-001".to_string(),
            portable_root_identity: "portable:/workspace".to_string(),
            execution_root_identity: "exec:/tmp/shadow-run".to_string(),
            policy: sample_policy(),
            subjects,
            tool_identity: "xtask-test@sha256:tool".to_string(),
            environment_class: "fixture_local".to_string(),
            working_directory: "/workspace".to_string(),
        }
    }

    #[test]
    fn verification_allocation_shadow_commits_recommendation_before_execution() -> Result<(), String>
    {
        let manifest = sample_manifest(vec![entry_with_full_audit("subject-a")]);
        let command = full_audit_command();
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        let receipt = execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[(&command, 0, "clean")]),
            &options,
        )?;
        assert_eq!(receipt.subjects.len(), 1);
        assert!(
            receipt.subjects[0]
                .recommendation
                .committed_before_execution
        );
        assert_eq!(
            receipt.subjects[0].full_verification.terminal_state,
            ShadowTerminalStateV1::Clean
        );
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_rejects_outcome_before_recommendation() -> Result<(), String>
    {
        if reject_recommendation_after_outcome(true, false).is_ok() {
            return Err("expected outcome-before-recommendation rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_rejects_subset_command_substitution() -> Result<(), String> {
        if reject_subset_command(
            "cargo clippy --workspace --all-targets",
            "cargo clippy -p ripr",
        )
        .is_ok()
        {
            return Err("expected subset substitution rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_rejects_missing_subject_in_receipt() -> Result<(), String> {
        let manifest = sample_manifest(vec![
            entry_with_command("subject-a", "cmd-a"),
            entry_with_command("subject-b", "cmd-b"),
        ]);
        let mut receipt = VerificationAllocationShadowReceiptV1 {
            schema_version: VERIFICATION_ALLOCATION_SHADOW_SCHEMA_VERSION.to_string(),
            manifest_id: manifest.manifest_id.clone(),
            manifest_digest: manifest_semantic_digest(&manifest),
            repository_identity: manifest.repository_identity.clone(),
            base_revision: manifest.base_revision.clone(),
            head_revision: manifest.head_revision.clone(),
            tree_identity: manifest.tree_identity.clone(),
            portable_root_identity: manifest.portable_root_identity.clone(),
            execution_root_identity: manifest.execution_root_identity.clone(),
            policy_generation: manifest.policy.generation,
            policy_semantic_digest: manifest.policy.semantic_digest.clone(),
            subjects: vec![ShadowSubjectReceiptV1 {
                subject_id: "subject-a".to_string(),
                recommendation: CommittedRecommendationV1 {
                    sequence: 1,
                    decision: recommend_action(&manifest.policy, &limited_evidence("subject-a")),
                    decision_digest: String::new(),
                    commitment_digest: "committed".to_string(),
                    committed_before_execution: true,
                },
                full_verification: FullVerificationExecutionV1 {
                    command_line: "cmd-a".to_string(),
                    working_directory: manifest.working_directory.clone(),
                    environment_class: manifest.environment_class.clone(),
                    tool_identity: manifest.tool_identity.clone(),
                    exit_code: Some(0),
                    terminal_state: ShadowTerminalStateV1::Clean,
                    detail: "clean".to_string(),
                    elapsed_ms: 0,
                    retry_attempt: 0,
                    first_attempt_retained: true,
                    first_attempt_exit_code: None,
                    first_attempt_detail: String::new(),
                },
            }],
            cleanup: ShadowCleanupRecordV1 {
                disposition: ShadowCleanupDispositionV1::Clean,
                detail: "ok".to_string(),
                owned_residue: Vec::new(),
            },
            claim_boundary: VERIFICATION_ALLOCATION_SHADOW_CLAIM_BOUNDARY.to_string(),
            non_claims: Vec::new(),
        };
        if validate_receipt_denominator(&manifest, &receipt).is_ok() {
            return Err("expected missing subject rejection".to_string());
        }
        receipt.subjects[0].recommendation.decision_digest =
            decision_digest(&receipt.subjects[0].recommendation.decision);
        let _ = receipt;
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_terminal_states_remain_distinct() -> Result<(), String> {
        let cases = [
            (
                ShadowExecutionFailureKindV1::Timeout,
                ShadowTerminalStateV1::Timeout,
            ),
            (
                ShadowExecutionFailureKindV1::Cancelled,
                ShadowTerminalStateV1::Cancelled,
            ),
            (
                ShadowExecutionFailureKindV1::Infrastructure,
                ShadowTerminalStateV1::InfrastructureFailure,
            ),
        ];
        for (kind, expected) in cases {
            let observation = ShadowCommandObservationV1 {
                exit_code: 1,
                detail: "synthetic".to_string(),
                elapsed_ms: 0,
                failure_kind: Some(kind),
            };
            assert_eq!(classify_terminal_state(&observation, 1), expected);
        }
        let build = ShadowCommandObservationV1 {
            exit_code: 101,
            detail: "build_failure".to_string(),
            elapsed_ms: 0,
            failure_kind: None,
        };
        assert_eq!(
            classify_terminal_state(&build, 101),
            ShadowTerminalStateV1::BuildFailure
        );
        let masked = ShadowCommandObservationV1 {
            exit_code: 1,
            detail: "no equivalent result: setup_failure".to_string(),
            elapsed_ms: 0,
            failure_kind: None,
        };
        assert_eq!(
            classify_terminal_state(&masked, 1),
            ShadowTerminalStateV1::SetupFailure
        );
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_stale_policy_stops_before_execution() -> Result<(), String> {
        let mut manifest = sample_manifest(vec![entry_with_command("subject-a", "cmd-a")]);
        manifest.policy.semantic_digest = "stale".to_string();
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        if execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[]),
            &options,
        )
        .is_ok()
        {
            return Err("expected stale policy rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_stale_subject_identity_stops_before_execution()
    -> Result<(), String> {
        let manifest = sample_manifest(vec![ShadowSubjectManifestEntryV1 {
            subject_id: "subject-a".to_string(),
            full_verification_command: "cmd-a".to_string(),
            evidence: sample_evidence("subject-b"),
        }]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        if execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[]),
            &options,
        )
        .is_ok()
        {
            return Err("expected subject identity mismatch rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_recommendation_does_not_alter_full_command()
    -> Result<(), String> {
        let command = "cargo  test -p xtask shadow".to_string();
        let manifest = sample_manifest(vec![entry_with_command("subject-a", &command)]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        let receipt = execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[(&command, 0, "clean")]),
            &options,
        )?;
        assert_eq!(receipt.subjects[0].full_verification.command_line, command);
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_retry_retains_first_result() -> Result<(), String> {
        let manifest = sample_manifest(vec![entry_with_command("subject-a", "cmd-a")]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        let receipt = execute_verification_allocation_shadow_with_retry(
            &manifest,
            &SequentialShadowAdapter::new(&[
                observation(1, "product_failure"),
                observation(0, "clean"),
            ]),
            &options,
            true,
        )?;
        assert!(receipt.subjects[0].full_verification.first_attempt_retained);
        assert_eq!(receipt.subjects[0].full_verification.retry_attempt, 1);
        assert_eq!(
            receipt.subjects[0]
                .full_verification
                .first_attempt_exit_code,
            Some(1)
        );
        assert_eq!(receipt.subjects[0].full_verification.exit_code, Some(0));
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_wrong_tool_identity_rejects() -> Result<(), String> {
        let manifest = sample_manifest(vec![entry_with_command("subject-a", "cmd-a")]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: "other-tool@sha256:dead".to_string(),
            ..Default::default()
        };
        if execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[("cmd-a", 0, "clean")]),
            &options,
        )
        .is_ok()
        {
            return Err("expected tool identity rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_cleanup_failure_is_non_green() -> Result<(), String> {
        let manifest = sample_manifest(vec![entry_with_command("subject-a", "cmd-a")]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            cleanup_succeeds: false,
            cleanup_residue: vec!["tmp/shadow-artifact".to_string()],
        };
        let receipt = execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[("cmd-a", 0, "clean")]),
            &options,
        )?;
        assert_eq!(
            receipt.cleanup.disposition,
            ShadowCleanupDispositionV1::Failed
        );
        assert_eq!(receipt.cleanup.owned_residue, vec!["tmp/shadow-artifact"]);
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_equivalent_roots_preserve_portable_identity()
    -> Result<(), String> {
        let manifest = sample_manifest(vec![entry_with_command("subject-a", "cmd-a")]);
        assert_ne!(
            manifest.portable_root_identity,
            manifest.execution_root_identity
        );
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        let receipt = execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[("cmd-a", 0, "equivalent")]),
            &options,
        )?;
        assert_eq!(
            receipt.portable_root_identity,
            manifest.portable_root_identity
        );
        assert_eq!(
            receipt.execution_root_identity,
            manifest.execution_root_identity
        );
        assert_eq!(
            receipt.subjects[0].full_verification.terminal_state,
            ShadowTerminalStateV1::Equivalent
        );
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_retains_failed_subjects_in_denominator() -> Result<(), String>
    {
        let manifest = sample_manifest(vec![
            entry_with_command("subject-a", "cmd-a"),
            entry_with_command("subject-b", "cmd-b"),
        ]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        let receipt = execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[("cmd-a", 0, "clean"), ("cmd-b", 1, "test_failure")]),
            &options,
        )?;
        assert_eq!(receipt.subjects.len(), 2);
        assert_eq!(
            receipt.subjects[1].full_verification.terminal_state,
            ShadowTerminalStateV1::TestFailure
        );
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_emits_no_savings_or_admission_claims() -> Result<(), String> {
        let manifest = sample_manifest(vec![entry_with_command("subject-a", "cmd-a")]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        let receipt = execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[("cmd-a", 0, "clean")]),
            &options,
        )?;
        assert!(
            receipt
                .non_claims
                .iter()
                .any(|claim| claim.contains("no_policy_admission"))
        );
        assert!(
            receipt
                .non_claims
                .iter()
                .any(|claim| claim.contains("no_verification_savings_claim"))
        );
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_rejects_targeted_manifest_full_command() -> Result<(), String>
    {
        let manifest = sample_manifest(vec![ShadowSubjectManifestEntryV1 {
            subject_id: "subject-a".to_string(),
            full_verification_command: sample_rule().targeted_command.clone(),
            evidence: sample_evidence("subject-a"),
        }]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        if execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[]),
            &options,
        )
        .is_ok()
        {
            return Err("expected targeted manifest full command rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn verification_allocation_shadow_does_not_skip_verification_work() -> Result<(), String> {
        let manifest = sample_manifest(vec![entry_with_command("subject-a", "cmd-a")]);
        let options = VerificationAllocationShadowRunOptionsV1 {
            expected_tool_identity: manifest.tool_identity.clone(),
            ..Default::default()
        };
        let receipt = execute_verification_allocation_shadow(
            &manifest,
            &ScriptShadowAdapter::new(&[]),
            &options,
        )?;
        assert_eq!(receipt.subjects[0].full_verification.exit_code, Some(127));
        assert_eq!(receipt.subjects[0].recommendation.sequence, 1_u64);
        Ok(())
    }
}
