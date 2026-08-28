use super::currentness::{
    BLOCKED_STALE, CURRENT, GapRecordSourceCurrentness, NOT_EVALUATED, QUEUED, STALE,
};
use crate::app::causal_projection::CausalDeltaArtifact;
use crate::output::agent_seam_packets_legacy as legacy;
use crate::output::gap_decision_ledger::GapRecord;
use serde_json::{Value, json};

pub(crate) fn render_agent_gap_record_queue_json_with_currentness(
    root: &str,
    gap_ledger_path: &str,
    records: &[GapRecord],
    language: &str,
    top: usize,
    source_currentness: &GapRecordSourceCurrentness,
) -> Result<String, String> {
    let rendered = legacy::render_agent_gap_record_queue_json(
        root,
        gap_ledger_path,
        records,
        language,
        records.len().max(top),
    )?;
    let mut envelope = serde_json::from_str::<Value>(&rendered)
        .map_err(|error| format!("parse typed swarm queue JSON failed: {error}"))?;
    let object = envelope
        .as_object_mut()
        .ok_or_else(|| "typed swarm queue renderer must produce an object".to_string())?;
    let packets = match object.remove("packets") {
        Some(Value::Array(packets)) => packets,
        Some(_) => return Err("typed swarm queue packets must be an array".to_string()),
        None => return Err("typed swarm queue renderer is missing packets".to_string()),
    };

    let mut current_total = 0usize;
    let mut stale_total = 0usize;
    let mut not_evaluated_total = 0usize;
    let mut assignable_total = 0usize;
    let mut decorated = Vec::with_capacity(packets.len());
    for mut packet in packets {
        let effective = effective_packet_currentness(&packet, source_currentness)?;
        match effective.status.as_str() {
            CURRENT => current_total += 1,
            STALE => stale_total += 1,
            _ => not_evaluated_total += 1,
        }
        if effective.is_assignable() {
            assignable_total += 1;
        }
        decorate_packet_currentness(&mut packet, &effective)?;
        decorated.push(packet);
    }

    let mut selected: Vec<_> = decorated.into_iter().take(top).collect();
    for (index, packet) in selected.iter_mut().enumerate() {
        if let Some(packet) = packet.as_object_mut() {
            packet.insert("priority".to_string(), json!(index + 1));
        }
    }
    let returned = selected.len();
    let returned_assignable_total = selected
        .iter()
        .filter(|packet| {
            packet.get("queue_state").and_then(Value::as_str) == Some(QUEUED)
                && packet.get("staleness_status").and_then(Value::as_str) == Some(CURRENT)
        })
        .count();

    let inputs = object
        .get_mut("inputs")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "typed swarm queue is missing inputs".to_string())?;
    inputs.insert("top".to_string(), json!(top));
    let summary = object
        .get_mut("summary")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "typed swarm queue is missing summary".to_string())?;
    summary.insert("returned".to_string(), json!(returned));
    summary.insert("current_total".to_string(), json!(current_total));
    summary.insert("stale_total".to_string(), json!(stale_total));
    summary.insert(
        "not_evaluated_total".to_string(),
        json!(not_evaluated_total),
    );
    summary.insert("assignable_total".to_string(), json!(assignable_total));
    summary.insert(
        "returned_assignable_total".to_string(),
        json!(returned_assignable_total),
    );
    summary.insert(
        "blocked_total".to_string(),
        json!(stale_total + not_evaluated_total),
    );

    object.insert("source_currentness".to_string(), source_currentness.json());
    object.insert(
        "assignment_policy".to_string(),
        json!({
            "assignable_when": "queue_state=queued and staleness_status=current",
            "not_evaluated_is_assignable": false,
            "source_currentness_owner": "producer_validated_repo_exposure",
        }),
    );
    object.insert("packets".to_string(), Value::Array(selected));
    if assignable_total == 0 && current_total + stale_total + not_evaluated_total > 0 {
        object.insert("status".to_string(), Value::String("blocked".to_string()));
    }
    let must_not_infer = object
        .get_mut("must_not_infer")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "typed swarm queue is missing must_not_infer".to_string())?;
    must_not_infer.push(Value::String(
        "only queue_state=queued with staleness_status=current is assignable; stale and not_evaluated packets are review-only"
            .to_string(),
    ));

    let mut rendered = serde_json::to_string_pretty(&envelope)
        .map_err(|error| format!("render typed swarm queue JSON failed: {error}"))?;
    rendered.push('\n');
    Ok(rendered)
}

