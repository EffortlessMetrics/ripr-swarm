//! Default diff-source selection for live-repository checks (RIPR-SPEC-0116
//! amendment).
//!
//! `ripr check` diffs a base against either committed history
//! (`<base>...HEAD`) or the live working tree (`git diff <merge-base>`,
//! staged and unstaged tracked edits included). This module owns the one
//! decision of which source a run reads when the caller did not force one,
//! so every command that wants `check`'s default (`ripr check` itself and
//! `ripr pilot`) answers it the same way instead of re-deriving it in a CLI
//! adapter.

use std::path::Path;

/// How the caller asked a live-repository diff to be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffSourceRequest {
    /// No flag: read the working tree when it holds uncommitted work.
    Default,
    /// `--worktree`: always read the working tree.
    WorkingTree,
    /// `--committed`: always read committed history only.
    CommittedOnly,
}

/// The diff source a live-repository run reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveDiffSource {
    /// `git diff <base>...HEAD`: committed history, files read as committed
    /// at `HEAD`.
    CommittedHistory,
    /// `git diff <merge-base>`: committed history plus staged and unstaged
    /// tracked edits, files read from the working tree.
    WorkingTree,
}

impl LiveDiffSource {
    /// `true` for the working-tree source: the analysis runs the worktree
    /// pipeline and drill-in commands must carry `--worktree` to reproduce
    /// the same subject.
    pub(crate) fn is_working_tree(self) -> bool {
        matches!(self, Self::WorkingTree)
    }
}

/// Choose the diff source for a run that diffs the live repository.
///
/// Only call this for live-repository diff runs: a `--diff` file, stdin, a
/// `--candidate-tree` subject, and repo-scope formats have no live diff
/// source to choose. With [`DiffSourceRequest::Default`], the working tree is
/// read when [`crate::analysis::working_tree_has_uncommitted_changes`]
/// reports uncommitted work (a staged or unstaged tracked edit; untracked
/// files never count, because the working-tree diff cannot contain them); a
/// clean or untracked-only tree, or a probe that could not run, keeps
/// committed history, as does an unborn or dangling `HEAD` (its staged files
/// read as additions, and the committed-history loader owns that refusal). An explicit `--base` does not change this: the base
/// names where the diff starts, not where it ends.
pub(crate) fn select_live_diff_source(
    root: &Path,
    request: DiffSourceRequest,
    git_timeout: Option<std::time::Duration>,
) -> LiveDiffSource {
    select_live_diff_source_with(request, || {
        crate::analysis::working_tree_has_uncommitted_changes(root, git_timeout)
    })
}

/// [`select_live_diff_source`] with the dirtiness probe injected, so the
/// selection rule is testable without a Git fixture. The probe runs only for
/// the default request.
pub(crate) fn select_live_diff_source_with(
    request: DiffSourceRequest,
    working_tree_is_dirty: impl FnOnce() -> bool,
) -> LiveDiffSource {
    match request {
        DiffSourceRequest::WorkingTree => LiveDiffSource::WorkingTree,
        DiffSourceRequest::CommittedOnly => LiveDiffSource::CommittedHistory,
        DiffSourceRequest::Default if working_tree_is_dirty() => LiveDiffSource::WorkingTree,
        DiffSourceRequest::Default => LiveDiffSource::CommittedHistory,
    }
}

/// The base a bare human `ripr check` rescopes to on a clean checkout of its
/// comparison base (#13644): the commit just made.
pub(crate) const LAST_COMMIT_BASE: &str = "HEAD~1";

