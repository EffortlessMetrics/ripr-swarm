// ---------------------------------------------------------------------------
// Tests (module named `python_repair_verification_semantics` under the file
// module `python_repair_verification`, so `cargo test -p xtask
// python_repair_verification` selects exactly these tests; no unwrap/expect —
// assert macros and Result returns only)
// ---------------------------------------------------------------------------

use super::*;

const GIT_SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const GIT_SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const DIGEST_ONE: &str = "1010101010101010101010101010101010101010101010101010101010101010";
const DIGEST_TWO: &str = "2020202020202020202020202020202020202020202020202020202020202020";
const DIGEST_THREE: &str = "3030303030303030303030303030303030303030303030303030303030303030";
const DIGEST_FOUR: &str = "4040404040404040404040404040404040404040404040404040404040404040";
const DIGEST_FIVE: &str = "5050505050505050505050505050505050505050505050505050505050505050";
const DIGEST_SIX: &str = "6060606060606060606060606060606060606060606060606060606060606060";
const DIGEST_SEVEN: &str = "7070707070707070707070707070707070707070707070707070707070707070";
const DIGEST_EIGHT: &str = "8080808080808080808080808080808080808080808080808080808080808080";
const DIGEST_NINE: &str = "9090909090909090909090909090909090909090909090909090909090909090";
const ATTEMPT: &str = "repair-attempt-0123456789abcdef01234567";
const TRUST_ATTEMPT: &str = "att-1";

fn object_mut<'a>(
    value: &'a mut Value,
    subject: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, String> {
    value
        .as_object_mut()
        .ok_or_else(|| format!("fixture {subject} is not an object"))
}

/// The fixture's accepted selection manifest and its exact-bytes digest.
fn fixture_manifest() -> Result<(Value, String), String> {
    let mut row = json!({
        "attempt_id": TRUST_ATTEMPT,
        "case_id": format!("case-{TRUST_ATTEMPT}"),
        "subject_id": "subj-1",
        "repository": "https://example.com/subj-1",
        "base": GIT_SHA_A,
        "head": GIT_SHA_B,
        "tree": DIGEST_ONE,
        "source_currentness": "candidate_current",
        "selection_reason": "behavior changed in the diff and the case discriminates it",
        "diversity_stratum": "pytest_library",
        "family": "predicate_boundary",
        "owner": "pricing.calculate_discount",
        "discriminator": "amount >= threshold",
        "relation": "test calls owner directly",
        "oracle": "assert exact boundary value",
        "expected_direction": "should_gap",
        "claim_boundary": "static exposure evidence only",
        "target_path": "tests/test_pricing.py",
        "target_state": "existing",
        "selected_at": "2026-09-10T00:00:00Z",
        "selector": "campaign-selector",
        "authority_snapshot_digest": DIGEST_TWO,
    });
    let row_digest = canonical_selection_digest(object_mut(&mut row, "row")?, TRUST_ATTEMPT)?;
    object_mut(&mut row, "row")?.insert("selection_digest".to_string(), Value::String(row_digest));
    let manifest = json!({
        "schema_version": "0.1",
        "kind": "python_repair_trust_manifest",
        "spec": "RIPR-SPEC-0176",
        "description": "verification receipt fixture",
        "selections": [row],
    });
    let serialized = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("manifest serialization: {error}"))?;
    let sha = crate::python_judged_panel_replay::sha256_hex(serialized.as_bytes());
    Ok((manifest, sha))
}

