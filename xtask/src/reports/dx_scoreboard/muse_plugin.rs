//! Supplied Muse trial records, not an agent runner or repair authority.
//!
//! Shape validation uses the shared schema owner. The checks here only join
//! observations within one run; they do not fetch or authenticate artifacts.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

mod envelope;

pub(super) const SCHEMA_VERSION: &str = "muse-plugin-eval-v1";
const SCHEMA: &str = include_str!("../../../../schemas/ripr/muse-plugin-eval.schema.json");
const METRICS: &[&str] = &[
    "setup_passed",
    "static_exposed_after",
    "durable_ready_reported",
    "original_test_passed",
    "wrong_control_test_failed",
    "restored_test_passed",
    "executed_discrimination",
    "model_calls",
    "tool_calls",
    "elapsed_ms",
    "peak_rss_bytes",
    "input_tokens",
    "cached_input_tokens",
    "uncached_input_tokens",
    "output_tokens",
    "total_tokens",
];

/// Preserve missing axes as missing, including a wholly incomplete run. Put
/// the supplied record on one sample only; the others name that same subject.
pub(super) fn complete_projection(
    value: &Value,
    config: &super::Config,
    mut samples: Vec<super::Sample>,
) -> Result<Vec<super::Sample>, String> {
    let subject = match value.get("observations") {
        Some(observations) => format!("run_id={}", text(observations, "run_id")?),
        None => "sparse document; no structured trial identity".to_string(),
    };
    let identity = sample_identity(value)?;
    for name in METRICS {
        let id = format!("agent.muse_{name}");
        super::ingest_metric(&id, "muse-plugin", config)?;
        if !samples.iter().any(|sample| sample.metric == id) {
            samples.push(super::Sample {
                metric: id,
                repo: Some(identity.clone()),
                outcome: super::SampleOutcome::NotMeasured,
                detail: if value.get("observations").is_some() {
                    "not observed or incomplete in supplied trial".to_string()
                } else {
                    "canonical record supplied without structured observations".to_string()
                },
            });
        }
    }
    samples.sort_by(|left, right| left.metric.cmp(&right.metric));
    let anchor = samples
        .first()
        .ok_or("Muse projection has no registered axes")?
        .metric
        .clone();
    for sample in &mut samples {
        sample.detail = format!(
            "{}; {subject}; supplied provenance on {anchor} (not authenticated)",
            sample.detail
        );
    }
    if let Some(first) = samples.first_mut() {
        let encoded = serde_json::to_string(value).map_err(|err| err.to_string())?;
        first.detail.push_str(&format!("; supplied_record={encoded}"));
    }
    Ok(samples)
}

/// Validate original JSON bytes before serde_json can round a numeric token.
/// Direct Value callers are checked again in to_input, without claiming to
/// recover precision already lost by their caller.
pub(super) fn validate_text(text: &str) -> Result<(), String> {
    let value = crate::exact_schema_json::parse(text.as_bytes())?;
    let schema = crate::exact_schema_json::parse(SCHEMA.as_bytes())?;
    let mut violations = Vec::new();
    crate::verification_contracts::validate_exact_value_against_schema(
        &value,
        &schema,
        &schema,
        "muse-plugin-eval".to_string(),
        &mut violations,
    );
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("; "))
    }
}

/// V1 projects one document, not a cohort. The existing scoreboard's min/max rollup
/// cannot aggregate these observations or retain missing dimensions per trial.
/// Other native input populations may still coexist with this one Muse run.
pub(super) fn register_run(value: &Value, seen: &mut BTreeSet<String>) -> Result<(), String> {
    if value["schema_version"].as_str() == Some(super::INPUT_SCHEMA_VERSION)
        && value["source"].as_str() == Some("muse-plugin")
    {
        return Err(format!(
            "external Muse input must use {SCHEMA_VERSION}, not generic metrics"
        ));
    }
    if value["schema_version"].as_str() == Some(SCHEMA_VERSION) {
        let id = match value.get("observations") {
            Some(observations) => format!("run:{}", text(observations, "run_id")?),
            None => sample_identity(value)?,
        };
        if !seen.insert(id.clone()) {
            return Err(format!("duplicate Muse plugin record '{id}'"));
        }
        if seen.len() > 1 {
            return Err(
                "only one Muse record per scoreboard invocation; write separate reports for distinct records"
                    .to_string(),
            );
        }
    }
    Ok(())
}

