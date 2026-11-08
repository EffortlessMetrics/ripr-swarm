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
    assert!(
        upload.contains("if: always() && steps.rust-tests.outcome != 'skipped'"),
        "failure-bearing JUnit must upload independently of the failing verdict"
    );
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

const GOOD_JUNIT: &str =
    "<testsuites name=\"nextest-run\" tests=\"3\" failures=\"0\"></testsuites>";
const FAILING_JUNIT: &str =
    "<testsuites name=\"nextest-run\" tests=\"3\" failures=\"1\"></testsuites>";

/// Runs the workflow's own `Verify retained JUnit report` block in a scratch
/// checkout shape and reports whether it exited successfully plus whether the
/// retained XML survived (the upload step reads it after this verdict step).
fn run_verify(outcome: &str, junit: Option<&str>) -> Result<(bool, bool), String> {
    let script =
        step("Verify retained JUnit report")?.replace("${{ steps.rust-tests.outcome }}", outcome);
    let dir = test_support::scratch("test-analytics-verify")?;
    let xml = dir.join("target/nextest/ci/junit.xml");
    if let Some(body) = junit {
        std::fs::create_dir_all(xml.parent().ok_or("junit parent")?).map_err(|e| e.to_string())?;
        std::fs::write(&xml, body).map_err(|e| e.to_string())?;
    }
    let status = std::process::Command::new("bash")
        .args(["-eo", "pipefail", "-c", &script])
        .current_dir(&dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    Ok((status.success(), xml.exists()))
}

#[test]
fn verify_step_keeps_failure_verdict_and_retains_failure_bearing_junit() -> Result<(), String> {
    // Completed run with a nonempty report: verdict passes.
    assert_eq!(run_verify("success", Some(GOOD_JUNIT))?, (true, true));
    // Nextest failed but wrote a failure-bearing report: the verdict still
    // fails and the report is left in place for the always() upload.
    assert_eq!(run_verify("failure", Some(FAILING_JUNIT))?, (false, true));
    assert_eq!(run_verify("cancelled", Some(FAILING_JUNIT))?, (false, true));
    // Nextest reported success without any report: verdict fails.
    assert_eq!(run_verify("success", None)?, (false, false));
    // A report that names zero tests is not a completed run.
    let empty = "<testsuites name=\"nextest-run\" tests=\"0\"></testsuites>";
    assert_eq!(run_verify("success", Some(empty))?, (false, true));
    Ok(())
}

#[test]
fn terminal_evidence_step_never_fails_the_job() -> Result<(), String> {
    let script = step("Write bounded nextest terminal evidence")?
        .replace("${{ steps.rust-tests.outcome }}", "failure");
    for log in [
        None,
        Some("        PASS [   0.003s] a::b\n     Summary [  40.0s] 3 tests run: 3 passed\n"),
        Some("nothing relevant\n"),
    ] {
        let dir = test_support::scratch("test-analytics-summary")?;
        std::fs::create_dir_all(dir.join("target/nextest/ci")).map_err(|e| e.to_string())?;
        if let Some(text) = log {
            std::fs::write(dir.join("target/test-analytics-nextest.log"), text)
                .map_err(|e| e.to_string())?;
        }
        let summary = dir.join("summary.md");
        let status = std::process::Command::new("bash")
            .args(["-eo", "pipefail", "-c", &script])
            .env("GITHUB_STEP_SUMMARY", &summary)
            .current_dir(&dir)
            .status()
            .map_err(|e| e.to_string())?;
        assert!(
            status.success(),
            "summary step must not fail the job: {log:?}"
        );
        let body = std::fs::read_to_string(&summary).map_err(|e| e.to_string())?;
        assert!(body.contains("step outcome: failure"));
    }
    Ok(())
}
