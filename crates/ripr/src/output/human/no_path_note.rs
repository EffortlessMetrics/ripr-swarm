//! RIPR-SPEC-0090 / #15051: how the all-no-path closing note counts related
//! tests that have no call path to the changed code.

use crate::app::CheckOutput;
use crate::domain::{Finding, RelatedTestMiss};
use std::collections::BTreeSet;

/// The related-test clause of the note's `Scope analyzed:` sentence when
/// every related test is a name or location match with no call path.
///
/// A row the producer typed [`RelatedTestMiss::NoCallPath`] is not a traced
/// link, and the finding's own `Related test` line says so. Counting those
/// rows as "statically linked" right after "found no static test path" reads
/// as a contradiction. The absence wording is fail-closed: it needs at least
/// one retained row, every retained row to carry that typed miss, and no row
/// packed away (a packed row's miss is not available to check). Anything
/// else returns `None` and the caller keeps its neutral count.
pub(super) fn no_call_path_phrase(output: &CheckOutput) -> Option<String> {
    let any_packed = output
        .findings
        .iter()
        .any(|finding| finding.related_tests_total() > finding.related_tests.len());
    if any_packed {
        return None;
    }
    let mut rows = output
        .findings
        .iter()
        .flat_map(|finding: &Finding| finding.related_tests.iter())
        .peekable();
    rows.peek()?;
    let mut tests = BTreeSet::new();
    for test in rows {
        if test.miss != Some(RelatedTestMiss::NoCallPath) {
            return None;
        }
        tests.insert((test.file.as_path(), test.name.as_str(), test.line));
    }
    Some(format!(
        "{} related test(s) with no established call path to the changed code",
        tests.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::super::render;
    use super::super::tests::{bounded_output_with_findings, sample_finding};
    use crate::app::CheckOutput;
    use crate::domain::{
        ExposureClass, Finding, RelatedTest, RelatedTestMiss, RelationReason, StageState, Summary,
    };

    const NO_CALL_PATH: &str =
        "related test(s) with no established call path to the changed code.";
    const LINKED: &str = "statically linked related test(s)";

    fn no_path_finding(tests: Vec<RelatedTest>) -> Finding {
        let mut finding = sample_finding();
        finding.class = ExposureClass::NoStaticPath;
        finding.ripr.reach.state = StageState::No;
        finding.related_tests_matched_total = None;
        finding.related_tests = tests;
        finding
    }

    fn row(name: &str, miss: Option<RelatedTestMiss>) -> RelatedTest {
        let mut test = sample_finding().related_tests.remove(0);
        test.name = name.to_string();
        test.relation_reason = Some(RelationReason::SameTestFile);
        test.miss = miss;
        test
    }

    fn all_no_path(findings: Vec<Finding>) -> CheckOutput {
        let count = findings.len();
        let mut output = bounded_output_with_findings(findings);
        output.summary = Summary {
            probes: count,
            findings: count,
            no_static_path: count,
            ..Summary::default()
        };
        output
    }

    /// The note wraps at the terminal width; compare its words.
    fn flat_note(output: &CheckOutput) -> String {
        let rendered = render(output);
        let flat = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
        match flat.find("Note: ripr found no static test path") {
            Some(start) => flat[start..].to_string(),
            None => String::new(),
        }
    }

    #[test]
    fn name_or_location_matches_are_not_called_statically_linked() {
        let no_path = Some(RelatedTestMiss::NoCallPath);
        // The same test under two findings counts once.
        let output = all_no_path(vec![
            no_path_finding(vec![row("near_a", no_path), row("near_b", no_path)]),
            no_path_finding(vec![row("near_a", no_path)]),
        ]);
        let note = flat_note(&output);
        assert!(
            note.contains(&format!("2 changed expression(s), and 2 {NO_CALL_PATH}"))
                || note.contains(&format!("2 changed expression(s) and 2 {NO_CALL_PATH}")),
            "{note}"
        );
        assert!(!note.contains(LINKED), "{note}");
    }

    #[test]
    fn one_row_without_the_typed_miss_keeps_the_neutral_count() {
        for other in [None, Some(RelatedTestMiss::NoAssertion)] {
            let output = all_no_path(vec![no_path_finding(vec![
                row("near_a", Some(RelatedTestMiss::NoCallPath)),
                row("other", other),
            ])]);
            let note = flat_note(&output);
            assert!(note.contains(&format!("2 {LINKED}")), "{note}");
            assert!(!note.contains("no established call path"), "{note}");
        }
    }

    #[test]
    fn a_packed_away_row_keeps_the_neutral_count() {
        let mut finding = no_path_finding(vec![row("near_a", Some(RelatedTestMiss::NoCallPath))]);
        finding.related_tests_matched_total = Some(3);
        let note = flat_note(&all_no_path(vec![finding]));
        assert!(note.contains(LINKED), "{note}");
        assert!(!note.contains("no established call path"), "{note}");
    }

    #[test]
    fn no_related_test_keeps_the_zero_count() {
        let note = flat_note(&all_no_path(vec![no_path_finding(Vec::new())]));
        assert!(note.contains(&format!("0 {LINKED}")), "{note}");
        assert!(!note.contains("no established call path"), "{note}");
    }
}
