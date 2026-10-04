use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

pub(crate) const SWARM_INGEST_SCHEMA_VERSION: &str = "0.1";

/// Closed set of machine-readable `reason` strings emitted by the outcome
/// classifier.  Every `unknown` outcome MUST carry one of these values.
/// Do not extend this set without updating `policy/output_contracts.txt` and
/// `docs/OUTPUT_SCHEMA.md`.
pub(crate) mod ingest_reason {
    /// The receipt claims a movement (improved / unchanged / regressed /
    /// resolved) but the before-artifact or after-artifact sha256 is absent
    /// from the provenance block.  Without snapshot provenance the movement
    /// claim cannot be validated; the classifier fails closed.
    pub(crate) const MOVEMENT_WITHOUT_SNAPSHOT_PROVENANCE: &str =
        "movement_without_snapshot_provenance";

    /// Verify evidence is absent or inconclusive.  The classifier cannot
    /// reach an improvement outcome without a passing verify signal.
    pub(crate) const MISSING_VERIFY: &str = "missing_verify";

    /// The input packet is marked stale.  Evidence from a stale packet is
    /// unreliable; the classifier fails closed rather than reporting any
    /// movement outcome.
    pub(crate) const STALE_PACKET: &str = "stale_packet";

    /// The agent edited at least one file on the packet's forbidden list.
    /// All movement claims are discarded regardless of verify or receipt
    /// evidence.
    pub(crate) const FORBIDDEN_EDIT: &str = "forbidden_edit";
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SwarmIngestFacts {
    gap_id: Option<String>,
    canonical_gap_id: Option<String>,
    agent_status: Option<String>,
    stop_reason: Option<String>,
    staleness_status: Option<String>,
    edited_files: Vec<String>,
    allowed_files: Vec<String>,
    forbidden_files: Vec<String>,
    edited_forbidden_files: Vec<String>,
    verify_present: bool,
    verify_status: Option<String>,
    verify_exit_code: Option<i64>,
    verify_passed: bool,
    verify_failed: bool,
    receipt_present: bool,
    receipt_path: Option<String>,
    receipt_movement: Option<String>,
    /// sha256 of the before-artifact recorded in the receipt provenance block.
    /// Required before any movement claim (improved/unchanged/regressed/resolved)
    /// can be accepted.
    receipt_before_sha256: Option<String>,
    /// sha256 of the after-artifact recorded in the receipt provenance block.
    /// Required before any movement claim can be accepted.
    receipt_after_sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SwarmIngestClassification {
    state: &'static str,
    outcome: &'static str,
    reason: &'static str,
    next_action: &'static str,
}

pub(crate) fn render_swarm_ingest_json(
    result_json: &str,
    result_path: &str,
    root: &Path,
) -> Result<String, String> {
    let value: Value = serde_json::from_str(result_json)
        .map_err(|err| format!("failed to parse swarm ingest result JSON: {err}"))?;
    let facts = swarm_ingest_facts(&value, root);
    let classification = classify_swarm_result(&facts);
    let rendered = serde_json::json!({
        "schema_version": SWARM_INGEST_SCHEMA_VERSION,
        "tool": "ripr",
        "report": "swarm-ingest",
        "scope": "agent_result",
        "source": "external_agent_result",
        "status": "advisory",
        "attempt_outcome": classification.outcome,
        "inputs": {
            "result": result_path,
        },
        "classification": {
            "state": classification.state,
            "outcome": classification.outcome,
            "reason": classification.reason,
            "gap_id": facts.gap_id.as_ref(),
            "canonical_gap_id": facts.canonical_gap_id.as_ref(),
        },
        "evidence": {
            "agent_status": facts.agent_status.as_ref(),
            "stop_reason": facts.stop_reason.as_ref(),
            "staleness_status": facts.staleness_status.as_ref(),
            "edited_files": &facts.edited_files,
            "allowed_files": &facts.allowed_files,
            "forbidden_files": &facts.forbidden_files,
            "edited_forbidden_files": &facts.edited_forbidden_files,
            "verify": {
                "present": facts.verify_present,
                "status": facts.verify_status,
                "exit_code": facts.verify_exit_code,
                "passed": facts.verify_passed,
                "failed": facts.verify_failed,
            },
            "receipt": {
                "present": facts.receipt_present,
                "path": facts.receipt_path.as_ref(),
                "movement": facts.receipt_movement.as_ref(),
                "provenance": {
                    "before_sha256": facts.receipt_before_sha256.as_ref(),
                    "after_sha256": facts.receipt_after_sha256.as_ref(),
                    "snapshot_provenance_present": has_snapshot_provenance(&facts),
                },
            },
        },
        "safety": {
            "forbidden_edit_flagged": !facts.edited_forbidden_files.is_empty(),
            "requires_human_review": true,
            "trusted_success": false,
        },
        "next_action": {
            "kind": classification.state,
            "summary": classification.next_action,
        },
        "must_not_infer": [
            "do not trust agent-reported success without verify evidence",
            "do not treat missing verify output as closed",
            "do not ignore forbidden production-code edits",
            "do not run providers, generate tests, run mutation testing, or claim runtime proof from ingest",
        ],
    });
    super::json::render_pretty_with_newline(&rendered, "swarm ingest")
}
