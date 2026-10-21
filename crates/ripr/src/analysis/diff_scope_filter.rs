//! Caller-chosen narrowing of a diff's changed-file set (#9277).
//!
//! `ripr check --path <glob>` / `--exclude <glob>` name the files an operator
//! wants analyzed. The filter is applied once, at pipeline intake, before the
//! diff-size guards and adapter dispatch, so the guards measure only the
//! scope the operator asked for. A file outside the scope is never silently
//! dropped: the pipeline records it as a typed `language_scope_unsupported`
//! limitation, which makes the outcome `partial_with_limitations`.

use super::path_glob::path_glob_matches;
use std::path::Path;

/// Repository-relative include and exclude patterns for one analysis run.
///
/// A pattern with no glob character (`*`, `?`) names a file or a directory:
/// `src/lib.rs` matches that file and `crates/api` matches everything under
/// it. A pattern with glob characters uses the shared `/`-separated dialect
/// (`**` spans segments, `*` and `?` stay inside one segment). No include
/// pattern admits every file the excludes do not remove.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffScopeFilter {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl DiffScopeFilter {
    /// Builds a filter, refusing a pattern that cannot name a repository file.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending pattern when it is empty,
    /// absolute, or contains a `..` segment.
    pub fn new(
        include: impl IntoIterator<Item = String>,
        exclude: impl IntoIterator<Item = String>,
    ) -> Result<Self, String> {
        Ok(Self {
            include: include
                .into_iter()
                .map(|pattern| normalize_pattern("--path", &pattern))
                .collect::<Result<_, _>>()?,
            exclude: exclude
                .into_iter()
                .map(|pattern| normalize_pattern("--exclude", &pattern))
                .collect::<Result<_, _>>()?,
        })
    }

    /// True when the filter admits every file (no pattern was supplied).
    pub fn is_unrestricted(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    /// Patterns a file must match at least one of; empty admits every file.
    pub fn include(&self) -> &[String] {
        &self.include
    }

    /// Patterns that remove a file even when an include pattern admits it.
    pub fn exclude(&self) -> &[String] {
        &self.exclude
    }

    /// Whether the filter keeps the repository-relative `path` in scope.
    pub fn admits(&self, path: &Path) -> bool {
        let path = path.to_string_lossy().replace('\\', "/");
        let included =
            self.include.is_empty() || self.include.iter().any(|p| pattern_matches(p, &path));
        included && !self.exclude.iter().any(|p| pattern_matches(p, &path))
    }

    /// The flags as the operator typed them, for refusal and limitation text.
    pub(crate) fn flags(&self) -> String {
        self.include
            .iter()
            .map(|pattern| format!("--path {pattern}"))
            .chain(
                self.exclude
                    .iter()
                    .map(|pattern| format!("--exclude {pattern}")),
            )
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Stable text bound into the analysis input identity, so the same diff
    /// under a different scope is a different analysis subject. Empty for an
    /// unrestricted filter, which keeps every existing identity unchanged.
    pub(crate) fn identity_suffix(&self) -> Option<String> {
        if self.is_unrestricted() {
            return None;
        }
        let mut include = self.include.clone();
        let mut exclude = self.exclude.clone();
        include.sort();
        exclude.sort();
        Some(format!(
            "\0ripr-diff-scope-filter-v1\0include={}\0exclude={}",
            include.join("\u{1f}"),
            exclude.join("\u{1f}")
        ))
    }
}

fn normalize_pattern(flag: &str, raw: &str) -> Result<String, String> {
    let normalized = raw.trim().replace('\\', "/");
    let trimmed = normalized.trim_start_matches("./").trim_end_matches('/');
    let invalid = |why: &str| {
        format!(
            "{flag} pattern {raw:?} {why}; pass a repository-relative path or glob such as \
             `src/lib.rs`, `crates/api` or `src/**/*.rs`"
        )
    };
    if trimmed.is_empty() {
        return Err(invalid("is empty"));
    }
    if trimmed.starts_with('/') || trimmed.as_bytes().get(1) == Some(&b':') {
        return Err(invalid("is absolute"));
    }
    if trimmed.split('/').any(|segment| segment == "..") {
        return Err(invalid("contains a `..` segment"));
    }
    Ok(trimmed.to_string())
}

fn pattern_matches(pattern: &str, path: &str) -> bool {
    let path = path.trim_start_matches("./");
    if pattern.contains(['*', '?']) {
        return path_glob_matches(pattern, path);
    }
    path == pattern
        || path
            .strip_prefix(pattern)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn filter(include: &[&str], exclude: &[&str]) -> Result<DiffScopeFilter, String> {
        DiffScopeFilter::new(
            include.iter().map(|p| p.to_string()),
            exclude.iter().map(|p| p.to_string()),
        )
    }

    #[test]
    fn an_empty_filter_admits_everything_and_leaves_identity_alone() -> Result<(), String> {
        let open = DiffScopeFilter::default();
        assert!(open.is_unrestricted());
        assert!(open.admits(Path::new("src/lib.rs")));
        assert_eq!(open.identity_suffix(), None);
        assert_eq!(filter(&[], &[])?, open);
        Ok(())
    }

    #[test]
    fn a_literal_pattern_names_a_file_or_a_directory_not_a_prefix() -> Result<(), String> {
        let scope = filter(&["src/lib.rs", "crates/api/"], &[])?;
        assert!(scope.admits(Path::new("src/lib.rs")));
        assert!(scope.admits(Path::new("crates/api/src/main.rs")));
        assert!(!scope.admits(Path::new("src/lib.rs.bak")));
        assert!(!scope.admits(Path::new("crates/api-extra/src/main.rs")));
        Ok(())
    }

    #[test]
    fn glob_patterns_use_the_shared_dialect() -> Result<(), String> {
        let scope = filter(&["src/**/*.rs"], &[])?;
        assert!(scope.admits(Path::new("src/a/b/c.rs")));
        assert!(scope.admits(Path::new("src/c.rs")));
        assert!(!scope.admits(Path::new("tests/c.rs")));
        Ok(())
    }

    #[test]
    fn exclude_wins_over_include_and_works_alone() -> Result<(), String> {
        let both = filter(&["src"], &["src/big.rs"])?;
        assert!(both.admits(Path::new("src/lib.rs")));
        assert!(!both.admits(Path::new("src/big.rs")));
        let only_exclude = filter(&[], &["vendor", "**/*.pb.rs"])?;
        assert!(only_exclude.admits(Path::new("src/lib.rs")));
        assert!(!only_exclude.admits(Path::new("vendor/dep/lib.rs")));
        assert!(!only_exclude.admits(Path::new("src/api.pb.rs")));
        Ok(())
    }

    #[test]
    fn a_pattern_that_cannot_name_a_repository_file_is_refused() {
        for bad in ["", "  ", "/etc/passwd", "C:/x", "../outside", "a/../b"] {
            let refusal = filter(&[bad], &[]).err().unwrap_or_default();
            assert!(refusal.contains("--path pattern"), "{bad:?}: {refusal}");
        }
        let refusal = filter(&[], &[".."]).err().unwrap_or_default();
        assert!(refusal.contains("--exclude pattern"), "{refusal}");
    }

    #[test]
    fn flags_and_identity_are_order_stable() -> Result<(), String> {
        let a = filter(&["b", "a"], &["z"])?;
        let b = filter(&["a", "b"], &["z"])?;
        assert_eq!(a.identity_suffix(), b.identity_suffix());
        assert_eq!(a.flags(), "--path b --path a --exclude z");
        assert_ne!(
            a.identity_suffix(),
            filter(&["a", "b"], &[])?.identity_suffix()
        );
        Ok(())
    }
}
