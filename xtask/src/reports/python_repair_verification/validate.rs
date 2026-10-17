//! Identity, currentness, and semantic validation of one verification receipt
//! against the accepted selection manifest.
//!
//! Digest anchors are recomputed, closed field sets are deny-unknown, and
//! execution/movement/rollback stay separate axes. Schema key lists and DTO
//! shape stay in `schema`.

use std::collections::BTreeSet;

use serde_json::Value;

use super::super::python_repair_trust::{
    EXECUTIONS, KNOWN_SPEC, MOVEMENTS, SelectionManifest, canonical_selection_digest,
    check_git_sha, check_portable_path, check_sha256_digest, known_value_or_fail,
    reject_secret_tokens, reject_unknown_keys, require_string,
};
use super::schema::{
    AFTER_RUN_STATUSES, AUTHORIZATION_KEYS, AUTHORIZATION_METHODS, AUTHORIZATION_STATUSES,
    COMMAND_KEYS, CURRENTNESS, EXECUTION_KEYS, IDENTITIES_KEYS, JOIN_KEYS, MOVEMENT_KEYS,
    NATIVE_IDENTITY_KEYS, PROCESS_DISPOSITIONS, RECEIPT_CLAIM_BOUNDARY, RECEIPT_KEYS, RECEIPT_KIND,
    RECEIPT_SCHEMA_VERSION, ROLLBACK_KEYS, UNRELATED_KEYS, VERIFICATION_NON_CLAIMS,
    VerificationReceipt, block, opt_bool_field, opt_i64_field, opt_receipt_string, opt_u64_field,
    verification_fail,
};

/// The durable attempt identity shape (#2927): `repair-attempt-` plus 24
/// lowercase hexadecimal characters.
fn check_durable_attempt_id(subject: &str, field: &str, value: &str) -> Result<(), String> {
    let shaped = match value.strip_prefix("repair-attempt-") {
        Some(suffix) => {
            suffix.len() == 24
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        }
        None => false,
    };
    if shaped {
        Ok(())
    } else {
        Err(verification_fail(
            subject,
            field,
            "durable attempt id must be `repair-attempt-` plus 24 lowercase hexadecimal characters",
        ))
    }
}

/// The oracle-strength rank the producing movement comparison uses
/// (strong > medium > weak > smoke > unknown > everything else). An
/// unrecognized spelling ranks lowest so an unrecognized oracle never counts
/// as evidence of strength — the same rule on both sides of the schema.
fn movement_oracle_rank(strength: &str) -> u8 {
    match strength {
        "strong" => 5,
        "medium" => 4,
        "weak" => 3,
        "smoke" => 2,
        "unknown" => 1,
        _ => 0,
    }
}

