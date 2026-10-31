//! Command-payload production for the LSP `workspace/executeCommand` surface:
//! gap-record context packets, receipt status, repair packets and evidence
//! context packets (#3021). Behavior-preserving move out of `backend.rs`.

use super::*;

/// Extract the String payload from rejection variants that carry one.
/// Unit and `&str`-reason variants emit an empty list.
/// gap_id-bearing sample sources are a deferred follow-up — the rejection does
/// not carry a gap_id.
pub(super) fn limitation_sample_sources(
    rejection: &crate::lsp::gap_artifacts::GapArtifactRejection,
) -> Vec<String> {
    use crate::lsp::gap_artifacts::GapArtifactRejection;
    match rejection {
        GapArtifactRejection::DisabledLanguage(s)
        | GapArtifactRejection::MalformedCommandPayload(s)
        | GapArtifactRejection::OutOfWorkspacePath(s)
        | GapArtifactRejection::UnavailableLanguage(s)
        | GapArtifactRejection::UnsupportedKind(s)
        | GapArtifactRejection::UnsupportedSchema(s)
        | GapArtifactRejection::UnsupportedStaticLimitKind(s)
        | GapArtifactRejection::WrongRoot(s)
        | GapArtifactRejection::StaleSubject(s) => vec![s.clone()],
        GapArtifactRejection::UnverifiableSubject(reason) => vec![(*reason).to_string()],
        GapArtifactRejection::MalformedArtifact(_)
        | GapArtifactRejection::MissingIdentity
        | GapArtifactRejection::StaleCorpus
        | GapArtifactRejection::StaleArtifact => vec![],
    }
}

/// Per-category static non-claims table.
/// Vocabulary: approved static-exposure terms only (exposed/weakly_exposed/etc.).
pub(super) fn limitation_non_claims(category: &str) -> Vec<&'static str> {
    match category {
        "disabled_language" | "unavailable_language" => vec![
            "not a Rust repair packet",
            "does not indicate the behavior is reachable",
            "does not indicate tests are absent",
        ],
        "wrong_root" | "out_of_workspace_path" => vec![
            "not a repair packet",
            "path resolution required before exposure can be assessed",
        ],
        "stale_artifact"
        | "stale_subject"
        | "unverifiable_subject"
        | "missing_identity"
        | "malformed_artifact"
        | "malformed_command_payload" => vec![
            "not a repair packet",
            "artifact regeneration required before exposure can be assessed",
        ],
        "unsupported_schema" | "unsupported_kind" | "unsupported_static_limit_kind" => vec![
            "not a repair packet",
            "ripr upgrade required before exposure can be assessed",
        ],
        _ => vec!["not a repair packet"],
    }
}

pub(super) fn collect_gap_record_context_packet(
    root: &Path,
    repo_config: &crate::config::RiprConfig,
    args: &serde_json::Map<String, serde_json::Value>,
    gap_id: &str,
) -> Option<LSPAny> {
    let gap_id = gap_id.trim();
    if gap_id.is_empty() {
        return None;
    }
    let ledger_arg = args
        .get("gap_ledger")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_GAP_DECISION_LEDGER_OUT);
    let ledger_path = absolute_join(root, Path::new(ledger_arg));
    // Client-supplied path: confine reads to the workspace root so a client
    // message cannot make the long-running server read arbitrary files.
    // `absolute_join` passes absolute paths through unchanged, so an
    // out-of-root absolute path or a symlink escape is rejected here
    // (canonicalizing both sides, like `validate_agent_brief_diff_path`).
    if !path_is_within_root(root, &ledger_path) {
        return None;
    }
    let contents = match read_artifact_capped(&ledger_path) {
        CappedArtifactRead::Contents(contents) => contents,
        // Absent ledger: no packet is available, which is a normal state.
        CappedArtifactRead::Missing => return None,
        // Present but oversize/unreadable: typed sentinel, not a silent None.
        CappedArtifactRead::Unusable => {
            return Some(repair_packet_sentinel(MALFORMED_GAP_LEDGER_REASON));
        }
    };
    let records = match parse_gap_records_json_for_root(&contents, Some(root)) {
        Ok(records) => records,
        Err(_) => return Some(repair_packet_sentinel(MALFORMED_GAP_LEDGER_REASON)),
    };
    let record = records
        .iter()
        .find(|record| gap_record_matches(record, gap_id))?;
    if let Some(sentinel) = ledger_source_subject_sentinel(root, repo_config, &contents) {
        return Some(sentinel);
    }
    let rendered = render_agent_gap_record_packet_json(&display_path(&ledger_path), record).ok()?;
    serde_json::from_str(&rendered).ok()
}

