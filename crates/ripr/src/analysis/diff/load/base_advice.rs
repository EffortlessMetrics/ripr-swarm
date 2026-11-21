//! #15484: advice for an unresolved base that a branch fetch cannot supply
//! (a commit id or a version tag), and the spelling of a remote-tracking ref
//! the advice quotes.

use std::path::Path;
use std::time::Duration;

/// The shape of a missing ref that no branch fetch brings in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NonBranchRef {
    /// 7 to 64 hex digits. `full` when it has the length of a whole SHA-1
    /// (40) or SHA-256 (64) object id, which a server can be asked for.
    CommitId { full: bool },
    /// `v` followed by a digit, the common release-tag spelling.
    VersionTag,
}

/// Classifies a missing ref by spelling alone. Only these narrow shapes are
/// matched, and each allows only characters that are safe to quote inside
/// a suggested command; every other name keeps the branch advice.
pub(super) fn non_branch_kind(missing_ref: &str) -> Option<NonBranchRef> {
    let len = missing_ref.len();
    if (7..=64).contains(&len) && missing_ref.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Some(NonBranchRef::CommitId {
            full: len == 40 || len == 64,
        });
    }
    let mut chars = missing_ref.chars();
    let tag_like = chars.next() == Some('v')
        && chars.next().is_some_and(|cell| cell.is_ascii_digit())
        && missing_ref
            .chars()
            .all(|cell| cell.is_ascii_alphanumeric() || matches!(cell, '.' | '-' | '_' | '+'))
        && !missing_ref.contains("..")
        && !missing_ref.ends_with('.');
    tag_like.then_some(NonBranchRef::VersionTag)
}

/// The remote a suggested fetch names: the only plain remote, else `origin`
/// when configured. `None` when no remote is configured. An unknown remote
/// list, or several remotes without `origin`, names no remote.
pub(super) enum FetchRemote<'a> {
    Named(&'a str),
    Unnamed,
    NoneConfigured,
}

pub(super) fn fetch_remote(remotes: Option<&[String]>) -> FetchRemote<'_> {
    match remotes {
        Some([]) => FetchRemote::NoneConfigured,
        Some([only]) if super::remote_name_is_plain(only) => FetchRemote::Named(only),
        Some(list) if list.iter().any(|remote| remote == "origin") => FetchRemote::Named("origin"),
        _ => FetchRemote::Unnamed,
    }
}

/// The repair sentence for a missing commit id or version tag. It ends
/// without a full stop so the caller can append its own alternative.
pub(super) fn non_branch_advice(
    missing_ref: &str,
    kind: NonBranchRef,
    remote: FetchRemote<'_>,
    shallow: bool,
) -> String {
    let remote = match remote {
        FetchRemote::NoneConfigured => {
            return match kind {
                NonBranchRef::CommitId { .. } => format!(
                    "`{missing_ref}` looks like a commit id, and no git remote is configured, so \
                     no fetch can supply it: check the id with `git log --oneline --all`"
                ),
                NonBranchRef::VersionTag => format!(
                    "`{missing_ref}` looks like a tag, and no git remote is configured, so no \
                     fetch can supply it: `git tag --list` shows this repository's tags"
                ),
            };
        }
        FetchRemote::Named(name) => format!(" {name}"),
        FetchRemote::Unnamed => " <remote>".to_string(),
    };
    match kind {
        NonBranchRef::CommitId { full: true } if shallow => format!(
            "`{missing_ref}` is a commit id and this is a shallow clone: fetch the full history \
             with `git fetch --unshallow{remote}`, or fetch that commit alone with `git \
             fetch{remote} {missing_ref}`"
        ),
        NonBranchRef::CommitId { full: true } => format!(
            "`{missing_ref}` is a commit id, which a branch fetch does not bring in: fetch it by \
             id with `git fetch{remote} {missing_ref}`"
        ),
        NonBranchRef::CommitId { full: false } if shallow => format!(
            "`{missing_ref}` looks like an abbreviated commit id and this is a shallow clone: \
             fetch the full history with `git fetch --unshallow{remote}`, or fetch that commit by \
             its full id with `git fetch{remote} <full commit id>`"
        ),
        NonBranchRef::CommitId { full: false } => format!(
            "`{missing_ref}` looks like an abbreviated commit id, which a branch fetch does not \
             bring in and Git fetches only by its full id: run `git fetch{remote} <full commit \
             id>`"
        ),
        NonBranchRef::VersionTag => format!(
            "`{missing_ref}` looks like a tag, which a branch fetch does not bring in: fetch tags \
             with `git fetch --tags{remote}`"
        ),
    }
}

/// The repair for a missing commit id or version tag, probing the repo's
/// remotes and depth. `None` for every other spelling. A branch that merely
/// looks like a tag or an id (`v2`, `deadbeef`) and already exists as a
/// fetched remote-tracking ref keeps that followable advice.
pub(super) fn non_branch_ref_repair(
    root: &Path,
    missing_ref: &str,
    flag: &str,
    git_timeout: Option<Duration>,
) -> Option<String> {
    let kind = non_branch_kind(missing_ref)?;
    let remotes = super::configured_remotes(root, git_timeout);
    if let Some(advice) =
        super::remote_tracking_advice(root, missing_ref, flag, remotes.as_deref(), git_timeout)
    {
        return Some(advice);
    }
    let shallow = super::is_shallow_repository(root, git_timeout);
    Some(non_branch_advice(
        missing_ref,
        kind,
        fetch_remote(remotes.as_deref()),
        shallow,
    ))
}

