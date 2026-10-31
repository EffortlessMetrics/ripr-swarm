//! SARIF 2.1.0 renderer for static `ripr` evidence.
//!
//! This module renders existing Finding and classified-seam facts. It does not
//! classify, suppress, or compare baselines; those decisions belong to
//! analysis, config/suppression loading, and future CI policy code.

use crate::analysis::ClassifiedSeam;
use crate::analysis::SeamLimitInfo;
use crate::analysis::seams::SeamGripClass;
use crate::analysis_outcome::AnalysisOutcome;
use crate::app::CheckOutput;
use crate::config::{ConfigSeverity, RiprConfig};
use crate::domain::{
    ExposureClass, Finding, LanguageId, LanguageStatus, MissingDiscriminatorFact, RelatedTest,
    StageEvidence,
};
use crate::output::next_step::reconcile_next_step;
use crate::output::observed_values::{bounded_observed_values, elided_observed_values_total};
use crate::output::path::{display_path_text, path_display_lossy, repository_relative_path};
use crate::output::perl_preview_card::perl_preview_card_json;
use crate::output::preview_actionability::{
    preview_actionability_for, preview_actionability_json_value,
};
use crate::output::python_repair_card::{python_repair_card, python_repair_card_json_value};
use crate::output::suppressions::{
    SuppressionEntry, SuppressionKind, current_iso_date, is_expired,
};
use crate::output::typescript_preview_card::{
    typescript_preview_card, typescript_preview_card_json_value,
};
use serde_json::{Map, Value, json};
use std::path::Path;

const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const SARIF_VERSION: &str = "2.1.0";
const RIPR_SARIF_SCHEMA_VERSION: &str = "0.1";
/// Match the producers' Unicode scalar-value columns. SARIF 2.1.0 §3.14.27
/// requires this declaration for nonempty text results; empty runs may carry it.
const SARIF_COLUMN_KIND: &str = "unicodeCodePoints";
/// Per-class rule help anchors (#7371, #7397). Finding rules point at the
/// exposure class table a maintainer landing on a code-scanning rule page
/// needs; seam rules point at the seam inventory spec's classification
/// contract. The SARIF policy spec (RIPR-SPEC-0008) documents severity
/// mapping, `--root` stability, and RFC 3986 URI encoding, not per-class
/// meaning, so it no longer backs every rule's helpUri. Cited docs use the
/// current `ripr-swarm` blob so code-scanning help tracks the emitting
/// development trunk instead of a stale `EffortlessMetrics/ripr` snapshot.
const STATIC_EXPOSURE_MODEL_URI: &str = "https://github.com/EffortlessMetrics/ripr-swarm/blob/main/docs/STATIC_EXPOSURE_MODEL.md#exposure-classes";
const SEAM_GRIP_SPEC_URI: &str = "https://github.com/EffortlessMetrics/ripr-swarm/blob/main/docs/specs/RIPR-SPEC-0005-repo-seam-inventory.md";
const PYTHON_PREVIEW_AUTHORITY_BOUNDARY: &str = "preview_advisory_only";

/// Render diff-scoped Findings as SARIF.
pub(crate) fn render_findings_sarif(
    output: &CheckOutput,
    config: &RiprConfig,
    suppressions: &[SuppressionEntry],
) -> String {
    let today = current_iso_date();
    let rules = finding_rules();
    // #5091: count what the candidate-actionable filter drops so an
    // all-omitted complete run is not indistinguishable from a clean run.
    // This is the SARIF sibling of the GitHub annotation denominator (#4393).
    let omitted_not_current = output
        .findings
        .iter()
        .filter(|finding| !finding.is_candidate_actionable())
        .count();
    let results = output
        .findings
        .iter()
        // Candidate-actionable eligibility (#3281): a SARIF result is a
        // current diagnostic for CI annotation consumers; base-side
        // evidence and unresolved subjects stay visible on the check JSON
        // and human surfaces instead of becoming annotation obligations.
        .filter(|finding| finding.is_candidate_actionable())
        .filter_map(|finding| finding_result(&output.root, finding, config, suppressions, &today))
        .collect::<Vec<_>>();
    let (results, folded_same_line) = fold_same_line_results(results);
    sarif_document(
        "finding",
        rules,
        results,
        output.analysis_outcome.as_ref(),
        output.analyzed_revisions.as_ref(),
        omitted_not_current,
        folded_same_line,
    )
}

/// One alert per changed line and class (#11219): two probes of the same
/// class on one line (for example an `error_path` and a `return_value`
/// probe) read as one edit to the author, yet code scanning opens one alert
/// for each. The kept result is the one with the smallest fingerprint, so a
/// reorder of the findings never changes which alert survives; it keeps its
/// message, fingerprint and location. The others are named, sorted by
/// fingerprint, in its `properties.folded_results` and counted for the run,
/// so nothing is dropped silently. Results with different classes or
/// different suppression metadata, and results without a line, stay
/// separate: a weaker class never hides a stronger one and a suppressed
/// result never hides an active one. Returns the kept results, in the order
/// their groups first appear, and how many were folded.
fn fold_same_line_results(results: Vec<Value>) -> (Vec<Value>, usize) {
    type Key = (String, String, u64, String);
    let mut groups: Vec<Vec<Value>> = Vec::new();
    let mut index: std::collections::HashMap<Key, usize> = std::collections::HashMap::new();
    for result in results {
        let region = &result["locations"][0]["physicalLocation"];
        let key = region["region"]["startLine"].as_u64().map(|line| {
            (
                result["ruleId"].as_str().unwrap_or_default().to_string(),
                region["artifactLocation"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                line,
                result
                    .get("suppressions")
                    .map(Value::to_string)
                    .unwrap_or_default(),
            )
        });
        match key {
            Some(key) => match index.get(&key) {
                Some(&at) => groups[at].push(result),
                None => {
                    index.insert(key, groups.len());
                    groups.push(vec![result]);
                }
            },
            None => groups.push(vec![result]),
        }
    }
    let fingerprint = |result: &Value| -> String {
        result["partialFingerprints"]["riprFingerprintV1"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let mut folded = 0usize;
    let mut kept = Vec::with_capacity(groups.len());
    for mut group in groups {
        group.sort_by_key(|result| fingerprint(result));
        let mut rest = group.split_off(1).into_iter();
        let Some(mut keeper) = group.pop() else {
            continue;
        };
        let notes: Vec<Value> = rest
            .by_ref()
            .map(|result| {
                json!({
                    "finding_id": result["properties"]["finding_id"],
                    "probe_id": result["properties"]["probe_id"],
                    "probe_family": result["properties"]["probe_family"],
                })
            })
            .collect();
        folded += notes.len();
        if !notes.is_empty()
            && let Some(Value::Object(properties)) = keeper.get_mut("properties")
        {
            properties.insert("folded_results".to_string(), Value::Array(notes));
        }
        kept.push(keeper);
    }
    (kept, folded)
}

/// Render repo-scoped classified seams as SARIF.
pub(crate) fn render_repo_seams_sarif(
    classified: &[ClassifiedSeam],
    limit_info: Option<&SeamLimitInfo>,
    config: &RiprConfig,
) -> String {
    let rules = seam_rules();
    // Ordinals are assigned over every classified seam, before the severity
    // filter, so a neighbour's class or an `off` setting never renumbers it.
    let ordinals = seam_ordinals(classified);
    let results = classified
        .iter()
        .zip(ordinals)
        .filter_map(|(entry, ordinal)| seam_result(entry, ordinal, config))
        .collect::<Vec<_>>();
    sarif_document_repo_seam(rules, results, limit_info)
}

fn sarif_document_repo_seam(
    rules: Vec<Value>,
    results: Vec<Value>,
    limit_info: Option<&SeamLimitInfo>,
) -> String {
    let mut props = Map::new();
    props.insert("tool".to_string(), json!("ripr"));
    props.insert(
        "schema_version".to_string(),
        json!(RIPR_SARIF_SCHEMA_VERSION),
    );
    props.insert("scope".to_string(), json!("repo_seam"));
    match limit_info {
        None => {
            props.insert("run_status".to_string(), json!("complete"));
        }
        Some(info) => {
            props.insert("run_status".to_string(), json!("seam_limit_applied"));
            let limitation = json!({
                "category": "repo_seam_limit_applied",
                "seams_analyzed": info.analyzed,
                "seams_total": info.total,
                "limit_source": info.source.as_str(),
                "control": "RIPR_REPO_EXPOSURE_SEAM_LIMIT"
            });
            props.insert("limitations".to_string(), json!([limitation]));
        }
    }
    let document = json!({
        "$schema": SARIF_SCHEMA,
        "version": SARIF_VERSION,
        "runs": [
            {
                "columnKind": SARIF_COLUMN_KIND,
                "tool": {
                    "driver": {
                        "name": "ripr",
                        "semanticVersion": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/EffortlessMetrics/ripr",
                        "rules": rules
                    }
                },
                "results": results,
                "properties": props
            }
        ]
    });
    json_pretty(document)
}

fn sarif_document(
    scope: &str,
    rules: Vec<Value>,
    results: Vec<Value>,
    analysis_outcome: Option<&AnalysisOutcome>,
    revisions: Option<&crate::analysis::AnalyzedRevisions>,
    omitted_not_current: usize,
    folded_same_line: usize,
) -> String {
    let mut properties = Map::new();
    properties.insert("tool".to_string(), json!("ripr"));
    properties.insert(
        "schema_version".to_string(),
        json!(RIPR_SARIF_SCHEMA_VERSION),
    );
    properties.insert("scope".to_string(), json!(scope));
    // RIPR-SPEC-0116 amendment: the run names the base and head it analyzed,
    // with the same field names the check JSON uses. Additive; absent for
    // diff-file, stdin and candidate-tree inputs.
    if let Some(revisions) = revisions {
        properties.insert("base".to_string(), json!(revisions.base_ref));
        if let Some(commit) = &revisions.base_commit {
            properties.insert("base_commit".to_string(), json!(commit));
        }
        if let Some(commit) = &revisions.merge_base_commit {
            properties.insert("merge_base_commit".to_string(), json!(commit));
        }
        let mut head = Map::new();
        head.insert(
            "source".to_string(),
            json!(crate::output::analyzed_revisions::head_source(revisions)),
        );
        if let Some(commit) = &revisions.head_commit {
            head.insert("commit".to_string(), json!(commit));
        }
        properties.insert("head".to_string(), Value::Object(head));
    }
    if let Some(outcome) = analysis_outcome {
        properties.insert(
            "run_status".to_string(),
            json!(if outcome.kind.is_complete() {
                "complete"
            } else {
                "incomplete"
            }),
        );
        properties.insert(
            "analysis_complete".to_string(),
            json!(outcome.kind.is_complete()),
        );
        properties.insert(
            "analysis_outcome".to_string(),
            serde_json::to_value(outcome).unwrap_or_else(
                |error| json!({"serialization_error": escape_message(&error.to_string())}),
            ),
        );
    }
    // Additive and absent when nothing was omitted, so ordinary clean runs
    // do not grow a zero-valued property. The omitted findings stay out of
    // `results`; this count only discloses the filter (#5091).
    if omitted_not_current > 0 {
        properties.insert(
            "omitted_not_current".to_string(),
            json!(omitted_not_current),
        );
    }
    if folded_same_line > 0 {
        properties.insert(
            "folded_same_line_results".to_string(),
            json!(folded_same_line),
        );
    }
    let document = json!({
        "$schema": SARIF_SCHEMA,
        "version": SARIF_VERSION,
        "runs": [
            {
                "columnKind": SARIF_COLUMN_KIND,
                "tool": {
                    "driver": {
                        "name": "ripr",
                        "semanticVersion": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/EffortlessMetrics/ripr",
                        "rules": rules
                    }
                },
                "results": results,
                "properties": properties
            }
        ]
    });
    json_pretty(document)
}

fn json_pretty(value: Value) -> String {
    match serde_json::to_string_pretty(&value) {
        Ok(mut rendered) => {
            rendered.push('\n');
            rendered
        }
        Err(err) => format!(
            "{{\"version\":\"{SARIF_VERSION}\",\"runs\":[],\"properties\":{{\"render_error\":\"{}\"}}}}\n",
            escape_message(err.to_string().as_str())
        ),
    }
}

/// The notification id every SARIF refusal document declares and references.
/// One declared id — the typed identity and repair route ride in the
/// notification's property bag and the run's properties, never in a
/// dynamically named rule.
const SARIF_REFUSAL_NOTIFICATION_ID: &str = "run_refused";

/// Render a SARIF 2.1.0 refusal document for a run that never analyzed
/// (#7332). Schema-valid and code-scanning-uploadable: one run with an
/// `invocations[].toolConfigurationNotifications` entry naming the typed
/// refusal identity and repair route, an empty `results` array, and the
/// ripr property bag echoing the invocation context. A SARIF consumer gets
/// a parseable document that says the run refused — never zero bytes that
/// could read as "the tool was never invoked".
pub(crate) fn render_refusal_sarif(
    notice: &crate::core_error::RefusalNotice,
    root: &Path,
    base: Option<&str>,
) -> String {
    let mut properties = Map::new();
    properties.insert("tool".to_string(), json!("ripr"));
    properties.insert(
        "schema_version".to_string(),
        json!(RIPR_SARIF_SCHEMA_VERSION),
    );
    properties.insert("scope".to_string(), json!("finding"));
    properties.insert("run_status".to_string(), json!("refused"));
    properties.insert("refusal_identity".to_string(), json!(notice.identity));
    properties.insert("repair_route".to_string(), json!(notice.repair_route));
    properties.insert(
        "root".to_string(),
        json!(crate::output::path::display_path(root)),
    );
    if let Some(base) = base {
        properties.insert("base".to_string(), json!(base));
    }
    let document = json!({
        "$schema": SARIF_SCHEMA,
        "version": SARIF_VERSION,
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "ripr",
                        "semanticVersion": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://github.com/EffortlessMetrics/ripr",
                        "notifications": [
                            {
                                "id": SARIF_REFUSAL_NOTIFICATION_ID,
                                "shortDescription": {
                                    "text": "The run refused to analyze; the invocation notification carries the typed identity and repair route."
                                }
                            }
                        ]
                    }
                },
                "invocations": [
                    {
                        "executionSuccessful": false,
                        "toolConfigurationNotifications": [
                            {
                                "level": "error",
                                "descriptor": { "id": SARIF_REFUSAL_NOTIFICATION_ID },
                                "message": { "text": notice.message },
                                "properties": {
                                    "identity": notice.identity,
                                    "repair_route": notice.repair_route
                                }
                            }
                        ]
                    }
                ],
                "results": [],
                "properties": properties
            }
        ]
    });
    json_pretty(document)
}

fn finding_result(
    root: &Path,
    finding: &Finding,
    config: &RiprConfig,
    suppressions: &[SuppressionEntry],
    today: &str,
) -> Option<Value> {
    let severity = config.severity().for_exposure(&finding.class);
    let level = sarif_level(severity)?;
    let rule_id = finding_rule_id(&finding.class);
    // The location and fingerprints must not move with `--root` spelling:
    // an absolute root would otherwise leak the checkout path into the uri
    // and change every fingerprint between a local and a CI run. The text
    // stays `normalize_path` so existing fingerprints (a literal `%` in a
    // file name, for instance) do not change for a relative root.
    let file = normalize_path(repository_relative_path(root, &finding.probe.location.file));
    let line = finding.probe.location.line;
    let mut result = Map::new();
    result.insert("ruleId".to_string(), json!(rule_id));
    result.insert("level".to_string(), json!(level));
    result.insert(
        "message".to_string(),
        json!({ "text": finding_message(finding) }),
    );
    result.insert(
        "locations".to_string(),
        json!([physical_location(
            &file,
            line,
            Some(finding.probe.location.column)
        )]),
    );
    result.insert(
        "partialFingerprints".to_string(),
        json!({ "riprFingerprintV1": finding_fingerprint(&rule_id, finding, &file, line) }),
    );
    // #2627: also emit top-level fingerprints for GitHub Code Scanning cross-run
    // dedup. partialFingerprints alone is insufficient — GitHub reads
    // fingerprints as the primary dedup key.
    result.insert(
        "fingerprints".to_string(),
        json!({ "riprFingerprintV1": finding_fingerprint(&rule_id, finding, &file, line) }),
    );
    result.insert(
        "properties".to_string(),
        finding_properties(finding, severity),
    );
    if let Some(suppression) = active_exposure_suppression(finding, suppressions, today) {
        result.insert(
            "suppressions".to_string(),
            json!([suppression_metadata(suppression)]),
        );
    }
    Some(Value::Object(result))
}

