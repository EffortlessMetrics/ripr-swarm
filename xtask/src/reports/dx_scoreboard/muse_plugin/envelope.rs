//! Join structured observations to the canonical evaluation context without
//! promoting prose judgments, command strings or absent fields to measurements.

use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn validate(record: &Value, observations: &Value) -> Result<(), String> {
    for (outer, inner) in [
        ("/identity/scenario_id", "/scenario/id"),
        ("/identity/plugin_version", "/subject/plugin_version"),
        ("/identity/ripr_version", "/subject/ripr_version"),
        ("/identity/ripr_commit", "/subject/ripr_commit"),
        ("/identity/muse_version", "/subject/muse_version"),
        ("/environment/os", "/subject/platform"),
        ("/model/model", "/subject/model"),
        ("/model/trial_id", "/trial_id"),
        ("/repair/canonical_id", "/static/gap_id"),
        ("/repair/attempt_id", "/durable/attempt_id"),
        ("/repair/before_class", "/static/before"),
        ("/repair/after_class", "/static/after"),
    ] {
        if let (Some(expected), Some(actual)) =
            (known_text(record, outer), known_text(observations, inner))
            && expected != actual
        {
            return Err(format!("canonical {outer} contradicts observations{inner}"));
        }
    }
    // A workspace HEAD and a seam ID are deliberately not aliases for a
    // binary commit and a canonical gap ID, respectively.
    if let Some(digest) = known_text(record, "/identity/fixture_digest")
        && Some(digest.strip_prefix("sha256:").unwrap_or(digest))
            != known_text(observations, "/scenario/fixture_sha256")
    {
        return Err("canonical fixture_digest contradicts observations fixture_sha256".to_string());
    }
    for (outer, inner) in [
        ("/task/initial_request", "/scenario/task_sha256"),
        ("/model/configuration", "/subject/model_config_sha256"),
    ] {
        if let Some(source) = record.pointer(outer).and_then(Value::as_str) {
            let expected = format!("{:x}", Sha256::digest(source.as_bytes()));
            if observations.pointer(inner).and_then(Value::as_str) != Some(expected.as_str()) {
                return Err(format!(
                    "observations{inner} must hash canonical {outer} UTF-8 bytes"
                ));
            }
        }
    }
    for (outer, inner, multiplier) in [
        ("/efficiency/model_calls", "/usage/model_calls", 1),
        ("/efficiency/tool_calls", "/usage/tool_calls", 1),
        ("/efficiency/input_tokens", "/usage/tokens/input", 1),
        ("/efficiency/output_tokens", "/usage/tokens/output", 1),
        ("/efficiency/wall_time", "/usage/elapsed_ms", 1000),
    ] {
        if let (Some(expected), Some(actual)) = (number(record, outer), number(observations, inner))
            && expected * bigdecimal::BigDecimal::from(multiplier) != actual
        {
            return Err(format!("canonical {outer} contradicts observations{inner}"));
        }
    }
    if let Some(setup) = observations.get("setup") {
        let outcome = record["mcp"]["setup_outcome"].as_str();
        if (outcome == Some("ready") && setup["status"] == "failed")
            || (matches!(outcome, Some("approval_required" | "server_unavailable"))
                && setup["mcp_available"] == true)
            || (matches!(
                record["environment"]["ripr_binary"].as_str(),
                Some("missing" | "predates_mcp")
            ) && setup["mcp_available"] == true)
        {
            return Err(
                "canonical setup context contradicts structured MCP observations".to_string(),
            );
        }
    }
    if record["verdicts"]["combined_claim"] == "completed_repair" {
        reject_refuted_completion(observations)?;
    }
    Ok(())
}

fn reject_refuted_completion(observations: &Value) -> Result<(), String> {
    let setup_failed = observations["setup"]["status"] == "failed";
    let static_not_exposed =
        super::static_exposure(&observations["static"]["after"]) == Some(false);
    let durable_not_ready = observations["durable"]
        .get("after_state")
        .is_some_and(|state| state != "ready_to_finish");
    let execution_failed = ["original", "counterexample", "restored"]
        .iter()
        .any(|name| {
            let leg = &observations["executions"][*name];
            match leg["status"].as_str() {
                Some("failed") => *name != "counterexample" || leg["failure_kind"] != "assertion",
                Some("passed") => *name == "counterexample",
                _ => false,
            }
        });
    if setup_failed || static_not_exposed || durable_not_ready || execution_failed {
        return Err("completed_repair contradicts supplied structured outcomes".to_string());
    }
    // Absent/incomplete observations are unknown, not a fabricated failure or
    // evidence for the outer judgment. The projection still preserves them as
    // NotMeasured and never emits an overall completed-repair metric.
    Ok(())
}

fn known_text<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty() && !matches!(*text, "unknown" | "not_measured"))
}

fn number(value: &Value, pointer: &str) -> Option<bigdecimal::BigDecimal> {
    let Value::Number(number) = value.pointer(pointer)? else {
        return None;
    };
    number.to_string().parse().ok()
}
