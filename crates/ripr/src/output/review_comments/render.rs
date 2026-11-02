use crate::agent::command_specs::command_displays_are_complete;
use crate::agent::loop_commands::{
    WORKFLOW_AFTER_SNAPSHOT_ARTIFACT, WORKFLOW_AGENT_BRIEF_ARTIFACT,
    WORKFLOW_AGENT_VERIFY_ARTIFACT, WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT,
    WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT, display_path, portable_agent_brief_command,
    portable_agent_verify_command, portable_check_analysis_outcome_command_with_base, shell_arg,
};
use crate::analysis::ClassifiedSeam;
use crate::analysis::canonical_gap::canonical_gap_identity;
use crate::analysis_outcome::AnalysisOutcome;
use crate::app::Mode;
use crate::app::agent_brief::{
    AgentBriefResolvedWorkingSet, AgentBriefSelectedSeam, AgentBriefSelection,
    AgentBriefWhyNowReason,
};
use crate::app::causal_projection::CausalDeltaArtifact;
use crate::config::RiprConfig;
use crate::output::agent_seam_packets;
use crate::output::agent_seam_packets::missing_discriminator_records_for as missing_records_for;
use crate::output::evidence_record::{
    CROSS_LANGUAGE_TARGET_UNRESOLVED_CATEGORY, CROSS_LANGUAGE_TARGET_UNRESOLVED_REPAIR_ROUTE,
    EvidenceRecordStaticLimitation, NO_TEST_REACHES_OWNER_CATEGORY, actionability_for,
    canonical_receipt_command_for, canonical_repair_command_for,
    cross_language_test_target_unresolved, gap_state_for, static_limitations_for,
};
use crate::output::gap_decision_ledger::{GapRecord, GapRepairRoute};
use crate::output::markdown::{code_span, inline_prose};
use serde_json::{Value, json};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::ReviewPlacement;
use super::placement::{is_test_like_path, normalize_path_text, placement_for, placement_json};
use super::publish_plan::{
    card_gap_state, card_is_actionable, changed_test_paths, gap_record_cap_suppressed_json,
    reason_for, review_warnings_json, suggested_test_intent, suppressed_json,
};
use super::{
    DEFAULT_REVIEW_MAX_INLINE_COMMENTS, DEFAULT_REVIEW_MAX_SUMMARY_ITEMS,
    LLM_PROMPT_VERIFY_SENTENCE, REVIEW_COMMENTS_SCHEMA_VERSION, ReviewCommentsAnalysisScope,
    ReviewCommentsRenderContext, SUMMARY_REASON_INLINE_CAP_REACHED,
    SUMMARY_REASON_NAVIGATION_ONLY_CROSS_LANGUAGE, SUMMARY_REASON_NO_SAFE_PLACEMENT,
};

