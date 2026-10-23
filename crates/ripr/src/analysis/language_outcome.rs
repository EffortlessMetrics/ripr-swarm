//! Full per-language analysis outcome (#6396, #6468; parent #5546).
//!
//! Carries only the statuses a producer can honestly construct today: repo
//! adapter runs, diff adapter runs and `Err`s, and the diff pipeline's own
//! unsupported / not-selected / no-current-scope dispositions. `cancelled`,
//! `timed_out` and `not_proven` land with a non-aborting producer so no variant
//! exists without a constructor.
//!
//! `LanguageRun` stays the public, non-success-only compatibility surface.
//! `LanguageAnalysisOutcomeV1` is the internal record of what one adapter did
//! for one mode, including successful languages, so aggregation can compute
//! mixed-language completeness without strengthening any child. The
//! compatibility `LanguageRun` is derived from it, never the reverse.
//!
//! Only producer-known facts are recorded. A count the producer did not measure
//! is `None`; it is never inferred, and no per-language CPU or memory is
//! attributed.

use crate::analysis::language::{
    LanguageDiffResult, LanguageRepoResult, PartialDiffScope, RepoFileAccounting,
};
use crate::analysis::{LanguageRun, LanguageRunStatus};
use crate::domain::{Finding, LanguageId};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;

mod diff_aggregate;
#[cfg(test)]
pub(crate) use diff_aggregate::DiffCompleteness;
pub(crate) use diff_aggregate::DiffLanguageAggregate;
pub(crate) use diff_aggregate::LanguageAggregate;
use diff_aggregate::repo_static_facts_limit;

pub(crate) const LANGUAGE_ANALYSIS_OUTCOME_SCHEMA_VERSION: &str = "1";

/// Which analysis mode a digest binds. Repo digests keep their original
/// spelling; diff digests additionally bind the selected diff scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutcomeMode {
    Repo,
    Diff,
}

impl OutcomeMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Repo => "repo",
            Self::Diff => "diff",
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

/// Closed per-language status. `CompleteZero` means the adapter ran to its
/// declared boundary and found nothing; it never stands for an unavailable,
/// stubbed, failed or capped run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LanguageOutcomeStatus {
    Complete,
    CompleteZero,
    Partial,
    /// The adapter could not run (no packet, feature off, read or parse error).
    Unavailable,
    /// The adapter ran on supplied evidence that failed an integrity check.
    Invalid,
    /// A configured producer ran and failed.
    Failed,
    /// Changed source in a language no ripr adapter reads.
    Unsupported,
    /// Changed files of a language the effective language set did not select
    /// (configuration-disabled, or its adapter is not compiled in).
    NotSelected,
    /// The adapter ran but the diff held none of its language: nothing in
    /// scope, which is not a `CompleteZero` boundary result.
    NoCurrentScope,
}

