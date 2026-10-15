//! Execute retained native readiness/result shell against independent API vectors.
#![cfg(target_os = "linux")]
use serde_json::{Value, json};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

const WORKFLOW: &str = include_str!("../../../.github/workflows/routed-rust.yml");
const VECTORS: &str =
    include_str!("../../../fixtures/convergence/native-late-admission/vectors.json");
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
fn body(name: &str) -> Result<String, String> {
    let (_, step) = WORKFLOW
        .split_once(&format!("      - name: {name}\n"))
        .ok_or("step missing")?;
    let (_, run) = step.split_once("        run: |\n").ok_or("run missing")?;
    Ok(run
        .lines()
        .take_while(|l| l.is_empty() || l.starts_with("          "))
        .map(|l| l.strip_prefix("          ").unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n"))
}
// Each Actions run step owns a separate shell. Preserve failures while allowing
// the terminal always() guard to execute; an AND-list would disable Bash errexit.
fn steps(scripts: &[String]) -> String {
    let mut sequence = String::from("step_result=0\n");
    for script in scripts {
        sequence.push_str(&format!("( {script}\n)\n[ \"$?\" = 0 ] || step_result=1\n"));
    }
    sequence.push_str("exit \"$step_result\"\n");
    sequence
}
fn execute(script: &str, pr: &Value, reviews: &Value, environment: &Value) -> Result<bool, String> {
    let temp = std::env::temp_dir().join(format!(
        "ripr-native-admission-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir(&temp).map_err(|e| e.to_string())?;
    let mock = r#"gh() {
      [ "$1" = api ] || return 81; shift
      [ "$1" != --paginate ] || shift
      case "$1" in
        */pulls/8008) printf '%s\n' "$MOCK_PR";;
        */pulls/8008/reviews*) printf '%s\n' "$MOCK_REVIEWS";;
        */collaborators/*/permission) printf '%s\n' write;;
        */git/commits/*) printf '%s\n' "$MOCK_COMMIT";;
        */actions/workflows/*) printf '%s\n' 1;;
        */actions/runs\?*) printf '%s\n' '{"workflow_runs":[]}' ;;
        graphql*) printf '%s\n' '{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}';;
        *) echo 'Unexpected API endpoint' >&2; return 82;;
      esac
    }
    "#;
    let mut command = std::process::Command::new("timeout");
    command
        .args([
            "--kill-after=1s",
            "10s",
            "bash",
            "-c",
            &format!("{mock}\n{script}"),
        ])
        .env_clear()
        .env("PATH", std::env::var("PATH").map_err(|e| e.to_string())?)
        .env("REPOSITORY", "EffortlessMetrics/ripr-swarm")
        .env("PR_NUMBER", "8008")
        .env("LABEL", "merge-ready")
        .env("ACTION", "labeled")
        .env("ACTOR", "trusted")
        .env("GITHUB_WORKFLOW_REF", "EffortlessMetrics/ripr-swarm/.github/workflows/routed-rust.yml@refs/pull/8008/merge")
        .env("GITHUB_RUN_NUMBER", "100")
        .env("MOCK_COMMIT", json!({"parents": [{"sha": "b".repeat(40)}, {"sha": "a".repeat(40)}], "tree": {"sha": "d".repeat(40)}}).to_string())
        .env("HEAD_SHA", "a".repeat(40))
        .env("BASE_SHA", "b".repeat(40))
        .env("SUBJECT_SHA", "c".repeat(40))
        .env("MOCK_PR", pr.to_string())
        .env("MOCK_REVIEWS", reviews.to_string())
        .env("GITHUB_OUTPUT", temp.join("out"))
        .env("GITHUB_STEP_SUMMARY", temp.join("summary"));
    for key in [
        "ADMISSION_RESULT",
        "EXECUTE",
        "PRIOR_RESULT",
        "WINDOWS_RESULT",
        "TARGET",
        "REASON",
        "ROUTE_RESULT",
        "CX43_RESULT",
        "CPX42_RESULT",
        "CX53_RESULT",
        "GITHUB_RESULT",
        "CX43_SCRATCH_STATUS",
        "CPX42_SCRATCH_STATUS",
        "CX53_SCRATCH_STATUS",
        "CX43_SCRATCH_MESSAGE",
        "CPX42_SCRATCH_MESSAGE",
        "CX53_SCRATCH_MESSAGE",
        "RUNNER_QUERY",
        "RUNNER_TOTAL",
        "CX43_ONLINE",
        "CX43_IDLE_READY",
        "CPX42_ONLINE",
        "CPX42_IDLE_READY",
        "CX53_ONLINE",
        "CX53_IDLE_READY",
        "EVENT_NAME",
        "DOCS_DETECT_RESULT",
        "DOCS_ONLY",
        "DOCS_GATE_RESULT",
    ] {
        command.env(
            key,
            environment.get(key).and_then(Value::as_str).unwrap_or(""),
        );
    }
    if let Some(label) = environment.get("LABEL").and_then(Value::as_str) {
        command.env("LABEL", label);
    }
    let output = command.output().map_err(|e| e.to_string())?;
    let result = output.status.success();
    if matches!(output.status.code(), Some(124 | 137)) {
        return Err("bounded workflow replay timed out; scratch retained".into());
    }
    fs::remove_dir_all(&temp).map_err(|e| e.to_string())?;
    Ok(result)
}
#[test]
fn final_readiness_replays_all_independent_live_drift_vectors() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let script = body("Recheck current head and base before result publication")?;
    let cases = vectors["live_drift_cases"]
        .as_array()
        .ok_or("cases missing")?;
    assert_eq!(cases.len(), 12);
    for case in cases {
        assert_eq!(
            execute(
                &script,
                &case["live_pull_request"],
                &case["reviews"],
                &json!({})
            )?,
            case["expected"]["contract_allows_required_success"]
                .as_bool()
                .ok_or("expected missing")?,
            "{}",
            case["id"]
        );
    }
    Ok(())
}
#[test]
fn final_result_replays_all_independent_terminal_vectors() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let script = steps(&[
        body("Recheck current head and base before result publication")?,
        body("Summarize routed result")?,
    ]);
    let drift = vectors["live_drift_cases"]
        .as_array()
        .ok_or("drift missing")?;
    let events = vectors["event_cases"].as_array().ok_or("events missing")?;
    let cases = vectors["terminal_cases"]
        .as_array()
        .ok_or("terminal missing")?;
    assert_eq!(cases.len(), 6);
    for case in cases {
        let live = drift
            .iter()
            .find(|v| v["id"] == case["drift_case"])
            .unwrap_or(&drift[0]);
        let event = events
            .iter()
            .find(|v| v["id"] == case["event_case"])
            .ok_or("event missing")?;
        let mut environment = case["environment"].clone();
        environment["LABEL"] = event["contexts"]["github"]["event"]["label"]["name"].clone();
        let requested = event["expected"]["qualification_request"]
            .as_bool()
            .ok_or("request missing")?;
        let expected = case["expected"]["contract_required_success"]
            .as_bool()
            .ok_or("expected missing")?;
        assert_eq!(
            execute(
                &script,
                &live["live_pull_request"],
                &live["reviews"],
                &environment
            )?,
            expected || !requested,
            "{}",
            case["id"]
        );
    }
    Ok(())
}

