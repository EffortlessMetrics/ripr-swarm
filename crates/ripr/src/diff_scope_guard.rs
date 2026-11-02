//! Typed diff-scope guard authority (#4861 / ERR3 under #2688).
//!
//! Oversized diff scope is a structured internal fact. Human text starting with
//! `diff_scope_oversized:` is rendered only at [`Display`] boundaries; semantic
//! consumers match [`CoreError::is_diff_scope_oversized`], never message prefixes.

use std::fmt;

/// Stable wire/LSP component-outcome kind (no trailing colon).
pub(crate) const DIFF_SCOPE_OVERSIZED_KIND: &str = "diff_scope_oversized";

/// Legacy display prefix rendered at output boundaries.
pub(crate) const DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX: &str = "diff_scope_oversized:";

const DIFF_FILE_LIMIT_ENV: &str = "RIPR_MAX_DIFF_INDEX_FILES";
const DIFF_CHANGED_LINE_LIMIT_ENV: &str = "RIPR_MAX_DIFF_CHANGED_LINES";
const DIFF_INDEX_FILE_LIMIT_ENV: &str = "RIPR_MAX_DIFF_INDEX_FILES";
const DIFF_CHANGED_RUST_LINE_LIMIT_ENV: &str = "RIPR_MAX_DIFF_CHANGED_RUST_LINES";
pub(crate) const DIFF_CHANGED_PYTHON_LINE_LIMIT_ENV: &str = "RIPR_MAX_DIFF_CHANGED_PYTHON_LINES";
pub(crate) const DIFF_CHANGED_TYPESCRIPT_LINE_LIMIT_ENV: &str =
    "RIPR_MAX_DIFF_CHANGED_TYPESCRIPT_LINES";

/// How the oversized input reached the guard (#4861).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffInputRoute {
    /// Bounded unified-diff parse (file, git load, candidate subject, stdin).
    UnifiedDiffParse,
    /// Rust adapter index admission after mode selection.
    RustIndexAdmission,
    /// Open-editor Rust paths admitted before tracking probe.
    RustOpenFilesAdmission,
    /// Changed-line budget before probe expansion (Rust or preview language).
    ChangedLineBudget,
}

/// Which budget fired; carries the facts needed to render legacy-compatible text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiffScopeGuardKind {
    ParserAtLeastChangedFiles {
        configured_limit: usize,
        observed_at_least: usize,
    },
    ParserAtLeastChangedLines {
        configured_limit: usize,
        observed_at_least: usize,
    },
    RustIndexedFiles {
        files: usize,
        scope_limit: usize,
    },
    RustOpenFilesAdmitted {
        files: usize,
        scope_limit: usize,
    },
    RustChangedLines {
        changed_line_count: usize,
        changed_file_count: usize,
        line_limit: usize,
    },
    PreviewLanguageChangedLines {
        language_label: &'static str,
        limit_env: &'static str,
        changed_line_count: usize,
        changed_file_count: usize,
        line_limit: usize,
    },
}

/// Typed oversized diff scope with governed limit/count/route data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffScopeOversized {
    pub(crate) route: DiffInputRoute,
    pub(crate) kind: DiffScopeGuardKind,
}

impl DiffScopeOversized {
    pub(crate) fn parser_at_least_changed_files(
        configured_limit: usize,
        observed_at_least: usize,
    ) -> Self {
        Self {
            route: DiffInputRoute::UnifiedDiffParse,
            kind: DiffScopeGuardKind::ParserAtLeastChangedFiles {
                configured_limit,
                observed_at_least,
            },
        }
    }

    pub(crate) fn parser_at_least_changed_lines(
        configured_limit: usize,
        observed_at_least: usize,
    ) -> Self {
        Self {
            route: DiffInputRoute::UnifiedDiffParse,
            kind: DiffScopeGuardKind::ParserAtLeastChangedLines {
                configured_limit,
                observed_at_least,
            },
        }
    }

