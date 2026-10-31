use serde_json::{Value, json};

const SCHEMA_VERSION: &str = "0.1";
const BASELINE_KIND: &str = "gate_baseline";
const DEFAULT_REVIEW_REASON: &str = "initial adoption baseline";
const LIMITS_NOTE: &str = "Reviewed baseline debt ledger over static RIPR gate evidence; baselines are not suppressions and do not change gate policy by themselves.";
pub(crate) const DEFAULT_BASELINE_OUT: &str = ".ripr/gate-baseline.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BaselineCreateReport {
    created_at: String,
    source_report: String,
    mode: String,
    reviewed: bool,
    /// Repository/root identity carried by the source gate-decision report
    /// (issue #1964). Preserved so a baseline entry stays interpretable when
    /// the ledger moves; delta joins across roots are refused as stale.
    root: Option<String>,
    /// Producing-build identity carried by the source gate-decision report's
    /// `subject.analyzer_version` (issue #5474). Persisted so `baseline diff`
    /// can disclose analyzer-version identity discontinuity (issue #7421)
    /// instead of reading identities invalidated across an analyzer upgrade
    /// as resolved debt.
    subject_analyzer_version: Option<String>,
    entries: Vec<BaselineEntry>,
    skipped: BaselineCreateSkipped,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineEntry {
    identity: BaselineIdentity,
    path: Option<String>,
    line: Option<u64>,
    static_class: Option<String>,
    decision: String,
    severity: Option<String>,
    source: Option<String>,
    gate_reason: Option<String>,
    evidence: BaselineEntryEvidence,
    review: BaselineEntryReview,
    /// Copy of the report root on the entry itself, so one entry remains
    /// interpretable if the ledger is ever split or relocated (issue #1964).
    root: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineIdentity {
    canonical_gap_id: Option<String>,
    seam_id: Option<String>,
    source_id: Option<String>,
    id: Option<String>,
    dedupe_key: Option<String>,
    fallback: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineEntryEvidence {
    missing_discriminator: Option<String>,
    assertion_shape: Option<String>,
    recommended_test: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineEntryReview {
    reviewed: bool,
    owner: Option<String>,
    reason: String,
    created_at: Option<String>,
    review_after: Option<String>,
    source: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BaselineCreateSkipped {
    suppressed: usize,
    not_applicable: usize,
    malformed: usize,
    /// Gate decisions that carried only a `path:line:static_class` fallback
    /// identity. New baselines refuse fallback identity as primary authority
    /// (issue #1964); these decisions are counted here, never written.
    fallback_only: usize,
    other: usize,
}

/// Outcome of screening one gate decision for baseline creation (issue #1964).
enum BaselineEntryOutcome {
    /// Canonical or governed identity: written to the new baseline. Boxed:
    /// the entry is hundreds of bytes and the sibling variants are tiny.
    Entry(Box<BaselineEntry>),
    /// Only a `path:line:static_class` fallback: refused, counted, warned.
    FallbackOnly(String),
    /// No stable identity at all: malformed, counted, warned.
    Malformed,
}

pub(crate) fn baseline_create_report_from_gate_decision_json(
    source_report: &str,
    created_at: &str,
    json_text: &str,
) -> Result<BaselineCreateReport, String> {
    let value = serde_json::from_str::<Value>(json_text)
        .map_err(|err| format!("parse gate-decision JSON failed: {err}; regenerate the input with the command that wrote it, then rerun"))?;
    baseline_create_report_from_gate_decision_value(source_report, created_at, &value)
}

/// Build the baseline from a whole-repository `ripr check --format json`
/// artifact (issue #11206 item 3). The artifact is projected onto the same
/// per-decision shape `ripr gate evaluate` writes and then goes through the one
/// entry builder, so identity, review and skip semantics have a single owner.
///
/// Fail-closed: only a complete outcome over the empty-tree base is accepted
/// (a PR-scoped or incomplete artifact would silently baseline a slice), and
/// only gap classes are baselined; every other finding is counted as skipped,
/// never written.
pub(crate) fn baseline_create_report_from_check_json(
    source_report: &str,
    created_at: &str,
    json_text: &str,
) -> Result<BaselineCreateReport, String> {
    let value = serde_json::from_str::<Value>(json_text)
        .map_err(|err| format!("parse check JSON failed: {err}; run `ripr check --candidate-tree HEAD^{{tree}} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file"))?;
    let decisions = check_findings_as_decisions(&value)?;
    let projected = json!({
        "schema_version": SCHEMA_VERSION,
        "mode": WHOLE_REPO_CHECK_MODE,
        "root": value.get("root").cloned().unwrap_or(Value::Null),
        "subject": {},
        "decisions": decisions,
    });
    let mut report =
        baseline_create_report_from_gate_decision_value(source_report, created_at, &projected)?;
    report.warnings.insert(
        0,
        "baseline built from a whole-repository `ripr check` artifact, not a gate decision; it records existing static gaps only and is unreviewed until a person confirms it"
            .to_string(),
    );
    Ok(report)
}

/// Gap classes a whole-repository baseline records. Unknown and exposed
/// classes are not debt a gate would block on, so they are skipped (counted),
/// never baselined.
const WHOLE_REPO_GAP_CLASSES: [&str; 3] =
    ["weakly_exposed", "reachable_unrevealed", "no_static_path"];
const WHOLE_REPO_CHECK_MODE: &str = "whole_repo_check";
const EMPTY_TREE_IDS: [&str; 2] = [
    "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
    "6ef19b41225c5369f1c104d45d8d85efa9d058d53bc6434cd0f5d23e5dc71d12",
];

fn check_findings_as_decisions(check: &Value) -> Result<Vec<Value>, String> {
    let schema = check
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "check JSON missing schema_version; run `ripr check --candidate-tree HEAD^{tree} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file".to_string())?;
    if check.get("tool").and_then(Value::as_str) != Some("ripr") {
        return Err(format!(
            "input is not a `ripr check` JSON artifact (schema_version `{schema}`); run `ripr check --candidate-tree HEAD^{{tree}} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file"
        ));
    }
    let outcome = check.pointer("/analysis_outcome").ok_or_else(|| {
        "check JSON carries no analysis_outcome, so completeness is not established; run `ripr check --candidate-tree HEAD^{tree} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file".to_string()
    })?;
    let kind = outcome.pointer("/outcome/kind").and_then(Value::as_str);
    let complete = outcome.get("analysis_complete").and_then(Value::as_bool) == Some(true);
    if !complete
        || !matches!(
            kind,
            Some("complete_with_findings" | "complete_no_findings")
        )
    {
        return Err(format!(
            "check analysis outcome is `{}` (analysis_complete = {complete}), not a complete whole-repository run; a partial run would baseline only part of the repository; run `ripr check --candidate-tree HEAD^{{tree}} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file",
            kind.unwrap_or("unknown")
        ));
    }
    // A findings-budget-bounded document is a prefix, not the whole set (#5203,
    // #9010): the omitted lowest-priority gaps would later read as new debt.
    if crate::output::gate::discloses_limited_findings_bound(check)
        || crate::output::gate::discloses_bounded_analysis_document(check)
    {
        return Err(format!(
            "check artifact discloses a findings-budget-bounded document (a prefix of the findings, not the whole set), so a baseline from it would be incomplete; re-run with `{}=0` for the full set; run `ripr check --candidate-tree HEAD^{{tree}} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file",
            crate::output::json::CHECK_FINDINGS_BYTES_ENV
        ));
    }
    // A partial-scope run is the same incomplete denominator the gate refuses
    // as an input in every position the run state is disclosed.
    if crate::output::gate::discloses_limited_partial_scope(check) {
        return Err(
            "check artifact discloses a partial-scope run (limited_partial_scope), so a baseline from it would cover only part of the repository; run `ripr check --candidate-tree HEAD^{tree} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file"
        .to_string());
    }
    let base_tree = outcome
        .pointer("/outcome/identity/git_candidate_subject/base_tree")
        .and_then(Value::as_str);
    if !base_tree.is_some_and(|tree| EMPTY_TREE_IDS.contains(&tree)) {
        return Err(format!(
            "check artifact is not a whole-repository run (candidate base tree is {}); a diff-scoped artifact would baseline only the slice it saw; run `ripr check --candidate-tree HEAD^{{tree}} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file",
            base_tree.map_or_else(|| "absent".to_string(), |tree| format!("`{tree}`"))
        ));
    }
    let findings = check
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "check JSON missing findings array; run `ripr check --candidate-tree HEAD^{tree} --format json > check.json` (the candidate tree is compared with the empty tree) and pass that file".to_string())?;
    Ok(findings.iter().map(check_finding_as_decision).collect())
}

fn check_finding_as_decision(finding: &Value) -> Value {
    let class = string_field(finding.get("classification"));
    if !class
        .as_deref()
        .is_some_and(|class| WHOLE_REPO_GAP_CLASSES.contains(&class))
    {
        // Not baselined: the existing builder counts it under `other`.
        return json!({"decision": "not_a_gap_class"});
    }
    let path =
        string_field(finding.pointer("/probe/file")).map(|path| normalize_fallback_path(&path));
    json!({
        "decision": "advisory",
        "canonical_gap_id": finding.get("canonical_gap_id").cloned().unwrap_or(Value::Null),
        "static_class": class,
        "severity": finding.get("severity").cloned().unwrap_or(Value::Null),
        "source": "whole_repo_check",
        "placement": {
            "path": path,
            "line": finding.pointer("/probe/line").cloned().unwrap_or(Value::Null),
        },
        "evidence": {
            "missing_discriminator": finding
                .pointer("/missing_discriminators/0/value")
                .cloned()
                .unwrap_or(Value::Null),
        },
    })
}

fn baseline_create_report_from_gate_decision_value(
    source_report: &str,
    created_at: &str,
    value: &Value,
) -> Result<BaselineCreateReport, String> {
    let schema_version = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "gate-decision JSON missing schema_version; regenerate the input with the command that wrote it, then rerun".to_string())?;
    if schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported gate-decision schema_version `{schema_version}`; expected `{SCHEMA_VERSION}`; regenerate the input with the command that wrote it, then rerun"
        ));
    }

    let decisions = value
        .get("decisions")
        .and_then(Value::as_array)
        .ok_or_else(|| "gate-decision JSON missing decisions array; regenerate the input with the command that wrote it, then rerun".to_string())?;
    let mode = string_field(value.get("mode")).unwrap_or_else(|| "unknown".to_string());
    let root = string_field(value.get("root"));
    let subject_analyzer_version = string_field(value.pointer("/subject/analyzer_version"));
    let mut entries = Vec::new();
    let mut skipped = BaselineCreateSkipped::default();
    let mut warnings = Vec::new();
    if subject_analyzer_version.is_none() {
        // Fail-closed disclosure (issue #7421): a baseline without a recorded
        // producing build cannot later distinguish "debt fixed" from
        // "identity scheme changed across analyzer builds", so its absence is
        // loud at creation time instead of silent at diff time.
        let source_kind = if mode == WHOLE_REPO_CHECK_MODE {
            "source check artifact"
        } else {
            "source gate-decision report"
        };
        warnings.push(format!(
            "{source_kind} carries no subject.analyzer_version; the baseline records no producing-build identity, so later diffs cannot disclose analyzer-version identity discontinuity"
        ));
    }

    for decision in decisions {
        match decision.get("decision").and_then(Value::as_str) {
            Some("advisory" | "acknowledged" | "blocking") => {
                match baseline_entry_from_decision(
                    decision,
                    created_at,
                    source_report,
                    root.as_deref(),
                ) {
                    BaselineEntryOutcome::Entry(entry) => entries.push(*entry),
                    // REFUSAL (issue #1964): no new baseline entry is written
                    // with fallback identity as primary authority. The refusal
                    // is counted and loud, never silent.
                    BaselineEntryOutcome::FallbackOnly(fallback) => {
                        skipped.fallback_only += 1;
                        warnings.push(format!(
                            "refused baseline entry from fallback path/line/static_class identity `{fallback}` alone; supply a canonical or governed identity"
                        ));
                    }
                    BaselineEntryOutcome::Malformed => {
                        skipped.malformed += 1;
                        warnings.push(
                            "skipped gate decision without a stable identity or fallback"
                                .to_string(),
                        );
                    }
                }
            }
            Some("suppressed") => skipped.suppressed += 1,
            Some("not_applicable") => skipped.not_applicable += 1,
            Some(_) | None => skipped.other += 1,
        }
    }

    entries.sort_by(|left, right| {
        entry_sort_key(left)
            .cmp(&entry_sort_key(right))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
    });

    Ok(BaselineCreateReport {
        created_at: created_at.to_string(),
        source_report: source_report.to_string(),
        mode,
        reviewed: false,
        root,
        subject_analyzer_version,
        entries,
        skipped,
        warnings,
    })
}

