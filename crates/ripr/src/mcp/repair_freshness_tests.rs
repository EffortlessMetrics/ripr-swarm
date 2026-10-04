//! #5399: genuine durable attempts, not hand-authored currentness flags.

use super::*;
use crate::app::repair_attempt::{
    BeforeArtifactSource, BeginRepairAttemptOptions, RepairAttemptId, begin_repair_attempt_with,
    edit_cage_policy_from_packet, finish_repair_attempt, load_repair_attempt_manifest,
    write_edit_cage_baseline,
};
use std::path::PathBuf;

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    crate::testing::fixture_git::fixture_git_ok(root, args)
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

fn prepared(label: &str) -> Result<(PathBuf, RepairAttemptId), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("fixture clock: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-mcp-freshness-{label}-{stamp}"));
    std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
    git(&root, &["init"])?;
    git(
        &root,
        &["config", "user.email", "ripr-test@example.invalid"],
    )?;
    git(&root, &["config", "user.name", "RIPR Test"])?;
    write(&root.join(".gitignore"), "/target/\n")?;
    write(&root.join("tests/target.rs"), "#[test]\nfn focused() {}\n")?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "--no-gpg-sign", "-m", "prepared"])?;
    let workflow = root.join("target/ripr/workflow");
    std::fs::create_dir_all(&workflow).map_err(|error| error.to_string())?;
    let before = workflow.join("before.json");
    let packet = workflow.join("packet.json");
    let baseline = workflow.join("baseline.json");
    write(&before, "{}")?;
    let packet_text = json!({
        "seam_id": "seam:freshness",
        "allowed_edit_surface": ["tests/target.rs"],
        "forbidden_files": [],
    })
    .to_string();
    write(&packet, &packet_text)?;
    let policy = edit_cage_policy_from_packet(&packet_text, "seam:freshness")?;
    write_edit_cage_baseline(&root, &baseline, &policy)?;
    let result = begin_repair_attempt_with(BeginRepairAttemptOptions {
        root: &root,
        root_argument: &root,
        seam_id: "seam:freshness",
        sources: &[
            BeforeArtifactSource {
                role: "before_snapshot",
                path: &before,
            },
            BeforeArtifactSource {
                role: "agent_packet",
                path: &packet,
            },
            BeforeArtifactSource {
                role: "edit_cage_baseline",
                path: &baseline,
            },
        ],
        expected_repository_head: None,
        next_command_suffix: None,
        store: None,
    })?;
    if result.manifest.state != RepairAttemptState::AwaitingEdit {
        return Err("fixture did not publish one awaiting attempt".to_string());
    }
    Ok((root, result.manifest.repair_attempt_id))
}

fn parity(root: &Path, id: &RepairAttemptId, expected: &str) -> Result<(Value, Value), String> {
    let session = WorkspaceSession::default();
    let attempt = session
        .repair_attempt_document(id.as_str(), Some(root), None)
        .map_err(|failure| failure.detail)?;
    let receipt = session
        .receipt_status_document(id.as_str(), Some(root), None)
        .map_err(|failure| failure.detail)?;
    let cli = crate::app::agent_status::build_agent_attempt_status(root, root, None, id)?;
    for document in [&attempt, &receipt] {
        assert_eq!(document["currentness"]["state"], expected);
        assert_eq!(document["currentness"]["state"], cli.attempt.currentness);
        assert_eq!(
            document["currentness"]["head_current"],
            json!(cli.attempt.head_current)
        );
        assert_eq!(
            document["currentness"]["evidence_head"],
            json!(cli.attempt.evidence_head)
        );
    }
    assert_eq!(attempt["attempt_id"], id.as_str());
    assert_eq!(receipt["attempt"]["attempt_id"], id.as_str());
    Ok((attempt, receipt))
}

