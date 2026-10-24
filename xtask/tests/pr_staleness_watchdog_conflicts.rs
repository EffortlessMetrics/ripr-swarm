//! Run the PR staleness watchdog's sweep against a fake `gh` (#10136).
//!
//! A merge-ready PR that conflicts with its base has no merge ref, so no
//! qualification run can exist. The notice workflow tells a PR at label time;
//! the sweep reaches the ones labeled earlier or conflicting since. It must
//! write only for same-repo, non-draft, labeled, dirty heads that this
//! repository's actions bot has not already flagged, and clear only its own
//! earlier failure. The fake `gh` serves fixtures through the real `jq`.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const WORKFLOW: &str = ".github/workflows/pr-staleness-watchdog.yml";
const OURS_FAILURE: &str = r#"[{"context":"merge-ready conflict","state":"failure","creator":{"login":"github-actions[bot]"}}]"#;

const FAKE_GH: &str = r#"#!/bin/bash
printf 'CALL %s\n' "$*" >> "$FAKE_DIR/log"
expr=""
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
  [ "${args[$i]}" = "--jq" ] && expr="${args[$((i + 1))]}"
done
serve() { if [ -f "$1" ]; then jq -r "$expr" < "$1"; else jq -r "$expr" <<< "$2"; fi; }
all="$*"
case "$all" in
  *"/pulls?state=open"*) serve "$FAKE_DIR/pulls.json" '[]' ;;
  *"/pulls/"*) n="${all#*/pulls/}"; n="${n%% *}"; serve "$FAKE_DIR/pull.$n.json" '{}' ;;
  *"/check-runs"*) sha="${all#*/commits/}"; sha="${sha%%/*}"; serve "$FAKE_DIR/checks.$sha.json" '{"check_runs":[]}' ;;
  *"/workflows/routed-rust.yml/runs"*) serve "$FAKE_DIR/runs.json" '{"workflow_runs":[]}' ;;
  *"/statuses?"*) sha="${all#*/commits/}"; sha="${sha%%/*}"; serve "$FAKE_DIR/statuses.$sha.json" '[]' ;;
  *"/comments?"*) echo "${FAKE_EXISTING_COMMENTS:-0}" ;;
esac
"#;

struct Pr {
    number: u32,
    draft: bool,
    labeled: bool,
    repo: &'static str,
    state: &'static str,
    prior_statuses: &'static str,
}

impl Pr {
    fn dirty(number: u32) -> Self {
        Self {
            number,
            draft: false,
            labeled: true,
            repo: "o/r",
            state: "dirty",
            prior_statuses: "[]",
        }
    }
    fn sha(&self) -> String {
        format!("{:040x}", self.number)
    }
}

struct Outcome {
    comments: usize,
    failures: usize,
    successes: usize,
    summary: String,
}