/// #4544: a command-time packet read re-checks the ledger's `source_subject`
/// stamp through the shared validator, so a packet computed for other file
/// contents is a typed stale disclosure instead of a current repair route.
pub(super) fn ledger_source_subject_sentinel(
    root: &Path,
    repo_config: &crate::config::RiprConfig,
    contents: &str,
) -> Option<LSPAny> {
    let Ok(artifact) = serde_json::from_str::<serde_json::Value>(contents) else {
        return Some(repair_packet_sentinel(MALFORMED_GAP_LEDGER_REASON));
    };
    source_subject_sentinel(
        root,
        repo_config,
        &artifact,
        crate::lsp::gap_artifacts::GapArtifactKind::GapDecisionLedger,
        "gap-decision-ledger.json",
        "ripr reports gap-ledger --repo-exposure target/ripr/reports/repo-exposure.json",
    )
}

pub(super) fn source_subject_sentinel(
    root: &Path,
    repo_config: &crate::config::RiprConfig,
    artifact: &serde_json::Value,
    kind: crate::lsp::gap_artifacts::GapArtifactKind,
    artifact_name: &str,
    regenerate: &str,
) -> Option<LSPAny> {
    use crate::lsp::gap_artifacts::GapArtifactRejection;
    let context = crate::lsp::gap_artifacts::GapArtifactValidationContext {
        root,
        enabled_languages: repo_config.languages().enabled(),
        repo_config,
    };
    let recovery = format!(
        "regenerate from fresh analysis in the selected workspace: capture ripr check --format repo-exposure-json as target/ripr/reports/repo-exposure.json then {regenerate}"
    );
    match crate::lsp::gap_artifacts::validate_gap_artifact(artifact, &context) {
        Ok(validated) if validated.kind == kind => None,
        Ok(_) => Some(repair_packet_sentinel(&format!(
            "unsupported_kind: expected {artifact_name}"
        ))),
        Err(GapArtifactRejection::StaleSubject(path)) => Some(repair_packet_sentinel(&format!(
            "stale_subject: {path} changed since {artifact_name} was written; {recovery}"
        ))),
        Err(GapArtifactRejection::StaleCorpus) => Some(repair_packet_sentinel(&format!(
            "stale_corpus: the analyzed Rust source/test corpus changed since {artifact_name} was written; {recovery}"
        ))),
        Err(GapArtifactRejection::UnverifiableSubject(reason)) => {
            Some(repair_packet_sentinel(&format!(
                "unverifiable_subject: {reason}: {artifact_name} cannot be matched to the current analyzed source inputs; {recovery}"
            )))
        }
        Err(GapArtifactRejection::WrongRoot(_)) => Some(repair_packet_sentinel(&format!(
            "wrong_root: {artifact_name} belongs to another workspace; select its producer workspace or regenerate it for the selected workspace"
        ))),
        Err(other) => Some(repair_packet_sentinel(other.as_str())),
    }
}

pub(super) const DEFAULT_ACTIONABLE_GAPS_OUT: &str = "target/ripr/reports/actionable-gaps.json";

