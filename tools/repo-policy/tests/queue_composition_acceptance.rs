//! Local component controls only: synthetic Git subjects are not native admission.
#![cfg(target_os = "linux")]

use std::process::Command;

#[test]
fn composed_changes_and_singleton_verifier_are_discriminated()
-> Result<(), Box<dyn std::error::Error>> {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/queue_composition_acceptance.py");
    let output = Command::new("timeout")
        .args(["--kill-after=1s", "180s", "python3", "-I"])
        .arg(script)
        .output()?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
