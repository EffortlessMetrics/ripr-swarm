#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "policy:4707: advisory adapter surface lands before command wiring"
    )
)]

//! Optional Stage B verification-allocation adapter (#4707, VA04).
//!
//! Disabled by default. When explicitly enabled with an admitted policy generation,
//! applies only producer-owned targeted/deferred actions in an advisory/manual route,
//! retains deterministic audit selection, escalates to full verification on uncertainty,
//! and emits a typed execution receipt.

use serde::{Deserialize, Serialize};

use crate::verification_allocation_policy::{
    EvidenceStrengthV1, VerificationAllocationActionV1, VerificationAllocationLaterOwnerV1,
    VerificationAllocationPolicyV1, VerificationAllocationSubjectEvidenceV1, recommend_action,
    validate_policy,
};

pub(crate) const VERIFICATION_ALLOCATION_ADAPTER_SCHEMA_VERSION: &str =
    "verification_allocation_adapter.v1";

pub(crate) const VERIFICATION_ALLOCATION_ADAPTER_CLAIM_BOUNDARY: &str = "Stage B advisory adapter: records what was recommended and what actually ran in an \
 optional manual route; it does not alter protected required checks, admit rules, or \
 authorize production verification reduction.";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AdapterEnablementV1 {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExecutionDispositionV1 {
    NotRun,
    Recommended,
    Executed,
    Audited,
    Deferred,
    Escalated,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandExecutionRecordV1 {
    pub command: String,
    pub disposition: ExecutionDispositionV1,
    pub exit_code: Option<i32>,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationAllocationAdapterReceiptV1 {
    pub schema_version: String,
    pub enablement: AdapterEnablementV1,
    pub operating_mode: String,
    pub policy_generation: Option<u64>,
    pub policy_semantic_digest: Option<String>,
    pub subject_id: String,
    pub recommended_action: VerificationAllocationActionV1,
    pub rule_id: Option<String>,
    pub audit_selected: bool,
    pub audit_subject_digest: String,
    pub targeted: CommandExecutionRecordV1,
    pub audit: CommandExecutionRecordV1,
    pub full_fallback: CommandExecutionRecordV1,
    pub deferred: CommandExecutionRecordV1,
    pub escalated: bool,
    pub escalation_reason: Option<String>,
    pub disabled_rules: Vec<String>,
    pub claim_boundary: String,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct LocalCommandOutcome {
    pub exit_code: i32,
    pub detail: String,
}

pub(crate) trait LocalCommandAdapter {
    fn run(&self, command: &str) -> LocalCommandOutcome;
}

#[derive(Clone, Debug)]
pub(crate) struct VerificationAllocationAdapterInputV1 {
    pub enablement: AdapterEnablementV1,
    pub policy: VerificationAllocationPolicyV1,
    pub subject: VerificationAllocationSubjectEvidenceV1,
    pub requested_operation: VerificationAllocationActionV1,
    pub disabled_rules: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct VerificationAllocationAdapterStateV1 {
    pub disabled_rules: Vec<String>,
}

impl VerificationAllocationAdapterStateV1 {
    pub(crate) fn new() -> Self {
        Self {
            disabled_rules: Vec::new(),
        }
    }
}

impl Default for VerificationAllocationAdapterStateV1 {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn audit_subject_digest(
    policy: &VerificationAllocationPolicyV1,
    subject: &VerificationAllocationSubjectEvidenceV1,
    rule_id: &str,
    audit_selection_token: &str,
) -> String {
    let material = format!(
        "{}|{}|{}|{}|{}|{}",
        policy.generation,
        policy.semantic_digest,
        subject.subject_id,
        subject.ripr_input_identity,
        rule_id,
        audit_selection_token
    );
    crate::verification_allocation_policy::sha256_hex(material.as_bytes())
}

pub(crate) fn deterministic_audit_selected(digest: &str) -> bool {
    digest
        .chars()
        .last()
        .map(|last| matches!(last, '0' | '2' | '4' | '6' | '8' | 'a' | 'c' | 'e'))
        .unwrap_or(false)
}

fn not_run(command: &str) -> CommandExecutionRecordV1 {
    CommandExecutionRecordV1 {
        command: command.to_string(),
        disposition: ExecutionDispositionV1::NotRun,
        exit_code: None,
        detail: "not_run".to_string(),
    }
}

fn recommended(command: &str) -> CommandExecutionRecordV1 {
    CommandExecutionRecordV1 {
        command: command.to_string(),
        disposition: ExecutionDispositionV1::Recommended,
        exit_code: None,
        detail: "recommended_only".to_string(),
    }
}

fn executed(command: &str, outcome: &LocalCommandOutcome) -> CommandExecutionRecordV1 {
    CommandExecutionRecordV1 {
        command: command.to_string(),
        disposition: if outcome.exit_code == 0 {
            ExecutionDispositionV1::Executed
        } else {
            ExecutionDispositionV1::Failed
        },
        exit_code: Some(outcome.exit_code),
        detail: outcome.detail.clone(),
    }
}

fn audited(command: &str, outcome: &LocalCommandOutcome) -> CommandExecutionRecordV1 {
    CommandExecutionRecordV1 {
        command: command.to_string(),
        disposition: if outcome.exit_code == 0 {
            ExecutionDispositionV1::Audited
        } else {
            ExecutionDispositionV1::Failed
        },
        exit_code: Some(outcome.exit_code),
        detail: outcome.detail.clone(),
    }
}

fn deferred_record(owner: &VerificationAllocationLaterOwnerV1) -> CommandExecutionRecordV1 {
    CommandExecutionRecordV1 {
        command: owner.command.clone(),
        disposition: ExecutionDispositionV1::Deferred,
        exit_code: None,
        detail: format!("deferred_to_lane:{}", owner.lane_id),
    }
}

fn escalate(
    command: &str,
    adapter: &dyn LocalCommandAdapter,
    reason: &str,
) -> (CommandExecutionRecordV1, bool, String) {
    let outcome = adapter.run(command);
    (
        CommandExecutionRecordV1 {
            command: command.to_string(),
            disposition: ExecutionDispositionV1::Escalated,
            exit_code: Some(outcome.exit_code),
            detail: format!("{reason}:{}", outcome.detail),
        },
        true,
        reason.to_string(),
    )
}

pub(crate) fn execute_verification_allocation_adapter(
    input: &VerificationAllocationAdapterInputV1,
    adapter: &dyn LocalCommandAdapter,
    state: &mut VerificationAllocationAdapterStateV1,
) -> Result<VerificationAllocationAdapterReceiptV1, String> {
    let mut disabled_rules = input.disabled_rules.clone();
    disabled_rules.extend(state.disabled_rules.clone());
    disabled_rules.sort();
    disabled_rules.dedup();

    let base_receipt = |recommended: VerificationAllocationActionV1,
                        rule_id: Option<String>,
                        audit_selected: bool,
                        audit_digest: String,
                        targeted: CommandExecutionRecordV1,
                        audit: CommandExecutionRecordV1,
                        full_fallback: CommandExecutionRecordV1,
                        deferred: CommandExecutionRecordV1,
                        escalated: bool,
                        escalation_reason: Option<String>,
                        policy_generation: Option<u64>,
                        policy_digest: Option<String>| {
        VerificationAllocationAdapterReceiptV1 {
            schema_version: VERIFICATION_ALLOCATION_ADAPTER_SCHEMA_VERSION.to_string(),
            enablement: input.enablement,
            operating_mode: if input.enablement == AdapterEnablementV1::Disabled {
                "disabled".to_string()
            } else {
                "advisory_audited".to_string()
            },
            policy_generation,
            policy_semantic_digest: policy_digest,
            subject_id: input.subject.subject_id.clone(),
            recommended_action: recommended,
            rule_id,
            audit_selected,
            audit_subject_digest: audit_digest,
            targeted,
            audit,
            full_fallback,
            deferred,
            escalated,
            escalation_reason,
            disabled_rules: disabled_rules.clone(),
            claim_boundary: VERIFICATION_ALLOCATION_ADAPTER_CLAIM_BOUNDARY.to_string(),
            non_claims: vec![VERIFICATION_ALLOCATION_ADAPTER_CLAIM_BOUNDARY.to_string()],
        }
    };

    if input.enablement == AdapterEnablementV1::Disabled {
        return Ok(base_receipt(
            VerificationAllocationActionV1::FullRequired,
            None,
            false,
            String::new(),
            not_run("disabled"),
            not_run("disabled"),
            not_run("disabled"),
            not_run("disabled"),
            false,
            None,
            None,
            None,
        ));
    }

    validate_policy(&input.policy)?;

    let decision = recommend_action(&input.policy, &input.subject);
    let rule = decision.rule_id.as_ref().and_then(|rule_id| {
        input
            .policy
            .rules
            .iter()
            .find(|rule| &rule.rule_id == rule_id)
    });

    if let Some(rule) = rule
        && disabled_rules.iter().any(|id| id == &rule.rule_id)
    {
        let full_command = rule.full_audit_command.clone();
        let (full_fallback, escalated, reason) = escalate(&full_command, adapter, "rule_disabled");
        return Ok(base_receipt(
            VerificationAllocationActionV1::FullRequired,
            Some(rule.rule_id.clone()),
            false,
            String::new(),
            not_run(&rule.targeted_command),
            not_run(&rule.full_audit_command),
            full_fallback,
            not_run("defer"),
            escalated,
            Some(reason),
            Some(input.policy.generation),
            Some(input.policy.semantic_digest.clone()),
        ));
    }

    if input.subject.evidence_strength != EvidenceStrengthV1::Complete {
        let full_command = rule
            .map(|rule| rule.full_audit_command.clone())
            .unwrap_or_else(|| "cargo xtask ci-full".to_string());
        let (full_fallback, escalated, reason) =
            escalate(&full_command, adapter, "uncertain_evidence");
        return Ok(base_receipt(
            VerificationAllocationActionV1::FullRequired,
            decision.rule_id.clone(),
            false,
            String::new(),
            not_run("targeted"),
            not_run("audit"),
            full_fallback,
            not_run("defer"),
            escalated,
            Some(reason),
            Some(input.policy.generation),
            Some(input.policy.semantic_digest.clone()),
        ));
    }

    let Some(rule) = rule else {
        return Ok(base_receipt(
            decision.recommended_action,
            None,
            false,
            String::new(),
            not_run("targeted"),
            not_run("audit"),
            not_run("full"),
            not_run("defer"),
            false,
            Some("no_applicable_rule".to_string()),
            Some(input.policy.generation),
            Some(input.policy.semantic_digest.clone()),
        ));
    };

    let audit_digest = audit_subject_digest(
        &input.policy,
        &input.subject,
        &rule.rule_id,
        &rule.audit_selection_token,
    );
    let audit_selected = deterministic_audit_selected(&audit_digest);

    match input.requested_operation {
        VerificationAllocationActionV1::DeferToNightlyOrRelease => {
            let Some(owner) = rule.later_owner.as_ref() else {
                let (full_fallback, escalated, reason) =
                    escalate(&rule.full_audit_command, adapter, "missing_later_owner");
                return Ok(base_receipt(
                    VerificationAllocationActionV1::FullRequired,
                    Some(rule.rule_id.clone()),
                    audit_selected,
                    audit_digest,
                    not_run(&rule.targeted_command),
                    not_run(&rule.full_audit_command),
                    full_fallback,
                    not_run("defer"),
                    escalated,
                    Some(reason),
                    Some(input.policy.generation),
                    Some(input.policy.semantic_digest.clone()),
                ));
            };
            Ok(base_receipt(
                VerificationAllocationActionV1::DeferToNightlyOrRelease,
                Some(rule.rule_id.clone()),
                audit_selected,
                audit_digest,
                not_run(&rule.targeted_command),
                not_run(&rule.full_audit_command),
                not_run(&rule.full_audit_command),
                deferred_record(owner),
                false,
                None,
                Some(input.policy.generation),
                Some(input.policy.semantic_digest.clone()),
            ))
        }
        VerificationAllocationActionV1::TargetedPlusAudit => {
            let targeted_outcome = adapter.run(&rule.targeted_command);
            let targeted_record = executed(&rule.targeted_command, &targeted_outcome);
            if targeted_outcome.exit_code != 0 {
                let (full_fallback, escalated, reason) =
                    escalate(&rule.full_audit_command, adapter, "targeted_failed");
                return Ok(base_receipt(
                    VerificationAllocationActionV1::FullRequired,
                    Some(rule.rule_id.clone()),
                    audit_selected,
                    audit_digest,
                    targeted_record,
                    not_run(&rule.full_audit_command),
                    full_fallback,
                    not_run("defer"),
                    escalated,
                    Some(reason),
                    Some(input.policy.generation),
                    Some(input.policy.semantic_digest.clone()),
                ));
            }

            let mut audit_record = not_run(&rule.full_audit_command);
            if audit_selected {
                let audit_outcome = adapter.run(&rule.full_audit_command);
                audit_record = audited(&rule.full_audit_command, &audit_outcome);
                if audit_outcome.exit_code != 0 {
                    if !state.disabled_rules.contains(&rule.rule_id) {
                        state.disabled_rules.push(rule.rule_id.clone());
                    }
                    let (full_fallback, escalated, reason) =
                        escalate(&rule.full_audit_command, adapter, "audit_failed");
                    return Ok(base_receipt(
                        VerificationAllocationActionV1::FullRequired,
                        Some(rule.rule_id.clone()),
                        true,
                        audit_digest,
                        targeted_record,
                        audit_record,
                        full_fallback,
                        not_run("defer"),
                        escalated,
                        Some(reason),
                        Some(input.policy.generation),
                        Some(input.policy.semantic_digest.clone()),
                    ));
                }
            }

            Ok(base_receipt(
                VerificationAllocationActionV1::TargetedPlusAudit,
                Some(rule.rule_id.clone()),
                audit_selected,
                audit_digest,
                targeted_record,
                audit_record,
                recommended(&rule.full_audit_command),
                not_run("defer"),
                false,
                None,
                Some(input.policy.generation),
                Some(input.policy.semantic_digest.clone()),
            ))
        }
        VerificationAllocationActionV1::FullRequired => {
            let (full_fallback, escalated, reason) =
                escalate(&rule.full_audit_command, adapter, "explicit_full");
            Ok(base_receipt(
                VerificationAllocationActionV1::FullRequired,
                Some(rule.rule_id.clone()),
                audit_selected,
                audit_digest,
                not_run(&rule.targeted_command),
                not_run(&rule.full_audit_command),
                full_fallback,
                not_run("defer"),
                escalated,
                Some(reason),
                Some(input.policy.generation),
                Some(input.policy.semantic_digest.clone()),
            ))
        }
        VerificationAllocationActionV1::UnsupportedNoRecommendation => Ok(base_receipt(
            VerificationAllocationActionV1::UnsupportedNoRecommendation,
            Some(rule.rule_id.clone()),
            audit_selected,
            audit_digest,
            not_run(&rule.targeted_command),
            not_run(&rule.full_audit_command),
            not_run(&rule.full_audit_command),
            not_run("defer"),
            false,
            Some("unsupported_request".to_string()),
            Some(input.policy.generation),
            Some(input.policy.semantic_digest.clone()),
        )),
    }
}

#[cfg(test)]
mod adapter_semantics {
    use super::*;
    use crate::verification_allocation_policy::{
        RuleAdmissionStatusV1, VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY,
        VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION, VerificationAllocationLaterOwnerV1,
        VerificationAllocationRuleV1, policy_semantic_digest,
    };

    struct ScriptAdapter {
        outcomes: std::collections::BTreeMap<String, LocalCommandOutcome>,
    }

    impl ScriptAdapter {
        fn new(outcomes: &[(&str, i32)]) -> Self {
            let mut map = std::collections::BTreeMap::new();
            for (command, code) in outcomes {
                map.insert(
                    (*command).to_string(),
                    LocalCommandOutcome {
                        exit_code: *code,
                        detail: format!("exit={code}"),
                    },
                );
            }
            Self { outcomes: map }
        }
    }

    impl LocalCommandAdapter for ScriptAdapter {
        fn run(&self, command: &str) -> LocalCommandOutcome {
            self.outcomes
                .get(command)
                .cloned()
                .unwrap_or(LocalCommandOutcome {
                    exit_code: 127,
                    detail: "missing_script".to_string(),
                })
        }
    }

    fn sample_subject() -> VerificationAllocationSubjectEvidenceV1 {
        VerificationAllocationSubjectEvidenceV1 {
            subject_id: "subject-1".to_string(),
            behavior_family: "rust_analysis".to_string(),
            evidence_strength: EvidenceStrengthV1::Complete,
            ripr_input_identity: "sha256:input".to_string(),
            route_state_digest: "sha256:route".to_string(),
        }
    }

    fn sample_policy_with_rule(
        rule: VerificationAllocationRuleV1,
    ) -> VerificationAllocationPolicyV1 {
        let mut policy = VerificationAllocationPolicyV1 {
            schema_version: VERIFICATION_ALLOCATION_POLICY_SCHEMA_VERSION.to_string(),
            generation: 1,
            semantic_digest: String::new(),
            rules: vec![rule],
            limitations: vec![],
            non_claims: vec![VERIFICATION_ALLOCATION_POLICY_CLAIM_BOUNDARY.to_string()],
        };
        policy.semantic_digest = policy_semantic_digest(&policy);
        policy
    }

    fn sample_rule() -> VerificationAllocationRuleV1 {
        VerificationAllocationRuleV1 {
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
        }
    }

    #[test]
    fn verification_allocation_adapter_disabled_performs_no_reduced_operation() -> Result<(), String>
    {
        let policy = sample_policy_with_rule(sample_rule());
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Disabled,
            policy,
            subject: sample_subject(),
            requested_operation: VerificationAllocationActionV1::TargetedPlusAudit,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        let receipt =
            execute_verification_allocation_adapter(&input, &ScriptAdapter::new(&[]), &mut state)?;
        assert_eq!(receipt.enablement, AdapterEnablementV1::Disabled);
        assert_eq!(receipt.targeted.disposition, ExecutionDispositionV1::NotRun);
        assert!(!receipt.escalated);
        Ok(())
    }

    #[test]
    fn verification_allocation_adapter_stale_policy_generation_rejects() -> Result<(), String> {
        let mut policy = sample_policy_with_rule(sample_rule());
        policy.semantic_digest = "stale".to_string();
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Enabled,
            policy,
            subject: sample_subject(),
            requested_operation: VerificationAllocationActionV1::TargetedPlusAudit,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        if execute_verification_allocation_adapter(&input, &ScriptAdapter::new(&[]), &mut state)
            .is_ok()
        {
            return Err("expected stale policy rejection".to_string());
        }
        Ok(())
    }

    #[test]
    fn verification_allocation_adapter_missing_evidence_escalates_to_full() -> Result<(), String> {
        let policy = sample_policy_with_rule(sample_rule());
        let mut subject = sample_subject();
        subject.evidence_strength = EvidenceStrengthV1::Missing;
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Enabled,
            policy,
            subject,
            requested_operation: VerificationAllocationActionV1::TargetedPlusAudit,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        let receipt = execute_verification_allocation_adapter(
            &input,
            &ScriptAdapter::new(&[("cargo clippy --workspace --all-targets", 0)]),
            &mut state,
        )?;
        assert!(receipt.escalated);
        assert_eq!(
            receipt.full_fallback.disposition,
            ExecutionDispositionV1::Escalated
        );
        Ok(())
    }

    #[test]
    fn verification_allocation_adapter_audit_selection_is_deterministic_and_not_manually_cleared()
    -> Result<(), String> {
        let policy = sample_policy_with_rule(sample_rule());
        let digest = audit_subject_digest(
            &policy,
            &sample_subject(),
            "rust-focused-clippy",
            "audit-token-a",
        );
        assert_eq!(
            digest,
            audit_subject_digest(
                &policy,
                &sample_subject(),
                "rust-focused-clippy",
                "audit-token-a"
            )
        );
        let selected = deterministic_audit_selected(&digest);
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Enabled,
            policy,
            subject: sample_subject(),
            requested_operation: VerificationAllocationActionV1::TargetedPlusAudit,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        let adapter = ScriptAdapter::new(&[
            ("cargo clippy -p ripr", 0),
            ("cargo clippy --workspace --all-targets", 0),
        ]);
        let receipt = execute_verification_allocation_adapter(&input, &adapter, &mut state)?;
        assert_eq!(receipt.audit_selected, selected);
        assert_eq!(receipt.audit_subject_digest, digest);
        Ok(())
    }

    #[test]
    fn verification_allocation_adapter_targeted_failure_escalates_to_full() -> Result<(), String> {
        let policy = sample_policy_with_rule(sample_rule());
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Enabled,
            policy,
            subject: sample_subject(),
            requested_operation: VerificationAllocationActionV1::TargetedPlusAudit,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        let receipt = execute_verification_allocation_adapter(
            &input,
            &ScriptAdapter::new(&[
                ("cargo clippy -p ripr", 1),
                ("cargo clippy --workspace --all-targets", 0),
            ]),
            &mut state,
        )?;
        assert!(receipt.escalated);
        assert_eq!(receipt.targeted.disposition, ExecutionDispositionV1::Failed);
        Ok(())
    }

    #[test]
    fn verification_allocation_adapter_audit_failure_disables_rule_for_later_subjects()
    -> Result<(), String> {
        let policy = sample_policy_with_rule(sample_rule());
        let digest = audit_subject_digest(
            &policy,
            &sample_subject(),
            "rust-focused-clippy",
            "audit-token-a",
        );
        if !deterministic_audit_selected(&digest) {
            return Ok(());
        }
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Enabled,
            policy: policy.clone(),
            subject: sample_subject(),
            requested_operation: VerificationAllocationActionV1::TargetedPlusAudit,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        let receipt = execute_verification_allocation_adapter(
            &input,
            &ScriptAdapter::new(&[
                ("cargo clippy -p ripr", 0),
                ("cargo clippy --workspace --all-targets", 2),
            ]),
            &mut state,
        )?;
        assert!(receipt.escalated);
        assert!(
            state
                .disabled_rules
                .contains(&"rust-focused-clippy".to_string())
        );

        let second = execute_verification_allocation_adapter(
            &VerificationAllocationAdapterInputV1 {
                disabled_rules: state.disabled_rules.clone(),
                ..input
            },
            &ScriptAdapter::new(&[("cargo clippy --workspace --all-targets", 0)]),
            &mut state,
        )?;
        assert!(second.escalated);
        assert_eq!(second.targeted.disposition, ExecutionDispositionV1::NotRun);
        Ok(())
    }

    #[test]
    fn verification_allocation_adapter_new_behavior_family_falls_back_without_targeted_execution()
    -> Result<(), String> {
        let policy = sample_policy_with_rule(sample_rule());
        let mut subject = sample_subject();
        subject.behavior_family = "python_analysis".to_string();
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Enabled,
            policy,
            subject,
            requested_operation: VerificationAllocationActionV1::TargetedPlusAudit,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        let receipt =
            execute_verification_allocation_adapter(&input, &ScriptAdapter::new(&[]), &mut state)?;
        assert_eq!(
            receipt.recommended_action,
            VerificationAllocationActionV1::UnsupportedNoRecommendation
        );
        assert_eq!(receipt.targeted.disposition, ExecutionDispositionV1::NotRun);
        Ok(())
    }

    #[test]
    fn verification_allocation_adapter_defer_without_later_owner_escalates() -> Result<(), String> {
        let mut rule = sample_rule();
        rule.later_owner = None;
        let policy = sample_policy_with_rule(rule);
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Enabled,
            policy,
            subject: sample_subject(),
            requested_operation: VerificationAllocationActionV1::DeferToNightlyOrRelease,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        let receipt = execute_verification_allocation_adapter(
            &input,
            &ScriptAdapter::new(&[("cargo clippy --workspace --all-targets", 0)]),
            &mut state,
        )?;
        assert!(receipt.escalated);
        assert_eq!(receipt.deferred.disposition, ExecutionDispositionV1::NotRun);
        Ok(())
    }

    #[test]
    fn verification_allocation_adapter_receipt_distinguishes_execution_states() -> Result<(), String>
    {
        let policy = sample_policy_with_rule(sample_rule());
        let input = VerificationAllocationAdapterInputV1 {
            enablement: AdapterEnablementV1::Enabled,
            policy,
            subject: sample_subject(),
            requested_operation: VerificationAllocationActionV1::TargetedPlusAudit,
            disabled_rules: Vec::new(),
        };
        let mut state = VerificationAllocationAdapterStateV1::new();
        let receipt = execute_verification_allocation_adapter(
            &input,
            &ScriptAdapter::new(&[("cargo clippy -p ripr", 0)]),
            &mut state,
        )?;
        assert_eq!(
            receipt.targeted.disposition,
            ExecutionDispositionV1::Executed
        );
        assert_ne!(receipt.targeted.disposition, receipt.deferred.disposition);
        Ok(())
    }
}