pub(super) fn render_review_comments_markdown_value(
    root: &Path,
    base: &str,
    head: &str,
    mode: &Mode,
    value: &Value,
) -> String {
    let summary = value.get("summary").and_then(Value::as_object);
    let comments = summary
        .and_then(|summary| summary.get("comments"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let summary_only = summary
        .and_then(|summary| summary.get("summary_only"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let suppressed = summary
        .and_then(|summary| summary.get("suppressed"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let mut lines = vec![
        "# RIPR PR Guidance".to_string(),
        String::new(),
        format!("- root: {}", display_path(root)),
        format!("- base: {base}"),
        format!("- head: {head}"),
        format!("- mode: {}", mode.as_str()),
        format!("- line annotations: {comments}"),
        format!("- summary-only recommendations: {summary_only}"),
        format!("- suppressed recommendations: {suppressed}"),
    ];
    push_analysis_scope_summary(&mut lines, value.get("analysis_scope"));
    push_analysis_outcome_summary(&mut lines, value.get("analysis_outcome"));
    lines.push(String::new());
    lines.push(
        "Advisory static evidence only. RIPR does not edit source, generate tests, run mutation testing, or make CI blocking by default."
            .to_string(),
    );
    if value.get("status").and_then(Value::as_str) == Some("incomplete") {
        lines.push(
            "Incomplete producer input is not a clean or zero-finding result; follow the recovery route before relying on this guidance."
                .to_string(),
        );
    }
    lines.push(String::new());

    push_markdown_items(&mut lines, "Line Annotations", value.get("comments"));
    push_markdown_items(
        &mut lines,
        "Summary-Only Recommendations",
        value.get("summary_only"),
    );
    push_suppressed_items(&mut lines, value.get("suppressed"));
    lines.push(String::new());
    lines.join("\n")
}

pub(super) fn push_analysis_outcome_summary(lines: &mut Vec<String>, outcome: Option<&Value>) {
    let Some(outcome) = outcome else {
        return;
    };
    let analysis_complete = outcome
        .get("analysis_complete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let kind = outcome
        .get("outcome")
        .and_then(|value| value.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    lines.push(format!("- producer analysis outcome: {kind}"));
    lines.push(format!("- producer analysis complete: {analysis_complete}"));
    if let Some(limitations) = outcome
        .get("outcome")
        .and_then(|value| value.get("limitations"))
        .and_then(Value::as_array)
    {
        for limitation in limitations {
            let limitation_kind = limitation
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let recovery = limitation
                .get("recovery")
                .and_then(|value| value.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            lines.push(format!(
                "- producer limitation: {limitation_kind}; recovery: {recovery}"
            ));
        }
    }
}

pub(super) fn gap_record_comment_json(
    root: &Path,
    gap_ledger_path: &str,
    record: &GapRecord,
    seen_dedupe: &mut BTreeSet<String>,
    seam_claimed_dedupe: &BTreeSet<String>,
    causal_projection: Option<&CausalDeltaArtifact>,
) -> Result<Value, Value> {
    // Route through the shared eligibility authority (#3281): a record
    // whose source is not established candidate-current produces no PR-comment
    // obligation even when a stale ledger's map still says eligible. The
    // authority call subsumes the map's own eligible flag.
    let Some(projection) = record.projection_eligibility.get("pr_comment") else {
        return Err(gap_record_suppressed_json(
            record,
            "not_pr_comment_eligible",
            "missing_pr_comment_projection",
        ));
    };
    if !crate::output::gap_decision_ledger::projection_eligible(record, "pr_comment") {
        return Err(gap_record_suppressed_json(
            record,
            "not_pr_comment_eligible",
            &projection.reason,
        ));
    }
    if record.scope != "pr_local" || record.repairability != "repairable" {
        return Err(gap_record_suppressed_json(
            record,
            "not_pr_local_repairable",
            "PR comments require a PR-local repairable gap.",
        ));
    }
    if matches!(
        record.policy_state.as_str(),
        "suppressed" | "waived" | "resolved"
    ) {
        return Err(gap_record_suppressed_json(
            record,
            "policy_state_not_commentable",
            "Suppressed, waived, or resolved gaps are not PR-comment repair cards.",
        ));
    }
    let Some(anchor) = record.anchor.as_ref() else {
        return Err(gap_record_suppressed_json(
            record,
            "missing_anchor",
            "PR comments require a stable anchor.",
        ));
    };
    let Some(file) = anchor
        .file
        .as_deref()
        .map(str::trim)
        .filter(|file| !file.is_empty())
    else {
        return Err(gap_record_suppressed_json(
            record,
            "missing_anchor",
            "PR comments require an anchor file.",
        ));
    };
    let Some(line) = anchor.line else {
        return Err(gap_record_suppressed_json(
            record,
            "missing_anchor",
            "PR comments require an anchor line.",
        ));
    };
    let Some(dedupe) = gap_record_dedupe_key(record) else {
        return Err(gap_record_suppressed_json(
            record,
            "missing_dedupe_fingerprint",
            "PR comments require a dedupe fingerprint.",
        ));
    };
    if file.is_empty() || dedupe.is_empty() {
        return Err(gap_record_suppressed_json(
            record,
            "missing_anchor",
            "PR comments require a non-empty anchor and dedupe fingerprint.",
        ));
    }
    // Seam identity is optional on a gap-record card (#4524): the card is
    // keyed by its GapRecord, and preview-language `--check-output` records
    // carry no seam. A record that has one still wins a shared dedupe key.
    let seam_id = record.seam_id.as_deref().and_then(non_empty);
    if seam_id.is_none() && seam_claimed_dedupe.contains(dedupe) {
        return Err(gap_record_suppressed_json(
            record,
            "duplicate_dedupe_fingerprint",
            "A GapRecord with producer-owned seam identity emits this PR comment dedupe key.",
        ));
    }
    // Every rejection comes before the dedupe insert, so only a card that
    // renders reserves its key.
    let Some(repair_route) = record.repair_route.as_ref() else {
        return Err(gap_record_suppressed_json(
            record,
            "missing_repair_route",
            "PR comments require a repair route.",
        ));
    };
    if !command_displays_are_complete(&record.verification_commands) {
        return Err(gap_record_suppressed_json(
            record,
            "missing_verification_command",
            "PR comments require only nonblank verification commands.",
        ));
    }
    if !seen_dedupe.insert(dedupe.to_string()) {
        return Err(gap_record_suppressed_json(
            record,
            "duplicate_dedupe_fingerprint",
            "A previous GapRecord already emitted this PR comment dedupe key.",
        ));
    }

    let gap_id = gap_record_id(record);
    let repair_text = repair_text(repair_route);
    let why = repair_why(record, repair_route);
    let verify_command = record.verification_commands[0].clone();
    let source_location = source_location_json(file, Some(line as usize));

    let mut value = json!({
        "id": format!("ripr-review-{gap_id}"),
        "source": "gap_decision_ledger",
        "gap_id": gap_id.as_str(),
        "canonical_gap_id": non_empty(&record.canonical_gap_id),
        "gap_kind": record.kind.as_str(),
        "language": record.language.as_str(),
        "language_status": record.language_status.as_str(),
        "gap_state": record.gap_state.as_str(),
        "policy_state": record.policy_state.as_str(),
        "repairability": record.repairability.as_str(),
        "dedupe_key": dedupe,
        "source_location": source_location,
        "placement": {
            "path": file,
            "line": line,
            "side": "RIGHT",
            "mode": "gap_record_anchor",
        },
        "kind": record.kind.as_str(),
        "grip_class": record.evidence_class.as_str(),
        "severity": "warning",
        "reason": why,
        "missing_discriminator": repair_route.changed_behavior.as_deref().or(repair_route.assertion_shape.as_deref()),
        "suggested_test": {
            "intent": repair_intent(&repair_route.route_kind),
            "candidate_values": [],
            "assertion_shape": repair_route.assertion_shape.as_deref(),
            "assertion_kind": repair_route.route_kind.as_str(),
            "recommended_file": repair_route.target_file.as_deref().or(repair_route.related_test.as_deref()),
            "recommended_name": repair_route.related_test.as_deref(),
            "near_test": repair_route.related_test.as_deref(),
            "related_test": gap_record_related_test(repair_route),
        },
        "repair_card": {
            "gap_kind": record.kind.as_str(),
            "changed_behavior": repair_route.changed_behavior.as_deref(),
            "why_this_matters": why,
            "repair": repair_text,
            "repair_route": repair_route,
            "evidence_ids": &record.evidence_ids,
            "verification_commands": &record.verification_commands,
            "verify_command": verify_command,
            "source_artifact": gap_ledger_path,
            "authority_boundary": record.authority_boundary.as_str(),
        },
        "llm_guidance": {
            "prompt": repair_prompt(repair_route, &verify_command),
            "command": format!(
                "ripr first-action --root {} --gap-ledger {}",
                shell_arg(&display_path(root)),
                shell_arg(gap_ledger_path)
            ),
            "verify_command": verify_command,
        },
    });
    if let Some(seam_id) = seam_id
        && let Some(object) = value.as_object_mut()
    {
        object.insert("seam_id".to_string(), json!(seam_id));
    }
    if let Some(projection) = causal_projection
        && let Some(object) = value.as_object_mut()
    {
        projection.insert_delta_fields(object, non_empty(&record.canonical_gap_id).as_deref());
    }
    Ok(value)
}

/// Dedupe keys a seam-carrying record renders a card for, so a seamless
/// record sharing the key yields to it regardless of ledger order.
pub(super) fn seam_claimed_dedupe_keys(
    root: &Path,
    gap_ledger_path: &str,
    records: &[GapRecord],
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let none_claimed = BTreeSet::new();
    records
        .iter()
        .filter(|record| record.seam_id.as_deref().and_then(non_empty).is_some())
        .filter_map(|record| {
            gap_record_comment_json(
                root,
                gap_ledger_path,
                record,
                &mut seen,
                &none_claimed,
                None,
            )
            .ok()
        })
        .filter_map(|card| {
            card.get("dedupe_key")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect()
}

pub(super) fn gap_record_suppressed_json(record: &GapRecord, reason: &str, message: &str) -> Value {
    json!({
        "gap_id": gap_record_id(record),
        "file": record.anchor.as_ref().and_then(|anchor| anchor.file.clone()),
        "line": record.anchor.as_ref().and_then(|anchor| anchor.line),
        "reason": reason,
        "message": message,
    })
}

pub(super) fn gap_record_dedupe_key(record: &GapRecord) -> Option<&str> {
    record
        .anchor
        .as_ref()?
        .dedupe_fingerprint
        .as_deref()
        .map(str::trim)
        .filter(|dedupe| !dedupe.is_empty())
}

pub(crate) fn gap_record_id(record: &GapRecord) -> String {
    non_empty(&record.gap_id)
        .or_else(|| non_empty(&record.canonical_gap_id))
        .unwrap_or_else(|| "unknown-gap".to_string())
}

pub(super) fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

/// Project the structured related-test object `{name, file, line}` from a
/// gap-ledger repair route per RIPR-SPEC-0068. The gap-ledger artifact carries
/// the related test as a single `related_test` string plus optional
/// `target_file` / `target_line`; this projects them into the structured
/// object, or `null` when no related test name resolves. The gap-ledger path
/// may carry a name-only object when the source route has no resolved location.
pub(super) fn gap_record_related_test(route: &GapRepairRoute) -> Value {
    match route.related_test.as_deref() {
        Some(name) => json!({
            "name": name,
            "file": route.target_file.as_deref(),
            "line": route.target_line,
        }),
        None => Value::Null,
    }
}

pub(super) fn repair_text(route: &GapRepairRoute) -> String {
    route
        .assertion_shape
        .clone()
        .or_else(|| route.changed_behavior.clone())
        .unwrap_or_else(|| format!("Follow repair route `{}`.", route.route_kind))
}

pub(super) fn repair_why(record: &GapRecord, route: &GapRepairRoute) -> String {
    if let Some(changed) = route.changed_behavior.as_deref() {
        return format!(
            "Changed behavior {} has a repairable {} gap.",
            code_span(changed),
            record.kind
        );
    }
    if let Some(assertion) = route.assertion_shape.as_deref() {
        return format!(
            "This PR-local {} gap needs a focused repair: {assertion}.",
            record.kind
        );
    }
    format!(
        "This PR-local {} gap has a bounded repair route.",
        record.kind
    )
}

pub(super) fn repair_intent(route_kind: &str) -> &'static str {
    match route_kind {
        "AddBoundaryAssertion" => "Add a boundary assertion.",
        "AddErrorAssertion" => "Add an error-path assertion.",
        "AddValueAssertion" => "Add an exact value assertion.",
        "AddSideEffectObserver" => "Add a side-effect observer.",
        "AddOutputGolden" => "Add or update output-contract golden evidence.",
        _ => "Add the focused proof named by the repair route.",
    }
}

pub(super) fn repair_prompt(route: &GapRepairRoute, verify_command: &str) -> String {
    let repair = repair_text(route);
    format!(
        "{repair} Do not change production behavior unless the existing tests prove it is necessary. Verify with {}.",
        code_span(verify_command)
    )
}

/// The card's `gap_state`, decided once: the review card projects it and the
/// slot ordering reads it, so the two cannot disagree.
pub(super) fn review_recommendation_json(
    root: &Path,
    base: &str,
    _mode: &Mode,
    config: &RiprConfig,
    selected: &AgentBriefSelectedSeam<'_>,
    causal_projection: Option<&CausalDeltaArtifact>,
) -> Value {
    let entry = selected.seam;
    let seam = &entry.seam;
    let missing = missing_records_for(entry);
    let recommended = agent_seam_packets::recommended_test_for(entry);
    let nearest = agent_seam_packets::nearest_strong_test_to_imitate(seam.kind(), &entry.evidence);
    let candidate_values = agent_seam_packets::candidate_values_for(entry, &missing);
    let assertion_shape = agent_seam_packets::assertion_shape_for_entry(entry);
    let seam_id = seam.id().as_str();
    // Review cards are published into pull requests, where a checkout path
    // from the machine that rendered them (often a CI runner) names nothing
    // the reader has: their commands keep the portable `--root` the report
    // was invoked with, meaning the reader's own checkout root (#4000).
    let root_display = display_path(root);
    let missing_value = missing.first().map(|record| record.value.clone());
    let seam_file = display_path(seam.file());
    let seam_line = seam.display_line();
    let target_unresolved = cross_language_test_target_unresolved(entry);
    let navigation_target = agent_seam_packets::navigation_only_external_target_for(entry);
    let navigation_target_json = navigation_target
        .as_ref()
        .map(agent_seam_packets::navigation_only_external_target_json);

    // Derive gap_state for every card using the canonical decision from evidence_record.
    let gap_state = card_gap_state(entry, &missing);

    // receipt_command: only for actionable cards; reuses canonical_receipt_command_for.
    let receipt_command = canonical_receipt_command_for(entry, gap_state, &root_display);

    // why_not_actionable + non_claims: for static_limitation cards.
    let static_limitations = if gap_state == "static_limitation" {
        static_limitations_for(entry)
    } else {
        Vec::new()
    };
    let why_not_actionable: Option<String> = if gap_state == "static_limitation" {
        static_limitations.first().map(|lim| lim.reason.clone())
    } else {
        None
    };
    // Only a limitation that is a plain statement about the change reaches
    // the sentence an author reads. The others name producer vocabulary
    // ("producer-owned missing discriminator fact") that stays in
    // `why_not_actionable` for tools.
    let readable_limitation_detail: Option<String> = static_limitations
        .first()
        .filter(|limitation| limitation.category == NO_TEST_REACHES_OWNER_CATEGORY)
        .map(|limitation| limitation.reason.clone());
    let non_claims = if gap_state == "static_limitation" {
        let lang_status = entry
            .evidence
            .related_tests
            .first()
            .map(|_| "preview_advisory")
            .unwrap_or("advisory");
        json!({
            "language_status": lang_status,
            "authority_boundary": "advisory_static_evidence_only",
        })
    } else {
        Value::Null
    };

    // Structured related-test object {name, file, line} per RIPR-SPEC-0068:
    // the navigational location of the nearest strong related test, or null
    // when no strong related test resolves.
    let related_test = nearest
        .map(|test| {
            json!({
                "name": test.test_name,
                "file": display_path(&test.file),
                "line": test.line,
            })
        })
        .unwrap_or(Value::Null);

    let llm_guidance = if target_unresolved {
        json!({
            "prompt": limitation_prompt(navigation_target.as_ref()),
            "command": portable_agent_brief_command(&root_display, seam_id, WORKFLOW_AGENT_BRIEF_ARTIFACT),
        })
    } else if gap_state == "actionable" {
        json!({
            "prompt": llm_prompt(&recommended.file, nearest.map(|test| test.test_name.as_str()), missing_value.as_deref()),
            "command": portable_agent_brief_command(&root_display, seam_id, WORKFLOW_AGENT_BRIEF_ARTIFACT),
            "analysis_outcome_command": portable_check_analysis_outcome_command_with_base(
                &root_display,
                Some(base),
                "draft",
                WORKFLOW_ANALYSIS_OUTCOME_ARTIFACT,
            ),
            "verify_command": portable_agent_verify_command(
                &root_display,
                WORKFLOW_BEFORE_SNAPSHOT_ARTIFACT,
                WORKFLOW_AFTER_SNAPSHOT_ARTIFACT,
                Some(WORKFLOW_AGENT_VERIFY_ARTIFACT),
            ),
        })
    } else if gap_state == "static_limitation" {
        json!({
            "prompt": match static_limitations.first() {
                Some(limitation) if limitation.category == NO_TEST_REACHES_OWNER_CATEGORY => {
                    no_test_reaches_owner_prompt(root, entry)
                }
                limitation => limitation_prompt_for(limitation),
            },
            "command": portable_agent_brief_command(&root_display, seam_id, WORKFLOW_AGENT_BRIEF_ARTIFACT),
        })
    } else {
        json!({
            "prompt": "No repair packet is available for this finding. Inspect the producer-owned evidence and policy state before taking action.",
            "command": portable_agent_brief_command(&root_display, seam_id, WORKFLOW_AGENT_BRIEF_ARTIFACT),
        })
    };

    let suggested_test = if gap_state == "actionable" {
        json!({
            "intent": suggested_test_intent(&assertion_shape),
            "candidate_values": candidate_values.iter().map(|record| record.value.clone()).collect::<Vec<_>>(),
            "assertion_shape": assertion_shape.example(),
            "assertion_kind": assertion_shape.kind().map(|kind| kind.as_str()),
            "assertion_guidance": agent_seam_packets::assertion_guidance_json(&assertion_shape),
            "recommended_file": recommended.file.as_str(),
            "recommended_name": recommended.name.as_str(),
            "near_test": nearest.map(|test| test.test_name.clone()),
            "related_test": related_test,
        })
    } else {
        json!({
            "intent": "Inspect the named static limitation.",
            "candidate_values": [],
            "assertion_shape": null,
            "assertion_kind": null,
            "assertion_guidance": null,
            "recommended_file": "not_applicable",
            "recommended_name": "not_applicable",
            "near_test": null,
            "related_test": null,
        })
    };

    // Card-level oracle (RIPR-SPEC-0068): the representative oracle for this
    // seam — the nearest strong related test if one resolves, else the
    // top-ranked related test, else no oracle observed (`unknown` / `none`).
    // This projects the same oracle facts agent briefs and seam packets carry;
    // the review card does not compute an oracle of its own.
    let representative_oracle =
        nearest.or_else(|| entry.evidence.related_tests.first().map(AsRef::as_ref));
    let oracle_kind = representative_oracle
        .map(|test| test.oracle_kind.as_str())
        .unwrap_or_else(|| crate::domain::OracleKind::Unknown.as_str());
    let oracle_strength = representative_oracle
        .map(|test| test.oracle_strength.as_str())
        .unwrap_or_else(|| crate::domain::OracleStrength::None.as_str());

    let mut recommendation = json!({
        "id": format!("ripr-review-{seam_id}"),
        "seam_id": seam_id,
        // Canonical gap identity is owned by the analysis domain. The review
        // renderer only projects it; it must not derive an ID from a locator
        // or other presentation fields.
        "canonical_gap_id": canonical_gap_identity(entry).map(|identity| identity.id),
        "dedupe_key": format!("ripr:{seam_id}:{seam_file}:{seam_line}"),
        "kind": seam.kind().as_str(),
        "grip_class": entry.class.as_str(),
        "gap_state": gap_state,
        "oracle_kind": oracle_kind,
        "oracle_strength": oracle_strength,
        "severity": config.severity().for_seam(entry.class).as_str(),
        "owner": seam.owner(),
        "seam": {
            "file": seam_file,
            "line": seam_line,
            "expression": seam.expression(),
        },
        "source_location": source_location_json(&seam_file, Some(seam_line)),
        "reason": reason_for(
            selected,
            gap_state,
            missing_value.as_deref(),
            readable_limitation_detail.as_deref(),
        ),
        "missing_discriminator": missing_value,
        "suggested_test": suggested_test,
        "llm_guidance": llm_guidance,
    });

    // Project actionable-only fields.
    if let (Some(cmd), Some(object)) = (receipt_command, recommendation.as_object_mut()) {
        object.insert("receipt_command".to_string(), json!(cmd));
    }
    // The repair transaction's start (#3906), only past the fail-closed
    // repair-packet flip. The evidence record owns the decision; the card
    // projects it.
    if let (Some(cmd), Some(guidance)) = (
        canonical_repair_command_for(entry, gap_state, &root_display),
        recommendation
            .get_mut("llm_guidance")
            .and_then(Value::as_object_mut),
    ) {
        guidance.insert("repair_command".to_string(), json!(cmd));
    }
    // Project limitation-only fields.
    if let (Some(why), Some(object)) = (why_not_actionable, recommendation.as_object_mut()) {
        object.insert("why_not_actionable".to_string(), json!(why));
    }
    if !non_claims.is_null()
        && let Some(object) = recommendation.as_object_mut()
    {
        object.insert("non_claims".to_string(), non_claims);
    }

    if target_unresolved && let Some(object) = recommendation.as_object_mut() {
        object.insert("repairability".to_string(), json!("no_action"));
        object.insert(
            "limitation".to_string(),
            json!(CROSS_LANGUAGE_TARGET_UNRESOLVED_CATEGORY),
        );
        object.insert(
            "limitation_route".to_string(),
            json!(CROSS_LANGUAGE_TARGET_UNRESOLVED_REPAIR_ROUTE),
        );
        object.insert(
            "projection_exclusion_reasons".to_string(),
            json!([CROSS_LANGUAGE_TARGET_UNRESOLVED_CATEGORY]),
        );
        if let Some(target) = navigation_target_json {
            object.insert("navigation_only_target".to_string(), target);
        }
    }
    if let Some(projection) = causal_projection
        && let Some(object) = recommendation.as_object_mut()
    {
        projection.insert_delta_fields(
            object,
            canonical_gap_identity(entry).map(|id| id.id).as_deref(),
        );
    }
    recommendation
}

pub(super) fn source_location_json(file: &str, line: Option<usize>) -> Value {
    match line {
        Some(line) => json!({
            "status": "resolved",
            "file": file,
            "line": line,
            "span": null,
            "limitation": null,
            "repair_route": null,
        }),
        None => unresolved_source_location_json(),
    }
}

pub(super) fn unresolved_source_location_json() -> Value {
    json!({
        "status": "source_location_unresolved",
        "file": "unknown",
        "line": null,
        "span": null,
        "limitation": "source_location_unresolved",
        "repair_route": "analysis/source-location-resolution",
    })
}

/// Scope disclosure for a gate decision built from a gap ledger (issue #9301
/// F2): the same shape review-comments emits for a ledger artifact.
///
/// With a verified producer `coverage` (a complete whole-Rust-workspace
/// repo-exposure run whose record set is intact, #9599) the scope is
/// `rust_workspace_complete`; otherwise the record-derived
/// `gap_ledger_artifact` scope, which baseline resolution never trusts.
pub(super) fn analysis_scope_json(scope: &ReviewCommentsAnalysisScope) -> Value {
    let mut value = json!({
        "scope": scope.scope,
        "run_status": scope.run_status,
        "basis": scope.basis,
        "changed_files": scope.changed_files,
        "changed_lines": scope.changed_lines,
        "changed_owner_functions": scope.changed_owner_functions,
        "changed_production_files": scope.changed_production_files,
        "immediate_caller_files": scope.immediate_caller_files,
        "scoped_production_files": scope.scoped_production_files,
        "total_rust_files": scope.total_rust_files,
        "total_production_files": scope.total_production_files,
        "production_files_considered": scope.production_files_considered,
        "classified_seams_considered": scope.classified_seams_considered,
        "downstream_consumable": scope.downstream_consumable,
        "limitation": scope.limitation,
        "repair_route": scope.repair_route,
    });
    // Present only when the staged scope skipped seams, so a consumer
    // can tell `classified_seams_considered` is not the whole scope.
    if scope.unevaluated_seams > 0 {
        value["unevaluated_seams"] = json!(scope.unevaluated_seams);
    }
    if !scope.parser_declined_files.is_empty() {
        value["parser_declined_files"] = json!(scope.parser_declined_files);
    }
    if !scope.absent_changed_files.is_empty() {
        value["absent_changed_files"] = json!(scope.absent_changed_files);
    }
    value
}

pub(crate) fn display_paths(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths.iter().map(|path| display_path(path)).collect()
}

pub(super) fn llm_prompt(
    recommended_file: &str,
    near_test: Option<&str>,
    missing: Option<&str>,
) -> String {
    let target = missing.unwrap_or("the missing discriminator named by the seam packet");
    let near = near_test
        .map(|test| format!(" near {test}"))
        .unwrap_or_default();
    format!(
        "Write one focused Rust test for {target}. Place it in {recommended_file}{near}. Do not change production code. Preserve existing fixture style. {LLM_PROMPT_VERIFY_SENTENCE}"
    )
}

pub(super) fn limitation_prompt(
    navigation_target: Option<&agent_seam_packets::NavigationOnlyExternalTarget>,
) -> String {
    let target = navigation_target
        .map(|target| {
            format!(
                " Navigation-only external observer target: {}:{}.",
                target.file, target.line
            )
        })
        .unwrap_or_default();
    format!(
        "Inspect the cross-language target limitation before repair. Do not write a Rust test, do not infer an edit surface, and route through `{CROSS_LANGUAGE_TARGET_UNRESOLVED_REPAIR_ROUTE}`.{target}"
    )
}

pub(super) fn limitation_prompt_for(limitation: Option<&EvidenceRecordStaticLimitation>) -> String {
    match limitation {
        Some(limitation) => format!(
            "Do not write a repair test or infer an edit surface from this finding. Inspect static limitation {}: {}. Route investigation through {}.",
            code_span(&limitation.category),
            limitation.reason,
            code_span(&limitation.repair_route)
        ),
        None => "Do not write a repair test or infer an edit surface from this finding. The producer-owned repair route is incomplete; inspect the evidence before taking action.".to_string(),
    }
}

/// Plain wording for a changed owner no existing test reaches (#4216 row 2):
/// a test gap in the change, not an analyzer limitation. It names no repair
/// target; the producer-owned route stays incomplete and fail-closed.
pub(super) fn no_test_reaches_owner_prompt(root: &Path, entry: &ClassifiedSeam) -> String {
    let seam = &entry.seam;
    let placement = match integration_test_files_for(root, seam.file()).as_slice() {
        [only] => format!(
            " A new test for it would usually go in {}, the crate's integration test file.",
            code_span(only)
        ),
        [] => String::new(),
        _ => " A new test for it would usually go in one of the crate's integration test files under `tests/`.".to_string(),
    };
    format!(
        "No existing test reaches {owner}: static evidence finds no test path to this changed owner (`no_static_path`), so static evidence shows no current test checking the changed behavior {expression}. This is a test gap in the change, not a RIPR analysis limitation. RIPR does not propose a target for a first test, so no repair route, verify command, or receipt is offered.{placement}",
        owner = code_span(seam.owner()),
        expression = code_span(seam.expression()),
    )
}

/// Top-level `tests/*.rs` files of the Cargo package owning `source_file`
/// (workspace-relative, sorted). Navigation text only, never a repair target.
pub(super) fn integration_test_files_for(root: &Path, source_file: &Path) -> Vec<String> {
    let mut package_dir = source_file.parent();
    while let Some(dir) = package_dir {
        if root.join(dir).join("Cargo.toml").is_file() {
            break;
        }
        package_dir = dir.parent();
    }
    let Some(package_dir) = package_dir else {
        return Vec::new();
    };
    let tests_dir = package_dir.join("tests");
    let Ok(read_dir) = std::fs::read_dir(root.join(&tests_dir)) else {
        return Vec::new();
    };
    let mut files = read_dir
        .filter_map(Result::ok)
        .filter(|item| item.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|item| item.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".rs"))
        .map(|name| display_path(&tests_dir.join(name)))
        .collect::<Vec<_>>();
    files.sort();
    files
}

pub(super) fn push_markdown_items(lines: &mut Vec<String>, heading: &str, value: Option<&Value>) {
    lines.push(format!("## {heading}"));
    lines.push(String::new());
    let items = value.and_then(Value::as_array);
    let Some(items) = items.filter(|items| !items.is_empty()) else {
        lines.push("- None.".to_string());
        lines.push(String::new());
        return;
    };

    for item in items {
        let seam_id = string_field(item, "seam_id")
            .or_else(|| string_field(item, "gap_id"))
            .unwrap_or("unknown");
        let reason = string_field(item, "reason").unwrap_or("No reason available.");
        let source_location = markdown_source_location(item);
        let state = string_field(item, "gap_state")
            .or_else(|| string_field(item, "grip_class"))
            .unwrap_or("unknown");
        let command = item
            .get("llm_guidance")
            .and_then(|guidance| string_field(guidance, "command"))
            .unwrap_or("ripr agent brief --root . --seam-id <id> --json");
        lines.push(format!(
            "- {} @ {}: {}",
            code_span(seam_id),
            code_span(&source_location),
            inline_prose(reason)
        ));
        let canonical_gap_id = string_field(item, "canonical_gap_id")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("null");
        lines.push(format!(
            "  - canonical_gap_id: {}",
            code_span(canonical_gap_id)
        ));
        if let Some(attribution) = string_field(item, "delta_attribution") {
            lines.push(format!("  - delta_attribution: {}", code_span(attribution)));
        }
        lines.push(format!("  - state: {}", code_span(state)));
        if let Some(route) = repair_route_kind(item) {
            lines.push(format!("  - repair_route: {}", code_span(route)));
        }
        if let Some(route) = string_field(item, "limitation_route") {
            lines.push(format!("  - limitation_route: {}", code_span(route)));
        }
        if let Some(target) = item.get("navigation_only_target") {
            push_navigation_only_target_markdown(lines, target);
        }
        if source_location == "unknown:unknown" {
            lines.push(
                "  - limitation: `source_location_unresolved`; repair_route: `analysis/source-location-resolution`"
                    .to_string(),
            );
        }
        lines.push(format!("  - command: {}", code_span(command)));
        if let Some(guidance) = item.get("llm_guidance")
            && let Some(outcome) = string_field(guidance, "analysis_outcome_command")
        {
            lines.push(format!(
                "  - analysis_outcome_command: {}",
                code_span(outcome)
            ));
            if let Some(verify) = string_field(guidance, "verify_command") {
                lines.push(format!("  - verify_command: {}", code_span(verify)));
            }
            if let Some(receipt) = string_field(item, "receipt_command") {
                lines.push(format!("  - receipt_command: {}", code_span(receipt)));
            }
        }
    }
    lines.push(String::new());
}

pub(super) fn push_navigation_only_target_markdown(lines: &mut Vec<String>, target: &Value) {
    let location = markdown_location_from_fields(
        target.get("file").and_then(Value::as_str),
        target.get("line").and_then(Value::as_u64),
    );
    let language = string_field(target, "language").unwrap_or("external");
    let ready = target
        .get("repair_packet_ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    lines.push(format!(
        "  - navigation_only_target: {} ({language}; repair_packet_ready={ready})",
        code_span(&location)
    ));
    if let Some(test_name) = string_field(target, "test_name") {
        lines.push(format!("  - external_observer: {}", code_span(test_name)));
    }
}

pub(super) fn push_analysis_scope_summary(lines: &mut Vec<String>, value: Option<&Value>) {
    let Some(scope) = value.and_then(Value::as_object) else {
        return;
    };
    let scope_name = scope
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let run_status = scope
        .get("run_status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let considered = scope
        .get("production_files_considered")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let classified = scope
        .get("classified_seams_considered")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_production = scope
        .get("total_production_files")
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    lines.push(format!("- analysis scope: `{scope_name}`"));
    lines.push(format!("- run status: `{run_status}`"));
    lines.push(format!(
        "- scoped production files: {considered}/{total_production}"
    ));
    lines.push(format!("- classified seams considered: {classified}"));
    if let Some(unevaluated) = scope.get("unevaluated_seams").and_then(Value::as_u64) {
        lines.push(format!(
            "- scoped seams not evaluated: {unevaluated} (changed-line seams filled every review slot)"
        ));
    }
    if let Some(absent) = scope
        .get("absent_changed_files")
        .and_then(Value::as_array)
        .filter(|files| !files.is_empty())
    {
        let listed = absent
            .iter()
            .filter_map(Value::as_str)
            .map(code_span)
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "- limitation: `changed_file_absent_from_worktree`; dropped file(s): {listed}"
        ));
        lines.push(
            "- repair: check out the missing file, or disable sparse checkout for it, then re-run"
                .to_string(),
        );
    }
    if let Some(declined) = scope
        .get("parser_declined_files")
        .and_then(Value::as_array)
        .filter(|files| !files.is_empty())
    {
        let listed = declined
            .iter()
            .filter_map(Value::as_str)
            .map(code_span)
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "- limitation: `parser_declined_files`; no seams were extracted from: {listed}, so a baselined gap in them is not evaluated, not resolved"
        ));
        lines.push(
            "- repair: split the file below the Rust parse budget, fix the parse error or save it as UTF-8, then re-run"
                .to_string(),
        );
    }
    if let (Some(limitation), Some(route)) = (
        scope.get("limitation").and_then(Value::as_str),
        scope.get("repair_route").and_then(Value::as_str),
    ) {
        lines.push(format!(
            "- limitation: `{limitation}`; repair_route: `{route}`"
        ));
    }
}

pub(super) fn markdown_source_location(item: &Value) -> String {
    if let Some(source) = item.get("source_location").and_then(Value::as_object) {
        return markdown_location_from_fields(
            source.get("file").and_then(Value::as_str),
            source.get("line").and_then(Value::as_u64),
        );
    }
    if let Some(seam) = item.get("seam").and_then(Value::as_object) {
        return markdown_location_from_fields(
            seam.get("file").and_then(Value::as_str),
            seam.get("line").and_then(Value::as_u64),
        );
    }
    if let Some(placement) = item.get("placement").and_then(Value::as_object) {
        return markdown_location_from_fields(
            placement.get("path").and_then(Value::as_str),
            placement.get("line").and_then(Value::as_u64),
        );
    }
    "unknown:unknown".to_string()
}

pub(super) fn markdown_location_from_fields(file: Option<&str>, line: Option<u64>) -> String {
    let file = file
        .map(str::trim)
        .filter(|file| !file.is_empty())
        .unwrap_or("unknown");
    let line = line
        .map(|line| line.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!("{file}:{line}")
}

pub(super) fn repair_route_kind(item: &Value) -> Option<&str> {
    item.get("repair_card")
        .and_then(|card| card.get("repair_route"))
        .and_then(|route| string_field(route, "route_kind"))
}

pub(super) fn push_suppressed_items(lines: &mut Vec<String>, value: Option<&Value>) {
    lines.push("## Suppressed".to_string());
    lines.push(String::new());
    let items = value.and_then(Value::as_array);
    let Some(items) = items.filter(|items| !items.is_empty()) else {
        lines.push("- None.".to_string());
        return;
    };
    for item in items {
        let seam_id = string_field(item, "seam_id")
            .or_else(|| string_field(item, "gap_id"))
            .unwrap_or("unknown");
        let reason = string_field(item, "reason").unwrap_or("unknown");
        lines.push(format!(
            "- {}: {}",
            code_span(seam_id),
            inline_prose(reason)
        ));
    }
}

pub(super) fn string_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}