/// Derive `receipt_status`, `missing_receipt_reason`, and `copy_receipt_command`
/// from the top actionable gap artifact plus the snapshot's first-safe-receipt-command
/// path.
///
/// Honesty rules:
/// - `copy_receipt_command` is only emitted when the packet is COMPLETE
///   (verify_command + receipt_command both present). A limitation or
///   incomplete packet MUST NOT surface a repair receipt command.
/// - `receipt_status` reports the real movement value (or "not_available"
///   when no gap artifact or receipt object exists).
pub(super) fn collect_receipt_status_fields(
    root: &std::path::Path,
    top_gap: Option<&crate::lsp::gap_artifacts::ValidatedGapArtifact>,
    snapshot: &crate::lsp::state::AnalysisSnapshot,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let not_available = serde_json::Value::String("not_available".to_string());

    // Top-level limitation present → no repair receipt command (RIPR-SPEC-0076
    // harmonization: limitations must never show a repair receipt command).
    let has_limitation = !snapshot.gap_artifact_rejections.is_empty();

    let Some(artifact) = top_gap else {
        return (not_available.clone(), not_available.clone(), not_available);
    };

    // receipt_status: from the gap-decision-ledger receipt.movement field.
    // Read directly from the artifact's receipt object via the ledger.
    // For LSP we can derive from the artifact's receipt_commands presence
    // as a proxy; the real movement comes from the ledger receipt record.
    let ledger_path =
        root.join(crate::output::gap_decision_ledger::DEFAULT_GAP_DECISION_LEDGER_OUT);
    let (receipt_status_val, missing_receipt_reason_val) =
        receipt_status_from_ledger(root, &ledger_path, artifact);

    // copy_receipt_command: only for complete packets (verify + receipt
    // commands both present). Incomplete packets → not_available.
    let copy_receipt_cmd_val = if has_limitation {
        // Limitation surface — never show receipt command.
        not_available
    } else {
        let has_verify = !artifact.verify_commands.is_empty();
        let has_receipt = !artifact.receipt_commands.is_empty();
        if has_verify && has_receipt {
            // Use the existing first_safe_receipt_command path from actions.rs
            // by reading it directly from the artifact.
            let cmd = artifact
                .receipt_commands
                .first()
                .map(String::as_str)
                .unwrap_or("");
            let bound = crate::lsp::gap_artifacts::command_payload_is_safe(root, cmd)
                .then(|| crate::lsp::gap_artifacts::bind_portable_command(root, cmd))
                .flatten();
            serde_json::Value::String(bound.unwrap_or_else(|| "not_available".to_string()))
        } else {
            // Incomplete packet — no receipt command shown.
            serde_json::Value::String("not_available".to_string())
        }
    };

    (
        receipt_status_val,
        missing_receipt_reason_val,
        copy_receipt_cmd_val,
    )
}

/// Read the gap-decision-ledger to get the real receipt movement + missing_reason
/// for the given gap artifact. Falls back to not_available on any read/parse error.
pub(super) fn receipt_status_from_ledger(
    root: &std::path::Path,
    ledger_path: &std::path::Path,
    artifact: &crate::lsp::gap_artifacts::ValidatedGapArtifact,
) -> (serde_json::Value, serde_json::Value) {
    let not_available = "not_available".to_string();

    let contents = match fs::read_to_string(ledger_path) {
        Ok(c) => c,
        Err(_) => {
            return (
                serde_json::Value::String(not_available.clone()),
                serde_json::Value::String(not_available),
            );
        }
    };
    let records = match crate::output::gap_decision_ledger::parse_gap_records_json_for_root(
        &contents,
        Some(root),
    ) {
        Ok(r) => r,
        Err(_) => {
            return (
                serde_json::Value::String(not_available.clone()),
                serde_json::Value::String(not_available),
            );
        }
    };

    // Match the ledger record to the top artifact using canonical_gap_id.
    let canonical_gap_id = artifact
        .identities
        .first()
        .and_then(|id| id.canonical_gap_id.as_deref())
        .unwrap_or("");

    let record = records.iter().find(|r| {
        r.canonical_gap_id == canonical_gap_id
            || (!canonical_gap_id.is_empty() && r.gap_id == canonical_gap_id)
    });

    let Some(record) = record else {
        // The artifact exists in the snapshot but the ledger doesn't have a
        // matching record — movement is genuinely unknown, not zero.
        return (
            serde_json::Value::String(not_available.clone()),
            serde_json::Value::String(not_available),
        );
    };

    // receipt.movement is the only reliably-populated field in production.
    let movement_val = match record.receipt.as_ref().and_then(|r| r.movement.as_deref()) {
        Some(m) => serde_json::Value::String(m.to_string()),
        None => serde_json::Value::String(not_available.clone()),
    };

    // missing_receipt_reason: only meaningful when the gap is actionable
    // but has no receipt. Otherwise not_available.
    let missing_receipt_reason_val =
        if crate::domain::EvidenceState::is_actionable_word(&record.gap_state)
            && record
                .receipt
                .as_ref()
                .and_then(|r| r.movement.as_deref())
                .is_none()
            && record.receipt_command.is_some()
        {
            // Receipt command exists but no movement yet — emit honest "no_attempt_recorded".
            serde_json::Value::String("no_attempt_recorded".to_string())
        } else {
            serde_json::Value::String(not_available)
        };

    (movement_val, missing_receipt_reason_val)
}

