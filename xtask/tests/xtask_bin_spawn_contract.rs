//! Contract tests for the shared `CARGO_BIN_EXE_xtask` spawn helper (#4293).

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

#[test]
fn help_hierarchy_spawn_sites_use_the_shared_resilience_contract() -> Result<(), String> {
    let output = xtask_bin::output(|command| {
        command.args(["help", "--all"]);
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "xtask help --all failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
