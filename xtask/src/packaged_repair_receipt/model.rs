use serde::{Deserialize, Serialize};

pub(crate) const RECEIPT_SCHEMA_VERSION: &str = "1.0";
pub(crate) const RECEIPT_KIND: &str = "packaged_repair_parity_receipt";
pub(crate) const GATE_SCHEMA_VERSION: &str = "1.0";
pub(crate) const GATE_KIND: &str = "packaged_repair_parity_gate";

pub(crate) const CLAIM_BOUNDARY: &str = "This receipt records the typed cross-channel comparison contract and closed section states for one external fixture. It does not claim that an installed CLI or packaged VSIX completed the canonical repair journey.";

pub(crate) const STANDING_NON_CLAIMS: [&str; 3] = [
    "not a qualified packaged repair transaction",
    "not a support-tier promotion",
    "not a runtime mutation result",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QualificationSectionState {
    Qualified,
    SemanticFailure,
    MissingEvidence,
    StaleOrWrongHead,
    InfrastructureTempfail,
    Incomparable,
}

impl QualificationSectionState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Qualified => "qualified",
            Self::SemanticFailure => "semantic_failure",
            Self::MissingEvidence => "missing_evidence",
            Self::StaleOrWrongHead => "stale_or_wrong_head",
            Self::InfrastructureTempfail => "infrastructure_tempfail",
            Self::Incomparable => "incomparable",
        }
    }

    pub(crate) fn is_passing(self) -> bool {
        matches!(self, Self::Qualified)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ParityVerdict {
    Qualified,
    SemanticFailure,
    MissingEvidence,
    StaleOrWrongHead,
    InfrastructureTempfail,
    Incomparable,
}

impl ParityVerdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Qualified => "qualified",
            Self::SemanticFailure => "semantic_failure",
            Self::MissingEvidence => "missing_evidence",
            Self::StaleOrWrongHead => "stale_or_wrong_head",
            Self::InfrastructureTempfail => "infrastructure_tempfail",
            Self::Incomparable => "incomparable",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackagedRepairParityReceipt {
    pub(crate) schema_version: String,
    pub(crate) kind: String,
    pub(crate) fixture: FixtureIdentity,
    pub(crate) candidate: CandidateIdentity,
    pub(crate) channels: ChannelResults,
    pub(crate) required_sections: Vec<RequiredSection>,
    pub(crate) negative_controls: Vec<NamedControl>,
    pub(crate) anti_ambient_controls: Vec<NamedControl>,
    pub(crate) parity: ParitySummary,
    pub(crate) limitations: Vec<String>,
    pub(crate) non_claims: Vec<String>,
    pub(crate) claim_boundary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureIdentity {
    pub(crate) manifest_path: String,
    pub(crate) repository_identity: String,
    pub(crate) base_commit: String,
    pub(crate) head_commit: String,
    pub(crate) canonical_gap_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CandidateIdentity {
    pub(crate) commit: String,
    pub(crate) tree: String,
    pub(crate) product_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChannelResults {
    pub(crate) installed_cli: ChannelResult,
    pub(crate) packaged_vsix: ChannelResult,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChannelResult {
    pub(crate) channel: String,
    pub(crate) artifact_identities: Vec<String>,
    pub(crate) attempt_id: Option<String>,
    pub(crate) semantic: Option<RepairAttemptSemanticSnapshot>,
    pub(crate) section_state: QualificationSectionState,
    pub(crate) executed_steps: Vec<String>,
    pub(crate) limitations: Vec<String>,
    pub(crate) non_claims: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepairAttemptSemanticSnapshot {
    pub(crate) canonical_gap_id: String,
    pub(crate) allowed_edit_surface: Vec<String>,
    pub(crate) must_not_change: Vec<String>,
    pub(crate) status_class: String,
    pub(crate) movement_state: String,
    pub(crate) receipt_status: String,
    pub(crate) verify_status: String,
    pub(crate) currentness: String,
    pub(crate) next_action_kind: String,
    pub(crate) limitations: Vec<String>,
    pub(crate) non_claims: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequiredSection {
    pub(crate) section_id: String,
    pub(crate) state: QualificationSectionState,
    pub(crate) channel: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NamedControl {
    pub(crate) control_id: String,
    pub(crate) state: QualificationSectionState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParitySummary {
    pub(crate) semantic_axes_compared: Vec<String>,
    pub(crate) verdict: ParityVerdict,
    pub(crate) mismatches: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GateVerdict {
    Passed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackagedRepairParityGate {
    pub(crate) schema_version: String,
    pub(crate) kind: String,
    pub(crate) verdict: GateVerdict,
    pub(crate) aggregate_section_state: QualificationSectionState,
    pub(crate) parity_verdict: ParityVerdict,
    pub(crate) failures: Vec<String>,
    pub(crate) fixture: FixtureIdentity,
    pub(crate) candidate: CandidateIdentity,
    pub(crate) limitations: Vec<String>,
    pub(crate) non_claims: Vec<String>,
    pub(crate) claim_boundary: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExpectedFixtureIdentity {
    pub(crate) base_commit: Option<String>,
    pub(crate) head_commit: Option<String>,
    pub(crate) canonical_gap_id: Option<String>,
    pub(crate) allowed_edit_surface: Option<Vec<String>>,
    pub(crate) must_not_change: Option<Vec<String>>,
}

pub(crate) struct GateInput<'a> {
    pub(crate) receipt: &'a PackagedRepairParityReceipt,
    pub(crate) expected_fixture: &'a ExpectedFixtureIdentity,
    pub(crate) require_full_qualification: bool,
}
