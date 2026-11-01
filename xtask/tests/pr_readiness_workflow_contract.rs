//! Pin the protected PR qualification event at the workflow boundary.
//!
//! Draft, Ready, push and ordinary-label activity must not create the required
//! `Ripr Rust Small Result` check. An explicit `merge-ready` label event is the
//! only pull-request admission event; main and explicit manual authorities remain separate. The PR
//! staleness watchdog is alert-only: it must never qualify a Ready head by
//! dispatching the protected workflow on its branch (#4986).

use std::fs;
use std::path::Path;

const WORKFLOW: &str = ".github/workflows/routed-rust.yml";
const EXPECTED_EVENT_DECLARATIONS: &[&str] = &[
    "on:",
    "  pull_request:",
    "    types: [labeled]",
    "  merge_group:",
    "    types: [checks_requested]",
    "  push:",
    "    branches: [main, master]",
    "  workflow_dispatch:",
];
const EXPECTED_CONCURRENCY_GROUP: &str = "  group: ${{ github.event_name == 'pull_request' && github.event.label.name != 'merge-ready' && format('{0}-{1}-nonrequest-{2}', github.workflow, github.event.pull_request.number, github.run_id) || format('{0}-{1}-{2}', github.workflow, github.event.pull_request.number || github.ref, github.event_name) }}";
const EXPECTED_CANCEL_IN_PROGRESS: &str =
    "  cancel-in-progress: ${{ github.event_name == 'pull_request' }}";
const REQUIRED_CONTEXT: &str = "Ripr Rust Small Result";
/// The only name expression that can post the required context: main/manual
/// runs and an explicit merge-ready request. Every other label event posts a
/// different name; a skipped job would report success for a static name.
const EXPECTED_RESULT_NAME: &str = "${{ (github.repository == 'EffortlessMetrics/ripr-swarm' && (github.event_name != 'pull_request' || (!github.event.pull_request.draft && github.event.label.name == 'merge-ready'))) && 'Ripr Rust Small Result' || 'Swarm Route Nonrequest' }}";
const WATCHDOG: &str = ".github/workflows/pr-staleness-watchdog.yml";
/// Executable (non-comment) fragments that would let the watchdog start or
/// authorize a workflow run.
const WATCHDOG_DISPATCH_FRAGMENTS: &[&str] = &["actions: write", "gh workflow run", "/dispatches"];

/// Read a workflow from the repository root above the xtask package.
fn repo_file(path: &str) -> Result<String, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    fs::read_to_string(root.join(path)).map_err(|error| error.to_string())
}

/// Read the candidate workflow from the repository root above the xtask package.
fn workflow_source() -> Result<String, String> {
    repo_file(WORKFLOW)
}

/// Return every dispatch-capable fragment present on a non-comment line.
fn watchdog_dispatch_fragments(source: &str) -> Vec<&'static str> {
    let executable: Vec<&str> = source
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect();
    WATCHDOG_DISPATCH_FRAGMENTS
        .iter()
        .copied()
        .filter(|fragment| executable.iter().any(|line| line.contains(fragment)))
        .collect()
}

/// Return the exact event declaration lines, excluding comments and blank lines.
fn event_declarations(source: &str) -> Result<Vec<&str>, String> {
    let events = source
        .split_once("\npermissions:\n")
        .map(|(events, _)| events)
        .ok_or_else(|| "workflow keeps permissions after event declarations".to_string())?;

    Ok(events
        .lines()
        .skip_while(|line| *line != "on:")
        .filter_map(|line| {
            let line = line.trim_end();
            if line.is_empty() || line.trim_start().starts_with('#') {
                None
            } else {
                Some(line)
            }
        })
        .collect())
}

/// Read a direct static job name from the `jobs` mapping.
///
/// Direct job keys have exactly two leading spaces and direct job fields have
/// four. More deeply indented block-scalar text therefore cannot impersonate a
/// sibling job or its `name` field.
fn direct_job_name<'a>(source: &'a str, job: &str) -> Option<&'a str> {
    let target = format!("  {job}:");
    let mut in_jobs = false;
    let mut in_target = false;

    for raw_line in source.lines() {
        let line = raw_line.trim_end();
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if !line.is_empty() && !line.starts_with(' ') {
            break;
        }

        let direct_job = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.ends_with(':')
            && !line.trim_start().starts_with('#');
        if direct_job {
            if in_target {
                return None;
            }
            in_target = line == target;
            continue;
        }

        if in_target && let Some(name) = line.strip_prefix("    name: ") {
            return Some(name.trim());
        }
    }

    None
}

