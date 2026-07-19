use super::{
    COLLECT_CONTEXT_COMMAND, COLLECT_EVIDENCE_CONTEXT_COMMAND, COLLECT_RECEIPT_STATUS_COMMAND,
    COLLECT_REPAIR_PACKET_COMMAND, COLLECT_TOP_LIMITATION_COMMAND,
    COLLECT_WORKSPACE_STATUS_COMMAND, REFRESH_COMMAND,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use tower_lsp_server::ls_types::LSPAny;

pub(super) const RIPR_AGENT_PROTOCOL_VERSION: &str = "0.1";
pub(super) const RIPR_AGENT_SCHEMA_VERSION: &str = "0.1";
const RIPR_AGENT_SUPPORTED_PROTOCOL_MAJOR: u16 = 0;
const RIPR_AGENT_SUPPORTED_SCHEMA_MAJOR: u16 = 0;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct RiprAgentProtocolVersion(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RiprAgentProtocolVersionError {
    InvalidFormat(String),
    UnsupportedMajor { received: u16, supported: u16 },
}

impl fmt::Display for RiprAgentProtocolVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(value) => write!(formatter, "invalid protocol version `{value}`"),
            Self::UnsupportedMajor {
                received,
                supported,
            } => write!(
                formatter,
                "unsupported protocol major `{received}`; supported major is `{supported}`"
            ),
        }
    }
}

impl RiprAgentProtocolVersion {
    fn current() -> Self {
        Self(RIPR_AGENT_PROTOCOL_VERSION.to_string())
    }

    fn parse(value: &str) -> Result<Self, RiprAgentProtocolVersionError> {
        let Some((major, minor)) = value.split_once('.') else {
            return Err(RiprAgentProtocolVersionError::InvalidFormat(
                value.to_string(),
            ));
        };
        if major.is_empty()
            || minor.is_empty()
            || !major.chars().all(|character| character.is_ascii_digit())
            || !minor.chars().all(|character| character.is_ascii_digit())
        {
            return Err(RiprAgentProtocolVersionError::InvalidFormat(
                value.to_string(),
            ));
        }
        let major = major
            .parse::<u16>()
            .map_err(|_error| RiprAgentProtocolVersionError::InvalidFormat(value.to_string()))?;
        if major != RIPR_AGENT_SUPPORTED_PROTOCOL_MAJOR {
            return Err(RiprAgentProtocolVersionError::UnsupportedMajor {
                received: major,
                supported: RIPR_AGENT_SUPPORTED_PROTOCOL_MAJOR,
            });
        }
        Ok(Self(value.to_string()))
    }

    #[expect(
        dead_code,
        reason = "reserved protocol DTO identity is consumed by future handlers"
    )]
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RiprAgentProtocolVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct RiprAgentSchemaVersion(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RiprAgentSchemaVersionError {
    InvalidFormat(String),
    UnsupportedMajor { received: u16, supported: u16 },
}

impl fmt::Display for RiprAgentSchemaVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(value) => write!(formatter, "invalid schema version `{value}`"),
            Self::UnsupportedMajor {
                received,
                supported,
            } => write!(
                formatter,
                "unsupported schema major `{received}`; supported major is `{supported}`"
            ),
        }
    }
}

impl RiprAgentSchemaVersion {
    fn current() -> Self {
        Self(RIPR_AGENT_SCHEMA_VERSION.to_string())
    }

    fn parse(value: &str) -> Result<Self, RiprAgentSchemaVersionError> {
        let Some((major, minor)) = value.split_once('.') else {
            return Err(RiprAgentSchemaVersionError::InvalidFormat(
                value.to_string(),
            ));
        };
        if major.is_empty()
            || minor.is_empty()
            || !major.chars().all(|character| character.is_ascii_digit())
            || !minor.chars().all(|character| character.is_ascii_digit())
        {
            return Err(RiprAgentSchemaVersionError::InvalidFormat(
                value.to_string(),
            ));
        }
        let major = major
            .parse::<u16>()
            .map_err(|_error| RiprAgentSchemaVersionError::InvalidFormat(value.to_string()))?;
        if major != RIPR_AGENT_SUPPORTED_SCHEMA_MAJOR {
            return Err(RiprAgentSchemaVersionError::UnsupportedMajor {
                received: major,
                supported: RIPR_AGENT_SUPPORTED_SCHEMA_MAJOR,
            });
        }
        Ok(Self(value.to_string()))
    }

