//! Execute retained native readiness/result shell against independent API vectors.
#![cfg(target_os = "linux")]
use serde_json::{Value, json};
#[path = "support/review_disposition.rs"]
mod review_disposition;
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
    execute_review_stream(script, pr, &reviews.to_string(), environment)
}
// `gh api --paginate` writes separate JSON arrays, not one nested array.
fn execute_review_stream(
    script: &str,
    pr: &Value,
    reviews: &str,
    environment: &Value,
) -> Result<bool, String> {
    let temp = std::env::temp_dir().join(format!(
        "ripr-native-admission-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir(&temp).map_err(|e| e.to_string())?;
    let mock = r#"gh() {
      [ "$1" = api ] || return 81; shift
      local paginated=false
      if [ "$1" = --paginate ]; then paginated=true; shift; fi
      case "$1" in
        */pulls/8008) printf '%s\n' "$MOCK_PR";;
        */pulls/8008/reviews*)
          if [ "$paginated" = true ]; then printf '%s\n' "$MOCK_REVIEWS";
          else jq -cs '.[0]' <<< "$MOCK_REVIEWS"; fi;;
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
        .env("REVIEW_DISPOSITION_JQ", review_disposition::program(WORKFLOW))
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
        .env("MOCK_REVIEWS", reviews)
        .env("GITHUB_OUTPUT", temp.join("out"))
        .env("GITHUB_STEP_SUMMARY", temp.join("summary"));
    for key in [
        "ADMISSION_RESULT",
        "EXECUTE",
        "PRIOR_RESULT",
        "WINDOWS_RESULT",
        "PASTE_RESULT",
        "CI_RESULT",
        "WHEEL_RESULT",
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
        "REF",
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
    if !result && script.contains("execute=true") {
        let allocation = fs::read_to_string(temp.join("out")).unwrap_or_default();
        assert!(
            !allocation.lines().any(|line| line == "execute=true"),
            "refused admission must never authorize product allocation"
        );
    }
    if matches!(output.status.code(), Some(124 | 137)) {
        return Err("bounded workflow replay timed out; scratch retained".into());
    }
    fs::remove_dir_all(&temp).map_err(|e| e.to_string())?;
    Ok(result)
}

#[test]
fn current_attestation_rejects_narrative_readiness_without_changing_native_reviews()
-> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let pr = &vectors["live_drift_cases"][0]["live_pull_request"];
    let comment = |body: &str, state: &str, head: char, second: u8| {
        json!({"id":second,"user":{"login":"author"},"state":state,
            "commit_id":head.to_string().repeat(40),
            "submitted_at":format!("2026-10-15T01:00:0{second}Z"),"body":body})
    };
    for script in review_boundary_bodies()? {
        for (body, expected) in [
            ("SOURCE_REVIEW_READY", true),
            ("REVIEW_READY", true),
            ("REVIEW_INCOMPLETE", false),
            (
                "REVIEW_INCOMPLETE. This candidate is not SOURCE_REVIEW_READY; behavioral proof is missing.",
                false,
            ),
            ("REVIEW_INCOMPLETE. Example:\n```\nREVIEW_READY\n```", false),
            (
                "Previous result was REVIEW_READY; current disposition: REPAIR_REQUIRED.",
                false,
            ),
            ("No longer SOURCE_REVIEW_READY. REPAIR_REQUIRED.", false),
            (
                "SOURCE_REVIEW_READY — source complete; overall REVIEW_INCOMPLETE pending hosted qualification.",
                true,
            ),
            ("SOURCE_REVIEW_READY remains current", true),
            ("\r\n  SOURCE_REVIEW_READY\r\n", true),
            (
                "reviewed_head_sha: a\ndisposition: SOURCE_REVIEW_READY\n",
                true,
            ),
            ("disposition: REVIEW_READY", true),
            (
                "disposition: REVIEW_READY\ndisposition: REPAIR_REQUIRED",
                false,
            ),
            ("SOURCE_REVIEW_READY\ndisposition: REVIEW_INCOMPLETE", false),
            ("disposition: UNKNOWN", false),
            ("`SOURCE_REVIEW_READY`", false),
            ("> SOURCE_REVIEW_READY", false),
            ("- SOURCE_REVIEW_READY", false),
            ("    SOURCE_REVIEW_READY", false),
            ("\tSOURCE_REVIEW_READY", false),
            ("SOURCE_REVIEW_READY_suffix", false),
            ("SOURCE_REVIEW_READYish", false),
            ("```\ndisposition: REVIEW_READY\n```", false),
            ("~~~text\ndisposition: REVIEW_READY\n~~~", false),
            ("````text\n```\ndisposition: REVIEW_READY\n````", false),
            ("> historical example\ndisposition: REVIEW_READY", false),
            (
                "historical example:\n```json\n{\"disposition\":\"REVIEW_READY\"}\n```",
                false,
            ),
            (
                "review surfaces\n```text\ndisposition: REPAIR_REQUIRED\n```\ndisposition: SOURCE_REVIEW_READY",
                true,
            ),
            ("SOURCE_REVIEW_READY\n```\nunterminated", false),
        ] {
            assert_eq!(
                execute(
                    &script,
                    pr,
                    &json!([comment(body, "COMMENTED", 'a', 1)]),
                    &json!({})
                )?,
                expected,
                "{body}"
            );
        }
        let ready = comment("SOURCE_REVIEW_READY", "COMMENTED", 'a', 1);
        for (later, expected) in [
            (
                comment(
                    "No longer SOURCE_REVIEW_READY. REPAIR_REQUIRED.",
                    "COMMENTED",
                    'a',
                    2,
                ),
                false,
            ),
            (
                comment("repair required", "CHANGES_REQUESTED", 'a', 2),
                false,
            ),
        ] {
            assert_eq!(
                execute(&script, pr, &json!([ready.clone(), later]), &json!({}))?,
                expected
            );
        }
        assert!(!execute(
            &script,
            pr,
            &json!([comment("SOURCE_REVIEW_READY", "COMMENTED", 'e', 1)]),
            &json!({})
        )?);
        assert!(execute(
            &script,
            pr,
            &json!([comment("", "APPROVED", 'a', 1)]),
            &json!({})
        )?);
    }
    Ok(())
}

