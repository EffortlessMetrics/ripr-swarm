//! Keep cohort accounting and real Cargo oracle controls in the native test lane.
//! No model provider, network request, or paid cohort is invoked.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn agent_cohort_accounting_and_native_oracle_tests_pass() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let script = root.join("benchmarks/agentic/cohort/harness/selftest.py");
    let output = Command::new("python3")
        .arg("-I")
        .arg(&script)
        .arg("--require-native")
        .output()
        .map_err(|error| format!("python3 is required for the agent cohort selftest: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!("agent cohort selftest failed: {stdout}{stderr}"));
    }
    let summary = stdout
        .lines()
        .find_map(|line| line.strip_prefix("COHORT_SELFTEST "))
        .ok_or_else(|| format!("agent cohort selftest omitted its summary: {stdout}{stderr}"))?;
    let record: serde_json::Value = serde_json::from_str(summary)
        .map_err(|error| format!("invalid cohort selftest summary: {error}"))?;
    if record.get("tests").and_then(serde_json::Value::as_u64) == Some(0)
        || record.get("tests").and_then(serde_json::Value::as_u64).is_none()
        || record.get("native_required").and_then(serde_json::Value::as_bool) != Some(true)
        || ["failures", "errors", "skipped"]
            .iter()
            .any(|key| record.get(*key).and_then(serde_json::Value::as_u64) != Some(0))
    {
        return Err(format!("incomplete cohort selftest evidence: {summary}"));
    }
    Ok(())
}