/// Read swarm-attempt-ledger.json and surface the latest attempt's outcome
/// for the canonical_gap_id of the top actionable gap. Returns not_available
/// when the artifact is absent (absence != no outcome) or when no matching
/// attempt entry is found.
pub(super) fn read_latest_attempt_outcome(
    ledger_path: &std::path::Path,
    top_gap: Option<&crate::lsp::gap_artifacts::ValidatedGapArtifact>,
) -> serde_json::Value {
    let not_available = serde_json::Value::String("not_available".to_string());

    // Artifact absent → not_available (do not fabricate).
    let contents = match fs::read_to_string(ledger_path) {
        Ok(c) => c,
        Err(_) => return not_available,
    };
    let report: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(_) => return not_available,
    };

    let canonical_gap_id = top_gap
        .and_then(|a| a.identities.first())
        .and_then(|id| id.canonical_gap_id.as_deref())
        .unwrap_or("");

    // latest_attempts is the preferred section; fall back to attempts.
    let attempts_val = report
        .get("latest_attempts")
        .and_then(|v| v.as_array())
        .or_else(|| report.get("attempts").and_then(|v| v.as_array()));

    let Some(attempts) = attempts_val else {
        return not_available;
    };

    let entry = if canonical_gap_id.is_empty() {
        attempts.first()
    } else {
        // Another gap's attempt is not this gap's outcome.
        attempts.iter().find(|entry| {
            entry
                .get("canonical_gap_id")
                .and_then(|v| v.as_str())
                .is_some_and(|cid| cid == canonical_gap_id)
        })
    };

    entry
        .and_then(|e| e.get("outcome"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::Value::String(s.to_string()))
        .unwrap_or(not_available)
}

/// Read route-quality.json (RIPR-SPEC-0080 output) and surface a compact
/// summary. Returns not_available when the artifact is absent OR when
/// status = "blocked" (no real data to summarize).
pub(super) fn read_route_quality_summary(path: &std::path::Path) -> serde_json::Value {
    let not_available = serde_json::Value::String("not_available".to_string());

    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return not_available,
    };
    let report: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(_) => return not_available,
    };

    // If status = "blocked", the rows are empty and there's nothing real to
    // summarize — emit not_available to be honest.
    if report.get("status").and_then(|v| v.as_str()) == Some("blocked") {
        return not_available;
    }

    // Produce a compact summary: top repair_kind rows from the latest array.
    let rows = report
        .get("repair_route_quality_latest")
        .and_then(|v| v.as_array());

    let Some(rows) = rows else {
        return not_available;
    };

    if rows.is_empty() {
        return not_available;
    }

    // Emit up to 3 top rows, carrying repair_kind and success_rate.
    let summary_rows: Vec<serde_json::Value> = rows
        .iter()
        .take(3)
        .map(|row| {
            let repair_kind = row
                .get("repair_kind")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let success_rate = row
                .get("repair_kind_success_rate")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let attempted = row
                .get("repair_kind_attempted")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "repair_kind": repair_kind,
                "attempted": attempted,
                "success_rate": success_rate,
            })
        })
        .collect();

    serde_json::json!({
        "report": "route-quality",
        "status": report.get("status").and_then(|v| v.as_str()).unwrap_or("unknown"),
        "top_repair_kind_rows": summary_rows,
    })
}

pub(super) fn workspace_receipt_status_report_paths() -> serde_json::Value {
    serde_json::json!({
        "gap_decision_ledger": crate::output::gap_decision_ledger::DEFAULT_GAP_DECISION_LEDGER_OUT,
        "swarm_attempt_ledger": "target/ripr/reports/swarm-attempt-ledger.json",
        "route_quality": "target/ripr/reports/route-quality.json",
    })
}

