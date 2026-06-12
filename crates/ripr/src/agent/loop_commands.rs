use std::path::Path;

pub(crate) const AGENT_LOOP_COMMAND_TEMPLATE_VERSION: &str = "0.1";

pub(crate) const WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT: &str =
    "target/ripr/workflow/before.repo-exposure.json";
pub(crate) const WORKFLOW_AFTER_SNAPSHOT_ARTIFACT: &str =
    "target/ripr/workflow/after.repo-exposure.json";
pub(crate) const WORKFLOW_MANIFEST_ARTIFACT: &str = "target/ripr/workflow/workflow.json";
pub(crate) const WORKFLOW_COMMANDS_MARKDOWN_ARTIFACT: &str = "target/ripr/workflow/commands.md";
pub(crate) const WORKFLOW_AGENT_SEAM_PACKETS_ARTIFACT: &str =
    "target/ripr/workflow/agent-seam-packets.json";
pub(crate) const WORKFLOW_AGENT_PACKET_ARTIFACT: &str = "target/ripr/workflow/agent-packet.json";
pub(crate) const WORKFLOW_AGENT_BRIEF_ARTIFACT: &str = "target/ripr/workflow/agent-brief.json";
pub(crate) const WORKFLOW_AGENT_VERIFY_ARTIFACT: &str = "target/ripr/workflow/agent-verify.json";
pub(crate) const WORKFLOW_AGENT_RECEIPT_ARTIFACT: &str = "target/ripr/reports/agent-receipt.json";

pub(crate) const PILOT_BEFORE_SNAPSHOT_ARTIFACT: &str = "target/ripr/pilot/repo-exposure.json";
pub(crate) const PILOT_AFTER_SNAPSHOT_ARTIFACT: &str = "target/ripr/pilot/after.repo-exposure.json";
pub(crate) const EDITOR_AGENT_PACKET_ARTIFACT: &str = "target/ripr/agent/agent-packet.json";
pub(crate) const EDITOR_AGENT_BRIEF_ARTIFACT: &str = "target/ripr/agent/agent-brief.json";
pub(crate) const EDITOR_AGENT_VERIFY_ARTIFACT: &str = "target/ripr/agent/agent-verify.json";
pub(crate) const EDITOR_AGENT_RECEIPT_ARTIFACT: &str = "target/ripr/agent/agent-receipt.json";

pub(crate) const WORKFLOW_AGENT_STATUS_ARTIFACT: &str = "target/ripr/workflow/agent-status.json";
pub(crate) const WORKFLOW_AGENT_STATUS_MARKDOWN_ARTIFACT: &str =
    "target/ripr/workflow/agent-status.md";
pub(crate) const WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT: &str =
    "target/ripr/workflow/agent-review-summary.json";
pub(crate) const WORKFLOW_AGENT_REVIEW_SUMMARY_MARKDOWN_ARTIFACT: &str =
    "target/ripr/workflow/agent-review-summary.md";

pub(crate) fn agent_start_command(root: &str, seam_id: &str, out_dir: &str) -> String {
    format!(
        "ripr agent start --root {} --seam-id {} --out {}",
        shell_arg(root),
        shell_arg(seam_id),
        shell_arg(out_dir)
    )
}

pub(crate) fn check_repo_exposure_command(root: &str, mode: &str, out_path: &str) -> String {
    check_repo_exposure_command_with_base(root, None, mode, out_path)
}

pub(crate) fn check_repo_exposure_command_with_base(
    root: &str,
    base: Option<&str>,
    mode: &str,
    out_path: &str,
) -> String {
    let base_arg = base
        .map(|base| format!(" --base {}", shell_arg(base)))
        .unwrap_or_default();
    format!(
        "ripr check --root {}{} --mode {} --format repo-exposure-json > {}",
        shell_arg(root),
        base_arg,
        shell_arg(mode),
        shell_arg(out_path)
    )
}

pub(crate) fn agent_seam_packets_command(root: &str, mode: &str, out_path: &str) -> String {
    format!(
        "ripr check --root {} --mode {} --format agent-seam-packets-json > {}",
        shell_arg(root),
        shell_arg(mode),
        shell_arg(out_path)
    )
}

pub(crate) fn agent_packet_command(root: &str, seam_id: &str, out_path: &str) -> String {
    format!(
        "ripr agent packet --root {} --seam-id {} --json > {}",
        shell_arg(root),
        shell_arg(seam_id),
        shell_arg(out_path)
    )
}

pub(crate) fn agent_brief_command(root: &str, seam_id: &str, out_path: &str) -> String {
    format!(
        "ripr agent brief --root {} --seam-id {} --json > {}",
        shell_arg(root),
        shell_arg(seam_id),
        shell_arg(out_path)
    )
}

pub(crate) fn agent_verify_command(
    root: &str,
    before_path: &str,
    after_path: &str,
    out_path: Option<&str>,
) -> String {
    let command = format!(
        "ripr agent verify --root {} --before {} --after {} --json",
        shell_arg(root),
        shell_arg(before_path),
        shell_arg(after_path)
    );
    append_redirect(command, out_path)
}

