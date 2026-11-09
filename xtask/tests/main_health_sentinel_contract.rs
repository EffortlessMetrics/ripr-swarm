//! Pin the scheduled main-health sentinel at the workflow boundary (#10991).
//!
//! The sentinel is advisory-detect only: it must stay a scheduled probe that
//! never becomes a required check, never touches branch protection or
//! rulesets, never dispatches another workflow, and never claims success from
//! missing or empty evidence. Its red record must be durable (step summary
//! plus the `main-red` tracking issue) and self-healing (a green probe closes
//! the tracker). Breaking any of those invariants should fail here, not
//! silently turn the sentinel into a gate or a false-confidence surface.

use std::fs;
use std::path::Path;

const WORKFLOW: &str = ".github/workflows/main-health-sentinel.yml";
const ALLOWLIST: &str = "policy/workflow_allowlist.txt";
const SMOKE_SELECTOR: &str = "version_is_exact_and_precedes_help_or_output_flags";
const TRACKER_MARKER: &str = "main-health-sentinel-tracker";

/// Executable (non-comment) fragments that would let the sentinel gate,
/// authorize, or mutate anything beyond its own advisory run.
const FORBIDDEN_FRAGMENTS: &[&str] = &[
    "actions: write",
    "gh workflow run",
    "required_status_checks",
    "required_pull_request_reviews",
    "/protection",
    "/rules/",
    "create-pull-request",
    "/rerun",
];

fn repo_file(path: &str) -> Result<String, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    fs::read_to_string(root.join(path)).map_err(|error| error.to_string())
}

/// Executable lines only: comment prose may explain the contract without
/// satisfying it.
fn executable_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect()
}

fn require(lines: &[&str], needle: &str, why: &str) -> Result<(), String> {
    if lines.iter().any(|line| line.contains(needle)) {
        return Ok(());
    }
    Err(format!("main-health sentinel is missing `{needle}`: {why}"))
}

fn forbid(lines: &[&str], needle: &str, why: &str) -> Result<(), String> {
    if !lines.iter().any(|line| line.contains(needle)) {
        return Ok(());
    }
    Err(format!(
        "main-health sentinel must not contain `{needle}`: {why}"
    ))
}

#[test]
fn sentinel_is_scheduled_and_manually_runnable() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "schedule:",
        "the probe exists only if it runs on a timer",
    )?;
    require(
        &lines,
        "cron: \"9,39 * * * *\"",
        "the twice-hourly cadence is the documented detection bound",
    )?;
    require(
        &lines,
        "workflow_dispatch:",
        "a manual fire is the only way to exercise a scheduled-only workflow on demand",
    )
}

#[test]
fn sentinel_serializes_overlapping_fires() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "group: main-health-sentinel-${{ github.repository }}",
        "one probe at a time, so tracker reconciliation cannot race itself",
    )?;
    require(
        &lines,
        "cancel-in-progress: false",
        "an in-progress probe must finish writing its summary and tracker state",
    )
}

#[test]
fn sentinel_is_bounded_and_repository_scoped() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "if: github.repository == 'EffortlessMetrics/ripr-swarm' && github.ref == 'refs/heads/main'",
        "a fork running the workflow must not open issues on the upstream tracker, \
         and a dispatch from a non-main ref must not judge that ref as main health",
    )?;
    require(
        &lines,
        "timeout-minutes: 30",
        "a hung probe is a red signal with an end, not a stuck run",
    )
}

#[test]
fn sentinel_stays_advisory_detect_only() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    for fragment in FORBIDDEN_FRAGMENTS {
        forbid(
            &lines,
            fragment,
            "the sentinel detects; it never gates or dispatches",
        )?;
    }
    // The only write authority is the tracking issue. Workflow-level
    // permissions stay read-only so no secret-holding job inherits a broad
    // token.
    forbid(
        &lines,
        "contents: write",
        "the sentinel never pushes commits or opens PRs",
    )?;
    require(
        &lines,
        "issues: write",
        "the main-red tracker is the durable record",
    )?;
    require(
        &lines,
        "GITHUB_STEP_SUMMARY",
        "the per-probe summary table is the visible audit trail",
    )
}

#[test]
fn sentinel_probe_is_bounded_and_executes_something() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "cargo check --locked -p ripr",
        "the compile-health half of the probe",
    )?;
    // The full test command is pinned, not just the selector name: the name
    // also appears in the summary row, so a name-only check would pass even
    // if the cargo test invocation were removed.
    require(
        &lines,
        "cargo test --locked -p ripr --test cli_smoke",
        "the smoke half actually invokes cargo test on the built binary",
    )?;
    require(
        &lines,
        &format!("{SMOKE_SELECTOR} -- --exact"),
        "the smoke half runs exactly one focused test, nothing broader",
    )?;
    // Evidence discipline: native exit statuses are captured next to the logs
    // so a failure inside a pipe cannot vanish, a hung step classifies as the
    // timeout status instead of skipping, and a zero-subject run fails.
    require(
        &lines,
        "PIPESTATUS",
        "a failure inside a tee pipe must survive",
    )?;
    require(
        &lines,
        "2>/dev/null || echo 124",
        "a probe terminated at its step timeout has no status file and must read as red (124), not as success",
    )?;
    require(
        &lines,
        "^test result:",
        "the judge reads the native test-result line, not cargo's success",
    )?;
    require(
        &lines,
        "executed zero tests",
        "a probe that ran nothing proves nothing and must fail",
    )
}

