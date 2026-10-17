//! DTOs, closed key sets, and schema-identity constants for verification
//! receipts. Decoding helpers live here; identity agreement with the accepted
//! selection row is owned by `validate`.
//!
//! Shared types stay with their semantic owner in `python_repair_trust`. This
//! module does not copy driver logic or invent a second parser.

use serde_json::Value;

use super::super::python_repair_trust::reject_unknown_keys;

pub(crate) const RERUN_COMMAND: &str = "cargo xtask python-repair-trust check-verification";

/// The receipt schema mirrors the crate's verification phase output exactly.
/// The crate and xtask share the schema by construction, not by a shared
/// dependency: the digest anchors are the contract.
pub(crate) const RECEIPT_KIND: &str = "python_repair_verification_receipt";
pub(crate) const RECEIPT_SCHEMA_VERSION: &str = "0.1";

pub(crate) const RECEIPT_KEYS: [&str; 16] = [
    "schema_version",
    "kind",
    "spec",
    "phase",
    "durable_attempt_id",
    "trust_attempt_id",
    "seam_id",
    "repository_head",
    "target_path",
    "identities",
    "command",
    "execution",
    "movement",
    "rollback",
    "non_claims",
    "claim_boundary",
];

/// The standing claim boundary every receipt carries verbatim (the same
/// constant the producing phase renders; the schema is shared by
/// construction, and a dropped or edited boundary fails validation).
pub(crate) const RECEIPT_CLAIM_BOUNDARY: &str = "Execution observation and static before/after movement are separate draft evidence axes; real mutation testing confirms behavior-change detection later.";

pub(crate) const IDENTITIES_KEYS: [&str; 8] = [
    "packet_sha256",
    "before_snapshot_sha256",
    "patch_sha256",
    "selection_manifest_sha256",
    "selection_digest",
    "binding_artifact_sha256",
    "config_profile",
    "analyzer_binary_sha256",
];

pub(crate) const COMMAND_KEYS: [&str; 3] = ["command_spec_sha256", "display", "authorization"];
pub(crate) const AUTHORIZATION_KEYS: [&str; 3] = ["status", "authority", "method"];
pub(crate) const AUTHORIZATION_STATUSES: [&str; 1] = ["granted"];
pub(crate) const AUTHORIZATION_METHODS: [&str; 1] = ["explicit-operator-flags"];

pub(crate) const EXECUTION_KEYS: [&str; 14] = [
    "state",
    "process_disposition",
    "exit_status",
    "exit_signal",
    "stdout_sha256",
    "stderr_sha256",
    "stdout_bytes",
    "stderr_bytes",
    "stdout_truncated",
    "stderr_truncated",
    "currentness",
    "duration_ms",
    "cancellation_requested",
    "reason",
];

pub(crate) const MOVEMENT_KEYS: [&str; 7] = [
    "state",
    "reason",
    "identity",
    "join",
    "unrelated",
    "after_snapshot_sha256",
    "after_run_status",
];

pub(crate) const NATIVE_IDENTITY_KEYS: [&str; 5] =
    ["family", "owner", "discriminator", "relation", "oracle"];

pub(crate) const JOIN_KEYS: [&str; 8] = [
    "before_seam_id",
    "after_seam_id",
    "before_grip_class",
    "after_grip_class",
    "before_oracle_strength",
    "after_oracle_strength",
    "before_headline_eligible",
    "after_headline_eligible",
];

pub(crate) const UNRELATED_KEYS: [&str; 4] = [
    "actionable_before",
    "actionable_after",
    "regressed",
    "improved",
];

pub(crate) const ROLLBACK_KEYS: [&str; 3] = ["state", "reason", "post_rollback_head"];

/// The retained process dispositions the bounded rails (and the phase's own
/// pre-execution refusals) can record.
pub(crate) const PROCESS_DISPOSITIONS: [&str; 7] = [
    "completed",
    "failed_to_start",
    "cancelled",
    "timed_out",
    "output_limit_exceeded",
    "rejected_before_execution",
    "route_unavailable",
];

