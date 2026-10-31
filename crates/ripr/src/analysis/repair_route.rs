//! Producer-owned repair-route readiness and repair-packet eligibility.
//!
//! This module decides whether the analyzer facts are sufficient to localize
//! one safe test-only repair. Output projections consume this result; they do
//! not infer test identity, effect sinks, or missing observations themselves.
//!
//! `repair_packet_eligibility` is the single authority for the
//! safe-for-repair-packet flip (`repair_packet_ready`). Consumers must route
//! through it (or its derived `is_safe_for_repair_packet`) instead of
//! hand-conjoining readiness, class, and cross-language predicates; the
//! actionability flip is fail-closed and every ineligible state carries a
//! typed reason.

pub(crate) use super::new_test_target::NewTestTargetAdmission;
use super::seam_classification::ClassifiedSeam;
use super::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind};
use super::test_grip_evidence::{RelatedTestGrip, TestGripEvidence, TestTargetEvidence};
use crate::analysis::canonical_gap::canonical_gap_identity;
use crate::domain::{OracleKind, OracleStrength, RelationReason, StageState};
use std::sync::Arc;

pub(crate) use super::new_test_target::{
    NewTestKind, NewTestProposalProvenance, NewTestTargetProposal,
};

pub(crate) const REPAIR_ROUTE_AUTHORITY_BOUNDARY: &str =
    "analysis/producer-owned-repair-route-readiness";
const PRODUCER_DISCRIMINATOR_EVIDENCE: &str = "producer-owned missing discriminator fact";
pub(crate) const SAFE_TEST_TARGET_EVIDENCE: &str = "safe test target";

/// Plain-language rendering of one readiness-evidence string for human
/// output surfaces only (#10644). A reader cannot act on the internal fact
/// code, so each known discriminator kind maps to the test evidence the
/// route still needs. Machine-readable readiness (card wire data, JSON
/// records) keeps the stable internal strings; consumers of those fields
/// never pass them through this mapping, and an unrecognized string passes
/// through unchanged rather than inventing a taxonomy.
pub(crate) fn humanize_evidence(evidence: &str) -> String {
    const FACT_PREFIX: &str = "producer-owned missing discriminator fact: ";
    // #10691: the internal "observable sink" noun becomes plain words on the
    // human screens; the machine list keeps the stable code.
    if evidence == "observable sink localization" {
        return "a way to follow the changed value to something a test can check".to_string();
    }
    // #11025: the stage the static chain stopped at, in plain words. The
    // machine list keeps the stable `incomplete evidence stage: <stage>` code;
    // a stage this table does not know passes through unchanged.
    if let Some(stage) = evidence.strip_prefix("incomplete evidence stage: ") {
        let phrase = match stage {
            "activation" => Some("a test input that reaches the changed code"),
            "propagation" => {
                Some("a test that follows the changed value through to something it can check")
            }
            "observation" => Some("a test that observes the changed behavior"),
            "discrimination" => {
                Some("a test assertion that would fail if the changed behavior were wrong")
            }
            "opaque" => {
                Some("code ripr cannot see into statically, so a test must check it by hand")
            }
            "unknown" => Some("more static evidence about the changed behavior"),
            _ => None,
        };
        if let Some(phrase) = phrase {
            return phrase.to_string();
        }
    }
    if let Some(kind) = evidence.strip_prefix(FACT_PREFIX) {
        let phrase = match kind {
            "boundary_value" => "a test assertion that pins the changed boundary value exactly",
            "error_variant" => "a test assertion that observes the changed error variant",
            "return_value" => "a test assertion that checks the changed return value",
            "field_value" => "a test assertion that checks the changed field value",
            "effect" => "a test that observes the changed effect",
            "match_arm_taken" => "a test assertion that pins which match arm runs",
            "call_site" => "a test that reaches the changed call",
            _ => "a test assertion that discriminates the changed behavior exactly",
        };
        phrase.to_string()
    } else {
        evidence.to_string()
    }
}

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
/// heuristic is not permission to edit that location. Ranking prefers an
/// admitted `Existing` target. A DirectOwnerCall related test that failed
/// target admission stays `Missing` rather than inventing a new test.
/// Advisory observers (`SameModule`, `WeakTokenSubstring`,
/// `ImportPathAffinity`) do not occupy that slot: a producer-owned
/// `Proposed` Integration or InlineUnit remains eligible when no suitable
/// existing target was admitted. `Proposed` is not an existing-test identity.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepairTargetSelection {
    Existing(TestTargetEvidence),
    Proposed(NewTestTargetProposal),
    Missing,
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

impl RepairRouteReadiness {
    pub(crate) fn is_repair_ready(&self) -> bool {
        self.state == RepairRouteState::Ready
            && !matches!(self.target_selection, RepairTargetSelection::Missing)
    }
}

/// Typed reason a seam is not eligible for a repair packet. The flip is
/// fail-closed: any state not explicitly mapped to eligible is ineligible,
/// and the first failing gate (in the fixed order below) is the reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RepairPacketIneligibility {
    /// Governance (`Intentional`/`Suppressed`), already-gripped, and
    /// non-headline classes are not targeted-test repair targets.
    ClassNotHeadlineEligible,
    /// A binding/FFI or external-language surface leaves the oracle path
    /// unresolved from Rust static evidence.
    CrossLanguageOracleVisibilityUnresolved,
    /// A binding/FFI or external-language surface leaves the safe repair
    /// test target unresolved.
    CrossLanguageTestTargetUnresolved,
    /// Producer-owned route readiness did not reach `Ready` with a
    /// selected target.
    RouteNotReady,
    /// The seam's evidence names a static limitation `check` reports as a
    /// limitation rather than a gap (today: a `#[should_panic]` test whose
    /// panic outcome is unresolved, #10703), so there is no gap to repair.
    NamedStaticLimitation,
}

/// Producer-owned repair-packet eligibility: the single authority for the
/// safe-for-repair-packet flip. Computed once from analyzer facts so no
/// output surface hand-conjoins readiness, class, and cross-language
/// predicates.
#[derive(Clone, Debug)]
pub(crate) struct RepairPacketEligibility {
    /// The underlying producer-owned route readiness.
    pub(crate) readiness: RepairRouteReadiness,
    /// First failing gate when the seam is not eligible; `None` only when
    /// every producer fact needed for a safe targeted-test repair packet
    /// is present.
    pub(crate) ineligibility: Option<RepairPacketIneligibility>,
}

