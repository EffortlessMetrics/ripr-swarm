//! Bounded terminal evidence inside the existing scoreboard report.
//!
//! Keep producer diagnostics after scratch cleanup, not arbitrary process
//! output or environment contents. Stream identities explicitly say that the
//! original bytes are not retained. This never decides metric admission.

use super::{CheckAdmission, CheckSubject, MeasuredOutput, admit_check_sample, exited_zero};
use serde_json::{Value, json};

const MAX_PHASES: usize = 16;

fn digest(text: &str) -> String {
    crate::blind_journey::sha256_hex(text.as_bytes())
}

pub(super) fn pilot_reason(summary: Option<&Value>) -> &'static str {
    match summary.and_then(|value| value["reason"].as_str()) {
        Some("timeout") => "timeout",
        Some(_) => "unrecognized_reason",
        None => "not_reported",
    }
}

pub(super) fn pilot_status(summary: Option<&Value>) -> &'static str {
    match summary.and_then(|value| value["status"].as_str()) {
        Some("complete") => "complete",
        Some("partial") => "partial",
        Some(_) => "unrecognized_status",
        None => "missing",
    }
}

fn pilot_projection(summary: Option<&Value>) -> Value {
    let Some(summary) = summary else {
        return json!({"state": "missing_or_invalid"});
    };
    let status = pilot_status(Some(summary));
    json!({
        "state": "retained_projection",
        "status": status,
        "reason": pilot_reason(Some(summary)),
        "timeout_ms": summary["timeout_ms"].as_u64(),
        "completed_phases_count": summary["completed_phases"].as_array().map(Vec::len),
        "raw_summary_retained": false,
    })
}

fn safe_phase(phase: &str) -> bool {
    matches!(
        phase,
        "diff_admission_index"
            | "diff_admission_probe"
            | "diff_drop_unreached_test_files"
            | "diff_main_index"
            | "cache_load"
            | "file_fact_cache"
            | "evidence_macro_scan"
            | "index_cached_parse"
            | "index_macro_generated_tests"
            | "index_test_styles"
            | "index_role_composition"
            | "index_test_helper_credit"
            | "index_doctests"
            | "index_proptest_tests"
            | "index_parameterized_tests"
            | "index_bolero_tests"
            | "index_harness_registry"
    )
}

fn safe_status(status: &str) -> bool {
    matches!(
        status,
        "start"
            | "finished"
            | "built"
            | "reused_admission_core"
            | "hit"
            | "miss"
            | "corrupt_ignored"
    ) || status
        .strip_prefix("loaded_files_")
        .is_some_and(|count| !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit()))
}

fn safe_resource(value: &Value) -> bool {
    // The existing typed parser validates schema/attribution and rejects
    // unknown fields. Bound the remaining producer string vocabulary too.
    fn strings_known(value: &Value) -> bool {
        match value {
            Value::String(text) => matches!(
                text.as_str(),
                "0.1"
                    | "ripr_process_self"
                    | "linux"
                    | "windows"
                    | "macos"
                    | "x86_64"
                    | "aarch64"
                    | "x86"
                    | "arm"
                    | "observed"
                    | "unavailable"
                    | "linux_user_hz_clock_ticks"
                    | "windows_hundred_nanoseconds"
                    | "platform_not_supported"
                    | "source_query_failed"
                    | "source_unreadable"
                    | "source_field_missing"
                    | "source_value_malformed"
                    | "receipt_serialization_failed"
            ),
            Value::Array(items) => items.iter().all(strings_known),
            Value::Object(fields) => fields.values().all(strings_known),
            _ => true,
        }
    }
    strings_known(value)
}

pub(super) fn not_run(kind: &str) -> Value {
    json!({"kind": kind, "capture": "not_run", "analysis": "not_observed", "streams": null})
}

pub(super) fn record(
    kind: &str,
    captured: &Result<MeasuredOutput, String>,
    summary: Option<&Value>,
    subject: Option<&CheckSubject>,
) -> Value {
    let measured = match captured {
        Ok(measured) => measured,
        Err(error) => {
            return json!({
                "kind": kind, "capture": "failed", "analysis": "not_observed",
                "capture_error_sha256": digest(error), "streams": null,
            });
        }
    };
    let output = &measured.output;
    let capture = if output.timed_out {
        "timed_out"
    } else if exited_zero(measured) {
        "completed"
    } else if output.status.and_then(|status| status.code()).is_some() {
        "failed_exit"
    } else {
        "terminated"
    };
    let (analysis, finding_count) = match subject {
        Some(subject) => match admit_check_sample(
            output.timed_out,
            exited_zero(measured),
            &output.stdout,
            subject,
        ) {
            CheckAdmission::Complete { findings, .. } => ("complete", Some(findings)),
            CheckAdmission::ExpectedPartial { findings, .. } => {
                ("expected_partial", Some(findings))
            }
            CheckAdmission::Rejected { .. } => ("rejected", None),
        },
        None if !output.timed_out
            && exited_zero(measured)
            && summary.is_some_and(|value| value["status"] == "complete") =>
        {
            ("complete", None)
        }
        None if summary.is_some_and(|value| value["status"] == "partial") => ("partial", None),
        None => ("incomplete", None),
    };
    let (cost, limitation) = crate::repo_exposure_resource_cost_from_stderr(&output.stderr);
    let resource = cost
        .and_then(|cost| serde_json::to_value(cost).ok())
        .filter(safe_resource);
    let resource_limitation = if resource.is_some() {
        None
    } else {
        Some(
            limitation
                .as_deref()
                .unwrap_or("unsupported_resource_vocabulary"),
        )
    };
    let prefix = format!(
        "ripr_repo_exposure_latency token={} ",
        crate::repo_exposure_receipt_token()
    );
    let authenticated_lines = output
        .stderr
        .lines()
        .filter(|line| line.starts_with(&prefix))
        .filter(|line| line.len() <= 512)
        .take(MAX_PHASES + 1)
        .collect::<Vec<_>>();
    let phase_limit_reached = authenticated_lines.len() > MAX_PHASES;
    let authenticated = authenticated_lines.join("\n");
    let parsed = crate::repo_exposure_latency_trace(&authenticated);
    let phases = parsed
        .iter()
        .filter(|phase| safe_phase(&phase.phase) && safe_status(&phase.status))
        .take(MAX_PHASES)
        .map(|phase| {
            json!({
                "phase": phase.phase, "status": phase.status, "duration_ms": phase.duration_ms,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "kind": kind, "capture": capture, "analysis": analysis,
        "check_finding_count": finding_count,
        "exit_code": output.status.and_then(|status| status.code()),
        "timed_out": output.timed_out, "elapsed_ms": output.duration.as_millis(),
        "sampled_peak_rss_bytes": measured.peak_rss_bytes,
        "pilot_summary": subject.is_none().then(|| pilot_projection(summary)),
        "streams": {
            "retention": "digests_only",
            "stdout": {"bytes": output.stdout.len(), "sha256": digest(&output.stdout)},
            "stderr": {"bytes": output.stderr.len(), "sha256": digest(&output.stderr)},
        },
        "producer_resource_cost": resource, "resource_limitation": resource_limitation,
        "authenticated_phases": phases,
        "phase_limit": MAX_PHASES,
        "phase_limit_reached": phase_limit_reached,
    })
}
