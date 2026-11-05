//! `cargo xtask token-cost` — measured output-cost scoreboard.
//!
//! Renders the dogfood fixtures (`boundary_gap`, `weak_error_oracle`) in
//! every check format and records bytes, finding/result counts, and
//! bytes-per-finding per format. Grain is one row per (fixture, format).
//! Advisory only: it never gates, it makes token cost visible so format
//! regressions are caught in review instead of in agent context windows.
//!
//! Definitions (do not change silently):
//! - `bytes`: exact stdout bytes of `ripr check --mode fast` for the row.
//! - `findings`: JSON `findings` array length for json/json-compact rows,
//!   SARIF `results` length for sarif rows; `null` for human rows, whose
//!   denominator is not machine-countable (bytes-per-finding stays null,
//!   never invented).
//! - `bytes_per_finding`: `bytes / findings` when findings > 0, else null.

use serde_json::{Value, json};
use std::fs;
use std::path::Path;

use super::fixtures::{FixtureCheckFormat, run_fixture_check};
use crate::normalize_path;

const FORMATS: [(&str, FixtureCheckFormat); 5] = [
    ("json", FixtureCheckFormat::Json),
    ("json-compact", FixtureCheckFormat::JsonCompact),
    ("sarif", FixtureCheckFormat::Sarif),
    ("human", FixtureCheckFormat::Human),
    ("human-full", FixtureCheckFormat::HumanFull),
];

#[derive(Debug, PartialEq)]
struct TokenCostRow {
    fixture: String,
    format: String,
    bytes: usize,
    findings: Option<usize>,
}

impl TokenCostRow {
    fn bytes_per_finding(&self) -> Option<f64> {
        match self.findings {
            Some(count) if count > 0 => Some(self.bytes as f64 / count as f64),
            _ => None,
        }
    }
}

fn findings_count(format: &str, rendered: &str) -> Option<usize> {
    let value: Value = serde_json::from_str(rendered).ok()?;
    match format {
        "json" | "json-compact" => value
            .get("findings")
            .and_then(Value::as_array)
            .map(Vec::len),
        "sarif" => value
            .get("runs")
            .and_then(Value::as_array)
            .and_then(|runs| runs.first())
            .and_then(|run| run.get("results"))
            .and_then(Value::as_array)
            .map(Vec::len),
        _ => None,
    }
}

fn measure_row(
    fixture: &str,
    format_name: &str,
    format: FixtureCheckFormat,
) -> Result<TokenCostRow, String> {
    let base = Path::new("fixtures").join(fixture);
    let root = normalize_path(&base.join("input"));
    let diff = normalize_path(&base.join("diff.patch"));
    let rendered = run_fixture_check(&root, &diff, format, None)?;
    Ok(TokenCostRow {
        fixture: fixture.to_string(),
        format: format_name.to_string(),
        bytes: rendered.len(),
        findings: findings_count(format_name, &rendered),
    })
}

fn report_json(rows: &[TokenCostRow], errors: &[String]) -> Value {
    json!({
        "schema_version": "token-cost-v1",
        "status": if errors.is_empty() { "pass" } else { "fail" },
        "advisory": true,
        "grain": "one row per (fixture, format); bytes are exact stdout bytes",
        "rows": rows.iter().map(|row| json!({
            "fixture": row.fixture,
            "format": row.format,
            "bytes": row.bytes,
            "findings": row.findings,
            "bytes_per_finding": row.bytes_per_finding(),
        })).collect::<Vec<_>>(),
        "errors": errors,
    })
}

fn report_markdown(rows: &[TokenCostRow], errors: &[String]) -> String {
    let mut out = String::from(
        "# token-cost scoreboard\n\n\
         Advisory measurement of `ripr check` output cost per format on the dogfood fixtures.\n\
         Grain: one row per (fixture, format). `bytes` is exact stdout bytes;\n\
         bytes-per-finding is null where the denominator is not machine-countable.\n\n",
    );
    if errors.is_empty() {
        out.push_str("Status: pass\n\n");
    } else {
        out.push_str("Status: fail\n\n");
        for error in errors {
            out.push_str(&format!("- error: {error}\n"));
        }
        out.push('\n');
    }
    out.push_str(
        "| fixture | format | bytes | findings | bytes/finding |\n|---|---|---:|---:|---:|\n",
    );
    for row in rows {
        let findings = row
            .findings
            .map(|count| count.to_string())
            .unwrap_or_else(|| "null".to_string());
        let per = row
            .bytes_per_finding()
            .map(|value| format!("{value:.0}"))
            .unwrap_or_else(|| "null".to_string());
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            row.fixture, row.format, row.bytes, findings, per
        ));
    }
    out
}

pub(crate) fn token_cost() -> Result<(), String> {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for scenario in crate::dogfood::dogfood_scenarios() {
        for (format_name, format) in FORMATS {
            match measure_row(&scenario.name, format_name, format) {
                Ok(row) => rows.push(row),
                Err(error) => errors.push(format!("{} {format_name}: {error}", scenario.name)),
            }
        }
    }
    let report_dir = Path::new("target").join("ripr").join("reports");
    fs::create_dir_all(&report_dir)
        .map_err(|err| format!("failed to create {}: {err}", normalize_path(&report_dir)))?;
    let json_path = report_dir.join("token-cost.json");
    let markdown_path = report_dir.join("token-cost.md");
    fs::write(&json_path, format!("{}\n", report_json(&rows, &errors)))
        .map_err(|err| format!("failed to write {}: {err}", normalize_path(&json_path)))?;
    fs::write(&markdown_path, report_markdown(&rows, &errors))
        .map_err(|err| format!("failed to write {}: {err}", normalize_path(&markdown_path)))?;
    println!(
        "token-cost: {} (wrote {} rows)",
        if errors.is_empty() { "pass" } else { "fail" },
        rows.len()
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "token-cost measurement failed: {}",
            errors.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_per_finding_needs_a_positive_denominator() {
        let row = |findings| TokenCostRow {
            fixture: "f".to_string(),
            format: "json".to_string(),
            bytes: 1000,
            findings,
        };
        assert_eq!(row(Some(4)).bytes_per_finding(), Some(250.0));
        assert_eq!(row(Some(0)).bytes_per_finding(), None);
        assert_eq!(row(None).bytes_per_finding(), None);
    }

    #[test]
    fn findings_count_reads_each_machine_format() {
        assert_eq!(
            findings_count("json", r#"{"findings":[{"id":"a"},{"id":"b"}]}"#),
            Some(2)
        );
        assert_eq!(
            findings_count("sarif", r#"{"runs":[{"results":[{"ruleId":"x"}]}]}"#),
            Some(1)
        );
        assert_eq!(findings_count("human", "anything"), None);
        assert_eq!(findings_count("json", "not json"), None);
    }
}
