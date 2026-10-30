//! Real-Git merge-ref controls (#10341) and reviewed-slice readback checks (#11094).
//!
//! `actions/checkout` tests GitHub's merge ref as it stood when the run was admitted.
//! When the base has moved and now conflicts with the PR head, the long test step
//! cannot produce a mergeable result, so the step must fail early and name the
//! cause. It must not block on an unrelated base move, a missing fetch, or an
//! unmoved base.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const WORKFLOW: &str = include_str!("../../.github/workflows/rust-gates.yml");
const STEP_ID: &str = "id: base_still_merges";

fn run_block() -> Result<String, String> {
    let (_, after_id) = WORKFLOW
        .split_once(STEP_ID)
        .ok_or_else(|| "rust-gates.yml keeps the merge-ref step".to_string())?;
    let (_, body) = after_id
        .split_once("        run: |\n")
        .ok_or_else(|| "the step keeps a literal run block".to_string())?;
    Ok(body
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn git_bytes(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&git_bytes(dir, args)?)
        .trim()
        .to_string())
}

fn commit_file(dir: &Path, name: &str, text: &str, message: &str) -> Result<(), String> {
    fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
    git(dir, &["add", name])?;
    git(dir, &["commit", "-q", "-m", message])?;
    Ok(())
}

/// A checkout of the merge of `pr` into `main` as it stood at admission, with
/// `origin/main` then advanced by `move_base`. Returns the checkout and the PR head.
fn admitted_merge(
    case: &str,
    move_base: impl Fn(&Path) -> Result<(), String>,
) -> Result<(PathBuf, String), String> {
    let root = std::env::temp_dir().join(format!("merge-ref-{}-{case}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let origin = root.join("origin.git");
    let work = root.join("work");
    fs::create_dir_all(&origin).map_err(|e| e.to_string())?;
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    git(&origin, &["init", "-q", "--bare", "-b", "main"])?;
    git(&work, &["init", "-q", "-b", "main"])?;
    let url = origin.to_string_lossy().to_string();
    git(&work, &["remote", "add", "origin", &url])?;
    commit_file(&work, "ledger.toml", "ceiling = 1\n", "base")?;
    commit_file(&work, "other.txt", "x\n", "other")?;
    git(&work, &["push", "-q", "origin", "main"])?;
    git(&work, &["checkout", "-q", "-b", "pr"])?;
    commit_file(&work, "ledger.toml", "ceiling = 2\n", "pr")?;
    let head = git(&work, &["rev-parse", "--short", "HEAD"])?;
    git(&work, &["checkout", "-q", "main"])?;
    git(&work, &["checkout", "-q", "--detach"])?;
    git(&work, &["merge", "-q", "--no-ff", "pr", "-m", "merge"])?;
    let merged = git(&work, &["rev-parse", "HEAD"])?;
    git(&work, &["checkout", "-q", "main"])?;
    move_base(&work)?;
    git(&work, &["push", "-q", "origin", "main"])?;
    git(&work, &["checkout", "-q", "--detach", &merged])?;
    Ok((work, head))
}

fn run_step(work: &Path, base_ref: &str) -> Result<Output, String> {
    Command::new("bash")
        .arg("-c")
        .arg(run_block()?)
        .current_dir(work)
        .env("BASE_REF", base_ref)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .map_err(|e| e.to_string())
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// `merge-tree --write-tree` needs Git 2.38. Older Git must degrade to a warning,
/// not fail the run, so the test asserts whichever contract the local Git can reach.
fn merge_tree_supported(work: &Path) -> bool {
    git(
        work,
        &[
            "merge-tree",
            "--write-tree",
            "--no-messages",
            "HEAD",
            "HEAD",
        ],
    )
    .is_ok()
}

#[test]
fn conflicting_base_move_fails_early_and_names_the_cause() -> Result<(), String> {
    let (work, head) = admitted_merge("conflict", |w| {
        commit_file(w, "ledger.toml", "ceiling = 3\n", "base moved the ceiling")
    })?;
    let out = run_step(&work, "main")?;
    let text = stdout(&out);
    if !merge_tree_supported(&work) {
        assert_eq!(out.status.code(), Some(0), "{text}");
        assert!(
            text.contains("::warning title=Conflict re-check unavailable"),
            "{text}"
        );
        return Ok(());
    }
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("::error title=Base moved and now conflicts::main moved"),
        "{text}"
    );
    assert!(text.contains(&format!("PR head {head}")), "{text}");
    assert!(text.contains("re-apply merge-ready"), "{text}");
    Ok(())
}

#[test]
fn unrelated_base_move_does_not_block() -> Result<(), String> {
    let (work, _) = admitted_merge("unrelated", |w| {
        commit_file(w, "other.txt", "y\n", "unrelated base change")
    })?;
    let out = run_step(&work, "main")?;
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(!stdout(&out).contains("::error"), "{}", stdout(&out));
    Ok(())
}

#[test]
fn unmoved_base_does_not_block() -> Result<(), String> {
    let (work, _) = admitted_merge("unmoved", |_| Ok(()))?;
    let out = run_step(&work, "main")?;
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    Ok(())
}

#[test]
fn unreadable_base_warns_instead_of_failing_the_run() -> Result<(), String> {
    let (work, _) = admitted_merge("fetch-fails", |_| Ok(()))?;
    let out = run_step(&work, "no-such-branch")?;
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("::warning title=Base fetch failed"), "{text}");
    Ok(())
}

#[test]
fn step_is_pull_request_only_and_precedes_the_test_run() -> Result<(), String> {
    let step_start = WORKFLOW
        .find("      - name: Base still merges cleanly")
        .ok_or_else(|| "step is named".to_string())?;
    let tests_start = WORKFLOW
        .find("      - name: Required Rust tests")
        .ok_or_else(|| "test step is named".to_string())?;
    assert!(
        step_start < tests_start,
        "merge-ref check must precede nextest"
    );
    let block = &WORKFLOW[step_start..tests_start];
    assert!(
        block.contains("        if: github.event_name == 'pull_request'"),
        "push and manual runs have no merge ref to re-check"
    );
    Ok(())
}

fn verify_slice_carrier(
    repo: &Path,
    reference: &str,
    head: &str,
    parent: &str,
    tree: &str,
    raw_diff: &[u8],
) -> Result<(), String> {
    if git(
        repo,
        &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
    )? != head
    {
        return Err("carrier reference changed".to_string());
    }
    if git(repo, &["show", "-s", "--format=%P", head])? != parent {
        return Err("carrier parents differ".to_string());
    }
    if git(repo, &["rev-parse", &format!("{head}^{{tree}}")])? != tree {
        return Err("carrier tree differs".to_string());
    }
    if git_bytes(
        repo,
        &[
            "diff",
            "--raw",
            "-z",
            "--no-renames",
            "--no-abbrev",
            parent,
            head,
        ],
    )? != raw_diff
    {
        return Err("carrier paths, modes or deletions differ".to_string());
    }
    Ok(())
}

fn require_slice_refusal(result: Result<(), String>, expected: &str) -> Result<(), String> {
    match result {
        Err(reason) if reason == expected => Ok(()),
        Err(reason) => Err(format!("expected {expected:?}, got {reason:?}")),
        Ok(()) => Err("mismatched slice carrier was accepted".to_string()),
    }
}

/// Disposable Git semantics, not a real hosted transfer or assembler admission.
#[test]
fn reviewed_slice_carrier_readback_and_refusals() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("slice-carrier-{}", std::process::id()));
    // Refuse a pre-existing directory rather than deleting unknown work.
    fs::create_dir(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    let producer = root.join("producer");
    let remote = root.join("remote.git");
    let consumer = root.join("consumer");
    fs::create_dir(&producer).map_err(|e| e.to_string())?;
    fs::create_dir(&remote).map_err(|e| e.to_string())?;
    git(&remote, &["init", "-q", "--bare", "-b", "main"])?;
    git(&producer, &["init", "-q", "-b", "main"])?;
    fs::write(producer.join("gone"), b"delete\n").map_err(|e| e.to_string())?;
    fs::write(producer.join("keep"), b"base\n").map_err(|e| e.to_string())?;
    git(&producer, &["add", "."])?;
    git(&producer, &["commit", "-qm", "base"])?;
    let base = git(&producer, &["rev-parse", "HEAD"])?;
    fs::remove_file(producer.join("gone")).map_err(|e| e.to_string())?;
    fs::write(producer.join("keep"), b"changed\0bytes\n").map_err(|e| e.to_string())?;
    fs::write(producer.join("new\nfile"), b"new\n").map_err(|e| e.to_string())?;
    git(&producer, &["add", "-A"])?;
    git(&producer, &["commit", "-qm", "slice"])?;
    let head = git(&producer, &["rev-parse", "HEAD"])?;
    let tree = git(&producer, &["rev-parse", "HEAD^{tree}"])?;
    let raw = git_bytes(
        &producer,
        &[
            "diff",
            "--raw",
            "-z",
            "--no-renames",
            "--no-abbrev",
            &base,
            &head,
        ],
    )?;
    let entries: Vec<_> = raw.split(|byte| *byte == 0).collect();
    assert_eq!(entries.len(), 7, "three complete NUL-delimited entries");
    assert_eq!(entries[1], b"gone");
    assert_eq!(entries[3], b"keep");
    assert_eq!(entries[5], b"new\nfile");
    assert!(entries[0].ends_with(b" D"));
    assert!(entries[2].ends_with(b" M"));
    assert!(entries[4].ends_with(b" A"));
    assert!(entries[4].starts_with(b":000000 100644 "));
    let remote_path = remote.to_string_lossy();
    let consumer_path = consumer.to_string_lossy();
    git(&producer, &["remote", "add", "origin", &remote_path])?;
    git(
        &producer,
        &["push", "-q", "origin", "HEAD:refs/heads/carrier"],
    )?;
    git(
        &root,
        &["clone", "-q", "--no-checkout", &remote_path, &consumer_path],
    )?;
    git(
        &consumer,
        &[
            "fetch",
            "-q",
            "origin",
            "refs/heads/carrier:refs/remotes/origin/carrier",
        ],
    )?;
    let reference = "refs/remotes/origin/carrier";
    verify_slice_carrier(&consumer, reference, &head, &base, &tree, &raw)?;
    assert_eq!(
        git_bytes(&consumer, &["cat-file", "blob", &format!("{head}:keep")])?,
        b"changed\0bytes\n"
    );
    assert_eq!(
        git_bytes(
            &consumer,
            &["cat-file", "blob", &format!("{head}:new\nfile")]
        )?,
        b"new\n"
    );
    match git(&consumer, &["cat-file", "-e", &format!("{head}:gone")]) {
        Err(_) => {}
        Ok(_) => return Err("deleted path survived in the carrier".to_string()),
    }
    let merge = git(
        &producer,
        &[
            "commit-tree",
            &tree,
            "-p",
            &base,
            "-p",
            &head,
            "-m",
            "merge",
        ],
    )?;
    git(&producer, &["update-ref", "refs/heads/merge-test", &merge])?;
    git(
        &producer,
        &["push", "-q", "origin", "refs/heads/merge-test"],
    )?;
    git(
        &consumer,
        &[
            "fetch",
            "-q",
            "origin",
            "refs/heads/merge-test:refs/remotes/origin/merge-test",
        ],
    )?;
    require_slice_refusal(
        verify_slice_carrier(
            &consumer,
            "refs/remotes/origin/merge-test",
            &merge,
            &base,
            &tree,
            &raw,
        ),
        "carrier parents differ",
    )?;
    require_slice_refusal(
        verify_slice_carrier(&consumer, reference, &base, &base, &tree, &raw),
        "carrier reference changed",
    )?;
    require_slice_refusal(
        verify_slice_carrier(&consumer, reference, &head, &head, &tree, &raw),
        "carrier parents differ",
    )?;
    require_slice_refusal(
        verify_slice_carrier(&consumer, reference, &head, &base, &base, &raw),
        "carrier tree differs",
    )?;
    let text = String::from_utf8(raw.clone()).map_err(|e| e.to_string())?;
    let wrong_path = text.replace("gone\0", "other\0").into_bytes();
    let wrong_mode = text.replacen("100644", "100755", 1).into_bytes();
    let deletion_end = raw
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == 0)
        .nth(1)
        .map(|(index, _)| index + 1)
        .ok_or("missing deletion entry")?;
    let missing_deletion = &raw[deletion_end..];
    for changed in [&wrong_path[..], &wrong_mode[..], missing_deletion] {
        assert_ne!(changed, &raw[..]);
        require_slice_refusal(
            verify_slice_carrier(&consumer, reference, &head, &base, &tree, changed),
            "carrier paths, modes or deletions differ",
        )?;
    }
    git(&consumer, &["update-ref", reference, &base])?;
    require_slice_refusal(
        verify_slice_carrier(&consumer, reference, &head, &base, &tree, &raw),
        "carrier reference changed",
    )?;
    fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(())
}