/// The spelling to quote for a resolvable `refs/remotes/<remote>/<ref>`:
/// the short `<remote>/<ref>` when Git resolves that short name to exactly
/// this ref, the full ref otherwise (a tag or local branch of the short name
/// shadows it, or the probe cannot answer).
pub(super) fn tracking_ref_spelling(
    root: &Path,
    full: &str,
    git_timeout: Option<Duration>,
) -> String {
    let Some(short) = full.strip_prefix("refs/remotes/") else {
        return full.to_string();
    };
    let resolves_to_full = crate::git::run_git_output_with_deadline(
        root,
        &["rev-parse", "--symbolic-full-name", short],
        git_timeout,
    )
    .ok()
    .filter(|output| output.status.success())
    .is_some_and(|output| String::from_utf8_lossy(&output.stdout).trim() == full);
    if resolves_to_full {
        short.to_string()
    } else {
        full.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{FetchRemote, NonBranchRef, fetch_remote, non_branch_advice, non_branch_kind};

    #[test]
    fn commit_ids_and_version_tags_are_told_apart_from_branch_names() {
        let full = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            non_branch_kind(full),
            Some(NonBranchRef::CommitId { full: true })
        );
        assert_eq!(
            non_branch_kind(&"a".repeat(64)),
            Some(NonBranchRef::CommitId { full: true })
        );
        assert_eq!(
            non_branch_kind("deadbee"),
            Some(NonBranchRef::CommitId { full: false })
        );
        for tag in ["v1", "v1.2.3", "v0.11.0-rc.1", "v2_beta+build"] {
            assert_eq!(
                non_branch_kind(tag),
                Some(NonBranchRef::VersionTag),
                "{tag}"
            );
        }
        // Branch names, short hex, offsets, ranges and hostile spellings keep
        // the branch advice.
        for name in [
            "main",
            "feature/v1",
            "dead",
            "abcdef",
            "vnext",
            "v",
            "v1..v2",
            "v1~1",
            "v1/x",
            "v1;rm",
            "v1.",
            "HEAD",
        ] {
            assert_eq!(non_branch_kind(name), None, "{name}");
        }
    }

    #[test]
    fn the_fetch_names_one_plain_remote_or_origin_and_never_invents_one() {
        let one = vec!["fork".to_string()];
        assert!(matches!(
            fetch_remote(Some(&one)),
            FetchRemote::Named("fork")
        ));
        let two = vec!["upstream".to_string(), "origin".to_string()];
        assert!(matches!(
            fetch_remote(Some(&two)),
            FetchRemote::Named("origin")
        ));
        let other = vec!["a".to_string(), "b".to_string()];
        assert!(matches!(fetch_remote(Some(&other)), FetchRemote::Unnamed));
        let hostile = vec!["-x".to_string()];
        assert!(matches!(fetch_remote(Some(&hostile)), FetchRemote::Unnamed));
        assert!(matches!(
            fetch_remote(Some(&[])),
            FetchRemote::NoneConfigured
        ));
        assert!(matches!(fetch_remote(None), FetchRemote::Unnamed));
    }

    #[test]
    fn a_commit_id_is_fetched_by_id_and_a_tag_with_tags() {
        let full = "0123456789abcdef0123456789abcdef01234567";
        let advice = non_branch_advice(
            full,
            NonBranchRef::CommitId { full: true },
            FetchRemote::Named("origin"),
            false,
        );
        assert!(
            advice.contains(&format!("`git fetch origin {full}`")),
            "{advice}"
        );
        assert!(!advice.contains("unshallow"), "{advice}");

        let shallow = non_branch_advice(
            full,
            NonBranchRef::CommitId { full: true },
            FetchRemote::Named("origin"),
            true,
        );
        assert!(
            shallow.contains("`git fetch --unshallow origin`"),
            "{shallow}"
        );
        assert!(
            shallow.contains(&format!("`git fetch origin {full}`")),
            "{shallow}"
        );

        // An abbreviated id cannot be fetched as typed: name the full id.
        let short = non_branch_advice(
            "deadbee",
            NonBranchRef::CommitId { full: false },
            FetchRemote::Named("origin"),
            false,
        );
        assert!(
            short.contains("`git fetch origin <full commit id>`"),
            "{short}"
        );
        assert!(!short.contains("git fetch origin deadbee"), "{short}");

        let tag = non_branch_advice(
            "v1.2.3",
            NonBranchRef::VersionTag,
            FetchRemote::Unnamed,
            false,
        );
        assert!(tag.contains("`git fetch --tags <remote>`"), "{tag}");

        let none = non_branch_advice(
            "v1.2.3",
            NonBranchRef::VersionTag,
            FetchRemote::NoneConfigured,
            false,
        );
        assert!(none.contains("no git remote is configured"), "{none}");
        assert!(!none.contains("git fetch"), "{none}");
    }
}