/// Read the direct static name emitted by the required result job.
fn terminal_context(source: &str) -> Option<&str> {
    direct_job_name(source, "result")
}

#[test]
fn required_pr_context_is_withheld_until_merge_ready() -> Result<(), String> {
    let source = workflow_source()?;

    assert_eq!(
        event_declarations(&source)?,
        EXPECTED_EVENT_DECLARATIONS,
        "protected workflow must expose only the label-request PR, main push, and manual authorities",
    );
    assert!(source.contains(EXPECTED_CONCURRENCY_GROUP));
    assert!(source.contains(EXPECTED_CANCEL_IN_PROGRESS));
    assert_eq!(terminal_context(&source), Some(EXPECTED_RESULT_NAME));
    assert!(EXPECTED_RESULT_NAME.contains(REQUIRED_CONTEXT));
    assert!(!source.contains("Ripr Rust Small Ignored Label Event"));
    for job in ["route", "detect-docs-only", "admission"] {
        let guard = "github.event_name != 'pull_request' || (!github.event.pull_request.draft && github.event.label.name == 'merge-ready')";
        let start = source
            .find(&format!("  {job}:\n"))
            .ok_or_else(|| format!("missing {job}"))?;
        let rest = &source[start..];
        let block = rest.lines().skip(1).take_while(|line| {
            !(line.starts_with("  ") && !line.starts_with("   ") && line.ends_with(':'))
        });
        assert!(
            block.into_iter().any(|line| line.contains(guard)),
            "{job} must exclude draft labels"
        );
    }
    Ok(())
}

#[test]
fn contract_rejects_draft_or_mutation_triggers() -> Result<(), String> {
    let source = workflow_source()?;
    for event in ["synchronize", "ready_for_review", "converted_to_draft"] {
        let changed = source.replace("types: [labeled]", &format!("types: [labeled, {event}]"));
        assert_ne!(changed, source, "trigger mutation must engage");
        assert_ne!(
            event_declarations(&changed)?,
            EXPECTED_EVENT_DECLARATIONS,
            "{event} must violate the protected event law"
        );
    }
    Ok(())
}

#[test]
fn contract_rejects_disabled_ready_cancellation() -> Result<(), String> {
    let source = workflow_source()?;
    let changed = source.replace(
        "  cancel-in-progress: ${{ github.event_name == 'pull_request' }}",
        "  cancel-in-progress: false",
    );
    assert_ne!(changed, source, "cancellation mutation must engage");
    assert!(
        !changed.contains(EXPECTED_CANCEL_IN_PROGRESS),
        "mutation must remove the pinned Ready-run cancellation expression"
    );
    Ok(())
}

#[test]
fn contract_rejects_a_noncanonical_terminal_context() -> Result<(), String> {
    let source = workflow_source()?;
    // A static required name would be skipped-as-success on ordinary labels.
    let changed = source.replace(
        &format!("  result:\n    # Static `Ripr Rust Small Result` only for main/manual runs and an explicit\n    # merge-ready request. Any other pull_request label event is a nonrequest\n    # under a different name so the required context is never satisfied by it.\n    name: {EXPECTED_RESULT_NAME}"),
        "  result:\n    name: Ripr Rust Small Result",
    );
    assert_ne!(changed, source, "result-name mutation must engage");
    assert_ne!(
        terminal_context(&changed),
        Some(EXPECTED_RESULT_NAME),
        "a static result name must violate the required-context contract",
    );
    let widened = source.replace(
        "label.name == 'merge-ready'))) &&",
        "label.name != ''))) &&",
    );
    assert_ne!(widened, source, "request-label mutation must engage");
    assert_ne!(terminal_context(&widened), Some(EXPECTED_RESULT_NAME));
    Ok(())
}

#[test]
fn block_scalar_decoy_cannot_hide_a_renamed_result_job() {
    let source = "jobs:\n  route:\n    run: |\n      result:\n        name: Ripr Rust Small Result\n  result:\n    name: Ripr Rust Small Draft Result\n";
    assert_eq!(
        terminal_context(source),
        Some("Ripr Rust Small Draft Result")
    );
}

