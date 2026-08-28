//! Durable repair-attempt identity, retained inputs, and finish authority.
//!
//! Repository-global artifacts under `target/ripr/workflow` remain compatibility
//! projections. The durable transaction is selected by `RepairAttemptId`, and
//! the after phase consumes the retained before snapshot and packet attached to
//! that exact attempt.

use crate::agent::loop_commands::{display_path, shell_arg};
use crate::edit_cage::{
    AttemptBaseline, EditCagePolicy, EditCageVerdict, evaluate_repository_edit_cage_with_delta,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const REPAIR_ATTEMPT_SCHEMA_VERSION: &str = "0.1";
pub(crate) const REPAIR_ATTEMPT_DIRECTORY: &str = "target/ripr/repair-attempts";
const REPAIR_ATTEMPT_MANIFEST: &str = "attempt.json";
const REPAIR_ATTEMPT_COMMITMENT: &str = "before-commitment.sha256";
const REPAIR_ATTEMPT_ARTIFACTS_DIRECTORY: &str = "artifacts";
const REPAIR_ATTEMPT_ID_PREFIX: &str = "repair-attempt-";
const REPAIR_ATTEMPT_ID_HEX_LEN: usize = 24;

static ATTEMPT_NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct RepairAttemptId(String);

impl RepairAttemptId {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let Some(suffix) = value.strip_prefix(REPAIR_ATTEMPT_ID_PREFIX) else {
            return Err(format!(
                "repair attempt ID must start with `{REPAIR_ATTEMPT_ID_PREFIX}`"
            ));
        };
        if suffix.len() != REPAIR_ATTEMPT_ID_HEX_LEN
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(format!(
                "repair attempt ID suffix must contain {REPAIR_ATTEMPT_ID_HEX_LEN} lowercase hexadecimal characters"
            ));
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepairAttemptState {
    Prepared,
    AwaitingEdit,
    ReadyToFinish,
    Stale,
    Incomparable,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepairAttemptArtifact {
    pub(crate) role: String,
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepairAttemptManifest {
    pub(crate) schema_version: String,
    pub(crate) kind: String,
    pub(crate) repair_attempt_id: RepairAttemptId,
    pub(crate) state: RepairAttemptState,
    pub(crate) root: String,
    pub(crate) repository_head: String,
    pub(crate) producer_version: String,
    pub(crate) seam_id: String,
    pub(crate) created_unix_ms: u64,
    pub(crate) artifacts: Vec<RepairAttemptArtifact>,
    pub(crate) next_command: String,
    pub(crate) limitations: Vec<String>,
    pub(crate) non_claims: Vec<String>,
    #[serde(default)]
    pub(crate) after: Option<RepairAttemptAfter>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepairAttemptAfter {
    pub(crate) attempt_id: RepairAttemptId,
    pub(crate) repository_head: String,
    pub(crate) delta_sha256: String,
    pub(crate) packet_sha256: String,
    pub(crate) current: bool,
    pub(crate) verdict: EditCageVerdict,
}

/// The only attempt state that may authorize a receipt. This is deliberately
/// derived from the durable manifest and its immutable before artifacts rather
/// than from the workflow filenames, which are compatibility outputs.
pub(crate) fn receipt_binding(
    root: &Path,
    seam_id: &str,
    packet_path: &Path,
    attempt_id: Option<&str>,
) -> Result<serde_json::Value, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize repair attempt root failed: {error}"))?;
    let packet = std::fs::read(packet_path).map_err(|error| {
        format!(
            "read repair packet {} failed: {error}",
            packet_path.display()
        )
    })?;
    let packet_sha256 = sha256_bytes(&packet);
    let policy = edit_cage_policy_from_packet(
        std::str::from_utf8(&packet)
            .map_err(|error| format!("repair packet is not UTF-8: {error}"))?,
        seam_id,
    )?;
    validate_trusted_head_surface(&root, &policy)?;
    let manifests_root = root.join(REPAIR_ATTEMPT_DIRECTORY);
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(&manifests_root)
        .map_err(|error| format!("read {} failed: {error}", manifests_root.display()))?
    {
        let entry = entry.map_err(|error| format!("read repair attempt entry failed: {error}"))?;
        let path = entry.path().join(REPAIR_ATTEMPT_MANIFEST);
        if !path.is_file() {
            continue;
        }
        let manifest = read_repair_attempt_manifest_at(&root, &path)?;
        if manifest.seam_id == seam_id
            && attempt_id.is_none_or(|id| manifest.repair_attempt_id.as_str() == id)
        {
            matches.push((path, manifest));
        }
    }
    if matches.len() != 1 {
        return Err(format!(
            "receipt requires exactly one repair attempt for seam `{seam_id}`, found {}",
            matches.len()
        ));
    }
    let (manifest_path, manifest) = matches.pop().ok_or_else(|| "missing attempt".to_string())?;
    let after = manifest
        .after
        .as_ref()
        .ok_or_else(|| "repair attempt has no after-phase verdict".to_string())?;
    if manifest.state != RepairAttemptState::ReadyToFinish
        || !after.current
        || after.verdict.status != crate::edit_cage::EditCageVerdictStatus::Compliant
    {
        return Err(format!(
            "repair attempt {} is not receipt-ready: state {:?}, current {}, verdict {:?}",
            manifest.repair_attempt_id.as_str(),
            manifest.state,
            after.current,
            after.verdict.status
        ));
    }
    let current_head = crate::agent::artifact::current_git_head(&root)?;
    if current_head != manifest.repository_head || current_head != after.repository_head {
        return Err("repair attempt receipt is stale relative to repository HEAD".to_string());
    }
    let packet_artifact = find_manifest_artifact(&manifest, "agent_packet")?;
    let packet_artifact_bytes = std::fs::read(root.join(&packet_artifact.path))
        .map_err(|error| format!("read staged agent packet failed: {error}"))?;
    if sha256_bytes(&packet_artifact_bytes) != packet_artifact.sha256
        || packet_artifact.sha256 != packet_sha256
        || after.packet_sha256 != packet_sha256
    {
        return Err("repair attempt packet binding is tampered or replayed".to_string());
    }
    let baseline_artifact = find_manifest_artifact(&manifest, "edit_cage_baseline")?;
    let baseline_bytes = std::fs::read(root.join(&baseline_artifact.path))
        .map_err(|error| format!("read staged edit-cage baseline failed: {error}"))?;
    if sha256_bytes(&baseline_bytes) != baseline_artifact.sha256 {
        return Err("repair attempt edit-cage baseline binding is tampered".to_string());
    }
    let baseline: AttemptBaseline = serde_json::from_slice(&baseline_bytes)
        .map_err(|error| format!("decode edit-cage baseline failed: {error}"))?;
    let (delta, verdict) = evaluate_repository_edit_cage_with_delta(&baseline)?;
    let delta_bytes = serde_json::to_vec(&delta)
        .map_err(|error| format!("serialize repair delta failed: {error}"))?;
    if sha256_bytes(&delta_bytes) != after.delta_sha256 || verdict != after.verdict {
        return Err("repair attempt after verdict binding is tampered or stale".to_string());
    }
    let manifest_path = display_path(&manifest_path);
    Ok(serde_json::json!({
        "attempt_id": after.attempt_id.as_str(),
        "manifest": manifest_path,
        "seam_id": manifest.seam_id,
        "before_head": manifest.repository_head,
        "after_head": after.repository_head,
        "packet_sha256": after.packet_sha256,
        "delta_sha256": after.delta_sha256,
        "current": after.current,
        "edit_cage_verdict": after.verdict,
    }))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BeforeArtifactSource<'a> {
    pub(crate) role: &'a str,
    pub(crate) path: &'a Path,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BeginRepairAttemptResult {
    pub(crate) manifest: RepairAttemptManifest,
    pub(crate) manifest_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedRepairAttempt {
    pub(crate) attempt_id: RepairAttemptId,
    pub(crate) seam_id: String,
    pub(crate) manifest_path: PathBuf,
    pub(crate) before_snapshot_path: PathBuf,
    pub(crate) packet_path: PathBuf,
}

pub(crate) fn begin_repair_attempt(
    root: &Path,
    root_argument: &Path,
    seam_id: &str,
    sources: &[BeforeArtifactSource<'_>],
) -> Result<BeginRepairAttemptResult, String> {
    if seam_id.trim().is_empty() {
        return Err("repair attempt requires a non-empty seam ID".to_string());
    }
    if sources.is_empty() {
        return Err("repair attempt requires at least one before-phase artifact".to_string());
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize repair attempt root failed: {error}"))?;
    let repository_head = crate::agent::artifact::current_git_head(&canonical_root)
        .map_err(|error| format!("repair attempt requires a concrete repository HEAD: {error}"))?;
    let created_unix_ms = current_unix_ms()?;
    let nonce = ATTEMPT_NONCE.fetch_add(1, Ordering::Relaxed);
    let repair_attempt_id = repair_attempt_id_from_parts(
        &display_path(&canonical_root),
        seam_id,
        &repository_head,
        created_unix_ms,
        std::process::id(),
        nonce,
    )?;
    let attempt_directory = reserve_attempt_directory(&canonical_root, &repair_attempt_id)?;
    complete_repair_attempt(
        &canonical_root,
        &attempt_directory,
        AttemptPublication {
            root_argument,
            seam_id,
            repository_head,
            created_unix_ms,
            repair_attempt_id,
            sources,
        },
    )
}

struct AttemptPublication<'a> {
    root_argument: &'a Path,
    seam_id: &'a str,
    repository_head: String,
    created_unix_ms: u64,
    repair_attempt_id: RepairAttemptId,
    sources: &'a [BeforeArtifactSource<'a>],
}

/// Stage artifacts and publish the manifest inside a reserved attempt
/// directory. Any failure removes the reserved directory, so a failed begin
/// leaves neither an orphan attempt nor a partial artifact set behind.
fn complete_repair_attempt(
    canonical_root: &Path,
    attempt_directory: &Path,
    publication: AttemptPublication<'_>,
) -> Result<BeginRepairAttemptResult, String> {
    let result = stage_before_artifacts(canonical_root, attempt_directory, publication.sources)
        .and_then(|artifacts| {
            let next_command = format!(
                "ripr agent repair --root {} --attempt {} --phase after",
                shell_arg(&display_path(publication.root_argument)),
                shell_arg(publication.repair_attempt_id.as_str())
            );
            let manifest = RepairAttemptManifest {
                schema_version: REPAIR_ATTEMPT_SCHEMA_VERSION.to_string(),
                kind: "repair_attempt".to_string(),
                repair_attempt_id: publication.repair_attempt_id,
                state: RepairAttemptState::AwaitingEdit,
                root: display_path(canonical_root),
                repository_head: publication.repository_head,
                producer_version: env!("CARGO_PKG_VERSION").to_string(),
                seam_id: publication.seam_id.to_string(),
                created_unix_ms: publication.created_unix_ms,
                artifacts,
                next_command,
                limitations: vec![
                    "after-phase verify and receipt outputs remain mirrored through target/ripr/workflow compatibility paths"
                        .to_string(),
                ],
                non_claims: vec![
                    "RIPR does not author or apply the focused test edit".to_string(),
                    "prepared evidence does not mean the gap is fixed or verified".to_string(),
                    "this manifest does not authorize mutation execution or merge".to_string(),
                ],
                after: None,
            };
            let manifest_path = write_repair_attempt_manifest(canonical_root, &manifest)?;
            Ok(BeginRepairAttemptResult {
                manifest,
                manifest_path,
            })
        });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(attempt_directory);
    }
    result
}

pub(crate) fn repair_attempt_directory(root: &Path, attempt_id: &RepairAttemptId) -> PathBuf {
    root.join(REPAIR_ATTEMPT_DIRECTORY)
        .join(attempt_id.as_str())
}

/// Reserve the attempt transaction exclusively. Creating the directory with
/// `create_dir` (not check-then-act) fails closed when the attempt identity is
/// already taken, so an existing attempt is never reused or overwritten.
fn reserve_attempt_directory(root: &Path, attempt_id: &RepairAttemptId) -> Result<PathBuf, String> {
    let attempts_root = root.join(REPAIR_ATTEMPT_DIRECTORY);
    std::fs::create_dir_all(&attempts_root)
        .map_err(|error| format!("create {} failed: {error}", attempts_root.display()))?;
    let attempt_directory = attempts_root.join(attempt_id.as_str());
    std::fs::create_dir(&attempt_directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            return format!(
                "repair attempt directory already exists: {}",
                attempt_directory.display()
            );
        }
        format!("create {} failed: {error}", attempt_directory.display())
    })?;
    Ok(attempt_directory)
}

pub(crate) fn write_repair_attempt_manifest(
    root: &Path,
    manifest: &RepairAttemptManifest,
) -> Result<PathBuf, String> {
    validate_manifest(manifest)?;
    let path =
        repair_attempt_directory(root, &manifest.repair_attempt_id).join(REPAIR_ATTEMPT_MANIFEST);
    let mut rendered = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("serialize repair attempt manifest failed: {error}"))?;
    rendered.push(b'\n');
    write_bytes_atomic(&path, &rendered)?;
    if manifest.state == RepairAttemptState::AwaitingEdit {
        let commitment_path = repair_attempt_directory(root, &manifest.repair_attempt_id)
            .join(REPAIR_ATTEMPT_COMMITMENT);
        let commitment = sha256_bytes(&manifest_before_bytes(manifest)?);
        if commitment_path.exists() {
            let existing = std::fs::read_to_string(&commitment_path)
                .map_err(|error| format!("read {} failed: {error}", commitment_path.display()))?;
            if existing.trim() != commitment {
                return Err("repair attempt before commitment mismatch".to_string());
            }
        } else {
            write_bytes_atomic(&commitment_path, commitment.as_bytes())?;
        }
    }
    Ok(path)
}

fn manifest_before_bytes(manifest: &RepairAttemptManifest) -> Result<Vec<u8>, String> {
    let mut before = manifest.clone();
    before.state = RepairAttemptState::AwaitingEdit;
    before.after = None;
    serde_json::to_vec_pretty(&before)
        .map_err(|error| format!("serialize repair attempt commitment failed: {error}"))
}

pub(crate) fn edit_cage_policy_from_packet(
    packet: &str,
    seam_id: &str,
) -> Result<EditCagePolicy, String> {
    let value: serde_json::Value = serde_json::from_str(packet)
        .map_err(|error| format!("decode repair packet for edit cage failed: {error}"))?;
    if value.get("packets").is_none()
        && value.get("seam_id").and_then(serde_json::Value::as_str) != Some(seam_id)
    {
        return Err(format!(
            "repair packet seam does not match requested seam `{seam_id}`"
        ));
    }
    // `agent packet` emits a repo packet containing a `packets` list, while
    // language adapters may emit a single packet. Select the only actionable
    // packet without treating the report envelope as edit-cage authority.
    let value = value
        .get("packets")
        .and_then(serde_json::Value::as_array)
        .map(|packets| {
            packets
                .iter()
                .filter(|packet| {
                    packet.get("seam_id").and_then(serde_json::Value::as_str) == Some(seam_id)
                })
                .collect::<Vec<_>>()
        })
        .map(|matches| {
            if matches.len() != 1 {
                return Err(format!(
                    "repair packet must contain exactly one packet for seam `{seam_id}`, found {}",
                    matches.len()
                ));
            }
            Ok(matches[0])
        })
        .transpose()?
        .unwrap_or(&value);
    let value =
        if value.get("allowed_edit_surface").is_none() && value.get("recommended_test").is_none() {
            let packets = value
                .get("packets")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| "repair packet is missing allowed_edit_surface".to_string())?;
            packets
                .first()
                .ok_or_else(|| "repair packet contains no actionable packet".to_string())?
        } else {
            value
        };
    let paths = |name: &str| -> Result<Vec<crate::edit_cage::CagePathRule>, String> {
        let values = value
            .get(name)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("repair packet is missing {name}"))?;
        values
            .iter()
            .map(|path| {
                path.as_str()
                    .ok_or_else(|| format!("repair packet {name} contains a non-string path"))
                    .and_then(crate::edit_cage::CagePathRule::exact)
            })
            .collect()
    };
    let allowed = match value.get("allowed_edit_surface") {
        Some(_) => paths("allowed_edit_surface")?,
        None => {
            let file = value
                .get("recommended_test")
                .and_then(|test| test.get("file"))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "repair packet is missing allowed edit target".to_string())?;
            vec![crate::edit_cage::CagePathRule::exact(file)?]
        }
    };
    let selected_target = allowed
        .first()
        .cloned()
        .ok_or_else(|| "repair packet has no selected edit target".to_string())?;
    Ok(EditCagePolicy {
        selected_target,
        allowed_edit_surface: allowed,
        forbidden_paths: value
            .get("forbidden_files")
            .map(|_| paths("forbidden_files"))
            .transpose()?
            .unwrap_or_default(),
        expected_operational_writes: vec![crate::edit_cage::CagePathRule::subtree("target/ripr")?],
    })
}

