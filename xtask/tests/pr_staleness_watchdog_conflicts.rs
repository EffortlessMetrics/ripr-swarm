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
  *"/pulls/"*)
    n="${all#*/pulls/}"; n="${n%% *}"
    # A `.later.json` fixture answers every read after the first (lazy mergeability).
    if [ -f "$FAKE_DIR/pull.$n.later.json" ] && [ -f "$FAKE_DIR/pull.$n.seen" ]; then
      serve "$FAKE_DIR/pull.$n.later.json" '{}'
    else
      touch "$FAKE_DIR/pull.$n.seen"; serve "$FAKE_DIR/pull.$n.json" '{}'
    fi ;;
  *"/check-runs"*) sha="${all#*/commits/}"; sha="${sha%%/*}"; serve "$FAKE_DIR/checks.$sha.json" '{"check_runs":[]}' ;;
  *"/workflows/routed-rust.yml/runs"*)
    # A `.later.json` fixture answers every look after the first (delayed run visibility).
    if [ -f "$FAKE_DIR/runs.later.fail" ] && [ -f "$FAKE_DIR/runs.seen" ]; then
      echo "HTTP 502" >&2; exit 1
    elif [ -f "$FAKE_DIR/runs.later.json" ] && [ -f "$FAKE_DIR/runs.seen" ]; then
      serve "$FAKE_DIR/runs.later.json" '{"workflow_runs":[]}'
    else
      touch "$FAKE_DIR/runs.seen"; serve "$FAKE_DIR/runs.json" '{"workflow_runs":[]}'
    fi ;;
  *"/statuses?"*) sha="${all#*/commits/}"; sha="${sha%%/*}"; serve "$FAKE_DIR/statuses.$sha.json" '[]' ;;
  *"/comments?"*) serve "$FAKE_DIR/comments.json" '[]' ;;
esac
"#;