pub(super) fn to_input(record: &Value) -> Result<Value, String> {
    let encoded = serde_json::to_string(record).map_err(|err| err.to_string())?;
    validate_text(&encoded)?;
    let Some(value) = record.get("observations") else {
        return Ok(json!({
            "schema_version": super::INPUT_SCHEMA_VERSION,
            "source": "muse-plugin",
            "evidence": "sparse canonical Muse record; no measured axes",
            "metrics": [],
        }));
    };
    reject_blank_strings(value)?;
    let artifacts = value["evidence"]
        .as_array()
        .ok_or("Muse receipt needs evidence")?;
    let mut evidence = BTreeMap::new();
    for artifact in artifacts {
        let id = text(artifact, "id")?;
        if evidence.insert(id, artifact).is_some() {
            return Err(format!("duplicate Muse evidence id '{id}'"));
        }
    }
    let reference = |id: &str, kind: &str| -> Result<(), String> {
        let artifact = evidence
            .get(id)
            .ok_or_else(|| format!("unknown Muse evidence reference '{id}'"))?;
        if artifact["kind"].as_str() != Some(kind) {
            return Err(format!("Muse evidence '{id}' must have kind '{kind}'"));
        }
        Ok(())
    };
    let mut rows = Vec::new();
    if let Some(setup) = value.get("setup") {
        reference(text(setup, "evidence")?, "tool_transcript")?;
        let status = text(setup, "status")?;
        let usable = setup["plugin_loaded"] == true && setup["mcp_available"] == true;
        if (status == "passed" && !usable) || (status == "failed" && usable) {
            return Err("Muse setup status contradicts plugin/MCP observations".to_string());
        }
        if status != "passed"
            && ["static", "durable", "executions"]
                .iter()
                .any(|key| value.get(*key).is_some())
        {
            return Err(
                "failed/incomplete setup cannot carry later repair observations".to_string(),
            );
        }
        observed_status(&mut rows, "setup_passed", status);
    }
    if let Some(observation) = value.get("static") {
        reference(text(observation, "before_evidence")?, "static_report")?;
        match (observation.get("after"), observation.get("after_evidence")) {
            (Some(after), Some(after_ref)) => {
                let after_ref = after_ref.as_str().ok_or("after_evidence must be a string")?;
                reference(after_ref, "static_report")?;
                distinct_snapshots(
                    observation,
                    "before_evidence",
                    "after_evidence",
                    observation["before"] != *after,
                    &evidence,
                )?;
                if let Some(exposed) = static_exposure(after) {
                    push(
                        &mut rows,
                        "static_exposed_after",
                        json!(u8::from(exposed)),
                        true,
                    );
                }
            }
            (None, None) => {}
            _ => {
                return Err(
                    "static after class and evidence must be supplied together".to_string(),
                );
            }
        }
    }
    if let Some(durable) = value.get("durable") {
        reference(text(durable, "before_manifest")?, "repair_manifest")?;
        let after_fields = ["after_manifest", "after_state", "current", "edit_cage"];
        let supplied = after_fields
            .iter()
            .filter(|key| durable.get(**key).is_some())
            .count();
        if supplied != 0 && supplied != after_fields.len() {
            return Err(
                "durable after observation needs manifest, state, current and edit_cage"
                    .to_string(),
            );
        }
        if supplied != 0 {
            reference(text(durable, "after_manifest")?, "repair_manifest")?;
            let ready = durable["after_state"] == "ready_to_finish";
            distinct_snapshots(durable, "before_manifest", "after_manifest", ready, &evidence)?;
            let compliant = durable["current"] == true && durable["edit_cage"] == "compliant";
            if ready && !compliant {
                return Err("ready_to_finish contradicts current/edit_cage observation".to_string());
            }
            // This records the supplied attempt status. It is not a substitute
            // for the canonical repair-manifest or workspace-currentness check.
            push(
                &mut rows,
                "durable_ready_reported",
                json!(u8::from(ready)),
                true,
            );
        }
    }
    if let Some(executions) = value.get("executions") {
        let mut ids = BTreeSet::new();
        let mut refs = BTreeSet::new();
        let mut tests_digest = None;
        for (leg, metric) in [
            ("original", "original_test_passed"),
            ("counterexample", "wrong_control_test_failed"),
            ("restored", "restored_test_passed"),
        ] {
            let Some(execution) = executions.get(leg) else {
                continue;
            };
            let id = text(execution, "id")?;
            let evidence_ref = text(execution, "evidence")?;
            reference(evidence_ref, "test_execution")?;
            if !ids.insert(id) || !refs.insert(evidence_ref) {
                return Err("execution legs need distinct IDs and evidence references".to_string());
            }
            let digest = text(execution, "tests_sha256")?;
            if tests_digest.is_some_and(|prior| prior != digest) {
                return Err("execution legs must exercise the same test bytes".to_string());
            }
            tests_digest = Some(digest);
            validate_execution(execution)?;
            if execution["failure_kind"] == "assertion"
                && execution["failed_test"] != executions["test_selector"]
            {
                return Err("control assertion must fail at the named test selector".to_string());
            }
            let status = text(execution, "status")?;
            if status != "incomplete" {
                let success = if leg == "counterexample" {
                    status == "failed" && execution["failure_kind"] == "assertion"
                } else {
                    status == "passed"
                };
                push(&mut rows, metric, json!(u8::from(success)), true);
            }
        }
        if ids.is_empty() {
            return Err("executions must contain at least one observed leg".to_string());
        }
        let original = executions.get("original");
        let counterexample = executions.get("counterexample");
        let restored = executions.get("restored");
        if let (Some(original), Some(restored)) = (original, restored)
            && original["implementation_sha256"] != restored["implementation_sha256"]
        {
            return Err(
                "restored execution does not identify the original implementation".to_string(),
            );
        }
        if let (Some(original), Some(counterexample)) = (original, counterexample)
            && original["implementation_sha256"] == counterexample["implementation_sha256"]
        {
            return Err("counterexample must identify different implementation bytes".to_string());
        }
        if let (Some(original), Some(counterexample), Some(restored)) =
            (original, counterexample, restored)
            && [original, counterexample, restored]
                .iter()
                .all(|leg| leg["status"] != "incomplete")
        {
            let discriminated = original["status"] == "passed"
                && counterexample["status"] == "failed"
                && counterexample["failure_kind"] == "assertion"
                && restored["status"] == "passed";
            push(
                &mut rows,
                "executed_discrimination",
                json!(u8::from(discriminated)),
                true,
            );
        }
    }
    if let Some(usage) = value.get("usage") {
        reference(text(usage, "evidence")?, "usage_report")?;
        let complete = usage["complete"] == true;
        let mut measured = 0;
        for name in ["model_calls", "tool_calls", "elapsed_ms", "peak_rss_bytes"] {
            if let Some(number) = usage.get(name) {
                push(&mut rows, name, number.clone(), complete);
                measured += 1;
            }
        }
        if let Some(tokens) = usage.get("tokens") {
            let input = count(tokens, "input")?;
            let cached = count(tokens, "cached_input")?;
            let output = count(tokens, "output")?;
            let total = count(tokens, "total")?;
            if cached > input || input.checked_add(output) != Some(total) {
                return Err(
                    "tokens require cached_input <= input and total = input + output".to_string(),
                );
            }
            for (name, number) in [
                ("input_tokens", input),
                ("cached_input_tokens", cached),
                ("uncached_input_tokens", input - cached),
                ("output_tokens", output),
                ("total_tokens", total),
            ] {
                push(&mut rows, name, json!(number), complete);
            }
            measured += 1;
        }
        if measured == 0 {
            return Err("usage needs at least one measurement; omit unmeasured usage".to_string());
        }
    }
    if !["setup", "static", "durable", "executions", "usage"]
        .iter()
        .any(|key| value.get(*key).is_some())
    {
        return Err("Muse receipt needs at least one observed dimension".to_string());
    }
    envelope::validate(record, value)?;
    let run_id = text(value, "run_id")?;
    let identity = sample_identity(record)?;
    for row in &mut rows {
        row["repo"] = json!(identity);
    }
    Ok(json!({
        "schema_version": super::INPUT_SCHEMA_VERSION,
        "source": "muse-plugin",
        "evidence": format!("supplied Muse trial run_id={run_id}"),
        "metrics": rows,
    }))
}