fn seam_result(entry: &ClassifiedSeam, ordinal: usize, config: &RiprConfig) -> Option<Value> {
    let severity = config.severity().for_seam(entry.class);
    let level = sarif_level(severity)?;
    let rule_id = seam_rule_id(entry.class);
    let file = normalize_path(entry.seam.file());
    let line = entry.seam.display_line();
    let mut result = Map::new();
    result.insert("ruleId".to_string(), json!(rule_id));
    result.insert("level".to_string(), json!(level));
    result.insert(
        "message".to_string(),
        json!({ "text": seam_message(entry) }),
    );
    result.insert(
        "locations".to_string(),
        json!([physical_location(&file, line, None)]),
    );
    result.insert(
        "partialFingerprints".to_string(),
        json!({ "riprFingerprintV1": seam_fingerprint(&rule_id, entry, &file, ordinal) }),
    );
    // #2627: also emit top-level fingerprints for GitHub Code Scanning cross-run dedup.
    result.insert(
        "fingerprints".to_string(),
        json!({ "riprFingerprintV1": seam_fingerprint(&rule_id, entry, &file, ordinal) }),
    );
    result.insert("properties".to_string(), seam_properties(entry, severity));
    if entry.class == SeamGripClass::Suppressed {
        result.insert(
            "suppressions".to_string(),
            json!([{
                "kind": "external",
                "justification": "seam is classified as suppressed by ripr configuration",
                "properties": {
                    "source": "ripr",
                    "grip_class": entry.class.as_str()
                }
            }]),
        );
    }
    Some(Value::Object(result))
}

fn sarif_level(severity: ConfigSeverity) -> Option<&'static str> {
    match severity {
        ConfigSeverity::Off => None,
        ConfigSeverity::Info | ConfigSeverity::Note => Some("note"),
        ConfigSeverity::Warning => Some("warning"),
    }
}

fn physical_location(file: &str, line: usize, column: Option<usize>) -> Value {
    let uri = sarif_uri_reference(file);
    let file = uri.as_str();
    // SARIF 2.1.0: `region` is optional. When the finding has no known line
    // (line == 0 — an unlocated probe or a producer bug), omit the region
    // entirely rather than fabricating startLine:1, which would point SARIF
    // viewers (GitHub code scanning, VS Code) at the wrong line (#2407).
    if line == 0 {
        return json!({
            "physicalLocation": {
                "artifactLocation": { "uri": file }
            }
        });
    }
    let mut region = Map::new();
    region.insert("startLine".to_string(), json!(line));
    if let Some(column) = column
        && column > 0
    {
        region.insert("startColumn".to_string(), json!(column));
    }
    json!({
        "physicalLocation": {
            "artifactLocation": { "uri": file },
            "region": region
        }
    })
}

fn finding_properties(finding: &Finding, severity: ConfigSeverity) -> Value {
    let mut properties = Map::new();
    properties.insert("tool".to_string(), json!("ripr"));
    properties.insert("kind".to_string(), json!("finding"));
    properties.insert("finding_id".to_string(), json!(finding.id.as_str()));
    if let Some(gap) = &finding.canonical_gap {
        properties.insert("canonical_gap_id".to_string(), json!(gap.id.as_str()));
        properties.insert(
            "canonical_gap".to_string(),
            json!({
                "id": gap.id.as_str(),
                "language": gap.language.as_str(),
                "file": display_path_text(&gap.file),
                "owner": gap.owner.as_str(),
                "behavior_kind": gap.behavior_kind.as_str(),
                "probe_kind": gap.probe_kind.as_str(),
                "normalized_discriminator": gap.normalized_discriminator.as_str()
            }),
        );
    }
    properties.insert("probe_id".to_string(), json!(finding.probe.id.0.as_str()));
    properties.insert("classification".to_string(), json!(finding.class.as_str()));
    properties.insert("severity".to_string(), json!(severity.as_str()));
    properties.insert(
        "probe_family".to_string(),
        json!(finding.probe.family.as_str()),
    );
    properties.insert(
        "probe_delta".to_string(),
        json!(finding.probe.delta.as_str()),
    );
    properties.insert(
        "confidence".to_string(),
        json!(crate::domain::contract_confidence(finding.confidence)),
    );
    if let Some(owner) = &finding.probe.owner {
        properties.insert("owner".to_string(), json!(owner.0.as_str()));
    }
    if let Some(language) = finding.language {
        properties.insert("language".to_string(), json!(language.as_str()));
    }
    if let Some(status) = finding.language_status {
        properties.insert("language_status".to_string(), json!(status.as_str()));
    }
    if let Some(kind) = finding.owner_kind {
        properties.insert("owner_kind".to_string(), json!(kind.as_str()));
    }
    if let Some(kind) = finding.static_limit_kind {
        properties.insert("static_limit_kind".to_string(), json!(kind.as_str()));
    }
    if let Some(actionability) = preview_actionability_for(finding) {
        properties.insert(
            "preview_actionability".to_string(),
            preview_actionability_json_value(&actionability),
        );
    }
    let python_card = python_repair_card(finding);
    if let Some(card) = &python_card {
        properties.insert(
            "python_repair_card".to_string(),
            python_repair_card_json_value(card),
        );
    } else if let Some(no_action) = python_no_action_properties(finding) {
        properties.insert("python_no_action".to_string(), no_action);
    }
    if let Some(card) = typescript_preview_card(finding) {
        properties.insert(
            "typescript_preview_card".to_string(),
            typescript_preview_card_json_value(&card),
        );
    }
    if let Some(card) = perl_preview_card_json(finding) {
        properties.insert("perl_preview_card".to_string(), card);
    }
    properties.insert(
        "changed_expression".to_string(),
        json!(finding.probe.expression.as_str()),
    );
    properties.insert("ripr".to_string(), finding_ripr_properties(finding));
    properties.insert(
        "stop_reasons".to_string(),
        json!(
            finding
                .effective_stop_reasons()
                .iter()
                .map(|reason| reason.as_str())
                .collect::<Vec<_>>()
        ),
    );
    properties.insert(
        "related_tests_total".to_string(),
        json!(finding.related_tests_total()),
    );
    properties.insert(
        "related_tests".to_string(),
        json!(
            finding
                .related_tests
                .iter()
                .take(5)
                .map(related_test_properties)
                .collect::<Vec<_>>()
        ),
    );
    properties.insert(
        "flow_sinks".to_string(),
        json!(
            finding
                .flow_sinks
                .iter()
                .map(|sink| {
                    json!({
                        "kind": sink.kind.as_str(),
                        "text": sink.text.as_str(),
                        "line": sink.line,
                        "owner": sink.owner.as_ref().map(|owner| owner.0.as_str())
                    })
                })
                .collect::<Vec<_>>()
        ),
    );
    properties.insert(
        "observed_values".to_string(),
        value_facts(&finding.activation.observed_values),
    );
    if let Some(total) = elided_observed_values_total(&finding.activation.observed_values) {
        properties.insert("observed_values_total".to_string(), json!(total));
    }
    properties.insert(
        "missing_discriminators".to_string(),
        missing_discriminators(&finding.activation.missing_discriminators),
    );
    // RIPR-SPEC-0088 §PR8: use the shared reconciliation fn so SARIF
    // suggested_next_action agrees with the human and JSON surfaces.
    properties.insert(
        "suggested_next_action".to_string(),
        json!(reconcile_next_step(finding)),
    );
    Value::Object(properties)
}

fn python_no_action_properties(finding: &Finding) -> Option<Value> {
    if finding.language != Some(LanguageId::Python)
        || finding.language_status != Some(LanguageStatus::Preview)
    {
        return None;
    }
    if let Some(no_action_kind) = python_ordinary_no_action_kind(finding) {
        return Some(python_ordinary_no_action_properties(
            finding,
            no_action_kind,
        ));
    }
    python_static_limit_no_action_properties(finding)
}

fn python_ordinary_no_action_kind(finding: &Finding) -> Option<&'static str> {
    match &finding.class {
        ExposureClass::Exposed => Some("already_observed"),
        ExposureClass::NoStaticPath => Some("no_related_test"),
        ExposureClass::WeaklyExposed if python_finding_is_heuristic_only(finding) => {
            Some("heuristic_only")
        }
        _ => None,
    }
}

fn python_finding_is_heuristic_only(finding: &Finding) -> bool {
    finding
        .evidence
        .iter()
        .any(|item| item.starts_with("related_test_uncertain:"))
        || finding
            .ripr
            .reach
            .summary
            .contains("heuristic Python test link")
}

fn python_ordinary_no_action_properties(finding: &Finding, no_action_kind: &str) -> Value {
    let changed_owner = finding.probe.owner.as_ref().map(|owner| owner.0.as_str());
    let stop_conditions = python_ordinary_no_action_stop_conditions(finding, no_action_kind);
    json!({
        "source": "check_python_preview",
        "language": "python",
        "language_status": "preview",
        "authority_boundary": PYTHON_PREVIEW_AUTHORITY_BOUNDARY,
        "repairability": "no_action",
        "repair_packet_ready": false,
        "repair_card_present": false,
        "gap_state": no_action_kind,
        "no_action_kind": no_action_kind,
        "changed_owner": changed_owner,
        "why_not_actionable": python_ordinary_no_action_reason(no_action_kind),
        "verify": {
            "command": Value::Null,
            "status": "not_applicable_no_action"
        },
        "receipt": {
            "command": Value::Null,
            "status": "not_applicable_no_action"
        },
        "stop_conditions": stop_conditions,
        "limits": [
            "Syntax-first Python preview evidence only.",
            "No repair card or agent packet emitted for no-action Python states.",
            "No source edits, generated tests, mutation execution, provider calls, or gate authority."
        ]
    })
}

fn python_ordinary_no_action_reason(no_action_kind: &str) -> &'static str {
    match no_action_kind {
        "already_observed" => {
            "Current Python test evidence already observes the changed behavior; no missing proof was routed."
        }
        "no_related_test" => {
            "No related Python test was statically linked, so RIPR cannot choose a safe edit target."
        }
        "heuristic_only" => {
            "Only heuristic Python related-test proximity was found, so bounded repair routing would overclaim."
        }
        _ => "Python preview did not find a bounded repair route.",
    }
}

fn python_ordinary_no_action_stop_conditions(
    finding: &Finding,
    no_action_kind: &str,
) -> Vec<&'static str> {
    let mut stop_conditions = finding
        .effective_stop_reasons()
        .iter()
        .map(|reason| reason.as_str())
        .collect::<Vec<_>>();
    if stop_conditions.is_empty() {
        stop_conditions.push(match no_action_kind {
            "already_observed" => "missing_proof_already_observed",
            "no_related_test" => "related_python_test_not_found",
            "heuristic_only" => "related_test_link_uncertain",
            _ => "no_repair_packet_emitted",
        });
    }
    stop_conditions
}

fn python_static_limit_no_action_properties(finding: &Finding) -> Option<Value> {
    let static_limit_kind = finding.static_limit_kind?;
    let static_limit_kind = static_limit_kind.as_str();
    let stop_reasons = finding
        .effective_stop_reasons()
        .iter()
        .map(|reason| reason.as_str())
        .collect::<Vec<_>>();
    let changed_owner = finding.probe.owner.as_ref().map(|owner| owner.0.as_str());
    let why_not_actionable = python_static_limit_detail(finding, static_limit_kind);

    Some(json!({
        "source": "check_python_preview",
        "language": "python",
        "language_status": "preview",
        "authority_boundary": PYTHON_PREVIEW_AUTHORITY_BOUNDARY,
        "repairability": "analyzer_limitation",
        "repair_packet_ready": false,
        "repair_card_present": false,
        "gap_state": "static_limitation",
        "no_action_kind": "static_limit",
        "static_limit_kind": static_limit_kind,
        "changed_owner": changed_owner,
        "why_not_actionable": why_not_actionable,
        "verify": {
            "command": Value::Null,
            "status": "not_applicable_static_limit"
        },
        "receipt": {
            "command": Value::Null,
            "status": "not_applicable_static_limit"
        },
        "stop_conditions": stop_reasons,
        "limits": [
            "Syntax-first Python preview evidence only.",
            "No repair card or agent packet emitted for static limits.",
            "No source edits, generated tests, mutation execution, provider calls, or gate authority."
        ]
    }))
}

fn python_static_limit_detail(finding: &Finding, static_limit_kind: &str) -> String {
    finding
        .missing
        .iter()
        .find_map(|detail| non_empty(detail).map(ToString::to_string))
        .or_else(|| {
            finding
                .evidence
                .iter()
                .find(|detail| {
                    detail.contains("static_limit") || detail.contains(static_limit_kind)
                })
                .cloned()
        })
        .unwrap_or_else(|| {
            format!(
                "Python preview reported static limit `{static_limit_kind}` without a bounded repair route."
            )
        })
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn seam_properties(entry: &ClassifiedSeam, severity: ConfigSeverity) -> Value {
    let seam = &entry.seam;
    let evidence = &entry.evidence;
    let mut properties = json!({
        "tool": "ripr",
        "kind": "seam",
        "seam_id": seam.id().as_str(),
        "grip_class": entry.class.as_str(),
        "severity": severity.as_str(),
        "seam_kind": seam.kind().as_str(),
        "owner": seam.owner(),
        "expression": seam.expression(),
        "expected_sink": seam.expected_sink().as_str(),
        "required_discriminator": seam.required_discriminator().as_str(),
        "headline_eligible": entry.class.is_headline_eligible(),
        "evidence": {
            "reach": stage_properties(&evidence.reach),
            "activate": stage_properties(&evidence.activate),
            "propagate": stage_properties(&evidence.propagate),
            "observe": stage_properties(&evidence.observe),
            "discriminate": stage_properties(&evidence.discriminate)
        },
        "related_tests_total": evidence.related_tests.len(),
        "related_tests": evidence
            .related_tests
            .iter()
            .take(8)
            .map(|test| json!({
                "name": test.test_name.as_str(),
                "file": normalize_path(&test.file),
                "line": test.line,
                "oracle_kind": test.oracle_kind.as_str(),
                "oracle_strength": test.oracle_strength.as_str(),
                "evidence_summary": test.evidence_summary.as_str(),
                "relation_reason": test.relation_reason.as_str(),
                "relation_confidence": test.relation_confidence.as_str()
            }))
            .collect::<Vec<_>>(),
        "observed_values": value_facts(&evidence.observed_values),
        "missing_discriminators": missing_discriminators(&evidence.missing_discriminators)
    });
    if let (Some(total), Some(object)) = (
        elided_observed_values_total(&evidence.observed_values),
        properties.as_object_mut(),
    ) {
        object.insert("observed_values_total".to_string(), json!(total));
    }
    properties
}

fn finding_ripr_properties(finding: &Finding) -> Value {
    json!({
        "reach": stage_properties(&finding.ripr.reach),
        "infect": stage_properties(&finding.ripr.infect),
        "propagate": stage_properties(&finding.ripr.propagate),
        "observe": stage_properties(&finding.ripr.reveal.observe),
        "discriminate": stage_properties(&finding.ripr.reveal.discriminate)
    })
}

fn stage_properties(stage: &StageEvidence) -> Value {
    json!({
        "state": stage.state.as_str(),
        "confidence": stage.confidence.as_str(),
        "summary": stage.summary.as_str()
    })
}

fn related_test_properties(test: &RelatedTest) -> Value {
    json!({
        "name": test.name.as_str(),
        "file": normalize_path(&test.file),
        "line": test.line,
        "oracle_kind": test.oracle_kind.as_str(),
        "oracle_strength": test.oracle_strength.as_str(),
        "oracle": test.oracle.as_deref()
    })
}

/// The bounded observed-value projection (`output::observed_values`); the
/// call site discloses `observed_values_total` when the cap drops values.
fn value_facts<T: crate::output::observed_values::SourceValue + serde::Serialize>(
    values: &[T],
) -> Value {
    json!(bounded_observed_values(values))
}

fn missing_discriminators(missing: &[MissingDiscriminatorFact]) -> Value {
    json!(
        missing
            .iter()
            .map(|missing| {
                json!({
                    "value": missing.value.as_str(),
                    "reason": missing.reason.as_str(),
                    "flow_sink": missing.flow_sink.as_ref().map(|sink| {
                        json!({
                            "kind": sink.kind.as_str(),
                            "text": sink.text.as_str(),
                            "line": sink.line,
                            "owner": sink.owner.as_ref().map(|owner| owner.0.as_str())
                        })
                    })
                })
            })
            .collect::<Vec<_>>()
    )
}

fn active_exposure_suppression<'a>(
    finding: &Finding,
    suppressions: &'a [SuppressionEntry],
    today: &str,
) -> Option<&'a SuppressionEntry> {
    suppressions.iter().find(|entry| {
        entry.kind == SuppressionKind::ExposureGap
            && entry.finding_id.as_deref() == Some(finding.id.as_str())
            && !is_expired(entry.expires.as_deref(), today)
    })
}

