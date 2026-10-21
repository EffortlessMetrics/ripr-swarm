//! Classify one finished run against its case's expectation. The only inputs
//! are the process output and one optional artifact the route wrote, so a
//! prose claim cannot strengthen a failing state.

use serde_json::Value;

use super::corpus::{Case, Expect, Route};
use super::model::RowResult;

pub(crate) struct Assessment {
    pub(crate) result: RowResult,
    pub(crate) detail: String,
    pub(crate) limitation: Option<String>,
    pub(crate) findings_reported: usize,
    pub(crate) findings_suppressed: usize,
    pub(crate) declined_files: usize,
    /// Deterministic projection of the route artifact. The raw artifact
    /// carries a generation timestamp and root, so it is never hashed.
    pub(crate) artifact_projection: Option<String>,
}

impl Assessment {
    pub(crate) fn fail(result: RowResult, detail: String) -> Self {
        Self {
            result,
            detail,
            limitation: None,
            findings_reported: 0,
            findings_suppressed: 0,
            declined_files: 0,
            artifact_projection: None,
        }
    }

    fn with_counts(mut self, findings: usize, suppressed: usize, declined: usize) -> Self {
        self.findings_reported = findings;
        self.findings_suppressed = suppressed;
        self.declined_files = declined;
        self
    }
}

pub(crate) struct Observed<'a> {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: &'a str,
    pub(crate) stderr: &'a str,
    pub(crate) artifact: Option<&'a str>,
}

pub(crate) fn assess(case: &Case, observed: &Observed<'_>) -> Assessment {
    let expected_exit = if matches!(case.expect, Expect::PolicyRefused) {
        2
    } else {
        0
    };
    if observed.exit_code != Some(expected_exit) && !matches!(case.expect, Expect::PolicyRefused) {
        return Assessment::fail(
            RowResult::SemanticFailure,
            format!(
                "exit code {:?}, expected {expected_exit}; output is not trusted",
                observed.exit_code
            ),
        );
    }
    match case.route {
        Route::Check => assess_check(case, observed),
        Route::Pilot => assess_pilot(case, observed),
    }
}

fn parse(text: &str, what: &str) -> Result<Value, Assessment> {
    serde_json::from_str(text).map_err(|error| {
        Assessment::fail(
            RowResult::SemanticFailure,
            format!("{what} is not JSON ({error})"),
        )
    })
}

fn finding_file(finding: &Value) -> Option<String> {
    let raw = finding
        .pointer("/canonical_gap/file")
        .or_else(|| finding.pointer("/probe/file"))
        .and_then(Value::as_str)?;
    Some(raw.trim_start_matches("./").to_string())
}

