//! Cross-layer consistency of the per-language aggregate (#6473).
//!
//! A pure, deterministic check that the internal aggregate, the retained
//! findings and (diff mode) the top-level `AnalysisOutcome` do not contradict
//! each other. It reads no rendering and changes nothing; the pipelines call it
//! next to the aggregate's own `validate`, as an internal invariant.
//!
//! Deliberately not enforced here: a `not_selected` child with a complete
//! `AnalysisOutcome.kind`. An adapter that is not compiled in is recorded as an
//! aggregate-only disposition by design (#6468), with no top-level limitation.

use super::{LanguageAggregate, LanguageOutcomeStatus, OutcomeMode};
use crate::analysis_outcome::{AnalysisLimitationKind, AnalysisOutcome};
use crate::domain::{Finding, LanguageId};

/// Statuses whose denominator the run could not establish and which the diff
/// pipeline always pairs with a typed top-level limitation.
const fn is_hard_blocker(status: LanguageOutcomeStatus) -> bool {
    matches!(
        status,
        LanguageOutcomeStatus::Partial
            | LanguageOutcomeStatus::Unavailable
            | LanguageOutcomeStatus::Invalid
            | LanguageOutcomeStatus::Failed
            | LanguageOutcomeStatus::Unsupported
    )
}

pub(crate) fn reconcile(
    aggregate: &LanguageAggregate,
    selected: &[LanguageId],
    findings: &[Finding],
    outcome: Option<&AnalysisOutcome>,
) -> Result<(), String> {
    aggregate.validate()?;
    aggregate.validate_selected(selected)?;

    let mut child_findings: usize = aggregate
        .outcomes
        .values()
        .map(|child| child.finding_count())
        .sum();
    // When both TypeScript ids are selected the one adapter runs per id and
    // the aggregate keeps one child per label (#6468). Repo mode retains the
    // adapter's findings once, while each label's child carries the adapter's
    // whole count, so the family counts once there (#10013). Both passes see
    // the same result, so the two children agree; the only legitimate
    // difference is a pass that errored (a zero-finding failed child). Two
    // different non-zero counts are a contradiction. Diff mode attributes
    // findings to a label by the finding's own language, so its children
    // already sum to the retained findings.
    if selected.contains(&LanguageId::TypeScript)
        && selected.contains(&LanguageId::JavaScript)
        && aggregate.mode == OutcomeMode::Repo
    {
        let [typescript, javascript] = ["typescript", "javascript"].map(|label| {
            aggregate
                .outcomes
                .get(label)
                .map_or(0, |child| child.finding_count())
        });
        if typescript != 0 && javascript != 0 && typescript != javascript {
            return Err(format!(
                "language reconcile: the shared TypeScript adapter family carries {typescript} and {javascript} finding(s) in its two children"
            ));
        }
        child_findings -= typescript.min(javascript);
    }
    let counts_agree = findings.len() == child_findings;
    if !counts_agree {
        return Err(format!(
            "language reconcile: children carry {child_findings} finding(s), retained findings are {}",
            findings.len()
        ));
    }

    let Some(outcome) = outcome else {
        return Ok(());
    };
    if outcome.counts.finding_count != findings.len() as u64 {
        return Err(format!(
            "language reconcile: outcome counts {} finding(s), retained findings are {}",
            outcome.counts.finding_count,
            findings.len()
        ));
    }
    if let Some((label, child)) = aggregate
        .outcomes
        .iter()
        .find(|(_, child)| is_hard_blocker(child.status))
        && outcome.kind.is_complete()
    {
        return Err(format!(
            "language reconcile: {label} is {} but outcome kind is {}",
            child.status.as_str(),
            outcome.kind.as_str()
        ));
    }
    let unsupported = aggregate
        .outcomes
        .values()
        .any(|child| child.status == LanguageOutcomeStatus::Unsupported);
    if unsupported
        && !outcome
            .limitations
            .iter()
            .any(|limitation| limitation.kind == AnalysisLimitationKind::LanguageScopeUnsupported)
    {
        return Err(
            "language reconcile: unsupported changed source has no top-level limitation"
                .to_string(),
        );
    }
    Ok(())
}
