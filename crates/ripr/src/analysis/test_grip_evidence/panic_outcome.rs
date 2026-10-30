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
//! The seam keeps its stage states and missing values, so seam
//! classification and every ranking that reads them (pilot picks, repo
//! exposure counts) are unchanged. Only the discrimination summary gains the
//! typed limit, and the repair eligibility flip refuses any seam whose
//! evidence names it (`TestGripEvidence::names_static_limit`). Fail-closed: it
//! only withholds a repair start, never admits one.
//!
//! Scope: `check` applies the predicate to each changed line; a repo seam is
//! a candidate changed line, so the repair decision for one seam is the
//! decision `check` would give if that line changed.

use super::TestGripEvidence;
use crate::analysis::classify::should_panic_tests_deciding_changed_panic;
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::analysis::seam_classification::classify_seam;
use crate::analysis::seams::{RepoSeam, SeamGripClass};
use crate::domain::StaticLimitKind;
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

/// Name the panic-outcome limit on `evidence`'s discrimination summary.
/// `tests` is [`deciding_tests`]. Returns `true` when the evidence changed.
///
/// Gated on the condition `withhold_should_panic_gap` applies to a finding:
/// the seam must read `reachable_unrevealed` (no assertion observes the
/// change). A seam that also has a weak asserting test is a weak-grip gap, so
/// it keeps its repair route.
pub(super) fn withhold(seam: &RepoSeam, evidence: &mut TestGripEvidence, tests: &[String]) -> bool {
    if tests.is_empty()
        || evidence.names_static_limit(LIMIT)
        || classify_seam(seam, evidence) != SeamGripClass::ReachableUnrevealed
    {
        return false;
    }
    evidence.discriminate.summary = format!(
        "{}; Discrimination unresolved: {}",
        evidence.discriminate.summary,
        clause(tests)
    );
    true
}

#[cfg(test)]
mod tests;