pub(crate) fn render_baseline_create_json(report: &BaselineCreateReport) -> Result<String, String> {
    serde_json::to_string_pretty(&json!({
        "schema_version": SCHEMA_VERSION,
        "tool": "ripr",
        "kind": BASELINE_KIND,
        "created_at": report.created_at,
        "source_report": report.source_report,
        "mode": report.mode,
        "reviewed": report.reviewed,
        "root": report.root,
        "subject": {
            "analyzer_version": report.subject_analyzer_version,
        },
        "summary": {
            "entries": report.entries.len(),
            "included": report.entries.len(),
            "skipped": skipped_json(&report.skipped),
        },
        "entries": report.entries.iter().map(entry_json).collect::<Vec<_>>(),
        "warnings": report.warnings,
        "limits_note": LIMITS_NOTE,
    }))
    .map_err(|err| format!("failed to render gate baseline JSON: {err}; this is a ripr bug, report it with the command you ran"))
}

pub(crate) fn baseline_entry_count(report: &BaselineCreateReport) -> usize {
    report.entries.len()
}

pub(crate) fn baseline_warnings(report: &BaselineCreateReport) -> &[String] {
    &report.warnings
}

pub(crate) use crate::output::path::display_path;

fn baseline_entry_from_decision(
    value: &Value,
    created_at: &str,
    source_report: &str,
    root: Option<&str>,
) -> BaselineEntryOutcome {
    let path = string_field(value.pointer("/placement/path"));
    let line = value.pointer("/placement/line").and_then(Value::as_u64);
    let static_class = string_field(value.get("static_class"));
    let identity = BaselineIdentity {
        canonical_gap_id: canonical_gap_id_from_value(value),
        seam_id: string_field(value.get("seam_id")),
        source_id: string_field(value.get("source_id")),
        id: string_field(value.get("id")),
        dedupe_key: string_field(value.get("dedupe_key")),
        fallback: fallback_identity(path.as_deref(), line, static_class.as_deref()),
    };
    if !identity.has_stable_value() {
        return BaselineEntryOutcome::Malformed;
    }
    // Canonical identities first, then governed seam identity where permitted
    // (issue #1964). A `path:line:static_class` fallback alone is never
    // primary authority for a new baseline entry.
    if !identity.has_primary_authority() {
        return match identity.fallback {
            Some(fallback) => BaselineEntryOutcome::FallbackOnly(fallback),
            None => BaselineEntryOutcome::Malformed,
        };
    }

    BaselineEntryOutcome::Entry(Box::new(BaselineEntry {
        identity,
        path,
        line,
        static_class,
        decision: string_field(value.get("decision")).unwrap_or_else(|| "unknown".to_string()),
        severity: string_field(value.get("severity")),
        source: string_field(value.get("source")),
        gate_reason: string_field(value.get("gate_reason")),
        evidence: BaselineEntryEvidence {
            missing_discriminator: string_field(value.pointer("/evidence/missing_discriminator")),
            assertion_shape: string_field(value.pointer("/evidence/assertion_shape")),
            recommended_test: string_field(value.pointer("/evidence/recommended_test")),
        },
        review: BaselineEntryReview {
            reviewed: false,
            owner: None,
            reason: DEFAULT_REVIEW_REASON.to_string(),
            created_at: Some(created_at.to_string()),
            review_after: None,
            source: Some(source_report.to_string()),
        },
        root: root.map(ToOwned::to_owned),
    }))
}

