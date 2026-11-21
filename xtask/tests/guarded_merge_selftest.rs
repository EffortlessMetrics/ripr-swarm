//! Runs the finish-pr Python guard and checkout-observation controls in CI.

use std::path::PathBuf;
use std::process::Command;

fn run_python_suite(name: &str) -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let test = root.join(".agents/skills/finish-pr/tests").join(name);
    let output = Command::new("python3")
        .arg("-I")
        .arg(&test)
        .output()
        .map_err(|error| format!("python3 must be available for {name}: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "{name} failed: {}{stderr}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    require_complete_suite(name, &stderr)
}

fn require_complete_suite(name: &str, stderr: &str) -> Result<(), String> {
    // unittest reports its summary on stderr. An unqualified OK excludes
    // skips and expected failures; require a single nonzero test count too.
    let counts: Vec<_> = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("Ran "))
        .collect();
    let count = match counts.as_slice() {
        [line] => line
            .split_whitespace()
            .next()
            .and_then(|n| n.parse::<usize>().ok()),
        _ => None,
    };
    let final_line = stderr.lines().rev().find(|line| !line.trim().is_empty());
    if !matches!(count, Some(n) if n > 0) || final_line != Some("OK") {
        return Err(format!(
            "{name} did not report a complete passing suite: {stderr}"
        ));
    }
    Ok(())
}

#[test]
fn guarded_merge_fake_api_tests_pass() -> Result<(), String> {
    run_python_suite("test_guarded_merge.py")
}

#[test]
fn checkout_observation_tests_pass() -> Result<(), String> {
    run_python_suite("test_checkout_observation.py")
}

#[test]
fn suite_report_rejects_skipped_and_empty_runs() -> Result<(), String> {
    require_complete_suite("fixture", "Ran 1 test in 0.001s\n\nOK\n")?;
    for report in [
        "Ran 1 test in 0.001s\n\nOK (skipped=1)\n",
        "OK\nRan 1 test in 0.001s\n\nOK (skipped=1)\n",
        "Ran 1 test in 0.001s\n\nOK (expected failures=1)\n",
        "Ran 0 tests in 0.001s\n\nOK\n",
        "Ran 1 test in 0.001s\nRan 1 test in 0.001s\n\nOK\n",
    ] {
        if require_complete_suite("fixture", report).is_ok() {
            return Err(format!("incomplete suite was accepted: {report}"));
        }
    }
    Ok(())
}
