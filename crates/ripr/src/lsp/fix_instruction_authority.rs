//! Shared fix-instruction authority for LSP surfaces (#1663 / #1895).
//!
//! Diagnostics and hover derive [`FixInstructionSummary`] from the witness;
//! code actions must consume the same summary instead of re-inferring
//! actionability from optional fields.

use super::action_contract::ActionDisabledReason;
use crate::analysis::ClassifiedSeam;
use crate::analysis::repair_route::{
    RepairPacketIneligibility, cross_language_test_target_unresolved, repair_packet_eligibility,
};
use crate::domain::{DiagnosticWitness, EvidenceState, FixInstructionState, FixInstructionSummary};
use crate::output::agent_seam_packets::suggested_assertion_for_classified_seam;
use serde_json::Value;
use tower_lsp_server::ls_types::Diagnostic;

fn json_str<'a>(data: &'a Value, key: &str) -> Option<&'a str> {
    data.get(key).and_then(Value::as_str)
}

fn summary_from_diagnostic_data(data: &Value) -> Option<FixInstructionSummary> {
    if let Some(value) = data.get("fix_instruction") {
        return serde_json::from_value(value.clone()).ok();
    }
    let witness: DiagnosticWitness = serde_json::from_value(data.get("witness")?.clone()).ok()?;
    Some(FixInstructionSummary::from_witness(&witness))
}

fn summary_from_gap_ledger_data(data: &Value) -> FixInstructionSummary {
    if let Some(summary) = summary_from_diagnostic_data(data) {
        return summary;
    }
    let gap_state = json_str(data, "gap_state");
    if gap_state.is_some_and(|state| state.contains("stale")) {
        return FixInstructionSummary {
            state: FixInstructionState::Stale,
            has_fix_site: false,
            has_suggested_assertion: false,
            limitation_kinds: vec!["snapshot_stale".to_string()],
            instruction_semantic_id: None,
            instruction_instance_id: None,
        };
    }
    if json_str(data, "static_limit_kind").is_some()
        || json_str(data, "evidence_class").is_some_and(|value| value == "static_limitation")
    {
        return FixInstructionSummary {
            state: FixInstructionState::StaticLimitation,
            has_fix_site: false,
            has_suggested_assertion: false,
            limitation_kinds: vec!["static_limitation".to_string()],
            instruction_semantic_id: None,
            instruction_instance_id: None,
        };
    }
    if gap_state.is_some_and(EvidenceState::is_actionable_word) {
        let has_fix_site = data.get("anchor").is_some()
            || data
                .get("repair_route")
                .and_then(|route| route.get("target_file"))
                .is_some();
        return FixInstructionSummary {
            state: if has_fix_site {
                FixInstructionState::FixSiteReady
            } else {
                FixInstructionState::InspectOnly
            },
            has_fix_site,
            has_suggested_assertion: false,
            limitation_kinds: Vec::new(),
            instruction_semantic_id: None,
            instruction_instance_id: None,
        };
    }
    FixInstructionSummary::unavailable()
}

/// The kind a not-yet-repairable seam carries: it fails the producer-owned
/// repair-packet flip, so ripr has no bounded test to point at yet.
const REPAIR_ROUTE_NOT_READY: &str = "repair_route_not_ready";

/// The seam's own fix-instruction summary, from the producer-owned
/// repair-packet flip (`repair_packet_eligibility`, RIPR-SPEC-0087 §8).
///
/// A seam passing the flip is `fix_site_ready`: the flip is the gate
/// `ripr agent repair --seam-id` and the seam hover's "repair (start here)"
/// line already consume, so the code action must agree with them (#13716).
/// The seam is not re-derived from a `ripr check` finding witness: that is
/// a finding-level projection keyed by a different id family, and on the
/// same change it can report no related test while the seam holds one, which
/// withheld the start on every real seam diagnostic.
fn summary_for_seam(seam: &ClassifiedSeam) -> FixInstructionSummary {
    let limitation = |kind: &str| FixInstructionSummary {
        state: FixInstructionState::StaticLimitation,
        has_fix_site: false,
        has_suggested_assertion: false,
        limitation_kinds: vec![kind.to_string()],
        instruction_semantic_id: None,
        instruction_instance_id: None,
    };
    if cross_language_test_target_unresolved(seam) {
        return limitation("cross_language_target_unresolved");
    }
    match repair_packet_eligibility(seam).ineligibility {
        None => {}
        // Real static limitations ripr cannot trace keep that name.
        Some(
            RepairPacketIneligibility::CrossLanguageOracleVisibilityUnresolved
            | RepairPacketIneligibility::CrossLanguageTestTargetUnresolved,
        ) => return limitation("cross_language_target_unresolved"),
        Some(RepairPacketIneligibility::NamedStaticLimitation) => {
            return limitation("static_limitation");
        }
        // No bounded test target yet: not a limitation of what ripr can see.
        Some(
            RepairPacketIneligibility::ClassNotHeadlineEligible
            | RepairPacketIneligibility::RouteNotReady,
        ) => return limitation(REPAIR_ROUTE_NOT_READY),
    }
    FixInstructionSummary {
        state: FixInstructionState::FixSiteReady,
        has_fix_site: true,
        has_suggested_assertion: suggested_assertion_for_classified_seam(seam).is_some(),
        limitation_kinds: Vec::new(),
        instruction_semantic_id: None,
        instruction_instance_id: None,
    }
}