pub(crate) fn write_edit_cage_baseline(
    root: &Path,
    path: &Path,
    policy: &EditCagePolicy,
) -> Result<(), String> {
    let baseline = crate::edit_cage::capture_attempt_baseline(root, policy)?;
    let bytes = serde_json::to_vec_pretty(&baseline)
        .map_err(|error| format!("serialize edit-cage baseline failed: {error}"))?;
    write_bytes_atomic(path, &bytes)
}

/// Resolve the durable before inputs for one after-phase invocation. Attempt ID
/// is the ordinary authority; seam selection remains a compatibility route and
/// fails closed when more than one awaiting attempt shares that seam.
pub(crate) fn resolve_awaiting_repair_attempt(
    root: &Path,
    attempt_id: Option<&str>,
    seam_id: Option<&str>,
) -> Result<ResolvedRepairAttempt, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize repair attempt root failed: {error}"))?;
    let (manifest_path, manifest) = match (attempt_id, seam_id) {
        (Some(attempt_id), None) => {
            let attempt_id = RepairAttemptId::parse(attempt_id.to_string())?;
            load_repair_attempt_by_id(&root, &attempt_id)?
        }
        (None, Some(seam_id)) => select_awaiting_repair_attempt_by_seam(&root, seam_id)?,
        (Some(_), Some(_)) => {
            return Err(
                "repair after selection accepts either attempt ID or seam ID, not both".to_string(),
            );
        }
        (None, None) => {
            return Err("repair after selection requires an attempt ID or seam ID".to_string());
        }
    };
    if manifest.state != RepairAttemptState::AwaitingEdit {
        return Err(format!(
            "repair attempt {} is {:?}; after phase requires awaiting_edit",
            manifest.repair_attempt_id.as_str(),
            manifest.state
        ));
    }
    let before_snapshot_path =
        root.join(&find_manifest_artifact(&manifest, "before_snapshot")?.path);
    let packet_path = root.join(&find_manifest_artifact(&manifest, "agent_packet")?.path);
    Ok(ResolvedRepairAttempt {
        attempt_id: manifest.repair_attempt_id,
        seam_id: manifest.seam_id,
        manifest_path,
        before_snapshot_path,
        packet_path,
    })
}

