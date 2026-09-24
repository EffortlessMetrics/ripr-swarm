use crate::agent::loop_commands::{
    WORKFLOW_AFTER_SNAPSHOT_ARTIFACT, WORKFLOW_AGENT_BRIEF_ARTIFACT,
    WORKFLOW_AGENT_PACKET_ARTIFACT, WORKFLOW_AGENT_RECEIPT_ARTIFACT,
    WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT, WORKFLOW_AGENT_REVIEW_SUMMARY_MARKDOWN_ARTIFACT,
    WORKFLOW_AGENT_STATUS_ARTIFACT, WORKFLOW_AGENT_STATUS_MARKDOWN_ARTIFACT,
    WORKFLOW_AGENT_VERIFY_ARTIFACT, WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT,
    WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT, agent_brief_command, agent_packet_command,
    agent_receipt_command, agent_review_summary_command, agent_review_summary_markdown_command,
    agent_status_command, agent_status_markdown_command, agent_verify_command,
    check_analysis_outcome_command, check_repo_exposure_command, display_path, shell_arg,
};
use crate::app::repair_attempt::{
    REPAIR_ATTEMPT_DIRECTORY, RepairAttemptInventoryEntry, RepairAttemptManifest,
    RepairAttemptState, inventory_repair_attempts,
};
use crate::output::markdown::{COMMAND_SHELL_DISCLOSURE, powershell_command};
use serde_json::Value;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const AGENT_STATUS_SCHEMA_VERSION: &str = "0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AgentStatusArtifactDef {
    name: &'static str,
    label: &'static str,
    path: &'static str,
}

const ARTIFACTS: &[AgentStatusArtifactDef] = &[
    AgentStatusArtifactDef {
        name: "before_snapshot",
        label: "before snapshot",
        path: WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
    },
    AgentStatusArtifactDef {
        name: "after_snapshot",
        label: "after snapshot",
        path: WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
    },
    AgentStatusArtifactDef {
        name: "analysis_outcome",
        label: "analysis outcome",
        path: WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT,
    },
    AgentStatusArtifactDef {
        name: "agent_brief",
        label: "agent brief",
        path: WORKFLOW_AGENT_BRIEF_ARTIFACT,
    },
    AgentStatusArtifactDef {
        name: "agent_packet",
        label: "agent packet",
        path: WORKFLOW_AGENT_PACKET_ARTIFACT,
    },
    AgentStatusArtifactDef {
        name: "agent_verify",
        label: "agent verify",
        path: WORKFLOW_AGENT_VERIFY_ARTIFACT,
    },
    AgentStatusArtifactDef {
        name: "agent_receipt",
        label: "agent receipt",
        path: WORKFLOW_AGENT_RECEIPT_ARTIFACT,
    },
];

const MISSING_COMMAND_ORDER: &[&str] = &[
    "before_snapshot",
    "agent_packet",
    "agent_brief",
    "after_snapshot",
    "analysis_outcome",
    "agent_verify",
    "agent_receipt",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentStatusReport {
    pub(crate) root: String,
    pub(crate) seam: Option<AgentStatusSeam>,
    pub(crate) artifacts: Vec<AgentStatusArtifact>,
    pub(crate) repair_attempts: Vec<AgentStatusRepairAttempt>,
    pub(crate) missing_commands: Vec<AgentStatusCommand>,
    /// The one command status recommends, or `None` when it cannot choose
    /// honestly (the warnings then say why). Selection order is documented in
    /// RIPR-SPEC-0011: a current awaiting repair attempt first, then a new
    /// attempt for a seam whose attempts ended without a receipt, then the
    /// legacy artifact loop, which never emits a placeholder seam or a redirect
    /// into a directory that does not exist.
    pub(crate) next_command: Option<AgentStatusCommand>,
    pub(crate) warnings: Vec<AgentStatusWarning>,
}

/// One repair attempt as status sees it. `disposition` is status's reading of
/// the manifest against the current `HEAD`; `command` is what would move that
/// attempt forward, if anything.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentStatusRepairAttempt {
    pub(crate) attempt_id: String,
    pub(crate) seam_id: String,
    pub(crate) state: &'static str,
    pub(crate) head_current: Option<bool>,
    pub(crate) disposition: &'static str,
    pub(crate) manifest: String,
    pub(crate) command: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentStatusSeam {
    pub(crate) seam_id: String,
    pub(crate) source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentStatusArtifact {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) path: String,
    pub(crate) present: bool,
    pub(crate) bytes: Option<u64>,
    pub(crate) modified: Option<SystemTime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentStatusCommand {
    pub(crate) step: String,
    pub(crate) artifact: String,
    pub(crate) reason: String,
    pub(crate) command: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentStatusWarning {
    pub(crate) kind: String,
    pub(crate) artifact: String,
    pub(crate) message: String,
}

impl AgentStatusReport {
    pub(crate) fn status(&self) -> &'static str {
        if self.next_command.is_some() || self.artifacts.iter().any(|artifact| !artifact.present) {
            "incomplete"
        } else if self.warnings.is_empty() {
            "complete"
        } else {
            "warning"
        }
    }
}

pub(crate) fn build_agent_status_report(root: &Path, root_argument: &Path) -> AgentStatusReport {
    let root_display = display_path(root_argument);
    keep_follow_up_templates_reachable(&root_display);
    let artifacts = ARTIFACTS
        .iter()
        .map(|artifact| inspect_artifact(root, artifact))
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    let seam = recover_seam_id(root, &artifacts, &mut warnings);
    warnings.extend(stale_warnings(&artifacts));
    let missing_commands = missing_commands(root_argument, seam.as_ref(), &artifacts);
    let repair_attempts = inspect_repair_attempts(root, &root_display, &mut warnings);
    let next_command = select_next_command(
        root,
        &root_display,
        seam.as_ref(),
        repair_attempts.as_ref(),
        &missing_commands,
        &mut warnings,
    );

    AgentStatusReport {
        root: root_display,
        seam,
        artifacts,
        repair_attempts: repair_attempts.unwrap_or_default(),
        missing_commands,
        next_command,
        warnings,
    }
}

/// Reads every repair attempt through the attempt authority. `None` means the
/// inventory could not be trusted (an unreadable directory or manifest), in
/// which case a warning says so and status selects no command.
fn inspect_repair_attempts(
    root: &Path,
    root_display: &str,
    warnings: &mut Vec<AgentStatusWarning>,
) -> Option<Vec<AgentStatusRepairAttempt>> {
    let entries = match inventory_repair_attempts(root) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(AgentStatusWarning {
                kind: "repair_attempt_unreadable".to_string(),
                artifact: REPAIR_ATTEMPT_DIRECTORY.to_string(),
                message: format!("could not list repair attempts: {error}"),
            });
            return None;
        }
    };
    if entries.is_empty() {
        return Some(Vec::new());
    }
    let current_head = crate::agent::artifact::current_git_head(root).ok();
    let mut attempts = Vec::new();
    let mut trusted = true;
    for entry in entries {
        match entry {
            RepairAttemptInventoryEntry::Valid(manifest) => attempts.push(status_repair_attempt(
                root_display,
                &manifest,
                current_head.as_deref(),
            )),
            RepairAttemptInventoryEntry::Invalid { directory, error } => {
                trusted = false;
                warnings.push(AgentStatusWarning {
                    kind: "repair_attempt_unreadable".to_string(),
                    artifact: format!("{REPAIR_ATTEMPT_DIRECTORY}/{directory}/attempt.json"),
                    message: format!(
                        "repair attempt `{directory}` was refused ({error}); status selects no next command until it is repaired or removed"
                    ),
                });
            }
        }
    }
    trusted.then_some(attempts)
}

