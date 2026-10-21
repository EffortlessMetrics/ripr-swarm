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
//!   A body that uses the owner name in any way other than a recognized
//!   non-invoking reference (`callable(owner)`, an alias never used again,
//!   `owner is ...`, `owner.__name__`) may still invoke it through a shape the
//!   call extractor does not model (`map(owner, ...)`, `f = owner; f(0)`), so
//!   it keeps no miss.
//! - `weak_assertion`: the family-selected assertion (#5572) is weak or smoke.
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
//! - `missing_input`: pending follow-up (#5491). The row-owned omission needs
//!   import-resolved owner calls and per-row operand resolution; until that
//!   lands a strong row that observes nothing falls to
//!   `observation_unconfirmed`, which claims less.
//! - `missing_exact_assertion`: no row-owned exact-discriminator fact exists;
//!   `missing_discriminators` is finding-wide.

use super::related_tests::{PythonRelatedCandidate, PythonRelationKind};
use super::sink_alignment::classify_sink_alignment_with_old;
use super::{PythonOwner, PythonTest};
use crate::domain::{ExposureClass, OracleStrength, OwnerKind, RelatedTest, RelatedTestMiss};

/// The changed line, as the classifier saw it.
pub(super) struct RowMissChange<'a> {
    pub(super) owner: &'a PythonOwner,
    pub(super) line_text: &'a str,
    pub(super) old_line_text: Option<&'a str>,
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
    for (row, candidate) in rows.iter_mut().zip(candidates) {
        row.miss = row_miss(row, candidate, change, all_tests);
    }
}

fn row_miss(
    row: &RelatedTest,
    candidate: &PythonRelatedCandidate<'_>,
    change: &RowMissChange<'_>,
    all_tests: &[PythonTest],
) -> Option<RelatedTestMiss> {
    if !candidate.relation.uses_oracle() {
        return match candidate.relation {
            PythonRelationKind::SameStem
            | PythonRelationKind::TestNameSimilarity
            | PythonRelationKind::FixtureName => (callable_owner(change.owner)
                && !body_may_invoke_owner(&candidate.test.body_text, &change.owner.name))
            .then_some(RelatedTestMiss::NoCallPath),
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
        OracleStrength::Strong => strong_row_miss(row, change, all_tests),
        _ => None,
    }
}

fn strong_row_miss(
    row: &RelatedTest,
    change: &RowMissChange<'_>,
    all_tests: &[PythonTest],
) -> Option<RelatedTestMiss> {
    let alignment = classify_sink_alignment_with_old(
        change.owner,
        change.line_text,
        change.old_line_text,
        std::slice::from_ref(row),
        all_tests,
    );
    (!alignment.observes()).then_some(RelatedTestMiss::ObservationUnconfirmed)
}

/// "No call to the changed code" is a claim about a callable. A module
/// constant or module-level statement is read, not called, so a row for it
/// cannot carry `no_call_path`.
fn callable_owner(owner: &PythonOwner) -> bool {
    matches!(
        owner.owner_kind,
        Some(OwnerKind::Function | OwnerKind::Method | OwnerKind::ClassMethod)
    )
}

/// Fail-closed: true unless every occurrence of `name` in `body` is a
/// recognized non-invoking reference. A body with no occurrence at all (a
/// fixture-name or stem relation) has nothing that could invoke the owner.
fn body_may_invoke_owner(body: &str, name: &str) -> bool {
    let bytes = body.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut from = 0;
    while let Some(rel) = body[from..].find(name) {
        let start = from + rel;
        let end = start + name.len();
        from = end;
        if (start > 0 && is_ident(bytes[start - 1])) || (end < bytes.len() && is_ident(bytes[end]))
        {
            continue;
        }
        let before = body[..start].trim_end();
        let after = body[end..].trim_start_matches([' ', '\t']);
        if after.starts_with("is ") || after.starts_with(".__") {
            continue;
        }
        if before.ends_with('(') && enclosing_callee(&body[..start]) == "callable" {
            continue;
        }
        if let Some(lhs) = before
            .strip_suffix('=')
            .filter(|l| !l.ends_with(['=', '!', '<', '>']))
        {
            let alias: String = lhs
                .trim_end()
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if !alias.is_empty()
                && word_count(body, &alias) == 1
                && (after.is_empty() || after.starts_with(['\n', '#', ';']))
            {
                continue;
            }
        }
        return true;
    }
    false
}

fn word_count(body: &str, word: &str) -> usize {
    let bytes = body.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    body.match_indices(word)
        .filter(|(i, _)| {
            let end = i + word.len();
            (*i == 0 || !is_ident(bytes[i - 1])) && (end >= bytes.len() || !is_ident(bytes[end]))
        })
        .count()
}

/// The identifier before the innermost unclosed `(` ending `prefix`.
fn enclosing_callee(prefix: &str) -> &str {
    let mut depth = 0usize;
    for (i, c) in prefix.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' if depth == 0 => {
                let head = &prefix[..i];
                let start = head
                    .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map_or(0, |p| p + 1);
                return &head[start..];
            }
            '(' => depth -= 1,
            _ => {}
        }
    }
    ""
}
