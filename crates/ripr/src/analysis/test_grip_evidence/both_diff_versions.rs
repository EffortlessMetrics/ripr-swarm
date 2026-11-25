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
/// The candidate rows are read first and the baseline body is built and
/// folded only when one of them contradicts the candidate (#11738). The
/// answer is the same as reading both versions row by row: it is `true`
/// exactly when some row contradicts each version, and every fold is pure.
/// A test that passes on the candidate, the usual case, no longer pays for
/// a copy and a fresh read of the whole baseline body; that body differs
/// for every changed line, so no memo could share it, and one long function
/// with many changed lines paid its length once per changed line and test.
pub(in crate::analysis) fn test_fails_on_both_diff_versions(
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
