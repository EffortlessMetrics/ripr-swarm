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
const EXPECTED_RESULT_NAME: &str = "${{ (github.event_name != 'pull_request' || github.event.label.name == 'merge-ready') && 'Ripr Rust Small Result' || 'Merge-ready Nonrequest' }}";
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
    assert!(!source.contains("github.event.pull_request.draft"));
    Ok(())
}

#[test]
fn contract_rejects_draft_or_mutation_triggers() -> Result<(), String> {
    let source = workflow_source()?;
    let changed = source.replace("types: [labeled]", "types: [labeled, synchronize]");
    assert_ne!(changed, source, "trigger mutation must engage");
    assert_ne!(
        event_declarations(&changed)?,
        EXPECTED_EVENT_DECLARATIONS,
        "synchronize must violate the protected event law",
    );
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
    let widened = source.replace("label.name == 'merge-ready') &&", "label.name != '') &&");
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

// Development review is session-owned under #4984/#4842. Keep the existing
// required caller's bootstrap controls above until the protected cutover, but
// do not restore any automatic advisory review subscriber alongside it.
fn parsed_types(source: &str) -> Result<Vec<String>, String> {
    repo_policy::routed_rust_pull_request_types(source)
        .ok_or_else(|| "workflow declares auditable on.pull_request.types".to_owned())
}

#[test]
fn advisory_review_actions_are_retired_without_removing_review_configuration() -> Result<(), String>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    for name in ["ub-review.yml", "ub-review.yaml", "droid.yml", "droid.yaml"] {
        assert!(
            !root
                .join(".github/workflows")
                .join(name)
                .try_exists()
                .map_err(|error| error.to_string())?,
            "{name}: development review belongs in the session"
        );
    }
    let config: toml::Value =
        toml::from_str(&repo_file(".ub-review.toml")?).map_err(|error| error.to_string())?;
    assert!(
        config.get("repo").is_some(),
        "retain tool configuration for explicit review"
    );
    assert!(!repo_file(".factory/skills/review-guidelines/SKILL.md")?.is_empty());
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
    // The separately purposed security scan is not a development review loop.
    for path in [".github/workflows/droid-security-scan.yml"] {
        assert!(
            !repo_file(path)?.is_empty(),
            "{path}: retained Droid workflow is required"
        );
    }
    Ok(())
}
