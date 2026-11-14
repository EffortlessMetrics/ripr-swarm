//! Preview-language path exclusion and fixture/data heuristics (#5380).
//!
//! `[languages.python]` and `[languages.typescript]` may declare
//! `excluded_file_patterns` so fixture or data trees stay out of preview
//! indexing. Parse-budget refusals on paths that look like fixture content
//! point operators at that config instead of advising a source rewrite.

use crate::analysis::path_glob::{path_glob_matches, segment_glob_matches};
use std::path::{Component, Path};

/// Directory-name markers that commonly identify fixture, golden, or data trees
/// rather than hand-written production source (#5380).
const FIXTURE_DIRECTORY_MARKERS: &[&str] = &[
    "__snapshots__",
    "cassettes",
    "corpus",
    "expected",
    "fixture",
    "fixtures",
    "golden",
    "goldens",
    "highlighted",
    "recordings",
    "snapshots",
    "syntax-tests",
    "syntax_tests",
    "test-data",
    "test_data",
    "testdata",
];

/// Whether `path` lies under a directory component that usually denotes
/// fixture or checked-in data rather than authored production code.
pub(crate) fn is_probable_fixture_or_data_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        let name = name.to_string_lossy();
        FIXTURE_DIRECTORY_MARKERS
            .iter()
            .any(|marker| name.eq_ignore_ascii_case(marker))
    })
}

/// Whether a repository-relative `path` matches any configured preview
/// exclusion glob (same dialect as Rust `generated_file_patterns`).
pub(crate) fn path_matches_preview_excluded_patterns(patterns: &[String], path: &Path) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let normalized_path = path.to_string_lossy().replace('\\', "/");
    patterns.iter().any(|pattern| {
        if pattern.contains('/') {
            path_glob_matches(pattern, &normalized_path)
        } else {
            path.file_name()
                .is_some_and(|name| segment_glob_matches(pattern, &name.to_string_lossy()))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_highlighted_fixture_path_is_probable_fixture() {
        assert!(is_probable_fixture_or_data_path(Path::new(
            "tests/syntax-tests/highlighted/Python/battest.py"
        )));
    }

    #[test]
    fn ordinary_src_path_is_not_probable_fixture() {
        assert!(!is_probable_fixture_or_data_path(Path::new("src/app.py")));
    }

    #[test]
    fn excluded_pattern_matches_repository_relative_globs() {
        let patterns = vec!["tests/syntax-tests/**".to_string()];
        assert!(path_matches_preview_excluded_patterns(
            &patterns,
            Path::new("tests/syntax-tests/highlighted/x.py")
        ));
        assert!(!path_matches_preview_excluded_patterns(
            &patterns,
            Path::new("src/app.py")
        ));
    }
}