pub(crate) fn finish_repair_attempt(
    root: &Path,
    attempt_id: &RepairAttemptId,
    packet_path: &Path,
) -> Result<RepairAttemptAfter, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize repair attempt root failed: {error}"))?;
    let (manifest_path, mut manifest) = load_repair_attempt_by_id(&root, attempt_id)?;
    if manifest.state != RepairAttemptState::AwaitingEdit {
        return Err(format!(
            "repair attempt {} is {:?}; after phase requires awaiting_edit",
            attempt_id.as_str(),
            manifest.state
        ));
    }
    let packet_artifact = find_manifest_artifact(&manifest, "agent_packet")?;
    let retained_packet_path = root.join(&packet_artifact.path);
    let supplied_packet_path = packet_path
        .canonicalize()
        .map_err(|error| format!("canonicalize repair packet failed: {error}"))?;
    let retained_packet_path = retained_packet_path
        .canonicalize()
        .map_err(|error| format!("canonicalize retained repair packet failed: {error}"))?;
    if supplied_packet_path != retained_packet_path {
        return Err(format!(
            "repair attempt {} must finish with its retained agent_packet artifact",
            attempt_id.as_str()
        ));
    }
    let packet = std::fs::read(&retained_packet_path).map_err(|error| {
        format!(
            "read repair packet {} failed: {error}",
            retained_packet_path.display()
        )
    })?;
    let packet_sha256 = sha256_bytes(&packet);
    if u64::try_from(packet.len()).map_err(|error| error.to_string())? != packet_artifact.bytes
        || packet_sha256 != packet_artifact.sha256
    {
        return Err("repair attempt packet binding failed".to_string());
    }
    let baseline_artifact = find_manifest_artifact(&manifest, "edit_cage_baseline")?;
    let baseline_path = root.join(&baseline_artifact.path);
    let baseline_bytes = std::fs::read(&baseline_path)
        .map_err(|error| format!("read {} failed: {error}", baseline_path.display()))?;
    if u64::try_from(baseline_bytes.len()).map_err(|error| error.to_string())?
        != baseline_artifact.bytes
        || sha256_bytes(&baseline_bytes) != baseline_artifact.sha256
    {
        return Err("repair attempt edit-cage baseline binding failed".to_string());
    }
    let baseline: AttemptBaseline = serde_json::from_slice(&baseline_bytes)
        .map_err(|error| format!("decode edit-cage baseline failed: {error}"))?;
    if baseline.root() != root {
        return Err("edit-cage baseline root does not match selected repository".to_string());
    }
    let current_head = crate::agent::artifact::current_git_head(&root)?;
    let current = current_head == manifest.repository_head;
    let (delta, mut verdict) = evaluate_repository_edit_cage_with_delta(&baseline)?;
    if !current {
        verdict.status = crate::edit_cage::EditCageVerdictStatus::Incomparable;
    }
    let delta_bytes = serde_json::to_vec(&delta)
        .map_err(|error| format!("serialize repair delta failed: {error}"))?;
    let after = RepairAttemptAfter {
        attempt_id: manifest.repair_attempt_id.clone(),
        repository_head: current_head,
        delta_sha256: sha256_bytes(&delta_bytes),
        packet_sha256,
        current,
        verdict,
    };
    manifest.after = Some(after.clone());
    manifest.state = if after.current {
        match after.verdict.status {
            crate::edit_cage::EditCageVerdictStatus::Compliant => RepairAttemptState::ReadyToFinish,
            crate::edit_cage::EditCageVerdictStatus::Violated => RepairAttemptState::Failed,
            crate::edit_cage::EditCageVerdictStatus::Incomparable => {
                RepairAttemptState::Incomparable
            }
        }
    } else {
        RepairAttemptState::Stale
    };
    let mut bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serialize completed repair attempt failed: {error}"))?;
    bytes.push(b'\n');
    replace_manifest_bytes(&manifest_path, &bytes)?;
    Ok(after)
}

