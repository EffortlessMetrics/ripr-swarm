//! Verify the explicit PR request boundary for advisory workflows.

use std::{fs, path::Path};

fn repo_file(path: &str) -> Result<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "missing repository root".to_string())?;
    fs::read_to_string(root.join(path)).map_err(|error| error.to_string())
}

// These advisory workflows keep their manual/push/scheduled paths, but PR
// activity must allocate work only for an explicit non-draft request.
fn quiet_advisory_contract(source: &str, jobs: &[&str]) -> bool {
    let Some((events, job_source)) = source.split_once("\njobs:\n") else {
        return false;
    };
    if !events.contains("  pull_request:\n    types: [labeled]\n") {
        return false;
    }
    jobs.iter().all(|job| {
        let marker = format!("  {job}:\n");
        let Some((_, tail)) = job_source.split_once(&marker) else {
            return false;
        };
        tail.lines()
            .take_while(|line| !line.starts_with("  ") || line.starts_with("   "))
            .any(|line| {
                line.starts_with("    if: ")
                    && line.contains("github.event_name != 'pull_request'")
                    && line.contains("!github.event.pull_request.draft")
                    && line.contains("github.event.label.name == 'merge-ready'")
            })
    })
}

#[test]
fn windows_and_spec_advisories_require_explicit_non_draft_pr_request() -> Result<(), String> {
    for (path, jobs) in [
        (
            ".github/workflows/windows-gate.yml",
            &["detect", "result"][..],
        ),
        (
            ".github/workflows/source-of-truth.yml",
            &["source-of-truth", "spec-maintenance-digest"][..],
        ),
    ] {
        let source = repo_file(path)?;
        assert!(quiet_advisory_contract(&source, jobs), "{path}");
        assert!(!quiet_advisory_contract(
            &source.replace("types: [labeled]", "types: [opened, synchronize, labeled]"),
            jobs
        ));
        assert!(!quiet_advisory_contract(
            &source.replace("!github.event.pull_request.draft", "true"),
            jobs
        ));
        assert!(!quiet_advisory_contract(
            &source.replace("github.event.label.name == 'merge-ready'", "true"),
            jobs
        ));
    }
    let windows = repo_file(".github/workflows/windows-gate.yml")?;
    assert!(windows.contains("github.event.pull_request.draft || github.event.label.name != 'merge-ready') && github.run_id"));
    Ok(())
}
