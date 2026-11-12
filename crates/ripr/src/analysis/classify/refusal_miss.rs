//! Split `assertion_not_credited` into inert vs admission-unproven (#5446).

use super::owner_pin::{AssertionRefusal, OwnerPinSyntax};
use crate::analysis::classify::{ProbeContext, direct_call_name};
use crate::analysis::facts::SourceRoleProvenanceEdgeKind;
use crate::analysis::rust_index::TestSummary;
use crate::domain::{RelatedTest, RelatedTestMiss};
use std::path::Path;

pub(in crate::analysis) fn assertion_refusal_counts_as_analyzer_limit(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    refusal: &AssertionRefusal,
) -> bool {
    let owner_name = context.owner_fn.map_or("", |owner| owner.name.as_str());
    (refusal.is_analyzer_limit()
        || (refusal.is_gated_item() && test_shares_owner_cfg_gates(context, test))
        || owner_compiled_only_under_refused_declaration(context, refusal))
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
/// module. The declaration must carry only `cfg` gates ripr cannot evaluate
/// (a provably-off gate such as `cfg(any())` keeps the gap, as it does for the
/// inline form), and the owner's module ancestry must be one linear chain, so
/// no other path can compile the owner without the gated declaration.
fn owner_compiled_only_under_refused_declaration(
    context: &ProbeContext<'_>,
    refusal: &AssertionRefusal,
) -> bool {
    let (Some((parent, line, declaration)), Some(owner)) =
        (refusal.module_declaration_site(), context.owner_fn)
    else {
        return false;
    };
    let Some(owner_context) = context.index.execution_context(&owner.file) else {
        return false;
    };
    let children = owner_context
        .edges
        .iter()
        .map(|edge| edge.child.as_path())
        .collect::<std::collections::BTreeSet<_>>();
    let declared_with_unknown_gate = context.index.files().get(parent).is_some_and(|facts| {
        !facts.used_lexical_fallback
            && crate::analysis::syntax::module_declaration_has_only_unestablished_cfg_gates(
                &facts.source,
                line,
            )
    });
    declared_with_unknown_gate
        && owner_context.roots.len() == 1
        && children.len() == owner_context.edges.len()
        && owner_context.edges.iter().any(|edge| {
            edge.kind == SourceRoleProvenanceEdgeKind::Module
                && edge.parent == parent
                && edge.line == line
                && edge.declaration == declaration
        })
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
                pin_syntax.equality_assertion_refusal(probe, test, assertion, context.index)
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
