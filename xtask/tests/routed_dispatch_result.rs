//! #5394: execute the checked-in Result shell, including a real manual
//! admission producer, without dispatching a workflow or creating a check.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const LIVE_PR: &str = r#"{"number":10,"state":"open","draft":false,"head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","repo":{"full_name":"EffortlessMetrics/ripr-swarm"}},"base":{"ref":"main","repo":{"full_name":"EffortlessMetrics/ripr-swarm"}},"labels":[{"name":"merge-ready"}]}"#;

const LIVE_GRAPHQL: &str = r#"{"data":{"repository":{"pullRequest":{"headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reviewDecision":"APPROVED","mergeQueueEntry":{"id":"entry-10"},"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}"#;

const GROUP_HEAD: &str = "cccccccccccccccccccccccccccccccccccccccc";
const PR_HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const REPOSITORY: &str = "EffortlessMetrics/ripr-swarm";

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let path = parent.join(format!(
            "ripr-dispatch-result-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn workflow() -> Result<String, String> {
    fs::read_to_string(root().join(".github/workflows/routed-rust.yml")).map_err(|e| e.to_string())
}

fn result_step(source: &str) -> Result<&str, String> {
    let (_, job) = source
        .split_once("\n  result:\n")
        .ok_or("missing actual result job")?;
    let (_, step) = job
        .split_once("      - name: Summarize routed result\n")
        .ok_or("missing actual Result step")?;
    Ok(step)
}

fn result_shell(step: &str) -> Result<String, String> {
    let (_, body) = step
        .split_once("        run: |\n")
        .ok_or("missing Result shell")?;
    body.lines()
        .map(|line| {
            if line.is_empty() {
                Ok("")
            } else {
                line.strip_prefix("          ")
                    .ok_or("unexpected Result shell nesting")
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|lines| lines.join("\n"))
        .map_err(str::to_string)
}

/// The run identity the eligibility step must find for `event`: the PR head
/// for a label request, the queue-entry commit for a merge-group request.
fn request_identity(event: &str) -> (&'static str, String) {
    if event == "merge_group" {
        (GROUP_HEAD, format!("Merge queue {GROUP_HEAD}"))
    } else {
        (PR_HEAD, format!("Merge-ready request {PR_HEAD}"))
    }
}

fn run_entry(event: &str, id: u64, number: u64, attempt: u64) -> serde_json::Value {
    let (head, title) = request_identity(event);
    serde_json::json!({
        "id": id,
        "run_number": number,
        "run_attempt": attempt,
        "workflow_id": 7,
        "head_sha": head,
        "event": event,
        "display_title": title,
        "repository": {"full_name": REPOSITORY},
    })
}

fn runs_page(total: u64, runs: &[serde_json::Value]) -> String {
    serde_json::json!({"total_count": total, "workflow_runs": runs}).to_string()
}

/// This invocation (run 100, number 7, attempt 1) and a page listing only it.
fn current_runs(event: &str) -> (String, String) {
    let own = run_entry(event, 100, 7, 1);
    (own.to_string(), runs_page(1, &[own]))
}

fn run_result(
    source: &str,
    event: &str,
    reference: &str,
    admission: &str,
    status: &str,
    overrides: &[(&str, &str)],
) -> Result<(bool, String), String> {
    let scratch = Scratch::new()?;
    let step = result_step(source)?;
    let mut env = BTreeMap::new();
    let (own_run, first_page) = current_runs(event);
    // Derive the consumed inputs from the actual step, then give its child
    // outcomes a successful fixture. Tests vary authoritative inputs below.
    for line in step
        .split("        run: |\n")
        .next()
        .ok_or("missing env")?
        .lines()
    {
        if let Some((name, _)) = line
            .strip_prefix("          ")
            .and_then(|l| l.split_once(':'))
        {
            env.insert(name.to_string(), "success".to_string());
        }
    }
    for (name, value) in [
        ("EVENT_NAME", event),
        ("WORKFLOW_REF", reference),
        ("ADMISSION_RESULT", admission),
        ("ADMISSION_STATUS", status),
        ("ADMISSION_REASON", "fixture"),
        ("TARGET", "github"),
        ("DOCS_ONLY", "false"),
        ("LABEL_NAME", "merge-ready"),
        ("REPOSITORY", "EffortlessMetrics/ripr-swarm"),
        ("PR_NUMBER", "10"),
        ("EXPECTED_HEAD", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ("EXPECTED_REQUEST", "100-1"),
        ("GITHUB_RUN_ID", "100"),
        ("GITHUB_RUN_ATTEMPT", "1"),
        ("EXPECTED_QUEUE_ENTRY", "entry-10"),
        ("MOCK_PR", LIVE_PR),
        ("MOCK_GRAPHQL", LIVE_GRAPHQL),
        ("GROUP_HEAD_SHA", GROUP_HEAD),
        ("MOCK_RUN", own_run.as_str()),
        ("MOCK_RUNS_1", first_page.as_str()),
    ]
    .into_iter()
    .chain(overrides.iter().copied())
    {
        env.insert(name.to_string(), value.to_string());
    }
    let summary = scratch.0.join("summary");
    // Passed as a file, not `-c <script>`: Windows argv quoting mangles a
    // multi-line script before bash sees it (#9272).
    let script = scratch.0.join("result.sh");
    fs::write(&script, result_shell(step)?).map_err(|e| e.to_string())?;
    let gh = scratch.0.join("gh");
    fs::write(
        &gh,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$MOCK_CALLS.log"
case "$2" in
  graphql)
    case "$*" in
      *cursor=next*) printf '%s\n' "${MOCK_GRAPHQL_NEXT:-$MOCK_GRAPHQL}" ;;
      *) printf '%s\n' "$MOCK_GRAPHQL" ;;
    esac ;;
  repos/*/actions/runs/100)
    [ "$MOCK_RUN" != fail ] || exit 99
    printf '%s\n' "$MOCK_RUN" ;;
  repos/*/actions/workflows/*/runs?*)
    case "${2##*page=}" in
      1) page="${MOCK_RUNS_1-fail}" ;;
      2) page="${MOCK_RUNS_2-fail}" ;;
      *) page=fail ;;
    esac
    [ "$page" != fail ] || exit 99
    printf '%s\n' "$page" ;;
  repos/*/pulls/10)
    if [ -f "$MOCK_CALLS" ]; then printf '%s\n' "${MOCK_PR_AFTER:-$MOCK_PR}"
    else : > "$MOCK_CALLS"; printf '%s\n' "$MOCK_PR"; fi ;;
  *) exit 97 ;;
esac
"#,
    )
    .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let paths = std::iter::once(scratch.0.clone())
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").ok_or("PATH unavailable")?,
        ))
        .collect::<Vec<_>>();
    env.insert(
        "MOCK_CALLS".to_string(),
        scratch.0.join("calls").to_string_lossy().into_owned(),
    );
    let run_shell = |script: &PathBuf, bindings: &BTreeMap<String, String>| {
        Command::new("bash")
            .arg("-e")
            .arg(script)
            .env_clear()
            .envs(bindings)
            .env(
                "PATH",
                std::env::join_paths(&paths).map_err(|e| e.to_string())?,
            )
            .env("GITHUB_STEP_SUMMARY", &summary)
            .output()
            .map_err(|e| e.to_string())
    };
    let mut eligibility = true;
    let mut eligibility_outcome = "skipped";
    let mut eligibility_log = String::new();
    if admission == "success"
        && status == "admitted"
        && (event == "merge_group"
            || (event == "pull_request"
                && env
                    .get("LABEL_NAME")
                    .is_some_and(|label| label == "merge-ready")))
    {
        let (_, job) = source
            .split_once("      - name: Recheck initially admitted eligibility\n")
            .ok_or("missing actual eligibility step")?;
        let (step, _) = job
            .split_once("      - name: Summarize routed result\n")
            .ok_or("missing summary after eligibility")?;
        let eligibility_script = scratch.0.join("eligibility.sh");
        fs::write(&eligibility_script, result_shell(step)?).map_err(|e| e.to_string())?;
        let output = run_shell(&eligibility_script, &env)?;
        eligibility = output.status.success();
        eligibility_log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        eligibility_outcome = if eligibility { "success" } else { "failure" };
    }
    env.insert(
        "ELIGIBILITY_RESULT".to_string(),
        eligibility_outcome.to_string(),
    );
    let output = run_shell(&script, &env)?;
    let report =
        fs::read_to_string(summary).map_err(|e| format!("missing summary: {e}; {output:?}"))?;
    assert!(report.contains("### Ripr Rust Small Result"));
    // The eligibility step's own log follows the summary so a test can read
    // the stage it named when it refused (#8858).
    Ok((
        eligibility && output.status.success(),
        format!("{report}\n{eligibility_log}"),
    ))
}