fn select_awaiting_repair_attempt_by_seam(
    root: &Path,
    seam_id: &str,
) -> Result<(PathBuf, RepairAttemptManifest), String> {
    if seam_id.trim().is_empty() {
        return Err("repair after selection requires a non-empty seam ID".to_string());
    }
    let manifests_root = root.join(REPAIR_ATTEMPT_DIRECTORY);
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(&manifests_root)
        .map_err(|error| format!("read {} failed: {error}", manifests_root.display()))?
    {
        let entry = entry.map_err(|error| format!("read repair attempt entry failed: {error}"))?;
        let path = entry.path().join(REPAIR_ATTEMPT_MANIFEST);
        if !path.is_file() {
            continue;
        }
        let manifest = read_repair_attempt_manifest_at(root, &path)?;
        if manifest.seam_id == seam_id && manifest.state == RepairAttemptState::AwaitingEdit {
            matches.push((path, manifest));
        }
    }
    matches.sort_by(|left, right| left.1.repair_attempt_id.cmp(&right.1.repair_attempt_id));
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one awaiting repair attempt for seam `{seam_id}`, found {}; pass --attempt <id> to select the prepared work exactly",
            matches.len()
        ));
    }
    matches.pop().ok_or_else(|| "missing attempt".to_string())
}

fn load_repair_attempt_by_id(
    root: &Path,
    attempt_id: &RepairAttemptId,
) -> Result<(PathBuf, RepairAttemptManifest), String> {
    let path = repair_attempt_directory(root, attempt_id).join(REPAIR_ATTEMPT_MANIFEST);
    if !path.is_file() {
        return Err(format!(
            "repair attempt manifest not found for {} at {}",
            attempt_id.as_str(),
            path.display()
        ));
    }
    let manifest = read_repair_attempt_manifest_at(root, &path)?;
    if manifest.repair_attempt_id != *attempt_id {
        return Err("repair attempt manifest identity does not match its selector".to_string());
    }
    Ok((path, manifest))
}

fn read_repair_attempt_manifest_at(
    root: &Path,
    path: &Path,
) -> Result<RepairAttemptManifest, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("read {} failed: {error}", path.display()))?;
    let manifest: RepairAttemptManifest = serde_json::from_str(&raw)
        .map_err(|error| format!("decode {} failed: {error}", path.display()))?;
    validate_manifest_at(root, path, &manifest)?;
    Ok(manifest)
}

fn find_manifest_artifact<'a>(
    manifest: &'a RepairAttemptManifest,
    role: &str,
) -> Result<&'a RepairAttemptArtifact, String> {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.role == role)
        .ok_or_else(|| format!("repair attempt is missing {role} artifact"))
}

fn replace_manifest_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("after-tmp-{}", std::process::id()));
    write_bytes_atomic(&temporary, bytes)?;
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|error| format!("replace {} failed: {error}", path.display()))?;
    }
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("replace {} failed: {error}", path.display())
    })
}

fn stage_before_artifacts(
    root: &Path,
    attempt_directory: &Path,
    sources: &[BeforeArtifactSource<'_>],
) -> Result<Vec<RepairAttemptArtifact>, String> {
    let destination_directory = attempt_directory.join(REPAIR_ATTEMPT_ARTIFACTS_DIRECTORY);
    let nonce = ATTEMPT_NONCE.fetch_add(1, Ordering::Relaxed);
    let staging_directory = attempt_directory.join(format!(
        ".{REPAIR_ATTEMPT_ARTIFACTS_DIRECTORY}.tmp-{}-{nonce}",
        std::process::id()
    ));
    let artifacts = match stage_sources(root, &staging_directory, &destination_directory, sources) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging_directory);
            return Err(error);
        }
    };
    if let Err(error) = std::fs::rename(&staging_directory, &destination_directory) {
        let _ = std::fs::remove_dir_all(&staging_directory);
        return Err(format!(
            "publish {} failed: {error}",
            destination_directory.display()
        ));
    }
    Ok(artifacts)
}

