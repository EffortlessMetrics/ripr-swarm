//! Row-local `RelatedTestMiss` for Perl v2 related tests (#5562, RIPR-SPEC-0224).
//!
//! Maps row-owned v2 producer facts onto the shared miss vocabulary. This pass
//! is evidence-only: it reads the final exposure class and typed row facts, then
//! writes `RelatedTest::miss` and `RelatedTest::miss_support`. It never changes
//! class, stages, confidence, ordering, repair placement or finding identity.

use super::{
    ActivationFrameworkFact, ActivationState, ChangeFact, FrameworkExecution, OracleRelevanceFact,
    OracleRelevanceState, OracleScopeCoverageFact, OracleStrength, PerlFactPacket,
    PerlRelatedTestEvidence, PredicateParticipation, PredicateParticipationFact, ReachabilityHint,
    RelationKind, RelationReachFact, RelationReachState,
};
use crate::domain::{ExposureClass, RelatedTest, RelatedTestMiss, RelatedTestMissSupport};

/// Stable join surface for one related-test row (#5562).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PerlRowJoinKey {
    change_id: String,
    test_id: String,
    relation_id: String,
    callsite_id: Option<String>,
    oracle_id: Option<String>,
}

impl PerlRowJoinKey {
    fn from_evidence(ev: &PerlRelatedTestEvidence) -> Self {
        Self {
            change_id: ev.change_id.clone(),
            test_id: ev.test_id.clone(),
            relation_id: ev.relation_id.clone(),
            callsite_id: ev.callsite_id.clone(),
            oracle_id: ev.oracle_id.clone(),
        }
    }
}

struct RowMissFacts<'a> {
    participation: Option<&'a PredicateParticipationFact>,
    relevance: Option<&'a OracleRelevanceFact>,
    activation: Option<&'a ActivationFrameworkFact>,
    reach: Option<&'a RelationReachFact>,
    scope: Option<&'a OracleScopeCoverageFact>,
}

struct RowMissIndex<'a> {
    participation: std::collections::HashMap<PerlRowJoinKey, &'a PredicateParticipationFact>,
    relevance: std::collections::HashMap<PerlRowJoinKey, &'a OracleRelevanceFact>,
    activation: std::collections::HashMap<String, &'a ActivationFrameworkFact>,
    reach: std::collections::HashMap<String, &'a RelationReachFact>,
    scope: std::collections::HashMap<String, &'a OracleScopeCoverageFact>,
}

impl<'a> RowMissIndex<'a> {
    fn build(packet: &'a PerlFactPacket) -> Self {
        use std::collections::HashMap;
        let mut participation = HashMap::new();
        for fact in &packet.predicate_participations {
            participation.insert(
                join_key_from_ids(
                    &fact.change_id,
                    &fact.test_id,
                    &fact.relation_id,
                    fact.callsite_id.as_deref(),
                    fact.oracle_id.as_deref(),
                ),
                fact,
            );
        }
        let mut relevance = HashMap::new();
        for fact in &packet.oracle_relevances {
            relevance.insert(
                join_key_from_ids(
                    &fact.change_id,
                    &fact.test_id,
                    &fact.relation_id,
                    fact.callsite_id.as_deref(),
                    fact.oracle_id.as_deref(),
                ),
                fact,
            );
        }
        let mut activation = HashMap::new();
        for fact in &packet.activation_framework {
            activation.insert(fact.relation_id.clone(), fact);
        }
        let mut reach = HashMap::new();
        for fact in &packet.relation_reach {
            reach.insert(fact.relation_id.clone(), fact);
        }
        let mut scope = HashMap::new();
        for fact in &packet.oracle_scope_coverage {
            scope.insert(fact.test_id.clone(), fact);
        }
        Self {
            participation,
            relevance,
            activation,
            reach,
            scope,
        }
    }

    fn for_row(&self, key: &PerlRowJoinKey) -> RowMissFacts<'_> {
        RowMissFacts {
            participation: self.participation.get(key).copied(),
            relevance: self.relevance.get(key).copied(),
            activation: self.activation.get(&key.relation_id).copied(),
            reach: self.reach.get(&key.relation_id).copied(),
            scope: self.scope.get(&key.test_id).copied(),
        }
    }
}

fn join_key_from_ids(
    change_id: &str,
    test_id: &str,
    relation_id: &str,
    callsite_id: Option<&str>,
    oracle_id: Option<&str>,
) -> PerlRowJoinKey {
    PerlRowJoinKey {
        change_id: change_id.to_string(),
        test_id: test_id.to_string(),
        relation_id: relation_id.to_string(),
        callsite_id: callsite_id.map(str::to_string),
        oracle_id: oracle_id.map(str::to_string),
    }
}

pub(super) fn packet_has_v2_row_miss_evidence(packet: &PerlFactPacket) -> bool {
    !packet.predicate_participations.is_empty()
        || !packet.oracle_relevances.is_empty()
        || !packet.activation_framework.is_empty()
        || !packet.relation_reach.is_empty()
        || !packet.oracle_scope_coverage.is_empty()
}