#[test]
fn observed_producer_review_bodies_reach_both_actual_consumers() -> Result<(), String> {
    let reviews: Value = serde_json::from_str(include_str!(
        "../../../fixtures/convergence/native-late-admission/observed-review-bodies.json"
    ))
    .map_err(|e| e.to_string())?;
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let pr = &vectors["live_drift_cases"][0]["live_pull_request"];
    let reviews = reviews.as_array().ok_or("observed reviews missing")?;
    assert_eq!(reviews.len(), 5, "nonempty retained producer sample");
    for observed in reviews {
        let review = json!({"id":observed["id"],"user":{"login":"author"},
            "state":"COMMENTED","commit_id":"a".repeat(40),
            "submitted_at":"2026-10-15T01:00:00Z","body":observed["body"]});
        // Real, unchanged published body; synthetic API subject isolates format
        // compatibility. Historical bodies are not fresh clearance for this PR.
        for script in review_boundary_bodies()? {
            assert!(
                execute(&script, pr, &json!([review.clone()]), &json!({}))?,
                "{}",
                observed["url"]
            );
        }
    }
    Ok(())
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
        environment["PASTE_RESULT"] = json!("success");
        environment["CI_RESULT"] = json!("success");
        environment["WHEEL_RESULT"] = json!("success");
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
        // The diagnostic jq also inspects review pages. Compare the actual
        // authoritative -s -e command, never its advisory lookalike.
        let command = "jq -s -e --arg head \"$HEAD_SHA\" --arg author \"$author\" \"$REVIEW_DISPOSITION_JQ\"'";
        assert_eq!(text.matches(command).count(), 1, "unique decision command");
        let decision = text.split_once(command).ok_or("review decision missing")?.1;
        Ok(decision
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
    let mut env = json!({"ADMISSION_RESULT":"success", "EXECUTE":"true", "PRIOR_RESULT":"none", "WINDOWS_RESULT":"success", "PASTE_RESULT":"success", "CI_RESULT":"success", "WHEEL_RESULT":"success",
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

fn paged_review(user: &str, state: &str, head: char, second: u8, id: u64, text: &str) -> Value {
    json!({"id": id, "user": {"login": user}, "state": state,
        "commit_id": head.to_string().repeat(40),
        "submitted_at": format!("2026-10-15T01:00:{second:02}Z"), "body": text})
}
fn review_pages(first: Value, tail: Vec<Value>) -> Vec<Value> {
    // Production asks for 100 reviews per page. The first page is genuinely full;
    // distinct inert comments keep pagination shape realistic without readiness.
    let mut page: Vec<_> = (1..100)
        .map(|id| paged_review("observer", "COMMENTED", 'e', 0, id, "ordinary"))
        .collect();
    page.push(first);
    vec![json!(page), json!(tail)]
}
fn review_stream(pages: &[Value]) -> String {
    pages
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
fn review_boundary_bodies() -> Result<Vec<String>, String> {
    [
        "Reconcile native request and current readiness",
        "Recheck current head and base before result publication",
    ]
    .into_iter()
    .map(body)
    .collect()
}

#[test]
fn streamed_review_pages_preserve_outstanding_and_tied_dispositions() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let pr = &vectors["live_drift_cases"][0]["live_pull_request"];
    let fresh = paged_review("bob", "APPROVED", 'a', 4, 9400, "");
    // Expected dispositions are literal. Realistic distinct IDs deliberately
    // decrease across a tie: API/input chronology, not ID sorting, is the contract.
    for (first, last, last_head, second, positive, expected) in [
        ("CHANGES_REQUESTED", "APPROVED", 'a', 1, true, true),
        ("APPROVED", "CHANGES_REQUESTED", 'e', 1, true, false),
        ("CHANGES_REQUESTED", "DISMISSED", 'e', 1, true, true),
        ("CHANGES_REQUESTED", "APPROVED", 'e', 1, false, false),
        ("CHANGES_REQUESTED", "DISMISSED", 'e', 1, false, false),
        ("CHANGES_REQUESTED", "APPROVED", 'a', 2, true, true),
        ("APPROVED", "CHANGES_REQUESTED", 'e', 2, true, false),
    ] {
        let initial = paged_review("alice", first, 'e', 1, 9300, "");
        let later = paged_review("alice", last, last_head, second, 9200, "");
        for marker in ["ordinary follow-up", "SOURCE_REVIEW_READY", "REVIEW_READY"] {
            // When no fresh positive is intended, keep even the marker stale.
            let comment = paged_review(
                "alice",
                "COMMENTED",
                if positive { 'a' } else { 'e' },
                3,
                9350,
                marker,
            );
            let mut tail = vec![later.clone(), comment];
            if positive {
                tail.push(fresh.clone());
            }
            let pages = review_pages(initial.clone(), tail);
            let stream = review_stream(&pages);
            assert!(
                serde_json::from_str::<Value>(&stream).is_err(),
                "stream must contain separate page arrays"
            );
            assert_eq!(
                pages[0].as_array().ok_or("first page not array")?.len(),
                100
            );
            for script in review_boundary_bodies()? {
                assert_eq!(
                    execute_review_stream(&script, pr, &stream, &json!({}))?,
                    expected,
                    "{first}->{last}, second={second}, positive={positive}, comment={marker}"
                );
            }
        }
    }
    // No later decisive state: fresh comments/markers and someone else's fresh
    // approval must not hide the outstanding first-page change request.
    for marker in ["ordinary", "SOURCE_REVIEW_READY", "REVIEW_READY"] {
        let pages = review_pages(
            paged_review("alice", "CHANGES_REQUESTED", 'e', 1, 9300, ""),
            vec![
                paged_review("alice", "COMMENTED", 'a', 3, 9350, marker),
                fresh.clone(),
            ],
        );
        for script in review_boundary_bodies()? {
            assert!(!execute_review_stream(
                &script,
                pr,
                &review_stream(&pages),
                &json!({})
            )?);
        }
    }
    Ok(())
}

#[test]
fn streamed_review_controls_reject_page_loss_and_first_disposition() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let pr = &vectors["live_drift_cases"][0]["live_pull_request"];
    let fresh = paged_review("bob", "APPROVED", 'a', 4, 9400, "");
    let requested = paged_review("alice", "CHANGES_REQUESTED", 'e', 1, 9300, "");
    let repaired = review_pages(
        requested.clone(),
        vec![
            paged_review("alice", "APPROVED", 'a', 1, 9200, ""),
            fresh.clone(),
        ],
    );
    let outstanding = review_pages(requested, vec![fresh]);
    for script in review_boundary_bodies()? {
        assert!(execute_review_stream(
            &script,
            pr,
            &review_stream(&repaired),
            &json!({})
        )?);
        assert!(!execute_review_stream(
            &script,
            pr,
            &review_stream(&outstanding),
            &json!({})
        )?);
        let one_page = script.replace(
            "gh api --paginate \"repos/$REPOSITORY/pulls/$PR_NUMBER/reviews?per_page=100\"",
            "gh api \"repos/$REPOSITORY/pulls/$PR_NUMBER/reviews?per_page=100\"",
        );
        assert_ne!(one_page, script, "pagination removal must engage");
        assert!(
            !execute_review_stream(&one_page, pr, &review_stream(&repaired), &json!({}))?,
            "dropping final page loses genuine resolution"
        );
        assert!(
            execute_review_stream(&script, pr, &review_stream(&outstanding[1..]), &json!({}))?,
            "dropping first page recreates false success; unchanged expected=false rejects it"
        );
        let first_only = script.replace(
            "map(max_by(.submitted_at))) as $dispositions",
            "map(first)) as $dispositions",
        );
        assert_ne!(first_only, script, "wrong disposition mutation must engage");
        assert!(
            !execute_review_stream(&first_only, pr, &review_stream(&repaired), &json!({}))?,
            "selecting first decisive review loses genuine resolution"
        );
    }
    Ok(())
}

#[test]
fn tied_review_order_challenge_pins_input_order_not_review_id() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let pr = &vectors["live_drift_cases"][0]["live_pull_request"];
    for second in [1, 2] {
        let pages = review_pages(
            paged_review("alice", "CHANGES_REQUESTED", 'e', 1, 9300, ""),
            vec![paged_review("alice", "APPROVED", 'a', second, 9200, "")],
        );
        for script in review_boundary_bodies()? {
            assert!(execute_review_stream(
                &script,
                pr,
                &review_stream(&pages),
                &json!({})
            )?);
            let reversed = vec![pages[1].clone(), pages[0].clone()];
            // Reversed pages violate the expected API chronology. At a tie the
            // result changes; with genuinely later time max_by still selects it.
            assert_eq!(
                execute_review_stream(&script, pr, &review_stream(&reversed), &json!({}))?,
                second == 2
            );
        }
    }
    Ok(())
}

#[test]
fn printed_child_is_mandatory_and_native_failures_reach_the_aggregate() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let live = &vectors["live_drift_cases"][0];
    let script = body("Summarize routed result")?;
    let mut env = json!({"ADMISSION_RESULT":"success", "EXECUTE":"true", "PRIOR_RESULT":"none",
        "WINDOWS_RESULT":"success", "PASTE_RESULT":"success", "CI_RESULT":"success", "WHEEL_RESULT":"success", "DOCS_DETECT_RESULT":"success",
        "DOCS_ONLY":"false", "ROUTE_RESULT":"success", "TARGET":"github", "GITHUB_RESULT":"success",
        "EVENT_NAME":"pull_request", "DOCS_GATE_RESULT":"success"});
    for docs in ["false", "true"] {
        env["DOCS_ONLY"] = json!(docs);
        for result in [
            "success",
            "failure",
            "skipped",
            "cancelled",
            "timed_out",
            "",
            "neutral",
        ] {
            env["PASTE_RESULT"] = json!(result);
            assert_eq!(
                execute(&script, &live["live_pull_request"], &live["reviews"], &env)?,
                result == "success",
                "docs={docs}, printed result={result}"
            );
        }
    }
    // Native prior-result reuse skips children only after admission/final refresh.
    env["EXECUTE"] = json!("false");
    env["PRIOR_RESULT"] = json!("success");
    env["PASTE_RESULT"] = json!("skipped");
    assert!(execute(
        &script,
        &live["live_pull_request"],
        &live["reviews"],
        &env
    )?);
    env["PRIOR_RESULT"] = json!("failure");
    assert!(!execute(
        &script,
        &live["live_pull_request"],
        &live["reviews"],
        &env
    )?);
    Ok(())
}

#[test]
fn printed_call_binds_every_admitted_input_and_required_dependency() -> Result<(), String> {
    let (_, call) = WORKFLOW
        .split_once("  printed-command:\n")
        .ok_or("printed call missing")?;
    let (call, _) = call.split_once("  result:\n").ok_or("result missing")?;
    assert!(call.contains("needs: [admission]"));
    assert!(call.contains("if: needs.admission.outputs.execute == 'true'"));
    assert!(call.contains("uses: ./.github/workflows/printed-command-paste.yml"));
    for binding in [
        "proof_profile: qualification",
        "request_id: ${{ format('{0}-{1}', github.run_id, github.run_attempt) }}",
        "head_sha: ${{ github.event.pull_request.head.sha }}",
        "base_sha: ${{ github.event.pull_request.base.sha }}",
        "subject_sha: ${{ github.sha }}",
        "subject_tree: ${{ needs.admission.outputs.tree }}",
        "workflow_generation: ${{ github.workflow_sha }}",
    ] {
        assert!(call.contains(binding), "missing {binding}");
    }
    assert!(!call.contains("continue-on-error"));
    let (_, result) = WORKFLOW.split_once("  result:\n").ok_or("result missing")?;
    let dependency = result
        .lines()
        .find(|line| line.starts_with("    needs:"))
        .ok_or("needs missing")?;
    assert!(dependency.contains(", printed-command]"));
    assert!(result.contains("PASTE_RESULT: ${{ needs.printed-command.result }}"));
    assert!(result.contains("    if: always()"));
    Ok(())
}

#[test]
fn ci_native_result_is_mandatory_for_product_and_docs_subjects() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let live = &vectors["live_drift_cases"][0];
    let script = body("Summarize routed result")?;
    let mut env = json!({"ADMISSION_RESULT":"success", "EXECUTE":"true", "PRIOR_RESULT":"none",
        "WINDOWS_RESULT":"success", "PASTE_RESULT":"success", "CI_RESULT":"success", "WHEEL_RESULT":"success",
        "DOCS_DETECT_RESULT":"success", "DOCS_ONLY":"false", "DOCS_GATE_RESULT":"success",
        "ROUTE_RESULT":"success", "TARGET":"github", "GITHUB_RESULT":"success", "EVENT_NAME":"pull_request"});
    for docs in ["false", "true"] {
        env["DOCS_ONLY"] = json!(docs);
        for result in [
            "success",
            "failure",
            "skipped",
            "cancelled",
            "timed_out",
            "",
            "neutral",
        ] {
            env["CI_RESULT"] = json!(result);
            assert_eq!(
                execute(&script, &live["live_pull_request"], &live["reviews"], &env)?,
                result == "success",
                "docs={docs}, CI result={result}"
            );
        }
    }
    env["EXECUTE"] = json!("false");
    env["PRIOR_RESULT"] = json!("success");
    env["CI_RESULT"] = json!("skipped");
    assert!(execute(
        &script,
        &live["live_pull_request"],
        &live["reviews"],
        &env
    )?);
    env["PRIOR_RESULT"] = json!("failure");
    assert!(!execute(
        &script,
        &live["live_pull_request"],
        &live["reviews"],
        &env
    )?);
    Ok(())
}

