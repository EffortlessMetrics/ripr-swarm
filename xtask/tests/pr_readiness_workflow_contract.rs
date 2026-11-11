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
fn direct_job_field<'a>(source: &'a str, job: &str, field: &str) -> Option<&'a str> {
    let target = format!("  {job}:");
    let prefix = format!("    {field}: ");
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

        if in_target && let Some(value) = line.strip_prefix(&prefix) {
            return Some(value.trim());
        }
    }

    None
}

/// Read a direct static job name from the `jobs` mapping.
fn direct_job_name<'a>(source: &'a str, job: &str) -> Option<&'a str> {
    direct_job_field(source, job, "name")
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

// These are event/config contract checks, not a scheduler, review cache, or
// a claim that a hosted lifecycle has been observed.
const UB_WORKFLOW: &str = ".github/workflows/ub-review.yml";
const UB_GUARD: &str = "github.event.pull_request.head.repo.full_name == github.repository && github.event.action == 'labeled' && github.event.label.name == 'full-ci'";
const UB_PUBLICATION_EVENTS: &[&str] = &["opened", "reopened", "synchronize"];

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

/// Model admission for review cases after pinning the actual guard string.
/// An unrecognized guard fails closed; this does not execute a GitHub event.
fn models_ub_guard_admission(
    source: &str,
    action: &str,
    label: Option<&str>,
    same_repository: bool,
) -> Result<bool, String> {
    let guard = direct_job_field(source, "ub-review", "if")
        .ok_or_else(|| "UB executable job guard must be directly auditable".to_owned())?;
    if guard != UB_GUARD {
        return Err(format!("unexpected UB executable guard: {guard}"));
    }
    Ok(same_repository && action == "labeled" && label == Some("full-ci"))
}

const UB_CONCURRENCY_SELECTOR: &str = "ub-review-${{ github.event.pull_request.number || github.ref }}-${{ github.event.label.name == 'full-ci' && 'full-ci' || github.run_id }}";

fn yaml_path<'a>(
    mapping: &'a serde_yaml_ng::Mapping,
    path: &[&str],
) -> Result<&'a serde_yaml_ng::Value, String> {
    let Some((last, parents)) = path.split_last() else {
        return Err("YAML field path must not be empty".to_owned());
    };
    let mut current = mapping;
    for key in parents {
        current = current
            .get(&serde_yaml_ng::Value::String((*key).to_owned()))
            .and_then(serde_yaml_ng::Value::as_mapping)
            .ok_or_else(|| format!("missing YAML mapping at {path:?}"))?;
    }
    current
        .get(&serde_yaml_ng::Value::String((*last).to_owned()))
        .ok_or_else(|| format!("missing YAML field {path:?}"))
}

/// Read typed concurrency fields from the workflow, rather than accepting a
/// matching line from another YAML scope or a continued scalar value.
fn ub_concurrency_fields(source: &str) -> Result<(String, bool), String> {
    let root: serde_yaml_ng::Mapping =
        serde_yaml_ng::from_str(source).map_err(|error| format!("parse UB YAML: {error}"))?;
    let concurrency = yaml_path(&root, &["concurrency"])?
        .as_mapping()
        .ok_or_else(|| "UB concurrency must be a mapping".to_owned())?;
    let group = yaml_path(concurrency, &["group"])?
        .as_str()
        .ok_or_else(|| "UB concurrency.group must be a string".to_owned())?;
    let cancel = yaml_path(concurrency, &["cancel-in-progress"])?
        .as_bool()
        .ok_or_else(|| "UB cancel-in-progress must be a boolean".to_owned())?;
    if group != UB_CONCURRENCY_SELECTOR {
        return Err(format!("unexpected UB concurrency.group: {group}"));
    }
    if !cancel {
        return Err("UB cancellation must stay enabled per group".to_owned());
    }
    Ok((group.to_owned(), cancel))
}

fn ub_action_run_pass(source: &str) -> Result<String, String> {
    let root: serde_yaml_ng::Mapping =
        serde_yaml_ng::from_str(source).map_err(|error| format!("parse UB YAML: {error}"))?;
    let steps = yaml_path(&root, &["jobs", "ub-review", "steps"])?
        .as_sequence()
        .ok_or_else(|| "UB action steps must be a sequence".to_owned())?;
    let step = steps
        .iter()
        .find(|step| {
            step.get(&serde_yaml_ng::Value::String("uses".to_owned()))
                .and_then(serde_yaml_ng::Value::as_str)
                == Some("EffortlessMetrics/ub-review@8d45f2cace407b1b0117cffe30e32bf6267d5182")
        })
        .and_then(serde_yaml_ng::Value::as_mapping)
        .ok_or_else(|| "UB pinned action step must be present".to_owned())?;
    let run_pass = yaml_path(step, &["with", "run-pass"])?
        .as_str()
        .ok_or_else(|| "UB run-pass must select the operator-requested manual pass".to_owned())?;
    (run_pass == "manual")
        .then(|| run_pass.to_owned())
        .ok_or_else(|| format!("unexpected UB run-pass: {run_pass}"))
}