fn status_repair_attempt(
    root_display: &str,
    manifest: &RepairAttemptManifest,
    current_head: Option<&str>,
) -> AgentStatusRepairAttempt {
    let head_current = current_head.map(|head| head == manifest.repository_head);
    let restart = Some(new_repair_attempt_command(root_display, &manifest.seam_id));
    let (state, disposition, command) = match manifest.state {
        RepairAttemptState::AwaitingEdit => match head_current {
            Some(true) => (
                "awaiting_edit",
                "resumable",
                Some(manifest.next_command.clone()),
            ),
            Some(false) => ("awaiting_edit", "prepared_at_other_head", restart),
            None => ("awaiting_edit", "head_unknown", None),
        },
        RepairAttemptState::Prepared => ("prepared", "not_published", restart),
        RepairAttemptState::ReadyToFinish => ("ready_to_finish", "finished", None),
        RepairAttemptState::Stale => ("stale", "ended", restart),
        RepairAttemptState::Incomparable => ("incomparable", "ended", restart),
        RepairAttemptState::Failed => ("failed", "ended", restart),
    };
    AgentStatusRepairAttempt {
        attempt_id: manifest.repair_attempt_id.as_str().to_string(),
        seam_id: manifest.seam_id.clone(),
        state,
        head_current,
        disposition,
        manifest: format!(
            "{REPAIR_ATTEMPT_DIRECTORY}/{}/attempt.json",
            manifest.repair_attempt_id.as_str()
        ),
        command,
    }
}

/// The documented start of the repair transaction. It creates every artifact
/// it needs, so it never depends on a workflow directory already existing.
fn new_repair_attempt_command(root_display: &str, seam_id: &str) -> String {
    format!(
        "ripr agent repair --root {} --seam-id {} --phase before",
        shell_arg(root_display),
        shell_arg(seam_id)
    )
}