    pub(crate) fn rust_indexed_files(files: usize, scope_limit: usize) -> Self {
        Self {
            route: DiffInputRoute::RustIndexAdmission,
            kind: DiffScopeGuardKind::RustIndexedFiles { files, scope_limit },
        }
    }

    pub(crate) fn rust_open_files_admitted(files: usize, scope_limit: usize) -> Self {
        Self {
            route: DiffInputRoute::RustOpenFilesAdmission,
            kind: DiffScopeGuardKind::RustOpenFilesAdmitted { files, scope_limit },
        }
    }

    pub(crate) fn rust_changed_lines(
        changed_line_count: usize,
        changed_file_count: usize,
        line_limit: usize,
    ) -> Self {
        Self {
            route: DiffInputRoute::ChangedLineBudget,
            kind: DiffScopeGuardKind::RustChangedLines {
                changed_line_count,
                changed_file_count,
                line_limit,
            },
        }
    }

    pub(crate) fn preview_language_changed_lines(
        language_label: &'static str,
        limit_env: &'static str,
        changed_line_count: usize,
        changed_file_count: usize,
        line_limit: usize,
    ) -> Self {
        Self {
            route: DiffInputRoute::ChangedLineBudget,
            kind: DiffScopeGuardKind::PreviewLanguageChangedLines {
                language_label,
                limit_env,
                changed_line_count,
                changed_file_count,
                line_limit,
            },
        }
    }

    /// `--json` limited-artifact `basis` for this guard (#10730).
    pub(crate) fn json_basis(&self) -> &'static str {
        match &self.kind {
            DiffScopeGuardKind::PreviewLanguageChangedLines { limit_env, .. } => {
                if *limit_env == DIFF_CHANGED_PYTHON_LINE_LIMIT_ENV {
                    "python_diff_scope_budget"
                } else if *limit_env == DIFF_CHANGED_TYPESCRIPT_LINE_LIMIT_ENV {
                    "typescript_diff_scope_budget"
                } else {
                    "rust_diff_scope_budget"
                }
            }
            _ => "rust_diff_scope_budget",
        }
    }
}

