//! Row-local related-test miss assignment for ordinary TypeScript findings
//! (#5495, RIPR-SPEC-0224 "Language-neutral contract").
//!
//! `miss` explains one related-test row. It is non-decisional: this module is
//! called after the finding's class, stages, confidence, actionability and
//! evidence are final, writes only `RelatedTest::miss`, and reads only facts
//! the merged TypeScript row substrate already owns:
//!
//! - the owner-path disposition of the row (#5523);
//! - the row's assertion admission state (#5524);
//! - the row's family-relevant assertion (#5525);
//! - the row's predicate-boundary activation (#5527).
//!
//! Every fact is the row's own. A finding-wide boolean is never stamped on a
//! row, and one test's assertion, boundary input or observed expression never
//! satisfies another row. A row whose own evidence is unresolved keeps
//! `miss: None`; there is no fill-every-row fallback.
//!
//! Shipped tokens: `no_call_path`, `weak_assertion` and
//! `observation_unconfirmed`.
//!
//! Deliberately not emitted:
//!
//! - `assertion_not_observing`: `RelatedTest::is_unmatched` reads it as a
//!   decision input (gap next-step choice, witness and row triage), so
//!   emitting it would change TypeScript behavior. Rows that would have
//!   carried it get `observation_unconfirmed` or no miss.
//! - `no_assertion`: the admission scan sees one file and cannot see project
//!   setup files or runner plugins (`admission.rs` known blind spots), so
//!   `NoAssertionLike` is not strong enough to claim absence.
//! - `missing_input`: the shared prose says "no test input reaches ...",
//!   stronger than a single-file static reading supports. A row whose own
//!   boundary fact says its input misses the boundary gets
//!   `observation_unconfirmed` instead.
//! - `assertion_not_credited`: reserved for a typed established refusal
//!   (#5446), which TypeScript does not produce;
//! - `missing_exact_assertion`: the TypeScript model carries no row-owned
//!   changed-observable or exact-discriminator fact.
//!
//! Bun bridge findings are built in `bun_bridge.rs`, never reach this module,
//! and keep `miss: None`.

use super::*;
use crate::domain::RelatedTestMiss;

/// The finding-level facts row assignment reads. All are already final.
pub(crate) struct TsRowMissContext<'a> {
    pub(crate) class: &'a ExposureClass,
    /// A static limit marks the finding as an advisory static-limit state;
    /// no row inside it receives a speculative reason.
    pub(crate) static_limited: bool,
    pub(crate) probe_shape: &'a TypeScriptProbeShape,
    pub(crate) line_text: &'a str,
    pub(crate) owner: &'a TypeScriptOwner,
    pub(crate) alias_map: Option<&'a TsAliasMap>,
    pub(crate) workspace_root: Option<&'a Path>,
    pub(crate) credits: CreditFacts<'a>,
}

/// Assign the typed miss of every row. `rows` maps one-to-one onto
/// `candidates` and `activations`.
pub(crate) fn assign_row_misses(
    context: &TsRowMissContext<'_>,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    activations: &[TypeScriptPredicateActivation],
    rows: &mut [RelatedTest],
) {
    // Class gate (#5424): only a concrete gap class may carry a row miss.
    // `exposed`, `static_unknown`, `no_static_path` and a static-limited
    // finding keep every row at `None`.
    if !matches!(context.class, ExposureClass::WeaklyExposed) || context.static_limited {
        return;
    }
    for ((row, candidate), activation) in rows.iter_mut().zip(candidates).zip(activations) {
        row.miss = ts_row_miss(context, candidate, row, *activation);
    }
}

/// The relations linked by an uncertain heuristic that name no owner call.
fn is_name_or_proximity_heuristic(relation: TypeScriptRelationKind) -> bool {
    matches!(
        relation,
        TypeScriptRelationKind::SameFileProximity
            | TypeScriptRelationKind::DescribeName
            | TypeScriptRelationKind::TestName
    )
}

fn is_strong(strength: &OracleStrength) -> bool {
    strength.rank() >= OracleStrength::Strong.rank()
}

