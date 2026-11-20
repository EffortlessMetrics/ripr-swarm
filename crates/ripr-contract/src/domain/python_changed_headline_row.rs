//! Python headline related-test row selection (#14027).
//!
//! A same-diff stamp is a final tie-break between equally ranked Python rows.

use super::{Finding, LanguageId, RelatedTest};
use std::cmp::Reverse;

/// Evidence line prefix: `related_test_changed_in_diff: <file>:<line>`.
pub const RELATED_TEST_CHANGED_IN_DIFF_PREFIX: &str = "related_test_changed_in_diff: ";

/// Prefer a row the producer tied to the changed behavior. Otherwise compare
/// contributing rows by their recorded miss, path and strength. A Python diff
/// stamp only breaks ties at the resulting rank (#14027).
pub fn python_headline_related_test(finding: &Finding) -> Option<&RelatedTest> {
    if let Some(observer) = finding.headline_observer() {
        return Some(observer);
    }
    let first = finding
        .oracle_related_tests()
        .filter(|row| finding.headline_candidate(row))
        .min_by_key(|row| Reverse(row.headline_rank()))?;
    if !matches!(finding.language, Some(LanguageId::Python)) {
        return Some(first);
    }
    let rank = first.headline_rank();
    let changed: Vec<(String, usize)> = finding
        .evidence
        .iter()
        .filter_map(|line| line.strip_prefix(RELATED_TEST_CHANGED_IN_DIFF_PREFIX))
        .filter_map(|reference| {
            let (file, line) = reference.trim().rsplit_once(':')?;
            Some((file.replace('\\', "/"), line.parse::<usize>().ok()?))
        })
        .collect();
    if changed.is_empty() {
        return Some(first);
    }
    finding
        .related_tests
        .iter()
        .find(|test| {
            let file = test.file.to_string_lossy().replace('\\', "/");
            finding.headline_candidate(test)
                && test.headline_rank() == rank
                && changed
                    .iter()
                    .any(|(changed_file, line)| *line == test.line && *changed_file == file)
        })
        .or(Some(first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, ExposureClass, OracleKind, OracleStrength,
        Probe, ProbeFamily, ProbeId, RelatedTestMiss, RelatedTestRole, RevealEvidence,
        RiprEvidence, SourceCurrentness, SourceLocation, StageEvidence, StageState,
    };
    use std::path::PathBuf;

    fn stage() -> StageEvidence {
        StageEvidence::new(StageState::Unknown, Confidence::Unknown, "fixture")
    }

    fn row(name: &str, file: &str, line: usize, strength: OracleStrength) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from(file),
            line,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: strength,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            miss_support: None,
            role: RelatedTestRole::Contributing,
        }
    }

    fn finding(
        language: Option<LanguageId>,
        related_tests: Vec<RelatedTest>,
        changed: &[&str],
    ) -> Finding {
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: "finding:headline".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:headline".to_string()),
                location: SourceLocation::new("svc/core.py", 3, 5),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "amount >= limit".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(),
                infect: stage(),
                propagate: stage(),
                reveal: RevealEvidence {
                    observe: stage(),
                    discriminate: stage(),
                },
            },
            confidence: 0.5,
            evidence: changed
                .iter()
                .map(|reference| format!("{RELATED_TEST_CHANGED_IN_DIFF_PREFIX}{reference}"))
                .collect(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests,
            recommended_next_step: None,
            language,
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    fn headline(finding: &Finding) -> Option<&str> {
        python_headline_related_test(finding).map(|test| test.name.as_str())
    }

    fn rows() -> Vec<RelatedTest> {
        vec![
            row("first", "tests/test_a.py", 10, OracleStrength::Strong),
            row("weaker", "tests/test_b.py", 20, OracleStrength::Weak),
            row("edited", "tests/test_c.py", 30, OracleStrength::Strong),
        ]
    }

    const CHANGED_B: &str = "related_test_changed_in_diff: tests/test_b.py:20";

    fn rows_same_tier() -> Vec<RelatedTest> {
        vec![
            row("anchor", "tests/test_a.py", 10, OracleStrength::Medium),
            row("touched", "tests/test_b.py", 20, OracleStrength::Medium),
        ]
    }

    fn finding_with_evidence(rows: Vec<RelatedTest>, evidence: &[&str]) -> Finding {
        let stage = || StageEvidence::new(StageState::Yes, Confidence::Medium, "s");
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: "finding:py".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:py".to_string()),
                location: SourceLocation::new("src/app.py", 3, 1),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "x > 5".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(),
                infect: stage(),
                propagate: stage(),
                reveal: RevealEvidence {
                    observe: stage(),
                    discriminate: stage(),
                },
            },
            confidence: 0.5,
            evidence: evidence.iter().map(|line| (*line).to_string()).collect(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: rows,
            recommended_next_step: None,
            language: Some(LanguageId::Python),
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    #[test]
    fn an_edited_row_of_the_anchor_strength_leads_a_python_headline() {
        let python = Some(LanguageId::Python);
        assert_eq!(
            headline(&finding(python, rows(), &["tests/test_c.py:30"])),
            Some("edited")
        );
        // One matching stamp among unrelated ones is enough.
        assert_eq!(
            headline(&finding(
                python,
                rows(),
                &["tests/test_x.py:1", "tests/test_c.py:30"]
            )),
            Some("edited")
        );
        // A Windows-spelled stamp names the same row.
        assert_eq!(
            headline(&finding(python, rows(), &["tests\\test_c.py:30"])),
            Some("edited")
        );
        // No stamp, or a stamp that matches only the file or only the line,
        // keeps the anchor.
        for changed in [
            &[][..],
            &["tests/test_c.py:31"][..],
            &["tests/test_x.py:30"][..],
        ] {
            assert_eq!(
                headline(&finding(python, rows(), changed)),
                Some("first"),
                "{changed:?}"
            );
        }
    }

    #[test]
    fn a_row_of_another_strength_or_tier_never_displaces_the_anchor() {
        let python = Some(LanguageId::Python);
        assert_eq!(
            headline(&finding(python, rows(), &["tests/test_b.py:20"])),
            Some("first")
        );
        let mut tiers = rows();
        tiers[2].miss = Some(RelatedTestMiss::AssertionNotObserving);
        assert_eq!(
            headline(&finding(python, tiers, &["tests/test_c.py:30"])),
            Some("first")
        );
    }

    #[test]
    fn other_languages_keep_the_anchor_row() {
        assert_eq!(
            headline(&finding(
                Some(LanguageId::Rust),
                rows(),
                &["tests/test_c.py:30"]
            )),
            Some("first")
        );
        assert_eq!(
            headline(&finding(None, rows(), &["tests/test_c.py:30"])),
            Some("first")
        );
        assert_eq!(
            headline(&finding(Some(LanguageId::Python), Vec::new(), &[])),
            None
        );
    }

    #[test]
    fn a_same_tier_row_in_a_changed_test_file_leads_the_python_headline() {
        assert_eq!(
            headline(&finding_with_evidence(rows_same_tier(), &[CHANGED_B])),
            Some("touched")
        );
        assert_eq!(
            headline(&finding_with_evidence(
                rows_same_tier(),
                &["related_test_changed_in_diff: tests\\test_b.py:20"]
            )),
            Some("touched")
        );
        assert_eq!(
            headline(&finding_with_evidence(rows_same_tier(), &[])),
            Some("anchor")
        );
        assert_eq!(
            headline(&finding_with_evidence(Vec::new(), &[CHANGED_B])),
            None
        );
    }

    #[test]
    fn a_stamp_must_match_both_file_and_line_of_a_listed_row() {
        for stamp in [
            "related_test_changed_in_diff: tests/test_b.py:21",
            "related_test_changed_in_diff: tests/test_c.py:20",
            "related_test_changed_in_diff: tests/test_b.py:twenty",
            "related_test_changed_in_diff: tests/test_b.py",
        ] {
            assert_eq!(
                headline(&finding_with_evidence(rows_same_tier(), &[stamp])),
                Some("anchor"),
                "{stamp}"
            );
        }
    }

    #[test]
    fn a_changed_row_of_another_strength_or_observation_tier_never_leads() {
        let mut weaker = rows_same_tier();
        weaker[1].oracle_strength = OracleStrength::Weak;
        assert_eq!(
            headline(&finding_with_evidence(weaker, &[CHANGED_B])),
            Some("anchor")
        );

        let mut unobserving = rows_same_tier();
        unobserving[1].miss = Some(RelatedTestMiss::AssertionNotObserving);
        assert_eq!(
            headline(&finding_with_evidence(unobserving, &[CHANGED_B])),
            Some("anchor")
        );

        // The anchor itself is the first observing row, not the first listed.
        let mut blind_first = rows_same_tier();
        blind_first[0].miss = Some(RelatedTestMiss::NoCallPath);
        assert_eq!(
            headline(&finding_with_evidence(blind_first, &[])),
            Some("touched")
        );
    }
    #[test]
    fn headline_prefers_the_observer_in_either_position() {
        let mut unrelated = row(
            "misses_boundary",
            "tests/owner.rs",
            2,
            OracleStrength::Strong,
        );
        unrelated.relation_reason = Some(crate::domain::RelationReason::DirectOwnerCall);
        unrelated.miss = Some(RelatedTestMiss::MissingInput);
        let mut observer = row("pins_boundary", "tests/owner.rs", 7, OracleStrength::Strong);
        observer.relation_reason = Some(crate::domain::RelationReason::HelperOwnerCall);
        for rows in [
            vec![unrelated.clone(), observer.clone()],
            vec![observer, unrelated],
        ] {
            assert_eq!(
                headline(&finding(Some(LanguageId::Rust), rows, &[])),
                Some("pins_boundary")
            );
        }
    }
}
