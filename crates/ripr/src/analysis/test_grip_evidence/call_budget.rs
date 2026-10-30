//! Unknown, not a gap, for a related test at the call-fact budget (#8600).
//!
//! `extract_call_facts` keeps at most `MAX_CALL_FACTS_PER_LINE` distinct
//! callees per source line and drops the rest without a marker. A seam's
//! absence claims (no input reaching the value, no observing assertion, no
//! discriminating oracle, a missing boundary value) read the related tests'
//! calls, so a related test with a line at that budget cannot establish them:
//! the dropped call may be the assertion helper or the boundary input.
//!
//! This is the repo-seam reading of the decision `withhold_call_budget_gap`
//! makes for a diff finding. It derives from the same shared saturation
//! predicate and names the same typed `rust_call_fact_budget` static limit, so
//! the `repo-exposure-json` seam carries the reason the diff finding does and
//! no consumer re-derives it.
//!
//! A stage that read `no` or `weak` becomes `unknown` and the missing values
//! built from those reads are dropped, so the seam reads as an unknown with a
//! limit instead of a gap and `is_static_limited` keeps it off every repair
//! route. A stage that is already `unknown` gains the reason. A positive
//! (every stage `yes`, no missing value), an `ungripped` seam and a seam whose
//! reach is already opaque are left alone: a dropped call can only remove
//! reach, never invent a gap.

use super::TestGripEvidence;
use crate::analysis::extract::{MAX_CALL_FACTS_PER_LINE, has_saturated_line};
use crate::analysis::rust_index::TestSummary;
use crate::domain::{Confidence, StageEvidence, StageState, StaticLimitKind};

const LIMIT: StaticLimitKind = StaticLimitKind::RustCallFactBudget;

/// Whether any of `related` has a line at the call-fact budget. `related` must
/// be every matched related test, read before any display cap trims the rows.
pub(super) fn any_saturated<'a>(related: impl IntoIterator<Item = &'a TestSummary>) -> bool {
    related
        .into_iter()
        .any(|test| has_saturated_line(test.calls.iter()))
}

fn clause() -> String {
    format!(
        "{} ({MAX_CALL_FACTS_PER_LINE} distinct calls on one line of a related test is the per-line call budget), so ripr may not see every call there; no gap is reported",
        LIMIT.as_str()
    )
}

fn summary(stage: &str) -> String {
    format!("{stage} unresolved: {}", clause())
}

/// Withhold the absence claims of `evidence` when a related test is at the
/// call-fact budget. `saturated` is [`any_saturated`] over the full related
/// set. Returns `true` when the evidence changed.
pub(super) fn withhold(evidence: &mut TestGripEvidence, saturated: bool) -> bool {
    if !saturated || !matches!(evidence.reach.state, StageState::Yes | StageState::Weak) {
        return false;
    }
    let mut changed = false;
    for (label, stage) in [
        ("Activation", &mut evidence.activate),
        ("Propagation", &mut evidence.propagate),
        ("Observation", &mut evidence.observe),
        ("Discrimination", &mut evidence.discriminate),
    ] {
        if matches!(stage.state, StageState::No | StageState::Weak) {
            *stage = StageEvidence::new(StageState::Unknown, Confidence::Low, summary(label));
            changed = true;
        }
    }
    if !evidence.missing_discriminators.is_empty() {
        // A missing boundary value is a claim over the test's inputs, and a
        // dropped call such as `row(99)` could be that input.
        evidence.missing_discriminators.clear();
        if evidence.discriminate.state == StageState::Yes {
            evidence.discriminate = StageEvidence::new(
                StageState::Unknown,
                Confidence::Low,
                summary("Discrimination"),
            );
        }
        changed = true;
    }
    // An already-unknown or opaque stage keeps its class and gains the
    // reason, even when a missing value was cleared above: otherwise the
    // typed category would be unreachable from the evidence record.
    if !stage_names_limit(evidence) {
        for stage in [
            &mut evidence.activate,
            &mut evidence.propagate,
            &mut evidence.observe,
            &mut evidence.discriminate,
        ] {
            if matches!(stage.state, StageState::Unknown | StageState::Opaque) {
                stage.summary = format!("{}; {}", stage.summary, clause());
                return true;
            }
        }
    }
    changed
}

fn stage_names_limit(evidence: &TestGripEvidence) -> bool {
    [
        &evidence.activate,
        &evidence.propagate,
        &evidence.observe,
        &evidence.discriminate,
    ]
    .iter()
    .any(|stage| stage.summary.contains(LIMIT.as_str()))
}

#[cfg(test)]
mod tests;
