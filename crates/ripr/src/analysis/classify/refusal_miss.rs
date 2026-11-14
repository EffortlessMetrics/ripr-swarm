//! Split `assertion_not_credited` into inert vs admission-unproven (#5446).

use super::owner_pin::{AssertionRefusal, OwnerPinSyntax};
use crate::analysis::classify::{ProbeContext, direct_call_name};
use crate::analysis::facts::{SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind};
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::domain::{RelatedTest, RelatedTestMiss};
use std::collections::BTreeSet;
use std::path::Path;

pub(in crate::analysis) fn assertion_refusal_counts_as_analyzer_limit(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    refusal: &AssertionRefusal,
) -> bool {
    let owner_name = context.owner_fn.map_or("", |owner| owner.name.as_str());
    (refusal.is_analyzer_limit()
        || (refusal.is_gated_item() && test_shares_owner_cfg_gates(context, test))
        || context.owner_fn.is_some_and(|owner| {
            owner_compiled_only_under_refused_declaration(context.index, owner, test, refusal)
        }))
        && refusal
            .value_guard_subject()
            .is_none_or(|subject| direct_call_name(subject) == Some(owner_name))
}

fn test_shares_owner_cfg_gates(context: &ProbeContext<'_>, test: &TestSummary) -> bool {
    let Some(owner) = context.owner_fn else {
        return false;
    };
    let gates = |file: &Path, name: &str, line: usize| {
        let facts = context.index.files().get(file)?;
        crate::analysis::syntax::unestablished_cfg_gates_of_fn(&facts.source, name, line)
    };
    let (Some(test_gates), Some(owner_gates)) = (
        gates(&test.file, &test.name, test.start_line),
        gates(&owner.file, &owner.name, owner.start_line),
    ) else {
        return false;
    };
    !test_gates.is_empty() && test_gates.is_subset(&owner_gates)
}

/// #14463: the test's file is refused because an out-of-line `mod name;`
/// declaration on its module chain carries a gate ripr cannot evaluate
/// (`#[cfg(feature = "x")] mod x;`). When the changed owner's file is reached
/// only through that same declaration, the owner is compiled exactly when the
/// test is, so the gate cannot keep the test from checking the owner: the
/// refusal is a limit of ripr's reading, as #9643 decided for an inline gated
/// module. Every gate on the test's side must be one the owner also carries:
/// - the owner's module ancestry is one linear chain through the refused
///   declaration, so no other path compiles the owner without it;
/// - every declaration on the test's chain is either on the owner's chain and
///   gated only by `cfg`s ripr cannot evaluate (or not gated), or not on it
///   and available in every test build (no attribute, or `cfg(test)`); a
///   provably-off gate such as `cfg(any())` keeps the gap, as it does for the
///   inline form;
/// - the test's in-file `cfg` gates are a subset of the owner's;
/// - the test's own refusal of each assertion, read past the provenance
///   refusal, is none, an analyzer limit, or that shared gate, so an ignored
///   test, an expected panic or a conditional assertion keeps the gap.
///
/// #14537: the predicate family reads this same rule rather than skipping the
/// Module edge unconditionally, so an unknown gate admits the equality only
/// when the owner is reached solely through that declaration.
pub(in crate::analysis) fn owner_compiled_only_under_refused_declaration(
    index: &RustIndex,
    owner: &FunctionSummary,
    test: &TestSummary,
    refusal: &AssertionRefusal,
) -> bool {
    let Some((parent, line, declaration)) = refusal.module_declaration_site() else {
        return false;
    };
    let (Some(owner_context), Some(test_context)) = (
        index.execution_context(&owner.file),
        index.execution_context(&test.file),
    ) else {
        return false;
    };
    let owner_edges = owner_context
        .edges
        .iter()
        .map(edge_key)
        .collect::<BTreeSet<_>>();
    let children = owner_context
        .edges
        .iter()
        .map(|edge| edge.child.as_path())
        .collect::<BTreeSet<_>>();
    if owner_context.roots.len() != 1
        || children.len() != owner_context.edges.len()
        || !owner_edges.contains(&(parent, line, declaration))
    {
        return false;
    }
    let gate_of = |edge: &SourceRoleProvenanceEdge| {
        (edge.kind == SourceRoleProvenanceEdgeKind::Module)
            .then(|| index.files().get(&edge.parent))
            .flatten()
            .filter(|facts| !facts.used_lexical_fallback)
            .and_then(|facts| {
                crate::analysis::syntax::module_declaration_unestablished_gate(
                    &facts.source,
                    edge.line,
                )
            })
    };
    let chain_admitted = test_context.edges.iter().all(|edge| match gate_of(edge) {
        // Shared with the owner: any unevaluated gate binds both.
        Some(_) if owner_edges.contains(&edge_key(edge)) => true,
        // The test's own declaration must be on in every test build.
        Some(false) => true,
        _ => false,
    });
    let refused_is_unknown_gate = test_context
        .edges
        .iter()
        .find(|edge| edge_key(edge) == (parent, line, declaration))
        .and_then(gate_of)
        == Some(true);
    let in_file_gates = |file: &Path, name: &str, line: usize| {
        let facts = index.files().get(file)?;
        crate::analysis::syntax::unestablished_cfg_gates_of_fn(&facts.source, name, line)
    };
    let in_file_shared = match (
        in_file_gates(&test.file, &test.name, test.start_line),
        in_file_gates(&owner.file, &owner.name, owner.start_line),
    ) {
        (Some(test_gates), Some(owner_gates)) => test_gates.is_subset(&owner_gates),
        _ => false,
    };
    // The shared declaration explains only the provenance refusal: every
    // assertion's own in-file reading must be admitted, an analyzer limit, or
    // the enclosing gate the subset check above shares with the owner.
    let test_local_admitted = AssertionRefusal::test_local_refusals(test, index)
        .is_some_and(|refusals| {
            !refusals.is_empty()
                && refusals.iter().all(|refusal| {
                    refusal.as_ref().is_none_or(|refusal| {
                        refusal.is_analyzer_limit() || refusal.is_gated_item()
                    })
                })
        });
    refused_is_unknown_gate && chain_admitted && in_file_shared && test_local_admitted
}