/// After parsing the group selector, model only the event's selected suffix.
/// Any wrong, missing or continued actual group value fails before this model.
fn models_ub_concurrency_group(source: &str, label: &str, run_id: u64) -> Result<String, String> {
    ub_concurrency_fields(source)?;
    Ok(if label == "full-ci" {
        "full-ci".to_owned()
    } else {
        run_id.to_string()
    })
}

fn advisory_contract(ub: &str, config: &str) -> Result<(), String> {
    if parsed_types(ub)? != ["labeled"] {
        return Err("UB subscribe only to labeled pull-request events".to_owned());
    }
    let mut posts = publication_events(config)?;
    posts.sort();
    if posts != UB_PUBLICATION_EVENTS {
        return Err(format!(
            "UB must retain prior publication and admit full-ci reviews: {posts:?}"
        ));
    }
    if !models_ub_guard_admission(ub, "labeled", Some("full-ci"), true)? {
        return Err("same-repository full-ci must execute the advisory review".to_owned());
    }
    if ub_action_run_pass(ub)? != "manual" {
        return Err("full-ci must select the publication-permitted pass".to_owned());
    }
    Ok(())
}

#[test]
fn full_ci_admission_and_publication_contract() -> Result<(), String> {
    advisory_contract(&repo_file(UB_WORKFLOW)?, &repo_file(".ub-review.toml")?)
}

#[test]
fn non_request_labels_ready_updates_and_forks_execute_no_ub_job() -> Result<(), String> {
    let source = repo_file(UB_WORKFLOW)?;
    for (action, label, same_repository) in [
        ("labeled", Some("merge-ready"), true),
        ("labeled", Some("windows-ci"), true),
        ("labeled", Some("full-ci"), false),
        ("labeled", None, true),
        ("opened", None, true),
        ("reopened", None, true),
        ("synchronize", None, true),
        ("ready_for_review", None, true),
    ] {
        assert!(
            !models_ub_guard_admission(&source, action, label, same_repository)?,
            "UB must not execute for {action}/{label:?}/same_repository={same_repository}"
        );
    }
    assert_eq!(parsed_types(&source)?, ["labeled"]);
    Ok(())
}

#[test]
fn full_ci_label_reapplication_admits_the_new_exact_head() -> Result<(), String> {
    let source = repo_file(UB_WORKFLOW)?;
    // The same label event requests review once. Re-applying it for a new head
    // is a separate request, not an opened/synchronize/Ready re-entry path.
    assert!(models_ub_guard_admission(
        &source,
        "labeled",
        Some("full-ci"),
        true
    )?);
    assert!(source.contains("          pr-head-sha: ${{ github.event.pull_request.head.sha }}\n"));
    let first = models_ub_concurrency_group(&source, "full-ci", 101)?;
    let later = models_ub_concurrency_group(&source, "full-ci", 202)?;
    assert_eq!(first, later, "full-ci requests intend supersession");
    Ok(())
}

#[test]
fn unrelated_labels_cannot_cancel_admitted_full_ci() -> Result<(), String> {
    let source = repo_file(UB_WORKFLOW)?;
    let admitted = models_ub_concurrency_group(&source, "full-ci", 101)?;
    let replacement = models_ub_concurrency_group(&source, "full-ci", 202)?;
    assert_eq!(
        admitted, replacement,
        "a repeated full-ci request must retain intended supersession"
    );
    for label in ["merge-ready", "windows-ci", "vscode"] {
        assert_ne!(
            models_ub_concurrency_group(&source, label, 303)?,
            admitted,
            "{label} must not group with or cancel admitted full-ci work"
        );
    }
    assert!(ub_concurrency_fields(&source)?.1);
    Ok(())
}