#[test]
fn ci_call_binds_identities_and_conservatively_freezes_all_rows() -> Result<(), String> {
    let (_, call) = WORKFLOW
        .split_once("  ci-feature:\n")
        .ok_or("CI call missing")?;
    let (call, _) = call
        .split_once("  printed-command:\n")
        .ok_or("printed child missing")?;
    assert!(call.contains("needs: [admission]"));
    assert!(call.contains("if: needs.admission.outputs.execute == 'true'"));
    assert!(call.contains("uses: ./.github/workflows/ci.yml"));
    for binding in [
        "proof_profile: qualification",
        "request_id: ${{ format('{0}-{1}', github.run_id, github.run_attempt) }}",
        "head_sha: ${{ github.event.pull_request.head.sha }}",
        "base_sha: ${{ github.event.pull_request.base.sha }}",
        "subject_sha: ${{ github.sha }}",
        "subject_tree: ${{ needs.admission.outputs.tree }}",
        "workflow_generation: ${{ github.workflow_sha }}",
        "run_rust_only: true",
        "run_release_proof: true",
        "run_msrv: true",
        "run_vscode: true",
    ] {
        assert!(call.contains(binding), "missing {binding}");
    }
    assert!(!call.contains("continue-on-error"));
    let (_, result) = WORKFLOW.split_once("  result:\n").ok_or("result missing")?;
    let dependency = result
        .lines()
        .find(|line| line.starts_with("    needs:"))
        .ok_or("needs missing")?;
    assert!(dependency.contains(", ci-feature,"));
    assert!(result.contains("CI_RESULT: ${{ needs.ci-feature.result }}"));
    Ok(())
}
#[test]
fn wheel_native_result_is_mandatory_for_product_and_docs_subjects() -> Result<(), String> {
    let vectors: Value = serde_json::from_str(VECTORS).map_err(|e| e.to_string())?;
    let live = &vectors["live_drift_cases"][0];
    let script = body("Summarize routed result")?;
    let mut env = json!({"ADMISSION_RESULT":"success", "EXECUTE":"true", "PRIOR_RESULT":"none",
        "WINDOWS_RESULT":"success", "PASTE_RESULT":"success", "WHEEL_RESULT":"success", "CI_RESULT":"success",
        "DOCS_DETECT_RESULT":"success", "DOCS_ONLY":"false", "DOCS_GATE_RESULT":"success",
        "ROUTE_RESULT":"success", "TARGET":"github", "GITHUB_RESULT":"success", "EVENT_NAME":"pull_request"});
    for docs in ["false", "true"] {
        env["DOCS_ONLY"] = json!(docs);
        for result in [
            "success",
            "failure",
            "skipped",
            "cancelled",
            "timed_out",
            "",
            "neutral",
        ] {
            env["WHEEL_RESULT"] = json!(result);
            assert_eq!(
                execute(&script, &live["live_pull_request"], &live["reviews"], &env)?,
                result == "success",
                "docs={docs}, wheel result={result}"
            );
        }
    }
    env["EXECUTE"] = json!("false");
    env["PRIOR_RESULT"] = json!("success");
    env["WHEEL_RESULT"] = json!("skipped");
    assert!(execute(
        &script,
        &live["live_pull_request"],
        &live["reviews"],
        &env
    )?);
    env["PRIOR_RESULT"] = json!("failure");
    assert!(!execute(
        &script,
        &live["live_pull_request"],
        &live["reviews"],
        &env
    )?);
    Ok(())
}