/// The headline/oracle transition table the producer applies to the joined
/// before/after seams. The validator re-derives the state from the recorded
/// joins so a receipt whose state contradicts its own evidence fails.
fn movement_state_from_joins(
    before_headline: bool,
    after_headline: bool,
    before_oracle_strength: &str,
    after_oracle_strength: &str,
) -> &'static str {
    use std::cmp::Ordering;
    match (
        before_headline,
        after_headline,
        movement_oracle_rank(after_oracle_strength)
            .cmp(&movement_oracle_rank(before_oracle_strength)),
    ) {
        (false, false, _) => "unchanged",
        (false, true, _) => "regressed",
        (true, false, _) => "closed",
        (true, true, Ordering::Greater) => "improved",
        (true, true, Ordering::Equal) => "unchanged",
        (true, true, Ordering::Less) => "regressed",
    }
}
/// Validates one candidate receipt against the accepted selection manifest.
/// Every digest anchor is recomputed, every closed field set is deny-unknown,
/// the execution and movement vocabularies stay closed and disposition-bound,
/// and the separation laws are pinned by what this function does NOT check:
/// no rule reads the execution state to derive the movement state or the
/// other way around.
pub(crate) fn validate_receipt(
    display: &str,
    record: &Value,
    manifest: &SelectionManifest,
    manifest_sha256: &str,
) -> Result<VerificationReceipt, String> {
    let top = record.as_object().ok_or_else(|| {
        verification_fail(
            display,
            "record",
            "verification receipt must be a JSON object",
        )
    })?;
    reject_unknown_keys(top, &RECEIPT_KEYS, display, "verification receipt")?;
    for (field, expected) in [
        ("schema_version", RECEIPT_SCHEMA_VERSION),
        ("kind", RECEIPT_KIND),
        ("spec", KNOWN_SPEC),
        ("phase", "verify"),
    ] {
        let actual = require_string(display, top, field)?;
        if actual != expected {
            return Err(verification_fail(
                display,
                field,
                format!("expected `{expected}`, got `{actual}`"),
            ));
        }
    }
    // The standing claim boundary rides on every receipt verbatim; a dropped,
    // edited, or weakened boundary is a different record, not this schema.
    let claim_boundary = require_string(display, top, "claim_boundary")?;
    if claim_boundary != RECEIPT_CLAIM_BOUNDARY {
        return Err(verification_fail(
            display,
            "claim_boundary",
            format!("expected the standing claim boundary verbatim, got `{claim_boundary}`"),
        ));
    }
    let durable_attempt_id = require_string(display, top, "durable_attempt_id")?;
    check_durable_attempt_id(display, "durable_attempt_id", &durable_attempt_id)?;
    let seam_id = require_string(display, top, "seam_id")?;
    let repository_head = require_string(display, top, "repository_head")?;
    check_git_sha(display, "repository_head", &repository_head)?;

    // Identities: every digest anchor recomputed or shape-checked.
    let identities = block(top, "receipt", "identities", &IDENTITIES_KEYS, display)?;
    for digest_field in [
        "packet_sha256",
        "before_snapshot_sha256",
        "patch_sha256",
        "selection_manifest_sha256",
        "selection_digest",
        "binding_artifact_sha256",
        "analyzer_binary_sha256",
    ] {
        let digest = require_string(display, identities, digest_field)?;
        check_sha256_digest(display, &format!("identities.{digest_field}"), &digest)?;
    }
    require_string(display, identities, "config_profile")?;
    let recorded_manifest_digest =
        require_string(display, identities, "selection_manifest_sha256")?;
    if recorded_manifest_digest != manifest_sha256 {
        return Err(verification_fail(
            display,
            "identities.selection_manifest_sha256",
            format!(
                "stale digest: the receipt binds to manifest {recorded_manifest_digest} but the accepted selection manifest is {manifest_sha256}"
            ),
        ));
    }
    let trust_attempt_id = require_string(display, top, "trust_attempt_id")?;
    let selection = manifest
        .selections
        .iter()
        .find(|selection| selection.attempt_id == trust_attempt_id)
        .ok_or_else(|| {
            verification_fail(
                &trust_attempt_id,
                "trust_attempt_id",
                "names an attempt identity outside the accepted selection denominator; selected rows cannot be substituted by name",
            )
        })?;
    let row_preimage = manifest
        .row_preimages
        .get(&trust_attempt_id)
        .ok_or_else(|| {
            verification_fail(
                &trust_attempt_id,
                "identities.selection_digest",
                "the accepted selection manifest retained no canonical preimage for this row",
            )
        })?;
    let recorded_selection_digest = require_string(display, identities, "selection_digest")?;
    let recomputed = canonical_selection_digest(row_preimage, &trust_attempt_id)?;
    if recorded_selection_digest != recomputed {
        return Err(verification_fail(
            &trust_attempt_id,
            "identities.selection_digest",
            format!(
                "selection row digest mismatch: the receipt pins `{recorded_selection_digest}` but the accepted row now digests to `{recomputed}`; a replaced or edited selected row makes the receipt stale"
            ),
        ));
    }

    // Command block: the typed route identity (both fields present together
    // or absent together) plus the explicit authorization pair.
    let command = block(top, "receipt", "command", &COMMAND_KEYS, display)?;
    let command_spec_sha256 = opt_receipt_string(command, "command_spec_sha256", display)?;
    let command_display = opt_receipt_string(command, "display", display)?;
    if command_spec_sha256.is_some() != command_display.is_some() {
        return Err(verification_fail(
            display,
            "command.command_spec_sha256",
            "the typed route digest and the display must be recorded together or omitted together",
        ));
    }
    if let Some(digest) = &command_spec_sha256 {
        check_sha256_digest(display, "command.command_spec_sha256", digest)?;
    }
    let authorization = block(
        command,
        "command",
        "authorization",
        &AUTHORIZATION_KEYS,
        display,
    )?;
    let status = require_string(display, authorization, "status")?;
    known_value_or_fail(
        display,
        "command.authorization.status",
        &status,
        &AUTHORIZATION_STATUSES,
        "authorization status",
    )?;
    require_string(display, authorization, "authority")?;
    let method = require_string(display, authorization, "method")?;
    known_value_or_fail(
        display,
        "command.authorization.method",
        &method,
        &AUTHORIZATION_METHODS,
        "authorization method",
    )?;

    // Execution axis: closed vocabulary, disposition agreement, and retained
    // output commitments. No rule below reads the movement block.
    let execution = block(top, "receipt", "execution", &EXECUTION_KEYS, display)?;
    let execution_state = require_string(display, execution, "state")?;
    known_value_or_fail(
        display,
        "execution.state",
        &execution_state,
        &EXECUTIONS,
        "verification execution",
    )?;
    let disposition = require_string(display, execution, "process_disposition")?;
    known_value_or_fail(
        display,
        "execution.process_disposition",
        &disposition,
        &PROCESS_DISPOSITIONS,
        "process disposition",
    )?;
    let exit_status = opt_i64_field(execution, "exit_status", display)?;
    let exit_signal = opt_i64_field(execution, "exit_signal", display)?;
    let stdout_digest = opt_receipt_string(execution, "stdout_sha256", display)?;
    let stderr_digest = opt_receipt_string(execution, "stderr_sha256", display)?;
    if let Some(digest) = &stdout_digest {
        check_sha256_digest(display, "execution.stdout_sha256", digest)?;
    }
    if let Some(digest) = &stderr_digest {
        check_sha256_digest(display, "execution.stderr_sha256", digest)?;
    }
    opt_u64_field(execution, "stdout_bytes", display)?;
    opt_u64_field(execution, "stderr_bytes", display)?;
    opt_u64_field(execution, "duration_ms", display)?;
    for flag_field in [
        "stdout_truncated",
        "stderr_truncated",
        "cancellation_requested",
    ] {
        opt_bool_field(execution, flag_field, display)?;
    }
    let currentness = opt_receipt_string(execution, "currentness", display)?;
    if let Some(currentness) = &currentness {
        known_value_or_fail(
            display,
            "execution.currentness",
            currentness,
            &CURRENTNESS,
            "execution currentness",
        )?;
    }
    let execution_reason = opt_receipt_string(execution, "reason", display)?;
    let route_recorded = command_spec_sha256.is_some();
    // Disposition/state agreement: `passed` is completed with the route's
    // accepted exit code (0) and full commitments; every non-verdict state
    // carries no exit status; each terminal state keeps its own name so
    // timeout, cancellation, spawn failure, non-zero exit, and unobservable
    // runs stay distinct.
    let ran_with_commitments = |display_field: &str| -> Result<(), String> {
        if stdout_digest.is_none() || stderr_digest.is_none() {
            return Err(verification_fail(
                display,
                display_field,
                "a stateless run still retains both output commitments; stdout_sha256 and stderr_sha256 are required once the route ran",
            ));
        }
        if currentness.is_none() {
            return Err(verification_fail(
                display,
                display_field,
                "a run retains its currentness disposition",
            ));
        }
        if !route_recorded {
            return Err(verification_fail(
                display,
                display_field,
                "a run retains the typed command identity it executed",
            ));
        }
        Ok(())
    };
    match execution_state.as_str() {
        "passed" => {
            if disposition != "completed" {
                return Err(verification_fail(
                    display,
                    "execution.state",
                    format!(
                        "`passed` requires process_disposition `completed`, got `{disposition}`"
                    ),
                ));
            }
            if exit_status != Some(0) {
                return Err(verification_fail(
                    display,
                    "execution.exit_status",
                    format!(
                        "`passed` requires exit status 0, got {exit_status:?}; a non-zero exit is `failed`"
                    ),
                ));
            }
            if exit_signal.is_some() {
                return Err(verification_fail(
                    display,
                    "execution.exit_signal",
                    "`passed` is a clean exit and carries no termination signal",
                ));
            }
            ran_with_commitments("execution.state")?;
            if currentness.as_deref() == Some("historical_noncurrent") {
                return Err(verification_fail(
                    display,
                    "execution.currentness",
                    "`passed` cannot ride a historical (non-current) run",
                ));
            }
        }
        "failed" => {
            if disposition != "completed" {
                return Err(verification_fail(
                    display,
                    "execution.state",
                    format!(
                        "`failed` requires process_disposition `completed`, got `{disposition}`"
                    ),
                ));
            }
            // A completed observation is failed on a non-zero exit or on a
            // termination signal (exit status null, signal retained); exactly
            // one of the two is present, so a fabricated pair fails closed.
            match (exit_status, exit_signal) {
                (Some(code), None) if code != 0 => {}
                (None, Some(_)) => {}
                _ => {
                    return Err(verification_fail(
                        display,
                        "execution.exit_status",
                        "`failed` requires a non-zero exit status or a retained termination signal",
                    ));
                }
            }
            ran_with_commitments("execution.state")?;
        }
        "timed_out" | "cancelled" => {
            if disposition != execution_state {
                return Err(verification_fail(
                    display,
                    "execution.state",
                    format!(
                        "`{}` requires process_disposition `{}`, got `{disposition}`",
                        execution_state, execution_state
                    ),
                ));
            }
            if exit_status.is_some() {
                return Err(verification_fail(
                    display,
                    "execution.exit_status",
                    format!("`{}` carries no exit status", execution_state),
                ));
            }
            if exit_signal.is_some() {
                return Err(verification_fail(
                    display,
                    "execution.exit_signal",
                    format!("`{}` carries no termination signal", execution_state),
                ));
            }
            ran_with_commitments("execution.state")?;
        }
        "unavailable" => {
            if disposition != "failed_to_start" && disposition != "route_unavailable" {
                return Err(verification_fail(
                    display,
                    "execution.state",
                    format!(
                        "`unavailable` requires process_disposition `failed_to_start` or `route_unavailable`, got `{disposition}`"
                    ),
                ));
            }
            if exit_status.is_some() {
                return Err(verification_fail(
                    display,
                    "execution.exit_status",
                    "`unavailable` carries no exit status",
                ));
            }
            if exit_signal.is_some() {
                return Err(verification_fail(
                    display,
                    "execution.exit_signal",
                    "`unavailable` carries no termination signal",
                ));
            }
            if execution_reason.is_none() {
                return Err(verification_fail(
                    display,
                    "execution.reason",
                    "`unavailable` retains the typed reason",
                ));
            }
        }
        "not_run" => {
            if disposition != "rejected_before_execution" {
                return Err(verification_fail(
                    display,
                    "execution.state",
                    format!(
                        "`not_run` requires process_disposition `rejected_before_execution`, got `{disposition}`"
                    ),
                ));
            }
            if exit_status.is_some()
                || stdout_digest.is_some()
                || stderr_digest.is_some()
                || currentness.is_some()
            {
                return Err(verification_fail(
                    display,
                    "execution.state",
                    "`not_run` retains no exit status, no output commitments, and no currentness",
                ));
            }
            if exit_signal.is_some() {
                return Err(verification_fail(
                    display,
                    "execution.exit_signal",
                    "`not_run` carries no termination signal",
                ));
            }
            if route_recorded {
                return Err(verification_fail(
                    display,
                    "execution.state",
                    "`not_run` retains no typed command identity",
                ));
            }
            if execution_reason.is_none() {
                return Err(verification_fail(
                    display,
                    "execution.reason",
                    "`not_run` retains the typed reason",
                ));
            }
        }
        "invalid" => {
            if disposition != "output_limit_exceeded" && disposition != "rejected_before_execution"
            {
                return Err(verification_fail(
                    display,
                    "execution.state",
                    format!(
                        "`invalid` requires process_disposition `output_limit_exceeded` or `rejected_before_execution`, got `{disposition}`"
                    ),
                ));
            }
            if exit_status.is_some() {
                return Err(verification_fail(
                    display,
                    "execution.exit_status",
                    "`invalid` carries no exit status",
                ));
            }
            if exit_signal.is_some() {
                return Err(verification_fail(
                    display,
                    "execution.exit_signal",
                    "`invalid` carries no termination signal",
                ));
            }
            if execution_reason.is_none() {
                return Err(verification_fail(
                    display,
                    "execution.reason",
                    "`invalid` retains the typed reason",
                ));
            }
        }
        _ => {
            // Unreachable: the vocabulary check above already failed.
        }
    }

    // Movement axis: closed vocabulary, recorded native-identity join, typed
    // reasons for degraded classifications, and the unrelated-finding block.
    // No rule below reads the execution block.
    let movement = block(top, "receipt", "movement", &MOVEMENT_KEYS, display)?;
    let movement_state = require_string(display, movement, "state")?;
    known_value_or_fail(
        display,
        "movement.state",
        &movement_state,
        &MOVEMENTS,
        "static movement",
    )?;
    let movement_reason = opt_receipt_string(movement, "reason", display)?;
    for degraded in ["stale", "uncertain", "limited"] {
        if movement_state == degraded && movement_reason.is_none() {
            return Err(verification_fail(
                display,
                "movement.reason",
                format!(
                    "a `{degraded}` movement retains the typed reason it is not a confident classification"
                ),
            ));
        }
    }
    for confident in ["closed", "improved", "unchanged", "regressed"] {
        if movement_state == confident && movement_reason.is_some() {
            return Err(verification_fail(
                display,
                "movement.reason",
                format!(
                    "a confident `{confident}` classification carries no degradation reason; the reason field is reserved for stale/uncertain/limited"
                ),
            ));
        }
    }
    let after_snapshot = require_string(display, movement, "after_snapshot_sha256")?;
    check_sha256_digest(display, "movement.after_snapshot_sha256", &after_snapshot)?;
    let after_run_status = require_string(display, movement, "after_run_status")?;
    known_value_or_fail(
        display,
        "movement.after_run_status",
        &after_run_status,
        &AFTER_RUN_STATUSES,
        "after-analysis run status",
    )?;
    // Native identity: the accepted row is authoritative, never the receipt.
    let identity = block(
        movement,
        "movement",
        "identity",
        &NATIVE_IDENTITY_KEYS,
        display,
    )?;
    for field in NATIVE_IDENTITY_KEYS {
        let receipt_value = require_string(display, identity, field)?;
        let row_value = row_preimage
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                verification_fail(
                    &trust_attempt_id,
                    &format!("movement.identity.{field}"),
                    format!("the accepted selection row carries no comparable `{field}`"),
                )
            })?;
        if receipt_value != row_value {
            return Err(verification_fail(
                &trust_attempt_id,
                &format!("movement.identity.{field}"),
                format!(
                    "identity disagreement: the receipt names `{receipt_value}` but the accepted selection row names `{row_value}`; a rewritten identity copy is rejected"
                ),
            ));
        }
    }
    let row_target = normalize_repo_relative_path(&selection.target_path);
    let receipt_target_raw = require_string(display, top, "target_path")?;
    check_portable_path(display, "target_path", &receipt_target_raw)?;
    let receipt_target = normalize_repo_relative_path(&receipt_target_raw);
    if receipt_target != row_target {
        return Err(verification_fail(
            &trust_attempt_id,
            "target_path",
            format!(
                "target identity disagreement: the receipt names `{receipt_target}` but the accepted selection row names `{row_target}`"
            ),
        ));
    }
    let join = block(movement, "movement", "join", &JOIN_KEYS, display)?;
    // Typed join fields: seam ids and oracle strengths are non-empty strings,
    // headline eligibility is a boolean, and an absent join is an explicit
    // null — never a mistyped scalar.
    let join_string = |field: &str| -> Result<Option<String>, String> {
        match join.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.clone())),
            Some(Value::String(_)) => Err(verification_fail(
                display,
                &format!("movement.join.{field}"),
                "join field must be a non-empty string when present",
            )),
            Some(_) => Err(verification_fail(
                display,
                &format!("movement.join.{field}"),
                "join field must be a string when present",
            )),
        }
    };
    let join_bool = |field: &str| -> Result<Option<bool>, String> {
        opt_bool_field(join, field, display).map_err(|error| {
            error.replace(
                &format!("field=`{field}`"),
                &format!("field=`movement.join.{field}`"),
            )
        })
    };
    let before_seam_id = join_string("before_seam_id")?;
    let after_seam_id = join_string("after_seam_id")?;
    let before_grip_class = join_string("before_grip_class")?;
    let after_grip_class = join_string("after_grip_class")?;
    let before_oracle_strength = join_string("before_oracle_strength")?;
    let after_oracle_strength = join_string("after_oracle_strength")?;
    let before_headline_eligible = join_bool("before_headline_eligible")?;
    let after_headline_eligible = join_bool("after_headline_eligible")?;
    let before_join = (
        &before_seam_id,
        &before_grip_class,
        &before_oracle_strength,
        &before_headline_eligible,
    );
    let after_join = (
        &after_seam_id,
        &after_grip_class,
        &after_oracle_strength,
        &after_headline_eligible,
    );
    let before_complete = before_join.0.is_some()
        && before_join.1.is_some()
        && before_join.2.is_some()
        && before_join.3.is_some();
    let before_absent = before_join.0.is_none()
        && before_join.1.is_none()
        && before_join.2.is_none()
        && before_join.3.is_none();
    let after_complete = after_join.0.is_some()
        && after_join.1.is_some()
        && after_join.2.is_some()
        && after_join.3.is_some();
    let after_absent = after_join.0.is_none()
        && after_join.1.is_none()
        && after_join.2.is_none()
        && after_join.3.is_none();
    // The join shapes mirror the producer exactly: a confident classification
    // resolves both joins and its recorded state must be the one its own
    // headline/oracle evidence implies (the same transition table the
    // producing comparison applies); the degraded shapes carry only what the
    // producer can record for them.
    match movement_state.as_str() {
        "closed" | "improved" | "unchanged" | "regressed" => {
            if !before_complete || !after_complete {
                return Err(verification_fail(
                    display,
                    "movement.join",
                    format!(
                        "a confident `{movement_state}` movement retains the complete before and after joins; a partial or null join cannot locate a confident classification"
                    ),
                ));
            }
            if before_seam_id.as_deref() != Some(seam_id.as_str()) {
                return Err(verification_fail(
                    display,
                    "movement.join.before_seam_id",
                    format!(
                        "a confident `{movement_state}` movement's before join names `{before_seam_id:?}` but the receipt binds seam `{seam_id}`; a different before seam is `stale`, not confident"
                    ),
                ));
            }
            let derived = movement_state_from_joins(
                before_headline_eligible.unwrap_or(false),
                after_headline_eligible.unwrap_or(false),
                before_oracle_strength.as_deref().unwrap_or_default(),
                after_oracle_strength.as_deref().unwrap_or_default(),
            );
            if derived != movement_state {
                return Err(verification_fail(
                    display,
                    "movement.state",
                    format!(
                        "the recorded `{movement_state}` movement contradicts its own joins (before headline {before_headline_eligible:?} / `{before_oracle_strength:?}`, after headline {after_headline_eligible:?} / `{after_oracle_strength:?}`), which resolve to `{derived}`"
                    ),
                ));
            }
        }
        "stale" => {
            if !before_complete || !after_absent {
                return Err(verification_fail(
                    display,
                    "movement.join",
                    "a `stale` movement retains the complete before join and no after join (the stale identity never resolved after the edit)",
                ));
            }
            if before_seam_id.as_deref() == Some(seam_id.as_str()) {
                return Err(verification_fail(
                    display,
                    "movement.join.before_seam_id",
                    format!(
                        "the `stale` movement's before join names the receipt's own seam `{seam_id}`; a stale join requires the identity to have moved to a different seam"
                    ),
                ));
            }
        }
        "limited" => {
            if after_run_status == "complete" {
                return Err(verification_fail(
                    display,
                    "movement.state",
                    "a `limited` movement requires a partial after analysis (`seam_limit_applied`); a complete analysis resolves the join or fails closed to `uncertain`",
                ));
            }
            if !before_absent || !after_absent {
                return Err(verification_fail(
                    display,
                    "movement.join",
                    "a `limited` movement carries no join (the comparison never resolves joins over a partial analysis)",
                ));
            }
        }
        _ => {
            // `uncertain`: the comparison can stop before the before join
            // (zero/multiple/empty identity — no join fields at all) or after
            // it (zero/multiple after join — the complete before join only).
            if !after_absent {
                return Err(verification_fail(
                    display,
                    "movement.join",
                    "an `uncertain` movement never retains an after join (a resolved after join is a confident classification)",
                ));
            }
            if !before_absent && !before_complete {
                return Err(verification_fail(
                    display,
                    "movement.join",
                    "an `uncertain` movement carries either no before join or the complete before join; a partial before join is not a producer shape",
                ));
            }
            if before_complete && before_seam_id.as_deref() != Some(seam_id.as_str()) {
                return Err(verification_fail(
                    display,
                    "movement.join.before_seam_id",
                    format!(
                        "the `uncertain` movement's before join names `{before_seam_id:?}` but the receipt binds seam `{seam_id}`; a resolved before join must be the receipt's own seam"
                    ),
                ));
            }
        }
    }
    let unrelated = block(movement, "movement", "unrelated", &UNRELATED_KEYS, display)?;
    let actionable_before = require_u64(unrelated, "actionable_before", display)?;
    let actionable_after = require_u64(unrelated, "actionable_after", display)?;
    let unrelated_regressed = require_bool(unrelated, "regressed", display)?;
    let unrelated_improved = require_bool(unrelated, "improved", display)?;
    // Row-derived consistency: the booleans restate the count comparison, so
    // a hand-edited flag fails even while the counts stay present.
    if unrelated_regressed && actionable_after <= actionable_before {
        return Err(verification_fail(
            display,
            "movement.unrelated.regressed",
            format!(
                "the receipt claims an unrelated regression but the counts show {actionable_before} -> {actionable_after}; a regression requires the unrelated actionable count to grow"
            ),
        ));
    }
    if unrelated_improved && actionable_after >= actionable_before {
        return Err(verification_fail(
            display,
            "movement.unrelated.improved",
            format!(
                "the receipt claims unrelated improvement but the counts show {actionable_before} -> {actionable_after}; an improvement requires the unrelated actionable count to shrink"
            ),
        ));
    }
    if !unrelated_regressed && !unrelated_improved && actionable_after != actionable_before {
        return Err(verification_fail(
            display,
            "movement.unrelated",
            format!(
                "the receipt claims no unrelated movement but the counts changed ({actionable_before} -> {actionable_after}); movement must stay visible"
            ),
        ));
    }

    // Rollback: a typed disposition with its evidence.
    let rollback = block(top, "receipt", "rollback", &ROLLBACK_KEYS, display)?;
    let rollback_state = require_string(display, rollback, "state")?;
    known_value_or_fail(
        display,
        "rollback.state",
        &rollback_state,
        &["proved", "blocked", "not_run"],
        "rollback disposition",
    )?;
    let rollback_reason = opt_receipt_string(rollback, "reason", display)?;
    let post_rollback_head = opt_receipt_string(rollback, "post_rollback_head", display)?;
    match rollback_state.as_str() {
        "proved" => {
            let head = post_rollback_head.ok_or_else(|| {
                verification_fail(
                    display,
                    "rollback.post_rollback_head",
                    "a proved rollback retains the restored head",
                )
            })?;
            check_git_sha(display, "rollback.post_rollback_head", &head)?;
        }
        "blocked" | "not_run" => {
            if rollback_reason.is_none() {
                return Err(verification_fail(
                    display,
                    "rollback.reason",
                    format!("a `{rollback_state}` rollback retains the typed reason"),
                ));
            }
            if post_rollback_head.is_some() {
                return Err(verification_fail(
                    display,
                    "rollback.post_rollback_head",
                    format!(
                        "a `{rollback_state}` rollback retains no restored head; only a proved rollback carries one"
                    ),
                ));
            }
        }
        _ => {
            // Unreachable: the vocabulary check above already failed.
        }
    }

    // Non-claims: the standing claim boundary must ride on the receipt
    // verbatim.
    let non_claims = match top.get("non_claims") {
        Some(Value::Array(values)) => values,
        _ => {
            return Err(verification_fail(
                display,
                "non_claims",
                "must be an array of non-claim strings",
            ));
        }
    };
    let mut recorded_non_claims = BTreeSet::new();
    for (index, value) in non_claims.iter().enumerate() {
        let text = value.as_str().ok_or_else(|| {
            verification_fail(
                display,
                &format!("non_claims[{index}]"),
                "non-claim must be a string",
            )
        })?;
        recorded_non_claims.insert(text);
    }
    let expected_non_claims: BTreeSet<&str> = VERIFICATION_NON_CLAIMS.into_iter().collect();
    if recorded_non_claims != expected_non_claims {
        return Err(verification_fail(
            display,
            "non_claims",
            format!(
                "must be exactly the standing non-claims {VERIFICATION_NON_CLAIMS:?}; a dropped or extra entry fails"
            ),
        ));
    }

    let record_value = Value::Object(top.clone());
    reject_secret_tokens(&record_value, display, "verification receipt")?;

    Ok(VerificationReceipt {
        display: display.to_string(),
        trust_attempt_id,
        execution_state,
        movement_state,
        rollback_state,
        unrelated_regressed,
        unrelated_improved,
    })
}
fn require_u64(
    object: &serde_json::Map<String, Value>,
    field: &str,
    display: &str,
) -> Result<u64, String> {
    match object.get(field) {
        Some(value) => value.as_u64().ok_or_else(|| {
            verification_fail(
                display,
                &format!("movement.unrelated.{field}"),
                "must be a non-negative integer",
            )
        }),
        None => Err(verification_fail(
            display,
            &format!("movement.unrelated.{field}"),
            "required count is missing",
        )),
    }
}

fn require_bool(
    object: &serde_json::Map<String, Value>,
    field: &str,
    display: &str,
) -> Result<bool, String> {
    match object.get(field) {
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => Err(verification_fail(
            display,
            &format!("movement.unrelated.{field}"),
            "must be a boolean",
        )),
        None => Err(verification_fail(
            display,
            &format!("movement.unrelated.{field}"),
            "required flag is missing",
        )),
    }
}

/// Normalizes a portable repo-relative path: `./` segments dropped, empty
/// segments collapsed (the same spelling rule the driver validator applies).
fn normalize_repo_relative_path(path: &str) -> String {
    path.split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>()
        .join("/")
}
