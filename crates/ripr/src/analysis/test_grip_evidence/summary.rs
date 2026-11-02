//! Portable `TestEvidenceSummary` projection for repo-seam grip evidence (#1658).
//!
//! Reuses analyzer-owned `TestGripEvidence` facts without inventing targets,
//! oracle strength, or missing-discriminator identities.

use super::{RelatedTestGrip, TestGripEvidence};
use crate::domain::{TestEvidenceEntry, TestEvidenceSummary, normalize_transport_path};

impl TestGripEvidence {
    /// Project this seam's related-test and discriminator facts into the shared
    /// portable summary used by cargo-allow relevance checks and witness parity.
    pub(crate) fn test_evidence_summary(&self) -> TestEvidenceSummary {
        let entries = self
            .related_tests
            .iter()
            .map(|test| related_test_entry(test.as_ref()))
            .collect::<Vec<_>>();
        let missing = self
            .missing_discriminators
            .iter()
            .map(|fact| fact.value.clone())
            .collect::<Vec<_>>();

        TestEvidenceSummary::from_parts(self.seam_id.as_str(), entries, &missing)
    }
}

fn related_test_entry(test: &RelatedTestGrip) -> TestEvidenceEntry {
    TestEvidenceEntry {
        test_name: test.test_name.clone(),
        file: normalize_transport_path(&test.file.to_string_lossy()),
        line: test.line,
        oracle_kind: test.oracle_kind.as_str().to_string(),
        oracle_strength: test.oracle_strength.as_str().to_string(),
        relation_reason: test.relation_reason.as_str().to_string(),
        has_test_target: test.test_target.is_some(),
    }
}

