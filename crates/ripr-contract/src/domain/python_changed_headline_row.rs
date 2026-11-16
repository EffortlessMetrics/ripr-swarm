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
