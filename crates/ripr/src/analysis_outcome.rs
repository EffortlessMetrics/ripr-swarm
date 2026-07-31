use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ANALYSIS_OUTCOME_SCHEMA_VERSION: &str = "0.1";
pub const ANALYSIS_OUTCOME_CLAIM_BOUNDARY: &str =
    "Static analysis outcome only; no correctness, test-adequacy, runtime-execution, or merge-readiness claim.";
pub const MAX_ANALYSIS_LIMITATION_DETAIL_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisOutcomeKind {
    NoScope,
    NoChangedLines,
    NoBehavioralCandidates,
    CompleteNoFindings,
    CompleteWithFindings,
    PartialWithLimitations,
    UnsupportedInput,
    AnalysisFailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStage {
    DiffLoad,
    DiffParse,
    LanguageAdapter,
    ProbeGeneration,
    FindingClassification,
    AnalysisPipeline,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisLimitationKind {
    CombinedHunkUnsupported,
    UnresolvedConflictMarkers,
    DiffScopeOversized,
    LanguageAdapterUnavailable,
    LanguageScopeUnsupported,
    ProducerTimeout,
    ProducerFailure,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisRecoveryKind {
    NarrowDiff,
    UseTwoWayDiff,
    ResolveConflicts,
    EnableLanguage,
    IncreaseConfiguredLimit,
    Retry,
    InspectFailure,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AnalysisIdentity {
    /// Stable repository identity or commitment. Never place an absolute
    /// checkout path in this field.
    pub repository_identity: Option<String>,
    /// Stable workspace/root identity or commitment. Never place an absolute
    /// checkout path in this field.
    pub root_identity: Option<String>,
    pub config_identity: Option<String>,
    pub base_revision: Option<String>,
    pub input_identity: Option<String>,
    pub snapshot_identity: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AnalysisOutcomeCounts {
    pub changed_file_count: u64,
    pub changed_line_count: u64,
    pub candidate_line_count: u64,
    pub probe_count: u64,
    pub finding_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AnalysisRecovery {
    pub kind: AnalysisRecoveryKind,
    pub detail: String,
}

impl AnalysisRecovery {
    pub fn new(kind: AnalysisRecoveryKind, detail: impl Into<String>) -> Result<Self, String> {
        let detail = bounded_nonempty_text("analysis recovery detail", detail.into())?;
        Ok(Self { kind, detail })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AnalysisLimitation {
    pub kind: AnalysisLimitationKind,
    pub producer_stage: AnalysisStage,
    /// Repository-relative portable path. Absolute paths and parent traversal
    /// are rejected before serialization.
    pub path: Option<String>,
    pub affected_items: Option<u64>,
    pub bounded_detail: Option<String>,
    pub recovery: AnalysisRecovery,
}

impl AnalysisLimitation {
    pub fn new(
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

    pub fn with_path(mut self, path: impl AsRef<str>) -> Result<Self, String> {
        self.path = Some(normalize_portable_analysis_path(path.as_ref())?);
        Ok(self)
    }

    pub fn with_affected_items(mut self, affected_items: u64) -> Self {
        self.affected_items = Some(affected_items);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Result<Self, String> {
        self.bounded_detail = Some(bounded_nonempty_text(
            "analysis limitation detail",
            detail.into(),
        )?);
        Ok(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnalysisOutcome {
    pub schema_version: String,
    pub kind: AnalysisOutcomeKind,
    pub identity: AnalysisIdentity,
    pub counts: AnalysisOutcomeCounts,
    pub limitations: Vec<AnalysisLimitation>,
    pub claim_boundary: String,
}

impl AnalysisOutcome {
    pub fn new(
        kind: AnalysisOutcomeKind,
        identity: AnalysisIdentity,
        counts: AnalysisOutcomeCounts,
        mut limitations: Vec<AnalysisLimitation>,
    ) -> Result<Self, String> {
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
        })
    }

    /// Stable semantic commitment over the versioned typed outcome. The DTO
    /// contains no timestamp, duration, map, or absolute checkout path; the
    /// constructor sorts and deduplicates limitations before hashing.
    pub fn semantic_digest(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("serialize analysis outcome for digest failed: {error}"))?;
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Ok(format!("sha256:{:x}", hasher.finalize()))
    }
}

pub fn normalize_portable_analysis_path(path: &str) -> Result<String, String> {
    let normalized = path.trim().replace('\\', "/");
    let without_current = normalized.trim_start_matches("./");
    let bytes = without_current.as_bytes();
    let has_drive_prefix = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if without_current.is_empty()
        || without_current.starts_with('/')
        || without_current.starts_with("//")
        || has_drive_prefix
    {
        return Err(format!(
            "analysis limitation path must be repository-relative: {path}"
        ));
    }

    let mut segments = Vec::new();
    for segment in without_current.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                return Err(format!(
                    "analysis limitation path contains parent traversal: {path}"
                ));
            }
            value => segments.push(value),
        }
    }
    if segments.is_empty() {
        return Err(format!(
            "analysis limitation path has no portable segments: {path}"
        ));
    }
    Ok(segments.join("/"))
}

fn bounded_nonempty_text(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!("{field} must not be empty"));
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
    if complete_or_subject_absent && !limitations.is_empty() {
        return Err(format!(
            "analysis outcome {kind:?} cannot carry incomplete-analysis limitations"
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
            "analysis outcome {kind:?} requires at least one typed limitation"
        ));
    }

    match kind {
        AnalysisOutcomeKind::NoScope | AnalysisOutcomeKind::NoChangedLines => {
            if counts.changed_line_count != 0
                || counts.candidate_line_count != 0
                || counts.probe_count != 0
                || counts.finding_count != 0
            {
                return Err(format!(
                    "analysis outcome {kind:?} requires zero changed-line, candidate, probe, and finding counts"
                ));
            }
        }
        AnalysisOutcomeKind::NoBehavioralCandidates => {
            if counts.changed_line_count == 0
                || counts.candidate_line_count != 0
                || counts.probe_count != 0
                || counts.finding_count != 0
            {
                return Err(
                    "no_behavioral_candidates requires changed lines and zero candidate, probe, and finding counts"
                        .to_string(),
                );
            }
        }
        AnalysisOutcomeKind::CompleteNoFindings => {
            if counts.finding_count != 0 {
                return Err("complete_no_findings requires finding_count = 0".to_string());
            }
        }
        AnalysisOutcomeKind::CompleteWithFindings => {
            if counts.finding_count == 0 {
                return Err("complete_with_findings requires finding_count > 0".to_string());
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

    #[test]
    fn closed_outcome_kinds_round_trip() -> Result<(), String> {
        let cases = vec![
            outcome(
                AnalysisOutcomeKind::NoScope,
                AnalysisOutcomeCounts::default(),
                Vec::new(),
            )?,
            outcome(
                AnalysisOutcomeKind::NoChangedLines,
                AnalysisOutcomeCounts::default(),
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
                vec![limitation(
                    AnalysisLimitationKind::CombinedHunkUnsupported,
                )?],
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

    #[test]
    fn no_behavioral_candidates_requires_positive_producer_facts() -> Result<(), String> {
        let invalid = outcome(
            AnalysisOutcomeKind::NoBehavioralCandidates,
            AnalysisOutcomeCounts::default(),
            Vec::new(),
        );
        assert!(invalid.is_err());

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
        let result = outcome(
            AnalysisOutcomeKind::CompleteNoFindings,
            AnalysisOutcomeCounts::default(),
            vec![limitation(
                AnalysisLimitationKind::CombinedHunkUnsupported,
            )?],
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn limitations_are_sorted_deduplicated_and_digest_stable() -> Result<(), String> {
        let combined = limitation(AnalysisLimitationKind::CombinedHunkUnsupported)?
            .with_path("src\\lib.rs")?
            .with_affected_items(1);
        let conflicts = limitation(AnalysisLimitationKind::UnresolvedConflictMarkers)?
            .with_path("./src/lib.rs")?
            .with_affected_items(2);
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
            vec![limitation(
                AnalysisLimitationKind::CombinedHunkUnsupported,
            )?],
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
            vec![limitation(
                AnalysisLimitationKind::CombinedHunkUnsupported,
            )?],
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
    fn portable_paths_reject_absolute_and_parent_traversal() {
        assert!(normalize_portable_analysis_path("/tmp/file.rs").is_err());
        assert!(normalize_portable_analysis_path("../file.rs").is_err());
        assert!(normalize_portable_analysis_path("C:\\tmp\\file.rs").is_err());
        assert_eq!(
            normalize_portable_analysis_path("src\\module\\file.rs"),
            Ok("src/module/file.rs".to_string())
        );
    }

    #[test]
    fn limitation_text_is_bounded() -> Result<(), String> {
        let too_long = "x".repeat(MAX_ANALYSIS_LIMITATION_DETAIL_CHARS + 1);
        assert!(
            AnalysisRecovery::new(AnalysisRecoveryKind::Retry, too_long.clone()).is_err()
        );
        assert!(
            limitation(AnalysisLimitationKind::ProducerFailure)?
                .with_detail(too_long)
                .is_err()
        );
        Ok(())
    }
}