impl LanguageOutcomeStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::CompleteZero => "complete_zero",
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
            Self::Invalid => "invalid",
            Self::Failed => "failed",
            Self::Unsupported => "unsupported",
            Self::NotSelected => "not_selected",
            Self::NoCurrentScope => "no_current_scope",
        }
    }

    /// Whether this status lets an aggregate claim whole-diff completeness
    /// over the language: a boundary result, or nothing in scope.
    pub(crate) const fn is_complete_for_aggregate(self) -> bool {
        matches!(
            self,
            Self::Complete | Self::CompleteZero | Self::NoCurrentScope
        )
    }

    /// Whether the status may coexist with findings.
    const fn may_carry_findings(self) -> bool {
        matches!(self, Self::Complete | Self::Partial)
    }

    /// Maps a failed adapter run's public status back to an outcome status.
    /// An `Err` is never `ok`/`partial`; those arms are failures by
    /// construction so the mapping stays total without inventing success.
    const fn from_err_run_status(status: LanguageRunStatus) -> Self {
        match status {
            LanguageRunStatus::Unavailable => Self::Unavailable,
            LanguageRunStatus::Invalid => Self::Invalid,
            LanguageRunStatus::Failed | LanguageRunStatus::Partial | LanguageRunStatus::Ok => {
                Self::Failed
            }
        }
    }

    /// The public `LanguageRunStatus` this status projects to, or `None` when
    /// the compatibility surface stays silent (a successful run, or a
    /// disposition that already surfaces as a typed limitation).
    const fn compat_status(self) -> Option<LanguageRunStatus> {
        match self {
            Self::Complete | Self::CompleteZero => None,
            Self::Partial => Some(LanguageRunStatus::Partial),
            Self::Unavailable => Some(LanguageRunStatus::Unavailable),
            Self::Invalid => Some(LanguageRunStatus::Invalid),
            Self::Failed => Some(LanguageRunStatus::Failed),
            // These already surface as typed limitations; projecting them
            // would double-count in `limitations_from_language_runs`.
            Self::Unsupported | Self::NotSelected | Self::NoCurrentScope => None,
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
    /// Canonical language label. A `LanguageId` wire string for adapters, or
    /// the router's label for an unsupported language (`go`, `ruby`, ...).
    /// JavaScript is its own label, never folded into TypeScript.
    pub(crate) language: String,
    pub(crate) mode: OutcomeMode,
    /// Diff scope label the child ran over; `None` in repo mode.
    pub(crate) scope: Option<String>,
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
        } else if !result.findings.is_empty() {
            LanguageOutcomeStatus::Complete
        } else if result.production_files == 0 {
            // No production file was positively discovered, so "found
            // nothing" has no boundary behind it: an empty or stub run is
            // nothing in scope, never `CompleteZero` (#6470).
            LanguageOutcomeStatus::NoCurrentScope
        } else {
            LanguageOutcomeStatus::CompleteZero
        };
        // A zero-finding run over a repo capability cell that does not
        // establish capability reports that exact limit. `Unsupported` has no
        // compatibility row, so the public projection is unchanged.
        if result.findings.is_empty()
            && status != LanguageOutcomeStatus::Partial
            && let Some(limit) = repo_static_facts_limit(language)
        {
            limitations = vec![limit.to_string()];
            return Self::new(
                language.as_str().to_string(),
                OutcomeMode::Repo,
                None,
                LanguageOutcomeStatus::Unsupported,
                BTreeMap::new(),
                LanguageResourceObservations {
                    files_skipped: Some(result.skipped_files),
                    production_files: Some(result.production_files),
                    files: result.file_accounting,
                },
                limitations,
            );
        }
        let resources = LanguageResourceObservations {
            files_skipped: Some(result.skipped_files),
            production_files: Some(result.production_files),
            files: result.file_accounting,
        };
        Self::new(
            language.as_str().to_string(),
            OutcomeMode::Repo,
            None,
            status,
            count_findings(&result.findings),
            resources,
            limitations,
        )
    }

    /// Builds the outcome of a repo-mode adapter that returned `Err`. The
    /// reason is the single limitation, so the derived compatibility
    /// `LanguageRun` carries exactly the string the pipeline recorded before.
    pub(crate) fn from_repo_err(
        language: LanguageId,
        reason: String,
        run_status: LanguageRunStatus,
    ) -> Self {
        Self::new(
            language.as_str().to_string(),
            OutcomeMode::Repo,
            None,
            LanguageOutcomeStatus::from_err_run_status(run_status),
            BTreeMap::new(),
            LanguageResourceObservations::default(),
            vec![reason],
        )
    }

    /// A language present in the repository whose adapter is disabled or not
    /// compiled in. Already surfaced as a typed advisory, so no compatibility
    /// row.
    pub(crate) fn repo_not_selected(language: &str, files: usize, reason: &str) -> Self {
        Self::new(
            language.to_string(),
            OutcomeMode::Repo,
            None,
            LanguageOutcomeStatus::NotSelected,
            BTreeMap::new(),
            LanguageResourceObservations::default(),
            vec![format!("{files} file(s) not analyzed: {reason}")],
        )
    }

    /// Builds the outcome of one diff-mode adapter run that returned `Ok`.
    ///
    /// `kept_limitations` are the typed limitation kinds the pipeline kept for
    /// this adapter (its own filter, not recomputed here); `touched` holds the
    /// language labels the diff routed changed files to (`javascript` apart
    /// from `typescript`).
    ///
    /// Returns one child per language label of the adapter's family that is in
    /// play: the loop language, any family label with routed files, and any
    /// family label with findings. The TypeScript adapter's family is
    /// `{typescript, javascript}`; every other adapter's is itself. Each
    /// child holds only the findings whose `finding.language` is its label;
    /// findings outside the family (a bridge profile's Rust-line findings,
    /// or none) stay with the loop language's child and never create another
    /// language's key. When both TypeScript ids run, the same label is
    /// produced twice and `DiffLanguageAggregate::record` keeps one child, so
    /// counts are never summed twice. A child that touched nothing and found
    /// nothing is `NoCurrentScope`, never `CompleteZero`. Limitations belong
    /// to the adapter run and attach to every child it produced.
    pub(crate) fn from_diff_result(
        language: LanguageId,
        result: &LanguageDiffResult,
        kept_limitations: &[String],
        touched: &std::collections::BTreeSet<String>,
        scope: &str,
    ) -> Vec<Self> {
        let family: &[&str] = match language {
            LanguageId::TypeScript | LanguageId::JavaScript => &["typescript", "javascript"],
            other => &[other.as_str()],
        };
        let own = language.as_str();
        let label_of = |finding: &Finding| {
            finding
                .language
                .map(|found| found.as_str())
                .filter(|found| family.contains(found))
                .unwrap_or(own)
        };
        let mut limitations = kept_limitations.to_vec();
        if let Some(partial) = &result.partial_scope {
            limitations.push(format!("limited_partial_scope: {}", partial.run_status));
        }
        if result.skipped_files > 0 {
            limitations.push(format!("generated_files_skipped: {}", result.skipped_files));
        }
        let mut labels = std::collections::BTreeSet::from([own]);
        labels.extend(
            family
                .iter()
                .copied()
                .filter(|label| touched.contains(*label)),
        );
        labels.extend(result.findings.iter().map(label_of));
        labels
            .into_iter()
            .map(|label| {
                let own_findings = result
                    .findings
                    .iter()
                    .filter(|finding| label_of(finding) == label)
                    .cloned()
                    .collect::<Vec<_>>();
                let status = if !limitations.is_empty() {
                    LanguageOutcomeStatus::Partial
                } else if !own_findings.is_empty() {
                    LanguageOutcomeStatus::Complete
                } else if touched.contains(label) {
                    LanguageOutcomeStatus::CompleteZero
                } else {
                    LanguageOutcomeStatus::NoCurrentScope
                };
                let resources = LanguageResourceObservations {
                    files_skipped: Some(result.skipped_files),
                    ..LanguageResourceObservations::default()
                };
                Self::new(
                    label.to_string(),
                    OutcomeMode::Diff,
                    Some(scope.to_string()),
                    status,
                    count_findings(&own_findings),
                    resources,
                    limitations.clone(),
                )
            })
            .collect()
    }

    /// The same failure for an optional adapter whose language the diff did
    /// not touch (no files, no packet needed): the error stays visible in the
    /// limitation and digest, but there is no denominator to claim over, so it
    /// is `NoCurrentScope` and does not block the aggregate. The public
    /// `LanguageRun` is derived from the original `Err` outcome, not this one.
    pub(crate) fn without_scope(&self) -> Self {
        Self::new(
            self.language.clone(),
            self.mode,
            self.scope.clone(),
            LanguageOutcomeStatus::NoCurrentScope,
            self.findings_by_language.clone(),
            self.resources.clone(),
            self.limitations.clone(),
        )
    }

    /// Builds the outcome of a diff-mode adapter that returned `Err`. The
    /// adapter's reason is the single limitation, so the derived compatibility
    /// `LanguageRun` carries exactly that string.
    pub(crate) fn from_diff_err(
        language: LanguageId,
        reason: String,
        run_status: LanguageRunStatus,
        scope: &str,
    ) -> Self {
        Self::new(
            language.as_str().to_string(),
            OutcomeMode::Diff,
            Some(scope.to_string()),
            LanguageOutcomeStatus::from_err_run_status(run_status),
            BTreeMap::new(),
            LanguageResourceObservations::default(),
            vec![reason],
        )
    }

    /// A changed-source language no ripr adapter reads.
    pub(crate) fn unsupported(label: &str, changed_files: usize, scope: &str) -> Self {
        // The router spells labels for display (`Go`); the key is lowercase,
        // like the adapter languages' wire strings.
        Self::disposition(
            &label.to_ascii_lowercase(),
            LanguageOutcomeStatus::Unsupported,
            format!("{changed_files} changed file(s) in a language with no ripr adapter"),
            scope,
        )
    }

    /// Changed files of a language the effective language set did not select.
    pub(crate) fn not_selected(
        language: &str,
        changed_files: usize,
        reason: &str,
        scope: &str,
    ) -> Self {
        Self::disposition(
            language,
            LanguageOutcomeStatus::NotSelected,
            format!("{changed_files} changed file(s) not analyzed: {reason}"),
            scope,
        )
    }

    fn disposition(
        label: &str,
        status: LanguageOutcomeStatus,
        limitation: String,
        scope: &str,
    ) -> Self {
        Self::new(
            label.to_string(),
            OutcomeMode::Diff,
            Some(scope.to_string()),
            status,
            BTreeMap::new(),
            LanguageResourceObservations::default(),
            vec![limitation],
        )
    }

    fn new(
        language: String,
        mode: OutcomeMode,
        scope: Option<String>,
        status: LanguageOutcomeStatus,
        findings_by_language: BTreeMap<String, usize>,
        resources: LanguageResourceObservations,
        limitations: Vec<String>,
    ) -> Self {
        let mut outcome = Self {
            language,
            mode,
            scope,
            status,
            findings_by_language,
            resources,
            limitations,
            semantic_digest: String::new(),
        };
        outcome.semantic_digest = outcome.compute_digest();
        outcome
    }

    pub(crate) fn finding_count(&self) -> usize {
        self.findings_by_language.values().sum()
    }

    /// The public compatibility record, or `None` for a successful run or a
    /// disposition that already surfaces as a typed limitation.
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
            language: self.language.clone(),
            status,
            reason: Some(reason),
        })
    }

    /// Whether the recorded state contradicts itself: a count where the
    /// status allows none, or a boundary-complete claim that disagrees with
    /// its own findings or limitations.
    pub(crate) fn contradiction(&self) -> Option<String> {
        let findings = self.finding_count();
        let label = &self.language;
        let status = self.status.as_str();
        if findings > 0 && !self.status.may_carry_findings() {
            return Some(format!(
                "{label}: status {status} carries {findings} finding(s)"
            ));
        }
        if self.status == LanguageOutcomeStatus::Complete && findings == 0 {
            return Some(format!("{label}: status complete has no findings"));
        }
        if self.status == LanguageOutcomeStatus::CompleteZero && findings != 0 {
            return Some(format!(
                "{label}: status complete_zero carries {findings} finding(s)"
            ));
        }
        if matches!(
            self.status,
            LanguageOutcomeStatus::Complete | LanguageOutcomeStatus::CompleteZero
        ) && !self.limitations.is_empty()
        {
            return Some(format!("{label}: status {status} carries limitations"));
        }
        None
    }

    /// Digest of the aggregate per-language state (mode, bound diff scope,
    /// status, per-language finding counts, limitations); it is not a snapshot
    /// or finding identity. Order-independent over limitations. Resource
    /// observations are included only through the counts that define identity,
    /// so telemetry alone cannot change finding identity. Repo digests keep
    /// their original spelling.
    fn compute_digest(&self) -> String {
        let mut canonical = format!(
            "v{}|{}|{}|{}",
            LANGUAGE_ANALYSIS_OUTCOME_SCHEMA_VERSION,
            self.language,
            self.mode.as_str(),
            self.status.as_str()
        );
        if let Some(scope) = &self.scope {
            let _ = write!(canonical, "|s:{scope}");
        }
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
mod diff_tests;
#[cfg(test)]
mod repo_tests;
#[cfg(test)]
mod tests;
