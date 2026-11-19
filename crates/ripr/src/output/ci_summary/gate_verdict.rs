//! The gate's failing verdict, stated where a pull request author looks
//! (#14081).
//!
//! A blocking decision, a configuration error, and an unknown
//! `RIPR_GATE_MODE` all fail a job whose mode is not advisory. The cause used
//! to be printed only inside the collapsed `Evaluate RIPR gate decision` log
//! group: the failing annotation carried the child command line and an exit
//! code, and the job summary opened as an advisory summary with the decision
//! near its end. This module reads the written `gate-decision.json`, or the
//! raw mode when no decision exists, and words the one verdict both surfaces
//! print. It reads only. The decision, its status tokens, and the accepted
//! modes stay with `output::gate`; a passing, advisory, or acknowledged
//! decision yields nothing, so those summaries are unchanged.

use crate::output::gate::{GATE_STATUS_BLOCKED, GATE_STATUS_CONFIG_ERROR, GateMode};
use crate::output::markdown::code_span;
use serde_json::Value;

/// Where the variable is edited, for the two surfaces that name it.
const VARIABLE_ROUTE: &str = "Settings > Secrets and variables > Actions > Variables";

/// Config errors listed in the summary before the rest are counted.
const MAX_CONFIG_ERRORS: usize = 3;

/// A failing gate verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GateVerdict {
    /// A completed evaluation that blocks.
    Blocked {
        mode: String,
        blocking: u64,
        /// The first blocking decision's reason and place, when listed.
        first: Option<(String, Option<String>)>,
        /// Blocking exception-policy violations: their count and the first
        /// one as `kind: detail`. A gate can block on these with no blocking
        /// gap, so they are the stated cause when no gap blocks.
        exception: Option<(u64, String)>,
    },
    /// An evaluation that could not complete.
    ConfigError { mode: String, errors: Vec<String> },
    /// `RIPR_GATE_MODE` is set to something the gate does not accept, so no
    /// decision was written. Carries the gate's own refusal.
    UnknownMode { refusal: String },
}

