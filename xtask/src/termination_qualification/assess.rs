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
    pub(crate) artifact: Option<&'a str>,
}

pub(crate) fn assess(case: &Case, observed: &Observed<'_>) -> Assessment {
    if observed.exit_code != Some(0) {
        return Assessment::fail(
            RowResult::SemanticFailure,
            format!(
                "exit code {:?}, expected 0; output is not trusted",
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
        Expect::Supported => {
            let names_changed = all_limitations
                .iter()
                .any(|limitation| limitation_text(limitation).contains(case.changed.as_str()));
            if names_changed {
                return counts(Assessment::fail(
                    RowResult::SemanticFailure,
                    "a limitation names the changed file; the row is not fully analyzed"
                        .to_string(),
                ));
            }
            if root
                .pointer("/analysis_outcome/analysis_complete")
                .and_then(Value::as_bool)
                != Some(true)
            {
                return counts(Assessment::fail(
                    RowResult::SemanticFailure,
                    "a supported input did not report analysis_complete=true".to_string(),
                ));
            }
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
    let run_status = exposure.get("run_status").and_then(Value::as_str);
    let hostile = case.hostile.as_deref().unwrap_or_default();
    // The decline is carried by the summary's `declined_rust_files` field and
    // by the artifact's `run_status` and `rust_files_declined` limitation
    // (#9308); stderr is no longer the evidence.
    let declined_entry = summary
        .pointer("/declined_rust_files/files")
        .and_then(Value::as_array)
        .and_then(|files| {
            files
                .iter()
                .find(|file| file.get("path").and_then(Value::as_str) == Some(hostile))
        });
    let declined_in_artifact = exposure
        .get("limitations")
        .and_then(Value::as_array)
        .is_some_and(|limitations| {
            limitations.iter().any(|limitation| {
                limitation.get("category").and_then(Value::as_str) == Some("rust_files_declined")
            })
        });
    match &case.expect {
        Expect::TypedLimitation { token, .. } => {
            let reason = declined_entry
                .and_then(|file| file.get("reason").and_then(Value::as_str))
                .filter(|reason| reason.contains(token));
            let Some(reason) = reason else {
                return attach(
                    Assessment::fail(
                        RowResult::MissingOrFalseCleanLimitation,
                        format!(
                            "pilot completed (status complete) with no `{token}` entry in declined_rust_files naming {hostile}"
                        ),
                    )
                    .with_counts(unaffected, 0, 0),
                );
            };
            if run_status != Some("limited_declined_files") || !declined_in_artifact {
                return attach(
                    Assessment::fail(
                        RowResult::MissingOrFalseCleanLimitation,
                        format!(
                            "declined file named in the summary but the artifact run_status is {run_status:?} (rust_files_declined limitation: {declined_in_artifact})"
                        ),
                    )
                    .with_counts(unaffected, 0, 1),
                );
            }
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
                    detail:
                        "declined with a typed limited run_status and declined_rust_files entry"
                            .to_string(),
                    limitation: Some(reason.to_string()),
                    findings_reported: 0,
                    findings_suppressed: 0,
                    declined_files: 0,
                    artifact_projection: None,
                }
                .with_counts(unaffected, 0, 1),
            )
        }
        _ => {
            if declined_entry.is_some() || declined_in_artifact || run_status != Some("complete") {
                return attach(
                    Assessment::fail(
                        RowResult::SemanticFailure,
                        format!(
                            "a supported input was not a whole run: run_status {run_status:?}, declined_rust_files entry: {}",
                            declined_entry.is_some()
                        ),
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
