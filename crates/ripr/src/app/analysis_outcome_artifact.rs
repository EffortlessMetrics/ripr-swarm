//! Shared validation for the producer-owned diff analysis artifact.
//!
//! Consumers may project this artifact, but they must not reconstruct its
//! completeness or identity from findings, packets, or exit status.

use crate::agent::loop_commands::WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT;
use crate::analysis_outcome::AnalysisOutcome;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AnalysisOutcomeArtifactError {
    Missing(String),
    Invalid(String),
}

impl AnalysisOutcomeArtifactError {
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Missing(_) => "missing",
            Self::Invalid(_) => "invalid",
        }
    }
}

impl std::fmt::Display for AnalysisOutcomeArtifactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(reason) | Self::Invalid(reason) => formatter.write_str(reason),
        }
    }
}

pub(crate) fn read_analysis_outcome_artifact(
    root: &Path,
    root_display: &str,
) -> Result<AnalysisOutcome, AnalysisOutcomeArtifactError> {
    let path = root.join(WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT);
    let text = std::fs::read_to_string(&path).map_err(|error| {
        AnalysisOutcomeArtifactError::Missing(format!(
            "Analysis outcome artifact {} is unavailable: {error}.",
            path.display()
        ))
    })?;
    let value = serde_json::from_str::<Value>(&text).map_err(|error| {
        AnalysisOutcomeArtifactError::Invalid(format!(
            "Analysis outcome artifact {} is malformed JSON: {error}.",
            path.display()
        ))
    })?;
    validate_analysis_outcome_artifact(root, root_display, &value)
}

pub(crate) fn validate_analysis_outcome_artifact(
    root: &Path,
    root_display: &str,
    value: &Value,
) -> Result<AnalysisOutcome, AnalysisOutcomeArtifactError> {
    if value.get("tool").and_then(Value::as_str) != Some("ripr") {
        return Err(invalid(
            "Analysis outcome artifact has an unknown producer tool.",
        ));
    }
    if value.get("root").and_then(Value::as_str) != Some(root_display) {
        return Err(invalid(
            "Analysis outcome artifact root does not match the review root.",
        ));
    }
    let Some(envelope) = value.get("analysis_outcome") else {
        return Err(invalid(
            "Analysis outcome artifact is missing the analysis_outcome envelope.",
        ));
    };
    let Some(declared_complete) = envelope.get("analysis_complete").and_then(Value::as_bool) else {
        return Err(invalid(
            "Analysis outcome artifact is missing boolean analysis_complete.",
        ));
    };
    let Some(outcome) = envelope.get("outcome") else {
        return Err(invalid(
            "Analysis outcome artifact is missing the typed outcome.",
        ));
    };
    let outcome = serde_json::from_value::<AnalysisOutcome>(outcome.clone()).map_err(|error| {
        invalid(format!(
            "Analysis outcome artifact has an invalid typed outcome: {error}."
        ))
    })?;
    if declared_complete != outcome.kind.is_complete() {
        return Err(invalid(
            "Analysis outcome artifact completeness disagrees with its typed outcome.",
        ));
    }
    let declared_base = value.get("base").and_then(Value::as_str);
    if outcome.identity.base_revision.as_deref() != declared_base {
        return Err(invalid(
            "Analysis outcome artifact base does not match its typed identity.",
        ));
    }
    if outcome.kind.is_complete()
        && !matches!(
            outcome.kind,
            crate::analysis_outcome::AnalysisOutcomeKind::NoScope
        )
    {
        if !root.join(".git").exists() {
            return Err(invalid(
                "Current analysis input could not be established: repository metadata is unavailable.",
            ));
        }
        let diff =
            crate::analysis::load_diff(root, declared_base, None, None).map_err(|error| {
                invalid(format!(
                    "Current analysis input could not be established: {error}."
                ))
            })?;
        let digest = Sha256::digest(diff.as_bytes());
        let expected_input_identity = format!("sha256:{digest:x}");
        if outcome.identity.input_identity.as_deref() != Some(expected_input_identity.as_str()) {
            return Err(invalid(
                "Analysis outcome artifact input identity does not match the current diff.",
            ));
        }
    }
    Ok(outcome)
}

