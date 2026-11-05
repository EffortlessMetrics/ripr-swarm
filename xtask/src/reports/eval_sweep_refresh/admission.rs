//! Admit only the producer fields the managed sweep consumes (#13132).
//!
//! The product's `AnalysisOutcome` and its artifact validator are crate-private;
//! the latter also reloads a repository diff instead of this sweep's synthetic
//! input. Keep this a narrow wire-field check, not another full output schema:
//! additive fields are allowed, exposure names come from their public owner,
//! and document completeness still comes from the shared xtask predicate.

use ripr::domain::ExposureClass;
use serde_json::{Map, Value};

use crate::python_judged_panel::parse_json_without_duplicate_keys;

/// Keep exact bytes outside the decoder for raw retention and digests. Lossy
/// UTF-8 or last-key-wins JSON could otherwise replace evidence before scoring.
pub(super) fn parse_check_document(stdout: &[u8]) -> Result<Value, String> {
    let text =
        std::str::from_utf8(stdout).map_err(|error| format!("stdout is not valid UTF-8: {error}"))?;
    let value = parse_json_without_duplicate_keys(text)
        .map_err(|error| format!("stdout is not well-formed JSON: {error}"))?;
    validate_consumed_fields(&value)?;
    Ok(value)
}

fn object<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{field} must be an object"))
}

fn string<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("{field} must be a nonempty string"))
}

fn optional_nonempty_string<'a>(
    value: Option<&'a Value>,
    field: &str,
) -> Result<Option<&'a str>, String> {
    value.map(|value| string(Some(value), field)).transpose()
}

fn validate_consumed_fields(value: &Value) -> Result<(), String> {
    let document = object(Some(value), "check document")?;
    let findings = document
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "findings must be an array".to_string())?;
    for (index, finding) in findings.iter().enumerate() {
        validate_finding(finding, index)?;
    }

    // Older/repo-scope producers may omit the envelope entirely. That remains
    // Partial with a named limitation; a present but malformed envelope is not
    // an absent one and can never authorize a complete/stable comparison.
    if let Some(envelope) = document.get("analysis_outcome") {
        validate_outcome(envelope)?;
    }
    if let Some(limitations) = document.get("run_limitations") {
        let limitations = limitations
            .as_array()
            .ok_or_else(|| "run_limitations must be an array".to_string())?;
        for (index, limitation) in limitations.iter().enumerate() {
            let field = format!("run_limitations[{index}]");
            let limitation = object(Some(limitation), &field)?;
            for key in ["category", "run_status"] {
                optional_nonempty_string(limitation.get(key), &format!("{field}.{key}"))?;
            }
        }
    }

    // An explicitly bounded document may contain an empty findings prefix.
    // The shared predicate already keeps both bound disclosures out of the
    // complete population; do not mistake that valid prefix for malformed JSON.
    if crate::check_document_analysis_complete(value) == Some(true) {
        let with_findings = value
            .pointer("/analysis_outcome/outcome/kind")
            .and_then(Value::as_str)
            == Some("complete_with_findings");
        let has_findings = !findings.is_empty();
        if with_findings != has_findings {
            return Err(
                "analysis_outcome.outcome.kind disagrees with findings being empty".to_string(),
            );
        }
    }
    Ok(())
}

fn validate_outcome(value: &Value) -> Result<(), String> {
    let envelope = object(Some(value), "analysis_outcome")?;
    let complete = envelope
        .get("analysis_complete")
        .and_then(Value::as_bool)
        .ok_or_else(|| "analysis_outcome.analysis_complete must be a boolean".to_string())?;
    if let Some(document_complete) = envelope.get("document_complete")
        && !document_complete.is_boolean()
    {
        return Err("analysis_outcome.document_complete must be a boolean".to_string());
    }
    let outcome = object(envelope.get("outcome"), "analysis_outcome.outcome")?;
    let kind = string(outcome.get("kind"), "analysis_outcome.outcome.kind")?;
    // This wire projection mirrors the private AnalysisOutcomeKind::is_complete
    // in crates/ripr/src/analysis_outcome.rs, as DX check admission does. An
    // unfamiliar kind cannot gain complete-run credit from a boolean alone.
    let kind_complete = match kind {
        "no_scope"
        | "no_changed_lines"
        | "no_behavioral_candidates"
        | "complete_no_findings"
        | "complete_with_findings" => true,
        "partial_with_limitations" | "unsupported_input" | "analysis_failed" => false,
        _ => return Err("analysis_outcome.outcome.kind is not recognized".to_string()),
    };
    if complete != kind_complete {
        return Err(
            "analysis_outcome.analysis_complete disagrees with analysis_outcome.outcome.kind"
                .to_string(),
        );
    }
    let limitations = outcome
        .get("limitations")
        .and_then(Value::as_array)
        .ok_or_else(|| "analysis_outcome.outcome.limitations must be an array".to_string())?;
    for (index, limitation) in limitations.iter().enumerate() {
        let field = format!("analysis_outcome.outcome.limitations[{index}]");
        let limitation = object(Some(limitation), &field)?;
        string(limitation.get("kind"), &format!("{field}.kind"))?;
    }
    Ok(())
}

fn validate_finding(value: &Value, index: usize) -> Result<(), String> {
    let field = format!("findings[{index}]");
    let finding = object(Some(value), &field)?;
    let classification = string(
        finding.get("classification"),
        &format!("{field}.classification"),
    )?;
    if !ExposureClass::ALL
        .iter()
        .any(|class| class.as_str() == classification)
    {
        return Err(format!("{field}.classification is not recognized"));
    }
    for key in ["static_limit_kind", "oracle_alignment"] {
        // New alignment tokens retain the existing `unknown` bucket. Only a
        // malformed value is rejected; this is not another alignment taxonomy.
        if finding.get(key).is_some_and(|value| !value.is_string()) {
            return Err(format!("{field}.{key} must be a string"));
        }
    }
    let gap_id = optional_nonempty_string(
        finding.get("canonical_gap_id"),
        &format!("{field}.canonical_gap_id"),
    )?;
    if let Some(gap) = finding.get("canonical_gap") {
        let gap = object(Some(gap), &format!("{field}.canonical_gap"))?;
        let nested_id = string(gap.get("id"), &format!("{field}.canonical_gap.id"))?;
        if gap_id.is_some_and(|id| id != nested_id) {
            return Err(format!(
                "{field}.canonical_gap_id disagrees with canonical_gap.id"
            ));
        }
    }
    if let Some(placement) = finding.get("repair_placement") {
        let placement = object(Some(placement), &format!("{field}.repair_placement"))?;
        if placement
            .get("verify_command")
            .is_some_and(|value| !value.is_string())
        {
            return Err(format!(
                "{field}.repair_placement.verify_command must be a string"
            ));
        }
    }
    if let Some(card) = finding.get("python_repair_card") {
        object(Some(card), &format!("{field}.python_repair_card"))?;
    }
    // Canonical IDs are optional and group IDs may repeat across findings.
    Ok(())
}

#[cfg(test)]
#[path = "admission/tests.rs"]
mod python_eval_sweep_refresh_admission;