#[test]
fn wheel_call_binds_every_identity_and_native_result() -> Result<(), String> {
    let (_, call) = WORKFLOW
        .split_once("  python-wheel:\n")
        .ok_or("wheel call missing")?;
    let (call, _) = call
        .split_once("  ci-feature:\n")
        .ok_or("CI child missing")?;
    assert!(call.contains("needs: [admission]"));
    assert!(call.contains("if: needs.admission.outputs.execute == 'true'"));
    assert!(call.contains("uses: ./.github/workflows/python-wheel-qualification.yml"));
    for binding in [
        "proof_profile: qualification",
        "request_id: ${{ format('{0}-{1}', github.run_id, github.run_attempt) }}",
        "head_sha: ${{ github.event.pull_request.head.sha }}",
        "base_sha: ${{ github.event.pull_request.base.sha }}",
        "subject_sha: ${{ github.sha }}",
        "subject_tree: ${{ needs.admission.outputs.tree }}",
        "workflow_generation: ${{ github.workflow_sha }}",
    ] {
        assert!(call.contains(binding), "missing {binding}");
    }
    assert!(!call.contains("continue-on-error"));
    assert!(!call.contains("secrets:"));
    let (_, result) = WORKFLOW.split_once("  result:\n").ok_or("result missing")?;
    let dependency = result
        .lines()
        .find(|line| line.starts_with("    needs:"))
        .ok_or("needs missing")?;
    assert!(dependency.contains(", python-wheel,"));
    assert!(result.contains("WHEEL_RESULT: ${{ needs.python-wheel.result }}"));
    Ok(())
}

