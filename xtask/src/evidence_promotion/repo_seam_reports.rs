//! Staleness guard for the repo-seam honesty reports (#12144).
//!
//! A repo-seam case judges a committed `ripr check --format repo-exposure-json`
//! capture. Nothing tied that capture to the analyzer: a capture taken with a
//! binary from before a fix still read as honest (#12132), and a later
//! regression of the subject could not turn the case red because the stored
//! file is what was judged. Every case with an `expected_repo_seam_grip`
//! assertion now names the Cargo subject the capture came from
//! (`report_subject`). The gate regenerates that subject with the current
//! binary and fails when the judged fields differ from the committed capture.
//!
//! Judged fields are the ones the corpus assertions and the honesty argument
//! rest on: the set of seams, each seam's `grip_class` and stage states, and
//! each related test's `oracle_strength` and `relation_reason`. Producer
//! version, cache schema, timestamps and the absolute root are expected to
//! differ between captures and are not judged.

use super::{
    EVIDENCE_PROMOTION_HONESTY_CORPUS, capture_output_with_timeout, normalize_path,
    read_evidence_promotion_corpus, ripr_debug_binary, run,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

const REGENERATION_TIMEOUT: Duration = Duration::from_mins(2);
const REGENERATION_ARGS: [&str; 5] = [
    "--mode",
    "draft",
    "--format",
    "repo-exposure-json",
    "--root",
];

/// Assertion types that judge a repo-exposure report's seams. A case using any
/// of them reads a stored capture, so each must be regenerable (#12188).
const REPO_SEAM_ASSERTIONS: [&str; 2] =
    ["expected_repo_seam_grip", "expected_repo_seam_related_test"];

/// A seam's identity within one report: file, line, column and kind. The
/// `seam_id` hash is deliberately not the key, so a change to how ids are
/// derived does not read as a different seam set.
type SeamKey = (String, u64, u64, String);

#[derive(Debug, PartialEq, Eq)]
struct JudgedSeam {
    /// The owning function; a seam that moved to another owner at the same
    /// position is a different seam.
    owner: String,
    /// The mutated expression text at the seam.
    expression: String,
    grip_class: String,
    stages: Vec<(String, String)>,
    related_tests: Vec<RelatedTest>,
}

/// A related test as judged: name, file, line, oracle strength, relation
/// reason. File and line are kept so a test that moved is a difference.
type RelatedTest = (String, String, u64, String, String);

fn string_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn judged_seams(report: &Value) -> Result<BTreeMap<SeamKey, JudgedSeam>, String> {
    let Some(seams) = report.get("seams").and_then(Value::as_array) else {
        return Err("report has no `seams` array".to_string());
    };
    let mut judged = BTreeMap::new();
    for seam in seams {
        let key = (
            string_field(seam, "file"),
            seam.get("line").and_then(Value::as_u64).unwrap_or(0),
            seam.get("column").and_then(Value::as_u64).unwrap_or(0),
            string_field(seam, "kind"),
        );
        let mut stages: Vec<(String, String)> = seam
            .get("evidence")
            .and_then(Value::as_object)
            .map(|stages| {
                stages
                    .iter()
                    .map(|(stage, state)| (stage.clone(), state.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        stages.sort();
        let mut related_tests: Vec<RelatedTest> = seam
            .get("related_tests")
            .and_then(Value::as_array)
            .map(|tests| {
                tests
                    .iter()
                    .map(|test| {
                        (
                            string_field(test, "name"),
                            string_field(test, "file"),
                            test.get("line").and_then(Value::as_u64).unwrap_or(0),
                            string_field(test, "oracle_strength"),
                            string_field(test, "relation_reason"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        related_tests.sort();
        let entry = JudgedSeam {
            owner: string_field(seam, "owner"),
            expression: string_field(seam, "expression"),
            grip_class: string_field(seam, "grip_class"),
            stages,
            related_tests,
        };
        if judged.insert(key.clone(), entry).is_some() {
            return Err(format!(
                "two seams share file `{}` line {} column {} kind `{}`",
                key.0, key.1, key.2, key.3
            ));
        }
    }
    Ok(judged)
}

/// Judged-field differences between a committed capture and a regeneration of
/// its subject. Empty means the capture still says what the analyzer says.
pub(super) fn compare_reports(
    case_label: &str,
    committed: &Value,
    regenerated: &Value,
) -> Vec<String> {
    let (committed, regenerated) = match (judged_seams(committed), judged_seams(regenerated)) {
        (Ok(committed), Ok(regenerated)) => (committed, regenerated),
        (Err(error), _) => return vec![format!("{case_label}: committed report: {error}")],
        (_, Err(error)) => return vec![format!("{case_label}: regenerated report: {error}")],
    };
    let mut violations = Vec::new();
    for (key, stored) in &committed {
        let location = format!("{}:{} ({})", key.0, key.1, key.3);
        let Some(fresh) = regenerated.get(key) else {
            violations.push(format!(
                "{case_label}: committed report has a seam at {location} that the current analyzer no longer produces"
            ));
            continue;
        };
        if stored.owner != fresh.owner {
            violations.push(format!(
                "{case_label}: seam at {location} is committed with owner `{}` but the current analyzer reports `{}`",
                stored.owner, fresh.owner
            ));
        }
        if stored.expression != fresh.expression {
            violations.push(format!(
                "{case_label}: seam at {location} is committed with expression `{}` but the current analyzer reports `{}`",
                stored.expression, fresh.expression
            ));
        }
        if stored.grip_class != fresh.grip_class {
            violations.push(format!(
                "{case_label}: seam at {location} is committed as grip_class `{}` but the current analyzer reports `{}`",
                stored.grip_class, fresh.grip_class
            ));
        }
        if stored.stages != fresh.stages {
            violations.push(format!(
                "{case_label}: seam at {location} committed stage states {:?} differ from the current analyzer's {:?}",
                stored.stages, fresh.stages
            ));
        }
        if stored.related_tests != fresh.related_tests {
            violations.push(format!(
                "{case_label}: seam at {location} committed related tests (name, file, line, oracle_strength, relation_reason) {:?} differ from the current analyzer's {:?}",
                stored.related_tests, fresh.related_tests
            ));
        }
    }
    for key in regenerated.keys() {
        if !committed.contains_key(key) {
            violations.push(format!(
                "{case_label}: the current analyzer reports a seam at {}:{} ({}) that the committed report does not carry",
                key.0, key.1, key.3
            ));
        }
    }
    violations
}

/// The cases that must be regenerable: every case that pins a repo seam, by
/// grip or by related test. A
/// case without `report_subject` would be judged from a stored file nothing
/// can refresh, which is the gap this guard closes, so it fails rather than
/// being skipped.
pub(super) fn repo_seam_cases(
    cases: &[Value],
    violations: &mut Vec<String>,
) -> Vec<(String, String, String)> {
    let mut regenerable = Vec::new();
    for case in cases {
        let id = case
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("<missing-id>");
        let pins_repo_seam = case
            .get("assertions")
            .and_then(Value::as_array)
            .is_some_and(|assertions| {
                assertions.iter().any(|assertion| {
                    matches!(
                        assertion.get("type").and_then(Value::as_str),
                        Some(kind) if REPO_SEAM_ASSERTIONS.contains(&kind)
                    )
                })
            });
        if !pins_repo_seam {
            continue;
        }
        let report = case.get("source_report").and_then(Value::as_str);
        let subject = case.get("report_subject").and_then(Value::as_str);
        match (report, subject) {
            (Some(report), Some(subject)) if !subject.trim().is_empty() => {
                regenerable.push((id.to_string(), report.to_string(), subject.to_string()));
            }
            _ => violations.push(format!(
                "evidence promotion case `{id}`: a case with `expected_repo_seam_grip` or `expected_repo_seam_related_test` needs `source_report` and `report_subject` (the Cargo subject the report was captured from), so the report can be regenerated and checked for staleness (#12144)"
            )),
        }
    }
    regenerable
}

fn regenerate(subject: &Path, binary: &Path) -> Result<Value, String> {
    let mut args = vec!["check".to_string()];
    for arg in REGENERATION_ARGS {
        args.push(arg.to_string());
    }
    args.push(normalize_path(subject));
    let output = capture_output_with_timeout(
        &binary.to_string_lossy(),
        &args,
        &[],
        REGENERATION_TIMEOUT,
        "check-evidence-promotion-honesty repo-seam report regeneration",
    )?;
    if output.timed_out {
        return Err(format!(
            "ripr check timed out after {}s",
            REGENERATION_TIMEOUT.as_secs()
        ));
    }
    if !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "ripr check exited non-zero: {}",
            output.stderr.trim()
        ));
    }
    serde_json::from_str(&output.stdout)
        .map_err(|error| format!("ripr check stdout was not JSON: {error}"))
}

/// Regenerate each repo-seam report from its subject with the current binary
/// and compare the judged fields. Violations name the case, the seam and both
/// values, and the rerun command.
pub(super) fn check_regenerated_reports(violations: &mut Vec<String>) -> Result<(), String> {
    let corpus = read_evidence_promotion_corpus(Path::new(EVIDENCE_PROMOTION_HONESTY_CORPUS))?;
    let Some(cases) = corpus.get("cases").and_then(Value::as_array) else {
        return Ok(());
    };
    let regenerable = repo_seam_cases(cases, violations);
    if regenerable.is_empty() {
        return Ok(());
    }
    run("cargo", &["build", "-p", "ripr", "--quiet"]).map_err(|error| {
        format!("failed to build ripr to regenerate repo-seam reports: {error}")
    })?;
    let binary = ripr_debug_binary();
    for (id, report_path, subject_path) in regenerable {
        let case_label = format!("evidence promotion case `{id}`");
        let subject = Path::new(&subject_path);
        if !subject.join("Cargo.toml").is_file() {
            violations.push(format!(
                "{case_label}: report_subject `{subject_path}` is not a Cargo subject (no Cargo.toml)"
            ));
            continue;
        }
        let committed_text = std::fs::read_to_string(&report_path)
            .map_err(|error| format!("{case_label}: failed to read {report_path}: {error}"))?;
        let committed: Value = serde_json::from_str(&committed_text)
            .map_err(|error| format!("{case_label}: {report_path} is not JSON: {error}"))?;
        match regenerate(subject, &binary) {
            Ok(regenerated) => {
                let found = compare_reports(&case_label, &committed, &regenerated);
                if !found.is_empty() {
                    violations.extend(found);
                    violations.push(format!(
                        "{case_label}: the committed report is stale or the analyzer regressed. Reproduce with `cargo run -p ripr -- check --root {subject_path} --mode draft --format repo-exposure-json`; if the new output is the intended behavior, recapture {report_path} from it, otherwise fix the analyzer (never edit the report by hand)"
                    ));
                }
            }
            Err(error) => violations.push(format!(
                "{case_label}: could not regenerate {subject_path}: {error}"
            )),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn report(grip: &str, propagate: &str, strength: &str) -> Value {
        json!({
            "seams": [{
                "seam_id": "a1",
                "file": "src/lib.rs", "line": 5, "column": 8, "kind": "predicate_boundary",
                "grip_class": grip,
                "evidence": {"reach": "yes", "activate": "yes", "propagate": propagate,
                             "observe": "yes", "discriminate": "yes"},
                "owner": "src/lib.rs::check",
                "expression": "a > b",
                "related_tests": [
                    {"name": "pins_it", "file": "tests/t.rs", "line": 12,
                     "oracle_strength": strength,
                     "relation_reason": "direct_owner_call"}
                ]
            }]
        })
    }

    #[test]
    fn an_unchanged_capture_has_no_differences() {
        let committed = report("strongly_gripped", "yes", "strong");
        let mut regenerated = committed.clone();
        // Fields outside the judged set are free to move between captures.
        regenerated["seams"][0]["seam_id"] = json!("b2");
        regenerated["artifact"] = json!({"generated_at": "unix_ms:2"});
        assert!(compare_reports("case", &committed, &regenerated).is_empty());
    }

    #[test]
    fn a_changed_grip_class_stage_or_related_oracle_is_a_difference() {
        let committed = report("strongly_gripped", "yes", "strong");
        for (fresh, needle) in [
            (report("weakly_gripped", "yes", "strong"), "grip_class"),
            (
                report("strongly_gripped", "unknown", "strong"),
                "stage states",
            ),
            // The #12060 shape: the seam class holds while the published
            // oracle of a related test changes.
            (report("strongly_gripped", "yes", "none"), "related tests"),
        ] {
            let found = compare_reports("case", &committed, &fresh);
            assert_eq!(found.len(), 1, "{needle}: {found:?}");
            assert!(found[0].contains(needle), "{needle}: {found:?}");
        }
    }

    #[test]
    fn a_changed_owner_expression_or_test_location_is_a_difference() {
        let committed = report("strongly_gripped", "yes", "strong");
        type Edit = fn(&mut Value);
        let edits: [(&str, Edit); 4] = [
            ("owner", |r| {
                r["seams"][0]["owner"] = json!("src/lib.rs::other")
            }),
            ("expression", |r| {
                r["seams"][0]["expression"] = json!("a >= b")
            }),
            ("related tests", |r| {
                r["seams"][0]["related_tests"][0]["line"] = json!(99)
            }),
            ("related tests", |r| {
                r["seams"][0]["related_tests"][0]["file"] = json!("tests/moved.rs")
            }),
        ];
        for (needle, edit) in edits {
            let mut fresh = committed.clone();
            edit(&mut fresh);
            let found = compare_reports("case", &committed, &fresh);
            assert_eq!(found.len(), 1, "{needle}: {found:?}");
            assert!(found[0].contains(needle), "{needle}: {found:?}");
        }
    }

    #[test]
    fn a_seam_present_on_only_one_side_is_a_difference() {
        let committed = report("strongly_gripped", "yes", "strong");
        let empty = json!({"seams": []});
        let gone = compare_reports("case", &committed, &empty);
        assert_eq!(gone.len(), 1, "{gone:?}");
        assert!(gone[0].contains("no longer produces"), "{gone:?}");
        let added = compare_reports("case", &empty, &committed);
        assert_eq!(added.len(), 1, "{added:?}");
        assert!(added[0].contains("does not carry"), "{added:?}");
    }

    #[test]
    fn a_report_without_seams_is_an_error_not_a_pass() {
        let committed = report("strongly_gripped", "yes", "strong");
        let found = compare_reports("case", &committed, &json!({"findings": []}));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("no `seams` array"), "{found:?}");
    }

    #[test]
    fn a_repo_seam_case_must_name_a_regenerable_subject() {
        let pinned = |extra: Value| {
            let mut case = json!({
                "id": "c",
                "source_report": "fixtures/x/report.json",
                "assertions": [{"type": "expected_repo_seam_grip"}]
            });
            if let (Some(case), Some(extra)) = (case.as_object_mut(), extra.as_object()) {
                case.extend(extra.clone());
            }
            case
        };
        let mut violations = Vec::new();
        let found = repo_seam_cases(&[pinned(json!({}))], &mut violations);
        assert!(found.is_empty());
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("report_subject"), "{violations:?}");

        let mut violations = Vec::new();
        let found = repo_seam_cases(
            &[pinned(json!({"report_subject": "fixtures/x/subject"}))],
            &mut violations,
        );
        assert!(violations.is_empty(), "{violations:?}");
        assert_eq!(found.len(), 1);

        // A related-test-only case reads the same stored capture, so it needs a
        // subject too (#12188).
        let related_only = json!({
            "id": "r",
            "source_report": "fixtures/x/report.json",
            "assertions": [{"type": "expected_repo_seam_related_test"}]
        });
        let mut violations = Vec::new();
        assert!(repo_seam_cases(&[related_only], &mut violations).is_empty());
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations[0].contains("expected_repo_seam_related_test"),
            "{violations:?}"
        );

        // A case that pins no repo seam is not subject to the rule.
        let other = json!({"id": "o", "assertions": [{"type": "must_not_promote"}]});
        let mut violations = Vec::new();
        assert!(repo_seam_cases(&[other], &mut violations).is_empty());
        assert!(violations.is_empty(), "{violations:?}");
    }
}
