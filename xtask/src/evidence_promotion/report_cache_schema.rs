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
//! The opt-in producer check compares clean captures with their producing
//! commits and requires pins for dirty captures (#15533).

use super::{
    EVIDENCE_PROMOTION_HONESTY_CORPUS, is_exact_git_commit, read_evidence_promotion_corpus,
};
use crate::run::capture_process_output;
use ra_ap_syntax::{
    Edition, SourceFile,
    ast::{self, HasAttrs, HasModuleItem, HasName, LiteralKind},
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const SEAM_CACHE_SOURCE: &str = "crates/ripr/src/analysis/seam_cache.rs";

/// Read the analyzer's current full-cache generation from its source, so the
/// gate follows the constant instead of a copy that could itself go stale.
pub(super) fn current_cache_schema_version(seam_cache_source: &str) -> Result<String, String> {
    let missing = || format!("could not read `CACHE_SCHEMA_VERSION` from {SEAM_CACHE_SOURCE}");
    let parsed = SourceFile::parse(seam_cache_source, Edition::Edition2024);
    if !parsed.errors().is_empty() {
        return Err(missing());
    }
    let mut declarations = parsed.tree().items().filter_map(|item| match item {
        ast::Item::Const(constant)
            if constant
                .name()
                .is_some_and(|name| name.text() == "CACHE_SCHEMA_VERSION") =>
        {
            Some(constant)
        }
        _ => None,
    });
    let constant = declarations.next().ok_or_else(missing)?;
    // Conditional definitions require knowing the producing build's cfg.
    if declarations.next().is_some() || constant.attrs().next().is_some() {
        return Err(missing());
    }
    let Some(ast::Expr::Literal(literal)) = constant.body() else {
        return Err(missing());
    };
    let LiteralKind::String(value) = literal.kind() else {
        return Err(missing());
    };
    value
        .value()
        .ok()
        .filter(|version| !version.is_empty())
        .map(|version| version.into_owned())
        .ok_or_else(missing)
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

/// The generation the analyzer is at now, read from its source in this tree.
/// One reader, so the currentness guard and the summary comparison can never
/// disagree about which generation is current.
pub(super) fn current_generation() -> Result<String, String> {
    current_cache_schema_version(
        &std::fs::read_to_string(SEAM_CACHE_SOURCE)
            .map_err(|error| format!("failed to read {SEAM_CACHE_SOURCE}: {error}"))?,
    )
}

pub(super) fn recorded_version(report: &Value) -> Option<&str> {
    report
        .pointer("/source_subject/corpus/cache_schema_version")
        .and_then(Value::as_str)
}

fn producer_commit(report: &Value) -> Option<(&str, bool)> {
    let stamp = report
        .pointer("/analysis_inputs/analyzer_version")?
        .as_str()?;
    let (version, build) = stamp.split_once('+')?;
    if version.is_empty() {
        return None;
    }
    // The build identity precedes the source digest and language features.
    let build = build.split('+').next()?;
    let dirty_commit = build.strip_suffix("-dirty");
    let commit = dirty_commit.unwrap_or(build);
    is_exact_git_commit(commit).then_some((commit, dirty_commit.is_some()))
}

fn producer_generation(commit: &str) -> Result<String, String> {
    let cat_file = |kind: &str, object: String| {
        let bytes = capture_process_output(
            "git",
            &[
                "--no-replace-objects".into(),
                "cat-file".into(),
                kind.into(),
                object,
            ],
            &[("GIT_NO_LAZY_FETCH", "1")],
        )
        .map_err(|error| error.message)?;
        String::from_utf8(bytes).map_err(|error| error.to_string())
    };
    if cat_file("-t", commit.into())?.trim() != "commit" {
        return Err(format!("Git object `{commit}` is not a commit"));
    }
    let source = cat_file("blob", format!("{commit}:{SEAM_CACHE_SOURCE}"))?;
    current_cache_schema_version(&source)
}

/// Whether a capture's per-stage `evidence_path` summaries are judged. Only a
/// capture recorded at the analyzer's current generation is compared: a pin
/// holds an older capture at its recorded generation, so judging it would test
/// a claim the pin does not make and leave the pin's own reason unverified
/// (#14555).
pub(super) fn judges_evidence_path(report: &Value, current: &str) -> bool {
    recorded_version(report) == Some(current)
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
    mut read_producer_generation: Option<impl FnMut(&str) -> Result<String, String>>,
) {
    let mut used = Vec::new();
    let mut producer_generations = BTreeMap::new();
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
        let pin = pins.get(path);
        match pin {
            Some((pinned, _)) => {
                used.push(path.as_str());
                if pinned != recorded {
                    violations.push(format!(
                        "{path}: pinned at cache schema `{pinned}` but the report records `{recorded}`; update the pin to the intended generation (#12293)"
                    ));
                    continue;
                } else if !is_older(pinned, current) {
                    violations.push(format!(
                        "{path}: pinned at cache schema `{pinned}` but a pin is only for a generation older than the current `{current}`; remove the pin or recapture the report (#12293)"
                    ));
                    continue;
                }
            }
            None if recorded != current => {
                violations.push(format!(
                    "{path}: records cache schema `{recorded}` but the analyzer is at `{current}`. Recapture it with `ripr check --mode draft --format repo-exposure-json --root <subject>` (never by hand), or, if an older generation is intended, pin it in `report_cache_schema_pins` with a reason (#12293)"
                ));
                continue;
            }
            None => {}
        }
        let Some(read_producer_generation) = read_producer_generation.as_mut() else {
            continue;
        };
        let Some((commit, dirty)) = producer_commit(report) else {
            violations.push(format!(
                "{path}: missing producer evidence: `analysis_inputs.analyzer_version` must identify a full build commit (#15533)"
            ));
            continue;
        };
        if dirty {
            if pin.is_none() {
                violations.push(format!(
                    "{path}: dirty producer `{commit}` needs a `report_cache_schema_pins` entry. Recapture current reports with a clean build (#15533)"
                ));
            }
            continue;
        }
        match producer_generations
            .entry(commit)
            .or_insert_with(|| read_producer_generation(commit))
        {
            Ok(produced) if produced.as_str() != recorded => violations.push(format!(
                "{path}: records cache schema `{recorded}` but clean producer `{commit}` declares `{produced}`; recapture with the intended build (#15533)"
            )),
            Err(error) => violations.push(format!(
                "{path}: missing cache-schema evidence for clean producer `{commit}`: {error}. Make its source commit available locally and rerun the gate (#15533)"
            )),
            Ok(_) => {}
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

pub(super) fn check_report_cache_schemas(
    violations: &mut Vec<String>,
    verify_producers: bool,
) -> Result<(), String> {
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
    if verify_producers
        && !reports
            .iter()
            .any(|(_, report)| recorded_version(report).is_some())
    {
        return Err(format!(
            "no schema-bearing captures in {}",
            reports_dir.display()
        ));
    }
    if reports.is_empty() && pins.is_empty() {
        return Ok(());
    }
    let current = current_generation()?;
    check_reports(
        &reports,
        &current,
        &pins,
        violations,
        verify_producers.then_some(producer_generation),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CURRENT_COMMIT: &str = "1111111111111111111111111111111111111111";
    const OLDER_COMMIT: &str = "2222222222222222222222222222222222222222";
    const MISSING_COMMIT: &str = "3333333333333333333333333333333333333333";

    fn capture(version: &str) -> Value {
        capture_at(version, CURRENT_COMMIT)
    }

    fn capture_at(version: &str, build: &str) -> Value {
        json!({
            "source_subject": {"corpus": {"cache_schema_version": version}},
            "analysis_inputs": {
                "analyzer_version": format!("0.11.0+{build}+languages:rust=1,typescript=1,python=1,perl=0"),
            },
        })
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
        check_reports(
            &reports,
            "1.146",
            &pins,
            &mut violations,
            Some(|commit: &str| match commit {
                CURRENT_COMMIT => Ok("1.146".into()),
                OLDER_COMMIT => Ok("1.74".into()),
                _ => Err("commit unavailable".into()),
            }),
        );
        violations
    }

    #[test]
    fn required_currentness_does_not_resolve_producers() {
        let mut violations = Vec::new();
        check_reports(
            &[("r.json".into(), capture_at("1.146", MISSING_COMMIT))],
            "1.146",
            &BTreeMap::new(),
            &mut violations,
            None::<fn(&str) -> Result<String, String>>,
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn comment_and_string_lookalikes_cannot_supply_the_schema() -> Result<(), String> {
        let decoys = r###"
/*
/* Nested comment */
pub(crate) const CACHE_SCHEMA_VERSION: &str = "1.146";
*/
const RAW: &str = r#"
pub(crate) const CACHE_SCHEMA_VERSION: &str = "1.146";
"#;
"###;
        assert!(current_cache_schema_version(decoys).is_err());
        let source = format!("{decoys}\npub(crate) const CACHE_SCHEMA_VERSION: &str = \"1.74\";");
        assert_eq!(current_cache_schema_version(&source)?, "1.74");
        Ok(())
    }

    #[test]
    fn a_conditional_or_duplicate_schema_declaration_is_ambiguous() {
        let declaration = "pub(crate) const CACHE_SCHEMA_VERSION: &str = \"1.146\";";
        assert!(current_cache_schema_version(&format!("#[cfg(any())]\n{declaration}")).is_err());
        assert!(current_cache_schema_version(&format!("{declaration}\n{declaration}")).is_err());
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
        let reports = [("r.json", capture_at("1.74", OLDER_COMMIT))];
        assert!(run(&reports, &[("r.json", "1.74", "historical capture")]).is_empty());
        let found = run(&reports, &[("r.json", "1.75", "historical capture")]);
        assert!(found[0].contains("pinned at cache schema `1.75`"));
    }

    #[test]
    fn a_current_report_from_an_older_producer_fails() {
        let reports = [("r.json", capture_at("1.146", OLDER_COMMIT))];
        let found = run(&reports, &[]);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains(&format!(
            "records cache schema `1.146` but clean producer `{OLDER_COMMIT}` declares `1.74`"
        )));
    }

    #[test]
    fn a_matching_pin_cannot_override_a_clean_producer() {
        let found = run(
            &[("r.json", capture("1.74"))],
            &[("r.json", "1.74", "historical capture")],
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].contains(&format!(
            "records cache schema `1.74` but clean producer `{CURRENT_COMMIT}` declares `1.146`"
        )));
    }

    #[test]
    fn a_dirty_producer_needs_a_valid_pin() {
        let build = format!("{OLDER_COMMIT}-dirty+src:00000000000000aa");
        let found = run(&[("r.json", capture_at("1.146", &build))], &[]);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("needs a `report_cache_schema_pins` entry"));
        assert!(
            run(
                &[("r.json", capture_at("1.1", &build))],
                &[("r.json", "1.1", "historical capture")],
            )
            .is_empty()
        );
    }

    #[test]
    fn an_unidentified_or_unavailable_producer_fails_closed() {
        let unidentified = json!({"source_subject": {"corpus": {"cache_schema_version": "1.146"}}});
        let found = run(
            &[
                ("unstamped.json", unidentified),
                ("unknown.json", capture_at("1.146", "unknown")),
                ("unavailable.json", capture_at("1.146", MISSING_COMMIT)),
            ],
            &[],
        );
        assert_eq!(found.len(), 3);
        assert!(found[0].contains("unstamped.json: missing producer evidence"));
        assert!(found[1].contains("unknown.json: missing producer evidence"));
        assert!(found[2].contains("unavailable.json: missing cache-schema evidence"));
        assert!(found[2].contains(MISSING_COMMIT));
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
