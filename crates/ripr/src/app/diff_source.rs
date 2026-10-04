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
/// reports uncommitted work (a tracked edit, or an untracked file a language
/// adapter reads); a clean tree, or a probe that could not run, keeps
/// committed history. An explicit `--base` does not change this: the base
/// names where the diff starts, not where it ends.
pub(crate) fn select_live_diff_source(root: &Path, request: DiffSourceRequest) -> LiveDiffSource {
    select_live_diff_source_with(request, || {
        crate::analysis::working_tree_has_uncommitted_changes(root)
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
