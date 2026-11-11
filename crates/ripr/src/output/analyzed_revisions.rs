//! Shared base/head labels for diff-scoped check headers.
//!
//! Every diff-scoped check format names the base and head it analyzed, so a
//! reader can tell a committed-history run (`HEAD <sha>`) from a working-tree
//! run (`working tree (uncommitted changes on HEAD <sha>)`) without
//! re-deriving it from flags. Human and GitHub text use these labels; JSON and
//! SARIF carry the same facts as fields ([`head_source`], full commits).

use crate::analysis::AnalyzedRevisions;

/// Length of the abbreviated commit ids in text headers.
const SHORT_COMMIT_LEN: usize = 7;

fn short(commit: &str) -> &str {
    commit.get(..SHORT_COMMIT_LEN).unwrap_or(commit)
}

/// `origin/main 1a2b3c4`, plus the merge base the diff started from when it
/// differs from the base tip, so a base that advanced after the branch forked
/// is not read as the diff origin.
pub(crate) fn base_label(revisions: &AnalyzedRevisions) -> String {
    let mut label = match revisions.base_commit.as_deref() {
        Some(commit) => format!("{} {}", revisions.base_ref, short(commit)),
        None => format!("{} (commit unresolved)", revisions.base_ref),
    };
    if let Some(merge_base) = revisions.merge_base_commit.as_deref() {
        label.push_str(&format!(" (diff from merge base {})", short(merge_base)));
    }
    label
}

/// `HEAD 5d6e7f8` for a committed-history run, or
/// `working tree (uncommitted changes on HEAD 5d6e7f8)` for a working-tree
/// run.
pub(crate) fn head_label(revisions: &AnalyzedRevisions) -> String {
    match (revisions.head_commit.as_deref(), revisions.working_tree) {
        (Some(commit), false) => format!("HEAD {}", short(commit)),
        (Some(commit), true) => {
            format!(
                "working tree (uncommitted changes on HEAD {})",
                short(commit)
            )
        }
        (None, false) => "HEAD (commit unresolved)".to_string(),
        (None, true) => "working tree (uncommitted changes; HEAD commit unresolved)".to_string(),
    }
}

/// Machine token for where the analyzed diff ended: `commit` or
/// `working_tree`.
pub(crate) fn head_source(revisions: &AnalyzedRevisions) -> &'static str {
    if revisions.working_tree {
        "working_tree"
    } else {
        "commit"
    }
}

/// `true` when the run's diff ended at the working tree (a default-selected
/// or `--worktree` read). Output notes use it to describe the working-tree
/// read (merge base to working tree) instead of a `<base>...HEAD` range.
pub(crate) fn is_working_tree_read(output: &crate::app::CheckOutput) -> bool {
    output
        .analyzed_revisions
        .as_ref()
        .is_some_and(|revisions| revisions.working_tree)
}

/// `true` when a committed-history run compared `HEAD` with a base that
/// already contains it: the merge base the diff started from is `HEAD`
/// itself while the base tip is another commit. The range is then empty by
/// construction (a branch that is behind its base, or already merged into
/// it), which is a different cause from "no file in a language ripr
/// analyzes". A working-tree read is excluded: its diff ends at the tree, not
/// at `HEAD`. Unresolved commits never establish it.
pub(crate) fn head_is_contained_in_base(revisions: &AnalyzedRevisions) -> bool {
    !revisions.working_tree
        && revisions.merge_base_commit.is_some()
        && revisions.merge_base_commit == revisions.head_commit
        && revisions.base_commit != revisions.head_commit
}

/// Names at most three paths, then `and N more`, after `display` escapes
/// each one for its surface.
pub(crate) fn name_paths(paths: &[String], display: impl Fn(&str) -> String) -> String {
    const NAMED_PATHS: usize = 3;
    let named = paths
        .iter()
        .take(NAMED_PATHS)
        .map(|path| display(path))
        .collect::<Vec<_>>();
    let more = paths.len().saturating_sub(NAMED_PATHS);
    if more > 0 {
        format!("{} and {more} more", named.join(", "))
    } else {
        named.join(", ")
    }
}

/// The disclosure for untracked routed source files on a working-tree read.
/// `git diff <merge-base>` covers tracked files only, so these files are not
/// in the analyzed diff; the remedy is intent-to-add or staging, never
/// `--worktree` (already in effect). `None` when no such file exists.
pub(crate) fn working_tree_untracked_message(
    paths: &[String],
    display: impl Fn(&str) -> String,
) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let listing = name_paths(paths, display);
    Some(format!(
        "Untracked files ({listing}) are not in the working-tree diff, which covers tracked \
         files only, so their behavior was not analyzed; run `git add -N <path>` (intent-to-add) \
         or stage them, then re-run `ripr check`."
    ))
}

