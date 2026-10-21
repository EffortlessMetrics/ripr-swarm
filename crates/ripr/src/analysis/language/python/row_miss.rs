//! Row-local `RelatedTestMiss` for Python related tests (#5491,
//! RIPR-SPEC-0224 language-neutral contract).
//!
//! `miss` explains one related-test row and is evidence, not a verdict: this
//! pass reads the finished class and the retained row evidence and writes only
//! `RelatedTest::miss`. It never changes the class, stages, confidence,
//! ordering, repair placement or identity.
//!
//! Emitted (only on a `weakly_exposed` finding, and only from facts the row
//! owns; a finding-wide fact never lends a row its reason):
//!
//! - `no_call_path`: a name-or-location-only relation (`same_stem`,
//!   `test_name_similarity`, `fixture_name`). `dunder_protocol` is excluded:
//!   the test builds the owner's class and reaches the method through an
//!   implicit protocol call, so "no call to the changed code" would be false.
//! - `weak_assertion`: the family-selected assertion (#5572) is weak or smoke.
//! - `missing_input`: a strong family-selected assertion on a predicate change
//!   whose own owner calls all resolve both compared operands off the boundary.
//! - `observation_unconfirmed`: a strong family-selected assertion that this
//!   row alone does not credit as observing the changed sink. Python's
//!   alignment is a credit-only model, so no credit is an unresolved
//!   observation edge, never an affirmative mismatch.
//!
//! Deliberately not emitted, so the row stays `miss: None`:
//!
//! - `no_assertion`: `extraction_complete_no_assertion` (#5571) still has the
//!   cross-file blind spots of #6657 (conftest autouse fixtures, plugins, ini
//!   `filterwarnings`, bare sibling imports); until they are accounted for the
//!   state is not an established absence.
//! - `assertion_not_observing`: alignment cannot tell an affirmative sibling
//!   sink from an unresolved one, and the value also marks a row as
//!   examined-only (`RelatedTest::is_unmatched`), which decisional consumers
//!   (`oracle_related_tests`) read, so emitting it would move decisions.
//! - `assertion_not_credited`: no typed established refusal exists (#5446);
//!   skip / xfail rows stay `activation_limited` with no oracle credit.
//! - a local stand-in row (`fee = Mock(...)` shadowing the imported owner) keeps
//!   its `observation_unconfirmed`: stand-in detection is textual and errs
//!   toward withholding credit, so it cannot support an affirmative no-call claim.
//! - `missing_exact_assertion`: no row-owned exact-discriminator fact exists;
//!   `missing_discriminators` is finding-wide.

use super::assertion_selection::PythonAssertionFocus;
use super::boundary::python_row_boundary_omission;
use super::related_tests::{PythonRelatedCandidate, PythonRelationKind};
use super::sink_alignment::classify_sink_alignment_with_old;
use super::{PythonOwner, PythonTest};
use crate::domain::{ExposureClass, OracleStrength, ProbeFamily, RelatedTest, RelatedTestMiss};

/// The changed line, as the classifier saw it.
pub(super) struct RowMissChange<'a> {
    pub(super) owner: &'a PythonOwner,
    pub(super) line_text: &'a str,
    pub(super) old_line_text: Option<&'a str>,
    pub(super) focus: &'a PythonAssertionFocus,
}

/// Annotate `rows` (one per `candidates` entry, in order) in place.
pub(super) fn annotate_row_misses(
    rows: &mut [RelatedTest],
    candidates: &[PythonRelatedCandidate<'_>],
    class: &ExposureClass,
    has_static_limit: bool,
    change: &RowMissChange<'_>,
    all_tests: &[PythonTest],
) {
    // Class gate: only a concrete supported gap class carries row misses.
    if has_static_limit || !matches!(class, ExposureClass::WeaklyExposed) {
        return;
    }
    for (row, candidate) in rows.iter_mut().zip(candidates) {
        row.miss = row_miss(row, candidate, candidates, change, all_tests);
    }
}

fn row_miss(
    row: &RelatedTest,
    candidate: &PythonRelatedCandidate<'_>,
    candidates: &[PythonRelatedCandidate<'_>],
    change: &RowMissChange<'_>,
    all_tests: &[PythonTest],
) -> Option<RelatedTestMiss> {
    if !candidate.relation.uses_oracle() {
        return match candidate.relation {
            PythonRelationKind::SameStem
            | PythonRelationKind::TestNameSimilarity
            | PythonRelationKind::FixtureName => Some(RelatedTestMiss::NoCallPath),
            // A fixture or same-module caller does call the owner, a dunder
            // protocol row reaches it implicitly, and a skip / xfail row is
            // `activation_limited`: no path claim.
            _ => None,
        };
    }
    // No selected assertion (none recognized, or none family-relevant):
    // admission is the authority and nothing is emitted from it here.
    row.oracle.as_ref()?;
    match row.oracle_strength {
        OracleStrength::Weak | OracleStrength::Smoke => Some(RelatedTestMiss::WeakAssertion),
        OracleStrength::Strong => strong_row_miss(row, candidate, candidates, change, all_tests),
        _ => None,
    }
}

fn strong_row_miss(
    row: &RelatedTest,
    candidate: &PythonRelatedCandidate<'_>,
    candidates: &[PythonRelatedCandidate<'_>],
    change: &RowMissChange<'_>,
    all_tests: &[PythonTest],
) -> Option<RelatedTestMiss> {
    if matches!(change.focus.family(), ProbeFamily::Predicate)
        && python_row_boundary_omission(
            change.line_text,
            change.old_line_text,
            change.owner,
            candidate,
            candidates,
            change.focus,
        )
    {
        return Some(RelatedTestMiss::MissingInput);
    }
    let alignment = classify_sink_alignment_with_old(
        change.owner,
        change.line_text,
        change.old_line_text,
        std::slice::from_ref(row),
        all_tests,
    );
    (!alignment.observes()).then_some(RelatedTestMiss::ObservationUnconfirmed)
}