#[test]
fn durable_currentness_keeps_ordinary_descendant_continuation() -> Result<(), String> {
    let (root, id) = prepared("descendant")?;
    let result = (|| {
        let (before, _) = parity(&root, &id, "current")?;
        assert!(before["next_command"].is_string());
        write(
            &root.join("tests/target.rs"),
            "#[test]\nfn focused() { assert_eq!(1, 1); }\n",
        )?;
        git(&root, &["add", "tests/target.rs"])?;
        git(&root, &["commit", "--no-gpg-sign", "-m", "focused edit"])?;
        let (after, receipt) = parity(&root, &id, "current")?;
        assert_eq!(after["next_command"], before["next_command"]);
        assert_eq!(receipt["status"], "awaiting_edit");
        Ok(())
    })();
    if result.is_ok() {
        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    result
}

#[test]
fn durable_currentness_refuses_diverged_continuation_without_rewriting_attempt()
-> Result<(), String> {
    let (root, id) = prepared("diverged")?;
    let result = (|| {
        let manifest_path = root.join(format!(
            "target/ripr/repair-attempts/{}/attempt.json",
            id.as_str()
        ));
        let bytes = std::fs::read(&manifest_path).map_err(|error| error.to_string())?;
        git(&root, &["checkout", "--orphan", "other-history"])?;
        git(&root, &["commit", "--no-gpg-sign", "-m", "unrelated root"])?;
        let (attempt, receipt) = parity(&root, &id, "historical")?;
        assert!(attempt["next_command"].is_null());
        assert_eq!(attempt["command_routes"], json!([]));
        assert_eq!(receipt["status"], "stale");
        assert_eq!(receipt["attempt"]["state"], "awaiting_edit");
        assert_eq!(
            std::fs::read(&manifest_path).map_err(|error| error.to_string())?,
            bytes
        );
        assert_eq!(parity(&root, &id, "historical")?, (attempt, receipt));
        Ok(())
    })();
    if result.is_ok() {
        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    result
}

#[test]
fn durable_currentness_does_not_reuse_recorded_after_admission() -> Result<(), String> {
    let (root, id) = prepared("terminal")?;
    let result = (|| {
        write(
            &root.join("tests/target.rs"),
            "#[test]\nfn focused() { assert_eq!(1, 1); }\n",
        )?;
        let manifest = load_repair_attempt_manifest(&root, &id)?;
        let packet = find_manifest_artifact_by_role(&manifest, "agent_packet")
            .ok_or_else(|| "fixture lost retained packet".to_string())?;
        let after = finish_repair_attempt(
            &root,
            &id,
            &root.join(&packet.path),
            crate::edit_cage::HeadMovement::AdmitDescendantCommits,
        )?;
        assert!(after.current);
        let terminal = load_repair_attempt_manifest(&root, &id)?;
        assert_eq!(terminal.state, RepairAttemptState::ReadyToFinish);
        let (_, before) = parity(&root, &id, "current")?;
        assert_eq!(before["status"], "verification_pending");
        git(&root, &["add", "tests/target.rs"])?;
        git(&root, &["commit", "--no-gpg-sign", "-m", "later head"])?;
        let (_, receipt) = parity(&root, &id, "historical")?;
        assert_eq!(receipt["currentness"]["after_current"], true);
        assert_eq!(receipt["status"], "stale");
        assert_eq!(receipt["receipt"], before["receipt"]);
        Ok(())
    })();
    if result.is_ok() {
        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    result
}

#[test]
fn durable_currentness_unknown_head_never_offers_continuation() -> Result<(), String> {
    let (root, id) = prepared("unknown")?;
    let result = (|| {
        std::fs::rename(root.join(".git"), root.join(".git-unavailable"))
            .map_err(|error| error.to_string())?;
        let observed = parity(&root, &id, "unknown");
        std::fs::rename(root.join(".git-unavailable"), root.join(".git"))
            .map_err(|error| error.to_string())?;
        let (attempt, receipt) = observed?;
        assert!(attempt["next_command"].is_null());
        assert_eq!(attempt["command_routes"], json!([]));
        assert_eq!(receipt["status"], "limited");
        Ok(())
    })();
    if result.is_ok() {
        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    result
}
