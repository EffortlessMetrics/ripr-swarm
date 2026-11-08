//! Keep the pre-compiler provisioning contract in ordinary Unix workspace tests.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn cloud_provisioning_handles_pin_updates_and_failure_boundaries() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = Command::new("node")
        .arg("--test")
        .arg("--test-reporter=tap")
        .arg(root.join("tools/provision-dev.test.cjs"))
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("Node must be available for provisioning controls: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "Provisioning controls failed ({}):\n{stdout}{stderr}",
            output.status
        ));
    }
    let count = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix)?.trim().parse::<usize>().ok())
    };
    if count("# tests ").is_none_or(|value| value == 0)
        || count("# pass ").is_none_or(|value| value == 0)
        || !stdout.lines().any(|line| line == "# fail 0")
        || (cfg!(target_os = "linux") && count("# skipped ") != Some(0))
    {
        return Err(format!(
            "Provisioning harness did not report nonempty executed controls:\n{stdout}{stderr}"
        ));
    }
    Ok(())
}