#[test]
fn ordinary_comments_cannot_erase_native_review_dispositions() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let pr = &vectors["live_drift_cases"][0]["live_pull_request"];
    let script = body("Recheck current head and base before result publication")?;
    let reviewer = |state: &str, time: &str, user: &str, text: &str| {
        json!({
            "user": {"login": user}, "state": state, "submitted_at": time,
            "commit_id": "a".repeat(40), "body": text
        })
    };
    let requested = reviewer(
        "CHANGES_REQUESTED",
        "2026-10-15T01:00:00Z",
        "alice",
        "repair required",
    );
    let comment = reviewer(
        "COMMENTED",
        "2026-10-15T02:00:00Z",
        "alice",
        "ordinary follow-up",
    );
    let approved = reviewer("APPROVED", "2026-10-15T01:00:00Z", "bob", "");
    let marker = reviewer(
        "COMMENTED",
        "2026-10-15T03:00:00Z",
        "alice",
        "SOURCE_REVIEW_READY",
    );
    for tail in [comment, marker] {
        assert!(!execute(
            &script,
            pr,
            &json!([requested.clone(), tail, approved.clone()]),
            &json!({})
        )?);
    }
    // Native approval or dismissal resolves that reviewer's requested changes;
    // an ordinary comment does not revoke another reviewer's approval.
    for state in ["APPROVED", "DISMISSED"] {
        let resolved = reviewer(state, "2026-10-15T04:00:00Z", "alice", "");
        assert!(execute(
            &script,
            pr,
            &json!([requested.clone(), resolved, approved.clone()]),
            &json!({})
        )?);
    }
    // Admission and final refresh must use exactly the same review reducer.
    let admission = body("Reconcile native request and current readiness")?;
    let reducer = |text: &str| -> Result<String, String> {
        Ok(text
            .split_once("[ .[][] ] as $reviews |")
            .ok_or("review reducer missing")?
            .1
            .split_once("' <<< \"$reviews\"")
            .ok_or("review reducer end missing")?
            .0
            .to_owned())
    };
    assert_eq!(reducer(&admission)?, reducer(&script)?);
    Ok(())
}

