//! Canonical `ripr receipt write` command construction (RIPR-SPEC-0079).
//!
//! Lives in the ripr output layer rather than `agent::loop_commands` because it
//! is consumed only by ripr's gap-ledger and first-PR receipt fallbacks, not by
//! the agent-loop cockpit commands that `xtask`'s `operator.rs` `#[path]`-shares.
//!
//! The built string stays bash source (`shell_arg`); presentation surfaces that
//! offer a copy-paste form derive the PowerShell variant with the shared
//! `output::markdown::powershell_command` translation (#2628) rather than a
//! second quoting implementation.

use crate::agent::loop_commands::shell_arg_for;

/// Build the canonical `ripr receipt write` command per RIPR-SPEC-0079.
///
/// `receipt_command` fields MUST name the command that **writes** a receipt.
/// `ripr outcome` is a movement command and MUST NOT appear in that slot.
///
/// Parameters:
/// - `canonical_gap_id`: the gap's stable canonical identifier.
/// - `verify_command`: the best-available verification command for this gap.
/// - `out_path`: if `Some`, appended as `--out <path>`.
///
/// Status is always `not_run` for synthesized template commands — the agent
/// will update this to `passed`, `failed`, or `unknown` after running.
pub(crate) fn receipt_write_command(
    canonical_gap_id: &str,
    verify_command: &str,
    out_path: Option<&str>,
) -> String {
    // The canonical command is stored in versioned JSON records and
    // compared byte-for-byte across hosts, so it always uses the POSIX
    // quoting form (this module's "stays bash source" rule, #8437).
    // Presentation surfaces derive the host paste form from it.
    let base = format!(
        "ripr receipt write --gap {} --verify-command {} --status not_run",
        shell_arg_for(canonical_gap_id, false),
        shell_arg_for(verify_command, false),
    );
    match out_path {
        Some(path) => format!("{base} --out {}", shell_arg_for(path, false)),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::receipt_write_command;

    #[test]
    fn receipt_write_command_emits_canonical_form_with_out_path() {
        // RIPR-SPEC-0079: receipt_command must start with `ripr receipt write --gap`.
        // Host-aware quoting (#7729).
        assert_eq!(
            receipt_write_command(
                "gap:rust:pricing:discount:threshold-boundary",
                "cargo xtask fixtures boundary_gap",
                Some("target/ripr/receipts/gap-rust-pricing.json"),
            ),
            format!(
                "ripr receipt write --gap gap:rust:pricing:discount:threshold-boundary --verify-command {} --status not_run --out target/ripr/receipts/gap-rust-pricing.json",
                "'cargo xtask fixtures boundary_gap'"
            )
        );
    }

    #[test]
    fn receipt_write_command_emits_canonical_form_without_out_path() {
        assert_eq!(
            receipt_write_command(
                "gap:rust:pricing:discount:threshold-boundary",
                "cargo xtask fixtures boundary_gap",
                None,
            ),
            format!(
                "ripr receipt write --gap gap:rust:pricing:discount:threshold-boundary --verify-command {} --status not_run",
                "'cargo xtask fixtures boundary_gap'"
            )
        );
    }

    #[test]
    fn receipt_write_command_quotes_gap_id_with_special_chars() {
        // Gap IDs with `>` / `=` / spaces need shell quoting.
        let cmd = receipt_write_command(
            "gap:python:app/pricing.py:calculate_discount:predicate_boundary:amount>=threshold",
            "pytest tests/test_pricing.py::test_smoke",
            Some("target/ripr/receipts/gap.json"),
        );
        assert!(
            cmd.starts_with("ripr receipt write --gap "),
            "command must start with ripr receipt write --gap, got: {cmd}"
        );
        assert!(
            !cmd.contains("ripr outcome"),
            "receipt_write_command must never emit ripr outcome, got: {cmd}"
        );
    }

    #[test]
    fn receipt_write_command_quoting_is_host_independent() {
        // #8437: the canonical string is stored in versioned records, so
        // its quoting must not depend on the host shell.
        assert_eq!(
            receipt_write_command(
                "gap:rust:pricing:discount",
                "npx --no-install jest tests/pricing.test.ts",
                Some("target/ripr/receipts/gap.json"),
            ),
            "ripr receipt write --gap gap:rust:pricing:discount --verify-command 'npx --no-install jest tests/pricing.test.ts' --status not_run --out target/ripr/receipts/gap.json"
        );
    }

    #[test]
    fn receipt_write_command_status_is_always_not_run() {
        let cmd = receipt_write_command("gap:rust:some:gap", "cargo test -p ripr", None);
        assert!(
            cmd.contains("--status not_run"),
            "synthesized receipt command must use --status not_run, got: {cmd}"
        );
    }
}