/// A complete, well-formed receipt bound to the fixture manifest: a
/// passed execution over a typed route next to an unchanged movement.
fn fixture_receipt(manifest_sha: &str) -> Result<Value, String> {
    let (mut manifest, _) = fixture_manifest()?;
    let row = manifest
        .get_mut("selections")
        .and_then(Value::as_array_mut)
        .and_then(|rows| rows.first_mut())
        .ok_or_else(|| "fixture manifest carries no row".to_string())?;
    let row_digest = canonical_selection_digest(object_mut(row, "row")?, TRUST_ATTEMPT)?;
    Ok(json!({
        "schema_version": "0.1",
        "kind": "python_repair_verification_receipt",
        "spec": "RIPR-SPEC-0176",
        "phase": "verify",
        "durable_attempt_id": ATTEMPT,
        "trust_attempt_id": TRUST_ATTEMPT,
        "seam_id": "seam-1",
        "repository_head": GIT_SHA_B,
        "target_path": "tests/test_pricing.py",
        "identities": {
            "packet_sha256": DIGEST_THREE,
            "before_snapshot_sha256": DIGEST_FOUR,
            "patch_sha256": DIGEST_FIVE,
            "selection_manifest_sha256": manifest_sha,
            "selection_digest": row_digest,
            "binding_artifact_sha256": DIGEST_SIX,
            "config_profile": "default",
            "analyzer_binary_sha256": DIGEST_SEVEN,
        },
        "command": {
            "command_spec_sha256": DIGEST_EIGHT,
            "display": "ripr agent verify --root . --before b --after a --json",
            "authorization": {
                "status": "granted",
                "authority": "operator-a",
                "method": "explicit-operator-flags",
            },
        },
        "execution": {
            "state": "passed",
            "process_disposition": "completed",
            "exit_status": 0,
            "exit_signal": null,
            "stdout_sha256": DIGEST_NINE,
            "stderr_sha256": DIGEST_TWO,
            "stdout_bytes": 10,
            "stderr_bytes": 0,
            "stdout_truncated": false,
            "stderr_truncated": false,
            "currentness": "current",
            "duration_ms": 25,
            "cancellation_requested": false,
            "reason": null,
        },
        "movement": {
            "state": "unchanged",
            "reason": null,
            "identity": {
                "family": "predicate_boundary",
                "owner": "pricing.calculate_discount",
                "discriminator": "amount >= threshold",
                "relation": "test calls owner directly",
                "oracle": "assert exact boundary value",
            },
            "join": {
                "before_seam_id": "seam-1",
                "after_seam_id": "seam-1",
                "before_grip_class": "weakly_gripped",
                "after_grip_class": "weakly_gripped",
                "before_oracle_strength": "weak",
                "after_oracle_strength": "weak",
                "before_headline_eligible": true,
                "after_headline_eligible": true,
            },
            "unrelated": {
                "actionable_before": 2,
                "actionable_after": 2,
                "regressed": false,
                "improved": false,
            },
            "after_snapshot_sha256": DIGEST_FIVE,
            "after_run_status": "complete",
        },
        "rollback": {
            "state": "not_run",
            "reason": "rollback was not requested on this verification run",
            "post_rollback_head": null,
        },
        "non_claims": VERIFICATION_NON_CLAIMS,
        "claim_boundary": RECEIPT_CLAIM_BOUNDARY,
    }))
}

/// A per-call scratch directory. The name comes from a process-wide
/// sequence, not the clock: on Windows two parallel tests read the same
/// `SystemTime` often enough to share a directory, overwrite each other's
/// receipt, and fail on a field neither of them changed.
fn check_directory() -> std::path::PathBuf {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ripr-verify-check-{}-{sequence}",
        std::process::id()
    ))
}

#[test]
fn check_directories_are_distinct_within_one_process() {
    assert_ne!(check_directory(), check_directory());
}

/// Writes manifest + receipt to a temp directory, runs the offline check,
/// and returns the outcome (cleaning up regardless of the result).
fn check(mutate: impl FnOnce(&mut Value)) -> Result<VerificationCheckOutcome, String> {
    let (manifest_value, manifest_sha) = fixture_manifest()?;
    let mut record = fixture_receipt(&manifest_sha)?;
    mutate(&mut record);
    let directory = check_directory();
    std::fs::create_dir_all(&directory).map_err(|error| format!("create temp dir: {error}"))?;
    let written = (|| -> Result<(), String> {
        let manifest_text = serde_json::to_string_pretty(&manifest_value)
            .map_err(|error| format!("manifest text: {error}"))?;
        std::fs::write(directory.join("manifest.json"), manifest_text)
            .map_err(|error| format!("manifest write: {error}"))?;
        let receipt_text = serde_json::to_string_pretty(&record)
            .map_err(|error| format!("receipt text: {error}"))?;
        std::fs::write(directory.join("receipt.json"), receipt_text)
            .map_err(|error| format!("receipt write: {error}"))?;
        Ok(())
    })();
    let manifest_display = directory.join("manifest.json").display().to_string();
    let receipt_display = directory.join("receipt.json").display().to_string();
    let outcome =
        written.and_then(|()| check_verification_artifacts(&manifest_display, &receipt_display));
    let _ = std::fs::remove_dir_all(&directory);
    outcome
}

