//! The route from a bare repository root to a work tree (#15297 review).
//!
//! `check`, `pilot`, `init` and `doctor` each refuse a bare repository root,
//! and each names the same next action, so one owner writes it. It sits
//! beside the analysis, which assembles the refusal, and the output and CLI
//! adapters, which print the line; the analysis may not import those.
//!
//! The commands name the repository's physical path, so they work from any
//! directory: a bare `git worktree add <path>` would act on whatever
//! repository the caller stands in. A repository whose `HEAD` resolves lends
//! a work tree through `--git-dir`; one with no commit has nothing to check
//! out, so the route is a clone, a first commit and a push; one whose `HEAD`
//! names no commit while another ref holds one is asked for that ref. `<path>`
//! is the new work-tree directory the user chooses.

use std::path::Path;
use std::time::Duration;

use crate::agent::loop_commands::shell_arg;
use crate::git::run_git_output_with_deadline;
use crate::output::path::{absolute_command_root_display, command_root_display};

/// How long each question put to Git about the repository may take. The
/// doctor gives each of its tools the same.
const ROUTE_TOOL_TIMEOUT: Duration = Duration::from_secs(5);

/// What Git says about the commit a bare repository can check out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BareHead {
    /// `HEAD` names a commit, which a new work tree checks out.
    Resolves,
    /// `HEAD` names nothing and no ref exists: the repository has no commit.
    Empty,
    /// `HEAD` names nothing while a ref holds a commit, which is the first
    /// such ref Git lists: a branch, a tag or a remote-tracking ref.
    Headless(String),
    /// Git did not answer, so none of the above can be claimed.
    Unconfirmed,
}

fn bare_head(root: &Path) -> BareHead {
    let resolved = run_git_output_with_deadline(
        root,
        &["rev-parse", "--verify", "--quiet", "HEAD"],
        Some(ROUTE_TOOL_TIMEOUT),
    );
    match resolved {
        Ok(output) if output.status.success() => return BareHead::Resolves,
        // Exit 1 is Git's own "no such revision"; 128 (not a repository, a
        // refused repository) claims nothing.
        Ok(output) if output.status.code() == Some(1) => {}
        Ok(_) | Err(_) => return BareHead::Unconfirmed,
    }
    // `HEAD` names nothing. That is an empty repository only when no ref at
    // all holds a commit: a bare repository can hold pushed branches while
    // `HEAD` still names the branch it was created with, and it can hold its
    // commit under a tag or a remote-tracking ref alone.
    let refs = run_git_output_with_deadline(
        root,
        &["for-each-ref", "--count=1", "--format=%(refname:short)"],
        Some(ROUTE_TOOL_TIMEOUT),
    );
    match refs {
        Ok(output) if output.status.success() => {
            let first = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if first.is_empty() {
                BareHead::Empty
            } else {
                BareHead::Headless(first)
            }
        }
        Ok(_) | Err(_) => BareHead::Unconfirmed,
    }
}

/// The route from a bare repository root to a work tree (#15297 review).
/// `<path>` is the new work-tree directory the user chooses. `Err` when the
/// root cannot be bound to one shell argument.
pub(crate) fn bare_root_work_tree_route(root: &Path) -> Result<String, String> {
    let bound = match root.canonicalize() {
        Ok(resolved) => command_root_display(root, &resolved)?,
        Err(_) => absolute_command_root_display(root)?,
    };
    let repository = shell_arg(&bound);
    let add = format!("git --git-dir={} worktree add <path>", shell_arg(&bound));
    require_single_line_command(&add)?;
    match bare_head(root) {
        BareHead::Resolves => Ok(format!(
            "Check out a work tree from it with `{add}` (`<path>` is the new work-tree \
             directory; the command works from any directory)"
        )),
        BareHead::Empty => Ok(format!(
            "This bare repository has no commits yet, so there is nothing to check out: \
             clone it with `git clone {repository} <path>`, make the first commit there and \
             push it"
        )),
        BareHead::Headless(reference) => Ok(format!(
            "This bare repository's `HEAD` names no commit, so a new work tree has nothing to \
             check out until it does: check out a commit that exists with `{add} {}` (`<path>` \
             is the new work-tree directory; the command works from any directory)",
            shell_arg(&reference)
        )),
        BareHead::Unconfirmed => Ok(format!(
            "Check out a work tree from it with `{add}` (`<path>` is the new work-tree \
             directory), or, if it has no commits yet, clone it with `git clone {repository} \
             <path>` and make the first commit there"
        )),
    }
}

/// [`bare_root_work_tree_route`] for a caller that prints a line, not a
/// `Result`: when the root cannot be bound, the route names no command.
pub(crate) fn bare_root_work_tree_route_or_plain(root: &Path) -> String {
    bare_root_work_tree_route(root).unwrap_or_else(|error| {
        format!("Check out a work tree from it ({error}), then use that work tree's root")
    })
}