pub(super) fn collect_repair_packet_from_actionable_gaps(
    root: &Path,
    repo_config: &crate::config::RiprConfig,
    path: &Path,
    gap_id: Option<&str>,
) -> Option<LSPAny> {
    let contents = match read_artifact_capped(path) {
        CappedArtifactRead::Contents(contents) => contents,
        // Absent artifact: falling back to the next packet source is honest.
        CappedArtifactRead::Missing => return None,
        // Present but oversize/unreadable: typed sentinel so the client can
        // distinguish a corrupt source from "no packet"; no fallback.
        CappedArtifactRead::Unusable => {
            return Some(repair_packet_sentinel(MALFORMED_ACTIONABLE_GAPS_REASON));
        }
    };
    let report: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(report) => report,
        Err(_) => return Some(repair_packet_sentinel(MALFORMED_ACTIONABLE_GAPS_REASON)),
    };
    let packets = report.get("packets").and_then(|v| v.as_array())?;
    // A requested gap must match exactly. Answering an unknown id with the
    // first packet handed out another gap's repair instructions; `None` lets
    // the caller try the ledger, then say which gap has no packet.
    let packet = if let Some(id) = gap_id {
        packets.iter().find(|p| {
            p.get("canonical_gap_id")
                .and_then(|v| v.as_str())
                .is_some_and(|cid| cid == id)
        })?
    } else {
        packets
            .iter()
            .find(|p| {
                p.get("gap_state")
                    .and_then(|v| v.as_str())
                    .is_some_and(crate::domain::EvidenceState::is_actionable_word)
            })
            .or_else(|| packets.first())?
    };

    if let Some(sentinel) = source_subject_sentinel(
        root,
        repo_config,
        &report,
        crate::lsp::gap_artifacts::GapArtifactKind::ActionableGaps,
        "actionable-gaps.json",
        "cargo xtask lane1-evidence-audit",
    ) {
        return Some(sentinel);
    }

    // Require the packet to be actionable to emit a complete repair packet.
    if !packet
        .get("gap_state")
        .and_then(|v| v.as_str())
        .is_some_and(crate::domain::EvidenceState::is_actionable_word)
    {
        return Some(repair_packet_sentinel(
            "gap is not actionable in actionable-gaps.json",
        ));
    }
    validate_and_render_actionable_gap_packet(root, packet)
}

