//! Offline controls for the unwired H-intake validator and Checks reader.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn queue_intake_controls_execute_both_nonempty_suites() -> Result<(), String> {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.github/actions/verify-queue-intake");
    for script in ["test_queue_intake.py", "test_checks_retrieval.py"] {
        let output = Command::new("python3")
            .arg(directory.join(script))
            .current_dir(&directory)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").ok_or("PATH missing")?)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .map_err(|error| format!("run {script}: {error}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        let count = stderr.lines().find_map(|line| {
            line.strip_prefix("Ran ")?
                .split_whitespace()
                .next()?
                .parse::<usize>()
                .ok()
        });
        if !output.status.success() || count.is_none_or(|value| value == 0) {
            return Err(format!("{script} failed or ran no tests: {stderr}"));
        }
    }
    Ok(())
}