fn invalid(reason: impl Into<String>) -> AnalysisOutcomeArtifactError {
    AnalysisOutcomeArtifactError::Invalid(reason.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis_outcome::{
        AnalysisIdentity, AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcomeCounts,
        AnalysisOutcomeKind, AnalysisRecovery, AnalysisRecoveryKind, AnalysisStage,
    };

    fn workspace_root() -> Result<std::path::PathBuf, String> {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .ok_or_else(|| "expected workspace root above crate manifest".to_string())
    }

    fn complete_outcome(kind: AnalysisOutcomeKind) -> Result<AnalysisOutcome, String> {
        let mut outcome = AnalysisOutcome::new(
            kind,
            AnalysisIdentity::default(),
            AnalysisOutcomeCounts {
                changed_file_count: 1,
                changed_line_count: 1,
                candidate_line_count: 1,
                probe_count: 1,
                finding_count: if kind == AnalysisOutcomeKind::CompleteWithFindings {
                    1
                } else {
                    0
                },
            },
            Vec::new(),
        )?;
        let diff = crate::analysis::load_diff(&workspace_root()?, None, None, None)?;
        let digest = Sha256::digest(diff.as_bytes());
        outcome.identity.input_identity = Some(format!("sha256:{digest:x}"));
        Ok(outcome)
    }

    fn incomplete_outcome(kind: AnalysisOutcomeKind) -> Result<AnalysisOutcome, String> {
        AnalysisOutcome::new(
            kind,
            AnalysisIdentity::default(),
            AnalysisOutcomeCounts {
                changed_file_count: 1,
                changed_line_count: 1,
                candidate_line_count: 1,
                probe_count: 0,
                finding_count: 0,
            },
            vec![AnalysisLimitation::new(
                AnalysisLimitationKind::ProducerFailure,
                AnalysisStage::AnalysisPipeline,
                AnalysisRecovery::new(AnalysisRecoveryKind::InspectFailure, "inspect the failure")?,
            )],
        )
    }

    fn artifact(outcome: &AnalysisOutcome, declared_complete: bool) -> Result<Value, String> {
        let root = workspace_root()?;
        Ok(serde_json::json!({
            "schema_version": "0.2",
            "tool": "ripr",
            "root": root.display().to_string(),
            "base": null,
            "analysis_outcome": {
                "analysis_complete": declared_complete,
                "outcome": outcome
            }
        }))
    }

    #[test]
    fn validates_complete_findings_and_zero_outcomes_without_count_inference() -> Result<(), String>
    {
        for outcome in [
            complete_outcome(AnalysisOutcomeKind::CompleteWithFindings)?,
            complete_outcome(AnalysisOutcomeKind::CompleteNoFindings)?,
        ] {
            let root = workspace_root()?;
            let declared_complete = outcome.kind.is_complete();
            assert_eq!(
                validate_analysis_outcome_artifact(
                    &root,
                    &root.display().to_string(),
                    &artifact(&outcome, declared_complete)?
                ),
                Ok(outcome)
            );
        }
        Ok(())
    }

    #[test]
    fn preserves_incomplete_and_unsupported_kinds() -> Result<(), String> {
        for kind in [
            AnalysisOutcomeKind::PartialWithLimitations,
            AnalysisOutcomeKind::UnsupportedInput,
        ] {
            let outcome = incomplete_outcome(kind)?;
            let root = workspace_root()?;
            assert_eq!(
                validate_analysis_outcome_artifact(
                    &root,
                    &root.display().to_string(),
                    &artifact(&outcome, false)?
                ),
                Ok(outcome)
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_missing_envelope_and_declared_completeness_disagreement() -> Result<(), String> {
        let outcome = complete_outcome(AnalysisOutcomeKind::CompleteNoFindings)?;
        let root = workspace_root()?;
        let root_display = root.display().to_string();
        let mut missing = artifact(&outcome, true)?;
        missing
            .as_object_mut()
            .ok_or_else(|| "fixture should be an object".to_string())?
            .remove("analysis_outcome");
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &root_display, &missing),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact is missing the analysis_outcome envelope.".to_string()
            ))
        );
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &root_display, &artifact(&outcome, false)?,),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact completeness disagrees with its typed outcome."
                    .to_string()
            ))
        );
        assert_eq!(
            validate_analysis_outcome_artifact(
                Path::new("target"),
                &root_display,
                &artifact(&outcome, true)?,
            ),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Current analysis input could not be established: repository metadata is unavailable."
                    .to_string()
            ))
        );
        Ok(())
    }
}