#[test]
fn sentinel_tracker_is_durable_and_self_healing() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "main-red: scheduled main-health sentinel found a broken main",
        "the tracker title is the findable red record",
    )?;
    require(
        &lines,
        TRACKER_MARKER,
        "the marker marks sentinel provenance in the tracker body and comments",
    )?;
    require(
        &lines,
        "select(.title == \\\"$title\\\")",
        "tracker creation deduplicates by exact title among open issues, not by the body marker",
    )?;
    require(
        &lines,
        "-f state=closed",
        "a green probe closes the tracker instead of leaving a stale red record",
    )?;
    require(
        &lines,
        "steps.judge.outputs.verdict != ''",
        "only a judge that actually classified main may touch the tracker: a failure \
         before the judge (checkout, toolchain) is a visible red run, not a main verdict",
    )
}

#[test]
fn sentinel_run_block_budget_is_registered() -> Result<(), String> {
    let allowlist = repo_file(ALLOWLIST)?;
    if allowlist
        .lines()
        .any(|line| line.starts_with(".github/workflows/main-health-sentinel.yml|"))
    {
        return Ok(());
    }
    Err(
        "check-workflows requires a run-block budget row for every workflow; \
         the sentinel's row in policy/workflow_allowlist.txt is part of its contract"
            .to_string(),
    )
}

/// Test-level breaks pass the bounded probe (#13793, #13639, #13783), so the
/// sentinel also judges the latest completed, non-cancelled main-push run of
/// each push gate. It reads results only: it never re-runs, dispatches or
/// rebuilds them, and a failed gate run must turn the verdict red.
#[test]
fn sentinel_judges_the_latest_completed_main_push_gates() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "actions: read",
        "reading workflow-run results needs only the read scope",
    )?;
    require(
        &lines,
        "for wf in routed-rust.yml ci.yml; do",
        "the required Rust gate and the CI lane (Rust-only features, editor e2e) are both judged",
    )?;
    require(
        &lines,
        "-f branch=main -f event=push -f status=completed",
        "only finished main-push runs are evidence about main",
    )?;
    require(
        &lines,
        "select(.conclusion != \"cancelled\" and .conclusion != \"skipped\")",
        "a superseded run says nothing about main; skip it and read the latest finished one",
    )?;
    require(
        &lines,
        "echo 1 > \"$d/gates.red\"",
        "a failed latest gate run must raise the red flag, not only the summary",
    )?;
    require(
        &lines,
        "gates_red=\"$(cat \"$d/gates.red\" 2>/dev/null || echo 0)\"",
        "the judge must read the gate red flag into its verdict",
    )?;
    require(
        &lines,
        "grep -qxF \"$fingerprint\"",
        "a red tracker gets one comment per new piece of evidence, not one per fire",
    )?;
    let read_step = source
        .split("- name: Read the latest completed main push gates")
        .nth(1)
        .and_then(|rest| rest.split("- name:").next())
        .ok_or("the gate read step must exist")?;
    if !read_step.contains("continue-on-error: true") {
        return Err("a hung or failed gate read must not skip the probe judge and tracker".into());
    }
    let probe_step = source
        .split("- name: Probe main build and one focused binary smoke test")
        .nth(1)
        .and_then(|rest| rest.split("- name:").next())
        .ok_or("the probe step must exist")?;
    if !probe_step.contains("continue-on-error: true") {
        return Err("a probe terminated at its timeout must still reach the judge".into());
    }
    require(
        &lines,
        "0:success|0:failure|0:timed_out|0:action_required|0:startup_failure) ;;",
        "a failed read or an unexpected conclusion must become unknown, not red",
    )?;
    require(
        &lines,
        "[ \"$(grep -c . \"$d/gates.tsv\" || true)\" -ge 2 ] || echo 1 > \"$d/gates.unknown\"",
        "a missing gate row must mark the gates unknown",
    )?;
    require(
        &lines,
        "unknown) echo 1 > \"$d/gates.unknown\" ;;",
        "an unknown gate row must mark the gates unknown",
    )?;
    require(
        &lines,
        "run=\"$(timeout 60 gh api -X GET",
        "each gate read is bounded so a hung API call becomes an unknown row",
    )?;
    require(
        &lines,
        "elif [ -n \"$number\" ] && [ \"$(cat \"$RUNNER_TEMP/main-health-sentinel/gates.unknown\" 2>/dev/null || echo 1)\" = 0 ]; then",
        "an unknown gate is not recovery and must not close an open tracker",
    )
}