#[test]
fn staleness_watchdog_reports_without_dispatching() -> Result<(), String> {
    let source = repo_file(WATCHDOG)?;
    assert!(
        source.contains("REQUIRED_CHECK: Ripr Rust Small Result"),
        "watchdog must still sweep for the exact required context",
    );
    assert_eq!(
        watchdog_dispatch_fragments(&source),
        Vec::<&str>::new(),
        "watchdog must be alert-only: a Ready head returns to Draft -> Ready",
    );
    Ok(())
}

/// The required check run appears only when `result` starts, so a head whose
/// Ready-triggered run is still queued or running has no required check yet.
/// Calling that head dark tells the author to re-toggle Draft -> Ready, which
/// cancels the real run (`cancel-in-progress` for pull_request).
const WATCHDOG_IN_FLIGHT_QUERY: &str =
    "actions/workflows/routed-rust.yml/runs?event=pull_request&head_sha=$head_sha";

fn watchdog_reports_in_flight_before_dark(source: &str) -> bool {
    let Some(query) = source.find(WATCHDOG_IN_FLIGHT_QUERY) else {
        return false;
    };
    let Some(dark) = source.find("dark=$((dark + 1))") else {
        return false;
    };
    source.contains("select(.status != \"completed\")")
        && source.contains("  actions: read\n")
        && query < dark
}

#[test]
fn staleness_watchdog_does_not_call_an_in_flight_run_dark() -> Result<(), String> {
    let source = repo_file(WATCHDOG)?;
    assert!(
        watchdog_reports_in_flight_before_dark(&source),
        "watchdog must look for a queued or in-progress Ready-triggered run before reporting DARK",
    );
    Ok(())
}

#[test]
fn contract_rejects_a_watchdog_without_the_in_flight_check() -> Result<(), String> {
    let source = repo_file(WATCHDOG)?;
    let changed = source.replace(WATCHDOG_IN_FLIGHT_QUERY, "actions/runs?head_sha=$head_sha");
    assert_ne!(changed, source, "in-flight mutation must engage");
    assert!(
        !watchdog_reports_in_flight_before_dark(&changed),
        "a watchdog that skips the in-flight run check must violate the contract",
    );
    Ok(())
}

/// A conflicting (`dirty`) PR silently gets zero `pull_request` runs:
/// `routed-rust.yml` evaluates from the merge ref, which does not exist while
/// the PR conflicts (#7079). The report must carry each PR's `mergeable_state`
/// so the conflicted dark mode ("merge main first") is distinguishable from a
/// dropped delivery ("re-toggle Draft -> Ready").
const WATCHDOG_MERGEABLE_STATE_QUERY: &str = "repos/$REPOSITORY/pulls/$number";
const WATCHDOG_MERGEABLE_STATE_COLUMN: &str = "| PR | head | mergeable_state | disposition |";

fn watchdog_reports_mergeable_state(source: &str) -> bool {
    source.contains(WATCHDOG_MERGEABLE_STATE_QUERY)
        && source.contains(".mergeable_state")
        && source.contains(WATCHDOG_MERGEABLE_STATE_COLUMN)
        && source.contains("DARK (conflicted)")
}

#[test]
fn staleness_watchdog_reports_mergeable_state() -> Result<(), String> {
    let source = repo_file(WATCHDOG)?;
    assert!(
        watchdog_reports_mergeable_state(&source),
        "watchdog must report each PR's mergeable_state so a conflicted dark head is distinguishable from a dropped delivery",
    );
    Ok(())
}

#[test]
fn contract_rejects_a_watchdog_without_mergeable_state() -> Result<(), String> {
    let source = repo_file(WATCHDOG)?;
    let changed = source.replace(
        WATCHDOG_MERGEABLE_STATE_QUERY,
        "repos/$REPOSITORY/issues/$number",
    );
    assert_ne!(changed, source, "mergeable-state mutation must engage");
    assert!(
        !watchdog_reports_mergeable_state(&changed),
        "a watchdog that stops reading the single-PR mergeable_state must violate the contract",
    );
    Ok(())
}