/// Expects a valid outcome and returns the validated receipt.
fn valid(mutate: impl FnOnce(&mut Value)) -> Result<VerificationReceipt, String> {
    let outcome = check(mutate)?;
    if outcome.verdict() != "valid" || !outcome.violations.is_empty() {
        return Err(format!(
            "expected a valid outcome, got {:?}: {:?}",
            outcome.verdict(),
            outcome.violations
        ));
    }
    outcome
        .receipts
        .into_iter()
        .next()
        .ok_or_else(|| "no receipt validated".to_string())
}

/// Expects an inconsistent outcome and returns the first violation.
fn violation(mutate: impl FnOnce(&mut Value), needle: &str) -> Result<String, String> {
    let outcome = check(mutate)?;
    if outcome.verdict() != "inconsistent" {
        return Err(format!(
            "expected an inconsistent outcome, got {:?}: {:?}",
            outcome.verdict(),
            outcome.violations
        ));
    }
    let first = outcome
        .violations
        .first()
        .ok_or_else(|| "no violation recorded".to_string())?;
    if !first.contains(needle) {
        return Err(format!("violation does not name `{needle}`: {first}"));
    }
    Ok(first.clone())
}

/// Replaces one field in the receipt. A missing parent silently leaves
/// the record untouched; the assertions below then fail on the outcome,
/// keeping a broken mutation visible instead of a panic.
fn set(path: &[&str], value: Value) -> impl FnOnce(&mut Value) {
    let path: Vec<String> = path.iter().map(|part| (*part).to_string()).collect();
    move |record: &mut Value| {
        let mut current = record;
        for part in &path[..path.len() - 1] {
            match current.get_mut(part) {
                Some(next) => current = next,
                None => return,
            }
        }
        if let Some(object) = current.as_object_mut() {
            object.insert(path[path.len() - 1].clone(), value);
        }
    }
}

fn execution_body(state: &str, disposition: &str, exit_status: Value) -> Value {
    json!({
        "state": state,
        "process_disposition": disposition,
        "exit_status": exit_status,
        "exit_signal": null,
        "stdout_sha256": DIGEST_NINE,
        "stderr_sha256": DIGEST_TWO,
        "stdout_bytes": 1,
        "stderr_bytes": 0,
        "stdout_truncated": false,
        "stderr_truncated": false,
        "currentness": "current",
        "duration_ms": 1,
        "cancellation_requested": false,
        "reason": null,
    })
}

// ------------------------------------------------------------------
// Acceptance: the clean receipt and the issue's representable pairs.
// ------------------------------------------------------------------

#[test]
fn accepts_the_clean_passed_receipt() -> Result<(), String> {
    let receipt = valid(|_record| {})?;
    if receipt.execution_state != "passed"
        || receipt.movement_state != "unchanged"
        || receipt.rollback_state != "not_run"
    {
        return Err(format!("fixture receipt reduced wrong: {receipt:?}"));
    }
    Ok(())
}

