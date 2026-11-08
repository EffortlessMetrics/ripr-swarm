//! Runs the stdlib-Python fake-API tests of the exact-head merge guard
//! (#11781) so the checked-in helper has a Rust-owned oracle; nothing else in
//! CI executes `.agents/skills/finish-pr/tests/test_guarded_merge.py`.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn guarded_merge_fake_api_tests_pass() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let test = root.join(".agents/skills/finish-pr/tests/test_guarded_merge.py");
    let output = Command::new("python3")
        .arg("-I")
        .arg(&test)
        .output()
        .map_err(|error| format!("python3 must be available for the merge guard tests: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "guarded merge tests failed: {}{stderr}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    // unittest reports its summary on stderr; require real tests ran and passed.
    if !stderr.contains("OK") || stderr.contains("Ran 0 tests") {
        return Err(format!("guarded merge tests did not report OK: {stderr}"));
    }
    Ok(())
}
