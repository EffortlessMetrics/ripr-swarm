//! Parser for Stryker's `mutation-testing-report-schema` JSON (the file the
//! `json` reporter writes, `reports/mutation/mutation.json`), producing the
//! language-neutral [`MutationSource`] values the synthetic-diff join scores.
//!
//! Schema fields used: `files{<path>: {language, source, mutants[]}}` with
//! each mutant's `id`, `mutatorName`, `replacement`, `location` (1-based
//! `line` and `column`; the `end` position is exclusive) and `status`.
//! Columns are UTF-16 offsets, as JavaScript counts them, so they convert to
//! the join's 0-based form by subtracting one.
//!
//! A mutant missing a required field fails the parse: a silently skipped
//! mutant would leave the population short of the real set.

use super::synthetic_diff::{MutationSource, SyntheticMutant};
use serde_json::{Value, json};

/// Join outcome token for a Stryker status. Only the detected status is
/// `caught` and only the undetected and no-coverage statuses are `missed`;
/// every other status keeps
/// its own token and is never scored.
pub(crate) fn outcome_of_status(status: &str) -> String {
    match status {
        "Killed" => "caught", // ripr-allow: static-language: Stryker status name
        "Survived" | "NoCoverage" => "missed", // ripr-allow: static-language: Stryker status name
        "Timeout" => "timeout",
        "CompileError" => "unviable",
        "Ignored" => "ignored",
        "RuntimeError" => "runtime_error",
        "Pending" => "pending",
        _ => "unknown_status",
    }
    .to_string()
}

fn position(value: Option<&Value>, what: &str) -> Result<(u64, u64), String> {
    let field = |key: &str| {
        value
            .and_then(|position| position.get(key))
            .and_then(Value::as_u64)
            .filter(|number| *number >= 1)
            .ok_or_else(|| format!("{what}.{key} is missing or not a 1-based integer"))
    };
    Ok((field("line")?, field("column")? - 1))
}

/// Parse a Stryker report into sources (sorted by path, mutants by position)
/// and a tool-identity object for the receipt.
pub(crate) fn parse_stryker_report(report: &Value) -> Result<(Vec<MutationSource>, Value), String> {
    let files = report
        .get("files")
        .and_then(Value::as_object)
        .ok_or("Stryker report has no `files` object")?;
    let mut sources = Vec::new();
    for (path, file) in files {
        let source = file
            .get("source")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Stryker report file `{path}` has no string `source`"))?;
        let mutants = file
            .get("mutants")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("Stryker report file `{path}` has no `mutants` array"))?;
        let mut parsed = Vec::new();
        for (index, mutant) in mutants.iter().enumerate() {
            let what = format!("`{path}` mutant {}", index + 1);
            let text = |key: &str| {
                mutant
                    .get(key)
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{what} has no string `{key}`"))
            };
            let status = text("status")?;
            let (start_line, start_col) = position(
                mutant.pointer("/location/start"),
                &format!("{what} location.start"),
            )?;
            let (end_line, end_col) = position(
                mutant.pointer("/location/end"),
                &format!("{what} location.end"),
            )?;
            parsed.push(SyntheticMutant {
                id: text("id")?.to_string(),
                mutator: text("mutatorName")?.to_string(),
                replacement: text("replacement")?.to_string(),
                start_line,
                start_col,
                end_line,
                end_col,
                status: status.to_string(),
                outcome: outcome_of_status(status),
            });
        }
        parsed.sort_by(|a, b| {
            (a.start_line, a.start_col, a.end_line, a.end_col, &a.id).cmp(&(
                b.start_line,
                b.start_col,
                b.end_line,
                b.end_col,
                &b.id,
            ))
        });
        sources.push(MutationSource {
            path: path.clone(),
            source: source.to_string(),
            mutants: parsed,
        });
    }
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    let tool = json!({
        "name": "stryker",
        "report_schema_version": report.get("schemaVersion").and_then(Value::as_str),
        "version": report.pointer("/framework/version").and_then(Value::as_str),
    });
    Ok((sources, tool))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Value {
        json!({
            "schemaVersion": "1",
            "thresholds": {"high": 80, "low": 60},
            "framework": {"name": "StrykerJS", "version": "8.7.1"},
            "files": {
                "src/b.ts": {
                    "language": "typescript",
                    "source": "export const b = 1;\n",
                    "mutants": []
                },
                "src/a.ts": {
                    "language": "typescript",
                    "source": "export const a = x >= 1;\n",
                    "mutants": [
                        {"id": "2", "mutatorName": "EqualityOperator", "replacement": ">",
                         "location": {"start": {"line": 1, "column": 20}, "end": {"line": 1, "column": 22}},
                         "status": "Survived"}, // ripr-allow: static-language: Stryker status name
                        {"id": "1", "mutatorName": "BooleanLiteral", "replacement": "false",
                         "location": {"start": {"line": 1, "column": 5}, "end": {"line": 1, "column": 6}},
                         "status": "Killed", "statusReason": "ignored extra field"} // ripr-allow: static-language: Stryker status name
                    ]
                }
            }
        })
    }

    #[test]
    fn parses_sorted_sources_zero_based_columns_and_outcomes() -> Result<(), String> {
        let (sources, tool) = parse_stryker_report(&report())?;
        assert_eq!(
            sources.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
            ["src/a.ts", "src/b.ts"]
        );
        let first = &sources[0].mutants[0];
        assert_eq!(first.id, "1");
        assert_eq!((first.start_line, first.start_col), (1, 4));
        assert_eq!((first.end_line, first.end_col), (1, 5));
        assert_eq!(first.outcome, "caught");
        assert_eq!(sources[0].mutants[1].outcome, "missed");
        assert_eq!(tool["name"], "stryker");
        assert_eq!(tool["version"], "8.7.1");
        assert_eq!(tool["report_schema_version"], "1");
        Ok(())
    }

    #[test]
    fn status_mapping_scores_only_killed_survived_and_nocoverage() {
        for (status, outcome) in [
            ("Killed", "caught"),   // ripr-allow: static-language: Stryker status name
            ("Survived", "missed"), // ripr-allow: static-language: Stryker status name
            ("NoCoverage", "missed"),
            ("Timeout", "timeout"),
            ("CompileError", "unviable"),
            ("Ignored", "ignored"),
            ("RuntimeError", "runtime_error"),
            ("Pending", "pending"),
            ("SomethingNew", "unknown_status"),
        ] {
            assert_eq!(outcome_of_status(status), outcome, "{status}");
        }
    }

    #[test]
    fn a_malformed_report_or_mutant_is_refused_not_skipped() {
        assert!(parse_stryker_report(&json!({})).err().is_some());
        let mut no_source = report();
        no_source["files"]["src/b.ts"]
            .as_object_mut()
            .map(|file| file.remove("source"));
        assert!(parse_stryker_report(&no_source).err().is_some());
        let mut bad_location = report();
        bad_location["files"]["src/a.ts"]["mutants"][0]["location"]["start"]["column"] = json!(0);
        let err = parse_stryker_report(&bad_location)
            .err()
            .unwrap_or_default();
        assert!(err.contains("location.start.column"), "{err}");
        let mut no_status = report();
        no_status["files"]["src/a.ts"]["mutants"][1]
            .as_object_mut()
            .map(|mutant| mutant.remove("status"));
        assert!(parse_stryker_report(&no_status).err().is_some());
    }
}
