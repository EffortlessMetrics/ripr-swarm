use serde::Serialize;
use serde_json::Value;

pub(super) const SCHEMA_VERSION: &str = "0.1";
pub(super) const REPORT_KIND: &str = "first_useful_action";
pub(super) const DEFAULT_GENERATED_AT: &str = "unknown";

pub(crate) const DEFAULT_FIRST_USEFUL_ACTION_OUT: &str =
    "target/ripr/reports/first-useful-action.json";
pub(crate) const DEFAULT_FIRST_USEFUL_ACTION_MD_OUT: &str =
    "target/ripr/reports/first-useful-action.md";
pub(crate) const DEFAULT_TEST_ORACLE_ASSISTANT_PROOF_OUT: &str =
    "target/ripr/reports/test-oracle-assistant-proof.json";
pub(crate) const DEFAULT_TEST_ORACLE_ASSISTANT_PROOF_MD_OUT: &str =
    "target/ripr/reports/test-oracle-assistant-proof.md";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FirstUsefulActionInput {
    pub(crate) root: String,
    pub(crate) generated_at: String,
    pub(crate) pr_guidance_path: Option<String>,
    pub(crate) assistant_proof_path: Option<String>,
    pub(crate) gap_ledger_path: Option<String>,
    pub(crate) ledger_path: Option<String>,
    pub(crate) baseline_delta_path: Option<String>,
    pub(crate) receipt_path: Option<String>,
    pub(crate) gate_decision_path: Option<String>,
    pub(crate) coverage_frontier_path: Option<String>,
    pub(crate) editor_context_path: Option<String>,
    pub(crate) pr_guidance_json: Option<Result<String, String>>,
    pub(crate) assistant_proof_json: Option<Result<String, String>>,
    pub(crate) gap_ledger_json: Option<Result<String, String>>,
    pub(crate) ledger_json: Option<Result<String, String>>,
    pub(crate) baseline_delta_json: Option<Result<String, String>>,
    pub(crate) receipt_json: Option<Result<String, String>>,
    pub(crate) gate_decision_json: Option<Result<String, String>>,
    pub(crate) coverage_frontier_json: Option<Result<String, String>>,
    pub(crate) editor_context_json: Option<Result<String, String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FirstUsefulActionReport {
    pub(super) status: String,
    pub(super) audience: String,
    pub(super) action_kind: String,
    pub(super) root: String,
    pub(super) generated_at: String,
    pub(super) inputs: ActionInputs,
    pub(super) selected: Option<ActionSelected>,
    pub(super) title: String,
    pub(super) why: String,
    pub(super) why_first: Vec<String>,
    pub(super) target: Option<ActionTarget>,
    pub(super) commands: ActionCommands,
    pub(super) evidence: ActionEvidence,
    pub(super) fallback: Option<ActionFallback>,
    pub(super) warnings: Vec<String>,
    pub(super) limits: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ActionInputs {
    pub(super) pr_guidance: Option<String>,
    pub(super) assistant_proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) gap_ledger: Option<String>,
    pub(super) ledger: Option<String>,
    pub(super) baseline_delta: Option<String>,
    pub(super) receipt: Option<String>,
    pub(super) gate_decision: Option<String>,
    pub(super) coverage_frontier: Option<String>,
    pub(super) editor_context: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ActionSelected {
    pub(super) source: String,
    pub(super) source_artifact: String,
    pub(super) seam_id: Option<String>,
    pub(super) seam_kind: Option<String>,
    pub(super) path: Option<String>,
    pub(super) line: Option<u64>,
    pub(super) classification: Option<String>,
    pub(super) missing_discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) gap_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) canonical_gap_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) repair_route: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ActionTarget {
    pub(super) file: Option<String>,
    pub(super) related_test: Option<String>,
    pub(super) suggested_test_name: Option<String>,
    pub(super) suggested_assertion: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub(super) struct ActionCommands {
    pub(super) context_packet: Option<String>,
    pub(super) after_snapshot: Option<String>,
    pub(super) verify: Option<String>,
    pub(super) receipt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) assistant_proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ActionEvidence {
    pub(super) pr_guidance: Option<String>,
    pub(super) assistant_proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) gap_ledger: Option<String>,
    pub(super) receipt: Option<String>,
    pub(super) ledger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) baseline_delta: Option<String>,
    pub(super) static_movement: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct ActionFallback {
    pub(super) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) missing: Option<String>,
}

#[derive(Default)]
pub(super) struct ParsedSources {
    pub(super) pr_guidance: Option<Value>,
    pub(super) assistant_proof: Option<Value>,
    pub(super) gap_ledger: Option<Value>,
    pub(super) ledger: Option<Value>,
    pub(super) baseline_delta: Option<Value>,
    pub(super) receipt: Option<Value>,
    pub(super) gate_decision: Option<Value>,
    pub(super) coverage_frontier: Option<Value>,
    pub(super) editor_context: Option<Value>,
    pub(super) warnings: Vec<String>,
    pub(super) read_errors: Vec<(String, String)>,
}