/// Path half of the stored fallback identity: `\` becomes `/` on every host and
/// leading `./` is stripped; case is preserved. Shared by baseline create/
/// delta/update and the gate's legacy-match disclosure.
pub(crate) fn normalize_fallback_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

/// Stored-string contract for the legacy `path:line:static_class` baseline
/// fallback identity (#9666). This is deliberately NOT
/// [`crate::output::path::path_identity_key`]: that key is host-dependent
/// (`\` is a separator only on Windows; ASCII case folds on Windows/macOS),
/// while a baseline is a committed text artifact that must key identically on
/// every host that reads it. The stored string therefore always rewrites `\`
/// to `/`, strips leading `./`, and never case-folds. Switching to the shared
/// host key would silently change stored identities (a Windows-authored
/// baseline would stop matching on Linux), so any change needs a versioned
/// key with compatibility reads. This is the single owner for create, delta
/// and update.
pub(crate) fn fallback_identity(
    path: Option<&str>,
    line: Option<u64>,
    static_class: Option<&str>,
) -> Option<String> {
    match (path, line, static_class) {
        (Some(path), Some(line), Some(static_class)) => Some(format!(
            "{}:{line}:{static_class}",
            normalize_fallback_path(path)
        )),
        _ => None,
    }
}

impl BaselineIdentity {
    fn has_stable_value(&self) -> bool {
        self.has_primary_authority() || self.fallback.is_some()
    }

