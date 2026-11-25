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
    /// `HEAD` names nothing and no ref holds a commit: the repository has none.
    Empty,
    /// `HEAD` names nothing while a ref holds a commit: the first such ref in
    /// Git's own order that Git shows to peel to a commit, a branch, a tag or
    /// a remote-tracking ref; a ref that holds a blob or a tree is passed over,
    /// because a work tree cannot check it out. The string is the name the
    /// route gives `git worktree add` for that ref ([`checkout_name`]).
    Headless(String),
    /// As [`BareHead::Headless`], for the ref whose name holds bytes that are
    /// not UTF-8. No command this output can hold spells that name, so the
    /// work tree is detached at the commit. The string is the full ID of the
    /// commit the same Git listing reports beside the ref.
    Detached(String),
    /// Git did not answer, or no ref it lists peels to a commit while the
    /// listing was cut short, so none of the above can be claimed.
    Unconfirmed,
}

/// How many refs the route reads to find one that holds a commit.
const MAX_ROUTE_REFS: usize = 64;

fn bare_head(root: &Path) -> BareHead {
    // `HEAD^{commit}` succeeds only when `HEAD` peels to a commit.
    let resolved = run_git_output_with_deadline(
        root,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
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
    // commit under a tag or a remote-tracking ref alone. A ref can also hold
    // a blob or a tree, so each is read with the type of the object it names
    // and, for a tag, of the object it peels to, and with the ID of the
    // commit, for a ref whose name this output cannot spell.
    let count = format!("--count={MAX_ROUTE_REFS}");
    let refs = run_git_output_with_deadline(
        root,
        &[
            "for-each-ref",
            count.as_str(),
            "--format=%(refname) %(objecttype) %(*objecttype) %(objectname) %(*objectname)",
        ],
        Some(ROUTE_TOOL_TIMEOUT),
    );
    match refs {
        Ok(output) if output.status.success() => listed_head(&output.stdout),
        Ok(_) | Err(_) => BareHead::Unconfirmed,
    }
}

/// What a `for-each-ref` listing holds for a repository whose `HEAD` names
/// nothing, read as bytes: Git prints a ref's name as the bytes it holds, and
/// a name need not be UTF-8 (#15548 review). A ref name cannot hold a space,
/// a tab or a line end, so one line is one ref and a space ends a field.
fn listed_head(listing: &[u8]) -> BareHead {
    // Git ends its listing with a line feed; a line without it is read too.
    let lines = listing.strip_suffix(b"\n").unwrap_or(listing);
    if lines.is_empty() {
        return BareHead::Empty;
    }
    let mut listed = 0;
    for line in lines.split(|byte| *byte == b'\n') {
        listed += 1;
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(head) = commit_ref(line) {
            return head;
        }
    }
    if listed >= MAX_ROUTE_REFS {
        // Cut short with no commit seen: a later ref may hold one.
        BareHead::Unconfirmed
    } else {
        BareHead::Empty
    }
}

/// What one `for-each-ref` line holds for a work tree: nothing, unless the
/// object the ref names, or the object a tag peels to, is a commit. The line
/// is the ref's full name, the type of the object it names, the type of the
/// object a tag peels to (empty for anything else), the ID of the object it
/// names and the ID of the object a tag peels to (empty for anything else).
fn commit_ref(line: &[u8]) -> Option<BareHead> {
    let mut fields = line.split(|byte| *byte == b' ');
    let reference = fields.next()?;
    let named = fields.next()?;
    let peeled = fields.next()?;
    let named_id = fields.next()?;
    let peeled_id = fields.next();
    // A tag's own ID names the tag: the commit is the one it peels to.
    let commit_id = if named == b"commit" {
        named_id
    } else if named == b"tag" && peeled == b"commit" {
        peeled_id?
    } else {
        return None;
    };
    if !reference.starts_with(b"refs/") {
        return None;
    }
    Some(match std::str::from_utf8(reference) {
        Ok(reference) => BareHead::Headless(checkout_name(reference)),
        Err(_) => BareHead::Detached(commit_id_text(commit_id)?),
    })
}

/// An object ID as Git prints it: 40 hexadecimal digits under SHA-1, 64 under
/// SHA-256. Anything else is not an ID a work tree can be placed at.
fn commit_id_text(id: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(id).ok()?;
    let id_length = matches!(text.len(), 40 | 64);
    (id_length && text.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| text.to_owned())
}

/// The name the route gives `git worktree add <path> <name>` for a ref the
/// listing showed to hold a commit. A branch is named by its own name, the one
/// form in which `worktree add` checks the branch out: given in full, it
/// detaches `HEAD` at the commit. `worktree add` reads a name as a branch
/// before anything else, so a tag of the same name does not come between. A
/// branch whose own name begins with `-` would be read as an option, so it is
/// named in full and the work tree is detached at its commit. A tag or a
/// remote-tracking ref is checked out detached either way and is named in
/// full, which nothing else answers to.
fn checkout_name(reference: &str) -> String {
    match reference.strip_prefix("refs/heads/") {
        Some(branch) if !branch.starts_with('-') => branch.to_owned(),
        _ => reference.to_owned(),
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
        BareHead::Detached(commit) => Ok(format!(
            "This bare repository's `HEAD` names no commit, so a new work tree has nothing to \
             check out until it does: check out a commit that exists with `git \
             --git-dir={repository} worktree add --detach <path> {commit}` (the commit's full ID \
             stands in for a ref whose name cannot be written in this output, so the new work \
             tree is on no branch; `<path>` is the new work-tree directory; the command works \
             from any directory)"
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
        // work tree needs that branch named. A branch is named by its own
        // name, the form in which `git worktree add` checks the branch out.
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
        assert!(!route.contains("refs/heads/feature"), "{route}");
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
        assert_eq!(
            bare_head(&tagged),
            BareHead::Headless("refs/tags/v1".to_owned())
        );
        let route = bare_root_work_tree_route(&tagged)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path> refs/tags/v1`",
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
            BareHead::Headless("refs/remotes/origin/main".to_owned())
        );
        let route = bare_root_work_tree_route(&remote_only)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path> refs/remotes/origin/main`",
                bound(&remote_only)?
            )),
            "{route}"
        );
        assert!(!route.contains("has no commits yet"), "{route}");

        // A ref can hold a blob or a tree, which a work tree cannot check
        // out. A tree tag sorts before a tag that holds the commit: the tag
        // is named, by its full name, and the tree is passed over (#15548
        // review).
        let mixed = create("mixed refs")?;
        git_in(&mixed, &["init", "--bare", "-q", "-b", "main", "."])?;
        git_in(
            &seed,
            &[
                "push",
                "-q",
                &mixed.display().to_string(),
                "main^{tree}:refs/tags/a-tree",
                "main:refs/tags/z-commit",
            ],
        )?;
        assert_eq!(
            bare_head(&mixed),
            BareHead::Headless("refs/tags/z-commit".to_owned())
        );
        let route = bare_root_work_tree_route(&mixed)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path> refs/tags/z-commit`",
                bound(&mixed)?
            )),
            "{route}"
        );
        assert!(!route.contains("a-tree"), "{route}");

        // A branch and a tag of one name both hold the commit. The branch
        // sorts first and is the ref selected, and it keeps its own name:
        // `git worktree add` reads a name as a branch before anything else.
        let clashed = create("same-named branch and tag")?;
        git_in(&clashed, &["init", "--bare", "-q", "-b", "main", "."])?;
        git_in(
            &seed,
            &[
                "push",
                "-q",
                &clashed.display().to_string(),
                "main:refs/heads/v1",
                "main:refs/tags/v1",
            ],
        )?;
        assert_eq!(bare_head(&clashed), BareHead::Headless("v1".to_owned()));
        let route = bare_root_work_tree_route(&clashed)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path> v1`",
                bound(&clashed)?
            )),
            "{route}"
        );
        assert!(!route.contains("refs/heads/v1"), "{route}");

        // A branch whose own name begins with `-` would be read as an option:
        // it is named in full, and the work tree is detached at its commit.
        let dashed = create("dash-leading branch")?;
        git_in(&dashed, &["init", "--bare", "-q", "-b", "main", "."])?;
        git_in(
            &seed,
            &[
                "push",
                "-q",
                &dashed.display().to_string(),
                "main:refs/heads/-weird",
            ],
        )?;
        assert_eq!(
            bare_head(&dashed),
            BareHead::Headless("refs/heads/-weird".to_owned())
        );
        let route = bare_root_work_tree_route(&dashed)?;
        assert!(
            route.contains(&format!(
                "`git --git-dir={} worktree add <path> refs/heads/-weird`",
                bound(&dashed)?
            )),
            "{route}"
        );
        assert!(!route.contains("<path> -weird`"), "{route}");

        // Only refs that hold no commit: the repository has none, and the
        // route offers a clone alone, not a work tree as the other choice.
        let no_commit = create("no commit refs")?;
        git_in(&no_commit, &["init", "--bare", "-q", "-b", "main", "."])?;
        git_in(
            &seed,
            &[
                "push",
                "-q",
                &no_commit.display().to_string(),
                "main^{tree}:refs/tags/a-tree",
            ],
        )?;
        assert_eq!(bare_head(&no_commit), BareHead::Empty);
        let route = bare_root_work_tree_route(&no_commit)?;
        assert!(
            route.contains("This bare repository has no commits yet"),
            "{route}"
        );
        assert!(!route.contains("worktree add"), "{route}");
        // `HEAD` itself pointing at a tree is not a commit either: the route
        // does not offer a work tree of it.
        git_in(&no_commit, &["symbolic-ref", "HEAD", "refs/tags/a-tree"])?;
        assert!(
            !matches!(bare_head(&no_commit), BareHead::Resolves),
            "a tree behind `HEAD` is not a commit"
        );
        let route = bare_root_work_tree_route(&no_commit)?;
        assert!(
            route.contains("This bare repository has no commits yet"),
            "{route}"
        );
        assert!(!route.contains("worktree add"), "{route}");

        // More refs than the route reads, none holding a commit among them
        // and a commit behind them: nothing is claimed, and the route names
        // both commands.
        let cut_short = create("cut short refs")?;
        git_in(&cut_short, &["init", "--bare", "-q", "-b", "main", "."])?;
        let mut pushes = vec!["push".to_string(), "-q".to_string()];
        pushes.push(cut_short.display().to_string());
        for index in 0..64 {
            pushes.push(format!("main^{{tree}}:refs/tags/t-{index:04}"));
        }
        pushes.push("main:refs/tags/z-commit".to_string());
        let pushes = pushes.iter().map(String::as_str).collect::<Vec<_>>();
        git_in(&seed, &pushes)?;
        assert_eq!(bare_head(&cut_short), BareHead::Unconfirmed);
        let route = bare_root_work_tree_route(&cut_short)?;
        assert!(route.contains("worktree add"), "{route}");
        assert!(route.contains("git clone"), "{route}");
        assert!(
            !route.contains("This bare repository has no commits yet"),
            "{route}"
        );
        assert!(!route.contains("names no commit"), "{route}");

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
            mixed,
            clashed,
            dashed,
            no_commit,
            cut_short,
        ] {
            let _ = std::fs::remove_dir_all(&dir);
        }
        Ok(())
    }

    /// #15548 review: Git prints a ref's name as the bytes it holds, and a
    /// name need not be UTF-8. The route cannot spell such a name, so the work
    /// tree is detached at the commit Git lists beside it. Every other ref is
    /// named as before, and such a line is counted like any other.
    #[test]
    fn a_ref_whose_name_is_not_utf8_detaches_the_work_tree_at_its_commit() {
        const COMMIT: &str = "56c29f99cbea8e0493607147d218e496b99ec9ad";
        const TAG: &[u8] = b"463a7b4bfd0406868f2d58591c82aa1aaf101031";
        const TREE: &[u8] = b"4b825dc642cb6eb9a060e54bf8d69288fbee4904";
        // One `for-each-ref` line without its line end, as Git prints it: five
        // fields, empty where Git has nothing to print.
        let listed =
            |name: &[u8], named: &[u8], peeled: &[u8], named_id: &[u8], peeled_id: &[u8]| {
                [
                    name, b" ", named, b" ", peeled, b" ", named_id, b" ", peeled_id,
                ]
                .concat()
            };
        let commit = |name: &[u8], id: &[u8]| listed(name, b"commit", b"", id, b"");
        let tree = |index: usize| {
            listed(
                format!("refs/tags/t-{index:04}").as_bytes(),
                b"tree",
                b"",
                TREE,
                b"",
            )
        };
        // A listing: the lines, each closed by `end`.
        let listing = |lines: Vec<Vec<u8>>, end: &[u8]| {
            let mut listing = lines.join(end);
            listing.extend_from_slice(end);
            listing
        };
        let detached = BareHead::Detached(COMMIT.to_owned());
        let opaque_commit = commit(b"refs/heads/opaque-\xff", COMMIT.as_bytes());
        let feature = commit(b"refs/heads/feature", COMMIT.as_bytes());
        let opaque_tag = || {
            listed(
                b"refs/tags/opaque-\xfe",
                b"tag",
                b"commit",
                TAG,
                COMMIT.as_bytes(),
            )
        };

        // A branch whose name holds a byte that is not UTF-8, behind a tree
        // tag that a work tree cannot check out.
        assert_eq!(
            listed_head(&listing(vec![tree(0), opaque_commit.clone()], b"\n")),
            detached
        );
        // An annotated tag: the commit it peels to, not the tag's own ID.
        assert_eq!(listed_head(&listing(vec![opaque_tag()], b"\n")), detached);
        // Beside a name that cannot be spelled, Git did not print an ID: the
        // ref is passed over, as one that holds no commit is.
        let opaque_without_id = commit(b"refs/heads/opaque-\xff", b"not-an-id");
        assert_eq!(
            listed_head(&listing(vec![opaque_without_id.clone()], b"\n")),
            BareHead::Empty
        );
        assert_eq!(
            listed_head(&listing(vec![opaque_without_id, feature.clone()], b"\n")),
            BareHead::Headless("feature".to_owned())
        );
        // Every other ref is named as before.
        assert_eq!(
            listed_head(&listing(vec![feature], b"\n")),
            BareHead::Headless("feature".to_owned())
        );
        assert_eq!(
            listed_head(&listing(
                vec![listed(
                    b"refs/tags/v1",
                    b"tag",
                    b"commit",
                    TAG,
                    COMMIT.as_bytes()
                )],
                b"\n"
            )),
            BareHead::Headless("refs/tags/v1".to_owned())
        );
        assert_eq!(listed_head(b""), BareHead::Empty);
        // Git ends its listing with a line feed; without it the line is read.
        assert_eq!(
            listed_head(&commit(b"refs/heads/feature", COMMIT.as_bytes())),
            BareHead::Headless("feature".to_owned())
        );
        // A carriage return before the line feed is no part of the last field,
        // which for a tag is the commit it peels to: left on it, the tag would
        // read as having no ID. The reader this one replaces dropped it; Git
        // prints none on either host this was measured on.
        assert_eq!(
            listed_head(&listing(vec![tree(0), opaque_tag()], b"\r\n")),
            detached
        );

        // The listing is cut at the last line it printed, which holds no ID:
        // a later ref may hold a commit, so nothing is claimed.
        let mut trees = (0..63).map(tree).collect::<Vec<_>>();
        let mut cut_short = trees.clone();
        cut_short.push(commit(b"refs/heads/opaque-\xff", b"not-an-id"));
        assert_eq!(
            listed_head(&listing(cut_short, b"\n")),
            BareHead::Unconfirmed
        );
        // The commit on the last line printed is still found.
        trees.push(commit(b"refs/heads/opaque-\xff", COMMIT.as_bytes()));
        assert_eq!(listed_head(&listing(trees, b"\n")), detached);
    }
}
