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

/// Render `value` as one complete Bash argv token for advisory command text.
///
/// Non-empty `[A-Za-z0-9./_:-]` values remain readable without quotes. Every
/// other value uses Bash's single-quote form, with embedded single quotes
/// represented by the standard `'\''` splice. Single quotes are the only Bash
/// quoting form with no interpretation inside, so `$`, backticks, `!`,
/// backslashes, newlines, redirect characters, and empty values all round-trip
/// without expansion or token loss.
pub(crate) fn shell_arg(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '_' | '-' | ':'))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
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
        assert_eq!(shell_arg("repo root"), "'repo root'");
        assert_eq!(shell_arg("target/ripr/workflow"), "target/ripr/workflow");
        assert_eq!(
            workflow_artifact_path(Path::new("target/ripr/workflow"), "workflow.json"),
            "target/ripr/workflow/workflow.json"
        );
        assert_eq!(
            agent_seam_packets_command(".", "draft mode", "target/ripr/workflow/packets.json"),
            "ripr check --root . --mode 'draft mode' --format agent-seam-packets-json > target/ripr/workflow/packets.json"
        );
        assert_eq!(
            agent_start_command("repo root", "seam a", "target/ripr/work flow"),
            "ripr agent start --root 'repo root' --seam-id 'seam a' --out 'target/ripr/work flow'"
        );
        assert_eq!(
            check_repo_exposure_command("repo root", "draft", "target/ripr/work flow/before.json"),
            "ripr check --root 'repo root' --mode draft --format repo-exposure-json > 'target/ripr/work flow/before.json'"
        );
        assert_eq!(
            check_repo_exposure_command_with_base(
                "repo root",
                Some("origin/main with space"),
                "draft",
                "target/ripr/work flow/before.json",
            ),
            "ripr check --root 'repo root' --base 'origin/main with space' --mode draft --format repo-exposure-json > 'target/ripr/work flow/before.json'"
        );
        assert_eq!(
            agent_verify_command(
                "repo root",
                "target/ripr/work flow/before.json",
                "target/ripr/work flow/after.json",
                Some("target/ripr/work flow/verify.json"),
            ),
            "ripr agent verify --root 'repo root' --before 'target/ripr/work flow/before.json' --after 'target/ripr/work flow/after.json' --json > 'target/ripr/work flow/verify.json'"
        );
        assert_eq!(
            agent_receipt_command(
                "repo root",
                "target/ripr/work flow/verify.json",
                "seam a",
                Some("target/ripr/work flow/receipt.json"),
            ),
            "ripr agent receipt --root 'repo root' --verify-json 'target/ripr/work flow/verify.json' --seam-id 'seam a' --json --out 'target/ripr/work flow/receipt.json'"
        );
    }

    #[test]
    fn shell_arg_is_total_for_empty_backslash_quotes_and_metacharacters() {
        assert_eq!(shell_arg(""), "''");
        assert_eq!(shell_arg("plain/path:one-two"), "plain/path:one-two");
        assert_eq!(shell_arg("a\\b"), "'a\\b'");
        assert_eq!(shell_arg("a'b"), "'a'\\''b'");
        assert_eq!(shell_arg("$HOME"), "'$HOME'");
        assert_eq!(shell_arg("`id`"), "'`id`'");
        assert_eq!(shell_arg("with!bang"), "'with!bang'");
        assert_eq!(
            shell_arg("gap:pr:amount>=threshold"),
            "'gap:pr:amount>=threshold'"
        );
    }

    #[cfg(unix)]
    fn run_bash(
        script: &str,
        cwd: Option<&std::path::Path>,
    ) -> Result<std::process::Output, String> {
        let mut command = std::process::Command::new("bash");
        command
            .args(["--noprofile", "--norc", "-c", script])
            .env_remove("BASH_ENV")
            .env_remove("ENV");
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        command
            .output()
            .map_err(|err| format!("run Bash argv proof failed: {err}"))
    }

    #[cfg(unix)]
    #[test]
    fn shell_arg_round_trips_hostile_values_through_real_bash() -> Result<(), String> {
        for value in [
            "",
            "plain",
            "repo root",
            "a\\b",
            "a'b",
            "a\"b",
            "$HOME",
            "$(printf injected)",
            "`printf injected`",
            "with!bang",
            "gap:pr:amount>=threshold",
            "line\nbreak",
        ] {
            let script = format!("set -- {}; printf '%s' \"$1\"", shell_arg(value));
            let output = run_bash(&script, None)?;
            if !output.status.success() {
                return Err(format!(
                    "Bash rejected shell_arg({value:?}): {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            if output.stdout != value.as_bytes() {
                return Err(format!(
                    "shell_arg did not round-trip: input={value:?} output={:?} token={}",
                    String::from_utf8_lossy(&output.stdout),
                    shell_arg(value)
                ));
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn redirect_shaped_gap_id_cannot_open_a_second_redirect() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| format!("system clock before UNIX_EPOCH: {err}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-shell-arg-redirect-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|err| format!("create redirect proof root failed: {err}"))?;

        let proof = (|| {
            let value = "gap:pr:amount>=threshold";
            let script = format!("printf '%s' {} > result.txt", shell_arg(value));
            let output = run_bash(&script, Some(&root))?;
            if !output.status.success() {
                return Err(format!(
                    "Bash rejected redirect proof: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            let actual = std::fs::read(root.join("result.txt"))
                .map_err(|err| format!("read redirect proof result failed: {err}"))?;
            if actual != value.as_bytes() {
                return Err(format!(
                    "redirect proof changed the gap id: {:?}",
                    String::from_utf8_lossy(&actual)
                ));
            }
            let mut entries = std::fs::read_dir(&root)
                .map_err(|err| format!("read redirect proof root failed: {err}"))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .map_err(|err| format!("read redirect proof entry failed: {err}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            if entries != ["result.txt"] {
                return Err(format!(
                    "redirect-shaped gap id created unexpected files: {entries:?}"
                ));
            }
            Ok(())
        })();

        let cleanup = std::fs::remove_dir_all(&root)
            .map_err(|err| format!("remove redirect proof root failed: {err}"));
        match (proof, cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(cleanup_error)) => Err(cleanup_error),
            (Err(error), Err(cleanup_error)) => {
                Err(format!("{error}; cleanup also failed: {cleanup_error}"))
            }
        }
    }
}
