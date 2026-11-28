//! Admission of baseline identities and file-backed event-count fractions.

use super::{SCHEMA_VERSION, Sample, SampleOutcome};
use serde_json::Value;
use std::collections::BTreeSet;

/// Admit a nonempty, unambiguous metric inventory before selecting baseline
/// rows. This is an identity check, not full validation of historical reports:
/// a valid baseline may cover only some of the currently configured metrics.
pub(crate) fn check_baseline(value: Value) -> Result<Value, String> {
    if value["schema_version"].as_str() != Some(SCHEMA_VERSION) {
        return Err(format!(
            "baseline schema_version must be `{SCHEMA_VERSION}`"
        ));
    }
    let rows = value["metrics"]
        .as_array()
        .ok_or("baseline has no metrics array")?;
    if rows.is_empty() {
        return Err(
            "baseline metrics array is empty; no baseline evidence was recorded".to_string(),
        );
    }
    let mut ids = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        let id = row
            .as_object()
            .and_then(|row| row.get("id"))
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| format!("baseline metrics[{index}] needs a non-empty string id"))?;
        if !ids.insert(id) {
            return Err(format!("duplicate baseline metric id `{id}`"));
        }
    }
    Ok(value)
}

/// Fractions describe event counts, not arbitrary numeric scores. Only 0/0
/// means no eligible cases; malformed or impossible counts fail the instrument.
pub(crate) fn ratio_sample(metric: &str, path: &str, pointer: &str, json: &Value) -> Sample {
    let mut node = json;
    for part in pointer.split('.').filter(|part| !part.is_empty()) {
        node = &node[part];
    }
    let (outcome, detail) = match (node["numerator"].as_u64(), node["denominator"].as_u64()) {
        (Some(n), Some(d)) if n > d => (
            SampleOutcome::Failed,
            format!("{path} {pointer}: numerator {n} exceeds denominator {d}"),
        ),
        (Some(0), Some(0)) => (
            SampleOutcome::NotMeasured,
            format!("{path} {pointer}: denominator is 0, no eligible cases yet"),
        ),
        (Some(n), Some(d)) => (
            SampleOutcome::Value(n as f64 / d as f64),
            format!("{path} {pointer}: {n}/{d}"),
        ),
        _ => (
            SampleOutcome::Failed,
            format!(
                "{path} {pointer}: numerator/denominator must be nonnegative integer counts, got {}/{}",
                node["numerator"], node["denominator"]
            ),
        ),
    };
    Sample {
        metric: metric.to_string(),
        repo: None,
        outcome,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::dx_scoreboard::{RunContext, build_report, file_samples, parse_config};
    use serde_json::json;
    use std::fs;
    use std::path::Path;

    #[test]
    fn baseline_rejects_empty_or_ambiguous_metric_inventory() {
        for rows in [
            json!([]),
            json!([null]),
            json!(["trust.rate"]),
            json!([{}]),
            json!([{"id": null}]),
            json!([{"id": 1}]),
            json!([{"id": ""}]),
            json!([{"id": "  \t"}]),
        ] {
            let baseline = json!({"schema_version": SCHEMA_VERSION, "metrics": rows});
            assert!(check_baseline(baseline).is_err(), "accepted {rows}");
        }
        for values in [[1, 9], [9, 1]] {
            let baseline = json!({
                "schema_version": SCHEMA_VERSION,
                "metrics": [
                    {"id": "ci.workflow_lines", "value": values[0]},
                    {"id": "ci.workflow_lines", "value": values[1]},
                ],
            });
            assert!(check_baseline(baseline).is_err_and(|error| {
                error.contains("duplicate baseline metric id `ci.workflow_lines`")
            }));
        }
    }

    #[test]
    fn baseline_keeps_valid_partial_and_committed_inventories() -> Result<(), String> {
        let partial = json!({
            "schema_version": SCHEMA_VERSION,
            "metrics": [{"id": "ci.workflow_lines", "value": 1}],
        });
        assert_eq!(check_baseline(partial.clone())?, partial);
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../metrics/dx-scoreboard");
        let mut count = 0;
        for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
            check_baseline(value).map_err(|error| format!("{}: {error}", path.display()))?;
            count += 1;
        }
        assert!(
            count >= 7,
            "the retained scoreboard baselines must be exercised"
        );
        Ok(())
    }

    #[test]
    fn ratio_distinguishes_valid_endpoints_and_empty_denominators() {
        for (n, d, expected) in [
            (0, 4, SampleOutcome::Value(0.0)),
            (1, 4, SampleOutcome::Value(0.25)),
            (4, 4, SampleOutcome::Value(1.0)),
            (0, 0, SampleOutcome::NotMeasured),
        ] {
            let sample = ratio_sample(
                "trust.rate",
                "receipt.json",
                "counts",
                &json!({"counts": {"numerator": n, "denominator": d}}),
            );
            assert_eq!(sample.outcome, expected, "{n}/{d}");
        }
    }

    #[test]
    fn malformed_file_fractions_fail_the_scoreboard_gate() -> Result<(), String> {
        let mut config = parse_config(
            r#"
schema_version = "ripr-dx-scoreboards-v1"
[[metric]]
id = "trust.rate"
board = "trust"
title = "Observed rate"
unit = "ratio"
direction = "lower_is_better"
target = 0
regression_pct = 0
regression_floor = 0
runner_dependent = false
source = "file:receipt.json#counts"
"#,
        )?;
        let scratch = crate::tests::temp_dir("dx-invalid-file-fractions");
        let path = scratch.join("receipt.json");
        let definition = config.metric.first_mut().ok_or("test metric missing")?;
        definition.source = format!("file:{}#counts", path.display());
        let boards = vec!["trust".to_string()];
        let context = RunContext::default();
        for counts in [
            json!({"numerator": -1, "denominator": 4}),
            json!({"numerator": 5, "denominator": 4}),
            json!({"numerator": 1, "denominator": 0}),
            json!({"numerator": 0, "denominator": -1}),
            json!({"numerator": 0.5, "denominator": 4}),
            json!({"numerator": 0, "denominator": 4.5}),
            json!({"numerator": "0", "denominator": 4}),
            json!({"numerator": 0, "denominator": true}),
            json!({"numerator": null, "denominator": 4}),
            json!({"denominator": 4}),
            json!({"numerator": 0}),
        ] {
            fs::write(&path, json!({"counts": counts}).to_string())
                .map_err(|error| error.to_string())?;
            let samples = file_samples(&config, &boards)?;
            assert_eq!(samples.len(), 1, "the file instrument must run");
            let report = build_report(&config, &boards, &samples, &context, None, true);
            assert_eq!(report["gate"]["status"], "fail", "{counts}");
            assert_eq!(report["gate"]["failed_instruments"], json!(["trust.rate"]));
            assert_eq!(report["metrics"][0]["status"], "failed");
            assert_eq!(report["metrics"][0]["value"], Value::Null);
            assert!(samples[0].detail.contains("receipt.json counts:"));
        }
        fs::remove_dir_all(&scratch).map_err(|error| error.to_string())?;
        Ok(())
    }
}
