//! Execute the Ubuntu shard-verifier workflow's actual launch step with inert
//! ZIP payloads and a fixture GitHub API. No hosted calls or workspace build.
#![cfg(target_os = "linux")]

use std::path::Path;
use std::process::Command;

#[test]
fn shard_verifier_artifact_command_entry_controls() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("/usr/bin/timeout")
        .args(["--kill-after=2s", "90s", "python3"])
        .arg(root.join("tools/ci/test_shard_verifier_artifact.py"))
        .arg("-v")
        .current_dir(root)
        .output()?;
    assert!(
        output.status.success(),
        "shard executable transport controls failed: {:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