/// A changed document is not a same-population baseline. A sparse document's
/// digest identifies only supplied bytes, never an invented run/trial identity.
/// serde_json's map representation orders keys; the exact parser's BTreeMap
/// also pins key order even if a dependency enables preserve_order later.
fn sample_identity(value: &Value) -> Result<String, String> {
    let encoded = serde_json::to_string(value).map_err(|err| err.to_string())?;
    let canonical = crate::exact_schema_json::parse(encoded.as_bytes())?.to_string();
    let digest = Sha256::digest(canonical.as_bytes());
    match value.get("observations") {
        Some(observations) => Ok(format!("muse:{}:{digest:x}", text(observations, "run_id")?)),
        None => Ok(format!("muse-document:{digest:x}")),
    }
}

fn static_exposure(class: &Value) -> Option<bool> {
    match class.as_str() {
        Some("exposed") => Some(true),
        Some("weakly_exposed" | "reachable_unrevealed" | "no_static_path") => Some(false),
        _ => None,
    }
}

fn validate_execution(execution: &Value) -> Result<(), String> {
    let status = text(execution, "status")?;
    let tests = count(execution, "tests_executed")?;
    let exit = execution
        .get("exit_code")
        .and_then(integral_i64)
        .and_then(|code| i32::try_from(code).ok())
        .map(i64::from);
    let failure = execution.get("failure_kind").and_then(Value::as_str);
    if failure != Some("assertion") && execution.get("failed_test").is_some() {
        return Err("failed_test belongs only to an observed assertion failure".to_string());
    }
    match status {
        "passed" if exit != Some(0) || tests == 0 || failure.is_some() => {
            Err("passing execution needs exit 0, executed tests and no failure".to_string())
        }
        "failed" if exit.is_none_or(|code| code == 0) || failure.is_none() => {
            Err("failed execution needs nonzero exit and failure_kind".to_string())
        }
        _ if failure == Some("assertion") && (tests == 0 || status != "failed") => {
            Err("assertion failure needs executed tests and failed status".to_string())
        }
        _ => Ok(()),
    }
}