fn suppression_metadata(entry: &SuppressionEntry) -> Value {
    json!({
        "kind": "external",
        "justification": entry.reason.as_str(),
        "properties": {
            "source": "ripr",
            "suppression_kind": entry.kind.as_str(),
            "owner": entry.owner.as_str(),
            "expires": entry.expires.as_deref(),
            "block_line": entry.block_line
        }
    })
}

fn finding_message(finding: &Finding) -> String {
    let mut message = String::new();
    // #11219: lead with the missing check when the analyzer named one, so the
    // length bound can never cut it; the full list stays in
    // `properties.missing_discriminators`.
    if let Some(missing) = finding.activation.missing_discriminators.first() {
        let reason = missing.reason.trim();
        if !reason.is_empty() {
            message.push_str("Missing check: ");
            message.push_str(reason.trim_end_matches('.'));
            message.push_str(". ");
        }
    }
    message.push_str(&format!(
        "{} static exposure for {} probe",
        finding.class.as_str(),
        finding.probe.family.as_str()
    ));
    if !finding.probe.expression.is_empty() {
        message.push_str(": ");
        message.push_str(&finding.probe.expression);
    }
    // Bound the evidence head only; the reconciled next step below is
    // appended whole so it keeps agreeing with the other surfaces.
    let mut message = bound_message(message);
    // RIPR-SPEC-0088 §PR8: the SARIF message text is a machine-consumed surface.
    // Use the shared reconciliation fn (not the raw field) so the embedded
    // "Next step" agrees with suggested_next_action and the other surfaces.
    if finding.recommended_next_step.is_some() {
        message.push_str(". Next step: ");
        message.push_str(&reconcile_next_step(finding));
    }
    message
}

/// Longest evidence head (missing check, class, probe, expression) a finding
/// message carries (#11219). The reconciled next step is appended whole
/// (RIPR-SPEC-0088); the complete evidence stays in `properties`.
const MAX_MESSAGE_CHARS: usize = 240;

fn bound_message(message: String) -> String {
    if message.chars().count() <= MAX_MESSAGE_CHARS {
        return message;
    }
    let mut bounded: String = message.chars().take(MAX_MESSAGE_CHARS - 3).collect();
    bounded.push_str("...");
    bounded
}

fn seam_message(entry: &ClassifiedSeam) -> String {
    format!(
        "{} seam grip for {}: {}",
        entry.class.as_str(),
        entry.seam.kind().as_str(),
        entry.seam.expression()
    )
}

fn finding_fingerprint(rule_id: &str, finding: &Finding, file: &str, line: usize) -> String {
    format!(
        "{rule_id}|{}|{}|{file}|{line}",
        finding.id, finding.probe.id.0
    )
}

/// Line- and offset-independent seam alert identity (#11219).
///
/// `rule_id | normalized_file | identity_hash | ordinal`. The hash covers the
/// owner, seam kind and whitespace-collapsed expression; `ordinal` ranks
/// textual duplicates of one `(file, owner, kind, expression)` group (see
/// [`seam_ordinals`]). Neither the display line nor the byte offset is read,
/// so an unrelated edit above a seam keeps its code-scanning alert. The
/// analysis `SeamId` is offset-derived and is deliberately not used here.
fn seam_fingerprint(rule_id: &str, entry: &ClassifiedSeam, file: &str, ordinal: usize) -> String {
    format!(
        "{rule_id}|{file}|{}|{ordinal}",
        seam_identity_hash(entry, file)
    )
}

/// Canonical identity text with length-prefixed fields so no field can bleed
/// into its neighbour (`a|b` + `c` never equals `a` + `b|c`).
fn seam_identity_text(entry: &ClassifiedSeam, file: &str) -> String {
    let expression = collapse_whitespace_outside_strings(entry.seam.expression());
    let kind = entry.seam.kind().as_str();
    let owner = entry.seam.owner();
    format!(
        "{}:{file}{}:{owner}{}:{kind}{}:{expression}",
        file.len(),
        owner.len(),
        kind.len(),
        expression.len()
    )
}

/// Collapses runs of whitespace between tokens to one space and trims the ends,
/// but keeps the contents of string literals byte for byte: `"a  b"` and
/// `"a b"` are different expressions, so a baseline must not conflate them.
/// Handles `"..."` with backslash escapes and raw strings (`r"..."`,
/// `r#"..."#`); an unterminated literal keeps the rest of the text verbatim
/// (fail closed toward distinct fingerprints).
fn collapse_whitespace_outside_strings(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut pending_space = false;
    while index < chars.len() {
        let current = chars[index];
        if current.is_whitespace() {
            pending_space = true;
            index += 1;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        let end = string_literal_end(&chars, index).unwrap_or(index + 1);
        out.extend(&chars[index..end]);
        index = end;
    }
    out
}

/// End (exclusive) of the string literal starting at `start`, if one starts
/// there; an unterminated literal runs to the end of the text.
fn string_literal_end(chars: &[char], start: usize) -> Option<usize> {
    let mut cursor = start;
    let mut hashes = 0;
    let raw = chars[cursor] == 'r' || (chars[cursor] == 'b' && chars.get(cursor + 1) == Some(&'r'));
    if raw {
        cursor += if chars[cursor] == 'b' { 2 } else { 1 };
        while chars.get(cursor) == Some(&'#') {
            hashes += 1;
            cursor += 1;
        }
        if chars.get(cursor) != Some(&'"') {
            return None;
        }
        cursor += 1;
        while cursor < chars.len() {
            let closing = chars.get(cursor + 1..cursor + 1 + hashes);
            if chars[cursor] == '"' && closing.is_some_and(|run| run.iter().all(|c| *c == '#')) {
                return Some(cursor + 1 + hashes);
            }
            cursor += 1;
        }
        return Some(chars.len());
    }
    if chars[cursor] == 'b' && chars.get(cursor + 1) == Some(&'"') {
        cursor += 1;
    }
    if chars[cursor] != '"' {
        return None;
    }
    cursor += 1;
    while cursor < chars.len() {
        match chars[cursor] {
            '\\' => cursor += 2,
            '"' => return Some(cursor + 1),
            _ => cursor += 1,
        }
    }
    Some(chars.len())
}

/// 128-bit identity: two FNV-1a passes with distinct offset bases, rendered as
/// 32 lowercase hex characters. Stable across Rust versions and platforms.
fn seam_identity_hash(entry: &ClassifiedSeam, file: &str) -> String {
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    const BASES: [u64; 2] = [0xcbf2_9ce4_8422_2325, 0x8422_2325_cbf2_9ce4];
    let text = seam_identity_text(entry, file);
    let mut out = String::with_capacity(32);
    for base in BASES {
        let mut hash = base;
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        out.push_str(&format!("{hash:016x}"));
    }
    out
}

/// Zero-based rank of each seam among those sharing its identity text, in
/// source order (byte offset, then input order). Only the relative order is
/// read, never an offset value, so distinct textual duplicates stay distinct
/// without making the fingerprint depend on where they sit.
fn seam_ordinals(classified: &[ClassifiedSeam]) -> Vec<usize> {
    let mut groups: std::collections::BTreeMap<String, Vec<(usize, usize)>> =
        std::collections::BTreeMap::new();
    for (index, entry) in classified.iter().enumerate() {
        let file = normalize_path(entry.seam.file());
        groups
            .entry(seam_identity_text(entry, &file))
            .or_default()
            .push((entry.seam.byte_offset(), index));
    }
    let mut ordinals = vec![0; classified.len()];
    for members in groups.values_mut() {
        members.sort();
        for (rank, (_, index)) in members.iter().enumerate() {
            ordinals[*index] = rank;
        }
    }
    ordinals
}

fn finding_rule_id(class: &ExposureClass) -> String {
    format!("ripr.finding.{}", class.as_str())
}

fn seam_rule_id(class: SeamGripClass) -> String {
    format!("ripr.seam.{}", class.as_str())
}

fn finding_rules() -> Vec<Value> {
    all_exposure_classes()
        .iter()
        .map(|class| {
            let (full_description, help) = finding_class_help(class);
            rule(
                finding_rule_id(class),
                format!("ripr {}", class.as_str()),
                finding_class_title(class).to_string(),
                full_description,
                help,
                STATIC_EXPOSURE_MODEL_URI,
            )
        })
        .collect()
}

fn seam_rules() -> Vec<Value> {
    SeamGripClass::ALL
        .iter()
        .map(|class| {
            let (full_description, help) = seam_class_help(class);
            rule(
                seam_rule_id(*class),
                format!("ripr seam {}", class.as_str()),
                seam_class_title(class).to_string(),
                full_description,
                help,
                SEAM_GRIP_SPEC_URI,
            )
        })
        .collect()
}

/// Author-facing alert titles (#11219): what the Security tab lists. They say
/// what the static evidence suggests and avoid runtime verdicts.
fn finding_class_title(class: &ExposureClass) -> &'static str {
    match class {
        ExposureClass::Exposed => "Changed behavior appears covered by a strong test oracle",
        ExposureClass::WeaklyExposed => "Changed behavior may not be caught by the current tests",
        ExposureClass::ReachableUnrevealed => {
            "Tests reach the changed code but no assertion appears to check it"
        }
        ExposureClass::NoStaticPath => "No test appears to reach the changed code",
        ExposureClass::InfectionUnknown => {
            "Tests may reach the change but their inputs are not visible to static analysis"
        }
        ExposureClass::PropagationUnknown => {
            "Static analysis cannot tell whether a wrong result would reach an assertion"
        }
        ExposureClass::StaticUnknown => "Static analysis could not judge this change",
    }
}

fn seam_class_title(class: &SeamGripClass) -> &'static str {
    match class {
        SeamGripClass::StronglyGripped => "Tests appear to grip this code strongly",
        SeamGripClass::WeaklyGripped => "Tests only weakly grip this code",
        SeamGripClass::Ungripped => "No test appears to grip this code",
        SeamGripClass::ReachableUnrevealed => {
            "Tests reach this code but no assertion appears to check it"
        }
        SeamGripClass::ActivationUnknown => {
            "Static analysis cannot tell whether tests activate this code"
        }
        SeamGripClass::PropagationUnknown => {
            "Static analysis cannot tell whether a wrong result here would be observed"
        }
        SeamGripClass::ObservationUnknown => {
            "Static analysis cannot tell where a wrong result here would be observed"
        }
        SeamGripClass::DiscriminationUnknown => {
            "An assertion observes this code but may not discriminate a wrong result"
        }
        SeamGripClass::Opaque => {
            "This code sits behind a boundary static analysis cannot see through"
        }
        SeamGripClass::Intentional => "Project records no discriminating test here by choice",
        SeamGripClass::Suppressed => "Exposure gap waived by project policy",
    }
}

/// Per-class rule metadata for someone who has never seen ripr (#7371): one
/// or two sentences defining the class, stating the static/advisory boundary,
/// and naming the typical next step. The vocabulary mirrors
/// `docs/STATIC_EXPOSURE_MODEL.md` and the seam inventory spec; these strings
/// are the rule-page text code scanning shows on its alert and rule pages.
fn finding_class_help(class: &ExposureClass) -> (&'static str, &'static str) {
    match class {
        ExposureClass::Exposed => (
            "Static analysis found a complete path from the changed code to a test oracle strong enough to observe the changed behavior.",
            "This is static evidence, not a runtime verdict: ripr did not execute any test. Next step: run the linked related test and confirm it observes the changed sink.",
        ),
        ExposureClass::WeaklyExposed => (
            "A static path from the change to a related test exists, but the infection or the oracle's discrimination looks weak, so the test could pass even when the behavior is wrong.",
            "Static evidence only; no test was executed. Next step: strengthen the nearest assertion so it observes the changed sink directly, then re-run ripr check.",
        ),
        ExposureClass::ReachableUnrevealed => (
            "Related tests appear to reach the changed code, but no meaningful oracle was found that would notice a wrong result.",
            "Static evidence only; no test was executed. Next step: add or sharpen an assertion that discriminates the changed behavior.",
        ),
        ExposureClass::NoStaticPath => (
            "No static test path to the changed owner was found; static analysis could not trace any related test to this behavior.",
            "This is a static reachability statement, not a runtime coverage measurement. Next step: add a focused test that exercises the changed behavior.",
        ),
        ExposureClass::InfectionUnknown => (
            "Tests may reach the change, but the input or fixture evidence is opaque, so static analysis cannot tell whether a wrong result would propagate.",
            "Static evidence only; no test was executed. Next step: make the decisive input or fixture value explicit, then re-run ripr check.",
        ),
        ExposureClass::PropagationUnknown => (
            "The changed behavior crosses a boundary static analysis cannot see through, so it is unknown whether a wrong result would reach an observable sink.",
            "Static evidence only; no test was executed. Next step: add an assertion at an observable sink beyond the boundary.",
        ),
        ExposureClass::StaticUnknown => (
            "Syntax-first analysis could not make a credible judgment for this finding, usually because parsing or symbol resolution was incomplete.",
            "This is an analysis limitation disclosure, not a coverage verdict. Next step: re-run with fuller analysis settings or triage the finding manually.",
        ),
    }
}

fn seam_class_help(class: &SeamGripClass) -> (&'static str, &'static str) {
    match class {
        SeamGripClass::StronglyGripped => (
            "Current tests strongly grip this seam: a related test reaches it with a discriminating oracle.",
            "Static grip evidence only; ripr did not execute any test. Confirm by running the related test named on the result.",
        ),
        SeamGripClass::WeaklyGripped => (
            "Tests reach this seam, but the grip is weak: infection or discrimination is thin, so the seam could move without any test failing.",
            "Static evidence only; no test was executed. Next step: strengthen the nearest assertion so it observes the seam's sink.",
        ),
        SeamGripClass::Ungripped => (
            "No related test grips this seam; static analysis found no path from a test to this seam.",
            "Static reachability evidence, not a runtime coverage measurement. Next step: add a focused test for this seam.",
        ),
        SeamGripClass::ReachableUnrevealed => (
            "Related tests appear to reach this seam, but no oracle was found that would notice a wrong outcome.",
            "Static evidence only; no test was executed. Next step: add a discriminating assertion at the seam's sink.",
        ),
        SeamGripClass::ActivationUnknown => (
            "Whether the changed path can be activated at all is unknown from the static evidence.",
            "This is an analysis limitation, not a verdict. Next step: exercise the path in a test to establish activation.",
        ),
        SeamGripClass::PropagationUnknown => (
            "The seam's effect crosses an opaque propagation boundary, so static analysis cannot tell whether a wrong outcome would reach an observable sink.",
            "Static evidence only. Next step: assert at a sink beyond the boundary.",
        ),
        SeamGripClass::ObservationUnknown => (
            "The sink a wrong outcome would manifest at is unknown, so no oracle can be checked against it.",
            "This is an analysis limitation. Next step: identify where the effect is observable and assert there.",
        ),
        SeamGripClass::DiscriminationUnknown => (
            "An oracle observes the seam, but whether its assertion discriminates the changed behavior is unknown.",
            "Static evidence only. Next step: run the test with a deliberate behavior change to see whether it notices.",
        ),
        SeamGripClass::Opaque => (
            "The seam sits behind an opaque helper or boundary that static analysis cannot see through.",
            "Static limitation disclosure, not a coverage verdict. Next step: make the boundary transparent to tests or assert across it.",
        ),
        SeamGripClass::Intentional => (
            "This seam is intentionally left without a discriminating test, recorded as such by the project.",
            "No action is required unless the intention changes.",
        ),
        SeamGripClass::Suppressed => (
            "This seam's exposure gap is suppressed by project policy for a recorded reason and expiry.",
            "No action is required while the suppression is active; check the suppression entry before relying on this seam.",
        ),
    }
}

