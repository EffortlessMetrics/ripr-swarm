//! Deterministic JSON and Markdown projection of a verification-check outcome.
//!
//! Both renderings are derived from the aggregated outcome only. Schema
//! version, accepted vocabulary, ordering, and claim-boundary text stay
//! byte-identical to the pre-split module.

use serde_json::{Value, json};

use super::super::python_repair_trust::KNOWN_SPEC;
use super::aggregate::VerificationCheckOutcome;
use super::schema::RERUN_COMMAND;

pub(crate) fn render_check_verification_json(
    outcome: &VerificationCheckOutcome,
) -> Result<String, String> {
    let receipts: Vec<Value> = outcome
        .receipts
        .iter()
        .map(|receipt| {
            json!({
                "record": receipt.display,
                "trust_attempt_id": receipt.trust_attempt_id,
                "execution": receipt.execution_state,
                "movement": receipt.movement_state,
                "rollback": receipt.rollback_state,
                "unrelated_regressed": receipt.unrelated_regressed,
                "unrelated_improved": receipt.unrelated_improved,
            })
        })
        .collect();
    let report = json!({
        "schema_version": "0.1",
        "kind": "python_repair_verification_check_report",
        "spec": KNOWN_SPEC,
        "manifest": outcome.manifest_path,
        "receipts_input": outcome.receipts_input,
        "receipt_count": outcome.receipts.len(),
        "receipts": receipts,
        "violations": outcome.violations,
        "verdict": outcome.verdict(),
        "claim_boundary": "Execution observation and static movement are separate draft evidence axes; no completed or correct repair is established.",
    });
    serde_json::to_string_pretty(&report).map_err(|error| {
        format!("serialize python-repair-verification check report failed: {error}")
    })
}

pub(crate) fn render_check_verification_markdown(
    outcome: &VerificationCheckOutcome,
    verdict: &str,
) -> String {
    let mut out = String::new();
    out.push_str("# Python Repair Verification Check\n\n");
    out.push_str(&format!("Verdict: **{verdict}**\n\n"));
    out.push_str(
        "Receipt structural validation only — execution and static movement are separate draft evidence axes; no completed or correct repair is established.\n\n",
    );
    out.push_str(&format!(
        "- manifest: {} ({} selections, sha256 `{}`)\n",
        outcome.manifest_path,
        outcome
            .manifest
            .as_ref()
            .map(|manifest| manifest.selections.len())
            .unwrap_or(0),
        outcome
            .manifest
            .as_ref()
            .map(|manifest| manifest.sha256.as_str())
            .unwrap_or("<none>")
    ));
    match &outcome.receipts_input {
        None => out.push_str("- receipts: none supplied (`not_run`; not a pass)\n"),
        Some(input) => {
            out.push_str(&format!(
                "- receipts: {} record(s) from `{input}`\n",
                outcome.receipts.len()
            ));
            for receipt in &outcome.receipts {
                out.push_str(&format!(
                    "  - {} ({}): execution `{}`, movement `{}`, rollback `{}`\n",
                    receipt.trust_attempt_id,
                    receipt.display,
                    receipt.execution_state,
                    receipt.movement_state,
                    receipt.rollback_state
                ));
            }
        }
    }
    if !outcome.violations.is_empty() {
        out.push_str(&format!(
            "\n## Violations ({})\n\n",
            outcome.violations.len()
        ));
        for violation in &outcome.violations {
            out.push_str(&format!("- {violation}\n"));
        }
    }
    out.push_str(&format!("\nrerun: `{RERUN_COMMAND}`\n"));
    out
}
