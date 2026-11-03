//! Aggregate evaluation and semantic packet digest (#4510).

use sha2::{Digest, Sha256};

use super::model::{
    Aggregate, CandidateProofControl, CandidateProofPacket, CandidateProofRow, SubjectCounts,
};

pub(crate) fn subject_counts(rows: &[CandidateProofRow]) -> SubjectCounts {
    let selected = rows.len();
    let executed = rows
        .iter()
        .filter(|row| {
            !matches!(
                row.state,
                super::model::HarnessExecutionState::NotRun
                    | super::model::HarnessExecutionState::InstrumentFailure
            )
        })
        .count();
    let passed = rows.iter().filter(|row| row.state.is_pass()).count();
    let failed = rows
        .iter()
        .filter(|row| {
            !matches!(
                row.state,
                super::model::HarnessExecutionState::NotRun
                    | super::model::HarnessExecutionState::Passed
            )
        })
        .count();
    let skipped = selected.saturating_sub(executed);
    SubjectCounts {
        selected,
        executed,
        passed,
        failed,
        skipped,
    }
}

pub(crate) fn aggregate_packet(
    counts: &SubjectCounts,
    rows: &[CandidateProofRow],
    controls: &[CandidateProofControl],
    work_cleanup: &str,
) -> (Aggregate, Vec<String>) {
    let mut reasons = Vec::new();
    if counts.selected == 0 {
        reasons.push("zero selected rows".to_string());
    }
    if counts.executed == 0 && counts.selected > 0 {
        reasons.push("zero executed rows".to_string());
    }
    for row in rows {
        if !row.state.is_pass() {
            reasons.push(format!("row `{}` is {}", row.id, row.state.as_str()));
        }
    }
    for control in controls {
        if !control.discriminated {
            reasons.push(format!("control `{}` did not discriminate", control.id));
        }
    }
    if !work_cleanup.starts_with("removed") {
        reasons.push(format!("work root cleanup: {work_cleanup}"));
    }
    let aggregate = if reasons.is_empty() {
        Aggregate::Passed
    } else {
        Aggregate::Failed
    };
    (aggregate, reasons)
}

pub(crate) fn packet_digest(packet: &CandidateProofPacket) -> Result<String, String> {
    let mut value = serde_json::to_value(packet)
        .map_err(|error| format!("failed to serialize harness packet: {error}"))?;
    if let Some(object) = value.as_object_mut() {
        object.remove("digest");
        if let Some(candidate) = object
            .get_mut("candidate")
            .and_then(|value| value.as_object_mut())
        {
            candidate.remove("path");
        }
    }
    let text = serde_json::to_string(&value)
        .map_err(|error| format!("failed to render harness packet identity: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(text.as_bytes())))
}
