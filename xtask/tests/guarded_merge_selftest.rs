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
    // unittest reports its summary on stderr; require real tests ran and passed.
    if !stderr.contains("OK") || stderr.contains("Ran 0 tests") {
        return Err(format!("{name} did not report OK: {stderr}"));
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
