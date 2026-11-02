//! Run the issue staleness watchdog sweep against a fake `gh` (#5079).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const WORKFLOW: &str = ".github/workflows/issue-staleness-watchdog.yml";
const MARKER: &str = "<!-- issue-staleness-watchdog -->";

const FAKE_GH: &str = r#"#!/bin/bash
printf 'CALL %s\n' "$*" >> "$FAKE_DIR/log"
expr=""
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
  [ "${args[$i]}" = "--jq" ] && expr="${args[$((i + 1))]}"
done
serve() { if [ -f "$1" ]; then jq -r "$expr" < "$1"; else jq -r "$expr" <<< "$2"; fi; }
decode() { python3 -c 'import urllib.parse,sys; print(urllib.parse.unquote(sys.argv[1]))' "$1"; }
all="$*"
case "$all" in
  *"label create"*) exit 0 ;;
  *"/issues?state=open"*) serve "$FAKE_DIR/issues.json" '[]'; exit 0 ;;
  *"/commits?"*)
    raw_path="${all#*path=}"; raw_path="${raw_path%%&*}"
    raw_since="${all#*since=}"; raw_since="${raw_since%%&*}"
    path="$(decode "$raw_path")"
    since="$(decode "$raw_since")"
    slug="${path//\//_}|${since}"
    serve "$FAKE_DIR/commits.${slug}.json" '[]'; exit 0 ;;
  *"/issues/"*"/comments"*|*"/issues/comments/"*)
    if [[ "$all" == *" -X PATCH"* ]]; then
      echo patch >> "$FAKE_DIR/patches"
      exit 0
    fi
    if [[ "$all" == *"--jq"* ]]; then
      file="$FAKE_DIR/comments.json"
      if [ ! -f "$file" ]; then
        serve "$file" '[]'
      elif [[ "$expr" == *'$m'* ]]; then
        jq -r "$expr" --arg m "$MARKER" < "$file"
      else
        serve "$file" '[]'
      fi
      exit 0
    fi
    echo comment >> "$FAKE_DIR/comments_post"
    exit 0 ;;
  *"/issues/"*"/labels"*)
    echo labeled >> "$FAKE_DIR/labels"
    exit 0 ;;
  *) echo "unmatched gh: $all" >&2; exit 1 ;;
esac
"#;

struct Issue {
    number: u32,
    created_at: &'static str,
    body: &'static str,
    labels: &'static [&'static str],
    stale_path: Option<&'static str>,
    commit_sha: &'static str,
    prior_comment: bool,
}

struct Outcome {
    labels: usize,
    comments: usize,
    patches: usize,
    summary: String,
    log: String,
}

