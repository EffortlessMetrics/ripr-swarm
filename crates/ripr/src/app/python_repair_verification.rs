//! Verification/acceptance phase for one trust-bound Python repair attempt
//! (RIPR-SPEC-0176, #3570).
//!
//! The phase closes the prepare→apply→verify chain the merged rounds built:
//! it revalidates every identity the retained #3569 apply record and the
//! #3568 binding pin (attempt, worktree, source tree, patch, config, input,
//! target, command), executes ONLY the producer-owned typed `CommandSpec`
//! the retained packet declares — under an explicit verification
//! authorization, through the bounded execution rails of the existing
//! `verification_execution` module — reruns the current RIPR analysis
//! against the exact post-edit state, compares the intended native Python
//! behavior/gap evidence before and after by exact native identity, and
//! publishes ONE immutable candidate receipt.
//!
//! Separation law (the receipt's reason to exist): execution and static
//! movement are two independent fields with no derivation between them. A
//! passed command never forces a movement state, and a movement state never
//! implies a command outcome; the receipt records both plus the evidence
//! digests, and no lifecycle, acceptance, or closure state exists anywhere
//! in its schema.
//!
//! Claim boundary: the receipt is draft evidence only. It claims no
//! lifecycle state, no repair correctness, no support/gate/badge/promotion
//! outcome; real mutation testing confirms behavior-change detection later.

use crate::app::python_repair_binding::APPLY_RECORD_COMPAT_PATH;
use crate::app::python_repair_binding::{self, RetainedBinding, VerifiedSelection};
use crate::app::repair_attempt::{
    RepairAttemptId, RepairAttemptState, find_manifest_artifact_by_role, load_edit_cage_policy,
    load_repair_attempt_manifest, replace_file_atomically,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;
use std::{io::Write, path::Path, path::PathBuf};

/// Wire schema of the verification receipt. Versioned independently of the
/// durable attempt manifest and of the driver binding record.
pub(crate) const RECEIPT_SCHEMA_VERSION: &str = "0.1";
pub(crate) const RECEIPT_KIND: &str = "python_repair_verification_receipt";
/// The phase extends the #3568/#3569 model, so it carries that spec identity;
/// it introduces no new spec number.
pub(crate) const RECEIPT_SPEC: &str = "RIPR-SPEC-0176";
/// Repository-global compatibility projection of the immutable receipt.
pub(crate) const RECEIPT_COMPAT_PATH: &str =
    "target/ripr/workflow/python-repair-driver-verification.json";
/// Repository-global compatibility projection of the bounded execution
/// observation (the full preflight plus the committed result of the packet
/// route). The receipt restates the observation without host-local paths;
/// this file is the workflow-side full record.
pub(crate) const EXECUTION_RESULT_COMPAT_PATH: &str =
    "target/ripr/workflow/python-repair-driver-verification-execution.json";
/// The fresh post-edit analysis the movement comparison binds (never the
/// apply phase's after snapshot, which binds the apply-time binary instead).
const AFTER_VERIFICATION_SNAPSHOT_REL: &str =
    "target/ripr/workflow/after-verification.repo-exposure.json";

/// The explicit verification authorization signals. Both are required
/// together; the phase never infers, defaults, or persists an authorization.
const VERIFY_AUTHORIZATION_STATUS: &str = "granted";
const VERIFY_AUTHORIZATION_METHOD: &str = "explicit-operator-flags";

/// The standing non-claims of every verification receipt.
pub(crate) const VERIFICATION_NON_CLAIMS: [&str; 3] = [
    "no lifecycle, acceptance, or closure state is claimed by the verification record",
    "no repair correctness is claimed by the verification record",
    "no support, gate, badge, or promotion claim is made",
];

const CLAIM_BOUNDARY: &str = "Execution observation and static before/after movement are separate draft evidence axes; real mutation testing confirms behavior-change detection later.";

/// Closed execution-state vocabulary (shared with the #3568 corpus model).
pub(crate) const EXECUTION_STATES: [&str; 7] = [
    "passed",
    "failed",
    "timed_out",
    "cancelled",
    "unavailable",
    "not_run",
    "invalid",
];

/// Closed movement-state vocabulary (shared with the #3568 corpus model).
pub(crate) const MOVEMENT_STATES: [&str; 7] = [
    "closed",
    "improved",
    "unchanged",
    "regressed",
    "limited",
    "stale",
    "uncertain",
];

/// Closed rollback-state vocabulary.
pub(crate) const ROLLBACK_STATES: [&str; 3] = ["proved", "blocked", "not_run"];

/// CLI-provided verification authorization: `authorized` is true only when
/// the explicit flag was passed; `authority` is the non-empty operator/agent
/// identity supplied with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyAuthorization {
    pub(crate) authorized: bool,
    pub(crate) authority: Option<String>,
}

impl VerifyAuthorization {
    /// The typed refusal names the missing signals; the phase never
    /// authorizes a verification automatically.
    fn verify(&self) -> Result<String, String> {
        if !self.authorized {
            return Err(
                "python repair verification refuses to run: explicit --verify-authorized and --verify-authority <identity> are required; the driver never authorizes a verification automatically"
                    .to_string(),
            );
        }
        self.authority
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                "python repair verification refuses to run: --verify-authorized requires --verify-authority <identity>"
                    .to_string()
            })
    }
}

/// Inputs of one verification phase run.
pub(crate) struct VerificationOptions<'a> {
    pub(crate) root: &'a Path,
    pub(crate) attempt_id: &'a str,
    pub(crate) authorization: VerifyAuthorization,
    pub(crate) rollback: bool,
}

/// SHA-256 over bytes, lowercase hex (no prefix) — the binding digest shape.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut rendered = String::with_capacity(64);
    for byte in digest {
        rendered.push_str(&format!("{byte:02x}"));
    }
    rendered
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn as_object<'a>(
    value: &'a Value,
    subject: &str,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    value.as_object().ok_or_else(|| {
        format!("python repair verification: subject=`{subject}`: must be a JSON object")
    })
}