#[test]
fn legacy_profiles_require_actual_product_results_and_no_pr_admission() -> Result<(), String> {
    let script = body("Summarize routed result")?;
    for (event, reference) in [
        ("push", "refs/heads/main"),
        ("push", "refs/heads/master"),
        ("workflow_dispatch", "refs/heads/main"),
        ("workflow_dispatch", "refs/heads/rehearsal"),
    ] {
        let mut env = json!({"EVENT_NAME":event,"REF":reference,"LABEL":"",
            "ADMISSION_RESULT":"skipped","EXECUTE":"","PRIOR_RESULT":"",
            "WINDOWS_RESULT":"skipped","PASTE_RESULT":"skipped","CI_RESULT":"skipped","WHEEL_RESULT":"skipped",
            "DOCS_DETECT_RESULT":"success","DOCS_ONLY":"false","DOCS_GATE_RESULT":"skipped",
            "ROUTE_RESULT":"success","TARGET":"github","GITHUB_RESULT":"success"});
        assert!(
            execute(&script, &json!({}), &json!([]), &env)?,
            "{event}/{reference}"
        );
        for key in ["DOCS_DETECT_RESULT", "ROUTE_RESULT", "GITHUB_RESULT"] {
            for state in [
                "failure",
                "cancelled",
                "skipped",
                "neutral",
                "timed_out",
                "",
            ] {
                let mut negative = env.clone();
                negative[key] = json!(state);
                assert!(
                    !execute(&script, &json!({}), &json!([]), &negative)?,
                    "{event} {key}={state}"
                );
            }
        }
        for key in [
            "ADMISSION_RESULT",
            "WINDOWS_RESULT",
            "PASTE_RESULT",
            "CI_RESULT",
            "WHEEL_RESULT",
        ] {
            for state in ["success", "failure", "cancelled", ""] {
                let mut negative = env.clone();
                negative[key] = json!(state);
                assert!(
                    !execute(&script, &json!({}), &json!([]), &negative)?,
                    "{event} {key}={state}"
                );
            }
        }
        // A legacy full run cannot accept a docs-only shortcut or unknown route.
        env["DOCS_ONLY"] = json!("true");
        env["DOCS_GATE_RESULT"] = json!("success");
        env["GITHUB_RESULT"] = json!("failure");
        assert!(!execute(&script, &json!({}), &json!([]), &env)?);
        env["GITHUB_RESULT"] = json!("success");
        env["TARGET"] = json!("unknown");
        assert!(!execute(&script, &json!({}), &json!([]), &env)?);
    }
    Ok(())
}

