//! Durable repair-attempt identity and before-phase manifest authority.
//!
//! The current `agent repair` driver still exposes compatibility artifacts under
//! `target/ripr/workflow`. This module adds one attempt-specific, atomically
//! published record without changing the after-phase command contract. A later
//! #2927 slice will make `--attempt <id>` the finishing authority.

use crate::agent::loop_commands::{display_path, shell_arg};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const REPAIR_ATTEMPT_SCHEMA_VERSION: &str = "0.1";
pub(crate) const REPAIR_ATTEMPT_DIRECTORY: &str = "target/ripr/repair-attempts";
const REPAIR_ATTEMPT_MANIFEST: &str = "attempt.json";
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
pub(crate) struct RepairAttemptArtifact {
    pub(crate) role: String,
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
                "ripr agent repair --root {} --seam-id {} --phase after",
                shell_arg(&display_path(publication.root_argument)),
                shell_arg(publication.seam_id)
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
                    "after phase remains seam-selected until #2927 PR B adds --attempt consumption"
                        .to_string(),
                ],
                non_claims: vec![
                    "RIPR does not author or apply the focused test edit".to_string(),
                    "prepared evidence does not mean the gap is fixed or verified".to_string(),
                    "this manifest does not authorize mutation execution or merge".to_string(),
                ],
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
    Ok(path)
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
        || manifest.state != RepairAttemptState::AwaitingEdit
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
    if manifest.artifacts.iter().any(|artifact| {
        artifact.role.is_empty() || artifact.path.is_empty() || !is_sha256_digest(&artifact.sha256)
    }) {
        return Err("repair attempt manifest contains an invalid artifact".to_string());
    }
    Ok(())
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
            next_command: "ripr agent repair --root . --seam-id seam:sample --phase after"
                .to_string(),
            limitations: Vec::new(),
            non_claims: vec!["not merge authority".to_string()],
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
            ("state is not awaiting_edit", |manifest| {
                manifest.state = RepairAttemptState::Prepared;
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