pub(crate) fn agent_receipt_command(
    root: &str,
    verify_json: &str,
    seam_id: &str,
    out_path: Option<&str>,
) -> String {
    let command = format!(
        "ripr agent receipt --root {} --verify-json {} --seam-id {} --json",
        shell_arg(root),
        shell_arg(verify_json),
        shell_arg(seam_id)
    );
    match out_path {
        Some(path) => format!("{command} --out {}", shell_arg(path)),
        None => command,
    }
}

pub(crate) fn agent_status_command(root: &str, out_path: Option<&str>) -> String {
    append_redirect(
        format!("ripr agent status --root {} --json", shell_arg(root)),
        out_path,
    )
}

pub(crate) fn agent_status_markdown_command(root: &str, out_path: Option<&str>) -> String {
    append_redirect(
        format!("ripr agent status --root {}", shell_arg(root)),
        out_path,
    )
}

pub(crate) fn agent_review_summary_command(root: &str, out_path: Option<&str>) -> String {
    append_redirect(
        format!(
            "ripr agent review-summary --root {} --json",
            shell_arg(root)
        ),
        out_path,
    )
}

pub(crate) fn agent_review_summary_markdown_command(root: &str, out_path: Option<&str>) -> String {
    append_redirect(
        format!("ripr agent review-summary --root {}", shell_arg(root)),
        out_path,
    )
}

pub(crate) fn outcome_command(
    before_path: &str,
    after_path: &str,
    out_path: Option<&str>,
) -> String {
    match out_path {
        Some(path) => {
            format!(
                "ripr outcome --before {} --after {} --format json --out {}",
                shell_arg(before_path),
                shell_arg(after_path),
                shell_arg(path)
            )
        }
        None => format!(
            "ripr outcome --before {} --after {}",
            shell_arg(before_path),
            shell_arg(after_path)
        ),
    }
}

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
    let base = format!(
        "ripr receipt write --gap {} --verify-command {} --status not_run",
        shell_arg(canonical_gap_id),
        shell_arg(verify_command),
    );
    match out_path {
        Some(path) => format!("{base} --out {}", shell_arg(path)),
        None => base,
    }
}

pub(crate) fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        ".".to_string()
    } else {
        text
    }
}

pub(crate) fn workflow_artifact_path(out_dir: &Path, file_name: &str) -> String {
    let out_dir = display_path(out_dir);
    if out_dir == "." {
        file_name.to_string()
    } else {
        format!("{}/{}", out_dir.trim_end_matches('/'), file_name)
    }
}

pub(crate) fn shell_path(path: &Path) -> String {
    shell_arg(&display_path(path))
}