#[test]
fn contract_rejects_a_dispatching_watchdog() -> Result<(), String> {
    let source = repo_file(WATCHDOG)?;
    let changed = source
        .replace("  contents: read\n", "  contents: read\n  actions: write\n")
        .replace(
            "          set -euo pipefail\n",
            "          set -euo pipefail\n          gh workflow run routed-rust.yml --ref=\"$head_ref\"\n",
        );
    assert_ne!(changed, source, "dispatch mutation must engage");
    assert_eq!(
        watchdog_dispatch_fragments(&changed),
        vec!["actions: write", "gh workflow run"],
        "a restored dispatch path must violate the alert-only contract",
    );
    Ok(())
}

// #4987: these are event/config contract checks, not a scheduler, review cache,
// or a claim that a hosted lifecycle has been observed.
const UB_WORKFLOW: &str = ".github/workflows/ub-review.yml";
const REVIEW_EVENTS: &[&str] = &["opened", "reopened", "synchronize"];
const UB_INTENT_GUARD: &str = "    if: github.event.pull_request.head.repo.full_name == github.repository && !github.event.pull_request.draft && github.event.action == 'labeled' && github.event.label.name == 'merge-ready'\n";
const UB_UNSELECTED_GROUP: &str = "!github.event.pull_request.draft && github.event.action == 'labeled' && github.event.label.name == 'merge-ready' && 'intent' || github.run_id";

fn parsed_types(source: &str) -> Result<Vec<String>, String> {
    repo_policy::routed_rust_pull_request_types(source)
        .ok_or_else(|| "workflow declares auditable on.pull_request.types".to_owned())
}

fn publication_events(config: &str) -> Result<Vec<String>, String> {
    let config: toml::Value = toml::from_str(config).map_err(|error| format!("{error}"))?;
    config
        .get("gate")
        .and_then(|gate| gate.get("post_review_on"))
        .and_then(toml::Value::as_array)
        .ok_or_else(|| {
            "explicit gate.post_review_on required: pin defaults omit repair publication".to_owned()
        })?
        .iter()
        .map(|event| {
            event
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "publication event is a string".to_owned())
        })
        .collect()
}

fn advisory_contract(ub: &str, config: &str) -> Result<(), String> {
    let mut ub_events = parsed_types(ub)?;
    ub_events.sort();
    if ub_events != ["labeled"] {
        return Err(format!("UB explicit-intent events changed: {ub_events:?}"));
    }
    for required in [
        UB_INTENT_GUARD,
        UB_UNSELECTED_GROUP,
        "          run-pass: manual\n",
    ] {
        if !ub.contains(required) {
            return Err(format!("UB explicit-intent guard missing {required}"));
        }
    }
    let mut posts = publication_events(config)?;
    posts.sort();
    if posts != REVIEW_EVENTS {
        return Err(format!("UB legacy publication policy changed: {posts:?}"));
    }
    Ok(())
}

#[test]
fn advisory_ready_dedup_contract() -> Result<(), String> {
    advisory_contract(&repo_file(UB_WORKFLOW)?, &repo_file(".ub-review.toml")?)
}

#[test]
fn ready_after_active_completed_or_unavailable_advisory_has_no_new_admission() -> Result<(), String>
{
    // GitHub event admission has no dependency on the previous run's outcome.
    // The real UB workflow covers all three prior states; there is no fixture
    // state machine that could manufacture review success.
    let events = parsed_types(&repo_file(UB_WORKFLOW)?)?;
    assert!(
        !events.iter().any(|event| event == "ready_for_review"),
        "UB: Ready must neither replace active work nor repeat completed/unavailable work"
    );
    assert_eq!(parsed_types(&workflow_source()?)?, ["labeled"]);
    Ok(())
}

#[test]
fn new_heads_wait_for_a_fresh_explicit_review_request() -> Result<(), String> {
    let events = parsed_types(&repo_file(UB_WORKFLOW)?)?;
    assert_eq!(events, ["labeled"]);
    for event in ["opened", "reopened", "synchronize", "ready_for_review"] {
        assert!(
            !events.iter().any(|actual| actual == event),
            "{event} must not start Cargo work"
        );
    }
    Ok(())
}

