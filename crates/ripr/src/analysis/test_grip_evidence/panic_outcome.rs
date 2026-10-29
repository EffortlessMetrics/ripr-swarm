//! Unknown, not a gap, for a `#[should_panic]` test whose panic may be the
//! changed code's (RIPR-SPEC-0240, #10703).
//!
//! `check` withholds a `reachable_unrevealed` gap under the typed
//! `rust_panic_outcome_unresolved` limit when an assertion-free
//! `#[should_panic]` test calls the owner directly and the owner can panic at
//! or after the changed line (`withhold_should_panic_gap`). This is the
//! repo-seam reading of that same decision: it calls the same shared
//! predicate ([`should_panic_tests_deciding_changed_panic`]) and names the
//! same typed limit, so `ripr repair` cannot start an attempt for a seam
//! `check` reports as a named limitation rather than a gap.
//!
//! Stages that read `no` or `weak` become `unknown` and the missing values
//! are dropped, so the seam reads as an unknown with a limit; the repair
//! eligibility flip additionally refuses any seam whose evidence names the
//! limit (`TestGripEvidence::names_static_limit`). Fail-closed: it only
//! withholds, never admits.

use super::TestGripEvidence;
use crate::analysis::classify::should_panic_tests_deciding_changed_panic;
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::domain::{Confidence, StageEvidence, StageState, StaticLimitKind};
use std::path::Path;

pub(super) const LIMIT: StaticLimitKind = StaticLimitKind::RustPanicOutcomeUnresolved;

/// The labels of the `#[should_panic]` tests that make the seam's panic
/// outcome unresolved; empty when the limitation does not apply.
/// `direct_related` is every directly-calling related test as
/// `(name, file, line)`, read before any display cap.
pub(super) fn deciding_tests<'a>(
    direct_related: impl IntoIterator<Item = (&'a str, &'a Path, usize)>,
    owner: Option<&FunctionSummary>,
    changed_line: usize,
    index: &RustIndex,
) -> Vec<String> {
    let Some(owner) = owner else {
        return Vec::new();
    };
    should_panic_tests_deciding_changed_panic(owner, changed_line, direct_related, index)
}

fn clause(tests: &[String]) -> String {
    format!(
        "{} ({} passes only if something panics, and ripr does not resolve whether the changed code causes that panic), so no gap is reported",
        LIMIT.as_str(),
        tests.join(", ")
    )
}

/// Withhold the absence claims of `evidence` under the panic-outcome limit.
/// `tests` is [`deciding_tests`]. Returns `true` when the evidence changed.
pub(super) fn withhold(evidence: &mut TestGripEvidence, tests: &[String]) -> bool {
    if tests.is_empty() || !matches!(evidence.reach.state, StageState::Yes | StageState::Weak) {
        return false;
    }
    let clause = clause(tests);
    let mut changed = false;
    for (label, stage) in [
        ("Activation", &mut evidence.activate),
        ("Propagation", &mut evidence.propagate),
        ("Observation", &mut evidence.observe),
        ("Discrimination", &mut evidence.discriminate),
    ] {
        if matches!(stage.state, StageState::No | StageState::Weak) {
            *stage = StageEvidence::new(
                StageState::Unknown,
                Confidence::Low,
                format!("{label} unresolved: {clause}"),
            );
            changed = true;
        }
    }
    if !evidence.missing_discriminators.is_empty() {
        evidence.missing_discriminators.clear();
        changed = true;
    }
    // A stage that was already unknown or yes keeps its state; the typed
    // reason still has to be reachable from the evidence record.
    if !evidence.names_static_limit(LIMIT) {
        evidence.discriminate = StageEvidence::new(
            if evidence.discriminate.state == StageState::Opaque {
                StageState::Opaque
            } else {
                StageState::Unknown
            },
            Confidence::Low,
            format!("Discrimination unresolved: {clause}"),
        );
        changed = true;
    }
    changed
}