#[test]
fn concurrency_parser_rejects_per_pr_continued_and_decoy_groups() -> Result<(), String> {
    let source = repo_file(UB_WORKFLOW)?;
    let per_pr = source.replace(
        "-${{ github.event.label.name == 'full-ci' && 'full-ci' || github.run_id }}",
        "",
    );
    let error = match models_ub_concurrency_group(&per_pr, "full-ci", 101) {
        Ok(_) => return Err("a per-PR group must be rejected".to_owned()),
        Err(error) => error,
    };
    assert!(
        error.starts_with("unexpected UB concurrency.group: "),
        "unexpected per-PR error: {error}"
    );

    let continued = source.replacen(
        "  cancel-in-progress: true\n",
        "    ub-review-continued\n  cancel-in-progress: true\n",
        1,
    );
    assert_ne!(continued, source, "continuation mutation must engage");
    let error = match models_ub_concurrency_group(&continued, "full-ci", 101) {
        Ok(_) => return Err("continued group text must be rejected".to_owned()),
        Err(error) => error,
    };
    assert!(
        error.starts_with("unexpected UB concurrency.group: "),
        "unexpected continuation error: {error}"
    );

    let decoy = source.replacen("concurrency:\n", "run-name: |\n  concurrency:\n", 1);
    assert!(
        models_ub_concurrency_group(&decoy, "full-ci", 101).is_err(),
        "group text outside the concurrency mapping must not supply identity"
    );
    Ok(())
}

#[test]
fn advisory_sensors_concurrency_and_trust_remain_exact() -> Result<(), String> {
    let ub = repo_file(UB_WORKFLOW)?;
    assert_eq!(direct_job_field(&ub, "ub-review", "if"), Some(UB_GUARD));
    assert_eq!(ub_action_run_pass(&ub)?, "manual");
    assert!(ub_concurrency_fields(&ub)?.0 == UB_CONCURRENCY_SELECTOR);
    assert!(ub.contains("          review-mode: advisory\n"));
    assert!(ub.contains("          config: .ub-review.toml\n"));
    assert!(ub.contains("          posting: review\n"));
    assert!(ub.contains("          pr-head-sha: ${{ github.event.pull_request.head.sha }}\n"));
    assert!(
        ub.contains("uses: EffortlessMetrics/ub-review@8d45f2cace407b1b0117cffe30e32bf6267d5182")
    );
    assert!(ub.contains("uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09"));
    assert!(ub.contains("uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9"));
    Ok(())
}

#[test]
fn contract_rejects_reentry_and_missing_publication() -> Result<(), String> {
    let ub = repo_file(UB_WORKFLOW)?;
    let config = repo_file(".ub-review.toml")?;
    let changed = ub.replacen("types: [", "types: [synchronize, ", 1);
    assert_ne!(
        changed, ub,
        "failed to find 'types: [' in workflow to apply mutation"
    );
    let error = match advisory_contract(&changed, &config) {
        Ok(()) => return Err("synchronize must be rejected as a UB trigger".to_owned()),
        Err(error) => error,
    };
    assert_eq!(error, "UB subscribe only to labeled pull-request events");
    let changed = ub.replacen(
        "          run-pass: manual\n",
        "          run-pass: auto\n",
        1,
    );
    assert_ne!(changed, ub, "run-pass mutation must engage");
    let error = match advisory_contract(&changed, &config) {
        Ok(()) => return Err("automatic label-to-other mapping must not post".to_owned()),
        Err(error) => error,
    };
    assert_eq!(error, "unexpected UB run-pass: auto");
    let changed = config.replace("\"synchronize\", ", "");
    assert_ne!(
        changed, config,
        "failed to find '\"synchronize\", ' in config to apply mutation"
    );
    let error = match advisory_contract(&ub, &changed) {
        Ok(()) => return Err("synchronize publication must be retained".to_owned()),
        Err(error) => error,
    };
    assert!(
        error.starts_with("UB must retain prior publication and admit full-ci reviews: "),
        "unexpected publication error: {error}"
    );
    let changed = config.replace("\"opened\", ", "");
    assert_ne!(
        changed, config,
        "failed to find '\"opened\", ' in config to apply mutation"
    );
    let error = match advisory_contract(&ub, &changed) {
        Ok(()) => return Err("full-ci publication must be retained".to_owned()),
        Err(error) => error,
    };
    assert!(
        error.starts_with("UB must retain prior publication and admit full-ci reviews: "),
        "unexpected publication error: {error}"
    );
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
    assert_eq!(
        direct_job_field(
            "jobs:\n  ub-review:\n    name: unrelated\n  other:\n    if: ignored\n",
            "ub-review",
            "if"
        ),
        None
    );
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
