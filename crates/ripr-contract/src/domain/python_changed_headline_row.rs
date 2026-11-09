//! Python headline related-test row selection (#14027).
//!
//! The Python adapter lists related rows in a stable file order, so the
//! `first()` row can be a test the diff never touched while a same-strength
//! row in a test file the same diff edited sits further down. The adapter
//! stamps one evidence line per related row that lives in a changed test
//! file (only when the first row is not already such a row); this selector
//! reads those lines. Display row selection only: class, oracle strength and
//! row order are not changed.

use super::{Finding, LanguageId, RelatedTest};

/// Evidence line prefix: `related_test_changed_in_diff: <file>:<line>`.
pub const RELATED_TEST_CHANGED_IN_DIFF_PREFIX: &str = "related_test_changed_in_diff: ";

/// The one related-test row every single-row Python projection names.
///
/// Among the rows with the same oracle strength as the first row, the first
/// row whose test file the diff changed leads; otherwise the first row.
/// Rows of another strength never displace the first row's tier, and an
/// unmatched stamp is ignored.
pub fn python_headline_related_test(finding: &Finding) -> Option<&RelatedTest> {
    let first = finding.related_tests.first()?;
    if !matches!(finding.language, Some(LanguageId::Python)) {
        return Some(first);
    }
    let tier = first.oracle_strength.rank();
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
            test.oracle_strength.rank() == tier
                && changed.iter().any(|(file, line)| {
                    *line == test.line && *file == test.file.to_string_lossy().replace('\\', "/")
                })
        })
        .or(Some(first))
}
