//! Shared receipt lifecycle labels for start-here surfaces.
//!
//! These labels describe whether static receipt evidence exists and how it
//! relates to the selected gap. They do not claim runtime adequacy, mutation
//! proof, gate eligibility, or merge readiness.

use serde_json::Value;

pub const RECEIPT_MISSING: &str = "receipt_missing";
pub const RECEIPT_FOUND: &str = "receipt_found";
pub const RECEIPT_STALE: &str = "receipt_stale";
pub const RECEIPT_GAP_MISMATCH: &str = "receipt_gap_mismatch";
pub const RECEIPT_MOVEMENT_IMPROVED: &str = "receipt_movement_improved";
pub const RECEIPT_MOVEMENT_UNCHANGED: &str = "receipt_movement_unchanged";
pub const RECEIPT_NOT_APPLICABLE: &str = "receipt_not_applicable";

pub fn normalize_receipt_lifecycle_state(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "missing" | "missing_receipt" | RECEIPT_MISSING => RECEIPT_MISSING.to_string(),
        "present" | "found" | RECEIPT_FOUND => RECEIPT_FOUND.to_string(),
        "stale" | "stale_receipt" | RECEIPT_STALE => RECEIPT_STALE.to_string(),
        "gap_mismatch" | "mismatch" | RECEIPT_GAP_MISMATCH => RECEIPT_GAP_MISMATCH.to_string(),
        "improved" | "receipt_improved" | "movement_improved" | RECEIPT_MOVEMENT_IMPROVED => {
            RECEIPT_MOVEMENT_IMPROVED.to_string()
        }
        "unchanged" | "receipt_unchanged" | "movement_unchanged" | RECEIPT_MOVEMENT_UNCHANGED => {
            RECEIPT_MOVEMENT_UNCHANGED.to_string()
        }
        "not_attempted"
        | "not_applicable"
        | "not_available"
        | "n/a"
        | "none"
        | RECEIPT_NOT_APPLICABLE => RECEIPT_NOT_APPLICABLE.to_string(),
        other => other.to_string(),
    }
}

pub fn receipt_lifecycle_state(raw: Option<&str>) -> String {
    raw.map(normalize_receipt_lifecycle_state)
        .unwrap_or_else(|| RECEIPT_MISSING.to_string())
}

pub fn receipt_lifecycle_state_from_movement(movement: Option<&str>) -> String {
    let Some(movement) = movement
        .map(str::trim)
        .filter(|movement| !movement.is_empty())
    else {
        // Fail-closed (#2404): an absent movement does not confirm a receipt
        // exists. Default to missing rather than found — the cardinal-sin
        // doctrine applies: under-emit before over-emit on the affirmative
        // receipt-lifecycle state.
        return RECEIPT_MISSING.to_string();
    };
    match movement.to_ascii_lowercase().as_str() {
        "improved" | "receipt_improved" | "movement_improved" | RECEIPT_MOVEMENT_IMPROVED => {
            RECEIPT_MOVEMENT_IMPROVED.to_string()
        }
        "unchanged" | "receipt_unchanged" | "movement_unchanged" | RECEIPT_MOVEMENT_UNCHANGED => {
            RECEIPT_MOVEMENT_UNCHANGED.to_string()
        }
        "missing" | "missing_receipt" | RECEIPT_MISSING => RECEIPT_MISSING.to_string(),
        "stale" | "stale_receipt" | RECEIPT_STALE => RECEIPT_STALE.to_string(),
        "gap_mismatch" | "mismatch" | RECEIPT_GAP_MISMATCH => RECEIPT_GAP_MISMATCH.to_string(),
        // "changed" is a generic movement direction from actionable-gap receipts
        // (summary.next_action.kind). It confirms the receipt exists and the
        // evidence was reviewed — map to RECEIPT_FOUND, not the unrecognized
        // default. Without this, the fail-closed default (#2404) would
        // downgrade a receipt that explicitly reports a movement to MISSING.
        "changed" | "present" | "found" => RECEIPT_FOUND.to_string(),
        "not_attempted"
        | "not_applicable"
        | "not_available"
        | "n/a"
        | "none"
        | RECEIPT_NOT_APPLICABLE => RECEIPT_NOT_APPLICABLE.to_string(),
        // Fail-closed (#2404): an unrecognized movement token (typo, schema
        // drift, future value) does not confirm a receipt exists.
        _ => RECEIPT_MISSING.to_string(),
    }
}