/// A module edge's identity: declaring file, line and declaration text.
fn edge_key(edge: &SourceRoleProvenanceEdge) -> (&Path, usize, &str) {
    (edge.parent.as_path(), edge.line, edge.declaration.as_str())
}

pub(in crate::analysis) fn related_test_miss_for_assertion_refusal(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    refusal: &AssertionRefusal,
) -> RelatedTestMiss {
    if assertion_refusal_counts_as_analyzer_limit(context, test, refusal) {
        RelatedTestMiss::AssertionAdmissionUnproven
    } else {
        RelatedTestMiss::AssertionInert
    }
}

/// Order-independent: agreeing refusals keep their miss, any disagreement is
/// admission-unproven (commutative and associative over the two values).
fn combine_refusal_misses(
    prior: Option<RelatedTestMiss>,
    next: RelatedTestMiss,
) -> RelatedTestMiss {
    match prior {
        Some(prior) if prior != next => RelatedTestMiss::AssertionAdmissionUnproven,
        _ => next,
    }
}

pub(in crate::analysis) fn split_assertion_not_credited_misses(
    context: &ProbeContext<'_>,
    pin_syntax: &OwnerPinSyntax,
    related_tests: &mut [RelatedTest],
) {
    for row in related_tests.iter_mut() {
        if row.miss != Some(RelatedTestMiss::AssertionNotCredited) {
            continue;
        }
        let Some((test, _)) = context.related_tests.iter().find(|(test, _)| {
            test.name == row.name && test.file == row.file && test.start_line == row.line
        }) else {
            row.miss = Some(RelatedTestMiss::AssertionAdmissionUnproven);
            continue;
        };
        let probe = context.probe;
        // Reveal keeps no pairing between a row and the refused assertion, so
        // the row cannot name the responsible one. Read every refusal and keep
        // `AssertionInert` only when all of them agree; any disagreement or
        // unproven refusal is admission-unproven, independent of source order.
        let mut split: Option<RelatedTestMiss> = None;
        for assertion in &test.assertions {
            let Some(refusal) =
                pin_syntax.equality_assertion_refusal(
                    probe,
                    context.owner_fn,
                    test,
                    assertion,
                    context.index,
                )
            else {
                continue;
            };
            let miss = related_test_miss_for_assertion_refusal(context, test, &refusal);
            split = Some(combine_refusal_misses(split, miss));
        }
        row.miss = Some(split.unwrap_or(RelatedTestMiss::AssertionAdmissionUnproven));
    }
}

#[cfg(test)]
mod combine_tests {
    use super::combine_refusal_misses;
    use crate::domain::RelatedTestMiss::{AssertionAdmissionUnproven, AssertionInert};

    #[test]
    fn mixed_refusals_are_unproven_in_either_order() {
        let fold = |misses: &[_]| {
            misses
                .iter()
                .fold(None, |acc, miss| Some(combine_refusal_misses(acc, *miss)))
        };
        assert_eq!(
            fold(&[AssertionInert, AssertionAdmissionUnproven]),
            Some(AssertionAdmissionUnproven)
        );
        assert_eq!(
            fold(&[AssertionAdmissionUnproven, AssertionInert]),
            Some(AssertionAdmissionUnproven)
        );
        assert_eq!(
            fold(&[AssertionInert, AssertionInert]),
            Some(AssertionInert)
        );
    }
}
