//! The diff-mode whole-test contradiction (#10980), moved beside its
//! parent so the candidate fold is read before the baseline body is built
//! (#11738).

use super::{
    ExactValueVerdict, FunctionSummary, RustIndex, TestSummary, assertion_verdict_for_test,
};

/// The diff-mode whole-test contradiction (#10980). A related test carries
/// the diff surface's credit only when it passes SOMEWHERE: it is withheld
/// when it fails on BOTH versions — some assertion row contradicts the
/// candidate body's fold and some row contradicts the baseline body — so
/// the test passes on neither version and cannot notice the change. A row
/// written for the baseline that the candidate breaks is the discriminator
/// itself (the exact-boundary witness of the agentic b4 journey), and a
/// row the candidate satisfies is the change's own credit; neither
/// withholds. The baseline body is the owner's current body with the
/// probe's `after` expression replaced by `before` exactly once (the
/// #8659/#10916 reconstruction); a probe without a baseline expression or
/// a body that does not reconstruct keeps the test's credit — the fold
/// can only withhold. The repo-seam rule in the parent module stays
/// current-fold-only:
/// there the current body is the shipped code and a contradicted row is
/// a wrong-way discriminator, while on the diff surface the current body
/// is the candidate under review.
///
/// A predicate probe in a complete workspace also reads each row's boolean
/// projection on both versions, so that path builds the baseline body up
/// front. Every other probe takes the fold-only reading below.
pub(in crate::analysis) fn test_fails_on_both_diff_versions(
    test: &TestSummary,
    owner: &FunctionSummary,
    probe: &crate::domain::Probe,
    pin_syntax: &crate::analysis::classify::OwnerPinSyntax,
    workspace_complete: bool,
    index: &RustIndex,
) -> bool {
    let (before, after) = (probe.before.as_deref(), probe.after.as_deref());
    if !workspace_complete || !matches!(probe.family, crate::domain::ProbeFamily::Predicate) {
        return fold_fails_on_both_diff_versions(test, owner, before, after, index);
    }
    let Some(baseline_owner) = baseline_owner(owner, before, after) else {
        return false;
    };
    let mut fails_on_candidate = false;
    let mut fails_on_base = false;
    for oracle in &test.assertions {
        if let Some((before, after)) =
            crate::analysis::classify::boolean_projection_matches_versions(
                owner,
                &baseline_owner,
                test,
                oracle,
                index,
                pin_syntax,
            )
        {
            fails_on_base |= !before;
            fails_on_candidate |= !after;
        }
        if !fails_on_candidate
            && matches!(
                assertion_verdict_for_test(Some(owner), &oracle.text, test, index),
                ExactValueVerdict::Contradicted { .. }
            )
        {
            fails_on_candidate = true;
        }
        if !fails_on_base
            && matches!(
                assertion_verdict_for_test(Some(&baseline_owner), &oracle.text, test, index),
                ExactValueVerdict::Contradicted { .. }
            )
        {
            fails_on_base = true;
        }
        if fails_on_candidate && fails_on_base {
            return true;
        }
    }
    false
}

/// Reconstruct an owner only when the changed expression occurs exactly once.
pub(in crate::analysis) fn baseline_owner(
    owner: &FunctionSummary,
    before: Option<&str>,
    after: Option<&str>,
) -> Option<FunctionSummary> {
    let (Some(before), Some(after)) = (before, after) else {
        return None;
    };
    if before == after || owner.body.matches(after).count() != 1 {
        return None;
    }
    let mut baseline_owner = owner.clone();
    baseline_owner.body =
        crate::analysis::facts::SourceText::from(owner.body.as_str().replacen(after, before, 1));
    Some(baseline_owner)
}

/// The fold-only reading. The candidate rows are read first and the baseline
/// body is built and folded only when one of them contradicts the candidate
/// (#11738). The answer is the same as reading both versions row by row: it
/// is `true` exactly when some row contradicts each version, and every fold
/// is pure. A test that passes on the candidate, the usual case, does not pay
/// for a copy and a fresh read of the whole baseline body; that body differs
/// for every changed line, so no memo could share it.
fn fold_fails_on_both_diff_versions(
    test: &TestSummary,
    owner: &FunctionSummary,
    before: Option<&str>,
    after: Option<&str>,
    index: &RustIndex,
) -> bool {
    let (Some(before), Some(after)) = (before, after) else {
        return false;
    };
    if before == after || owner.body.matches(after).count() != 1 {
        return false;
    }
    let contradicts = |owner: &FunctionSummary| {
        test.assertions.iter().any(|oracle| {
            matches!(
                assertion_verdict_for_test(Some(owner), &oracle.text, test, index),
                ExactValueVerdict::Contradicted { .. }
            )
        })
    };
    if !contradicts(owner) {
        return false;
    }
    let mut baseline_owner = owner.clone();
    baseline_owner.body =
        crate::analysis::facts::SourceText::from(owner.body.as_str().replacen(after, before, 1));
    contradicts(&baseline_owner)
}

#[cfg(test)]
mod tests;
