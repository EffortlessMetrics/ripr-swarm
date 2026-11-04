use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const SNAPSHOT_SCHEMA_VERSION: &str = "0.1";
pub(crate) const SNAPSHOT_KIND: &str = "readiness_evidence_snapshot";
pub(crate) const REHEARSAL_RECEIPT_SCHEMA_VERSION: &str = "1.0";
pub(crate) const REHEARSAL_PLAN_SCHEMA_VERSION: &str = "1.1";
pub(crate) const REHEARSAL_RECEIPT_KIND: &str = "readiness_no_publish_rehearsal_receipt";

pub(crate) const NON_CLAIMS: [&str; 4] = [
    "not a ship or release verdict",
    "not a candidate freeze or version bump",
    "not a registry, marketplace, tag, or signing action",
    "not proof that every expensive dogfood or performance lane ran",
];

/// Closed dimension vocabulary from #1705.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadinessDimensionState {
    Pass,
    Limited,
    Regressed,
    NotProven,
    Stale,
    NotApplicable,
}

impl ReadinessDimensionState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Limited => "limited",
            Self::Regressed => "regressed",
            Self::NotProven => "not_proven",
            Self::Stale => "stale",
            Self::NotApplicable => "not_applicable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadinessDimensionId {
    MainHealth,
    CorrectnessAndHonesty,
    EditorStandardBaseline,
    EditorEnhancedVscode,
    HeadlessAgentInterface,
    SavedWorkspaceLifecycle,
    PositionAndPathCorrectness,
    DiagnosticSignalAndPayload,
    LargeWorkspaceOperability,
    RealRepairRouteEvidence,
    ToolchainReproducibility,
    PackageAndInstallability,
    SupplyChainArtifacts,
    DocumentationAndKnownLimits,
}

