use super::ReviewPlacement;
use crate::agent::loop_commands::display_path;
use crate::analysis::ClassifiedSeam;
use crate::app::agent_brief::{AgentBriefResolvedWorkingSet, AgentBriefSelectedSeam};
use crate::output::agent_seam_packets;
use crate::output::agent_seam_packets::missing_discriminator_records_for as missing_records_for;
use crate::output::evidence_record::{
    actionability_for, cross_language_test_target_unresolved, gap_state_for,
};
use crate::output::gap_decision_ledger::GapRecord;
use crate::output::markdown::code_span;
use serde_json::{Value, json};
use std::path::Path;

use super::placement::{is_test_like_path, normalize_path_text};
use super::render::gap_record_id;

pub(super) fn review_warnings_json(warnings: &[String]) -> Vec<Value> {
    warnings
        .iter()
        .filter(|warning| !warning.trim().is_empty())
        .map(|warning| json!({ "kind": "other", "message": warning }))
        .collect()
}

pub(super) fn gap_record_cap_suppressed_json(record: &GapRecord) -> Value {
    json!({
        "gap_id": gap_record_id(record),
        "file": record.anchor.as_ref().and_then(|anchor| anchor.file.clone()),
        "line": record.anchor.as_ref().and_then(|anchor| anchor.line),
        "reason": "summary_cap",
        "message": "The PR guidance summary item cap was reached.",
    })
}

pub(super) fn card_gap_state(
    entry: &ClassifiedSeam,
    missing: &[crate::output::agent_seam_packets_legacy::MissingRecord],
) -> &'static str {
    if cross_language_test_target_unresolved(entry) {
        return "static_limitation";
    }
    gap_state_for(entry, &actionability_for(entry, missing))
}

/// Whether the card for `entry` offers a test to add.
pub(super) fn card_is_actionable(entry: &ClassifiedSeam) -> bool {
    card_gap_state(entry, &missing_records_for(entry)) == "actionable"
}

pub(super) fn suppressed_json(
    selected: &AgentBriefSelectedSeam<'_>,
    reason: &str,
    message: &str,
) -> Value {
    let seam = &selected.seam.seam;
    json!({
        "seam_id": seam.id().as_str(),
        "file": display_path(seam.file()),
        "line": seam.display_line(),
        "reason": reason,
        "message": message,
    })
}

pub(super) fn changed_test_paths(working_set: &AgentBriefResolvedWorkingSet) -> Vec<String> {
    let mut paths = working_set
        .changed_lines
        .iter()
        .filter(|line| is_test_like_path(&line.file))
        .map(|line| normalize_path_text(&line.file))
        .chain(
            working_set
                .changed_tests
                .iter()
                .flat_map(|changes| &changes.files)
                .map(|path| normalize_path_text(path)),
        )
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

pub(super) fn reason_for(
    selected: &AgentBriefSelectedSeam<'_>,
    gap_state: &str,
    missing: Option<&str>,
    why_not_actionable: Option<&str>,
) -> String {
    // Optional discriminator values do not grant repair authority. Project
    // the same canonical decision that owns the card's typed guidance. This
    // sentence reaches a pull request author as an annotation, so it says
    // what RIPR can do (nothing to add) and why, not which internal state
    // the card is in.
    if gap_state != "actionable" {
        return match (
            why_not_actionable
                .map(|why| why.trim().trim_end_matches('.'))
                .filter(|why| !why.is_empty()),
            gap_state,
        ) {
            (Some(why), _) => format!("RIPR has no test to suggest here: {why}."),
            (None, "static_limitation") => {
                "RIPR has no test to suggest here: it could not follow this change through to a test assertion."
                    .to_string()
            }
            (None, state) => {
                format!("RIPR has no test to suggest here (static evidence state: {state}).")
            }
        };
    }
    if let Some(missing) = missing {
        return format!(
            "Static evidence names missing discriminator {} for this seam.",
            code_span(missing)
        );
    }
    format!(
        "Static evidence class is {}; a focused test can strengthen the named seam.",
        selected.seam.class.as_str()
    )
}

pub(super) fn suggested_test_intent(shape: &agent_seam_packets::AssertionShape) -> &'static str {
    if !shape.is_concrete() {
        return match shape.state() {
            crate::repair_guidance::AssertionState::RequiresObserverSetup => {
                "Establish the named observer before adding an assertion."
            }
            crate::repair_guidance::AssertionState::FixSiteOnly => {
                "Inspect the named fix site; no assertion example is available."
            }
            crate::repair_guidance::AssertionState::VerificationOnly => {
                "Use the verification route; no assertion example is available."
            }
            crate::repair_guidance::AssertionState::Stale => {
                "Refresh the analysis before using assertion guidance."
            }
            crate::repair_guidance::AssertionState::Unresolved => {
                "Inspect the producer-owned seam evidence before designing an assertion."
            }
            crate::repair_guidance::AssertionState::Concrete => {
                "Add one focused discriminator test."
            }
        };
    }
    match shape.kind().map(|kind| kind.as_str()).unwrap_or_default() {
        "exact_error_variant" => "Add an exact error-variant test.",
        "side_effect_observer" => "Add a side-effect observer test.",
        "call_expectation" => "Add a call-observation test.",
        _ => "Add one focused discriminator test.",
    }
}
