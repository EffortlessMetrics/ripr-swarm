use super::build::{stage_delta_json, targeted_test_outcome_gap_summary};
use super::render_md::{
    review_focused_proof_added, review_movement_after_verification,
    review_remaining_weak_or_unknown, review_ripr_flagged_before, review_should_inspect,
    review_what_changed, reviewer_may_believe, reviewer_should_not_believe,
};
use super::*;
use crate::output::json;

pub(crate) fn render_targeted_test_outcome_json(
    report: &TargetedTestOutcomeReport,
) -> Result<String, String> {
    let value = serde_json::json!({
        "schema_version": TARGETED_TEST_OUTCOME_SCHEMA_VERSION,
        "tool": "ripr",
        "status": "advisory",
        "inputs": {
            "before": report.before_path.as_str(),
            "after": report.after_path.as_str()
        },
        "before": report.before_counts,
        "after": report.after_counts,
        "summary": {
            "moved": report.moved.len(),
            "unchanged": report.unchanged.len(),
            "regressed": report.regressed.len(),
            "new": report.new.len(),
            "removed": report.removed.len(),
            "gap_movement": targeted_test_outcome_gap_summary_json(report)
        },
        "moved": report.moved.iter().map(targeted_test_outcome_movement_json).collect::<Vec<_>>(),
        "unchanged": report.unchanged.iter().map(targeted_test_outcome_movement_json).collect::<Vec<_>>(),
        "regressed": report.regressed.iter().map(targeted_test_outcome_movement_json).collect::<Vec<_>>(),
        "new": report.new.iter().map(targeted_test_outcome_seam_json).collect::<Vec<_>>(),
        "removed": report.removed.iter().map(targeted_test_outcome_seam_json).collect::<Vec<_>>(),
        "review_receipt": targeted_test_outcome_review_receipt_json(report)
    });
    json::render_pretty_with_newline(&value, "targeted-test outcome")
}

pub(crate) fn render_agent_verify_json(
    report: &TargetedTestOutcomeReport,
) -> Result<String, String> {
    let improved = report
        .moved
        .iter()
        .filter(|movement| movement.direction == "improved")
        .count();
    let changed = report
        .moved
        .iter()
        .filter(|movement| movement.direction != "improved")
        .count();
    let changed_seams = report
        .moved
        .iter()
        .chain(report.regressed.iter())
        .map(agent_verify_movement_json)
        .collect::<Vec<_>>();

    let value = serde_json::json!({
        "schema_version": AGENT_VERIFY_SCHEMA_VERSION,
        "tool": "ripr",
        "status": "advisory",
        "inputs": {
            "before": report.before_path.as_str(),
            "after": report.after_path.as_str()
        },
        "summary": {
            "improved": improved,
            "changed": changed,
            "regressed": report.regressed.len(),
            "unchanged": report.unchanged.len(),
            "new": report.new.len(),
            "resolved": report.removed.len(),
            "gap_movement": targeted_test_outcome_gap_summary_json(report)
        },
        "changed_seams": changed_seams,
        "unchanged_seams": report.unchanged.iter().map(agent_verify_movement_json).collect::<Vec<_>>(),
        "new_gaps": report.new.iter().map(|seam| agent_verify_seam_json(seam, "new")).collect::<Vec<_>>(),
        "resolved_gaps": report.removed.iter().map(|seam| agent_verify_seam_json(seam, "resolved")).collect::<Vec<_>>()
    });
    json::render_pretty_with_newline(&value, "agent verify")
}

