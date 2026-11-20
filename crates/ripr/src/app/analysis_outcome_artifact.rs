//! Shared validation for the producer-owned diff analysis artifact.
//!
//! Consumers may project this artifact, but they must not reconstruct its
//! completeness or identity from findings, packets, or exit status.

use crate::analysis_outcome::AnalysisOutcome;
use crate::app::CHECK_OUTPUT_SCHEMA_VERSION;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AnalysisOutcomeUnavailableStatus {
    Missing,
    Invalid,
}

impl AnalysisOutcomeUnavailableStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Invalid => "invalid",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AnalysisOutcomeProjection {
    pub(crate) status: &'static str,
    pub(crate) outcome: Value,
    pub(crate) error: Option<String>,
}

pub(crate) fn analysis_outcome_projection(
    analysis_outcome: Option<&AnalysisOutcome>,
    required: bool,
) -> AnalysisOutcomeProjection {
    let Some(analysis_outcome) = analysis_outcome else {
        return if required {
            AnalysisOutcomeProjection {
                status: "missing",
                outcome: Value::Null,
                error: Some(
                    "diff-scoped agent packet requires the producer AnalysisOutcome artifact"
                        .to_string(),
                ),
            }
        } else {
            AnalysisOutcomeProjection {
                status: "not_applicable",
                outcome: Value::Null,
                error: None,
            }
        };
    };

    let outcome_value = match serde_json::to_value(analysis_outcome) {
        Ok(value) => value,
        Err(error) => {
            return AnalysisOutcomeProjection {
                status: "invalid",
                outcome: Value::Null,
                error: Some(format!(
                    "serialize producer AnalysisOutcome failed: {error}"
                )),
            };
        }
    };
    let digest = match analysis_outcome.semantic_digest() {
        Ok(digest) => digest,
        Err(error) => {
            return AnalysisOutcomeProjection {
                status: "invalid",
                outcome: Value::Null,
                error: Some(format!(
                    "compute producer AnalysisOutcome digest failed: {error}"
                )),
            };
        }
    };

    AnalysisOutcomeProjection {
        status: if analysis_outcome.kind.is_complete() {
            "complete"
        } else {
            "incomplete"
        },
        outcome: serde_json::json!({
            "analysis_complete": analysis_outcome.kind.is_complete(),
            "outcome": outcome_value,
            "semantic_digest": digest,
        }),
        error: None,
    }
}

pub(crate) fn unavailable_analysis_outcome_projection(
    status: AnalysisOutcomeUnavailableStatus,
    reason: &str,
) -> AnalysisOutcomeProjection {
    AnalysisOutcomeProjection {
        status: status.as_str(),
        outcome: Value::Null,
        error: Some(reason.to_string()),
    }
}

impl std::fmt::Display for AnalysisOutcomeArtifactError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(reason) | Self::Invalid(reason) => formatter.write_str(reason),
        }
    }
}

pub(crate) fn read_analysis_outcome_artifact_at(
    root: &Path,
    path: &Path,
) -> Result<AnalysisOutcome, AnalysisOutcomeArtifactError> {
    let text = crate::bounded_input::read_to_string(path).map_err(|error| {
        let message = format!(
            "Analysis outcome artifact {} is unavailable: {error}.",
            path.display()
        );
        if error.kind() == std::io::ErrorKind::NotFound {
            AnalysisOutcomeArtifactError::Missing(message)
        } else {
            AnalysisOutcomeArtifactError::Invalid(message)
        }
    })?;
    let value = serde_json::from_str::<Value>(&text).map_err(|error| {
        AnalysisOutcomeArtifactError::Invalid(format!(
            "Analysis outcome artifact {} is malformed JSON: {error}.",
            path.display()
        ))
    })?;
    validate_analysis_outcome_artifact(root, &value)
}

pub(crate) fn analysis_outcome_artifact_path_for_verify(
    verify_path: &Path,
) -> Result<std::path::PathBuf, String> {
    verify_path
        .parent()
        .map(|parent| parent.join("analysis-outcome.json"))
        .ok_or_else(|| "agent verify path has no parent directory; pass the verify JSON as a file path inside a directory".to_string())
}