struct Pr {
    number: u32,
    draft: bool,
    labeled: bool,
    repo: &'static str,
    state: &'static str,
    prior_statuses: &'static str,
    marker_comment: bool,
    moved: bool,
    later_state: Option<&'static str>,
    covered: bool,
    in_flight: bool,
    completed_run: bool,
    fresh: bool,
    late_run: Option<&'static str>,
    late_fail: bool,
    renewed: bool,
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
            marker_comment: false,
            moved: false,
            later_state: None,
            covered: false,
            in_flight: false,
            completed_run: false,
            fresh: false,
            late_run: None,
            late_fail: false,
            renewed: false,
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
    log: String,
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

fn run(prs: &[Pr], case: &str) -> Result<Outcome, String> {
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
                r#"{{"number":{},"head":{{"sha":"{}","ref":"b{}","repo":{{"full_name":"{}"}}}},"base":{{"ref":"main"}},"draft":{},"labels":{labels},"updated_at":"{}"}}"#,
                pr.number,
                pr.sha(),
                pr.number,
                pr.repo,
                pr.draft,
                // A far-future stamp stands in for "touched within the grace window".
                if pr.fresh {
                    "2999-01-01T00:00:00Z"
                } else {
                    "2026-01-01T00:00:00Z"
                }
            )
        })
        .collect();
    fs::write(dir.join("pulls.json"), format!("[{}]", list.join(",")))
        .map_err(|e| e.to_string())?;
    for pr in prs {
        fs::write(
            dir.join(format!("pull.{}.json", pr.number)),
            // `moved` models a push between the list call and the live read.
            format!(
                r#"{{"mergeable_state":{},"head":{{"sha":"{}","repo":{{"full_name":"{}"}}}},"base":{{"ref":"main"}},"draft":{},"labels":{}}}"#,
                if pr.state == "null" {
                    "null".to_string()
                } else {
                    format!("\"{}\"", pr.state)
                },
                if pr.moved {
                    format!("{:040x}", pr.number + 1000)
                } else {
                    pr.sha()
                },
                pr.repo,
                // `renewed` models the label or ready state coming back after the list call.
                pr.draft && !pr.renewed,
                if pr.labeled || pr.renewed {
                    r#"[{"name":"merge-ready"}]"#
                } else {
                    "[]"
                }
            ),
        )
        .map_err(|e| e.to_string())?;
        fs::write(
            dir.join(format!("statuses.{}.json", pr.sha())),
            pr.prior_statuses,
        )
        .map_err(|e| e.to_string())?;
        if pr.covered {
            fs::write(
                dir.join(format!("checks.{}.json", pr.sha())),
                r#"{"check_runs":[{"name":"Ripr Rust Small Result"}]}"#,
            )
            .map_err(|e| e.to_string())?;
        }
        if pr.in_flight {
            fs::write(
                dir.join("runs.json"),
                r#"{"workflow_runs":[{"status":"in_progress","display_title":"Merge-ready request abc"}]}"#,
            )
            .map_err(|e| e.to_string())?;
        }
        if pr.completed_run {
            fs::write(
                dir.join("runs.json"),
                r#"{"workflow_runs":[{"status":"completed","display_title":"Merge-ready request abc"}]}"#,
            )
            .map_err(|e| e.to_string())?;
        }
        if pr.late_fail {
            fs::write(dir.join("runs.later.fail"), "").map_err(|e| e.to_string())?;
        }
        if let Some(status) = pr.late_run {
            fs::write(
                dir.join("runs.later.json"),
                format!(
                    r#"{{"workflow_runs":[{{"status":"{status}","display_title":"Merge-ready request abc"}}]}}"#
                ),
            )
            .map_err(|e| e.to_string())?;
        }
        if let Some(later) = pr.later_state {
            fs::write(
                dir.join(format!("pull.{}.later.json", pr.number)),
                format!(
                    r#"{{"mergeable_state":"{later}","head":{{"sha":"{}"}},"base":{{"ref":"main"}}}}"#,
                    pr.sha()
                ),
            )
            .map_err(|e| e.to_string())?;
        }
        if pr.marker_comment {
            fs::write(
                dir.join("comments.json"),
                format!(
                    r#"[{{"body":"unrelated"}},{{"body":"<!-- merge-ready-conflict:{}:main -->\nbody"}},{{"body":"<!-- merge-ready-dark:{} -->\nbody"}}]"#,
                    pr.sha(),
                    pr.sha()
                ),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    fs::write(dir.join("bin/gh"), FAKE_GH).map_err(|e| e.to_string())?;
    fs::write(dir.join("bin/sleep"), "#!/bin/bash\nexit 0\n").map_err(|e| e.to_string())?;
    fs::write(dir.join("block.sh"), run_block()?).map_err(|e| e.to_string())?;
    for tool in ["gh", "sleep"] {
        let path = dir.join("bin").join(tool);
        let mut mode = fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions();
        mode.set_mode(0o755);
        fs::set_permissions(&path, mode).map_err(|e| e.to_string())?;
    }
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
        log: log.clone(),
    };
    let _ = fs::remove_dir_all(&dir);
    Ok(outcome)
}

#[test]
fn a_dirty_labeled_head_is_told_once_with_a_comment_and_a_failing_status() -> Result<(), String> {
    let outcome = run(&[Pr::dirty(1)], "dirty")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (1, 1, 0),
        "{}",
        outcome.summary
    );
    assert!(outcome.summary.contains("conflict reported"));
    let sha = Pr::dirty(1).sha();
    assert!(
        outcome.log.contains(&format!(
            "repos/o/r/statuses/{sha} -f state=failure -f context=merge-ready conflict"
        )),
        "{}",
        outcome.log
    );
    assert!(
        outcome
            .log
            .contains(&format!("<!-- merge-ready-conflict:{sha}:main -->")),
        "{}",
        outcome.log
    );
    Ok(())
}