impl ReadinessDimensionId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MainHealth => "main_health",
            Self::CorrectnessAndHonesty => "correctness_and_honesty",
            Self::EditorStandardBaseline => "editor_standard_baseline",
            Self::EditorEnhancedVscode => "editor_enhanced_vscode",
            Self::HeadlessAgentInterface => "headless_agent_interface",
            Self::SavedWorkspaceLifecycle => "saved_workspace_lifecycle",
            Self::PositionAndPathCorrectness => "position_and_path_correctness",
            Self::DiagnosticSignalAndPayload => "diagnostic_signal_and_payload",
            Self::LargeWorkspaceOperability => "large_workspace_operability",
            Self::RealRepairRouteEvidence => "real_repair_route_evidence",
            Self::ToolchainReproducibility => "toolchain_reproducibility",
            Self::PackageAndInstallability => "package_and_installability",
            Self::SupplyChainArtifacts => "supply_chain_artifacts",
            Self::DocumentationAndKnownLimits => "documentation_and_known_limits",
        }
    }

    pub(crate) fn all() -> &'static [Self] {
        &[
            Self::MainHealth,
            Self::CorrectnessAndHonesty,
            Self::EditorStandardBaseline,
            Self::EditorEnhancedVscode,
            Self::HeadlessAgentInterface,
            Self::SavedWorkspaceLifecycle,
            Self::PositionAndPathCorrectness,
            Self::DiagnosticSignalAndPayload,
            Self::LargeWorkspaceOperability,
            Self::RealRepairRouteEvidence,
            Self::ToolchainReproducibility,
            Self::PackageAndInstallability,
            Self::SupplyChainArtifacts,
            Self::DocumentationAndKnownLimits,
        ]
    }

    pub(crate) fn default_owner(self) -> &'static str {
        match self {
            Self::ToolchainReproducibility
            | Self::PackageAndInstallability
            | Self::SupplyChainArtifacts => "#4640",
            _ => "#4639",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadinessEvidenceRecord {
    pub(crate) producer: String,
    pub(crate) schema_version: String,
    pub(crate) command: String,
    pub(crate) artifact: String,
    pub(crate) artifact_digest: Option<String>,
    pub(crate) freshness_policy: String,
    pub(crate) stale_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadinessDimension {
    pub(crate) state: ReadinessDimensionState,
    pub(crate) source_sha: String,
    pub(crate) tree_clean: bool,
    pub(crate) evidence: Vec<ReadinessEvidenceRecord>,
    pub(crate) limitations: Vec<String>,
    pub(crate) non_claims: Vec<String>,
    pub(crate) owners: Vec<String>,
    pub(crate) next_proof: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadinessSnapshot {
    pub(crate) schema_version: String,
    pub(crate) kind: String,
    pub(crate) source_sha: String,
    pub(crate) tree_clean: bool,
    /// Telemetry only; excluded from semantic identity digest.
    pub(crate) generated_at: String,
    pub(crate) release_authority: bool,
    pub(crate) candidate_frozen: bool,
    pub(crate) ship_verdict: Option<String>,
    pub(crate) identity_inputs: IdentityInputs,
    pub(crate) dimensions: BTreeMap<String, ReadinessDimension>,
    pub(crate) blocking_unknowns: Vec<String>,
    pub(crate) published_channels: Vec<String>,
    pub(crate) non_claims: Vec<String>,
    pub(crate) semantic_identity_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IdentityInputs {
    pub(crate) cargo_lock_digest: Option<String>,
    pub(crate) package_lock_digest: Option<String>,
    pub(crate) rust_toolchain_digest: Option<String>,
    pub(crate) crate_version: Option<String>,
    pub(crate) extension_version: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RehearsalComponent {
    pub(crate) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) limitation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RehearsalReceipt {
    pub(crate) schema_version: String,
    pub(crate) kind: String,
    pub(crate) source_sha: String,
    pub(crate) tree_clean: bool,
    pub(crate) identity: RehearsalIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) plan: Option<RehearsalPlan>,
    #[serde(default)]
    pub(crate) crate_package: Option<RehearsalComponent>,
    #[serde(default)]
    pub(crate) installed_binary: Option<RehearsalComponent>,
    #[serde(default)]
    pub(crate) server_artifacts: Vec<RehearsalComponent>,
    #[serde(default)]
    pub(crate) vsix: Option<RehearsalComponent>,
    #[serde(default)]
    pub(crate) install_smokes: Vec<RehearsalComponent>,
    #[serde(default)]
    pub(crate) dependency_policy: Option<RehearsalComponent>,
    #[serde(default)]
    pub(crate) sbom: Option<RehearsalComponent>,
    #[serde(default)]
    pub(crate) provenance_plan: Option<RehearsalComponent>,
    #[serde(default)]
    pub(crate) limitations: Vec<String>,
    #[serde(default)]
    pub(crate) non_claims: Vec<String>,
    #[serde(default)]
    pub(crate) published_channels: Vec<String>,
}

/// Schema 1.1 is planning evidence only. Execution/fan-in remains owned by #4850.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RehearsalPlan {
    pub(crate) source_tree: String,
    pub(crate) host_os: String,
    pub(crate) host_arch: String,
    pub(crate) profile: String,
    pub(crate) reads: Vec<String>,
    pub(crate) writes: Vec<String>,
    pub(crate) cache_roots: Vec<String>,
    pub(crate) tools: Vec<String>,
    pub(crate) prohibited_operations: Vec<String>,
    pub(crate) tags_created: Vec<String>,
    pub(crate) versions_changed: bool,
    pub(crate) candidate_frozen: bool,
    pub(crate) release_cut: bool,
    pub(crate) release_authority: bool,
    pub(crate) signing_performed: bool,
    pub(crate) credentials_accessed: bool,
    pub(crate) deployments_performed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RehearsalIdentity {
    pub(crate) cargo_lock_digest: String,
    #[serde(default)]
    pub(crate) package_lock_digest: Option<String>,
    pub(crate) rust_toolchain_digest: String,
    pub(crate) crate_version: String,
    #[serde(default)]
    pub(crate) extension_version: Option<String>,
}

pub(crate) fn not_proven_dimension(
    source_sha: &str,
    tree_clean: bool,
    id: ReadinessDimensionId,
    limitation: &str,
    next: &str,
) -> ReadinessDimension {
    ReadinessDimension {
        state: ReadinessDimensionState::NotProven,
        source_sha: source_sha.to_string(),
        tree_clean,
        evidence: Vec::new(),
        limitations: vec![limitation.to_string()],
        non_claims: Vec::new(),
        owners: vec![id.default_owner().to_string()],
        next_proof: vec![next.to_string()],
    }
}