fn rule(
    id: String,
    name: String,
    short_description: String,
    full_description: &'static str,
    help: &'static str,
    help_uri: &'static str,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "shortDescription": {
            "text": short_description
        },
        "fullDescription": {
            "text": full_description
        },
        "help": {
            "text": help
        },
        "helpUri": help_uri
    })
}

fn all_exposure_classes() -> [ExposureClass; 7] {
    [
        ExposureClass::Exposed,
        ExposureClass::WeaklyExposed,
        ExposureClass::ReachableUnrevealed,
        ExposureClass::NoStaticPath,
        ExposureClass::InfectionUnknown,
        ExposureClass::PropagationUnknown,
        ExposureClass::StaticUnknown,
    ]
}

/// Percent-encode a repository path as an RFC 3986 relative reference for
/// `artifactLocation.uri` (SARIF 2.1.0 §3.4.3). A raw `#` or `?` would be
/// read as a fragment or query and point consumers at a different file; a
/// space or a stray `%` makes the uri invalid. `:` is encoded too, since a
/// colon in the first segment would parse as a scheme. Fingerprints keep
/// the unencoded text so existing alert identity does not move.
fn sarif_uri_reference(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(char::from(byte));
            }
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(char::from(HEX[usize::from(byte >> 4)]));
                encoded.push(char::from(HEX[usize::from(byte & 0x0F)]));
            }
        }
    }
    encoded
}

fn normalize_path(path: &Path) -> String {
    let raw = path_display_lossy(path);
    raw.strip_prefix("./").unwrap_or(&raw).to_string()
}

