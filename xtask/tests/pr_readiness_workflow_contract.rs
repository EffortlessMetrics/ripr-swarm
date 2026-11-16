//! Pin the protected qualification event at the merge-queue boundary.
//!
//! Draft, Ready, synchronize, labels and direct pushes do not allocate the
//! required proof. GitHub requests it only for `merge_group.checks_requested`;
//! `workflow_dispatch` remains an explicit diagnostic authority.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const WORKFLOW: &str = ".github/workflows/routed-rust.yml";
const WATCHDOG: &str = ".github/workflows/pr-staleness-watchdog.yml";
const REQUIRED_CONTEXT: &str = "Ripr Rust Small Result";
const EXPECTED_CONCURRENCY_GROUP: &str =
    "  group: ${{ format('{0}-{1}', github.workflow, github.event.merge_group.head_sha || github.sha) }}";
const EXPECTED_CANCEL_IN_PROGRESS: &str = "  cancel-in-progress: false";
const WATCHDOG_DISPATCH_FRAGMENTS: &[&str] = &["actions: write", "gh workflow run", "/dispatches"];

fn repo_file(path: &str) -> Result<String, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    fs::read_to_string(root.join(path)).map_err(|error| error.to_string())
}

fn workflow_source() -> Result<String, String> {
    repo_file(WORKFLOW)
}

fn direct_job_field<'a>(source: &'a str, job: &str, field: &str) -> Option<&'a str> {
    let target = format!("  {job}:");
    let prefix = format!("    {field}: ");
    let mut in_jobs = false;
    let mut in_target = false;
    for raw in source.lines() {
        let line = raw.trim_end();
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

#[test]
fn protected_context_is_merge_queue_only() -> Result<(), String> {
    let source = workflow_source()?;
    let declarations = repo_policy::workflow_lifecycle::event_declarations(&source)?;
    assert_eq!(
        declarations,
        BTreeMap::from([
            (
                "merge_group".to_owned(),
                vec!["types: [checks_requested]".to_owned()],
            ),
            ("workflow_dispatch".to_owned(), Vec::new()),
        ])
    );
    assert!(!declarations.contains_key("pull_request"));
    assert!(!declarations.contains_key("pull_request_target"));
    assert!(!declarations.contains_key("push"));
    assert!(source.contains(EXPECTED_CONCURRENCY_GROUP));
    assert!(source.contains(EXPECTED_CANCEL_IN_PROGRESS));
    assert_eq!(
        direct_job_field(&source, "result", "name"),
        Some("${{ (github.event_name != 'pull_request' || github.event.label.name == 'merge-ready') && 'Ripr Rust Small Result' || 'Merge-ready Nonrequest' }}")
    );
    assert!(source.contains(REQUIRED_CONTEXT));
    assert!(!source.contains("types: [labeled]"));
    assert!(!source.contains("ready_for_review"));
    assert!(!source.contains("synchronize"));
    Ok(())
}

#[test]
fn contract_rejects_restoring_a_pr_or_push_root() -> Result<(), String> {
    let source = workflow_source()?;
    for event in ["pull_request", "pull_request_target", "push"] {
        let changed = source.replacen(
            "  workflow_dispatch:\n",
            &format!("  {event}:\n  workflow_dispatch:\n"),
            1,
        );
        assert_ne!(changed, source);
        let declarations = repo_policy::workflow_lifecycle::event_declarations(&changed)?;
        assert!(declarations.contains_key(event));
        assert!(!repo_policy::workflow_lifecycle::violations(WORKFLOW, &changed).is_empty());
    }
    Ok(())
}

#[test]
fn contract_rejects_a_noncanonical_queue_request() -> Result<(), String> {
    let source = workflow_source()?;
    for replacement in ["types: [destroyed]", "types: [checks_requested, destroyed]"] {
        let changed = source.replace("types: [checks_requested]", replacement);
        assert_ne!(changed, source);
        assert!(!repo_policy::workflow_lifecycle::violations(WORKFLOW, &changed).is_empty());
    }
    Ok(())
}

#[test]
fn staleness_watchdog_reports_without_dispatching() -> Result<(), String> {
    let source = repo_file(WATCHDOG)?;
    assert!(source.contains("REQUIRED_CHECK: Ripr Rust Small Result"));
    assert_eq!(watchdog_dispatch_fragments(&source), Vec::<&str>::new());
    Ok(())
}
