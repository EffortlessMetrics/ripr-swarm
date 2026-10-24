//! Run the merge-ready conflict notice's script block against a fake `gh`.
//!
//! GitHub creates no `pull_request` run for a PR that conflicts with its base,
//! so a `merge-ready` request on one started nothing and said nothing (#9842).
//! The companion `pull_request_target` workflow must comment and fail a status
//! only for the exact head and base of the request, and only when the PR is
//! `dirty`. The fake `gh` serves JSON fixtures through the real `jq`, so the
//! workflow's own `--jq` expressions are executed, not mirrored.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const WORKFLOW: &str = ".github/workflows/merge-ready-conflict-notice.yml";
const HEAD: &str = "abc12345abc12345abc12345abc12345abc12345";

const FAKE_GH: &str = r#"#!/bin/bash
printf 'CALL %s\n' "$*" >> "$FAKE_DIR/log"
case "$*" in
  *"/pulls/"*)
    n=$(( $(cat "$FAKE_DIR/n" 2>/dev/null || echo 0) + 1 ))
    echo "$n" > "$FAKE_DIR/n"
    f="$FAKE_DIR/pr.$n.json"
    [ -f "$f" ] || f="$FAKE_DIR/pr.last.json"
    expr=""
    while [ $# -gt 0 ]; do [ "$1" = "--jq" ] && expr="$2"; shift; done
    jq -r "$expr" < "$f" ;;
  *"/comments?"*) echo "${FAKE_EXISTING_COMMENTS:-0}" ;;
esac
"#;

fn repo_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask package has a repository parent".to_string())
}

/// The step's `run: |` body, dedented. The block is the workflow's last key.
fn run_block() -> Result<String, String> {
    let source = fs::read_to_string(repo_root()?.join(WORKFLOW)).map_err(|e| e.to_string())?;
    let (_, body) = source
        .split_once("        run: |\n")
        .ok_or_else(|| "workflow keeps a literal run block".to_string())?;
    Ok(body
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn pr_json(mergeable: &str, state: &str, sha: &str, base: &str) -> String {
    format!(
        r#"{{"mergeable":{mergeable},"mergeable_state":"{state}","head":{{"sha":"{sha}"}},"base":{{"ref":"{base}"}}}}"#
    )
}

struct Outcome {
    comments: usize,
    statuses: usize,
    log: String,
    stdout: String,
}

/// Run the block for an event on `main` at `HEAD` with the PR reads in `reads`
/// (the last one repeats). `existing` is the count of marker comments present.
fn run(reads: &[String], existing: &str, case: &str) -> Result<Outcome, String> {
    let dir =
        std::env::temp_dir().join(format!("merge-ready-notice-{}-{case}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin")).map_err(|e| e.to_string())?;
    for (index, read) in reads.iter().enumerate() {
        let name = if index + 1 == reads.len() {
            "pr.last.json".to_string()
        } else {
            format!("pr.{}.json", index + 1)
        };
        fs::write(dir.join(name), read).map_err(|e| e.to_string())?;
    }
    fs::write(dir.join("bin/gh"), FAKE_GH).map_err(|e| e.to_string())?;
    fs::write(dir.join("bin/sleep"), "#!/bin/bash\nexit 0\n").map_err(|e| e.to_string())?;
    fs::write(dir.join("block.sh"), run_block()?).map_err(|e| e.to_string())?;
    for tool in ["gh", "sleep"] {
        let status = Command::new("chmod")
            .arg("+x")
            .arg(dir.join("bin").join(tool))
            .status()
            .map_err(|e| e.to_string())?;
        assert!(status.success());
    }
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("bash")
        .arg(dir.join("block.sh"))
        .env("PATH", path)
        .env("FAKE_DIR", &dir)
        .env("FAKE_EXISTING_COMMENTS", existing)
        .env("GH_TOKEN", "unused")
        .env("REPOSITORY", "o/r")
        .env("PR_NUMBER", "1")
        .env("HEAD_SHA", HEAD)
        .env("BASE_REF", "main")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "block exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let log = fs::read_to_string(dir.join("log")).unwrap_or_default();
    let count = |prefix: &str| log.lines().filter(|l| l.starts_with(prefix)).count();
    let outcome = Outcome {
        comments: count("CALL api repos/o/r/issues/1/comments -f body="),
        statuses: count(&format!("CALL api repos/o/r/statuses/{HEAD} ")),
        log,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    };
    let _ = fs::remove_dir_all(&dir);
    Ok(outcome)
}

#[test]
fn dirty_exact_head_and_base_comments_once_and_fails_a_status() -> Result<(), String> {
    let dirty = pr_json("false", "dirty", HEAD, "main");
    let outcome = run(&[dirty], "0", "dirty")?;
    assert_eq!(
        (outcome.comments, outcome.statuses),
        (1, 1),
        "{}",
        outcome.log
    );
    assert!(outcome.log.contains("state=failure"));
    assert!(outcome.log.contains("context=merge-ready conflict"));
    assert!(outcome.log.contains("conflicts with `main`"));
    Ok(())
}

#[test]
fn null_mergeability_is_retried_until_computed() -> Result<(), String> {
    let unknown = pr_json("null", "unknown", HEAD, "main");
    let dirty = pr_json("false", "dirty", HEAD, "main");
    let outcome = run(&[unknown.clone(), unknown, dirty], "0", "null-then-dirty")?;
    assert_eq!(
        (outcome.comments, outcome.statuses),
        (1, 1),
        "{}",
        outcome.log
    );
    Ok(())
}

#[test]
fn mergeability_that_never_computes_warns_and_writes_nothing() -> Result<(), String> {
    let unknown = pr_json("null", "unknown", HEAD, "main");
    let outcome = run(&[unknown], "0", "never-computed")?;
    assert_eq!((outcome.comments, outcome.statuses), (0, 0));
    assert!(outcome.stdout.contains("mergeability not computed"));
    Ok(())
}

#[test]
fn mergeable_pr_gets_nothing_beyond_the_normal_run() -> Result<(), String> {
    for state in ["clean", "blocked", "behind", "unstable"] {
        let outcome = run(&[pr_json("true", state, HEAD, "main")], "0", state)?;
        assert_eq!((outcome.comments, outcome.statuses), (0, 0), "{state}");
    }
    Ok(())
}

#[test]
fn a_moved_head_or_retargeted_base_writes_nothing() -> Result<(), String> {
    let moved = pr_json(
        "false",
        "dirty",
        "def67890def67890def67890def67890def67890",
        "main",
    );
    let retargeted = pr_json("false", "dirty", HEAD, "release/current");
    for (name, read) in [("moved", moved), ("retargeted", retargeted)] {
        let outcome = run(&[read], "0", name)?;
        assert_eq!((outcome.comments, outcome.statuses), (0, 0), "{name}");
    }
    Ok(())
}

#[test]
fn an_existing_marker_suppresses_the_comment_but_not_the_status() -> Result<(), String> {
    let dirty = pr_json("false", "dirty", HEAD, "main");
    let outcome = run(&[dirty], "1", "deduped")?;
    assert_eq!(
        (outcome.comments, outcome.statuses),
        (0, 1),
        "{}",
        outcome.log
    );
    Ok(())
}
