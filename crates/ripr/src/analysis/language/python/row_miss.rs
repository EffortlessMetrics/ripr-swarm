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
//! - `weak_assertion`: the family-selected assertion (#5572) is weak or smoke.
//! - `missing_input`: a strong family-selected assertion on a predicate change
//!   whose own owner calls all resolve both compared operands off the boundary
//!   (`python_row_boundary_omission`). Fail closed: an unbound call, an
//!   unresolved or computed operand, or another row's input is unknown, never
//!   an omission.
//! - `no_assertion`: a call-bearing row whose test is
//!   `extraction_complete_no_assertion` (#5571) after the conftest chain pass
//!   (#6657), with no activation control and no unread bare sibling module.
//!   Parametrize is activation evidence, never an oracle. Anything
//!   assertion-like, custom or dynamic is unresolved admission and keeps the
//!   row at `miss: None`.
//! - `observation_unconfirmed`: a strong family-selected assertion that this
//!   row alone does not credit as observing the changed sink. Python's
//!   alignment is a credit-only model, so no credit is an unresolved
//!   observation edge, never an affirmative mismatch. A local stand-in row
//!   (`fee = Mock(...)` shadowing the imported owner) lands here too:
//!   stand-in detection is textual and errs toward withholding credit.
//!
//! Deliberately not emitted, so the row stays `miss: None`:
//!
//! - `no_call_path`: a name-or-location-only relation (`same_stem`,
//!   `test_name_similarity`, `fixture_name`) cannot yet establish that the test
//!   never reaches the owner. Hidden call paths the extractor does not model
//!   (callbacks and aliases, parametrize and fixture `params`, `usefixtures`,
//!   module `pytestmark`, conftest and plugins) are the same blind spots as
//!   `no_assertion` (#6657), so the claim is withheld rather than guessed.
//! - `assertion_not_observing`: alignment cannot tell an affirmative sibling
//!   sink from an unresolved one. Row eligibility is the explicit
//!   `RelatedTest::role` (#5563), not this value, so emitting it no longer
//!   moves decisions; the value stays withheld here until alignment can make
//!   the affirmative claim.
//! - `assertion_not_credited`: no typed established refusal exists (#5446);
//!   skip / xfail rows stay `activation_limited` with no oracle credit.
//! - `missing_exact_assertion`: no row-owned exact-discriminator fact exists;
//!   `missing_discriminators` is finding-wide.
//!
//! `missing_input` carries `RelatedTest::miss_support` (#5564): the finding's
//! predicate boundary fact, attached only because this row's own omission
//! proof holds against it. When no boundary fact exists the row keeps the miss
//! with no support and renders the generic wording. No other miss carries
//! support.

use super::admission::PythonAssertionAdmission;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::python_row_boundary_omission;
use super::related_tests::PythonRelatedCandidate;
use super::sink_alignment::{AlignmentPrep, classify_sink_alignment_prepared};
use super::{PythonOwner, PythonTest};
use crate::domain::{
    ExposureClass, MissingDiscriminatorFact, OracleStrength, ProbeFamily, RelatedTest,
    RelatedTestMiss, RelatedTestMissSupport,
};

/// The changed line, as the classifier saw it.
pub(super) struct RowMissChange<'a> {
    pub(super) owner: &'a PythonOwner,
    pub(super) line_text: &'a str,
    pub(super) old_line_text: Option<&'a str>,
    /// Every strong related call provably enters the guards around the line
    /// (#9833), so a guarded write is read as run, as the finding read it.
    pub(super) guard_entered: bool,
    pub(super) literal_lines: Option<&'a (String, String)>,
    pub(super) focus: &'a PythonAssertionFocus,
    /// The finding's changed-boundary fact. It describes the change, not a
    /// row: a row may carry it as `miss_support` only after that row's own
    /// omission proof (`python_row_boundary_omission`) holds against it.
    pub(super) boundary_fact: Option<&'a MissingDiscriminatorFact>,
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
    // Rows are built one per candidate, in order (`classify.rs`), and nothing
    // reorders them before this pass. Fail closed if that ever stops holding:
    // a row must never take another test's reason.
    if rows.len() != candidates.len()
        || rows.iter().zip(candidates).any(|(row, candidate)| {
            row.name != candidate.test.name
                || row.file != candidate.test.file
                || row.line != candidate.test.line
        })
    {
        return;
    }
    // Built once per finding; each strong row is still aligned on its own.
    let prep = AlignmentPrep::new(change.owner, all_tests);
    for (row, candidate) in rows.iter_mut().zip(candidates) {
        row.miss = row_miss(row, candidate, candidates, change, &prep);
        // #5564: `missing_input` is assigned only from this row's own
        // omission proof, so the boundary that proof was made against is
        // this row's support. Any other miss carries none; a missing
        // boundary fact leaves the row generic.
        row.miss_support = match row.miss {
            Some(RelatedTestMiss::MissingInput) => {
                change.boundary_fact.map(RelatedTestMissSupport::from_fact)
            }
            _ => None,
        };
    }
}

fn row_miss(
    row: &RelatedTest,
    candidate: &PythonRelatedCandidate<'_>,
    candidates: &[PythonRelatedCandidate<'_>],
    change: &RowMissChange<'_>,
    prep: &AlignmentPrep<'_>,
) -> Option<RelatedTestMiss> {
    // A row related by name or location only (or a fixture, helper or dunder
    // protocol relation) carries no reason here: see the module docs.
    if !candidate.relation.uses_oracle() {
        return None;
    }
    // No selected assertion (none recognized, or none family-relevant):
    // admission is the authority, and only its established-absence state
    // writes `no_assertion`.
    if row.oracle.is_none() {
        return no_assertion_miss(candidate.test, change.owner);
    }
    match row.oracle_strength {
        OracleStrength::Weak | OracleStrength::Smoke => Some(RelatedTestMiss::WeakAssertion),
        OracleStrength::Strong => strong_row_miss(row, candidate, candidates, change, prep),
        _ => None,
    }
}

/// `no_assertion`: the row calls the owner and its test is established to hold no
/// assertion-like construct. Admission (`admission.rs`) is the authority, the
/// conftest chain pass (`fixtures.rs`, #6657) has already withheld every test
/// a conftest can assert around, and this adds the row-local fail-closed
/// guards: a test with a skip / xfail / expected-failure control is never
/// credited with a reason it cannot reach, and a bare-importable module
/// beside the test (test support RIPR cannot read) other than the module or
/// package under test keeps the row unknown. Invisible limits stay documented in RIPR-SPEC-0224.
fn no_assertion_miss(test: &PythonTest, owner: &PythonOwner) -> Option<RelatedTestMiss> {
    (test.assertion_admission == PythonAssertionAdmission::NoAssertionLike
        && test.assertions.is_empty()
        && test.activation_controls.is_empty()
        && test
            .bare_sibling_modules
            .iter()
            .all(|module| owner.file.starts_with(module)))
    .then_some(RelatedTestMiss::NoAssertion)
}

fn strong_row_miss(
    row: &RelatedTest,
    candidate: &PythonRelatedCandidate<'_>,
    candidates: &[PythonRelatedCandidate<'_>],
    change: &RowMissChange<'_>,
    prep: &AlignmentPrep<'_>,
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
    let alignment = classify_sink_alignment_prepared(
        change.owner,
        change.line_text,
        change.old_line_text,
        std::slice::from_ref(row),
        prep,
        change.guard_entered,
        change.literal_lines,
    );
    (!alignment.observes()).then_some(RelatedTestMiss::ObservationUnconfirmed)
}