#[test]
fn branch_dispatch_cannot_qualify_even_when_every_child_succeeds() -> Result<(), String> {
    for reference in ["refs/heads/fix/5394", "refs/tags/v0.11.0", "", "main"] {
        let (success, report) = run_result(
            &workflow()?,
            "workflow_dispatch",
            reference,
            "success",
            "not_applicable",
            &[],
        )?;
        assert!(
            !success,
            "diagnostic dispatch must not qualify: {reference}: {report}"
        );
        assert!(
            report.contains("diagnostic-only"),
            "must explain the event refusal: {report}"
        );
    }
    Ok(())
}

#[test]
fn ready_and_main_authorities_keep_real_child_result_checks() -> Result<(), String> {
    for (event, reference, status) in [
        ("pull_request", "refs/pull/10/merge", "admitted"),
        (
            "merge_group",
            "refs/heads/gh-readonly-queue/main/pr-10-aaaa",
            "admitted",
        ),
        ("workflow_dispatch", "refs/heads/main", "not_applicable"),
        ("workflow_dispatch", "refs/heads/master", "not_applicable"),
        ("push", "refs/heads/main", "not_applicable"),
    ] {
        assert!(run_result(&workflow()?, event, reference, "success", status, &[])?.0);
        assert!(
            !run_result(
                &workflow()?,
                event,
                reference,
                "success",
                status,
                &[("GITHUB_RESULT", "failure")]
            )?
            .0
        );
    }
    for (admission, status) in [
        ("failure", "admitted"),
        ("success", "refused"),
        ("success", ""),
    ] {
        assert!(
            !run_result(
                &workflow()?,
                "pull_request",
                "refs/pull/10/merge",
                admission,
                status,
                &[]
            )?
            .0
        );
    }
    assert!(
        run_result(
            &workflow()?,
            "pull_request",
            "refs/pull/10/merge",
            "success",
            "admitted",
            &[("DOCS_ONLY", "true")]
        )?
        .0
    );
    assert!(
        !run_result(
            &workflow()?,
            "pull_request",
            "refs/pull/10/merge",
            "success",
            "admitted",
            &[("DOCS_ONLY", "true"), ("DOCS_GATE_RESULT", "skipped")]
        )?
        .0
    );
    Ok(())
}