/// Keep the repo-seam summary projection on the production inventory path
/// without changing public seam or cache bytes.
pub(crate) fn retain_test_grip_summary(evidence: &TestGripEvidence) {
    let summary = evidence.test_evidence_summary();
    debug_assert_eq!(
        summary.seam_id,
        evidence.seam_id.as_str(),
        "test evidence summary must stay bound to its producer seam"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
    use crate::analysis::test_grip_evidence::{RelationConfidence, TestTargetEvidence};
    use crate::domain::{
        Confidence, MissingDiscriminatorFact, OracleKind, OracleStrength, RelationReason,
        StageEvidence, StageState,
    };
    use std::path::PathBuf;
    use std::sync::Arc;

    fn stage(summary: &str) -> StageEvidence {
        StageEvidence::new(StageState::Unknown, Confidence::Unknown, summary)
    }

    fn sample_seam() -> RepoSeam {
        RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            12,
            42,
            "amount >= threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        )
    }

    fn empty_grip(
        related: Vec<RelatedTestGrip>,
        missing: Vec<MissingDiscriminatorFact>,
    ) -> TestGripEvidence {
        let seam = sample_seam();
        TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: related.into_iter().map(Arc::new).collect(),
            reach: stage("reach"),
            activate: stage("activate"),
            propagate: stage("propagate"),
            observe: stage("observe"),
            discriminate: stage("discriminate"),
            observed_values: Vec::new(),
            missing_discriminators: missing,
            statically_contradicted_related_tests: 0,
            new_test_target: None,
        }
    }

    fn related(
        name: &str,
        file: &str,
        line: usize,
        kind: OracleKind,
        strength: OracleStrength,
        reason: RelationReason,
        target: Option<TestTargetEvidence>,
    ) -> RelatedTestGrip {
        RelatedTestGrip {
            test_name: name.to_string(),
            file: PathBuf::from(file),
            line,
            test_target: target,
            oracle_kind: kind,
            oracle_strength: strength,
            evidence_summary: "fixture".to_string(),
            relation_reason: reason,
            relation_confidence: RelationConfidence::Medium,
        }
    }

    #[test]
    fn exact_and_broad_oracles_produce_distinct_fingerprints() {
        let broad = empty_grip(
            vec![related(
                "smoke",
                "tests/pricing.rs",
                1,
                OracleKind::BroadError,
                OracleStrength::Weak,
                RelationReason::DirectOwnerCall,
                None,
            )],
            Vec::new(),
        )
        .test_evidence_summary();
        let exact = empty_grip(
            vec![related(
                "smoke",
                "tests/pricing.rs",
                1,
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
                RelationReason::DirectOwnerCall,
                None,
            )],
            Vec::new(),
        )
        .test_evidence_summary();

        assert_ne!(broad.fingerprint, exact.fingerprint);
        assert_eq!(exact.strongest_oracle, "strong");
        assert_eq!(broad.strongest_oracle, "weak");
    }

    #[test]
    fn producer_owned_target_presence_is_explicit() {
        let summary = empty_grip(
            vec![related(
                "exact_case",
                "./tests\\pricing.rs",
                12,
                OracleKind::ExactValue,
                OracleStrength::Strong,
                RelationReason::DirectOwnerCall,
                Some(TestTargetEvidence::fixture(
                    "exact_case",
                    PathBuf::from("tests/pricing.rs").as_path(),
                    12,
                )),
            )],
            Vec::new(),
        )
        .test_evidence_summary();

        assert_eq!(summary.seam_id, sample_seam().id().as_str());
        assert!(summary.related_tests[0].has_test_target);
        assert_eq!(summary.related_tests[0].file, "tests/pricing.rs");
    }

    #[test]
    fn proximity_relation_stays_in_the_semantic_fingerprint() {
        let proximity = empty_grip(
            vec![related(
                "token_coincidence",
                "tests/other.rs",
                3,
                OracleKind::Unknown,
                OracleStrength::None,
                RelationReason::WeakTokenSubstring,
                None,
            )],
            Vec::new(),
        )
        .test_evidence_summary();
        let direct = empty_grip(
            vec![related(
                "token_coincidence",
                "tests/other.rs",
                3,
                OracleKind::Unknown,
                OracleStrength::None,
                RelationReason::DirectOwnerCall,
                None,
            )],
            Vec::new(),
        )
        .test_evidence_summary();

        assert_ne!(proximity.fingerprint, direct.fingerprint);
        assert_eq!(
            proximity.related_tests[0].relation_reason,
            "weak_token_substring"
        );
    }

    #[test]
    fn missing_discriminator_identity_changes_the_fingerprint() {
        let entries = vec![related(
            "boundary_case",
            "tests/pricing.rs",
            4,
            OracleKind::ExactValue,
            OracleStrength::Strong,
            RelationReason::DirectOwnerCall,
            None,
        )];
        let with_gap = empty_grip(
            entries.clone(),
            vec![MissingDiscriminatorFact {
                value: "amount == threshold".to_string(),
                reason: "fixture".to_string(),
                flow_sink: None,
            }],
        )
        .test_evidence_summary();
        let resolved = empty_grip(entries, Vec::new()).test_evidence_summary();

        assert_eq!(with_gap.missing_discriminator_count, 1);
        assert_ne!(with_gap.fingerprint, resolved.fingerprint);
    }

    #[test]
    fn fingerprint_ignores_transport_spelling_and_selector_names() {
        let first = empty_grip(
            vec![
                related(
                    "exact_a",
                    "tests/a.rs",
                    10,
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                    RelationReason::DirectOwnerCall,
                    None,
                ),
                related(
                    "broad_a",
                    "./tests\\b.rs",
                    20,
                    OracleKind::BroadError,
                    OracleStrength::Weak,
                    RelationReason::SameTestFile,
                    None,
                ),
            ],
            Vec::new(),
        )
        .test_evidence_summary();
        let second = empty_grip(
            vec![
                related(
                    "renamed_broad",
                    "different/b.rs",
                    200,
                    OracleKind::BroadError,
                    OracleStrength::Weak,
                    RelationReason::SameTestFile,
                    None,
                ),
                related(
                    "renamed_exact",
                    "../different/a.rs",
                    100,
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                    RelationReason::DirectOwnerCall,
                    None,
                ),
            ],
            Vec::new(),
        )
        .test_evidence_summary();

        assert_eq!(first.fingerprint, second.fingerprint);
    }
}