/// The currentness labels the bounded rails retain.
pub(crate) const CURRENTNESS: [&str; 3] = ["current", "dirty_worktree", "historical_noncurrent"];

/// The after-analysis run statuses the producer can retain (the repo-exposure
/// document's own run_status vocabulary, restated).
pub(crate) const AFTER_RUN_STATUSES: [&str; 2] = ["complete", "seam_limit_applied"];

/// The standing non-claims every verification receipt must carry verbatim.
pub(crate) const VERIFICATION_NON_CLAIMS: [&str; 3] = [
    "no lifecycle, acceptance, or closure state is claimed by the verification record",
    "no repair correctness is claimed by the verification record",
    "no support, gate, badge, or promotion claim is made",
];

/// Fail-closed error text: names subject/field/reason plus the rerun command.
pub(crate) fn verification_fail(
    subject: &str,
    field: &str,
    reason: impl std::fmt::Display,
) -> String {
    format!(
        "python-repair-trust check-verification failed: subject=`{subject}` field=`{field}`: {reason}
rerun: {RERUN_COMMAND}"
    )
}

/// One validated receipt, reduced to what the report renders.
#[derive(Debug)]
pub(crate) struct VerificationReceipt {
    pub(crate) display: String,
    pub(crate) trust_attempt_id: String,
    pub(crate) execution_state: String,
    pub(crate) movement_state: String,
    pub(crate) rollback_state: String,
    pub(crate) unrelated_regressed: bool,
    pub(crate) unrelated_improved: bool,
}

/// Reads one nested closed object block, denying unknown keys.
pub(crate) fn block<'a>(
    top: &'a serde_json::Map<String, Value>,
    parent: &str,
    field: &str,
    allowed: &[&str],
    display: &str,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    let subject = format!("{parent}.{field}");
    let value = top
        .get(field)
        .ok_or_else(|| verification_fail(display, &subject, "required block is missing"))?;
    let object = value
        .as_object()
        .ok_or_else(|| verification_fail(display, &subject, "block must be a JSON object"))?;
    reject_unknown_keys(object, allowed, display, &subject)?;
    Ok(object)
}

/// Optional string field: `None` when absent or explicitly null (the same
/// null-as-absent rule the corpus validator applies to optional identities),
/// an error when blank or mistyped. Required fields go through
/// `require_string`, which never accepts a null.
pub(crate) fn opt_receipt_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
    display: &str,
) -> Result<Option<String>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.clone())),
        Some(Value::String(_)) => Err(verification_fail(
            display,
            field,
            "string field must be non-empty when present",
        )),
        Some(_) => Err(verification_fail(
            display,
            field,
            "field must be a string when present",
        )),
    }
}

pub(crate) fn opt_u64_field(
    object: &serde_json::Map<String, Value>,
    field: &str,
    display: &str,
) -> Result<Option<u64>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            verification_fail(
                display,
                field,
                "field must be a non-negative integer when present",
            )
        }),
    }
}

/// Signed integer field: the producer records `exit_status`/`exit_signal` as
/// `Option<i32>`, and a Windows termination code is a negative `i32`, so a
/// receipt carrying one must validate instead of being misread as malformed.
pub(crate) fn opt_i64_field(
    object: &serde_json::Map<String, Value>,
    field: &str,
    display: &str,
) -> Result<Option<i64>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_i64().map(Some).ok_or_else(|| {
            verification_fail(display, field, "field must be an integer when present")
        }),
    }
}

/// Boolean field: present means a JSON boolean; absent or null means `None`.
pub(crate) fn opt_bool_field(
    object: &serde_json::Map<String, Value>,
    field: &str,
    display: &str,
) -> Result<Option<bool>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(flag)) => Ok(Some(*flag)),
        Some(_) => Err(verification_fail(
            display,
            field,
            "field must be a boolean when present",
        )),
    }
}
