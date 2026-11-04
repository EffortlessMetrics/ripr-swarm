//! Queue-only trigger boundary for the staged advisory workflows.
use std::{fs, path::Path};

fn repo_file(path: &str) -> Result<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "missing repository root".to_string())?;
    fs::read_to_string(root.join(path)).map_err(|error| error.to_string())
}

fn queue_only_events(source: &str) -> bool {
    let Some((events, _)) = source.split_once("\npermissions:\n") else {
        return false;
    };
    events.contains("  merge_group:\n    types: [checks_requested]\n")
        && !events.contains("pull_request")
        && events.contains("  push:\n")
        && events.contains("  workflow_dispatch:")
}

#[test]
fn advisories_start_on_queue_and_not_pr_events() -> Result<(), String> {
    for path in [
        ".github/workflows/windows-gate.yml",
        ".github/workflows/source-of-truth.yml",
    ] {
        let source = repo_file(path)?;
        assert!(queue_only_events(&source), "{path}");
        assert!(!queue_only_events(
            &source.replace("  merge_group:", "  pull_request:")
        ));
        assert!(!queue_only_events(
            &source.replace("[checks_requested]", "[destroyed]")
        ));
        assert!(!queue_only_events(&source.replace("  push:\n", "")));
    }
    let windows = repo_file(".github/workflows/windows-gate.yml")?;
    assert!(windows.contains("github.event.merge_group.base_sha || github.event.before"));
    assert!(windows.contains("github.event.merge_group.head_sha || github.sha"));
    assert!(windows.contains("merge_group|push) ;;"));
    assert!(windows.contains("ref: ${{ github.event.merge_group.head_sha || github.sha }}"));
    Ok(())
}
