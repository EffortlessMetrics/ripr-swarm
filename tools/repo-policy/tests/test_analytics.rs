//! #7142: Test Analytics must finish with retained JUnit evidence and not treat a
//! missing report as success.

#[path = "../test_support.rs"]
mod test_support;

fn workflow() -> Result<String, String> {
    std::fs::read_to_string(
        test_support::workspace_root()?.join(".github/workflows/test-analytics.yml"),
    )
    .map_err(|e| e.to_string())
}

fn step(name: &str) -> Result<String, String> {
    let text = workflow()?;
    let (_, tail) = text
        .split_once(&format!("- name: {name}\n"))
        .ok_or_else(|| format!("missing workflow step: {name}"))?;
    repo_policy::extract_workflow_run_blocks(tail)
        .first()
        .map(|block| block.text.clone())
        .ok_or_else(|| format!("missing run block: {name}"))
}

#[test]
fn measured_budget_and_junit_honesty_are_wired() -> Result<(), String> {
    let text = workflow()?;
    let job = text
        .split_once("jobs:\n")
        .and_then(|(_, tail)| tail.split_once("\n\n"))
        .map(|(job, _)| job)
        .ok_or("missing rust-tests job")?;
    let timeout = job
        .lines()
        .find_map(|line| line.trim().strip_prefix("timeout-minutes: "))
        .ok_or("job timeout-minutes missing")?;
    let minutes = timeout
        .parse::<u32>()
        .map_err(|error| format!("timeout not numeric: {error}"))?;
    assert!(
        minutes >= 50,
        "job budget must cover the measured ~26527-test all-features workload (#7142)"
    );

    let tests = step("Run Rust tests with JUnit output")?;
    assert!(tests.contains("mkdir -p target/nextest/ci"));
    assert!(tests.contains("run-context.txt"));
    assert!(tests.contains("--all-features --profile ci"));
    assert!(tests.contains("target/test-analytics-nextest.log"));
    assert!(tests.contains("nextest succeeded without a fresh JUnit report"));

    let (_, upload) = text
        .split_once("- name: Upload JUnit test results\n")
        .ok_or("missing junit upload")?;
    assert!(upload.contains("if-no-files-found: error"));
    assert!(upload.contains("path: target/nextest/ci/junit.xml"));
    assert!(
        text.contains("- name: Verify retained JUnit report\n"),
        "missing explicit JUnit verification step"
    );
    assert!(
        text.contains("- name: Upload Test Analytics diagnostics\n"),
        "diagnostics must upload separately from JUnit"
    );

    let summary = step("Write bounded nextest terminal evidence")?;
    assert!(summary.contains("GITHUB_STEP_SUMMARY"));
    assert!(summary.contains("SLOW"));
    assert!(
        text.contains("- name: Write bounded nextest terminal evidence\n        if: always()"),
        "summary step must retain terminal evidence on cancel or failure"
    );

    Ok(())
}
