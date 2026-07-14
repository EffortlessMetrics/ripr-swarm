use super::{
    COLLECT_CONTEXT_COMMAND, COLLECT_EVIDENCE_CONTEXT_COMMAND, COLLECT_RECEIPT_STATUS_COMMAND,
    COLLECT_REPAIR_PACKET_COMMAND, COLLECT_TOP_LIMITATION_COMMAND,
    COLLECT_WORKSPACE_STATUS_COMMAND, REFRESH_COMMAND,
};
use tower_lsp_server::ls_types::LSPAny;

pub(super) const RIPR_AGENT_PROTOCOL_VERSION: &str = "0.1";

pub(super) const RESERVED_REQUESTS: &[&str] = &[
    "ripr/workspaceStatus",
    "ripr/refreshAnalysis",
    "ripr/listActionableItems",
    "ripr/getRepairPacket",
    "ripr/getEvidenceContext",
    "ripr/getTopLimitation",
    "ripr/getReceiptStatus",
];

pub(super) const RESERVED_ERROR_KINDS: &[&str] = &[
    "no_snapshot",
    "analysis_in_flight",
    "stale_snapshot",
    "stale_continuation",
    "workspace_ambiguous",
    "config_invalid",
    "item_not_found",
    "route_static_limitation",
    "unsupported_protocol_version",
    "unsupported_profile",
    "cancelled",
    "superseded",
];

const RESERVED_PROFILES: &[&str] = &["actionable", "full"];

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
    serde_json::json!({
        "riprAgent": {
            "protocol_version": RIPR_AGENT_PROTOCOL_VERSION,
            "implementation_state": "capability_only",
            "supported_requests": [],
            "reserved_requests": RESERVED_REQUESTS,
            "supported_profiles": [],
            "reserved_profiles": RESERVED_PROFILES,
            "diagnostic_modes": ["push"],
            "snapshot_handles": false,
            "continuations": false,
            "work_done_progress": false,
            "cancellation": false,
            "source_edit_capability": "none",
            "analysis_status_notification": "ripr/analysisStatus",
            "compatibility_commands": compatibility_commands(),
            "error_kinds": RESERVED_ERROR_KINDS,
            "claim_boundary": "Capability negotiation only; no riprAgent requests are implemented by this slice."
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn require_unique(values: &[&str], label: &str) -> Result<(), String> {
        let unique = values.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != values.len() {
            return Err(format!("{label} must not contain duplicate values"));
        }
        Ok(())
    }

    #[test]
    fn capability_is_fail_closed_until_handlers_land() -> Result<(), String> {
        let capability = server_capability();
        let agent = capability
            .get("riprAgent")
            .ok_or_else(|| "expected experimental.riprAgent capability".to_string())?;

        if agent.get("protocol_version").and_then(serde_json::Value::as_str)
            != Some(RIPR_AGENT_PROTOCOL_VERSION)
        {
            return Err("protocol version must be explicit".to_string());
        }
        if !agent
            .get("supported_requests")
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            return Err("unimplemented request handlers must not be advertised".to_string());
        }
        if !agent
            .get("supported_profiles")
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            return Err("unimplemented diagnostic profiles must not be advertised".to_string());
        }
        if agent.get("source_edit_capability").and_then(serde_json::Value::as_str)
            != Some("none")
        {
            return Err("the capability must remain read-only".to_string());
        }
        if agent.get("snapshot_handles").and_then(serde_json::Value::as_bool) != Some(false)
            || agent.get("continuations").and_then(serde_json::Value::as_bool) != Some(false)
            || agent.get("cancellation").and_then(serde_json::Value::as_bool) != Some(false)
        {
            return Err("future protocol behavior must remain explicitly disabled".to_string());
        }
        Ok(())
    }

    #[test]
    fn reserved_protocol_vocabularies_are_closed_and_unique() -> Result<(), String> {
        require_unique(RESERVED_REQUESTS, "reserved requests")?;
        require_unique(RESERVED_ERROR_KINDS, "reserved errors")?;
        require_unique(RESERVED_PROFILES, "reserved profiles")?;

        let capability = server_capability();
        let agent = capability
            .get("riprAgent")
            .ok_or_else(|| "expected experimental.riprAgent capability".to_string())?;
        if agent.get("reserved_requests") != Some(&serde_json::json!(RESERVED_REQUESTS)) {
            return Err("capability request vocabulary drifted".to_string());
        }
        if agent.get("error_kinds") != Some(&serde_json::json!(RESERVED_ERROR_KINDS)) {
            return Err("capability error vocabulary drifted".to_string());
        }
        Ok(())
    }
}
