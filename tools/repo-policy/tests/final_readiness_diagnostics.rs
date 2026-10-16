//! Replay the exact final-readiness shell; diagnostics must not change admission.
#![cfg(target_os = "linux")]
use serde_json::{Value, json};
#[path = "support/review_disposition.rs"]
mod review_disposition;
use std::{
    fs,
    process::Output,
    sync::atomic::{AtomicU64, Ordering},
};

const WORKFLOW: &str = include_str!("../../../.github/workflows/routed-rust.yml");
// Retained bd8/f541 final shell, SHA256 dff5a540b5ab1dfea64b93535258f3894da190e16dbd13982ac3720ffac4666c.
// This predecessor is an independent decision/exit oracle, not a derived expected result.
const ORIGINAL: &str = r#"set -euo pipefail
[ "${LABEL,,}" = merge-ready ] || exit 0
pr=$(gh api "repos/$REPOSITORY/pulls/$PR_NUMBER")
jq -e --arg repo "$REPOSITORY" --arg head "$HEAD_SHA" --arg base "$BASE_SHA" --arg subject "$SUBJECT_SHA" '
  .state == "open" and .draft == false and .head.repo.full_name == $repo and .base.ref == "main" and
  .mergeable == true and .head.sha == $head and .base.sha == $base and
  .merge_commit_sha == $subject and any(.labels[]; (.name | ascii_downcase) == "merge-ready")' <<< "$pr" >/dev/null
reviews=$(gh api --paginate "repos/$REPOSITORY/pulls/$PR_NUMBER/reviews?per_page=100")
author=$(jq -r .user.login <<< "$pr")
jq -s -e --arg head "$HEAD_SHA" --arg author "$author" '
  [ .[][] ] as $reviews |
  ($reviews | map(select(.state == "APPROVED" or .state == "CHANGES_REQUESTED" or .state == "DISMISSED")) |
    group_by(.user.login) | map(max_by(.submitted_at))) as $dispositions |
  all($dispositions[]; .state != "CHANGES_REQUESTED") and
  (any($dispositions[]; .commit_id == $head and .state == "APPROVED") or
   any($reviews | group_by(.user.login) | map(max_by(.submitted_at))[];
     .commit_id == $head and .state == "COMMENTED" and
     (.body | test("(^|[^A-Z_])(SOURCE_REVIEW_READY|REVIEW_READY)([^A-Z_]|$)"))))' <<< "$reviews" >/dev/null
cursor=null
while :; do
  threads=$(gh api graphql -f query='query($number:Int!,$cursor:String){repository(owner:"EffortlessMetrics",name:"ripr-swarm"){pullRequest(number:$number){reviewThreads(first:100,after:$cursor){nodes{isResolved}pageInfo{hasNextPage endCursor}}}}}' -F number="$PR_NUMBER" -F cursor="$cursor")
  jq -e '.errors == null and (.data.repository.pullRequest.reviewThreads.nodes | all(.isResolved == true))' <<< "$threads" >/dev/null
  more=$(jq -er '.data.repository.pullRequest.reviewThreads.pageInfo.hasNextPage | tostring' <<< "$threads")
  [ "$more" = true ] || break
  cursor=$(jq -er '.data.repository.pullRequest.reviewThreads.pageInfo.endCursor | select(length > 0)' <<< "$threads")
done
"#;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const REVIEW_SECRET: &str = "review-body-must-not-be-logged";
const TOKEN_SECRET: &str = "token-must-not-be-logged";

