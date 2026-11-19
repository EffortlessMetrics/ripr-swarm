//! Compatibility facade for the language-outcome contract (#12726 sub-slice 2b).
//! Decisions live in ripr-contract. These adapters only borrow the same fields
//! the former constructors/checker read, leaving producer records intact.

use crate::analysis::language::{LanguageDiffResult, LanguageRepoResult, PartialDiffScope};
#[cfg(test)]
use crate::analysis::{LanguageRun, LanguageRunStatus};
use crate::analysis_outcome::{AnalysisLimitationKind, AnalysisOutcome};
use crate::domain::{Finding, LanguageId};
use ripr_contract::analysis::language_outcome::{
    DiffOutcomeInput, OutcomeReconciliationInput, RepoOutcomeInput,
};

#[cfg(test)]
mod diff_aggregate {
    pub(super) use ripr_contract::analysis::language_outcome::DiffCompleteness;
}
#[cfg(test)]
use crate::analysis::language::RepoFileAccounting;
#[cfg(test)]
pub(crate) use ripr_contract::analysis::language_outcome::{
    DiffCompleteness, LanguageOutcomeStatus, LanguageResourceObservations, OutcomeMode,
};
pub(crate) use ripr_contract::analysis::language_outcome::{
    DiffLanguageAggregate, LanguageAggregate, LanguageAnalysisOutcomeV1, LanguageCompletenessView,
    LanguageDisclosure,
};
#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::BTreeMap;

#[cfg(test)]
pub(crate) use crate::analysis::language::{
    LanguageDiffResult as FixtureDiffResult, LanguageRepoResult as FixtureRepoResult,
};

impl<'a> From<&'a LanguageRepoResult> for RepoOutcomeInput<'a> {
    fn from(result: &'a LanguageRepoResult) -> Self {
        Self {
            findings: &result.findings,
            production_files: result.production_files,
            skipped_files: result.skipped_files,
            partial_reason: result.partial_reason.as_deref(),
            file_accounting: result.file_accounting,
        }
    }
}

impl<'a> From<&'a LanguageDiffResult> for DiffOutcomeInput<'a> {
    fn from(result: &'a LanguageDiffResult) -> Self {
        Self {
            findings: &result.findings,
            skipped_files: result.skipped_files,
            partial_run_status: result
                .partial_scope
                .as_ref()
                .map(|scope| scope.run_status.as_str()),
        }
    }
}

/// Stable label of the diff scope an adapter ran over: the full diff, or the
/// deterministic partition the Rust adapter selected.
pub(crate) fn diff_scope_label(partial: Option<&PartialDiffScope>) -> String {
    partial.map_or_else(
        || "full".to_string(),
        |scope| format!("partial:{}", scope.diff_identity),
    )
}

pub(crate) fn reconcile(
    aggregate: &LanguageAggregate,
    selected: &[LanguageId],
    findings: &[Finding],
    outcome: Option<&AnalysisOutcome>,
) -> Result<(), String> {
    ripr_contract::analysis::language_outcome::reconcile(
        aggregate,
        selected,
        findings,
        outcome.map(|outcome| OutcomeReconciliationInput {
            finding_count: outcome.counts.finding_count,
            kind: outcome.kind.as_str(),
            complete: outcome.kind.is_complete(),
            has_unsupported_limitation: outcome.limitations.iter().any(|limitation| {
                limitation.kind == AnalysisLimitationKind::LanguageScopeUnsupported
            }),
        }),
    )
}

#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod repo_tests;
#[cfg(test)]
mod tests;