#[test]
fn every_execution_state_stays_representable() -> Result<(), String> {
    // A failed command next to improved movement stays representable.
    valid(|record| {
        set(
            &["execution"],
            execution_body("failed", "completed", json!(1)),
        )(record);
        set(&["movement", "state"], json!("improved"))(record);
        set(
            &["movement", "join", "after_oracle_strength"],
            json!("strong"),
        )(record);
    })?;

    valid(|record| {
        set(
            &["execution"],
            execution_body("timed_out", "timed_out", json!(null)),
        )(record);
    })?;

    valid(|record| {
        set(
            &["execution"],
            execution_body("cancelled", "cancelled", json!(null)),
        )(record);
    })?;

    // An unavailable runner next to an uncertain movement (a zero
    // native-identity join) stays representable.
    valid(|record| {
        let mut body = execution_body("unavailable", "failed_to_start", json!(null));
        let Ok(object) = object_mut(&mut body, "execution body") else {
            return;
        };
        object.insert("stdout_sha256".to_string(), Value::Null);
        object.insert("stderr_sha256".to_string(), Value::Null);
        object.insert("stdout_bytes".to_string(), Value::Null);
        object.insert("stderr_bytes".to_string(), Value::Null);
        object.insert("currentness".to_string(), Value::Null);
        object.insert("duration_ms".to_string(), Value::Null);
        object.insert(
            "reason".to_string(),
            json!("spawn failed: program not found"),
        );
        set(&["execution"], body)(record);
        set(&["movement", "state"], json!("uncertain"))(record);
        set(
            &["movement", "reason"],
            json!("the native identity joins zero seams in the before analysis"),
        )(record);
        set(
            &["movement", "join"],
            json!({
                "before_seam_id": null,
                "after_seam_id": null,
                "before_grip_class": null,
                "after_grip_class": null,
                "before_oracle_strength": null,
                "after_oracle_strength": null,
                "before_headline_eligible": null,
                "after_headline_eligible": null,
            }),
        )(record);
    })?;

    // A rejected pre-execution is not_run with no route identity and no
    // commitments at all.
    valid(|record| {
        let mut body = execution_body("not_run", "rejected_before_execution", json!(null));
        let Ok(object) = object_mut(&mut body, "execution body") else {
            return;
        };
        object.insert("stdout_sha256".to_string(), Value::Null);
        object.insert("stderr_sha256".to_string(), Value::Null);
        object.insert("stdout_bytes".to_string(), Value::Null);
        object.insert("stderr_bytes".to_string(), Value::Null);
        object.insert("currentness".to_string(), Value::Null);
        object.insert("duration_ms".to_string(), Value::Null);
        object.insert(
            "reason".to_string(),
            json!("the retained packet declares no canonical typed verify route"),
        );
        set(&["execution"], body)(record);
        set(&["command", "command_spec_sha256"], json!(null))(record);
        set(&["command", "display"], json!(null))(record);
    })?;

    // An output-limit run is invalid with a reason but still carries the
    // route identity: the run happened, only its verdict is unobservable.
    valid(|record| {
        let mut body = execution_body("invalid", "output_limit_exceeded", json!(null));
        let Ok(object) = object_mut(&mut body, "execution body") else {
            return;
        };
        object.insert("stdout_truncated".to_string(), json!(true));
        object.insert("stdout_bytes".to_string(), json!(1_048_576));
        object.insert(
            "reason".to_string(),
            json!("output limit exceeded before the route could report"),
        );
        set(&["execution"], body)(record);
    })?;

    Ok(())
}