fn stage_sources(
    root: &Path,
    staging_directory: &Path,
    destination_directory: &Path,
    sources: &[BeforeArtifactSource<'_>],
) -> Result<Vec<RepairAttemptArtifact>, String> {
    let mut roles = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut artifacts = Vec::with_capacity(sources.len());

    for source in sources {
        if source.role.trim().is_empty() || !roles.insert(source.role) {
            return Err(format!(
                "repair attempt artifact role is blank or duplicated: `{}`",
                source.role
            ));
        }
        let source_path = source
            .path
            .canonicalize()
            .map_err(|error| format!("canonicalize {} failed: {error}", source.path.display()))?;
        if !source_path.starts_with(root) {
            return Err(format!(
                "repair attempt artifact escapes root: {}",
                source.path.display()
            ));
        }
        let file_name = source_path
            .file_name()
            .ok_or_else(|| {
                format!(
                    "repair attempt artifact has no file name: {}",
                    source_path.display()
                )
            })?
            .to_owned();
        if !names.insert(file_name.clone()) {
            return Err(format!(
                "repair attempt artifact file name is duplicated: {}",
                file_name.to_string_lossy()
            ));
        }
        let bytes = std::fs::read(&source_path)
            .map_err(|error| format!("read {} failed: {error}", source_path.display()))?;
        let staged = staging_directory.join(&file_name);
        write_bytes_atomic(&staged, &bytes)?;
        let destination = destination_directory.join(&file_name);
        let relative = destination.strip_prefix(root).map_err(|error| {
            format!(
                "repair attempt destination {} is not under root {}: {error}",
                destination.display(),
                root.display()
            )
        })?;
        artifacts.push(RepairAttemptArtifact {
            role: source.role.to_string(),
            path: display_path(relative),
            sha256: sha256_bytes(&bytes),
            bytes: u64::try_from(bytes.len()).map_err(|error| {
                format!("repair attempt artifact size does not fit u64: {error}")
            })?,
        });
    }

    artifacts.sort_by(|left, right| left.role.cmp(&right.role));
    Ok(artifacts)
}

fn validate_manifest(manifest: &RepairAttemptManifest) -> Result<(), String> {
    if manifest.schema_version != REPAIR_ATTEMPT_SCHEMA_VERSION {
        return Err(format!(
            "repair attempt schema must be {REPAIR_ATTEMPT_SCHEMA_VERSION}, got {}",
            manifest.schema_version
        ));
    }
    RepairAttemptId::parse(manifest.repair_attempt_id.as_str())?;
    if manifest.kind != "repair_attempt"
        || manifest.root.is_empty()
        || manifest.repository_head.len() != 40
        || !manifest
            .repository_head
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || manifest.producer_version.is_empty()
        || manifest.seam_id.is_empty()
        || manifest.artifacts.is_empty()
        || manifest.next_command.is_empty()
        || manifest
            .limitations
            .iter()
            .any(|limitation| limitation.is_empty())
        || manifest.non_claims.is_empty()
        || manifest
            .non_claims
            .iter()
            .any(|non_claim| non_claim.is_empty())
    {
        return Err("repair attempt manifest is incomplete or malformed".to_string());
    }
    let has_after = manifest.after.is_some();
    let state_requires_after = matches!(
        manifest.state,
        RepairAttemptState::ReadyToFinish
            | RepairAttemptState::Stale
            | RepairAttemptState::Incomparable
            | RepairAttemptState::Failed
    );
    if has_after != state_requires_after {
        return Err("repair attempt state/after boundary is inconsistent".to_string());
    }
    if manifest.artifacts.iter().any(|artifact| {
        artifact.role.is_empty() || artifact.path.is_empty() || !is_sha256_digest(&artifact.sha256)
    }) {
        return Err("repair attempt manifest contains an invalid artifact".to_string());
    }
    let mut roles = BTreeSet::new();
    let mut paths = BTreeSet::new();
    if manifest
        .artifacts
        .iter()
        .any(|artifact| !roles.insert(&artifact.role) || !paths.insert(&artifact.path))
    {
        return Err("repair attempt manifest contains duplicate artifact identity".to_string());
    }
    Ok(())
}

fn validate_manifest_at(
    root: &Path,
    manifest_path: &Path,
    manifest: &RepairAttemptManifest,
) -> Result<(), String> {
    validate_manifest(manifest)?;
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize manifest root failed: {error}"))?;
    let declared_root = PathBuf::from(&manifest.root)
        .canonicalize()
        .map_err(|error| format!("canonicalize declared manifest root failed: {error}"))?;
    if declared_root != root {
        return Err("repair attempt manifest root does not match selected repository".to_string());
    }
    let expected_manifest =
        repair_attempt_directory(&root, &manifest.repair_attempt_id).join(REPAIR_ATTEMPT_MANIFEST);
    if manifest_path
        .canonicalize()
        .map_err(|error| format!("canonicalize manifest path failed: {error}"))?
        != expected_manifest
    {
        return Err("repair attempt manifest path is not bound to its identity".to_string());
    }
    let commitment_path = repair_attempt_directory(&root, &manifest.repair_attempt_id)
        .join(REPAIR_ATTEMPT_COMMITMENT);
    let commitment = std::fs::read_to_string(&commitment_path)
        .map_err(|error| format!("read before commitment failed: {error}"))?;
    if commitment.trim() != sha256_bytes(&manifest_before_bytes(manifest)?) {
        return Err("repair attempt before commitment failed".to_string());
    }
    let artifacts_root = repair_attempt_directory(&root, &manifest.repair_attempt_id)
        .join(REPAIR_ATTEMPT_ARTIFACTS_DIRECTORY);
    for artifact in &manifest.artifacts {
        let path = root.join(&artifact.path);
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("canonicalize artifact {} failed: {error}", artifact.path))?;
        if !canonical.starts_with(&artifacts_root) {
            return Err(format!(
                "repair attempt artifact escapes its attempt: {}",
                artifact.path
            ));
        }
        let bytes = std::fs::read(&canonical)
            .map_err(|error| format!("read artifact {} failed: {error}", artifact.path))?;
        if u64::try_from(bytes.len()).map_err(|error| error.to_string())? != artifact.bytes
            || sha256_bytes(&bytes) != artifact.sha256
        {
            return Err(format!(
                "repair attempt artifact binding failed: {}",
                artifact.path
            ));
        }
    }
    Ok(())
}