#[test]
fn advisory_guards_and_concurrency_remain_exact() -> Result<(), String> {
    let ub = repo_file(UB_WORKFLOW)?;
    // Pin executable guard fields, not coincidental words in comments.
    assert!(
        ub.contains("    if: github.event.pull_request.head.repo.full_name == github.repository && !github.event.pull_request.draft && github.event.action == 'labeled' && github.event.label.name == 'merge-ready'\n")
    );
    assert!(ub.contains("  group: ub-review-${{ github.event.pull_request.number || github.ref }}-${{ !github.event.pull_request.draft && github.event.action == 'labeled' && github.event.label.name == 'merge-ready' && 'intent' || github.run_id }}\n  cancel-in-progress: true\n"));
    assert!(ub.contains("          review-mode: advisory\n"));
    assert!(ub.contains("          config: .ub-review.toml\n"));
    assert!(ub.contains("          posting: review\n"));
    assert!(ub.contains("          pr-head-sha: ${{ github.event.pull_request.head.sha }}\n"));
    Ok(())
}

#[test]
fn contract_rejects_ready_reentry_and_missing_repair_publication() -> Result<(), String> {
    let ub = repo_file(UB_WORKFLOW)?;
    let config = repo_file(".ub-review.toml")?;
    let changed = ub.replacen("types: [", "types: [ready_for_review, ", 1);
    assert_ne!(changed, ub, "mutation engages UB review");
    assert!(
        advisory_contract(&changed, &config).is_err(),
        "Ready reentry must violate UB review"
    );
    let changed = config.replace("\"synchronize\", ", "");
    assert_ne!(changed, config, "publication mutation engages");
    assert!(advisory_contract(&ub, &changed).is_err());
    for (from, to) in [
        ("!github.event.pull_request.draft", "true"),
        ("github.event.label.name == 'merge-ready'", "true"),
        (UB_UNSELECTED_GROUP, "'intent'"),
        ("          run-pass: manual\n", ""),
    ] {
        let weakened = ub.replace(from, to);
        assert_ne!(weakened, ub);
        assert!(advisory_contract(&weakened, &config).is_err());
    }
    Ok(())
}

const WHEEL_WORKFLOW: &str = ".github/workflows/python-wheel-qualification.yml";
const WHEEL_INTENT_GUARD: &str = "(github.event_name != 'pull_request' || (!github.event.pull_request.draft && github.event.action == 'labeled' && github.event.label.name == 'merge-ready'))";
const WHEEL_UNSELECTED_GROUP: &str = "github.event_name == 'pull_request' && (github.event.pull_request.draft || github.event.action != 'labeled' || github.event.label.name != 'merge-ready') && github.run_id || 'intent'";

fn wheel_quiet_intent_contract(source: &str) -> Result<(), String> {
    if parsed_types(source)? != ["labeled"] {
        return Err("wheel PR events must be labeled only".into());
    }
    if !source.contains("  merge_group:\n    types: [checks_requested]\n") {
        return Err("wheel merge-group qualification missing".into());
    }
    if !source.contains(WHEEL_UNSELECTED_GROUP) {
        return Err("unselected wheel events can cancel active qualification".into());
    }
    for job in ["applicability", "source-contract", "build-wheel"] {
        let marker = format!("\n  {job}:\n");
        let body = source
            .split_once(&marker)
            .ok_or_else(|| format!("missing {job}"))?
            .1;
        let condition = body
            .lines()
            .take_while(|line| !(line.starts_with("  ") && !line.starts_with("    ")))
            .find_map(|line| line.strip_prefix("    if: "))
            .ok_or_else(|| format!("missing {job} condition"))?;
        if !condition.contains(WHEEL_INTENT_GUARD) {
            return Err(format!("{job} can run without explicit intent"));
        }
        if job != "applicability" && (!condition.contains("needs.applicability.result != 'success' || needs.applicability.outputs.qualify != 'false'") || !condition.contains("always() && !cancelled()")) {
            return Err(format!("{job} lost fail-closed qualification fallback"));
        }
    }
    let consume = source
        .split_once("\n  consume:\n")
        .ok_or("missing wheel consumer")?
        .1;
    if !consume
        .lines()
        .take_while(|line| !(line.starts_with("  ") && !line.starts_with("    ")))
        .any(|line| {
            line.contains(
        "needs.build-wheel.result == 'success' && needs.source-contract.result == 'success'",
    )
        })
    {
        return Err("wheel consumer lost producer requirements".into());
    }
    Ok(())
}