/// One row's miss from its own evidence, or `None` when that evidence is
/// below the honesty threshold.
pub(crate) fn ts_row_miss(
    context: &TsRowMissContext<'_>,
    candidate: &TypeScriptRelatedCandidate<'_>,
    row: &RelatedTest,
    activation: TypeScriptPredicateActivation,
) -> Option<RelatedTestMiss> {
    use TypeScriptOwnerPathDisposition as Disp;
    let test = candidate.test;
    let family = &context.probe_shape.family;
    let admission = test.assertion_admission;
    let selection = select_family_relevant_assertion(&test.assertions, Some(family));
    match candidate.owner_path() {
        // 1. Relation path. The heuristic never named an owner call.
        Disp::HeuristicOnly => is_name_or_proximity_heuristic(candidate.relation)
            .then_some(RelatedTestMiss::NoCallPath),
        // The identity gates found affirmative evidence that the apparent
        // call does not reach the changed owner.
        Disp::RejectedLocalShadow | Disp::RejectedUnrelatedImportOrDestructure => {
            Some(RelatedTestMiss::NoCallPath)
        }
        // The call reaches a replacement, so the row does call something, but
        // whether the assertion observes the changed owner is unknown.
        Disp::RejectedOwnerModuleMock | Disp::RejectedSpyFabrication => {
            admitted_assertion_row(admission, test)
                .then_some(RelatedTestMiss::ObservationUnconfirmed)
        }
        // Identity is possible but unproved, or the binding cannot be placed.
        // A relationship exists only when an assertion was admitted.
        Disp::OwnerNameCallUnanchored | Disp::UnresolvedAliasOrReexport => {
            admitted_assertion_row(admission, test)
                .then_some(RelatedTestMiss::ObservationUnconfirmed)
        }
        // The path is established (a module entry's path is real but
        // indirect). Judge admission, then the family-relevant assertion.
        Disp::TrustedOwnerPath | Disp::ModuleEntryPath => {
            established_path_miss(context, candidate, row, activation, admission, selection)
        }
    }
}

/// A relationship exists only when an assertion was admitted.
fn admitted_assertion_row(admission: TypeScriptAssertionAdmission, test: &TypeScriptTest) -> bool {
    admission == TypeScriptAssertionAdmission::Recognized && !test.assertions.is_empty()
}

fn established_path_miss(
    context: &TsRowMissContext<'_>,
    candidate: &TypeScriptRelatedCandidate<'_>,
    row: &RelatedTest,
    activation: TypeScriptPredicateActivation,
    admission: TypeScriptAssertionAdmission,
    selection: TypeScriptAssertionSelection<'_>,
) -> Option<RelatedTestMiss> {
    // 2. Assertion admission. Absence is never claimed (see the module docs),
    // so an unresolved or merely unobserved test keeps no reason.
    if admission != TypeScriptAssertionAdmission::Recognized {
        return None;
    }
    let assertion = match selection {
        TypeScriptAssertionSelection::Selected(assertion) => assertion,
        // Every admitted assertion observes another behavior family. A strong
        // one leaves the observed sink unconfirmed; a weak one says nothing.
        TypeScriptAssertionSelection::NoFamilyRelevant => {
            return candidate
                .test
                .assertions
                .iter()
                .any(|assertion| is_strong(&assertion.oracle_strength))
                .then_some(RelatedTestMiss::ObservationUnconfirmed);
        }
        TypeScriptAssertionSelection::NoAssertion => return None,
    };
    // 3. Weak oracle. The row's own oracle (selected assertion plus any
    // credit projected onto it) is below the strong threshold.
    let strength = if row.oracle_strength == OracleStrength::Unknown {
        &assertion.oracle_strength
    } else {
        &row.oracle_strength
    };
    if !is_strong(strength) {
        return Some(RelatedTestMiss::WeakAssertion);
    }
    // 4. Predicate activation: the row's own module-derived boundary fact
    // says its input misses the boundary. The stronger `missing_input` prose
    // is withheld, so the row reports the unconfirmed observation.
    if activation == TypeScriptPredicateActivation::MissedBoundary {
        return Some(RelatedTestMiss::ObservationUnconfirmed);
    }
    // 7. Observation. A module entry's own code stands between the assertion
    // and the owner, so observation is never confirmed from here.
    if candidate.owner_path() == TypeScriptOwnerPathDisposition::ModuleEntryPath {
        return Some(RelatedTestMiss::ObservationUnconfirmed);
    }
    // A predicate row that is not the witness has a boundary question this
    // row cannot answer.
    if !matches!(
        activation,
        TypeScriptPredicateActivation::NotApplicable | TypeScriptPredicateActivation::Witnessed
    ) {
        return Some(RelatedTestMiss::ObservationUnconfirmed);
    }
    // The finding-wide observation guard, evaluated over this row alone: a
    // row that confirms its own observation has no miss to explain.
    let observed = ts_changed_value_is_observed(
        context.probe_shape,
        context.line_text,
        context.owner,
        std::slice::from_ref(candidate),
        context.alias_map,
        context.workspace_root,
        context.credits,
    );
    (!observed).then_some(RelatedTestMiss::ObservationUnconfirmed)
}