fn require_string(
    subject: &str,
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, String> {
    match object.get(field) {
        None => Err(format!(
            "python repair verification: subject=`{subject}` field=`{field}`: required field is missing"
        )),
        Some(Value::Null) => Err(format!(
            "python repair verification: subject=`{subject}` field=`{field}`: a present null is not a value"
        )),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(text.clone()),
        Some(Value::String(_)) => Err(format!(
            "python repair verification: subject=`{subject}` field=`{field}`: required field must be non-empty"
        )),
        Some(_) => Err(format!(
            "python repair verification: subject=`{subject}` field=`{field}`: field must be a string"
        )),
    }
}

fn opt_string(object: &serde_json::Map<String, Value>, field: &str) -> Option<String> {
    match object.get(field) {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    }
}

fn opt_u64(object: &serde_json::Map<String, Value>, field: &str) -> Option<u64> {
    object.get(field).and_then(Value::as_u64)
}

fn opt_bool(object: &serde_json::Map<String, Value>, field: &str) -> Option<bool> {
    object.get(field).and_then(Value::as_bool)
}

// ---------------------------------------------------------------------------
// Revalidation (before anything runs)
// ---------------------------------------------------------------------------

/// Everything the phase revalidated and needs downstream, bound together so
/// the receipt restates verified identities instead of re-deriving them.
struct RevalidatedAttempt {
    verified: VerifiedSelection,
    binding_artifact_sha256: String,
    manifest_seam_id: String,
    packet_path: PathBuf,
    before_snapshot_path: PathBuf,
    packet_sha256: String,
    before_snapshot_sha256: String,
    patch_sha256: String,
    changed_paths: Vec<String>,
    allowed_surface: std::collections::BTreeSet<String>,
    repository_head: String,
    config_profile: String,
    /// The producer-owned typed verify route (digest + display), or `None`
    /// when the retained packet declares no canonical typed route.
    command_route: Option<(String, String)>,
}

/// Revalidates attempt, worktree, source tree, patch, config, input, target,
/// and command identities against the retained #3569 apply record and the
/// #3568 binding. Every drift refuses before execution with a typed error
/// naming the drifted identity.
fn revalidate_for_verification(
    root: &Path,
    attempt_id: &RepairAttemptId,
    authority: &str,
) -> Result<RevalidatedAttempt, String> {
    let manifest = load_repair_attempt_manifest(root, attempt_id)?;

    // Durable state gate: only an applied, current, compliant attempt
    // carries a post-edit state a verification could observe.
    if manifest.state != RepairAttemptState::ReadyToFinish {
        return Err(match manifest.state {
            RepairAttemptState::AwaitingEdit => format!(
                "python repair verification refuses attempt `{}`: the attempt is awaiting_edit; the verification phase requires an applied edit (run the after phase first)",
                attempt_id.as_str()
            ),
            RepairAttemptState::Stale => format!(
                "python repair verification refuses attempt `{}`: the attempt is stale (the repository moved after the edit); a drifted tree requires a new re-authorized attempt",
                attempt_id.as_str()
            ),
            RepairAttemptState::Failed => format!(
                "python repair verification refuses attempt `{}`: the attempt failed its edit cage; an escaped edit is retained failure evidence, never a verification subject",
                attempt_id.as_str()
            ),
            RepairAttemptState::Incomparable => format!(
                "python repair verification refuses attempt `{}`: the attempt is incomparable; the edit cage could not measure the delta",
                attempt_id.as_str()
            ),
            RepairAttemptState::Prepared => format!(
                "python repair verification refuses attempt `{}`: the attempt is prepared but its after phase never ran",
                attempt_id.as_str()
            ),
            RepairAttemptState::ReadyToFinish => String::new(),
        });
    }
    let after = manifest.after.as_ref().ok_or_else(|| {
        format!(
            "python repair verification refuses attempt `{}`: the ready_to_finish attempt carries no after verdict",
            attempt_id.as_str()
        )
    })?;
    if !after.current || after.verdict.status != crate::edit_cage::EditCageVerdictStatus::Compliant
    {
        return Err(format!(
            "python repair verification refuses attempt `{}`: the after verdict is not a current compliant edit (current {}, cage status {:?})",
            attempt_id.as_str(),
            after.current,
            after.verdict.status
        ));
    }

    // Tree identity: the exact post-edit tree the finish measured must still
    // be checked out. A moved HEAD is a different tree; the phase refuses
    // instead of verifying against drifted state.
    let repository_head = crate::agent::artifact::current_git_head(root)?;
    if repository_head != manifest.repository_head || repository_head != after.repository_head {
        return Err(format!(
            "python repair verification refuses attempt `{}`: stale tree — the attempt pins head `{}` (after `{}`) but the repository HEAD is `{repository_head}`; a moved repository requires a new re-authorized attempt",
            attempt_id.as_str(),
            manifest.repository_head,
            after.repository_head
        ));
    }

    // Source-tree identity: the worktree must still carry the exact applied
    // edit ON THE DECLARED EDIT SURFACE. The edit-cage baseline is
    // re-evaluated through the same authority the finish used; the fresh
    // verdict must stay compliant, and the fresh changed paths restricted to
    // the cage's allowed surface must equal the retained ones. Paths outside
    // the cage (the driver's own target/ripr workflow artifacts, which the
    // phases keep writing between finish and verification) are not the edit
    // and are excluded from the identity on both sides.
    let baseline = load_baseline(root, &manifest, attempt_id)?;
    let policy = load_edit_cage_policy(root, attempt_id)?;
    let (_, verdict) = crate::edit_cage::evaluate_repository_edit_cage_with_delta(&baseline)?;
    if verdict.status != crate::edit_cage::EditCageVerdictStatus::Compliant {
        return Err(format!(
            "python repair verification refuses attempt `{}`: stale tree — the worktree no longer satisfies the edit cage ({:?}); the exact post-edit state is gone",
            attempt_id.as_str(),
            verdict.status
        ));
    }
    let allowed_paths: std::collections::BTreeSet<String> = policy
        .allowed_edit_surface
        .iter()
        .map(|rule| rule.path().to_string())
        .collect();
    let on_surface = |paths: &[String]| -> Vec<String> {
        paths
            .iter()
            .filter(|path| allowed_paths.contains(*path))
            .cloned()
            .collect()
    };
    let fresh_surface = on_surface(&verdict.changed_paths);
    let retained_surface = on_surface(&after.verdict.changed_paths);
    if fresh_surface != retained_surface || fresh_surface.is_empty() {
        return Err(format!(
            "python repair verification refuses attempt `{}`: stale tree — the worktree no longer carries the applied edit on the declared surface (fresh {fresh_surface:?}, retained {retained_surface:?}); the exact post-edit state is gone",
            attempt_id.as_str()
        ));
    }
    let patch_sha256 = after.delta_sha256.trim_start_matches("sha256:").to_string();
    if !is_sha256_hex(&patch_sha256) {
        return Err(
            "python repair verification refuses the attempt: the durable patch digest is malformed"
                .to_string(),
        );
    }

    // Input identity: the retained packet is the command authority and the
    // retained before snapshot is the movement baseline; both are
    // digest-pinned by the attempt manifest (re-verified at load).
    let packet_artifact = find_manifest_artifact_by_role(&manifest, "agent_packet").ok_or_else(
        || {
            format!(
                "python repair verification refuses attempt `{}`: the attempt carries no retained agent packet",
                attempt_id.as_str()
            )
        },
    )?;
    let packet_path = root.join(&packet_artifact.path);
    let packet_bytes = std::fs::read(&packet_path).map_err(|error| {
        format!(
            "python repair verification: read retained packet {} failed: {error}",
            packet_path.display()
        )
    })?;
    let packet_sha256 = sha256_hex(&packet_bytes);
    let before_artifact = find_manifest_artifact_by_role(&manifest, "before_snapshot")
        .ok_or_else(|| {
            format!(
                "python repair verification refuses attempt `{}`: the attempt carries no retained before snapshot",
                attempt_id.as_str()
            )
        })?;
    let before_snapshot_path = root.join(&before_artifact.path);
    let before_snapshot_sha256 = before_artifact
        .sha256
        .trim_start_matches("sha256:")
        .to_string();
    if !is_sha256_hex(&before_snapshot_sha256) {
        return Err(
            "python repair verification refuses the attempt: the retained before-snapshot digest is malformed"
                .to_string(),
        );
    }

    // Trust binding: digest recompute + identity agreement, including the
    // selection manifest bytes, the selection row digest, the target
    // alignment with the packet's selected edit target, the packet digest,
    // and the authorization authority. The verify invocation must re-affirm
    // the SAME authority that authorized the edit.
    let binding = python_repair_binding::load_retained_binding(root, attempt_id)?.ok_or_else(
        || {
            format!(
                "python repair verification refuses attempt `{}`: the attempt records no python repair-trust binding; the verification phase runs only for trust-bound attempts",
                attempt_id.as_str()
            )
        },
    )?;
    let verified = python_repair_binding::reverify_for_apply(
        &manifest.seam_id,
        &policy,
        &packet_bytes,
        &binding,
        &python_repair_binding::EditAuthorization {
            authorized: true,
            authority: Some(authority.to_string()),
        },
    )?;
    python_repair_binding::confirm_manifest_unchanged(&binding)?;

    // Apply-record identity: the retained #3569 record must agree with the
    // durable attempt, the binding artifact chain, the patch, the packet,
    // and the resulting head.
    let apply_record_path = root.join(APPLY_RECORD_COMPAT_PATH);
    let apply_text = std::fs::read_to_string(&apply_record_path).map_err(|error| {
        format!(
            "python repair verification: read apply record {} failed: {error}",
            apply_record_path.display()
        )
    })?;
    let apply_value: Value = serde_json::from_str(&apply_text).map_err(|error| {
        format!(
            "python repair verification: apply record {} is not well-formed JSON: {error}",
            apply_record_path.display()
        )
    })?;
    let apply = as_object(&apply_value, "apply record")?;
    check_apply_record(
        apply,
        attempt_id,
        &binding,
        &verified,
        ApplyRecordPins {
            patch_sha256: &patch_sha256,
            packet_sha256: &packet_sha256,
            before_snapshot_sha256: &before_snapshot_sha256,
            repository_head: &repository_head,
        },
    )?;

    // Config identity: the analyzed root's config profile must still be the
    // profile the apply record retained.
    let config_profile = detect_config_profile(root);
    let record_profile = apply
        .get("config")
        .and_then(|config| config.get("profile"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if record_profile != config_profile {
        return Err(format!(
            "python repair verification refuses attempt `{}`: stale config — the apply record pins config profile `{record_profile}` but the analyzed root now detects `{config_profile}`",
            attempt_id.as_str()
        ));
    }

    // Command identity: the packet's headline verify route must reproduce as
    // one typed CommandSpec before anything runs. The authoritative route
    // validation (consistency plus producer provenance) is re-run by the
    // bounded execution rails. A packet that declares NO canonical typed
    // verify route is a producer state, not a tamper — the packet bytes are
    // already digest-pinned — so the phase records the route as unavailable
    // and still compares movement; it never executes a reconstructed command.
    let packet_text = String::from_utf8(packet_bytes).map_err(|error| {
        format!("python repair verification: retained packet is not UTF-8: {error}")
    })?;
    let packet_value: Value = serde_json::from_str(&packet_text).map_err(|error| {
        format!("python repair verification: retained packet is not well-formed JSON: {error}")
    })?;
    // The headline route lives on the single packet entry of the envelope
    // (the same place the bounded rails read it from).
    let packet_entry = packet_value
        .get("packets")
        .and_then(Value::as_array)
        .and_then(|packets| packets.first())
        .cloned()
        .ok_or_else(|| {
            format!(
                "python repair verification refuses attempt `{}`: the retained packet envelope carries no packet entry",
                attempt_id.as_str()
            )
        })?;
    let command_display = packet_entry
        .get("verify_command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let command_route: Option<(String, String)> =
        match crate::agent::command_specs::agent_command_spec_from_display(&command_display) {
            Some(command_spec) => {
                let digest = crate::domain::command_spec_sha256(&command_spec).map_err(|error| {
                    format!(
                        "python repair verification refuses attempt `{}`: the verify command digest failed: {error}",
                        attempt_id.as_str()
                    )
                })?;
                let digest = digest.trim_start_matches("sha256:").to_string();
                if !is_sha256_hex(&digest) {
                    return Err(
                        "python repair verification refuses the attempt: the verify command digest is malformed"
                            .to_string(),
                    );
                }
                Some((digest, command_display))
            }
            None if command_display.trim().is_empty() => None,
            None => {
                return Err(format!(
                    "python repair verification refuses attempt `{}`: stale command — the retained packet's verify route is not a canonical typed route; a non-canonical display is never executed",
                    attempt_id.as_str()
                ));
            }
        };

    Ok(RevalidatedAttempt {
        verified,
        binding_artifact_sha256: binding.artifact_sha256,
        manifest_seam_id: manifest.seam_id,
        packet_path,
        before_snapshot_path,
        packet_sha256,
        before_snapshot_sha256,
        patch_sha256,
        changed_paths: after.verdict.changed_paths.clone(),
        allowed_surface: allowed_paths,
        repository_head,
        config_profile,
        command_route,
    })
}

/// Loads and digest-verifies the retained edit-cage baseline (the same
/// artifact the finish authority used).
fn load_baseline(
    root: &Path,
    manifest: &crate::app::repair_attempt::RepairAttemptManifest,
    attempt_id: &RepairAttemptId,
) -> Result<crate::edit_cage::AttemptBaseline, String> {
    let artifact = find_manifest_artifact_by_role(manifest, "edit_cage_baseline").ok_or_else(
        || {
            format!(
                "python repair verification refuses attempt `{}`: the attempt carries no edit-cage baseline",
                attempt_id.as_str()
            )
        },
    )?;
    let path = root.join(&artifact.path);
    let bytes =
        std::fs::read(&path).map_err(|error| format!("read {} failed: {error}", path.display()))?;
    if sha256_hex(&bytes) != artifact.sha256.trim_start_matches("sha256:") {
        return Err(
            "python repair verification refuses the attempt: the edit-cage baseline binding failed"
                .to_string(),
        );
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("decode edit-cage baseline failed: {error}"))
}

/// The pinned identities the apply record must restate, sliced out of the
/// revalidated attempt by the caller.
struct ApplyRecordPins<'a> {
    patch_sha256: &'a str,
    packet_sha256: &'a str,
    before_snapshot_sha256: &'a str,
    repository_head: &'a str,
}

/// The apply record must agree with the durable attempt, the binding
/// artifact chain, the patch, the input digests, and the resulting head.
fn check_apply_record(
    apply: &serde_json::Map<String, Value>,
    attempt_id: &RepairAttemptId,
    binding: &RetainedBinding,
    verified: &VerifiedSelection,
    pins: ApplyRecordPins<'_>,
) -> Result<(), String> {
    let ApplyRecordPins {
        patch_sha256,
        packet_sha256,
        before_snapshot_sha256,
        repository_head,
    } = pins;
    let record_attempt = require_string("apply record", apply, "durable_attempt_id")?;
    if record_attempt != attempt_id.as_str() {
        return Err(format!(
            "python repair verification refuses the attempt: the apply record names durable attempt `{record_attempt}`, not `{}`",
            attempt_id.as_str()
        ));
    }
    let record_binding_digest = require_string("apply record", apply, "binding_artifact_sha256")?;
    if record_binding_digest != binding.artifact_sha256 {
        return Err(format!(
            "python repair verification refuses the attempt: the apply record chains binding artifact `{record_binding_digest}` but the retained binding digests to `{}`",
            binding.artifact_sha256
        ));
    }
    let record_head = require_string("apply record", apply, "repository_head")?;
    if record_head != verified.head || repository_head != verified.head {
        return Err(format!(
            "python repair verification refuses the attempt: stale tree — the apply record pins head `{record_head}`, the binding pins `{}`, and the repository HEAD is `{repository_head}`; all three must agree",
            verified.head
        ));
    }
    let trust = as_object(
        apply.get("trust").ok_or_else(|| {
            "python repair verification: apply record is missing trust".to_string()
        })?,
        "apply record trust",
    )?;
    let record_trust_attempt = require_string("apply record trust", trust, "attempt_id")?;
    if record_trust_attempt != verified.attempt_id {
        return Err(format!(
            "python repair verification refuses the attempt: the apply record binds trust attempt `{record_trust_attempt}` but the retained binding binds `{}`",
            verified.attempt_id
        ));
    }
    let record_selection_digest = require_string("apply record trust", trust, "selection_digest")?;
    if record_selection_digest != verified.selection_digest {
        return Err(
            "python repair verification refuses the attempt: the apply record's selection digest leaves the retained binding"
                .to_string(),
        );
    }
    let record_apply = as_object(
        apply.get("apply").ok_or_else(|| {
            "python repair verification: apply record is missing its apply block".to_string()
        })?,
        "apply record apply block",
    )?;
    let record_patch = require_string("apply record apply block", record_apply, "patch_sha256")?;
    if record_patch != patch_sha256 {
        return Err(format!(
            "python repair verification refuses the attempt: stale patch — the apply record pins patch `{record_patch}` but the durable verdict digests to `{patch_sha256}`"
        ));
    }
    if !opt_bool(record_apply, "current").unwrap_or(false) {
        return Err(
            "python repair verification refuses the attempt: the apply record does not carry a current repository"
                .to_string(),
        );
    }
    let record_cage = require_string("apply record apply block", record_apply, "cage_status")?;
    if record_cage != "compliant" {
        return Err(format!(
            "python repair verification refuses the attempt: the apply record's edit-cage decision is `{record_cage}`, not compliant"
        ));
    }
    let input = as_object(
        apply.get("input").ok_or_else(|| {
            "python repair verification: apply record is missing input".to_string()
        })?,
        "apply record input",
    )?;
    let record_packet = require_string("apply record input", input, "packet_sha256")?;
    if record_packet != packet_sha256 {
        return Err(format!(
            "python repair verification refuses the attempt: stale packet — the apply record pins packet `{record_packet}` but the retained packet digests to `{packet_sha256}`"
        ));
    }
    let record_before = require_string("apply record input", input, "before_snapshot_sha256")?;
    if record_before != before_snapshot_sha256 {
        return Err(format!(
            "python repair verification refuses the attempt: stale input — the apply record pins before snapshot `{record_before}` but the attempt retains `{before_snapshot_sha256}`"
        ));
    }
    Ok(())
}

/// The analyzed root's config identity (the same real producer the binding
/// uses: `ripr.toml` presence under the analyzed root).
fn detect_config_profile(root: &Path) -> String {
    if root.join("ripr.toml").is_file() {
        "subject-ripr-toml".to_string()
    } else {
        "default".to_string()
    }
}

// ---------------------------------------------------------------------------
// Execution mapping
// ---------------------------------------------------------------------------

/// The execution observation the receipt retains, restated from the bounded
/// rails' committed response. Host-local paths (the root identity, the
/// artifact paths) are deliberately dropped here; the digests are the
/// commitments.
struct ObservedExecution {
    state: &'static str,
    process_disposition: String,
    exit_status: Option<i32>,
    exit_signal: Option<i32>,
    stdout_sha256: Option<String>,
    stderr_sha256: Option<String>,
    stdout_bytes: Option<u64>,
    stderr_bytes: Option<u64>,
    stdout_truncated: bool,
    stderr_truncated: bool,
    currentness: Option<String>,
    duration_ms: Option<u64>,
    cancellation_requested: bool,
    reason: Option<String>,
}

/// The typed observation for a packet that declares no canonical producer
/// route: the runner is unavailable, nothing runs, nothing is reconstructed.
fn unavailable_route_observation() -> ObservedExecution {
    ObservedExecution {
        state: "unavailable",
        process_disposition: "route_unavailable".to_string(),
        exit_status: None,
        exit_signal: None,
        stdout_sha256: None,
        stderr_sha256: None,
        stdout_bytes: None,
        stderr_bytes: None,
        stdout_truncated: false,
        stderr_truncated: false,
        currentness: None,
        duration_ms: None,
        cancellation_requested: false,
        reason: Some(
            "the retained packet declares no canonical typed verify route; the packet producer owns the verification route and this packet carries none"
                .to_string(),
        ),
    }
}

/// Executes ONLY the packet's producer-owned verify route through the bounded
/// rails and maps the observation onto the closed execution vocabulary.
fn execute_packet_route(root: &Path, packet_path: &Path) -> Result<ObservedExecution, String> {
    let result_path = root.join(EXECUTION_RESULT_COMPAT_PATH);
    let outcome = crate::app::verification_execution::execute_verify_packet(
        root,
        packet_path,
        &result_path,
        // Authorization was verified and re-affirmed against the retained
        // binding before this call; the rails see the granted pair.
        true,
        None,
    );
    if outcome.failed {
        let state = match outcome.disposition {
            "verification_command_not_found" => "unavailable",
            _ => "invalid",
        };
        let reason =
            outcome_reason(&outcome.rendered).unwrap_or_else(|| outcome.disposition.to_string());
        return Ok(ObservedExecution {
            state,
            process_disposition: "rejected_before_execution".to_string(),
            exit_status: None,
            exit_signal: None,
            stdout_sha256: None,
            stderr_sha256: None,
            stdout_bytes: None,
            stderr_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
            currentness: None,
            duration_ms: None,
            cancellation_requested: false,
            reason: Some(reason),
        });
    }
    let response: Value = serde_json::from_str(outcome.rendered.trim()).map_err(|error| {
        format!(
            "python repair verification: the committed execution response is not well-formed JSON: {error}"
        )
    })?;
    let result = response
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            "python repair verification: the committed execution response carries no result"
                .to_string()
        })?;
    let disposition = require_string("execution result", result, "process_disposition")?;
    let exit_status = result.get("exit_status").and_then(Value::as_i64);
    let state = map_execution_state(&disposition, exit_status)?;
    Ok(ObservedExecution {
        state,
        process_disposition: disposition,
        exit_status: exit_status.map(|value| value as i32),
        exit_signal: result
            .get("exit_signal")
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        stdout_sha256: opt_string(result, "stdout_sha256"),
        stderr_sha256: opt_string(result, "stderr_sha256"),
        stdout_bytes: opt_u64(result, "stdout_bytes"),
        stderr_bytes: opt_u64(result, "stderr_bytes"),
        stdout_truncated: opt_bool(result, "stdout_truncated").unwrap_or(false),
        stderr_truncated: opt_bool(result, "stderr_truncated").unwrap_or(false),
        currentness: opt_string(result, "currentness"),
        duration_ms: opt_u64(result, "duration_ms"),
        cancellation_requested: opt_bool(result, "cancellation_requested").unwrap_or(false),
        reason: response
            .get("reason")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn outcome_reason(rendered: &str) -> Option<String> {
    let value: Value = serde_json::from_str(rendered.trim()).ok()?;
    value
        .get("reason")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The closed mapping from the bounded observation to the execution
/// vocabulary. `completed` with the route's accepted exit code (0) is the
/// only pass; every other terminal state keeps its own name so timeout,
/// cancellation, spawn failure, non-zero exit, and unobservable runs stay
/// distinct.
fn map_execution_state(
    disposition: &str,
    exit_status: Option<i64>,
) -> Result<&'static str, String> {
    match disposition {
        "completed" => match exit_status {
            Some(0) => Ok("passed"),
            Some(_) => Ok("failed"),
            None => Err(
                "python repair verification: a completed execution carries no exit status"
                    .to_string(),
            ),
        },
        "timed_out" => Ok("timed_out"),
        "cancelled" => Ok("cancelled"),
        "failed_to_start" => Ok("unavailable"),
        "output_limit_exceeded" => Ok("invalid"),
        other => Err(format!(
            "python repair verification: unknown execution disposition `{other}`"
        )),
    }
}

// ---------------------------------------------------------------------------
// After analysis and movement
// ---------------------------------------------------------------------------

/// Writes the fresh post-edit repo-exposure snapshot the movement comparison
/// binds (the same producer the after phase uses, re-run now so the analyzer
/// identity is the current binary/config/input).
fn write_after_verification_snapshot(root: &Path) -> Result<(String, String), String> {
    let config = crate::config::load_for_root(root)?;
    let (classified, limit_info) =
        crate::analysis::inventory_classified_seams_at_with_config(root, &config)?;
    let ts_guidance = crate::output::render::detect_ts_full_repo_guidance_pub(root, &classified);
    let context = crate::agent::artifact::RepoExposureArtifactContext::for_repo_exposure(
        root.to_path_buf(),
        "ready".to_string(),
        None,
        &config,
    )?;
    let path = root.join(AFTER_VERIFICATION_SNAPSHOT_REL);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temporary = path.with_extension(format!("json.tmp-{}-{nonce}", std::process::id()));
    let run_status = if limit_info.is_some() {
        "seam_limit_applied".to_string()
    } else {
        "complete".to_string()
    };
    let write_result = (|| -> Result<(), String> {
        let file = std::fs::File::create(&temporary)
            .map_err(|error| format!("create {} failed: {error}", temporary.display()))?;
        let mut writer = std::io::BufWriter::new(file);
        crate::output::repo_exposure::write_repo_exposure_json_with_context(
            &classified,
            limit_info.as_ref(),
            ts_guidance.as_ref(),
            &context,
            &mut writer,
        )
        .map_err(|error| format!("write {} failed: {error}", temporary.display()))?;
        writer
            .flush()
            .map_err(|error| format!("flush {} failed: {error}", temporary.display()))?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    let bytes = std::fs::read(&temporary)
        .map_err(|error| format!("read {} failed: {error}", temporary.display()))?;
    let digest = sha256_hex(&bytes);
    replace_file_atomically(&path, &bytes)?;
    Ok((digest, run_status))
}

/// One seam entry reduced to the fields the native-identity join and the
/// movement comparison read.
struct SeamEntry {
    seam_id: String,
    owner: String,
    expression: String,
    headline_eligible: bool,
    grip_class: String,
    best_oracle_rank: u8,
    best_oracle_strength: String,
}

fn parse_seams(snapshot: &Value, subject: &str) -> Result<Vec<SeamEntry>, String> {
    let seams = snapshot
        .get("seams")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!(
                "python repair verification: subject=`{subject}`: the analysis snapshot carries no seams array"
            )
        })?;
    let mut parsed = Vec::with_capacity(seams.len());
    for seam in seams {
        let object = as_object(seam, "seam entry")?;
        let related = object
            .get("related_tests")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut best_rank = 0u8;
        let mut best_strength = "none".to_string();
        for test in related {
            if let Some(strength) = test.get("oracle_strength").and_then(Value::as_str) {
                let rank = oracle_rank(strength);
                if rank > best_rank {
                    best_rank = rank;
                    best_strength = strength.to_string();
                }
            }
        }
        parsed.push(SeamEntry {
            seam_id: object
                .get("seam_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            owner: object
                .get("owner")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            expression: object
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            headline_eligible: object
                .get("headline_eligible")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            grip_class: object
                .get("grip_class")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            best_oracle_rank: best_rank,
            best_oracle_strength: best_strength,
        });
    }
    Ok(parsed)
}

/// Oracle-strength rank over the analyzer's vocabulary (strong > medium >
/// weak > smoke > unknown > none). An unrecognized spelling ranks lowest so
/// an unrecognized oracle never counts as evidence of strength.
fn oracle_rank(strength: &str) -> u8 {
    match strength {
        "strong" => 5,
        "medium" => 4,
        "weak" => 3,
        "smoke" => 2,
        "unknown" => 1,
        _ => 0,
    }
}

fn join_seams<'a>(seams: &'a [SeamEntry], owner: &str, discriminator: &str) -> Vec<&'a SeamEntry> {
    seams
        .iter()
        .filter(|seam| seam.owner == owner && seam.expression == discriminator)
        .collect()
}

/// The movement comparison between the retained before snapshot and the
/// fresh after snapshot, joined on the exact native identity
/// (owner + discriminator). Zero and multiple joins fail closed to
/// `uncertain`; a before join that no longer names the packet's seam fails
/// closed to `stale`; a partial after analysis discloses `limited`.
/// Unrelated actionable movement stays visible in its own block either way.
struct MovementOutcome {
    state: &'static str,
    reason: Option<String>,
    before_seam_id: Option<String>,
    after_seam_id: Option<String>,
    before_grip_class: Option<String>,
    after_grip_class: Option<String>,
    before_oracle: Option<String>,
    after_oracle: Option<String>,
    before_headline: Option<bool>,
    after_headline: Option<bool>,
    unrelated_actionable_before: usize,
    unrelated_actionable_after: usize,
    unrelated_regressed: bool,
    unrelated_improved: bool,
}

fn compare_movement(
    before: &Value,
    after: &Value,
    after_run_status: &str,
    owner: &str,
    discriminator: &str,
    expected_seam_id: &str,
) -> Result<MovementOutcome, String> {
    let before_seams = parse_seams(before, "before snapshot")?;
    let after_seams = parse_seams(after, "after snapshot")?;

    let matches_identity =
        |seam: &SeamEntry| seam.owner == owner && seam.expression == discriminator;
    let unrelated_before = before_seams
        .iter()
        .filter(|seam| seam.headline_eligible && !matches_identity(seam))
        .count();
    let unrelated_after = after_seams
        .iter()
        .filter(|seam| seam.headline_eligible && !matches_identity(seam))
        .count();
    let mut outcome = MovementOutcome {
        state: "uncertain",
        reason: None,
        before_seam_id: None,
        after_seam_id: None,
        before_grip_class: None,
        after_grip_class: None,
        before_oracle: None,
        after_oracle: None,
        before_headline: None,
        after_headline: None,
        unrelated_actionable_before: unrelated_before,
        unrelated_actionable_after: unrelated_after,
        unrelated_regressed: unrelated_after > unrelated_before,
        unrelated_improved: unrelated_after < unrelated_before,
    };

    if owner.trim().is_empty() || discriminator.trim().is_empty() {
        outcome.reason = Some(
            "the binding's native identity carries an empty owner or discriminator; the before/after join would not be exact"
                .to_string(),
        );
        return Ok(outcome);
    }
    if after_run_status != "complete" {
        outcome.state = "limited";
        outcome.reason = Some(format!(
            "the after analysis was partial (run_status `{after_run_status}`); movement over a partial analysis is typed limited"
        ));
        return Ok(outcome);
    }

    // Before join: exactly one seam must carry the native identity, and it
    // must still be the seam the attempt's packet was generated for.
    let before_matches = join_seams(&before_seams, owner, discriminator);
    let before_seam = match before_matches.len() {
        0 => {
            outcome.reason = Some(
                "the native identity joins zero seams in the before analysis; the intended behavior cannot be located by native identity"
                    .to_string(),
            );
            return Ok(outcome);
        }
        1 => {
            let seam = before_matches[0];
            outcome.before_seam_id = Some(seam.seam_id.clone());
            outcome.before_grip_class = Some(seam.grip_class.clone());
            outcome.before_oracle = Some(seam.best_oracle_strength.clone());
            outcome.before_headline = Some(seam.headline_eligible);
            if seam.seam_id != expected_seam_id {
                outcome.state = "stale";
                outcome.reason = Some(format!(
                    "stale join: the native identity now resolves to seam `{}` but the attempt's packet names seam `{expected_seam_id}`; the identity moved",
                    seam.seam_id
                ));
                return Ok(outcome);
            }
            seam
        }
        _ => {
            outcome.reason = Some(format!(
                "the native identity joins {} seams in the before analysis; a multiple join is ambiguous",
                before_matches.len()
            ));
            return Ok(outcome);
        }
    };

    let after_matches = join_seams(&after_seams, owner, discriminator);
    let after_seam = match after_matches.len() {
        0 => {
            outcome.reason = Some(
                "the native identity joins zero seams in the after analysis; the post-edit evidence cannot be located by native identity"
                    .to_string(),
            );
            return Ok(outcome);
        }
        1 => after_matches[0],
        _ => {
            outcome.reason = Some(format!(
                "the native identity joins {} seams in the after analysis; a multiple join is ambiguous",
                after_matches.len()
            ));
            return Ok(outcome);
        }
    };
    outcome.after_seam_id = Some(after_seam.seam_id.clone());
    outcome.after_grip_class = Some(after_seam.grip_class.clone());
    outcome.after_oracle = Some(after_seam.best_oracle_strength.clone());
    outcome.after_headline = Some(after_seam.headline_eligible);

    use std::cmp::Ordering;
    outcome.state = match (
        before_seam.headline_eligible,
        after_seam.headline_eligible,
        after_seam
            .best_oracle_rank
            .cmp(&before_seam.best_oracle_rank),
    ) {
        (false, false, _) => "unchanged",
        (false, true, _) => "regressed",
        (true, false, _) => "closed",
        (true, true, Ordering::Greater) => "improved",
        (true, true, Ordering::Equal) => "unchanged",
        (true, true, Ordering::Less) => "regressed",
    };
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Rollback
// ---------------------------------------------------------------------------

struct RollbackOutcome {
    state: &'static str,
    reason: Option<String>,
    post_rollback_head: Option<String>,
}

/// Rolls the applied edit back through the bounded git rail and demonstrates
/// the result: after restoring the changed paths, the re-evaluated edit cage
/// must be compliant with NO changed paths left on the declared edit surface,
/// at an unchanged HEAD. Paths outside the cage (the driver's own workflow
/// artifacts) are not edit residue. Any failure is a typed `blocked`
/// disposition — never a silent skip.
fn run_rollback(
    root: &Path,
    changed_paths: &[String],
    allowed_paths: &std::collections::BTreeSet<String>,
    baseline: &crate::edit_cage::AttemptBaseline,
) -> RollbackOutcome {
    // Only the edit surface is restored: the applied edit is the surface
    // change set, while paths outside the cage are the driver's own workflow
    // artifacts and are never touched.
    let surface_paths: Vec<String> = changed_paths
        .iter()
        .filter(|path| allowed_paths.contains(*path))
        .cloned()
        .collect();
    let changed_paths: &[String] = &surface_paths;
    if changed_paths.is_empty() {
        return RollbackOutcome {
            state: "blocked",
            reason: Some(
                "the applied edit names no changed paths; there is nothing to restore and nothing to demonstrate"
                    .to_string(),
            ),
            post_rollback_head: None,
        };
    }
    let mut args: Vec<&str> = Vec::with_capacity(changed_paths.len() + 2);
    args.push("checkout");
    args.push("--");
    for path in changed_paths {
        args.push(path);
    }
    let restored = match crate::git::run_git_output_with_deadline_and_limit(
        root,
        &args,
        Duration::from_mins(1),
        4 * 1024 * 1024,
    ) {
        Ok(output) => output,
        Err(error) => {
            return RollbackOutcome {
                state: "blocked",
                reason: Some(format!("git restore of the changed paths failed: {error}")),
                post_rollback_head: None,
            };
        }
    };
    if !restored.status.success() {
        return RollbackOutcome {
            state: "blocked",
            reason: Some(format!(
                "git restore of the changed paths failed: {}",
                String::from_utf8_lossy(&restored.stderr).trim()
            )),
            post_rollback_head: None,
        };
    }
    let proof = crate::edit_cage::evaluate_repository_edit_cage_with_delta(baseline);
    let head = crate::agent::artifact::current_git_head(root);
    // The proof reads the raw delta, not the cage verdict: after a successful
    // restore the selected target legitimately no longer differs from the
    // baseline, which the cage verdict alone would report as a violation.
    let surface_residue = match &proof {
        Ok((delta, _)) => {
            let changes = serde_json::to_value(&delta.changes).unwrap_or_default();
            changes
                .as_array()
                .map(|changes| {
                    changes
                        .iter()
                        .filter_map(|change| {
                            let raw = change.get("path")?.as_str()?;
                            let normalized = raw.replace(std::path::MAIN_SEPARATOR, "/");
                            allowed_paths.contains(&normalized).then_some(normalized)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        Err(_) => Vec::new(),
    };
    match (proof, head) {
        (Ok((delta, _)), Ok(head)) if delta.comparable && surface_residue.is_empty() => {
            RollbackOutcome {
                state: "proved",
                reason: None,
                post_rollback_head: Some(head),
            }
        }
        (Ok((delta, _)), head_outcome) => RollbackOutcome {
            state: "blocked",
            reason: Some(format!(
                "the worktree still carries edit residue after the restore: [{residue}] (comparable {comparable}, head {post_rollback_head:?})",
                residue = surface_residue.join(", "),
                comparable = delta.comparable,
                post_rollback_head = head_outcome
            )),
            post_rollback_head: head_outcome.ok(),
        },
        (Err(error), _) => RollbackOutcome {
            state: "blocked",
            reason: Some(format!(
                "the rollback proof could not re-evaluate the edit cage: {error}"
            )),
            post_rollback_head: None,
        },
    }
}

// ---------------------------------------------------------------------------
// Receipt
// ---------------------------------------------------------------------------

/// Renders the immutable candidate receipt. Execution and movement are
/// separate blocks end to end; the schema has no lifecycle field at all.
#[allow(
    clippy::too_many_arguments,
    reason = "the receipt restates every revalidated identity explicitly; grouping them would hide which field comes from which authority"
)]
fn render_receipt(
    attempt_id: &RepairAttemptId,
    revalidated: &RevalidatedAttempt,
    verified: &VerifiedSelection,
    authority: &str,
    execution: &ObservedExecution,
    movement: &MovementOutcome,
    after_snapshot_sha256: &str,
    after_run_status: &str,
    analyzer_binary_sha256: &str,
    rollback: &RollbackOutcome,
) -> Result<Value, String> {
    if !EXECUTION_STATES.contains(&execution.state) {
        return Err(format!(
            "python repair verification: execution state `{}` is outside the closed vocabulary",
            execution.state
        ));
    }
    if !MOVEMENT_STATES.contains(&movement.state) {
        return Err(format!(
            "python repair verification: movement state `{}` is outside the closed vocabulary",
            movement.state
        ));
    }
    if !ROLLBACK_STATES.contains(&rollback.state) {
        return Err(format!(
            "python repair verification: rollback state `{}` is outside the closed vocabulary",
            rollback.state
        ));
    }
    let execution_value = json!({
        "state": execution.state,
        "process_disposition": execution.process_disposition,
        "exit_status": execution.exit_status,
        "exit_signal": execution.exit_signal,
        "stdout_sha256": execution.stdout_sha256.clone(),
        "stderr_sha256": execution.stderr_sha256.clone(),
        "stdout_bytes": execution.stdout_bytes,
        "stderr_bytes": execution.stderr_bytes,
        "stdout_truncated": execution.stdout_truncated,
        "stderr_truncated": execution.stderr_truncated,
        "currentness": execution.currentness.clone(),
        "duration_ms": execution.duration_ms,
        "cancellation_requested": execution.cancellation_requested,
        "reason": execution.reason.clone(),
    });
    let movement_value = json!({
        "state": movement.state,
        "reason": movement.reason.clone(),
        "identity": {
            "family": verified.family.clone(),
            "owner": verified.owner.clone(),
            "discriminator": verified.discriminator.clone(),
            "relation": verified.relation.clone(),
            "oracle": verified.oracle.clone(),
        },
        "join": {
            "before_seam_id": movement.before_seam_id.clone(),
            "after_seam_id": movement.after_seam_id.clone(),
            "before_grip_class": movement.before_grip_class.clone(),
            "after_grip_class": movement.after_grip_class.clone(),
            "before_oracle_strength": movement.before_oracle.clone(),
            "after_oracle_strength": movement.after_oracle.clone(),
            "before_headline_eligible": movement.before_headline,
            "after_headline_eligible": movement.after_headline,
        },
        "unrelated": {
            "actionable_before": movement.unrelated_actionable_before,
            "actionable_after": movement.unrelated_actionable_after,
            "regressed": movement.unrelated_regressed,
            "improved": movement.unrelated_improved,
        },
        "after_snapshot_sha256": after_snapshot_sha256,
        "after_run_status": after_run_status,
    });
    Ok(json!({
        "schema_version": RECEIPT_SCHEMA_VERSION,
        "kind": RECEIPT_KIND,
        "spec": RECEIPT_SPEC,
        "phase": "verify",
        "durable_attempt_id": attempt_id.as_str(),
        "trust_attempt_id": verified.attempt_id.clone(),
        "seam_id": revalidated.manifest_seam_id.clone(),
        "repository_head": revalidated.repository_head.clone(),
        "target_path": verified.target_path.clone(),
        "identities": {
            "packet_sha256": revalidated.packet_sha256.clone(),
            "before_snapshot_sha256": revalidated.before_snapshot_sha256.clone(),
            "patch_sha256": revalidated.patch_sha256.clone(),
            "selection_manifest_sha256": verified.selection_manifest_sha256.clone(),
            "selection_digest": verified.selection_digest.clone(),
            "binding_artifact_sha256": revalidated.binding_artifact_sha256.clone(),
            "config_profile": revalidated.config_profile.clone(),
            "analyzer_binary_sha256": analyzer_binary_sha256,
        },
        "command": {
            "command_spec_sha256": revalidated.command_route.as_ref().map(|(digest, _)| digest.clone()),
            "display": revalidated.command_route.as_ref().map(|(_, display)| display.clone()),
            "authorization": {
                "status": VERIFY_AUTHORIZATION_STATUS,
                "authority": authority,
                "method": VERIFY_AUTHORIZATION_METHOD,
            },
        },
        "execution": execution_value,
        "movement": movement_value,
        "rollback": {
            "state": rollback.state,
            "reason": rollback.reason.clone(),
            "post_rollback_head": rollback.post_rollback_head.clone(),
        },
        "non_claims": VERIFICATION_NON_CLAIMS,
        "claim_boundary": CLAIM_BOUNDARY,
    }))
}

/// Runs the full verification phase and publishes the immutable candidate
/// receipt. Every identity drift refuses before execution; the receipt is
/// written last and refuses to overwrite an existing one. Returns the
/// receipt path.
pub(crate) fn run_verification_phase(options: VerificationOptions<'_>) -> Result<PathBuf, String> {
    let authority = options.authorization.verify()?;
    let root = options
        .root
        .canonicalize()
        .map_err(|error| format!("canonicalize verification root failed: {error}"))?;
    let attempt_id = RepairAttemptId::parse(options.attempt_id.to_string())?;

    // One immutable candidate receipt: an existing receipt is never
    // overwritten or rewritten.
    let receipt_path = root.join(RECEIPT_COMPAT_PATH);
    if receipt_path.exists() {
        return Err(format!(
            "python repair verification refuses to run: the candidate receipt {} already exists; the receipt is immutable and a new verification requires removing it explicitly",
            receipt_path.display()
        ));
    }

    // 1. Revalidate every identity BEFORE anything runs.
    let revalidated = revalidate_for_verification(&root, &attempt_id, &authority)?;

    // 2-4. Execute the producer-owned typed route through the bounded rails
    // (cwd/root confinement, environment floor, timeout, bounded output,
    // owned-child termination, no-secret handling) and retain the typed
    // result plus the stdout/stderr commitments. A packet with no canonical
    // typed route runs nothing: the observation is typed unavailable.
    let execution = match &revalidated.command_route {
        Some(_) => execute_packet_route(&root, &revalidated.packet_path)?,
        None => unavailable_route_observation(),
    };

    // 5. Rerun the current analysis against the exact post-edit state,
    // binding the current binary/config/input identities.
    let (after_snapshot_sha256, after_run_status) = write_after_verification_snapshot(&root)?;
    let after_value: Value = {
        let path = root.join(AFTER_VERIFICATION_SNAPSHOT_REL);
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("read {} failed: {error}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "python repair verification: the after snapshot is not well-formed JSON: {error}"
            )
        })?
    };
    let before_value: Value = {
        let bytes = std::fs::read(&revalidated.before_snapshot_path).map_err(|error| {
            format!(
                "read retained before snapshot {} failed: {error}",
                revalidated.before_snapshot_path.display()
            )
        })?;
        serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "python repair verification: the before snapshot is not well-formed JSON: {error}"
            )
        })?
    };

    // 6. Compare the intended native behavior evidence before/after by exact
    // native identity, with zero/multiple/stale joins failing closed.
    let movement = compare_movement(
        &before_value,
        &after_value,
        &after_run_status,
        &revalidated.verified.owner,
        &revalidated.verified.discriminator,
        &revalidated.manifest_seam_id,
    )?;

    // 8. Rollback proof or an explicit blocked/not_run disposition.
    let rollback = if options.rollback {
        let baseline = load_baseline(
            &root,
            &load_repair_attempt_manifest(&root, &attempt_id)?,
            &attempt_id,
        )?;
        run_rollback(
            &root,
            &revalidated.changed_paths,
            &revalidated.allowed_surface,
            &baseline,
        )
    } else {
        RollbackOutcome {
            state: "not_run",
            reason: Some("rollback was not requested on this verification run".to_string()),
            post_rollback_head: None,
        }
    };

    // Analyzer identity: the running binary's digest — the same real
    // producer the binding records.
    let driver_exe = std::env::current_exe().map_err(|error| {
        format!(
            "python repair verification could not identify the running analyzer binary: {error}"
        )
    })?;
    let driver_bytes = std::fs::read(&driver_exe).map_err(|error| {
        format!(
            "python repair verification could not read the running analyzer binary {}: {error}",
            driver_exe.display()
        )
    })?;
    let analyzer_binary_sha256 = sha256_hex(&driver_bytes);

    // 7. One immutable candidate receipt; execution and movement stay
    // separate fields end to end.
    let receipt = render_receipt(
        &attempt_id,
        &revalidated,
        &revalidated.verified,
        &authority,
        &execution,
        &movement,
        &after_snapshot_sha256,
        &after_run_status,
        &analyzer_binary_sha256,
        &rollback,
    )?;
    let mut rendered = serde_json::to_vec_pretty(&receipt)
        .map_err(|error| format!("serialize verification receipt failed: {error}"))?;
    rendered.push(b'\n');
    replace_file_atomically(&receipt_path, &rendered)?;
    Ok(receipt_path)
}

#[cfg(test)]
mod python_repair_verification_semantics {
    use super::*;

    fn seam(
        seam_id: &str,
        owner: &str,
        expression: &str,
        headline: bool,
        grip_class: &str,
        oracle: &str,
    ) -> Value {
        json!({
            "seam_id": seam_id,
            "owner": owner,
            "expression": expression,
            "headline_eligible": headline,
            "grip_class": grip_class,
            "related_tests": [
                {"oracle_strength": oracle}
            ],
        })
    }

    fn snapshot(seams: Vec<Value>) -> Value {
        json!({ "seams": seams, "run_status": "complete" })
    }

    #[test]
    fn execution_mapping_keeps_every_terminal_state_distinct() -> Result<(), String> {
        let cases = [
            ("completed", Some(0i64), "passed"),
            ("completed", Some(1), "failed"),
            ("completed", Some(-2), "failed"),
            ("timed_out", None, "timed_out"),
            ("cancelled", None, "cancelled"),
            ("failed_to_start", None, "unavailable"),
            ("output_limit_exceeded", None, "invalid"),
        ];
        for (disposition, exit_status, expected) in cases {
            let mapped = map_execution_state(disposition, exit_status)?;
            if mapped != expected {
                return Err(format!(
                    "execution mapping of `{disposition}` with {exit_status:?} produced `{mapped}`, expected `{expected}`"
                ));
            }
        }
        if map_execution_state("completed", None).is_ok() {
            return Err("a completed execution without an exit status must be refused".to_string());
        }
        if map_execution_state("mystery", Some(0)).is_ok() {
            return Err("an unknown disposition must be refused".to_string());
        }
        Ok(())
    }

    #[test]
    fn movement_examples_remain_representable() -> Result<(), String> {
        let owner = "price";
        let discriminator = "amount >= discount_threshold";
        let before_actionable = || {
            snapshot(vec![seam(
                "s1",
                owner,
                discriminator,
                true,
                "weakly_gripped",
                "weak",
            )])
        };
        // Closed: the seam is still located by the exact identity but no
        // longer headline-eligible. A seam that vanished entirely joins zero
        // after seams and fails closed to uncertain instead.
        let closed_after = || {
            snapshot(vec![seam(
                "s1",
                owner,
                discriminator,
                false,
                "strongly_gripped",
                "strong",
            )])
        };
        let unchanged_after = || {
            snapshot(vec![seam(
                "s1",
                owner,
                discriminator,
                true,
                "weakly_gripped",
                "weak",
            )])
        };
        let improved_after = || {
            snapshot(vec![seam(
                "s1",
                owner,
                discriminator,
                true,
                "weakly_gripped",
                "strong",
            )])
        };
        let regressed_after = || {
            snapshot(vec![seam(
                "s1",
                owner,
                discriminator,
                true,
                "ungripped",
                "none",
            )])
        };

        // Command-passed-does-not-imply-movement remains representable:
        // classification never reads an execution field.
        let closed = compare_movement(
            &before_actionable(),
            &closed_after(),
            "complete",
            owner,
            discriminator,
            "s1",
        )?;
        if closed.state != "closed" || closed.reason.is_some() {
            return Err(format!("expected closed, got {:?}", closed.state));
        }
        let unchanged = compare_movement(
            &before_actionable(),
            &unchanged_after(),
            "complete",
            owner,
            discriminator,
            "s1",
        )?;
        if unchanged.state != "unchanged" {
            return Err("expected unchanged".to_string());
        }
        let improved = compare_movement(
            &before_actionable(),
            &improved_after(),
            "complete",
            owner,
            discriminator,
            "s1",
        )?;
        if improved.state != "improved" {
            return Err("expected improved".to_string());
        }
        let regressed = compare_movement(
            &before_actionable(),
            &regressed_after(),
            "complete",
            owner,
            discriminator,
            "s1",
        )?;
        if regressed.state != "regressed" {
            return Err("expected regressed".to_string());
        }

        // Zero/multiple/stale joins fail closed.
        let wrong_identity = compare_movement(
            &before_actionable(),
            &unchanged_after(),
            "complete",
            "other_owner",
            discriminator,
            "s1",
        )?;
        if wrong_identity.state != "uncertain" || wrong_identity.reason.is_none() {
            return Err("a zero join must fail closed to uncertain with a reason".to_string());
        }
        let ambiguous_before = snapshot(vec![
            seam("a", owner, discriminator, true, "weakly_gripped", "weak"),
            seam("b", owner, discriminator, true, "weakly_gripped", "weak"),
        ]);
        let multiple = compare_movement(
            &ambiguous_before,
            &unchanged_after(),
            "complete",
            owner,
            discriminator,
            "a",
        )?;
        if multiple.state != "uncertain" {
            return Err("a multiple join must fail closed to uncertain".to_string());
        }
        let stale = compare_movement(
            &before_actionable(),
            &unchanged_after(),
            "complete",
            owner,
            discriminator,
            "different-seam",
        )?;
        if stale.state != "stale" {
            return Err("a stale join must fail closed to stale".to_string());
        }
        let limited = compare_movement(
            &before_actionable(),
            &unchanged_after(),
            "seam_limit_applied",
            owner,
            discriminator,
            "s1",
        )?;
        if limited.state != "limited" {
            return Err("a partial after analysis must disclose limited".to_string());
        }
        Ok(())
    }

    #[test]
    fn unrelated_actionable_movement_stays_visible() -> Result<(), String> {
        let owner = "price";
        let discriminator = "amount >= threshold";
        let before = snapshot(vec![seam(
            "s1",
            owner,
            discriminator,
            true,
            "weakly_gripped",
            "weak",
        )]);
        // The intended seam closes while an unrelated actionable seam appears.
        let after = snapshot(vec![
            seam(
                "s1",
                owner,
                discriminator,
                false,
                "strongly_gripped",
                "strong",
            ),
            seam("other", "unrelated", "expr", true, "ungripped", "none"),
        ]);
        let outcome = compare_movement(&before, &after, "complete", owner, discriminator, "s1")?;
        if outcome.state != "closed" {
            return Err("the intended seam's movement must remain closed".to_string());
        }
        if outcome.unrelated_actionable_before != 0
            || outcome.unrelated_actionable_after != 1
            || !outcome.unrelated_regressed
        {
            return Err(format!(
                "unrelated regression must stay visible: before {}, after {}, regressed {}",
                outcome.unrelated_actionable_before,
                outcome.unrelated_actionable_after,
                outcome.unrelated_regressed
            ));
        }
        Ok(())
    }

    #[test]
    fn oracle_ranks_stay_ordered_and_unknown_never_counts_strong() -> Result<(), String> {
        if !(oracle_rank("strong") > oracle_rank("medium")
            && oracle_rank("medium") > oracle_rank("weak")
            && oracle_rank("weak") > oracle_rank("smoke")
            && oracle_rank("smoke") > oracle_rank("none")
            && oracle_rank("none") == 0
            && oracle_rank("mystery") == 0)
        {
            return Err("oracle rank order drifted".to_string());
        }
        Ok(())
    }

    #[test]
    fn authorization_refusals_name_the_missing_signal() -> Result<(), String> {
        let missing = VerifyAuthorization {
            authorized: false,
            authority: None,
        };
        let error = match missing.verify() {
            Ok(_) => return Err("an absent authorization must be refused".to_string()),
            Err(error) => error,
        };
        if !error.contains("--verify-authorized") || !error.contains("--verify-authority") {
            return Err(format!("the refusal must name both signals: {error}"));
        }
        let flag_only = VerifyAuthorization {
            authorized: true,
            authority: None,
        };
        let error = match flag_only.verify() {
            Ok(_) => return Err("a flag without an authority must be refused".to_string()),
            Err(error) => error,
        };
        if !error.contains("--verify-authority") {
            return Err(format!(
                "the refusal must name the missing authority: {error}"
            ));
        }
        let granted = VerifyAuthorization {
            authorized: true,
            authority: Some("operator-a".to_string()),
        };
        if granted.verify()? != "operator-a" {
            return Err("a granted authorization must return its authority".to_string());
        }
        Ok(())
    }

    #[test]
    fn receipt_refuses_states_outside_the_closed_vocabularies() -> Result<(), String> {
        // Vocabulary membership is checked before serialization; pin the
        // closed sets themselves so a widened vocabulary is a visible change.
        if EXECUTION_STATES.len() != 7 {
            return Err("the execution vocabulary must stay the issue's 7 states".to_string());
        }
        if MOVEMENT_STATES.len() != 7 {
            return Err("the movement vocabulary must stay the issue's 7 states".to_string());
        }
        if ROLLBACK_STATES != ["proved", "blocked", "not_run"] {
            return Err("the rollback vocabulary drifted".to_string());
        }
        Ok(())
    }

    #[test]
    fn non_claims_never_carry_forbidden_lifecycle_words() {
        for non_claim in VERIFICATION_NON_CLAIMS {
            let lowered = non_claim.to_ascii_lowercase();
            for forbidden in ["accepted", "verified", "reviewed"] {
                assert!(
                    !lowered.contains(forbidden),
                    "non-claim `{non_claim}` must not carry lifecycle word `{forbidden}`"
                );
            }
        }
    }
}
