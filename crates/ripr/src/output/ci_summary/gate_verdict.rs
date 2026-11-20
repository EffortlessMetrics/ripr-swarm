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
mod tests {
    use super::{GateVerdict, missing_decision_line};
    use serde_json::json;

    const ACCEPTED: [&str; 4] = [
        "visible-only",
        "acknowledgeable",
        "baseline-check",
        "calibrated-gate",
    ];

    #[test]
    fn a_blocked_decision_names_mode_count_and_first_reason() -> Result<(), String> {
        let decision = json!({
            "status": "blocked",
            "mode": "acknowledgeable",
            "summary": {"blocking": 2},
            "decisions": [
                {"decision": "advisory", "gate_reason": "not this one"},
                {"decision": "blocking", "gate_reason": "needs `ripr-waive`", "placement": {"path": "src/a.rs", "line": 9}},
                {"decision": "blocking", "gate_reason": "second"},
            ],
        });
        let verdict = GateVerdict::from_decision(&decision);
        assert_eq!(
            verdict,
            Some(GateVerdict::Blocked {
                mode: "acknowledgeable".to_string(),
                blocking: 2,
                first: Some((
                    "needs `ripr-waive`".to_string(),
                    Some("src/a.rs:9".to_string())
                )),
                exception: None,
            })
        );
        let verdict = verdict.ok_or("no verdict")?;
        assert_eq!(
            verdict.annotation(),
            "RIPR gate blocked this pull request under acknowledgeable mode: 2 blocking gaps; first: needs `ripr-waive` [src/a.rs:9]. See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact."
        );
        assert_eq!(
            verdict.summary_lines(),
            [
                "### RIPR gate: blocked",
                "**The RIPR gate blocked this pull request** under `acknowledgeable` mode: 2 blocking gaps.",
                "- First blocking reason: `` needs `ripr-waive` `` at `src/a.rs:9`",
                "- Detail: the `Gate decision` section below, and `gate-decision.md` in the `ripr-reports` artifact.",
            ]
        );
        Ok(())
    }

    /// The stated count and the listed decisions can disagree in a damaged
    /// document; the larger is shown so a blocked gate never reads as zero.
    #[test]
    fn a_blocked_decision_never_reports_fewer_gaps_than_it_lists() -> Result<(), String> {
        let listed_only = json!({
            "status": "blocked",
            "mode": "calibrated-gate",
            "decisions": [{"decision": "blocking", "gate_reason": "only", "placement": {"path": "src/a.rs"}}],
        });
        assert_eq!(
            GateVerdict::from_decision(&listed_only),
            Some(GateVerdict::Blocked {
                mode: "calibrated-gate".to_string(),
                blocking: 1,
                first: Some(("only".to_string(), Some("src/a.rs".to_string()))),
                exception: None,
            })
        );
        let counted_only =
            json!({"status": "blocked", "mode": "calibrated-gate", "summary": {"blocking": 4}});
        let verdict = GateVerdict::from_decision(&counted_only);
        assert_eq!(
            verdict,
            Some(GateVerdict::Blocked {
                mode: "calibrated-gate".to_string(),
                blocking: 4,
                first: None,
                exception: None,
            })
        );
        let verdict = verdict.ok_or("no verdict")?;
        assert_eq!(
            verdict.annotation(),
            "RIPR gate blocked this pull request under calibrated-gate mode: 4 blocking gaps. See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact."
        );
        assert_eq!(verdict.summary_lines().len(), 3);
        Ok(())
    }

    /// A gate can block on an exception-policy violation with no blocking
    /// gap. The verdict then names the violation, not "0 blocking gaps".
    #[test]
    fn an_exception_policy_block_names_the_violation() -> Result<(), String> {
        let decision = json!({
            "status": "blocked",
            "mode": "acknowledgeable",
            "summary": {"blocking": 0},
            "decisions": [{"decision": "advisory", "gate_reason": "not this one"}],
            "exception_policy": {"violations": [
                {"kind": "note", "detail": "does not stop the gate", "blocking": false},
                {"kind": "expired_exception", "detail": "EX-1 expired 2026-01-01", "blocking": true},
                {"kind": "missing_owner", "detail": "EX-2 has no owner", "blocking": true},
            ]},
        });
        let verdict = GateVerdict::from_decision(&decision).ok_or("no verdict")?;
        assert_eq!(
            verdict.annotation(),
            "RIPR gate blocked this pull request under acknowledgeable mode: 2 blocking exception-policy violations; first: expired_exception: EX-1 expired 2026-01-01. See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact."
        );
        assert_eq!(
            verdict.summary_lines(),
            [
                "### RIPR gate: blocked",
                "**The RIPR gate blocked this pull request** under `acknowledgeable` mode: 2 blocking exception-policy violations.",
                "- First blocking exception-policy violation: `expired_exception: EX-1 expired 2026-01-01`",
                "- Detail: the `Gate decision` section below, and `gate-decision.md` in the `ripr-reports` artifact.",
            ]
        );

        // Gaps and violations together: both are counted, the gap leads.
        let both = json!({
            "status": "blocked",
            "mode": "acknowledgeable",
            "summary": {"blocking": 1},
            "decisions": [{"decision": "blocking", "gate_reason": "no label"}],
            "exception_policy": {"violations": [
                {"kind": "expired_exception", "detail": "EX-1 expired", "blocking": true},
            ]},
        });
        let verdict = GateVerdict::from_decision(&both).ok_or("no verdict")?;
        assert_eq!(
            verdict.annotation(),
            "RIPR gate blocked this pull request under acknowledgeable mode: 1 blocking gap and 1 blocking exception-policy violation; first: no label. See the gate verdict at the top of the job summary, or gate-decision.md in the ripr-reports artifact."
        );
        Ok(())
    }