#[test]
fn outstanding_old_head_changes_requests_block_both_readiness_boundaries() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let pr = &vectors["live_drift_cases"][0]["live_pull_request"];
    let review = |user: &str, state: &str, head: char, time: u8, text: &str| {
        json!({
            "user": {"login": user}, "state": state,
            "commit_id": head.to_string().repeat(40), "submitted_at": format!("2026-10-15T0{time}:00:00Z"), "body": text
        })
    };
    let requested = review("alice", "CHANGES_REQUESTED", 'e', 1, "unresolved");
    let approved = review("bob", "APPROVED", 'a', 2, "");
    let old_approval = review("bob", "APPROVED", 'e', 2, "");
    for name in [
        "Reconcile native request and current readiness",
        "Recheck current head and base before result publication",
    ] {
        let script = body(name)?;
        for state in ["ordinary follow-up", "SOURCE_REVIEW_READY", "REVIEW_READY"] {
            let comment = review("alice", "COMMENTED", 'a', 3, state);
            assert!(
                !execute(
                    &script,
                    pr,
                    &json!([requested.clone(), comment, approved.clone()]),
                    &json!({})
                )?,
                "{name}: {state}"
            );
        }
        assert!(
            !execute(
                &script,
                pr,
                &json!([requested.clone(), approved.clone()]),
                &json!({})
            )?,
            "{name}: unresolved older CR"
        );
        for state in ["APPROVED", "DISMISSED"] {
            let resolved = review("alice", state, 'e', 4, "");
            assert!(
                execute(
                    &script,
                    pr,
                    &json!([requested.clone(), resolved.clone(), approved.clone()]),
                    &json!({})
                )?,
                "{name}: resolved CR with fresh approval"
            );
            assert!(
                !execute(
                    &script,
                    pr,
                    &json!([requested.clone(), resolved, old_approval.clone()]),
                    &json!({})
                )?,
                "{name}: resolution is not fresh positive evidence"
            );
        }
        assert!(
            !execute(&script, pr, &json!([old_approval.clone()]), &json!({}))?,
            "{name}: stale positive evidence"
        );
    }
    Ok(())
}

#[test]
fn terminal_cancellation_check_rejects_green_dependencies_and_prior_success() -> Result<(), String>
{
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let live = &vectors["live_drift_cases"][0];
    let aggregate = body("Summarize routed result")?;
    let guard = body("Reject workflow cancellation before final result")?;
    // Separate steps: exit 0 in the reuse branch must not bypass the terminal guard.
    let mut env = json!({"ADMISSION_RESULT":"success", "EXECUTE":"true", "PRIOR_RESULT":"none", "WINDOWS_RESULT":"success",
        "DOCS_DETECT_RESULT":"success", "DOCS_ONLY":"false", "DOCS_GATE_RESULT":"skipped", "ROUTE_RESULT":"success", "TARGET":"github", "GITHUB_RESULT":"success", "EVENT_NAME":"pull_request"});
    let terminal = WORKFLOW
        .split_once("      - name: Reject workflow cancellation before final result\n")
        .ok_or("guard step missing")?
        .1;
    assert!(terminal.starts_with("        if: always() && cancelled()\n        run: |\n"));
    assert!(!terminal.contains("continue-on-error"));
    assert!(
        !terminal.contains("      - name:"),
        "cancellation evaluated last"
    );
    // Pin the actual native step condition above. These literal condition cases
    // model its documented boolean semantics, not GitHub runner event delivery.
    for reuse in [false, true] {
        env["EXECUTE"] = json!(if reuse { "false" } else { "true" });
        env["PRIOR_RESULT"] = json!(if reuse { "success" } else { "none" });
        env["WINDOWS_RESULT"] = json!(if reuse { "skipped" } else { "success" });
        for cancelled in [false, true] {
            let sequence = steps(&[
                aggregate.clone(),
                if cancelled {
                    guard.clone()
                } else {
                    String::from(":")
                },
            ]);
            assert_eq!(
                execute(
                    &sequence,
                    &live["live_pull_request"],
                    &live["reviews"],
                    &env
                )?,
                !cancelled,
                "reuse={reuse}, cancellation={cancelled}"
            );
        }
    }
    // The rejection body fails even when all default-success steps were skipped
    // or interrupted by cancellation, and a skipped guard cannot erase failure.
    assert!(!execute(
        &guard,
        &live["live_pull_request"],
        &live["reviews"],
        &json!({})
    )?);
    env["EXECUTE"] = json!("true");
    env["WINDOWS_RESULT"] = json!("cancelled");
    assert!(!execute(
        &aggregate,
        &live["live_pull_request"],
        &live["reviews"],
        &env
    )?);
    Ok(())
}
