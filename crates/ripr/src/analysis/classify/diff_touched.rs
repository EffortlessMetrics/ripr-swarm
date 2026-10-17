//! Tests whose body lies in lines the diff adds or changes (#8387).
//!
//! A commit that edits production code and adds or edits a test in the same
//! diff has told us which test its author wrote for the change. That is a
//! ranking signal, not a relation: it never admits a test into a related-test
//! list and never raises a relation's confidence, so no classification,
//! reach credit or oracle credit can move because of it. It only decides which
//! of the tests that are already candidates leads.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::related_tests::normalize_path;
use crate::analysis::diff::ChangedFile;
use crate::analysis::facts::TestSummary;

/// The tests, by file and start line, that contain a line the diff adds or
/// changes (new-file side).
#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct DiffTouchedTests {
    starts: BTreeSet<(String, usize)>,
}

impl DiffTouchedTests {
    pub(in crate::analysis) fn from_changed_files<'a>(
        files: impl IntoIterator<Item = &'a ChangedFile>,
        tests: impl IntoIterator<Item = &'a TestSummary>,
    ) -> Self {
        let mut added_lines: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for file in files {
            if file.added_lines.is_empty() {
                continue;
            }
            let lines = added_lines.entry(normalize_path(&file.path)).or_default();
            lines.extend(file.added_lines.iter().map(|line| line.new_side_line));
        }
        for lines in added_lines.values_mut() {
            lines.sort_unstable();
            lines.dedup();
        }
        let mut starts = BTreeSet::new();
        if !added_lines.is_empty() {
            for test in tests {
                let file = normalize_path(&test.file);
                let Some(lines) = added_lines.get(&file) else {
                    continue;
                };
                let first_at_or_after = lines.partition_point(|line| *line < test.start_line);
                if lines
                    .get(first_at_or_after)
                    .is_some_and(|line| *line <= test.end_line)
                {
                    starts.insert((file, test.start_line));
                }
            }
        }
        Self { starts }
    }

    /// Whether the test starting at `start_line` of `file` contains a line
    /// this diff adds or changes. A pure removal touches no test.
    pub(in crate::analysis) fn touches(&self, file: &Path, start_line: usize) -> bool {
        !self.starts.is_empty() && self.starts.contains(&(normalize_path(file), start_line))
    }

    #[cfg(test)]
    pub(in crate::analysis) fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::diff::ChangedLine;
    use std::path::PathBuf;

    fn changed(path: &str, lines: &[usize]) -> ChangedFile {
        ChangedFile {
            path: PathBuf::from(path),
            added_lines: lines
                .iter()
                .map(|line| ChangedLine {
                    line: *line,
                    text: String::new(),
                    new_side_line: *line,
                })
                .collect(),
            removed_lines: Vec::new(),
        }
    }

    fn test_span(file: &str, start: usize, end: usize) -> TestSummary {
        TestSummary {
            name: format!("t{start}"),
            file: PathBuf::from(file),
            start_line: start,
            end_line: end,
            body: Default::default(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    #[test]
    fn given_added_line_inside_test_body_when_touching_then_that_test_only_is_touched() {
        let tests = [
            test_span("tests/a.rs", 10, 20),
            test_span("tests/a.rs", 30, 38),
            test_span("tests/a.rs", 40, 50),
            test_span("tests/b.rs", 10, 20),
        ];
        let files = [changed("tests/a.rs", &[12, 40])];
        let touched = DiffTouchedTests::from_changed_files(&files, &tests);
        assert!(touched.touches(Path::new("tests/a.rs"), 10), "body line 12");
        assert!(
            touched.touches(Path::new("tests/a.rs"), 40),
            "start line 40"
        );
        assert!(
            !touched.touches(Path::new("tests/a.rs"), 30),
            "no added line"
        );
        assert!(!touched.touches(Path::new("tests/b.rs"), 10), "other file");
    }

    #[test]
    fn given_removal_only_file_when_touching_then_no_test_is_touched() {
        let mut file = changed("tests/a.rs", &[]);
        file.removed_lines.push(ChangedLine {
            line: 15,
            text: String::new(),
            new_side_line: 15,
        });
        let tests = [test_span("tests/a.rs", 1, 100)];
        let files = [file];
        let touched = DiffTouchedTests::from_changed_files(&files, &tests);
        assert!(touched.is_empty());
        assert!(!touched.touches(Path::new("tests/a.rs"), 1));
    }
}