pub(crate) fn shell_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '\\' | '_' | '-' | ':'))
    {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn append_redirect(command: String, out_path: Option<&str>) -> String {
    match out_path {
        Some(path) => format!("{command} > {}", shell_arg(path)),
        None => command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_commands_match_existing_status_templates() {
        assert_eq!(
            agent_start_command(".", "seam-a", "target/ripr/workflow"),
            "ripr agent start --root . --seam-id seam-a --out target/ripr/workflow"
        );
        assert_eq!(
            check_repo_exposure_command(".", "draft", WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT),
            "ripr check --root . --mode draft --format repo-exposure-json > target/ripr/workflow/before.repo-exposure.json"
        );
        assert_eq!(
            check_repo_exposure_command(".", "draft", WORKFLOW_AFTER_SNAPSHOT_ARTIFACT),
            "ripr check --root . --mode draft --format repo-exposure-json > target/ripr/workflow/after.repo-exposure.json"
        );
        assert_eq!(
            agent_packet_command(".", "seam-a", WORKFLOW_AGENT_PACKET_ARTIFACT),
            "ripr agent packet --root . --seam-id seam-a --json > target/ripr/workflow/agent-packet.json"
        );
        assert_eq!(
            agent_brief_command(".", "seam-a", WORKFLOW_AGENT_BRIEF_ARTIFACT),
            "ripr agent brief --root . --seam-id seam-a --json > target/ripr/workflow/agent-brief.json"
        );
        assert_eq!(
            agent_verify_command(
                ".",
                WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
                WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
                Some(WORKFLOW_AGENT_VERIFY_ARTIFACT),
            ),
            "ripr agent verify --root . --before target/ripr/workflow/before.repo-exposure.json --after target/ripr/workflow/after.repo-exposure.json --json > target/ripr/workflow/agent-verify.json"
        );
        assert_eq!(
            agent_receipt_command(
                ".",
                WORKFLOW_AGENT_VERIFY_ARTIFACT,
                "seam-a",
                Some(WORKFLOW_AGENT_RECEIPT_ARTIFACT),
            ),
            "ripr agent receipt --root . --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-a --json --out target/ripr/reports/agent-receipt.json"
        );
        assert_eq!(
            agent_status_command(".", Some(WORKFLOW_AGENT_STATUS_ARTIFACT)),
            "ripr agent status --root . --json > target/ripr/workflow/agent-status.json"
        );
        assert_eq!(
            agent_status_markdown_command(".", Some(WORKFLOW_AGENT_STATUS_MARKDOWN_ARTIFACT)),
            "ripr agent status --root . > target/ripr/workflow/agent-status.md"
        );
        assert_eq!(
            agent_review_summary_command(".", Some(WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT)),
            "ripr agent review-summary --root . --json > target/ripr/workflow/agent-review-summary.json"
        );
        assert_eq!(
            agent_review_summary_markdown_command(
                ".",
                Some(WORKFLOW_AGENT_REVIEW_SUMMARY_MARKDOWN_ARTIFACT),
            ),
            "ripr agent review-summary --root . > target/ripr/workflow/agent-review-summary.md"
        );
    }

    #[test]
    fn editor_commands_match_existing_lsp_templates() {
        assert_eq!(
            agent_packet_command(".", "seam-a", EDITOR_AGENT_PACKET_ARTIFACT),
            "ripr agent packet --root . --seam-id seam-a --json > target/ripr/agent/agent-packet.json"
        );
        assert_eq!(
            check_repo_exposure_command(".", "ready", PILOT_AFTER_SNAPSHOT_ARTIFACT),
            "ripr check --root . --mode ready --format repo-exposure-json > target/ripr/pilot/after.repo-exposure.json"
        );
        assert_eq!(
            agent_verify_command(
                ".",
                PILOT_BEFORE_SNAPSHOT_ARTIFACT,
                PILOT_AFTER_SNAPSHOT_ARTIFACT,
                Some(EDITOR_AGENT_VERIFY_ARTIFACT),
            ),
            "ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json > target/ripr/agent/agent-verify.json"
        );
    }

    #[test]
    fn command_args_quote_spaces_without_touching_plain_tokens() {
        assert_eq!(shell_arg("repo root"), "\"repo root\"");
        assert_eq!(shell_arg("target/ripr/workflow"), "target/ripr/workflow");
        assert_eq!(
            workflow_artifact_path(Path::new("target/ripr/workflow"), "workflow.json"),
            "target/ripr/workflow/workflow.json"
        );
        assert_eq!(
            agent_seam_packets_command(".", "draft mode", "target/ripr/workflow/packets.json"),
            "ripr check --root . --mode \"draft mode\" --format agent-seam-packets-json > target/ripr/workflow/packets.json"
        );
        assert_eq!(
            agent_start_command("repo root", "seam a", "target/ripr/work flow"),
            "ripr agent start --root \"repo root\" --seam-id \"seam a\" --out \"target/ripr/work flow\""
        );
        assert_eq!(
            check_repo_exposure_command("repo root", "draft", "target/ripr/work flow/before.json"),
            "ripr check --root \"repo root\" --mode draft --format repo-exposure-json > \"target/ripr/work flow/before.json\""
        );
        assert_eq!(
            check_repo_exposure_command_with_base(
                "repo root",
                Some("origin/main with space"),
                "draft",
                "target/ripr/work flow/before.json",
            ),
            "ripr check --root \"repo root\" --base \"origin/main with space\" --mode draft --format repo-exposure-json > \"target/ripr/work flow/before.json\""
        );
        assert_eq!(
            agent_verify_command(
                "repo root",
                "target/ripr/work flow/before.json",
                "target/ripr/work flow/after.json",
                Some("target/ripr/work flow/verify.json"),
            ),
            "ripr agent verify --root \"repo root\" --before \"target/ripr/work flow/before.json\" --after \"target/ripr/work flow/after.json\" --json > \"target/ripr/work flow/verify.json\""
        );
        assert_eq!(
            agent_receipt_command(
                "repo root",
                "target/ripr/work flow/verify.json",
                "seam a",
                Some("target/ripr/work flow/receipt.json"),
            ),
            "ripr agent receipt --root \"repo root\" --verify-json \"target/ripr/work flow/verify.json\" --seam-id \"seam a\" --json --out \"target/ripr/work flow/receipt.json\""
        );
    }

    #[test]
    fn receipt_write_command_emits_canonical_form_with_out_path() {
        // RIPR-SPEC-0079: receipt_command must start with `ripr receipt write --gap`.
        assert_eq!(
            receipt_write_command(
                "gap:rust:pricing:discount:threshold-boundary",
                "cargo xtask fixtures boundary_gap",
                Some("target/ripr/receipts/gap-rust-pricing.json"),
            ),
            "ripr receipt write --gap gap:rust:pricing:discount:threshold-boundary --verify-command \"cargo xtask fixtures boundary_gap\" --status not_run --out target/ripr/receipts/gap-rust-pricing.json"
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
            "ripr receipt write --gap gap:rust:pricing:discount:threshold-boundary --verify-command \"cargo xtask fixtures boundary_gap\" --status not_run"
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
    fn receipt_write_command_status_is_always_not_run() {
        let cmd = receipt_write_command("gap:rust:some:gap", "cargo test -p ripr", None);
        assert!(
            cmd.contains("--status not_run"),
            "synthesized receipt command must use --status not_run, got: {cmd}"
        );
    }
}
