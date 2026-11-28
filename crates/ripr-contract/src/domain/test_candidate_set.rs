//! Bounded seam-to-test candidate envelope for requirement-aware discovery (#1664).
//!
//! This layer projects existing [`RelatedTest`] rows onto [`CandidateRelation`]
//! channels and separates producer-backed selectors from context-only rows.
//! It does not execute tests, rank the whole repository, or decide adequacy.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{CandidateRelation, RelatedTest, RelationReason};

/// Recommended next step after inspecting a finite candidate set (#1660 route).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestCandidateNextRoute {
    /// At least one producer-backed candidate was selected; run those tests.
    RunTargetedPreflight,
    /// No candidate was selected, but context-only rows exist or the window
    /// was truncated (possibly with no rows in it); inspect the rows or widen
    /// discovery before writing a new test.
    InspectWeakCandidates,
    /// The complete discovery set is empty; add a new focused test.
    AddNewFocusedTest,
    /// The seam or its owner needs resolving first. Reserved: no current
    /// producer selects it.
    ResolveSeamOrOwner,
    /// The analysis is stale and needs refreshing first. Reserved: no current
    /// producer selects it.
    RefreshStaleAnalysis,
    /// The case needs manual handling or is unsupported. Reserved: no current
    /// producer selects it.
    ManualOrUnsupported,
}

impl TestCandidateNextRoute {
    /// The stable `snake_case` word, as serialized.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunTargetedPreflight => "run_targeted_preflight",
            Self::InspectWeakCandidates => "inspect_weak_candidates",
            Self::AddNewFocusedTest => "add_new_focused_test",
            Self::ResolveSeamOrOwner => "resolve_seam_or_owner",
            Self::RefreshStaleAnalysis => "refresh_stale_analysis",
            Self::ManualOrUnsupported => "manual_or_unsupported",
        }
    }
}

/// One bounded candidate row with discovery relation and eligibility.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TestCandidateRow {
    /// Name of the related test.
    pub test_name: String,
    /// File that defines the test.
    pub source_file: PathBuf,
    /// Line of the test in `source_file`.
    pub line: usize,
    /// Discovery channel derived from `relation_reason`; `Unknown` when the
    /// row carries no reason.
    pub candidate_relation: CandidateRelation,
    /// Whether the relation is producer-backed, which places the row in
    /// `selected` rather than `context_only`.
    pub eligible: bool,
    /// Why discovery related the test to the seam, when the producer said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_reason: Option<RelationReason>,
}

/// Envelope over a finite related-test candidate set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TestCandidateSet {
    /// Producer-backed rows, ordered by relation, then name, file and line.
    pub selected: Vec<TestCandidateRow>,
    /// Rows whose relation is not producer-backed (proximity, coincidence,
    /// opaque helper or macro, or unknown), in the same order. They are
    /// context for a reader, not selectors.
    pub context_only: Vec<TestCandidateRow>,
    /// Rows present in this bounded window (for example after reveal truncation).
    pub total_discovered: usize,
    /// Producer-reported match count before any window cap, when known.
    pub matched_total: usize,
    /// False when upstream discovery omitted rows from this window.
    pub complete: bool,
    /// What to do next with this set.
    pub next_route: TestCandidateNextRoute,
}

impl TestCandidateSet {
    /// Build a deterministic candidate set from related-test rows already
    /// discovered for one seam. Producer-backed relations become `selected`;
    /// proximity/coincidence relations stay in `context_only`.
    pub fn from_related_tests(related: &[RelatedTest]) -> Self {
        Self::from_bounded_related_tests(related, related.len(), true)
    }

    /// Same as [`Self::from_related_tests`], but preserves producer totals when
    /// the input slice is a capped window over a larger discovery set.
    pub fn from_bounded_related_tests(
        related: &[RelatedTest],
        matched_total: usize,
        complete: bool,
    ) -> Self {
        let mut selected = Vec::new();
        let mut context_only = Vec::new();

        for test in related {
            let relation_reason = test.relation_reason;
            let candidate_relation = relation_reason
                .map(RelationReason::candidate_relation)
                .unwrap_or(CandidateRelation::Unknown);
            let row = TestCandidateRow {
                test_name: test.name.clone(),
                source_file: test.file.clone(),
                line: test.line,
                eligible: candidate_relation.is_producer_backed(),
                candidate_relation,
                relation_reason,
            };
            if row.eligible {
                selected.push(row);
            } else {
                context_only.push(row);
            }
        }

        selected.sort_by(candidate_row_order);
        context_only.sort_by(candidate_row_order);

        let truncated = !complete || matched_total > related.len();
        let next_route = if !selected.is_empty() {
            TestCandidateNextRoute::RunTargetedPreflight
        } else if truncated || !context_only.is_empty() {
            TestCandidateNextRoute::InspectWeakCandidates
        } else {
            TestCandidateNextRoute::AddNewFocusedTest
        };

        Self {
            total_discovered: related.len(),
            matched_total,
            complete,
            selected,
            context_only,
            next_route,
        }
    }
}