pub fn receipt_lifecycle_state_from_presence(
    receipt_present: bool,
    receipt_input_present: bool,
) -> String {
    if receipt_present {
        RECEIPT_FOUND.to_string()
    } else if receipt_input_present {
        RECEIPT_MISSING.to_string()
    } else {
        RECEIPT_NOT_APPLICABLE.to_string()
    }
}

pub fn receipt_lifecycle_state_from_receipt_value(receipt: &Value) -> String {
    if let Some(state) = string_path(receipt, &["summary", "receipt_state"])
        .or_else(|| string_path(receipt, &["receipt_state"]))
        .or_else(|| string_path(receipt, &["receipt", "state"]))
    {
        return normalize_receipt_lifecycle_state(&state);
    }

    let movement = string_path(receipt, &["provenance", "movement"])
        .or_else(|| string_path(receipt, &["static_movement", "state"]))
        .or_else(|| string_path(receipt, &["seam", "change"]))
        .or_else(|| string_path(receipt, &["summary", "next_action", "kind"]));
    if movement.is_some() {
        return receipt_lifecycle_state_from_movement(movement.as_deref());
    }

    // Canonical `ripr receipt write` artifacts (RIPR-SPEC-0079 schema 0.1)
    // carry `kind`, `schema_version`, and `verify_status` instead of
    // legacy `provenance.movement`. Parsing that shape proves presence;
    // mapping it to RECEIPT_MISSING is a false negative, not #2404
    // fail-closed conservatism. Absent or unrecognized movement still
    // defaults to missing below.
    if canonical_receipt_write_presence(receipt) {
        return RECEIPT_FOUND.to_string();
    }

    receipt_lifecycle_state_from_movement(None)
}

