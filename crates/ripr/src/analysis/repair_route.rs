//! Producer-owned repair-route readiness.
//!
//! This module decides whether the analyzer facts are sufficient to localize
//! one safe test-only repair. Output projections consume this result; they do
//! not infer test identity, effect sinks, or missing observations themselves.

use super::seam_classification::ClassifiedSeam;
use super::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind};
use super::test_grip_evidence::{RelatedTestGrip, TestGripEvidence, TestTargetEvidence};
use crate::analysis::canonical_gap::canonical_gap_identity;
use crate::domain::{OracleKind, OracleStrength, StageState};
use std::path::PathBuf;

pub(crate) const REPAIR_ROUTE_AUTHORITY_BOUNDARY: &str =
    "analysis/producer-owned-repair-route-readiness";
const PRODUCER_DISCRIMINATOR_EVIDENCE: &str = "producer-owned missing discriminator fact";
const SAFE_TEST_TARGET_EVIDENCE: &str = "safe test target";

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepairRouteState {
    Ready,
    AlreadyGripped,
    PolicyExcluded,
    StaticLimitation,
}

/// The producer-owned choice of where a test-only repair may land.
///
/// `Missing` is deliberate: a related-test summary, a path, or a renderer
/// heuristic is not permission to edit that location. New-test proposals are
/// represented explicitly so a future producer can supply them without
/// overloading an existing-test identity.
#[allow(
    dead_code,
    reason = "reserved typed target proposal variants await a RustIndex proposal producer"
)]
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepairTargetSelection {
    Existing(TestTargetEvidence),
    Proposed(NewTestTargetProposal),
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct NewTestTargetProposal {
    pub(crate) kind: NewTestKind,
    pub(crate) file: PathBuf,
    pub(crate) owner: String,
    pub(crate) provenance: NewTestProposalProvenance,
}

#[allow(
    dead_code,
    reason = "reserved typed new-test kinds await a RustIndex proposal producer"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestKind {
    InlineUnit,
    Integration,
}

#[allow(
    dead_code,
    reason = "reserved typed proposal provenance awaits a RustIndex proposal producer"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestProposalProvenance {
    ProducerOwned,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct RepairRouteReadiness {
    pub(crate) state: RepairRouteState,
    pub(crate) seam_id: String,
    pub(crate) canonical_gap_id: Option<String>,
    pub(crate) required_evidence: Vec<String>,
    pub(crate) present_evidence: Vec<String>,
    pub(crate) missing_evidence: Vec<String>,
    pub(crate) target_selection: RepairTargetSelection,
    pub(crate) test_target: Option<TestTargetEvidence>,
    pub(crate) proposed_oracle: Option<OracleKind>,
    pub(crate) current_oracle: Option<OracleKind>,
    pub(crate) authority_boundary: &'static str,
}

pub(crate) fn repair_route_readiness(entry: &ClassifiedSeam) -> RepairRouteReadiness {
    let seam = &entry.seam;
    let evidence = &entry.evidence;
    let mut readiness = match seam.kind() {
        SeamKind::PredicateBoundary
        | SeamKind::ErrorVariant
        | SeamKind::ReturnValue
        | SeamKind::FieldConstruction
        | SeamKind::MatchArm => value_route_readiness(seam, evidence),
        SeamKind::SideEffect | SeamKind::CallPresence => effect_route_readiness(seam, evidence),
    };
    apply_grip_class_ceiling(entry.class, &mut readiness);
    readiness.seam_id = seam.id().as_str().to_string();
    readiness.canonical_gap_id = canonical_gap_identity(entry).map(|identity| identity.id);
    readiness.authority_boundary = REPAIR_ROUTE_AUTHORITY_BOUNDARY;
    readiness
}

fn apply_grip_class_ceiling(class: SeamGripClass, readiness: &mut RepairRouteReadiness) {
    match class {
        SeamGripClass::StronglyGripped => {
            readiness.state = RepairRouteState::AlreadyGripped;
            readiness.missing_evidence.clear();
        }
        SeamGripClass::Intentional | SeamGripClass::Suppressed => {
            readiness.state = RepairRouteState::PolicyExcluded;
            readiness.missing_evidence.clear();
        }
        SeamGripClass::ActivationUnknown
        | SeamGripClass::PropagationUnknown
        | SeamGripClass::ObservationUnknown
        | SeamGripClass::DiscriminationUnknown
        | SeamGripClass::Opaque => {
            let missing_stage = incomplete_stage_evidence(class).to_string();
            if !readiness
                .required_evidence
                .iter()
                .any(|evidence| evidence == &missing_stage)
            {
                readiness.required_evidence.push(missing_stage.clone());
            }
            if !readiness
                .missing_evidence
                .iter()
                .any(|evidence| evidence == &missing_stage)
            {
                readiness.missing_evidence.push(missing_stage);
            }
            readiness.state = RepairRouteState::StaticLimitation;
        }
        SeamGripClass::WeaklyGripped
        | SeamGripClass::Ungripped
        | SeamGripClass::ReachableUnrevealed => {}
    }
}

fn incomplete_stage_evidence(class: SeamGripClass) -> &'static str {
    match class {
        SeamGripClass::ActivationUnknown => "incomplete evidence stage: activation",
        SeamGripClass::PropagationUnknown => "incomplete evidence stage: propagation",
        SeamGripClass::ObservationUnknown => "incomplete evidence stage: observation",
        SeamGripClass::DiscriminationUnknown => "incomplete evidence stage: discrimination",
        SeamGripClass::Opaque => "incomplete evidence stage: opaque",
        _ => "incomplete evidence stage: unknown",
    }
}

fn value_route_readiness(seam: &RepoSeam, evidence: &TestGripEvidence) -> RepairRouteReadiness {
    let discriminator_evidence = format!(
        "{PRODUCER_DISCRIMINATOR_EVIDENCE}: {}",
        seam.required_discriminator().as_str()
    );
    let required = vec![
        discriminator_evidence.clone(),
        SAFE_TEST_TARGET_EVIDENCE.to_string(),
    ];
    let has_discriminator = has_exact_discriminator(seam, evidence);
    let selected_test_target = existing_test_target(evidence, false);
    // No related test is not evidence of a safe new-test location. A future
    // producer may populate `Proposed`; until then the target is Missing.
    let has_safe_target = selected_test_target.is_some();
    let state = if has_discriminator && has_safe_target {
        RepairRouteState::Ready
    } else {
        RepairRouteState::StaticLimitation
    };
    let mut present_evidence = Vec::new();
    if has_discriminator {
        present_evidence.push(discriminator_evidence.clone());
    }
    if has_safe_target {
        present_evidence.push(SAFE_TEST_TARGET_EVIDENCE.to_string());
    }
    let mut missing_evidence = Vec::new();
    if !has_discriminator {
        missing_evidence.push(discriminator_evidence);
    }
    if !has_safe_target {
        missing_evidence.push(SAFE_TEST_TARGET_EVIDENCE.to_string());
    }
    RepairRouteReadiness {
        state,
        seam_id: String::new(),
        canonical_gap_id: None,
        required_evidence: required,
        present_evidence,
        missing_evidence,
        target_selection: selected_test_target
            .clone()
            .map(RepairTargetSelection::Existing)
            .unwrap_or(RepairTargetSelection::Missing),
        test_target: selected_test_target,
        proposed_oracle: Some(oracle_for_seam(seam.kind())),
        current_oracle: current_oracle(evidence, false, true),
        authority_boundary: REPAIR_ROUTE_AUTHORITY_BOUNDARY,
    }
}

fn effect_route_readiness(seam: &RepoSeam, evidence: &TestGripEvidence) -> RepairRouteReadiness {
    let mut required_evidence = vec![
        "known production owner/caller".to_string(),
        "exact effect or call target".to_string(),
        "producer-owned test symbol".to_string(),
        "direct owner relationship".to_string(),
        "observable sink localization".to_string(),
    ];
    let mut present_evidence = Vec::new();
    let mut missing_evidence = Vec::new();

    if seam.owner().trim().is_empty() {
        missing_evidence.push("known production owner/caller".to_string());
    } else {
        present_evidence.push("known production owner/caller".to_string());
    }

    let target_is_exact = match (
        seam.kind(),
        seam.required_discriminator(),
        seam.expected_sink(),
    ) {
        (
            SeamKind::CallPresence,
            RequiredDiscriminator::CallSite { target },
            ExpectedSink::SideEffect,
        ) => !target.trim().is_empty(),
        (
            SeamKind::SideEffect,
            RequiredDiscriminator::Effect { sink },
            ExpectedSink::SideEffect,
        ) => !sink.trim().is_empty(),
        _ => false,
    };
    if target_is_exact {
        present_evidence.push("exact effect or call target".to_string());
    } else {
        missing_evidence.push("exact effect or call target".to_string());
    }

    let related = direct_owner_related_test(evidence);
    let test_target = related.and_then(|test| test.test_target.clone());
    if test_target.is_some() {
        present_evidence.push("producer-owned test symbol".to_string());
    } else {
        missing_evidence.push("producer-owned test symbol".to_string());
    }
    if related.is_some() {
        present_evidence.push("direct owner relationship".to_string());
    } else {
        missing_evidence.push("direct owner relationship".to_string());
    }

    let current = related.map(|test| test.oracle_kind.clone());
    let current_strength = related.map(|test| test.oracle_strength.clone());
    let stages_localize_sink = matches!(
        (
            &evidence.reach.state,
            &evidence.activate.state,
            &evidence.propagate.state
        ),
        (StageState::Yes, StageState::Yes, StageState::Yes)
    );
    if stages_localize_sink {
        present_evidence.push("observable sink localization".to_string());
    } else {
        missing_evidence.push("observable sink localization".to_string());
    }

    let current_mock_is_strong = current.as_ref() == Some(&OracleKind::MockExpectation)
        && current_strength.as_ref() == Some(&OracleStrength::Strong)
        && evidence.observe.state == StageState::Yes
        && evidence.discriminate.state == StageState::Yes;
    let route_is_localized = missing_evidence.is_empty();
    let state = if current_mock_is_strong && route_is_localized {
        RepairRouteState::AlreadyGripped
    } else if route_is_localized {
        RepairRouteState::Ready
    } else {
        RepairRouteState::StaticLimitation
    };

    required_evidence.push("current observation may be absent or weak".to_string());
    if current_mock_is_strong {
        present_evidence.push("strong current MockExpectation".to_string());
    } else {
        present_evidence.push("current observation is absent or weak".to_string());
    }

    RepairRouteReadiness {
        state,
        seam_id: String::new(),
        canonical_gap_id: None,
        required_evidence,
        present_evidence,
        missing_evidence,
        target_selection: test_target
            .clone()
            .map(RepairTargetSelection::Existing)
            .unwrap_or(RepairTargetSelection::Missing),
        test_target,
        proposed_oracle: Some(OracleKind::MockExpectation),
        current_oracle: current,
        authority_boundary: REPAIR_ROUTE_AUTHORITY_BOUNDARY,
    }
}

fn has_exact_discriminator(seam: &RepoSeam, evidence: &TestGripEvidence) -> bool {
    evidence
        .missing_discriminators
        .iter()
        .any(|fact| discriminator_fact_matches(seam.required_discriminator(), &fact.value))
}

fn discriminator_fact_matches(required: &RequiredDiscriminator, fact: &str) -> bool {
    let fact = fact.trim().to_ascii_lowercase();
    if fact.is_empty() {
        return false;
    }
    match required {
        RequiredDiscriminator::BoundaryValue { description } => significant_tokens(description)
            .last()
            .is_some_and(|token| fact.contains(token)),
        RequiredDiscriminator::ReturnValue { description } => {
            contains_all_tokens(&fact, description)
        }
        RequiredDiscriminator::ErrorVariant { variant } => contains_all_tokens(&fact, variant),
        RequiredDiscriminator::MatchArmTaken { arm } => contains_all_tokens(&fact, arm),
        RequiredDiscriminator::FieldValue { field } => contains_all_tokens(&fact, field),
        RequiredDiscriminator::Effect { sink }
        | RequiredDiscriminator::CallSite { target: sink } => contains_all_tokens(&fact, sink),
    }
}

fn contains_all_tokens(fact: &str, required: &str) -> bool {
    let tokens = significant_tokens(required);
    !tokens.is_empty() && tokens.into_iter().all(|token| fact.contains(&token))
}

fn significant_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            (token.len() >= 3).then_some(token)
        })
        .collect()
}

