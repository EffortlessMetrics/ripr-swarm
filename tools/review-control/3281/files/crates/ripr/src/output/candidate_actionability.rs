//! Shared candidate-side source eligibility for actionability-bearing outputs.
//!
//! The producer-owned [`Finding::candidate_actionability`] projection is the
//! only authority on whether a finding may address candidate source. Output
//! modules compose their existing class, policy, suppression, route, and
//! verification rules with this boundary; they never infer currentness from a
//! path, coordinate, classification, or prose.

use crate::domain::{CandidateActionability, ExposureClass, Finding};
use serde_json::{Value, json};

pub(crate) fn candidate_actionability_for(finding: &Finding) -> CandidateActionability {
    finding.candidate_actionability()
}

pub(crate) fn is_candidate_eligible(finding: &Finding) -> bool {
    candidate_actionability_for(finding).eligible()
}

/// Candidate-current findings that already satisfy the diff finding class
/// boundary for a concrete repair investigation. Static limitations and
/// already-observed findings remain non-actionable even when current.
pub(crate) fn is_candidate_actionable_finding(finding: &Finding) -> bool {
    is_candidate_eligible(finding)
        && matches!(
            finding.class,
            ExposureClass::ReachableUnrevealed
                | ExposureClass::WeaklyExposed
                | ExposureClass::NoStaticPath
        )
}

/// Resolve candidate actionability from the producer-owned wire field.
///
/// Missing, malformed, or unknown values fail closed to `SubjectUnresolved`;
/// consumers must never recover currentness from file paths, coordinates,
/// classes, or repair prose.
pub(crate) fn candidate_actionability_from_json(finding: &Value) -> CandidateActionability {
    match finding.get("source_currentness").and_then(Value::as_str) {
        Some("candidate_current") => CandidateActionability::CandidateEligible,
        Some("base_deleted") => CandidateActionability::HistoricalDeleted,
        Some("moved_or_renamed") => CandidateActionability::MovementUnresolved,
        Some("unresolved_subject") | Some(_) | None => CandidateActionability::SubjectUnresolved,
    }
}

pub(crate) fn candidate_actionability_json_from_status(
    actionability: CandidateActionability,
) -> Value {
    json!({
        "status": actionability.as_str(),
        "eligible": actionability.eligible(),
        "edit_target": actionability.edit_target(),
        "revision_context": actionability.revision_context(),
        "reason": actionability.reason(),
    })
}

pub(crate) fn candidate_actionability_json_value(finding: &Finding) -> Value {
    candidate_actionability_json_from_status(candidate_actionability_for(finding))
}

pub(crate) fn non_candidate_next_step(finding: &Finding) -> Option<String> {
    let actionability = candidate_actionability_for(finding);
    match actionability {
        CandidateActionability::CandidateEligible => None,
        CandidateActionability::HistoricalDeleted => Some(concat!(
            "No candidate repair is available: the source expression was deleted from the ",
            "candidate. Retain this as base-side change evidence and inspect the base revision ",
            "only when historical context is required."
        )
        .to_string()),
        CandidateActionability::MovementUnresolved => Some(concat!(
            "No candidate repair target is proven: movement or rename evidence exists, but the ",
            "producer did not establish the exact candidate source identity. Resolve movement ",
            "identity before projecting a head edit."
        )
        .to_string()),
        CandidateActionability::SubjectUnresolved => Some(concat!(
            "No candidate repair target is proven: the producing surface did not establish ",
            "source currentness. Resolve source identity before projecting a head edit."
        )
        .to_string()),
    }
}

pub(crate) fn non_candidate_limitation_category(finding: &Finding) -> Option<&'static str> {
    match candidate_actionability_for(finding) {
        CandidateActionability::CandidateEligible => None,
        CandidateActionability::HistoricalDeleted => Some("historical_deleted_source"),
        CandidateActionability::MovementUnresolved => Some("candidate_movement_unresolved"),
        CandidateActionability::SubjectUnresolved => Some("candidate_source_unresolved"),
    }
}

pub(crate) fn non_candidate_gap_state(finding: &Finding) -> Option<&'static str> {
    match candidate_actionability_for(finding) {
        CandidateActionability::CandidateEligible => None,
        CandidateActionability::HistoricalDeleted => Some("historical_evidence"),
        CandidateActionability::MovementUnresolved | CandidateActionability::SubjectUnresolved => {
            Some("static_limitation")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{candidate_actionability_from_json, candidate_actionability_json_from_status};
    use crate::domain::CandidateActionability;
    use serde_json::json;

    #[test]
    fn json_currentness_resolution_is_controlled_and_fail_closed() {
        let cases = [
            ("candidate_current", CandidateActionability::CandidateEligible),
            ("base_deleted", CandidateActionability::HistoricalDeleted),
            ("moved_or_renamed", CandidateActionability::MovementUnresolved),
            ("unresolved_subject", CandidateActionability::SubjectUnresolved),
            ("future_value", CandidateActionability::SubjectUnresolved),
        ];
        for (value, expected) in cases {
            assert_eq!(
                candidate_actionability_from_json(&json!({"source_currentness": value})),
                expected
            );
        }
        assert_eq!(
            candidate_actionability_from_json(&json!({})),
            CandidateActionability::SubjectUnresolved
        );
        assert_eq!(
            candidate_actionability_from_json(&json!({"source_currentness": 7})),
            CandidateActionability::SubjectUnresolved
        );
    }

    #[test]
    fn candidate_actionability_json_names_revision_and_edit_boundary() {
        let current = candidate_actionability_json_from_status(
            CandidateActionability::CandidateEligible,
        );
        assert_eq!(current["status"], "candidate_eligible");
        assert_eq!(current["eligible"], true);
        assert_eq!(current["edit_target"], true);
        assert_eq!(current["revision_context"], "candidate");

        let deleted = candidate_actionability_json_from_status(
            CandidateActionability::HistoricalDeleted,
        );
        assert_eq!(deleted["status"], "historical_deleted");
        assert_eq!(deleted["eligible"], false);
        assert_eq!(deleted["edit_target"], false);
        assert_eq!(deleted["revision_context"], "base");
    }
}