fn distinct_snapshots(
    value: &Value,
    before: &str,
    after: &str,
    changed: bool,
    evidence: &BTreeMap<&str, &Value>,
) -> Result<(), String> {
    let first = text(value, before)?;
    let second = text(value, after)?;
    if first == second || (changed && evidence[first]["sha256"] == evidence[second]["sha256"]) {
        return Err(format!("{before} and {after} need distinct snapshot evidence"));
    }
    Ok(())
}

fn observed_status(rows: &mut Vec<Value>, metric: &str, status: &str) {
    if status != "incomplete" {
        push(rows, metric, json!(u8::from(status == "passed")), true);
    }
}

fn push(rows: &mut Vec<Value>, name: &str, value: Value, complete: bool) {
    rows.push(json!({"id": format!("agent.muse_{name}"), "value": value, "completed": complete}));
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("Muse receipt needs string '{key}'"))
}

fn count(value: &Value, key: &str) -> Result<u64, String> {
    integral_i64(&value[key])
        .and_then(|number| u64::try_from(number).ok())
        .filter(|number| *number <= 9_007_199_254_740_991)
        .ok_or_else(|| format!("Muse receipt needs integer '{key}'"))
}

/// JSON Schema integer is mathematical, not a restriction to integer tokens.
/// Raw source validation has already rejected fractions before serde_json can
/// round them. Normalize integral decimal/exponent spellings exactly here;
/// bounded scale and checked parsing avoid float casts or silent truncation.
fn integral_i64(value: &Value) -> Option<i64> {
    let Value::Number(number) = value else {
        return None;
    };
    let decimal = number
        .to_string()
        .parse::<bigdecimal::BigDecimal>()
        .ok()?
        .normalized();
    let (_, scale) = decimal.as_bigint_and_exponent();
    if !(-256..=0).contains(&scale) {
        return None;
    }
    let (integer, _) = decimal.with_scale(0).as_bigint_and_exponent();
    integer.to_string().parse().ok()
}

fn reject_blank_strings(value: &Value) -> Result<(), String> {
    match value {
        Value::String(text) if text.trim().is_empty() => {
            Err("Muse receipt has a blank identity/reference".to_string())
        }
        Value::Array(values) => values.iter().try_for_each(reject_blank_strings),
        Value::Object(values) => values.values().try_for_each(reject_blank_strings),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