#[test]
fn an_already_flagged_head_is_not_told_again_every_interval() -> Result<(), String> {
    let flagged = Pr {
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(2)
    };
    let outcome = run(&[flagged], "flagged")?;
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
    let marked = Pr {
        marker_comment: true,
        ..Pr::dirty(3)
    };
    let outcome = run(&[marked], "marker")?;
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
    let outcome = run(&[cleared], "cleared")?;
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
    let outcome = run(&[foreign], "foreign")?;
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
    let outcome = run(&[unknown], "unknown")?;
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
            covered: true,
            ..Pr::dirty(10)
        },
        Pr {
            state: "blocked",
            covered: true,
            ..Pr::dirty(11)
        },
    ];
    let outcome = run(&prs, "left-alone")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (0, 0, 0),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn the_newest_status_decides_and_a_null_state_is_not_a_conflict() -> Result<(), String> {
    // Newest first: an older failure under a newer success is already cleared.
    let cleared_since = Pr {
        state: "clean",
        prior_statuses: r#"[{"context":"merge-ready conflict","state":"success","creator":{"login":"github-actions[bot]"}},{"context":"merge-ready conflict","state":"failure","creator":{"login":"github-actions[bot]"}}]"#,
        ..Pr::dirty(12)
    };
    let outcome = run(&[cleared_since], "newest-success")?;
    assert_eq!(
        (outcome.successes, outcome.failures),
        (0, 0),
        "{}",
        outcome.summary
    );

    // A newer failure above an older success is still flagged: stay quiet when dirty.
    let flagged_since = Pr {
        prior_statuses: r#"[{"context":"merge-ready conflict","state":"failure","creator":{"login":"github-actions[bot]"}},{"context":"merge-ready conflict","state":"success","creator":{"login":"github-actions[bot]"}}]"#,
        ..Pr::dirty(13)
    };
    let outcome = run(&[flagged_since], "newest-failure")?;
    assert_eq!(
        (outcome.comments, outcome.failures),
        (0, 0),
        "{}",
        outcome.summary
    );

    // GitHub has not computed mergeability yet: nothing is written either way.
    let uncomputed = Pr {
        state: "null",
        ..Pr::dirty(14)
    };
    let outcome = run(&[uncomputed], "null-state")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (0, 0, 0),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn a_head_pushed_since_the_list_call_is_left_for_the_next_sweep() -> Result<(), String> {
    let pushed = Pr {
        moved: true,
        ..Pr::dirty(15)
    };
    let outcome = run(&[pushed], "moved")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (0, 0, 0),
        "{}",
        outcome.summary
    );
    assert!(outcome.summary.contains("moved"), "{}", outcome.summary);
    Ok(())
}

