//! Versioned producer-owned analysis completeness and limitation contract.
//!
//! This module is internal until the parser and public output projections have
//! adopted the contract. Renderers must consume these typed facts rather than
//! infer completeness from empty findings or `probe_count == 0`. (#2827)

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

pub(crate) const ANALYSIS_OUTCOME_SCHEMA_VERSION: &str = "0.1";
pub(crate) const ANALYSIS_OUTCOME_CLAIM_BOUNDARY: &str = "Static analysis outcome only; no correctness, test-adequacy, runtime-execution, or merge-readiness claim.";
pub(crate) const MAX_ANALYSIS_LIMITATION_DETAIL_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnalysisOutcomeKind {
    NoScope,
    NoChangedLines,
    NoBehavioralCandidates,
    CompleteNoFindings,
    CompleteWithFindings,
    PartialWithLimitations,
    UnsupportedInput,
    AnalysisFailed,
}

impl AnalysisOutcomeKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NoScope => "no_scope",
            Self::NoChangedLines => "no_changed_lines",
            Self::NoBehavioralCandidates => "no_behavioral_candidates",
            Self::CompleteNoFindings => "complete_no_findings",
            Self::CompleteWithFindings => "complete_with_findings",
            Self::PartialWithLimitations => "partial_with_limitations",
            Self::UnsupportedInput => "unsupported_input",
            Self::AnalysisFailed => "analysis_failed",
        }
    }

    /// What the outcome means, in words, for the human `check` header. The
    /// schema value (`as_str`) is printed beside it.
    pub(crate) const fn plain_label(self) -> &'static str {
        match self {
            Self::NoScope => "nothing was in scope",
            Self::NoChangedLines => "the diff has no changed lines",
            Self::NoBehavioralCandidates => "no changed line is behavior ripr checks",
            Self::CompleteNoFindings => "no findings",
            Self::CompleteWithFindings => "findings below",
            Self::PartialWithLimitations => "limitations below",
            Self::UnsupportedInput => "the input is not supported",
            Self::AnalysisFailed => "the analysis failed",
        }
    }

    pub(crate) const fn is_complete(self) -> bool {
        matches!(
            self,
            Self::NoScope
                | Self::NoChangedLines
                | Self::NoBehavioralCandidates
                | Self::CompleteNoFindings
                | Self::CompleteWithFindings
        )
    }
}

impl AnalysisLimitationKind {
    /// A disclosure about the shape of the change that leaves the analysis
    /// complete: the full changed scope was read (`EolOnlyChurn`,
    /// `VerificationOnlyChange`). Every other kind marks the outcome
    /// incomplete.
    pub(crate) const fn is_advisory_disclosure(self) -> bool {
        matches!(self, Self::EolOnlyChurn | Self::VerificationOnlyChange)
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::CombinedHunkUnsupported => "combined_hunk_unsupported",
            Self::UnresolvedConflictMarkers => "unresolved_conflict_markers",
            Self::MalformedDiff => "malformed_diff",
            Self::DiffScopeOversized => "diff_scope_oversized",
            Self::LanguageAdapterUnavailable => "language_adapter_unavailable",
            Self::LanguageScopeUnsupported => "language_scope_unsupported",
            Self::ChangedFileAbsentFromWorktree => "changed_file_absent_from_worktree",
            Self::ProducerTimeout => "producer_timeout",
            Self::ProducerFailure => "producer_failure",
            Self::EolOnlyChurn => "eol_only_churn",
            Self::VerificationOnlyChange => "verification_only_change",
            Self::UnevaluatedCondition => "unevaluated_condition",
        }
    }

    /// Plain words for human output; `as_str` stays the schema token.
    pub(crate) const fn plain_label(self) -> &'static str {
        match self {
            Self::CombinedHunkUnsupported => "a combined (merge) diff hunk is not supported",
            Self::UnresolvedConflictMarkers => "the diff has unresolved conflict markers",
            Self::MalformedDiff => "the diff could not be parsed",
            Self::DiffScopeOversized => "the diff is larger than the configured limit",
            Self::LanguageAdapterUnavailable => "no analyzer is available for a changed language",
            Self::LanguageScopeUnsupported => "some changed files were not analyzed",
            Self::ChangedFileAbsentFromWorktree => "a changed file is absent from the working tree",
            Self::ProducerTimeout => "the analysis ran out of time",
            Self::ProducerFailure => "part of the analysis failed",
            Self::EolOnlyChurn => "some files changed only in line endings",
            Self::VerificationOnlyChange => {
                "verifier-only contract or proof text changed; ripr does not evaluate it"
            }
            Self::UnevaluatedCondition => {
                "a test or credited assertion sits under a condition static analysis cannot evaluate"
            }
        }
    }
}

impl AnalysisStage {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::DiffLoad => "diff_load",
            Self::DiffParse => "diff_parse",
            Self::LanguageAdapter => "language_adapter",
            Self::ProbeGeneration => "probe_generation",
            Self::FindingClassification => "finding_classification",
            Self::AnalysisPipeline => "analysis_pipeline",
        }
    }

    /// Plain words for human output; `as_str` stays the schema token.
    pub(crate) const fn plain_label(self) -> &'static str {
        match self {
            Self::DiffLoad => "reading the diff",
            Self::DiffParse => "parsing the diff",
            Self::LanguageAdapter => "language analysis",
            Self::ProbeGeneration => "change point detection",
            Self::FindingClassification => "finding classification",
            Self::AnalysisPipeline => "the analysis pipeline",
        }
    }
}