#[test]
fn the_issue_pair_matrix_stays_representable() -> Result<(), String> {
    // command passed + gap unchanged (the fixture default) is pinned by
    // `accepts_the_clean_passed_receipt`.

    // command failed + static evidence improved.
    valid(|record| {
        set(&["execution", "state"], json!("failed"))(record);
        set(&["execution", "exit_status"], json!(3))(record);
        set(&["movement", "state"], json!("improved"))(record);
        set(
            &["movement", "join", "after_oracle_strength"],
            json!("strong"),
        )(record);
    })?;

    // command unavailable + movement uncertain: the comparison resolved
    // the before join and found no after join, so the receipt carries the
    // complete before join and no after join (the producer shape).
    valid(|record| {
        let mut body = execution_body("unavailable", "route_unavailable", json!(null));
        let Ok(object) = object_mut(&mut body, "execution body") else {
            return;
        };
        object.insert("stdout_sha256".to_string(), Value::Null);
        object.insert("stderr_sha256".to_string(), Value::Null);
        object.insert("stdout_bytes".to_string(), Value::Null);
        object.insert("stderr_bytes".to_string(), Value::Null);
        object.insert("currentness".to_string(), Value::Null);
        object.insert("duration_ms".to_string(), Value::Null);
        object.insert("reason".to_string(), json!("no producer route"));
        set(&["execution"], body)(record);
        set(&["command", "command_spec_sha256"], json!(null))(record);
        set(&["command", "display"], json!(null))(record);
        set(&["movement", "state"], json!("uncertain"))(record);
        set(&["movement", "reason"], json!("stale join"))(record);
        set(&["movement", "join", "after_seam_id"], json!(null))(record);
        set(&["movement", "join", "after_grip_class"], json!(null))(record);
        set(&["movement", "join", "after_oracle_strength"], json!(null))(record);
        set(
            &["movement", "join", "after_headline_eligible"],
            json!(null),
        )(record);
    })?;

    // command passed + wrong target discovered in review: the passed run
    // rides a stale movement without the receipt becoming invalid — the
    // review dimension lives in the corpus, not here. The stale shape
    // carries the moved before seam (a different seam than the receipt
    // binds) and no after join.
    valid(|record| {
        set(&["movement", "state"], json!("stale"))(record);
        set(
            &["movement", "reason"],
            json!("stale join: the native identity now resolves to another seam"),
        )(record);
        set(&["movement", "join", "before_seam_id"], json!("seam-moved"))(record);
        set(&["movement", "join", "after_seam_id"], json!(null))(record);
        set(&["movement", "join", "after_grip_class"], json!(null))(record);
        set(&["movement", "join", "after_oracle_strength"], json!(null))(record);
        set(
            &["movement", "join", "after_headline_eligible"],
            json!(null),
        )(record);
    })?;

    // gap closed + unrelated findings regressed: both stay visible.
    valid(|record| {
        set(&["movement", "state"], json!("closed"))(record);
        set(
            &["movement", "join", "after_headline_eligible"],
            json!(false),
        )(record);
        set(
            &["movement", "unrelated"],
            json!({
                "actionable_before": 2,
                "actionable_after": 3,
                "regressed": true,
                "improved": false,
            }),
        )(record);
    })?;

    Ok(())
}

// ------------------------------------------------------------------
// Fail-closed rules.
// ------------------------------------------------------------------

#[test]
fn rejects_a_stale_manifest_digest() -> Result<(), String> {
    let (_, real_sha) = fixture_manifest()?;
    let forged = if real_sha.ends_with('0') {
        format!("{}1", &real_sha[..63])
    } else {
        format!("{}0", &real_sha[..63])
    };
    violation(
        set(&["identities", "selection_manifest_sha256"], json!(forged)),
        "stale digest",
    )?;
    Ok(())
}

#[test]
fn rejects_a_replaced_selection_row_digest() -> Result<(), String> {
    violation(
        set(&["identities", "selection_digest"], json!(DIGEST_ONE)),
        "selection row digest mismatch",
    )?;
    Ok(())
}

#[test]
fn rejects_an_unknown_trust_attempt() -> Result<(), String> {
    violation(
        set(&["trust_attempt_id"], json!("att-outside")),
        "outside the accepted selection denominator",
    )?;
    Ok(())
}

#[test]
fn rejects_rewritten_identity_copies() -> Result<(), String> {
    for field in ["family", "owner", "discriminator", "relation", "oracle"] {
        violation(
            set(&["movement", "identity", field], json!("rewritten")),
            "identity disagreement",
        )?;
    }
    violation(
        set(&["target_path"], json!("tests/other_test.py")),
        "target identity disagreement",
    )?;
    Ok(())
}

#[test]
fn rejects_disagreement_between_execution_state_and_disposition() -> Result<(), String> {
    violation(
        set(&["execution", "exit_status"], json!(1)),
        "`passed` requires exit status 0",
    )?;
    violation(
        |record| {
            set(&["execution", "state"], json!("failed"))(record);
            set(&["execution", "exit_status"], json!(0))(record);
        },
        "`failed` requires a non-zero exit status",
    )?;
    violation(
        |record| {
            set(&["execution", "state"], json!("failed"))(record);
            set(&["execution", "exit_status"], json!(null))(record);
        },
        "`failed` requires a non-zero exit status",
    )?;
    violation(
        set(&["execution", "process_disposition"], json!("cancelled")),
        "`passed` requires process_disposition `completed`",
    )?;
    violation(
        |record| {
            set(&["execution", "state"], json!("cancelled"))(record);
            set(&["execution", "process_disposition"], json!("completed"))(record);
        },
        "`cancelled` requires process_disposition `cancelled`",
    )?;
    violation(
        set(&["execution", "state"], json!("bogus")),
        "unknown verification execution",
    )?;
    Ok(())
}

