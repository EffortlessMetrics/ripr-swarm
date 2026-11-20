//! The advisory sentinel observes exact landed native qualification (#14500).
//! It does not rebuild, dispatch or become a required producer. The production
//! Python reader and actual tracker shell have executable API-fixture controls.

use std::fs;
use std::path::Path;

const WORKFLOW: &str = ".github/workflows/main-health-sentinel.yml";

fn repo_file(path: &str) -> Result<String, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    fs::read_to_string(root.join(path)).map_err(|error| error.to_string())
}

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

fn executable(source: &str) -> String {
    executable_lines(source).join("\n")
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
        "timeout-minutes: 10",
        "a hung probe is a red signal with an end, not a stuck run",
    )
}

#[test]
fn sentinel_reads_without_product_execution_or_merge_authority() -> Result<(), String> {
    let source = executable(&repo_file(WORKFLOW)?);
    for forbidden in [
        "actions: write",
        "checks: write",
        "contents: write",
        "pull-requests: write",
        "gh workflow run",
        "required_status_checks",
        "/protection",
        "/rules/",
        "/rerun",
        "cargo ",
        "rust-toolchain@",
        "Swatinem/rust-cache@",
        "npm ",
        "pip ",
        "  pull_request:",
        "  push:",
    ] {
        assert!(
            !source.contains(forbidden),
            "advisory reader contains {forbidden}"
        );
    }
    for expected in [
        "actions: read",
        "checks: read",
        "contents: read",
        "issues: write",
        "ref: ${{ github.sha }}",
        "sparse-checkout: tools/ci",
        "persist-credentials: false",
    ] {
        assert!(source.contains(expected), "missing {expected}");
    }
    Ok(())
}

#[test]
fn actual_reader_is_invoked_for_this_exact_landed_subject() -> Result<(), String> {
    let source = executable(&repo_file(WORKFLOW)?);
    for expected in [
        "python3 -I tools/ci/main-qualification.py \\",
        "--repository \"$GITHUB_REPOSITORY\" --sha \"$GITHUB_SHA\"",
        "--out \"$RUNNER_TEMP/main-health-sentinel/qualification.json\"",
        "continue-on-error: true",
        "verdict=unknown",
        "if [ \"$verdict\" != green ]; then",
    ] {
        assert!(source.contains(expected), "missing {expected}");
    }
    Ok(())
}

#[test]
fn unknown_never_heals_and_every_tracker_write_rechecks_native_authority() -> Result<(), String> {
    let source = executable(&repo_file(WORKFLOW)?);
    for expected in [
        "main-red: scheduled main-health sentinel found a broken main",
        "<!-- main-health-sentinel-tracker -->",
        "[ \"$VERDICT\" != unknown ] || exit 0",
        "elif [ \"$VERDICT\" = green ] && [ -n \"$number\" ]; then",
        "rm -f \"$recheck\"",
        ".verdict == $verdict and .run_id == $id and .run_attempt == $attempt",
        "grep -qxF \"$fingerprint\"",
        "-f state=closed",
    ] {
        assert!(source.contains(expected), "missing {expected}");
    }
    assert_eq!(
        source.matches("still_current ||").count(),
        4,
        "create, failure comment, recovery comment and close each re-observe"
    );
    Ok(())
}

#[test]
fn sentinel_run_block_budget_is_registered() -> Result<(), String> {
    let allowlist = repo_file("policy/workflow_allowlist.txt")?;
    assert!(
        allowlist
            .lines()
            .any(|line| line.starts_with(".github/workflows/main-health-sentinel.yml|"))
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn reader_and_actual_tracker_execute_native_fact_controls() -> Result<(), Box<dyn std::error::Error>>
{
    use std::process::Command;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("repository root")?;
    let output = Command::new("/usr/bin/timeout")
        .args(["--kill-after=2s", "60s", "python3", "-I"])
        .arg(root.join("tools/ci/test_main_qualification.py"))
        .arg("-v")
        .current_dir(root)
        .output()?;
    assert!(
        output.status.success(),
        "native observer controls failed: {:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
