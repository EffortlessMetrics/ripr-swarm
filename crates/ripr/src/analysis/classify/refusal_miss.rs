//! Split `assertion_not_credited` into inert vs admission-unproven (#5446).

use super::owner_pin::{AssertionRefusal, OwnerPinSyntax};
use crate::analysis::classify::{ProbeContext, direct_call_name};
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
        || (refusal.is_gated_item() && test_shares_owner_cfg_gates(context, test)))
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
