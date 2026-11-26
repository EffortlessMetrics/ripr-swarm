//! Whether a caller-reported focused run covered the test the attempt's edit
//! added or changed (#16311).
//!
//! The focused-run gate credits any executed test defined in the selected
//! file, so a passing sibling test satisfies it. This reading compares the
//! file's executable tests at attempt start with those at finish and asks
//! whether a ran test is among the added or changed ones. It fails closed: a
//! start text it cannot recover exactly, a file that does not parse, an edit
//! that changed no test item, or a libtest path it cannot attribute all read
//! as `unknown`, never as covered.

use super::{file_internal_path, file_module_path};
use crate::edit_cage::AttemptBaseline;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

/// Whether the reported run included a test the edit added or changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditedTestCoverage {
    /// A ran test is one the edit added or changed.
    Covered,
    /// The edit's added or changed tests are known and none of them ran.
    NotCovered,
    /// ripr could not tell which tests the edit added or changed.
    Unknown,
}

impl EditedTestCoverage {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Covered => "covered",
            Self::NotCovered => "not_covered",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "covered" => Some(Self::Covered),
            "not_covered" => Some(Self::NotCovered),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// The selected file's text when the attempt began: the baseline index blob,
/// used only when its bytes hash to the baseline worktree digest, so a file
/// that was dirty or normalized at start reads as unrecoverable.
pub(crate) fn baseline_source(
    root: &Path,
    baseline: &AttemptBaseline,
    file: &str,
) -> Option<String> {
    let digest = baseline.worktree_digest(file)?;
    let object = baseline.index_entry(file)?.split_whitespace().nth(1)?;
    let output = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["cat-file", "blob", object],
        Duration::from_mins(1),
        4 * 1024 * 1024,
    )
    .ok()?;
    if !output.status.success() || format!("{:x}", Sha256::digest(&output.stdout)) != digest {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// File-internal names of the executable tests the edit added or whose body
/// changed. `None` when either text does not parse.
pub(crate) fn edited_tests(file: &str, before: &str, after: &str) -> Option<BTreeSet<String>> {
    let path = Path::new(file);
    let before = crate::analysis::executable_test_function_bodies(path, before).ok()?;
    let after = crate::analysis::executable_test_function_bodies(path, after).ok()?;
    Some(
        after
            .into_iter()
            .filter(|(name, body)| before.get(name) != Some(body))
            .map(|(name, _)| name)
            .collect(),
    )
}

/// Whether any ran libtest name is one of `edited`.
pub(crate) fn coverage(
    file: &str,
    edited: Option<&BTreeSet<String>>,
    ran: &[String],
) -> EditedTestCoverage {
    let (Some(edited), Some(prefix)) = (edited, file_module_path(file)) else {
        return EditedTestCoverage::Unknown;
    };
    if edited.is_empty() {
        return EditedTestCoverage::Unknown;
    }
    let prefix = prefix.join("::");
    if ran
        .iter()
        .any(|name| file_internal_path(name, &prefix).is_some_and(|path| edited.contains(path)))
    {
        EditedTestCoverage::Covered
    } else {
        EditedTestCoverage::NotCovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEFORE: &str = "#[test]\nfn below() { assert_eq!(1, 1); }\n";

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn an_added_or_changed_test_is_edited_and_an_untouched_sibling_is_not() {
        let added = format!("{BEFORE}#[test]\nfn boundary() {{ assert_eq!(2, 2); }}\n");
        let edited = edited_tests("tests/p.rs", BEFORE, &added);
        assert_eq!(edited, Some(BTreeSet::from(["boundary".to_string()])));
        let changed = "#[test]\nfn below() { assert_eq!(1, 2); }\n";
        assert_eq!(
            edited_tests("tests/p.rs", BEFORE, changed),
            Some(BTreeSet::from(["below".to_string()]))
        );
        assert_eq!(
            edited_tests("tests/p.rs", BEFORE, BEFORE),
            Some(BTreeSet::new())
        );
        assert_eq!(edited_tests("tests/p.rs", BEFORE, "fn broken( {"), None);
    }

    #[test]
    fn only_a_ran_edited_test_covers_the_edit() {
        let edited = BTreeSet::from(["boundary".to_string()]);
        let file = "tests/p.rs";
        assert_eq!(
            coverage(file, Some(&edited), &names(&["below", "boundary"])),
            EditedTestCoverage::Covered
        );
        assert_eq!(
            coverage(file, Some(&edited), &names(&["below"])),
            EditedTestCoverage::NotCovered
        );
        assert_eq!(
            coverage(file, None, &names(&["boundary"])),
            EditedTestCoverage::Unknown
        );
        assert_eq!(
            coverage(file, Some(&BTreeSet::new()), &names(&["boundary"])),
            EditedTestCoverage::Unknown
        );
        // A file whose libtest path cannot be derived reads as unknown.
        assert_eq!(
            coverage("src/bin/tool.rs", Some(&edited), &names(&["boundary"])),
            EditedTestCoverage::Unknown
        );
        let inline = BTreeSet::from(["tests::boundary".to_string()]);
        assert_eq!(
            coverage(
                "src/pricing.rs",
                Some(&inline),
                &names(&["pricing::tests::boundary"])
            ),
            EditedTestCoverage::Covered
        );
        for coverage in [
            EditedTestCoverage::Covered,
            EditedTestCoverage::NotCovered,
            EditedTestCoverage::Unknown,
        ] {
            assert_eq!(EditedTestCoverage::parse(coverage.as_str()), Some(coverage));
        }
        assert_eq!(EditedTestCoverage::parse("yes"), None);
    }
}
