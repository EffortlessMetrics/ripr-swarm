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