#[test]
fn refused_admission_summary_names_the_concrete_detail() -> Result<(), String> {
    let detail = "missing or placeholder PR body line(s): Governing issue or accepted claim:, Acceptance delta (one sentence):";
    let (success, report) = run_result(
        &workflow()?,
        "pull_request",
        "refs/pull/10/merge",
        "failure",
        "refused",
        &[
            ("ADMISSION_REASON", "claim_identity_missing"),
            ("ADMISSION_DETAIL", detail),
        ],
    )?;
    assert!(!success, "refused admission must fail the result: {report}");
    assert!(
        report.contains("claim_identity_missing") && report.contains(detail),
        "result summary must name the single concrete refusal reason, got: {report}"
    );
    Ok(())
}

#[test]
fn hard_cut_refusal_of_admitted_candidate_names_the_cut_reason() -> Result<(), String> {
    let cut_detail = "the active hard-cut freeze refuses ordinary qualification";
    let (success, report) = run_result(
        &workflow()?,
        "pull_request",
        "refs/pull/10/merge",
        "failure",
        "admitted",
        &[
            ("ADMISSION_REASON", "candidate_current_unique_bounded"),
            ("ADMISSION_DETAIL", ""),
            ("ADMISSION_CUT_STATUS", "refused"),
            ("ADMISSION_CUT_REASON", "cut_frozen"),
            ("ADMISSION_CUT_DETAIL", cut_detail),
        ],
    )?;
    assert!(!success, "hard-cut refusal must fail the result: {report}");
    assert!(
        report.contains("hard-cut admission refused: cut_frozen")
            && report.contains(cut_detail)
            && report.contains("- admission cut reason: `cut_frozen`"),
        "result must carry the hard-cut refusal reason, got: {report}"
    );
    assert!(
        !report.contains("candidate_current_unique_bounded —"),
        "result must not pair the success reason with the cut detail, got: {report}"
    );
    Ok(())
}

#[test]
fn manual_admission_producer_does_not_supply_pr_qualification() -> Result<(), String> {
    let scratch = Scratch::new()?;
    let action = root().join(".github/actions/pr-candidate-admission");
    let source = fs::read_to_string(action.join("action.yml")).map_err(|e| e.to_string())?;
    let (_, body) = source
        .split_once("      run: |\n")
        .ok_or("missing action shell")?;
    let shell = body
        .lines()
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let outputs = scratch.0.join("outputs");
    let script = scratch.0.join("action.sh");
    fs::write(&script, &shell).map_err(|e| e.to_string())?;
    let output = Command::new("bash")
        .arg("-e")
        .arg(&script)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").ok_or("PATH unavailable")?)
        .env("GITHUB_ACTION_PATH", action)
        .env("GITHUB_WORKSPACE", root())
        .env("GITHUB_OUTPUT", &outputs)
        .env("GITHUB_STEP_SUMMARY", scratch.0.join("summary"))
        .env("ADMISSION_MODE", "admit")
        .env("ADMISSION_PR_NUMBER", "")
        .output()
        .map_err(|e| e.to_string())?;
    assert!(output.status.success(), "{output:?}");
    let outputs = fs::read_to_string(outputs).map_err(|e| e.to_string())?;
    let status = outputs
        .lines()
        .find_map(|line| line.strip_prefix("status="))
        .ok_or("no producer status")?;
    assert_eq!(status, "not_applicable");
    assert!(outputs.contains("cut_reason=cut_non_pull_request_invocation"));
    assert!(
        !run_result(
            &workflow()?,
            "workflow_dispatch",
            "refs/heads/fix/5394",
            "success",
            status,
            &[]
        )?
        .0
    );
    assert!(
        run_result(
            &workflow()?,
            "workflow_dispatch",
            "refs/heads/main",
            "success",
            status,
            &[]
        )?
        .0
    );
    Ok(())
}