fn limitations(root: &Value) -> Vec<Value> {
    root.pointer("/analysis_outcome/outcome/limitations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn limitation_text(limitation: &Value) -> String {
    format!(
        "{} {}",
        limitation
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        limitation
            .get("bounded_detail")
            .and_then(Value::as_str)
            .unwrap_or_default()
    )
}

fn assess_check(case: &Case, observed: &Observed<'_>) -> Assessment {
    let root = match parse(observed.stdout, "check stdout") {
        Ok(root) => root,
        Err(failure) => return failure,
    };
    let findings = root
        .get("findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let changed_findings = findings
        .iter()
        .filter(|finding| finding_file(finding).as_deref() == Some(case.changed.as_str()))
        .count();
    let suppressed = root
        .pointer("/suppression_policy/suppressed")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let all_limitations = limitations(&root);
    let hostile = case.hostile.as_deref();
    let declined: Vec<&Value> = all_limitations
        .iter()
        .filter(|limitation| hostile.is_some_and(|path| limitation_text(limitation).contains(path)))
        .collect();
    let counts =
        |result: Assessment| result.with_counts(changed_findings, suppressed, declined.len());

    match &case.expect {
        Expect::Supported | Expect::TerminatesWithEvidence => {
            if !declined.is_empty() {
                return counts(Assessment::fail(
                    RowResult::SemanticFailure,
                    "a supported input received a limitation naming it".to_string(),
                ));
            }
            if changed_findings == 0 {
                return counts(Assessment::fail(
                    RowResult::WrongSubject,
                    format!("no finding for changed file {}", case.changed),
                ));
            }
            counts(Assessment {
                result: RowResult::PassedSupportedAnalysis,
                detail: "changed-file finding present, no limitation on the unchanged file"
                    .to_string(),
                limitation: None,
                findings_reported: 0,
                findings_suppressed: 0,
                declined_files: 0,
                artifact_projection: None,
            })
        }
        Expect::TypedLimitation {
            token,
            language_class,
        } => {
            let matching: Vec<&&Value> = declined
                .iter()
                .filter(|limitation| limitation_text(limitation).contains(token))
                .collect();
            let Some(first) = matching.first() else {
                return counts(Assessment::fail(
                    RowResult::MissingOrFalseCleanLimitation,
                    format!(
                        "no `{token}` limitation names {}; analysis_complete={}",
                        hostile.unwrap_or("the input"),
                        root.pointer("/analysis_outcome/analysis_complete")
                            .map(Value::to_string)
                            .unwrap_or_else(|| "absent".to_string())
                    ),
                ));
            };
            if let Some(class) = language_class
                && first.get("kind").and_then(Value::as_str) != Some(class)
            {
                return counts(Assessment::fail(
                    RowResult::SemanticFailure,
                    format!(
                        "limitation kind is {:?}, expected {class}",
                        first.get("kind").and_then(Value::as_str)
                    ),
                ));
            }
            if root
                .pointer("/analysis_outcome/analysis_complete")
                .and_then(Value::as_bool)
                != Some(false)
            {
                return counts(Assessment::fail(
                    RowResult::MissingOrFalseCleanLimitation,
                    "limitation present but analysis_complete is not false".to_string(),
                ));
            }
            if changed_findings == 0 {
                return counts(Assessment::fail(
                    RowResult::WrongSubject,
                    format!("hostile file declined but no finding for {}", case.changed),
                ));
            }
            let detail = first
                .get("bounded_detail")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            counts(Assessment {
                result: RowResult::PassedTypedLimitation,
                detail: detail.clone(),
                limitation: Some(detail),
                findings_reported: 0,
                findings_suppressed: 0,
                declined_files: 0,
                artifact_projection: None,
            })
        }
        Expect::GlobMatch => {
            let summary_suppressed = root
                .pointer("/summary/suppressed_by_policy")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if suppressed == 0 || summary_suppressed == 0 {
                return counts(Assessment::fail(
                    RowResult::SemanticFailure,
                    "expected the pattern to suppress the finding; none was suppressed".to_string(),
                ));
            }
            counts(Assessment {
                result: RowResult::PassedSupportedAnalysis,
                detail: format!("{suppressed} finding(s) suppressed by the pattern"),
                limitation: None,
                findings_reported: 0,
                findings_suppressed: 0,
                declined_files: 0,
                artifact_projection: None,
            })
        }
        Expect::GlobNonmatch => {
            if suppressed != 0 {
                return counts(Assessment::fail(
                    RowResult::SemanticFailure,
                    format!("pattern suppressed {suppressed} finding(s); expected none"),
                ));
            }
            if changed_findings == 0 {
                return counts(Assessment::fail(
                    RowResult::WrongSubject,
                    format!(
                        "no finding for {} after a nonmatching pattern",
                        case.changed
                    ),
                ));
            }
            counts(Assessment {
                result: RowResult::PassedSupportedAnalysis,
                detail: "pattern did not match; finding stays visible".to_string(),
                limitation: None,
                findings_reported: 0,
                findings_suppressed: 0,
                declined_files: 0,
                artifact_projection: None,
            })
        }
        Expect::PolicyRefused => {
            let limitation = root
                .pointer("/analysis_scope/limitation")
                .and_then(Value::as_str);
            if observed.exit_code != Some(2)
                || limitation != Some("suppression_policy_invalid")
                || !findings.is_empty()
            {
                return counts(Assessment::fail(
                    RowResult::MissingOrFalseCleanLimitation,
                    format!(
                        "malformed policy was not refused: exit {:?}, limitation {limitation:?}, {} finding(s)",
                        observed.exit_code,
                        findings.len()
                    ),
                ));
            }
            counts(Assessment {
                result: RowResult::PassedTypedLimitation,
                detail: "malformed selector refused as suppression_policy_invalid".to_string(),
                limitation: Some("suppression_policy_invalid".to_string()),
                findings_reported: 0,
                findings_suppressed: 0,
                declined_files: 0,
                artifact_projection: None,
            })
        }
        Expect::GeneratedExcluded => {
            let excluded = all_limitations.iter().any(|limitation| {
                limitation_text(limitation).contains("intentionally skipped")
                    && limitation_text(limitation).contains(case.changed.as_str())
            });
            if !excluded {
                return counts(Assessment::fail(
                    RowResult::MissingOrFalseCleanLimitation,
                    "expected the file to be excluded as generated with a disclosure".to_string(),
                ));
            }
            if changed_findings != 0 {
                return counts(Assessment::fail(
                    RowResult::SemanticFailure,
                    "excluded file still produced a finding".to_string(),
                ));
            }
            counts(Assessment {
                result: RowResult::PassedTypedLimitation,
                detail: "excluded as generated with disclosure".to_string(),
                limitation: Some(format!("generated exclusion names {}", case.changed)),
                findings_reported: 0,
                findings_suppressed: 0,
                declined_files: 0,
                artifact_projection: None,
            })
            .with_counts(0, 0, 1)
        }
        Expect::GeneratedNotExcluded => {
            let excluded = all_limitations.iter().any(|limitation| {
                limitation_text(limitation).contains("intentionally skipped")
                    && limitation_text(limitation).contains(case.changed.as_str())
            });
            if excluded {
                return counts(Assessment::fail(
                    RowResult::SemanticFailure,
                    "a nonmatching pattern silently excluded the file".to_string(),
                ));
            }
            if changed_findings == 0 {
                return counts(Assessment::fail(
                    RowResult::WrongSubject,
                    format!("no finding for {} though it was not excluded", case.changed),
                ));
            }
            counts(Assessment {
                result: RowResult::PassedSupportedAnalysis,
                detail: "pattern did not match; file analyzed".to_string(),
                limitation: None,
                findings_reported: 0,
                findings_suppressed: 0,
                declined_files: 0,
                artifact_projection: None,
            })
        }
    }
}

fn assess_pilot(case: &Case, observed: &Observed<'_>) -> Assessment {
    let summary = match parse(observed.stdout, "pilot stdout") {
        Ok(summary) => summary,
        Err(failure) => return failure,
    };
    if summary.get("status").and_then(Value::as_str) != Some("complete") {
        return Assessment::fail(
            RowResult::SemanticFailure,
            format!(
                "pilot status is {:?}",
                summary.get("status").and_then(Value::as_str)
            ),
        );
    }
    let Some(artifact) = observed.artifact else {
        return Assessment::fail(
            RowResult::SemanticFailure,
            "pilot wrote no repo-exposure artifact".to_string(),
        );
    };
    let exposure = match parse(artifact, "repo-exposure artifact") {
        Ok(exposure) => exposure,
        Err(failure) => return failure,
    };
    let seams = exposure
        .get("seams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unaffected = seams
        .iter()
        .filter(|seam| seam.get("file").and_then(Value::as_str) == Some(case.changed.as_str()))
        .count();
    let mut projection: Vec<String> = seams
        .iter()
        .map(|seam| {
            format!(
                "{}|{}|{}",
                seam.get("seam_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                seam.get("file").and_then(Value::as_str).unwrap_or_default(),
                seam.get("grip_class")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )
        })
        .collect();
    projection.sort();
    let projection = format!(
        "run_status={}\n{}",
        exposure
            .get("run_status")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        projection.join("\n")
    );
    let attach = |mut assessment: Assessment| {
        assessment.artifact_projection = Some(projection.clone());
        assessment
    };
    let hostile = case.hostile.as_deref().unwrap_or_default();
    let disclosure = observed
        .stderr
        .lines()
        .find(|line| line.contains(hostile) && line.contains("static limit rust_nesting_budget"));
    match &case.expect {
        Expect::TypedLimitation { token, .. } => {
            let Some(line) = disclosure.filter(|line| line.contains(token)) else {
                return attach(Assessment::fail(
                    RowResult::MissingOrFalseCleanLimitation,
                    format!(
                        "pilot completed (status complete) with no `{token}` disclosure naming {hostile}"
                    ),
                )
                .with_counts(unaffected, 0, 0));
            };
            if unaffected == 0 {
                return attach(
                    Assessment::fail(
                        RowResult::WrongSubject,
                        format!("hostile file declined but no seam for {}", case.changed),
                    )
                    .with_counts(0, 0, 1),
                );
            }
            attach(
                Assessment {
                    result: RowResult::PassedTypedLimitation,
                    detail: "declined with a stderr static-limit disclosure".to_string(),
                    limitation: Some(line.trim_start_matches("ripr: ").to_string()),
                    findings_reported: 0,
                    findings_suppressed: 0,
                    declined_files: 0,
                    artifact_projection: None,
                }
                .with_counts(unaffected, 0, 1),
            )
        }
        _ => {
            if disclosure.is_some() {
                return attach(
                    Assessment::fail(
                        RowResult::SemanticFailure,
                        "a supported input received a rust_nesting_budget disclosure".to_string(),
                    )
                    .with_counts(unaffected, 0, 1),
                );
            }
            if unaffected == 0 {
                return attach(Assessment::fail(
                    RowResult::WrongSubject,
                    format!("no seam for {}", case.changed),
                ));
            }
            attach(
                Assessment {
                    result: RowResult::PassedSupportedAnalysis,
                    detail: "whole-repository analysis kept the unaffected seam".to_string(),
                    limitation: None,
                    findings_reported: 0,
                    findings_suppressed: 0,
                    declined_files: 0,
                    artifact_projection: None,
                }
                .with_counts(unaffected, 0, 0),
            )
        }
    }
}
