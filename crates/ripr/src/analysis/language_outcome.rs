//! Full per-language analysis outcome (#6396, parent #5546).
//!
//! This first slice is repo mode only and carries only the statuses a repo
//! adapter run can produce today; the remaining closed states land with their
//! producers so no variant exists without a constructor.
//!
//! `LanguageRun` stays the public, non-success-only compatibility surface.
//! `LanguageAnalysisOutcomeV1` is the internal record of what one adapter did
//! for one mode, including successful languages, so later aggregation can
//! compute mixed-repository completeness without strengthening any child. The
//! compatibility `LanguageRun` is derived from it, never the reverse.
//!
//! Only producer-known facts are recorded. A count the producer did not measure
//! is `None`; it is never inferred, and no per-language CPU or memory is
//! attributed.

use crate::analysis::language::{LanguageRepoResult, RepoFileAccounting};
use crate::analysis::{LanguageRun, LanguageRunStatus};
use crate::domain::{Finding, LanguageId};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;

pub(crate) const LANGUAGE_ANALYSIS_OUTCOME_SCHEMA_VERSION: &str = "1";

/// Closed per-language status. `CompleteZero` means the adapter ran to its
/// declared boundary and found nothing; it never stands for an unavailable,
/// stubbed, failed or capped run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LanguageOutcomeStatus {
    Complete,
    CompleteZero,
    Partial,
}

impl LanguageOutcomeStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::CompleteZero => "complete_zero",
            Self::Partial => "partial",
        }
    }

    /// The public `LanguageRunStatus` this status projects to, or `None` when
    /// the compatibility surface stays silent (a successful run).
    const fn compat_status(self) -> Option<LanguageRunStatus> {
        match self {
            Self::Complete | Self::CompleteZero => None,
            Self::Partial => Some(LanguageRunStatus::Partial),
        }
    }
}

/// Facts the producer actually measured. `None` is "not measured". Named
/// bounds come only from the producer's typed `files.first_cap_fired`; a
/// disclosure string is never parsed and is not evidence that a bound fired.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LanguageResourceObservations {
    pub(crate) files_skipped: Option<usize>,
    pub(crate) production_files: Option<usize>,
    /// Typed file accounting and the first fired cap, exactly as the producer
    /// reported it. Per-language; never summed across languages.
    pub(crate) files: RepoFileAccounting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguageAnalysisOutcomeV1 {
    pub(crate) language: LanguageId,
    pub(crate) status: LanguageOutcomeStatus,
    /// Findings by finding-level language wire string, so a TypeScript-family
    /// run keeps TypeScript and JavaScript counts separate.
    pub(crate) findings_by_language: BTreeMap<String, usize>,
    pub(crate) resources: LanguageResourceObservations,
    pub(crate) limitations: Vec<String>,
    pub(crate) semantic_digest: String,
}

impl LanguageAnalysisOutcomeV1 {
    /// Builds the outcome of one repo-mode adapter run that returned `Ok`.
    ///
    /// `partial_reason` is the adapter's typed partial disclosure; callers that
    /// own an additional disclosure (Rust generated-file skips) pass it as
    /// `extra_partial_reason`.
    pub(crate) fn from_repo_result(
        language: LanguageId,
        result: &LanguageRepoResult,
        extra_partial_reason: Option<String>,
    ) -> Self {
        let mut limitations = Vec::new();
        limitations.extend(result.partial_reason.clone());
        limitations.extend(extra_partial_reason);
        let status = if !limitations.is_empty() {
            LanguageOutcomeStatus::Partial
        } else if result.findings.is_empty() {
            LanguageOutcomeStatus::CompleteZero
        } else {
            LanguageOutcomeStatus::Complete
        };
        let resources = LanguageResourceObservations {
            files_skipped: Some(result.skipped_files),
            production_files: Some(result.production_files),
            files: result.file_accounting,
        };
        Self::new(
            language,
            status,
            count_findings(&result.findings),
            resources,
            limitations,
        )
    }

    fn new(
        language: LanguageId,
        status: LanguageOutcomeStatus,
        findings_by_language: BTreeMap<String, usize>,
        resources: LanguageResourceObservations,
        limitations: Vec<String>,
    ) -> Self {
        let mut outcome = Self {
            language,
            status,
            findings_by_language,
            resources,
            limitations,
            semantic_digest: String::new(),
        };
        outcome.semantic_digest = outcome.compute_digest();
        outcome
    }

    #[cfg(test)]
    pub(crate) fn finding_count(&self) -> usize {
        self.findings_by_language.values().sum()
    }

    /// The public compatibility record, or `None` for a successful run.
    ///
    /// The reason joins the typed limitations; a non-success status without one
    /// still names its status so the record is never reasonless.
    pub(crate) fn compat_run(&self) -> Option<LanguageRun> {
        let status = self.status.compat_status()?;
        let reason = if self.limitations.is_empty() {
            self.status.as_str().to_string()
        } else {
            self.limitations.join(" | ")
        };
        Some(LanguageRun {
            language: self.language.as_str().to_string(),
            status,
            reason: Some(reason),
        })
    }

    /// Digest of the aggregate per-language state (status, per-language finding
    /// counts, limitations); it is not a snapshot or finding identity. Order-independent over limitations. Resource observations
    /// are included only through the counts that define identity, so telemetry
    /// alone cannot change finding identity.
    fn compute_digest(&self) -> String {
        let mut canonical = format!(
            "v{}|{}|repo|{}",
            LANGUAGE_ANALYSIS_OUTCOME_SCHEMA_VERSION,
            self.language.as_str(),
            self.status.as_str()
        );
        for (language, count) in &self.findings_by_language {
            let _ = write!(canonical, "|f:{language}={count}");
        }
        let mut limitations = self.limitations.clone();
        limitations.sort();
        for limitation in limitations {
            let _ = write!(canonical, "|l:{limitation}");
        }
        format!("{:x}", Sha256::digest(canonical.as_bytes()))
    }
}

fn count_findings(findings: &[Finding]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for finding in findings {
        let key = finding
            .language
            .map_or("unknown", |language| language.as_str());
        *counts.entry(key.to_string()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests;
