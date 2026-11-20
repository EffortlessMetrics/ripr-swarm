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

fn run_bash(work: &Path, script: &str, envs: &[(&str, &str)]) -> Result<Output, String> {
    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(script)
        .current_dir(work)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for &(k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().map_err(|e| e.to_string())
}

fn run_step(work: &Path, base_ref: &str) -> Result<Output, String> {
    run_bash(work, &run_block()?, &[("BASE_REF", base_ref)])
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
        Ok(_) => return Err("deleted path remained in the carrier".to_string()),
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

fn step_run_block(step_id: &str) -> Result<String, String> {
    let (_, after_id) = WORKFLOW
        .split_once(step_id)
        .ok_or_else(|| format!("rust-gates.yml keeps {step_id}"))?;
    // Read the first `run:` after the step id, then branch on its form. A
    // separate search for `run: |` and `run: ` can match different steps:
    // `run: |` also starts with `run: `, and a single-line step's first
    // `run: |` may belong to a later step (#14999 review).
    let (_, after_run) = after_id
        .split_once("        run: ")
        .ok_or_else(|| format!("{step_id} keeps a run block"))?;
    if let Some(body) = after_run.strip_prefix("|\n") {
        return Ok(body
            .lines()
            .take_while(|line| line.is_empty() || line.starts_with("          "))
            .map(|line| line.strip_prefix("          ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    Ok(after_run
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string())
}

#[test]
fn goldens_use_the_fresh_source_and_preserve_the_admitted_build() -> Result<(), String> {
    let (work, _) = admitted_merge("goldens-fresh-move", |w| {
        commit_file(w, "other.txt", "fresh-main\n", "base moved")
    })?;
    if !merge_tree_supported(&work) {
        return Ok(());
    }
    let root = work.parent().ok_or("test checkout needs a parent")?;
    let scratch = root.join("leased-scratch");
    fs::create_dir(&scratch).map_err(|e| e.to_string())?;
    let orig_head = git(&work, &["rev-parse", "HEAD"])?;
    let pr_head = git(&work, &["rev-parse", "HEAD^2"])?;
    let output = root.join("step-output");
    let workspace = work.to_str().ok_or("non-UTF-8 test workspace")?;
    let prep = step_run_block("id: goldens_fresh_merge")?;
    let prepared = run_bash(
        &work,
        &prep,
        &[
            ("BASE_REF", "main"),
            ("GITHUB_OUTPUT", output.to_str().ok_or("non-UTF-8 output")?),
            ("TMPDIR", scratch.to_str().ok_or("non-UTF-8 scratch")?),
            ("GITHUB_WORKSPACE", workspace),
            ("RUNNER_TEMP", root.to_str().ok_or("non-UTF-8 temp")?),
        ],
    )?;
    if !prepared.status.success() {
        return Err(String::from_utf8_lossy(&prepared.stderr).into());
    }
    let outputs = fs::read_to_string(output).map_err(|e| e.to_string())?;
    let fresh = outputs
        .lines()
        .find_map(|line| line.strip_prefix("worktree="))
        .ok_or("missing fresh worktree")?;
    let fresh_dir = Path::new(fresh);
    if fresh_dir.parent() != Some(scratch.as_path()) {
        return Err("fresh worktree escaped the configured scratch volume".into());
    }
    if !outputs
        .lines()
        .any(|line| line == format!("orig_head={orig_head}"))
        || git(&work, &["rev-parse", "HEAD"])? != orig_head
        || git(fresh_dir, &["rev-parse", "HEAD^1"])? != git(&work, &["rev-parse", "origin/main"])?
        || git(fresh_dir, &["rev-parse", "HEAD^2"])? != pr_head
    {
        return Err("fresh and original merge identities diverged".into());
    }
    // Two binaries have the same package identity and differ only in the input
    // compiled into the executable. Sharing undifferentiated artifacts can
    // otherwise silently run the first binary on the second tree.
    let fixture = r#"fn main() -> std::io::Result<()> {
        std::fs::create_dir_all("target/ripr/reports")?;
        std::fs::write("target/ripr/reports/sample", include_bytes!("../other.txt"))
    }"#;
    for checkout in [work.as_path(), fresh_dir] {
        fs::create_dir_all(checkout.join(".cargo")).map_err(|e| e.to_string())?;
        fs::create_dir_all(checkout.join("src")).map_err(|e| e.to_string())?;
        fs::write(
            checkout.join("Cargo.toml"),
            "[package]\nname='golden-probe'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        fs::write(checkout.join("src/main.rs"), fixture).map_err(|e| e.to_string())?;
        fs::write(
            checkout.join(".cargo/config.toml"),
            "[alias]\nxtask='run --quiet --'\n",
        )
        .map_err(|e| e.to_string())?;
    }
    let gate = step_run_block("id: goldens_fixtures")?;
    let target = work.join("target");
    let binary = target.join("debug/golden-probe");
    let probe = |tree| -> Result<(), String> {
        let result = run_bash(
            &work,
            &gate,
            &[
                ("FRESH_TREE", tree),
                ("GITHUB_WORKSPACE", workspace),
                (
                    "CARGO_TARGET_DIR",
                    target.to_str().ok_or("non-UTF-8 target")?,
                ),
                (
                    "CARGO_BUILD_BUILD_DIR",
                    target.to_str().ok_or("non-UTF-8 build directory")?,
                ),
                ("CARGO_NET_OFFLINE", "true"),
            ],
        )?;
        if result.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&result.stderr).into())
        }
    };
    probe("")?;
    let bytes = fs::read(&binary).map_err(|e| e.to_string())?;
    let report = target.join("ripr/reports/sample");
    if fs::read_to_string(&report).map_err(|e| e.to_string())? != "x\n" {
        return Err("admitted producer used the wrong source".into());
    }
    probe(fresh)?;
    if fs::read_to_string(&report).map_err(|e| e.to_string())? != "fresh-main\n"
        || fs::read(&binary).map_err(|e| e.to_string())? != bytes
    {
        return Err("fresh golden receipt or admitted executable crossed subjects".into());
    }
    if fresh_dir.join("target/debug/deps").exists() {
        return Err("fresh goldens allocated a second intermediate build directory".into());
    }
    let reused = run_bash(
        &work,
        "cargo build --locked --offline --message-format=json",
        &[
            (
                "CARGO_TARGET_DIR",
                target.to_str().ok_or("non-UTF-8 target")?,
            ),
            (
                "CARGO_BUILD_BUILD_DIR",
                target.to_str().ok_or("non-UTF-8 build directory")?,
            ),
        ],
    )?;
    if !reused.status.success()
        || !stdout(&reused).lines().any(|line| {
            line.contains("\"reason\":\"compiler-artifact\"")
                && line.contains("\"name\":\"golden-probe\"")
                && line.contains("\"fresh\":true")
        })
    {
        return Err("Cargo rebuilt the admitted package after fresh goldens".into());
    }
    let cleanup = step_run_block("id: goldens_cleanup")?;
    let removed = run_bash(
        &work,
        &cleanup,
        &[("FRESH_TREE", fresh), ("ORIG_HEAD", &orig_head)],
    )?;
    if !removed.status.success()
        || fresh_dir.exists()
        || git(&work, &["rev-parse", "HEAD"])? != orig_head
    {
        return Err("cleanup lost the admitted checkout or left the fresh tree".into());
    }
    if fs::read_to_string(&report).map_err(|e| e.to_string())? != "fresh-main\n" {
        return Err("cleanup removed the fresh producer report before its upload".into());
    }
    probe("")?;
    if fs::read_to_string(report).map_err(|e| e.to_string())? != "x\n" {
        return Err("restored producer read the fresh source".into());
    }
    fs::remove_dir_all(root).map_err(|e| e.to_string())
}

#[test]
fn fresh_goldens_cleanup_owns_failed_worktree_add_before_and_after_registration()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let prep = step_run_block("id: goldens_fresh_merge")?;
    let cleanup = step_run_block("id: goldens_cleanup")?;
    for registered in [false, true] {
        let (work, _) = admitted_merge(&format!("goldens-failed-add-{registered}"), |w| {
            commit_file(w, "other.txt", "new base\n", "advance base")
        })?;
        if !merge_tree_supported(&work) {
            return Ok(());
        }
        let root = work.parent().ok_or("fixture needs a parent")?;
        let orig = git(&work, &["rev-parse", "HEAD"])?;
        let output = root.join("outputs");
        if registered {
            let hook = work.join(".git/hooks/post-checkout");
            fs::write(&hook, "#!/bin/sh\nexit 7\n").map_err(|e| e.to_string())?;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        } else {
            fs::write(work.join(".git/worktrees"), "cannot register here")
                .map_err(|e| e.to_string())?;
        }
        let failed = run_bash(
            &work,
            &prep,
            &[
                ("BASE_REF", "main"),
                ("GITHUB_OUTPUT", output.to_str().ok_or("non-UTF-8 output")?),
                (
                    "GITHUB_WORKSPACE",
                    work.to_str().ok_or("non-UTF-8 workspace")?,
                ),
                ("TMPDIR", ""),
                ("RUNNER_TEMP", root.to_str().ok_or("non-UTF-8 temp")?),
            ],
        )?;
        let ownership = fs::read_to_string(&output).map_err(|e| e.to_string())?;
        let path = ownership
            .lines()
            .find_map(|line| line.strip_prefix("worktree="))
            .ok_or("failed setup lost the allocated path")?;
        let listing = git(&work, &["worktree", "list", "--porcelain"])?;
        if failed.status.success()
            || listing
                .lines()
                .any(|line| line == format!("worktree {path}"))
                != registered
            || Path::new(path).parent() != Some(root)
        {
            return Err(format!(
                "wrong partial setup (registered={registered}): {listing}"
            ));
        }
        let removed = run_bash(
            &work,
            &cleanup,
            &[("FRESH_TREE", path), ("ORIG_HEAD", &orig)],
        )?;
        if !removed.status.success()
            || Path::new(path).exists()
            || git(&work, &["worktree", "list", "--porcelain"])?
                .lines()
                .any(|line| line == format!("worktree {path}"))
        {
            return Err(format!(
                "cleanup failed for partial checkout {path}: {}",
                String::from_utf8_lossy(&removed.stderr)
            ));
        }
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    }
    Ok(())
}