fn body() -> Result<String, String> {
    let (_, step) = WORKFLOW
        .split_once("      - name: Recheck current head and base before result publication\n")
        .ok_or("final step missing")?;
    let (_, run) = step.split_once("        run: |\n").ok_or("run missing")?;
    Ok(run
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
}

struct Case {
    pr: String,
    reviews: String,
    threads: Vec<String>,
    fail: &'static str,
    label: &'static str,
    closed_stderr: bool,
}

fn positive() -> Case {
    Case {
        pr: json!({"state":"open", "draft":false,
            "head":{"repo":{"full_name":"EffortlessMetrics/ripr-swarm"},"sha":"a".repeat(40)},
            "base":{"ref":"main","sha":"b".repeat(40)}, "mergeable":true,
            "merge_commit_sha":"c".repeat(40), "labels":[{"name":"merge-ready"}],
            "user":{"login":"author"}})
        .to_string(),
        reviews: json!([{"id":1, "user":{"login":"reviewer"}, "state":"COMMENTED",
            "commit_id":"a".repeat(40), "submitted_at":"2026-10-15T11:51:29Z",
            "body":format!("SOURCE_REVIEW_READY {REVIEW_SECRET}")}])
        .to_string(),
        threads: vec![
            json!({"data":{"repository":{"pullRequest":{"reviewThreads":{
                "nodes":[{"isResolved":true}], "pageInfo":{"hasNextPage":false,"endCursor":null}
            }}}}})
            .to_string(),
        ],
        fail: "",
        label: "merge-ready",
        closed_stderr: false,
    }
}

fn run(script: &str, case: &Case) -> Result<(Output, String), String> {
    let temp = std::env::temp_dir().join(format!(
        "ripr-final-diagnostic-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir(&temp).map_err(|e| e.to_string())?;
    for (name, data) in [("pr", &case.pr), ("reviews", &case.reviews)] {
        fs::write(temp.join(name), data).map_err(|e| e.to_string())?;
    }
    for (index, data) in case.threads.iter().enumerate() {
        fs::write(temp.join(format!("threads-{index}")), data).map_err(|e| e.to_string())?;
    }
    let mock = r#"gh() {
      [ "$1" = api ] || return 81; shift
      if [ "$1" = --paginate ]; then shift; fi
      local endpoint
      case "$1" in
        */pulls/8008) endpoint=pr;;
        */pulls/8008/reviews*) endpoint=reviews;;
        graphql) endpoint=threads;;
        *) echo 'Unexpected API endpoint' >&2; return 82;;
      esac
      printf '%s\n' "$endpoint" >> "$MOCK_ROOT/calls"
      [ "$MOCK_FAIL" != "$endpoint" ] || return 73
      if [ "$endpoint" = threads ]; then
        local page=0
        [ ! -f "$MOCK_ROOT/page" ] || page=$(cat "$MOCK_ROOT/page")
        printf '%s\n' "$((page + 1))" > "$MOCK_ROOT/page"
        cat "$MOCK_ROOT/threads-$page"
      else
        cat "$MOCK_ROOT/$endpoint"
      fi
    }
    "#;
    let stderr_setup = if case.closed_stderr {
        "exec 2>&-\n"
    } else {
        ""
    };
    let output = std::process::Command::new("timeout")
        .args([
            "--kill-after=1s",
            "10s",
            "bash",
            "-c",
            &format!("{stderr_setup}{mock}\n{script}"),
        ])
        .env_clear()
        .env("PATH", std::env::var("PATH").map_err(|e| e.to_string())?)
        .env("REPOSITORY", "EffortlessMetrics/ripr-swarm")
        .env(
            "REVIEW_DISPOSITION_JQ",
            review_disposition::program(WORKFLOW),
        )
        .env("PR_NUMBER", "8008")
        .env("LABEL", case.label)
        .env("HEAD_SHA", "a".repeat(40))
        .env("BASE_SHA", "b".repeat(40))
        .env("SUBJECT_SHA", "c".repeat(40))
        .env("GH_TOKEN", TOKEN_SECRET)
        .env("MOCK_ROOT", &temp)
        .env("MOCK_FAIL", case.fail)
        .output()
        .map_err(|e| e.to_string())?;
    if matches!(output.status.code(), Some(124 | 137)) {
        return Err(format!(
            "bounded replay timed out; retained {}",
            temp.display()
        ));
    }
    let calls = match fs::read_to_string(temp.join("calls")) {
        Ok(calls) => calls,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    fs::remove_dir_all(&temp).map_err(|e| e.to_string())?;
    Ok((output, calls))
}

