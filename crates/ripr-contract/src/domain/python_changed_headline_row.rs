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
}