fn targeted_test_outcome_movement_json(movement: &TargetedTestOutcomeMovement) -> Value {
    serde_json::json!({
        "seam_id": movement.seam_id.as_str(),
        "seam_kind": movement.seam_kind.as_str(),
        "file": movement.file.as_str(),
        "line": movement.line,
        "before": movement.before.as_str(),
        "after": movement.after.as_str(),
        "direction": movement.direction.as_str(),
        "gap_movement": movement.gap_movement.as_str(),
        "evidence_delta": movement.evidence_delta,
        "evidence_source": movement.evidence_source.as_str(),
        "reach_delta": movement.reach_delta.as_ref().map(stage_delta_json),
        "activate_delta": movement.activate_delta.as_ref().map(stage_delta_json),
        "propagate_delta": movement.propagate_delta.as_ref().map(stage_delta_json),
        "observe_delta": movement.observe_delta.as_ref().map(stage_delta_json),
        "discriminate_delta": movement.discriminate_delta.as_ref().map(stage_delta_json),
        "observed_values_added": movement.observed_values_added,
        "observed_values_removed": movement.observed_values_removed,
        "missing_discriminators_resolved": movement.missing_discriminators_resolved,
        "missing_discriminators_reopened": movement.missing_discriminators_reopened,
        "oracle_strength_delta": movement.oracle_strength_delta.as_deref(),
        "related_test_delta": movement.related_test_delta,
        "no_movement_reason": movement.no_movement_reason.as_deref()
    })
}

fn targeted_test_outcome_seam_json(seam: &TargetedTestOutcomeSeam) -> Value {
    serde_json::json!({
        "seam_id": seam.seam_id.as_str(),
        "seam_kind": seam.seam_kind.as_str(),
        "file": seam.file.as_str(),
        "line": seam.line,
        "grip_class": seam.grip_class.as_str()
    })
}

fn targeted_test_outcome_review_receipt_json(report: &TargetedTestOutcomeReport) -> Value {
    serde_json::json!({
        "gap_movement": targeted_test_outcome_gap_summary_json(report),
        "what_changed": review_what_changed(report),
        "ripr_flagged_before": review_ripr_flagged_before(report),
        "focused_proof_added": review_focused_proof_added(report),
        "movement_after_verification": review_movement_after_verification(report),
        "remaining_weak_or_unknown": review_remaining_weak_or_unknown(report),
        "reviewer_should_inspect": review_should_inspect(report),
        "reviewer_may_believe": reviewer_may_believe(report),
        "reviewer_should_not_believe": reviewer_should_not_believe()
    })
}

fn targeted_test_outcome_gap_summary_json(report: &TargetedTestOutcomeReport) -> Value {
    let summary = targeted_test_outcome_gap_summary(report);
    serde_json::json!({
        "closed": summary.closed,
        "opened": summary.opened,
        "strengthened": summary.strengthened,
        "weakened": summary.weakened,
        "unchanged": summary.unchanged,
        "new": summary.new,
        "removed": summary.removed,
        "changed": summary.changed,
    })
}

fn agent_verify_movement_json(movement: &TargetedTestOutcomeMovement) -> Value {
    serde_json::json!({
        "seam_id": movement.seam_id.as_str(),
        "seam_kind": movement.seam_kind.as_str(),
        "file": movement.file.as_str(),
        "line": movement.line,
        "before": movement.before.as_str(),
        "after": movement.after.as_str(),
        "change": movement.direction.as_str(),
        "gap_movement": movement.gap_movement.as_str(),
        "evidence_delta": movement.evidence_delta,
        "evidence_source": movement.evidence_source.as_str(),
        "reach_delta": movement.reach_delta.as_ref().map(stage_delta_json),
        "activate_delta": movement.activate_delta.as_ref().map(stage_delta_json),
        "propagate_delta": movement.propagate_delta.as_ref().map(stage_delta_json),
        "observe_delta": movement.observe_delta.as_ref().map(stage_delta_json),
        "discriminate_delta": movement.discriminate_delta.as_ref().map(stage_delta_json),
        "observed_values_added": movement.observed_values_added,
        "observed_values_removed": movement.observed_values_removed,
        "missing_discriminators_resolved": movement.missing_discriminators_resolved,
        "missing_discriminators_reopened": movement.missing_discriminators_reopened,
        "oracle_strength_delta": movement.oracle_strength_delta.as_deref(),
        "related_test_delta": movement.related_test_delta,
        "no_movement_reason": movement.no_movement_reason.as_deref()
    })
}

fn agent_verify_seam_json(seam: &TargetedTestOutcomeSeam, change: &str) -> Value {
    serde_json::json!({
        "seam_id": seam.seam_id.as_str(),
        "seam_kind": seam.seam_kind.as_str(),
        "file": seam.file.as_str(),
        "line": seam.line,
        "grip_class": seam.grip_class.as_str(),
        "change": change
    })
}
