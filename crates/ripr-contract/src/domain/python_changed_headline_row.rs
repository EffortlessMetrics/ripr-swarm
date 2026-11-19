//! Python headline related-test row selection (#14027).
//!
//! The Python adapter lists related rows in a stable file order, so the
//! `first()` row can be a test the diff never touched while a same-strength
//! row in a test file the same diff edited sits further down. The adapter
//! stamps one evidence line per related row that lives in a changed test
//! file (only when the first row is not already such a row); this selector
//! reads those lines. Display row selection only: class, oracle strength and
//! row order are not changed.

use super::{Finding, LanguageId, RelatedTest, first_observing_related_test};

/// Evidence line prefix: `related_test_changed_in_diff: <file>:<line>`.
pub const RELATED_TEST_CHANGED_IN_DIFF_PREFIX: &str = "related_test_changed_in_diff: ";

/// The related-test row the human `Related test` headline names for a Python
/// finding. Only that headline routes through this selector; other Python
/// projections keep their own row picks.
///
/// The anchor row is the first row of the best
/// [`RelatedTest::observation_tier`] (#14483; for every non-Python language
/// that anchor is the headline row). Among the rows with the same oracle
/// strength and observation tier as the anchor, the first
/// row whose test file the diff changed leads; otherwise the anchor.
/// Rows of another strength never displace the anchor's tier, and an
/// unmatched stamp is ignored.
pub fn python_headline_related_test(finding: &Finding) -> Option<&RelatedTest> {
    // #14483: the anchor is the first row of the best observation tier, so a
    // listed row whose assertion cannot observe the change never leads while
    // one that can is listed.
    let first = first_observing_related_test(&finding.related_tests)?;
    if !matches!(finding.language, Some(LanguageId::Python)) {
        return Some(first);
    }
    let tier = first.oracle_strength.rank();
    let observation = first.observation_tier();
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
            test.oracle_strength.rank() == tier
                && test.observation_tier() == observation
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
        ActivationEvidence, AnalysisScope, Confidence, DeltaKind, ExposureClass, OracleKind,
        OracleStrength, Probe, ProbeFamily, ProbeId, RelatedTestMiss, RelatedTestRole,
        RevealEvidence, RiprEvidence, SourceCurrentness, SourceLocation, StageEvidence, StageState,
    };
    use std::path::PathBuf;

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
            role: RelatedTestRole::Contributing,
            miss_support: None,
        }
    }

    fn finding(rows: Vec<RelatedTest>, evidence: &[&str]) -> Finding {
        let stage = || StageEvidence::new(StageState::Yes, Confidence::Medium, "s");
        Finding {
            analysis_scope: AnalysisScope::Diff,
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

    fn headline(finding: &Finding) -> Option<&str> {
        python_headline_related_test(finding).map(|test| test.name.as_str())
    }

    const CHANGED_B: &str = "related_test_changed_in_diff: tests/test_b.py:20";

    fn rows() -> Vec<RelatedTest> {
        vec![
            row("anchor", "tests/test_a.py", 10, OracleStrength::Medium),
            row("touched", "tests/test_b.py", 20, OracleStrength::Medium),
        ]
    }

    #[test]
    fn a_same_tier_row_in_a_changed_test_file_leads_the_python_headline() {
        assert_eq!(headline(&finding(rows(), &[CHANGED_B])), Some("touched"));
        assert_eq!(
            headline(&finding(
                rows(),
                &["related_test_changed_in_diff: tests\\test_b.py:20"]
            )),
            Some("touched")
        );
        assert_eq!(headline(&finding(rows(), &[])), Some("anchor"));
        assert_eq!(headline(&finding(Vec::new(), &[CHANGED_B])), None);
    }

    #[test]
    fn other_languages_keep_the_anchor_row() {
        let mut subject = finding(rows(), &[CHANGED_B]);
        subject.language = Some(LanguageId::Rust);
        assert_eq!(headline(&subject), Some("anchor"));
        subject.language = None;
        assert_eq!(headline(&subject), Some("anchor"));
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
                headline(&finding(rows(), &[stamp])),
                Some("anchor"),
                "{stamp}"
            );
        }
    }

    #[test]
    fn a_changed_row_of_another_strength_or_observation_tier_never_leads() {
        let mut weaker = rows();
        weaker[1].oracle_strength = OracleStrength::Weak;
        assert_eq!(headline(&finding(weaker, &[CHANGED_B])), Some("anchor"));

        let mut unobserving = rows();
        unobserving[1].miss = Some(RelatedTestMiss::AssertionNotObserving);
        assert_eq!(
            headline(&finding(unobserving, &[CHANGED_B])),
            Some("anchor")
        );

        // The anchor itself is the first observing row, not the first listed.
        let mut blind_first = rows();
        blind_first[0].miss = Some(RelatedTestMiss::NoCallPath);
        assert_eq!(headline(&finding(blind_first, &[])), Some("touched"));
    }
}
