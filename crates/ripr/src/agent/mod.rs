pub(crate) mod artifact;
pub(crate) mod command_specs;
pub(crate) mod loop_commands;
pub(crate) mod provenance;

/// Trust-bound repair continuations require a fresh, explicit authorization.
pub(crate) const PYTHON_REPAIR_AUTHORIZATION_SUFFIX: &str =
    " --edit-authorized --edit-authority <operator-or-agent-identity>";

/// Human-facing after-phase follow-up for an ordinary repair attempt (#7879).
///
/// The persisted manifest `next_command` stays the advanced
/// `ripr agent repair --attempt … --phase after` spelling so machine
/// drivers and trust-bound routes keep a canonical form. Ordinary human
/// next-step lines teach `ripr continue` instead, matching the façade
/// help. Trust-bound, store-qualified, and unparseable commands keep the
/// persisted spelling because `ripr continue` cannot carry those flags.
///
/// Lives here rather than `loop_commands` because that file is path-included
/// into unpublished xtask and must not grow ripr-only renderers.
pub(crate) fn human_after_phase_follow_up(persisted_next_command: &str) -> String {
    if after_phase_requires_advanced_spelling(persisted_next_command) {
        return persisted_next_command.to_string();
    }
    continue_from_agent_after(persisted_next_command)
        .unwrap_or_else(|| persisted_next_command.to_string())
}

fn after_phase_requires_advanced_spelling(command: &str) -> bool {
    command.contains("--edit-authorized")
        || command.contains("--edit-authority")
        || command.contains("--verify-authorized")
        || command.contains("--store")
}

fn continue_from_agent_after(persisted: &str) -> Option<String> {
    const PREFIX: &str = "ripr agent repair --root ";
    const PHASE: &str = " --phase after";
    const ATTEMPT: &str = " --attempt ";
    let rest = persisted.strip_prefix(PREFIX)?.strip_suffix(PHASE)?;
    let attempt_at = rest.find(ATTEMPT)?;
    let root = &rest[..attempt_at];
    let attempt = &rest[attempt_at + ATTEMPT.len()..];
    if root.is_empty() || attempt.is_empty() || attempt.contains(" --") {
        return None;
    }
    Some(format!("ripr continue --root {root} --attempt {attempt}"))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Ordinary after-phase follow-ups teach `ripr continue`; routes the
    /// façade cannot run keep the persisted advanced spelling (#7879).
    #[test]
    fn ordinary_after_phase_follow_up_names_ripr_continue() {
        assert_eq!(
            human_after_phase_follow_up(
                "ripr agent repair --root /tmp/probe --attempt repair-attempt-de699662a4338840692e0700 --phase after"
            ),
            "ripr continue --root /tmp/probe --attempt repair-attempt-de699662a4338840692e0700"
        );
        assert_eq!(
            human_after_phase_follow_up(
                "ripr agent repair --root 'repo root' --attempt 'abc def' --phase after"
            ),
            "ripr continue --root 'repo root' --attempt 'abc def'"
        );
        let trust_bound = format!(
            "ripr agent repair --root . --attempt repair-attempt-0123456789abcdef01234567 --phase after{PYTHON_REPAIR_AUTHORIZATION_SUFFIX}"
        );
        assert_eq!(human_after_phase_follow_up(&trust_bound), trust_bound);
        let store_bound = "ripr agent repair --root . --store /tmp/store --attempt repair-attempt-aa --phase after";
        assert_eq!(human_after_phase_follow_up(store_bound), store_bound);
        let incomplete = "ripr agent repair --phase after";
        assert_eq!(human_after_phase_follow_up(incomplete), incomplete);
    }
}