    #[test]
    fn a_config_error_carries_its_messages() -> Result<(), String> {
        let decision = json!({
            "status": "config_error",
            "mode": "baseline-check",
            "config_errors": ["one", "two", "three", "four", "five"],
        });
        let verdict =
            GateVerdict::from_decision(&decision).ok_or("config_error must be a verdict")?;
        assert_eq!(
            verdict.annotation(),
            "RIPR gate could not evaluate this pull request under baseline-check mode (configuration error, not a finding about the code): one"
        );
        assert_eq!(
            verdict.summary_lines(),
            [
                "### RIPR gate: configuration error",
                "**The RIPR gate could not evaluate this pull request** under `baseline-check` mode. This is a setup problem, not a finding about the code.",
                "- Config error: `one`",
                "- Config error: `two`",
                "- Config error: `three`",
                "- 2 more in `gate-decision.md` in the `ripr-reports` artifact.",
            ]
        );
        let bare = GateVerdict::from_decision(
            &json!({"status": "config_error", "mode": "baseline-check"}),
        );
        assert_eq!(
            bare.map(|verdict| verdict.summary_lines().len()),
            Some(3),
            "a config_error without messages still points at the report"
        );
        Ok(())
    }

    /// Only the two statuses this module words are verdicts. A passing
    /// status, and a status this module does not know, print nothing.
    #[test]
    fn other_statuses_are_not_verdicts() {
        for status in ["pass", "advisory", "acknowledged", "fail", "", "BLOCKED"] {
            let decision =
                json!({"status": status, "mode": "acknowledgeable", "summary": {"blocking": 3}});
            assert_eq!(GateVerdict::from_decision(&decision), None, "{status:?}");
        }
        assert_eq!(
            GateVerdict::from_decision(&json!({"mode": "acknowledgeable"})),
            None
        );
        assert_eq!(GateVerdict::from_decision(&json!([])), None);
    }

    #[test]
    fn only_a_set_mode_the_gate_refuses_is_a_verdict() -> Result<(), String> {
        assert_eq!(GateVerdict::from_mode(""), None);
        for mode in ACCEPTED {
            assert_eq!(GateVerdict::from_mode(mode), None, "{mode}");
        }
        for mode in ["visible_only", "Visible-Only", " visible-only", "off", "  "] {
            let verdict =
                GateVerdict::from_mode(mode).ok_or(format!("{mode:?} must be refused"))?;
            let annotation = verdict.annotation();
            assert!(
                annotation.starts_with("RIPR gate did not run: unknown gate mode "),
                "{annotation}"
            );
            assert!(annotation.contains(&format!("{mode:?}")), "{annotation}");
            assert!(
                annotation.contains("RIPR_GATE_MODE repository variable"),
                "{annotation}"
            );
            assert!(!annotation.contains('\n'), "{annotation}");
        }
        assert_eq!(
            GateVerdict::from_mode("visible_only").map(|verdict| verdict.summary_lines()),
            Some(vec![
                "### RIPR gate: unknown `RIPR_GATE_MODE`".to_string(),
                "**The RIPR gate did not run**, because `RIPR_GATE_MODE` is not a gate mode. This is a setup problem, not a finding about the code.".to_string(),
                "- Gate message: ``unknown gate mode \"visible_only\". Did you mean `visible-only`? Accepted: visible-only, acknowledgeable, baseline-check, calibrated-gate.``".to_string(),
                "- Next step: fix the `RIPR_GATE_MODE` repository variable (Settings > Secrets and variables > Actions > Variables), or unset it to stay advisory, then re-run the job.".to_string(),
            ])
        );
        Ok(())
    }

    #[test]
    fn the_missing_decision_line_follows_the_mode() {
        assert_eq!(
            missing_decision_line(""),
            "Gate decision was not run. Set `RIPR_GATE_MODE` to `visible-only`, `acknowledgeable`, `baseline-check`, or `calibrated-gate` to opt in."
        );
        assert_eq!(
            missing_decision_line("visible_only"),
            "Gate decision was not run: `RIPR_GATE_MODE` is set to `visible_only`, which is not a gate mode. Set it to `visible-only`, `acknowledgeable`, `baseline-check`, or `calibrated-gate`, or unset it to stay advisory."
        );
        for mode in ACCEPTED {
            let line = missing_decision_line(mode);
            assert!(
                line.starts_with(&format!(
                    "Gate decision was not written although `RIPR_GATE_MODE` is `{mode}`: "
                )),
                "{line}"
            );
            assert!(!line.contains("opt in"), "{line}");
        }
    }
}