    /// Canonical gap id first, then governed seam identity where explicitly
    /// permitted (issue #1964). The legacy `path:line:static_class` fallback
    /// is compatibility evidence only, never primary authority.
    fn has_primary_authority(&self) -> bool {
        self.canonical_gap_id.is_some()
            || self.seam_id.is_some()
            || self.source_id.is_some()
            || self.id.is_some()
            || self.dedupe_key.is_some()
    }
}

fn entry_sort_key(entry: &BaselineEntry) -> String {
    entry
        .identity
        .canonical_gap_id
        .as_deref()
        .or(entry.identity.seam_id.as_deref())
        .or(entry.identity.source_id.as_deref())
        .or(entry.identity.id.as_deref())
        .or(entry.identity.dedupe_key.as_deref())
        .or(entry.identity.fallback.as_deref())
        .unwrap_or("")
        .to_string()
}

fn canonical_gap_id_from_value(value: &Value) -> Option<String> {
    string_field(value.get("canonical_gap_id"))
        .or_else(|| string_field(value.pointer("/identity/canonical_gap_id")))
        .or_else(|| string_field(value.pointer("/evidence_record/canonical_gap_id")))
}

fn entry_json(entry: &BaselineEntry) -> Value {
    json!({
        "identity": {
            "canonical_gap_id": entry.identity.canonical_gap_id,
            "seam_id": entry.identity.seam_id,
            "source_id": entry.identity.source_id,
            "id": entry.identity.id,
            "dedupe_key": entry.identity.dedupe_key,
            "fallback": entry.identity.fallback,
        },
        "path": entry.path,
        "line": entry.line,
        "static_class": entry.static_class,
        "root": entry.root,
        "decision": entry.decision,
        "severity": entry.severity,
        "source": entry.source,
        "gate_reason": entry.gate_reason,
        "evidence": {
            "missing_discriminator": entry.evidence.missing_discriminator,
            "assertion_shape": entry.evidence.assertion_shape,
            "recommended_test": entry.evidence.recommended_test,
        },
        "review": {
            "reviewed": entry.review.reviewed,
            "owner": entry.review.owner,
            "reason": entry.review.reason,
            "created_at": entry.review.created_at,
            "review_after": entry.review.review_after,
            "source": entry.review.source,
        }
    })
}

