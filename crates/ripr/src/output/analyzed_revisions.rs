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

#[cfg(test)]
mod tests {
    use super::*;

    fn revisions(working_tree: bool) -> AnalyzedRevisions {
        AnalyzedRevisions {
            base_ref: "origin/main".to_string(),
            base_commit: Some("1a2b3c4d5e6f".to_string()),
            merge_base_commit: None,
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
