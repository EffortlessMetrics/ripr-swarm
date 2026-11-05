//! Keep captured repeat evidence separate from comparison credit.

use std::path::Path;

use serde_json::{Value, json};

use super::{
    AnalysisPass, Materialization, RAW_DIR, ReadyContext, RowInputs, RunStatus, StabilityResult,
    SubjectExecution, analysis_limitations, assemble_row, binary_identity_drift, compare_passes,
    corpus_selection_state, infrastructure_failure_execution, sha256_hex, write_repeat_raw_output,
};

/// A comparison result plus any captured output it cannot represent. Complete
/// repeats keep their bytes in `Compared`; non-complete repeats retain them here
/// before the comparison owner consumes the pass.
pub(super) struct StabilityAttempt {
    pub(super) result: StabilityResult,
    performed: bool,
    not_compared: Option<CapturedRepeat>,
}

struct CapturedRepeat {
    status: RunStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl StabilityAttempt {
    pub(super) fn not_started(limitation: String) -> Self {
        Self {
            result: StabilityResult::NotCompared { limitation },
            performed: false,
            not_compared: None,
        }
    }

    /// Capture can fail after spawning, so this records an attempted route
    /// with unavailable bytes, not proof that no child executed.
    pub(super) fn capture_failed(limitation: String) -> Self {
        Self {
            result: StabilityResult::NotCompared { limitation },
            performed: true,
            not_compared: None,
        }
    }

    pub(super) fn captured(first: &AnalysisPass, mut second: AnalysisPass) -> Self {
        let not_compared = if matches!(second.status, RunStatus::Complete) {
            None
        } else {
            Some(CapturedRepeat {
                status: second.status,
                stdout: std::mem::take(&mut second.stdout),
                stderr: std::mem::take(&mut second.stderr),
            })
        };
        Self {
            result: compare_passes(first, second),
            performed: true,
            not_compared,
        }
    }

    /// Return diagnostic identities only after BOTH streams were retained.
    /// Compared repeats preserve their existing candidate-row binding.
    fn retain_output(
        &self,
        staging: &Path,
        subject_id: &str,
    ) -> Result<Option<RetainedRepeat>, String> {
        if let Some(captured) = &self.not_compared {
            write_repeat_raw_output(staging, subject_id, &captured.stdout, &captured.stderr)?;
            return Ok(Some(RetainedRepeat {
                status: captured.status,
                stdout_sha256: sha256_hex(&captured.stdout),
                stderr_sha256: sha256_hex(&captured.stderr),
            }));
        }
        if let StabilityResult::Compared {
            repeat_stdout,
            repeat_stderr,
            ..
        } = &self.result
        {
            write_repeat_raw_output(staging, subject_id, repeat_stdout, repeat_stderr)?;
        }
        Ok(None)
    }
}

/// Diagnostic identity of a retained non-complete repeat. This is not a
/// candidate-row comparison and does not enter accepted-receipt promotion.
pub(super) struct RetainedRepeat {
    status: RunStatus,
    stdout_sha256: String,
    stderr_sha256: String,
}

impl RetainedRepeat {
    pub(super) fn to_json(&self, subject_id: &str) -> Value {
        json!({
            "status": self.status.as_str(),
            "stdout": {
                "path": format!("{RAW_DIR}/{subject_id}/pass2-stdout.txt"),
                "sha256": self.stdout_sha256,
            },
            "stderr": {
                "path": format!("{RAW_DIR}/{subject_id}/pass2-stderr.txt"),
                "sha256": self.stderr_sha256,
            },
        })
    }
}

/// Finalize a captured first pass whose raw evidence has already been written.
/// Repeat retention can withdraw comparison evidence, but cannot erase the
/// retained first analysis from the run denominator and improve the gate.
pub(super) fn finish_subject_analysis(
    context: ReadyContext,
    first: AnalysisPass,
    stability: Option<StabilityAttempt>,
    staging: &Path,
) -> SubjectExecution {
    let (retained_repeat, retention_failure) = match stability.as_ref() {
        Some(attempt) => match attempt.retain_output(staging, &context.subject.id) {
            Ok(retained) => (retained, None),
            Err(error) => (
                None,
                Some(format!(
                    "the repeat pass ran but its raw evidence could not be retained under the candidate tree: {error}"
                )),
            ),
        },
        None => (None, None),
    };

    // Retention failure must not bypass the existing binary re-verification:
    // retained bytes cannot be attributed to an unknown or rebuilt analyzer.
    if let Some(drift) = binary_identity_drift(context.binary) {
        return infrastructure_failure_execution(context, drift);
    }

    let result = stability.as_ref().map(|attempt| &attempt.result);
    let comparison = if retention_failure.is_none() {
        result
    } else {
        None
    };
    let row = assemble_row(RowInputs {
        subject: context.subject,
        binary: context.binary,
        diff_portable: context.diff_portable,
        diff_digest: context.diff_digest,
        materialization: &Materialization::Materialized {
            dir: context.dir,
            source: context.source,
        },
        first: Some(&first),
        stability: comparison,
        corpus: context.corpus,
        config_profile: &context.config_profile,
    });
    let (mut stability_limitation, mut output_identity_stable) = match result {
        Some(StabilityResult::Compared {
            output_identity_stable,
            ..
        }) => (None, Some(*output_identity_stable)),
        Some(StabilityResult::NotCompared { limitation }) => (Some(limitation.clone()), None),
        None => (
            Some(format!(
                "first pass ended in status `{}`; a stability comparison needs a complete first pass",
                first.status.as_str()
            )),
            None,
        ),
    };
    if let Some(error) = &retention_failure {
        stability_limitation = Some(match stability_limitation {
            Some(limitation) => format!("{limitation}; {error}"),
            None => error.clone(),
        });
        output_identity_stable = None;
    }
    SubjectExecution {
        subject_id: context.subject.id.clone(),
        row,
        materialization_state: "materialized",
        materialization_source: Some(context.source),
        materialization_limitation: None,
        analysis_complete: Some(first.complete),
        analysis_limitations: analysis_limitations(&first),
        stability_performed: stability.as_ref().is_some_and(|attempt| attempt.performed),
        stability_limitation,
        output_identity_stable,
        retained_repeat,
        config_profile: context.config_profile,
        subject_config_path: context.subject_config_path,
        corpus_state: corpus_selection_state(context.corpus),
        corpus_limitation: context.corpus.and_then(|counts| counts.limitation.clone()),
        infrastructure_limitation: retention_failure,
    }
}
