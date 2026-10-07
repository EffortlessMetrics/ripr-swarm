pub(crate) mod artifact;
pub(crate) mod command_specs;
pub(crate) mod loop_commands;
pub(crate) mod provenance;

/// Trust-bound repair continuations require a fresh, explicit authorization.
pub(crate) const PYTHON_REPAIR_AUTHORIZATION_SUFFIX: &str =
    " --edit-authorized --edit-authority <operator-or-agent-identity>";

/// Cwd lock for `loop_commands` tests compiled as part of `ripr`.
///
/// The same file is also `#[path]`-included into xtask, where the parent
/// supplies `acquire_test_cwd_read_guard()`. Ripr unit tests share one
/// process with `set_current_dir` writers (`testing::cwd_lock`, #6965),
/// so this hook must hold that lock rather than a no-op (#7034).
#[cfg(test)]
pub(crate) fn loop_commands_cwd_read_guard() -> impl Drop {
    crate::testing::cwd_lock::hold_cwd()
}