fn skipped_json(skipped: &BaselineCreateSkipped) -> Value {
    json!({
        "suppressed": skipped.suppressed,
        "not_applicable": skipped.not_applicable,
        "malformed": skipped.malformed,
        "fallback_only": skipped.fallback_only,
        "other": skipped.other,
    })
}

fn string_field(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{
        baseline_create_report_from_check_json, baseline_create_report_from_gate_decision_json,
        render_baseline_create_json,
    };
    use serde_json::{Value, json};

    const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

    fn finding(class: &str, canonical: Option<&str>, file: &str, line: u64) -> Value {
        json!({
            "id": format!("probe:{file}:{line}"),
            "canonical_gap_id": canonical,
            "classification": class,
            "severity": "warning",
            "probe": {"file": file, "line": line},
            "missing_discriminators": [{"value": "n == 10"}],
        })
    }

    fn check_artifact(
        kind: &str,
        complete: bool,
        base_tree: Option<&str>,
        findings: Vec<Value>,
    ) -> String {
        json!({
            "schema_version": "0.2",
            "tool": "ripr",
            "root": ".",
            "analysis_outcome": {
                "analysis_complete": complete,
                "outcome": {
                    "kind": kind,
                    "identity": {"git_candidate_subject": {"base_tree": base_tree}},
                },
            },
            "findings": findings,
        })
        .to_string()
    }

    fn from_check(text: &str) -> Result<super::BaselineCreateReport, String> {
        baseline_create_report_from_check_json("check.json", "unix_ms:1", text)
    }

    #[test]
    fn whole_repo_check_baselines_only_gap_classes_with_canonical_identity() -> Result<(), String> {
        let text = check_artifact(
            "complete_with_findings",
            true,
            Some(EMPTY_TREE),
            vec![
                finding(
                    "weakly_exposed",
                    Some("gap:rust:src/a.rs:a:p:x"),
                    "./src/a.rs",
                    3,
                ),
                finding(
                    "reachable_unrevealed",
                    Some("gap:rust:src/b.rs:b:p:y"),
                    "src/b.rs",
                    9,
                ),
                finding(
                    "no_static_path",
                    Some("gap:rust:src/c.rs:c:p:z"),
                    "src/c.rs",
                    1,
                ),
                finding("exposed", Some("gap:rust:src/d.rs:d:p:w"), "src/d.rs", 2),
                finding(
                    "static_unknown",
                    Some("gap:rust:src/e.rs:e:p:v"),
                    "src/e.rs",
                    2,
                ),
                finding("weakly_exposed", None, "src/f.rs", 4),
            ],
        );
        let report = from_check(&text)?;
        let rendered: Value = serde_json::from_str(&render_baseline_create_json(&report)?)
            .map_err(|err| err.to_string())?;
        assert_eq!(rendered["kind"], "gate_baseline");
        assert_eq!(rendered["mode"], "whole_repo_check");
        assert_eq!(rendered["reviewed"], false);
        assert_eq!(rendered["summary"]["entries"], 3);
        // exposed + static_unknown are counted, the canonical-less gap is a
        // refused fallback-only identity; none is written.
        assert_eq!(rendered["summary"]["skipped"]["other"], 2);
        assert_eq!(rendered["summary"]["skipped"]["fallback_only"], 1);
        let ids: Vec<&str> = rendered["entries"]
            .as_array()
            .ok_or("entries")?
            .iter()
            .filter_map(|entry| entry.pointer("/identity/canonical_gap_id")?.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "gap:rust:src/a.rs:a:p:x",
                "gap:rust:src/b.rs:b:p:y",
                "gap:rust:src/c.rs:c:p:z"
            ]
        );
        // The path is normalized like the gate-decision route's fallback.
        assert_eq!(rendered["entries"][0]["path"], "src/a.rs");
        assert_eq!(
            rendered["entries"][0]["evidence"]["missing_discriminator"],
            "n == 10"
        );
        assert!(
            rendered["warnings"][0]
                .as_str()
                .is_some_and(|text| text.contains("whole-repository `ripr check` artifact"))
        );
        assert!(
            rendered["warnings"][1]
                .as_str()
                .is_some_and(|text| text.starts_with("source check artifact carries no"))
        );
        Ok(())
    }

    #[test]
    fn whole_repo_check_entry_joins_on_the_same_identity_as_a_gate_decision() -> Result<(), String>
    {
        let canonical = "gap:rust:src/a.rs:a:p:x";
        let from_gate = baseline_create_report_from_gate_decision_json(
            "gate.json",
            "unix_ms:1",
            &json!({
                "schema_version": "0.1",
                "mode": "visible-only",
                "decisions": [{
                    "decision": "advisory",
                    "canonical_gap_id": canonical,
                    "static_class": "weakly_exposed",
                    "placement": {"path": "src/a.rs", "line": 3},
                }],
            })
            .to_string(),
        )?;
        let text = check_artifact(
            "complete_with_findings",
            true,
            Some(EMPTY_TREE),
            vec![finding("weakly_exposed", Some(canonical), "src/a.rs", 3)],
        );
        let from_check = from_check(&text)?;
        let identity = |report: &super::BaselineCreateReport| -> Result<Value, String> {
            let value: Value = serde_json::from_str(&render_baseline_create_json(report)?)
                .map_err(|err| err.to_string())?;
            Ok(value["entries"][0]["identity"].clone())
        };
        assert_eq!(identity(&from_check)?, identity(&from_gate)?);
        Ok(())
    }

    #[test]
    fn whole_repo_check_refuses_scoped_incomplete_or_foreign_artifacts() {
        let gap = vec![finding("weakly_exposed", Some("gap:a"), "src/a.rs", 1)];
        let diff_scoped = check_artifact(
            "complete_with_findings",
            true,
            Some("1f0f141e3f5501c293980d256a55df8bbda6b24c"),
            gap.clone(),
        );
        let no_subject = check_artifact("complete_with_findings", true, None, gap.clone());
        let partial = check_artifact(
            "partial_with_limitations",
            false,
            Some(EMPTY_TREE),
            gap.clone(),
        );
        let flag_false = check_artifact("complete_with_findings", false, Some(EMPTY_TREE), gap);
        for (name, text, needle) in [
            ("diff-scoped", diff_scoped, "not a whole-repository run"),
            ("no subject", no_subject, "not a whole-repository run"),
            ("partial", partial, "not a complete whole-repository run"),
            (
                "flag false",
                flag_false,
                "not a complete whole-repository run",
            ),
            (
                "gate decision",
                r#"{"schema_version":"0.1","decisions":[]}"#.to_string(),
                "not a `ripr check` JSON artifact",
            ),
            (
                "no outcome",
                r#"{"schema_version":"0.2","tool":"ripr","findings":[]}"#.to_string(),
                "completeness is not established",
            ),
            ("not json", "nope".to_string(), "parse check JSON failed"),
        ] {
            let error = from_check(&text).err().unwrap_or_default();
            assert!(error.contains(needle), "{name}: {error}");
            if name != "not json" {
                assert!(
                    error.contains("--candidate-tree HEAD^{tree}"),
                    "{name}: {error}"
                );
            }
        }
    }

    #[test]
    fn whole_repo_check_refuses_bounded_and_partial_scope_documents() -> Result<(), String> {
        let gap = vec![finding("weakly_exposed", Some("gap:a"), "src/a.rs", 1)];
        let base: Value = serde_json::from_str(&check_artifact(
            "complete_with_findings",
            true,
            Some(EMPTY_TREE),
            gap,
        ))
        .map_err(|err| err.to_string())?;
        let mut document_incomplete = base.clone();
        document_incomplete["analysis_outcome"]["document_complete"] = json!(false);
        document_incomplete["analysis_outcome"]["findings_rendered"] = json!(1);
        document_incomplete["analysis_outcome"]["findings_total"] = json!(900);
        let mut limited_run = base.clone();
        limited_run["run_limitations"] = json!([
            {"run_status": crate::output::json::FINDINGS_BOUND_RUN_STATUS}
        ]);
        for (name, doc) in [
            ("document_complete:false", document_incomplete),
            ("run_limitations", limited_run),
        ] {
            let error = from_check(&doc.to_string()).err().unwrap_or_default();
            assert!(error.contains("findings-budget-bounded"), "{name}: {error}");
            assert!(
                error.contains("RIPR_CHECK_FINDINGS_BYTES=0"),
                "{name}: {error}"
            );
        }
        let mut partial_scope = base.clone();
        partial_scope["analysis_scope"] = json!({"run_status": "limited_partial_scope"});
        let error = from_check(&partial_scope.to_string())
            .err()
            .unwrap_or_default();
        assert!(error.contains("partial-scope run"), "{error}");
        let mut partial_language = base.clone();
        partial_language["language_runs"] = json!([{"language": "rust", "status": "partial"}]);
        let error = from_check(&partial_language.to_string())
            .err()
            .unwrap_or_default();
        assert!(error.contains("partial-scope run"), "{error}");
        // The same artifact without the bound is accepted.
        assert!(
            from_check(&base.to_string())
                .is_ok_and(|report| super::baseline_entry_count(&report) == 1)
        );
        Ok(())
    }

    #[test]
    fn whole_repo_check_without_findings_writes_an_empty_baseline() -> Result<(), String> {
        let text = check_artifact("complete_no_findings", true, Some(EMPTY_TREE), vec![]);
        let report = from_check(&text)?;
        assert_eq!(super::baseline_entry_count(&report), 0);
        // The sha256 empty tree is also a whole-repository base.
        let sha256 = check_artifact(
            "complete_no_findings",
            true,
            Some("6ef19b41225c5369f1c104d45d8d85efa9d058d53bc6434cd0f5d23e5dc71d12"),
            vec![],
        );
        assert_eq!(super::baseline_entry_count(&from_check(&sha256)?), 0);
        Ok(())
    }

    #[test]
    fn fallback_identity_is_a_host_independent_stored_string() {
        let id = |p| super::fallback_identity(Some(p), Some(7), Some("weakly_gripped"));
        let want = Some("src/a/B.rs:7:weakly_gripped".to_string());
        assert_eq!(id("src/a/B.rs"), want);
        // Linux discriminator: the host-dependent shared key leaves `\\` alone there.
        assert_eq!(id("src\\a\\B.rs"), want);
        assert_eq!(id("./src/a/B.rs"), want);
        // Guards Windows/macOS, where the shared key would fold case.
        // No case folding: stored identity stays case-exact on every host.
        assert_ne!(id("src/a/b.rs"), want);
        assert_eq!(
            super::fallback_identity(Some("a.rs"), None, Some("x")),
            None
        );
    }

    #[test]
    fn baseline_create_includes_visible_gate_decisions_and_skips_hidden_ones() -> Result<(), String>
    {
        let json = r#"{
          "schema_version": "0.1",
          "mode": "acknowledgeable",
          "decisions": [
            {
              "decision": "suppressed",
              "id": "ripr-gate-hidden",
              "seam_id": "hidden",
              "source_id": "hidden-source",
              "static_class": "weakly_gripped",
              "severity": "off",
              "placement": {"path": "src/lib.rs", "line": 1},
              "evidence": {"configured_off": true, "suppressed": true}
            },
            {
              "decision": "acknowledged",
              "id": "ripr-gate-bbb",
              "seam_id": "bbb",
              "source_id": "ripr-review-bbb",
              "static_class": "weakly_gripped",
              "severity": "warning",
              "source": "pr_guidance",
              "gate_reason": "policy-eligible gap acknowledged by ripr-waive",
              "placement": {"path": "src/b.rs", "line": 20},
              "evidence": {
                "missing_discriminator": "b == 2",
                "assertion_shape": "assert_eq!(b(), 2)",
                "recommended_test": "tests/b.rs::boundary"
              }
            },
            {
              "decision": "advisory",
              "id": "ripr-gate-aaa",
              "seam_id": "aaa",
              "source_id": "ripr-review-aaa",
              "static_class": "weakly_gripped",
              "severity": "warning",
              "source": "pr_guidance",
              "gate_reason": "visible-only mode records evidence without blocking",
              "placement": {"path": "src/a.rs", "line": 10},
              "evidence": {
                "missing_discriminator": "a == 1",
                "assertion_shape": "assert_eq!(a(), 1)",
                "recommended_test": "tests/a.rs::boundary"
              }
            }
          ]
        }"#;

        let report = baseline_create_report_from_gate_decision_json(
            "target/ripr/reports/gate-decision.json",
            "unix_ms:1",
            json,
        )?;
        let rendered = render_baseline_create_json(&report)?;
        assert!(rendered.contains("\"kind\": \"gate_baseline\""));
        assert!(rendered.contains("\"created_at\": \"unix_ms:1\""));
        assert!(rendered.contains("\"entries\": 2"));
        assert!(rendered.contains("\"suppressed\": 1"));
        assert!(rendered.contains("\"review_after\": null"));
        assert!(rendered.contains("\"source\": \"target/ripr/reports/gate-decision.json\""));
        assert!(rendered.contains("\"seam_id\": \"aaa\""));
        assert!(rendered.contains("\"seam_id\": \"bbb\""));
        assert!(rendered.find("\"aaa\"") < rendered.find("\"bbb\""));
        assert!(!rendered.contains("hidden-source"));
        Ok(())
    }

    #[test]
    fn baseline_create_uses_canonical_gap_identity_when_supplied() -> Result<(), String> {
        let json = r#"{
          "schema_version": "0.1",
          "mode": "visible-only",
          "decisions": [
            {
              "decision": "advisory",
              "id": "ripr-gate-old",
              "seam_id": "old",
              "source_id": "ripr-review-old",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/old.rs", "line": 7},
              "evidence_record": {
                "canonical_gap_id": "pricing::discount::threshold_equality"
              },
              "evidence": {"missing_discriminator": "amount == threshold"}
            }
          ]
        }"#;

        let report =
            baseline_create_report_from_gate_decision_json("gate.json", "unix_ms:1", json)?;
        let rendered = render_baseline_create_json(&report)?;
        assert!(
            rendered.contains("\"canonical_gap_id\": \"pricing::discount::threshold_equality\"")
        );
        Ok(())
    }

    #[test]
    fn baseline_create_refuses_fallback_only_identity_as_primary_authority() -> Result<(), String> {
        // Issue #1964: no new baseline entry is written with fallback identity
        // as primary authority. The refusal is counted and loud, never silent.
        let json = r#"{
          "schema_version": "0.1",
          "mode": "visible-only",
          "root": ".",
          "decisions": [
            {
              "decision": "advisory",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/pricing.rs", "line": 88},
              "evidence": {}
            }
          ]
        }"#;

        let report =
            baseline_create_report_from_gate_decision_json("gate.json", "unix_ms:2", json)?;
        let rendered = render_baseline_create_json(&report)?;
        assert!(rendered.contains("\"entries\": 0"));
        assert!(rendered.contains("\"fallback_only\": 1"));
        assert!(rendered.contains(
            "refused baseline entry from fallback path/line/static_class identity `src/pricing.rs:88:weakly_gripped` alone"
        ));
        assert!(!rendered.contains("\"fallback\": \"src/pricing.rs:88:weakly_gripped\""));
        Ok(())
    }

    #[test]
    fn baseline_create_preserves_root_identity_on_report_and_entries() -> Result<(), String> {
        // Issue #1964: repository/root identity travels with the new baseline
        // so each entry stays interpretable and cross-root fallback joins can
        // be refused downstream.
        let json = r#"{
          "schema_version": "0.1",
          "mode": "visible-only",
          "root": "/repo/acme",
          "decisions": [
            {
              "decision": "advisory",
              "id": "ripr-gate-rooted",
              "seam_id": "rooted",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/rooted.rs", "line": 3},
              "evidence": {}
            }
          ]
        }"#;

        let report =
            baseline_create_report_from_gate_decision_json("gate.json", "unix_ms:5", json)?;
        let rendered = render_baseline_create_json(&report)?;
        assert!(rendered.contains("\"entries\": 1"));
        assert!(rendered.contains("\"root\": \"/repo/acme\""));
        assert!(rendered.contains("\"fallback\": \"src/rooted.rs:3:weakly_gripped\""));
        Ok(())
    }

    #[test]
    fn baseline_create_reports_malformed_entries_without_stopping() -> Result<(), String> {
        let json = r#"{
          "schema_version": "0.1",
          "mode": "visible-only",
          "decisions": [
            {
              "decision": "advisory",
              "evidence": {}
            }
          ]
        }"#;

        let report =
            baseline_create_report_from_gate_decision_json("gate.json", "unix_ms:3", json)?;
        let rendered = render_baseline_create_json(&report)?;
        assert!(rendered.contains("\"entries\": 0"));
        assert!(rendered.contains("\"malformed\": 1"));
        assert!(rendered.contains("skipped gate decision without a stable identity or fallback"));
        Ok(())
    }

    #[test]
    fn baseline_create_persists_producing_analyzer_version_from_subject() -> Result<(), String> {
        // Issue #7421: the gate decision's `subject.analyzer_version`
        // (issue #5474) must travel into the baseline so later diffs can
        // disclose analyzer-version identity discontinuity.
        let json = r#"{
          "schema_version": "0.1",
          "mode": "visible-only",
          "root": ".",
          "subject": {
            "analyzer_version": "0.11.0+a079b4d61983f4f199cd1895af581a3495847093"
          },
          "decisions": [
            {
              "decision": "advisory",
              "id": "ripr-gate-versioned",
              "seam_id": "versioned",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/versioned.rs", "line": 4},
              "evidence": {}
            }
          ]
        }"#;

        let report =
            baseline_create_report_from_gate_decision_json("gate.json", "unix_ms:6", json)?;
        let rendered = render_baseline_create_json(&report)?;
        assert!(
            rendered.contains(
                "\"analyzer_version\": \"0.11.0+a079b4d61983f4f199cd1895af581a3495847093\""
            )
        );
        assert!(!rendered.contains("no subject.analyzer_version"));
        Ok(())
    }

    #[test]
    fn baseline_create_discloses_missing_producing_analyzer_version() -> Result<(), String> {
        // Issue #7421: a pre-#5474 gate decision carries no subject block;
        // the baseline records the absence as null and warns loudly, so the
        // missing producing-build identity is a creation-time disclosure, not
        // a silent diff-time blind spot.
        let json = r#"{
          "schema_version": "0.1",
          "mode": "visible-only",
          "decisions": [
            {
              "decision": "advisory",
              "id": "ripr-gate-unversioned",
              "seam_id": "unversioned",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/unversioned.rs", "line": 9},
              "evidence": {}
            }
          ]
        }"#;

        let report =
            baseline_create_report_from_gate_decision_json("gate.json", "unix_ms:7", json)?;
        let rendered = render_baseline_create_json(&report)?;
        assert!(rendered.contains("\"analyzer_version\": null"));
        assert!(rendered.contains(
            "source gate-decision report carries no subject.analyzer_version; the baseline records no producing-build identity, so later diffs cannot disclose analyzer-version identity discontinuity"
        ));
        Ok(())
    }

    #[test]
    fn baseline_create_rejects_unsupported_schema_version() {
        let err = baseline_create_report_from_gate_decision_json(
            "gate.json",
            "unix_ms:4",
            r#"{"schema_version":"9.9","decisions":[]}"#,
        );
        assert_eq!(
            err,
            Err("unsupported gate-decision schema_version `9.9`; expected `0.1`; regenerate the input with the command that wrote it, then rerun".to_string())
        );
    }
}
