//! Runs the stdlib-Python self-tests of the pre-allocation admission action
//! (#5391) so the checked-in decision scripts have a Rust-owned oracle.

use std::path::PathBuf;
use std::process::Command;

fn run_self_test(script: &str, expected: &str) -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let path = root
        .join(".github/actions/pr-candidate-admission")
        .join(script);
    let output = Command::new("python3")
        .arg(&path)
        .arg("--self-test")
        .output()
        .map_err(|error| format!("python3 must be available for {script}: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        return Err(format!(
            "{script} self-test failed: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if !stdout.contains(expected) {
        return Err(format!("{script} unexpected output: {stdout}"));
    }
    Ok(())
}

#[test]
fn candidate_admission_self_test_passes() -> Result<(), String> {
    run_self_test("candidate_admission.py", "controls passed")
}

#[test]
fn release_cut_admission_self_test_passes() -> Result<(), String> {
    run_self_test("release_cut_admission.py", "controls passed")
}