fn paired(case: &Case, expected: i32, phase: Option<&str>) -> Result<Value, String> {
    let (before, old_calls) = run(ORIGINAL, case)?;
    let (after, new_calls) = run(&body()?, case)?;
    assert_eq!(before.status.code(), Some(expected), "predecessor oracle");
    assert_eq!(
        after.status.code(),
        before.status.code(),
        "native exit changed"
    );
    assert_eq!(
        new_calls, old_calls,
        "diagnostics must not fetch more API state"
    );
    let stderr = String::from_utf8(after.stderr).map_err(|e| e.to_string())?;
    assert!(!stderr.contains(REVIEW_SECRET), "review body leaked");
    assert!(!stderr.contains(TOKEN_SECRET), "token leaked");
    match phase {
        Some(phase) => {
            assert!(
                stderr.contains(&format!(
                    "readiness_refusal phase={phase} status={expected}"
                )),
                "missing phase: {stderr}"
            );
            let details: Vec<_> = stderr
                .lines()
                .filter_map(|line| line.strip_prefix("readiness_detail="))
                .collect();
            assert_eq!(details.len(), 1, "one diagnostic per refusal");
            assert!(details[0].len() < 4096, "diagnostic must be bounded");
            serde_json::from_str(details[0]).map_err(|e| e.to_string())
        }
        None => {
            assert!(
                !stderr.contains("readiness_refusal"),
                "success must not be reported as refusal"
            );
            Ok(Value::Null)
        }
    }
}

#[test]
fn only_comment_interpretation_changes_in_original_decision_lines() -> Result<(), String> {
    let current = body()?;
    let (prefix, inserted) = current
        .split_once("readiness_refused() { # Advisory diagnostics preserve the original status.\n")
        .ok_or("handler missing")?;
    let (_, original) = inserted
        .split_once("trap readiness_refused ERR\n")
        .ok_or("trap missing")?;
    let stripped = prefix.to_owned()
        + &original
            .lines()
            .filter(|line| !line.trim_start().starts_with("readiness_phase="))
            .map(|line| format!("{line}\n"))
            .collect::<String>();
    let interpreted = ORIGINAL
        .replace(
            "--arg author \"$author\" '",
            "--arg author \"$author\" \"$REVIEW_DISPOSITION_JQ\"'",
        )
        .replace(
            "(.body | test(\"(^|[^A-Z_])(SOURCE_REVIEW_READY|REVIEW_READY)([^A-Z_]|$)\"))",
            "(.body | current_comment_ready)",
        );
    assert_ne!(
        interpreted, ORIGINAL,
        "bounded parser replacement must engage"
    );
    assert_eq!(
        stripped, interpreted,
        "all other decisions must be unchanged"
    );
    paired(&positive(), 0, None)?;
    let mut nonrequest = positive();
    nonrequest.label = "ordinary";
    paired(&nonrequest, 0, None)?;
    Ok(())
}

#[test]
fn corrected_false_positives_report_the_same_parser_decision() -> Result<(), String> {
    for text in [
        "REVIEW_INCOMPLETE. This candidate is not SOURCE_REVIEW_READY; behavioral proof is missing.",
        "REVIEW_INCOMPLETE. Example:\n```\nREVIEW_READY\n```",
        "Previous result was REVIEW_READY; current disposition: REPAIR_REQUIRED.",
        "No longer SOURCE_REVIEW_READY. REPAIR_REQUIRED.",
        "Example: `\ndisposition: REVIEW_READY\n`",
        "<!--\ndisposition: REVIEW_READY\n-->",
        "<pre>\ndisposition: REVIEW_READY\n</pre>",
    ] {
        let mut case = positive();
        let mut reviews: Value = serde_json::from_str(&case.reviews).map_err(|e| e.to_string())?;
        reviews[0]["body"] = json!(text);
        case.reviews = reviews.to_string();
        let (old, _) = run(ORIGINAL, &case)?;
        assert!(
            old.status.success(),
            "retained old matcher must expose defect"
        );
        let (new, _) = run(&body()?, &case)?;
        assert_eq!(new.status.code(), Some(1));
        let stderr = String::from_utf8(new.stderr).map_err(|e| e.to_string())?;
        let detail = stderr
            .lines()
            .find_map(|line| line.strip_prefix("readiness_detail="))
            .ok_or("diagnostic missing")?;
        let detail: Value = serde_json::from_str(detail).map_err(|e| e.to_string())?;
        assert_eq!(detail["checks"]["current_head_marker"], false);
        assert!(!stderr.contains(text), "review text must not be logged");
    }
    Ok(())
}

