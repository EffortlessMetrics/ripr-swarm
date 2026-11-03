//! Cache-schema currentness for the committed honesty reports (#12293).
//!
//! A repo-exposure capture records the `cache_schema_version` of the analyzer
//! that produced it (`source_subject.corpus.cache_schema_version`). The
//! regeneration guard judges seams and grips but not that field, so a capture
//! could stay a generation behind (`1.136` while the analyzer was at `1.144`)
//! and still read as current. Every committed report that carries the field
//! must equal `CACHE_SCHEMA_VERSION`, or be pinned in the corpus under
//! `report_cache_schema_pins` with the version it is intentionally held at and
//! a reason. A pin is checked against the report too: it must match the
//! report's recorded version and must be older than the current one, so a pin
//! cannot silently outlive the need for it.

use super::{EVIDENCE_PROMOTION_HONESTY_CORPUS, read_evidence_promotion_corpus};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const SEAM_CACHE_SOURCE: &str = "crates/ripr/src/analysis/seam_cache.rs";
const CONSTANT_PREFIX: &str = "pub(crate) const CACHE_SCHEMA_VERSION: &str = \"";

/// Read the analyzer's current full-cache generation from its source, so the
/// gate follows the constant instead of a copy that could itself go stale.
fn current_cache_schema_version(seam_cache_source: &str) -> Result<String, String> {
    seam_cache_source
        .lines()
        .find_map(|line| line.trim().strip_prefix(CONSTANT_PREFIX))
        .and_then(|rest| rest.split('"').next())
        .filter(|version| !version.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("could not read `CACHE_SCHEMA_VERSION` from {SEAM_CACHE_SOURCE}"))
}

