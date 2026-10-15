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
    let script = format!(
        "{}\n{}",
        body("Recheck current head and base before result publication")?,
        body("Summarize routed result")?
    );
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
