use serde_json::{Map, Value, json};

const QUEUED_STATE: &str = "queued";
const BLOCKED_STALE_STATE: &str = "blocked_stale";
const STALE_STATUS: &str = "stale";
const BLOCKED_PACKET_GUARDRAIL: &str =
    "blocked_packets are review-only and must not be assigned or promoted into repair work";

/// Project the renderer's typed candidate set into the bounded frontier an
/// external agent may actually take. The output renderer remains authority
/// for validation, freshness, source ordering, and conflict identity; this
/// projection only keeps typed non-assignable records from consuming `--top`.
pub(super) fn project_assignable_frontier(rendered: &str, top: usize) -> Result<String, String> {
    let mut envelope = serde_json::from_str::<Value>(rendered)
        .map_err(|error| format!("swarm queue renderer produced invalid JSON: {error}"))?;
    let object = envelope
        .as_object_mut()
        .ok_or_else(|| "swarm queue renderer must produce a JSON object".to_string())?;
    let raw_packets = match object.remove("packets") {
        Some(Value::Array(packets)) => packets,
        Some(_) => {
            return Err("swarm queue renderer packets must be a JSON array".to_string());
        }
        None => return Err("swarm queue renderer is missing packets".to_string()),
    };

    let mut assignable_packets = Vec::new();
    let mut blocked_packets = Vec::new();
    let mut blocked_stale_total = 0usize;
    for packet in raw_packets {
        let (assignable, blocked_stale) = {
            let packet_object = packet
                .as_object()
                .ok_or_else(|| "swarm queue renderer packet must be an object".to_string())?;
            let queue_state = required_string(packet_object, "queue_state")?;
            let staleness_status = required_string(packet_object, "staleness_status")?;
            (
                queue_state == QUEUED_STATE && staleness_status != STALE_STATUS,
                queue_state == BLOCKED_STALE_STATE || staleness_status == STALE_STATUS,
            )
        };
        if assignable {
            assignable_packets.push(packet);
        } else {
            if blocked_stale {
                blocked_stale_total += 1;
            }
            blocked_packets.push(blocked_packet_projection(&packet)?);
        }
    }

    let assignable_total = assignable_packets.len();
    let mut selected_packets: Vec<_> = assignable_packets.into_iter().take(top).collect();
    for (index, packet) in selected_packets.iter_mut().enumerate() {
        let packet_object = packet
            .as_object_mut()
            .ok_or_else(|| "swarm queue selected packet must be an object".to_string())?;
        packet_object.insert("priority".to_string(), json!(index + 1));
    }
    let returned = selected_packets.len();
    let unreturned_assignable_total = assignable_total.saturating_sub(returned);

    let blocked_total = blocked_packets.len();
    let selected_blocked_packets: Vec<_> = blocked_packets.into_iter().take(top).collect();
    let blocked_returned = selected_blocked_packets.len();
    let blocked_unreturned_total = blocked_total.saturating_sub(blocked_returned);

    let inputs = object
        .get_mut("inputs")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "swarm queue renderer is missing object inputs".to_string())?;
    inputs.insert("top".to_string(), json!(top));

    let summary = object
        .get_mut("summary")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "swarm queue renderer is missing object summary".to_string())?;
    summary.insert("assignable_total".to_string(), json!(assignable_total));
    summary.insert("returned".to_string(), json!(returned));
    summary.insert(
        "unreturned_assignable_total".to_string(),
        json!(unreturned_assignable_total),
    );
    summary.insert("blocked_total".to_string(), json!(blocked_total));
    summary.insert(
        "blocked_stale_total".to_string(),
        json!(blocked_stale_total),
    );
    summary.insert("blocked_returned".to_string(), json!(blocked_returned));
    summary.insert(
        "blocked_unreturned_total".to_string(),
        json!(blocked_unreturned_total),
    );

    let must_not_infer = object
        .get_mut("must_not_infer")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "swarm queue renderer is missing must_not_infer array".to_string())?;
    if !must_not_infer
        .iter()
        .any(|item| item.as_str() == Some(BLOCKED_PACKET_GUARDRAIL))
    {
        must_not_infer.push(Value::String(BLOCKED_PACKET_GUARDRAIL.to_string()));
    }

    object.insert(
        "selection_policy".to_string(),
        json!({
            "order": "upstream",
            "assignable_when": "queue_state=queued and staleness_status!=stale",
            "top_applies_to": "assignable_packets",
            "blocked_projection": "review_only_bounded_by_top",
        }),
    );
    object.insert("packets".to_string(), Value::Array(selected_packets));
    object.insert(
        "blocked_packets".to_string(),
        Value::Array(selected_blocked_packets),
    );

    let mut projected = serde_json::to_string_pretty(&envelope)
        .map_err(|error| format!("render assignable swarm queue JSON failed: {error}"))?;
    projected.push('\n');
    Ok(projected)
}