/// The disclosure for gitignored routed source files on a working-tree read
/// (#7467). Like untracked files they are in neither diff, but plain
/// `git add -N` refuses ignored paths, so the remedy names the force
/// intent-to-add form. `None` when no such file exists.
pub(crate) fn working_tree_ignored_message(
    paths: &[String],
    display: impl Fn(&str) -> String,
) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let listing = name_paths(paths, display);
    Some(format!(
        "Gitignored files ({listing}) are not in the working-tree diff, which covers tracked \
         files only, so their behavior was not analyzed; run `git add -f -N <path>` (force \
         intent-to-add) or stage them with `git add -f`, then re-run `ripr check`."
    ))
}

/// The disclosure for gitignored routed source files on a committed-history
/// read (#7467). The committed diff covers tracked files only, so these
/// files were read as absent, never as evidence; the remedy is the same
/// force intent-to-add form as the working-tree arm. `None` when no such
/// file exists.
pub(crate) fn committed_history_ignored_message(
    paths: &[String],
    display: impl Fn(&str) -> String,
) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let listing = name_paths(paths, display);
    Some(format!(
        "Gitignored files ({listing}) are not in the committed-history diff, which covers \
         tracked files only, so their behavior was not analyzed; `git add -f <path>` and \
         commit them before re-running `ripr check --committed`, or run `git add -f -N <path>` \
         (force intent-to-add) and re-run `ripr check --worktree`."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #14265: only a committed-history run whose merge base is `HEAD`
    /// while the base tip is another commit establishes "HEAD is already
    /// contained in the base".
    #[test]
    fn head_is_contained_only_when_the_merge_base_is_head_and_the_base_is_ahead() {
        let contained = AnalyzedRevisions {
            base_ref: "origin/main".to_string(),
            base_commit: Some("b".repeat(40)),
            merge_base_commit: Some("a".repeat(40)),
            loaded_diff_origin: None,
            head_commit: Some("a".repeat(40)),
            working_tree: false,
        };
        assert!(head_is_contained_in_base(&contained));

        // The same commits read through the working tree end at the tree.
        let mut working_tree = contained.clone();
        working_tree.working_tree = true;
        assert!(!head_is_contained_in_base(&working_tree));

        // An ordinary feature branch: the merge base is behind HEAD.
        let mut diverged = contained.clone();
        diverged.head_commit = Some("c".repeat(40));
        assert!(!head_is_contained_in_base(&diverged));

        // Base tip equal to HEAD is the #13036 state (no separate merge
        // base is recorded), not this one.
        let mut same = contained.clone();
        same.base_commit = Some("a".repeat(40));
        same.merge_base_commit = None;
        assert!(!head_is_contained_in_base(&same));
        let mut same_with_origin = contained.clone();
        same_with_origin.base_commit = Some("a".repeat(40));
        assert!(!head_is_contained_in_base(&same_with_origin));

        // Unresolved commits never establish it, including two unknowns
        // that would otherwise compare equal.
        let mut no_merge_base = contained.clone();
        no_merge_base.merge_base_commit = None;
        assert!(!head_is_contained_in_base(&no_merge_base));
        let mut no_head = contained.clone();
        no_head.head_commit = None;
        assert!(!head_is_contained_in_base(&no_head));
        let mut neither = contained;
        neither.merge_base_commit = None;
        neither.head_commit = None;
        assert!(!head_is_contained_in_base(&neither));
    }

    fn revisions(working_tree: bool) -> AnalyzedRevisions {
        AnalyzedRevisions {
            base_ref: "origin/main".to_string(),
            base_commit: Some("1a2b3c4d5e6f".to_string()),
            merge_base_commit: None,
            loaded_diff_origin: None,
            head_commit: Some("5d6e7f8a9b0c".to_string()),
            working_tree,
        }
    }

    #[test]
    fn labels_name_ref_short_commits_and_the_head_source() {
        assert_eq!(base_label(&revisions(false)), "origin/main 1a2b3c4");
        assert_eq!(head_label(&revisions(false)), "HEAD 5d6e7f8");
        assert_eq!(
            head_label(&revisions(true)),
            "working tree (uncommitted changes on HEAD 5d6e7f8)"
        );
        assert_eq!(head_source(&revisions(false)), "commit");
        assert_eq!(head_source(&revisions(true)), "working_tree");
    }

    #[test]
    fn labels_disclose_a_distinct_merge_base_and_unresolved_commits() {
        let mut moved = revisions(false);
        moved.merge_base_commit = Some("9f8e7d6c5b4a".to_string());
        assert_eq!(
            base_label(&moved),
            "origin/main 1a2b3c4 (diff from merge base 9f8e7d6)"
        );
        let unresolved = AnalyzedRevisions {
            base_ref: "main".to_string(),
            working_tree: true,
            ..AnalyzedRevisions::default()
        };
        assert_eq!(base_label(&unresolved), "main (commit unresolved)");
        assert_eq!(
            head_label(&unresolved),
            "working tree (uncommitted changes; HEAD commit unresolved)"
        );
    }
}