impl GateVerdict {
    /// The verdict a written decision document states, if it fails the gate
    /// for one of the two reasons this module words. Any other status,
    /// including an unknown one, yields `None`: the caller keeps its own
    /// handling rather than have this module guess a cause.
    pub(crate) fn from_decision(decision: &Value) -> Option<Self> {
        let mode = decision["mode"].as_str().unwrap_or("unknown").to_string();
        match decision["status"].as_str()? {
            GATE_STATUS_BLOCKED => {
                let blocking_decisions: Vec<&Value> = decision["decisions"]
                    .as_array()
                    .map(|decisions| {
                        decisions
                            .iter()
                            .filter(|item| item["decision"].as_str() == Some("blocking"))
                            .collect()
                    })
                    .unwrap_or_default();
                // The producer states the count and lists the decisions;
                // trust the larger so a missing count never reads as zero.
                let blocking = decision["summary"]["blocking"]
                    .as_u64()
                    .unwrap_or(0)
                    .max(blocking_decisions.len() as u64);
                let first = blocking_decisions.first().and_then(|item| {
                    let reason = item["gate_reason"].as_str()?.to_string();
                    let place = match (
                        item["placement"]["path"].as_str(),
                        item["placement"]["line"].as_u64(),
                    ) {
                        (Some(path), Some(line)) => Some(format!("{path}:{line}")),
                        (Some(path), None) => Some(path.to_string()),
                        _ => None,
                    };
                    Some((reason, place))
                });
                let violations: Vec<&Value> = decision["exception_policy"]["violations"]
                    .as_array()
                    .map(|violations| {
                        violations
                            .iter()
                            .filter(|item| item["blocking"].as_bool() == Some(true))
                            .collect()
                    })
                    .unwrap_or_default();
                let exception = violations.first().map(|item| {
                    (
                        violations.len() as u64,
                        format!(
                            "{}: {}",
                            item["kind"].as_str().unwrap_or("violation"),
                            item["detail"].as_str().unwrap_or("no detail recorded")
                        ),
                    )
                });
                Some(Self::Blocked {
                    mode,
                    blocking,
                    first,
                    exception,
                })
            }
            GATE_STATUS_CONFIG_ERROR => {
                let errors = decision["config_errors"]
                    .as_array()
                    .map(|errors| {
                        errors
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                Some(Self::ConfigError { mode, errors })
            }
            _ => None,
        }
    }

    /// The verdict for a run with no decision document: only a set mode the
    /// gate refuses is a verdict. An empty mode is the advisory default and
    /// an accepted mode without a decision has some other cause.
    pub(crate) fn from_mode(raw_mode: &str) -> Option<Self> {
        if raw_mode.is_empty() {
            return None;
        }
        GateMode::parse(raw_mode)
            .err()
            .map(|refusal| Self::UnknownMode { refusal })
    }

    /// One line for the failing step's `::error` annotation.
    pub(crate) fn annotation(&self) -> String {
        match self {
            Self::Blocked {
                mode,
                blocking,
                first,
                exception,
            } => {
                let mut text = format!(
                    "RIPR gate blocked this pull request under {mode} mode: {}",
                    blocked_by(*blocking, exception.as_ref())
                );
                match (first, exception) {
                    (Some((reason, place)), _) => {
                        text.push_str(&format!("; first: {reason}"));
                        if let Some(place) = place {
                            text.push_str(&format!(" [{place}]"));
                        }
                    }
                    (None, Some((_, violation))) if *blocking == 0 => {
                        text.push_str(&format!("; first: {violation}"));
                    }
                    _ => {}
                }
                text.push_str(
                    ". See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact.",
                );
                text
            }
            Self::ConfigError { mode, errors } => match errors.first() {
                Some(first) => format!(
                    "RIPR gate could not evaluate this pull request under {mode} mode (configuration error, not a finding about the code): {first}"
                ),
                None => format!(
                    "RIPR gate could not evaluate this pull request under {mode} mode (configuration error, not a finding about the code); see gate-decision.md in the ripr-reports artifact."
                ),
            },
            Self::UnknownMode { refusal } => format!(
                "RIPR gate did not run: {refusal} Fix the RIPR_GATE_MODE repository variable ({VARIABLE_ROUTE}), or unset it to stay advisory, then re-run the job."
            ),
        }
    }

    /// The job summary block, without a trailing blank line.
    pub(crate) fn summary_lines(&self) -> Vec<String> {
        match self {
            Self::Blocked {
                mode,
                blocking,
                first,
                exception,
            } => {
                let mut lines = vec![
                    "### RIPR gate: blocked".to_string(),
                    format!(
                        "**The RIPR gate blocked this pull request** under {} mode: {}.",
                        code_span(mode),
                        blocked_by(*blocking, exception.as_ref())
                    ),
                ];
                if let Some((_, violation)) = exception {
                    lines.push(format!(
                        "- First blocking exception-policy violation: {}",
                        code_span(violation)
                    ));
                }
                if let Some((reason, place)) = first {
                    lines.push(match place {
                        Some(place) => format!(
                            "- First blocking reason: {} at {}",
                            code_span(reason),
                            code_span(place)
                        ),
                        None => format!("- First blocking reason: {}", code_span(reason)),
                    });
                }
                lines.push("- Detail: the `Gate decision` section below, and `gate-decision.md` in the `ripr-reports` artifact.".to_string());
                lines
            }
            Self::ConfigError { mode, errors } => {
                let mut lines = vec![
                    "### RIPR gate: configuration error".to_string(),
                    format!(
                        "**The RIPR gate could not evaluate this pull request** under {} mode. This is a setup problem, not a finding about the code.",
                        code_span(mode)
                    ),
                ];
                for error in errors.iter().take(MAX_CONFIG_ERRORS) {
                    lines.push(format!("- Config error: {}", code_span(error)));
                }
                if errors.len() > MAX_CONFIG_ERRORS {
                    lines.push(format!(
                        "- {} more in `gate-decision.md` in the `ripr-reports` artifact.",
                        errors.len() - MAX_CONFIG_ERRORS
                    ));
                }
                if errors.is_empty() {
                    lines.push("- Detail: `gate-decision.md` in the `ripr-reports` artifact.".to_string());
                }
                lines
            }
            Self::UnknownMode { refusal } => vec![
                "### RIPR gate: unknown `RIPR_GATE_MODE`".to_string(),
                "**The RIPR gate did not run**, because `RIPR_GATE_MODE` is not a gate mode. This is a setup problem, not a finding about the code.".to_string(),
                format!("- Gate message: {}", code_span(refusal)),
                format!("- Next step: fix the `RIPR_GATE_MODE` repository variable ({VARIABLE_ROUTE}), or unset it to stay advisory, then re-run the job."),
            ],
        }
    }
}

/// What blocks: the gaps, the exception-policy violations, or both. With
/// neither listed the gap count is still stated, so the line never claims a
/// cause the document does not carry.
fn blocked_by(blocking: u64, exception: Option<&(u64, String)>) -> String {
    let violations = |count: u64| {
        if count == 1 {
            "1 blocking exception-policy violation".to_string()
        } else {
            format!("{count} blocking exception-policy violations")
        }
    };
    match exception {
        Some((count, _)) if blocking == 0 => violations(*count),
        Some((count, _)) => format!("{} and {}", gaps(blocking), violations(*count)),
        None => gaps(blocking),
    }
}

fn gaps(count: u64) -> String {
    if count == 1 {
        "1 blocking gap".to_string()
    } else {
        format!("{count} blocking gaps")
    }
}

/// The `Gate decision` section's line when no decision report exists.
pub(crate) fn missing_decision_line(raw_mode: &str) -> String {
    if raw_mode.is_empty() {
        return "Gate decision was not run. Set `RIPR_GATE_MODE` to `visible-only`, `acknowledgeable`, `baseline-check`, or `calibrated-gate` to opt in.".to_string();
    }
    match GateMode::parse(raw_mode) {
        Err(_) => format!(
            "Gate decision was not run: `RIPR_GATE_MODE` is set to {}, which is not a gate mode. Set it to `visible-only`, `acknowledgeable`, `baseline-check`, or `calibrated-gate`, or unset it to stay advisory.",
            code_span(raw_mode)
        ),
        Ok(mode) => format!(
            "Gate decision was not written although `RIPR_GATE_MODE` is `{}`: the gate step failed before writing it, or was skipped because the pull request guidance it reads is missing. Open the `Run RIPR` step log for the cause.",
            mode.as_str()
        ),
    }
}

#[cfg(test)]
mod tests;