impl RepairPacketEligibility {
    /// Fail-closed flip: true only when no ineligibility gate fired.
    pub(crate) fn eligible(&self) -> bool {
        self.ineligibility.is_none()
    }
}

pub(crate) fn repair_packet_eligibility(entry: &ClassifiedSeam) -> RepairPacketEligibility {
    let readiness = repair_route_readiness(entry);
    let oracle_unresolved = cross_language_oracle_visibility_unresolved(entry);
    let target_unresolved = cross_language_test_target_unresolved(entry);
    let ineligibility = if !entry.class.is_headline_eligible() {
        Some(RepairPacketIneligibility::ClassNotHeadlineEligible)
    } else if oracle_unresolved {
        Some(RepairPacketIneligibility::CrossLanguageOracleVisibilityUnresolved)
    } else if target_unresolved {
        Some(RepairPacketIneligibility::CrossLanguageTestTargetUnresolved)
    } else if entry
        .evidence
        .names_static_limit(crate::domain::StaticLimitKind::RustPanicOutcomeUnresolved)
    {
        Some(RepairPacketIneligibility::NamedStaticLimitation)
    } else if !readiness.is_repair_ready() {
        Some(RepairPacketIneligibility::RouteNotReady)
    } else {
        None
    };
    RepairPacketEligibility {
        readiness,
        ineligibility,
    }
}

pub(crate) fn is_safe_for_repair_packet(entry: &ClassifiedSeam) -> bool {
    repair_packet_eligibility(entry).eligible()
}

/// Whether the seam appears in the agent packet queue at all. This is the
/// queue-inclusion decision, not the repair flip: queued seams that are not
/// repair-packet eligible render as `inspect_static_limitation` packets.
///
/// Headline-eligible classes are the natural agent targets. `Opaque` is
/// also queued as `inspect_static_limitation`. `Intentional` and
/// `Suppressed` are governance classes; the agent should not be told to
/// repair them.
pub(crate) fn repair_packet_queue_visible(entry: &ClassifiedSeam) -> bool {
    (entry.class.is_headline_eligible() || matches!(entry.class, SeamGripClass::Opaque))
        && !cross_language_oracle_visibility_unresolved(entry)
        && !cross_language_test_target_unresolved(entry)
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

/// The missing/required-evidence string for a discriminator kind. The one
/// producer of this format; human refusal text that needs to recognise a kind
/// goes through [`is_boundary_value_discriminator_evidence`] instead of parsing
/// the string itself.
pub(crate) fn producer_discriminator_evidence(kind: &str) -> String {
    format!("{PRODUCER_DISCRIMINATOR_EVIDENCE}: {kind}")
}

/// The `ripr agent stub --kind` spelling for a missing discriminator item
/// whose gap has a working hand-written route, if any.
pub(crate) fn stub_kind_for_missing_evidence(item: &str) -> Option<&'static str> {
    let kind = item
        .strip_prefix(PRODUCER_DISCRIMINATOR_EVIDENCE)?
        .strip_prefix(": ")?;
    match kind {
        "return_value" => Some("return_value"),
        "error_variant" => Some("error_path"),
        "boundary_value" => Some("predicate"),
        _ => None,
    }
}

/// Whether a readiness evidence item names the boundary-value discriminator.
pub(crate) fn is_boundary_value_discriminator_evidence(item: &str) -> bool {
    item == producer_discriminator_evidence(
        RequiredDiscriminator::BoundaryValue {
            description: String::new(),
        }
        .as_str(),
    )
}

/// The seam-side twin of `check`'s boundary-observed weak-oracle gate (#10683;
/// `domain::boundary_input_observed_with_weak_oracle` is the finding side): a
/// weakly gripped predicate boundary whose activation is observed, with no
/// missing boundary fact left to name, and whose only credited related oracle
/// is a weak check on a direct, admitted existing Rust test. Returns that test:
/// the remaining work is strengthening its assertion, not writing a new test.
/// Guidance only; repair eligibility is decided by `repair_packet_eligibility`
/// and stays fail-closed (this seam has no discriminator fact, so it is never
/// repair-ready).
pub(crate) fn boundary_observed_weak_oracle_test(
    entry: &ClassifiedSeam,
) -> Option<&RelatedTestGrip> {
    let evidence = &entry.evidence;
    if entry.seam.kind() != SeamKind::PredicateBoundary
        || entry.class != SeamGripClass::WeaklyGripped
        || evidence.activate.state != StageState::Yes
        || !evidence.missing_discriminators.is_empty()
        // Parity with the finding-side gate: the discriminate stage must
        // itself grade the oracle weak.
        || evidence.discriminate.state != StageState::Weak
        || cross_language_oracle_visibility_unresolved(entry)
        || cross_language_test_target_unresolved(entry)
        // The activation stage aggregates observed boundary values across all
        // related tests, so with several related tests the observer of the
        // boundary input may not be the weakly asserting one. Name a test only
        // when it is the sole related test, so the observer is unambiguous;
        // otherwise withhold the claim (previous guidance applies).
        || evidence.related_tests.len() != 1
        || evidence.related_tests.iter().any(|test| {
            matches!(
                test.oracle_strength,
                OracleStrength::Strong | OracleStrength::Medium
            )
        })
    {
        return None;
    }
    evidence
        .related_tests
        .iter()
        .find(|test| {
            test.relation_reason == RelationReason::DirectOwnerCall
                && test.test_target.is_some()
                && test.oracle_strength == OracleStrength::Weak
        })
        .map(Arc::as_ref)
}