fn select_next_command(
    root: &Path,
    root_display: &str,
    seam: Option<&AgentStatusSeam>,
    repair_attempts: Option<&Vec<AgentStatusRepairAttempt>>,
    missing_commands: &[AgentStatusCommand],
    warnings: &mut Vec<AgentStatusWarning>,
) -> Option<AgentStatusCommand> {
    // An inventory status could not read is not an empty one: choosing a
    // command past it could resume or restart the wrong transaction.
    let attempts = repair_attempts?;

    let resumable = attempts
        .iter()
        .filter(|attempt| attempt.disposition == "resumable")
        .collect::<Vec<_>>();
    match resumable.as_slice() {
        [attempt] => {
            return Some(AgentStatusCommand {
                step: "repair_attempt_after".to_string(),
                artifact: attempt.manifest.clone(),
                reason: format!(
                    "repair attempt `{}` for seam `{}` is awaiting the focused test edit; once the test is in place, run the after phase its before phase recorded",
                    attempt.attempt_id, attempt.seam_id
                ),
                command: attempt.command.clone().unwrap_or_default(),
            });
        }
        [] => {}
        several => {
            warnings.push(AgentStatusWarning {
                kind: "ambiguous_repair_attempts".to_string(),
                artifact: REPAIR_ATTEMPT_DIRECTORY.to_string(),
                message: format!(
                    "{} repair attempts are awaiting an edit at the current HEAD; status does not choose between them. Run the after phase of the attempt you edited: {}",
                    several.len(),
                    attempt_command_list(several)
                ),
            });
            return None;
        }
    }

    let head_unknown = attempts
        .iter()
        .filter(|attempt| attempt.disposition == "head_unknown")
        .collect::<Vec<_>>();
    if !head_unknown.is_empty() {
        warnings.push(AgentStatusWarning {
            kind: "repair_attempt_head_unknown".to_string(),
            artifact: REPAIR_ATTEMPT_DIRECTORY.to_string(),
            message: format!(
                "the current Git HEAD could not be read, so status cannot tell whether {} awaiting repair attempt(s) are still current",
                head_unknown.len()
            ),
        });
        return None;
    }

    // A seam is open when none of its attempts finished and at least one ended
    // or went stale. Grouping by seam rather than picking "the latest" attempt
    // keeps status from reading creation order into the inventory.
    let finished_seams = attempts
        .iter()
        .filter(|attempt| attempt.disposition == "finished")
        .map(|attempt| attempt.seam_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut open = std::collections::BTreeMap::<&str, Vec<&AgentStatusRepairAttempt>>::new();
    for attempt in attempts {
        if attempt.command.is_some() && !finished_seams.contains(attempt.seam_id.as_str()) {
            open.entry(attempt.seam_id.as_str())
                .or_default()
                .push(attempt);
        }
    }
    let mut open_seams = open.into_iter();
    match (open_seams.next(), open_seams.next()) {
        (Some((seam_id, ended)), None) => {
            return Some(AgentStatusCommand {
                step: "repair_attempt_before".to_string(),
                artifact: REPAIR_ATTEMPT_DIRECTORY.to_string(),
                reason: format!(
                    "no repair attempt for seam `{seam_id}` can continue ({}); start a new attempt",
                    ended
                        .iter()
                        .map(|attempt| format!(
                            "`{}` is {}",
                            attempt.attempt_id,
                            attempt_condition(attempt)
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                command: new_repair_attempt_command(root_display, seam_id),
            });
        }
        (Some((first, _)), Some((second, _))) => {
            let mut seams = vec![first, second];
            seams.extend(open_seams.map(|(seam_id, _)| seam_id));
            warnings.push(AgentStatusWarning {
                kind: "multiple_open_repair_seams".to_string(),
                artifact: REPAIR_ATTEMPT_DIRECTORY.to_string(),
                message: format!(
                    "repair attempts for {} seams ended without a receipt; status does not choose between them. Start a new attempt for the seam you mean: {}",
                    seams.len(),
                    seams
                        .iter()
                        .map(|seam_id| format!("`{}`", new_repair_attempt_command(root_display, seam_id)))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            });
            return None;
        }
        (None, _) => {}
    }

    legacy_next_command(root, root_display, seam, missing_commands)
}

/// Where `ripr pilot` writes its summary by default, relative to the root.
const PILOT_SUMMARY_ARTIFACT: &str = "target/ripr/pilot/pilot-summary.json";

/// The repair start `ripr pilot` recorded for its top seam (#3906), carried
/// verbatim. Pilot fills `next.repair_command` only past the repair-packet
/// flip, so status repeats that decision instead of re-deriving it. A missing,
/// unreadable, `null`, or non-repair value leaves status on `select_seam`.
fn pilot_repair_command(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(PILOT_SUMMARY_ARTIFACT)).ok()?;
    let summary = serde_json::from_str::<Value>(&text).ok()?;
    summary
        .pointer("/next/repair_command")
        .and_then(Value::as_str)
        .filter(|command| command.starts_with("ripr agent repair "))
        .map(str::to_string)
}

/// The legacy seven-artifact loop, kept for `agent start` and manual users,
/// with two refusals: it never recommends a command that needs a seam status
/// does not know, and never a redirect into a directory that does not exist.
fn legacy_next_command(
    root: &Path,
    root_display: &str,
    seam: Option<&AgentStatusSeam>,
    missing_commands: &[AgentStatusCommand],
) -> Option<AgentStatusCommand> {
    let first = missing_commands.first()?;
    let Some(seam) = seam else {
        if let Some(command) = pilot_repair_command(root) {
            return Some(AgentStatusCommand {
                step: "repair_attempt_before".to_string(),
                artifact: PILOT_SUMMARY_ARTIFACT.to_string(),
                reason: "`ripr pilot` selected a seam the repair transaction can target; start its repair attempt".to_string(),
                command,
            });
        }
        return Some(AgentStatusCommand {
            step: "select_seam".to_string(),
            artifact: "target/ripr/pilot".to_string(),
            reason: "no repair seam is known yet; `ripr pilot` inspects the workspace and selects the seam to repair".to_string(),
            command: format!("ripr pilot --root {}", shell_arg(root_display)),
        });
    };
    let target_directory_exists = Path::new(&first.artifact)
        .parent()
        .is_none_or(|parent| root.join(parent).is_dir());
    if !target_directory_exists {
        return Some(AgentStatusCommand {
            step: "repair_attempt_before".to_string(),
            artifact: REPAIR_ATTEMPT_DIRECTORY.to_string(),
            reason: format!(
                "{}; start a repair attempt for seam `{}`, which writes the workflow artifacts itself",
                first.reason, seam.seam_id
            ),
            command: new_repair_attempt_command(root_display, &seam.seam_id),
        });
    }
    Some(first.clone())
}

fn attempt_condition(attempt: &AgentStatusRepairAttempt) -> String {
    match attempt.disposition {
        "prepared_at_other_head" => {
            "awaiting an edit but was prepared at a different HEAD".to_string()
        }
        "not_published" => "prepared but never published".to_string(),
        _ => attempt.state.to_string(),
    }
}

fn attempt_command_list(attempts: &[&AgentStatusRepairAttempt]) -> String {
    attempts
        .iter()
        .map(|attempt| {
            format!(
                "`{}` (seam `{}`): `{}`",
                attempt.attempt_id,
                attempt.seam_id,
                attempt.command.as_deref().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn keep_follow_up_templates_reachable(root: &str) {
    drop((
        agent_status_command(root, Some(WORKFLOW_AGENT_STATUS_ARTIFACT)),
        agent_status_markdown_command(root, Some(WORKFLOW_AGENT_STATUS_MARKDOWN_ARTIFACT)),
        agent_review_summary_command(root, Some(WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT)),
        agent_review_summary_markdown_command(
            root,
            Some(WORKFLOW_AGENT_REVIEW_SUMMARY_MARKDOWN_ARTIFACT),
        ),
    ));
}

pub(crate) fn render_agent_status_json(report: &AgentStatusReport) -> Result<String, String> {
    let next_command = report.next_command.as_ref().map(agent_status_command_json);
    let value = serde_json::json!({
        "schema_version": AGENT_STATUS_SCHEMA_VERSION,
        "tool": "ripr",
        "status": report.status(),
        "root": report.root,
        "seam": report.seam.as_ref().map(agent_status_seam_json),
        "artifacts": report.artifacts.iter().map(agent_status_artifact_json).collect::<Vec<_>>(),
        "repair_attempts": report.repair_attempts.iter().map(agent_status_repair_attempt_json).collect::<Vec<_>>(),
        "missing_commands": report.missing_commands.iter().map(agent_status_command_json).collect::<Vec<_>>(),
        "next_command": next_command,
        "warnings": report.warnings.iter().map(agent_status_warning_json).collect::<Vec<_>>()
    });
    serde_json::to_string_pretty(&value)
        .map(|mut rendered| {
            rendered.push('\n');
            rendered
        })
        .map_err(|err| format!("failed to render agent status JSON: {err}"))
}

pub(crate) fn render_agent_status_markdown(report: &AgentStatusReport) -> String {
    let mut rendered = String::new();
    rendered.push_str("# RIPR Agent Status\n\n");
    rendered.push_str(&format!("Status: {}\n", report.status()));
    rendered.push_str(&format!("Root: {}\n", report.root));
    match &report.seam {
        Some(seam) => rendered.push_str(&format!("Seam: {} ({})\n", seam.seam_id, seam.source)),
        None => rendered.push_str("Seam: unknown\n"),
    }

    rendered.push_str("\n## Artifacts\n\n");
    rendered.push_str("| Artifact | State | Path |\n");
    rendered.push_str("| --- | --- | --- |\n");
    for artifact in &report.artifacts {
        let state = if artifact.present {
            "present"
        } else {
            "missing"
        };
        rendered.push_str(&format!(
            "| {} | {} | `{}` |\n",
            artifact.label, state, artifact.path
        ));
    }

    if !report.repair_attempts.is_empty() {
        rendered.push_str("\n## Repair Attempts\n\n");
        rendered.push_str("| Attempt | Seam | State | Status reading |\n");
        rendered.push_str("| --- | --- | --- | --- |\n");
        for attempt in &report.repair_attempts {
            rendered.push_str(&format!(
                "| `{}` | `{}` | {} | {} |\n",
                attempt.attempt_id, attempt.seam_id, attempt.state, attempt.disposition
            ));
        }
    }

    if let Some(next) = &report.next_command {
        rendered.push_str("\n## Next Command\n\n");
        rendered.push_str(&format!("{}\n\n", next.reason));
        rendered.push_str(COMMAND_SHELL_DISCLOSURE);
        rendered.push_str("```bash\n");
        rendered.push_str(&next.command);
        rendered.push_str("\n```\n");
        match powershell_command(&next.command) {
            Some(line) => {
                rendered.push_str("\n```powershell\n");
                rendered.push_str(&line);
                rendered.push_str("\n```\n");
            }
            None => rendered.push_str(&format!(
                "{}: `{}`\n",
                crate::output::markdown::POWERSHELL_UNAVAILABLE_DISCLOSURE,
                next.command
            )),
        }
    } else if report.missing_commands.is_empty() {
        rendered.push_str("\nNo missing agent-loop artifacts were detected.\n");
    } else {
        rendered.push_str("\n## Next Command\n\nStatus selects no next command; the warnings below say why and list the choices.\n");
    }

    if !report.warnings.is_empty() {
        rendered.push_str("\n## Warnings\n\n");
        for warning in &report.warnings {
            rendered.push_str(&format!(
                "- {}: {} (`{}`)\n",
                warning.kind, warning.message, warning.artifact
            ));
        }
    }

    rendered.push_str("\n## Limits\n\n");
    rendered.push_str("- Reads existing artifacts only.\n");
    rendered.push_str("- No repo analysis is run by this command.\n");
    rendered.push_str("- No runtime mutation execution.\n");
    rendered.push_str("- No automatic source edits.\n");
    rendered.push_str("- No generated tests.\n");
    rendered
}

fn agent_status_seam_json(seam: &AgentStatusSeam) -> Value {
    serde_json::json!({
        "seam_id": seam.seam_id,
        "source": seam.source
    })
}

fn agent_status_artifact_json(artifact: &AgentStatusArtifact) -> Value {
    serde_json::json!({
        "name": artifact.name,
        "label": artifact.label,
        "path": artifact.path,
        "required": true,
        "state": if artifact.present { "present" } else { "missing" },
        "bytes": artifact.bytes,
        "modified_unix_ms": modified_unix_ms(artifact.modified)
    })
}

fn agent_status_repair_attempt_json(attempt: &AgentStatusRepairAttempt) -> Value {
    serde_json::json!({
        "attempt_id": attempt.attempt_id,
        "seam_id": attempt.seam_id,
        "state": attempt.state,
        "head_current": attempt.head_current,
        "disposition": attempt.disposition,
        "manifest": attempt.manifest,
        "command": attempt.command
    })
}

fn agent_status_command_json(command: &AgentStatusCommand) -> Value {
    serde_json::json!({
        "step": command.step,
        "artifact": command.artifact,
        "reason": command.reason,
        "command": command.command
    })
}

fn agent_status_warning_json(warning: &AgentStatusWarning) -> Value {
    serde_json::json!({
        "kind": warning.kind,
        "artifact": warning.artifact,
        "message": warning.message
    })
}

fn inspect_artifact(root: &Path, artifact: &AgentStatusArtifactDef) -> AgentStatusArtifact {
    let path = root.join(artifact.path);
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => AgentStatusArtifact {
            name: artifact.name.to_string(),
            label: artifact.label.to_string(),
            path: artifact.path.to_string(),
            present: true,
            bytes: Some(metadata.len()),
            modified: metadata.modified().ok(),
        },
        _ => AgentStatusArtifact {
            name: artifact.name.to_string(),
            label: artifact.label.to_string(),
            path: artifact.path.to_string(),
            present: false,
            bytes: None,
            modified: None,
        },
    }
}

fn recover_seam_id(
    root: &Path,
    artifacts: &[AgentStatusArtifact],
    warnings: &mut Vec<AgentStatusWarning>,
) -> Option<AgentStatusSeam> {
    for (artifact_name, source) in [
        ("agent_receipt", "agent_receipt"),
        ("agent_verify", "agent_verify"),
        ("agent_packet", "agent_packet"),
        ("agent_brief", "agent_brief"),
    ] {
        let Some(artifact) = artifact_by_name(artifacts, artifact_name).filter(|a| a.present)
        else {
            continue;
        };
        let path = root.join(&artifact.path);
        let Some(value) = read_json_artifact(&path, artifact, warnings) else {
            continue;
        };
        if let Some(seam_id) = seam_id_from_source(&value, source) {
            return Some(AgentStatusSeam {
                seam_id,
                source: source.to_string(),
            });
        }
    }
    None
}

fn read_json_artifact(
    path: &Path,
    artifact: &AgentStatusArtifact,
    warnings: &mut Vec<AgentStatusWarning>,
) -> Option<Value> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            warnings.push(AgentStatusWarning {
                kind: "artifact_json_unreadable".to_string(),
                artifact: artifact.name.clone(),
                message: format!("could not read {}: {err}", artifact.path),
            });
            return None;
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => Some(value),
        Err(err) => {
            warnings.push(AgentStatusWarning {
                kind: "artifact_json_unreadable".to_string(),
                artifact: artifact.name.clone(),
                message: format!("could not parse {} as JSON: {err}", artifact.path),
            });
            None
        }
    }
}

fn seam_id_from_source(value: &Value, source: &str) -> Option<String> {
    match source {
        "agent_receipt" => value
            .get("seam")
            .and_then(|seam| string_field(seam, "seam_id")),
        "agent_verify" => seam_id_from_verify(value),
        "agent_packet" => value
            .get("packets")
            .and_then(Value::as_array)
            .and_then(|packets| packets.first())
            .and_then(|packet| string_field(packet, "seam_id")),
        "agent_brief" => value
            .get("working_set")
            .and_then(|working_set| string_field(working_set, "seam_id"))
            .or_else(|| {
                value
                    .get("top_seams")
                    .and_then(Value::as_array)
                    .and_then(|seams| seams.first())
                    .and_then(|seam| string_field(seam, "seam_id"))
            }),
        _ => None,
    }
}

fn seam_id_from_verify(value: &Value) -> Option<String> {
    for bucket in [
        "changed_seams",
        "unchanged_seams",
        "new_gaps",
        "resolved_gaps",
    ] {
        let Some(seam_id) = value
            .get(bucket)
            .and_then(Value::as_array)
            .and_then(|seams| seams.first())
            .and_then(|seam| string_field(seam, "seam_id"))
        else {
            continue;
        };
        return Some(seam_id);
    }
    None
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn stale_warnings(artifacts: &[AgentStatusArtifact]) -> Vec<AgentStatusWarning> {
    let mut warnings = Vec::new();
    push_snapshot_order_warning(artifacts, &mut warnings);
    push_stale_warning(
        artifacts,
        "agent_verify",
        "before_snapshot",
        "agent verify is older than the before snapshot; rerun verify after refreshing snapshots",
        &mut warnings,
    );
    push_stale_warning(
        artifacts,
        "agent_verify",
        "after_snapshot",
        "agent verify is older than the after snapshot; rerun verify after refreshing snapshots",
        &mut warnings,
    );
    push_stale_warning(
        artifacts,
        "agent_receipt",
        "agent_verify",
        "agent receipt is older than agent verify; rerun receipt after refreshing verify",
        &mut warnings,
    );
    warnings
}

fn push_snapshot_order_warning(
    artifacts: &[AgentStatusArtifact],
    warnings: &mut Vec<AgentStatusWarning>,
) {
    let Some(before) = artifact_by_name(artifacts, "before_snapshot").filter(|a| a.present) else {
        return;
    };
    let Some(after) = artifact_by_name(artifacts, "after_snapshot").filter(|a| a.present) else {
        return;
    };
    let (Some(before_modified), Some(after_modified)) = (before.modified, after.modified) else {
        return;
    };
    if before_modified > after_modified {
        warnings.push(AgentStatusWarning {
            kind: "stale_artifact".to_string(),
            artifact: before.name.clone(),
            message: "before snapshot is newer than after snapshot; the before artifact may have been overwritten after editing".to_string(),
        });
    }
}

fn push_stale_warning(
    artifacts: &[AgentStatusArtifact],
    stale_candidate: &str,
    newer_input: &str,
    message: &str,
    warnings: &mut Vec<AgentStatusWarning>,
) {
    let Some(candidate) = artifact_by_name(artifacts, stale_candidate).filter(|a| a.present) else {
        return;
    };
    let Some(input) = artifact_by_name(artifacts, newer_input).filter(|a| a.present) else {
        return;
    };
    let (Some(candidate_modified), Some(input_modified)) = (candidate.modified, input.modified)
    else {
        return;
    };
    if candidate_modified.duration_since(input_modified).is_err() {
        warnings.push(AgentStatusWarning {
            kind: "stale_artifact".to_string(),
            artifact: candidate.name.clone(),
            message: message.to_string(),
        });
    }
}

fn missing_commands(
    root_argument: &Path,
    seam: Option<&AgentStatusSeam>,
    artifacts: &[AgentStatusArtifact],
) -> Vec<AgentStatusCommand> {
    let mut commands = Vec::new();
    for artifact_name in MISSING_COMMAND_ORDER {
        let Some(artifact) = artifact_by_name(artifacts, artifact_name) else {
            continue;
        };
        if artifact.present {
            continue;
        }
        commands.push(AgentStatusCommand {
            step: artifact.name.clone(),
            artifact: artifact.path.clone(),
            reason: format!("{} artifact is missing", artifact.label),
            command: command_for_missing_artifact(root_argument, seam, artifact),
        });
    }
    commands
}

fn command_for_missing_artifact(
    root_argument: &Path,
    seam: Option<&AgentStatusSeam>,
    artifact: &AgentStatusArtifact,
) -> String {
    let root = display_path(root_argument);
    let seam_id = seam
        .map(|seam| seam.seam_id.as_str())
        .unwrap_or("<seam-id>");
    match artifact.name.as_str() {
        "before_snapshot" => {
            check_repo_exposure_command(&root, "draft", WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT)
        }
        "after_snapshot" => {
            check_repo_exposure_command(&root, "draft", WORKFLOW_AFTER_SNAPSHOT_ARTIFACT)
        }
        "analysis_outcome" => {
            check_analysis_outcome_command(&root, "draft", WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT)
        }
        "agent_packet" => agent_packet_command(&root, seam_id, WORKFLOW_AGENT_PACKET_ARTIFACT),
        "agent_brief" => agent_brief_command(&root, seam_id, WORKFLOW_AGENT_BRIEF_ARTIFACT),
        "agent_verify" => agent_verify_command(
            &root,
            WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
            WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
            Some(WORKFLOW_AGENT_VERIFY_ARTIFACT),
        ),
        "agent_receipt" => agent_receipt_command(
            &root,
            WORKFLOW_AGENT_VERIFY_ARTIFACT,
            seam_id,
            Some(WORKFLOW_AGENT_RECEIPT_ARTIFACT),
        ),
        _ => String::new(),
    }
}

fn artifact_by_name<'a>(
    artifacts: &'a [AgentStatusArtifact],
    name: &str,
) -> Option<&'a AgentStatusArtifact> {
    artifacts.iter().find(|artifact| artifact.name == name)
}

fn modified_unix_ms(time: Option<SystemTime>) -> Option<u64> {
    let millis = time?.duration_since(UNIX_EPOCH).ok()?.as_millis();
    u64::try_from(millis).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn unique_agent_status_test_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "ripr-agent-status-{label}-{}-{stamp}",
            std::process::id()
        ))
    }

    fn write_file(path: &Path, text: &str) -> Result<(), String> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|err| format!("create parent: {err}"))?;
        }
        std::fs::write(path, text).map_err(|err| format!("write {}: {err}", path.display()))
    }

    fn artifact(name: &str, present: bool, modified: Option<SystemTime>) -> AgentStatusArtifact {
        AgentStatusArtifact {
            name: name.to_string(),
            label: name.replace('_', " "),
            path: format!("target/ripr/{name}.json"),
            present,
            bytes: Some(1),
            modified,
        }
    }

    #[test]
    fn agent_status_reports_missing_artifacts_and_next_commands() -> Result<(), String> {
        let root = unique_agent_status_test_dir("missing");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;

        let report = build_agent_status_report(&root, Path::new("."));
        let rendered = render_agent_status_json(&report)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse status JSON: {err}"))?;

        assert_eq!(value["schema_version"], AGENT_STATUS_SCHEMA_VERSION);
        assert_eq!(value["status"], "incomplete");
        assert_eq!(value["seam"], Value::Null);
        assert_eq!(value["artifacts"].as_array().map(Vec::len), Some(7));
        assert_eq!(value["missing_commands"].as_array().map(Vec::len), Some(7));
        assert_eq!(value["repair_attempts"], serde_json::json!([]));
        // A fresh workspace knows no seam and has no workflow directory, so the
        // next command is the product route that selects a seam, not a
        // redirect into `target/ripr/workflow/` (#3906).
        assert_eq!(value["next_command"]["step"], "select_seam");
        assert_eq!(value["next_command"]["command"], "ripr pilot --root .");

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn agent_status_reports_complete_when_all_artifacts_are_present() -> Result<(), String> {
        let root = unique_agent_status_test_dir("complete");
        write_file(&root.join(WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT), "{}")?;
        write_file(&root.join(WORKFLOW_AFTER_SNAPSHOT_ARTIFACT), "{}")?;
        write_file(&root.join(WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT), "{}")?;
        write_file(&root.join(WORKFLOW_AGENT_BRIEF_ARTIFACT), "{}")?;
        write_file(&root.join(WORKFLOW_AGENT_PACKET_ARTIFACT), "{}")?;
        write_file(
            &root.join(WORKFLOW_AGENT_VERIFY_ARTIFACT),
            r#"{
  "changed_seams": [],
  "unchanged_seams": [{"seam_id": "from-verify"}],
  "new_gaps": [],
  "resolved_gaps": []
}"#,
        )?;
        write_file(
            &root.join(WORKFLOW_AGENT_RECEIPT_ARTIFACT),
            r#"{"seam":{"seam_id":"from-receipt"}}"#,
        )?;

        let report = build_agent_status_report(&root, Path::new("."));
        let rendered = render_agent_status_json(&report)?;
        let value: Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse status JSON: {err}"))?;

        assert_eq!(value["status"], "complete");
        assert_eq!(value["seam"]["seam_id"], "from-receipt");
        assert_eq!(value["seam"]["source"], "agent_receipt");
        assert_eq!(value["missing_commands"].as_array().map(Vec::len), Some(0));
        assert_eq!(value["next_command"], Value::Null);
        assert_eq!(
            value["artifacts"][0]["state"],
            serde_json::Value::String("present".to_string())
        );
        assert!(value["artifacts"][0]["bytes"].as_u64().is_some());
        assert!(value["artifacts"][0]["modified_unix_ms"].as_u64().is_some());

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn agent_status_markdown_names_next_command_and_limits() -> Result<(), String> {
        let root = unique_agent_status_test_dir("markdown");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;

        let report = build_agent_status_report(&root, Path::new("."));
        let rendered = render_agent_status_markdown(&report);

        assert!(rendered.contains("# RIPR Agent Status"));
        assert!(rendered.contains("Status: incomplete"));
        assert!(rendered.contains("| before snapshot | missing |"));
        assert!(rendered.contains("## Next Command"));
        assert!(rendered.contains("ripr pilot --root ."));
        assert!(rendered.contains("No runtime mutation execution."));
        assert!(rendered.contains("No generated tests."));

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    /// The agent-status Next Command block must offer both shells (#2628): the
    /// bash fence stays byte-identical, the PowerShell fence derives through
    /// the shared `powershell_command` translation (redirect becomes a UTF-8
    /// .NET write; the quoted root survives as a single-quoted literal), and
    /// the cmd.exe boundary is stated.
    #[test]
    fn agent_status_markdown_next_command_offers_powershell_variant() -> Result<(), String> {
        let root = unique_agent_status_test_dir("markdown-powershell");
        // A known seam and an existing workflow directory keep the legacy
        // redirect route selected, which is the translation this pins.
        write_file(
            &root.join(WORKFLOW_AGENT_PACKET_ARTIFACT),
            r#"{"packets":[{"seam_id":"seam-a"}]}"#,
        )?;

        let report = build_agent_status_report(&root, Path::new("repo root"));
        let rendered = render_agent_status_markdown(&report);

        // Issue #3872: the next-command redirect anchors at the resolved
        // --root, so both presented forms build from the same builder output
        // (the anchor math itself is pinned in loop_commands tests).
        let next =
            check_repo_exposure_command("repo root", "draft", WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT);
        let bash_form = format!("```bash\n{next}\n```\n");
        assert!(
            rendered.contains(bash_form.as_str()),
            "bash next command drifted:\n{rendered}"
        );
        let powershell_form = format!(
            "```powershell\n{}\n```\n",
            powershell_command(&next)
                .ok_or_else(|| "redirect commands gain a powershell variant".to_string())?
        );
        assert!(
            rendered.contains(powershell_form.as_str()),
            "powershell next command missing or drifted:\n{rendered}"
        );
        let bash_fence = rendered
            .find(bash_form.as_str())
            .ok_or_else(|| format!("bash fence must exist: {rendered}"))?;
        let powershell_fence = rendered
            .find(powershell_form.as_str())
            .ok_or_else(|| format!("powershell fence must exist: {rendered}"))?;
        assert!(
            bash_fence < powershell_fence,
            "bash form must be presented before the PowerShell variant"
        );
        assert!(
            rendered.contains("cmd.exe is not supported."),
            "next command presentation must state the cmd.exe boundary:\n{rendered}"
        );

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn agent_status_recovers_seam_id_from_receipt() -> Result<(), String> {
        let root = unique_agent_status_test_dir("receipt");
        write_file(
            &root.join(WORKFLOW_AGENT_RECEIPT_ARTIFACT),
            r#"{"seam":{"seam_id":"67fc764ba37d77bd"}}"#,
        )?;

        let report = build_agent_status_report(&root, Path::new("repo root"));
        let seam = report
            .seam
            .as_ref()
            .ok_or_else(|| "expected recovered seam".to_string())?;

        assert_eq!(seam.seam_id, "67fc764ba37d77bd");
        assert_eq!(seam.source, "agent_receipt");
        assert!(report.missing_commands.iter().any(|command| {
            command.step == "agent_packet"
                && command.command
                    == agent_packet_command(
                        "repo root",
                        "67fc764ba37d77bd",
                        WORKFLOW_AGENT_PACKET_ARTIFACT,
                    )
        }));

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn agent_status_warns_for_malformed_present_json() -> Result<(), String> {
        let root = unique_agent_status_test_dir("malformed");
        write_file(&root.join(WORKFLOW_AGENT_RECEIPT_ARTIFACT), "{not json")?;

        let report = build_agent_status_report(&root, Path::new("."));

        assert!(report.seam.is_none());
        assert!(report.warnings.iter().any(|warning| {
            warning.kind == "artifact_json_unreadable" && warning.artifact == "agent_receipt"
        }));

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn agent_status_recovers_seam_id_from_verify_packet_or_brief() {
        let verify: Value = serde_json::json!({
            "changed_seams": [],
            "unchanged_seams": [{"seam_id": "from-verify"}],
            "new_gaps": [],
            "resolved_gaps": []
        });
        let packet: Value = serde_json::json!({
            "packets": [{"seam_id": "from-packet"}]
        });
        let brief: Value = serde_json::json!({
            "working_set": {"seam_id": "from-brief"},
            "top_seams": [{"seam_id": "from-top-seam"}]
        });

        assert_eq!(
            seam_id_from_source(&verify, "agent_verify"),
            Some("from-verify".to_string())
        );
        assert_eq!(
            seam_id_from_source(&packet, "agent_packet"),
            Some("from-packet".to_string())
        );
        assert_eq!(
            seam_id_from_source(&brief, "agent_brief"),
            Some("from-brief".to_string())
        );
    }

    #[test]
    fn agent_status_recovers_brief_top_seam_without_working_set_seam() {
        let brief: Value = serde_json::json!({
            "working_set": {"seam_id": null},
            "top_seams": [{"seam_id": "from-top-seam"}]
        });

        assert_eq!(
            seam_id_from_source(&brief, "agent_brief"),
            Some("from-top-seam".to_string())
        );
    }

    #[test]
    fn agent_status_warns_when_verify_or_receipt_look_stale() {
        let early = UNIX_EPOCH + Duration::from_secs(1);
        let middle = UNIX_EPOCH + Duration::from_secs(2);
        let late = UNIX_EPOCH + Duration::from_secs(3);
        let warnings = stale_warnings(&[
            artifact("before_snapshot", true, Some(late)),
            artifact("after_snapshot", true, Some(late)),
            artifact("agent_verify", true, Some(middle)),
            artifact("agent_receipt", true, Some(early)),
        ]);

        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().any(|warning| {
            warning.artifact == "agent_verify"
                && warning.message.contains("older than the before snapshot")
        }));
        assert!(warnings.iter().any(|warning| {
            warning.artifact == "agent_receipt"
                && warning.message.contains("older than agent verify")
        }));
    }

    #[test]
    fn agent_status_warns_when_before_snapshot_is_newer_than_after() {
        let after = UNIX_EPOCH + Duration::from_secs(2);
        let before = UNIX_EPOCH + Duration::from_secs(3);
        let completed = UNIX_EPOCH + Duration::from_secs(4);
        let warnings = stale_warnings(&[
            artifact("before_snapshot", true, Some(before)),
            artifact("after_snapshot", true, Some(after)),
            artifact("agent_verify", true, Some(completed)),
            artifact("agent_receipt", true, Some(completed)),
        ]);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].artifact, "before_snapshot");
        assert!(
            warnings[0]
                .message
                .contains("before snapshot is newer than after snapshot")
        );
    }

    fn synthetic_attempt(
        id: &str,
        seam: &str,
        disposition: &'static str,
        command: Option<&str>,
    ) -> AgentStatusRepairAttempt {
        AgentStatusRepairAttempt {
            attempt_id: id.to_string(),
            seam_id: seam.to_string(),
            state: "failed",
            head_current: Some(true),
            disposition,
            manifest: format!("{REPAIR_ATTEMPT_DIRECTORY}/{id}/attempt.json"),
            command: command.map(str::to_string),
        }
    }

    #[test]
    fn agent_status_refuses_to_choose_between_open_seams() -> Result<(), String> {
        let root = unique_agent_status_test_dir("open-seams");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        let attempts = vec![
            synthetic_attempt("a", "seam-a", "ended", Some("restart a")),
            synthetic_attempt("b", "seam-b", "ended", Some("restart b")),
        ];
        let mut warnings = Vec::new();
        let next = select_next_command(&root, ".", None, Some(&attempts), &[], &mut warnings);
        assert_eq!(next, None);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, "multiple_open_repair_seams");
        assert!(warnings[0].message.contains("--seam-id seam-a"));
        assert!(warnings[0].message.contains("--seam-id seam-b"));
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn agent_status_does_not_restart_a_seam_that_finished() -> Result<(), String> {
        let root = unique_agent_status_test_dir("finished-seam");
        std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
        let attempts = vec![
            synthetic_attempt("a", "seam-a", "ended", Some("restart a")),
            synthetic_attempt("b", "seam-a", "finished", None),
            synthetic_attempt("c", "seam-b", "ended", Some("restart c")),
        ];
        let mut warnings = Vec::new();
        let next = select_next_command(&root, ".", None, Some(&attempts), &[], &mut warnings)
            .ok_or_else(|| "expected a restart for the one open seam".to_string())?;
        assert_eq!(next.step, "repair_attempt_before");
        assert_eq!(
            next.command,
            "ripr agent repair --root . --seam-id seam-b --phase before"
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    /// After `ripr pilot`, status continues with the repair start pilot
    /// recorded rather than sending the user back to pilot. A summary whose
    /// top seam did not pass the flip (`null`) leaves status on `select_seam`.
    #[test]
    fn agent_status_continues_from_the_pilot_repair_start() -> Result<(), String> {
        let root = unique_agent_status_test_dir("after-pilot");
        let command = "ripr agent repair --root . --seam-id 8f7fa8644fd12280 --phase before";
        write_file(
            &root.join(PILOT_SUMMARY_ARTIFACT),
            &serde_json::json!({"next": {"repair_command": command}}).to_string(),
        )?;
        let report = build_agent_status_report(&root, Path::new("."));
        let next = report
            .next_command
            .as_ref()
            .ok_or_else(|| "expected a next command".to_string())?;
        assert_eq!(next.step, "repair_attempt_before");
        assert_eq!(next.command, command);
        assert_eq!(next.artifact, PILOT_SUMMARY_ARTIFACT);

        write_file(
            &root.join(PILOT_SUMMARY_ARTIFACT),
            r#"{"next": {"repair_command": null}}"#,
        )?;
        let report = build_agent_status_report(&root, Path::new("."));
        let next = report
            .next_command
            .as_ref()
            .ok_or_else(|| "expected a next command".to_string())?;
        assert_eq!(next.step, "select_seam");
        assert_eq!(next.command, "ripr pilot --root .");

        // Status repeats the pilot value as its next command, so only a repair
        // start may pass. Any other string, even another ripr command, leaves
        // status on `select_seam`, exactly as `null` does. `ripr pilot` never
        // writes such a value, so no end-to-end run reaches this case.
        write_file(
            &root.join(PILOT_SUMMARY_ARTIFACT),
            r#"{"next": {"repair_command": "ripr check --root ."}}"#,
        )?;
        let report = build_agent_status_report(&root, Path::new("."));
        let next = report
            .next_command
            .as_ref()
            .ok_or_else(|| "expected a next command".to_string())?;
        assert_eq!(next.step, "select_seam");
        assert_eq!(next.command, "ripr pilot --root .");

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    /// A seam known only from the receipt, with no workflow directory: the
    /// first missing artifact would be a redirect into that missing directory,
    /// so status starts a repair attempt instead.
    #[test]
    fn agent_status_never_redirects_into_a_missing_workflow_directory() -> Result<(), String> {
        let root = unique_agent_status_test_dir("receipt-only");
        write_file(
            &root.join(WORKFLOW_AGENT_RECEIPT_ARTIFACT),
            r#"{"seam":{"seam_id":"from-receipt"}}"#,
        )?;

        let report = build_agent_status_report(&root, Path::new("repo root"));
        let next = report
            .next_command
            .as_ref()
            .ok_or_else(|| "expected a next command".to_string())?;
        assert_eq!(report.missing_commands[0].step, "before_snapshot");
        assert_eq!(next.step, "repair_attempt_before");
        assert_eq!(
            next.command,
            "ripr agent repair --root 'repo root' --seam-id from-receipt --phase before"
        );
        assert!(!next.command.contains('>'));

        std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
        Ok(())
    }

    #[test]
    fn agent_status_emits_all_missing_command_templates() {
        let artifacts = ARTIFACTS
            .iter()
            .map(|def| AgentStatusArtifact {
                name: def.name.to_string(),
                label: def.label.to_string(),
                path: def.path.to_string(),
                present: false,
                bytes: None,
                modified: None,
            })
            .collect::<Vec<_>>();
        let seam = AgentStatusSeam {
            seam_id: "seam-a".to_string(),
            source: "agent_verify".to_string(),
        };
        let commands = missing_commands(Path::new("."), Some(&seam), &artifacts);

        assert_eq!(commands.len(), 7);
        assert!(commands.iter().any(|command| {
            command.step == "after_snapshot"
                && command.command
                    == check_repo_exposure_command(".", "draft", WORKFLOW_AFTER_SNAPSHOT_ARTIFACT)
        }));
        assert!(commands.iter().any(|command| {
            command.step == "analysis_outcome"
                && command.command
                    == check_analysis_outcome_command(
                        ".",
                        "draft",
                        WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT,
                    )
        }));
        assert!(commands.iter().any(|command| {
            command.step == "agent_brief"
                && command.command
                    == agent_brief_command(".", "seam-a", WORKFLOW_AGENT_BRIEF_ARTIFACT)
        }));
        assert!(commands.iter().any(|command| {
            command.step == "agent_verify"
                && command.command
                    == agent_verify_command(
                        ".",
                        WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
                        WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
                        Some(WORKFLOW_AGENT_VERIFY_ARTIFACT),
                    )
        }));
        assert!(commands.iter().any(|command| {
            command.step == "agent_receipt"
                && command.command
                    == "ripr agent receipt --root . --verify-json target/ripr/workflow/agent-verify.json --seam-id seam-a --json --out target/ripr/reports/agent-receipt.json"
        }));
    }

    #[test]
    fn agent_status_quotes_paths_with_spaces() {
        // Issue #3872: the redirect anchors at the resolved root, so the
        // quoted expectation names the anchored absolute target: the quoting
        // under test is the single-quote shell encoding around both values.
        let command = agent_packet_command("repo root", "seam-a", WORKFLOW_AGENT_PACKET_ARTIFACT);
        assert!(
            command.starts_with("ripr agent packet --root 'repo root' --seam-id seam-a --json > '"),
            "root and target must stay single-quoted: {command}"
        );
        assert!(
            command.ends_with("/repo root/target/ripr/workflow/agent-packet.json'"),
            "redirect must anchor under the resolved root: {command}"
        );
    }
}