#[test]
fn wheel_only_builds_after_explicit_pr_intent_or_for_merge_group() -> Result<(), String> {
    let wheel = repo_file(WHEEL_WORKFLOW)?;
    wheel_quiet_intent_contract(&wheel)?;
    for (from, to) in [
        ("types: [labeled]", "types: [opened, labeled]"),
        ("    types: [checks_requested]", "    types: [destroyed]"),
        (WHEEL_INTENT_GUARD, "true"),
        (WHEEL_UNSELECTED_GROUP, "'intent'"),
        ("!github.event.pull_request.draft", "true"),
        ("github.event.label.name == 'merge-ready'", "true"),
        (
            "needs.applicability.result != 'success' || needs.applicability.outputs.qualify != 'false'",
            "true",
        ),
        (
            "needs.build-wheel.result == 'success' && needs.source-contract.result == 'success'",
            "true",
        ),
    ] {
        let weakened = wheel.replacen(from, to, 1);
        assert_ne!(weakened, wheel, "wheel negative control must change source");
        assert!(
            wheel_quiet_intent_contract(&weakened).is_err(),
            "wheel guard accepted mutation: {from}"
        );
    }
    for job in ["source-contract", "build-wheel"] {
        let marker = format!("\n  {job}:\n");
        let start = wheel.find(&marker).ok_or("missing target job")?;
        let offset = wheel[start..]
            .find(WHEEL_INTENT_GUARD)
            .ok_or("missing target guard")?
            + start;
        let mut weakened = wheel.clone();
        weakened.replace_range(offset..offset + WHEEL_INTENT_GUARD.len(), "true");
        assert!(
            wheel_quiet_intent_contract(&weakened).is_err(),
            "{job} ran without intent"
        );
    }
    Ok(())
}

#[test]
fn event_parser_ignores_decoys_outside_pull_request() -> Result<(), String> {
    let source = "# types: [ready_for_review]\non:\n  workflow_dispatch:\n    inputs:\n      types: [ready_for_review]\n  pull_request:\n    types: ['opened', \"synchronize\", reopened] # real events\njobs:\n  decoy:\n    run: |\n      types: [ready_for_review]\n";
    assert_eq!(parsed_types(source)?, ["opened", "synchronize", "reopened"]);
    assert!(
        repo_policy::routed_rust_pull_request_types(
            "jobs:\n  decoy:\n    run: |\n      types: [ready_for_review]\n"
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn unauditable_event_layouts_and_missing_publication_fail_closed() -> Result<(), String> {
    for source in [
        "on:\n  pull_request:\n",
        "on:\n  pull_request:\n    types:\n      - opened\n",
        "on:\n  pull_request:\n    types: [opened\n",
        "on:\n  pull_request:\n    types: ['opened\"]\n",
    ] {
        assert!(repo_policy::routed_rust_pull_request_types(source).is_none());
    }
    for config in [
        "[gate]\nrequired_check = 'ub-review/gate'\n",
        "[gate]\npost_review_on = 'opened'\n",
    ] {
        let Err(error) = publication_events(config) else {
            return Err("missing/non-array publication policy must fail closed".to_owned());
        };
        assert!(error.contains("explicit gate.post_review_on required"));
    }
    Ok(())
}

#[test]
fn contract_rejects_replacing_required_merge_ready_admission() -> Result<(), String> {
    let source = workflow_source()?;
    let changed = source.replace("types: [labeled]", "types: [opened]");
    assert_ne!(changed, source, "required-admission mutation engages");
    assert_ne!(event_declarations(&changed)?, EXPECTED_EVENT_DECLARATIONS);
    assert_ne!(parsed_types(&changed)?, ["labeled"]);
    Ok(())
}

#[test]
fn automatic_droid_review_is_retired() -> Result<(), String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    for path in [
        ".github/workflows/droid-review.yml",
        ".github/workflows/droid-review.yaml",
    ] {
        assert!(
            !root
                .join(path)
                .try_exists()
                .map_err(|error| error.to_string())?,
            "{path}: automatic Droid review was retired under #5086"
        );
    }
    // Retiring automatic review must not remove the independently invoked lanes.
    for path in [
        ".github/workflows/droid.yml",
        ".github/workflows/droid-security-scan.yml",
    ] {
        assert!(
            !repo_file(path)?.is_empty(),
            "{path}: retained Droid workflow is required"
        );
    }
    Ok(())
}
