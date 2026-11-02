//! Shared fix-instruction authority for LSP surfaces (#1663 / #1895).
//!
//! Diagnostics and hover derive [`FixInstructionSummary`] from the witness;
//! code actions must consume the same summary instead of re-inferring
//! actionability from optional fields.

use super::action_contract::ActionDisabledReason;
use super::state::AnalysisSnapshot;
use crate::domain::{DiagnosticWitness, FixInstructionState, FixInstructionSummary};
use serde_json::Value;
use tower_lsp_server::ls_types::Diagnostic;

fn summary_from_diagnostic_data(data: &Value) -> Option<FixInstructionSummary> {
    if let Some(value) = data.get("fix_instruction") {
        return serde_json::from_value(value.clone()).ok();
    }
    let witness: DiagnosticWitness = serde_json::from_value(data.get("witness")?.clone()).ok()?;
    Some(FixInstructionSummary::from_witness(&witness))
}

/// The fix-instruction summary already projected into diagnostic data, if any.
pub(super) fn optional_summary_from_diagnostic(
    diagnostic: &Diagnostic,
) -> Option<FixInstructionSummary> {
    diagnostic
        .data
        .as_ref()
        .and_then(summary_from_diagnostic_data)
}

/// Summary for a seam-addressed action: diagnostic payload first, then the
/// finding witness the CLI card and hover already use.
pub(super) fn summary_for_seam_action(
    snapshot: &AnalysisSnapshot,
    diagnostic: &Diagnostic,
) -> Option<FixInstructionSummary> {
    if let Some(summary) = optional_summary_from_diagnostic(diagnostic) {
        return Some(summary);
    }
    snapshot
        .finding_for_diagnostic(diagnostic)
        .and_then(DiagnosticWitness::from_finding)
        .map(|witness| FixInstructionSummary::from_witness(&witness))
}

/// Summary for a gap-addressed action (witness/fix_instruction on the gap
/// diagnostic only).
pub(super) fn summary_for_gap_action(diagnostic: &Diagnostic) -> Option<FixInstructionSummary> {
    optional_summary_from_diagnostic(diagnostic)
}

/// Repair-packet and agent-loop actions require the shared `fix_site_ready`
/// vocabulary today; other states stay inspect-only or inert.
pub(super) fn blocks_repair_ready_actions(summary: &FixInstructionSummary) -> bool {
    summary.state != FixInstructionState::FixSiteReady
}

pub(super) fn disabled_reason_for_instruction_state(
    state: FixInstructionState,
) -> Option<ActionDisabledReason> {
    use ActionDisabledReason::{PreviewOrStaticLimitation, StaleSnapshot};
    match state {
        FixInstructionState::FixSiteReady => None,
        FixInstructionState::Stale => Some(StaleSnapshot),
        FixInstructionState::StaticLimitation => Some(PreviewOrStaticLimitation),
        FixInstructionState::InspectOnly | FixInstructionState::Unavailable => {
            Some(PreviewOrStaticLimitation)
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
        let parsed = optional_summary_from_diagnostic(&diagnostic);
        assert_eq!(parsed.as_ref(), Some(&summary));
        assert!(parsed.is_some_and(|parsed| !blocks_repair_ready_actions(&parsed)));
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
            disabled_reason_for_instruction_state(summary.state),
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
            disabled_reason_for_instruction_state(summary.state),
            Some(ActionDisabledReason::StaleSnapshot)
        );
    }
}