#[test]
fn a_head_whose_mergeability_is_still_being_computed_is_read_again() -> Result<(), String> {
    // The first read is "unknown" (the base just moved); the retry sees the conflict.
    let lazy = Pr {
        state: "unknown",
        later_state: Some("dirty"),
        ..Pr::dirty(16)
    };
    let outcome = run(&[lazy], "lazy")?;
    assert_eq!(
        (outcome.comments, outcome.failures),
        (1, 1),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn a_clean_head_pushed_after_the_label_is_told_once_to_re_apply_it() -> Result<(), String> {
    let pushed = Pr {
        state: "clean",
        ..Pr::dirty(17)
    };
    let outcome = run(&[pushed], "dark-push")?;
    let sha = Pr::dirty(17).sha();
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (1, 0, 0),
        "{}",
        outcome.summary
    );
    assert!(
        outcome
            .log
            .contains(&format!("<!-- merge-ready-dark:{sha} -->")),
        "{}",
        outcome.log
    );
    assert!(outcome.summary.contains("re-apply notice posted"));

    // Already told for this head: stay quiet.
    let told = Pr {
        state: "clean",
        marker_comment: true,
        ..Pr::dirty(18)
    };
    let outcome = run(&[told], "dark-told")?;
    assert_eq!(outcome.comments, 0, "{}", outcome.summary);
    assert!(
        outcome.summary.contains("re-apply notice already posted")
            && outcome.summary.contains("re-apply notices: 0"),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn a_covered_in_flight_or_uncomputed_head_gets_no_re_apply_notice() -> Result<(), String> {
    let covered = Pr {
        state: "clean",
        covered: true,
        ..Pr::dirty(19)
    };
    let in_flight = Pr {
        state: "clean",
        in_flight: true,
        ..Pr::dirty(20)
    };
    let outcome = run(&[covered], "dark-covered")?;
    assert_eq!(outcome.comments, 0, "{}", outcome.summary);
    let outcome = run(&[in_flight], "dark-in-flight")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (0, 0, 0),
        "{}",
        outcome.summary
    );
    // Mergeability never computed: it may be a conflict, so do not blame a push.
    // (Separate run: the in-flight fixture is shared by every PR in a run.)
    let uncomputed = Pr {
        state: "unknown",
        ..Pr::dirty(21)
    };
    let outcome = run(&[uncomputed], "dark-uncomputed")?;
    assert_eq!(
        (outcome.comments, outcome.failures, outcome.successes),
        (0, 0, 0),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn a_finished_run_without_the_check_is_named_and_a_fresh_or_moved_head_is_left_alone()
-> Result<(), String> {
    let ran = Pr {
        state: "clean",
        completed_run: true,
        ..Pr::dirty(27)
    };
    let outcome = run(&[ran], "dark-ran")?;
    assert_eq!(outcome.comments, 1, "{}", outcome.summary);
    assert!(
        outcome.log.contains("finished without the required check"),
        "{}",
        outcome.log
    );

    // Touched within the grace window: a label applied moments ago has not run yet.
    let fresh = Pr {
        state: "clean",
        fresh: true,
        ..Pr::dirty(28)
    };
    let outcome = run(&[fresh], "dark-fresh")?;
    assert_eq!(outcome.comments, 0, "{}", outcome.summary);

    // Pushed between the list call and the live read: the next sweep reads the new head.
    let moved = Pr {
        state: "clean",
        moved: true,
        ..Pr::dirty(29)
    };
    let outcome = run(&[moved], "dark-moved")?;
    assert_eq!(outcome.comments, 0, "{}", outcome.summary);
    Ok(())
}

#[test]
fn a_run_that_becomes_visible_before_the_notice_is_left_alone() -> Result<(), String> {
    // The sweep's first look finds no run; by the time the notice is composed an active
    // request is visible. Re-applying the label would cancel it, so say nothing.
    for status in ["in_progress", "queued"] {
        let delayed = Pr {
            state: "clean",
            late_run: Some(status),
            ..Pr::dirty(30)
        };
        let outcome = run(&[delayed], &format!("late-{status}"))?;
        assert_eq!(outcome.comments, 0, "{status}: {}", outcome.summary);
        // The audit row must not advise a re-apply that would cancel the live run.
        assert!(
            outcome
                .summary
                .contains("in flight (a merge-ready run became visible"),
            "{status}: {}",
            outcome.summary
        );
        assert!(
            !outcome.summary.contains("needs merge-ready re-applied")
                && !outcome.summary.contains("re-apply notice posted")
                && outcome.summary.contains("re-apply notices: 0"),
            "{status}: {}",
            outcome.summary
        );
    }
    Ok(())
}

#[test]
fn a_failed_lookup_before_the_notice_posts_nothing_and_is_reported() -> Result<(), String> {
    // The second run lookup errors: an unknown run state must not read as "no run", or a
    // request that just started would be told to restart.
    let failing = Pr {
        state: "clean",
        late_fail: true,
        ..Pr::dirty(31)
    };
    let outcome = run(&[failing], "late-fail")?;
    assert_eq!(outcome.comments, 0, "{}", outcome.summary);
    assert!(
        outcome.summary.contains("re-apply notice failed")
            && outcome.summary.contains("re-apply notices: 0"),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn a_failing_status_is_retired_when_the_label_is_removed_or_the_pr_is_a_draft() -> Result<(), String>
{
    let unlabeled = Pr {
        labeled: false,
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(22)
    };
    let draft = Pr {
        draft: true,
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(23)
    };
    let outcome = run(&[unlabeled, draft], "retire")?;
    assert_eq!(
        (outcome.successes, outcome.failures, outcome.comments),
        (2, 0, 0),
        "{}",
        outcome.summary
    );
    for number in [22, 23] {
        let sha = Pr::dirty(number).sha();
        assert!(
            outcome.log.contains(&format!(
                "repos/o/r/statuses/{sha} -f state=success -f context=merge-ready conflict"
            )),
            "{}",
            outcome.log
        );
    }
    Ok(())
}

#[test]
fn only_the_actions_bots_failure_on_a_same_repo_pr_is_retired() -> Result<(), String> {
    let foreign = Pr {
        labeled: false,
        prior_statuses: r#"[{"context":"merge-ready conflict","state":"failure","creator":{"login":"someone"}}]"#,
        ..Pr::dirty(24)
    };
    let fork = Pr {
        labeled: false,
        repo: "fork/r",
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(25)
    };
    let already_clear = Pr {
        labeled: false,
        prior_statuses: r#"[{"context":"merge-ready conflict","state":"success","creator":{"login":"github-actions[bot]"}},{"context":"merge-ready conflict","state":"failure","creator":{"login":"github-actions[bot]"}}]"#,
        ..Pr::dirty(26)
    };
    let outcome = run(&[foreign, fork, already_clear], "retire-quiet")?;
    assert_eq!(
        (outcome.successes, outcome.failures, outcome.comments),
        (0, 0, 0),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn a_request_renewed_since_the_list_call_keeps_its_conflict_failure() -> Result<(), String> {
    let relabeled = Pr {
        labeled: false,
        renewed: true,
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(27)
    };
    let readied = Pr {
        draft: true,
        renewed: true,
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(28)
    };
    let moved = Pr {
        labeled: false,
        moved: true,
        prior_statuses: OURS_FAILURE,
        ..Pr::dirty(29)
    };
    let outcome = run(&[relabeled, readied, moved], "retire-renewed")?;
    assert_eq!(
        (outcome.successes, outcome.failures, outcome.comments),
        (0, 0, 0),
        "{}",
        outcome.log
    );
    Ok(())
}