/// Whether a bare human `ripr check` that produced `output` from its default
/// range rescopes to the last commit (#13644). The caller owns the flag
/// guards (no `--base`, `--scope`, `--diff`, `--worktree`, candidate tree or
/// artifact output, human format); this owns the output guards: nothing was
/// found or changed, no working-tree source sits outside the range, and the
/// default base names the checked-out branch, which has a parent.
///
/// `ripr check` and the default check that `ripr repair` and `ripr agent
/// stub` reconcile against (#16082) both read this one decision.
pub(crate) fn last_commit_fallback_applies(
    output: &crate::CheckOutput,
    root: &Path,
    git_timeout: Option<std::time::Duration>,
) -> bool {
    if !output.findings.is_empty()
        || output.unanalyzed_working_tree
        || !output.untracked_working_tree_source_paths.is_empty()
        || !output.ignored_working_tree_source_paths.is_empty()
        || output
            .analysis_outcome
            .as_ref()
            .is_none_or(|outcome| outcome.counts.changed_file_count != 0)
    {
        return false;
    }
    let Some(base) = output.base.as_deref() else {
        return false;
    };
    crate::analysis::base_is_checked_out_head_with_parent(root, base, git_timeout)
}

/// The last commit's diff text, when a bare human `ripr check` that read
/// the committed-history default range (`default_text`, against
/// `default_base`) would analyze the last commit ([`LAST_COMMIT_BASE`])
/// instead (#13644, #16082). For readers that need check's range without
/// running check (`ripr repair`'s change scoping, `ripr pilot`'s current
/// change); the caller has already established that no `--base` was given
/// and that the committed-history source was selected.
///
/// It applies [`last_commit_fallback_applies`]'s guards at the diff level:
/// the default range changes no file, no untracked or ignored routed source
/// sits outside it, the default base names the checked-out branch with a
/// parent, and the last commit changes a file. A guard that cannot be read
/// keeps the default range, which is what these readers did before #16082.
/// The diff level cannot see check's findings or its adapter-specific drop
/// of never-analyzed untracked paths, so in those rare states it keeps the
/// default range where check might still rescope; it never rescopes where
/// check would not.
pub(crate) fn last_commit_fallback_diff(
    root: &Path,
    default_text: &str,
    default_base: Option<&str>,
    git_timeout: Option<std::time::Duration>,
) -> Option<String> {
    if !crate::analysis::parse_unified_diff(default_text).is_empty() {
        return None;
    }
    let base = default_base?;
    if !crate::analysis::base_is_checked_out_head_with_parent(root, base, git_timeout) {
        return None;
    }
    let (untracked, ignored) =
        crate::analysis::committed_source::untracked_and_ignored_routed_paths(root, git_timeout)
            .ok()?;
    if !untracked.is_empty() || !ignored.is_empty() {
        return None;
    }
    let last = crate::analysis::load_diff_with_effective_base_core(
        root,
        Some(LAST_COMMIT_BASE),
        None,
        git_timeout,
    )
    .ok()?;
    (!crate::analysis::parse_unified_diff(&last.text).is_empty()).then_some(last.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_reads_the_working_tree_only_when_it_is_dirty() {
        assert_eq!(
            select_live_diff_source_with(DiffSourceRequest::Default, || true),
            LiveDiffSource::WorkingTree
        );
        assert_eq!(
            select_live_diff_source_with(DiffSourceRequest::Default, || false),
            LiveDiffSource::CommittedHistory
        );
    }

    #[test]
    fn only_the_working_tree_source_reports_working_tree() {
        assert!(LiveDiffSource::WorkingTree.is_working_tree());
        assert!(!LiveDiffSource::CommittedHistory.is_working_tree());
    }

    #[test]
    fn forced_sources_win_without_running_the_probe() {
        let ran = std::cell::Cell::new(false);
        let probe = || {
            ran.set(true);
            true
        };
        assert_eq!(
            select_live_diff_source_with(DiffSourceRequest::CommittedOnly, probe),
            LiveDiffSource::CommittedHistory
        );
        assert_eq!(
            select_live_diff_source_with(DiffSourceRequest::WorkingTree, || {
                ran.set(true);
                false
            }),
            LiveDiffSource::WorkingTree
        );
        assert!(
            !ran.get(),
            "a forced source must not spawn the dirtiness probe"
        );
    }
}