    #[expect(
        dead_code,
        reason = "reserved schema DTO identity is consumed by future handlers"
    )]
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RiprAgentSchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RiprAgentVersionIdentity {
    pub(super) protocol_version: RiprAgentProtocolVersion,
    pub(super) schema_version: RiprAgentSchemaVersion,
}

impl RiprAgentVersionIdentity {
    fn current() -> Self {
        Self {
            protocol_version: RiprAgentProtocolVersion::current(),
            schema_version: RiprAgentSchemaVersion::current(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub(super) enum RiprAgentRequest {
    #[serde(rename = "ripr/workspaceStatus")]
    WorkspaceStatus,
    #[serde(rename = "ripr/refreshAnalysis")]
    RefreshAnalysis,
    #[serde(rename = "ripr/listActionableItems")]
    ListActionableItems,
    #[serde(rename = "ripr/getRepairPacket")]
    GetRepairPacket,
    #[serde(rename = "ripr/getEvidenceContext")]
    GetEvidenceContext,
    #[serde(rename = "ripr/getTopLimitation")]
    GetTopLimitation,
    #[serde(rename = "ripr/getReceiptStatus")]
    GetReceiptStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiprAgentRequestMode {
    ReadOnly,
    Refresh,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiprAgentProfile {
    Actionable,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiprAgentErrorKind {
    NoSnapshot,
    AnalysisInFlight,
    StaleSnapshot,
    StaleContinuation,
    WorkspaceAmbiguous,
    ConfigInvalid,
    ItemNotFound,
    RouteStaticLimitation,
    UnsupportedProtocolVersion,
    UnsupportedProfile,
    Cancelled,
    Superseded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiprAgentResponseKind {
    WorkspaceStatus,
    AnalysisRefreshed,
    ActionableItems,
    RepairPacket,
    EvidenceContext,
    TopLimitation,
    ReceiptStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiprAgentRecoveryRoute {
    Refresh,
    SelectWorkspace,
    FixConfiguration,
    RequestSupportedVersion,
    Retry,
    InspectLimitation,
    DiscardStaleRequest,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RiprAgentImplementationState {
    CapabilityOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RiprAgentDiagnosticMode {
    Push,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiprAgentSourceEditCapability {
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiprAgentAllowedEditSurface {
    ReadOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiprAgentMustNotChange {
    SourceEdits,
    WorkspaceEdit,
    AutonomousRepair,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RiprAgentFreshness {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RiprAgentRunStatus {
    Ready,
    AnalysisInFlight,
    Stale,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RiprAgentSuccessStatus {
    #[serde(rename = "ok")]
    Ok,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RiprAgentErrorStatus {
    #[serde(rename = "error")]
    Error,
}

pub(super) const RESERVED_REQUESTS: &[RiprAgentRequest] = &[
    RiprAgentRequest::WorkspaceStatus,
    RiprAgentRequest::RefreshAnalysis,
    RiprAgentRequest::ListActionableItems,
    RiprAgentRequest::GetRepairPacket,
    RiprAgentRequest::GetEvidenceContext,
    RiprAgentRequest::GetTopLimitation,
    RiprAgentRequest::GetReceiptStatus,
];

pub(super) const RESERVED_ERROR_KINDS: &[RiprAgentErrorKind] = &[
    RiprAgentErrorKind::NoSnapshot,
    RiprAgentErrorKind::AnalysisInFlight,
    RiprAgentErrorKind::StaleSnapshot,
    RiprAgentErrorKind::StaleContinuation,
    RiprAgentErrorKind::WorkspaceAmbiguous,
    RiprAgentErrorKind::ConfigInvalid,
    RiprAgentErrorKind::ItemNotFound,
    RiprAgentErrorKind::RouteStaticLimitation,
    RiprAgentErrorKind::UnsupportedProtocolVersion,
    RiprAgentErrorKind::UnsupportedProfile,
    RiprAgentErrorKind::Cancelled,
    RiprAgentErrorKind::Superseded,
];

pub(super) const RESERVED_PROFILES: &[RiprAgentProfile] =
    &[RiprAgentProfile::Actionable, RiprAgentProfile::Full];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RiprAgentCapability {
    #[serde(flatten)]
    pub(super) versions: RiprAgentVersionIdentity,
    implementation_state: RiprAgentImplementationState,
    pub(super) supported_requests: Vec<RiprAgentRequest>,
    reserved_requests: Vec<RiprAgentRequest>,
    pub(super) supported_profiles: Vec<RiprAgentProfile>,
    reserved_profiles: Vec<RiprAgentProfile>,
    diagnostic_modes: Vec<RiprAgentDiagnosticMode>,
    snapshot_handles: bool,
    continuations: bool,
    work_done_progress: bool,
    cancellation: bool,
    pub(super) source_edit_capability: RiprAgentSourceEditCapability,
    analysis_status_notification: String,
    compatibility_commands: Vec<String>,
    error_kinds: Vec<RiprAgentErrorKind>,
    claim_boundary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    dead_code,
    reason = "reserved request envelope is defined before handlers land"
)]
pub(super) struct RiprAgentRequestEnvelope {
    #[serde(flatten)]
    pub(super) versions: RiprAgentVersionIdentity,
    pub(super) request: RiprAgentRequest,
    pub(super) mode: RiprAgentRequestMode,
    pub(super) profile: Option<RiprAgentProfile>,
    pub(super) snapshot_id: Option<String>,
    pub(super) continuation_id: Option<String>,
}

impl RiprAgentCapability {
    fn v0_1() -> Self {
        Self {
            versions: RiprAgentVersionIdentity::current(),
            implementation_state: RiprAgentImplementationState::CapabilityOnly,
            supported_requests: Vec::new(),
            reserved_requests: RESERVED_REQUESTS.to_vec(),
            supported_profiles: Vec::new(),
            reserved_profiles: RESERVED_PROFILES.to_vec(),
            diagnostic_modes: vec![RiprAgentDiagnosticMode::Push],
            snapshot_handles: false,
            continuations: false,
            work_done_progress: false,
            cancellation: false,
            source_edit_capability: RiprAgentSourceEditCapability::None,
            analysis_status_notification: "ripr/analysisStatus".to_string(),
            compatibility_commands: compatibility_commands()
                .into_iter()
                .map(str::to_string)
                .collect(),
            error_kinds: RESERVED_ERROR_KINDS.to_vec(),
            claim_boundary: concat!(
                "Capability negotiation only; ",
                "no riprAgent requests are implemented by this slice."
            )
            .to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RiprAgentServerCapability {
    #[serde(rename = "riprAgent")]
    ripr_agent: RiprAgentCapability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    dead_code,
    reason = "reserved success envelope is defined before handlers land"
)]
pub(super) struct RiprAgentSuccessEnvelope {
    #[serde(flatten)]
    pub(super) versions: RiprAgentVersionIdentity,
    pub(super) request: RiprAgentRequest,
    pub(super) kind: RiprAgentResponseKind,
    status: RiprAgentSuccessStatus,
    pub(super) snapshot_id: Option<String>,
    pub(super) input_identity: Option<String>,
    pub(super) root_identity: Option<String>,
    pub(super) config_identity: Option<String>,
    pub(super) base_identity: Option<String>,
    freshness: RiprAgentFreshness,
    run_status: RiprAgentRunStatus,
    pub(super) profile: Option<RiprAgentProfile>,
    pub(super) budget_identity: Option<String>,
    selected_count: u64,
    omitted_count: u64,
    total_count: u64,
    complete_evidence_identity: Option<String>,
    continuation_identity: Option<String>,
    pub(super) allowed_edit_surface: RiprAgentAllowedEditSurface,
    pub(super) must_not_change: Vec<RiprAgentMustNotChange>,
    verify_route: Option<String>,
    receipt_route: Option<String>,
    limitations: Vec<String>,
    non_claims: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RiprAgentError {
    pub(super) kind: RiprAgentErrorKind,
    retryable: bool,
    recovery_route: Option<RiprAgentRecoveryRoute>,
    snapshot_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    dead_code,
    reason = "reserved error envelope is defined before handlers land"
)]
pub(super) struct RiprAgentErrorEnvelope {
    #[serde(flatten)]
    pub(super) versions: RiprAgentVersionIdentity,
    pub(super) request: RiprAgentRequest,
    status: RiprAgentErrorStatus,
    pub(super) error: RiprAgentError,
    pub(super) allowed_edit_surface: RiprAgentAllowedEditSurface,
    pub(super) must_not_change: Vec<RiprAgentMustNotChange>,
}

fn compatibility_commands() -> [&'static str; 7] {
    [
        REFRESH_COMMAND,
        COLLECT_CONTEXT_COMMAND,
        COLLECT_EVIDENCE_CONTEXT_COMMAND,
        COLLECT_WORKSPACE_STATUS_COMMAND,
        COLLECT_REPAIR_PACKET_COMMAND,
        COLLECT_TOP_LIMITATION_COMMAND,
        COLLECT_RECEIPT_STATUS_COMMAND,
    ]
}

pub(super) fn server_capability() -> LSPAny {
    let capability = RiprAgentServerCapability {
        ripr_agent: RiprAgentCapability::v0_1(),
    };
    serde_json::to_value(capability).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn require_unique<T>(values: &[T], label: &str) -> Result<(), String>
    where
        T: Ord,
    {
        let unique = values.iter().collect::<BTreeSet<_>>();
        if unique.len() != values.len() {
            return Err(format!("{label} must not contain duplicate values"));
        }
        Ok(())
    }

    fn capability_fixture() -> Result<RiprAgentCapability, String> {
        let capability = server_capability();
        let agent = capability
            .get("riprAgent")
            .cloned()
            .ok_or_else(|| "expected riprAgent capability fixture".to_string())?;
        serde_json::from_value(agent).map_err(|error| format!("decode capability fixture: {error}"))
    }

    fn read_only_boundaries() -> Vec<RiprAgentMustNotChange> {
        vec![
            RiprAgentMustNotChange::SourceEdits,
            RiprAgentMustNotChange::WorkspaceEdit,
            RiprAgentMustNotChange::AutonomousRepair,
        ]
    }

    fn success_fixture() -> &'static str {
        r#"
        {
          "protocol_version": "0.1",
          "schema_version": "0.1",
          "request": "ripr/listActionableItems",
          "kind": "actionable_items",
          "status": "ok",
          "snapshot_id": "snapshot:abc",
          "input_identity": "input:def",
          "root_identity": "root:ghi",
          "config_identity": "config:jkl",
          "base_identity": "base:mno",
          "freshness": "fresh",
          "run_status": "ready",
          "profile": "actionable",
          "budget_identity": "budget:pqr",
          "selected_count": 1,
          "omitted_count": 0,
          "total_count": 1,
          "complete_evidence_identity": "evidence:stu",
          "continuation_identity": null,
          "allowed_edit_surface": "read_only",
          "must_not_change": ["source_edits", "workspace_edit", "autonomous_repair"],
          "verify_route": "ripr/verify",
          "receipt_route": "ripr/receipt",
          "limitations": ["capability_only"],
          "non_claims": ["not_runtime_mutation_proof"]
        }
        "#
    }

    fn error_fixture() -> &'static str {
        r#"
        {
          "protocol_version": "0.1",
          "schema_version": "0.1",
          "request": "ripr/getRepairPacket",
          "status": "error",
          "error": {
            "kind": "stale_snapshot",
            "retryable": true,
            "recovery_route": "refresh",
            "snapshot_id": "snapshot:old"
          },
          "allowed_edit_surface": "read_only",
          "must_not_change": ["source_edits", "workspace_edit", "autonomous_repair"]
        }
        "#
    }

    #[test]
    fn reserved_protocol_vocabularies_are_closed_and_unique() -> Result<(), String> {
        require_unique(RESERVED_REQUESTS, "reserved requests")?;
        require_unique(RESERVED_ERROR_KINDS, "reserved errors")?;
        require_unique(RESERVED_PROFILES, "reserved profiles")?;

        let capability = capability_fixture()?;
        if capability.reserved_requests != RESERVED_REQUESTS {
            return Err("capability request vocabulary drifted".to_string());
        }
        if capability.reserved_profiles != RESERVED_PROFILES {
            return Err("capability profile vocabulary drifted".to_string());
        }
        if capability.error_kinds != RESERVED_ERROR_KINDS {
            return Err("capability error vocabulary drifted".to_string());
        }

        for (label, unknown) in [
            ("request", r#""ripr/unknown""#),
            ("profile", r#""unknown""#),
            ("error", r#""unknown""#),
        ] {
            let result = match label {
                "request" => serde_json::from_str::<RiprAgentRequest>(unknown).map(|_| ()),
                "profile" => serde_json::from_str::<RiprAgentProfile>(unknown).map(|_| ()),
                "error" => serde_json::from_str::<RiprAgentErrorKind>(unknown).map(|_| ()),
                _ => Ok(()),
            };
            if result.is_ok() {
                return Err(format!(
                    "closed {label} vocabulary accepted an unknown value"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn capability_is_fail_closed_until_handlers_land() -> Result<(), String> {
        let capability = capability_fixture()?;
        if !capability.supported_requests.is_empty() {
            return Err("unimplemented request handlers must not be advertised".to_string());
        }
        if !capability.supported_profiles.is_empty() {
            return Err("unimplemented diagnostic profiles must not be advertised".to_string());
        }
        if capability.source_edit_capability != RiprAgentSourceEditCapability::None {
            return Err("the capability must remain read-only".to_string());
        }
        Ok(())
    }

    #[test]
    fn version_identities_are_explicit_and_independent() -> Result<(), String> {
        let capability = capability_fixture()?;
        if capability.versions.protocol_version.as_str() != RIPR_AGENT_PROTOCOL_VERSION {
            return Err("protocol version identity must be explicit".to_string());
        }
        if capability.versions.schema_version.as_str() != RIPR_AGENT_SCHEMA_VERSION {
            return Err("schema version identity must be explicit".to_string());
        }
        let encoded = serde_json::to_value(&capability)
            .map_err(|error| format!("encode capability fixture: {error}"))?;
        for field in ["protocol_version", "schema_version"] {
            if encoded
                .get(field)
                .and_then(serde_json::Value::as_str)
                .is_none()
            {
                return Err(format!(
                    "capability is missing independent `{field}` identity"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn unsupported_protocol_major_is_rejected() -> Result<(), String> {
        match RiprAgentProtocolVersion::parse("1.0") {
            Err(RiprAgentProtocolVersionError::UnsupportedMajor {
                received,
                supported,
            }) if received == 1 && supported == RIPR_AGENT_SUPPORTED_PROTOCOL_MAJOR => {}
            Err(error) => return Err(format!("unexpected protocol-version error: {error}")),
            Ok(_) => return Err("unsupported protocol major was accepted".to_string()),
        }
        if serde_json::from_str::<RiprAgentProtocolVersion>(r#""1.0""#).is_ok() {
            return Err("serde accepted an unsupported protocol major".to_string());
        }
        Ok(())
    }

    #[test]
    fn unsupported_schema_major_is_rejected() -> Result<(), String> {
        match RiprAgentSchemaVersion::parse("1.0") {
            Err(RiprAgentSchemaVersionError::UnsupportedMajor {
                received,
                supported,
            }) if received == 1 && supported == RIPR_AGENT_SUPPORTED_SCHEMA_MAJOR => {}
            Err(error) => return Err(format!("unexpected schema-version error: {error}")),
            Ok(_) => return Err("unsupported schema major was accepted".to_string()),
        }
        if serde_json::from_str::<RiprAgentSchemaVersion>(r#"\"1.0\""#).is_ok() {
            return Err("serde accepted an unsupported schema major".to_string());
        }
        Ok(())
    }

    #[test]
    fn request_envelope_keeps_mode_and_profile_typed() -> Result<(), String> {
        let request = RiprAgentRequestEnvelope {
            versions: RiprAgentVersionIdentity::current(),
            request: RiprAgentRequest::ListActionableItems,
            mode: RiprAgentRequestMode::ReadOnly,
            profile: Some(RiprAgentProfile::Actionable),
            snapshot_id: Some("snapshot:6".to_string()),
            continuation_id: None,
        };
        let encoded = serde_json::to_value(request).map_err(|error| error.to_string())?;
        for (field, expected) in [
            ("request", "ripr/listActionableItems"),
            ("mode", "read_only"),
            ("profile", "actionable"),
        ] {
            let actual = encoded
                .get(field)
                .ok_or_else(|| format!("request envelope omitted `{field}`"))?;
            if actual != &serde_json::Value::String(expected.to_string()) {
                return Err(format!("request envelope lost typed `{field}` wire field"));
            }
        }
        Ok(())
    }

    #[test]
    fn success_and_error_envelopes_are_typed_and_bounded() -> Result<(), String> {
        let success = serde_json::from_str::<RiprAgentSuccessEnvelope>(success_fixture())
            .map_err(|error| format!("decode success fixture: {error}"))?;
        if success.allowed_edit_surface != RiprAgentAllowedEditSurface::ReadOnly {
            return Err("success envelope must be read-only".to_string());
        }
        if success.must_not_change != read_only_boundaries() {
            return Err("success envelope read-only boundary drifted".to_string());
        }

        let error = serde_json::from_str::<RiprAgentErrorEnvelope>(error_fixture())
            .map_err(|error| format!("decode error fixture: {error}"))?;
        if error.error.kind != RiprAgentErrorKind::StaleSnapshot {
            return Err("error envelope kind did not remain typed".to_string());
        }
        if error.allowed_edit_surface != RiprAgentAllowedEditSurface::ReadOnly {
            return Err("error envelope must be read-only".to_string());
        }

        let bounded = format!("{}\n", success_fixture().trim());
        let unknown_field = bounded.replace(
            "\"status\": \"ok\"",
            "\"status\": \"ok\", \"unbounded\": true",
        );
        if serde_json::from_str::<RiprAgentSuccessEnvelope>(&unknown_field).is_ok() {
            return Err("success envelope accepted an unknown field".to_string());
        }
        Ok(())
    }

    #[test]
    fn source_edit_boundary_is_explicitly_read_only() -> Result<(), String> {
        let capability = capability_fixture()?;
        if capability.source_edit_capability != RiprAgentSourceEditCapability::None {
            return Err("capability advertised a source-edit surface".to_string());
        }
        let success = serde_json::from_str::<RiprAgentSuccessEnvelope>(success_fixture())
            .map_err(|error| format!("decode success fixture: {error}"))?;
        if success.allowed_edit_surface != RiprAgentAllowedEditSurface::ReadOnly {
            return Err("success envelope advertised a source-edit surface".to_string());
        }
        if !success
            .must_not_change
            .contains(&RiprAgentMustNotChange::SourceEdits)
        {
            return Err("success envelope must forbid source edits".to_string());
        }
        Ok(())
    }
}