fn blocked_packet_projection(packet: &Value) -> Result<Value, String> {
    let object = packet
        .as_object()
        .ok_or_else(|| "swarm queue blocked packet must be an object".to_string())?;
    let source_index = object
        .get("source_index")
        .and_then(Value::as_u64)
        .ok_or_else(|| "swarm queue blocked packet requires numeric source_index".to_string())?;
    let gap_id = required_string(object, "gap_id")?;
    let canonical_gap_id = match object.get("canonical_gap_id") {
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.as_str()),
        Some(Value::Null) => None,
        Some(_) => {
            return Err(
                "swarm queue blocked packet canonical_gap_id must be nonblank or null".to_string(),
            );
        }
        None => {
            return Err("swarm queue blocked packet is missing canonical_gap_id".to_string());
        }
    };
    let queue_state = required_string(object, "queue_state")?;
    let staleness_status = required_string(object, "staleness_status")?;
    let staleness_reason = required_string(object, "staleness_reason")?;
    let conflict_group = required_string(object, "conflict_group")?;

    Ok(json!({
        "source_index": source_index,
        "gap_id": gap_id,
        "canonical_gap_id": canonical_gap_id,
        "queue_state": queue_state,
        "staleness_status": staleness_status,
        "staleness_reason": staleness_reason,
        "conflict_group": conflict_group,
    }))
}

fn required_string<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("swarm queue renderer packet requires nonblank {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(
        source_index: usize,
        gap_id: &str,
        queue_state: &str,
        staleness_status: &str,
    ) -> Value {
        json!({
            "priority": source_index + 1,
            "source_index": source_index,
            "queue_state": queue_state,
            "staleness_status": staleness_status,
            "staleness_reason": format!("{gap_id} currentness state"),
            "gap_id": gap_id,
            "canonical_gap_id": format!("canonical:{gap_id}"),
            "conflict_group": format!("file:tests/{source_index}.rs"),
            "verify_command": format!("cargo test packet_{source_index}"),
            "receipt_command": format!("ripr agent receipt --seam-id {gap_id}"),
            "suggested_test_file": format!("tests/{source_index}.rs"),
            "allowed_edit_surface": [format!("tests/{source_index}.rs")],
            "packet_command_args": ["ripr", "agent", "packet", "--gap-id", gap_id],
        })
    }

    fn raw_queue(packets: Vec<Value>) -> String {
        let packets_total = packets.len();
        json!({
            "inputs": {"top": packets_total},
            "summary": {
                "queue_total": packets_total,
                "returned": packets_total,
            },
            "packets": packets,
            "must_not_infer": [],
        })
        .to_string()
    }

    #[test]
    fn stale_packet_does_not_consume_the_assignable_top_frontier() -> Result<(), String> {
        let raw = raw_queue(vec![
            packet(0, "gap:stale", BLOCKED_STALE_STATE, STALE_STATUS),
            packet(1, "gap:assignable", QUEUED_STATE, "not_evaluated"),
        ]);
        let json = project_assignable_frontier(&raw, 1)?;
        let value = serde_json::from_str::<Value>(&json)
            .map_err(|error| format!("projected queue JSON should parse: {error}"))?;
        let selected = value
            .get("packets")
            .and_then(Value::as_array)
            .and_then(|packets| packets.first())
            .ok_or_else(|| format!("missing selected packet in: {json}"))?;
        assert_eq!(
            selected.get("gap_id").and_then(Value::as_str),
            Some("gap:assignable")
        );
        assert_eq!(
            selected.get("source_index").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(selected.get("priority").and_then(Value::as_u64), Some(1));

        let blocked = value
            .get("blocked_packets")
            .and_then(Value::as_array)
            .and_then(|packets| packets.first())
            .ok_or_else(|| format!("missing blocked packet in: {json}"))?;
        assert_eq!(
            blocked.get("gap_id").and_then(Value::as_str),
            Some("gap:stale")
        );
        assert_eq!(blocked.get("source_index").and_then(Value::as_u64), Some(0));
        for forbidden in [
            "packet_command_args",
            "verify_command",
            "receipt_command",
            "suggested_test_file",
            "allowed_edit_surface",
        ] {
            assert!(
                blocked.get(forbidden).is_none(),
                "blocked review projection leaked {forbidden}: {blocked}"
            );
        }

        let summary = value
            .get("summary")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("missing summary in: {json}"))?;
        assert_eq!(
            summary.get("assignable_total").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(summary.get("returned").and_then(Value::as_u64), Some(1));
        assert_eq!(
            summary.get("blocked_total").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            summary.get("blocked_stale_total").and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            value
                .get("selection_policy")
                .and_then(|policy| policy.get("top_applies_to"))
                .and_then(Value::as_str),
            Some("assignable_packets")
        );
        Ok(())
    }

    #[test]
    fn stale_only_queue_returns_review_state_without_assignable_packets() -> Result<(), String> {
        let raw = raw_queue(vec![packet(
            0,
            "gap:stale",
            BLOCKED_STALE_STATE,
            STALE_STATUS,
        )]);
        let json = project_assignable_frontier(&raw, 1)?;
        let value = serde_json::from_str::<Value>(&json)
            .map_err(|error| format!("projected queue JSON should parse: {error}"))?;
        assert!(
            value
                .get("packets")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty),
            "stale-only queue emitted assignable packets: {json}"
        );
        assert_eq!(
            value
                .get("blocked_packets")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value
                .get("summary")
                .and_then(|summary| summary.get("assignable_total"))
                .and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            value
                .get("summary")
                .and_then(|summary| summary.get("blocked_stale_total"))
                .and_then(Value::as_u64),
            Some(1)
        );
        Ok(())
    }

    #[test]
    fn projection_fails_closed_without_typed_queue_state() -> Result<(), String> {
        let raw = raw_queue(vec![json!({
            "source_index": 0,
            "staleness_status": "not_evaluated",
        })]);
        let error = project_assignable_frontier(&raw, 1)
            .err()
            .ok_or_else(|| "queue packet without queue_state was projected".to_string())?;
        assert!(
            error.contains("queue_state"),
            "missing typed state should name queue_state: {error}"
        );
        Ok(())
    }
}