pub(super) fn assign_perl_v2_row_misses(
    packet: &PerlFactPacket,
    change: &ChangeFact,
    class: &ExposureClass,
    class_blocked: bool,
    actionability_blocked: bool,
    packet_admitted: bool,
    related: &mut [RelatedTest],
    related_evidence: &[PerlRelatedTestEvidence],
) {
    if related.len() != related_evidence.len() {
        return;
    }
    if class_blocked
        || actionability_blocked
        || !packet_admitted
        || matches!(class, ExposureClass::Exposed | ExposureClass::StaticUnknown)
    {
        return;
    }
    let index = RowMissIndex::build(packet);
    for (row, ev) in related.iter_mut().zip(related_evidence) {
        let key = PerlRowJoinKey::from_evidence(ev);
        let facts = index.for_row(&key);
        row.miss = perl_v2_row_miss(class, ev, change, &facts);
        row.miss_support = match row.miss {
            Some(RelatedTestMiss::MissingInput) => facts.participation.and_then(|p| {
                p.support_value
                    .as_ref()
                    .map(|value| RelatedTestMissSupport {
                        value: value.clone(),
                        flow_sink: None,
                    })
            }),
            Some(RelatedTestMiss::MissingExactAssertion) => facts.relevance.and_then(|r| {
                r.required_discriminator
                    .as_ref()
                    .map(|value| RelatedTestMissSupport {
                        value: value.clone(),
                        flow_sink: None,
                    })
            }),
            _ => None,
        };
    }
}

fn perl_v2_row_miss(
    class: &ExposureClass,
    ev: &PerlRelatedTestEvidence,
    change: &ChangeFact,
    facts: &RowMissFacts<'_>,
) -> Option<RelatedTestMiss> {
    if activation_withholds_miss(facts.activation) {
        return None;
    }
    if let Some(participation) = facts.participation
        && participation.participation == PredicateParticipation::Distinguishes
    {
        return None;
    }
    if let Some(ScopeAdmission::Unresolved) = scope_admission(facts.scope, ev) {
        return None;
    }

    if matches!(class, ExposureClass::NoStaticPath)
        && heuristic_relation(ev.relation_kind)
        && !admitted_exact_or_bounded_reach(facts.reach, ev)
    {
        return Some(RelatedTestMiss::NoCallPath);
    }

    if admitted_call_relation(ev)
        && matches!(
            scope_admission(facts.scope, ev),
            Some(ScopeAdmission::CompleteEmpty)
        )
    {
        return Some(RelatedTestMiss::NoAssertion);
    }

    if admitted_call_relation(ev)
        && weak_oracle_decisive(ev)
        && !same_outcome_participation(facts.participation)
    {
        return Some(RelatedTestMiss::WeakAssertion);
    }

    if admitted_call_relation(ev)
        && same_outcome_participation(facts.participation)
        && !activation_withholds_miss(facts.activation)
    {
        return Some(RelatedTestMiss::MissingInput);
    }

    if admitted_call_relation(ev)
        && matches!(
            facts.relevance.map(|r| r.state),
            Some(OracleRelevanceState::AffirmativeOtherSink)
        )
    {
        return Some(RelatedTestMiss::AssertionNotObserving);
    }

    if admitted_call_relation(ev)
        && matches!(
            facts.relevance.map(|r| r.state),
            Some(OracleRelevanceState::ExactSinkBroadAssertion)
        )
    {
        return Some(RelatedTestMiss::MissingExactAssertion);
    }

    if admitted_call_relation(ev)
        && ev.oracle_id.is_some()
        && matches!(
            facts.relevance.map(|r| r.state),
            Some(
                OracleRelevanceState::CandidateSink
                    | OracleRelevanceState::Bounded
                    | OracleRelevanceState::Unlinked
            )
        )
    {
        return Some(RelatedTestMiss::ObservationUnconfirmed);
    }

    if admitted_call_relation(ev)
        && ev.class == ExposureClass::WeaklyExposed
        && ev.relation_kind == RelationKind::DirectOwnerCall
        && ev.reachability_hint == ReachabilityHint::Reachable
        && ev.oracle_strength == Some(OracleStrength::StrongExact)
        && change.changed_observable.as_deref().is_some()
        && facts.relevance.is_none()
    {
        return Some(RelatedTestMiss::ObservationUnconfirmed);
    }

    None
}

enum ScopeAdmission {
    CompleteEmpty,
    Unresolved,
}

fn scope_admission(
    scope: Option<&OracleScopeCoverageFact>,
    ev: &PerlRelatedTestEvidence,
) -> Option<ScopeAdmission> {
    let scope = scope?;
    if scope.test_id != ev.test_id {
        return None;
    }
    if scope.unresolved_admission {
        return Some(ScopeAdmission::Unresolved);
    }
    (scope.extraction_complete && scope.recognized_assertion_count == 0)
        .then_some(ScopeAdmission::CompleteEmpty)
}

fn activation_withholds_miss(activation: Option<&ActivationFrameworkFact>) -> bool {
    matches!(
        activation.map(|a| (a.activation, a.framework_execution)),
        Some((ActivationState::Impossible, _)) | Some((_, FrameworkExecution::Skipped))
    )
}

fn heuristic_relation(kind: RelationKind) -> bool {
    matches!(
        kind,
        RelationKind::TestNameMatch | RelationKind::FileProximity | RelationKind::PackageReference
    )
}

fn admitted_call_relation(ev: &PerlRelatedTestEvidence) -> bool {
    ev.relation_kind.supports_positive_reachability()
        && ev.reachability_hint == ReachabilityHint::Reachable
}

fn admitted_exact_or_bounded_reach(
    reach: Option<&RelationReachFact>,
    ev: &PerlRelatedTestEvidence,
) -> bool {
    reach.is_some_and(|fact| {
        fact.relation_id == ev.relation_id
            && matches!(
                fact.state,
                RelationReachState::ExactStaticCall | RelationReachState::BoundedHelper
            )
    })
}

fn weak_oracle_decisive(ev: &PerlRelatedTestEvidence) -> bool {
    matches!(
        ev.oracle_strength,
        Some(OracleStrength::WeakSmoke | OracleStrength::WeakBroad)
    )
}

fn same_outcome_participation(participation: Option<&PredicateParticipationFact>) -> bool {
    participation.is_some_and(|p| p.participation == PredicateParticipation::SameOutcome)
}