fn candidate_row_order(left: &TestCandidateRow, right: &TestCandidateRow) -> std::cmp::Ordering {
    left.candidate_relation
        .cmp(&right.candidate_relation)
        .then_with(|| left.test_name.cmp(&right.test_name))
        .then_with(|| left.source_file.cmp(&right.source_file))
        .then_with(|| left.line.cmp(&right.line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OracleKind, OracleStrength, RelatedTestRole};

    fn sample_test(name: &str, reason: RelationReason) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("tests/sample.rs"),
            line: 10,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_reason: Some(reason),
            relation_confidence: Some(reason.confidence()),
            miss: None,
            role: RelatedTestRole::Contributing,
            miss_support: None,
        }
    }

    #[test]
    fn producer_backed_candidates_are_selected_and_ordered_before_context() {
        let set = TestCandidateSet::from_related_tests(&[
            sample_test("weak_name_match", RelationReason::OwnerNamedTest),
            sample_test("direct", RelationReason::DirectOwnerCall),
            sample_test("helper", RelationReason::HelperOwnerCall),
        ]);

        assert_eq!(set.total_discovered, 3);
        assert_eq!(set.selected.len(), 2);
        assert_eq!(set.context_only.len(), 1);
        assert_eq!(set.next_route, TestCandidateNextRoute::RunTargetedPreflight);
        assert_eq!(
            set.selected[0].candidate_relation,
            CandidateRelation::DirectOwnerCall
        );
        assert_eq!(
            set.selected[1].candidate_relation,
            CandidateRelation::EstablishedOneHopHelperChain
        );
        assert_eq!(
            set.context_only[0].candidate_relation,
            CandidateRelation::TokenCoincidenceOnly
        );
        assert!(!set.context_only[0].eligible);
    }

    #[test]
    fn proximity_only_set_routes_to_inspect_weak_candidates() {
        let set = TestCandidateSet::from_related_tests(&[sample_test(
            "same_file",
            RelationReason::SameTestFile,
        )]);

        assert!(set.selected.is_empty());
        assert_eq!(set.context_only.len(), 1);
        assert_eq!(
            set.next_route,
            TestCandidateNextRoute::InspectWeakCandidates
        );
    }

    #[test]
    fn empty_related_tests_route_to_add_new_focused_test() {
        let set = TestCandidateSet::from_related_tests(&[]);
        assert_eq!(set.total_discovered, 0);
        assert_eq!(set.matched_total, 0);
        assert!(set.complete);
        assert_eq!(set.next_route, TestCandidateNextRoute::AddNewFocusedTest);
    }

    #[test]
    fn fixture_owner_affinity_stays_context_only() {
        let set = TestCandidateSet::from_related_tests(&[sample_test(
            "fixture_helper",
            RelationReason::FixtureOwnerAffinity,
        )]);
        assert!(set.selected.is_empty());
        assert_eq!(set.context_only.len(), 1);
        assert_eq!(
            set.next_route,
            TestCandidateNextRoute::InspectWeakCandidates
        );
    }

    #[test]
    fn truncated_window_never_routes_to_add_new_focused_test_when_matches_exist() {
        let window: Vec<_> = (0..8)
            .map(|idx| sample_test(&format!("ctx_{idx}"), RelationReason::SameTestFile))
            .collect();
        let set = TestCandidateSet::from_bounded_related_tests(&window, 9, false);
        assert_eq!(set.total_discovered, 8);
        assert_eq!(set.matched_total, 9);
        assert!(!set.complete);
        assert_eq!(
            set.next_route,
            TestCandidateNextRoute::InspectWeakCandidates
        );
    }

    #[test]
    fn an_empty_window_adds_a_new_test_only_when_discovery_was_complete() {
        let route = |matched_total: usize, complete: bool| {
            TestCandidateSet::from_bounded_related_tests(&[], matched_total, complete).next_route
        };
        assert_eq!(route(0, true), TestCandidateNextRoute::AddNewFocusedTest);
        assert_eq!(
            route(0, false),
            TestCandidateNextRoute::InspectWeakCandidates
        );
        assert_eq!(
            route(3, true),
            TestCandidateNextRoute::InspectWeakCandidates
        );
    }

    #[test]
    fn next_routes_use_their_wire_tokens() {
        let cases = [
            (
                TestCandidateNextRoute::RunTargetedPreflight,
                "run_targeted_preflight",
            ),
            (
                TestCandidateNextRoute::InspectWeakCandidates,
                "inspect_weak_candidates",
            ),
            (
                TestCandidateNextRoute::AddNewFocusedTest,
                "add_new_focused_test",
            ),
            (
                TestCandidateNextRoute::ResolveSeamOrOwner,
                "resolve_seam_or_owner",
            ),
            (
                TestCandidateNextRoute::RefreshStaleAnalysis,
                "refresh_stale_analysis",
            ),
            (
                TestCandidateNextRoute::ManualOrUnsupported,
                "manual_or_unsupported",
            ),
        ];
        for (route, token) in cases {
            assert_eq!(route.as_str(), token);
        }
    }
}