/// A command with a line end in it is not one command that can be pasted.
pub(crate) fn require_single_line_command(line: &str) -> Result<(), String> {
    if line.contains('\n') || line.contains('\r') {
        Err(
            "selected root cannot be represented as one pasteable command line; rerun doctor from a parent using a single-line alias"
                .to_string(),
        )
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::path::human_path;
    use std::path::PathBuf;

    fn create(label: &str) -> Result<PathBuf, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ripr-work-tree-route-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).map_err(|error| format!("create {label}: {error}"))?;
        Ok(dir)
    }

    /// Run `git` in `dir`. A Git that does not answer is a fixture failure,
    /// not a finding about the route.
    fn git_in(dir: &Path, args: &[&str]) -> Result<(), String> {
        let output = run_git_output_with_deadline(dir, args, Some(ROUTE_TOOL_TIMEOUT))
            .map_err(|error| format!("git {args:?} in {}: {error}", dir.display()))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?} in {} exited {}: {}",
                dir.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    /// #15297 review: the work-tree route names the bare repository's own
    /// path, so it does not depend on the caller's directory, and it tells a
    /// repository without a commit apart from one with a commit.
    #[test]
    fn bare_root_work_tree_route_is_bound_and_knows_an_empty_repository() -> Result<(), String> {
        let bound = |root: &Path| -> Result<String, String> {
            let resolved = std::fs::canonicalize(root)
                .map_err(|error| format!("resolve {}: {error}", root.display()))?;
            Ok(shell_arg(&human_path(&resolved)))
        };

        // A bare repository with no commit: nothing to check out.
        let empty = create("empty")?;
        git_in(&empty, &["init", "--bare", "-b", "main", "."])?;
        assert_eq!(bare_head(&empty), BareHead::Empty);
        let route = bare_root_work_tree_route(&empty)?;
        assert!(route.contains("has no commits yet"), "{route}");
        assert!(
            route.contains(&format!("`git clone {} <path>`", bound(&empty)?)),
            "{route}"
        );
        assert!(!route.contains("worktree add"), "{route}");

        // The same repository after a commit is pushed: a work tree through
        // `--git-dir`, named with the repository's own path.
        let seed = create("seed")?;
        git_in(&seed, &["init", "-q", "-b", "main", "."])?;
        git_in(
            &seed,
            &[
                "-c",
                "user.email=ripr@example.invalid",
                "-c",
                "user.name=ripr tests",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "base",
            ],
        )?;
        git_in(&seed, &["push", "-q", &empty.display().to_string(), "main"])?;
        assert_eq!(bare_head(&empty), BareHead::Resolves);
        let route = bare_root_work_tree_route(&empty)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path>`",
                bound(&empty)?
            )),
            "{route}"
        );
        assert!(route.contains("from any directory"), "{route}");
        assert!(!route.contains("has no commits yet"), "{route}");
        assert!(!route.contains("git clone"), "{route}");

        // A bare repository whose `HEAD` still names the branch it was created
        // with, while a pushed branch holds the commit: not empty, and the
        // work tree needs that branch named.
        let misdirected = create("misdirected")?;
        git_in(&misdirected, &["init", "--bare", "-b", "main", "."])?;
        git_in(
            &seed,
            &[
                "push",
                "-q",
                &misdirected.display().to_string(),
                "main:refs/heads/feature",
            ],
        )?;
        assert_eq!(
            bare_head(&misdirected),
            BareHead::Headless("feature".to_owned())
        );
        let route = bare_root_work_tree_route(&misdirected)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path> feature`",
                bound(&misdirected)?
            )),
            "{route}"
        );
        assert!(route.contains("from any directory"), "{route}");
        assert!(!route.contains("has no commits yet"), "{route}");
        assert!(!route.contains("git clone"), "{route}");

        // Git refused the directory (an unreadable gitfile): the route names
        // both commands and claims neither state.
        let refused = create("refused")?;
        std::fs::write(refused.join(".git"), "not a gitfile\n")
            .map_err(|error| format!("write fixture gitfile: {error}"))?;
        assert_eq!(bare_head(&refused), BareHead::Unconfirmed);
        let route = bare_root_work_tree_route(&refused)?;
        assert!(route.contains("worktree add"), "{route}");
        assert!(route.contains("git clone"), "{route}");
        assert!(
            !route.contains("This bare repository has no commits yet"),
            "{route}"
        );
        assert!(!route.contains("names no commit"), "{route}");

        // A bare repository whose commit sits under a tag alone, or under a
        // remote-tracking ref alone: not empty, and the route names that ref.
        let tagged = create("tagged")?;
        git_in(&tagged, &["init", "--bare", "-q", "-b", "main", "."])?;
        git_in(
            &seed,
            &[
                "push",
                "-q",
                &tagged.display().to_string(),
                "main:refs/tags/v1",
            ],
        )?;
        assert_eq!(bare_head(&tagged), BareHead::Headless("v1".to_owned()));
        let route = bare_root_work_tree_route(&tagged)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path> v1`",
                bound(&tagged)?
            )),
            "{route}"
        );
        assert!(!route.contains("has no commits yet"), "{route}");

        let remote_only = create("remote-only")?;
        git_in(&remote_only, &["init", "--bare", "-q", "-b", "main", "."])?;
        git_in(
            &seed,
            &[
                "push",
                "-q",
                &remote_only.display().to_string(),
                "main:refs/remotes/origin/main",
            ],
        )?;
        assert_eq!(
            bare_head(&remote_only),
            BareHead::Headless("origin/main".to_owned())
        );
        let route = bare_root_work_tree_route(&remote_only)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path> origin/main`",
                bound(&remote_only)?
            )),
            "{route}"
        );

        // A spaced path stays one shell argument in the command, closed in
        // the quote the host's shell reads: `'` on Unix, `"` on Windows.
        let spaced = create("spaced path")?;
        git_in(&spaced, &["init", "--bare", "-q", "."])?;
        let route = bare_root_work_tree_route(&spaced)?;
        let quoted = bound(&spaced)?;
        assert!(
            route.contains(&format!("`git clone {quoted} <path>`")),
            "{route}"
        );
        assert!(
            matches!(quoted.chars().next(), Some('\'' | '"'))
                && matches!(quoted.chars().next_back(), Some('\'' | '"')),
            "a spaced path is quoted: {quoted}"
        );

        for dir in [
            empty,
            seed,
            misdirected,
            tagged,
            remote_only,
            refused,
            spaced,
        ] {
            let _ = std::fs::remove_dir_all(&dir);
        }
        Ok(())
    }
}