fn value_route_readiness(seam: &RepoSeam, evidence: &TestGripEvidence) -> RepairRouteReadiness {
    let discriminator_evidence =
        producer_discriminator_evidence(seam.required_discriminator().as_str());
    let required = vec![
        discriminator_evidence.clone(),
        SAFE_TEST_TARGET_EVIDENCE.to_string(),
    ];
    let has_discriminator = has_exact_discriminator(seam, evidence);
    let selected_test_target = existing_test_target(evidence, false);
    let admission = evidence.new_test_target.as_ref();
    let target_selection = value_target_selection(
        selected_test_target.as_ref(),
        admission,
        &evidence.related_tests,
    );
    let has_safe_target = !matches!(target_selection, RepairTargetSelection::Missing);
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
        if matches!(target_selection, RepairTargetSelection::Proposed(_))
            && let Some(admission) = admission
        {
            present_evidence.push(admission.present_reason().to_string());
        }
    }
    let mut missing_evidence = Vec::new();
    if !has_discriminator {
        missing_evidence.push(discriminator_evidence);
    }
    if !has_safe_target {
        missing_evidence.push(SAFE_TEST_TARGET_EVIDENCE.to_string());
        if let Some(reason) = admission.and_then(NewTestTargetAdmission::missing_reason) {
            missing_evidence.push(reason);
        }
    }
    RepairRouteReadiness {
        state,
        seam_id: String::new(),
        canonical_gap_id: None,
        required_evidence: required,
        present_evidence,
        missing_evidence,
        target_selection,
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
    let target_related = direct_owner_related_test_with_target(evidence);
    let test_target = target_related.and_then(|test| test.test_target.clone());
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

    let current = target_related.map(|test| test.oracle_kind.clone());
    let current_strength = target_related.map(|test| test.oracle_strength.clone());
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum DiscriminatorCompatibilityKey {
    Comparison {
        left: String,
        operator: String,
        right: String,
    },
    EqualityBoundary {
        right: String,
    },
    Exact {
        kind: &'static str,
        value: String,
    },
}

fn discriminator_fact_matches(required: &RequiredDiscriminator, fact: &str) -> bool {
    let Some(required_key) = required_discriminator_key(required) else {
        return false;
    };
    let Some(fact_key) = fact_discriminator_key(required, fact) else {
        return false;
    };
    match (required_key, fact_key) {
        (
            DiscriminatorCompatibilityKey::Comparison {
                left: required_left,
                operator: required_operator,
                right: required_right,
            },
            DiscriminatorCompatibilityKey::Comparison {
                left: fact_left,
                operator: fact_operator,
                right: fact_right,
            },
        ) if required_operator == fact_operator => {
            required_left == fact_left && required_right == fact_right
        }
        (
            DiscriminatorCompatibilityKey::Comparison {
                left: required_left,
                right: required_right,
                ..
            },
            DiscriminatorCompatibilityKey::Comparison {
                left: fact_left,
                operator: fact_operator,
                right: fact_right,
            },
        ) => fact_operator == "==" && required_left == fact_left && required_right == fact_right,
        (
            DiscriminatorCompatibilityKey::Comparison { right, .. },
            DiscriminatorCompatibilityKey::EqualityBoundary { right: fact_right },
        ) => right == fact_right,
        (
            DiscriminatorCompatibilityKey::Exact {
                kind: required_kind,
                value: required_value,
            },
            DiscriminatorCompatibilityKey::Exact {
                kind: fact_kind,
                value: fact_value,
            },
        ) => required_kind == fact_kind && required_value == fact_value,
        _ => false,
    }
}

fn required_discriminator_key(
    required: &RequiredDiscriminator,
) -> Option<DiscriminatorCompatibilityKey> {
    match required {
        RequiredDiscriminator::BoundaryValue { description } => {
            let (left, operator, right) = comparison_parts(description)?;
            Some(DiscriminatorCompatibilityKey::Comparison {
                left,
                operator,
                right,
            })
        }
        RequiredDiscriminator::ReturnValue { description } => {
            exact_key("return_value", description)
        }
        RequiredDiscriminator::ErrorVariant { variant } => exact_key("error_variant", variant),
        RequiredDiscriminator::MatchArmTaken { arm } => exact_key("match_arm", arm),
        RequiredDiscriminator::FieldValue { field } => exact_key("field_value", field),
        RequiredDiscriminator::Effect { sink } => exact_key("effect", sink),
        RequiredDiscriminator::CallSite { target } => exact_key("call_site", target),
    }
}

fn fact_discriminator_key(
    required: &RequiredDiscriminator,
    fact: &str,
) -> Option<DiscriminatorCompatibilityKey> {
    match required {
        RequiredDiscriminator::BoundaryValue { .. } => {
            if let Some((left, operator, right)) = comparison_parts(fact) {
                return Some(DiscriminatorCompatibilityKey::Comparison {
                    left,
                    operator,
                    right,
                });
            }
            let trimmed = fact.trim();
            let right = trimmed
                .strip_suffix(" (equality boundary)")
                .or_else(|| trimmed.strip_suffix(" (boundary value)"))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(normalize_discriminator_text)?;
            Some(DiscriminatorCompatibilityKey::EqualityBoundary { right })
        }
        RequiredDiscriminator::ReturnValue { .. } => exact_key("return_value", fact),
        RequiredDiscriminator::ErrorVariant { .. } => exact_key("error_variant", fact),
        RequiredDiscriminator::MatchArmTaken { .. } => exact_key("match_arm", fact),
        RequiredDiscriminator::FieldValue { .. } => exact_key("field_value", fact),
        RequiredDiscriminator::Effect { .. } => exact_key("effect", fact),
        RequiredDiscriminator::CallSite { .. } => exact_key("call_site", fact),
    }
}

fn exact_key(kind: &'static str, value: &str) -> Option<DiscriminatorCompatibilityKey> {
    let value = normalize_discriminator_text(value);
    (!value.is_empty()).then_some(DiscriminatorCompatibilityKey::Exact { kind, value })
}

fn comparison_parts(value: &str) -> Option<(String, String, String)> {
    for operator in [" >= ", " <= ", " == ", " != ", " > ", " < "] {
        if let Some((left, right)) = value.split_once(operator) {
            let left = normalize_discriminator_text(left);
            let right = normalize_discriminator_text(right);
            if !left.is_empty() && !right.is_empty() {
                return Some((left, operator.trim().to_string(), right));
            }
        }
    }
    None
}

fn normalize_discriminator_text(value: &str) -> String {
    // Source identities are case-sensitive, including literal contents.
    // Only surrounding formatting whitespace may be discarded.
    value.trim().to_string()
}

fn direct_owner_related_test(evidence: &TestGripEvidence) -> Option<&RelatedTestGrip> {
    evidence
        .related_tests
        .iter()
        .find(|test| test.relation_reason == crate::domain::RelationReason::DirectOwnerCall)
        .map(Arc::as_ref)
}

fn direct_owner_related_test_with_target(evidence: &TestGripEvidence) -> Option<&RelatedTestGrip> {
    evidence
        .related_tests
        .iter()
        .find(|test| {
            test.relation_reason == crate::domain::RelationReason::DirectOwnerCall
                && test.test_target.is_some()
        })
        .map(Arc::as_ref)
}

fn value_target_selection(
    existing: Option<&TestTargetEvidence>,
    admission: Option<&NewTestTargetAdmission>,
    related_tests: &[Arc<RelatedTestGrip>],
) -> RepairTargetSelection {
    if let Some(existing) = existing {
        return RepairTargetSelection::Existing(existing.clone());
    }
    // A DirectOwnerCall observer that failed target admission occupies the
    // existing-test slot. Falling through to Proposed would keep a repair
    // packet after authority refused the test we already found. Advisory
    // relations never supplied a suitable Existing target, so they must not
    // block an independently admitted producer-owned proposal.
    if related_tests.iter().any(|test| {
        test.relation_reason == RelationReason::DirectOwnerCall && test.test_target.is_none()
    }) {
        return RepairTargetSelection::Missing;
    }
    if let Some(proposal) = admission.and_then(|admission| admission.proposal.clone()) {
        if !matches!(
            proposal.kind,
            NewTestKind::Integration | NewTestKind::InlineUnit
        ) || proposal.provenance != NewTestProposalProvenance::ProducerOwned
        {
            return RepairTargetSelection::Missing;
        }
        return RepairTargetSelection::Proposed(proposal);
    }
    RepairTargetSelection::Missing
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

/// Whether a binding/FFI or external-language surface leaves the oracle
/// path unresolved from Rust static evidence. Pure producer fact over the
/// classified seam; moved up from `output::evidence_record` so the
/// repair-packet eligibility authority owns it.
pub(crate) fn cross_language_oracle_visibility_unresolved(entry: &ClassifiedSeam) -> bool {
    if !entry.class.is_headline_eligible() && !matches!(entry.class, SeamGripClass::Opaque) {
        return false;
    }

    if has_external_language_related_test(entry) {
        return true;
    }

    cross_language_surface_hint(entry) && !has_rust_related_test(entry)
}

/// Whether a binding/FFI or external-language surface leaves the safe
/// repair test target unresolved. Pure producer fact over the classified
/// seam; moved up from `output::evidence_record` so the repair-packet
/// eligibility authority owns it.
pub(crate) fn cross_language_test_target_unresolved(entry: &ClassifiedSeam) -> bool {
    if !entry.class.is_headline_eligible() && !matches!(entry.class, SeamGripClass::Opaque) {
        return false;
    }

    (has_external_language_related_test(entry) || cross_language_surface_hint(entry))
        && !has_rust_side_target_context(entry)
}

fn has_rust_related_test(entry: &ClassifiedSeam) -> bool {
    entry
        .evidence
        .related_tests
        .iter()
        .any(|test| related_test_is_rust(test))
}

fn has_rust_side_target_context(entry: &ClassifiedSeam) -> bool {
    entry.evidence.related_tests.iter().any(|test| {
        related_test_is_rust(test)
            && matches!(
                test.relation_reason,
                RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall
            )
    })
}

fn related_test_is_rust(test: &RelatedTestGrip) -> bool {
    test.file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
}

fn has_external_language_related_test(entry: &ClassifiedSeam) -> bool {
    entry.evidence.related_tests.iter().any(|test| {
        test.file
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(is_external_language_extension)
    })
}

// One shared inventory, not a private list: #4094 routed `.mts`/`.cts` into the
// TypeScript adapter, so a modern related test is cross-language evidence here
// exactly like a `.ts` one. The TS/JS family consumes the routed extension
// authority from `analysis::language::router`, the same owner the router itself
// and the agent packet projection read; the remaining bridge languages keep
// their historical lowercase fold here. See
// `tests::external_language_test_extensions_cover_bridge_language_families` and
// `tests::modern_ts_related_tests_keep_the_repair_packet_fail_closed`.
fn is_external_language_extension(extension: &str) -> bool {
    let extension = extension.to_ascii_lowercase();
    // #4116: the TS/JS family consumes the shared extension authority; the
    // remaining bridge languages keep their historical lowercase fold.
    crate::analysis::is_ts_js_source_extension(&extension)
        || matches!(
            extension.as_str(),
            "py" | "rb" | "java" | "c" | "cc" | "cpp" | "cxx" | "swift" | "kt" | "kts"
        )
}

fn cross_language_surface_hint(entry: &ClassifiedSeam) -> bool {
    // Stable slash separators, identical to `output::path::display_path`;
    // inlined here so `analysis` does not depend on `output`.
    let file = entry
        .seam
        .file()
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let owner = entry.seam.owner().to_ascii_lowercase();
    let expression = entry.seam.expression().to_ascii_lowercase();

    path_has_cross_language_segment(&file)
        || text_has_cross_language_marker(&owner)
        || text_has_cross_language_marker(&expression)
}

fn path_has_cross_language_segment(file: &str) -> bool {
    let normalized = file.replace('\\', "/");
    normalized.split('/').any(|segment| {
        matches!(
            segment,
            "ffi"
                | "binding"
                | "bindings"
                | "napi"
                | "neon"
                | "wasm"
                | "wasm_bindgen"
                | "pyo3"
                | "python"
                | "jni"
                | "uniffi"
                | "cxx"
                | "node"
                | "jsc"
                | "javascriptcore"
        )
    })
}

fn text_has_cross_language_marker(text: &str) -> bool {
    const TOKEN_MARKERS: &[&str] = &[
        "from_js",
        "to_js",
        "jsvalue",
        "js_value",
        "napi",
        "wasm_bindgen",
        "extern_c",
        "no_mangle",
        "export_name",
        "ffi",
        "pyo3",
        "node_api",
        "jni",
        "uniffi",
    ];
    let has_token_marker = text
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .any(|token| {
            TOKEN_MARKERS.iter().any(|marker| {
                token == *marker
                    || token.starts_with(marker)
                        && token.as_bytes().get(marker.len()) == Some(&b'_')
            })
        });

    has_token_marker
        || text.contains("jsc::")
        || text.contains("javascriptcore")
        || text.contains("extern \"c\"")
        || text.contains("cxx::bridge")
        || text.starts_with("python::")
        || text.contains("::python::")
}

#[cfg(test)]
mod tests {
    mod case_identity;

    use super::{
        ClassifiedSeam, RepairPacketIneligibility, RepairTargetSelection,
        cross_language_oracle_visibility_unresolved, discriminator_fact_matches, humanize_evidence,
        is_safe_for_repair_packet, repair_packet_eligibility, repair_packet_queue_visible,
    };
    use crate::analysis::new_test_target::{
        NewTestKind, NewTestProposalProvenance, NewTestTargetAdmission, NewTestTargetProposal,
    };
    use crate::analysis::seams::{
        ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind,
    };
    use crate::analysis::test_grip_evidence::{
        RelatedTestGrip, RelationConfidence, TestGripEvidence, TestTargetEvidence,
    };
    use crate::domain::{
        Confidence, MissingDiscriminatorFact, OracleKind, OracleStrength, RelationReason,
        StageEvidence, StageState,
    };
    use std::path::{Path, PathBuf};

    fn stage(state: StageState) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, "test stage")
    }

    fn boundary_seam() -> RepoSeam {
        RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            42,
            88,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        )
    }

    fn rust_related_test(relation_reason: RelationReason) -> RelatedTestGrip {
        RelatedTestGrip {
            test_name: "below_threshold_has_no_discount".to_string(),
            file: PathBuf::from("tests/pricing.rs"),
            line: 12,
            test_target: Some(TestTargetEvidence::fixture(
                "below_threshold_has_no_discount",
                Path::new("tests/pricing.rs"),
                12,
            )),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            evidence_summary: "exact value assertion".to_string(),
            relation_reason,
            relation_confidence: RelationConfidence::High,
        }
    }

    fn external_related_test() -> RelatedTestGrip {
        RelatedTestGrip {
            test_name: "fetch blob resizes".to_string(),
            file: PathBuf::from("test/js/web/fetch/blob.test.ts"),
            line: 9,
            test_target: Some(TestTargetEvidence::fixture(
                "fetch blob resizes",
                Path::new("test/js/web/fetch/blob.test.ts"),
                9,
            )),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            evidence_summary: "external observer assertion".to_string(),
            relation_reason: RelationReason::DirectOwnerCall,
            relation_confidence: RelationConfidence::High,
        }
    }

    fn classified_with(
        seam: RepoSeam,
        class: SeamGripClass,
        related_tests: Vec<RelatedTestGrip>,
    ) -> ClassifiedSeam {
        let seam_id = seam.id().clone();
        ClassifiedSeam {
            seam,
            evidence: TestGripEvidence {
                seam_id,
                related_tests: related_tests.into_iter().map(std::sync::Arc::new).collect(),
                reach: stage(StageState::Yes),
                activate: stage(StageState::Yes),
                propagate: stage(StageState::Yes),
                observe: stage(StageState::Yes),
                discriminate: stage(StageState::Yes),
                observed_values: Vec::new(),
                missing_discriminators: vec![MissingDiscriminatorFact {
                    value: "discount_threshold (equality boundary)".to_string(),
                    reason: "observed values do not include the equality-boundary case".to_string(),
                    flow_sink: None,
                }],
                new_test_target: None,
                statically_contradicted_related_tests: 0,
            },
            class,
        }
    }

    fn inline_unit_admission() -> NewTestTargetAdmission {
        NewTestTargetAdmission {
            proposal: Some(NewTestTargetProposal {
                kind: NewTestKind::InlineUnit,
                file: PathBuf::from("src/pricing.rs"),
                owner: "pricing::discounted_total".to_string(),
                provenance: NewTestProposalProvenance::ProducerOwned,
            }),
            region: None,
            blocker: None,
            owner_inline_region: None,
        }
    }

    #[test]
    fn humanize_evidence_maps_each_discriminator_kind_to_plain_words() -> Result<(), String> {
        let cases = [
            (
                "producer-owned missing discriminator fact: boundary_value",
                "a test assertion that pins the changed boundary value exactly",
            ),
            (
                "producer-owned missing discriminator fact: error_variant",
                "a test assertion that observes the changed error variant",
            ),
            (
                "producer-owned missing discriminator fact: return_value",
                "a test assertion that checks the changed return value",
            ),
            (
                "producer-owned missing discriminator fact: field_value",
                "a test assertion that checks the changed field value",
            ),
            (
                "producer-owned missing discriminator fact: effect",
                "a test that observes the changed effect",
            ),
            (
                "producer-owned missing discriminator fact: match_arm_taken",
                "a test assertion that pins which match arm runs",
            ),
            (
                "producer-owned missing discriminator fact: call_site",
                "a test that reaches the changed call",
            ),
            // #10691: the internal "observable sink" noun is rendered in words.
            (
                "observable sink localization",
                "a way to follow the changed value to something a test can check",
            ),
        ];
        for (internal, plain) in cases {
            if humanize_evidence(internal) != plain {
                return Err(format!(
                    "humanize_evidence({internal:?}) must be {plain:?}, got {:?}",
                    humanize_evidence(internal)
                ));
            }
            if humanize_evidence(internal).contains("producer-owned") {
                return Err(format!(
                    "humanized text leaks the internal phrase: {internal}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn humanize_evidence_maps_each_evidence_stage_to_plain_words() -> Result<(), String> {
        let cases = [
            ("activation", "a test input that reaches the changed code"),
            (
                "propagation",
                "a test that follows the changed value through to something it can check",
            ),
            ("observation", "a test that observes the changed behavior"),
            (
                "discrimination",
                "a test assertion that would fail if the changed behavior were wrong",
            ),
            (
                "opaque",
                "code ripr cannot see into statically, so a test must check it by hand",
            ),
            ("unknown", "more static evidence about the changed behavior"),
        ];
        for (stage, plain) in cases {
            let internal = format!("incomplete evidence stage: {stage}");
            let got = humanize_evidence(&internal);
            if got != plain || got.contains("evidence stage") {
                return Err(format!("{internal:?} must render {plain:?}, got {got:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn humanize_evidence_falls_back_and_passes_non_fact_strings_through() -> Result<(), String> {
        let unknown = humanize_evidence("producer-owned missing discriminator fact: future_kind");
        if unknown.contains("producer-owned") || unknown.contains("future_kind") {
            return Err(format!(
                "unknown kind must not leak internal spelling, got {unknown:?}"
            ));
        }
        // #11025: a stage this table does not know passes through unchanged.
        for already_plain in [
            "safe test target",
            "incomplete evidence stage: future_stage",
        ] {
            if humanize_evidence(already_plain) != already_plain {
                return Err(format!(
                    "non-fact evidence must pass through unchanged, got {:?}",
                    humanize_evidence(already_plain)
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn readiness_data_keeps_internal_strings_for_machine_consumers() -> Result<(), String> {
        // #10644 control: the humanizer is a render-boundary translation
        // only; the producer-owned readiness vectors that JSON and card wire
        // data carry must keep the stable internal strings.
        let mut entry = classified_with(boundary_seam(), SeamGripClass::WeaklyGripped, Vec::new());
        entry.evidence.new_test_target = Some(inline_unit_admission());
        let readiness = repair_packet_eligibility(&entry).readiness;
        if !readiness
            .required_evidence
            .iter()
            .any(|evidence| evidence.starts_with("producer-owned missing discriminator fact"))
        {
            return Err(format!(
                "machine readiness must keep the internal string: {:?}",
                readiness.required_evidence
            ));
        }
        Ok(())
    }

    #[test]
    fn producer_owned_inline_unit_proposal_is_a_safe_target() -> Result<(), String> {
        let mut entry = classified_with(boundary_seam(), SeamGripClass::WeaklyGripped, Vec::new());
        entry.evidence.new_test_target = Some(inline_unit_admission());
        let eligibility = repair_packet_eligibility(&entry);
        match &eligibility.readiness.target_selection {
            RepairTargetSelection::Proposed(proposal) => {
                assert_eq!(proposal.kind, NewTestKind::InlineUnit);
            }
            other => return Err(format!("expected Proposed InlineUnit, got {other:?}")),
        }
        assert!(eligibility.readiness.is_repair_ready());
        assert!(eligibility.eligible());
        Ok(())
    }

    #[test]
    fn existing_target_stays_preferred_when_an_inline_proposal_is_also_present()
    -> Result<(), String> {
        let mut entry = classified_with(
            boundary_seam(),
            SeamGripClass::WeaklyGripped,
            vec![rust_related_test(RelationReason::DirectOwnerCall)],
        );
        entry.evidence.new_test_target = Some(inline_unit_admission());
        match repair_packet_eligibility(&entry).readiness.target_selection {
            RepairTargetSelection::Existing(_) => Ok(()),
            other => Err(format!("Existing must outrank Proposed, got {other:?}")),
        }
    }

    #[test]
    fn refused_direct_owner_target_does_not_fall_through_to_proposed() -> Result<(), String> {
        let mut related = rust_related_test(RelationReason::DirectOwnerCall);
        related.test_target = None;
        let mut entry =
            classified_with(boundary_seam(), SeamGripClass::WeaklyGripped, vec![related]);
        entry.evidence.new_test_target = Some(inline_unit_admission());
        let eligibility = repair_packet_eligibility(&entry);
        match eligibility.readiness.target_selection {
            RepairTargetSelection::Missing => {}
            other => {
                return Err(format!(
                    "refused DirectOwnerCall must stay Missing, not fall through to {other:?}"
                ));
            }
        }
        if eligibility.eligible() || eligibility.readiness.is_repair_ready() {
            return Err(
                "failed existing-target authority must not keep a repair packet".to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn advisory_related_observer_does_not_block_producer_owned_proposal() -> Result<(), String> {
        for reason in [
            RelationReason::WeakTokenSubstring,
            RelationReason::SameModule,
            RelationReason::ImportPathAffinity,
        ] {
            let mut related = rust_related_test(reason);
            related.test_target = None;
            let mut entry =
                classified_with(boundary_seam(), SeamGripClass::WeaklyGripped, vec![related]);
            entry.evidence.new_test_target = Some(inline_unit_admission());
            let eligibility = repair_packet_eligibility(&entry);
            match &eligibility.readiness.target_selection {
                RepairTargetSelection::Proposed(proposal)
                    if proposal.kind == NewTestKind::InlineUnit => {}
                other => {
                    return Err(format!(
                        "{reason:?}: advisory observer must not block Proposed, got {other:?}"
                    ));
                }
            }
            if !eligibility.eligible() || !eligibility.readiness.is_repair_ready() {
                return Err(format!(
                    "{reason:?}: independently admitted proposal must stay eligible"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn producer_owned_integration_proposal_is_a_safe_target() -> Result<(), String> {
        let mut entry = classified_with(boundary_seam(), SeamGripClass::WeaklyGripped, Vec::new());
        let mut admission = inline_unit_admission();
        if let Some(proposal) = admission.proposal.as_mut() {
            proposal.kind = NewTestKind::Integration;
            proposal.file = PathBuf::from("tests/pricing.rs");
        }
        entry.evidence.new_test_target = Some(admission);
        match repair_packet_eligibility(&entry).readiness.target_selection {
            RepairTargetSelection::Proposed(proposal) => {
                assert_eq!(proposal.kind, NewTestKind::Integration);
                Ok(())
            }
            other => Err(format!("expected Proposed Integration, got {other:?}")),
        }
    }

    #[test]
    fn repair_ready_headline_seam_is_safe_for_repair_packet() -> Result<(), String> {
        let entry = classified_with(
            boundary_seam(),
            SeamGripClass::WeaklyGripped,
            vec![rust_related_test(RelationReason::DirectOwnerCall)],
        );

        let eligibility = repair_packet_eligibility(&entry);
        assert!(eligibility.eligible());
        assert_eq!(eligibility.ineligibility, None);
        assert!(is_safe_for_repair_packet(&entry));
        assert!(eligibility.readiness.is_repair_ready());
        assert!(repair_packet_queue_visible(&entry));
        Ok(())
    }

    #[test]
    fn headline_seam_without_route_readiness_is_fail_closed() -> Result<(), String> {
        let entry = classified_with(boundary_seam(), SeamGripClass::WeaklyGripped, Vec::new());

        let eligibility = repair_packet_eligibility(&entry);
        assert!(!eligibility.eligible());
        assert_eq!(
            eligibility.ineligibility,
            Some(RepairPacketIneligibility::RouteNotReady)
        );
        assert!(!is_safe_for_repair_packet(&entry));
        assert!(repair_packet_queue_visible(&entry));
        Ok(())
    }

    #[test]
    fn non_headline_classes_are_fail_closed_with_typed_reason() -> Result<(), String> {
        for class in [
            SeamGripClass::StronglyGripped,
            SeamGripClass::Intentional,
            SeamGripClass::Suppressed,
            SeamGripClass::Opaque,
        ] {
            let entry = classified_with(
                boundary_seam(),
                class,
                vec![rust_related_test(RelationReason::DirectOwnerCall)],
            );
            let eligibility = repair_packet_eligibility(&entry);
            assert!(!eligibility.eligible());
            assert_eq!(
                eligibility.ineligibility,
                Some(RepairPacketIneligibility::ClassNotHeadlineEligible)
            );
        }

        // Queue visibility still matches the historical packet inclusion
        // rule: `Opaque` is queued, governance and gripped classes are not.
        let opaque = classified_with(boundary_seam(), SeamGripClass::Opaque, Vec::new());
        assert!(repair_packet_queue_visible(&opaque));
        let gripped = classified_with(boundary_seam(), SeamGripClass::StronglyGripped, Vec::new());
        assert!(!repair_packet_queue_visible(&gripped));
        let intentional = classified_with(boundary_seam(), SeamGripClass::Intentional, Vec::new());
        assert!(!repair_packet_queue_visible(&intentional));
        Ok(())
    }

    #[test]
    fn external_language_oracle_surface_is_fail_closed_even_when_route_ready() -> Result<(), String>
    {
        let entry = classified_with(
            boundary_seam(),
            SeamGripClass::WeaklyGripped,
            vec![external_related_test()],
        );

        let eligibility = repair_packet_eligibility(&entry);
        // Plain route readiness alone would have flipped this packet; the
        // authority must stay fail-closed on the cross-language surface.
        assert!(eligibility.readiness.is_repair_ready());
        assert!(!eligibility.eligible());
        assert_eq!(
            eligibility.ineligibility,
            Some(RepairPacketIneligibility::CrossLanguageOracleVisibilityUnresolved)
        );
        assert!(!is_safe_for_repair_packet(&entry));
        assert!(!repair_packet_queue_visible(&entry));
        Ok(())
    }

    /// Every routed TypeScript/JavaScript extension, including the modern
    /// `.mts`/`.cts` suffixes from #4094, must stay visible to the
    /// fail-closed cross-language gate. `.mts`/`.cts` are the discriminating
    /// rows: without the shared external-language authority this seam looks
    /// repair-ready while its only related test is a TypeScript observer.
    #[test]
    fn external_language_test_extensions_cover_bridge_language_families() -> Result<(), String> {
        for extension in [
            "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "py", "rb", "java", "c", "cc",
            "cpp", "cxx", "swift", "kt", "kts",
        ] {
            let mut related_test = external_related_test();
            related_test.file = PathBuf::from(format!("tests/bridge.{extension}"));
            let entry = classified_with(
                boundary_seam(),
                SeamGripClass::WeaklyGripped,
                vec![related_test],
            );

            assert!(
                cross_language_oracle_visibility_unresolved(&entry),
                "expected .{extension} related test to remain visible to the external-language gate"
            );
        }

        // Near-miss suffixes stay unknown: admitting routed extensions must not
        // become broad suffix matching, or an unrelated file would block repair.
        for extension in ["rs", "txt", "mt", "mjsx", "ctsx"] {
            let mut related_test = external_related_test();
            related_test.file = PathBuf::from(format!("tests/bridge.{extension}"));
            let entry = classified_with(
                boundary_seam(),
                SeamGripClass::WeaklyGripped,
                vec![related_test],
            );

            assert!(
                !cross_language_oracle_visibility_unresolved(&entry),
                "did not expect .{extension} related test to trigger the external-language gate"
            );
        }

        Ok(())
    }

    /// End-to-end control for the modern suffixes: a `.mts` related test must
    /// produce the same fail-closed ineligibility a `.ts` one does, with the
    /// named reason. #4116/#4120 routed `.mts`/`.cts` through the shared
    /// extension authority; before that the entry reported repair-ready, which
    /// is the fail-open this pin guards. `external_language_test_extensions_
    /// cover_bridge_language_families` pins the extension list itself, so this
    /// test is the one that fails if the gate stops consulting it.
    #[test]
    fn modern_ts_related_tests_keep_the_repair_packet_fail_closed() -> Result<(), String> {
        for extension in ["ts", "mts", "cts", "tsx"] {
            let mut related_test = external_related_test();
            related_test.file = PathBuf::from(format!("tests/bridge.{extension}"));
            let entry = classified_with(
                boundary_seam(),
                SeamGripClass::WeaklyGripped,
                vec![related_test],
            );

            let eligibility = repair_packet_eligibility(&entry);
            assert_eq!(
                eligibility.ineligibility,
                Some(RepairPacketIneligibility::CrossLanguageOracleVisibilityUnresolved),
                ".{extension} related test must block the packet with the cross-language reason"
            );
            assert!(
                !is_safe_for_repair_packet(&entry),
                ".{extension} related test must not be repair-ready"
            );
        }
        Ok(())
    }

    #[test]
    fn generic_binding_and_python_words_do_not_hide_a_plain_rust_route() -> Result<(), String> {
        let seam = RepoSeam::new(
            "src/metrics.rs",
            "metrics::python_binding_count",
            SeamKind::PredicateBoundary,
            42,
            88,
            "python_binding_count >= binding_total",
            RequiredDiscriminator::BoundaryValue {
                description: "python_binding_count >= binding_total".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let entry = classified_with(seam, SeamGripClass::WeaklyGripped, Vec::new());

        assert!(!cross_language_oracle_visibility_unresolved(&entry));
        Ok(())
    }

    #[test]
    fn unqualified_python_identifier_does_not_hide_a_plain_rust_route() -> Result<(), String> {
        let seam = RepoSeam::new(
            "src/metrics.rs",
            "metrics::python",
            SeamKind::PredicateBoundary,
            42,
            88,
            "value >= threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "value >= threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let entry = classified_with(seam, SeamGripClass::WeaklyGripped, Vec::new());

        assert!(!cross_language_oracle_visibility_unresolved(&entry));
        Ok(())
    }

    #[test]
    fn snake_case_bridge_markers_keep_same_test_file_routes_fail_closed() -> Result<(), String> {
        for owner in [
            "metrics::ffi_read",
            "metrics::pyo3_parse",
            "metrics::napi_call",
            "metrics::jni_handle",
            "metrics::uniffi_parse",
            "metrics::wasm_bindgen_bridge",
        ] {
            let seam = RepoSeam::new(
                "src/metrics.rs",
                owner,
                SeamKind::PredicateBoundary,
                42,
                88,
                "value >= discount_threshold",
                RequiredDiscriminator::BoundaryValue {
                    description: "value >= discount_threshold".to_string(),
                },
                ExpectedSink::ReturnValue,
            );
            let entry = classified_with(
                seam,
                SeamGripClass::WeaklyGripped,
                vec![rust_related_test(RelationReason::SameTestFile)],
            );

            let eligibility = repair_packet_eligibility(&entry);
            assert!(eligibility.readiness.is_repair_ready());
            assert_eq!(
                eligibility.ineligibility,
                Some(RepairPacketIneligibility::CrossLanguageTestTargetUnresolved),
                "expected {owner} to retain the fail-closed target limitation"
            );
            assert!(!eligibility.eligible());
        }

        Ok(())
    }

    #[test]
    fn specialized_cross_language_markers_still_block_without_external_tests() -> Result<(), String>
    {
        let seam = RepoSeam::new(
            "src/metrics.rs",
            "metrics::from_js_boundary",
            SeamKind::PredicateBoundary,
            42,
            88,
            "from_js_boundary >= threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "from_js_boundary >= threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let entry = classified_with(seam, SeamGripClass::WeaklyGripped, Vec::new());

        assert!(cross_language_oracle_visibility_unresolved(&entry));
        Ok(())
    }

    #[test]
    fn exact_python_namespace_marker_still_blocks_without_external_tests() -> Result<(), String> {
        let seam = RepoSeam::new(
            "src/metrics.rs",
            "python::parse_record",
            SeamKind::PredicateBoundary,
            42,
            88,
            "record.value >= threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "record.value >= threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let entry = classified_with(seam, SeamGripClass::WeaklyGripped, Vec::new());

        assert!(cross_language_oracle_visibility_unresolved(&entry));
        Ok(())
    }

    #[test]
    fn cross_language_hint_without_rust_target_context_blocks_the_flip() -> Result<(), String> {
        let seam = RepoSeam::new(
            "src/ffi/bindings.rs",
            "ffi::bindings::discounted_total",
            SeamKind::PredicateBoundary,
            42,
            88,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        // A Rust related test resolves oracle visibility, but without a
        // direct-owner/helper relation there is no Rust-side target context.
        let entry = classified_with(
            seam,
            SeamGripClass::WeaklyGripped,
            vec![rust_related_test(RelationReason::SameTestFile)],
        );

        let eligibility = repair_packet_eligibility(&entry);
        assert!(!eligibility.eligible());
        assert_eq!(
            eligibility.ineligibility,
            Some(RepairPacketIneligibility::CrossLanguageTestTargetUnresolved)
        );
        assert!(!is_safe_for_repair_packet(&entry));
        assert!(!repair_packet_queue_visible(&entry));
        Ok(())
    }

    #[test]
    fn boundary_matching_requires_the_same_operator_and_operands() {
        let required = RequiredDiscriminator::BoundaryValue {
            description: "amount >= discount_threshold".to_string(),
        };

        assert!(discriminator_fact_matches(
            &required,
            "discount_threshold (equality boundary)"
        ));
        assert!(!discriminator_fact_matches(
            &required,
            "amount != discount_threshold"
        ));
        assert!(!discriminator_fact_matches(
            &required,
            "discount_threshold_cache_key (equality boundary)"
        ));
    }

    #[test]
    fn boundary_matching_accepts_same_operands_from_equality_fact() {
        let required = RequiredDiscriminator::BoundaryValue {
            description: "amount >= discount_threshold".to_string(),
        };

        assert!(discriminator_fact_matches(
            &required,
            "amount == discount_threshold"
        ));
        assert!(!discriminator_fact_matches(
            &required,
            "amount == discount_threshold_cache_key"
        ));
        assert!(!discriminator_fact_matches(
            &required,
            "other_amount == discount_threshold"
        ));
    }

    #[test]
    fn boundary_matching_accepts_literal_equality_fact_for_same_boundary() {
        let required = RequiredDiscriminator::BoundaryValue {
            description: "amount > 10".to_string(),
        };

        assert!(discriminator_fact_matches(&required, "amount == 10"));
        assert!(!discriminator_fact_matches(&required, "amount == 100"));
    }

    #[test]
    fn non_boundary_matching_requires_exact_producer_text() {
        let required = RequiredDiscriminator::FieldValue {
            field: "discount_threshold".to_string(),
        };

        assert!(discriminator_fact_matches(&required, "discount_threshold"));
        assert!(!discriminator_fact_matches(
            &required,
            "discount_threshold_cache_key"
        ));
    }
}