fn escape_message(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
    use crate::analysis::test_grip_evidence::{
        RelatedTestGrip, RelationConfidence, RelationReason, TestGripEvidence,
    };
    use crate::analysis_outcome::{
        AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcome, AnalysisOutcomeCounts,
        AnalysisOutcomeKind, AnalysisRecovery, AnalysisRecoveryKind, AnalysisStage,
    };
    use crate::app::{CheckOutput, Mode};
    use crate::domain::ValueFact;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, FindingCanonicalGap, FlowSinkFact, FlowSinkKind,
        LanguageId, LanguageStatus, OracleKind, OracleStrength, OwnerKind, Probe, ProbeFamily,
        ProbeId, RelatedTest, RevealEvidence, RiprEvidence, SourceLocation, StageEvidence,
        StageState, StaticLimitKind, StopReason, Summary, SymbolId, ValueContext,
    };
    use serde_json::Value;
    use std::path::PathBuf;

    fn assert_sarif_scalar_column_contract(source: &str) -> Result<(), String> {
        let offset = source.rfind('x').ok_or("fixture x missing")?;
        let span = crate::analysis::seams::byte_span_to_lines(source, 1, offset, offset + 1)
            .ok_or("producer did not derive a source span")?;
        assert_eq!(span.start_column, 14, "literal scalar-column fixture pin");
        assert_eq!(span.end_column, 15);
        let mut output = sample_output();
        output.findings.truncate(1);
        let finding = output
            .findings
            .first_mut()
            .ok_or("fixture finding missing")?;
        finding.probe.location = SourceLocation::new("src/日本語.rs", 1, span.start_column);
        let json = parse_json(&crate::output::json::render(&output))?;
        assert_eq!(json["findings"][0]["probe"]["file"], "src/日本語.rs");
        assert_eq!(json["findings"][0]["probe"]["line"], 1);
        let human = crate::output::human::render(&output);
        assert!(
            human.contains("src/日本語.rs:1"),
            "human filename and line control"
        );
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        let run = &sarif["runs"][0];
        assert_eq!(run["results"].as_array().map(Vec::len), Some(1));
        let location = &run["results"][0]["locations"][0]["physicalLocation"];
        assert_eq!(
            location["artifactLocation"]["uri"],
            "src/%E6%97%A5%E6%9C%AC%E8%AA%9E.rs"
        );
        assert_eq!(location["region"]["startLine"], 1);
        assert_eq!(location["region"]["startColumn"], 14);
        // SARIF 2.1.0 section 3.14.27 requires a unit declaration for
        // nonempty text results; neither bytes nor UTF-16 units match this producer.
        assert_eq!(run["columnKind"], "unicodeCodePoints");
        Ok(())
    }

    #[test]
    fn sarif_column_contract_ascii() -> Result<(), String> {
        assert_sarif_scalar_column_contract("fn f(){ 'a'; x }\n")
    }

    #[test]
    fn sarif_column_contract_bmp() -> Result<(), String> {
        assert_sarif_scalar_column_contract("fn f(){ 'é'; x }\n")
    }

    #[test]
    fn sarif_column_contract_non_bmp() -> Result<(), String> {
        let source = "fn f(){ '🦀'; x }\n";
        let prefix = source.split('x').next().ok_or("prefix missing")?;
        assert_eq!(prefix.encode_utf16().count() + 1, 15);
        assert_eq!(prefix.len() + 1, 17);
        assert_sarif_scalar_column_contract(source)
    }

    #[test]
    fn sarif_column_contract_repo_seam() -> Result<(), String> {
        let sarif = parse_json(&render_repo_seams_sarif(
            &[weakly_gripped_classified()],
            None,
            &RiprConfig::default(),
        ))?;
        let run = &sarif["runs"][0];
        assert_eq!(run["results"].as_array().map(Vec::len), Some(1));
        assert_eq!(run["columnKind"], "unicodeCodePoints");
        Ok(())
    }

    #[test]
    fn sarif_column_contract_empty_control() -> Result<(), String> {
        let mut output = sample_output();
        output.findings.clear();
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        assert_eq!(
            sarif["runs"][0]["results"].as_array().map(Vec::len),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn refusal_sarif_is_a_schema_valid_run_with_the_typed_notification() -> Result<(), String> {
        // #7332: a refused run must still upload to code scanning — one run
        // whose only signal is the tool-configuration notification naming the
        // typed identity and repair route, and an empty results array.
        let notice = crate::core_error::CoreError::base_unresolvable(
            "the base `deadbeef` does not resolve to a commit",
        )
        .refusal_notice();
        let rendered = render_refusal_sarif(&notice, std::path::Path::new("."), Some("deadbeef"));
        let sarif = parse_json(&rendered)?;
        if sarif["version"] != Value::String("2.1.0".to_string()) {
            return Err(format!("version must be 2.1.0: {sarif}"));
        }
        let run = &sarif["runs"][0];
        if run["tool"]["driver"]["notifications"][0]["id"] != Value::String("run_refused".into()) {
            return Err(format!("driver must declare run_refused: {sarif}"));
        }
        let notification = &run["invocations"][0]["toolConfigurationNotifications"][0];
        for (pointer, expected) in [
            ("/level", "error"),
            ("/descriptor/id", "run_refused"),
            ("/properties/identity", "base_unresolvable"),
            ("/properties/repair_route", "analysis/base-resolution"),
        ] {
            let actual = notification.pointer(pointer).and_then(Value::as_str);
            if actual != Some(expected) {
                return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
            }
        }
        if notification["message"]["text"] != Value::String(notice.message.clone()) {
            return Err(format!("notification must echo the diagnostic: {sarif}"));
        }
        if run["results"].as_array().map(Vec::len) != Some(0) {
            return Err(format!("results must be empty: {sarif}"));
        }
        for (pointer, expected) in [
            ("/properties/run_status", "refused"),
            ("/properties/refusal_identity", "base_unresolvable"),
            ("/properties/repair_route", "analysis/base-resolution"),
            ("/properties/base", "deadbeef"),
        ] {
            let actual = run.pointer(pointer).and_then(Value::as_str);
            if actual != Some(expected) {
                return Err(format!("{pointer}: expected {expected}, got {actual:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn sarif_preserves_matched_total_for_sparse_retained_rows() -> Result<(), String> {
        let mut output = sample_output();
        let finding = output
            .findings
            .first_mut()
            .ok_or("missing sample finding")?;
        let retained = finding.related_tests.clone();
        assert!(!retained.is_empty());
        finding.related_tests_matched_total = Some(9);
        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;
        assert_eq!(result["properties"]["related_tests_total"], 9);
        assert_eq!(
            result["properties"]["related_tests"]
                .as_array()
                .ok_or("missing rows")?
                .len(),
            retained.len().min(5)
        );
        assert_eq!(
            output
                .findings
                .first()
                .ok_or("missing finding")?
                .related_tests,
            retained
        );
        Ok(())
    }

    #[test]
    fn sarif_renders_findings_with_stable_rule_ids() -> Result<(), String> {
        let rendered = render_findings_sarif(&sample_output(), &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let rule_ids = rule_ids(&sarif)?;
        let result = first_result(&sarif)?;

        assert_eq!(sarif["version"], "2.1.0");
        assert!(rule_ids.contains(&"ripr.finding.weakly_exposed".to_string()));
        assert_eq!(result["ruleId"], "ripr.finding.weakly_exposed");
        assert_eq!(result["level"], "warning");
        assert_eq!(result["properties"]["kind"], "finding");
        assert_eq!(result["properties"]["finding_id"], "finding:discount");
        assert_eq!(
            result["partialFingerprints"]["riprFingerprintV1"],
            "ripr.finding.weakly_exposed|finding:discount|probe:src/pricing.rs:88:predicate|src/pricing.rs|88"
        );
        Ok(())
    }

    #[test]
    fn sarif_location_and_fingerprints_ignore_root_spelling() -> Result<(), String> {
        // Determinism: a finding rendered under `--root .` and under the same
        // checkout's absolute path must carry identical uri and fingerprints.
        let absolute_root = std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join("repo");
        let mut rendered = Vec::new();
        for (root, file) in [
            (PathBuf::from("."), PathBuf::from("./src/pricing.rs")),
            (absolute_root.clone(), absolute_root.join("src/pricing.rs")),
        ] {
            let mut output = sample_output();
            output.root = root;
            output.findings[0].probe.location.file = file;
            let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
            let result = first_result(&sarif)?.clone();
            rendered.push((
                result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"].clone(),
                result["partialFingerprints"].clone(),
                result["fingerprints"].clone(),
            ));
        }
        assert_eq!(rendered[0], rendered[1]);
        assert_eq!(rendered[0].0, "src/pricing.rs");

        // A literal `%` keeps the fingerprint text it had before the root
        // fix, so baselines keyed on it still match.
        let mut output = sample_output();
        output.root = PathBuf::from(".");
        output.findings[0].probe.location.file = PathBuf::from("./src/rate%limit.rs");
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        assert_eq!(
            first_result(&sarif)?["partialFingerprints"]["riprFingerprintV1"],
            "ripr.finding.weakly_exposed|finding:discount|probe:src/pricing.rs:88:predicate|src/rate%limit.rs|88"
        );
        Ok(())
    }

    /// RIPR-SPEC-0116 amendment: the run properties name the analyzed base
    /// and head with the check JSON's field names; absent without revisions.
    #[test]
    fn sarif_run_properties_name_analyzed_base_and_head() -> Result<(), String> {
        let mut output = sample_output();
        output.analyzed_revisions = Some(crate::analysis::AnalyzedRevisions {
            base_ref: "origin/main".to_string(),
            base_commit: Some("1a2b3c4d5e6f".to_string()),
            merge_base_commit: Some("9f8e7d6c5b4a".to_string()),
            loaded_diff_origin: None,
            head_commit: Some("5d6e7f8a9b0c".to_string()),
            working_tree: true,
        });
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        let properties = &sarif["runs"][0]["properties"];
        assert_eq!(properties["base"], "origin/main");
        assert_eq!(properties["base_commit"], "1a2b3c4d5e6f");
        assert_eq!(properties["merge_base_commit"], "9f8e7d6c5b4a");
        assert_eq!(
            properties["head"],
            serde_json::json!({ "source": "working_tree", "commit": "5d6e7f8a9b0c" })
        );
        output.analyzed_revisions = None;
        let plain = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        let plain_properties = &plain["runs"][0]["properties"];
        for key in ["base", "base_commit", "merge_base_commit", "head"] {
            assert!(
                plain_properties.get(key).is_none(),
                "{key}: {plain_properties}"
            );
        }
        Ok(())
    }

    #[test]
    fn sarif_discloses_incomplete_zero_finding_outcome_at_run_level() -> Result<(), String> {
        let mut output = sample_output();
        output.findings.clear();
        output.analysis_outcome = Some(incomplete_outcome()?);

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let props = &sarif["runs"][0]["properties"];

        assert_eq!(
            sarif["runs"][0]["results"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(props["run_status"], "incomplete");
        assert_eq!(props["analysis_complete"], false);
        assert_eq!(props["analysis_outcome"]["kind"], "unsupported_input");
        assert_eq!(
            props["analysis_outcome"]["limitations"][0]["kind"],
            "unresolved_conflict_markers"
        );
        Ok(())
    }

    #[test]
    fn sarif_renders_seams_with_stable_rule_ids() -> Result<(), String> {
        let rendered =
            render_repo_seams_sarif(&[weakly_gripped_classified()], None, &RiprConfig::default());
        let sarif = parse_json(&rendered)?;
        let rule_ids = rule_ids(&sarif)?;
        let result = first_result(&sarif)?;

        assert!(rule_ids.contains(&"ripr.seam.weakly_gripped".to_string()));
        assert!(rule_ids.contains(&"ripr.seam.suppressed".to_string()));
        assert_eq!(result["ruleId"], "ripr.seam.weakly_gripped");
        assert_eq!(result["properties"]["kind"], "seam");
        assert_eq!(result["properties"]["grip_class"], "weakly_gripped");
        assert_eq!(result["properties"]["seam_kind"], "predicate_boundary");
        Ok(())
    }

    #[test]
    fn sarif_preserves_finding_canonical_gap_properties() -> Result<(), String> {
        let mut output = sample_output();
        output.findings[0].canonical_gap = Some(FindingCanonicalGap {
            id: "gap:python:src/pricing.py:apply_discount:predicate_boundary:predicate:amount>=threshold"
                .to_string(),
            language: "python".to_string(),
            file: "src/pricing.py".to_string(),
            owner: "apply_discount".to_string(),
            behavior_kind: "predicate_boundary".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "amount>=threshold".to_string(),
        });

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;

        assert_eq!(
            result["properties"]["canonical_gap_id"],
            "gap:python:src/pricing.py:apply_discount:predicate_boundary:predicate:amount>=threshold"
        );
        assert_eq!(result["properties"]["canonical_gap"]["language"], "python");
        assert_eq!(
            result["properties"]["canonical_gap"]["normalized_discriminator"],
            "amount>=threshold"
        );
        Ok(())
    }

    #[test]
    fn sarif_normalizes_backslash_canonical_gap_file_to_forward_slash() -> Result<(), String> {
        // Proves gap.file backslash paths are normalized in SARIF properties.
        let mut output = sample_output();
        output.findings[0].canonical_gap = Some(FindingCanonicalGap {
            id: "gap:python:src/pricing.py:discount:predicate_boundary:predicate:x>=0".to_string(),
            language: "python".to_string(),
            file: r"src\pricing.py".to_string(),
            owner: "discount".to_string(),
            behavior_kind: "predicate_boundary".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "x>=0".to_string(),
        });

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;

        assert_eq!(
            result["properties"]["canonical_gap"]["file"], "src/pricing.py",
            "canonical_gap file must use forward slashes in SARIF properties"
        );
        Ok(())
    }

    #[test]
    fn sarif_projects_python_repair_card_properties() -> Result<(), String> {
        let mut output = sample_output();
        let finding = &mut output.findings[0];
        finding.language = Some(LanguageId::Python);
        finding.language_status = Some(LanguageStatus::Preview);
        finding.canonical_gap = Some(FindingCanonicalGap {
            id: "gap:python:src/pricing.py:apply_discount:predicate_boundary:predicate:amount>=threshold"
                .to_string(),
            language: "python".to_string(),
            file: "src/pricing.py".to_string(),
            owner: "apply_discount".to_string(),
            behavior_kind: "predicate_boundary".to_string(),
            probe_kind: "predicate".to_string(),
            normalized_discriminator: "amount>=threshold".to_string(),
        });
        finding.evidence.extend([
            "suggested_test_file: tests/test_pricing.py".to_string(),
            "suggested_test_name: test_apply_discount_threshold_boundary".to_string(),
            "suggested_test_node_id: tests/test_pricing.py::test_apply_discount_threshold_boundary"
                .to_string(),
            "suggested_verify_command: pytest tests/test_pricing.py::test_apply_discount_threshold_boundary"
                .to_string(),
            "suggested_verify_command_confidence: high".to_string(),
        ]);
        // The selected Python location must have its own related row; the
        // sample output's unrelated Rust row cannot supply card evidence.
        finding.related_tests = vec![RelatedTest {
            name: "test_apply_discount_threshold_boundary".to_string(),
            file: PathBuf::from("tests/test_pricing.py"),
            line: 12,
            oracle: Some("assert apply_discount(50, 100)".to_string()),
            oracle_kind: OracleKind::SmokeOnly,
            oracle_strength: OracleStrength::Smoke,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }];

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;
        let card = &result["properties"]["python_repair_card"];

        assert_eq!(card["language"], "python");
        assert_eq!(card["language_status"], "preview");
        assert_eq!(card["authority_boundary"], "preview_advisory_only");
        assert_eq!(
            card["current_test_evidence"],
            "tests/test_pricing.py:12 test_apply_discount_threshold_boundary currently has oracle_strength=smoke, oracle_kind=smoke_only: assert apply_discount(50, 100)"
        );
        assert_eq!(
            card["canonical_gap_id"],
            "gap:python:src/pricing.py:apply_discount:predicate_boundary:predicate:amount>=threshold"
        );
        assert_eq!(
            card["missing_discriminator"],
            "amount == discount_threshold"
        );
        assert_eq!(
            card["suggested_location"]["test_file"],
            "tests/test_pricing.py"
        );
        assert_eq!(
            card["verify"]["command"],
            "pytest tests/test_pricing.py::test_apply_discount_threshold_boundary"
        );
        assert_eq!(
            card["receipt"]["status"],
            "unavailable_until_python_gap_ledger"
        );
        assert!(result["properties"].get("python_no_action").is_none());
        Ok(())
    }

    #[test]
    fn sarif_projects_python_static_limit_no_action_properties() -> Result<(), String> {
        let mut output = sample_output();
        let finding = &mut output.findings[0];
        finding.id = "probe:src_runtime.py:2:python_preview".to_string();
        finding.class = ExposureClass::StaticUnknown;
        finding.language = Some(LanguageId::Python);
        finding.language_status = Some(LanguageStatus::Preview);
        finding.static_limit_kind = Some(StaticLimitKind::DynamicDispatch);
        finding.probe.id = ProbeId("probe:src_runtime.py:2:python_preview".to_string());
        finding.probe.location = SourceLocation::new("src/runtime.py", 2, 1);
        finding.probe.owner = Some(SymbolId("python:src/runtime.py::dispatch".to_string()));
        finding.probe.expression = "return getattr(handler, name)(payload)".to_string();
        finding.missing = vec![
            "Static limit `dynamic_dispatch` prevents bounded repair routing because syntax alone cannot resolve runtime getattr dispatch.".to_string(),
        ];
        finding.stop_reasons = vec![StopReason::DynamicDispatchUnresolved];
        finding.related_tests = vec![RelatedTest {
            name: "test_dispatch_total".to_string(),
            file: PathBuf::from("tests/test_runtime.py"),
            line: 4,
            oracle: Some("assert dispatch(\"total\", 10) == 10".to_string()),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }];

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;
        let no_action = &result["properties"]["python_no_action"];

        assert_eq!(result["ruleId"], "ripr.finding.static_unknown");
        assert_eq!(result["properties"]["language"], "python");
        assert_eq!(result["properties"]["language_status"], "preview");
        assert_eq!(
            result["properties"]["static_limit_kind"],
            "dynamic_dispatch"
        );
        assert!(result["properties"].get("python_repair_card").is_none());
        assert_eq!(no_action["authority_boundary"], "preview_advisory_only");
        assert_eq!(no_action["repairability"], "analyzer_limitation");
        assert_eq!(no_action["repair_packet_ready"], false);
        assert_eq!(no_action["repair_card_present"], false);
        assert_eq!(no_action["static_limit_kind"], "dynamic_dispatch");
        assert_eq!(
            no_action["changed_owner"],
            "python:src/runtime.py::dispatch"
        );
        assert!(
            no_action["why_not_actionable"]
                .as_str()
                .is_some_and(|detail| detail.contains("dynamic_dispatch"))
        );
        assert_eq!(no_action["verify"]["command"], Value::Null);
        assert_eq!(no_action["verify"]["status"], "not_applicable_static_limit");
        assert_eq!(no_action["receipt"]["command"], Value::Null);
        assert_eq!(
            no_action["receipt"]["status"],
            "not_applicable_static_limit"
        );
        assert_eq!(
            no_action["stop_conditions"][0],
            "dynamic_dispatch_unresolved"
        );
        Ok(())
    }

    #[test]
    fn sarif_projects_perl_preview_card_properties() -> Result<(), String> {
        let mut output = sample_output();
        add_perl_preview_card_inputs(&mut output.findings[0]);

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;
        let card = &result["properties"]["perl_preview_card"];

        assert_eq!(card["card_version"], "perl_preview_card.v1");
        assert_eq!(card["language"], "perl");
        assert_eq!(card["language_status"], "preview");
        assert_eq!(card["authority_boundary"], "preview_advisory_only");
        assert_eq!(
            card["surface_scope"],
            "check_json_human_sarif_github_gap_ledger_markdown"
        );
        assert_eq!(card["public_repair_packet"], false);
        assert_eq!(card["repair_packet_ready"], false);
        assert_eq!(card["agent_packet_ready"], false);
        assert_eq!(card["gate_candidate"], false);
        assert_eq!(card["badge_candidate"], false);
        assert_eq!(card["ripr_zero_candidate"], false);
        assert_eq!(card["packet_id"], "perl-preview:gap-return");
        assert_eq!(
            card["canonical_gap_id"],
            "gap:perl:lib/My/App.pm:My::App::discount:return_value:exact_return_assertion:return_value"
        );
        assert_eq!(
            card["changed_owner"],
            "perl:lib/My/App.pm::My::App::discount"
        );
        assert_eq!(card["repair_route"], "add_exact_return_assertion");
        assert_eq!(card["missing_discriminator"], "return_value");
        assert_eq!(
            card["target_test_shape"],
            "Test::More exact_return_assertion"
        );
        assert_eq!(card["suggested_test_location"], "t/app.t::discount_smoke");
        assert_eq!(card["verify"]["command"], "prove t/app.t");
        assert_eq!(card["verify"]["status"], "preview_fact_only_not_delegated");
        assert!(card["receipt"]["command"].is_null());
        assert_eq!(card["receipt"]["status"], "preview_available_not_delegated");
        assert_eq!(card["raw_evidence_refs"][0]["file"], "lib/My/App.pm");
        assert_eq!(card["raw_evidence_refs"][0]["line"], 8);
        assert!(card.get("allowed_edit_surface").is_none());
        assert!(card.get("allowed_edit_boundaries").is_none());
        assert!(card.get("forbidden_files").is_none());
        assert!(card.get("receipt_command").is_none());
        assert!(result["properties"].get("perl_repair_card").is_none());
        assert!(
            result["properties"]
                .get("perl_internal_agent_packet")
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn sarif_projects_python_ordinary_no_action_properties() -> Result<(), String> {
        let mut already_observed = sample_finding();
        already_observed.id = "probe:src_pricing.py:2:observed".to_string();
        already_observed.class = ExposureClass::Exposed;
        already_observed.language = Some(LanguageId::Python);
        already_observed.language_status = Some(LanguageStatus::Preview);
        already_observed.probe.owner =
            Some(SymbolId("python:src/pricing.py::discount".to_string()));
        already_observed.recommended_next_step = None;

        let mut no_related = sample_finding();
        no_related.id = "probe:src_pricing.py:4:no_path".to_string();
        no_related.class = ExposureClass::NoStaticPath;
        no_related.language = Some(LanguageId::Python);
        no_related.language_status = Some(LanguageStatus::Preview);
        no_related.probe.location = SourceLocation::new("src/pricing.py", 4, 1);
        no_related.probe.owner = Some(SymbolId("python:src/pricing.py::discount".to_string()));
        no_related.related_tests.clear();
        no_related.recommended_next_step = None;

        let mut heuristic_only = sample_finding();
        heuristic_only.id = "probe:src_pricing.py:6:heuristic".to_string();
        heuristic_only.class = ExposureClass::WeaklyExposed;
        heuristic_only.language = Some(LanguageId::Python);
        heuristic_only.language_status = Some(LanguageStatus::Preview);
        heuristic_only.probe.location = SourceLocation::new("src/pricing.py", 6, 1);
        heuristic_only.probe.owner = Some(SymbolId("python:src/pricing.py::discount".to_string()));
        heuristic_only.evidence =
            vec!["related_test_uncertain: test_name_similarity (test_discount)".to_string()];
        heuristic_only.recommended_next_step = None;

        let output = CheckOutput {
            findings: vec![already_observed, no_related, heuristic_only],
            ..sample_output()
        };

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let results = results(&sarif)?;

        let expected = [
            (
                "already_observed",
                "missing_proof_already_observed",
                "already observes",
            ),
            (
                "no_related_test",
                "related_python_test_not_found",
                "No related Python test",
            ),
            (
                "heuristic_only",
                "related_test_link_uncertain",
                "Only heuristic Python related-test proximity",
            ),
        ];
        for (result, (kind, stop_condition, reason_text)) in results.iter().zip(expected) {
            let no_action = &result["properties"]["python_no_action"];
            assert!(result["properties"].get("python_repair_card").is_none());
            assert_eq!(no_action["authority_boundary"], "preview_advisory_only");
            assert_eq!(no_action["repairability"], "no_action");
            assert_eq!(no_action["repair_packet_ready"], false);
            assert_eq!(no_action["repair_card_present"], false);
            assert_eq!(no_action["gap_state"], kind);
            assert_eq!(no_action["no_action_kind"], kind);
            assert_eq!(
                no_action["changed_owner"],
                "python:src/pricing.py::discount"
            );
            assert!(
                no_action["why_not_actionable"]
                    .as_str()
                    .is_some_and(|detail| detail.contains(reason_text)),
                "expected reason containing {reason_text:?}, got {no_action:?}"
            );
            assert_eq!(no_action["verify"]["command"], Value::Null);
            assert_eq!(no_action["verify"]["status"], "not_applicable_no_action");
            assert_eq!(no_action["receipt"]["command"], Value::Null);
            assert_eq!(no_action["receipt"]["status"], "not_applicable_no_action");
            assert_eq!(no_action["stop_conditions"][0], stop_condition);
        }
        Ok(())
    }

    #[test]
    fn sarif_preserves_preview_actionability_properties() -> Result<(), String> {
        let mut output = sample_output();
        let finding = &mut output.findings[0];
        finding.language = Some(LanguageId::TypeScript);
        finding.language_status = Some(LanguageStatus::Preview);
        finding.owner_kind = Some(OwnerKind::Function);
        finding.static_limit_kind = Some(StaticLimitKind::MockedModule);
        finding.evidence = vec![
            "gap_state: unknown".to_string(),
            "actionability_category: incomplete_repair_packet".to_string(),
            "why_not_actionable: TypeScript preview lacks a complete repair packet contract"
                .to_string(),
            "repair_route: project canonical TypeScript repair packet fields later".to_string(),
            "missing_actionability_fields: canonical_gap_id, verify_command".to_string(),
            "evidence_needed_to_promote: canonical gap identity and verify command".to_string(),
            "raw_evidence_ref: file=src/lib.ts;line=2;kind=typescript_preview_probe;source_id=probe:src_lib.ts:2:typescript_preview;owner=discountedTotal".to_string(),
        ];

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;

        assert_eq!(result["properties"]["language"], "typescript");
        assert_eq!(result["properties"]["language_status"], "preview");
        assert_eq!(result["properties"]["owner_kind"], "function");
        assert_eq!(result["properties"]["static_limit_kind"], "mocked_module");
        assert_eq!(
            result["properties"]["preview_actionability"]["authority_boundary"],
            "preview_advisory_only"
        );
        assert_eq!(
            result["properties"]["preview_actionability"]["repair_packet_ready"],
            false
        );
        assert_eq!(
            result["properties"]["preview_actionability"]["raw_evidence_refs"][0]["file"],
            "src/lib.ts"
        );
        Ok(())
    }

    #[test]
    fn sarif_preserves_bun_cross_language_grip_card_properties() -> Result<(), String> {
        let mut output = sample_output();
        let finding = &mut output.findings[0];
        finding.language = Some(LanguageId::TypeScript);
        finding.language_status = Some(LanguageStatus::Preview);
        finding.owner_kind = Some(OwnerKind::Function);
        finding.evidence = vec![
            "owner: Blob::from_js_without_defer_gc".to_string(),
            "gap_state: static_limitation".to_string(),
            "actionability_category: cross_language_oracle_visibility_unresolved".to_string(),
            "why_not_actionable: configured Bun Blob TypeScript preview evidence is missing external discriminator(s): resizable_array_buffer; placement can name the existing TypeScript Blob test file, but RIPR cannot emit a public repair packet without verification, receipt, and edit-surface evidence".to_string(),
            "repair_route: analysis/cross-language-oracle-visibility".to_string(),
            "missing_actionability_fields: verify_command, receipt_command, must_not_change, allowed_edit_surface".to_string(),
            "missing_graph_legs: boundary_discriminator:resizable_array_buffer".to_string(),
            "unlock_condition: add or inspect the missing external TypeScript discriminator(s) in test/js/web/fetch/blob.test.ts and keep repair-packet projection blocked until verify, receipt, and edit-surface evidence exists".to_string(),
            "evidence_needed_to_promote: the missing TypeScript discriminator in the configured Blob test file plus verify command, receipt command, and edit constraints before repair-packet projection".to_string(),
            "raw_evidence_ref: leg=rust_seam;file=src/jsc/Blob.rs;line=42;kind=rust_boundary;source_id=probe:src_jsc_Blob_rs:42:typescript_bun_ub_cross_language_preview;owner=Blob::from_js_without_defer_gc;sample=array_buffer.shared || array_buffer.resizable".to_string(),
            "typescript_bun_ub_bridge_hint: confidence=configured_hint rust_file=src/jsc/Blob.rs rust_owner=Blob::from_js_without_defer_gc rust_boundary=\"array_buffer.shared || array_buffer.resizable\" ts_test_file=test/js/web/fetch/blob.test.ts".to_string(),
            "typescript_bun_ub_bridge_verdict: ts_missing_resizable missing_discriminators=resizable_array_buffer action=route_cross_language_oracle_visibility_limitation suggested_test_file=test/js/web/fetch/blob.test.ts repair_packet_ready=false".to_string(),
            "typescript_bun_ub_cross_language_grip: state=rust_ungripped_ts_missing_discriminator rust_grip=ungripped ts_verdict=ts_missing_resizable action=route_cross_language_oracle_visibility_limitation authority=preview_advisory_only suggested_test_file=test/js/web/fetch/blob.test.ts repair_packet_ready=false".to_string(),
            "typescript_bun_ub_test_placement: rank=1 suggested_test_file=test/js/web/fetch/blob.test.ts reason=\"existing Blob + ArrayBuffer integration tests live there; missing discriminator is resizable ArrayBuffer\" basis=configured_bridge_suggested_test_file,same_js_surface,same_boundary_vocabulary authority=preview_advisory_only repair_packet_ready=false".to_string(),
            "typescript_bun_ub_bridge_hint: confidence=configured_hint rust_file=src/jsc/array_buffer.rs rust_owner=copy_to_unshared rust_boundary=\"array_buffer.shared || array_buffer.resizable\" ts_test_file=test/js/web/fetch/blob.test.ts".to_string(),
            "typescript_bun_ub_bridge_verdict: ts_missing_shared missing_discriminators=shared_array_buffer action=route_cross_language_oracle_visibility_limitation suggested_test_file=test/js/web/fetch/blob.test.ts repair_packet_ready=false".to_string(),
            "typescript_bun_ub_cross_language_grip: state=rust_ungripped_ts_missing_discriminator rust_grip=ungripped ts_verdict=ts_missing_shared action=route_cross_language_oracle_visibility_limitation authority=preview_advisory_only suggested_test_file=test/js/web/fetch/blob.test.ts repair_packet_ready=false".to_string(),
        ];
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "resizable_array_buffer".to_string(),
            reason: "missing resizable ArrayBuffer discriminator".to_string(),
            flow_sink: None,
        }];

        let rendered = render_findings_sarif(&output, &RiprConfig::default(), &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;
        let grip = &result["properties"]["typescript_preview_card"]["bun_cross_language_grip"];

        assert_eq!(grip["state"], "rust_ungripped_ts_missing_discriminator");
        assert_eq!(grip["rust_seam"]["file"], "src/jsc/Blob.rs");
        assert_eq!(
            grip["typescript_evidence"]["missing_discriminators"][0],
            "resizable_array_buffer"
        );
        assert_eq!(grip["profiles"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            grip["profiles"][0]["rust_seam"]["owner"],
            "Blob::from_js_without_defer_gc"
        );
        assert_eq!(
            grip["profiles"][0]["placement"]["reason"],
            "existing Blob + ArrayBuffer integration tests live there; missing discriminator is resizable ArrayBuffer"
        );
        assert_eq!(
            grip["profiles"][1]["rust_seam"]["owner"],
            "copy_to_unshared"
        );
        assert!(grip["profiles"][1]["placement"].is_null());
        assert_eq!(
            grip["limitation_category"],
            "cross_language_oracle_visibility_unresolved"
        );
        assert_eq!(
            grip["repair_route"],
            "analysis/cross-language-oracle-visibility"
        );
        assert_eq!(
            grip["missing_graph_legs"][0],
            "boundary_discriminator:resizable_array_buffer"
        );
        assert_eq!(grip["raw_evidence_refs"][0]["leg"], "rust_seam");
        assert_eq!(
            grip["suggested_test_file"],
            "test/js/web/fetch/blob.test.ts"
        );
        assert_eq!(
            grip["placement"]["suggested_test_file"],
            "test/js/web/fetch/blob.test.ts"
        );
        assert_eq!(
            grip["placement"]["reason"],
            "existing Blob + ArrayBuffer integration tests live there; missing discriminator is resizable ArrayBuffer"
        );
        assert_eq!(grip["placement"]["repair_packet_ready"], false);
        assert_eq!(grip["repair_packet_ready"], false);
        Ok(())
    }

    #[test]
    fn sarif_uses_configured_finding_severity() -> Result<(), String> {
        let config = parse_config(
            r#"
[severity.findings]
weakly_exposed = "note"
"#,
        )?;
        let rendered = render_findings_sarif(&sample_output(), &config, &[]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;

        assert_eq!(result["level"], "note");
        assert_eq!(result["properties"]["severity"], "note");
        Ok(())
    }

    /// #10717: `properties.confidence` is the contract's two-decimal value, not
    /// the `f32` widened to `f64` (`0.4000000059604645`).
    #[test]
    fn sarif_finding_confidence_uses_contract_precision() -> Result<(), String> {
        for (stored, wire, noisy) in [
            (0.4_f32, 0.4_f64, true),
            (0.92, 0.92, true),
            (0.6, 0.6, true),
            (0.75, 0.75, false),
        ] {
            // Fixture check: the pre-fix widening really is noisy for the
            // inexact values, and exact ones (0.75) are the clean control.
            let widened = serde_json::json!(stored).to_string();
            assert_eq!(widened.len() > 6, noisy, "fixture {stored}: {widened}");
            let mut output = sample_output();
            output.findings[0].confidence = stored;
            let rendered = render_findings_sarif(&output, &parse_config("")?, &[]);
            let sarif = parse_json(&rendered)?;
            let result = first_result(&sarif)?;
            assert_eq!(result["properties"]["confidence"], serde_json::json!(wire));
            assert_eq!(
                result["properties"]["confidence"].to_string(),
                format!("{wire}")
            );
        }
        Ok(())
    }

    #[test]
    fn sarif_uses_configured_seam_severity() -> Result<(), String> {
        let config = parse_config(
            r#"
[severity.seams]
weakly_gripped = "note"
"#,
        )?;
        let rendered = render_repo_seams_sarif(&[weakly_gripped_classified()], None, &config);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;

        assert_eq!(result["level"], "note");
        assert_eq!(result["properties"]["severity"], "note");
        Ok(())
    }

    #[test]
    fn sarif_omits_off_seam_class() -> Result<(), String> {
        let rendered = render_repo_seams_sarif(
            &[strongly_gripped_classified()],
            None,
            &RiprConfig::default(),
        );
        let sarif = parse_json(&rendered)?;
        let results = results(&sarif)?;

        assert!(results.is_empty(), "strongly_gripped seams default to off");
        Ok(())
    }

    #[test]
    fn sarif_attaches_suppression_metadata() -> Result<(), String> {
        let suppression = SuppressionEntry {
            kind: SuppressionKind::ExposureGap,
            finding_id: Some("finding:discount".to_string()),
            test: None,
            path: None,
            reason: "tracked in integration suite".to_string(),
            owner: "team-ripr".to_string(),
            expires: Some("2099-01-01".to_string()),
            scope: None,
            created_at: None,
            last_seen: None,
            review_by: None,
            expected_visibility: None,
            static_class: None,
            language: None,
            language_status: None,
            block_line: 4,
        };
        let rendered =
            render_findings_sarif(&sample_output(), &RiprConfig::default(), &[suppression]);
        let sarif = parse_json(&rendered)?;
        let result = first_result(&sarif)?;

        assert_eq!(result["suppressions"][0]["kind"], "external");
        assert_eq!(
            result["suppressions"][0]["justification"],
            "tracked in integration suite"
        );
        assert_eq!(
            result["suppressions"][0]["properties"]["owner"],
            "team-ripr"
        );
        Ok(())
    }

    #[test]
    fn sarif_output_is_valid_json() -> Result<(), String> {
        let findings = render_findings_sarif(&sample_output(), &RiprConfig::default(), &[]);
        let seams =
            render_repo_seams_sarif(&[weakly_gripped_classified()], None, &RiprConfig::default());

        let _ = parse_json(&findings)?;
        let _ = parse_json(&seams)?;
        Ok(())
    }

    #[test]
    fn sarif_preserves_static_language() {
        let rendered = render_findings_sarif(&sample_output(), &RiprConfig::default(), &[]);
        assert!(rendered.contains("weakly_exposed"));
        assert!(rendered.contains("static exposure"));
        assert!(rendered.contains("equality boundary is absent"));
    }

    fn assertion_values(count: usize) -> Vec<ValueFact> {
        (1..=count)
            .map(|line| ValueFact {
                line,
                text: format!("assert_eq!(value, {line});"),
                value: format!("value_{line}"),
                context: ValueContext::AssertionArgument,
            })
            .collect()
    }

    #[test]
    fn sarif_caps_observed_values_and_discloses_the_total() -> Result<(), String> {
        use crate::output::observed_values::MAX_OBSERVED_VALUES_PER_FINDING;

        let mut output = sample_output();
        output.findings[0].activation.observed_values = assertion_values(500);
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        let props = &first_result(&sarif)?["properties"];
        assert_eq!(
            props["observed_values"].as_array().map(Vec::len),
            Some(MAX_OBSERVED_VALUES_PER_FINDING)
        );
        assert_eq!(props["observed_values_total"], 500);

        let mut seam = weakly_gripped_classified();
        seam.evidence.observed_values = assertion_values(500).into_iter().map(Into::into).collect();
        let sarif = parse_json(&render_repo_seams_sarif(
            &[seam],
            None,
            &RiprConfig::default(),
        ))?;
        let props = &first_result(&sarif)?["properties"];
        assert_eq!(
            props["observed_values"].as_array().map(Vec::len),
            Some(MAX_OBSERVED_VALUES_PER_FINDING)
        );
        assert_eq!(props["observed_values_total"], 500);
        Ok(())
    }

    #[test]
    fn sarif_omits_observed_values_total_under_the_cap() -> Result<(), String> {
        let sarif = parse_json(&render_findings_sarif(
            &sample_output(),
            &RiprConfig::default(),
            &[],
        ))?;
        let props = &first_result(&sarif)?["properties"];
        assert_eq!(props["observed_values"].as_array().map(Vec::len), Some(1));
        assert!(props.get("observed_values_total").is_none());

        let sarif = parse_json(&render_repo_seams_sarif(
            &[weakly_gripped_classified()],
            None,
            &RiprConfig::default(),
        ))?;
        let props = &first_result(&sarif)?["properties"];
        assert_eq!(props["observed_values"].as_array().map(Vec::len), Some(1));
        assert!(props.get("observed_values_total").is_none());
        Ok(())
    }

    #[test]
    fn sarif_repo_seams_discloses_seam_limit_in_run_properties() -> Result<(), String> {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        let info = SeamLimitInfo {
            analyzed: 7,
            total: 2000,
            source: SeamLimitSource::Default,
        };
        let rendered = render_repo_seams_sarif(
            &[weakly_gripped_classified()],
            Some(&info),
            &RiprConfig::default(),
        );
        let sarif = parse_json(&rendered)?;
        let props = &sarif["runs"][0]["properties"];
        assert_eq!(
            props["run_status"], "seam_limit_applied",
            "run_status missing in properties:\n{sarif}"
        );
        let limitations = props["limitations"]
            .as_array()
            .ok_or("limitations array missing")?;
        assert_eq!(limitations.len(), 1);
        assert_eq!(limitations[0]["category"], "repo_seam_limit_applied");
        assert_eq!(limitations[0]["seams_analyzed"], 7);
        assert_eq!(limitations[0]["seams_total"], 2000);
        assert_eq!(limitations[0]["control"], "RIPR_REPO_EXPOSURE_SEAM_LIMIT");
        Ok(())
    }

    #[test]
    fn sarif_repo_seams_shows_complete_run_status_when_no_cap() -> Result<(), String> {
        let rendered =
            render_repo_seams_sarif(&[weakly_gripped_classified()], None, &RiprConfig::default());
        let sarif = parse_json(&rendered)?;
        let props = &sarif["runs"][0]["properties"];
        assert_eq!(
            props["run_status"], "complete",
            "run_status complete missing in:\n{sarif}"
        );
        assert!(
            props["limitations"].is_null(),
            "limitations must be absent when no cap fired:\n{sarif}"
        );
        Ok(())
    }

    fn parse_config(text: &str) -> Result<RiprConfig, String> {
        crate::config::tests_only_parse(text)
    }

    fn parse_json(text: &str) -> Result<Value, String> {
        serde_json::from_str(text).map_err(|err| err.to_string())
    }

    /// #7371: every rule carries a real per-class fullDescription, help, and
    /// a helpUri worth clicking — not the class name rearranged and one
    /// shared spec link.
    /// #11219: the Security-tab title says what to do, not just the class, and
    /// the message is bounded and leads with the missing check.
    #[test]
    fn rule_titles_are_author_facing_and_messages_are_bounded() -> Result<(), String> {
        for rule in finding_rules().iter().chain(seam_rules().iter()) {
            let id = rule["id"].as_str().unwrap_or_default();
            let title = rule["shortDescription"]["text"]
                .as_str()
                .unwrap_or_default();
            let class = id.rsplit('.').next().unwrap_or_default();
            if title.is_empty() || title.contains(class) || title.starts_with("Static exposure") {
                return Err(format!("rule {id} title is not author-facing: {title}"));
            }
        }
        let message = finding_message(&sample_finding());
        assert!(
            message.starts_with("Missing check: no related test calls the equality boundary. "),
            "{message}"
        );
        assert!(message.contains("Next step:"), "{message}");

        // The next step is never cut: it stays whole after the bounded head.
        let mut long = sample_finding();
        long.recommended_next_step = Some("word ".repeat(100));
        let message = finding_message(&long);
        assert!(message.ends_with(&reconcile_next_step(&long)), "{message}");
        // A long expression is cut by the bound, never the leading missing check.
        let mut wide = sample_finding();
        wide.probe.expression = "x".repeat(2000);
        let message = finding_message(&wide);
        assert!(message.starts_with("Missing check: no related test calls the equality boundary"));
        let head = message.split(". Next step:").next().unwrap_or_default();
        assert_eq!(head.chars().count(), MAX_MESSAGE_CHARS);
        assert!(head.ends_with("..."));
        // Control: without a missing discriminator no "Missing check" is invented.
        let mut none = sample_finding();
        none.activation.missing_discriminators.clear();
        assert!(!finding_message(&none).contains("Missing check"));
        Ok(())
    }

    #[test]
    fn rules_carry_per_class_help_and_meaningful_help_uri() -> Result<(), String> {
        let finding = finding_rules();
        assert_eq!(finding.len(), 7);
        let seam = seam_rules();
        assert_eq!(seam.len(), 11);
        for rule in finding.iter().chain(seam.iter()) {
            let id = rule["id"]
                .as_str()
                .ok_or_else(|| "rule missing id".to_string())?;
            if rule["shortDescription"]["text"].as_str().is_none() {
                return Err(format!("rule {id} missing shortDescription"));
            }
            let full = rule["fullDescription"]["text"]
                .as_str()
                .ok_or_else(|| format!("rule {id} missing fullDescription"))?;
            let help = rule["help"]["text"]
                .as_str()
                .ok_or_else(|| format!("rule {id} missing help"))?;
            let help_uri = rule["helpUri"]
                .as_str()
                .ok_or_else(|| format!("rule {id} missing helpUri"))?;
            let class = id
                .strip_prefix("ripr.finding.")
                .or_else(|| id.strip_prefix("ripr.seam."));
            let Some(class) = class else {
                return Err(format!("rule id {id} does not name its class"));
            };
            if full == format!("Static exposure finding classified as {class}")
                || full == format!("Repo seam grip evidence classified as {class}")
            {
                return Err(format!("rule {id} fullDescription restates the class name"));
            }
            if help.len() < 20 {
                return Err(format!("rule {id} help is too thin: {help}"));
            }
            if help_uri.contains("RIPR-SPEC-0008") {
                return Err(format!(
                    "rule {id} keeps the severity-policy helpUri instead of a class anchor"
                ));
            }
            if !help_uri.starts_with(SWARM_BLOB_MAIN) {
                return Err(format!(
                    "rule {id} helpUri is not a current swarm blob: {help_uri}"
                ));
            }
            if help_uri.contains("https://github.com/EffortlessMetrics/ripr/blob/") {
                return Err(format!(
                    "rule {id} helpUri still cites the release-repo snapshot: {help_uri}"
                ));
            }
        }
        // Spot anchors: the finding rules point at the exposure class table,
        // the seam rules at the seam inventory spec.
        assert!(
            finding_rules()[0]["helpUri"]
                .as_str()
                .is_some_and(|uri| uri.contains("STATIC_EXPOSURE_MODEL.md#exposure-classes"))
        );
        assert!(
            seam_rules()[0]["helpUri"]
                .as_str()
                .is_some_and(|uri| uri.contains("RIPR-SPEC-0005-repo-seam-inventory.md"))
        );
        Ok(())
    }

    const SWARM_BLOB_MAIN: &str = "https://github.com/EffortlessMetrics/ripr-swarm/blob/main/";

    /// #7397: the cited swarm docs exist in this checkout and cover the
    /// class-meaning (helpUri) plus SARIF URI-encoding / `--root` stability
    /// invariants the emitter implements. RIPR-SPEC-0008 is not the per-rule
    /// helpUri (#7371) but remains the SARIF contract for those invariants.
    #[test]
    fn cited_swarm_docs_cover_class_meaning_and_sarif_uri_invariants() -> Result<(), String> {
        let exposure = include_str!("../../../../docs/STATIC_EXPOSURE_MODEL.md");
        let seam_spec =
            include_str!("../../../../docs/specs/RIPR-SPEC-0005-repo-seam-inventory.md");
        let sarif_spec = include_str!("../../../../docs/specs/RIPR-SPEC-0008-sarif-ci-policy.md");
        if !exposure.contains("## Exposure Classes") {
            return Err(
                "cited STATIC_EXPOSURE_MODEL.md is missing ## Exposure Classes".to_string(),
            );
        }
        if !seam_spec.contains("SeamGripClass") {
            return Err("cited RIPR-SPEC-0005 is missing SeamGripClass".to_string());
        }
        if !sarif_spec.contains("do not depend on how")
            || !sarif_spec.contains("`--root` is spelled")
        {
            return Err("RIPR-SPEC-0008 lost the --root spelling stability invariant".to_string());
        }
        if !sarif_spec.contains("percent-encoded as an RFC 3986") {
            return Err("RIPR-SPEC-0008 lost the RFC 3986 percent-encoding invariant".to_string());
        }
        Ok(())
    }

    fn rule_ids(sarif: &Value) -> Result<Vec<String>, String> {
        let Some(rules) = sarif["runs"][0]["tool"]["driver"]["rules"].as_array() else {
            return Err("missing SARIF rules array".to_string());
        };
        rules
            .iter()
            .map(|rule| {
                rule["id"]
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "SARIF rule missing id".to_string())
            })
            .collect()
    }

    fn first_result(sarif: &Value) -> Result<&Value, String> {
        let results = results(sarif)?;
        results
            .first()
            .copied()
            .ok_or_else(|| "expected at least one SARIF result".to_string())
    }

    fn results(sarif: &Value) -> Result<Vec<&Value>, String> {
        let Some(results) = sarif["runs"][0]["results"].as_array() else {
            return Err("missing SARIF results array".to_string());
        };
        Ok(results.iter().collect())
    }

    fn add_perl_preview_card_inputs(finding: &mut Finding) {
        finding.id = "probe:lib_My_App_pm:8:perl_return".to_string();
        finding.canonical_gap = Some(FindingCanonicalGap {
            id: "gap:perl:lib/My/App.pm:My::App::discount:return_value:exact_return_assertion:return_value"
                .to_string(),
            language: "perl".to_string(),
            file: "lib/My/App.pm".to_string(),
            owner: "perl:lib/My/App.pm::My::App::discount".to_string(),
            behavior_kind: "return_value".to_string(),
            probe_kind: "exact_return_assertion".to_string(),
            normalized_discriminator: "return_value".to_string(),
        });
        finding.probe = Probe {
            id: ProbeId("probe:lib_My_App_pm:8:perl_return".to_string()),
            location: SourceLocation::new("lib/My/App.pm", 8, 5),
            owner: Some(SymbolId(
                "perl:lib/My/App.pm::My::App::discount".to_string(),
            )),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("return $price".to_string()),
            after: Some("return $discounted".to_string()),
            expression: "return $discounted".to_string(),
            expected_sinks: vec!["return_value".to_string()],
            required_oracles: vec!["exact_return_assertion".to_string()],
        };
        finding.class = ExposureClass::WeaklyExposed;
        finding.ripr = RiprEvidence {
            reach: stage(
                StageState::Yes,
                "Perl fact packet links the related test to the changed owner",
            ),
            infect: stage(
                StageState::Yes,
                "Changed return value reaches the owner result",
            ),
            propagate: stage(
                StageState::Yes,
                "Return value can propagate to Test::More assertion",
            ),
            reveal: RevealEvidence {
                observe: stage(StageState::Yes, "Related test reaches the changed owner"),
                discriminate: stage(StageState::Weak, "Exact return discriminator is missing"),
            },
        };
        finding.confidence = 0.8;
        finding.evidence = vec![
            "perl_packet_id: perl-preview:gap-return".to_string(),
            "perl_repair_kind: add_exact_return_assertion".to_string(),
            "perl_target_test_shape: Test::More exact_return_assertion".to_string(),
            "perl_suggested_test_location: t/app.t::discount_smoke".to_string(),
            "perl_suggested_assertion: assert the exact returned `return_value` value".to_string(),
            "perl_verify_command: prove t/app.t".to_string(),
            "perl_receipt_command: ripr agent receipt --root . --verify-json target/ripr/workflow/agent-verify.json --seam-id perl-gap --json".to_string(),
            "perl_confidence: medium".to_string(),
            "perl_allowed_edit_boundary: t/app.t".to_string(),
            "perl_forbidden_edit_boundary: lib/My/App.pm, badges/ripr-plus.json".to_string(),
            "perl_stop_if: perl-lsp packet status changes".to_string(),
            "perl_must_not_change: do not edit Perl production code".to_string(),
            "raw_evidence_ref: leg=perl_change;file=lib/My/App.pm;line=8;kind=perl_change;source_id=change:lib/My/App.pm:8:return;owner=perl:lib/My/App.pm::My::App::discount;sample=return $discounted".to_string(),
            "raw_evidence_ref: leg=perl_oracle;file=t/app.t;line=7;kind=perl_oracle;source_id=oracle:t/app.t:7:is;owner=perl:lib/My/App.pm::My::App::discount;sample=is(discount(...), 90)".to_string(),
        ];
        finding.missing = vec!["return_value".to_string()];
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "return_value".to_string(),
            reason: "Related Perl test reaches the owner but lacks an exact return discriminator"
                .to_string(),
            flow_sink: None,
        }];
        finding.related_tests = vec![RelatedTest {
            name: "discount_smoke".to_string(),
            file: PathBuf::from("t/app.t"),
            line: 7,
            oracle: Some("ok(discount(...))".to_string()),
            oracle_kind: OracleKind::SmokeOnly,
            oracle_strength: OracleStrength::Weak,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }];
        finding.recommended_next_step = Some("Add a focused Perl assertion.".to_string());
        finding.language = Some(LanguageId::Perl);
        finding.language_status = Some(LanguageStatus::Preview);
    }

    fn sample_output() -> CheckOutput {
        CheckOutput {
            value_origins: Default::default(),
            harness_projections: Vec::new(),
            schema_version: "0.1".to_string(),
            tool: "ripr".to_string(),
            mode: Mode::Draft,
            root: PathBuf::from("repo"),
            base: Some("origin/main".to_string()),
            summary: Summary::default(),
            findings: vec![sample_finding()],
            preview_language_advisories: Vec::new(),
            language_runs: Vec::new(),
            no_scope_provided: false,
            unanalyzed_working_tree: false,
            untracked_working_tree_source_paths: Vec::new(),
            ignored_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            analysis_outcome: None,
            partial_scope: None,
            analyzed_revisions: None,
        }
    }

    fn incomplete_outcome() -> Result<AnalysisOutcome, String> {
        let recovery = AnalysisRecovery::new(
            AnalysisRecoveryKind::ResolveConflicts,
            "resolve conflict markers before rerunning analysis",
        )?;
        let limitation = AnalysisLimitation::new(
            AnalysisLimitationKind::UnresolvedConflictMarkers,
            AnalysisStage::DiffParse,
            recovery,
        );
        AnalysisOutcome::new(
            AnalysisOutcomeKind::UnsupportedInput,
            Default::default(),
            AnalysisOutcomeCounts::default(),
            vec![limitation],
        )
    }

    fn complete_with_findings_outcome(finding_count: u64) -> Result<AnalysisOutcome, String> {
        AnalysisOutcome::new(
            AnalysisOutcomeKind::CompleteWithFindings,
            Default::default(),
            AnalysisOutcomeCounts {
                changed_file_count: 1,
                changed_line_count: 1,
                candidate_line_count: 1,
                probe_count: 1,
                finding_count,
            },
            Vec::new(),
        )
    }

    fn complete_no_findings_outcome() -> Result<AnalysisOutcome, String> {
        AnalysisOutcome::new(
            AnalysisOutcomeKind::CompleteNoFindings,
            Default::default(),
            AnalysisOutcomeCounts {
                changed_file_count: 1,
                changed_line_count: 1,
                candidate_line_count: 1,
                probe_count: 1,
                finding_count: 0,
            },
            Vec::new(),
        )
    }

    fn sample_finding() -> Finding {
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: "finding:discount".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:src/pricing.rs:88:predicate".to_string()),
                location: SourceLocation::new("src/pricing.rs", 88, 9),
                owner: Some(SymbolId("pricing::discounted_total".to_string())),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: Some("amount > discount_threshold".to_string()),
                after: Some("amount >= discount_threshold".to_string()),
                expression: "amount >= discount_threshold".to_string(),
                expected_sinks: vec!["return_value".to_string()],
                required_oracles: vec!["exact returned value".to_string()],
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes, "related tests call discounted_total"),
                infect: stage(StageState::Weak, "boundary value is missing"),
                propagate: stage(StageState::Yes, "local flow reaches return value"),
                reveal: RevealEvidence {
                    observe: stage(StageState::Yes, "tests assert returned values"),
                    discriminate: stage(StageState::Weak, "equality boundary is absent"),
                },
            },
            confidence: 0.75,
            evidence: vec!["local flow reaches return value".to_string()],
            missing: vec!["amount == discount_threshold".to_string()],
            flow_sinks: vec![FlowSinkFact {
                kind: FlowSinkKind::ReturnValue,
                text: "returned discounted total".to_string(),
                line: 90,
                owner: Some(SymbolId("pricing::discounted_total".to_string())),
            }],
            activation: ActivationEvidence {
                observed_values: vec![ValueFact {
                    line: 12,
                    text: "discounted_total(50, 100)".to_string(),
                    value: "amount = 50".to_string(),
                    context: ValueContext::FunctionArgument,
                }],
                missing_discriminators: vec![MissingDiscriminatorFact {
                    value: "amount == discount_threshold".to_string(),
                    reason: "no related test calls the equality boundary".to_string(),
                    flow_sink: None,
                }],
            },
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: vec![RelatedTest {
                name: "below_threshold_has_no_discount".to_string(),
                file: PathBuf::from("tests/pricing.rs"),
                line: 12,
                oracle: Some("assert_eq!(discounted_total(50, 100), 50)".to_string()),
                oracle_kind: OracleKind::ExactValue,
                oracle_strength: OracleStrength::Strong,
                relation_reason: None,
                relation_confidence: None,
                miss: None,
                miss_support: None,
                role: crate::domain::RelatedTestRole::Contributing,
            }],
            recommended_next_step: Some("Add an equality-boundary assertion".to_string()),
            language: None,
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }

    fn seam_fingerprints(seams: &[ClassifiedSeam]) -> Result<Vec<String>, String> {
        let sarif = parse_json(&render_repo_seams_sarif(
            seams,
            None,
            &RiprConfig::default(),
        ))?;
        results(&sarif)?
            .iter()
            .map(|result| {
                let partial = result["partialFingerprints"]["riprFingerprintV1"]
                    .as_str()
                    .ok_or_else(|| format!("missing partial fingerprint: {result}"))?;
                if result["fingerprints"]["riprFingerprintV1"] != partial {
                    return Err(format!("fingerprints differ from partial: {result}"));
                }
                Ok(partial.to_string())
            })
            .collect()
    }

    fn weak_seam(
        owner: &str,
        kind: SeamKind,
        position: (usize, usize),
        expr: &str,
    ) -> ClassifiedSeam {
        classified_seam_at(
            SeamGripClass::WeaklyGripped,
            "src/pricing.rs",
            owner,
            kind,
            position,
            expr,
        )
    }

    /// #11219: an unrelated edit above a seam moves its byte offset, display
    /// line and analysis `SeamId`, but not its code-scanning alert identity.
    #[test]
    fn seam_fingerprint_survives_lines_inserted_above() -> Result<(), String> {
        let before = weak_seam(
            "pricing::total",
            SeamKind::PredicateBoundary,
            (42, 3),
            "a >= b",
        );
        let after = weak_seam(
            "pricing::total",
            SeamKind::PredicateBoundary,
            (142, 13),
            "a >= b",
        );
        // Fixture construction: the shift really moved every location input.
        assert_ne!(
            before.seam.id(),
            after.seam.id(),
            "analysis id is offset-derived"
        );
        assert_ne!(before.seam.display_line(), after.seam.display_line());
        assert_ne!(before.seam.byte_offset(), after.seam.byte_offset());
        let first = seam_fingerprints(&[before])?;
        let second = seam_fingerprints(&[after])?;
        assert_eq!(first.len(), 1);
        assert_eq!(
            first, second,
            "fingerprint must not follow lines or offsets"
        );
        assert!(
            first[0].starts_with("ripr.seam.weakly_gripped|src/pricing.rs|"),
            "{}",
            first[0]
        );
        assert!(first[0].ends_with("|0"), "{}", first[0]);
        Ok(())
    }

    #[test]
    fn seam_fingerprint_ignores_expression_whitespace_layout() -> Result<(), String> {
        let tight = weak_seam("m::f", SeamKind::ReturnValue, (10, 2), "a >= b");
        let spread = weak_seam("m::f", SeamKind::ReturnValue, (10, 2), "a  >=\n    b");
        assert_ne!(tight.seam.expression(), spread.seam.expression());
        assert_eq!(seam_fingerprints(&[tight])?, seam_fingerprints(&[spread])?);
        Ok(())
    }

    #[test]
    fn seam_fingerprint_keeps_string_literal_contents_distinct() -> Result<(), String> {
        let seam = |expression: &str| weak_seam("m::f", SeamKind::ReturnValue, (10, 2), expression);
        let literals = [
            seam("pick(\"a  b\")"),
            seam("pick(\"a b\")"),
            seam("pick(\"a\\\" b\")"),
            seam("pick(r#\"a  b\"#)"),
            seam("pick(r#\"a b\"#)"),
        ];
        // The fixture really holds literals that differ only by inner spacing.
        assert_eq!(literals[0].seam.expression(), "pick(\"a  b\")");
        assert_eq!(literals[3].seam.expression(), "pick(r#\"a  b\"#)");
        let fingerprints = seam_fingerprints(&literals)?;
        let unique: std::collections::BTreeSet<_> = fingerprints.iter().collect();
        assert_eq!(unique.len(), literals.len(), "{fingerprints:?}");
        // Whitespace outside the literal still collapses.
        let loose = seam("pick(  \"a  b\"\n)");
        let tight = seam("pick( \"a  b\" )");
        assert_eq!(seam_fingerprints(&[loose])?, seam_fingerprints(&[tight])?);
        Ok(())
    }

    #[test]
    fn distinct_seams_never_share_a_seam_fingerprint() -> Result<(), String> {
        let variants = vec![
            weak_seam("m::f", SeamKind::PredicateBoundary, (10, 2), "a >= b"),
            // Same name, different scope.
            weak_seam("m::g", SeamKind::PredicateBoundary, (10, 2), "a >= b"),
            // Different kind.
            weak_seam("m::f", SeamKind::ReturnValue, (10, 2), "a >= b"),
            // Different expression.
            weak_seam("m::f", SeamKind::PredicateBoundary, (10, 2), "a > b"),
            // Different file.
            classified_seam_at(
                SeamGripClass::WeaklyGripped,
                "src/other.rs",
                "m::f",
                SeamKind::PredicateBoundary,
                (10, 2),
                "a >= b",
            ),
            // Field-boundary bleed: owner/expression split differently.
            weak_seam("m::f|a", SeamKind::PredicateBoundary, (10, 2), ">= b"),
        ];
        let fingerprints = seam_fingerprints(&variants)?;
        assert_eq!(fingerprints.len(), variants.len());
        let unique: std::collections::BTreeSet<_> = fingerprints.iter().collect();
        assert_eq!(unique.len(), variants.len(), "{fingerprints:?}");
        Ok(())
    }

    #[test]
    fn duplicate_seams_in_one_scope_get_distinct_stable_ordinals() -> Result<(), String> {
        let seam_at = |offset: usize, line: usize| {
            weak_seam("m::f", SeamKind::CallPresence, (offset, line), "log(x)")
        };
        // Input order is not source order; ordinals follow source order.
        let fingerprints =
            seam_fingerprints(&[seam_at(300, 30), seam_at(100, 10), seam_at(200, 20)])?;
        let unique: std::collections::BTreeSet<_> = fingerprints.iter().collect();
        assert_eq!(unique.len(), 3, "{fingerprints:?}");
        assert!(fingerprints[1].ends_with("|0"), "{fingerprints:?}");
        assert!(fingerprints[2].ends_with("|1"), "{fingerprints:?}");
        assert!(fingerprints[0].ends_with("|2"), "{fingerprints:?}");
        // Shifting all of them down keeps each duplicate's identity.
        let shifted = seam_fingerprints(&[seam_at(900, 90), seam_at(700, 70), seam_at(800, 80)])?;
        assert_eq!(fingerprints, shifted);
        Ok(())
    }

    #[test]
    fn seam_ordinals_ignore_neighbour_class_and_visibility() -> Result<(), String> {
        let both = seam_fingerprints(&[
            weak_seam("m::f", SeamKind::CallPresence, (10, 1), "log(x)"),
            weak_seam("m::f", SeamKind::CallPresence, (20, 2), "log(x)"),
        ])?;
        assert_eq!(both.len(), 2);
        // The first duplicate is reclassified to a class the default policy
        // hides: the remaining sibling keeps ordinal 1, not 0.
        let hidden = classified_seam_at(
            SeamGripClass::StronglyGripped,
            "src/pricing.rs",
            "m::f",
            SeamKind::CallPresence,
            (10, 1),
            "log(x)",
        );
        let sibling = weak_seam("m::f", SeamKind::CallPresence, (20, 2), "log(x)");
        let visible = seam_fingerprints(&[hidden, sibling])?;
        assert_eq!(visible.len(), 1, "default policy hides strongly_gripped");
        assert_eq!(visible[0], both[1]);
        Ok(())
    }

    fn weakly_gripped_classified() -> ClassifiedSeam {
        classified_seam(SeamGripClass::WeaklyGripped)
    }

    fn strongly_gripped_classified() -> ClassifiedSeam {
        classified_seam(SeamGripClass::StronglyGripped)
    }

    fn classified_seam(class: SeamGripClass) -> ClassifiedSeam {
        classified_seam_at(
            class,
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            (42, 88),
            "amount >= discount_threshold",
        )
    }

    /// A seam whose identity inputs (file, owner, kind, expression) and
    /// position (`(byte_offset, display_line)`) are explicit.
    fn classified_seam_at(
        class: SeamGripClass,
        file: &str,
        owner: &str,
        kind: SeamKind,
        position: (usize, usize),
        expression: &str,
    ) -> ClassifiedSeam {
        let seam = RepoSeam::new(
            file,
            owner,
            kind,
            position.0,
            position.1,
            expression,
            RequiredDiscriminator::BoundaryValue {
                description: "amount == discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let evidence = TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: vec![std::sync::Arc::new(RelatedTestGrip {
                test_name: "below_threshold_has_no_discount".to_string(),
                file: PathBuf::from("tests/pricing.rs"),
                line: 12,
                test_target: Some(
                    crate::analysis::test_grip_evidence::TestTargetEvidence::fixture(
                        "below_threshold_has_no_discount",
                        std::path::Path::new("tests/pricing.rs"),
                        12,
                    ),
                ),
                oracle_kind: OracleKind::ExactValue,
                oracle_strength: OracleStrength::Strong,
                evidence_summary: "exact value assertion".to_string(),
                relation_reason: RelationReason::DirectOwnerCall,
                relation_confidence: RelationConfidence::High,
            })],
            reach: stage(StageState::Yes, "related test calls owner"),
            activate: stage(StageState::Yes, "amount value observed"),
            propagate: stage(StageState::Yes, "return value sink reached"),
            observe: stage(StageState::Yes, "returned value asserted"),
            discriminate: stage(StageState::Weak, "equality boundary absent"),
            observed_values: vec![
                ValueFact {
                    line: 12,
                    text: "discounted_total(50, 100)".to_string(),
                    value: "amount = 50".to_string(),
                    context: ValueContext::FunctionArgument,
                }
                .into(),
            ],
            missing_discriminators: vec![MissingDiscriminatorFact {
                value: "amount == discount_threshold".to_string(),
                reason: "observed values do not include equality boundary".to_string(),
                flow_sink: None,
            }],
            new_test_target: None,
            statically_contradicted_related_tests: 0,
        };
        ClassifiedSeam {
            seam,
            evidence,
            class,
        }
    }

    #[test]
    fn physical_location_omits_region_for_unlocated_finding() {
        // #2407: a finding with line == 0 (unlocated probe) must not fabricate
        // startLine:1. The region should be omitted entirely so SARIF viewers
        // don't point at the wrong line.
        let loc = physical_location("src/lib.rs", 0, None);
        assert!(
            loc["physicalLocation"]["region"].is_null(),
            "line-0 finding must omit region, got: {loc}"
        );
        assert_eq!(
            loc["physicalLocation"]["artifactLocation"]["uri"],
            "src/lib.rs"
        );
    }

    #[test]
    fn physical_location_includes_region_for_located_finding() {
        let loc = physical_location("src/lib.rs", 42, None);
        assert_eq!(loc["physicalLocation"]["region"]["startLine"], 42);
    }

    #[test]
    fn physical_location_uri_is_percent_encoded_reference() {
        // A raw `#` splits the path into a fragment, a space and a bare `%`
        // are not legal in a URI; each must arrive percent-encoded.
        let loc = physical_location("src/we ird#dir/a%b,c:d?e.rs", 3, None);
        assert_eq!(
            loc["physicalLocation"]["artifactLocation"]["uri"],
            "src/we%20ird%23dir/a%25b%2Cc%3Ad%3Fe.rs"
        );
        let plain = physical_location("crates/ripr-x/src/lib_2.rs", 3, None);
        assert_eq!(
            plain["physicalLocation"]["artifactLocation"]["uri"],
            "crates/ripr-x/src/lib_2.rs"
        );
        let unicode = physical_location("src/dé.rs", 3, None);
        assert_eq!(
            unicode["physicalLocation"]["artifactLocation"]["uri"],
            "src/d%C3%A9.rs"
        );
    }

    #[test]
    fn finding_fingerprint_hashes_unencoded_path_while_uri_is_encoded() {
        // Encoding the uri must not move alert identity: fingerprints keep
        // the raw repository path so alerts dedupe across the change.
        let mut finding = sample_finding();
        finding.probe.location.file = std::path::PathBuf::from("src/we ird#dir/a.rs");
        let mut output = sample_output();
        output.findings = vec![finding];
        let sarif = render_findings_sarif(&output, &crate::config::RiprConfig::default(), &[]);
        let parsed: serde_json::Value = serde_json::from_str(&sarif)
            .map_err(|error| error.to_string())
            .unwrap_or_default();
        let result = &parsed["runs"][0]["results"][0];
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "src/we%20ird%23dir/a.rs"
        );
        let fingerprint = result["fingerprints"]["riprFingerprintV1"]
            .as_str()
            .unwrap_or_default();
        assert!(
            fingerprint.contains("|src/we ird#dir/a.rs|"),
            "fingerprint must hash the unencoded path: {fingerprint}"
        );
        assert_eq!(
            result["partialFingerprints"]["riprFingerprintV1"],
            result["fingerprints"]["riprFingerprintV1"]
        );
    }

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    #[test]
    fn base_deleted_findings_emit_no_sarif_results() {
        // RIPR-SPEC-0152: SARIF results are current CI diagnostics;
        // base-side evidence is not one.
        let mut deleted = sample_finding();
        deleted.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        let mut deleted_output = sample_output();
        deleted_output.findings = vec![deleted];
        let sarif =
            render_findings_sarif(&deleted_output, &crate::config::RiprConfig::default(), &[]);
        let parsed: serde_json::Value = serde_json::from_str(&sarif)
            .map_err(|error| error.to_string())
            .unwrap_or_default();
        assert_eq!(
            parsed["runs"][0]["results"].as_array().map(Vec::len),
            Some(0),
            "base-deleted finding must not become a SARIF result"
        );
        assert_eq!(
            parsed["runs"][0]["properties"]["omitted_not_current"], 1,
            "the omitted finding must still be counted (#5091): {parsed}"
        );

        let mut current = sample_finding();
        current.source_currentness = crate::domain::SourceCurrentness::CandidateCurrent;
        let mut current_output = sample_output();
        current_output.findings = vec![current];
        let sarif =
            render_findings_sarif(&current_output, &crate::config::RiprConfig::default(), &[]);
        let parsed: serde_json::Value = serde_json::from_str(&sarif)
            .map_err(|error| error.to_string())
            .unwrap_or_default();
        assert_eq!(
            parsed["runs"][0]["results"].as_array().map(Vec::len),
            Some(1),
            "candidate-current twin keeps its SARIF result"
        );
        assert!(
            parsed["runs"][0]["properties"]
                .get("omitted_not_current")
                .is_none(),
            "a fully current run must not grow omitted_not_current: {parsed}"
        );
    }

    /// #5091: an all-not-current complete run still looks finished, but the
    /// run properties must say how many findings the filter withheld.
    #[test]
    fn sarif_run_with_only_not_current_findings_records_omitted_property() -> Result<(), String> {
        let kinds = [
            crate::domain::SourceCurrentness::BaseDeleted,
            crate::domain::SourceCurrentness::MovedOrRenamed,
            crate::domain::SourceCurrentness::UnresolvedSubject,
        ];
        let mut findings = Vec::new();
        for (index, currentness) in kinds.into_iter().enumerate() {
            let mut finding = sample_finding();
            finding.id = format!("finding:not-current-{index}");
            finding.source_currentness = currentness;
            findings.push(finding);
        }
        let mut output = sample_output();
        output.findings = findings;
        output.analysis_outcome = Some(complete_with_findings_outcome(3)?);
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        let props = &sarif["runs"][0]["properties"];
        assert_eq!(
            sarif["runs"][0]["results"].as_array().map(Vec::len),
            Some(0),
            "not-current findings stay out of results: {sarif}"
        );
        assert_eq!(props["run_status"], "complete");
        assert_eq!(
            props["omitted_not_current"], 3,
            "every not-current kind must count: {props}"
        );
        Ok(())
    }

    /// Empty analysis is not "N omitted". A zero-finding complete run must
    /// not grow the property, or code-scanning consumers cannot tell a
    /// clean run from an all-dropped run.
    #[test]
    fn sarif_empty_findings_run_omits_omitted_not_current_property() -> Result<(), String> {
        let mut output = sample_output();
        output.findings.clear();
        output.analysis_outcome = Some(complete_no_findings_outcome()?);
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        let props = &sarif["runs"][0]["properties"];
        assert_eq!(
            sarif["runs"][0]["results"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(props["run_status"], "complete");
        assert!(
            props.get("omitted_not_current").is_none(),
            "empty findings must not emit omitted_not_current: {props}"
        );
        Ok(())
    }

    /// A mixed run must keep current results *and* count the omitted ones.
    /// Emitting the property only when `results` is empty would hide the
    /// same filter on ordinary PRs.
    #[test]
    fn sarif_mixed_current_and_not_current_records_omitted_count_beside_results()
    -> Result<(), String> {
        let mut current = sample_finding();
        current.id = "finding:current".to_string();
        current.source_currentness = crate::domain::SourceCurrentness::CandidateCurrent;
        let mut omitted = sample_finding();
        omitted.id = "finding:base-deleted".to_string();
        omitted.source_currentness = crate::domain::SourceCurrentness::BaseDeleted;
        let mut output = sample_output();
        output.findings = vec![current, omitted];
        output.analysis_outcome = Some(complete_with_findings_outcome(2)?);
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        let props = &sarif["runs"][0]["properties"];
        assert_eq!(
            sarif["runs"][0]["results"].as_array().map(Vec::len),
            Some(1),
            "the current finding remains a result: {sarif}"
        );
        assert_eq!(props["run_status"], "complete");
        assert_eq!(
            props["omitted_not_current"], 1,
            "the not-current sibling still counts: {props}"
        );
        Ok(())
    }

    fn sibling_probe(id: &str, family: ProbeFamily) -> Finding {
        let mut finding = sample_finding();
        finding.id = format!("finding:{id}");
        finding.probe.id = ProbeId(format!("probe:src/pricing.rs:88:{id}"));
        finding.probe.family = family;
        finding
    }

    #[test]
    fn same_line_same_class_findings_fold_into_one_alert_that_names_the_rest() -> Result<(), String>
    {
        // #11219: an error_path and a return_value probe on one line are one
        // edit to the author, so one code-scanning alert.
        let mut output = sample_output();
        output.findings = vec![
            sibling_probe("first", ProbeFamily::ErrorPath),
            sibling_probe("second", ProbeFamily::ReturnValue),
        ];
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        let results = results(&sarif)?;
        assert_eq!(results.len(), 1, "{sarif}");
        assert_eq!(results[0]["properties"]["finding_id"], "finding:first");
        assert_eq!(
            results[0]["partialFingerprints"]["riprFingerprintV1"],
            "ripr.finding.weakly_exposed|finding:first|probe:src/pricing.rs:88:first|src/pricing.rs|88",
            "the kept result keeps its own fingerprint"
        );
        let folded = &results[0]["properties"]["folded_results"];
        assert_eq!(folded.as_array().map(Vec::len), Some(1), "{folded}");
        assert_eq!(folded[0]["finding_id"], "finding:second");
        assert_eq!(folded[0]["probe_family"], "return_value");
        assert_eq!(
            sarif["runs"][0]["properties"]["folded_same_line_results"], 1,
            "the run discloses the fold"
        );
        Ok(())
    }

    #[test]
    fn the_kept_alert_does_not_depend_on_input_order() -> Result<(), String> {
        let render = |order: [&str; 2]| -> Result<Value, String> {
            let mut output = sample_output();
            output.findings = order
                .iter()
                .map(|id| sibling_probe(id, ProbeFamily::ErrorPath))
                .collect();
            parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))
        };
        let forward = render(["alpha", "beta"])?;
        let reversed = render(["beta", "alpha"])?;
        assert_eq!(
            forward["runs"][0]["results"],
            reversed["runs"][0]["results"]
        );
        let kept = first_result(&forward)?;
        assert_eq!(kept["properties"]["finding_id"], "finding:alpha");
        assert_eq!(
            kept["properties"]["folded_results"][0]["finding_id"],
            "finding:beta"
        );
        Ok(())
    }

    #[test]
    fn a_suppressed_finding_is_not_folded_into_an_active_one() -> Result<(), String> {
        let suppression = SuppressionEntry {
            kind: SuppressionKind::ExposureGap,
            finding_id: Some("finding:first".to_string()),
            test: None,
            path: None,
            reason: "tracked".to_string(),
            owner: "team-ripr".to_string(),
            expires: Some("2099-01-01".to_string()),
            scope: None,
            created_at: None,
            last_seen: None,
            review_by: None,
            expected_visibility: None,
            static_class: None,
            language: None,
            language_status: None,
            block_line: 4,
        };
        let mut output = sample_output();
        output.findings = vec![
            sibling_probe("first", ProbeFamily::ErrorPath),
            sibling_probe("second", ProbeFamily::ReturnValue),
        ];
        let sarif = parse_json(&render_findings_sarif(
            &output,
            &RiprConfig::default(),
            &[suppression],
        ))?;
        let results = results(&sarif)?;
        assert_eq!(results.len(), 2, "{sarif}");
        assert_eq!(
            results
                .iter()
                .filter(|result| result.get("suppressions").is_some())
                .count(),
            1
        );
        Ok(())
    }

    #[test]
    fn findings_without_a_line_are_never_folded() -> Result<(), String> {
        let mut first = sibling_probe("first", ProbeFamily::ErrorPath);
        first.probe.location = SourceLocation::new("src/pricing.rs", 0, 0);
        let mut second = sibling_probe("second", ProbeFamily::ReturnValue);
        second.probe.location = SourceLocation::new("src/pricing.rs", 0, 0);
        let mut output = sample_output();
        output.findings = vec![first, second];
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        assert_eq!(results(&sarif)?.len(), 2, "{sarif}");
        Ok(())
    }

    #[test]
    fn findings_that_differ_in_class_or_line_stay_separate() -> Result<(), String> {
        let mut other_class = sibling_probe("class", ProbeFamily::ReturnValue);
        other_class.class = ExposureClass::InfectionUnknown;
        let mut other_line = sibling_probe("line", ProbeFamily::ReturnValue);
        other_line.probe.location = SourceLocation::new("src/pricing.rs", 89, 9);
        let mut output = sample_output();
        output.findings = vec![
            sibling_probe("first", ProbeFamily::ErrorPath),
            other_class,
            other_line,
        ];
        let sarif = parse_json(&render_findings_sarif(&output, &RiprConfig::default(), &[]))?;
        assert_eq!(results(&sarif)?.len(), 3, "{sarif}");
        assert!(
            sarif["runs"][0]["properties"]
                .get("folded_same_line_results")
                .is_none(),
            "nothing folded, nothing disclosed"
        );
        assert!(
            results(&sarif)?[0]["properties"]
                .get("folded_results")
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn refusal_sarif_root_renders_through_display_path() -> Result<(), String> {
        // #9806: `properties.root` uses the check-JSON renderer, which escapes
        // `%` on every host, so the raw `display()` spelling fails here.
        let notice = crate::core_error::CoreError::base_unresolvable(
            "the base `deadbeef` does not resolve to a commit",
        )
        .refusal_notice();
        let rendered = render_refusal_sarif(&notice, std::path::Path::new("repo%41"), None);
        let sarif = parse_json(&rendered)?;
        let root = sarif
            .pointer("/runs/0/properties/root")
            .and_then(Value::as_str);
        if root != Some("repo%2541") {
            return Err(format!(
                "refusal SARIF root must use display_path spelling, got {root:?}"
            ));
        }
        Ok(())
    }
}