#[test]
fn event_identity_is_wired_from_github_not_candidate_inputs() -> Result<(), String> {
    let source = workflow()?;
    let step = result_step(&source)?;
    assert!(step.contains("          EVENT_NAME: ${{ github.event_name }}\n"));
    assert!(step.contains("          WORKFLOW_REF: ${{ github.ref }}\n"));
    assert!(!source.contains("    continue-on-error: true"));
    Ok(())
}

#[test]
fn hosted_result_requires_the_sharded_tests_and_self_hosted_does_not() -> Result<(), String> {
    let result = |overrides: &[(&str, &str)]| {
        run_result(
            &workflow()?,
            "pull_request",
            "refs/pull/10/merge",
            "success",
            "admitted",
            overrides,
        )
    };
    // Hosted route: green gates job alone is not a green result (#10507).
    assert!(result(&[("TARGET", "github")])?.0);
    for tests in ["failure", "cancelled", "skipped", ""] {
        let (success, report) = result(&[("TARGET", "github"), ("GITHUB_TESTS_RESULT", tests)])?;
        assert!(
            !success,
            "hosted tests `{tests}` must fail the result: {report}"
        );
        assert!(report.contains("sharded tests result"), "{report}");
    }
    // Self-hosted route ran its own nextest step; the skipped hosted shards do not matter.
    assert!(
        result(&[
            ("TARGET", "cx43"),
            ("CX43_RESULT", "success"),
            ("GITHUB_RESULT", "skipped"),
            ("GITHUB_TESTS_RESULT", "skipped"),
        ])?
        .0
    );
    // A tempfailed self-hosted job falls back to the hosted route, shards included.
    for (tests, expected) in [("success", true), ("failure", false), ("skipped", false)] {
        assert_eq!(
            result(&[
                ("TARGET", "cx43"),
                ("CX43_RESULT", "failure"),
                ("CX43_SCRATCH_STATUS", "tempfail"),
                ("GITHUB_RESULT", "success"),
                ("GITHUB_TESTS_RESULT", tests),
            ])?
            .0,
            expected,
            "fallback with sharded tests `{tests}`"
        );
    }
    Ok(())
}