pub(super) fn validate_and_render_actionable_gap_packet(
    root: &Path,
    packet: &serde_json::Value,
) -> Option<LSPAny> {
    use crate::lsp::gap_artifacts::{
        GapArtifactRejection, require_actionable_packet_render_fields,
    };

    // The render-field contract is owned by the ingest boundary
    // (`lsp::gap_artifacts::require_actionable_packet_render_fields`,
    // RIPR-SPEC-0087 §8); this command-time re-check calls the same shared
    // validator instead of re-walking packet fields.
    if let Err(rejection) = require_actionable_packet_render_fields(packet) {
        let reason = match rejection {
            GapArtifactRejection::MalformedArtifact(message) => message.to_string(),
            other => other.as_str().to_string(),
        };
        return Some(repair_packet_sentinel(&reason));
    }

    let str_field = |key: &str| -> Option<String> {
        packet
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    };

    // The shared validator above has already established that these fields
    // are present and well-formed; the let-else fallbacks are defensive only.
    let Some(canonical_gap_id) = str_field("canonical_gap_id") else {
        return Some(repair_packet_sentinel(
            "actionable packet is missing canonical_gap_id",
        ));
    };
    let Some(repair_kind) = str_field("repair_kind") else {
        return Some(repair_packet_sentinel(
            "actionable packet is missing repair_kind",
        ));
    };
    let Some(verify_command) = str_field("verify_command") else {
        return Some(repair_packet_sentinel(
            "actionable packet is missing verify_command",
        ));
    };
    let Some(receipt_command) = str_field("receipt_command") else {
        return Some(repair_packet_sentinel(
            "actionable packet is missing receipt_command",
        ));
    };
    // #4001: the copied commands name the selected workspace, not `.`.
    let (Some(verify_command), Some(receipt_command)) = (
        crate::lsp::gap_artifacts::bind_portable_command(root, &verify_command),
        crate::lsp::gap_artifacts::bind_portable_command(root, &receipt_command),
    ) else {
        return Some(repair_packet_sentinel(UNBOUND_PACKET_COMMANDS_REASON));
    };

    let allowed_edit_surface: Vec<serde_json::Value> = packet
        .get("allowed_edit_surface")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let must_not_change: Vec<serde_json::Value> = packet
        .get("must_not_change")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let raw_evidence_refs: Vec<serde_json::Value> = packet
        .get("raw_evidence_refs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let confidence = packet
        .get("confidence_basis")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("static_only")
        .to_owned();

    // Derive language from raw_evidence_refs or canonical_gap_id.
    let language = raw_evidence_refs
        .iter()
        .find_map(|r| {
            r.get("language")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default();

    // Resolve source location from primary_anchor — never fabricate.
    let anchor = packet.get("primary_anchor");
    let anchor_file = anchor
        .and_then(|a| a.get("file"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    let anchor_line = anchor
        .and_then(|a| a.get("line"))
        .and_then(|v| v.as_u64())
        .filter(|&n| n > 0);

    let source_location = match (anchor_file, anchor_line) {
        (Some(file), Some(line)) => serde_json::json!({ "file": file, "line": line }),
        (Some(file), None) => serde_json::json!({
            "status": "source_location_unresolved",
            "file": file,
        }),
        _ => serde_json::json!({ "status": "source_location_unresolved" }),
    };

    let result = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "kind": "repair_packet",
        "canonical_gap_id": canonical_gap_id,
        "language": language,
        "repair_kind": repair_kind,
        "source_location": source_location,
        "allowed_edit_surface": allowed_edit_surface,
        "verify_command": verify_command,
        "receipt_command": receipt_command,
        "must_not_change": must_not_change,
        "raw_evidence_refs": raw_evidence_refs,
        "confidence": confidence,
        "limits_note": "Static evidence only; advisory, not a gate decision.",
    });
    serde_json::from_value(result).ok()
}

pub(super) fn collect_repair_packet_from_ledger(
    root: &Path,
    repo_config: &crate::config::RiprConfig,
    path: &Path,
    gap_id: Option<&str>,
) -> Option<LSPAny> {
    let contents = match read_artifact_capped(path) {
        CappedArtifactRead::Contents(contents) => contents,
        // Absent artifact: no packet is available, which is a normal state.
        CappedArtifactRead::Missing => return None,
        // Present but oversize/unreadable: typed sentinel, not a silent None.
        CappedArtifactRead::Unusable => {
            return Some(repair_packet_sentinel(MALFORMED_GAP_LEDGER_REASON));
        }
    };
    let records = match parse_gap_records_json_for_root(&contents, Some(root)) {
        Ok(records) => records,
        Err(_) => return Some(repair_packet_sentinel(MALFORMED_GAP_LEDGER_REASON)),
    };
    let record = if let Some(id) = gap_id {
        records
            .iter()
            .find(|r| r.gap_id == id || r.canonical_gap_id == id)?
    } else {
        records
            .iter()
            .find(|r| crate::domain::EvidenceState::is_actionable_word(&r.gap_state))?
    };
    if let Some(sentinel) = ledger_source_subject_sentinel(root, repo_config, &contents) {
        return Some(sentinel);
    }
    // Use the existing validator for completeness gate.
    if let Err(reason) = validate_agent_gap_record_packet(record) {
        return Some(serde_json::json!({
            "schema_version": "0.1",
            "tool": "ripr",
            "kind": "repair_packet",
            "status": "not_actionable_or_incomplete",
            "reason": reason,
        }));
    }

    let route = record.repair_route.as_ref()?;
    // #4001: the copied commands name the selected workspace, not `.`.
    let bind = |command: &str| crate::lsp::gap_artifacts::bind_portable_command(root, command);
    let (Some(verify_command), Some(receipt_command)) = (
        bind(record.verification_commands.first()?),
        bind(record.receipt_command.as_deref()?),
    ) else {
        return Some(repair_packet_sentinel(UNBOUND_PACKET_COMMANDS_REASON));
    };
    let allowed_edit_surface =
        crate::output::agent_seam_packets::allowed_edit_surface_for_gap_route(route);
    let must_not_change: Vec<String> =
        crate::output::agent_seam_packets::gap_record_packet_do_not_do(record);

    let anchor = record.anchor.as_ref();
    let anchor_file = anchor
        .and_then(|a| a.file.as_deref())
        .filter(|s| !s.is_empty())
        .map(crate::output::path::display_path_text);
    let anchor_line = anchor.and_then(|a| a.line).filter(|&n| n > 0);
    let source_location = match (anchor_file, anchor_line) {
        (Some(file), Some(line)) => serde_json::json!({ "file": file, "line": line }),
        (Some(file), None) => serde_json::json!({
            "status": "source_location_unresolved",
            "file": file,
        }),
        _ => serde_json::json!({ "status": "source_location_unresolved" }),
    };

    let canonical_gap_id = if record.canonical_gap_id.trim().is_empty() {
        &record.gap_id
    } else {
        &record.canonical_gap_id
    };

    let result = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "kind": "repair_packet",
        "canonical_gap_id": canonical_gap_id,
        "language": &record.language,
        "repair_kind": route.route_kind.as_str(),
        "source_location": source_location,
        "allowed_edit_surface": allowed_edit_surface,
        "verify_command": verify_command,
        "receipt_command": receipt_command,
        "must_not_change": must_not_change,
        "raw_evidence_refs": &record.evidence_ids,
        "confidence": "static_only",
        "limits_note": "Static evidence only; advisory, not a gate decision.",
    });
    serde_json::from_value(result).ok()
}

pub(super) fn no_repair_packet_reason(gap_id: Option<&str>) -> String {
    let subject = match gap_id {
        Some(id) => {
            let mut echoed: String = id.chars().take(ECHOED_ID_MAX_CHARS).collect();
            if echoed.len() < id.len() {
                echoed.push_str("...");
            }
            format!("no repair packet for gap `{echoed}`")
        }
        None => "no repair packet".to_string(),
    };
    format!(
        "{subject}: neither {DEFAULT_ACTIONABLE_GAPS_OUT} nor {DEFAULT_GAP_DECISION_LEDGER_OUT} \
holds one; run `ripr pilot --root .` for seams with their repair commands, or write the gap \
ledger with `ripr reports gap-ledger --check-output PATH` from saved `ripr check --format json` \
output"
    )
}

pub(super) fn repair_packet_sentinel(reason: &str) -> LSPAny {
    serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "kind": "repair_packet",
        "status": "not_actionable_or_incomplete",
        "reason": reason,
    })
}