impl AnalysisRecoveryKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NarrowDiff => "narrow_diff",
            Self::UseTwoWayDiff => "use_two_way_diff",
            Self::ResolveConflicts => "resolve_conflicts",
            Self::EnableLanguage => "enable_language",
            Self::IncreaseConfiguredLimit => "increase_configured_limit",
            Self::Retry => "retry",
            Self::InspectFailure => "inspect_failure",
            Self::DeclareHandwrittenFiles => "declare_handwritten_files",
            Self::DeclarePreviewExcludedPatterns => "declare_preview_excluded_patterns",
            Self::RenameGeneratedMarkedSource => "rename_generated_marked_source",
        }
    }

    /// Plain words for human output; `as_str` stays the schema token.
    pub(crate) const fn plain_label(self) -> &'static str {
        match self {
            Self::NarrowDiff => "narrow the diff",
            Self::UseTwoWayDiff => "use a two-way diff",
            Self::ResolveConflicts => "resolve the conflicts",
            Self::EnableLanguage => "enable the language",
            Self::IncreaseConfiguredLimit => "raise the configured limit",
            Self::Retry => "retry",
            Self::InspectFailure => "inspect the failure",
            Self::DeclareHandwrittenFiles => "declare hand-written files",
            Self::DeclarePreviewExcludedPatterns => "declare preview exclusion patterns",
            Self::RenameGeneratedMarkedSource => "rename the generated-marked source",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnalysisStage {
    DiffLoad,
    DiffParse,
    LanguageAdapter,
    ProbeGeneration,
    FindingClassification,
    AnalysisPipeline,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnalysisLimitationKind {
    CombinedHunkUnsupported,
    UnresolvedConflictMarkers,
    MalformedDiff,
    DiffScopeOversized,
    LanguageAdapterUnavailable,
    LanguageScopeUnsupported,
    ChangedFileAbsentFromWorktree,
    ProducerTimeout,
    ProducerFailure,
    /// #4952: a file's changed lines pair identical before/after text at the
    /// same positions, so the churn is line-ending-only. This is a
    /// disclosure about the churn shape, not an incomplete analysis — the
    /// full changed scope was read and probed — so unlike the kinds above it
    /// may ride on a complete outcome.
    EolOnlyChurn,
    /// RIPR-SPEC-0368: changed lines are verification-erased text (contract
    /// attributes such as `#[requires]`/`#[ensures]`, Creusot/Prusti ghost
    /// items and proof macros). No test can reach or observe them, so they
    /// seed no probe and no repair start. Like `EolOnlyChurn` this is a
    /// disclosure about what the changed lines are, not an incomplete
    /// analysis, so it may ride on a complete outcome.
    VerificationOnlyChange,
    /// #8714: a credited assertion sits under a branch condition static
    /// analysis cannot evaluate, so the credited oracle may never execute.
    /// The limitation content discloses conditional credit rather than
    /// unanalyzed files (#10438 reuses it for a test registered under an
    /// unevaluated `if`, which is indexed for reach but not credited), but unlike `EolOnlyChurn` this kind still marks the
    /// outcome incomplete: consumers gating on `analysis_complete` see these
    /// runs as partial (#8714 review). Letting it ride a complete outcome is
    /// a separate, consumer-visible design decision.
    UnevaluatedCondition,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnalysisRecoveryKind {
    NarrowDiff,
    UseTwoWayDiff,
    ResolveConflicts,
    EnableLanguage,
    IncreaseConfiguredLimit,
    Retry,
    InspectFailure,
    /// Admit an exact hand-written path past the built-in excluded-directory
    /// conventions through the language's `handwritten_files` list.
    DeclareHandwrittenFiles,
    /// Omit fixture or data paths from preview indexing through
    /// `excluded_file_patterns` (#5380).
    DeclarePreviewExcludedPatterns,
    /// A file name that marks generated code stays excluded even when its
    /// exact path is declared; the only route is renaming the file.
    RenameGeneratedMarkedSource,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct AnalysisIdentity {
    /// Stable repository identity or commitment. Absolute checkout paths do
    /// not belong in this portable identity field.
    pub(crate) repository_identity: Option<String>,
    /// Stable root identity or commitment. Absolute checkout paths do not
    /// belong in this portable identity field.
    pub(crate) root_identity: Option<String>,
    pub(crate) config_identity: Option<String>,
    pub(crate) base_revision: Option<String>,
    pub(crate) input_identity: Option<String>,
    pub(crate) snapshot_identity: Option<String>,
    /// #3237/#3278: the immutable Git candidate subject this run
    /// analyzed, when the input was a typed subject. Additive; ordinary
    /// inputs leave it absent.
    pub(crate) git_candidate_subject: Option<GitCandidateSubjectIdentity>,
}

/// Machine-visible identity of an analyzed immutable Git candidate
/// (#3278 R3). Every field binds directly to the R2 producer's resolved
/// state — never inferred from command construction.
#[derive(Clone, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct GitCandidateSubjectIdentity {
    /// `subject_kind`: `tree_to_tree`.
    pub(crate) subject_kind: String,
    /// The resolved base tree object ID (the empty tree carries the
    /// repository's real per-format empty-tree OID).
    pub(crate) base_tree: String,
    /// The exact candidate tree object ID whose blobs were analyzed.
    pub(crate) candidate_tree: String,
    /// SHA-256 of the derived base→candidate unified diff.
    pub(crate) diff_identity: String,
}

impl AnalysisIdentity {
    fn normalize(&mut self) -> Result<(), String> {
        for (field, value) in [
            ("repository_identity", &mut self.repository_identity),
            ("root_identity", &mut self.root_identity),
            ("config_identity", &mut self.config_identity),
            ("base_revision", &mut self.base_revision),
            ("input_identity", &mut self.input_identity),
            ("snapshot_identity", &mut self.snapshot_identity),
        ] {
            normalize_optional_bounded_text(field, value)?;
        }
        if let Some(subject) = self.git_candidate_subject.as_mut() {
            for (field, value) in [
                ("subject_kind", &mut Some(subject.subject_kind.clone())),
                ("base_tree", &mut Some(subject.base_tree.clone())),
                ("candidate_tree", &mut Some(subject.candidate_tree.clone())),
                ("diff_identity", &mut Some(subject.diff_identity.clone())),
            ] {
                normalize_optional_bounded_text(field, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct AnalysisOutcomeCounts {
    pub(crate) changed_file_count: u64,
    pub(crate) changed_line_count: u64,
    pub(crate) candidate_line_count: u64,
    pub(crate) probe_count: u64,
    pub(crate) finding_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct AnalysisRecovery {
    pub(crate) kind: AnalysisRecoveryKind,
    pub(crate) detail: String,
}

impl AnalysisRecovery {
    pub(crate) fn new(
        kind: AnalysisRecoveryKind,
        detail: impl Into<String>,
    ) -> Result<Self, String> {
        let detail = bounded_nonempty_text("analysis recovery detail", detail.into())?;
        Ok(Self { kind, detail })
    }

    fn normalize(&mut self) -> Result<(), String> {
        self.detail = bounded_nonempty_text("analysis recovery detail", self.detail.clone())?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct AnalysisLimitation {
    pub(crate) kind: AnalysisLimitationKind,
    pub(crate) producer_stage: AnalysisStage,
    /// Repository-relative portable path. Absolute paths and parent traversal
    /// are rejected before serialization. Whitespace belongs to the filename
    /// and is preserved, including at the beginning or end.
    pub(crate) path: Option<String>,
    pub(crate) affected_items: Option<u64>,
    pub(crate) bounded_detail: Option<String>,
    pub(crate) recovery: AnalysisRecovery,
}

impl AnalysisLimitation {
    pub(crate) fn new(
        kind: AnalysisLimitationKind,
        producer_stage: AnalysisStage,
        recovery: AnalysisRecovery,
    ) -> Self {
        Self {
            kind,
            producer_stage,
            path: None,
            affected_items: None,
            bounded_detail: None,
            recovery,
        }
    }

    pub(crate) fn with_path(mut self, path: impl AsRef<str>) -> Result<Self, String> {
        self.path = Some(normalize_portable_analysis_path(path.as_ref())?);
        Ok(self)
    }

    pub(crate) fn with_affected_items(mut self, affected_items: u64) -> Result<Self, String> {
        if affected_items == 0 {
            return Err("analysis limitation affected_items must be positive; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it".to_string());
        }
        self.affected_items = Some(affected_items);
        Ok(self)
    }

    pub(crate) fn with_detail(mut self, detail: impl Into<String>) -> Result<Self, String> {
        self.bounded_detail = Some(bounded_nonempty_text(
            "analysis limitation detail",
            detail.into(),
        )?);
        Ok(self)
    }

    fn normalize(&mut self) -> Result<(), String> {
        if let Some(path) = self.path.take() {
            self.path = Some(normalize_portable_analysis_path(&path)?);
        }
        if self.affected_items == Some(0) {
            return Err("analysis limitation affected_items must be positive; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it".to_string());
        }
        normalize_optional_bounded_text("analysis limitation detail", &mut self.bounded_detail)?;
        self.recovery.normalize()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AnalysisOutcome {
    pub(crate) schema_version: String,
    pub(crate) kind: AnalysisOutcomeKind,
    pub(crate) identity: AnalysisIdentity,
    pub(crate) counts: AnalysisOutcomeCounts,
    pub(crate) limitations: Vec<AnalysisLimitation>,
    pub(crate) claim_boundary: String,
    /// Run-local producer inputs, never reconstructed from a serialized outcome.
    #[serde(skip)]
    pub(crate) check_findings_inputs: Option<CheckFindingsInputs>,
    /// Run-local mixed-language completeness view (#11272). Read only by the
    /// check JSON renderer's `language_completeness` member; never part of the
    /// serialized outcome or its identity.
    #[serde(skip)]
    pub(crate) language_view: Option<crate::analysis::language_outcome::LanguageCompletenessView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckFindingsInputs {
    pub(crate) config_identity: String,
    pub(crate) include_unchanged_tests: bool,
    pub(crate) languages: Vec<crate::domain::LanguageId>,
    pub(crate) perl_facts_used: bool,
}

#[derive(Deserialize)]
struct AnalysisOutcomeWire {
    schema_version: String,
    kind: AnalysisOutcomeKind,
    identity: AnalysisIdentity,
    counts: AnalysisOutcomeCounts,
    limitations: Vec<AnalysisLimitation>,
    claim_boundary: String,
}

impl<'de> Deserialize<'de> for AnalysisOutcome {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = AnalysisOutcomeWire::deserialize(deserializer)?;
        if wire.schema_version != ANALYSIS_OUTCOME_SCHEMA_VERSION {
            return Err(D::Error::custom(format!(
                "analysis outcome schema_version must be {ANALYSIS_OUTCOME_SCHEMA_VERSION}, got {}",
                wire.schema_version
            )));
        }
        if wire.claim_boundary != ANALYSIS_OUTCOME_CLAIM_BOUNDARY {
            return Err(D::Error::custom(
                "analysis outcome claim_boundary does not match the versioned contract",
            ));
        }
        AnalysisOutcome::new(wire.kind, wire.identity, wire.counts, wire.limitations)
            .map_err(D::Error::custom)
    }
}

impl AnalysisOutcome {
    pub(crate) fn new(
        kind: AnalysisOutcomeKind,
        mut identity: AnalysisIdentity,
        counts: AnalysisOutcomeCounts,
        mut limitations: Vec<AnalysisLimitation>,
    ) -> Result<Self, String> {
        identity.normalize()?;
        for limitation in &mut limitations {
            limitation.normalize()?;
        }
        limitations.sort();
        limitations.dedup();
        validate_outcome(kind, counts, &limitations)?;
        Ok(Self {
            schema_version: ANALYSIS_OUTCOME_SCHEMA_VERSION.to_string(),
            kind,
            identity,
            counts,
            limitations,
            claim_boundary: ANALYSIS_OUTCOME_CLAIM_BOUNDARY.to_string(),
            check_findings_inputs: None,
            language_view: None,
        })
    }

    /// Stable semantic commitment over the versioned typed outcome. The DTO
    /// contains no timestamp, duration, map, or absolute checkout path; the
    /// constructor sorts and deduplicates limitations before hashing.
    pub(crate) fn semantic_digest(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("serialize analysis outcome for digest failed: {error}; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"))?;
        let digest = Sha256::digest(bytes);
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(&mut hex, "{byte:02x}")
                .map_err(|error| format!("format analysis outcome digest failed: {error}; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"))?;
        }
        Ok(format!("sha256:{hex}"))
    }
}

pub(crate) fn normalize_portable_analysis_path(path: &str) -> Result<String, String> {
    // Paths are filesystem identities, not prose: trimming can name a different
    // existing file and disconnect a limitation from its admitted diff path.
    let normalized = path.replace('\\', "/");
    let without_current = normalized.trim_start_matches("./");
    let bytes = without_current.as_bytes();
    let has_drive_prefix = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if without_current.is_empty() || without_current.starts_with('/') || has_drive_prefix {
        return Err(format!(
            "analysis limitation path must be repository-relative: {path}; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"
        ));
    }

    let mut segments = Vec::new();
    for segment in without_current.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                return Err(format!(
                    "analysis limitation path contains parent traversal: {path}; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"
                ));
            }
            value => segments.push(value),
        }
    }
    if segments.is_empty() {
        return Err(format!(
            "analysis limitation path has no portable segments: {path}; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"
        ));
    }
    Ok(segments.join("/"))
}

fn normalize_optional_bounded_text(field: &str, value: &mut Option<String>) -> Result<(), String> {
    if let Some(current) = value.take() {
        *value = Some(bounded_nonempty_text(field, current)?);
    }
    Ok(())
}

fn bounded_nonempty_text(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!(
            "{field} must not be empty; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"
        ));
    }
    let count = value.chars().count();
    if count > MAX_ANALYSIS_LIMITATION_DETAIL_CHARS {
        return Err(format!(
            "{field} has {count} characters; maximum is {MAX_ANALYSIS_LIMITATION_DETAIL_CHARS}"
        ));
    }
    Ok(value)
}

fn validate_outcome(
    kind: AnalysisOutcomeKind,
    counts: AnalysisOutcomeCounts,
    limitations: &[AnalysisLimitation],
) -> Result<(), String> {
    let complete_or_subject_absent = matches!(
        kind,
        AnalysisOutcomeKind::NoScope
            | AnalysisOutcomeKind::NoChangedLines
            | AnalysisOutcomeKind::NoBehavioralCandidates
            | AnalysisOutcomeKind::CompleteNoFindings
            | AnalysisOutcomeKind::CompleteWithFindings
    );
    // #4952: EolOnlyChurn (and RIPR-SPEC-0368 VerificationOnlyChange) is a
    // change-shape disclosure, not an incomplete-analysis limitation — the
    // changed scope was fully analyzed — so a complete outcome may carry
    // those, and only those. Every other kind
    // still marks the outcome incomplete.
    if complete_or_subject_absent
        && limitations
            .iter()
            .any(|limitation| !limitation.kind.is_advisory_disclosure())
    {
        return Err(format!(
            "analysis outcome {kind:?} cannot carry incomplete-analysis limitations; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"
        ));
    }

    let limited_or_failed = matches!(
        kind,
        AnalysisOutcomeKind::PartialWithLimitations
            | AnalysisOutcomeKind::UnsupportedInput
            | AnalysisOutcomeKind::AnalysisFailed
    );
    if limited_or_failed && limitations.is_empty() {
        return Err(format!(
            "analysis outcome {kind:?} requires at least one typed limitation; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"
        ));
    }

    match kind {
        AnalysisOutcomeKind::NoScope => {
            if counts != AnalysisOutcomeCounts::default() {
                return Err("analysis outcome NoScope requires every count to be zero; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it".to_string());
            }
        }
        AnalysisOutcomeKind::NoChangedLines => {
            if counts.changed_line_count != 0
                || counts.candidate_line_count != 0
                || counts.probe_count != 0
                || counts.finding_count != 0
            {
                return Err(
                    "analysis outcome NoChangedLines requires zero changed-line, candidate, probe, and finding counts; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"
                        .to_string(),
                );
            }
        }
        AnalysisOutcomeKind::NoBehavioralCandidates => {
            if counts.changed_line_count == 0
                || counts.candidate_line_count != 0
                || counts.probe_count != 0
                || counts.finding_count != 0
            {
                return Err(format!(
                    "no_behavioral_candidates requires changed lines and zero candidate, probe, and finding counts (changed_line_count={}, candidate_line_count={}, probe_count={}, finding_count={}); regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it",
                    counts.changed_line_count,
                    counts.candidate_line_count,
                    counts.probe_count,
                    counts.finding_count,
                ));
            }
        }
        AnalysisOutcomeKind::CompleteNoFindings => {
            if counts.finding_count != 0 {
                return Err("complete_no_findings requires finding_count = 0; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it".to_string());
            }
            if counts.candidate_line_count == 0 && counts.probe_count == 0 {
                return Err(
                    "complete_no_findings requires a behavioral candidate or probe subject; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it"
                        .to_string(),
                );
            }
        }
        AnalysisOutcomeKind::CompleteWithFindings => {
            if counts.finding_count == 0 {
                return Err("complete_with_findings requires finding_count > 0; regenerate the artifact with the current ripr, or report this as a ripr bug with the command that produced it".to_string());
            }
        }
        AnalysisOutcomeKind::PartialWithLimitations
        | AnalysisOutcomeKind::UnsupportedInput
        | AnalysisOutcomeKind::AnalysisFailed => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::{Debug, Display};

    #[test]
    fn limitation_plain_labels_are_words_not_schema_tokens() {
        use AnalysisLimitationKind as K;
        use AnalysisRecoveryKind as R;
        use AnalysisStage as S;
        let labels = [
            K::CombinedHunkUnsupported.plain_label(),
            K::UnresolvedConflictMarkers.plain_label(),
            K::MalformedDiff.plain_label(),
            K::DiffScopeOversized.plain_label(),
            K::LanguageAdapterUnavailable.plain_label(),
            K::LanguageScopeUnsupported.plain_label(),
            K::ProducerTimeout.plain_label(),
            K::ProducerFailure.plain_label(),
            S::DiffLoad.plain_label(),
            S::DiffParse.plain_label(),
            S::LanguageAdapter.plain_label(),
            S::ProbeGeneration.plain_label(),
            S::FindingClassification.plain_label(),
            S::AnalysisPipeline.plain_label(),
            R::NarrowDiff.plain_label(),
            R::UseTwoWayDiff.plain_label(),
            R::ResolveConflicts.plain_label(),
            R::EnableLanguage.plain_label(),
            R::IncreaseConfiguredLimit.plain_label(),
            R::Retry.plain_label(),
            R::InspectFailure.plain_label(),
        ];
        for label in labels {
            assert!(!label.is_empty() && !label.contains('_'), "{label}");
        }
    }

    fn recovery() -> Result<AnalysisRecovery, String> {
        AnalysisRecovery::new(
            AnalysisRecoveryKind::InspectFailure,
            "Inspect the typed limitation before retrying.",
        )
    }

    fn limitation(kind: AnalysisLimitationKind) -> Result<AnalysisLimitation, String> {
        Ok(AnalysisLimitation::new(
            kind,
            AnalysisStage::DiffParse,
            recovery()?,
        ))
    }

    fn outcome(
        kind: AnalysisOutcomeKind,
        counts: AnalysisOutcomeCounts,
        limitations: Vec<AnalysisLimitation>,
    ) -> Result<AnalysisOutcome, String> {
        AnalysisOutcome::new(kind, AnalysisIdentity::default(), counts, limitations)
    }

    fn expect_error<T: Debug, E: Display>(
        result: Result<T, E>,
        expected: &str,
    ) -> Result<(), String> {
        match result {
            Ok(value) => Err(format!(
                "expected error containing `{expected}`, got success: {value:?}"
            )),
            Err(error) if error.to_string().contains(expected) => Ok(()),
            Err(error) => Err(format!(
                "expected error containing `{expected}`, got `{error}`"
            )),
        }
    }

    #[test]
    fn closed_outcome_kinds_round_trip() -> Result<(), String> {
        let cases = [
            outcome(
                AnalysisOutcomeKind::NoScope,
                AnalysisOutcomeCounts::default(),
                Vec::new(),
            )?,
            outcome(
                AnalysisOutcomeKind::NoChangedLines,
                AnalysisOutcomeCounts {
                    changed_file_count: 1,
                    ..AnalysisOutcomeCounts::default()
                },
                Vec::new(),
            )?,
            outcome(
                AnalysisOutcomeKind::NoBehavioralCandidates,
                AnalysisOutcomeCounts {
                    changed_file_count: 1,
                    changed_line_count: 2,
                    ..AnalysisOutcomeCounts::default()
                },
                Vec::new(),
            )?,
            outcome(
                AnalysisOutcomeKind::CompleteNoFindings,
                AnalysisOutcomeCounts {
                    changed_file_count: 1,
                    changed_line_count: 1,
                    candidate_line_count: 1,
                    probe_count: 1,
                    finding_count: 0,
                },
                Vec::new(),
            )?,
            outcome(
                AnalysisOutcomeKind::CompleteWithFindings,
                AnalysisOutcomeCounts {
                    changed_file_count: 1,
                    changed_line_count: 1,
                    candidate_line_count: 1,
                    probe_count: 1,
                    finding_count: 1,
                },
                Vec::new(),
            )?,
            outcome(
                AnalysisOutcomeKind::PartialWithLimitations,
                AnalysisOutcomeCounts::default(),
                vec![limitation(AnalysisLimitationKind::DiffScopeOversized)?],
            )?,
            outcome(
                AnalysisOutcomeKind::UnsupportedInput,
                AnalysisOutcomeCounts::default(),
                vec![limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?],
            )?,
            outcome(
                AnalysisOutcomeKind::AnalysisFailed,
                AnalysisOutcomeCounts::default(),
                vec![limitation(AnalysisLimitationKind::ProducerFailure)?],
            )?,
        ];

        for expected in cases {
            let json = serde_json::to_string(&expected)
                .map_err(|error| format!("serialize outcome fixture failed: {error}"))?;
            let actual: AnalysisOutcome = serde_json::from_str(&json)
                .map_err(|error| format!("parse outcome fixture failed: {error}"))?;
            assert_eq!(actual, expected);
        }
        Ok(())
    }

    /// Wildcard-free match: a new `AnalysisLimitationKind` is a compile error
    /// until this test lists its wire token.
    fn exhaustive_limitation_wire_token(kind: AnalysisLimitationKind) -> &'static str {
        match kind {
            AnalysisLimitationKind::CombinedHunkUnsupported => "combined_hunk_unsupported",
            AnalysisLimitationKind::UnresolvedConflictMarkers => "unresolved_conflict_markers",
            AnalysisLimitationKind::MalformedDiff => "malformed_diff",
            AnalysisLimitationKind::DiffScopeOversized => "diff_scope_oversized",
            AnalysisLimitationKind::LanguageAdapterUnavailable => "language_adapter_unavailable",
            AnalysisLimitationKind::LanguageScopeUnsupported => "language_scope_unsupported",
            AnalysisLimitationKind::ChangedFileAbsentFromWorktree => {
                "changed_file_absent_from_worktree"
            }
            AnalysisLimitationKind::ProducerTimeout => "producer_timeout",
            AnalysisLimitationKind::ProducerFailure => "producer_failure",
            AnalysisLimitationKind::EolOnlyChurn => "eol_only_churn",
            AnalysisLimitationKind::VerificationOnlyChange => "verification_only_change",
            AnalysisLimitationKind::UnevaluatedCondition => "unevaluated_condition",
        }
    }

    const ALL_ANALYSIS_LIMITATION_KINDS: [AnalysisLimitationKind; 12] = [
        AnalysisLimitationKind::CombinedHunkUnsupported,
        AnalysisLimitationKind::UnresolvedConflictMarkers,
        AnalysisLimitationKind::MalformedDiff,
        AnalysisLimitationKind::DiffScopeOversized,
        AnalysisLimitationKind::LanguageAdapterUnavailable,
        AnalysisLimitationKind::LanguageScopeUnsupported,
        AnalysisLimitationKind::ChangedFileAbsentFromWorktree,
        AnalysisLimitationKind::ProducerTimeout,
        AnalysisLimitationKind::ProducerFailure,
        AnalysisLimitationKind::EolOnlyChurn,
        AnalysisLimitationKind::VerificationOnlyChange,
        AnalysisLimitationKind::UnevaluatedCondition,
    ];

    fn analysis_limitation_kind_ordinal(kind: AnalysisLimitationKind) -> usize {
        match kind {
            AnalysisLimitationKind::CombinedHunkUnsupported => 0,
            AnalysisLimitationKind::UnresolvedConflictMarkers => 1,
            AnalysisLimitationKind::MalformedDiff => 2,
            AnalysisLimitationKind::DiffScopeOversized => 3,
            AnalysisLimitationKind::LanguageAdapterUnavailable => 4,
            AnalysisLimitationKind::LanguageScopeUnsupported => 5,
            AnalysisLimitationKind::ChangedFileAbsentFromWorktree => 6,
            AnalysisLimitationKind::ProducerTimeout => 7,
            AnalysisLimitationKind::ProducerFailure => 8,
            AnalysisLimitationKind::EolOnlyChurn => 9,
            AnalysisLimitationKind::VerificationOnlyChange => 10,
            AnalysisLimitationKind::UnevaluatedCondition => 11,
        }
    }

    #[test]
    fn typed_limitation_tokens_match_the_wire_contract() -> Result<(), String> {
        use std::collections::BTreeSet;

        const LIMITATION_KIND_COUNT: usize = 12;
        assert_eq!(ALL_ANALYSIS_LIMITATION_KINDS.len(), LIMITATION_KIND_COUNT);
        let ordinals: Vec<usize> = ALL_ANALYSIS_LIMITATION_KINDS
            .iter()
            .copied()
            .map(analysis_limitation_kind_ordinal)
            .collect();
        assert_eq!(
            ordinals,
            (0..ALL_ANALYSIS_LIMITATION_KINDS.len()).collect::<Vec<_>>(),
            "ALL_ANALYSIS_LIMITATION_KINDS must list every variant once"
        );

        let mut producer_tokens = BTreeSet::new();
        for kind in ALL_ANALYSIS_LIMITATION_KINDS {
            let expected = exhaustive_limitation_wire_token(kind);
            assert_eq!(kind.as_str(), expected, "{kind:?}");
            producer_tokens.insert(expected);
        }

        // Every wire token the producer can emit must be accepted by the
        // published review-comments schema, so a new limitation kind cannot
        // reach `check-output-contracts`/PR evidence as a schema violation.
        let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/ripr/review-comments.schema.json");
        let schema_text = std::fs::read_to_string(&schema_path)
            .map_err(|error| format!("read {} failed: {error}", schema_path.display()))?;
        let schema: serde_json::Value = serde_json::from_str(&schema_text)
            .map_err(|error| format!("parse review-comments schema failed: {error}"))?;
        let schema_kind_enum = schema["$defs"]["analysis_limitation"]["properties"]["kind"]["enum"]
            .as_array()
            .ok_or_else(|| "analysis_limitation.kind enum missing".to_string())?;
        let mut schema_kind_strings = Vec::with_capacity(schema_kind_enum.len());
        for (index, value) in schema_kind_enum.iter().enumerate() {
            let token = value.as_str().ok_or_else(|| {
                format!("analysis_limitation.kind enum entry {index} is not a string")
            })?;
            schema_kind_strings.push(token);
        }
        let schema_tokens: BTreeSet<&str> = schema_kind_strings.iter().copied().collect();
        if schema_tokens.len() != schema_kind_strings.len() {
            return Err(format!(
                "analysis_limitation.kind enum contains duplicate tokens: {schema_kind_strings:?}"
            ));
        }
        let missing_from_schema: Vec<&str> = producer_tokens
            .iter()
            .copied()
            .filter(|token| !schema_tokens.contains(token))
            .collect();
        let extra_in_schema: Vec<&str> = schema_tokens
            .iter()
            .copied()
            .filter(|token| !producer_tokens.contains(token))
            .collect();
        if !missing_from_schema.is_empty() || !extra_in_schema.is_empty() {
            return Err(format!(
                "analysis_limitation.kind enum mismatch: missing from schema: {missing_from_schema:?}; extra in schema: {extra_in_schema:?}"
            ));
        }

        // The single owner of advisory-disclosure membership: only the two
        // change-shape disclosures leave an outcome complete.
        for kind in ALL_ANALYSIS_LIMITATION_KINDS {
            let advisory = matches!(
                kind,
                AnalysisLimitationKind::EolOnlyChurn
                    | AnalysisLimitationKind::VerificationOnlyChange
            );
            assert_eq!(kind.is_advisory_disclosure(), advisory, "{kind:?}");
        }

        let stages = [
            (AnalysisStage::DiffLoad, "diff_load"),
            (AnalysisStage::DiffParse, "diff_parse"),
            (AnalysisStage::LanguageAdapter, "language_adapter"),
            (AnalysisStage::ProbeGeneration, "probe_generation"),
            (
                AnalysisStage::FindingClassification,
                "finding_classification",
            ),
            (AnalysisStage::AnalysisPipeline, "analysis_pipeline"),
        ];
        for (stage, expected) in stages {
            assert_eq!(stage.as_str(), expected);
        }

        let recoveries = [
            (AnalysisRecoveryKind::NarrowDiff, "narrow_diff"),
            (AnalysisRecoveryKind::UseTwoWayDiff, "use_two_way_diff"),
            (AnalysisRecoveryKind::ResolveConflicts, "resolve_conflicts"),
            (AnalysisRecoveryKind::EnableLanguage, "enable_language"),
            (
                AnalysisRecoveryKind::IncreaseConfiguredLimit,
                "increase_configured_limit",
            ),
            (AnalysisRecoveryKind::Retry, "retry"),
            (AnalysisRecoveryKind::InspectFailure, "inspect_failure"),
            (
                AnalysisRecoveryKind::DeclareHandwrittenFiles,
                "declare_handwritten_files",
            ),
            (
                AnalysisRecoveryKind::DeclarePreviewExcludedPatterns,
                "declare_preview_excluded_patterns",
            ),
            (
                AnalysisRecoveryKind::RenameGeneratedMarkedSource,
                "rename_generated_marked_source",
            ),
        ];
        for (recovery, expected) in recoveries {
            assert_eq!(recovery.as_str(), expected);
        }
        Ok(())
    }

    #[test]
    fn no_behavioral_candidates_requires_positive_producer_facts() -> Result<(), String> {
        expect_error(
            outcome(
                AnalysisOutcomeKind::NoBehavioralCandidates,
                AnalysisOutcomeCounts::default(),
                Vec::new(),
            ),
            "requires changed lines",
        )?;

        let valid = outcome(
            AnalysisOutcomeKind::NoBehavioralCandidates,
            AnalysisOutcomeCounts {
                changed_file_count: 1,
                changed_line_count: 3,
                ..AnalysisOutcomeCounts::default()
            },
            Vec::new(),
        )?;
        assert_eq!(valid.kind, AnalysisOutcomeKind::NoBehavioralCandidates);
        Ok(())
    }

    #[test]
    fn complete_outcome_cannot_hide_a_limitation() -> Result<(), String> {
        expect_error(
            outcome(
                AnalysisOutcomeKind::CompleteNoFindings,
                AnalysisOutcomeCounts {
                    changed_file_count: 1,
                    changed_line_count: 1,
                    candidate_line_count: 1,
                    probe_count: 1,
                    finding_count: 0,
                },
                vec![limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?],
            ),
            "cannot carry incomplete-analysis limitations",
        )
    }

    #[test]
    fn complete_no_findings_requires_a_behavioral_subject() -> Result<(), String> {
        expect_error(
            outcome(
                AnalysisOutcomeKind::CompleteNoFindings,
                AnalysisOutcomeCounts {
                    changed_file_count: 1,
                    changed_line_count: 2,
                    ..AnalysisOutcomeCounts::default()
                },
                Vec::new(),
            ),
            "requires a behavioral candidate or probe subject",
        )
    }

    #[test]
    fn limitations_are_sorted_deduplicated_and_digest_stable() -> Result<(), String> {
        let combined = limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?
            .with_path("src\\lib.rs")?
            .with_affected_items(1)?;
        let conflicts = limitation(AnalysisLimitationKind::UnresolvedConflictMarkers)?
            .with_path("./src/lib.rs")?
            .with_affected_items(2)?;
        let left = outcome(
            AnalysisOutcomeKind::UnsupportedInput,
            AnalysisOutcomeCounts::default(),
            vec![conflicts.clone(), combined.clone(), combined.clone()],
        )?;
        let right = outcome(
            AnalysisOutcomeKind::UnsupportedInput,
            AnalysisOutcomeCounts::default(),
            vec![combined, conflicts],
        )?;
        assert_eq!(left.limitations, right.limitations);
        assert_eq!(left.semantic_digest()?, right.semantic_digest()?);
        assert_eq!(left.limitations[0].path.as_deref(), Some("src/lib.rs"));
        Ok(())
    }

    #[test]
    fn adding_a_limitation_changes_semantic_identity() -> Result<(), String> {
        let one = outcome(
            AnalysisOutcomeKind::UnsupportedInput,
            AnalysisOutcomeCounts::default(),
            vec![limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?],
        )?;
        let two = outcome(
            AnalysisOutcomeKind::UnsupportedInput,
            AnalysisOutcomeCounts::default(),
            vec![
                limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?,
                limitation(AnalysisLimitationKind::UnresolvedConflictMarkers)?,
            ],
        )?;
        assert_ne!(one.semantic_digest()?, two.semantic_digest()?);
        Ok(())
    }

    #[test]
    fn unsupported_input_and_producer_failure_remain_distinct() -> Result<(), String> {
        let unsupported = outcome(
            AnalysisOutcomeKind::UnsupportedInput,
            AnalysisOutcomeCounts::default(),
            vec![limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?],
        )?;
        let failed = outcome(
            AnalysisOutcomeKind::AnalysisFailed,
            AnalysisOutcomeCounts::default(),
            vec![limitation(AnalysisLimitationKind::ProducerFailure)?],
        )?;
        assert_ne!(unsupported.kind, failed.kind);
        assert_ne!(unsupported.semantic_digest()?, failed.semantic_digest()?);
        Ok(())
    }

    #[test]
    fn portable_paths_reject_absolute_and_parent_traversal() -> Result<(), String> {
        expect_error(
            normalize_portable_analysis_path("/tmp/file.rs"),
            "repository-relative",
        )?;
        expect_error(
            normalize_portable_analysis_path("../file.rs"),
            "parent traversal",
        )?;
        let windows_absolute = ["C:", "tmp", "file.rs"].join("\\");
        expect_error(
            normalize_portable_analysis_path(&windows_absolute),
            "repository-relative",
        )?;
        let normalized = normalize_portable_analysis_path("src\\module\\file.rs")?;
        assert_eq!(normalized, "src/module/file.rs");
        Ok(())
    }

    #[test]
    fn limitation_paths_preserve_whitespace_identity_through_round_trip() -> Result<(), String> {
        let paths = [
            "leading.py",
            " leading.py",
            "trailing.py ",
            " spaced/discount.py",
            "\tfile.rs",
            " ",
        ];
        let limitations = paths
            .iter()
            .map(|path| {
                limitation(AnalysisLimitationKind::ChangedFileAbsentFromWorktree)?.with_path(path)
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (path, limitation) in paths.iter().zip(&limitations) {
            assert_eq!(limitation.path.as_deref(), Some(*path));
        }
        let value = outcome(
            AnalysisOutcomeKind::PartialWithLimitations,
            AnalysisOutcomeCounts::default(),
            limitations,
        )?;
        assert_eq!(
            value.limitations.len(),
            paths.len(),
            "different file names must not deduplicate"
        );
        let json = serde_json::to_string(&value).map_err(|error| error.to_string())?;
        let restored: AnalysisOutcome =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        assert_eq!(restored, value);
        assert_eq!(restored.semantic_digest()?, value.semantic_digest()?);
        let spaced = outcome(
            AnalysisOutcomeKind::PartialWithLimitations,
            AnalysisOutcomeCounts::default(),
            vec![
                limitation(AnalysisLimitationKind::ChangedFileAbsentFromWorktree)?
                    .with_path(" leading.py")?,
            ],
        )?;
        let plain = outcome(
            AnalysisOutcomeKind::PartialWithLimitations,
            AnalysisOutcomeCounts::default(),
            vec![
                limitation(AnalysisLimitationKind::ChangedFileAbsentFromWorktree)?
                    .with_path("leading.py")?,
            ],
        )?;
        assert_ne!(spaced.semantic_digest()?, plain.semantic_digest()?);
        assert_eq!(
            normalize_portable_analysis_path("./ spaced\\file.rs ")?,
            " spaced/file.rs "
        );
        Ok(())
    }

    #[test]
    fn limitation_text_and_affected_counts_are_bounded() -> Result<(), String> {
        let too_long = "x".repeat(MAX_ANALYSIS_LIMITATION_DETAIL_CHARS + 1);
        expect_error(
            AnalysisRecovery::new(AnalysisRecoveryKind::Retry, too_long.clone()),
            "maximum",
        )?;
        expect_error(
            limitation(AnalysisLimitationKind::ProducerFailure)?.with_detail(too_long),
            "maximum",
        )?;
        expect_error(
            limitation(AnalysisLimitationKind::ProducerFailure)?.with_affected_items(0),
            "must be positive",
        )?;
        Ok(())
    }

    #[test]
    fn deserialization_revalidates_schema_claims_and_paths() -> Result<(), String> {
        let valid = outcome(
            AnalysisOutcomeKind::UnsupportedInput,
            AnalysisOutcomeCounts::default(),
            vec![limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?],
        )?;
        let mut value = serde_json::to_value(valid)
            .map_err(|error| format!("serialize outcome value failed: {error}"))?;
        value["schema_version"] = serde_json::json!("9.9");
        expect_error(
            serde_json::from_value::<AnalysisOutcome>(value),
            "schema_version must be",
        )?;

        let malformed = serde_json::json!({
            "schema_version": ANALYSIS_OUTCOME_SCHEMA_VERSION,
            "kind": "unsupported_input",
            "identity": {},
            "counts": {
                "changed_file_count": 0,
                "changed_line_count": 0,
                "candidate_line_count": 0,
                "probe_count": 0,
                "finding_count": 0
            },
            "limitations": [{
                "kind": "combined_hunk_unsupported",
                "producer_stage": "diff_parse",
                "path": "../outside.rs",
                "affected_items": 1,
                "bounded_detail": null,
                "recovery": {
                    "kind": "use_two_way_diff",
                    "detail": "Use a two-way diff."
                }
            }],
            "claim_boundary": ANALYSIS_OUTCOME_CLAIM_BOUNDARY
        });
        expect_error(
            serde_json::from_value::<AnalysisOutcome>(malformed),
            "parent traversal",
        )
    }
}