fn run_block() -> Result<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    let source = fs::read_to_string(root.join(WORKFLOW)).map_err(|e| e.to_string())?;
    let (_, body) = source
        .split_once("        run: |\n")
        .ok_or_else(|| "workflow keeps a literal run block".to_string())?;
    Ok(body
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn run(prs: &[Pr], existing_comments: &str, case: &str) -> Result<Outcome, String> {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("watchdog-conflicts-{}-{case}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin")).map_err(|e| e.to_string())?;
    let list: Vec<String> = prs
        .iter()
        .map(|pr| {
            let labels = if pr.labeled {
                r#"[{"name":"merge-ready"}]"#
            } else {
                "[]"
            };
            format!(
                r#"{{"number":{},"head":{{"sha":"{}","ref":"b{}","repo":{{"full_name":"{}"}}}},"base":{{"ref":"main"}},"draft":{},"labels":{labels}}}"#,
                pr.number,
                pr.sha(),
                pr.number,
                pr.repo,
                pr.draft
            )
        })
        .collect();
    fs::write(dir.join("pulls.json"), format!("[{}]", list.join(",")))
        .map_err(|e| e.to_string())?;
    for pr in prs {
        fs::write(
            dir.join(format!("pull.{}.json", pr.number)),
            format!(r#"{{"mergeable_state":"{}"}}"#, pr.state),
        )
        .map_err(|e| e.to_string())?;
        fs::write(
            dir.join(format!("statuses.{}.json", pr.sha())),
            pr.prior_statuses,
        )
        .map_err(|e| e.to_string())?;
    }
    fs::write(dir.join("bin/gh"), FAKE_GH).map_err(|e| e.to_string())?;
    fs::write(dir.join("block.sh"), run_block()?).map_err(|e| e.to_string())?;
    let gh = dir.join("bin/gh");
    let mut mode = fs::metadata(&gh).map_err(|e| e.to_string())?.permissions();
    mode.set_mode(0o755);
    fs::set_permissions(&gh, mode).map_err(|e| e.to_string())?;
    let summary_path = dir.join("summary.md");
    let path = format!(
        "{}:{}",
        dir.join("bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("bash")
        .arg(dir.join("block.sh"))
        .env("PATH", path)
        .env("FAKE_DIR", &dir)
        .env("FAKE_EXISTING_COMMENTS", existing_comments)
        .env("GH_TOKEN", "unused")
        .env("REPOSITORY", "o/r")
        .env("REQUIRED_CHECK", "Ripr Rust Small Result")
        .env("GITHUB_STEP_SUMMARY", &summary_path)
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
    let count = |needle: &str| log.lines().filter(|l| l.contains(needle)).count();
    let outcome = Outcome {
        comments: count("/comments -f body="),
        failures: count("state=failure"),
        successes: count("state=success"),
        summary: fs::read_to_string(&summary_path).unwrap_or_default(),
    };
    let _ = fs::remove_dir_all(&dir);
    Ok(outcome)
}

#[test]
fn a_dirty_labeled_head_is_told_once_with_a_comment_and_a_failing_status() -> Result<(), String> {
    let outcome = run(&[Pr::dirty(1)], "0", "dirty")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (1, 1, 0),
        "{}",
        outcome.summary
    );
    assert!(outcome.summary.contains("conflict reported"));
    Ok(())
}

#[test]
fn an_already_flagged_head_is_not_told_again_every_interval() -> Result<(), String> {
    let flagged = Pr {
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(2)
    };
    let outcome = run(&[flagged], "0", "flagged")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (0, 0, 0),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn an_existing_marker_comment_is_not_duplicated_but_the_status_is_posted() -> Result<(), String> {
    let outcome = run(&[Pr::dirty(3)], "1", "marker")?;
    assert_eq!(
        (outcome.comments, outcome.failures),
        (0, 1),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn a_resolved_conflict_clears_only_the_actions_bot_failure() -> Result<(), String> {
    let cleared = Pr {
        state: "clean",
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(4)
    };
    let outcome = run(&[cleared], "0", "cleared")?;
    assert_eq!(
        (outcome.successes, outcome.failures),
        (1, 0),
        "{}",
        outcome.summary
    );

    let foreign = Pr {
        state: "clean",
        prior_statuses: r#"[{"context":"merge-ready conflict","state":"failure","creator":{"login":"someone"}}]"#,
        ..Pr::dirty(5)
    };
    let outcome = run(&[foreign], "0", "foreign")?;
    assert_eq!(
        (outcome.successes, outcome.failures),
        (0, 0),
        "{}",
        outcome.summary
    );

    let unknown = Pr {
        state: "unknown",
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(6)
    };
    let outcome = run(&[unknown], "0", "unknown")?;
    assert_eq!(
        (outcome.successes, outcome.failures),
        (0, 0),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn drafts_unlabeled_forks_and_clean_heads_are_left_alone() -> Result<(), String> {
    let prs = [
        Pr {
            draft: true,
            ..Pr::dirty(7)
        },
        Pr {
            labeled: false,
            ..Pr::dirty(8)
        },
        Pr {
            repo: "fork/r",
            ..Pr::dirty(9)
        },
        Pr {
            state: "clean",
            ..Pr::dirty(10)
        },
        Pr {
            state: "blocked",
            ..Pr::dirty(11)
        },
    ];
    let outcome = run(&prs, "0", "left-alone")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (0, 0, 0),
        "{}",
        outcome.summary
    );
    Ok(())
}
