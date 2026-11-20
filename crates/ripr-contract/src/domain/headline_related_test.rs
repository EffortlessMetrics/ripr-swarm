//! Row selection shared by the single-test diff projections.

use super::{
    Finding, LanguageId, OracleKind, OracleStrength, RelatedTest, RelatedTestMiss,
    python_headline_related_test,
};

impl RelatedTest {
    /// Identify this test in display evidence using the producer's exact test start.
    pub fn headline_observer_evidence(&self) -> String {
        format!(
            "related_test_observer: {}:{}",
            self.file.to_string_lossy().replace('\\', "/"),
            self.line
        )
    }

    pub(super) fn headline_has_path(&self, finding: &Finding) -> bool {
        // A missing relation is the legacy representation, including Python's
        // adapter. Its alignment and row grade can supply independent evidence.
        self.relation_reason
            .is_none_or(|reason| reason.establishes_reach(finding.probe.owner.is_some()))
            && self.miss != Some(RelatedTestMiss::NoCallPath)
    }

    /// Order the remaining rows by the producer's miss, whether they carry a
    /// graded oracle, and its strength. Equal keys retain the producer order.
    pub(super) fn headline_rank(&self) -> (u8, bool, bool, u8) {
        let graded = !matches!(
            self.oracle_strength,
            OracleStrength::Unknown | OracleStrength::None
        );
        (
            self.observation_tier(),
            graded,
            self.miss.is_none(),
            self.oracle_strength.rank(),
        )
    }
}

impl Finding {
    /// Keep uncertain or examined rows in the evidence table. Multiple such
    /// rows cannot single out a test whose assertion observes this change.
    pub(super) fn headline_candidate(&self, row: &RelatedTest) -> bool {
        if !row.headline_has_path(self) || !row.is_contributing() {
            return false;
        }
        if self.related_tests.len() == 1 {
            return true;
        }
        let unconfirmed = matches!(
            row.miss,
            Some(
                RelatedTestMiss::ObservationUnconfirmed
                    | RelatedTestMiss::AssertionNotObserving
                    | RelatedTestMiss::AssertionAdmissionUnproven
                    | RelatedTestMiss::AssertionInert
                    | RelatedTestMiss::AssertionNotCredited
            )
        );
        let ungraded = row.oracle_kind == OracleKind::Unknown
            && matches!(
                row.oracle_strength,
                OracleStrength::Unknown | OracleStrength::None
            );
        !unconfirmed && !ungraded
    }

    /// Explain abstention without treating every matched row as retained.
    pub fn missing_related_test_headline(&self) -> String {
        let only_location_matches = !self.related_tests.is_empty()
            && self.related_tests.iter().all(|row| {
                row.relation_reason.is_some_and(|reason| {
                    reason.is_proximity_only() || reason == super::RelationReason::OwnerNamedTest
                })
            });
        let detail = if only_location_matches {
            "only name or file matches were retained, with no established test path"
        } else {
            "the retained rows do not identify an observer"
        };
        format!(
            "No discriminating test identified: {detail} ({} matched before the display limit).",
            self.related_tests_total()
        )
    }

    /// An explicit observer must resolve to one retained test at its start line.
    pub(super) fn headline_observer(&self) -> Option<&RelatedTest> {
        let mut references = self
            .evidence
            .iter()
            .filter(|line| line.starts_with("related_test_observer: "));
        let reference = references.next()?;
        if references.next().is_some() {
            return None;
        }
        let mut rows = self
            .related_tests
            .iter()
            .filter(|test| test.headline_observer_evidence() == *reference);
        let row = rows.next()?;
        (rows.next().is_none()
            && row.is_contributing()
            && row.miss.is_none()
            && row.headline_has_path(self))
        .then_some(row)
    }

    /// Representative related test for diff displays. The packet can still
    /// list examined and location-only rows when none can supply a headline.
    pub fn headline_related_test(&self) -> Option<&RelatedTest> {
        if matches!(
            self.language,
            Some(LanguageId::TypeScript | LanguageId::JavaScript)
        ) {
            return super::typescript_script_preview_related_test(self);
        }
        python_headline_related_test(self)
    }
}