fn direct_owner_related_test(evidence: &TestGripEvidence) -> Option<&RelatedTestGrip> {
    evidence
        .related_tests
        .iter()
        .find(|test| test.relation_reason == crate::domain::RelationReason::DirectOwnerCall)
}

fn existing_test_target(
    evidence: &TestGripEvidence,
    require_direct_owner: bool,
) -> Option<TestTargetEvidence> {
    evidence
        .related_tests
        .iter()
        .find(|test| {
            (!require_direct_owner
                || test.relation_reason == crate::domain::RelationReason::DirectOwnerCall)
                && test.test_target.is_some()
        })
        .and_then(|test| test.test_target.clone())
}

fn current_oracle(
    evidence: &TestGripEvidence,
    require_direct_owner: bool,
    require_test_target: bool,
) -> Option<OracleKind> {
    evidence
        .related_tests
        .iter()
        .find(|test| {
            (!require_direct_owner
                || test.relation_reason == crate::domain::RelationReason::DirectOwnerCall)
                && (!require_test_target || test.test_target.is_some())
        })
        .map(|test| test.oracle_kind.clone())
}

fn oracle_for_seam(kind: SeamKind) -> OracleKind {
    match kind {
        SeamKind::ErrorVariant => OracleKind::ExactErrorVariant,
        SeamKind::SideEffect | SeamKind::CallPresence => OracleKind::MockExpectation,
        _ => OracleKind::ExactValue,
    }
}