pub(crate) fn validate_analysis_outcome_artifact(
    root: &Path,
    value: &Value,
) -> Result<AnalysisOutcome, AnalysisOutcomeArtifactError> {
    if value.get("tool").and_then(Value::as_str) != Some("ripr") {
        return Err(invalid(
            "Analysis outcome artifact has an unknown producer tool.",
        ));
    }
    if value.get("schema_version").and_then(Value::as_str) != Some(CHECK_OUTPUT_SCHEMA_VERSION) {
        return Err(invalid(format!(
            "Analysis outcome artifact has unsupported schema_version; expected {}.",
            CHECK_OUTPUT_SCHEMA_VERSION
        )));
    }
    if !recorded_root_matches(root, value.get("root").and_then(Value::as_str)) {
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
    // #9010: the production command builder redirects `ripr check --format
    // json` into this artifact, so a findings-bounded check document can
    // arrive here. It holds only a prefix of the findings and is not a
    // complete analysis outcome, whatever `analysis_complete` says.
    // Both disclosure shapes refuse: the `document_complete` flag (present
    // non-`true` included) and the legacy `run_limitations[]` bound.
    if crate::output::gate::discloses_limited_findings_bound(value)
        || crate::output::gate::discloses_bounded_analysis_document(value)
    {
        return Err(invalid(
            "Analysis outcome artifact is a findings-bounded check document (document_complete is not true or run_limitations names limited_findings_bound); re-run with RIPR_CHECK_FINDINGS_BYTES=0 for the full set.",
        ));
    }
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
    if declared_base.is_some_and(|base| base.starts_with('-')) {
        return Err(invalid(
            "Analysis outcome artifact base must not begin with '-'.",
        ));
    }
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
        // RIPR-SPEC-0116 amendment: a check run on a dirty tree reads the
        // working tree by default and records `head.source: "working_tree"`;
        // re-derive the same diff source it analyzed, or a valid working-tree
        // artifact would be compared with the committed diff.
        let working_tree = value
            .pointer("/head/source")
            .and_then(Value::as_str)
            .is_some_and(|source| source == "working_tree");
        let loaded = if working_tree {
            crate::analysis::load_worktree_diff(root, declared_base, None)
        } else {
            crate::analysis::load_diff(root, declared_base, None, None)
        };
        let diff = loaded.map_err(|error| {
            invalid(format!(
                "Current analysis input could not be established: {error}."
            ))
        })?;
        let digest = Sha256::digest(diff.as_bytes());
        let expected_input_identity = format!("sha256:{}", digest_hex(digest.as_ref()));
        if outcome.identity.input_identity.as_deref() != Some(expected_input_identity.as_str()) {
            return Err(invalid(
                "Analysis outcome artifact input identity does not match the current diff.",
            ));
        }
    }
    Ok(outcome)
}

/// #15239: the artifact records the producer's spelling of the root and the
/// reviewer passes its own, so `--root .` and `--root "$PWD"` name the same
/// directory with different strings. The producer writes `root` through the
/// stable path encoder (`output::path::display_path`, `%` as `%25`), so the
/// recorded text is decoded once with its own decoder before any comparison;
/// the raw encoded text is never treated as a path, since a literal
/// `100%25x` directory and an encoded `100%x` root share one spelling.
/// After decoding, accept the same path, or a spelling that canonicalizes to
/// the same existing directory (a relative spelling resolves against the
/// process working directory, as the CLI resolves `--root`). A missing root,
/// or one that canonicalizes elsewhere, still fails closed.
fn recorded_root_matches(root: &Path, recorded: Option<&str>) -> bool {
    recorded.is_some_and(|recorded| {
        let decoded = crate::analysis::decode_stable_path_text(recorded);
        decoded == root || crate::output::gap_source_subject::same_root(root, &decoded)
    })
}

fn invalid(reason: impl Into<String>) -> AnalysisOutcomeArtifactError {
    AnalysisOutcomeArtifactError::Invalid(reason.into())
}