/// Dotted numeric comparison (`1.74` < `1.146`). A version that does not parse
/// is not older, so a malformed pin fails instead of passing by accident.
fn is_older(version: &str, current: &str) -> bool {
    let parts = |text: &str| {
        text.split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    match (parts(version), parts(current)) {
        (Some(version), Some(current)) => version < current,
        _ => false,
    }
}

fn recorded_version(report: &Value) -> Option<&str> {
    report
        .pointer("/source_subject/corpus/cache_schema_version")
        .and_then(Value::as_str)
}

/// The pins as `report path -> (version, reason)`. A pin without a version or
/// a non-empty reason is itself a violation.
fn read_pins(corpus: &Value, violations: &mut Vec<String>) -> BTreeMap<String, (String, String)> {
    let mut pins = BTreeMap::new();
    let Some(entries) = corpus
        .get("report_cache_schema_pins")
        .and_then(Value::as_array)
    else {
        return pins;
    };
    for entry in entries {
        let text = |field: &str| {
            entry
                .get(field)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
        };
        let (report, version, reason) =
            (text("report"), text("cache_schema_version"), text("reason"));
        if report.is_empty() || version.is_empty() || reason.is_empty() {
            violations.push(format!(
                "report_cache_schema_pins entry `{report}`: needs `report`, `cache_schema_version` and a non-empty `reason` saying why this capture is held at an older generation (#12293)"
            ));
            continue;
        }
        if pins
            .insert(
                report.to_string(),
                (version.to_string(), reason.to_string()),
            )
            .is_some()
        {
            violations.push(format!(
                "report_cache_schema_pins: `{report}` is pinned twice (#12293)"
            ));
        }
    }
    pins
}

/// Judge each `(path, parsed report)` against the current generation and the
/// pins. Reports without `source_subject.corpus` are not repo-exposure
/// captures and are not judged; one that has `corpus` but no version fails.
fn check_reports(
    reports: &[(String, Value)],
    current: &str,
    pins: &BTreeMap<String, (String, String)>,
    violations: &mut Vec<String>,
) {
    let mut used = Vec::new();
    for (path, report) in reports {
        if report.pointer("/source_subject/corpus").is_none() {
            continue;
        }
        let Some(recorded) = recorded_version(report) else {
            violations.push(format!(
                "{path}: a repo-exposure capture without `source_subject.corpus.cache_schema_version` cannot be checked for currentness (#12293)"
            ));
            continue;
        };
        match pins.get(path) {
            Some((pinned, _)) => {
                used.push(path.as_str());
                if pinned != recorded {
                    violations.push(format!(
                        "{path}: pinned at cache schema `{pinned}` but the report records `{recorded}`; update the pin to the intended generation (#12293)"
                    ));
                } else if !is_older(pinned, current) {
                    violations.push(format!(
                        "{path}: pinned at cache schema `{pinned}` but a pin is only for a generation older than the current `{current}`; remove the pin or recapture the report (#12293)"
                    ));
                }
            }
            None if recorded != current => violations.push(format!(
                "{path}: records cache schema `{recorded}` but the analyzer is at `{current}`. Recapture it with `ripr check --mode draft --format repo-exposure-json --root <subject>` (never by hand), or, if an older generation is intended, pin it in `report_cache_schema_pins` with a reason (#12293)"
            )),
            None => {}
        }
    }
    for path in pins.keys() {
        if !used.contains(&path.as_str()) {
            violations.push(format!(
                "report_cache_schema_pins: `{path}` is not a committed repo-exposure capture; remove the pin (#12293)"
            ));
        }
    }
}

pub(super) fn check_report_cache_schemas(violations: &mut Vec<String>) -> Result<(), String> {
    let corpus_path = Path::new(EVIDENCE_PROMOTION_HONESTY_CORPUS);
    let corpus = read_evidence_promotion_corpus(corpus_path)?;
    let Some(parent) = corpus_path.parent() else {
        return Ok(());
    };
    let reports_dir = parent.join("reports");
    let mut paths = Vec::new();
    // A corpus without a `reports` directory has no captures to date. Only an
    // absent directory is optional: any other failure is an error.
    match std::fs::read_dir(&reports_dir) {
        Ok(entries) => {
            for entry in entries {
                let path = entry
                    .map_err(|error| format!("failed to list {}: {error}", reports_dir.display()))?
                    .path();
                if path.extension().is_some_and(|ext| ext == "json") {
                    paths.push(path);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!("failed to read {}: {error}", reports_dir.display()));
        }
    }
    paths.sort();
    let mut reports = Vec::new();
    for path in paths {
        let display = super::normalize_path(&path);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {display}: {error}"))?;
        let report: Value = serde_json::from_str(&text)
            .map_err(|error| format!("{display} is not JSON: {error}"))?;
        reports.push((display, report));
    }
    let pins = read_pins(&corpus, violations);
    if reports.is_empty() && pins.is_empty() {
        return Ok(());
    }
    let current = current_cache_schema_version(
        &std::fs::read_to_string(SEAM_CACHE_SOURCE)
            .map_err(|error| format!("failed to read {SEAM_CACHE_SOURCE}: {error}"))?,
    )?;
    check_reports(&reports, &current, &pins, violations);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn capture(version: &str) -> Value {
        json!({"source_subject": {"corpus": {"cache_schema_version": version}}})
    }

    fn run(reports: &[(&str, Value)], pins: &[(&str, &str, &str)]) -> Vec<String> {
        let reports: Vec<(String, Value)> = reports
            .iter()
            .map(|(path, report)| ((*path).to_string(), report.clone()))
            .collect();
        let pins = pins
            .iter()
            .map(|(path, version, reason)| {
                (
                    (*path).to_string(),
                    ((*version).to_string(), (*reason).to_string()),
                )
            })
            .collect();
        let mut violations = Vec::new();
        check_reports(&reports, "1.146", &pins, &mut violations);
        violations
    }

    #[test]
    fn a_report_at_the_current_generation_passes() {
        assert!(run(&[("r.json", capture("1.146"))], &[]).is_empty());
    }

    #[test]
    fn a_report_left_one_generation_behind_fails() {
        // The negative experiment: the same report one version behind.
        let found = run(&[("r.json", capture("1.145"))], &[]);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("records cache schema `1.145`"));
        assert!(found[0].contains("analyzer is at `1.146`"));
    }

    #[test]
    fn a_pinned_older_generation_passes_only_while_the_pin_matches() {
        let reports = [("r.json", capture("1.74"))];
        assert!(run(&reports, &[("r.json", "1.74", "historical capture")]).is_empty());
        let found = run(&reports, &[("r.json", "1.75", "historical capture")]);
        assert!(found[0].contains("pinned at cache schema `1.75`"));
    }

    #[test]
    fn a_pin_on_a_current_report_or_a_missing_report_is_a_violation() {
        let found = run(
            &[("r.json", capture("1.146"))],
            &[("r.json", "1.146", "why"), ("gone.json", "1.1", "why")],
        );
        assert!(
            found
                .iter()
                .any(|v| v.contains("only for a generation older"))
        );
        assert!(
            found
                .iter()
                .any(|v| v.contains("`gone.json` is not a committed"))
        );
    }

    #[test]
    fn a_capture_without_a_version_fails_and_a_non_capture_is_skipped() {
        let found = run(
            &[("r.json", json!({"source_subject": {"corpus": {}}}))],
            &[],
        );
        assert!(found[0].contains("cannot be checked for currentness"));
        assert!(run(&[("other.json", json!({"cases": []}))], &[]).is_empty());
    }

    #[test]
    fn pins_need_a_version_and_a_reason() {
        let corpus = json!({"report_cache_schema_pins": [
            {"report": "a.json", "cache_schema_version": "1.1", "reason": ""},
            {"report": "b.json", "cache_schema_version": "1.1", "reason": "r"},
            {"report": "b.json", "cache_schema_version": "1.1", "reason": "r"}
        ]});
        let mut violations = Vec::new();
        let pins = read_pins(&corpus, &mut violations);
        assert_eq!(pins.len(), 1);
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn the_current_generation_is_read_from_the_constant() -> Result<(), String> {
        let source = "/// doc\npub(crate) const CACHE_SCHEMA_VERSION: &str = \"1.146\";\n";
        assert_eq!(current_cache_schema_version(source)?, "1.146");
        assert_eq!(
            current_cache_schema_version("const OTHER: &str = \"1\";"),
            Err(format!(
                "could not read `CACHE_SCHEMA_VERSION` from {SEAM_CACHE_SOURCE}"
            ))
        );
        Ok(())
    }

    #[test]
    fn a_pin_must_be_older_than_the_current_generation() {
        // Numeric, not lexical: 1.74 is older than 1.146.
        assert!(is_older("1.74", "1.146"));
        assert!(!is_older("1.146", "1.146"));
        assert!(!is_older("1.147", "1.146"));
        assert!(!is_older("one", "1.146"));
        // A report and matching pin at a future generation still fail.
        let found = run(
            &[("r.json", capture("1.147"))],
            &[("r.json", "1.147", "why")],
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("only for a generation older"));
    }
}
