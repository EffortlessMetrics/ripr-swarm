use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use crate::app::causal_projection::{CausalDeltaArtifact, insert_canonical_delta_fields};
use crate::output::gate::{GATE_DECISION_KNOWN_STATUSES, GATE_STATUS_CONFIG_ERROR};

const SCHEMA_VERSION: &str = "0.1";
pub(crate) const BASELINE_SCHEMA_VERSION: &str = SCHEMA_VERSION;
const REPORT_KIND: &str = "baseline_debt_delta";
const STATUS: &str = "advisory";
/// Identity authority recorded on a delta item whose current candidate matched
/// a reviewed baseline entry only through the legacy `path:line:static_class`
/// fallback (issue #1964, slice of #1934). Canonical matches never carry this
/// marker: the compatibility match must be impossible to mistake for a
/// canonical identity match.
pub(crate) const BASELINE_MATCH_KIND_LEGACY_PATH_LINE_CLASS: &str = "legacy_path_line_class";
const LIMITS_NOTE: &str = "Advisory baseline debt movement over static RIPR gate evidence; pass/fail remains owned by ripr gate evaluate.";
pub(crate) const DEFAULT_BASELINE_DELTA_OUT: &str = "target/ripr/reports/baseline-debt-delta.json";
pub(crate) const DEFAULT_BASELINE_DELTA_MD_OUT: &str = "target/ripr/reports/baseline-debt-delta.md";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BaselineDeltaInput {
    pub(crate) root: String,
    pub(crate) baseline_path: String,
    pub(crate) current_gate_decision_path: String,
    pub(crate) baseline_json: Result<String, String>,
    pub(crate) current_gate_decision_json: Result<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BaselineDeltaReport {
    root: String,
    inputs: BaselineDeltaInputs,
    baseline: BaselineSummary,
    delta: DeltaCounts,
    items: Vec<DeltaItem>,
    warnings: Vec<String>,
    causal_projection: Option<CausalDeltaArtifact>,
    /// The current side's run-state disclosure, propagated verbatim (#6257).
    /// `None` for complete runs, which carry no limitation envelope.
    run_limitations: Option<Value>,
    analysis_outcome: Option<Value>,
    analysis_scope: Option<Value>,
    /// The current gate decision's `status`, propagated verbatim (#6257
    /// review): a failed evaluation (`config_error`) has no decisions, so
    /// without this the delta presents an empty denominator as complete.
    current_gate_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineDeltaInputs {
    baseline: String,
    current_gate_decision: String,
    pr_guidance: Option<String>,
    agent_receipt: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineSummary {
    path: String,
    schema_version: Option<String>,
    entries: usize,
    valid: usize,
    stale: usize,
    invalid: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DeltaCounts {
    still_present: usize,
    resolved: usize,
    new_policy_eligible: usize,
    acknowledged: usize,
    suppressed: usize,
    stale_baseline_entry: usize,
    invalid_baseline_entry: usize,
    missing_current_input: usize,
    legacy_fallback_match: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeltaItem {
    bucket: Bucket,
    identity: Identity,
    path: Option<String>,
    line: Option<u64>,
    static_class: Option<String>,
    decision: Option<String>,
    reason: String,
    missing_discriminator: Option<String>,
    suggested_test: SuggestedTest,
    repair: Repair,
    review: Option<ReviewMetadata>,
    /// `Some(legacy_path_line_class)` when the current candidate matched the
    /// reviewed baseline entry only through the legacy fallback (issue #1964).
    /// `None` on every canonical match, so canonical items render exactly as
    /// before the disclosure existed.
    baseline_match_kind: Option<String>,
    /// True exactly when `baseline_match_kind` is the legacy fallback: the
    /// match is a reviewable compatibility event, never silent.
    stale_baseline_warning: bool,
    /// The legacy `path:line:static_class` identity that joined the match.
    matched_legacy_identity: Option<String>,
    /// The current candidate's canonical gap id when it carries one, retained
    /// so `ripr baseline update --migrate-legacy-identities` can replace the
    /// reviewed legacy identity deterministically.
    canonical_replacement_candidate: Option<String>,
    /// The two sides' producing-build identity when a baseline entry went
    /// unmatched across an analyzer-version boundary (issue #7421). `None`
    /// on every same-build item, so canonical renders stay byte-identical.
    analyzer_version_discontinuity: Option<AnalyzerVersionDiscontinuity>,
    /// `Some(reason)` when the baseline entry was absent from current
    /// evidence but its path was outside the current decision's disclosed
    /// analysis scope (issue #9301 F2): the entry is `not_evaluated`, never
    /// `resolved`. `None` on every in-scope item, so those render unchanged.
    not_evaluated: Option<String>,
}

/// The baseline and current sides' producing analyzer builds for one
/// identity-discontinuity item (issue #7421). `baseline`/`current` are
/// `None` when that side predates `subject.analyzer_version` recording
/// (issue #5474): exactly one recorded side already proves the builds
/// differed.
#[derive(Clone, Debug, Eq, PartialEq)]
struct AnalyzerVersionDiscontinuity {
    baseline: Option<String>,
    current: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ReviewMetadata {
    reviewed: Option<bool>,
    owner: Option<String>,
    reason: Option<String>,
    created_at: Option<String>,
    review_after: Option<String>,
    source: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SuggestedTest {
    recommended_test: Option<String>,
    assertion_shape: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Repair {
    action: String,
    verify_command: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Bucket {
    StillPresent,
    Resolved,
    NewPolicyEligible,
    Acknowledged,
    Suppressed,
    StaleBaselineEntry,
    InvalidBaselineEntry,
    MissingCurrentInput,
}

impl Bucket {
    fn as_str(self) -> &'static str {
        match self {
            Self::StillPresent => "still_present",
            Self::Resolved => "resolved",
            Self::NewPolicyEligible => "new_policy_eligible",
            Self::Acknowledged => "acknowledged",
            Self::Suppressed => "suppressed",
            Self::StaleBaselineEntry => "stale_baseline_entry",
            Self::InvalidBaselineEntry => "invalid_baseline_entry",
            Self::MissingCurrentInput => "missing_current_input",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::StillPresent => "Still present",
            Self::Resolved => "Resolved",
            Self::NewPolicyEligible => "New policy-eligible",
            Self::Acknowledged => "Acknowledged",
            Self::Suppressed => "Suppressed",
            Self::StaleBaselineEntry => "Stale baseline entry",
            Self::InvalidBaselineEntry => "Invalid baseline entry",
            Self::MissingCurrentInput => "Missing current input",
        }
    }

    fn order(self) -> u8 {
        match self {
            Self::StillPresent => 0,
            Self::Resolved => 1,
            Self::NewPolicyEligible => 2,
            Self::Acknowledged => 3,
            Self::Suppressed => 4,
            Self::StaleBaselineEntry => 5,
            Self::InvalidBaselineEntry => 6,
            Self::MissingCurrentInput => 7,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Identity {
    canonical_gap_id: Option<String>,
    seam_id: Option<String>,
    source_id: Option<String>,
    id: Option<String>,
    dedupe_key: Option<String>,
    fallback: Option<String>,
    matched_by: Option<String>,
}

impl Identity {
    fn has_stable_value(&self) -> bool {
        self.canonical_gap_id.is_some()
            || self.seam_id.is_some()
            || self.source_id.is_some()
            || self.id.is_some()
            || self.dedupe_key.is_some()
            || self.fallback.is_some()
    }

    fn sort_key(&self) -> String {
        match self
            .canonical_gap_id
            .as_deref()
            .or(self.seam_id.as_deref())
            .or(self.source_id.as_deref())
            .or(self.id.as_deref())
            .or(self.dedupe_key.as_deref())
            .or(self.fallback.as_deref())
        {
            Some(value) => value.to_string(),
            None => String::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineRecord {
    identity: Identity,
    path: Option<String>,
    line: Option<u64>,
    static_class: Option<String>,
    decision: Option<String>,
    evidence: Evidence,
    review: Option<ReviewMetadata>,
    /// Repository/root identity preserved when the baseline entry was created
    /// (issue #1964). `None` on entries written before root preservation: those
    /// stay comparable under the documented compatibility window.
    root: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CurrentDecision {
    identity: Identity,
    path: Option<String>,
    line: Option<u64>,
    static_class: Option<String>,
    decision: String,
    severity: Option<String>,
    gate_reason: Option<String>,
    evidence: Evidence,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Evidence {
    missing_discriminator: Option<String>,
    assertion_shape: Option<String>,
    recommended_test: Option<String>,
    suppressed: bool,
    configured_off: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BaselineParse {
    schema_version: Option<String>,
    /// Producing-build identity recorded by `baseline create` from the source
    /// gate decision's `subject.analyzer_version` (issue #7421). `None` on
    /// baselines written before build identity was persisted; those cannot
    /// prove same-build continuity and fail safe toward stale disclosure.
    subject_analyzer_version: Option<String>,
    entries: Vec<BaselineRecord>,
    invalid_items: Vec<DeltaItem>,
    warnings: Vec<String>,
    unavailable: bool,
    raw_entries: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CurrentParse {
    decisions: Vec<CurrentDecision>,
    warnings: Vec<String>,
    unavailable: bool,
    /// Top-level `root` of the current gate-decision report, used to refuse
    /// cross-repository fallback joins (issue #1964).
    root: Option<String>,
    /// The current side's `run_limitations[]`, propagated so a delta derived
    /// from a bounded run never presents as complete (#6257, mirroring the
    /// #5203 gap-ledger propagation). The zero-status guard reads the same
    /// position on the delta it reads on check output.
    run_limitations: Option<Value>,
    /// The current side's typed `analysis_outcome` envelope, propagated for
    /// the same reason: an incomplete producer outcome is an incomplete
    /// denominator, and the delta must not launder it (#6257).
    analysis_outcome: Option<Value>,
    /// The current side's `analysis_scope`, propagated for the same reason: a
    /// `limited_partial_scope` check run discloses its run state here, not in
    /// `run_limitations[]`, so dropping it would drop the disclosure (#6257).
    analysis_scope: Option<Value>,
    /// The current gate decision's `status` verbatim (#6257 review). The
    /// current side is gate-decision shaped, and the gate refuses limited
    /// inputs as `config_error` with empty decisions rather than carrying
    /// a limitation envelope, so the status is the production-live
    /// disclosure that a delta is built from a failed evaluation.
    current_gate_status: Option<String>,
    /// The current gate decision's producing-build identity
    /// (`subject.analyzer_version`, issue #5474). Joined against the
    /// baseline's recorded build so identity loss across an analyzer
    /// upgrade is disclosed, never read as resolved (issue #7421).
    subject_analyzer_version: Option<String>,
}

#[derive(Clone, Debug)]
struct CurrentIndexes {
    canonical_gap_id: BTreeMap<String, Vec<usize>>,
    seam_id: BTreeMap<String, Vec<usize>>,
    source_id: BTreeMap<String, Vec<usize>>,
    id: BTreeMap<String, Vec<usize>>,
    dedupe_key: BTreeMap<String, Vec<usize>>,
    fallback: BTreeMap<String, Vec<usize>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MatchResult {
    Match { index: usize, matched_by: String },
    Ambiguous { matched_by: String, count: usize },
    None,
}

pub(crate) fn build_baseline_delta_report(input: BaselineDeltaInput) -> BaselineDeltaReport {
    let baseline = parse_baseline_records(&input.baseline_path, input.baseline_json);
    let current = parse_current_decisions(
        &input.current_gate_decision_path,
        input.current_gate_decision_json,
    );
    let mut warnings = baseline.warnings.clone();
    warnings.extend(current.warnings.clone());
    let causal_projection = match CausalDeltaArtifact::load(std::path::Path::new(&input.root)) {
        Ok(projection) => projection,
        Err(error) => {
            warnings.push(format!(
                "causal comparison artifact is unavailable; causal fields omitted: {error}"
            ));
            None
        }
    };
    let absence_scope = PreparedAnalysisScope::new(current.analysis_scope.as_ref());
    let mut items = baseline.invalid_items.clone();
    let mut matched_current = BTreeSet::new();

    if baseline.unavailable || current.unavailable {
        push_missing_input_items(&baseline, &current, &mut items);
    } else {
        let indexes = build_current_indexes(&current.decisions);
        for baseline_record in &baseline.entries {
            match match_current_decision(&baseline_record.identity, &indexes) {
                MatchResult::Match { index, matched_by } if matched_current.contains(&index) => {
                    let item = stale_item(
                        baseline_record,
                        format!(
                            "Baseline identity also matched a current decision already joined by another baseline entry using {matched_by}."
                        ),
                    );
                    // Every legacy-flagged item has a matching warning (issue
                    // #1964, review): warning consumers must see the same
                    // compatibility events the counter reports.
                    if matched_by == "fallback" {
                        warnings.push(format!(
                            "baseline entry {} also matched an already joined current decision by fallback path/line/static_class; treated as stale, not historical",
                            baseline_record.identity.sort_key()
                        ));
                        items.push(with_legacy_disclosure(
                            item,
                            baseline_record.identity.fallback.clone(),
                            current.decisions[index].identity.canonical_gap_id.clone(),
                        ));
                    } else {
                        items.push(item);
                    }
                }
                MatchResult::Match { index, matched_by } => {
                    let current_decision = &current.decisions[index];
                    // CANONICAL DIVERGENCE (issue #1964): both sides carry a
                    // canonical gap id and they differ, so the fallback join
                    // is stale evidence, not a historical match. The baseline
                    // entry goes stale and the current decision stays
                    // unmatched, so the genuinely new canonical gap surfaces
                    // as new policy-eligible instead of looking historical.
                    if matched_by == "fallback"
                        && let Some(divergence) =
                            canonical_divergence(baseline_record, current_decision)
                    {
                        warnings.push(format!(
                            "baseline entry {} matched current evidence by fallback path/line/static_class but canonical identity diverged ({divergence}); treated as stale, not historical",
                            baseline_record.identity.sort_key()
                        ));
                        items.push(with_legacy_disclosure(
                            stale_item(
                                baseline_record,
                                format!(
                                    "Reviewed baseline identity matched current evidence only by legacy fallback, but the canonical gap id diverged ({divergence}); refresh the baseline identity instead of treating the current gap as historical."
                                ),
                            ),
                            baseline_record.identity.fallback.clone(),
                            current_decision.identity.canonical_gap_id.clone(),
                        ));
                    } else if matched_by == "fallback"
                        && let Some(roots) =
                            foreign_root_join(baseline_record, current.root.as_deref())
                    {
                        // FOREIGN ROOT (issue #1964): the reviewed entry was
                        // recorded for another repository. Consuming the
                        // current decision here would launder foreign history
                        // into this repo's delta, so both sides stay visible.
                        warnings.push(format!(
                            "baseline entry {} matched current evidence by fallback path/line/static_class but belongs to another repository root ({roots}); treated as stale, not historical",
                            baseline_record.identity.sort_key()
                        ));
                        items.push(with_legacy_disclosure(
                            stale_item(
                                baseline_record,
                                format!(
                                    "Reviewed baseline identity matched current evidence only by legacy fallback, but it belongs to another repository root ({roots}); the current gap is evaluated on this repository's own history."
                                ),
                            ),
                            baseline_record.identity.fallback.clone(),
                            current_decision.identity.canonical_gap_id.clone(),
                        ));
                    } else {
                        matched_current.insert(index);
                        if matched_by == "fallback" {
                            warnings.push(format!(
                                "baseline entry {} matched current evidence by fallback path/line/static_class",
                                baseline_record.identity.sort_key()
                            ));
                        }
                        items.push(matched_item(baseline_record, current_decision, matched_by));
                    }
                }
                MatchResult::Ambiguous { matched_by, count } => {
                    let item = stale_item(
                        baseline_record,
                        format!(
                            "Baseline identity matched {count} current decisions by {matched_by}; refresh or narrow the baseline identity."
                        ),
                    );
                    // An ambiguous fallback join names no single replacement
                    // candidate, but the legacy identity stays retained and
                    // visible (issue #1964). The stale warning replaces the
                    // generic preserved notice so one join emits one warning.
                    if matched_by == "fallback" {
                        warnings.push(format!(
                            "baseline entry {} matched {count} current decisions by fallback path/line/static_class; treated as stale, not historical",
                            baseline_record.identity.sort_key()
                        ));
                    }
                    items.push(if matched_by == "fallback" {
                        with_legacy_disclosure(
                            item,
                            baseline_record.identity.fallback.clone(),
                            None,
                        )
                    } else {
                        item
                    });
                }
                MatchResult::None => {
                    // IDENTITY DISCONTINUITY (issue #7421): an unmatched
                    // baseline entry across an analyzer-build boundary is
                    // disclosed as stale, never silently resolved. The join
                    // cannot distinguish "debt fixed" from "identity scheme
                    // changed across analyzer builds" (#7250 is exactly such
                    // a boundary), so the entry keeps the stale-bucket
                    // semantics of the existing fallback-divergence refusal
                    // and carries both sides' builds.
                    match analyzer_version_discontinuity(
                        baseline.subject_analyzer_version.as_deref(),
                        current.subject_analyzer_version.as_deref(),
                    ) {
                        Some(discontinuity) => {
                            warnings.push(format!(
                                "baseline entry {} is absent from current evidence but the producing analyzer build changed (baseline `{}` -> current `{}`); treated as stale identity discontinuity, not resolved debt",
                                baseline_record.identity.sort_key(),
                                disclosed_version(discontinuity.baseline.as_deref()),
                                disclosed_version(discontinuity.current.as_deref()),
                            ));
                            items.push(identity_discontinuity_item(baseline_record, discontinuity));
                        }
                        None => match absence_not_evaluated_reason(
                            &absence_scope,
                            baseline_record.path.as_deref(),
                        ) {
                            // ISSUE #9301 F2: absence outside the analyzed
                            // scope is not evidence of resolution.
                            Some(reason) => {
                                warnings.push(format!(
                                    "baseline entry {} is absent from current evidence but not evaluated: {reason}",
                                    baseline_record.identity.sort_key()
                                ));
                                items.push(not_evaluated_item(baseline_record, reason));
                            }
                            None => items.push(resolved_item(baseline_record)),
                        },
                    }
                }
            }
        }

        for (index, current_decision) in current.decisions.iter().enumerate() {
            if !matched_current.contains(&index) {
                items.push(unmatched_current_item(current_decision));
            }
        }
    }

    items.sort_by(|left, right| {
        left.bucket
            .order()
            .cmp(&right.bucket.order())
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.identity.sort_key().cmp(&right.identity.sort_key()))
    });

    drop(absence_scope);
    let delta = count_items(&items);
    BaselineDeltaReport {
        root: input.root,
        inputs: BaselineDeltaInputs {
            baseline: input.baseline_path.clone(),
            current_gate_decision: input.current_gate_decision_path,
            pr_guidance: None,
            agent_receipt: None,
        },
        baseline: BaselineSummary {
            path: input.baseline_path,
            schema_version: baseline.schema_version,
            entries: baseline.raw_entries,
            valid: baseline.entries.len(),
            stale: delta.stale_baseline_entry,
            invalid: delta.invalid_baseline_entry,
        },
        delta,
        items,
        warnings,
        causal_projection,
        run_limitations: current.run_limitations,
        analysis_outcome: current.analysis_outcome,
        analysis_scope: current.analysis_scope,
        current_gate_status: current.current_gate_status,
    }
}

pub(crate) fn render_baseline_delta_json(report: &BaselineDeltaReport) -> Result<String, String> {
    let mut output = json!({
        "schema_version": SCHEMA_VERSION,
        "tool": "ripr",
        "kind": REPORT_KIND,
        "status": STATUS,
        "root": report.root,
        "inputs": inputs_json(&report.inputs),
        "baseline": baseline_summary_json(&report.baseline),
        "delta": delta_json(&report.delta),
        "items": report
            .items
            .iter()
            .map(|item| item_json(item, report.causal_projection.as_ref()))
            .collect::<Vec<_>>(),
        "warnings": report.warnings,
        "limits_note": LIMITS_NOTE,
    });
    // #6257: additive run-state disclosure, present only when the current
    // side carried it. The limitation positions are the shared vocabulary
    // the zero-status partial-denominator guard (#5251/#6095) reads; the
    // gate status is the production-live disclosure, since gate decisions
    // carry no limitation envelope of their own.
    if let Some(object) = output.as_object_mut() {
        if let Some(outcome) = report.analysis_outcome.as_ref() {
            object.insert("analysis_outcome".to_string(), outcome.clone());
        }
        if let Some(scope) = report.analysis_scope.as_ref() {
            object.insert("analysis_scope".to_string(), scope.clone());
        }
        if let Some(limitations) = report.run_limitations.as_ref() {
            object.insert("run_limitations".to_string(), limitations.clone());
        }
        if let Some(status) = report.current_gate_status.as_ref() {
            object.insert(
                "current_gate_status".to_string(),
                Value::String(status.clone()),
            );
        }
    }
    if let Some(projection) = report.causal_projection.as_ref()
        && let Some(object) = output.as_object_mut()
    {
        projection.insert_comparison_fields(object);
    }
    serde_json::to_string_pretty(&output)
        .map_err(|err| format!("failed to render baseline debt delta JSON: {err}"))
}

pub(crate) fn render_baseline_delta_markdown(report: &BaselineDeltaReport) -> String {
    let mut out = String::new();
    out.push_str("# RIPR Baseline Debt Delta\n\n");
    out.push_str("Status: advisory\n");
    out.push_str(&format!("Baseline: {}\n", report.baseline.path));
    // #6257: the human surface discloses the propagated run state too, so a
    // delta from a limited current run never reads as complete. Absent for
    // complete runs, which carry no limitation envelope.
    if let Some(outcome) = &report.analysis_outcome {
        let complete = outcome
            .get("analysis_complete")
            .and_then(Value::as_bool)
            .map_or_else(|| "not_available".to_string(), |value| value.to_string());
        let kind = outcome
            .pointer("/outcome/kind")
            .and_then(Value::as_str)
            .unwrap_or("not_available");
        out.push_str(&format!(
            "Analysis complete: `{complete}`; analysis outcome: `{kind}`\n"
        ));
        // #9010: a budget-bounded check document says so beside the verdict.
        if outcome.get("document_complete").and_then(Value::as_bool) == Some(false) {
            out.push_str(
                "Document complete: `false` (findings array bounded by the byte budget)\n",
            );
        }
    }
    if let Some(scope) = report
        .analysis_scope
        .as_ref()
        .and_then(|scope| scope.get("run_status"))
        .and_then(Value::as_str)
    {
        out.push_str(&format!(
            "Current run scope (propagated from the current run): `{scope}`\n"
        ));
    }
    if let Some(limitations) = report.run_limitations.as_ref().and_then(Value::as_array) {
        let states = limitations
            .iter()
            .filter_map(|entry| {
                entry
                    .get("run_status")
                    .and_then(Value::as_str)
                    .or_else(|| entry.get("category").and_then(Value::as_str))
            })
            .collect::<Vec<_>>();
        if !states.is_empty() {
            out.push_str(&format!(
                "Run limitations (propagated from the current run): `{}`\n",
                states.join(", ")
            ));
        }
    }
    // The human surface names a failed current evaluation; successful
    // evaluations stay undisclosed (failure-only, unlike the JSON echo).
    if report.current_gate_status.as_deref() == Some(GATE_STATUS_CONFIG_ERROR) {
        out.push_str(
            "Current gate status: `config_error` (evaluation did not complete; counts are not a denominator)\n",
        );
    }
    out.push('\n');
    out.push_str("| Bucket | Count |\n");
    out.push_str("| --- | ---: |\n");
    for bucket in bucket_order() {
        out.push_str(&format!(
            "| {} | {} |\n",
            bucket.title(),
            count_for_bucket(&report.delta, bucket)
        ));
    }

    let new_items = report
        .items
        .iter()
        .filter(|item| item.bucket == Bucket::NewPolicyEligible)
        .take(10)
        .collect::<Vec<_>>();
    if !new_items.is_empty() {
        out.push_str("\nTop new policy-eligible gaps:\n");
        for item in new_items {
            out.push_str(&format!("- {}\n", item_headline(item)));
            if let Some(missing) = item.missing_discriminator.as_deref() {
                out.push_str(&format!("  Missing: {missing}\n"));
            }
            out.push_str("  Action: add a focused test or acknowledge visibly.\n");
        }
    }

    let resolved_items = report
        .items
        .iter()
        .filter(|item| item.bucket == Bucket::Resolved)
        .take(10)
        .collect::<Vec<_>>();
    if !resolved_items.is_empty() {
        out.push_str("\nResolved baseline entries:\n");
        for item in resolved_items {
            out.push_str(&format!("- {}\n", item_headline(item)));
        }
    }

    // Human-visible legacy disclosure (issue #1964): every fallback-only
    // match names the retained legacy identity and the canonical replacement
    // candidate, so the compatibility event cannot stay silent in Markdown.
    // Unlike the capped top-new/resolved lists, this section is exhaustive:
    // the contract promises every fallback-only match is visible in human
    // output, and the JSON items carry the same set for machine consumers.
    let legacy_items = report
        .items
        .iter()
        .filter(|item| item.stale_baseline_warning)
        .collect::<Vec<_>>();
    if !legacy_items.is_empty() {
        out.push_str(&format!(
            "\nLegacy fallback matches ({} total):\n",
            report.delta.legacy_fallback_match
        ));
        for item in legacy_items {
            out.push_str(&format!("- {}\n", item_headline(item)));
            if let Some(legacy) = item.matched_legacy_identity.as_deref() {
                out.push_str(&format!("  Legacy identity: {legacy}\n"));
            }
            match item.canonical_replacement_candidate.as_deref() {
                Some(replacement) => out.push_str(&format!(
                    "  Canonical replacement candidate: {replacement}\n"
                )),
                None => out.push_str(
                    "  Canonical replacement candidate: none; refresh or narrow the baseline identity.\n",
                ),
            }
            out.push_str(
                "  Action: review, then run `ripr baseline update --migrate-legacy-identities` or refresh the entry.\n",
            );
        }
    }

    // Human-visible identity-discontinuity disclosure (issue #7421): every
    // analyzer-version-crossing unmatched entry names both sides' builds, so
    // an upgrade cannot silently read identity loss as resolved improvement.
    // Exhaustive, like the legacy-fallback section: the JSON items carry the
    // same set for machine consumers.
    let discontinuity_items = report
        .items
        .iter()
        .filter(|item| item.analyzer_version_discontinuity.is_some())
        .collect::<Vec<_>>();
    if !discontinuity_items.is_empty() {
        out.push_str(&format!(
            "\nAnalyzer-version identity discontinuities ({} total):\n",
            discontinuity_items.len()
        ));
        for item in discontinuity_items {
            out.push_str(&format!("- {}\n", item_headline(item)));
            if let Some(discontinuity) = item.analyzer_version_discontinuity.as_ref() {
                out.push_str(&format!(
                    "  Baseline build: {}\n",
                    disclosed_version(discontinuity.baseline.as_deref())
                ));
                out.push_str(&format!(
                    "  Current build: {}\n",
                    disclosed_version(discontinuity.current.as_deref())
                ));
            }
            out.push_str(
                "  Action: re-create the baseline with the current build (`ripr baseline create --from <current gate-decision> --force`) or verify the gap is genuinely fixed before treating it as resolved.\n",
            );
        }
    }

    // Not-evaluated disclosure (issue #9301): entries absent from current
    // evidence whose file the current analysis did not cover. Exhaustive.
    let not_evaluated_items = report
        .items
        .iter()
        .filter(|item| item.not_evaluated.is_some())
        .collect::<Vec<_>>();
    if !not_evaluated_items.is_empty() {
        out.push_str(&format!(
            "\nNot evaluated by the current analysis ({} total):\n",
            not_evaluated_items.len()
        ));
        for item in not_evaluated_items {
            out.push_str(&format!("- {}\n", item_headline(item)));
            if let Some(reason) = item.not_evaluated.as_deref() {
                out.push_str(&format!("  Reason: {reason}\n"));
            }
            out.push_str(
                "  Action: run an analysis that covers this file before treating the entry as fixed; it is neither resolved nor removable.\n",
            );
        }
    }

    if !report.warnings.is_empty() {
        out.push_str("\nWarnings:\n");
        for warning in &report.warnings {
            out.push_str(&format!("- {warning}\n"));
        }
    }

    out.push_str("\nLimits: ");
    out.push_str(LIMITS_NOTE);
    out.push('\n');
    out
}

pub(crate) fn baseline_delta_item_count(report: &BaselineDeltaReport) -> usize {
    report.items.len()
}

/// Advisory warnings the report also carries in its JSON/Markdown bodies; the
/// CLI echoes them to stderr (#9856) so a run that routed from missing inputs
/// is visible in terminals and `2>&1` CI logs.
pub(crate) fn baseline_delta_warnings(report: &BaselineDeltaReport) -> &[String] {
    &report.warnings
}

fn parse_baseline_records(path: &str, json_text: Result<String, String>) -> BaselineParse {
    let value = match parse_required_json_input(path, "baseline", json_text) {
        Ok(value) => value,
        Err(warning) => {
            return BaselineParse {
                unavailable: true,
                warnings: vec![warning],
                ..BaselineParse::default()
            };
        }
    };
    let schema_version = string_field(value.get("schema_version"));
    if schema_version.as_deref() != Some(SCHEMA_VERSION) {
        return BaselineParse {
            schema_version,
            unavailable: true,
            warnings: vec![format!(
                "required baseline input {path} has unsupported schema_version; expected {SCHEMA_VERSION}"
            )],
            ..BaselineParse::default()
        };
    }

    let records = value
        .get("entries")
        .and_then(Value::as_array)
        .or_else(|| value.get("decisions").and_then(Value::as_array));
    let Some(records) = records else {
        return BaselineParse {
            schema_version,
            unavailable: true,
            warnings: vec![format!(
                "required baseline input {path} is missing entries or decisions array"
            )],
            ..BaselineParse::default()
        };
    };

    let mut entries = Vec::new();
    let mut invalid_items = Vec::new();
    for record in records {
        match baseline_record_from_value(record) {
            Some(entry) => entries.push(entry),
            None => invalid_items.push(invalid_baseline_item(record)),
        }
    }

    BaselineParse {
        schema_version,
        subject_analyzer_version: string_field(value.pointer("/subject/analyzer_version")),
        entries,
        invalid_items,
        warnings: Vec::new(),
        unavailable: false,
        raw_entries: records.len(),
    }
}

fn parse_current_decisions(path: &str, json_text: Result<String, String>) -> CurrentParse {
    let value = match parse_required_json_input(path, "current gate-decision", json_text) {
        Ok(value) => value,
        Err(warning) => {
            return CurrentParse {
                unavailable: true,
                warnings: vec![warning],
                ..CurrentParse::default()
            };
        }
    };
    if value.get("schema_version").and_then(Value::as_str) != Some(SCHEMA_VERSION) {
        return CurrentParse {
            unavailable: true,
            warnings: vec![format!(
                "required current gate-decision input {path} has unsupported schema_version; expected {SCHEMA_VERSION}"
            )],
            ..CurrentParse::default()
        };
    }
    let Some(decisions) = value.get("decisions").and_then(Value::as_array) else {
        return CurrentParse {
            unavailable: true,
            warnings: vec![format!(
                "required current gate-decision input {path} is missing decisions array"
            )],
            ..CurrentParse::default()
        };
    };

    // Dropping an unidentifiable decision would turn an incomplete current
    // denominator into apparent resolution of reviewed baseline debt.
    let Some(decisions) = decisions
        .iter()
        .map(current_decision_from_value)
        .collect::<Option<Vec<_>>>()
    else {
        return CurrentParse {
            unavailable: true,
            warnings: vec![format!(
                "required current gate-decision input {path} contains a decision without stable identity"
            )],
            ..CurrentParse::default()
        };
    };

    CurrentParse {
        decisions,
        warnings: Vec::new(),
        unavailable: false,
        root: string_field(value.get("root")),
        // #6257: propagate the producer's run-state disclosure verbatim, so a
        // delta built from a limited current side stays visibly limited. Only
        // presence propagates: complete runs carry none of these positions.
        run_limitations: value.get("run_limitations").cloned(),
        analysis_outcome: value.get("analysis_outcome").cloned(),
        analysis_scope: value.get("analysis_scope").cloned(),
        // #6257 review: a present-but-malformed gate status is rejected,
        // never silently discarded: without the propagated status, a failed
        // evaluation's empty decisions read as a complete denominator.
        // Absent (or explicit null) stays accepted for older inputs. This
        // deliberately bypasses string_field's blank-to-None mapping: a
        // disclosure carrier fails closed where an identity hint stays lenient.
        // A non-blank status outside the schema-closed set is likewise
        // rejected: only config_error withholds achieved downstream, so an
        // out-of-contract status would otherwise read as completed.
        current_gate_status: match value.get("status") {
            None => None,
            Some(status) if status.is_null() => None,
            Some(Value::String(text))
                if !text.trim().is_empty()
                    && GATE_DECISION_KNOWN_STATUSES.contains(&text.as_str()) =>
            {
                Some(text.clone())
            }
            Some(Value::String(text)) if !text.trim().is_empty() => {
                return CurrentParse {
                    unavailable: true,
                    warnings: vec![format!(
                        "required current gate-decision input {path} has unknown status `{text}`; status must be one of {}",
                        GATE_DECISION_KNOWN_STATUSES.join(", ")
                    )],
                    ..CurrentParse::default()
                };
            }
            Some(_) => {
                return CurrentParse {
                    unavailable: true,
                    warnings: vec![format!(
                        "required current gate-decision input {path} has malformed status; status must be a non-blank string"
                    )],
                    ..CurrentParse::default()
                };
            }
        },
        subject_analyzer_version: string_field(value.pointer("/subject/analyzer_version")),
    }
}

fn parse_required_json_input(
    path: &str,
    input_name: &str,
    json_text: Result<String, String>,
) -> Result<Value, String> {
    let text = json_text
        .map_err(|error| format!("required {input_name} input {path} is invalid: {error}"))?;
    serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("required {input_name} input {path} is invalid: {error}"))
}

fn baseline_record_from_value(value: &Value) -> Option<BaselineRecord> {
    let identity_value = match value.get("identity") {
        Some(identity) => identity,
        None => value,
    };
    let path =
        string_field(value.get("path")).or_else(|| string_field(value.pointer("/placement/path")));
    let line = value
        .get("line")
        .and_then(Value::as_u64)
        .or_else(|| value.pointer("/placement/line").and_then(Value::as_u64));
    let static_class = string_field(value.get("static_class"));
    let identity = Identity {
        canonical_gap_id: canonical_gap_id_from_value(value),
        seam_id: string_field(identity_value.get("seam_id"))
            .or_else(|| string_field(value.get("seam_id"))),
        source_id: string_field(identity_value.get("source_id"))
            .or_else(|| string_field(value.get("source_id"))),
        id: string_field(identity_value.get("id")).or_else(|| string_field(value.get("id"))),
        dedupe_key: string_field(identity_value.get("dedupe_key"))
            .or_else(|| string_field(value.get("dedupe_key"))),
        fallback: string_field(identity_value.get("fallback")).or_else(|| {
            crate::output::baseline::fallback_identity(
                path.as_deref(),
                line,
                static_class.as_deref(),
            )
        }),
        matched_by: None,
    };
    if !identity.has_stable_value() {
        return None;
    }

    Some(BaselineRecord {
        identity,
        path,
        line,
        static_class,
        decision: string_field(value.get("decision")),
        evidence: evidence_from_value(value),
        review: review_metadata_from_value(value.get("review")),
        root: string_field(value.get("root")),
    })
}

fn current_decision_from_value(value: &Value) -> Option<CurrentDecision> {
    let path = string_field(value.pointer("/placement/path"));
    let line = value.pointer("/placement/line").and_then(Value::as_u64);
    let static_class = string_field(value.get("static_class"));
    let identity = Identity {
        canonical_gap_id: canonical_gap_id_from_value(value),
        seam_id: string_field(value.get("seam_id")),
        source_id: string_field(value.get("source_id")),
        id: string_field(value.get("id")),
        dedupe_key: string_field(value.get("dedupe_key")),
        fallback: crate::output::baseline::fallback_identity(
            path.as_deref(),
            line,
            static_class.as_deref(),
        ),
        matched_by: None,
    };
    if !identity.has_stable_value() {
        return None;
    }

    Some(CurrentDecision {
        identity,
        path,
        line,
        static_class,
        decision: match string_field(value.get("decision")) {
            Some(decision) => decision,
            None => "unknown".to_string(),
        },
        severity: string_field(value.get("severity")),
        gate_reason: string_field(value.get("gate_reason")),
        evidence: evidence_from_value(value),
    })
}

fn evidence_from_value(value: &Value) -> Evidence {
    Evidence {
        missing_discriminator: string_field(value.pointer("/evidence/missing_discriminator")),
        assertion_shape: string_field(value.pointer("/evidence/assertion_shape")),
        recommended_test: string_field(value.pointer("/evidence/recommended_test")),
        suppressed: value
            .pointer("/evidence/suppressed")
            .and_then(Value::as_bool)
            .is_some_and(|value| value),
        configured_off: value
            .pointer("/evidence/configured_off")
            .and_then(Value::as_bool)
            .is_some_and(|value| value),
    }
}

fn build_current_indexes(decisions: &[CurrentDecision]) -> CurrentIndexes {
    let mut indexes = CurrentIndexes {
        canonical_gap_id: BTreeMap::new(),
        seam_id: BTreeMap::new(),
        source_id: BTreeMap::new(),
        id: BTreeMap::new(),
        dedupe_key: BTreeMap::new(),
        fallback: BTreeMap::new(),
    };
    for (index, decision) in decisions.iter().enumerate() {
        push_index(
            &mut indexes.canonical_gap_id,
            decision.identity.canonical_gap_id.as_ref(),
            index,
        );
        push_index(
            &mut indexes.seam_id,
            decision.identity.seam_id.as_ref(),
            index,
        );
        push_index(
            &mut indexes.source_id,
            decision.identity.source_id.as_ref(),
            index,
        );
        push_index(&mut indexes.id, decision.identity.id.as_ref(), index);
        push_index(
            &mut indexes.dedupe_key,
            decision.identity.dedupe_key.as_ref(),
            index,
        );
        push_index(
            &mut indexes.fallback,
            decision.identity.fallback.as_ref(),
            index,
        );
    }
    indexes
}

fn push_index(index: &mut BTreeMap<String, Vec<usize>>, key: Option<&String>, value: usize) {
    if let Some(key) = key {
        index.entry(key.clone()).or_default().push(value);
    }
}

fn match_current_decision(identity: &Identity, indexes: &CurrentIndexes) -> MatchResult {
    let mut first_ambiguity = None;
    for (method, key, index) in [
        (
            "canonical_gap_id",
            identity.canonical_gap_id.as_ref(),
            &indexes.canonical_gap_id,
        ),
        ("seam_id", identity.seam_id.as_ref(), &indexes.seam_id),
        ("source_id", identity.source_id.as_ref(), &indexes.source_id),
        ("id", identity.id.as_ref(), &indexes.id),
        (
            "dedupe_key",
            identity.dedupe_key.as_ref(),
            &indexes.dedupe_key,
        ),
        ("fallback", identity.fallback.as_ref(), &indexes.fallback),
    ] {
        if let Some(key) = key
            && let Some(matches) = index.get(key)
        {
            if matches.len() == 1 {
                return MatchResult::Match {
                    index: matches[0],
                    matched_by: method.to_string(),
                };
            }
            if first_ambiguity.is_none() {
                first_ambiguity = Some(MatchResult::Ambiguous {
                    matched_by: method.to_string(),
                    count: matches.len(),
                });
            }
        }
    }
    first_ambiguity.unwrap_or(MatchResult::None)
}

/// The `old-canonical -> new-canonical` pair when a fallback join hides a
/// canonical identity change (issue #1964). `None` unless both sides carry a
/// canonical gap id and they differ.
fn canonical_divergence(baseline: &BaselineRecord, current: &CurrentDecision) -> Option<String> {
    canonical_identity_divergence(
        baseline.identity.canonical_gap_id.as_deref(),
        current.identity.canonical_gap_id.as_deref(),
    )
}

/// The single owner of the fallback-join divergence rule, shared by the delta
/// and by the gate's baseline comparison so they cannot disagree (issue #1964).
pub(super) fn canonical_identity_divergence(
    baseline: Option<&str>,
    current: Option<&str>,
) -> Option<String> {
    match (baseline, current) {
        (Some(old), Some(new)) if old != new => Some(format!("{old} -> {new}")),
        _ => None,
    }
}

/// The `baseline-root -> current-root` pair when a fallback join crosses
/// repository boundaries (issue #1964). `None` when either side omits root
/// (pre-root-preservation baselines stay comparable under the compatibility
/// window) or when both roots agree.
fn foreign_root_join(baseline: &BaselineRecord, current_root: Option<&str>) -> Option<String> {
    match (baseline.root.as_deref(), current_root) {
        (Some(baseline_root), Some(current_root)) => {
            let normalized_baseline = normalize_root_for_comparison(baseline_root);
            let normalized_current = normalize_root_for_comparison(current_root);
            if normalized_baseline != normalized_current {
                Some(format!("{baseline_root} -> {current_root}"))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The two sides' producing-build pair when the baseline and the current
/// gate decision provably come from different analyzer builds (issue #7421).
/// Both sides recording the same build is continuity, and both sides
/// recording no build (pre-#5474 artifacts on both ends, the old-vs-old
/// control) carries no discontinuity evidence, so the legacy `resolved`
/// behavior holds. Exactly one recorded side already proves the builds
/// differed: the unrecorded side predates `subject.analyzer_version`
/// persistence, so absence fails safe toward stale disclosure.
fn analyzer_version_discontinuity(
    baseline: Option<&str>,
    current: Option<&str>,
) -> Option<AnalyzerVersionDiscontinuity> {
    match (baseline, current) {
        (Some(baseline), Some(current)) if baseline == current => None,
        (None, None) => None,
        (baseline, current) => Some(AnalyzerVersionDiscontinuity {
            baseline: baseline.map(ToOwned::to_owned),
            current: current.map(ToOwned::to_owned),
        }),
    }
}

fn disclosed_version(version: Option<&str>) -> String {
    version.map_or_else(|| "none recorded".to_string(), ToOwned::to_owned)
}

/// The stale item for one identity-discontinuity entry (issue #7421): the
/// reason names both sides' builds and the refresh route, and the typed
/// disclosure carries the two versions for machine consumers.
fn identity_discontinuity_item(
    baseline: &BaselineRecord,
    discontinuity: AnalyzerVersionDiscontinuity,
) -> DeltaItem {
    let mut item = stale_item(
        baseline,
        format!(
            "Reviewed baseline identity is absent from current gate-decision evidence, but the producing analyzer build changed (baseline `{}` -> current `{}`); the identity may have been invalidated by the upgrade rather than the debt resolved. Re-create the baseline with the current build (`ripr baseline create --from <current gate-decision> --force`) or verify the gap is genuinely fixed before treating it as resolved.",
            disclosed_version(discontinuity.baseline.as_deref()),
            disclosed_version(discontinuity.current.as_deref()),
        ),
    );
    item.analyzer_version_discontinuity = Some(discontinuity);
    item
}

/// Normalize repository-root spellings before comparison (issue #1964,
/// review): separators unify, a single leading `./` folds away, and trailing
/// slashes drop, so `.`, `./`, and `.\\` compare equal. Spellings that name
/// the same checkout in genuinely different forms (relative vs absolute) stay
/// distinct: the refusal fails safe toward stale visibility, and the pair is
/// always quoted in the warning so the operator can see why.
pub(crate) fn normalize_root_for_comparison(root: &str) -> String {
    let mut normalized = root.replace('\\', "/");
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_string()
}

/// Attach the legacy-fallback disclosure quartet to a delta item built from a
/// fallback-only join (issue #1964): the match kind, the never-silent warning
/// flag, the retained legacy identity, and the retained canonical replacement
/// candidate (if the current candidate carries one).
fn with_legacy_disclosure(
    mut item: DeltaItem,
    matched_legacy_identity: Option<String>,
    canonical_replacement_candidate: Option<String>,
) -> DeltaItem {
    item.baseline_match_kind = Some(BASELINE_MATCH_KIND_LEGACY_PATH_LINE_CLASS.to_string());
    item.stale_baseline_warning = true;
    item.matched_legacy_identity = matched_legacy_identity;
    item.canonical_replacement_candidate = canonical_replacement_candidate;
    item
}

fn push_missing_input_items(
    baseline: &BaselineParse,
    current: &CurrentParse,
    items: &mut Vec<DeltaItem>,
) {
    if !baseline.unavailable && !baseline.entries.is_empty() {
        for record in &baseline.entries {
            items.push(missing_current_input_item(record));
        }
    } else if items.is_empty() || current.unavailable {
        items.push(DeltaItem {
            bucket: Bucket::MissingCurrentInput,
            identity: Identity::default(),
            path: None,
            line: None,
            static_class: None,
            decision: None,
            reason:
                "Required baseline or current gate-decision input is unavailable; rerun or repair the missing artifact."
                    .to_string(),
            missing_discriminator: None,
            suggested_test: SuggestedTest::default(),
            repair: repair("provide_required_input"),
            review: None,
            baseline_match_kind: None,
            stale_baseline_warning: false,
            matched_legacy_identity: None,
            canonical_replacement_candidate: None,
            analyzer_version_discontinuity: None,
            not_evaluated: None,
        });
    }
}

fn matched_item(
    baseline: &BaselineRecord,
    current: &CurrentDecision,
    matched_by: String,
) -> DeltaItem {
    let bucket = current_matched_bucket(current);
    let mut identity = current.identity.clone();
    identity.matched_by = Some(matched_by.clone());
    let item = DeltaItem {
        bucket,
        identity,
        path: current.path.clone().or_else(|| baseline.path.clone()),
        line: current.line.or(baseline.line),
        static_class: current
            .static_class
            .clone()
            .or_else(|| baseline.static_class.clone()),
        decision: Some(current.decision.clone()),
        reason: matched_reason(bucket, current),
        missing_discriminator: current
            .evidence
            .missing_discriminator
            .clone()
            .or_else(|| baseline.evidence.missing_discriminator.clone()),
        suggested_test: suggested_test(&current.evidence, &baseline.evidence),
        repair: repair_for_bucket(bucket),
        review: baseline.review.clone(),
        baseline_match_kind: None,
        stale_baseline_warning: false,
        matched_legacy_identity: None,
        canonical_replacement_candidate: None,
        analyzer_version_discontinuity: None,
        not_evaluated: None,
    };
    // Every fallback-only join is a reviewable compatibility event (issue
    // #1964): the match kind, the warning flag, the retained legacy identity,
    // and the retained canonical replacement candidate travel on the item.
    if matched_by == "fallback" {
        with_legacy_disclosure(
            item,
            baseline.identity.fallback.clone(),
            current.identity.canonical_gap_id.clone(),
        )
    } else {
        item
    }
}

fn current_matched_bucket(current: &CurrentDecision) -> Bucket {
    if is_suppressed(current) {
        Bucket::Suppressed
    } else if current.decision == "acknowledged" {
        Bucket::Acknowledged
    } else {
        Bucket::StillPresent
    }
}

fn unmatched_current_item(current: &CurrentDecision) -> DeltaItem {
    let bucket = unmatched_current_bucket(current);
    DeltaItem {
        bucket,
        identity: current.identity.clone(),
        path: current.path.clone(),
        line: current.line,
        static_class: current.static_class.clone(),
        decision: Some(current.decision.clone()),
        reason: unmatched_current_reason(bucket, current),
        missing_discriminator: current.evidence.missing_discriminator.clone(),
        suggested_test: suggested_test(&current.evidence, &Evidence::default()),
        repair: repair_for_bucket(bucket),
        review: None,
        baseline_match_kind: None,
        stale_baseline_warning: false,
        matched_legacy_identity: None,
        canonical_replacement_candidate: None,
        analyzer_version_discontinuity: None,
        not_evaluated: None,
    }
}

fn unmatched_current_bucket(current: &CurrentDecision) -> Bucket {
    if is_suppressed(current) {
        Bucket::Suppressed
    } else if current.decision == "acknowledged" {
        Bucket::Acknowledged
    } else {
        Bucket::NewPolicyEligible
    }
}

fn resolved_item(baseline: &BaselineRecord) -> DeltaItem {
    DeltaItem {
        bucket: Bucket::Resolved,
        identity: baseline.identity.clone(),
        path: baseline.path.clone(),
        line: baseline.line,
        static_class: baseline.static_class.clone(),
        decision: baseline.decision.clone(),
        reason: "Reviewed baseline identity is absent from current gate-decision evidence."
            .to_string(),
        missing_discriminator: baseline.evidence.missing_discriminator.clone(),
        suggested_test: suggested_test(&baseline.evidence, &Evidence::default()),
        repair: repair("remove_resolved_from_baseline_when_reviewed"),
        review: baseline.review.clone(),
        baseline_match_kind: None,
        stale_baseline_warning: false,
        matched_legacy_identity: None,
        canonical_replacement_candidate: None,
        analyzer_version_discontinuity: None,
        not_evaluated: None,
    }
}

fn stale_item(baseline: &BaselineRecord, reason: String) -> DeltaItem {
    DeltaItem {
        bucket: Bucket::StaleBaselineEntry,
        identity: baseline.identity.clone(),
        path: baseline.path.clone(),
        line: baseline.line,
        static_class: baseline.static_class.clone(),
        decision: baseline.decision.clone(),
        reason,
        missing_discriminator: baseline.evidence.missing_discriminator.clone(),
        suggested_test: suggested_test(&baseline.evidence, &Evidence::default()),
        repair: repair("inspect_or_refresh_baseline_entry"),
        review: baseline.review.clone(),
        baseline_match_kind: None,
        stale_baseline_warning: false,
        matched_legacy_identity: None,
        canonical_replacement_candidate: None,
        analyzer_version_discontinuity: None,
        not_evaluated: None,
    }
}

/// Prepare a gate scope once per artifact, retaining the binding between the
/// raw disclosure and its validated membership for every absent baseline row.
pub(super) struct PreparedAnalysisScope<'a> {
    scope: Option<&'a Value>,
    analyzed_files: Option<BTreeSet<&'a str>>,
}

impl<'a> PreparedAnalysisScope<'a> {
    pub(super) fn new(scope: Option<&'a Value>) -> Self {
        let analyzed_files = scope
            .filter(|scope| {
                scope.get("scope").and_then(Value::as_str) == Some("rust_workspace_complete")
            })
            .and_then(|scope| scope.get("corpus"))
            .and_then(crate::output::gap_source_subject::analyzed_production_files);
        Self {
            scope,
            analyzed_files,
        }
    }
}

/// Whether a baseline entry absent from current evidence may be read as
/// resolved (issue #9301 F2). Returns `Some(reason)` when it may not.
///
/// A gate decision built from a diff-scoped review-comments document lists
/// only gaps in the analyzed files, so absence elsewhere says nothing about
/// the gap. Resolution needs the entry's file inside the disclosed
/// `analysis_scope`; a missing, malformed, incomplete, or unrecognized
/// disclosure fails closed. `baseline update --remove-resolved` shares this
/// predicate so delta and update cannot disagree.
pub(super) fn absence_not_evaluated_reason(
    analysis_scope: &PreparedAnalysisScope<'_>,
    entry_path: Option<&str>,
) -> Option<String> {
    let Some(scope) = analysis_scope.scope.filter(|scope| scope.is_object()) else {
        return Some(
            "the current gate decision discloses no analysis_scope, so it is not established that this entry's file was analyzed; regenerate the gate decision with a current build"
                .to_string(),
        );
    };
    let kind = scope
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if kind == "rust_workspace_complete" {
        return workspace_scope_not_evaluated_reason(
            scope,
            analysis_scope.analyzed_files.as_ref(),
            entry_path,
        );
    }
    if kind == "gap_ledger_artifact" {
        let refused = scope
            .get("coverage_refused")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .filter(|items| !items.is_empty())
            .map(|items| {
                format!(
                    " (the ledger's workspace coverage was refused at gate time; differing: {})",
                    items.join(", ")
                )
            })
            .unwrap_or_default();
        return Some(format!(
            "a gap-ledger gate decision lists only the ledger's own anchor files, not the files analyzed, so absence is not evidence of resolution{refused} (regenerate the ledger from a complete `ripr check --format repo-exposure-json` run with `reports gap-ledger --repo-exposure` so it discloses workspace coverage, #9599)"
        ));
    }
    if kind != "diff_scoped_changed_files" {
        return Some(format!(
            "the current gate decision's analysis scope `{kind}` is not recognized, so file coverage is not established"
        ));
    }
    // `unevaluated_seams` is emitted only when above zero, so absent means
    // zero; present must be a nonnegative integer.
    match scope.get("unevaluated_seams") {
        None => {}
        Some(value) => match value.as_u64() {
            Some(0) => {}
            Some(_) => {
                return Some(
                    "the current analysis did not evaluate every seam in its scope (`unevaluated_seams` > 0), so absence is not evidence of resolution"
                        .to_string(),
                );
            }
            None => {
                return Some(
                    "the current gate decision's analysis scope has a malformed `unevaluated_seams`, so coverage is not established"
                        .to_string(),
                );
            }
        },
    }
    // These two are always emitted by a current producer (0 when nothing was
    // dropped); a missing key means the document predates the disclosure, so
    // dropped seams cannot be ruled out.
    for key in ["cap_omitted_seams", "omitted_by_policy"] {
        match scope.get(key).map(Value::as_u64) {
            Some(Some(0)) => {}
            Some(Some(_)) => {
                return Some(format!(
                    "the current analysis dropped seams (`{key}` > 0), so absence is not evidence of resolution"
                ));
            }
            Some(None) => {
                return Some(format!(
                    "the current gate decision's analysis scope has a malformed `{key}`, so coverage is not established"
                ));
            }
            None => {
                return Some(format!(
                    "the current gate decision's analysis scope does not disclose `{key}` (produced by an older build), so dropped seams cannot be ruled out; regenerate it"
                ));
            }
        }
    }
    let Some(path) = entry_path
        .map(normalize_scope_path)
        .filter(|p| !p.is_empty())
    else {
        return Some(
            "the baseline entry has no path to join against the analyzed scope".to_string(),
        );
    };
    // Coverage is the set of production files the seam analysis evaluated.
    // `changed_files` also lists non-Rust and test files the Rust-seam
    // analysis never read, so it is not a coverage claim.
    if scope.get("changed_files").is_some_and(|v| !v.is_array()) {
        return Some(
            "the current gate decision's analysis scope has a malformed `changed_files`, so coverage is not established"
                .to_string(),
        );
    }
    let Some(production) = scope
        .get("changed_production_files")
        .and_then(Value::as_array)
    else {
        return Some(
            "the current gate decision's analysis scope has no usable `changed_production_files`, so coverage is not established"
                .to_string(),
        );
    };
    let mut covered = false;
    for item in production {
        let Some(file) = item.as_str() else {
            return Some(
                "the current gate decision's analysis scope has a non-string entry in `changed_production_files`, so coverage is not established"
                    .to_string(),
            );
        };
        if normalize_scope_path(file) == path {
            covered = true;
        }
    }
    let declined_hit = match scope.get("parser_declined_files") {
        None => false,
        Some(Value::Array(items)) => items.iter().any(|item| {
            item.as_str()
                .is_none_or(|f| normalize_scope_path(f) == path)
        }),
        Some(_) => {
            return Some(
                "the current gate decision's analysis scope has a malformed `parser_declined_files`, so coverage is not established"
                    .to_string(),
            );
        }
    };
    if declined_hit {
        return Some(format!(
            "`{path}` was not parsed (nesting budget, parse error or non-UTF-8; lexical fallback extracts no seams), so absence is not evidence of resolution"
        ));
    }
    let absent_hit = match scope.get("absent_changed_files") {
        None => false,
        Some(Value::Array(items)) => items.iter().any(|item| {
            item.as_str()
                .is_none_or(|f| normalize_scope_path(f) == path)
        }),
        Some(_) => {
            return Some(
                "the current gate decision's analysis scope has a malformed `absent_changed_files`, so coverage is not established"
                    .to_string(),
            );
        }
    };
    if absent_hit {
        return Some(format!(
            "`{path}` is absent from the working tree the current analysis read, so it was not evaluated"
        ));
    }
    if covered {
        None
    } else {
        Some(format!(
            "`{path}` is outside the files the current analysis covered (scope `{kind}`)"
        ))
    }
}

/// Coverage by a producer-disclosed complete whole-Rust-workspace run (#9599,
/// the gap-ledger path). The disclosure is independent of which records
/// survive, and `gate evaluate` only emits it while the ledger still holds
/// exactly the record set it was made for. Any non-Rust path (Python,
/// TypeScript, Markdown, ...) was never read by that analysis.
fn workspace_scope_not_evaluated_reason(
    scope: &Value,
    analyzed_files: Option<&BTreeSet<&str>>,
    entry_path: Option<&str>,
) -> Option<String> {
    let well_formed = scope.get("run_status").and_then(Value::as_str) == Some("complete")
        && scope.get("basis").and_then(Value::as_str) == Some("repo_exposure_complete_run")
        && scope.pointer("/corpus/scope").and_then(Value::as_str) == Some("rust_workspace")
        && scope.pointer("/corpus/unavailable").is_none()
        && scope
            .pointer("/corpus/files_content_hash")
            .and_then(Value::as_str)
            .is_some_and(crate::output::gap_source_subject::is_corpus_hash_shape)
        && scope
            .pointer("/corpus/cache_schema_version")
            .is_some_and(Value::is_string)
        && analyzed_files.is_some()
        && scope
            .get("corpus")
            .and_then(crate::output::gap_source_subject::analysis_inputs)
            .is_some()
        && scope.get("records_total").is_some_and(Value::is_u64)
        && scope
            .get("records_digest")
            .and_then(Value::as_str)
            .is_some_and(|digest| {
                digest.strip_prefix("sha256:").is_some_and(|hex| {
                    hex.len() == 64
                        && hex
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
            });
    if !well_formed {
        return Some(
            "the current gate decision's workspace coverage disclosure is malformed or incomplete, so file coverage is not established"
                .to_string(),
        );
    }
    let Some(raw) = entry_path.filter(|p| !p.is_empty()) else {
        return Some(
            "the baseline entry has no path to join against the analyzed scope".to_string(),
        );
    };
    if let Some(why) = crate::output::gap_source_subject::rust_workspace_path_gap(raw) {
        return Some(format!(
            "`{raw}` is not established as a file the complete workspace run analyzed: {why}"
        ));
    }
    if analyzed_files.is_some_and(|files| files.contains(raw)) {
        return None;
    }
    Some(format!(
        "`{raw}` is not established as a file the complete workspace run analyzed: the producer did not disclose this path as analyzed production source"
    ))
}

fn normalize_scope_path(path: &str) -> String {
    let unified = path.trim().replace('\\', "/");
    unified.trim_start_matches("./").to_string()
}

fn not_evaluated_item(baseline: &BaselineRecord, reason: String) -> DeltaItem {
    let mut item = stale_item(
        baseline,
        format!(
            "Reviewed baseline identity is absent from current gate-decision evidence but was not evaluated: {reason}. It is neither resolved nor removable; run a full-scope analysis covering the entry before treating it as fixed."
        ),
    );
    item.not_evaluated = Some(reason);
    item
}

fn missing_current_input_item(baseline: &BaselineRecord) -> DeltaItem {
    DeltaItem {
        bucket: Bucket::MissingCurrentInput,
        identity: baseline.identity.clone(),
        path: baseline.path.clone(),
        line: baseline.line,
        static_class: baseline.static_class.clone(),
        decision: baseline.decision.clone(),
        reason: "Required current gate-decision evidence is unavailable; baseline movement cannot be classified.".to_string(),
        missing_discriminator: baseline.evidence.missing_discriminator.clone(),
        suggested_test: suggested_test(&baseline.evidence, &Evidence::default()),
        repair: repair("provide_current_gate_decision"),
        review: baseline.review.clone(),
        baseline_match_kind: None,
        stale_baseline_warning: false,
        matched_legacy_identity: None,
        canonical_replacement_candidate: None,
        analyzer_version_discontinuity: None,
        not_evaluated: None,
    }
}

fn invalid_baseline_item(value: &Value) -> DeltaItem {
    DeltaItem {
        bucket: Bucket::InvalidBaselineEntry,
        identity: Identity::default(),
        path: string_field(value.get("path"))
            .or_else(|| string_field(value.pointer("/placement/path"))),
        line: value
            .get("line")
            .and_then(Value::as_u64)
            .or_else(|| value.pointer("/placement/line").and_then(Value::as_u64)),
        static_class: string_field(value.get("static_class")),
        decision: string_field(value.get("decision")),
        reason:
            "Baseline entry is missing canonical_gap_id, seam_id, source_id, id, dedupe_key, and fallback identity."
                .to_string(),
        missing_discriminator: string_field(value.pointer("/evidence/missing_discriminator")),
        suggested_test: SuggestedTest {
            recommended_test: string_field(value.pointer("/evidence/recommended_test")),
            assertion_shape: string_field(value.pointer("/evidence/assertion_shape")),
        },
        repair: repair("repair_or_remove_baseline_entry"),
        review: review_metadata_from_value(value.get("review")),
        baseline_match_kind: None,
        stale_baseline_warning: false,
        matched_legacy_identity: None,
        canonical_replacement_candidate: None,
        analyzer_version_discontinuity: None,
        not_evaluated: None,
    }
}

fn is_suppressed(current: &CurrentDecision) -> bool {
    current.decision == "suppressed"
        || current.decision == "not_applicable"
        || current.severity.as_deref() == Some("off")
        || current.evidence.suppressed
        || current.evidence.configured_off
}

fn matched_reason(bucket: Bucket, current: &CurrentDecision) -> String {
    match bucket {
        Bucket::StillPresent => {
            "Reviewed baseline identity is still present in current gate-decision evidence."
                .to_string()
        }
        Bucket::Acknowledged => current
            .gate_reason
            .clone()
            .unwrap_or_else(|| "Current decision is acknowledged and remains visible.".to_string()),
        Bucket::Suppressed => current
            .gate_reason
            .clone()
            .unwrap_or_else(|| "Current decision is suppressed or configured off.".to_string()),
        _ => "Current decision matched reviewed baseline evidence.".to_string(),
    }
}

fn unmatched_current_reason(bucket: Bucket, current: &CurrentDecision) -> String {
    match bucket {
        Bucket::NewPolicyEligible => {
            "Current policy-eligible gap is not present in the reviewed baseline.".to_string()
        }
        Bucket::Acknowledged => current
            .gate_reason
            .clone()
            .unwrap_or_else(|| "Current decision is acknowledged and remains visible.".to_string()),
        Bucket::Suppressed => current
            .gate_reason
            .clone()
            .unwrap_or_else(|| "Current decision is suppressed or configured off.".to_string()),
        _ => "Current decision is not present in the reviewed baseline.".to_string(),
    }
}

fn suggested_test(primary: &Evidence, fallback: &Evidence) -> SuggestedTest {
    SuggestedTest {
        recommended_test: primary
            .recommended_test
            .clone()
            .or_else(|| fallback.recommended_test.clone()),
        assertion_shape: primary
            .assertion_shape
            .clone()
            .or_else(|| fallback.assertion_shape.clone()),
    }
}

fn repair_for_bucket(bucket: Bucket) -> Repair {
    match bucket {
        Bucket::StillPresent => repair("keep_visible_or_burn_down"),
        Bucket::Resolved => repair("remove_resolved_from_baseline_when_reviewed"),
        Bucket::NewPolicyEligible => Repair {
            action: "add_focused_test_or_acknowledge".to_string(),
            verify_command: Some("ripr agent verify --root . --before target/ripr/pilot/repo-exposure.json --after target/ripr/pilot/after.repo-exposure.json --json".to_string()),
        },
        Bucket::Acknowledged => repair("review_acknowledgement"),
        Bucket::Suppressed => repair("review_suppression_or_config"),
        Bucket::StaleBaselineEntry => repair("inspect_or_refresh_baseline_entry"),
        Bucket::InvalidBaselineEntry => repair("repair_or_remove_baseline_entry"),
        Bucket::MissingCurrentInput => repair("provide_current_gate_decision"),
    }
}

fn repair(action: &str) -> Repair {
    Repair {
        action: action.to_string(),
        verify_command: None,
    }
}

fn count_items(items: &[DeltaItem]) -> DeltaCounts {
    let mut counts = DeltaCounts::default();
    for item in items {
        match item.bucket {
            Bucket::StillPresent => counts.still_present += 1,
            Bucket::Resolved => counts.resolved += 1,
            Bucket::NewPolicyEligible => counts.new_policy_eligible += 1,
            Bucket::Acknowledged => counts.acknowledged += 1,
            Bucket::Suppressed => counts.suppressed += 1,
            Bucket::StaleBaselineEntry => counts.stale_baseline_entry += 1,
            Bucket::InvalidBaselineEntry => counts.invalid_baseline_entry += 1,
            Bucket::MissingCurrentInput => counts.missing_current_input += 1,
        }
        if item.stale_baseline_warning {
            counts.legacy_fallback_match += 1;
        }
    }
    counts
}

fn inputs_json(inputs: &BaselineDeltaInputs) -> Value {
    json!({
        "baseline": inputs.baseline,
        "current_gate_decision": inputs.current_gate_decision,
        "pr_guidance": inputs.pr_guidance,
        "agent_receipt": inputs.agent_receipt,
    })
}

fn baseline_summary_json(summary: &BaselineSummary) -> Value {
    json!({
        "path": summary.path,
        "schema_version": summary.schema_version,
        "entries": summary.entries,
        "valid": summary.valid,
        "stale": summary.stale,
        "invalid": summary.invalid,
    })
}

fn delta_json(delta: &DeltaCounts) -> Value {
    json!({
        "still_present": delta.still_present,
        "resolved": delta.resolved,
        "new_policy_eligible": delta.new_policy_eligible,
        "acknowledged": delta.acknowledged,
        "suppressed": delta.suppressed,
        "stale_baseline_entry": delta.stale_baseline_entry,
        "invalid_baseline_entry": delta.invalid_baseline_entry,
        "missing_current_input": delta.missing_current_input,
        "legacy_fallback_match": delta.legacy_fallback_match,
    })
}

fn item_json(item: &DeltaItem, causal_projection: Option<&CausalDeltaArtifact>) -> Value {
    let mut output = json!({
        "bucket": item.bucket.as_str(),
        "identity": {
            "canonical_gap_id": item.identity.canonical_gap_id,
            "seam_id": item.identity.seam_id,
            "source_id": item.identity.source_id,
            "id": item.identity.id,
            "dedupe_key": item.identity.dedupe_key,
            "fallback": item.identity.fallback,
            "matched_by": item.identity.matched_by,
        },
        "path": item.path,
        "line": item.line,
        "static_class": item.static_class,
        "decision": item.decision,
        "reason": item.reason,
        "missing_discriminator": item.missing_discriminator,
        "suggested_test": {
            "recommended_test": item.suggested_test.recommended_test,
            "assertion_shape": item.suggested_test.assertion_shape,
        },
        "repair": {
            "action": item.repair.action,
            "verify_command": item.repair.verify_command,
        },
        "review": review_metadata_json(&item.review),
    });
    if let Some(projection) = causal_projection
        && let Some(delta) = projection.delta_for(item.identity.canonical_gap_id.as_deref())
        && let Some(object) = output.as_object_mut()
    {
        insert_canonical_delta_fields(object, delta);
    }
    // Additive (issue #1964): present only on legacy fallback-only joins, so
    // canonical-match and baseline-new items render byte-identical to before
    // the disclosure existed. Mirrors the gate-decision `baseline_match_kind`
    // vocabulary (RIPR-SPEC-0014 § Baseline Comparison).
    if item.stale_baseline_warning
        && let Some(object) = output.as_object_mut()
    {
        object.insert(
            "baseline_match_kind".to_string(),
            Value::String(
                item.baseline_match_kind
                    .clone()
                    .unwrap_or_else(|| BASELINE_MATCH_KIND_LEGACY_PATH_LINE_CLASS.to_string()),
            ),
        );
        object.insert("stale_baseline_warning".to_string(), Value::Bool(true));
        object.insert(
            "matched_legacy_identity".to_string(),
            item.matched_legacy_identity
                .clone()
                .map_or(Value::Null, Value::String),
        );
        object.insert(
            "canonical_replacement_candidate".to_string(),
            item.canonical_replacement_candidate
                .clone()
                .map_or(Value::Null, Value::String),
        );
    }
    // Additive (issue #7421): present only on analyzer-version identity
    // discontinuity items, so same-build items render byte-identical to
    // before the disclosure existed. A `null` side records that side's build
    // predates subject.analyzer_version persistence.
    if let Some(discontinuity) = item.analyzer_version_discontinuity.as_ref()
        && let Some(object) = output.as_object_mut()
    {
        object.insert(
            "analyzer_version_discontinuity".to_string(),
            json!({
                "baseline_analyzer_version": discontinuity.baseline,
                "current_analyzer_version": discontinuity.current,
            }),
        );
    }
    // Additive (issue #9301 F2): present only on baseline entries whose
    // absence was outside the analyzed scope, so in-scope items render
    // byte-identical to before. The bucket stays `stale_baseline_entry` so
    // every consumer that withholds on stale entries keeps withholding.
    if let Some(reason) = item.not_evaluated.as_ref()
        && let Some(object) = output.as_object_mut()
    {
        object.insert("evaluation".to_string(), json!("not_evaluated"));
        object.insert("not_evaluated_reason".to_string(), json!(reason));
    }
    output
}

fn review_metadata_from_value(value: Option<&Value>) -> Option<ReviewMetadata> {
    let value = value?;
    if !value.is_object() {
        return None;
    }
    Some(ReviewMetadata {
        reviewed: value.get("reviewed").and_then(Value::as_bool),
        owner: string_field(value.get("owner")),
        reason: string_field(value.get("reason")),
        created_at: string_field(value.get("created_at")),
        review_after: string_field(value.get("review_after")),
        source: string_field(value.get("source")),
    })
}

fn review_metadata_json(review: &Option<ReviewMetadata>) -> Value {
    match review {
        Some(review) => json!({
            "reviewed": review.reviewed,
            "owner": review.owner,
            "reason": review.reason,
            "created_at": review.created_at,
            "review_after": review.review_after,
            "source": review.source,
        }),
        None => Value::Null,
    }
}

fn bucket_order() -> [Bucket; 8] {
    [
        Bucket::StillPresent,
        Bucket::Resolved,
        Bucket::NewPolicyEligible,
        Bucket::Acknowledged,
        Bucket::Suppressed,
        Bucket::StaleBaselineEntry,
        Bucket::InvalidBaselineEntry,
        Bucket::MissingCurrentInput,
    ]
}

fn count_for_bucket(delta: &DeltaCounts, bucket: Bucket) -> usize {
    match bucket {
        Bucket::StillPresent => delta.still_present,
        Bucket::Resolved => delta.resolved,
        Bucket::NewPolicyEligible => delta.new_policy_eligible,
        Bucket::Acknowledged => delta.acknowledged,
        Bucket::Suppressed => delta.suppressed,
        Bucket::StaleBaselineEntry => delta.stale_baseline_entry,
        Bucket::InvalidBaselineEntry => delta.invalid_baseline_entry,
        Bucket::MissingCurrentInput => delta.missing_current_input,
    }
}

fn item_headline(item: &DeltaItem) -> String {
    match (
        item.path.as_deref(),
        item.line,
        item.static_class.as_deref(),
    ) {
        (Some(path), Some(line), Some(class)) => format!("{path}:{line} {class}"),
        (Some(path), Some(line), None) => format!("{path}:{line}"),
        (Some(path), None, Some(class)) => format!("{path} {class}"),
        (Some(path), None, None) => path.to_string(),
        _ => item.identity.sort_key(),
    }
}

pub(super) fn canonical_gap_id_from_value(value: &Value) -> Option<String> {
    string_field(value.get("canonical_gap_id"))
        .or_else(|| string_field(value.pointer("/identity/canonical_gap_id")))
        .or_else(|| string_field(value.pointer("/evidence_record/canonical_gap_id")))
}

fn string_field(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) use crate::output::path::display_path;

#[cfg(test)]
mod tests {
    #[test]
    fn canonical_identity_divergence_matrix_is_pinned_for_gate_and_delta() {
        use super::canonical_identity_divergence as div;
        // The gate and `baseline diff` both call this one rule (issue #1964).
        assert_eq!(div(Some("a"), Some("a")), None);
        assert_eq!(div(Some("a"), Some("b")).as_deref(), Some("a -> b"));
        // A side without a canonical id carries no divergence evidence, so a
        // legacy/identityless side keeps the disclosed fallback join.
        assert_eq!(div(Some("a"), None), None);
        assert_eq!(div(None, Some("b")), None);
        assert_eq!(div(None, None), None);
    }

    use super::{
        BaselineDeltaInput, PreparedAnalysisScope, absence_not_evaluated_reason,
        build_baseline_delta_report, render_baseline_delta_json, render_baseline_delta_markdown,
    };
    use serde_json::{Value, json};
    use std::path::{Path, PathBuf};

    #[test]
    fn baseline_delta_rejects_identityless_current_decisions() -> Result<(), String> {
        use crate::output::baseline::{
            baseline_create_report_from_gate_decision_json, render_baseline_create_json,
        };
        use crate::output::baseline_update::{
            BaselineUpdateInput, build_baseline_update_remove_resolved,
        };
        use serde_json::json;

        let source = json!({
            "schema_version": "0.1",
            "decisions": [{"seam_id": "reviewed", "decision": "advisory"}]
        });
        let created = baseline_create_report_from_gate_decision_json(
            "gate.json",
            "unix_ms:1",
            &source.to_string(),
        )?;
        let baseline = render_baseline_create_json(&created)?;
        let baseline_value: Value = serde_json::from_str(&baseline).map_err(|e| e.to_string())?;
        assert_eq!(baseline_value["entries"].as_array().map(Vec::len), Some(1));
        let warning = "required current gate-decision input current.json contains a decision without stable identity";

        for malformed in [Value::Null, json!(42), json!({}), json!({"seam_id": "  "})] {
            // A valid sibling must not hide a corrupt record in a partial denominator.
            for decisions in [
                json!([malformed]),
                json!([source["decisions"][0], malformed]),
            ] {
                let current = json!({"schema_version": "0.1", "decisions": decisions});
                let report = build_baseline_delta_report(BaselineDeltaInput {
                    root: ".".to_string(),
                    baseline_path: "baseline.json".to_string(),
                    current_gate_decision_path: "current.json".to_string(),
                    baseline_json: Ok(baseline.clone()),
                    current_gate_decision_json: Ok(current.to_string()),
                });
                let rendered: Value = serde_json::from_str(&render_baseline_delta_json(&report)?)
                    .map_err(|e| e.to_string())?;
                assert_eq!(rendered["delta"]["resolved"], 0, "{rendered}");
                assert_eq!(rendered["delta"]["still_present"], 0, "{rendered}");
                assert_eq!(rendered["delta"]["missing_current_input"], 1, "{rendered}");
                assert_eq!(rendered["items"][0]["identity"]["seam_id"], "reviewed");
                assert!(
                    rendered["warnings"]
                        .as_array()
                        .is_some_and(|warnings| warnings.contains(&json!(warning))),
                    "{rendered}"
                );
                let markdown = render_baseline_delta_markdown(&report);
                assert!(markdown.contains("Missing current input"));
                assert!(markdown.contains(warning), "{markdown}");

                for (remove_resolved, migrate_legacy_identities) in [(true, false), (false, true)] {
                    let update = build_baseline_update_remove_resolved(BaselineUpdateInput {
                        baseline_path: "baseline.json".to_string(),
                        current_gate_decision_path: "current.json".to_string(),
                        baseline_json: baseline.clone(),
                        current_gate_decision_json: current.to_string(),
                        remove_resolved,
                        migrate_legacy_identities,
                    });
                    assert!(
                        matches!(update, Err(error) if error.contains("without stable identity"))
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn baseline_delta_reports_all_primary_buckets() -> Result<(), String> {
        let baseline = r#"{
          "schema_version": "0.1",
          "tool": "ripr",
          "kind": "gate_baseline",
          "entries": [
            {
              "identity": {"seam_id": "same"},
              "path": "src/same.rs",
              "line": 1,
              "static_class": "weakly_gripped",
              "decision": "advisory",
              "evidence": {"missing_discriminator": "same == 1"}
            },
            {
              "identity": {"seam_id": "gone"},
              "path": "src/gone.rs",
              "line": 2,
              "static_class": "weakly_gripped",
              "decision": "advisory"
            },
            {
              "identity": {},
              "decision": "advisory"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "analysis_scope": {"scope": "diff_scoped_changed_files", "changed_production_files": ["src/gone.rs"], "cap_omitted_seams": 0, "omitted_by_policy": 0},
          "decisions": [
            {
              "decision": "advisory",
              "id": "ripr-gate-same",
              "seam_id": "same",
              "source_id": "ripr-review-same",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/same.rs", "line": 1},
              "evidence": {"missing_discriminator": "same == 1", "suppressed": false, "configured_off": false}
            },
            {
              "decision": "blocking",
              "id": "ripr-gate-new",
              "seam_id": "new",
              "source_id": "ripr-review-new",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/new.rs", "line": 4},
              "evidence": {"missing_discriminator": "new == 4", "assertion_shape": "assert_eq!(new(), 4)", "recommended_test": "tests/new.rs::boundary", "suppressed": false, "configured_off": false}
            },
            {
              "decision": "acknowledged",
              "id": "ripr-gate-ack",
              "seam_id": "ack",
              "source_id": "ripr-review-ack",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/ack.rs", "line": 5},
              "gate_reason": "policy-eligible gap acknowledged by ripr-waive",
              "evidence": {"suppressed": false, "configured_off": false}
            },
            {
              "decision": "suppressed",
              "id": "ripr-gate-suppressed",
              "seam_id": "suppressed",
              "source_id": "ripr-review-suppressed",
              "static_class": "weakly_gripped",
              "severity": "off",
              "placement": {"path": "src/suppressed.rs", "line": 6},
              "evidence": {"suppressed": true, "configured_off": true}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: ".ripr/gate-baseline.json".to_string(),
            current_gate_decision_path: "target/ripr/reports/gate-decision.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 1"));
        assert!(rendered.contains("\"resolved\": 1"));
        assert!(rendered.contains("\"new_policy_eligible\": 1"));
        assert!(rendered.contains("\"acknowledged\": 1"));
        assert!(rendered.contains("\"suppressed\": 1"));
        assert!(rendered.contains("\"invalid_baseline_entry\": 1"));
        assert!(rendered.contains("\"matched_by\": \"seam_id\""));
        assert!(rendered.contains("\"verify_command\""));

        let markdown = render_baseline_delta_markdown(&report);
        assert!(markdown.contains("| New policy-eligible | 1 |"));
        assert!(markdown.contains("Top new policy-eligible gaps:"));
        assert!(markdown.contains("Missing: new == 4"));
        assert!(markdown.contains("Resolved baseline entries:"));
        Ok(())
    }

    #[test]
    fn baseline_delta_matches_by_fallback_and_warns() -> Result<(), String> {
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"fallback": "src/fallback.rs:7:weakly_gripped"},
              "path": "src/fallback.rs",
              "line": 7,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [
            {
              "decision": "advisory",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/fallback.rs", "line": 7},
              "evidence": {}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 1"));
        assert!(rendered.contains("\"matched_by\": \"fallback\""));
        assert!(rendered.contains("matched current evidence by fallback"));
        Ok(())
    }

    #[test]
    fn baseline_delta_matches_by_canonical_gap_id_across_line_movement() -> Result<(), String> {
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {
                "canonical_gap_id": "pricing::discount::threshold_equality",
                "seam_id": "old-seam"
              },
              "path": "src/pricing.rs",
              "line": 10,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [
            {
              "decision": "advisory",
              "seam_id": "new-seam-after-refactor",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/pricing.rs", "line": 88},
              "evidence_record": {
                "canonical_gap_id": "pricing::discount::threshold_equality"
              },
              "evidence": {"missing_discriminator": "amount == threshold"}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 1"));
        assert!(rendered.contains("\"matched_by\": \"canonical_gap_id\""));
        assert!(rendered.contains("\"seam_id\": \"new-seam-after-refactor\""));
        assert!(!rendered.contains("\"resolved\": 1"));
        Ok(())
    }

    #[test]
    fn baseline_delta_uses_unique_seam_after_shared_canonical_gap_is_ambiguous()
    -> Result<(), String> {
        let baseline = r#"{"schema_version":"0.1","entries":[
          {"identity":{"canonical_gap_id":"gap:shared","seam_id":"seam:a"}},
          {"identity":{"canonical_gap_id":"gap:shared","seam_id":"seam:b"}}
        ]}"#;
        let current = r#"{"schema_version":"0.1","decisions":[
          {"decision":"advisory","canonical_gap_id":"gap:shared","seam_id":"seam:a","static_class":"weakly_gripped","placement":{"path":"src/a.rs","line":1}},
          {"decision":"advisory","canonical_gap_id":"gap:shared","seam_id":"seam:b","static_class":"weakly_gripped","placement":{"path":"src/b.rs","line":2}}
        ]}"#;
        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 2"));
        assert!(rendered.contains("\"stale_baseline_entry\": 0"));
        assert!(rendered.contains("\"resolved\": 0"));
        assert!(rendered.contains("\"new_policy_eligible\": 0"));
        assert!(rendered.contains("\"matched_by\": \"seam_id\""));
        Ok(())
    }

    #[test]
    fn baseline_delta_marks_ambiguous_fallback_as_stale() -> Result<(), String> {
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"fallback": "src/ambiguous.rs:7:weakly_gripped"},
              "path": "src/ambiguous.rs",
              "line": 7,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [
            {
              "decision": "advisory",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/ambiguous.rs", "line": 7},
              "evidence": {}
            },
            {
              "decision": "advisory",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/ambiguous.rs", "line": 7},
              "evidence": {}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"stale_baseline_entry\": 1"));
        assert!(rendered.contains("matched 2 current decisions by fallback"));
        Ok(())
    }

    #[test]
    fn baseline_delta_reports_missing_current_input() -> Result<(), String> {
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {"identity": {"seam_id": "same"}, "path": "src/same.rs", "line": 1}
          ]
        }"#;
        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "missing.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Err("read missing.json failed: not found".to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"missing_current_input\": 1"));
        assert!(rendered.contains("required current gate-decision input missing.json is invalid"));
        Ok(())
    }

    #[test]
    fn baseline_delta_treats_non_object_review_metadata_as_absent() -> Result<(), String> {
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"seam_id": "legacy"},
              "path": "src/legacy.rs",
              "line": 7,
              "review": "legacy-freeform-note"
            }
          ]
        }"#;
        let current = r#"{"schema_version": "0.1", "decisions": [], "analysis_scope": {"scope": "diff_scoped_changed_files", "changed_production_files": ["src/legacy.rs"], "cap_omitted_seams": 0, "omitted_by_policy": 0}}"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"resolved\": 1"));
        assert!(rendered.contains("\"review\": null"));
        Ok(())
    }

    #[test]
    fn baseline_delta_reports_input_shape_errors() -> Result<(), String> {
        let valid_baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {"identity": {"seam_id": "same"}, "path": "src/same.rs", "line": 1}
          ]
        }"#;
        let valid_current = r#"{
          "schema_version": "0.1",
          "decisions": [
            {"decision": "advisory", "seam_id": "same", "evidence": {}}
          ]
        }"#;

        for (baseline_json, current_json, expected) in [
            (
                Err("read baseline.json failed: not found".to_string()),
                Ok(valid_current.to_string()),
                "required baseline input baseline.json is invalid",
            ),
            (
                Ok("{".to_string()),
                Ok(valid_current.to_string()),
                "required baseline input baseline.json is invalid",
            ),
            (
                Ok(r#"{"schema_version":"9","entries":[]}"#.to_string()),
                Ok(valid_current.to_string()),
                "unsupported schema_version",
            ),
            (
                Ok(r#"{"schema_version":"0.1"}"#.to_string()),
                Ok(valid_current.to_string()),
                "missing entries or decisions array",
            ),
            (
                Ok(valid_baseline.to_string()),
                Ok("{".to_string()),
                "required current gate-decision input current.json is invalid",
            ),
            (
                Ok(valid_baseline.to_string()),
                Ok(r#"{"schema_version":"9","decisions":[]}"#.to_string()),
                "current gate-decision input current.json has unsupported schema_version",
            ),
            (
                Ok(valid_baseline.to_string()),
                Ok(r#"{"schema_version":"0.1"}"#.to_string()),
                "current gate-decision input current.json is missing decisions array",
            ),
            (
                Ok(valid_baseline.to_string()),
                Ok(r#"{"schema_version":"0.1","decisions":[],"status":42}"#.to_string()),
                "current gate-decision input current.json has malformed status",
            ),
            (
                Ok(valid_baseline.to_string()),
                Ok(r#"{"schema_version":"0.1","decisions":[],"status":"  "}"#.to_string()),
                "current gate-decision input current.json has malformed status",
            ),
            (
                Ok(valid_baseline.to_string()),
                Ok(r#"{"schema_version":"0.1","decisions":[],"status":"error"}"#.to_string()),
                "current gate-decision input current.json has unknown status",
            ),
        ] {
            let report = build_baseline_delta_report(BaselineDeltaInput {
                root: ".".to_string(),
                baseline_path: "baseline.json".to_string(),
                current_gate_decision_path: "current.json".to_string(),
                baseline_json,
                current_gate_decision_json: current_json,
            });
            let rendered = render_baseline_delta_json(&report)?;
            assert!(rendered.contains(expected), "{rendered}");
            assert!(rendered.contains("\"missing_current_input\""), "{rendered}");
        }

        Ok(())
    }

    #[test]
    fn baseline_delta_matches_identity_methods_and_matched_states() -> Result<(), String> {
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {"identity": {"source_id": "source-match"}, "path": "src/source.rs", "line": 1},
            {"identity": {"id": "id-match"}, "path": "src/id.rs", "line": 2},
            {"identity": {"dedupe_key": "dedupe-match"}, "path": "src/dedupe.rs", "line": 3},
            {"identity": {"seam_id": "ack-match"}, "path": "src/ack.rs", "line": 4},
            {"identity": {"seam_id": "suppressed-match"}, "path": "src/suppressed.rs", "line": 5},
            {"identity": {"seam_id": "unknown-decision"}, "path": "src/unknown.rs", "line": 6},
            {"identity": {"seam_id": "duplicate"}, "path": "src/dup_a.rs", "line": 7},
            {"identity": {"seam_id": "duplicate"}, "path": "src/dup_b.rs", "line": 8}
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [
            {"decision": "advisory", "source_id": "source-match", "evidence": {}},
            {"decision": "advisory", "id": "id-match", "evidence": {}},
            {"decision": "advisory", "dedupe_key": "dedupe-match", "evidence": {}},
            {"decision": "acknowledged", "seam_id": "ack-match", "gate_reason": "waived by ripr-waive", "evidence": {}},
            {"decision": "not_applicable", "seam_id": "suppressed-match", "gate_reason": "configured off", "evidence": {"configured_off": true}},
            {"seam_id": "unknown-decision", "evidence": {}},
            {"decision": "advisory", "seam_id": "duplicate", "evidence": {}},
            {"decision": "advisory", "source_id": "new-source", "evidence": {}}
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"matched_by\": \"source_id\""));
        assert!(rendered.contains("\"matched_by\": \"id\""));
        assert!(rendered.contains("\"matched_by\": \"dedupe_key\""));
        assert!(rendered.contains("\"decision\": \"unknown\""));
        assert!(rendered.contains("\"acknowledged\": 1"));
        assert!(rendered.contains("\"suppressed\": 1"));
        assert!(rendered.contains("\"stale_baseline_entry\": 1"));
        assert!(rendered.contains("\"new_policy_eligible\": 1"));
        assert!(rendered.contains("already joined by another baseline entry"));
        assert!(rendered.contains("waived by ripr-waive"));
        assert!(rendered.contains("configured off"));
        Ok(())
    }

    #[test]
    fn baseline_delta_markdown_renders_headline_variants() -> Result<(), String> {
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {"identity": {"seam_id": "line-only"}, "path": "src/line.rs", "line": 3},
            {"identity": {"seam_id": "class-only"}, "path": "src/class.rs", "static_class": "weakly_gripped"},
            {"identity": {"seam_id": "path-only"}, "path": "src/path.rs"},
            {"identity": {"seam_id": "identity-only"}}
          ]
        }"#;
        let current = r#"{"schema_version": "0.1", "decisions": [], "analysis_scope": {"scope": "diff_scoped_changed_files", "changed_production_files": ["src/line.rs", "src/class.rs", "src/path.rs"], "cap_omitted_seams": 0, "omitted_by_policy": 0}}"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let markdown = render_baseline_delta_markdown(&report);
        assert!(markdown.contains("- src/line.rs:3"));
        assert!(markdown.contains("- src/class.rs weakly_gripped"));
        assert!(markdown.contains("- src/path.rs"));
        // A pathless entry cannot be placed inside the analyzed scope, so it
        // is `not_evaluated` (stale), never resolved (issue #9301 F2).
        let json: Value = serde_json::from_str(&render_baseline_delta_json(&report)?)
            .map_err(|err| err.to_string())?;
        assert_eq!(json["delta"]["resolved"], 3, "{json}");
        assert_eq!(json["delta"]["stale_baseline_entry"], 1, "{json}");
        let pathless = json["items"].as_array().and_then(|items| {
            items
                .iter()
                .find(|item| item["bucket"] == "stale_baseline_entry")
        });
        assert_eq!(
            pathless.map(|i| i["evaluation"].clone()),
            Some(json!("not_evaluated"))
        );
        assert!(markdown.contains("Not evaluated by the current analysis (1 total):"));
        assert!(!markdown.contains("Resolved baseline entries:\n- identity-only"));
        Ok(())
    }

    #[test]
    fn baseline_delta_matches_mixed_fixture_contract() -> Result<(), String> {
        let repo_root = repo_root()?;
        let fixture = repo_root.join("fixtures/boundary_gap/expected/baseline-debt-delta/mixed");
        let baseline_path = fixture.join("baseline.json");
        let current_path = fixture.join("current-gate-decision.json");
        let expected_json_path = fixture.join("baseline-debt-delta.json");
        let expected_md_path = fixture.join("baseline-debt-delta.md");
        let baseline_text = read_file(&baseline_path)?;
        let current_text = read_file(&current_path)?;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: fixture_path(&repo_root, &baseline_path),
            current_gate_decision_path: fixture_path(&repo_root, &current_path),
            baseline_json: Ok(baseline_text),
            current_gate_decision_json: Ok(current_text),
        });
        let rendered_json = render_baseline_delta_json(&report)?;
        let rendered_md = render_baseline_delta_markdown(&report);
        if crate::testing::rebless::fixture_rebless_enabled() {
            std::fs::write(&expected_json_path, &rendered_json).map_err(|err| err.to_string())?;
            std::fs::write(&expected_md_path, &rendered_md).map_err(|err| err.to_string())?;
            return Ok(());
        }
        assert_eq!(rendered_json, read_file(&expected_json_path)?.trim_end());
        assert_eq!(rendered_md, read_file(&expected_md_path)?);
        Ok(())
    }

    #[test]
    fn baseline_delta_treats_diverged_canonical_behind_shared_fallback_as_stale()
    -> Result<(), String> {
        // Issue #1964, fixture 1: same path/line/class, but the canonical gap
        // changed after a source edit. The stale fallback must not make the
        // genuinely new canonical gap look historical.
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {
                "canonical_gap_id": "pricing::discount::old_rule",
                "fallback": "src/pricing.rs:88:weakly_gripped"
              },
              "path": "src/pricing.rs",
              "line": 88,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "root": ".",
          "decisions": [
            {
              "decision": "advisory",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/pricing.rs", "line": 88},
              "evidence_record": {
                "canonical_gap_id": "pricing::discount::new_rule"
              },
              "evidence": {"missing_discriminator": "amount > threshold"}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(
            rendered.contains("\"stale_baseline_entry\": 1"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"new_policy_eligible\": 1"),
            "{rendered}"
        );
        assert!(rendered.contains("\"still_present\": 0"), "{rendered}");
        assert!(
            rendered.contains("\"legacy_fallback_match\": 1"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"baseline_match_kind\": \"legacy_path_line_class\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"stale_baseline_warning\": true"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"matched_legacy_identity\": \"src/pricing.rs:88:weakly_gripped\""),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("\"canonical_replacement_candidate\": \"pricing::discount::new_rule\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("canonical identity diverged"),
            "{rendered}"
        );
        assert!(
            rendered.contains("pricing::discount::old_rule -> pricing::discount::new_rule"),
            "{rendered}"
        );
        let markdown = render_baseline_delta_markdown(&report);
        assert!(
            markdown.contains("Legacy fallback matches (1 total):"),
            "{markdown}"
        );
        assert!(
            markdown.contains("Legacy identity: src/pricing.rs:88:weakly_gripped"),
            "{markdown}"
        );
        assert!(
            markdown.contains("Canonical replacement candidate: pricing::discount::new_rule"),
            "{markdown}"
        );
        Ok(())
    }

    #[test]
    fn baseline_delta_discloses_pure_legacy_match_with_replacement_candidate() -> Result<(), String>
    {
        // Issue #1964, fixture 4: the reviewed entry carries no canonical
        // identity at all, but the current candidate does. The match is
        // preserved (compatibility window) and impossible to miss: match kind,
        // warning flag, retained legacy identity, retained replacement.
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"fallback": "src/legacy.rs:7:weakly_gripped"},
              "path": "src/legacy.rs",
              "line": 7,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "root": ".",
          "decisions": [
            {
              "decision": "advisory",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/legacy.rs", "line": 7},
              "evidence_record": {"canonical_gap_id": "legacy::gap::seven"},
              "evidence": {}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 1"), "{rendered}");
        assert!(
            rendered.contains("\"legacy_fallback_match\": 1"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"baseline_match_kind\": \"legacy_path_line_class\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"stale_baseline_warning\": true"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"matched_legacy_identity\": \"src/legacy.rs:7:weakly_gripped\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"canonical_replacement_candidate\": \"legacy::gap::seven\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("matched current evidence by fallback"),
            "{rendered}"
        );
        Ok(())
    }

    #[test]
    fn baseline_delta_refuses_cross_root_fallback_join_as_stale() -> Result<(), String> {
        // Issue #1964, fixture 5: a legacy baseline recorded for another
        // repository must not launder its history into this repo's delta. The
        // reviewed entry goes stale and the current gap stays new.
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"fallback": "src/foreign.rs:7:weakly_gripped"},
              "path": "src/foreign.rs",
              "line": 7,
              "static_class": "weakly_gripped",
              "root": "/other/repo"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "root": ".",
          "decisions": [
            {
              "decision": "advisory",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/foreign.rs", "line": 7},
              "evidence": {}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(
            rendered.contains("\"stale_baseline_entry\": 1"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"new_policy_eligible\": 1"),
            "{rendered}"
        );
        assert!(rendered.contains("\"still_present\": 0"), "{rendered}");
        assert!(rendered.contains("another repository root"), "{rendered}");
        assert!(rendered.contains("/other/repo -> ."), "{rendered}");
        Ok(())
    }

    #[test]
    fn baseline_delta_keeps_same_root_fallback_match_comparable() -> Result<(), String> {
        // Same-root entries (and pre-root-preservation entries without a root)
        // stay comparable: the foreign-root refusal only fires on an explicit
        // mismatch.
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"fallback": "src/a.rs:1:weakly_gripped"},
              "path": "src/a.rs",
              "line": 1,
              "static_class": "weakly_gripped",
              "root": "."
            },
            {
              "identity": {"fallback": "src/b.rs:2:weakly_gripped"},
              "path": "src/b.rs",
              "line": 2,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "root": ".",
          "decisions": [
            {"decision": "advisory", "static_class": "weakly_gripped", "placement": {"path": "src/a.rs", "line": 1}, "evidence": {}},
            {"decision": "advisory", "static_class": "weakly_gripped", "placement": {"path": "src/b.rs", "line": 2}, "evidence": {}}
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 2"), "{rendered}");
        assert!(
            rendered.contains("\"stale_baseline_entry\": 0"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"legacy_fallback_match\": 2"),
            "{rendered}"
        );
        assert!(!rendered.contains("another repository root"), "{rendered}");
        Ok(())
    }

    #[test]
    fn baseline_delta_marks_ambiguous_fallback_with_retained_legacy_identity() -> Result<(), String>
    {
        // Issue #1964, fixture 3: two gaps on one line with the same class.
        // No single replacement candidate exists, but the legacy identity is
        // retained and the match stays visible.
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"fallback": "src/ambiguous.rs:7:weakly_gripped"},
              "path": "src/ambiguous.rs",
              "line": 7,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [
            {"decision": "advisory", "static_class": "weakly_gripped", "placement": {"path": "src/ambiguous.rs", "line": 7}, "evidence": {}},
            {"decision": "advisory", "static_class": "weakly_gripped", "placement": {"path": "src/ambiguous.rs", "line": 7}, "evidence": {}}
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(
            rendered.contains("\"stale_baseline_entry\": 1"),
            "{rendered}"
        );
        assert!(
            rendered.contains("matched 2 current decisions by fallback"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"baseline_match_kind\": \"legacy_path_line_class\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"stale_baseline_warning\": true"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"matched_legacy_identity\": \"src/ambiguous.rs:7:weakly_gripped\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"canonical_replacement_candidate\": null"),
            "{rendered}"
        );
        Ok(())
    }

    #[test]
    fn baseline_delta_canonical_match_carries_no_legacy_disclosure() -> Result<(), String> {
        // Canonical authority must render exactly as before the disclosure
        // existed: no match-kind key, no warning flag, no legacy fields.
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"canonical_gap_id": "gap::stable"},
              "path": "src/moved.rs",
              "line": 10,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [
            {
              "decision": "advisory",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/moved.rs", "line": 44},
              "evidence_record": {"canonical_gap_id": "gap::stable"},
              "evidence": {}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 1"), "{rendered}");
        assert!(
            rendered.contains("\"legacy_fallback_match\": 0"),
            "{rendered}"
        );
        assert!(!rendered.contains("baseline_match_kind"), "{rendered}");
        assert!(!rendered.contains("stale_baseline_warning"), "{rendered}");
        assert!(!rendered.contains("matched_legacy_identity"), "{rendered}");
        assert!(
            !rendered.contains("canonical_replacement_candidate"),
            "{rendered}"
        );
        let markdown = render_baseline_delta_markdown(&report);
        assert!(!markdown.contains("Legacy fallback matches"), "{markdown}");
        Ok(())
    }

    #[test]
    fn baseline_delta_treats_equivalent_root_spellings_as_same_repository() -> Result<(), String> {
        // Issue #1964, review: `.`, `./`, and `.\\` name the same checkout
        // after normalization, so the cross-root refusal must not fire on
        // spelling alone.
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"fallback": "src/a.rs:1:weakly_gripped"},
              "path": "src/a.rs",
              "line": 1,
              "static_class": "weakly_gripped",
              "root": "./"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "root": ".",
          "decisions": [
            {"decision": "advisory", "static_class": "weakly_gripped", "placement": {"path": "src/a.rs", "line": 1}, "evidence": {}}
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 1"), "{rendered}");
        assert!(
            rendered.contains("\"stale_baseline_entry\": 0"),
            "{rendered}"
        );
        assert!(!rendered.contains("another repository root"), "{rendered}");
        Ok(())
    }

    #[test]
    fn baseline_delta_warns_on_already_joined_fallback_match() -> Result<(), String> {
        // Issue #1964, review: the second entry sharing one fallback identity
        // is counted as a legacy match AND warned, so warning consumers see
        // the same compatibility events the counter reports.
        let baseline = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"fallback": "src/dup.rs:7:weakly_gripped"},
              "path": "src/dup_a.rs",
              "line": 7,
              "static_class": "weakly_gripped"
            },
            {
              "identity": {"fallback": "src/dup.rs:7:weakly_gripped"},
              "path": "src/dup_b.rs",
              "line": 8,
              "static_class": "weakly_gripped"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [
            {"decision": "advisory", "static_class": "weakly_gripped", "placement": {"path": "src/dup.rs", "line": 7}, "evidence": {}}
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 1"), "{rendered}");
        assert!(
            rendered.contains("\"stale_baseline_entry\": 1"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"legacy_fallback_match\": 2"),
            "{rendered}"
        );
        assert!(
            rendered.contains("also matched an already joined current decision by fallback"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"baseline_match_kind\": \"legacy_path_line_class\""),
            "{rendered}"
        );
        Ok(())
    }

    #[test]
    fn baseline_delta_markdown_lists_every_legacy_match() -> Result<(), String> {
        // Issue #1964, review: the human compatibility section is exhaustive.
        // Twelve legacy joins list twelve headlines, not ten plus silence.
        let mut entries = Vec::new();
        let mut decisions = Vec::new();
        for index in 0..12 {
            entries.push(format!(
                "{{\"identity\": {{\"fallback\": \"src/legacy{index}.rs:7:weakly_gripped\"}}, \"path\": \"src/legacy{index}.rs\", \"line\": 7, \"static_class\": \"weakly_gripped\"}}"
            ));
            decisions.push(format!(
                "{{\"decision\": \"advisory\", \"static_class\": \"weakly_gripped\", \"placement\": {{\"path\": \"src/legacy{index}.rs\", \"line\": 7}}, \"evidence\": {{}}}}"
            ));
        }
        let baseline = format!(
            "{{\"schema_version\": \"0.1\", \"entries\": [{}]}}",
            entries.join(",")
        );
        let current = format!(
            "{{\"schema_version\": \"0.1\", \"decisions\": [{}]}}",
            decisions.join(",")
        );

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline),
            current_gate_decision_json: Ok(current),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 12"), "{rendered}");
        assert!(
            rendered.contains("\"legacy_fallback_match\": 12"),
            "{rendered}"
        );
        let markdown = render_baseline_delta_markdown(&report);
        assert!(
            markdown.contains("Legacy fallback matches (12 total):"),
            "{markdown}"
        );
        for index in 0..12 {
            assert!(
                markdown.contains(&format!("src/legacy{index}.rs:7")),
                "{markdown}"
            );
        }
        Ok(())
    }

    #[test]
    fn baseline_delta_propagates_current_run_limitations() -> Result<(), String> {
        // Issue #6257: a current side disclosing a limited run must surface
        // verbatim in the delta, in the shared vocabulary positions the
        // zero-status partial-denominator guard (#5251/#6095) reads. The
        // guard's own predicates are the oracle: no reimplemented matching.
        // Shape note: production gate decisions carry no limitation
        // envelope (a limited input is refused as config_error), so this
        // decisions-plus-envelope document is hand-supplied; the
        // production-live disclosure is the propagated gate status below.
        let baseline = r#"{"schema_version": "0.1", "entries": []}"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [],
          "analysis_scope": {
            "scope": "diff",
            "run_status": "limited_partial_scope",
            "basis": "rust_partial_diff_budget",
            "downstream_consumable": false
          },
          "run_limitations": [
            {
              "category": "limited_findings_bound",
              "run_status": "limited_findings_bound",
              "basis": "check_findings_bytes",
              "downstream_consumable": false,
              "message": "findings prefix rendered",
              "repair_route": "analysis/check-findings-bytes"
            }
          ],
          "analysis_outcome": {
            "analysis_complete": false,
            "outcome": {"kind": "diff_scope_oversized"}
          }
        }"#;
        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        let value: Value = serde_json::from_str(&rendered)
            .map_err(|err| format!("parse self-produced delta: {err}"))?;
        assert!(
            crate::output::gate::discloses_limited_partial_scope(&value),
            "{rendered}"
        );
        assert!(
            crate::output::gate::discloses_limited_findings_bound(&value),
            "{rendered}"
        );
        assert!(
            crate::output::gate::discloses_incomplete_analysis_outcome(&value),
            "{rendered}"
        );
        assert_eq!(
            value["analysis_scope"]["run_status"],
            "limited_partial_scope"
        );
        assert_eq!(
            value["run_limitations"][0]["run_status"],
            "limited_findings_bound"
        );
        assert_eq!(value["analysis_outcome"]["analysis_complete"], false);
        let markdown = render_baseline_delta_markdown(&report);
        assert!(markdown.contains("limited_partial_scope"), "{markdown}");
        assert!(markdown.contains("limited_findings_bound"), "{markdown}");
        assert!(markdown.contains("diff_scope_oversized"), "{markdown}");
        Ok(())
    }

    #[test]
    fn baseline_delta_markdown_renders_document_complete_line_only_when_bounded() {
        // #9010: the propagated bounded-document disclosure is visible in the
        // human surface; the unbounded control carries no such line.
        let render = |outcome: &str| {
            let current = format!(
                r#"{{"schema_version": "0.1", "decisions": [], "analysis_outcome": {outcome}}}"#
            );
            render_baseline_delta_markdown(&build_baseline_delta_report(BaselineDeltaInput {
                root: ".".to_string(),
                baseline_path: "baseline.json".to_string(),
                current_gate_decision_path: "current.json".to_string(),
                baseline_json: Ok(r#"{"schema_version": "0.1", "entries": []}"#.to_string()),
                current_gate_decision_json: Ok(current),
            }))
        };
        let bounded = render(
            r#"{"analysis_complete": true, "document_complete": false, "outcome": {"kind": "complete_with_findings"}}"#,
        );
        assert!(bounded.contains("Analysis complete: `true`"), "{bounded}");
        assert!(bounded.contains("Document complete: `false`"), "{bounded}");
        let control =
            render(r#"{"analysis_complete": true, "outcome": {"kind": "complete_with_findings"}}"#);
        assert!(!control.contains("Document complete"), "{control}");
    }

    #[test]
    fn zero_status_withholds_achieved_over_self_produced_limited_delta() -> Result<(), String> {
        // Issue #6257: the existing #5251/#6095 guard becomes reachable on
        // self-produced deltas. An all-clear delta built from a limited
        // current run withholds `achieved`: the denominator is partial, so
        // the state is unknown, never achieved.
        let baseline = r#"{"schema_version": "0.1", "entries": []}"#;
        let current = r#"{
          "schema_version": "0.1",
          "decisions": [],
          "run_limitations": [
            {
              "category": "limited_findings_bound",
              "run_status": "limited_findings_bound",
              "basis": "check_findings_bytes",
              "downstream_consumable": false,
              "message": "findings prefix rendered",
              "repair_route": "analysis/check-findings-bytes"
            }
          ]
        }"#;
        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 0"), "{rendered}");
        let status = crate::output::ripr_zero_status::build_ripr_zero_status_report(
            crate::output::ripr_zero_status::RiprZeroStatusInput {
                root: ".".to_string(),
                generated_at: "unix_ms:100000000".to_string(),
                baseline_path: None,
                delta_path: "delta.json".to_string(),
                gap_ledger_path: None,
                gate_path: None,
                pr_guidance_path: None,
                recommendation_calibration_path: None,
                baseline_json: None,
                delta_json: Ok(rendered),
                gap_ledger_json: None,
                gate_json: None,
                pr_guidance_json: None,
                recommendation_calibration_json: None,
            },
        );
        let status_rendered =
            crate::output::ripr_zero_status::render_ripr_zero_status_json(&status)?;
        let status_value: Value = serde_json::from_str(&status_rendered)
            .map_err(|err| format!("parse zero status: {err}"))?;
        assert_eq!(status_value["status"], "incomplete");
        assert_eq!(status_value["ripr_zero"]["state"], "unknown");
        assert!(
            status_rendered.contains("bounded denominator can never yield achieved"),
            "{status_rendered}"
        );
        Ok(())
    }

    #[test]
    fn baseline_delta_propagates_config_error_gate_status() -> Result<(), String> {
        // Issue #6257 review: the current side is gate-decision shaped, and
        // the gate refuses limited inputs as config_error with empty
        // decisions rather than carrying a limitation envelope. A delta
        // from a config_error evaluation must carry the status verbatim, or
        // its empty counts present a complete denominator. The current document
        // below mirrors render_gate_decision_json (status, config_errors,
        // decisions; no limitation fields): the production-live shape.
        let baseline = r#"{"schema_version": "0.1", "entries": []}"#;
        let current = r#"{
          "schema_version": "0.1",
          "tool": "ripr",
          "status": "config_error",
          "mode": "ci",
          "root": ".",
          "decisions": [],
          "warnings": [],
          "config_errors": ["gate input check.json discloses a limited_partial_scope producer run"],
          "limits_note": "Advisory."
        }"#;
        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 0"), "{rendered}");
        assert!(
            rendered.contains("\"current_gate_status\": \"config_error\""),
            "{rendered}"
        );
        let markdown = render_baseline_delta_markdown(&report);
        assert!(
            markdown.contains("Current gate status: `config_error`"),
            "{markdown}"
        );
        // Control: a successful evaluation echoes its status in JSON and
        // stays undisclosed in Markdown (failure-only human surface).
        let passing = current.replace("\"config_error\"", "\"advisory\"");
        let passing_report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(passing),
        });
        let passing_rendered = render_baseline_delta_json(&passing_report)?;
        assert!(
            passing_rendered.contains("\"current_gate_status\": \"advisory\""),
            "{passing_rendered}"
        );
        let passing_markdown = render_baseline_delta_markdown(&passing_report);
        assert!(
            !passing_markdown.contains("Current gate status:"),
            "{passing_markdown}"
        );
        Ok(())
    }

    #[test]
    fn zero_status_withholds_achieved_over_config_error_gate_delta() -> Result<(), String> {
        // Issue #6257 review, end to end: a delta-only zero-status
        // invocation over a delta built from a config_error gate evaluation
        // withholds achieved. Without the propagated status the empty
        // counts would read all-clear.
        let baseline = r#"{"schema_version": "0.1", "entries": []}"#;
        let current = r#"{
          "schema_version": "0.1",
          "tool": "ripr",
          "status": "config_error",
          "mode": "ci",
          "root": ".",
          "decisions": [],
          "warnings": [],
          "config_errors": ["missing required PR guidance file"],
          "limits_note": "Advisory."
        }"#;
        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"still_present\": 0"), "{rendered}");
        let status = crate::output::ripr_zero_status::build_ripr_zero_status_report(
            crate::output::ripr_zero_status::RiprZeroStatusInput {
                root: ".".to_string(),
                generated_at: "unix_ms:100000000".to_string(),
                baseline_path: None,
                delta_path: "delta.json".to_string(),
                gap_ledger_path: None,
                gate_path: None,
                pr_guidance_path: None,
                recommendation_calibration_path: None,
                baseline_json: None,
                delta_json: Ok(rendered),
                gap_ledger_json: None,
                gate_json: None,
                pr_guidance_json: None,
                recommendation_calibration_json: None,
            },
        );
        let status_rendered =
            crate::output::ripr_zero_status::render_ripr_zero_status_json(&status)?;
        let status_value: Value = serde_json::from_str(&status_rendered)
            .map_err(|err| format!("parse zero status: {err}"))?;
        assert_eq!(status_value["status"], "incomplete");
        assert_eq!(status_value["ripr_zero"]["state"], "unknown");
        assert!(
            status_rendered.contains("failed evaluation can never yield achieved"),
            "{status_rendered}"
        );
        Ok(())
    }

    #[test]
    fn baseline_delta_omits_limitation_disclosure_for_complete_runs() -> Result<(), String> {
        // Issue #6257, happy path: a current side without limitation
        // disclosures yields a delta without the disclosure positions — no
        // new noise on complete runs.
        let baseline = r#"{"schema_version": "0.1", "entries": []}"#;
        let current = r#"{"schema_version": "0.1", "decisions": []}"#;
        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(!rendered.contains("run_limitations"), "{rendered}");
        assert!(!rendered.contains("analysis_outcome"), "{rendered}");
        assert!(!rendered.contains("analysis_scope"), "{rendered}");
        let value: Value = serde_json::from_str(&rendered)
            .map_err(|err| format!("parse self-produced delta: {err}"))?;
        assert!(!crate::output::gate::discloses_limited_partial_scope(
            &value
        ));
        assert!(!crate::output::gate::discloses_limited_findings_bound(
            &value
        ));
        assert!(!crate::output::gate::discloses_incomplete_analysis_outcome(
            &value
        ));
        let markdown = render_baseline_delta_markdown(&report);
        assert!(
            !markdown.contains("propagated from the current run"),
            "{markdown}"
        );
        assert!(!markdown.contains("Analysis complete:"), "{markdown}");
        Ok(())
    }

    #[test]
    fn baseline_delta_discloses_analyzer_version_identity_discontinuity_as_stale()
    -> Result<(), String> {
        // Issue #7421 exact repro shape: a baseline written by a build that
        // predated subject.analyzer_version persistence (the pre-#7250
        // transition) diffed against current evidence from a different,
        // versioned build. The renamed seam identity finds no current match;
        // the old behavior silently counted it `resolved` and zero status
        // read "achieved". The identity loss must be disclosed instead.
        let baseline = r#"{
          "schema_version": "0.1",
          "kind": "gate_baseline",
          "entries": [
            {
              "identity": {
                "canonical_gap_id": "gap:19fe16a568d1c87e",
                "seam_id": "98cec8f197508a31",
                "fallback": "src/lib.rs:10:activation_unknown"
              },
              "path": "src/lib.rs",
              "line": 10,
              "static_class": "activation_unknown",
              "decision": "advisory"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "subject": {
            "analyzer_version": "0.11.0+9c88b96acd0f51b91c246de419946ca8692f4f8b"
          },
          "decisions": [
            {
              "decision": "advisory",
              "canonical_gap_id": "gap:rust:src/lib.rs:shipping_cost:predicate_boundary:predicate:weight_grams==1_000",
              "seam_id": "98d24ff19753a56d",
              "static_class": "no_static_path",
              "placement": {"path": "src/lib.rs", "line": 10},
              "evidence": {}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        assert!(rendered.contains("\"resolved\": 0"), "{rendered}");
        assert!(
            rendered.contains("\"stale_baseline_entry\": 1"),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"bucket\": \"stale_baseline_entry\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"analyzer_version_discontinuity\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"baseline_analyzer_version\": null"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "\"current_analyzer_version\": \"0.11.0+9c88b96acd0f51b91c246de419946ca8692f4f8b\""
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("stale identity discontinuity, not resolved debt"),
            "{rendered}"
        );
        assert!(
            rendered.contains("baseline `none recorded` -> current `0.11.0+9c88b96acd0f51b91c246de419946ca8692f4f8b`"),
            "{rendered}"
        );
        let markdown = render_baseline_delta_markdown(&report);
        assert!(
            markdown.contains("Analyzer-version identity discontinuities (1 total):"),
            "{markdown}"
        );
        assert!(
            markdown.contains("Baseline build: none recorded"),
            "{markdown}"
        );
        assert!(
            markdown.contains("Current build: 0.11.0+9c88b96acd0f51b91c246de419946ca8692f4f8b"),
            "{markdown}"
        );
        assert!(
            markdown.contains("re-create the baseline with the current build"),
            "{markdown}"
        );
        Ok(())
    }

    #[test]
    fn baseline_delta_discloses_version_crossing_when_both_sides_recorded() -> Result<(), String> {
        // Issue #7421: once baselines persist their producing build, a
        // recorded version mismatch is disclosed with both versions — and a
        // canonical identity that persisted across the upgrade still joins
        // normally, so the disclosure never over-reaches into successful
        // matches.
        let baseline = r#"{
          "schema_version": "0.1",
          "subject": {
            "analyzer_version": "0.11.0+a079b4d61983f4f199cd1895af581a3495847093"
          },
          "entries": [
            {
              "identity": {
                "canonical_gap_id": "gap:surviving",
                "seam_id": "old-seam"
              },
              "path": "src/kept.rs",
              "line": 2,
              "static_class": "weakly_gripped",
              "decision": "advisory"
            },
            {
              "identity": {
                "canonical_gap_id": "gap:renamed",
                "seam_id": "old-renamed-seam"
              },
              "path": "src/renamed.rs",
              "line": 5,
              "static_class": "activation_unknown",
              "decision": "advisory"
            }
          ]
        }"#;
        let current = r#"{
          "schema_version": "0.1",
          "subject": {
            "analyzer_version": "0.11.0+9c88b96acd0f51b91c246de419946ca8692f4f8b"
          },
          "decisions": [
            {
              "decision": "advisory",
              "canonical_gap_id": "gap:surviving",
              "seam_id": "new-seam",
              "static_class": "weakly_gripped",
              "placement": {"path": "src/kept.rs", "line": 2},
              "evidence": {}
            }
          ]
        }"#;

        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline.to_string()),
            current_gate_decision_json: Ok(current.to_string()),
        });
        let rendered = render_baseline_delta_json(&report)?;
        // The surviving canonical id still joins across the version boundary.
        assert!(rendered.contains("\"still_present\": 1"), "{rendered}");
        assert!(rendered.contains("\"resolved\": 0"), "{rendered}");
        assert!(
            rendered.contains("\"stale_baseline_entry\": 1"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "\"baseline_analyzer_version\": \"0.11.0+a079b4d61983f4f199cd1895af581a3495847093\""
            ),
            "{rendered}"
        );
        assert!(!rendered.contains("baseline_match_kind"), "{rendered}");
        Ok(())
    }

    #[test]
    fn baseline_delta_keeps_same_or_unversioned_builds_resolved() -> Result<(), String> {
        // Issue #7421 control: without evidence that the producing build
        // changed — both sides unversioned (the old-vs-old world) or both
        // sides recording the same build — an absent baseline identity stays
        // `resolved`. The discontinuity disclosure fires only across a
        // provable build boundary.
        let baseline_body = r#"{
          "schema_version": "0.1",
          "entries": [
            {
              "identity": {"canonical_gap_id": "gap:fixed"},
              "path": "src/fixed.rs",
              "line": 3,
              "static_class": "weakly_gripped",
              "decision": "advisory"
            }
          ]
        }"#;
        let baseline_with_version = baseline_body.replace(
            "\"entries\":",
            "\"subject\": {\"analyzer_version\": \"0.11.0+same\"},\n  \"entries\":",
        );
        assert!(baseline_with_version.contains("0.11.0+same"));
        let unversioned_current = r#"{
          "schema_version": "0.1",
          "analysis_scope": {"scope": "diff_scoped_changed_files", "changed_production_files": ["src/fixed.rs"], "cap_omitted_seams": 0, "omitted_by_policy": 0},
          "decisions": []
        }"#;
        let versioned_current = r#"{
          "schema_version": "0.1",
          "subject": {"analyzer_version": "0.11.0+same"},
          "analysis_scope": {"scope": "diff_scoped_changed_files", "changed_production_files": ["src/fixed.rs"], "cap_omitted_seams": 0, "omitted_by_policy": 0},
          "decisions": []
        }"#;

        let unversioned_report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline_body.to_string()),
            current_gate_decision_json: Ok(unversioned_current.to_string()),
        });
        let rendered = render_baseline_delta_json(&unversioned_report)?;
        assert!(rendered.contains("\"resolved\": 1"), "{rendered}");
        assert!(
            rendered.contains("\"stale_baseline_entry\": 0"),
            "{rendered}"
        );
        assert!(
            !rendered.contains("analyzer_version_discontinuity"),
            "{rendered}"
        );

        let same_version_report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline_with_version.clone()),
            current_gate_decision_json: Ok(versioned_current.to_string()),
        });
        let rendered = render_baseline_delta_json(&same_version_report)?;
        assert!(rendered.contains("\"resolved\": 1"), "{rendered}");
        assert!(
            rendered.contains("\"stale_baseline_entry\": 0"),
            "{rendered}"
        );
        assert!(
            !rendered.contains("analyzer_version_discontinuity"),
            "{rendered}"
        );

        // The same baseline against a genuinely different recorded build is
        // disclosed, keeping this control honest.
        let other_version_current = versioned_current.replace("0.11.0+same", "0.11.0+other");
        let crossing_report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(baseline_with_version),
            current_gate_decision_json: Ok(other_version_current),
        });
        let rendered = render_baseline_delta_json(&crossing_report)?;
        assert!(rendered.contains("\"resolved\": 0"), "{rendered}");
        assert!(
            rendered.contains("\"stale_baseline_entry\": 1"),
            "{rendered}"
        );
        Ok(())
    }

    fn scoped_gate_decision(changed_files: &[&str]) -> String {
        serde_json::json!({
            "schema_version": "0.1",
            "decisions": [],
            "analysis_scope": {
                "scope": "diff_scoped_changed_files",
                "run_status": "limited_diff_scope",
                "changed_files": changed_files,
                "changed_production_files": changed_files,
                "cap_omitted_seams": 0,
                "omitted_by_policy": 0,
            },
        })
        .to_string()
    }

    fn two_gap_baseline() -> &'static str {
        r#"{
          "schema_version": "0.1",
          "kind": "gate_baseline",
          "entries": [
            {"identity": {"canonical_gap_id": "gap:fixed"}, "path": "src/fixed.rs", "line": 3, "static_class": "weakly_gripped", "decision": "advisory"},
            {"identity": {"canonical_gap_id": "gap:elsewhere"}, "path": "src/elsewhere.rs", "line": 9, "static_class": "weakly_gripped", "decision": "advisory"}
          ]
        }"#
    }

    #[test]
    fn baseline_delta_and_update_keep_out_of_scope_gaps_not_evaluated() -> Result<(), String> {
        // Issue #9301 F2: a diff-scoped gate decision lists only gaps in the
        // analyzed files. Positive control: `src/fixed.rs` is inside the
        // scope and absent, so it is genuinely `resolved` and removable.
        // Discriminating negative: `src/elsewhere.rs` is outside the scope,
        // so its absence is `not_evaluated` (never resolved) and
        // `--remove-resolved` must keep the live debt.
        let current = scoped_gate_decision(&["src/fixed.rs"]);
        let report = build_baseline_delta_report(BaselineDeltaInput {
            root: ".".to_string(),
            baseline_path: "baseline.json".to_string(),
            current_gate_decision_path: "current.json".to_string(),
            baseline_json: Ok(two_gap_baseline().to_string()),
            current_gate_decision_json: Ok(current.clone()),
        });
        let rendered: Value = serde_json::from_str(&render_baseline_delta_json(&report)?)
            .map_err(|err| err.to_string())?;
        assert_eq!(rendered["delta"]["resolved"], 1, "{rendered}");
        assert_eq!(rendered["delta"]["stale_baseline_entry"], 1, "{rendered}");
        let items = rendered["items"].as_array().ok_or("items missing")?;
        let resolved = items
            .iter()
            .find(|item| item["bucket"] == "resolved")
            .ok_or("no resolved item")?;
        assert_eq!(resolved["path"], "src/fixed.rs");
        assert!(resolved.get("evaluation").is_none(), "{resolved}");
        let skipped = items
            .iter()
            .find(|item| item["path"] == "src/elsewhere.rs")
            .ok_or("no elsewhere item")?;
        assert_eq!(skipped["bucket"], "stale_baseline_entry", "{skipped}");
        assert_eq!(skipped["evaluation"], "not_evaluated", "{skipped}");
        assert!(
            skipped["not_evaluated_reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("outside the files")),
            "{skipped}"
        );

        let update = crate::output::baseline_update::build_baseline_update_remove_resolved(
            crate::output::baseline_update::BaselineUpdateInput {
                baseline_path: "baseline.json".to_string(),
                current_gate_decision_path: "current.json".to_string(),
                baseline_json: two_gap_baseline().to_string(),
                current_gate_decision_json: current,
                remove_resolved: true,
                migrate_legacy_identities: false,
            },
        )?;
        let updated = crate::output::baseline_update::render_baseline_update_json(&update)?;
        assert!(updated.contains("gap:elsewhere"), "{updated}");
        assert!(!updated.contains("gap:fixed"), "{updated}");
        assert_eq!(
            crate::output::baseline_update::baseline_update_removed_resolved_count(&update),
            1
        );
        Ok(())
    }

    #[test]
    fn baseline_delta_unrelated_diff_resolves_nothing_and_missing_scope_fails_closed()
    -> Result<(), String> {
        // The audit repro: a diff touching only unrelated files.
        for current in [
            scoped_gate_decision(&["tests/unrelated.rs"]),
            // A legacy decision with no scope disclosure fails closed too.
            r#"{"schema_version": "0.1", "decisions": []}"#.to_string(),
        ] {
            let report = build_baseline_delta_report(BaselineDeltaInput {
                root: ".".to_string(),
                baseline_path: "baseline.json".to_string(),
                current_gate_decision_path: "current.json".to_string(),
                baseline_json: Ok(two_gap_baseline().to_string()),
                current_gate_decision_json: Ok(current.clone()),
            });
            let rendered: Value = serde_json::from_str(&render_baseline_delta_json(&report)?)
                .map_err(|err| err.to_string())?;
            assert_eq!(rendered["delta"]["resolved"], 0, "{rendered}");
            assert_eq!(rendered["delta"]["stale_baseline_entry"], 2, "{rendered}");
            let update = crate::output::baseline_update::build_baseline_update_remove_resolved(
                crate::output::baseline_update::BaselineUpdateInput {
                    baseline_path: "baseline.json".to_string(),
                    current_gate_decision_path: "current.json".to_string(),
                    baseline_json: two_gap_baseline().to_string(),
                    current_gate_decision_json: current,
                    remove_resolved: true,
                    migrate_legacy_identities: false,
                },
            )?;
            assert_eq!(
                crate::output::baseline_update::baseline_update_after_entry_count(&update),
                2
            );
        }
        Ok(())
    }

    #[test]
    fn absence_predicate_covers_scope_edge_cases() {
        let scope = |extra: Value| {
            let mut base = json!({
                "scope": "diff_scoped_changed_files",
                "changed_files": ["src/a.rs", "src/a.py", "README.md"],
                "changed_production_files": ["src/a.rs"],
                "cap_omitted_seams": 0,
                "omitted_by_policy": 0,
            });
            if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
                for (k, v) in e {
                    b.insert(k.clone(), v.clone());
                }
            }
            base
        };
        let reason = |s: &Value, path: Option<&str>| {
            absence_not_evaluated_reason(&PreparedAnalysisScope::new(Some(s)), path)
        };
        // Covered: plain, `./`, and backslash spellings normalize.
        assert!(reason(&scope(json!({})), Some("src/a.rs")).is_none());
        assert!(reason(&scope(json!({})), Some("./src/a.rs")).is_none());
        assert!(reason(&scope(json!({})), Some("src\\a.rs")).is_none());
        assert!(
            reason(
                &scope(json!({"changed_production_files": ["./src/a.rs"]})),
                Some("src/a.rs")
            )
            .is_none()
        );
        // changed_production_files alone is enough for coverage.
        assert!(
            reason(
                &scope(json!({"changed_files": [], "changed_production_files": ["src/b.rs"]})),
                Some("src/b.rs")
            )
            .is_none()
        );
        // Non-Rust changed files are listed in `changed_files` but were never
        // evaluated by the Rust-seam analysis: not covered (#9301).
        assert!(reason(&scope(json!({})), Some("src/a.py")).is_some());
        assert!(reason(&scope(json!({})), Some("README.md")).is_some());
        // `changed_files` alone is not a coverage claim.
        assert!(
            reason(
                &json!({"scope": "diff_scoped_changed_files", "changed_files": ["src/a.rs"], "cap_omitted_seams": 0, "omitted_by_policy": 0}),
                Some("src/a.rs")
            )
            .is_some()
        );
        // Missing or nonzero producer-drop counters fail closed.
        for key in ["cap_omitted_seams", "omitted_by_policy"] {
            let mut missing = scope(json!({}));
            if let Some(object) = missing.as_object_mut() {
                object.remove(key);
            }
            assert!(reason(&missing, Some("src/a.rs")).is_some(), "{key}");
            assert!(reason(&scope(json!({ key: 1 })), Some("src/a.rs")).is_some());
            assert!(reason(&scope(json!({ key: "0" })), Some("src/a.rs")).is_some());
        }
        // Negatives.
        assert!(reason(&scope(json!({})), Some("src/other.rs")).is_some());
        assert!(reason(&scope(json!({})), None).is_some());
        assert!(reason(&scope(json!({})), Some("")).is_some());
        assert!(reason(&scope(json!({"scope": "mystery"})), Some("src/a.rs")).is_some());
        assert!(reason(&scope(json!({"scope": "working_set"})), Some("src/a.rs")).is_some());
        let ledger = reason(
            &scope(json!({"scope": "gap_ledger_artifact"})),
            Some("src/a.rs"),
        );
        assert!(ledger.is_some_and(|r| r.contains("#9599")));
        // A refused workspace coverage names the differing classes, including
        // the non-input labels; an empty or absent list adds nothing.
        for classes in [
            json!(["config", "membership"]),
            json!(["records_or_disclosure"]),
            json!(["config_unloadable"]),
        ] {
            let expected = classes
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            let refused = reason(
                &scope(json!({"scope": "gap_ledger_artifact", "coverage_refused": classes})),
                Some("src/a.rs"),
            );
            assert!(
                refused.is_some_and(|r| r.contains(&format!("differing: {expected})"))),
                "{expected}"
            );
        }
        for none in [json!([]), json!("config"), json!([7])] {
            let plain = reason(
                &scope(json!({"scope": "gap_ledger_artifact", "coverage_refused": none})),
                Some("src/a.rs"),
            );
            assert!(plain.is_some_and(|r| !r.contains("differing:")));
        }
        assert!(reason(&scope(json!({"unevaluated_seams": 2})), Some("src/a.rs")).is_some());
        assert!(reason(&scope(json!({"unevaluated_seams": 0})), Some("src/a.rs")).is_none());
        assert!(reason(&scope(json!({"unevaluated_seams": "1"})), Some("src/a.rs")).is_some());
        assert!(reason(&scope(json!({"cap_omitted_seams": 1})), Some("src/a.rs")).is_some());
        assert!(
            reason(
                &scope(json!({"absent_changed_files": ["src/a.rs"]})),
                Some("src/a.rs")
            )
            .is_some()
        );
        assert!(
            reason(
                &scope(json!({"absent_changed_files": "src/a.rs"})),
                Some("src/a.rs")
            )
            .is_some()
        );
        assert!(
            reason(
                &scope(json!({"changed_production_files": "src/a.rs"})),
                Some("src/a.rs")
            )
            .is_some()
        );
        // A parser-declined file is not coverage even if listed elsewhere;
        // malformed disclosure fails closed; absence of the key reads as before.
        assert!(
            reason(
                &scope(json!({"parser_declined_files": ["src/a.rs"]})),
                Some("src/a.rs")
            )
            .is_some_and(|r| r.contains("not parsed"))
        );
        assert!(
            reason(
                &scope(json!({"parser_declined_files": ["src/other.rs"]})),
                Some("src/a.rs")
            )
            .is_none()
        );
        assert!(
            reason(
                &scope(json!({"parser_declined_files": [7]})),
                Some("src/a.rs")
            )
            .is_some()
        );
        assert!(
            reason(
                &scope(json!({"parser_declined_files": "src/a.rs"})),
                Some("src/a.rs")
            )
            .is_some()
        );
        // Non-string entry in absent_changed_files fails closed; a scope
        // object with no `scope` key is unrecognized.
        assert!(
            reason(
                &scope(json!({"absent_changed_files": [7]})),
                Some("src/a.rs")
            )
            .is_some()
        );
        let mut no_kind = scope(json!({}));
        if let Some(object) = no_kind.as_object_mut() {
            object.remove("scope");
        }
        assert!(reason(&no_kind, Some("src/a.rs")).is_some_and(|r| r.contains("unknown")));
        // Complete whole-Rust-workspace coverage from the ledger path (#9599):
        // Rust paths are covered, non-Rust and malformed disclosures are not.
        let workspace = json!({
            "scope": "rust_workspace_complete",
            "run_status": "complete",
            "basis": "repo_exposure_complete_run",
            "corpus": {
                "scope": "rust_workspace",
                "cache_schema_version": "1.93",
                "files_content_hash": "0123456789abcdef",
                "analyzed_production_files": [
                    "src/anywhere/new.rs", "crates/a/src/lib.rs", "src/target.rs"
                ],
                "analysis_inputs": {"cfg_features_hash": "a", "config_hash": "b", "test_intent_hash": "c", "suppressions_hash": "d", "workspace_manifests_hash": "e", "lockfile_hash": "f", "toolchain_hash": "g", "analyzer_version": "h"},
            },
            "records_total": 1,
            "records_digest": format!("sha256:{}", "0".repeat(64)),
        });
        assert!(reason(&workspace, Some("src/anywhere/new.rs")).is_none());
        assert!(reason(&workspace, Some("crates/a/src/lib.rs")).is_none());
        assert!(reason(&workspace, Some("src/not-admitted.rs")).is_some());
        for files in [
            Value::Null,
            json!([]),
            json!([7]),
            json!(["src/anywhere/new.rs", "src/anywhere/new.rs"]),
            json!(["src/../anywhere/new.rs"]),
        ] {
            let mut malformed = workspace.clone();
            malformed["corpus"]["analyzed_production_files"] = files;
            assert!(reason(&malformed, Some("src/anywhere/new.rs")).is_some());
        }
        let mut missing = workspace.clone();
        if let Some(corpus) = missing["corpus"].as_object_mut() {
            corpus.remove("analyzed_production_files");
        }
        assert!(reason(&missing, Some("src/anywhere/new.rs")).is_some());
        // Spellings that could name a different file than the run saw, and
        // directories the producer's walk never enters, are not covered.
        for odd in [
            "../outside/x.rs",
            "src/../x.rs",
            "/abs/y.rs",
            "vol:/abs/y.rs",
            "vol:\\abs\\y.rs",
            "./src/win.rs",
            "src\\win.rs",
            "src/./a.rs",
            "src//a.rs",
            "src/lib.rs ",
            " src/lib.rs",
            "target/debug/build/gen.rs",
            "crates/a/target/gen.rs",
            "fixtures/case/src/lib.rs",
            "node_modules/pkg/x.rs",
            ".git/hooks/x.rs",
            ".ripr/x.rs",
            "src/",
            "x.rs/",
        ] {
            assert!(reason(&workspace, Some(odd)).is_some(), "{odd:?}");
        }
        // A file *named* like an ignored directory is still a file.
        assert!(reason(&workspace, Some("src/target.rs")).is_none());
        assert!(reason(&workspace, Some("scripts/tool.py")).is_some_and(|r| r.contains("Rust")));
        assert!(reason(&workspace, None).is_some());
        for broken in [
            json!({"run_status": "limited"}),
            json!({"basis": "other"}),
            json!({"corpus": {"scope": "other", "files_content_hash": "x"}}),
            json!({"corpus": {"scope": "rust_workspace"}}),
            json!({"corpus": {"scope": "rust_workspace", "files_content_hash": "0123456789abcdef"}}),
            json!({"corpus": {"scope": "rust_workspace", "cache_schema_version": "1.93", "files_content_hash": "DEADBEEF"}}),
            json!({"records_total": "1"}),
            json!({"records_digest": "deadbeef"}),
            json!({"records_digest": "sha256:00"}),
            json!({"records_digest": format!("sha256:{}", "z".repeat(64))}),
            json!({"corpus": {"scope": "rust_workspace", "files_content_hash": "x", "unavailable": "root"}}),
            json!({"records_digest": 7}),
        ] {
            let mut scope_value = workspace.clone();
            if let (Some(target), Some(patch)) = (scope_value.as_object_mut(), broken.as_object()) {
                for (key, value) in patch {
                    target.insert(key.clone(), value.clone());
                }
            }
            assert!(reason(&scope_value, Some("src/a.rs")).is_some(), "{broken}");
        }
        let missing = json!({"scope": "diff_scoped_changed_files"});
        assert!(reason(&missing, Some("src/a.rs")).is_some());
        // Non-object or absent scope fails closed.
        assert!(reason(&json!("scoped"), Some("src/a.rs")).is_some());
        assert!(reason(&json!([1]), Some("src/a.rs")).is_some());
        assert!(
            absence_not_evaluated_reason(&PreparedAnalysisScope::new(None), Some("src/a.rs"))
                .is_some()
        );
    }

    #[test]
    fn baseline_update_reports_preserved_not_evaluated_entries() -> Result<(), String> {
        let current = scoped_gate_decision(&["src/fixed.rs"]);
        let update = crate::output::baseline_update::build_baseline_update_remove_resolved(
            crate::output::baseline_update::BaselineUpdateInput {
                baseline_path: "baseline.json".to_string(),
                current_gate_decision_path: "current.json".to_string(),
                baseline_json: two_gap_baseline().to_string(),
                current_gate_decision_json: current,
                remove_resolved: true,
                migrate_legacy_identities: false,
            },
        )?;
        assert_eq!(
            crate::output::baseline_update::baseline_update_preserved_stale_count(&update),
            1
        );
        let rendered = crate::output::baseline_update::render_baseline_update_json(&update)?;
        assert!(
            rendered.contains("outside the files the current analysis covered"),
            "{rendered}"
        );
        Ok(())
    }

    fn repo_root() -> Result<PathBuf, String> {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .ok_or_else(|| "CARGO_MANIFEST_DIR did not have a workspace parent".to_string())
    }

    fn read_file(path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path)
            .map_err(|err| format!("read {} failed: {err}", path.display()))
    }

    fn fixture_path(repo_root: &Path, path: &Path) -> String {
        match path.strip_prefix(repo_root) {
            Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
            Err(_) => path.to_string_lossy().replace('\\', "/"),
        }
    }
}
