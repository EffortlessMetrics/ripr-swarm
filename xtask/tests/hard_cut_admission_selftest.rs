//! Runs the stdlib-Python self-tests of the pre-allocation admission action
//! (#5391) so the checked-in decision scripts have a Rust-owned oracle.

use std::path::PathBuf;
use std::process::Command;

fn run_self_test(script: &str, expected: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let path = root
        .join(".github/actions/pr-candidate-admission")
        .join(script);
    let output = Command::new("python3")
        .arg(&path)
        .arg("--self-test")
        .output()
        .expect("python3 must be available to run the admission self-tests");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{script} self-test failed: {stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains(expected),
        "{script} unexpected output: {stdout}"
    );
}

#[test]
fn candidate_admission_self_test_passes() {
    run_self_test("candidate_admission.py", "controls passed");
}

#[test]
fn release_cut_admission_self_test_passes() {
    run_self_test("release_cut_admission.py", "controls passed");
}