fn digest_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::loop_commands::WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT;
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
        let diff = crate::analysis::load_diff(&workspace_root()?, Some("HEAD"), None, None)?;
        let digest = Sha256::digest(diff.as_bytes());
        outcome.identity.base_revision = Some("HEAD".to_string());
        outcome.identity.input_identity = Some(format!("sha256:{}", digest_hex(digest.as_ref())));
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
            "base": outcome.identity.base_revision.as_deref(),
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
                validate_analysis_outcome_artifact(&root, &artifact(&outcome, declared_complete)?),
                Ok(outcome)
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_findings_bounded_check_document_but_admits_unbounded_control() -> Result<(), String>
    {
        let outcome = complete_outcome(AnalysisOutcomeKind::CompleteWithFindings)?;
        let root = workspace_root()?;
        let control = artifact(&outcome, true)?;
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &control),
            Ok(outcome.clone())
        );
        let mut shapes = Vec::new();
        for member in [
            serde_json::json!(false),
            serde_json::json!("false"),
            serde_json::Value::Null,
        ] {
            let mut bounded = artifact(&outcome, true)?;
            bounded["analysis_outcome"]["document_complete"] = member;
            shapes.push(bounded);
        }
        // Legacy shape: no document_complete member, run_limitations only.
        let mut legacy = artifact(&outcome, true)?;
        legacy["run_limitations"] = serde_json::json!([
            {"category": "limited_findings_bound", "run_status": "limited_findings_bound"}
        ]);
        shapes.push(legacy);
        for bounded in shapes {
            let result = validate_analysis_outcome_artifact(&root, &bounded);
            assert!(
                matches!(&result, Err(AnalysisOutcomeArtifactError::Invalid(reason))
                    if reason.contains("findings-bounded")),
                "{bounded}: {result:?}"
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
                validate_analysis_outcome_artifact(&root, &artifact(&outcome, false)?),
                Ok(outcome)
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_missing_envelope_and_declared_completeness_disagreement() -> Result<(), String> {
        let outcome = complete_outcome(AnalysisOutcomeKind::CompleteNoFindings)?;
        let root = workspace_root()?;
        let mut missing = artifact(&outcome, true)?;
        missing
            .as_object_mut()
            .ok_or_else(|| "fixture should be an object".to_string())?
            .remove("analysis_outcome");
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &missing),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact is missing the analysis_outcome envelope.".to_string()
            ))
        );
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &artifact(&outcome, false)?,),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact completeness disagrees with its typed outcome."
                    .to_string()
            ))
        );
        // Use an absolute path that cannot accidentally resolve back into the
        // checkout. Hosted CI places `CARGO_TARGET_DIR` outside the checkout,
        // while local Cargo commonly creates an in-tree `target`; using the
        // relative `target` path would therefore exercise different Git
        // discovery behavior in the two environments.
        let invalid_root = std::env::temp_dir().join(format!(
            "ripr-analysis-outcome-invalid-root-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&invalid_root);
        // The artifact names the same (missing) root, so the root check
        // passes and the diff re-derivation is what refuses.
        let mut missing_root_artifact = artifact(&outcome, true)?;
        missing_root_artifact["root"] =
            Value::String(crate::output::path::display_path(&invalid_root));
        assert!(matches!(
            validate_analysis_outcome_artifact(&invalid_root, &missing_root_artifact),
            Err(AnalysisOutcomeArtifactError::Invalid(reason))
                if reason.contains("Current analysis input could not be established")
        ));
        Ok(())
    }

    #[test]
    fn rejects_invalid_producer_metadata_and_identity() -> Result<(), String> {
        let outcome = complete_outcome(AnalysisOutcomeKind::CompleteNoFindings)?;
        let root = workspace_root()?;

        let mut unknown_tool = artifact(&outcome, true)?;
        unknown_tool["tool"] = Value::String("other".to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &unknown_tool),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact has an unknown producer tool.".to_string()
            ))
        );

        let mut wrong_schema = artifact(&outcome, true)?;
        wrong_schema["schema_version"] = Value::String("0.1".to_string());
        assert!(matches!(
            validate_analysis_outcome_artifact(&root, &wrong_schema),
            Err(AnalysisOutcomeArtifactError::Invalid(reason))
                if reason.contains("unsupported schema_version")
        ));

        let mut wrong_root = artifact(&outcome, true)?;
        wrong_root["root"] = Value::String("other-root".to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &wrong_root),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact root does not match the review root.".to_string()
            ))
        );

        let mut missing_complete = artifact(&outcome, true)?;
        missing_complete["analysis_outcome"]
            .as_object_mut()
            .ok_or_else(|| "expected outcome envelope".to_string())?
            .remove("analysis_complete");
        assert!(matches!(
            validate_analysis_outcome_artifact(&root, &missing_complete),
            Err(AnalysisOutcomeArtifactError::Invalid(reason))
                if reason.contains("missing boolean analysis_complete")
        ));

        let mut missing_outcome = artifact(&outcome, true)?;
        missing_outcome["analysis_outcome"]
            .as_object_mut()
            .ok_or_else(|| "expected outcome envelope".to_string())?
            .remove("outcome");
        assert!(matches!(
            validate_analysis_outcome_artifact(&root, &missing_outcome),
            Err(AnalysisOutcomeArtifactError::Invalid(reason))
                if reason.contains("missing the typed outcome")
        ));

        let mut invalid_typed = artifact(&outcome, true)?;
        invalid_typed["analysis_outcome"]["outcome"]["kind"] =
            Value::String("not-a-kind".to_string());
        assert!(matches!(
            validate_analysis_outcome_artifact(&root, &invalid_typed),
            Err(AnalysisOutcomeArtifactError::Invalid(reason))
                if reason.contains("invalid typed outcome")
        ));

        let mut wrong_base = artifact(&outcome, true)?;
        wrong_base["base"] = Value::String("origin/main".to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &wrong_base),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact base does not match its typed identity.".to_string()
            ))
        );

        let mut option_like_base = artifact(&outcome, true)?;
        option_like_base["base"] = Value::String("--output".to_string());
        option_like_base["analysis_outcome"]["outcome"]["identity"]["base_revision"] =
            Value::String("--output".to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &option_like_base),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact base must not begin with '-'.".to_string()
            ))
        );

        let mut wrong_identity = artifact(&outcome, true)?;
        wrong_identity["analysis_outcome"]["outcome"]["identity"]["input_identity"] =
            Value::String("sha256:wrong".to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(&root, &wrong_identity),
            Err(AnalysisOutcomeArtifactError::Invalid(
                "Analysis outcome artifact input identity does not match the current diff."
                    .to_string()
            ))
        );
        Ok(())
    }

    /// RIPR-SPEC-0116 amendment: an artifact whose `head.source` is
    /// `working_tree` is checked against the working-tree diff, and any other
    /// artifact against the committed diff. The fixture's two diffs differ
    /// (an uncommitted edit on a clean `HEAD`), so each identity is accepted
    /// only under its own head source.
    #[test]
    fn validates_working_tree_head_source_against_the_working_tree_diff() -> Result<(), String> {
        use crate::testing::fixture_git::{fixture_git_ok, remove_fixture_tree};
        let root = std::env::temp_dir().join(format!(
            "ripr-analysis-outcome-working-tree-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("clock before epoch: {error}"))?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create fixture: {error}"))?;
        fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
        fixture_git_ok(&root, &["config", "user.email", "test@test.com"])?;
        fixture_git_ok(&root, &["config", "user.name", "Test"])?;
        std::fs::write(root.join("src/lib.rs"), "pub fn f() -> i32 { 1 }\n")
            .map_err(|error| format!("write lib.rs: {error}"))?;
        fixture_git_ok(&root, &["add", "."])?;
        fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
        std::fs::write(root.join("src/lib.rs"), "pub fn f() -> i32 { 2 }\n")
            .map_err(|error| format!("write dirty lib.rs: {error}"))?;
        let identity_of = |diff: String| {
            let digest = Sha256::digest(diff.as_bytes());
            format!("sha256:{}", digest_hex(digest.as_ref()))
        };
        let working_tree_identity = identity_of(crate::analysis::load_worktree_diff(
            &root,
            Some("HEAD"),
            None,
        )?);
        let committed_identity =
            identity_of(crate::analysis::load_diff(&root, Some("HEAD"), None, None)?);
        assert_ne!(
            working_tree_identity, committed_identity,
            "fixture precondition: the working-tree and committed diffs must differ"
        );
        let root_display = root.display().to_string();
        let artifact_for = |identity: &str, head_source: Option<&str>| -> Result<Value, String> {
            let mut outcome = complete_outcome(AnalysisOutcomeKind::CompleteNoFindings)?;
            outcome.identity.input_identity = Some(identity.to_string());
            let mut value = artifact(&outcome, true)?;
            value["root"] = Value::String(root_display.clone());
            if let Some(source) = head_source {
                value["head"] = serde_json::json!({ "source": source });
            }
            Ok(value)
        };
        let mismatch = Err(AnalysisOutcomeArtifactError::Invalid(
            "Analysis outcome artifact input identity does not match the current diff.".to_string(),
        ));
        let accepted = validate_analysis_outcome_artifact(
            &root,
            &artifact_for(&working_tree_identity, Some("working_tree"))?,
        );
        let committed_accepted = validate_analysis_outcome_artifact(
            &root,
            &artifact_for(&committed_identity, Some("commit"))?,
        );
        let working_tree_identity_as_commit = validate_analysis_outcome_artifact(
            &root,
            &artifact_for(&working_tree_identity, Some("commit"))?,
        );
        let committed_identity_as_working_tree = validate_analysis_outcome_artifact(
            &root,
            &artifact_for(&committed_identity, Some("working_tree"))?,
        );
        let working_tree_identity_without_head =
            validate_analysis_outcome_artifact(&root, &artifact_for(&working_tree_identity, None)?);
        remove_fixture_tree(&root)?;
        assert!(accepted.is_ok(), "{accepted:?}");
        assert!(committed_accepted.is_ok(), "{committed_accepted:?}");
        assert_eq!(working_tree_identity_as_commit, mismatch);
        assert_eq!(committed_identity_as_working_tree, mismatch);
        assert_eq!(working_tree_identity_without_head, mismatch);
        Ok(())
    }

    #[test]
    fn decodes_the_encoded_root_once_and_keeps_percent_named_workspaces_apart() -> Result<(), String>
    {
        // #15239: `ripr check --format json` writes `root` through the stable
        // path encoder, which spells a literal `%` as `%25`. Two distinct
        // workspaces, `r_100%x` and a literal `r_100%25x`, therefore record
        // `r_100%25x` and `r_100%2525x`. Each must accept its own artifact
        // and refuse the other's: matching the raw recorded text as a path
        // would let the literal `r_100%25x` workspace accept the `r_100%x`
        // artifact.
        let base = std::env::temp_dir().join(format!(
            "ripr-analysis-outcome-percent-root-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let encoded_looking = base.join("r_100%x");
        let literal = base.join("r_100%25x");
        for dir in [&encoded_looking, &literal] {
            std::fs::create_dir_all(dir).map_err(|error| format!("create {dir:?}: {error}"))?;
        }
        let outcome = AnalysisOutcome::new(
            AnalysisOutcomeKind::NoScope,
            AnalysisIdentity::default(),
            AnalysisOutcomeCounts::default(),
            Vec::new(),
        )?;
        let written_for = |root: &Path| -> Value {
            serde_json::json!({
                "schema_version": "0.2",
                "tool": "ripr",
                "root": crate::output::path::display_path(root),
                "base": null,
                "analysis_outcome": {
                    "analysis_complete": true,
                    "outcome": outcome
                }
            })
        };
        let encoded_artifact = written_for(&encoded_looking);
        let literal_artifact = written_for(&literal);
        let fixture = (
            encoded_artifact["root"].as_str().map(str::to_string),
            literal_artifact["root"].as_str().map(str::to_string),
        );
        let own_encoded = validate_analysis_outcome_artifact(&encoded_looking, &encoded_artifact);
        let own_literal = validate_analysis_outcome_artifact(&literal, &literal_artifact);
        let literal_reads_encoded = validate_analysis_outcome_artifact(&literal, &encoded_artifact);
        let encoded_reads_literal =
            validate_analysis_outcome_artifact(&encoded_looking, &literal_artifact);
        let _ = std::fs::remove_dir_all(&base);

        assert_eq!(
            fixture,
            (
                Some(format!(
                    "{}/r_100%25x",
                    crate::output::path::display_path(&base)
                )),
                Some(format!(
                    "{}/r_100%2525x",
                    crate::output::path::display_path(&base)
                )),
            ),
            "fixture precondition: the producer encodes each literal `%`"
        );
        assert_eq!(own_encoded, Ok(outcome.clone()));
        assert_eq!(own_literal, Ok(outcome));
        let mismatch = Err(AnalysisOutcomeArtifactError::Invalid(
            "Analysis outcome artifact root does not match the review root.".to_string(),
        ));
        assert_eq!(literal_reads_encoded, mismatch);
        assert_eq!(encoded_reads_literal, mismatch);
        Ok(())
    }

    #[test]
    fn accepts_differently_spelled_same_root_and_refuses_another_directory() -> Result<(), String> {
        // #15239: the artifact records an absolute root while the reviewer
        // passes a relative one (`--root .` in the CLI). Lib tests run with
        // the crate manifest directory as the working directory, so `../..`
        // is a relative spelling of the workspace root.
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let cwd = std::env::current_dir().map_err(|error| format!("current dir: {error}"))?;
        assert_eq!(
            std::fs::canonicalize(&cwd).map_err(|error| format!("canonicalize cwd: {error}"))?,
            std::fs::canonicalize(manifest)
                .map_err(|error| format!("canonicalize manifest: {error}"))?,
            "test precondition: lib tests run from the crate manifest directory"
        );
        let outcome = complete_outcome(AnalysisOutcomeKind::CompleteNoFindings)?;
        let absolute = workspace_root()?;
        let absolute_display = absolute.display().to_string();
        let relative = Path::new("../..");
        let absolute_recorded = artifact(&outcome, true)?;
        assert_eq!(
            absolute_recorded["root"],
            Value::String(absolute_display.clone()),
            "fixture records the absolute root"
        );

        // Absolute recording, relative review root.
        assert_eq!(
            validate_analysis_outcome_artifact(relative, &absolute_recorded),
            Ok(outcome.clone())
        );
        // Relative recording, absolute review root.
        let mut relative_recorded = absolute_recorded.clone();
        relative_recorded["root"] = Value::String("../..".to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(&absolute, &relative_recorded),
            Ok(outcome.clone())
        );
        // A non-normalized absolute spelling of the same directory.
        let mut dotted_recorded = absolute_recorded.clone();
        dotted_recorded["root"] =
            Value::String(absolute.join("crates").join("..").display().to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(&absolute, &dotted_recorded),
            Ok(outcome.clone())
        );

        // Another existing directory still fails closed, in either spelling.
        let mismatch = Err(AnalysisOutcomeArtifactError::Invalid(
            "Analysis outcome artifact root does not match the review root.".to_string(),
        ));
        let mut other_absolute = absolute_recorded.clone();
        other_absolute["root"] = Value::String(manifest.display().to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(relative, &other_absolute),
            mismatch
        );
        let mut other_relative = absolute_recorded.clone();
        other_relative["root"] = Value::String(".".to_string());
        assert_eq!(
            validate_analysis_outcome_artifact(&absolute, &other_relative),
            mismatch
        );
        // A missing root member fails closed.
        let mut no_root = absolute_recorded;
        if let Some(object) = no_root.as_object_mut() {
            object.remove("root");
        }
        assert_eq!(
            validate_analysis_outcome_artifact(&absolute, &no_root),
            mismatch
        );
        Ok(())
    }

    #[test]
    fn classifies_non_not_found_artifact_io_as_invalid() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-analysis-outcome-io-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("clock before epoch: {error}"))?
                .as_nanos()
        ));
        let artifact_path = root.join(WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT);
        std::fs::create_dir_all(&artifact_path)
            .map_err(|error| format!("create directory artifact: {error}"))?;
        let result = read_analysis_outcome_artifact_at(&root, &artifact_path);
        std::fs::remove_dir_all(&root).map_err(|error| format!("remove test root: {error}"))?;
        assert!(matches!(
            result,
            Err(AnalysisOutcomeArtifactError::Invalid(reason))
                if reason.contains("is unavailable")
        ));
        Ok(())
    }

    #[test]
    fn derives_analysis_outcome_path_from_custom_verify_directory() -> Result<(), String> {
        assert_eq!(
            analysis_outcome_artifact_path_for_verify(Path::new(
                "target/custom/agent-verify.json"
            ))?,
            Path::new("target/custom/analysis-outcome.json")
        );
        Ok(())
    }
}