impl fmt::Display for DiffScopeOversized {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            DiffScopeGuardKind::ParserAtLeastChangedFiles {
                configured_limit,
                observed_at_least,
            } => write!(
                formatter,
                "{DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX} at least {observed_at_least} changed files exceed the \
                 {DIFF_FILE_LIMIT_ENV} limit ({configured_limit}); parsing stopped before the \
                 remaining file bodies and analysis was not run. Repair route: reduce \
                 the diff scope, split the extraction PR, run a narrower diff, or \
                 raise the limit via {DIFF_FILE_LIMIT_ENV}=<number>."
            ),
            DiffScopeGuardKind::ParserAtLeastChangedLines {
                configured_limit,
                observed_at_least,
            } => write!(
                formatter,
                "{DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX} at least {observed_at_least} changed lines exceed the \
                 {DIFF_CHANGED_LINE_LIMIT_ENV} limit ({configured_limit}); parsing stopped \
                 before the remaining diff lines and analysis was not run. Repair \
                 route: reduce the diff scope, split the extraction PR, run a \
                 narrower diff, or raise the limit via \
                 {DIFF_CHANGED_LINE_LIMIT_ENV}=<number>."
            ),
            DiffScopeGuardKind::RustIndexedFiles { files, scope_limit } => write!(
                formatter,
                "{DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX} {files} indexed Rust files exceed the \
                 {DIFF_INDEX_FILE_LIMIT_ENV} limit ({scope_limit}); analysis was not run to \
                 protect runner memory. Repair route: reduce the diff scope, run a narrower \
                 mode, or raise the limit via {DIFF_INDEX_FILE_LIMIT_ENV}=<number>."
            ),
            DiffScopeGuardKind::RustOpenFilesAdmitted { files, scope_limit } => write!(
                formatter,
                "{DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX} {files} admitted open Rust files exceed the \
                 {DIFF_INDEX_FILE_LIMIT_ENV} limit ({scope_limit}) before tracking; \
                 reduce the open-file scope or raise the limit"
            ),
            DiffScopeGuardKind::RustChangedLines {
                changed_line_count,
                changed_file_count,
                line_limit,
            } => write!(
                formatter,
                "{DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX} {changed_line_count} changed Rust lines across \
                 {changed_file_count} Rust files exceed the {DIFF_CHANGED_RUST_LINE_LIMIT_ENV} \
                 limit ({line_limit}); analysis was not run to protect runner memory before \
                 probe expansion. Repair route: reduce the diff scope, split the extraction \
                 PR, run a narrower diff, or raise the limit via \
                 {DIFF_CHANGED_RUST_LINE_LIMIT_ENV}=<number>. To analyze a subset, pass \
                 `--path <glob>` and/or `--exclude <glob>` to `ripr check` (repeatable, \
                 repository-relative; files left out are reported as not analyzed)."
            ),
            DiffScopeGuardKind::PreviewLanguageChangedLines {
                language_label,
                limit_env,
                changed_line_count,
                changed_file_count,
                line_limit,
            } => write!(
                formatter,
                "{DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX} {changed_line_count} changed {language_label} lines across \
                 {changed_file_count} {language_label} files exceed the {limit_env} limit ({line_limit}); \
                 analysis was not run to protect runner memory before probe expansion. Repair route: \
                 reduce the diff scope, split the extraction PR, run a narrower diff, or raise the \
                 limit via {limit_env}=<number>. To analyze a subset, pass `--path <glob>` and/or \
                 `--exclude <glob>` to `ripr check` (repeatable, repository-relative; files left out \
                 are reported as not analyzed)."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_error::CoreError;

    #[test]
    fn display_oracle_matches_parser_file_guard() {
        let guard = DiffScopeOversized::parser_at_least_changed_files(5, 6);
        assert_eq!(
            guard.to_string(),
            "diff_scope_oversized: at least 6 changed files exceed the \
             RIPR_MAX_DIFF_INDEX_FILES limit (5); parsing stopped before the \
             remaining file bodies and analysis was not run. Repair route: reduce \
             the diff scope, split the extraction PR, run a narrower diff, or \
             raise the limit via RIPR_MAX_DIFF_INDEX_FILES=<number>."
        );
    }

    #[test]
    fn typed_guard_survives_context_without_prefix_matching() {
        let guard = CoreError::from(DiffScopeOversized::rust_changed_lines(3, 1, 2));
        let wrapped = guard.with_context("workspace analysis failed");
        assert!(wrapped.is_diff_scope_oversized());
        assert!(
            !wrapped
                .to_string()
                .starts_with(DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX),
            "wrapped Display must not start with the legacy prefix"
        );
    }

    #[test]
    fn lookalike_message_is_not_typed() {
        let lookalike = CoreError::message("diff_scope_oversized: forged prefix must not match");
        assert!(!lookalike.is_diff_scope_oversized());
    }

    #[test]
    fn prefix_matching_display_is_not_the_semantic_oracle() {
        let guard = CoreError::from(DiffScopeOversized::rust_indexed_files(900, 800));
        let wrapped = guard.with_context("workspace analysis failed");
        let lookalike = CoreError::message("diff_scope_oversized: forged");
        assert_ne!(
            wrapped.is_diff_scope_oversized(),
            wrapped
                .to_string()
                .trim_start()
                .starts_with(DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX)
        );
        assert_ne!(
            lookalike.is_diff_scope_oversized(),
            lookalike
                .to_string()
                .trim_start()
                .starts_with(DIFF_SCOPE_OVERSIZED_DISPLAY_PREFIX)
        );
    }
}