#[test]
fn quoted_negative_fields_do_not_withdraw_outside_current_disposition() -> Result<(), String> {
    for text in [
        "Example: `\ndisposition: REPAIR_REQUIRED\n`\ndisposition: SOURCE_REVIEW_READY",
        "<!--\ndisposition: REPAIR_REQUIRED\n-->\ndisposition: SOURCE_REVIEW_READY",
        "<pre>\ndisposition: REPAIR_REQUIRED\n</pre>\ndisposition: SOURCE_REVIEW_READY",
    ] {
        let mut case = positive();
        let mut reviews: Value = serde_json::from_str(&case.reviews).map_err(|e| e.to_string())?;
        reviews[0]["body"] = json!(text);
        case.reviews = reviews.to_string();
        paired(&case, 0, None)?;
    }
    Ok(())
}

#[test]
fn every_pr_comparison_reports_false_null_and_missing_without_relaxing() -> Result<(), String> {
    let fields = [
        ("state", "open"),
        ("draft", "non_draft"),
        ("head/repo/full_name", "same_repository"),
        ("base/ref", "main_base"),
        ("mergeable", "mergeable"),
        ("head/sha", "exact_head"),
        ("base/sha", "exact_base"),
        ("merge_commit_sha", "exact_subject"),
        ("labels", "merge_ready"),
    ];
    for (path, check) in fields {
        for variant in ["wrong", "null", "missing"] {
            let mut case = positive();
            let mut pr: Value = serde_json::from_str(&case.pr).map_err(|e| e.to_string())?;
            let parts: Vec<_> = path.split('/').collect();
            let (last, parents) = parts.split_last().ok_or("field path empty")?;
            let mut parent = &mut pr;
            for key in parents {
                parent = &mut parent[*key];
            }
            if variant == "missing" {
                parent
                    .as_object_mut()
                    .ok_or("fixture parent missing")?
                    .remove(*last);
            } else {
                parent[*last] = if variant == "null" {
                    Value::Null
                } else if path == "draft" {
                    json!(true)
                } else if path == "labels" {
                    json!([])
                } else {
                    json!(false)
                };
            }
            case.pr = pr.to_string();
            let expected = if path == "labels" && variant != "wrong" {
                5
            } else {
                1
            };
            let detail = paired(&case, expected, Some("pull-request-check"))?;
            assert_eq!(
                detail["checks"][check],
                if expected == 5 {
                    json!("unreadable")
                } else {
                    json!(false)
                },
                "{path} {variant}"
            );
        }
    }
    Ok(())
}

#[test]
fn pr_parse_and_fetch_failures_are_distinct_and_do_not_replace_status() -> Result<(), String> {
    for (payload, status, kind) in [
        ("{", 5, "invalid-json-or-shape"),
        ("", 4, "missing"),
        ("null", 1, "null"),
        ("[]", 5, "array"),
    ] {
        let mut case = positive();
        case.pr = payload.into();
        let detail = paired(&case, status, Some("pull-request-check"))?;
        assert_eq!(detail["payload"], kind);
    }
    for (endpoint, phase) in [
        ("pr", "pull-request-fetch"),
        ("reviews", "reviews-fetch"),
        ("threads", "threads-fetch"),
    ] {
        let mut case = positive();
        case.fail = endpoint;
        let detail = paired(&case, 73, Some(phase))?;
        assert_eq!(detail["payload"], "not-read");
    }
    Ok(())
}

