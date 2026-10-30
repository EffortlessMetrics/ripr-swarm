//! #4836/#8082: execute the checked-in `Resolve queued PR` shell against real
//! Git objects, without enqueueing, dispatching or creating a check.
//!
//! The production step binds a merge-queue commit G to a live PR head H. Ref
//! suffixes and commit dates cannot do that (the queue ref may name only the
//! base), so the step compares G's tree with `merge-tree(B, H)`. Each case here
//! builds real commits behind a stubbed read-only `gh`, clones them shallowly
//! the way `actions/checkout` does, and runs the extracted shell.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;

const REPOSITORY: &str = "EffortlessMetrics/ripr-swarm";
const PR: &str = "42";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "ripr-queue-resolution-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The `run:` body of the production `Resolve queued PR` step, de-indented.
fn resolve_shell(source: &str) -> Result<String, String> {
    let (_, rest) = source
        .split_once("      - name: Resolve queued PR\n")
        .ok_or("missing actual Resolve queued PR step")?;
    let (step, _) = rest
        .split_once("\n      - name:")
        .ok_or("missing step after Resolve queued PR")?;
    let (_, body) = step
        .split_once("        run: |\n")
        .ok_or("missing Resolve queued PR shell")?;
    body.lines()
        .map(|line| {
            if line.is_empty() {
                Ok("")
            } else {
                line.strip_prefix("          ")
                    .ok_or("unexpected Resolve queued PR shell nesting")
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|lines| lines.join("\n"))
        .map_err(str::to_string)
}

fn workflow() -> Result<String, String> {
    fs::read_to_string(root().join(".github/workflows/routed-rust.yml")).map_err(|e| e.to_string())
}

/// Hermetic Git: no user, system or global configuration, fixed identity.
fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
    command
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_command(dir)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn commit_file(dir: &Path, file: &str, content: &str, message: &str) -> Result<String, String> {
    fs::write(dir.join(file), content).map_err(|e| e.to_string())?;
    git(dir, &["add", file])?;
    git(dir, &["commit", "-q", "-m", message])?;
    git(dir, &["rev-parse", "HEAD"])
}

/// Real commits behind a bare `origin`, published as the refs a queue run sees.
struct World {
    scratch: Scratch,
    work: PathBuf,
    origin: PathBuf,
    /// Group base B: main after it moved past the PR's merge base.
    base: String,
    /// A PR head branched from the old base, changing `a.txt`.
    head: String,
    /// A different PR head (changes `c.txt`) branched from the same old base.
    other_head: String,
    /// A PR head whose `b.txt` edit conflicts with B.
    conflicting_head: String,
}

impl World {
    fn new() -> Result<Self, String> {
        let scratch = Scratch::new()?;
        let work = scratch.0.join("work");
        let origin = scratch.0.join("origin.git");
        fs::create_dir(&work).map_err(|e| e.to_string())?;
        fs::create_dir(&origin).map_err(|e| e.to_string())?;
        git(&origin, &["init", "-q", "--bare"])?;
        git(&work, &["init", "-q", "-b", "main"])?;
        commit_file(&work, "a.txt", "one\n", "root")?;
        commit_file(&work, "b.txt", "base\n", "base files")?;
        let old_base = git(&work, &["rev-parse", "HEAD"])?;
        git(&work, &["checkout", "-q", "-b", "feature", &old_base])?;
        let head = commit_file(&work, "a.txt", "one\nfeature\n", "feature")?;
        git(&work, &["checkout", "-q", "-b", "other", &old_base])?;
        let other_head = commit_file(&work, "c.txt", "other\n", "other")?;
        git(&work, &["checkout", "-q", "-b", "conflict", &old_base])?;
        let conflicting_head = commit_file(&work, "b.txt", "theirs\n", "conflicting")?;
        git(&work, &["checkout", "-q", "main"])?;
        // The base moved after the PR was branched, so the merge is not a
        // fast-forward and the merge base is not B.
        let base = commit_file(&work, "b.txt", "ours\n", "main moved")?;
        let url = format!("file://{}", origin.display());
        git(&work, &["remote", "add", "origin", &url])?;
        git(&work, &["push", "-q", "origin", "main"])?;
        Ok(Self {
            scratch,
            work,
            origin,
            base,
            head,
            other_head,
            conflicting_head,
        })
    }

    /// What GitHub builds for a one-entry squash group: `merge-tree(B, H)` as a
    /// single child of B whose subject ends with the PR number.
    fn squash(&self, head: &str, subject: &str) -> Result<String, String> {
        let tree = git(
            &self.work,
            &["merge-tree", "--write-tree", &self.base, head],
        )?;
        self.commit_tree(&tree, &self.base, subject)
    }

    fn commit_tree(&self, tree: &str, parent: &str, subject: &str) -> Result<String, String> {
        git(
            &self.work,
            &["commit-tree", tree, "-p", parent, "-m", subject],
        )
    }

    fn publish(&self, reference: &str, sha: &str) -> Result<(), String> {
        git(
            &self.work,
            &["push", "-q", "-f", "origin", &format!("{sha}:{reference}")],
        )
        .map(|_| ())
    }

    /// Mirror `actions/checkout` for a merge_group run: depth 1 at G.
    fn shallow_checkout_of(&self, queued: &str) -> Result<PathBuf, String> {
        self.publish("refs/heads/queue", queued)?;
        let clone = self.scratch.0.join("clone");
        if clone.exists() {
            fs::remove_dir_all(&clone).map_err(|e| e.to_string())?;
        }
        fs::create_dir(&clone).map_err(|e| e.to_string())?;
        let url = format!("file://{}", self.origin.display());
        git(&clone, &["init", "-q"])?;
        git(&clone, &["remote", "add", "origin", &url])?;
        git(
            &clone,
            &[
                "fetch",
                "-q",
                "--no-tags",
                "--depth",
                "1",
                "origin",
                "refs/heads/queue",
            ],
        )?;
        git(&clone, &["checkout", "-q", "--detach", "FETCH_HEAD"])?;
        assert_eq!(
            git(&clone, &["rev-parse", "--is-shallow-repository"])?,
            "true",
            "the fixture must start from a depth-1 checkout like the workflow"
        );
        Ok(clone)
    }
}

/// What the stubbed GitHub API reports and what the queue event carries.
struct Run {
    queued: String,
    /// Live head the pulls API reports for the PR.
    api_head: String,
    /// The sha embedded in the queue ref name.
    ref_sha: String,
    /// `refs/pull/<n>/head` on the remote.
    pull_ref: String,
    /// The group base B the queue event reports.
    group_base: String,
}

struct Outcome {
    accepted: bool,
    log: String,
    outputs: BTreeMap<String, String>,
}

fn run_resolve(world: &World, shell: &str, run: &Run) -> Result<Outcome, String> {
    world.publish(&format!("refs/pull/{PR}/head"), &run.pull_ref)?;
    let clone = world.shallow_checkout_of(&run.queued)?;
    let scratch = &world.scratch.0;

    let parents = git(&world.work, &["log", "-1", "--format=%P", &run.queued])?;
    let subject = git(&world.work, &["log", "-1", "--format=%s", &run.queued])?;
    let commit = json!({
        "parents": parents.split_whitespace().map(|p| json!({"sha": p})).collect::<Vec<_>>(),
        "commit": {"message": format!("{subject}\n\nbody")},
    });
    let pr = json!({"head": {"sha": run.api_head}, "labels": [{"name": "merge-ready"}]});
    let graphql = json!({"data": {"repository": {"pullRequest": {
        "headRefOid": run.api_head,
        "mergeQueueEntry": {"id": "entry-42"},
    }}}});

    let bin = scratch.join("bin");
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let gh = bin.join("gh");
    fs::write(
        &gh,
        r#"#!/bin/sh
case "$2" in
  graphql) printf '%s\n' "$MOCK_GRAPHQL" ;;
  repos/*/pulls/42) printf '%s\n' "$MOCK_PR" ;;
  repos/*/commits/*) printf '%s\n' "$MOCK_COMMIT" ;;
  *) exit 97 ;;
esac
"#,
    )
    .map_err(|e| e.to_string())?;
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let script = scratch.join("resolve.sh");
    fs::write(&script, shell).map_err(|e| e.to_string())?;
    let github_output = scratch.join("github-output");
    let _ = fs::remove_file(&github_output);

    let paths = std::iter::once(bin)
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").ok_or("PATH unavailable")?,
        ))
        .collect::<Vec<_>>();
    let head_ref = format!("refs/heads/gh-readonly-queue/main/pr-{PR}-{}", run.ref_sha);
    let output = Command::new("bash")
        .arg("-e")
        .arg(&script)
        .current_dir(&clone)
        .env_clear()
        .env(
            "PATH",
            std::env::join_paths(&paths).map_err(|e| e.to_string())?,
        )
        .env("HOME", scratch)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_TOKEN", "fixture")
        .env("EVENT_NAME", "merge_group")
        .env("REPOSITORY", REPOSITORY)
        .env("PR_NUMBER", "")
        .env("PR_HEAD_SHA", "")
        .env("PR_LABELS", "")
        .env("GROUP_HEAD_REF", head_ref)
        .env("GROUP_BASE_SHA", &run.group_base)
        .env("GROUP_HEAD_SHA", &run.queued)
        .env("GITHUB_RUN_ID", "100")
        .env("GITHUB_RUN_ATTEMPT", "1")
        .env("GITHUB_OUTPUT", &github_output)
        .env("MOCK_PR", pr.to_string())
        .env("MOCK_COMMIT", commit.to_string())
        .env("MOCK_GRAPHQL", graphql.to_string())
        .output()
        .map_err(|e| e.to_string())?;
    let outputs = fs::read_to_string(&github_output)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Ok(Outcome {
        accepted: output.status.success(),
        log: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        outputs,
    })
}

/// A run for the unchanged live head `head`, queued as its real squash.
fn honest(world: &World, head: &str, ref_sha: &str) -> Result<Run, String> {
    Ok(Run {
        queued: world.squash(head, &format!("feature (#{PR})"))?,
        api_head: head.to_string(),
        ref_sha: ref_sha.to_string(),
        pull_ref: head.to_string(),
        group_base: world.base.clone(),
    })
}

fn assert_refused(outcome: &Outcome, needle: &str) {
    assert!(!outcome.accepted, "must refuse: {}", outcome.log);
    assert!(
        outcome.log.contains(needle),
        "refusal must name `{needle}`: {}",
        outcome.log
    );
    assert!(
        !outcome.outputs.contains_key("queue_entry"),
        "a refusal must publish no admission outputs: {:?}",
        outcome.outputs
    );
}

#[test]
fn queued_squash_of_the_live_head_is_accepted_whether_the_ref_names_head_or_base()
-> Result<(), String> {
    let world = World::new()?;
    let shell = resolve_shell(&workflow()?)?;
    for ref_sha in [world.head.clone(), world.base.clone()] {
        let run = honest(&world, &world.head, &ref_sha)?;
        let outcome = run_resolve(&world, &shell, &run)?;
        assert!(
            outcome.accepted,
            "honest queue entry refused: {}",
            outcome.log
        );
        assert_eq!(outcome.outputs.get("pr").map(String::as_str), Some(PR));
        assert_eq!(
            outcome.outputs.get("head"),
            Some(&world.head),
            "the admitted head is the live head"
        );
        assert_eq!(
            outcome.outputs.get("queue_entry").map(String::as_str),
            Some("entry-42")
        );
    }
    Ok(())
}

/// #4836 October 23: the queue ref names B, the live head H2 differs from the
/// head H1 that G squashed, and H2's commit predates G. Dates and the ref
/// suffix cannot tell; the tree can.
#[test]
fn queued_squash_of_a_different_head_is_refused_even_when_it_predates_the_queue_commit()
-> Result<(), String> {
    let world = World::new()?;
    let shell = resolve_shell(&workflow()?)?;
    let mut run = honest(&world, &world.head, &world.base)?;
    run.api_head = world.other_head.clone();
    run.pull_ref = world.other_head.clone();
    let outcome = run_resolve(&world, &shell, &run)?;
    assert_refused(&outcome, "is not PR #42 head");

    // Control: the same inputs are admitted when the one binding is removed,
    // so the refusal above is caused by the tree comparison alone.
    let weakened = shell.replace(
        "[ \"${merged%%$'\\n'*}\" = \"$(git rev-parse \"${GROUP_HEAD_SHA}^{tree}\")\" ]",
        "true",
    );
    assert_ne!(
        weakened, shell,
        "the tree comparison must exist to be removed"
    );
    let outcome = run_resolve(&world, &weakened, &run)?;
    assert!(
        outcome.accepted,
        "without the tree comparison the counterexample is admitted: {}",
        outcome.log
    );
    assert_eq!(outcome.outputs.get("head"), Some(&world.other_head));
    Ok(())
}

#[test]
fn queued_commit_with_extra_content_is_refused() -> Result<(), String> {
    let world = World::new()?;
    let shell = resolve_shell(&workflow()?)?;
    let tree = git(
        &world.work,
        &["merge-tree", "--write-tree", &world.base, &world.head],
    )?;
    // Same parent and subject as an honest squash, but one extra file.
    git(
        &world.work,
        &["checkout", "-q", "-b", "tamper", &world.base],
    )?;
    git(&world.work, &["read-tree", &tree])?;
    fs::write(world.work.join("extra.txt"), "smuggled\n").map_err(|e| e.to_string())?;
    git(&world.work, &["add", "extra.txt"])?;
    let tampered_tree = git(&world.work, &["write-tree"])?;
    let queued = world.commit_tree(&tampered_tree, &world.base, "feature (#42)")?;
    let run = Run {
        queued,
        api_head: world.head.clone(),
        ref_sha: world.head.clone(),
        pull_ref: world.head.clone(),
        group_base: world.base.clone(),
    };
    assert_refused(&run_resolve(&world, &shell, &run)?, "is not PR #42 head");
    Ok(())
}

#[test]
fn moved_or_unavailable_pull_ref_is_refused() -> Result<(), String> {
    let world = World::new()?;
    let shell = resolve_shell(&workflow()?)?;
    // The PR's own ref no longer names the head the API reported.
    let mut run = honest(&world, &world.head, &world.head)?;
    run.pull_ref = world.other_head.clone();
    assert_refused(&run_resolve(&world, &shell, &run)?, "is not the live head");

    // The reported head cannot be fetched at all: nothing is invented.
    let mut run = honest(&world, &world.head, &world.base)?;
    run.api_head = "f".repeat(40);
    run.ref_sha = world.base.clone();
    assert_refused(&run_resolve(&world, &shell, &run)?, "is not the live head");
    Ok(())
}

#[test]
fn head_that_conflicts_with_the_base_cannot_bind_any_queued_tree() -> Result<(), String> {
    let world = World::new()?;
    let shell = resolve_shell(&workflow()?)?;
    // A conflicting `merge-tree --write-tree` exits 1 but still prints a tree OID
    // (with conflict markers) on its first stdout line. Queue exactly that tree,
    // so only the step honouring merge-tree's exit status can refuse it.
    let output = git_command(&world.work)
        .args([
            "merge-tree",
            "--write-tree",
            &world.base,
            &world.conflicting_head,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.code() != Some(1) {
        return Err(format!(
            "expected a conflicting merge, got {:?}",
            output.status
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let tree = stdout
        .lines()
        .next()
        .ok_or("conflicting merge-tree printed no tree")?;
    let queued = world.commit_tree(tree, &world.base, "conflicting (#42)")?;
    let run = Run {
        queued,
        api_head: world.conflicting_head.clone(),
        ref_sha: world.base.clone(),
        pull_ref: world.conflicting_head.clone(),
        group_base: world.base.clone(),
    };
    assert_refused(&run_resolve(&world, &shell, &run)?, "is not PR #42 head");
    Ok(())
}

#[test]
fn batch_wrong_parent_and_wrong_subject_still_refuse_before_the_tree_is_read() -> Result<(), String>
{
    let world = World::new()?;
    let shell = resolve_shell(&workflow()?)?;

    // Subject does not name the PR.
    let mut run = honest(&world, &world.head, &world.base)?;
    run.queued = world.squash(&world.head, "feature (#43)")?;
    assert_refused(
        &run_resolve(&world, &shell, &run)?,
        "not a single squash commit",
    );

    // Parent is not the group base (a batch built on an earlier entry).
    let mut run = honest(&world, &world.head, &world.base)?;
    run.group_base = world.head.clone();
    run.ref_sha = world.head.clone();
    assert_refused(
        &run_resolve(&world, &shell, &run)?,
        "not a single squash commit",
    );

    // The ref sha is neither the live head nor the base.
    let mut run = honest(&world, &world.head, &world.base)?;
    run.ref_sha = world.other_head.clone();
    assert_refused(
        &run_resolve(&world, &shell, &run)?,
        "is neither the live head",
    );
    Ok(())
}