#[test]
fn rejects_a_passed_run_without_output_commitments() -> Result<(), String> {
    violation(
        set(&["execution", "stdout_sha256"], json!(null)),
        "retains both output commitments",
    )?;
    violation(
        set(&["execution", "currentness"], json!(null)),
        "retains its currentness disposition",
    )?;
    violation(
        |record| {
            set(&["command", "command_spec_sha256"], json!(null))(record);
            set(&["command", "display"], json!(null))(record);
        },
        "retains the typed command identity",
    )?;
    Ok(())
}

#[test]
fn rejects_movement_outside_the_closed_vocabulary_and_unreasoned_degradation() -> Result<(), String>
{
    violation(
        set(&["movement", "state"], json!("unbounded")),
        "unknown static movement",
    )?;
    violation(
        set(&["movement", "state"], json!("uncertain")),
        "retains the typed reason",
    )?;
    violation(
        set(&["movement", "state"], json!("stale")),
        "retains the typed reason",
    )?;
    Ok(())
}

#[test]
fn rejects_a_confident_movement_carrying_a_degradation_reason() -> Result<(), String> {
    violation(
        set(&["movement", "reason"], json!("downgraded")),
        "carries no degradation reason",
    )?;
    Ok(())
}

#[test]
fn rejects_hand_edited_unrelated_flags() -> Result<(), String> {
    violation(
        set(&["movement", "unrelated", "regressed"], json!(true)),
        "claims an unrelated regression",
    )?;
    violation(
        set(&["movement", "unrelated", "improved"], json!(true)),
        "claims unrelated improvement",
    )?;
    violation(
        set(&["movement", "unrelated", "actionable_after"], json!(5)),
        "movement must stay visible",
    )?;
    Ok(())
}

#[test]
fn rejects_incomplete_rollback_dispositions() -> Result<(), String> {
    violation(
        set(
            &["rollback"],
            json!({
                "state": "proved",
                "reason": null,
                "post_rollback_head": null,
            }),
        ),
        "retains the restored head",
    )?;
    violation(
        set(
            &["rollback"],
            json!({
                "state": "blocked",
                "reason": null,
                "post_rollback_head": null,
            }),
        ),
        "retains the typed reason",
    )?;
    violation(
        set(&["rollback", "post_rollback_head"], json!("not-a-sha")),
        "retains no restored head",
    )?;
    violation(
        set(
            &["rollback"],
            json!({
                "state": "proved",
                "reason": null,
                "post_rollback_head": "not-a-sha",
            }),
        ),
        "git identity must be a bare lowercase 40-character commit SHA",
    )?;
    Ok(())
}

#[test]
fn rejects_dropped_extra_or_non_string_non_claims() -> Result<(), String> {
    violation(
        set(&["non_claims"], json!(["only one claim"])),
        "must be exactly the standing non-claims",
    )?;
    let mut with_extra = VERIFICATION_NON_CLAIMS.to_vec();
    with_extra.push("an extra softening claim");
    violation(
        set(&["non_claims"], json!(with_extra)),
        "must be exactly the standing non-claims",
    )?;
    Ok(())
}

#[test]
fn lifecycle_fields_are_denied_so_a_pass_cannot_mark_acceptance() -> Result<(), String> {
    for field in ["states", "lifecycle", "accepted", "reviewed"] {
        violation(set(&[field], json!(["smuggled"])), "unknown field")?;
    }
    Ok(())
}

#[test]
fn malformed_shapes_fail_with_the_field_named() -> Result<(), String> {
    violation(
        set(&["durable_attempt_id"], json!("attempt-1")),
        "durable attempt id must be",
    )?;
    violation(
        set(&["identities", "patch_sha256"], json!("short")),
        "bare lowercase sha256 hex",
    )?;
    violation(
        set(&["schema_version"], json!("9.9")),
        "expected `0.1`, got `9.9`",
    )?;
    violation(
        set(&["phase"], json!("accept")),
        "expected `verify`, got `accept`",
    )?;
    violation(
        set(&["movement", "after_run_status"], json!("partial")),
        "unknown after-analysis run status",
    )?;
    Ok(())
}

