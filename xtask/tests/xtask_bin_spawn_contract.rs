//! Contract tests for the shared `CARGO_BIN_EXE_xtask` spawn helper (#4293).

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

#[test]
fn help_hierarchy_spawn_sites_use_the_shared_resilience_contract() -> Result<(), String> {
    xtask_bin::output(|command| {
        command.args(["help", "--all"]);
    })?;
    Ok(())
}