pub(crate) fn render_agent_gap_record_packet_json_with_live_currentness(
    gap_ledger_path: &str,
    record: &GapRecord,
    causal_projection: Option<&CausalDeltaArtifact>,
    source_currentness: &GapRecordSourceCurrentness,
) -> Result<String, String> {
    let legacy_packet = legacy::render_agent_gap_record_packet_json_with_causal(
        gap_ledger_path,
        record,
        causal_projection,
    )?;
    let queue = legacy::render_agent_gap_record_queue_json(
        ".",
        gap_ledger_path,
        std::slice::from_ref(record),
        &record.language,
        1,
    )?;
    let queue_value = serde_json::from_str::<Value>(&queue)
        .map_err(|error| format!("parse packet currentness source failed: {error}"))?;
    let queue_packet = queue_value
        .get("packets")
        .and_then(Value::as_array)
        .and_then(|packets| packets.first())
        .ok_or_else(|| "agent packet currentness source did not emit a candidate".to_string())?;
    let effective = effective_packet_currentness(queue_packet, source_currentness)?;

    if effective.is_assignable() {
        let mut envelope = serde_json::from_str::<Value>(&legacy_packet)
            .map_err(|error| format!("parse agent gap packet JSON failed: {error}"))?;
        let object = envelope
            .as_object_mut()
            .ok_or_else(|| "agent gap packet renderer must produce an object".to_string())?;
        let packet = object
            .get_mut("packets")
            .and_then(Value::as_array_mut)
            .and_then(|packets| packets.first_mut())
            .ok_or_else(|| "agent gap packet renderer is missing its packet".to_string())?;
        decorate_packet_currentness(packet, &effective)?;
        object.insert("status".to_string(), Value::String("advisory".to_string()));
        object.insert("source_currentness".to_string(), effective.json());
        let mut rendered = serde_json::to_string_pretty(&envelope)
            .map_err(|error| format!("render current agent gap packet failed: {error}"))?;
        rendered.push('\n');
        return Ok(rendered);
    }

    render_blocked_agent_gap_packet(gap_ledger_path, record, &effective)
}

fn render_blocked_agent_gap_packet(
    gap_ledger_path: &str,
    record: &GapRecord,
    currentness: &GapRecordSourceCurrentness,
) -> Result<String, String> {
    let gap_id = if record.gap_id.trim().is_empty() {
        record.canonical_gap_id.as_str()
    } else {
        record.gap_id.as_str()
    };
    let canonical_gap_id = if record.canonical_gap_id.trim().is_empty() {
        None
    } else {
        Some(record.canonical_gap_id.as_str())
    };
    let envelope = json!({
        "schema_version": crate::app::AGENT_SEAM_PACKET_SCHEMA_VERSION,
        "scope": "repo",
        "source": "gap_decision_ledger",
        "status": "blocked",
        "inputs": {
            "gap_ledger": gap_ledger_path,
        },
        "source_currentness": currentness.json(),
        "packets_total": 0,
        "packets": [],
        "blocked_candidate": {
            "gap_id": gap_id,
            "canonical_gap_id": canonical_gap_id,
            "language": record.language.as_str(),
            "queue_state": currentness.queue_state.as_str(),
            "staleness_status": currentness.status.as_str(),
            "staleness_reason": currentness.reason.as_str(),
            "refresh_commands": &currentness.refresh_commands,
        },
        "must_not_infer": [
            "blocked_candidate is review-only and carries no edit, verify, receipt, or packet command authority",
            "source_currentness=candidate_current does not establish live packet currentness",
            "stale or not_evaluated currentness must be refreshed before assignment"
        ],
    });
    let mut rendered = serde_json::to_string_pretty(&envelope)
        .map_err(|error| format!("render blocked agent gap packet failed: {error}"))?;
    rendered.push('\n');
    Ok(rendered)
}

fn effective_packet_currentness(
    packet: &Value,
    source_currentness: &GapRecordSourceCurrentness,
) -> Result<GapRecordSourceCurrentness, String> {
    let object = packet
        .as_object()
        .ok_or_else(|| "agent packet currentness candidate must be an object".to_string())?;
    let queue_state = object
        .get("queue_state")
        .and_then(Value::as_str)
        .ok_or_else(|| "agent packet currentness candidate is missing queue_state".to_string())?;
    let status = object
        .get("staleness_status")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "agent packet currentness candidate is missing staleness_status".to_string()
        })?;
    let reason = object
        .get("staleness_reason")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "agent packet currentness candidate is missing staleness_reason".to_string()
        })?;

    match (queue_state, status) {
        (BLOCKED_STALE, STALE) => Ok(GapRecordSourceCurrentness::stale(
            reason,
            source_currentness.refresh_commands.clone(),
            source_currentness.source_kind.clone(),
            source_currentness.source_path.clone(),
        )),
        (QUEUED, NOT_EVALUATED) | (QUEUED, CURRENT) => Ok(source_currentness.clone()),
        _ => Ok(GapRecordSourceCurrentness::not_evaluated(
            format!(
                "packet producer emitted unsupported currentness combination queue_state={queue_state}, staleness_status={status}"
            ),
            source_currentness.refresh_commands.clone(),
            source_currentness.source_kind.clone(),
            source_currentness.source_path.clone(),
        )),
    }
}

