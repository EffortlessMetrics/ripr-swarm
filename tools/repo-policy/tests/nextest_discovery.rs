//! Execute bounded discovery controls and the actual shard workflow with inert commands.
#![cfg(target_os = "linux")]

use std::path::Path;
use std::process::Command;

#[test]
fn nextest_discovery_command_entry_controls() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("/usr/bin/timeout")
        .args(["--kill-after=2s", "90s", "python3"])
        .arg(root.join("tools/ci/test_nextest_discovery.py"))
        .arg("-v")
        .current_dir(root)
        .output()?;
    assert!(
        output.status.success(),
        "nextest discovery controls failed: {:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