fn validate_trusted_head_surface(root: &Path, policy: &EditCagePolicy) -> Result<(), String> {
    let tracked = git_paths(root, &["diff", "--name-only", "-z", "HEAD"])?;
    let untracked = git_paths(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for path in tracked.into_iter().chain(untracked) {
        if !policy.allows_path(&path) {
            return Err(format!(
                "repair receipt observed repository path outside trusted edit surface: {path}"
            ));
        }
    }
    Ok(())
}

fn git_paths(root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("run git {} failed: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Schema 0.1 artifact digest shape: `sha256:` plus 64 lowercase hex digits.
fn is_sha256_digest(value: &str) -> bool {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return false;
    };
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn repair_attempt_id_from_parts(
    root: &str,
    seam_id: &str,
    repository_head: &str,
    created_unix_ms: u64,
    process_id: u32,
    nonce: u64,
) -> Result<RepairAttemptId, String> {
    let mut hasher = Sha256::new();
    for value in [root, seam_id, repository_head] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hasher.update(created_unix_ms.to_le_bytes());
    hasher.update(process_id.to_le_bytes());
    hasher.update(nonce.to_le_bytes());
    let digest = hasher.finalize();
    let mut suffix = String::with_capacity(REPAIR_ATTEMPT_ID_HEX_LEN);
    for byte in digest.iter().take(REPAIR_ATTEMPT_ID_HEX_LEN / 2) {
        suffix.push_str(&format!("{byte:02x}"));
    }
    RepairAttemptId::parse(format!("{REPAIR_ATTEMPT_ID_PREFIX}{suffix}"))
}

fn current_unix_ms() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?;
    u64::try_from(duration.as_millis())
        .map_err(|error| format!("current Unix timestamp does not fit u64: {error}"))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut rendered = String::from("sha256:");
    for byte in digest {
        rendered.push_str(&format!("{byte:02x}"));
    }
    rendered
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
    }
    let nonce = ATTEMPT_NONCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("tmp-{}-{nonce}", std::process::id()));
    let write_result = (|| -> Result<(), String> {
        let mut file = File::create(&temporary)
            .map_err(|error| format!("create {} failed: {error}", temporary.display()))?;
        file.write_all(bytes)
            .map_err(|error| format!("write {} failed: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("sync {} failed: {error}", temporary.display()))?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }

    // Publish exclusively: attempt destinations are immutable, so linking the
    // temporary file into place fails closed instead of deleting or
    // overwriting an existing destination (which also removes the Windows
    // delete-before-rename crash gap).
    if let Err(error) = std::fs::hard_link(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            return Err(format!(
                "repair attempt destination is immutable and already exists: {}",
                path.display()
            ));
        }
        return Err(format!("publish {} failed: {error}", path.display()));
    }
    let _ = std::fs::remove_file(&temporary);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("test clock failed: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-repair-attempt-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("create {} failed: {error}", root.display()))?;
        root.canonicalize()
            .map_err(|error| format!("canonicalize {} failed: {error}", root.display()))
    }

    fn sample_manifest(root: &Path) -> Result<RepairAttemptManifest, String> {
        let attempt_id = repair_attempt_id_from_parts(
            &display_path(root),
            "seam:sample",
            "0123456789abcdef0123456789abcdef01234567",
            1,
            2,
            3,
        )?;
        let next_command = format!(
            "ripr agent repair --root . --attempt {} --phase after",
            attempt_id.as_str()
        );
        Ok(RepairAttemptManifest {
            schema_version: REPAIR_ATTEMPT_SCHEMA_VERSION.to_string(),
            kind: "repair_attempt".to_string(),
            repair_attempt_id: attempt_id,
            state: RepairAttemptState::AwaitingEdit,
            root: display_path(root),
            repository_head: "0123456789abcdef0123456789abcdef01234567".to_string(),
            producer_version: env!("CARGO_PKG_VERSION").to_string(),
            seam_id: "seam:sample".to_string(),
            created_unix_ms: 1,
            artifacts: vec![RepairAttemptArtifact {
                role: "before_snapshot".to_string(),
                path: "target/ripr/repair-attempts/sample/artifacts/before.json".to_string(),
                sha256: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                bytes: 2,
            }],
            next_command,
            limitations: Vec::new(),
            non_claims: vec!["not merge authority".to_string()],
            after: None,
        })
    }

    #[test]
    fn repair_attempt_id_has_a_closed_shape() -> Result<(), String> {
        let id = repair_attempt_id_from_parts(
            "/repo",
            "seam:sample",
            "0123456789abcdef0123456789abcdef01234567",
            1,
            2,
            3,
        )?;
        if id.as_str().len() != REPAIR_ATTEMPT_ID_PREFIX.len() + REPAIR_ATTEMPT_ID_HEX_LEN {
            return Err(format!("unexpected repair attempt ID: {}", id.as_str()));
        }
        if RepairAttemptId::parse("attempt-not-valid").is_ok() {
            return Err("invalid repair attempt ID was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn packet_policy_selects_exact_seam_and_rejects_wrong_selection() -> Result<(), String> {
        let packet = serde_json::json!({
            "packets": [
                {"seam_id": "other", "allowed_edit_surface": ["tests/other.rs"], "forbidden_files": []},
                {"seam_id": "seam:sample", "allowed_edit_surface": ["tests/target.rs"], "forbidden_files": []}
            ]
        });
        let rendered = serde_json::to_string(&packet).map_err(|error| error.to_string())?;
        let selected = edit_cage_policy_from_packet(&rendered, "seam:sample")?;
        let selected = serde_json::to_value(selected).map_err(|error| error.to_string())?;
        if !selected.to_string().contains("tests/target.rs") {
            return Err("packet policy selected the wrong seam".to_string());
        }
        if edit_cage_policy_from_packet(&rendered, "missing-seam").is_ok() {
            return Err("packet policy accepted a missing seam".to_string());
        }
        Ok(())
    }

    #[test]
    fn repair_attempt_schema_carries_terminal_after_contract() -> Result<(), String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/ripr/repair-attempt.schema.json");
        let schema: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path).map_err(|error| format!("read schema failed: {error}"))?,
        )
        .map_err(|error| format!("decode schema failed: {error}"))?;
        let states = schema["properties"]["state"]["enum"]
            .as_array()
            .ok_or_else(|| "schema state enum missing".to_string())?;
        for state in [
            "awaiting_edit",
            "ready_to_finish",
            "stale",
            "incomparable",
            "failed",
        ] {
            if !states.iter().any(|value| value == state) {
                return Err(format!("schema omitted state {state}"));
            }
        }
        if schema["$defs"]["after"].is_null() {
            return Err("schema omitted terminal after definition".to_string());
        }
        let required = schema["$defs"]["after"]["required"]
            .as_array()
            .ok_or_else(|| "schema after required list missing".to_string())?;
        if !required.iter().any(|value| value == "attempt_id") {
            return Err("schema after omitted durable attempt_id".to_string());
        }
        Ok(())
    }

    #[test]
    fn manifest_write_is_atomic_and_round_trips() -> Result<(), String> {
        let root = test_root("manifest")?;
        let manifest = sample_manifest(&root)?;
        let path = write_repair_attempt_manifest(&root, &manifest)?;
        let raw = std::fs::read_to_string(&path)
            .map_err(|error| format!("read {} failed: {error}", path.display()))?;
        let decoded: RepairAttemptManifest = serde_json::from_str(&raw)
            .map_err(|error| format!("decode repair attempt manifest failed: {error}"))?;
        if decoded != manifest {
            return Err("repair attempt manifest changed during round trip".to_string());
        }
        let mut unknown_top_level = serde_json::to_value(&manifest)
            .map_err(|error| format!("encode manifest for unknown-field test failed: {error}"))?;
        unknown_top_level["unexpected"] = serde_json::Value::Bool(true);
        if serde_json::from_value::<RepairAttemptManifest>(unknown_top_level).is_ok() {
            return Err("manifest decoder accepted an unknown top-level field".to_string());
        }
        let mut unknown_artifact = serde_json::to_value(&manifest)
            .map_err(|error| format!("encode artifact for unknown-field test failed: {error}"))?;
        unknown_artifact["artifacts"][0]["unexpected"] = serde_json::Value::Bool(true);
        if serde_json::from_value::<RepairAttemptManifest>(unknown_artifact).is_ok() {
            return Err("manifest decoder accepted an unknown artifact field".to_string());
        }
        let leftovers = std::fs::read_dir(
            path.parent()
                .ok_or_else(|| "repair attempt manifest has no parent directory".to_string())?,
        )
        .map_err(|error| format!("read manifest directory failed: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains("tmp-"))
        .count();
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
        if leftovers != 0 {
            return Err(format!(
                "atomic manifest write left {leftovers} temporary files"
            ));
        }
        Ok(())
    }

    #[test]
    fn staged_artifacts_are_isolated_and_digest_bound() -> Result<(), String> {
        let root = test_root("artifacts")?;
        let source = root.join("before.json");
        std::fs::write(&source, b"{}")
            .map_err(|error| format!("write {} failed: {error}", source.display()))?;
        let manifest = sample_manifest(&root)?;
        let attempt_directory = repair_attempt_directory(&root, &manifest.repair_attempt_id);
        let artifacts = stage_before_artifacts(
            &root,
            &attempt_directory,
            &[BeforeArtifactSource {
                role: "before_snapshot",
                path: &source,
            }],
        )?;
        if artifacts.len() != 1
            || artifacts[0].bytes != 2
            || artifacts[0].sha256 != sha256_bytes(b"{}")
            || !root.join(&artifacts[0].path).is_file()
        {
            return Err(format!("unexpected staged artifact: {artifacts:?}"));
        }
        let staging_leftovers = std::fs::read_dir(&attempt_directory)
            .map_err(|error| format!("read attempt directory failed: {error}"))?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("tmp-"))
            .count();
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
        if staging_leftovers != 0 {
            return Err(format!(
                "staging left {staging_leftovers} temporary entries in the attempt directory"
            ));
        }
        Ok(())
    }

    #[test]
    fn repair_attempt_id_rejects_uppercase_and_preserves_value() -> Result<(), String> {
        let canonical = "repair-attempt-0123456789abcdef01234567";
        let parsed = RepairAttemptId::parse(canonical)?;
        if parsed.as_str() != canonical {
            return Err(format!(
                "repair attempt ID was rewritten during parse: {}",
                parsed.as_str()
            ));
        }
        if RepairAttemptId::parse("repair-attempt-0123456789ABCDEF01234567").is_ok() {
            return Err("uppercase repair attempt ID was accepted".to_string());
        }
        if RepairAttemptId::parse("repair-attempt-0123456789abcdef0123456").is_ok() {
            return Err("short repair attempt ID was accepted".to_string());
        }
        Ok(())
    }

    type ManifestMutation = (&'static str, fn(&mut RepairAttemptManifest));

    #[test]
    fn validate_manifest_accepts_sample_and_rejects_each_schema_invariant() -> Result<(), String> {
        let root = test_root("validate")?;
        validate_manifest(&sample_manifest(&root)?)
            .map_err(|error| format!("sample manifest was rejected: {error}"))?;
        let cases: Vec<ManifestMutation> = vec![
            ("schema_version is not 0.1", |manifest| {
                manifest.schema_version = "9.9".to_string();
            }),
            ("kind is not repair_attempt", |manifest| {
                manifest.kind = "repair_attempts".to_string();
            }),
            ("repair_attempt_id has uppercase hex", |manifest| {
                manifest.repair_attempt_id =
                    RepairAttemptId("repair-attempt-0123456789ABCDEF01234567".to_string());
            }),
            ("repair_attempt_id has wrong length", |manifest| {
                manifest.repair_attempt_id = RepairAttemptId("repair-attempt-0123".to_string());
            }),
            ("terminal state is missing after", |manifest| {
                manifest.state = RepairAttemptState::ReadyToFinish;
            }),
            ("root is empty", |manifest| {
                manifest.root.clear();
            }),
            ("repository_head is not 40 characters", |manifest| {
                manifest.repository_head = "0123".to_string();
            }),
            ("repository_head is not hexadecimal", |manifest| {
                manifest.repository_head = "g".repeat(40);
            }),
            ("producer_version is empty", |manifest| {
                manifest.producer_version.clear();
            }),
            ("seam_id is empty", |manifest| {
                manifest.seam_id.clear();
            }),
            ("artifacts is empty", |manifest| {
                manifest.artifacts.clear();
            }),
            ("next_command is empty", |manifest| {
                manifest.next_command.clear();
            }),
            ("artifact role is blank", |manifest| {
                manifest.artifacts[0].role.clear();
            }),
            ("artifact path is blank", |manifest| {
                manifest.artifacts[0].path.clear();
            }),
            ("artifact role is duplicated", |manifest| {
                let mut duplicate = manifest.artifacts[0].clone();
                duplicate.path =
                    "target/ripr/repair-attempts/sample/artifacts/other.json".to_string();
                manifest.artifacts.push(duplicate);
            }),
            ("artifact path is duplicated", |manifest| {
                let mut duplicate = manifest.artifacts[0].clone();
                duplicate.role = "other_snapshot".to_string();
                manifest.artifacts.push(duplicate);
            }),
            ("artifact sha256 is missing the prefix", |manifest| {
                manifest.artifacts[0].sha256 =
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
            }),
            ("artifact sha256 has the wrong length", |manifest| {
                manifest.artifacts[0].sha256 = "sha256:0123".to_string();
            }),
            ("artifact sha256 has uppercase hex", |manifest| {
                manifest.artifacts[0].sha256 = format!("sha256:{}", "A".repeat(64));
            }),
            ("limitations entry is empty", |manifest| {
                manifest.limitations = vec![String::new()];
            }),
            ("non_claims is empty", |manifest| {
                manifest.non_claims.clear();
            }),
            ("non_claims entry is empty", |manifest| {
                manifest.non_claims = vec![String::new()];
            }),
        ];
        for (name, mutate) in cases {
            let mut manifest = sample_manifest(&root)?;
            mutate(&mut manifest);
            if validate_manifest(&manifest).is_ok() {
                std::fs::remove_dir_all(&root)
                    .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
                return Err(format!("validate_manifest accepted invalid case: {name}"));
            }
        }
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
        Ok(())
    }

    #[test]
    fn attempt_directory_reservation_is_exclusive() -> Result<(), String> {
        let root = test_root("reserve")?;
        let attempt_id = RepairAttemptId::parse("repair-attempt-0123456789abcdef01234567")?;
        reserve_attempt_directory(&root, &attempt_id)?;
        let second = reserve_attempt_directory(&root, &attempt_id);
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
        match second {
            Err(error) if error.contains("already exists") => Ok(()),
            other => Err(format!(
                "second reservation of the same attempt was not rejected: {other:?}"
            )),
        }
    }

    #[test]
    fn write_bytes_atomic_refuses_to_overwrite_a_destination() -> Result<(), String> {
        let root = test_root("immutable")?;
        let path = root.join("attempt.json");
        write_bytes_atomic(&path, b"first")?;
        let second = write_bytes_atomic(&path, b"second");
        let contents = std::fs::read(&path)
            .map_err(|error| format!("read {} failed: {error}", path.display()))?;
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
        if second.is_ok() {
            return Err("immutable repair attempt destination was overwritten".to_string());
        }
        if contents != b"first" {
            return Err("immutable repair attempt destination bytes changed".to_string());
        }
        Ok(())
    }

    #[test]
    fn failed_begin_leaves_no_orphan_attempt_directory() -> Result<(), String> {
        let root = test_repo_root("orphan")?;
        let missing = root.join("missing-before.json");
        let result = begin_repair_attempt(
            &root,
            &root,
            "seam:sample",
            &[BeforeArtifactSource {
                role: "before_snapshot",
                path: &missing,
            }],
        );
        if result.is_ok() {
            return Err("begin_repair_attempt accepted a missing artifact source".to_string());
        }
        let attempts_root = root.join(REPAIR_ATTEMPT_DIRECTORY);
        let orphans = std::fs::read_dir(&attempts_root)
            .map_err(|error| format!("read {} failed: {error}", attempts_root.display()))?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(REPAIR_ATTEMPT_ID_PREFIX)
            })
            .count();
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
        if orphans != 0 {
            return Err(format!(
                "failed begin_repair_attempt left {orphans} orphan attempt directories"
            ));
        }
        Ok(())
    }

    #[test]
    fn failed_manifest_write_removes_staged_attempt_and_artifacts() -> Result<(), String> {
        let root = test_root("publish")?;
        let attempt_id = RepairAttemptId::parse("repair-attempt-0123456789abcdef01234567")?;
        let attempt_directory = reserve_attempt_directory(&root, &attempt_id)?;
        let source = root.join("before.json");
        std::fs::write(&source, b"{}")
            .map_err(|error| format!("write {} failed: {error}", source.display()))?;
        // Force the manifest publish to fail after staging succeeds: the
        // manifest destination already exists as a directory, so the
        // exclusive link in write_bytes_atomic fails closed.
        std::fs::create_dir(attempt_directory.join(REPAIR_ATTEMPT_MANIFEST))
            .map_err(|error| format!("create occupied manifest path failed: {error}"))?;
        let result = complete_repair_attempt(
            &root,
            &attempt_directory,
            AttemptPublication {
                root_argument: &root,
                seam_id: "seam:sample",
                repository_head: "0123456789abcdef0123456789abcdef01234567".to_string(),
                created_unix_ms: 1,
                repair_attempt_id: attempt_id,
                sources: &[BeforeArtifactSource {
                    role: "before_snapshot",
                    path: &source,
                }],
            },
        );
        let attempt_remaining = attempt_directory.exists();
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
        if result.is_ok() {
            return Err(
                "complete_repair_attempt published over an occupied manifest path".to_string(),
            );
        }
        if attempt_remaining {
            return Err(
                "failed manifest write left the attempt directory and staged artifacts behind"
                    .to_string(),
            );
        }
        Ok(())
    }

    #[test]
    fn exact_attempt_selection_survives_same_seam_concurrency() -> Result<(), String> {
        let root = test_repo_root("exact-selection")?;
        let first = prepare_sample_attempt(&root, "seam:sample", "first")?;
        let second = prepare_sample_attempt(&root, "seam:sample", "second")?;

        let exact = resolve_awaiting_repair_attempt(
            &root,
            Some(first.manifest.repair_attempt_id.as_str()),
            None,
        )?;
        if exact.attempt_id != first.manifest.repair_attempt_id
            || exact.seam_id != "seam:sample"
            || !exact
                .before_snapshot_path
                .starts_with(repair_attempt_directory(
                    &root,
                    &first.manifest.repair_attempt_id,
                ))
            || !exact.packet_path.starts_with(repair_attempt_directory(
                &root,
                &first.manifest.repair_attempt_id,
            ))
        {
            return Err(format!(
                "exact attempt resolved the wrong inputs: {exact:?}"
            ));
        }

        let ambiguous = resolve_awaiting_repair_attempt(&root, None, Some("seam:sample"));
        match ambiguous {
            Err(error) if error.contains("found 2") && error.contains("--attempt") => {}
            other => {
                return Err(format!(
                    "same-seam compatibility selection was not rejected: {other:?}"
                ));
            }
        }

        let second_exact = resolve_awaiting_repair_attempt(
            &root,
            Some(second.manifest.repair_attempt_id.as_str()),
            None,
        )?;
        let wrong_packet = finish_repair_attempt(
            &root,
            &first.manifest.repair_attempt_id,
            &second_exact.packet_path,
        );
        match wrong_packet {
            Err(error) if error.contains("retained agent_packet") => {}
            other => {
                return Err(format!(
                    "finish accepted another attempt's retained packet: {other:?}"
                ));
            }
        }

        let test_path = root.join("tests/target.rs");
        let test_parent = test_path
            .parent()
            .ok_or_else(|| "test path has no parent".to_string())?;
        std::fs::create_dir_all(test_parent)
            .map_err(|error| format!("create {} failed: {error}", test_parent.display()))?;
        std::fs::write(&test_path, "#[test]\nfn focused() {}\n")
            .map_err(|error| format!("write {} failed: {error}", test_path.display()))?;

        let after =
            finish_repair_attempt(&root, &first.manifest.repair_attempt_id, &exact.packet_path)?;
        if after.attempt_id != first.manifest.repair_attempt_id
            || !after.current
            || after.verdict.status != crate::edit_cage::EditCageVerdictStatus::Compliant
        {
            return Err(format!("unexpected exact finish result: {after:?}"));
        }
        let (_, first_manifest) =
            load_repair_attempt_by_id(&root, &first.manifest.repair_attempt_id)?;
        let (_, second_manifest) =
            load_repair_attempt_by_id(&root, &second.manifest.repair_attempt_id)?;
        if first_manifest.state != RepairAttemptState::ReadyToFinish
            || second_manifest.state != RepairAttemptState::AwaitingEdit
        {
            return Err(format!(
                "exact finish changed the wrong attempt states: first={:?}, second={:?}",
                first_manifest.state, second_manifest.state
            ));
        }

        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("remove {} failed: {error}", root.display()))?;
        Ok(())
    }

    fn prepare_sample_attempt(
        root: &Path,
        seam_id: &str,
        label: &str,
    ) -> Result<BeginRepairAttemptResult, String> {
        let workflow = root.join("target/ripr/workflow");
        std::fs::create_dir_all(&workflow)
            .map_err(|error| format!("create {} failed: {error}", workflow.display()))?;
        let before = workflow.join(format!("before-{label}.json"));
        let packet = workflow.join(format!("packet-{label}.json"));
        let baseline = workflow.join(format!("baseline-{label}.json"));
        std::fs::write(&before, b"{}")
            .map_err(|error| format!("write {} failed: {error}", before.display()))?;
        let packet_value = serde_json::json!({
            "seam_id": seam_id,
            "allowed_edit_surface": ["tests/target.rs"],
            "forbidden_files": []
        });
        let packet_text = serde_json::to_string_pretty(&packet_value)
            .map_err(|error| format!("serialize test packet failed: {error}"))?;
        std::fs::write(&packet, packet_text.as_bytes())
            .map_err(|error| format!("write {} failed: {error}", packet.display()))?;
        let policy = edit_cage_policy_from_packet(&packet_text, seam_id)?;
        write_edit_cage_baseline(root, &baseline, &policy)?;
        begin_repair_attempt(
            root,
            root,
            seam_id,
            &[
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
        )
    }

    fn test_repo_root(label: &str) -> Result<PathBuf, String> {
        let root = test_root(label)?;
        run_git(&root, &["init"])?;
        run_git(
            &root,
            &["config", "user.email", "ripr-test@example.invalid"],
        )?;
        run_git(&root, &["config", "user.name", "RIPR Test"])?;
        let readme = root.join("README.md");
        std::fs::write(&readme, "# test\n")
            .map_err(|error| format!("write {} failed: {error}", readme.display()))?;
        run_git(&root, &["add", "."])?;
        run_git(&root, &["commit", "--no-gpg-sign", "-m", "initial"])?;
        Ok(root)
    }

    fn run_git(root: &Path, args: &[&str]) -> Result<(), String> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|error| format!("run git {args:?} failed: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }
}