#[test]
fn review_order_body_changes_and_paginated_streams_remain_authoritative() -> Result<(), String> {
    for marker in ["ordinary follow-up", "REPAIR_REQUIRED"] {
        let mut case = positive();
        let mut latest: Value = serde_json::from_str(&case.reviews).map_err(|e| e.to_string())?;
        latest[0]["id"] = json!(2);
        latest[0]["submitted_at"] = json!("2026-10-15T12:00:00Z");
        latest[0]["body"] = json!(marker);
        case.reviews += &format!("\n{latest}");
        let detail = paired(&case, 1, Some("reviews-check"))?;
        assert_eq!(detail["checks"]["current_head_marker"], false);
        assert_eq!(detail["latest_ids"], json!([2]));
    }
    let mut edited = positive();
    edited.reviews = edited
        .reviews
        .replace("SOURCE_REVIEW_READY", "REPAIR_REQUIRED");
    paired(&edited, 1, Some("reviews-check"))?;
    let mut approved = positive();
    approved.reviews = approved.reviews.replace("COMMENTED", "APPROVED");
    paired(&approved, 0, None)?;
    let mut blocking = positive();
    blocking.reviews += &format!(
        "\n{}",
        json!([{"id":3,"user":{"login":"blocker"},
        "state":"CHANGES_REQUESTED", "commit_id":"f".repeat(40),
        "submitted_at":"2026-10-15T12:00:00Z","body":REVIEW_SECRET}])
    );
    let detail = paired(&blocking, 1, Some("reviews-check"))?;
    assert_eq!(detail["checks"]["no_changes_requested"], false);
    let mut multi = positive();
    multi.reviews = "[]\n".repeat(2) + &multi.reviews;
    paired(&multi, 0, None)?;
    Ok(())
}

#[test]
fn review_missing_null_invalid_and_body_types_preserve_parser_outcomes() -> Result<(), String> {
    for (payload, status) in [("", 1), ("[]", 1), ("null", 5), ("{}", 1), ("{", 5)] {
        let mut case = positive();
        case.reviews = payload.into();
        paired(&case, status, Some("reviews-check"))?;
    }
    for body in [Value::Null, json!(false), json!({})] {
        let mut case = positive();
        let mut reviews: Value = serde_json::from_str(&case.reviews).map_err(|e| e.to_string())?;
        reviews[0]["body"] = body;
        case.reviews = reviews.to_string();
        paired(&case, 5, Some("reviews-check"))?;
    }
    let mut stale = positive();
    stale.reviews = stale.reviews.replace(&"a".repeat(40), &"f".repeat(40));
    let detail = paired(&stale, 1, Some("reviews-check"))?;
    assert_eq!(detail["checks"]["current_head_marker"], false);
    let mut dismissed = positive();
    dismissed.reviews += &format!(
        "\n{}",
        json!([
        {"id":2,"user":{"login":"other"},"state":"CHANGES_REQUESTED",
            "commit_id":"f".repeat(40),"submitted_at":"2026-10-15T12:00:00Z","body":REVIEW_SECRET},
        {"id":3,"user":{"login":"other"},"state":"DISMISSED",
            "commit_id":"f".repeat(40),"submitted_at":"2026-10-15T12:01:00Z","body":REVIEW_SECRET}])
    );
    paired(&dismissed, 0, None)?;
    Ok(())
}

#[test]
fn reported_identities_and_review_metadata_are_bounded_and_non_secret() -> Result<(), String> {
    let mut case = positive();
    let mut pr: Value = serde_json::from_str(&case.pr).map_err(|e| e.to_string())?;
    pr["head"]["sha"] = json!(format!("invalid-{REVIEW_SECRET}"));
    case.pr = pr.to_string();
    let detail = paired(&case, 1, Some("pull-request-check"))?;
    assert_eq!(detail["checks"]["exact_head"], false);
    assert_eq!(detail["observed"]["head"], Value::Null);
    assert_eq!(detail["expected"]["head"], "a".repeat(40));
    let mut case = positive();
    let reviews: Vec<_> = (0..30)
        .map(|id| {
            json!({
        "id":id,"user":{"login":format!("reviewer-{id}")},"state":"COMMENTED",
        "commit_id":"a".repeat(40),"submitted_at":"2026-10-15T12:00:00Z",
        "body":REVIEW_SECRET})
        })
        .collect();
    case.reviews = serde_json::to_string(&reviews).map_err(|e| e.to_string())?;
    let detail = paired(&case, 1, Some("reviews-check"))?;
    assert_eq!(detail["latest_count"], 30);
    assert_eq!(
        detail["latest_ids"].as_array().ok_or("IDs missing")?.len(),
        20
    );
    Ok(())
}

