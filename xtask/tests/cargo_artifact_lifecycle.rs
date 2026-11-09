//! Runs the stdlib-Python Cargo artifact lifecycle tests so the pre-build
//! cleanup boundary has a Rust-owned CI oracle (#13965).

use std::path::PathBuf;
use std::process::Command;

#[test]
fn cargo_artifact_lifecycle_tests_pass() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let test = root.join("tools/python/cargo-artifacts/test_run.py");
    let output = Command::new("python3")
        .arg("-I")
        .arg(&test)
        .output()
        .map_err(|error| {
            format!("python3 must be available for Cargo artifact lifecycle tests: {error}")
        })?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "Cargo artifact lifecycle tests failed: {}{stderr}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    if !stderr.contains("OK") || stderr.contains("Ran 0 tests") {
        return Err(format!(
            "Cargo artifact lifecycle tests did not report OK: {stderr}"
        ));
    }
    Ok(())
}