#[test]
fn legacy_profiles_retain_selected_runner_failure_and_declared_fallback() -> Result<(), String> {
    let script = body("Summarize routed result")?;
    for (target, result_key, scratch_key) in [
        ("cx43", "CX43_RESULT", "CX43_SCRATCH_STATUS"),
        ("cpx42", "CPX42_RESULT", "CPX42_SCRATCH_STATUS"),
        ("cx53", "CX53_RESULT", "CX53_SCRATCH_STATUS"),
    ] {
        let mut env = json!({"EVENT_NAME":"push","REF":"refs/heads/main","LABEL":"",
            "ADMISSION_RESULT":"skipped","WINDOWS_RESULT":"skipped","PASTE_RESULT":"skipped","CI_RESULT":"skipped","WHEEL_RESULT":"skipped",
            "DOCS_DETECT_RESULT":"success","DOCS_ONLY":"false","ROUTE_RESULT":"success",
            "TARGET":target,"GITHUB_RESULT":"skipped"});
        env[result_key] = json!("success");
        assert!(execute(&script, &json!({}), &json!([]), &env)?);
        for native in ["failure", "cancelled", "skipped", ""] {
            env[result_key] = json!(native);
            env[scratch_key] = json!("");
            env["GITHUB_RESULT"] = json!("success");
            assert!(
                !execute(&script, &json!({}), &json!([]), &env)?,
                "{target}/{native}"
            );
        }
        env[scratch_key] = json!("tempfail");
        assert!(execute(&script, &json!({}), &json!([]), &env)?);
        env["GITHUB_RESULT"] = json!("failure");
        assert!(!execute(&script, &json!({}), &json!([]), &env)?);
    }
    for (event, reference) in [
        ("push", "refs/heads/feature"),
        ("schedule", "refs/heads/main"),
        ("", ""),
    ] {
        assert!(!execute(
            &script,
            &json!({}),
            &json!([]),
            &json!({"EVENT_NAME":event,"REF":reference,"LABEL":""})
        )?);
    }
    Ok(())
}