fn slug(path: &str, since: &str) -> String {
    format!("{}|{}", path.replace('/', "_"), since)
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

fn run(issues: &[Issue], refresh: &str, case: &str) -> Result<Outcome, String> {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("issue-watchdog-{}-{case}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin")).map_err(|e| e.to_string())?;
    let list: Vec<String> = issues
        .iter()
        .map(|issue| {
            let labels: Vec<String> = issue
                .labels
                .iter()
                .map(|name| format!(r#"{{"name":"{name}"}}"#))
                .collect();
            format!(
                r#"{{"number":{},"created_at":"{}","title":"t","body":"{}","labels":[{}],"pull_request":null}}"#,
                issue.number,
                issue.created_at,
                issue.body.replace('\\', "\\\\").replace('"', "\\\""),
                labels.join(",")
            )
        })
        .collect();
    fs::write(dir.join("issues.json"), format!("[{}]", list.join(",")))
        .map_err(|e| e.to_string())?;
    fs::write(dir.join("comments.json"), "[]").map_err(|e| e.to_string())?;
    for issue in issues {
        if let Some(path) = issue.stale_path {
            fs::write(
                dir.join(format!("commits.{}.json", slug(path, issue.created_at))),
                format!(
                    r#"[{{"sha":"{}","commit":{{"committer":{{"date":"2026-10-03T00:00:00Z"}}}}}}]"#,
                    issue.commit_sha
                ),
            )
            .map_err(|e| e.to_string())?;
        }
        if issue.prior_comment {
            fs::write(
                dir.join("comments.json"),
                format!(r#"[{{"id":99,"body":"{MARKER}\nold"}}]"#),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    fs::write(dir.join("bin/gh"), FAKE_GH).map_err(|e| e.to_string())?;
    let mut mode = fs::metadata(dir.join("bin/gh"))
        .map_err(|e| e.to_string())?
        .permissions();
    mode.set_mode(0o755);
    fs::set_permissions(dir.join("bin/gh"), mode).map_err(|e| e.to_string())?;
    fs::write(dir.join("block.sh"), run_block()?).map_err(|e| e.to_string())?;
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
        .env("REFRESH", refresh)
        .env("MAX_ISSUES", "50")
        .env("MAX_COMMITS_CALLS", "300")
        .env("MAX_PATHS", "5")
        .env("MARKER", MARKER)
        .env("LABEL", "status/needs-reverify")
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
    let outcome = Outcome {
        labels: fs::read_to_string(dir.join("labels"))
            .map(|s| s.lines().count())
            .unwrap_or(0),
        comments: fs::read_to_string(dir.join("comments_post"))
            .map(|s| s.lines().count())
            .unwrap_or(0),
        patches: fs::read_to_string(dir.join("patches"))
            .map(|s| s.lines().count())
            .unwrap_or(0),
        summary: fs::read_to_string(&summary_path).unwrap_or_default(),
        log,
    };
    let _ = fs::remove_dir_all(&dir);
    Ok(outcome)
}

#[test]
fn a_stale_cited_path_is_labeled_and_commented_once() -> Result<(), String> {
    let issue = Issue {
        number: 1,
        created_at: "2026-10-01T00:00:00Z",
        body: "see crates/ripr/src/app/check.rs:42 for the bug",
        labels: &[],
        stale_path: Some("crates/ripr/src/app/check.rs"),
        commit_sha: "cafebabecafebabecafebabecafebabecafebabe",
        prior_comment: false,
    };
    let outcome = run(&[issue], "false", "stale")?;
    assert_eq!(outcome.labels, 1, "{}", outcome.log);
    assert_eq!(outcome.comments, 1, "{}", outcome.log);
    assert!(
        outcome.summary.contains("labeled and commented"),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn an_already_labeled_issue_is_skipped_on_a_normal_sweep() -> Result<(), String> {
    let issue = Issue {
        number: 2,
        created_at: "2026-10-01T00:00:00Z",
        body: "crates/ripr/src/lib.rs",
        labels: &["status/needs-reverify"],
        stale_path: Some("crates/ripr/src/lib.rs"),
        commit_sha: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        prior_comment: false,
    };
    let outcome = run(&[issue], "false", "skip-labeled")?;
    assert_eq!(
        (outcome.labels, outcome.comments),
        (0, 0),
        "{}",
        outcome.summary
    );
    assert!(
        outcome.summary.contains("already labeled"),
        "{}",
        outcome.summary
    );
    Ok(())
}

#[test]
fn refresh_edits_the_owned_comment_without_relabeling() -> Result<(), String> {
    let issue = Issue {
        number: 3,
        created_at: "2026-10-01T00:00:00Z",
        body: "xtask/src/main.rs:1",
        labels: &["status/needs-reverify"],
        stale_path: Some("xtask/src/main.rs"),
        commit_sha: "0123456789abcdef0123456789abcdef01234567",
        prior_comment: true,
    };
    let outcome = run(&[issue], "true", "refresh")?;
    assert_eq!(outcome.labels, 0, "{}", outcome.log);
    assert_eq!(outcome.patches, 1, "{}", outcome.log);
    assert!(
        outcome.summary.contains("comment updated"),
        "{}",
        outcome.summary
    );
    Ok(())
}