#[test]
fn unavailable_diagnostic_stderr_cannot_replace_the_original_failure() -> Result<(), String> {
    let mut case = positive();
    case.closed_stderr = true;
    for endpoint in ["pr", "reviews", "threads"] {
        case.fail = endpoint;
        let (before, calls_before) = run(ORIGINAL, &case)?;
        let (after, calls_after) = run(&body()?, &case)?;
        assert_eq!(before.status.code(), Some(73));
        assert_eq!(after.status.code(), before.status.code());
        assert_eq!(calls_after, calls_before);
    }
    Ok(())
}

#[test]
fn thread_pagination_errors_and_missing_fields_preserve_native_refusals() -> Result<(), String> {
    let mut pages = positive();
    let mut page: Value = serde_json::from_str(&pages.threads[0]).map_err(|e| e.to_string())?;
    page["data"]["repository"]["pullRequest"]["reviewThreads"]["pageInfo"] =
        json!({"hasNextPage":true,"endCursor":"cursor-1"});
    pages.threads.insert(0, page.to_string());
    paired(&pages, 0, None)?;
    let mut unresolved = pages;
    let mut page: Value =
        serde_json::from_str(&unresolved.threads[1]).map_err(|e| e.to_string())?;
    page["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"][0]["isResolved"] =
        json!(false);
    unresolved.threads[1] = page.to_string();
    let detail = paired(&unresolved, 1, Some("threads-check"))?;
    assert_eq!(detail["checks"]["all_resolved"], false);
    assert_eq!(detail["unresolved_count"], 1);
    for payload in [
        json!({"errors":[{"message":REVIEW_SECRET}]}).to_string(),
        json!({"data":null}).to_string(),
    ] {
        let mut case = positive();
        case.threads = vec![payload];
        let status = if case.threads[0].contains("errors") {
            1
        } else {
            5
        };
        paired(&case, status, Some("threads-check"))?;
    }
    for (payload, status) in [("null", 5), ("{}", 5), ("[]", 5), ("{", 5), ("", 4)] {
        let mut case = positive();
        case.threads = vec![payload.into()];
        paired(&case, status, Some("threads-check"))?;
    }
    let mut case = positive();
    let mut page: Value = serde_json::from_str(&case.threads[0]).map_err(|e| e.to_string())?;
    page["data"]["repository"]["pullRequest"]["reviewThreads"]["pageInfo"] =
        json!({"hasNextPage":true,"endCursor":null});
    case.threads[0] = page.to_string();
    paired(&case, 4, Some("threads-cursor-parse"))?;
    for missing in [false, true] {
        let mut case = positive();
        let mut page: Value = serde_json::from_str(&case.threads[0]).map_err(|e| e.to_string())?;
        let node = &mut page["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"][0];
        if missing {
            node.as_object_mut()
                .ok_or("node missing")?
                .remove("isResolved");
        } else {
            node["isResolved"] = Value::Null;
        }
        case.threads[0] = page.to_string();
        let detail = paired(&case, 1, Some("threads-check"))?;
        assert_eq!(detail["checks"]["all_resolved"], false);
    }
    // Preserve the predecessor's tostring/break behavior. This diagnostic-only
    // change does not silently add a new pagination-schema admission predicate.
    for field in ["missing", "null"] {
        let mut case = positive();
        let mut page: Value = serde_json::from_str(&case.threads[0]).map_err(|e| e.to_string())?;
        let info = &mut page["data"]["repository"]["pullRequest"]["reviewThreads"]["pageInfo"];
        if field == "missing" {
            info.as_object_mut()
                .ok_or("page info missing")?
                .remove("hasNextPage");
        } else {
            info["hasNextPage"] = Value::Null;
        }
        case.threads[0] = page.to_string();
        paired(&case, 0, None)?;
    }
    Ok(())
}