#[test]
fn terminal_result_rejects_late_eligibility_changes_and_old_attempts() -> Result<(), String> {
    let source = workflow()?;
    let bad_prs = [
        LIVE_PR.replace("\"open\"", "\"closed\""),
        LIVE_PR.replace("\"draft\":false", "\"draft\":true"),
        LIVE_PR.replace(&"a".repeat(40), &"b".repeat(40)),
        LIVE_PR.replace("\"main\"", "\"other\""),
    ];
    let bad_frames = [
        "{}".to_string(),
        r#"{"errors":[{"message":"refused"}]}"#.to_string(),
        LIVE_GRAPHQL.replace("APPROVED", "CHANGES_REQUESTED"),
        LIVE_GRAPHQL.replace(r#""reviewDecision":"APPROVED","#, ""),
        LIVE_GRAPHQL.replace("\"nodes\":[]", "\"nodes\":[{\"isResolved\":false}]"),
        LIVE_GRAPHQL.replace("\"hasNextPage\":false", "\"hasNextPage\":null"),
    ];
    for event in ["pull_request", "merge_group"] {
        for (name, value) in bad_prs
            .iter()
            .map(|v| ("MOCK_PR", v.as_str()))
            .chain(bad_frames.iter().map(|v| ("MOCK_GRAPHQL", v.as_str())))
            .chain([
                ("EXPECTED_HEAD", ""),
                ("EXPECTED_REQUEST", "100-0"),
                ("GITHUB_RUN_ATTEMPT", "2"),
            ])
        {
            let (success, report) = run_result(
                &source,
                event,
                "refs/pull/10/merge",
                "success",
                "admitted",
                &[(name, value)],
            )?;
            assert!(
                !success,
                "late {name}={value} must refuse {event}: {report}"
            );
        }
        let changed = LIVE_PR.replace(&"a".repeat(40), &"b".repeat(40));
        assert!(
            !run_result(
                &source,
                event,
                "refs/pull/10/merge",
                "success",
                "admitted",
                &[("MOCK_PR_AFTER", &changed)]
            )?
            .0,
            "a head change during final review must refuse"
        );
    }
    let no_required_review = LIVE_GRAPHQL.replace("\"APPROVED\"", "null");
    assert!(
        run_result(
            &source,
            "pull_request",
            "pr",
            "success",
            "admitted",
            &[("MOCK_GRAPHQL", &no_required_review)]
        )?
        .0,
        "a present nullable decision remains valid"
    );
    let withdrawn = LIVE_PR.replace(r#"[{"name":"merge-ready"}]"#, "[]");
    assert!(
        !run_result(
            &source,
            "pull_request",
            "refs/pull/10/merge",
            "success",
            "admitted",
            &[("MOCK_PR", &withdrawn)]
        )?
        .0
    );
    for frame in [
        LIVE_GRAPHQL.replace("entry-10", "entry-11"),
        LIVE_GRAPHQL.replace(r#"{"id":"entry-10"}"#, "null"),
    ] {
        assert!(
            !run_result(
                &source,
                "merge_group",
                "queue",
                "success",
                "admitted",
                &[("MOCK_GRAPHQL", &frame)]
            )?
            .0,
            "withdrawn/replaced queue entry must refuse"
        );
    }
    let page = LIVE_GRAPHQL.replace(
        r#""hasNextPage":false,"endCursor":null"#,
        r#""hasNextPage":true,"endCursor":"next""#,
    );
    let blocked = LIVE_GRAPHQL.replace(r#""nodes":[]"#, r#""nodes":[{"isResolved":false}]"#);
    assert!(
        run_result(
            &source,
            "pull_request",
            "pr",
            "success",
            "admitted",
            &[("MOCK_GRAPHQL", &page), ("MOCK_GRAPHQL_NEXT", LIVE_GRAPHQL)]
        )?
        .0
    );
    assert!(
        !run_result(
            &source,
            "pull_request",
            "pr",
            "success",
            "admitted",
            &[("MOCK_GRAPHQL", &page), ("MOCK_GRAPHQL_NEXT", &blocked)]
        )?
        .0
    );
    Ok(())
}

#[test]
fn refused_eligibility_names_the_stage_that_refused() -> Result<(), String> {
    let source = workflow()?;
    let closed = LIVE_PR.replace("\"open\"", "\"closed\"");
    let unresolved = LIVE_GRAPHQL.replace("\"nodes\":[]", "\"nodes\":[{\"isResolved\":false}]");
    // This invocation is attempt 1; a run read showing attempt 2 is superseded.
    let advanced = run_entry("pull_request", 100, 7, 2);
    let advanced_run = advanced.to_string();
    let advanced_page = runs_page(1, &[advanced]);
    for (overrides, stage) in [
        (
            vec![("MOCK_PR_AFTER", closed.as_str())],
            "pull_request_state_after_threads",
        ),
        (
            vec![
                ("MOCK_RUN", advanced_run.as_str()),
                ("MOCK_RUNS_1", advanced_page.as_str()),
            ],
            "same_head_succession",
        ),
        (vec![("MOCK_PR", closed.as_str())], "pull_request_state"),
        (
            vec![("MOCK_GRAPHQL", unresolved.as_str())],
            "review_decision_and_threads",
        ),
        (vec![("EXPECTED_REQUEST", "100-0")], "admission_binding"),
    ] {
        let (success, report) = run_result(
            &source,
            "pull_request",
            "refs/pull/10/merge",
            "success",
            "admitted",
            &overrides,
        )?;
        assert!(!success, "{stage} must still refuse");
        assert!(
            report.contains(&format!("eligibility recheck refused at stage {stage} ")),
            "the refused stage must be named: {stage}\n{report}"
        );
    }
    // A re-run (attempt 2) of a request minted at attempt 1 is explained, not
    // just refused, and the first attempt gets no such hint.
    let (success, report) = run_result(
        &source,
        "pull_request",
        "refs/pull/10/merge",
        "success",
        "admitted",
        &[("GITHUB_RUN_ATTEMPT", "2")],
    )?;
    assert!(!success, "a re-run must still refuse");
    assert!(
        report.contains("this is attempt 2 but the admitted request was minted by attempt 1")
            && report.contains("re-add merge-ready"),
        "{report}"
    );
    // A queued commit cannot be re-qualified by a label, so the merge-group
    // recovery text must not send the operator there.
    let (success, report) = run_result(
        &source,
        "merge_group",
        "queue",
        "success",
        "admitted",
        &[("GITHUB_RUN_ATTEMPT", "2")],
    )?;
    assert!(!success, "a merge-group re-run must still refuse");
    assert!(
        report.contains("Re-queue the pull request") && !report.contains("re-add merge-ready"),
        "{report}"
    );
    let (success, report) = run_result(
        &source,
        "pull_request",
        "refs/pull/10/merge",
        "success",
        "admitted",
        &[],
    )?;
    assert!(success, "a live request is unchanged");
    assert!(!report.contains("eligibility recheck refused"));
    assert!(!report.contains("minted by attempt 1"));
    Ok(())
}

#[test]
fn removed_terminal_guard_exposes_the_same_closed_pr_false_green() -> Result<(), String> {
    let source = workflow()?;
    let guard = "          current_eligibility\n";
    assert_eq!(source.matches(guard).count(), 1);
    let removed = source.replace(guard, "          true\n");
    let closed = LIVE_PR.replace("\"open\"", "\"closed\"");
    assert!(
        run_result(
            &removed,
            "pull_request",
            "pr",
            "success",
            "admitted",
            &[("MOCK_PR", &closed)]
        )?
        .0,
        "removing the actual guard must expose the unchanged negative"
    );
    assert!(source.contains("ELIGIBILITY_RESULT: ${{ steps.eligibility.outcome }}"));
    for output in ["pr", "head", "queue_entry", "request"] {
        assert!(source.contains(&format!(
            "{output}: ${{{{ steps.entry.outputs.{output} }}}}"
        )));
    }
    Ok(())
}

/// Runs eligibility and the Result for `event` with a custom own-run read and
/// run-list pages, returning whether the whole Result passed.
fn with_runs(event: &str, own: &str, pages: &[String]) -> Result<bool, String> {
    let mut overrides = vec![("MOCK_RUN", own)];
    if let Some(page) = pages.first() {
        overrides.push(("MOCK_RUNS_1", page.as_str()));
    }
    if let Some(page) = pages.get(1) {
        overrides.push(("MOCK_RUNS_2", page.as_str()));
    }
    run_result(
        &workflow()?,
        event,
        "refs/pull/10/merge",
        "success",
        "admitted",
        &overrides,
    )
    .map(|(success, _)| success)
}

#[test]
fn current_invocation_is_accepted_and_older_or_unrelated_runs_do_not_revoke_it()
-> Result<(), String> {
    for event in ["pull_request", "merge_group"] {
        let own = run_entry(event, 100, 7, 1);
        let mut other_head = run_entry(event, 101, 9, 1);
        other_head["head_sha"] = "b".repeat(40).into();
        let mut other_event = run_entry(event, 102, 10, 1);
        other_event["event"] = if event == "merge_group" {
            "pull_request"
        } else {
            "merge_group"
        }
        .into();
        let mut other_workflow = run_entry(event, 103, 11, 1);
        other_workflow["workflow_id"] = 8.into();
        let mut other_repository = run_entry(event, 104, 12, 1);
        other_repository["repository"]["full_name"] = "someone/else".into();
        // An ordinary label event is a newer run on the same head, but it is a
        // different request kind (`Label metadata <id>`) and must revoke nothing.
        let mut label_metadata = run_entry(event, 105, 13, 1);
        label_metadata["display_title"] = "Label metadata 105".into();
        let older = run_entry(event, 90, 3, 5);
        let cases = [
            ("alone", vec![own.clone()]),
            ("older run on the same head", vec![older, own.clone()]),
            ("newer run on another head", vec![own.clone(), other_head]),
            ("newer run of another event", vec![own.clone(), other_event]),
            (
                "newer run of another workflow",
                vec![own.clone(), other_workflow],
            ),
            (
                "newer run of another repository",
                vec![own.clone(), other_repository],
            ),
            (
                "newer label-metadata run",
                vec![own.clone(), label_metadata],
            ),
        ];
        for (name, runs) in cases {
            let page = runs_page(runs.len() as u64, &runs);
            assert!(
                with_runs(event, &own.to_string(), &[page])?,
                "{event}: {name} must not revoke the current invocation"
            );
        }
    }
    Ok(())
}

#[test]
fn a_newer_run_or_attempt_on_the_same_head_revokes_the_superseded_invocation() -> Result<(), String>
{
    for event in ["pull_request", "merge_group"] {
        let own = run_entry(event, 100, 7, 1);
        // Newer runs revoke regardless of their own outcome or attempt count.
        for (name, newer) in [
            ("newer run", run_entry(event, 101, 8, 1)),
            ("newer run, later attempt", run_entry(event, 101, 8, 3)),
            ("much newer run", run_entry(event, 250, 99, 1)),
            (
                "same number, other id (ambiguous order)",
                run_entry(event, 101, 7, 1),
            ),
        ] {
            let mut failed = newer.clone();
            failed["conclusion"] = "failure".into();
            for candidate in [newer, failed] {
                let runs = [own.clone(), candidate];
                assert!(
                    !with_runs(event, &own.to_string(), &[runs_page(2, &runs)])?,
                    "{event}: {name} must revoke the superseded run"
                );
            }
        }
        // A newer attempt of this very run: GitHub reports only the latest
        // attempt, so both the run read and the listing show attempt 2 while
        // this invocation is attempt 1.
        let advanced = run_entry(event, 100, 7, 2);
        assert!(
            !with_runs(
                event,
                &advanced.to_string(),
                &[runs_page(1, std::slice::from_ref(&advanced))]
            )?,
            "{event}: a newer attempt must revoke the superseded attempt"
        );
        // Run read is current but the listing already shows the advanced attempt.
        assert!(
            !with_runs(event, &own.to_string(), &[runs_page(1, &[advanced])])?,
            "{event}: an attempt that advances between reads must refuse"
        );
    }
    Ok(())
}

#[test]
fn succession_reads_follow_every_page_and_fail_closed() -> Result<(), String> {
    for event in ["pull_request", "merge_group"] {
        let own = run_entry(event, 100, 7, 1);
        let own_text = own.to_string();
        let filler = |first: u64, count: u64| -> Vec<serde_json::Value> {
            (0..count)
                .map(|i| run_entry(event, first + i, 1, 1))
                .collect()
        };
        // A clean multi-page inventory is accepted.
        let mut page_one = filler(1000, 99);
        page_one.push(own.clone());
        let page_two = filler(2000, 5);
        assert!(
            with_runs(
                event,
                &own_text,
                &[runs_page(104, &page_one), runs_page(104, &page_two)]
            )?,
            "{event}: complete clean pagination"
        );
        // A newer request that is only on a later page still revokes.
        let mut late = page_two.clone();
        late.push(run_entry(event, 300, 50, 1));
        assert!(
            !with_runs(
                event,
                &own_text,
                &[runs_page(105, &page_one), runs_page(105, &late)]
            )?,
            "{event}: a newer run on page two revokes"
        );
        let single = runs_page(1, std::slice::from_ref(&own));
        let malformed_newer = {
            let mut bad = run_entry(event, 101, 8, 1);
            bad["display_title"] = serde_json::Value::Null;
            bad
        };
        let missing_number = {
            let mut bad = run_entry(event, 101, 8, 1);
            bad.as_object_mut().ok_or("object")?.remove("run_number");
            bad
        };
        let mut wrong_title = own.clone();
        wrong_title["display_title"] = "Label metadata 100".into();
        let mut wrong_head = own.clone();
        wrong_head["head_sha"] = "b".repeat(40).into();
        let mut wrong_event = own.clone();
        wrong_event["event"] = "workflow_dispatch".into();
        let mut no_attempt = own.clone();
        no_attempt
            .as_object_mut()
            .ok_or("object")?
            .remove("run_attempt");
        let mut lower_attempt = own.clone();
        lower_attempt["run_attempt"] = 0.into();
        let mut other_repository = own.clone();
        other_repository["repository"]["full_name"] = "someone/else".into();
        let cases: Vec<(&str, String, Vec<String>)> = vec![
            ("own run read fails", "fail".into(), vec![single.clone()]),
            ("own run read is not JSON", "{".into(), vec![single.clone()]),
            ("own run read is empty", "".into(), vec![single.clone()]),
            (
                "own run has a foreign title",
                wrong_title.to_string(),
                vec![single.clone()],
            ),
            (
                "own run has a foreign head",
                wrong_head.to_string(),
                vec![single.clone()],
            ),
            (
                "own run has a foreign event",
                wrong_event.to_string(),
                vec![single.clone()],
            ),
            (
                "own run lacks an attempt",
                no_attempt.to_string(),
                vec![single.clone()],
            ),
            (
                "own run attempt is lower than the invocation",
                lower_attempt.to_string(),
                vec![single.clone()],
            ),
            (
                "own run is another repository",
                other_repository.to_string(),
                vec![single.clone()],
            ),
            ("listing read fails", own_text.clone(), vec!["fail".into()]),
            ("listing is not JSON", own_text.clone(), vec!["{".into()]),
            (
                "listing has no total_count",
                own_text.clone(),
                vec![serde_json::json!({"workflow_runs": [own]}).to_string()],
            ),
            (
                "listing has no workflow_runs",
                own_text.clone(),
                vec![serde_json::json!({"total_count": 1}).to_string()],
            ),
            (
                "listing omits this run",
                own_text.clone(),
                vec![runs_page(0, &[])],
            ),
            (
                "listing is an inconsistent count",
                own_text.clone(),
                vec![runs_page(2, std::slice::from_ref(&own))],
            ),
            (
                "later page read fails",
                own_text.clone(),
                vec![runs_page(104, &page_one), "fail".into()],
            ),
            (
                "later page is empty before the total",
                own_text.clone(),
                vec![runs_page(104, &page_one), runs_page(104, &[])],
            ),
            (
                "duplicate run ids across pages",
                own_text.clone(),
                vec![
                    runs_page(101, &page_one),
                    runs_page(101, std::slice::from_ref(&own)),
                ],
            ),
            (
                "newer run lacks a title",
                own_text.clone(),
                vec![runs_page(2, &[own.clone(), malformed_newer])],
            ),
            (
                "newer run lacks a run number",
                own_text.clone(),
                vec![runs_page(2, &[own.clone(), missing_number])],
            ),
        ];
        for (name, own_read, pages) in cases {
            assert!(
                !with_runs(event, &own_read, &pages)?,
                "{event}: {name} must fail closed"
            );
        }
        // Truncation: the inventory claims more runs than the bounded read
        // will page through.
        let hundred = filler(5000, 100);
        let mut first = hundred.clone();
        first[0] = own.clone();
        assert!(
            !with_runs(
                event,
                &own_text,
                &[runs_page(5000, &first), runs_page(5000, &hundred)]
            )?,
            "{event}: a truncated inventory must refuse"
        );
    }
    Ok(())
}

#[test]
fn succession_reads_are_pinned_read_only_gets_of_the_same_workflow_head_and_event()
-> Result<(), String> {
    // The step must only GET its own run and the same-workflow listing for the
    // same head and event; a mutation or a broader listing would be a new
    // authority.
    let source = workflow()?;
    let (_, job) = source
        .split_once("      - name: Recheck initially admitted eligibility\n")
        .ok_or("missing eligibility step")?;
    let (step, _) = job
        .split_once("      - name: Summarize routed result\n")
        .ok_or("missing summary")?;
    for needle in [
        "gh api \"repos/$REPOSITORY/actions/runs/$GITHUB_RUN_ID\"",
        "gh api \"repos/$REPOSITORY/actions/workflows/$wf/runs?head_sha=$run_head&event=$EVENT_NAME&per_page=100&page=$spage\"",
    ] {
        assert!(step.contains(needle), "missing read-only call: {needle}");
    }
    for forbidden in [
        "-X ",
        "--method",
        "-f ",
        "-F ",
        "--input",
        "gh run ",
        "gh workflow",
    ] {
        let mutated = step
            .lines()
            .filter(|line| line.contains("actions/") && line.contains(forbidden))
            .count();
        assert_eq!(mutated, 0, "Actions reads must not use {forbidden}");
    }
    assert!(!step.contains("permissions:"));
    Ok(())
}

#[test]
fn removed_succession_guard_exposes_the_superseded_false_green() -> Result<(), String> {
    let source = workflow()?;
    let guard = "            read_succession\n          }\n";
    assert_eq!(source.matches(guard).count(), 1);
    let removed = source.replace(guard, "            true\n          }\n");
    for event in ["pull_request", "merge_group"] {
        let own = run_entry(event, 100, 7, 1);
        let newer = run_entry(event, 101, 8, 1);
        let page = runs_page(2, &[own.clone(), newer]);
        let overrides = [("MOCK_RUN", own.to_string()), ("MOCK_RUNS_1", page)];
        let overrides = overrides
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>();
        let attempt = run_entry(event, 100, 7, 2);
        let advanced = [
            ("MOCK_RUN", attempt.to_string()),
            ("MOCK_RUNS_1", runs_page(1, &[attempt])),
        ];
        let advanced = advanced
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>();
        for (name, case) in [("newer run", overrides), ("newer attempt", advanced)] {
            let run = |wf: &str| {
                run_result(
                    wf,
                    event,
                    "refs/pull/10/merge",
                    "success",
                    "admitted",
                    &case,
                )
                .map(|(success, _)| success)
            };
            assert!(!run(&source)?, "{event}: {name} refuses with the guard");
            assert!(
                run(&removed)?,
                "{event}: {name} passes without the guard, so the control discriminates"
            );
        }
    }
    // The decision reaches the real consumer: the Result step reads the
    // eligibility outcome, and a failed outcome fails the Result.
    assert!(source.contains("ELIGIBILITY_RESULT: ${{ steps.eligibility.outcome }}"));
    assert!(source.contains("GROUP_HEAD_SHA: ${{ github.event.merge_group.head_sha }}"));
    Ok(())
}