fn decorate_packet_currentness(
    packet: &mut Value,
    currentness: &GapRecordSourceCurrentness,
) -> Result<(), String> {
    let object = packet
        .as_object_mut()
        .ok_or_else(|| "agent packet currentness target must be an object".to_string())?;
    object.insert(
        "queue_state".to_string(),
        Value::String(currentness.queue_state.clone()),
    );
    object.insert(
        "staleness_status".to_string(),
        Value::String(currentness.status.clone()),
    );
    object.insert(
        "staleness_reason".to_string(),
        Value::String(currentness.reason.clone()),
    );
    object.insert("source_currentness".to_string(), currentness.json());
    object.insert(
        "assignment".to_string(),
        json!({
            "eligible": currentness.is_assignable(),
            "reason": if currentness.is_assignable() {
                "producer-validated live-current packet"
            } else {
                currentness.reason.as_str()
            },
        }),
    );
    if !currentness.is_assignable() {
        strip_assignment_authority(object);
        object.insert(
            "refresh_commands".to_string(),
            json!(&currentness.refresh_commands),
        );
    }
    Ok(())
}

fn strip_assignment_authority(object: &mut serde_json::Map<String, Value>) {
    for field in [
        "packet_command_args",
        "verify_command",
        "receipt_command",
        "verification_commands",
        "command_specs",
        "suggested_test_file",
        "suggested_test_name",
        "allowed_edit_surface",
        "allowed_files",
        "forbidden_files",
    ] {
        object.remove(field);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_record(receipt: Option<Value>) -> Result<GapRecord, String> {
        serde_json::from_value(json!({
            "gap_id": "gap:python:pricing-boundary",
            "source_currentness": "candidate_current",
            "canonical_gap_id": "gap:python:src/pricing.py:calculate_discount:predicate_boundary",
            "kind": "MissingBoundaryAssertion",
            "language": "python",
            "language_status": "preview",
            "scope": "repo",
            "evidence_class": "predicate_boundary",
            "gap_state": "actionable",
            "policy_state": "new",
            "repairability": "repairable",
            "anchor": {
                "file": "src/pricing.py",
                "line": 7,
                "owner": "calculate_discount"
            },
            "repair_route": {
                "route_kind": "AddBoundaryAssertion",
                "target_file": "tests/test_pricing.py",
                "assertion_shape": "assert calculate_discount(100, 100) == 90",
                "changed_behavior": "amount >= threshold"
            },
            "verification_commands": ["pytest tests/test_pricing.py"],
            "receipt_command": "ripr agent receipt --verify-json target/ripr/workflow/verify.json --seam-id gap:python:pricing-boundary --test-changed tests/test_pricing.py",
            "receipt": receipt,
            "projection_eligibility": {
                "agent_packet": {
                    "eligible": true,
                    "reason": "bounded repair route"
                }
            }
        }))
        .map_err(|error| format!("build GapRecord fixture failed: {error}"))
    }

    fn current_source() -> GapRecordSourceCurrentness {
        GapRecordSourceCurrentness::current(
            "exact current fixture",
            vec!["refresh source".to_string(), "refresh ledger".to_string()],
            Some("repo_exposure".to_string()),
            Some("target/ripr/reports/repo-exposure.json".to_string()),
        )
    }

    fn unknown_source() -> GapRecordSourceCurrentness {
        GapRecordSourceCurrentness::not_evaluated(
            "legacy ledger has no live snapshot binding",
            vec!["refresh source".to_string(), "refresh ledger".to_string()],
            None,
            None,
        )
    }

    #[test]
    fn candidate_current_alone_remains_non_assignable() -> Result<(), String> {
        let record = valid_record(None)?;
        let json = render_agent_gap_record_queue_json_with_currentness(
            ".",
            "gap-ledger.json",
            &[record],
            "python",
            1,
            &unknown_source(),
        )?;
        let value = serde_json::from_str::<Value>(&json)
            .map_err(|error| format!("queue JSON should parse: {error}"))?;
        let packet = value
            .get("packets")
            .and_then(Value::as_array)
            .and_then(|packets| packets.first())
            .ok_or_else(|| format!("missing review-only packet: {json}"))?;
        assert_eq!(
            packet.get("queue_state").and_then(Value::as_str),
            Some(super::super::currentness::BLOCKED_NOT_EVALUATED)
        );
        assert_eq!(
            packet.get("staleness_status").and_then(Value::as_str),
            Some(NOT_EVALUATED)
        );
        assert!(packet.get("packet_command_args").is_none());
        assert!(packet.get("verify_command").is_none());
        assert_eq!(
            value
                .get("summary")
                .and_then(|summary| summary.get("assignable_total"))
                .and_then(Value::as_u64),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn exact_current_source_keeps_packet_assignment_surface() -> Result<(), String> {
        let record = valid_record(None)?;
        let json = render_agent_gap_record_queue_json_with_currentness(
            ".",
            "gap-ledger.json",
            &[record],
            "python",
            1,
            &current_source(),
        )?;
        let value = serde_json::from_str::<Value>(&json)
            .map_err(|error| format!("queue JSON should parse: {error}"))?;
        let packet = value
            .get("packets")
            .and_then(Value::as_array)
            .and_then(|packets| packets.first())
            .ok_or_else(|| format!("missing current packet: {json}"))?;
        assert_eq!(
            packet.get("queue_state").and_then(Value::as_str),
            Some(QUEUED)
        );
        assert_eq!(
            packet.get("staleness_status").and_then(Value::as_str),
            Some(CURRENT)
        );
        assert!(packet.get("packet_command_args").is_some());
        assert!(packet.get("verify_command").is_some());
        Ok(())
    }

    #[test]
    fn receipt_stale_overrides_current_source() -> Result<(), String> {
        let record = valid_record(Some(json!({"state": "stale"})))?;
        let json = render_agent_gap_record_queue_json_with_currentness(
            ".",
            "gap-ledger.json",
            &[record],
            "python",
            1,
            &current_source(),
        )?;
        let value = serde_json::from_str::<Value>(&json)
            .map_err(|error| format!("queue JSON should parse: {error}"))?;
        let packet = value
            .get("packets")
            .and_then(Value::as_array)
            .and_then(|packets| packets.first())
            .ok_or_else(|| format!("missing stale review packet: {json}"))?;
        assert_eq!(
            packet.get("queue_state").and_then(Value::as_str),
            Some(BLOCKED_STALE)
        );
        assert_eq!(
            packet.get("staleness_status").and_then(Value::as_str),
            Some(STALE)
        );
        assert!(packet.get("packet_command_args").is_none());
        Ok(())
    }

    #[test]
    fn queue_and_single_packet_copy_the_same_non_current_disposition() -> Result<(), String> {
        let record = valid_record(None)?;
        let source = unknown_source();
        let queue = render_agent_gap_record_queue_json_with_currentness(
            ".",
            "gap-ledger.json",
            std::slice::from_ref(&record),
            "python",
            1,
            &source,
        )?;
        let packet = render_agent_gap_record_packet_json_with_live_currentness(
            "gap-ledger.json",
            &record,
            None,
            &source,
        )?;
        let queue_value = serde_json::from_str::<Value>(&queue)
            .map_err(|error| format!("queue JSON should parse: {error}"))?;
        let packet_value = serde_json::from_str::<Value>(&packet)
            .map_err(|error| format!("packet JSON should parse: {error}"))?;
        let queue_candidate = queue_value
            .get("packets")
            .and_then(Value::as_array)
            .and_then(|packets| packets.first())
            .ok_or_else(|| format!("missing queue candidate: {queue}"))?;
        let blocked_candidate = packet_value
            .get("blocked_candidate")
            .ok_or_else(|| format!("missing blocked candidate: {packet}"))?;
        assert_eq!(
            queue_candidate.get("queue_state").and_then(Value::as_str),
            blocked_candidate.get("queue_state").and_then(Value::as_str)
        );
        assert_eq!(
            queue_candidate
                .get("staleness_status")
                .and_then(Value::as_str),
            blocked_candidate
                .get("staleness_status")
                .and_then(Value::as_str)
        );
        assert!(
            packet_value
                .get("packets")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        );
        for forbidden in [
            "packet_command_args",
            "verify_command",
            "receipt_command",
            "allowed_edit_surface",
            "allowed_files",
        ] {
            assert!(
                !packet.contains(&format!("\"{forbidden}\"")),
                "blocked single packet leaked {forbidden}: {packet}"
            );
        }
        Ok(())
    }
}