/// Summary for a seam-addressed action: an explicit diagnostic payload
/// first, then the seam's own producer-owned repair-packet flip.
pub(super) fn summary_for_seam_action(
    diagnostic: &Diagnostic,
    seam: &ClassifiedSeam,
) -> FixInstructionSummary {
    if let Some(data) = diagnostic.data.as_ref()
        && let Some(summary) = summary_from_diagnostic_data(data)
    {
        return summary;
    }
    summary_for_seam(seam)
}

/// Summary for a gap-addressed action from embedded witness/fix_instruction
/// or producer-owned gap-ledger fields.
pub(super) fn summary_for_gap_action(diagnostic: &Diagnostic) -> FixInstructionSummary {
    diagnostic
        .data
        .as_ref()
        .map(summary_from_gap_ledger_data)
        .unwrap_or_else(FixInstructionSummary::unavailable)
}

/// Repair-packet and agent-loop actions require the shared `fix_site_ready`
/// vocabulary today; other states stay inspect-only or inert.
pub(super) fn blocks_repair_ready_actions(summary: &FixInstructionSummary) -> bool {
    summary.state != FixInstructionState::FixSiteReady
}

pub(super) fn disabled_reason_for_instruction_state(
    summary: &FixInstructionSummary,
) -> Option<ActionDisabledReason> {
    use ActionDisabledReason::{PreviewOrStaticLimitation, RepairRouteNotReady, StaleSnapshot};
    match summary.state {
        FixInstructionState::FixSiteReady => None,
        FixInstructionState::Stale => Some(StaleSnapshot),
        // Only a real static limitation names one; a seam that merely has no
        // bounded test yet must not be blamed on a language boundary (#13716).
        FixInstructionState::StaticLimitation
            if summary
                .limitation_kinds
                .iter()
                .any(|kind| kind == REPAIR_ROUTE_NOT_READY) =>
        {
            Some(RepairRouteNotReady)
        }
        FixInstructionState::StaticLimitation => Some(PreviewOrStaticLimitation),
        // No limitation was named and no fix site is known: ripr simply has
        // not picked a test to extend, which is not a language boundary.
        FixInstructionState::InspectOnly | FixInstructionState::Unavailable => {
            Some(RepairRouteNotReady)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        DiagnosticConfidence, DiagnosticFixSite, DiagnosticWitness, DiagnosticWitnessLimitation,
    };

    fn witness_fix_site_ready() -> DiagnosticWitness {
        DiagnosticWitness {
            kind: "static_discriminator_gap".to_string(),
            probe_family: "test".to_string(),
            changed_expression: "expr".to_string(),
            before: None,
            after: None,
            expected_sink: None,
            missing_discriminators: Vec::new(),
            fix_site: Some(DiagnosticFixSite {
                file: "src/lib.rs".to_string(),
                line: 10,
                test_name: "test_foo".to_string(),
                current_oracle: None,
                oracle_kind: "exact_value".to_string(),
                oracle_strength: "strong".to_string(),
                oracle_location: None,
            }),
            suggested_assertion: None,
            explain_command: "ripr explain".to_string(),
            confidence: DiagnosticConfidence {
                value: Some(0.8),
                basis: "static_only".to_string(),
            },
            limitations: Vec::new(),
        }
    }

    #[test]
    fn diagnostic_data_fix_instruction_round_trips() {
        let summary = FixInstructionSummary::from_witness(&witness_fix_site_ready());
        let data = serde_json::json!({ "fix_instruction": summary });
        let diagnostic = Diagnostic {
            data: Some(data),
            ..Diagnostic::default()
        };
        let parsed = summary_for_gap_action(&diagnostic);
        assert_eq!(parsed, summary);
        assert!(!blocks_repair_ready_actions(&parsed));
    }

    #[test]
    fn gap_ledger_actionable_state_with_anchor_is_fix_site_ready() {
        let data = serde_json::json!({
            "source": "gap_decision_ledger",
            "gap_state": "actionable",
            "anchor": { "file": "src/lib.rs", "line": 1 }
        });
        let summary = summary_from_gap_ledger_data(&data);
        assert_eq!(summary.state, FixInstructionState::FixSiteReady);
        assert!(!blocks_repair_ready_actions(&summary));
    }

    #[test]
    fn gap_ledger_static_limit_blocks_repair_ready_actions() {
        let data = serde_json::json!({
            "source": "gap_decision_ledger",
            "gap_state": "actionable",
            "static_limit_kind": "missing_discriminator"
        });
        let summary = summary_from_gap_ledger_data(&data);
        assert!(blocks_repair_ready_actions(&summary));
    }

    #[test]
    fn static_limitation_blocks_repair_ready_actions() {
        let mut witness = witness_fix_site_ready();
        witness.fix_site = None;
        witness.limitations = vec![DiagnosticWitnessLimitation {
            kind: "missing_discriminator_unavailable".to_string(),
            detail: "no safe discriminator".to_string(),
        }];
        let summary = FixInstructionSummary::from_witness(&witness);
        assert!(blocks_repair_ready_actions(&summary));
        assert_eq!(
            disabled_reason_for_instruction_state(&summary),
            Some(ActionDisabledReason::PreviewOrStaticLimitation)
        );
    }

    #[test]
    fn stale_maps_to_stale_snapshot_disabled_reason() {
        let mut witness = witness_fix_site_ready();
        witness.limitations = vec![DiagnosticWitnessLimitation {
            kind: "snapshot_stale".to_string(),
            detail: "stale".to_string(),
        }];
        let summary = FixInstructionSummary::from_witness(&witness);
        assert_eq!(
            disabled_reason_for_instruction_state(&summary),
            Some(ActionDisabledReason::StaleSnapshot)
        );
    }
}
