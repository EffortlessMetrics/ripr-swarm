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

/// #9981 forwarding residual: a row read as `no_assertion` whose test calls a
/// same-file helper that asserts after forwarding the test's closure to a call
/// ripr cannot resolve (tracing's `with_default(subscriber, fun)`) does have
/// an assertion; whether it observes the closure is what is unknown. Such a
/// row becomes the analyzer-limit miss `assertion_admission_unproven`
/// (RIPR-SPEC-0221) and the finding names the helper and callee. It grants no
/// oracle, strength or class, so no class can rise from it; only the false
/// absence claim (and its "add an assertion" repair) is withdrawn.
///
/// [`ForwardedCallbackLimits::only_limits`] is true when at least one row was
/// rewritten, no `no_assertion` row is left, no row carries an oracle, and
/// every oracle-crediting related test, including any the display window
/// omits, is a forwarded-limit test with no assertion of its own: the
/// only assertions the related tests hold are the forwarded helpers'. Only
/// then may the caller replace the "no assertion" stage summaries with the
/// existing refused-context ones (RIPR-SPEC-0221), so the finding-level next
/// step stops prescribing an assertion the test already has.
pub(in crate::analysis) fn name_forwarded_callback_limits(
    context: &ProbeContext<'_>,
    related_tests: &mut [RelatedTest],
) -> ForwardedCallbackLimits {
    let mut named = ForwardedCallbackLimits::default();
    // Recorded by this producer loop as it rewrites, so the `only_limits`
    // decision below reads these facts rather than re-reading the miss.
    let mut rows_clear = true;
    for row in related_tests.iter_mut() {
        if row.oracle.is_some() {
            rows_clear = false;
            continue;
        }
        if row.miss != Some(RelatedTestMiss::NoAssertion) {
            continue;
        }
        let Some(limit) = context
            .index
            .forwarded_callback_limit(&row.file, row.line, &row.name)
        else {
            rows_clear = false;
            continue;
        };
        row.miss = Some(RelatedTestMiss::AssertionAdmissionUnproven);
        let line = format!(
            "{FORWARDED_CALLBACK_PREFIX}test `{}` calls helper `{}`, which asserts after forwarding the closure to `{}`; whether it runs the closure is not established statically",
            row.name, limit.helper, limit.callee
        );
        if !named.evidence.contains(&line) {
            named.evidence.push(line);
        }
    }
    // The rows are the capped display window; the stage summaries read every
    // related test. So the decision also reads every related test: each one
    // whose relation may credit an oracle must be a forwarded-limit test with
    // no assertion of its own. A hidden assertion-free or oracle-bearing test
    // anywhere keeps the finding's no-assertion wording.
    let credits = super::oracle_crediting_relations(&context.related_tests);
    named.only_limits = !named.evidence.is_empty()
        && rows_clear
        && context
            .related_tests
            .iter()
            .filter(|(_, reason)| credits(*reason))
            .all(|(test, _)| {
                test.assertions.is_empty()
                    && context
                        .index
                        .forwarded_callback_limit(&test.file, test.start_line, &test.name)
                        .is_some()
            });
    named
}

/// What [`name_forwarded_callback_limits`] rewrote.
#[derive(Default)]
pub(in crate::analysis) struct ForwardedCallbackLimits {
    /// One evidence line per rewritten test, naming the helper and callee.
    pub(in crate::analysis) evidence: Vec<String>,
    /// Every assertion the related tests hold is a forwarded helper's.
    pub(in crate::analysis) only_limits: bool,
}

/// Evidence prefix of a [`name_forwarded_callback_limits`] line.
pub(in crate::analysis) const FORWARDED_CALLBACK_PREFIX: &str = "closure forwarded by helper: ";

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
