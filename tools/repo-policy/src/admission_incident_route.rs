//! Executable continuation routes for current `admission_incident.v1` packets (#5076).
//!
//! This module consumes compiled incident packets; it does not classify raw workflow
//! facts (#5075) or mutate live repository settings.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const ADMISSION_INCIDENT_SCHEMA: &str = "admission_incident.v1";
pub const ROUTE_CORPUS_SCHEMA: &str = "admission_incident_route_corpus.v1";
pub const DEFAULT_ROUTE_CORPUS_DIR: &str = "fixtures/admission_incident_route";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriStateV1 {
    Yes,
    No,
    NotProven,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentClassV1 {
    CandidateEscape,
    IntegrationInteraction,
    MainUnhealthy,
    InstrumentDefect,
    InfrastructureFailure,
    SupersededNoVerdict,
    NotProven,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentSubjectV1 {
    pub candidate_head_sha: String,
    pub merge_group_id: Option<String>,
    pub workflow_attempt_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentCurrentnessV1 {
    pub basis_is_current: bool,
    pub observed_candidate_head_sha: String,
    pub observed_merge_group_id: Option<String>,
    pub observed_workflow_attempt_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct RouteInputFlagsV1 {
    pub regression_guard_or_limitation_recorded: bool,
    pub behind_main_only: bool,
    pub slow_in_progress: bool,
    pub independent_defect_evidence_for_candidate_mutation: bool,
    pub material_finding_during_repair: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionIncidentV1 {
    pub schema: String,
    pub subject: IncidentSubjectV1,
    pub currentness: IncidentCurrentnessV1,
    pub incident_class: IncidentClassV1,
    #[serde(default)]
    pub classification_evidence: Vec<String>,
    pub candidate_mutation_allowed: TriStateV1,
    pub same_subject_retry_allowed: TriStateV1,
    pub owner_class: String,
    #[serde(default)]
    pub linked_prior_incident_id: Option<String>,
    #[serde(default)]
    pub route_inputs: RouteInputFlagsV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentSubjectContextV1 {
    pub candidate_head_sha: String,
    pub merge_group_id: Option<String>,
    pub workflow_attempt_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationRouteV1 {
    pub route_id: String,
    pub revoke_admission_custody: bool,
    pub return_pr_to_draft: bool,
    pub mutate_candidate_bytes: bool,
    pub same_subject_retry: bool,
    pub requires_new_ready_transaction: bool,
    pub requires_regression_guard_or_limitation: bool,
    pub route_complete: bool,
    pub preserve_unaffected_review: bool,
    pub handoff_repository_health_owner: bool,
    pub release_pr_local_actor: bool,
    pub forbid_poll_child: bool,
    pub retain_incident_lineage: bool,
    pub visible_blocker: bool,
    pub owner_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteErrorV1 {
    WrongSchema(String),
    StaleOrMismatchedSubject(String),
    PacketNotCurrent(String),
}

impl RouteErrorV1 {
    pub fn message(&self) -> String {
        match self {
            Self::WrongSchema(got) => {
                format!("incident packet schema must be `{ADMISSION_INCIDENT_SCHEMA}`, got `{got}`")
            }
            Self::StaleOrMismatchedSubject(detail) => {
                format!("incident packet is not current for exact H/G: {detail}")
            }
            Self::PacketNotCurrent(detail) => {
                format!("incident currentness basis is not current: {detail}")
            }
        }
    }
}

pub fn route_admission_incident(
    packet: &AdmissionIncidentV1,
    context: &CurrentSubjectContextV1,
) -> Result<ContinuationRouteV1, RouteErrorV1> {
    if packet.schema != ADMISSION_INCIDENT_SCHEMA {
        return Err(RouteErrorV1::WrongSchema(packet.schema.clone()));
    }
    verify_currentness(packet, context)?;
    Ok(route_for_class(packet))
}

fn verify_currentness(
    packet: &AdmissionIncidentV1,
    context: &CurrentSubjectContextV1,
) -> Result<(), RouteErrorV1> {
    if !packet.currentness.basis_is_current {
        return Err(RouteErrorV1::PacketNotCurrent(
            "compiler marked basis_is_current=false".to_string(),
        ));
    }
    if packet.subject.candidate_head_sha != context.candidate_head_sha {
        return Err(RouteErrorV1::StaleOrMismatchedSubject(format!(
            "candidate head {} != current {}",
            packet.subject.candidate_head_sha, context.candidate_head_sha
        )));
    }
    if packet.subject.workflow_attempt_id != context.workflow_attempt_id {
        return Err(RouteErrorV1::StaleOrMismatchedSubject(format!(
            "attempt {} != current {}",
            packet.subject.workflow_attempt_id, context.workflow_attempt_id
        )));
    }
    if packet.subject.merge_group_id != context.merge_group_id {
        return Err(RouteErrorV1::StaleOrMismatchedSubject(
            "merge group identity mismatch".to_string(),
        ));
    }
    if packet.currentness.observed_candidate_head_sha != context.candidate_head_sha
        || packet.currentness.observed_workflow_attempt_id != context.workflow_attempt_id
        || packet.currentness.observed_merge_group_id != context.merge_group_id
    {
        return Err(RouteErrorV1::StaleOrMismatchedSubject(
            "currentness observation does not match the active subject".to_string(),
        ));
    }
    Ok(())
}

fn route_for_class(packet: &AdmissionIncidentV1) -> ContinuationRouteV1 {
    let owner = packet.owner_class.clone();
    let retain_lineage = packet.linked_prior_incident_id.is_some()
        || matches!(
            packet.incident_class,
            IncidentClassV1::CandidateEscape | IncidentClassV1::IntegrationInteraction
        );

    match packet.incident_class {
        IncidentClassV1::CandidateEscape => {
            let complete = packet.route_inputs.regression_guard_or_limitation_recorded;
            ContinuationRouteV1 {
                route_id: "candidate_escape_repair".to_string(),
                revoke_admission_custody: true,
                return_pr_to_draft: true,
                mutate_candidate_bytes: packet.candidate_mutation_allowed == TriStateV1::Yes,
                same_subject_retry: false,
                requires_new_ready_transaction: true,
                requires_regression_guard_or_limitation: true,
                route_complete: complete,
                preserve_unaffected_review: packet.route_inputs.material_finding_during_repair,
                handoff_repository_health_owner: false,
                release_pr_local_actor: false,
                forbid_poll_child: true,
                retain_incident_lineage: retain_lineage,
                visible_blocker: false,
                owner_class: owner,
                incomplete_reason: if complete {
                    None
                } else {
                    Some(
                        "candidate escape requires regression-guard or reviewed limitation before re-admission"
                            .to_string(),
                    )
                },
            }
        }
        IncidentClassV1::IntegrationInteraction => {
            let behind_only = packet.route_inputs.behind_main_only;
            ContinuationRouteV1 {
                route_id: "integration_interaction_repair".to_string(),
                revoke_admission_custody: true,
                return_pr_to_draft: true,
                mutate_candidate_bytes: !behind_only
                    && packet.candidate_mutation_allowed == TriStateV1::Yes,
                same_subject_retry: false,
                requires_new_ready_transaction: true,
                requires_regression_guard_or_limitation: false,
                route_complete: !behind_only,
                preserve_unaffected_review: true,
                handoff_repository_health_owner: false,
                release_pr_local_actor: false,
                forbid_poll_child: true,
                retain_incident_lineage: retain_lineage,
                visible_blocker: behind_only,
                owner_class: owner,
                incomplete_reason: if behind_only {
                    Some(
                        "integration interaction cannot be inferred from behind-main metadata alone"
                            .to_string(),
                    )
                } else {
                    None
                },
            }
        }
        IncidentClassV1::InstrumentDefect => {
            let mutate = packet
                .route_inputs
                .independent_defect_evidence_for_candidate_mutation
                && packet.candidate_mutation_allowed == TriStateV1::Yes;
            ContinuationRouteV1 {
                route_id: "instrument_defect_repair".to_string(),
                revoke_admission_custody: false,
                return_pr_to_draft: false,
                mutate_candidate_bytes: mutate,
                same_subject_retry: packet.same_subject_retry_allowed == TriStateV1::Yes,
                requires_new_ready_transaction: false,
                requires_regression_guard_or_limitation: false,
                route_complete: true,
                preserve_unaffected_review: true,
                handoff_repository_health_owner: false,
                release_pr_local_actor: false,
                forbid_poll_child: true,
                retain_incident_lineage: retain_lineage,
                visible_blocker: false,
                owner_class: owner,
                incomplete_reason: None,
            }
        }
        IncidentClassV1::InfrastructureFailure => ContinuationRouteV1 {
            route_id: "infrastructure_same_subject_retry".to_string(),
            revoke_admission_custody: false,
            return_pr_to_draft: false,
            mutate_candidate_bytes: false,
            same_subject_retry: packet.same_subject_retry_allowed == TriStateV1::Yes,
            requires_new_ready_transaction: false,
            requires_regression_guard_or_limitation: false,
            route_complete: true,
            preserve_unaffected_review: true,
            handoff_repository_health_owner: false,
            release_pr_local_actor: false,
            forbid_poll_child: true,
            retain_incident_lineage: retain_lineage,
            visible_blocker: false,
            owner_class: owner,
            incomplete_reason: None,
        },
        IncidentClassV1::SupersededNoVerdict => ContinuationRouteV1 {
            route_id: "superseded_reclaim".to_string(),
            revoke_admission_custody: true,
            return_pr_to_draft: false,
            mutate_candidate_bytes: false,
            same_subject_retry: false,
            requires_new_ready_transaction: false,
            requires_regression_guard_or_limitation: false,
            route_complete: true,
            preserve_unaffected_review: true,
            handoff_repository_health_owner: false,
            release_pr_local_actor: false,
            forbid_poll_child: true,
            retain_incident_lineage: true,
            visible_blocker: false,
            owner_class: owner,
            incomplete_reason: None,
        },
        IncidentClassV1::NotProven => ContinuationRouteV1 {
            route_id: "not_proven_block".to_string(),
            revoke_admission_custody: false,
            return_pr_to_draft: false,
            mutate_candidate_bytes: false,
            same_subject_retry: false,
            requires_new_ready_transaction: false,
            requires_regression_guard_or_limitation: false,
            route_complete: true,
            preserve_unaffected_review: true,
            handoff_repository_health_owner: false,
            release_pr_local_actor: false,
            forbid_poll_child: true,
            retain_incident_lineage: retain_lineage,
            visible_blocker: true,
            owner_class: owner,
            incomplete_reason: Some(
                "missing evidence; do not retry or mutate by inference".to_string(),
            ),
        },
        IncidentClassV1::MainUnhealthy => ContinuationRouteV1 {
            route_id: "main_unhealthy_handoff".to_string(),
            revoke_admission_custody: true,
            return_pr_to_draft: false,
            mutate_candidate_bytes: false,
            same_subject_retry: false,
            requires_new_ready_transaction: false,
            requires_regression_guard_or_limitation: false,
            route_complete: true,
            preserve_unaffected_review: true,
            handoff_repository_health_owner: true,
            release_pr_local_actor: true,
            forbid_poll_child: true,
            retain_incident_lineage: retain_lineage,
            visible_blocker: false,
            owner_class: owner,
            incomplete_reason: None,
        },
    }
}

/// Slow in-progress work must not authorize cancellation or supersession by itself.
pub fn slowness_must_not_authorize_cancellation(route: &ContinuationRouteV1) -> bool {
    !route.revoke_admission_custody
        || route.route_id == "main_unhealthy_handoff"
        || route.route_id == "superseded_reclaim"
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteScenarioV1 {
    pub id: String,
    pub packet: AdmissionIncidentV1,
    pub context: CurrentSubjectContextV1,
    pub expected: ContinuationRouteV1,
    #[serde(default)]
    pub slow_in_progress: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteCorpusV1 {
    pub schema_version: String,
    pub claim_boundary: String,
    pub scenarios: Vec<RouteScenarioV1>,
}

pub fn load_route_corpus(path: &Path) -> Result<RouteCorpusV1, String> {
    let body = fs::read_to_string(path).map_err(|error| format!("{path:?}: {error}"))?;
    let corpus: RouteCorpusV1 =
        serde_json::from_str(&body).map_err(|error| format!("{path:?}: {error}"))?;
    if corpus.schema_version != ROUTE_CORPUS_SCHEMA {
        return Err(format!(
            "{path:?}: schema_version must be `{ROUTE_CORPUS_SCHEMA}`"
        ));
    }
    Ok(corpus)
}

pub fn validate_route_corpus(corpus: &RouteCorpusV1) -> Vec<String> {
    let mut findings = Vec::new();
    let mut ids = BTreeSet::new();
    for scenario in &corpus.scenarios {
        if !ids.insert(scenario.id.clone()) {
            findings.push(format!("duplicate scenario id `{}`", scenario.id));
        }
        match route_admission_incident(&scenario.packet, &scenario.context) {
            Ok(route) => {
                if route != scenario.expected {
                    findings.push(format!(
                        "scenario `{}`: expected route {:?}, got {:?}",
                        scenario.id, scenario.expected, route
                    ));
                }
                if scenario.slow_in_progress && !slowness_must_not_authorize_cancellation(&route) {
                    findings.push(format!(
                        "scenario `{}`: slow in-progress must not authorize arbitrary revocation",
                        scenario.id
                    ));
                }
            }
            Err(error) => {
                findings.push(format!(
                    "scenario `{}`: routing failed: {}",
                    scenario.id,
                    error.message()
                ));
            }
        }
    }
    findings
}

pub fn check_committed_route_corpus() -> Result<(), String> {
    let corpus_path = Path::new(DEFAULT_ROUTE_CORPUS_DIR).join("corpus.json");
    let corpus = load_route_corpus(&corpus_path)?;
    let findings = validate_route_corpus(&corpus);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(findings.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> CurrentSubjectContextV1 {
        CurrentSubjectContextV1 {
            candidate_head_sha: "sha256:head-h1".to_string(),
            merge_group_id: Some("g1".to_string()),
            workflow_attempt_id: "attempt-1".to_string(),
        }
    }

    fn sample_packet(class: IncidentClassV1, inputs: RouteInputFlagsV1) -> AdmissionIncidentV1 {
        let context = sample_context();
        AdmissionIncidentV1 {
            schema: ADMISSION_INCIDENT_SCHEMA.to_string(),
            subject: IncidentSubjectV1 {
                candidate_head_sha: context.candidate_head_sha.clone(),
                merge_group_id: context.merge_group_id.clone(),
                workflow_attempt_id: context.workflow_attempt_id.clone(),
            },
            currentness: IncidentCurrentnessV1 {
                basis_is_current: true,
                observed_candidate_head_sha: context.candidate_head_sha.clone(),
                observed_merge_group_id: context.merge_group_id.clone(),
                observed_workflow_attempt_id: context.workflow_attempt_id.clone(),
            },
            incident_class: class,
            classification_evidence: vec!["fixture".to_string()],
            candidate_mutation_allowed: TriStateV1::Yes,
            same_subject_retry_allowed: TriStateV1::No,
            owner_class: "candidate_writer".to_string(),
            linked_prior_incident_id: None,
            route_inputs: inputs,
        }
    }

    #[test]
    fn stale_attempt_refuses_routing() -> Result<(), String> {
        let mut packet = sample_packet(
            IncidentClassV1::CandidateEscape,
            RouteInputFlagsV1 {
                regression_guard_or_limitation_recorded: true,
                ..RouteInputFlagsV1::default()
            },
        );
        packet.subject.workflow_attempt_id = "attempt-0".to_string();
        let error = match route_admission_incident(&packet, &sample_context()) {
            Ok(_) => return Err("stale attempt must fail closed".to_string()),
            Err(error) => error,
        };
        assert!(matches!(error, RouteErrorV1::StaleOrMismatchedSubject(_)));
        Ok(())
    }

    #[test]
    fn committed_corpus_routes_match_expectations() -> Result<(), String> {
        crate::enter_workspace_root()?;
        check_committed_route_corpus()
    }
}