/// Typed-degradation reasons for a present-but-unparseable packet source.
/// Vocabulary mirrors `limitation_non_claims("malformed_artifact")`: the
/// client must be able to tell "no packet" apart from "packet source corrupt;
/// artifact regeneration required before exposure can be assessed".
pub(super) const MALFORMED_ACTIONABLE_GAPS_REASON: &str = "actionable-gaps.json is malformed; artifact regeneration required before exposure can be assessed";
pub(super) const UNBOUND_PACKET_COMMANDS_REASON: &str =
    "packet commands cannot be bound to the selected workspace";

pub(super) const MALFORMED_GAP_LEDGER_REASON: &str = "gap-decision-ledger.json is malformed; artifact regeneration required before exposure can be assessed";

pub(super) fn gap_record_matches(record: &GapRecord, gap_id: &str) -> bool {
    record.gap_id == gap_id || record.canonical_gap_id == gap_id
}

pub(super) fn evidence_context_packet(
    snapshot: &AnalysisSnapshot,
    entry: &ClassifiedSeam,
) -> LSPAny {
    // #4001/#3999: loop commands bind the snapshot's selected workspace root,
    // not the language-server process working directory; the packet's
    // `root` field stays the portable role.
    let root = loop_commands::bound_root(&snapshot.root.to_string_lossy());
    let seam = &entry.seam;
    let evidence = &entry.evidence;
    let seam_id = seam.id().as_str();
    let outline = targeted_test_brief_outline_for_classified_seam(entry);
    let related_test = evidence.related_tests.first();
    let missing_discriminator = evidence
        .missing_discriminators
        .first()
        .map(|missing| missing.value.as_str());
    serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "root": ".",
        "base": snapshot.base.as_deref(),
        "mode": snapshot.mode.as_str(),
        "seam_id": seam_id,
        "file": display_path(seam.file()),
        "range": {
            "start": seam.display_line(),
            "end": seam.display_line(),
        },
        "class": entry.class.as_str(),
        "seam_kind": seam.kind().as_str(),
        "owner": seam.owner(),
        "expression": seam.expression(),
        "required_discriminator": seam.required_discriminator().as_str(),
        "expected_sink": seam.expected_sink().as_str(),
        "evidence_path": {
            "reach": evidence_stage_status(&evidence.reach),
            "activate": evidence_stage_status(&evidence.activate),
            "propagate": evidence_stage_status(&evidence.propagate),
            "observe": evidence_stage_status(&evidence.observe),
            "discriminate": evidence_stage_status(&evidence.discriminate),
        },
        "evidence_summaries": {
            "reach": evidence.reach.summary.as_str(),
            "activate": evidence.activate.summary.as_str(),
            "propagate": evidence.propagate.summary.as_str(),
            "observe": evidence.observe.summary.as_str(),
            "discriminate": evidence.discriminate.summary.as_str(),
        },
        "missing_discriminator": missing_discriminator,
        "missing_discriminators": evidence.missing_discriminators.iter().map(|missing| {
            serde_json::json!({
                "value": missing.value.as_str(),
                "reason": missing.reason.as_str(),
            })
        }).collect::<Vec<_>>(),
        "related_test": related_test.map(|test| {
            format!("{}::{}", display_path(&test.file), test.test_name)
        }),
        "related_test_location": related_test.map(|test| {
            serde_json::json!({
                "file": display_path(&test.file),
                "line": test.line,
                "test_name": test.test_name.as_str(),
                "oracle_kind": test.oracle_kind.as_str(),
                "oracle_strength": test.oracle_strength.as_str(),
            })
        }),
        "suggested_assertion": suggested_assertion_for_classified_seam(entry),
        "suggested_test": {
            "file": outline.suggested_file,
            "name": outline.suggested_name,
            "candidate_value": outline.candidate_value,
            "assertion_shape": outline.assertion_shape,
        },
        // The repair start only for a seam `agent repair` would accept (the
        // fail-closed repair-packet flip, RIPR-SPEC-0087 §8, plus a
        // test-surface target); `null` otherwise (#3906).
        "repair_command": repair_start_command_for(entry, &root),
        "agent_packet_command": loop_commands::agent_packet_command(
            &root,
            seam_id,
            loop_commands::EDITOR_AGENT_PACKET_ARTIFACT,
        ),
        "agent_brief_command": loop_commands::agent_brief_command(
            &root,
            seam_id,
            loop_commands::EDITOR_AGENT_BRIEF_ARTIFACT,
        ),
        "after_snapshot_command": loop_commands::check_repo_exposure_command_with_base(
            &root,
            snapshot.base.as_deref(),
            snapshot.mode.as_str(),
            loop_commands::PILOT_AFTER_SNAPSHOT_ARTIFACT,
        ),
        "verify_command": loop_commands::agent_verify_command(
            &root,
            loop_commands::PILOT_BEFORE_SNAPSHOT_ARTIFACT,
            loop_commands::PILOT_AFTER_SNAPSHOT_ARTIFACT,
            Some(loop_commands::EDITOR_AGENT_VERIFY_ARTIFACT),
        ),
        "receipt_command": loop_commands::agent_receipt_command(
            &root,
            loop_commands::EDITOR_AGENT_VERIFY_ARTIFACT,
            seam_id,
            Some(loop_commands::EDITOR_AGENT_RECEIPT_ARTIFACT),
        ),
        "limits_note": "Static evidence only; no runtime mutation execution.",
    })
}

pub(super) fn evidence_stage_status(evidence: &StageEvidence) -> &'static str {
    match evidence.state {
        StageState::Yes => "present",
        StageState::Weak => "weak",
        StageState::No => "missing",
        StageState::Unknown => "unknown",
        StageState::Opaque => "opaque",
        StageState::NotApplicable => "not_applicable",
    }
}
