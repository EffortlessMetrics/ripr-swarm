//! Keep the #13123 source-only Bash activation contract in ordinary workspace tests.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn sourced_rust_activation_preserves_identity_and_parent_shell() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = Command::new("bash")
        .arg(root.join("tools/test-activate-rust.sh"))
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("Bash must be available for Rust activation tests: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "Rust activation tests failed ({}):\n{stdout}{stderr}",
            output.status
        ));
    }
    let cases = stdout.lines().find_map(|line| {
        line.strip_prefix("Passed ")?
            .split_whitespace()
            .next()?
            .parse::<usize>()
            .ok()
    });
    // Case-transform and nameref controls additionally run when Bash supports them.
    if !cases.is_some_and(|count| count >= 35)
        || !stdout.contains("Bash activation cases with controlled tool fixtures.")
    {
        return Err(format!(
            "Rust activation harness did not report its behavioral controls:\n{stdout}{stderr}"
        ));
    }
    Ok(())
}