// ------------------------------------------------------------------
// Fabricated joins, scalar types, signal termination, claim boundary.
// ------------------------------------------------------------------

#[test]
fn requires_the_standing_claim_boundary_verbatim() -> Result<(), String> {
    violation(
        set(&["claim_boundary"], json!("a softened boundary")),
        "expected the standing claim boundary verbatim",
    )?;
    violation(
        set(&["claim_boundary"], json!(null)),
        "a present null is not a value",
    )?;
    Ok(())
}

#[test]
fn rejects_fabricated_or_contradictory_movement_joins() -> Result<(), String> {
    // The review's exact example: a confident state over null joins.
    violation(
        |record| {
            set(&["movement", "state"], json!("closed"))(record);
            set(
                &["movement", "join"],
                json!({
                    "before_seam_id": null,
                    "after_seam_id": null,
                    "before_grip_class": null,
                    "after_grip_class": null,
                    "before_oracle_strength": null,
                    "after_oracle_strength": null,
                    "before_headline_eligible": null,
                    "after_headline_eligible": null,
                }),
            )(record);
        },
        "retains the complete before and after joins",
    )?;
    // A confident state its own headline/oracle evidence contradicts.
    violation(
        set(&["movement", "state"], json!("closed")),
        "contradicts its own joins",
    )?;
    // A before seam the receipt does not bind is `stale`, never confident.
    violation(
        set(
            &["movement", "join", "before_seam_id"],
            json!("seam-elsewhere"),
        ),
        "a different before seam is `stale`",
    )?;
    // A stale join that still names the receipt's own seam is impossible.
    violation(
        |record| {
            set(&["movement", "state"], json!("stale"))(record);
            set(
                &["movement", "reason"],
                json!("stale join: the identity moved"),
            )(record);
            set(&["movement", "join", "after_seam_id"], json!(null))(record);
            set(&["movement", "join", "after_grip_class"], json!(null))(record);
            set(&["movement", "join", "after_oracle_strength"], json!(null))(record);
            set(
                &["movement", "join", "after_headline_eligible"],
                json!(null),
            )(record);
        },
        "requires the identity to have moved",
    )?;
    // `limited` over a complete analysis is impossible: the producer
    // either resolves the join or degrades to `uncertain`.
    violation(
        |record| {
            set(&["movement", "state"], json!("limited"))(record);
            set(
                &["movement", "reason"],
                json!("the after analysis was partial"),
            )(record);
            set(
                &["movement", "join"],
                json!({
                    "before_seam_id": null,
                    "after_seam_id": null,
                    "before_grip_class": null,
                    "after_grip_class": null,
                    "before_oracle_strength": null,
                    "after_oracle_strength": null,
                    "before_headline_eligible": null,
                    "after_headline_eligible": null,
                }),
            )(record);
        },
        "requires a partial after analysis",
    )?;
    // An uncertain movement never retains an after join (the fixture's
    // after join stays present, which the producer cannot record for a
    // degraded state).
    violation(
        |record| {
            set(&["movement", "state"], json!("uncertain"))(record);
            set(
                &["movement", "reason"],
                json!("the native identity joins zero seams in the after analysis"),
            )(record);
        },
        "never retains an after join",
    )?;
    // A partial before join is not a producer shape.
    violation(
        |record| {
            set(&["movement", "state"], json!("uncertain"))(record);
            set(
                &["movement", "reason"],
                json!("the native identity joins zero seams in the before analysis"),
            )(record);
            set(&["movement", "join", "after_seam_id"], json!(null))(record);
            set(&["movement", "join", "after_grip_class"], json!(null))(record);
            set(&["movement", "join", "after_oracle_strength"], json!(null))(record);
            set(
                &["movement", "join", "after_headline_eligible"],
                json!(null),
            )(record);
            set(&["movement", "join", "before_seam_id"], json!("seam-1"))(record);
            set(&["movement", "join", "before_grip_class"], json!(null))(record);
        },
        "a partial before join is not a producer shape",
    )?;
    // The documented `limited` shape itself stays representable.
    valid(|record| {
        set(&["movement", "state"], json!("limited"))(record);
        set(
            &["movement", "reason"],
            json!("the after analysis was partial (run_status `seam_limit_applied`)"),
        )(record);
        set(
            &["movement", "after_run_status"],
            json!("seam_limit_applied"),
        )(record);
        set(
            &["movement", "join"],
            json!({
                "before_seam_id": null,
                "after_seam_id": null,
                "before_grip_class": null,
                "after_grip_class": null,
                "before_oracle_strength": null,
                "after_oracle_strength": null,
                "before_headline_eligible": null,
                "after_headline_eligible": null,
            }),
        )(record);
    })?;
    Ok(())
}