fn canonical_receipt_write_presence(receipt: &Value) -> bool {
    let Some(kind) = receipt.get("kind").and_then(Value::as_str) else {
        return false;
    };
    let Some(schema_version) = receipt.get("schema_version").and_then(Value::as_str) else {
        return false;
    };
    let Some(verify_status) = receipt
        .get("verify_status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|status| !status.is_empty())
    else {
        return false;
    };
    kind == "receipt"
        && schema_version == "0.1"
        && matches!(verify_status, "passed" | "failed" | "not_run" | "unknown")
}

pub fn receipt_lifecycle_state_is_present(state: &str) -> bool {
    matches!(
        normalize_receipt_lifecycle_state(state).as_str(),
        RECEIPT_FOUND | RECEIPT_MOVEMENT_IMPROVED | RECEIPT_MOVEMENT_UNCHANGED
    )
}

fn string_path(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_legacy_presence_labels() {
        assert_eq!(normalize_receipt_lifecycle_state("present"), RECEIPT_FOUND);
        assert_eq!(
            normalize_receipt_lifecycle_state("missing"),
            RECEIPT_MISSING
        );
        assert_eq!(
            normalize_receipt_lifecycle_state("not_attempted"),
            RECEIPT_NOT_APPLICABLE
        );
    }

    #[test]
    fn maps_static_movement_to_lifecycle_state() {
        assert_eq!(
            receipt_lifecycle_state_from_movement(Some("improved")),
            RECEIPT_MOVEMENT_IMPROVED
        );
        assert_eq!(
            receipt_lifecycle_state_from_movement(Some("unchanged")),
            RECEIPT_MOVEMENT_UNCHANGED
        );
        assert_eq!(
            receipt_lifecycle_state_from_movement(Some("regressed")),
            RECEIPT_MISSING,
            "unrecognized movement must default to missing, not found (#2404 fail-closed)"
        );
    }

    #[test]
    fn absent_movement_defaults_to_missing_not_found() {
        // #2404: None and empty movement must not confirm a receipt exists.
        assert_eq!(receipt_lifecycle_state_from_movement(None), RECEIPT_MISSING);
        assert_eq!(
            receipt_lifecycle_state_from_movement(Some("")),
            RECEIPT_MISSING
        );
    }

    #[test]
    fn extracts_lifecycle_state_from_receipt_json() {
        let receipt = json!({
            "provenance": {"movement": "improved"},
            "summary": {"next_action": {"kind": "improved"}}
        });
        assert_eq!(
            receipt_lifecycle_state_from_receipt_value(&receipt),
            RECEIPT_MOVEMENT_IMPROVED
        );
    }

    fn canonical_receipt_write_value(verify_status: &str) -> Value {
        json!({
            "schema_version": "0.1",
            "tool": "ripr",
            "kind": "receipt",
            "canonical_gap_id": "gap:5a536229e5ed368b",
            "packet_id": null,
            "packet_id_available": false,
            "verify_command": "cargo test large_order_gets_discount",
            "verify_status": verify_status,
            "current_head": "f0500079aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "written_at": "2026-10-04T23:00:00Z",
            "limits_note": "Static evidence only. Receipt records what was run; does not certify semantic correctness."
        })
    }

    #[test]
    fn canonical_receipt_write_artifact_is_found_not_missing() {
        let rendered =
            crate::app::receipt::write_receipt(&crate::app::receipt::ReceiptWriteOptions {
                canonical_gap_id: "gap:5a536229e5ed368b".to_string(),
                packet_id: None,
                verify_command: "cargo test large_order_gets_discount".to_string(),
                verify_status: "not_run".to_string(),
                current_head: None,
                out: None,
                json: true,
                root: None,
            })
            .expect("fixture setup: canonical writer must emit JSON");
        let receipt: Value =
            serde_json::from_str(&rendered).expect("fixture setup: writer JSON must parse");

        assert_eq!(receipt["schema_version"], "0.1");
        assert_eq!(receipt["kind"], "receipt");
        assert!(
            receipt["verify_status"]
                .as_str()
                .is_some_and(|status| !status.is_empty()),
            "fixture setup: writer must emit verify_status"
        );
        assert!(
            receipt.pointer("/provenance/movement").is_none(),
            "canonical writer shape must not carry provenance.movement"
        );

        let state = receipt_lifecycle_state_from_receipt_value(&receipt);
        assert_eq!(
            state, RECEIPT_FOUND,
            "a parsed canonical receipt write artifact is present; receipt_missing is a false negative"
        );
        assert!(
            receipt_lifecycle_state_is_present(&state),
            "pr-ledger consumers treat receipt_found as present"
        );

        for status in ["passed", "failed", "not_run", "unknown"] {
            let from_shape =
                receipt_lifecycle_state_from_receipt_value(&canonical_receipt_write_value(status));
            assert_eq!(
                from_shape, RECEIPT_FOUND,
                "canonical writer shape with verify_status {status} must be found, not {from_shape}"
            );
        }
    }

    #[test]
    fn incomplete_or_unrecognized_receipt_json_stays_missing() {
        assert_eq!(
            receipt_lifecycle_state_from_receipt_value(&json!({})),
            RECEIPT_MISSING,
            "empty JSON is absent movement, not a found receipt (#2404)"
        );
        assert_eq!(
            receipt_lifecycle_state_from_receipt_value(&json!({
                "schema_version": "0.1",
                "kind": "receipt"
            })),
            RECEIPT_MISSING,
            "kind/schema without verify_status is not a valid receipt write artifact"
        );
        assert_eq!(
            receipt_lifecycle_state_from_receipt_value(&json!({
                "schema_version": "0.1",
                "kind": "usefulness_feedback_receipt",
                "verify_status": "passed"
            })),
            RECEIPT_MISSING,
            "a different kind with verify_status is unrecognized, not found"
        );
        assert_eq!(
            receipt_lifecycle_state_from_receipt_value(&json!({
                "schema_version": "0.1",
                "kind": "receipt",
                "verify_status": "bogus"
            })),
            RECEIPT_MISSING,
            "unrecognized verify_status must stay fail-closed"
        );
    }
}