#[test]
fn rejects_mistyped_producer_execution_scalars() -> Result<(), String> {
    violation(
        set(&["execution", "exit_signal"], json!({})),
        "field must be an integer when present",
    )?;
    violation(
        set(&["execution", "exit_signal"], json!("6")),
        "field must be an integer when present",
    )?;
    violation(
        set(&["execution", "duration_ms"], json!("fast")),
        "field must be a non-negative integer when present",
    )?;
    violation(
        set(&["execution", "stdout_truncated"], json!("yes")),
        "field must be a boolean when present",
    )?;
    violation(
        set(&["execution", "cancellation_requested"], json!(1)),
        "field must be a boolean when present",
    )?;
    Ok(())
}

#[test]
fn keeps_signal_termination_and_negative_exit_codes_representable() -> Result<(), String> {
    // A completed observation terminated by signal: exit status null, the
    // signal retained, execution state `failed`.
    valid(|record| {
        set(&["execution", "state"], json!("failed"))(record);
        set(&["execution", "exit_status"], json!(null))(record);
        set(&["execution", "exit_signal"], json!(6))(record);
    })?;
    // A Windows access violation is a negative exit code, not a malformed
    // receipt: the signed producer type must validate.
    valid(|record| {
        set(&["execution", "state"], json!("failed"))(record);
        set(&["execution", "exit_status"], json!(-1073741819))(record);
    })?;
    // An exit status paired with a signal is not a producer shape.
    violation(
        |record| {
            set(&["execution", "state"], json!("failed"))(record);
            set(&["execution", "exit_status"], json!(1))(record);
            set(&["execution", "exit_signal"], json!(6))(record);
        },
        "`failed` requires a non-zero exit status or a retained termination signal",
    )?;
    // A clean exit paired with a signal is not a producer shape.
    violation(
        set(&["execution", "exit_signal"], json!(6)),
        "`passed` is a clean exit and carries no termination signal",
    )?;
    // A timed-out run carries no signal (the rails drop it off the
    // completed disposition only).
    violation(
        |record| {
            set(
                &["execution"],
                execution_body("timed_out", "timed_out", json!(null)),
            )(record);
            set(&["execution", "exit_signal"], json!(9))(record);
        },
        "`timed_out` carries no termination signal",
    )?;
    Ok(())
}

// ------------------------------------------------------------------
// Verdicts.
// ------------------------------------------------------------------

#[test]
fn not_run_is_never_a_pass() -> Result<(), String> {
    let outcome = VerificationCheckOutcome {
        manifest_path: "manifest.json".to_string(),
        manifest: None,
        receipts_input: None,
        receipts: Vec::new(),
        violations: Vec::new(),
    };
    if outcome.verdict() != "not_run" {
        return Err("an empty input must be not_run".to_string());
    }
    Ok(())
}

#[test]
fn invalid_only_receipts_fail_closed_as_inconsistent() -> Result<(), String> {
    let outcome = check(|record| {
        set(&["execution", "exit_status"], json!(7))(record);
    })?;
    if outcome.verdict() != "inconsistent" || outcome.violations.is_empty() {
        return Err(format!(
            "all-invalid receipts must fail closed, got {:?}",
            outcome.verdict()
        ));
    }
    Ok(())
}
