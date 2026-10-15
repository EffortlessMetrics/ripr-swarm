use super::*;
use tokio::io::AsyncReadExt;

#[test]
fn readable_giant_id_cannot_escape_the_output_cap() -> Result<(), String> {
    let message = ServerJsonRpcMessage::error(
        ErrorData::internal_error("failure", None),
        Some(rmcp::model::RequestId::String(
            "\n".repeat(70 * 1024).into(),
        )),
    );
    let error = encode_message(&message)
        .err()
        .ok_or_else(|| "giant correlated ID unexpectedly fit".to_string())?;
    if error.to_string() != "MCP output limit" {
        return Err("giant ID refusal lost bounded reason".into());
    }
    Ok(())
}

/// #7768: the semantic layers compare envelopes against
/// `MAX_RESPONSE_BYTES` minus the request ID minus
/// `OUTER_RESPONSE_OVERHEAD_BYTES`. That reserve is honest only while the
/// pinned SDK (rmcp 3.5.1) adds no more than the reserve around the
/// semantically measured envelope when it serializes the complete
/// response. Measure both negotiated shapes through the real encoder: the
/// fixed frame fields are exactly 33 payload bytes
/// (`{"jsonrpc":"2.0","id":` + `,"result":` + `}`; the newline follows the
/// cap check), the modern `resultType` field adds exactly 24, and both
/// must stay inside the reserve.
#[test]
fn pinned_sdk_wraps_the_measured_envelope_within_the_outer_reserve() -> Result<(), String> {
    let envelope = serde_json::json!({
        "content": [{ "type": "text", "text": "boundary pin" }],
        "structuredContent": { "schema_version": "outer-budget-pin" },
        "isError": false,
    });
    let envelope_bytes = serde_json::to_vec(&envelope)
        .map(|bytes| bytes.len())
        .map_err(|error| error.to_string())?;
    let id = rmcp::model::RequestId::String("budget-pin".to_owned().into());
    let id_bytes = serde_json::to_vec(&id)
        .map(|bytes| bytes.len())
        .map_err(|error| error.to_string())?;
    for (shape, modern, expected_fixed) in [
        ("modern resultType", true, 33 + 24),
        ("legacy stripped shape", false, 33),
    ] {
        let mut tool_result: rmcp::model::CallToolResult =
            serde_json::from_value(envelope.clone()).map_err(|error| error.to_string())?;
        tool_result.result_type = modern.then_some(rmcp::model::ResultType::COMPLETE);
        let message = ServerJsonRpcMessage::response(
            rmcp::model::ServerResult::CallToolResult(tool_result),
            id.clone(),
        );
        let frame = encode_message(&message).map_err(|error| error.to_string())?;
        let overhead = frame.len() - 1 - id_bytes - envelope_bytes;
        if overhead != expected_fixed {
            return Err(format!(
                "pinned rmcp {shape} overhead drifted: {overhead} != {expected_fixed}"
            ));
        }
        if overhead > super::super::OUTER_RESPONSE_OVERHEAD_BYTES {
            return Err(format!(
                "pinned rmcp {shape} overhead {overhead} exceeds the outer reserve"
            ));
        }
    }
    Ok(())
}

/// #7768: the budget charges the request ID's real JSON serialization, so
/// every escaping multiplier class must be accounted exactly. For each id,
/// an envelope sized from the id-aware budget must fit the writer cap with
/// the full modern-shape wrapper, and the fixed frame arithmetic must hold
/// byte-exactly through the real serializer.
#[test]
fn request_id_escaping_is_charged_exactly_by_the_outer_budget() -> Result<(), String> {
    const RESPONSE_BOUND: usize = 128 * 1024;
    const RESULT_TYPE_FIELD: usize = 24;
    let envelope = serde_json::json!({
        "content": [{ "type": "text", "text": "escaping class probe" }],
        "structuredContent": { "schema_version": "outer-budget-pin" },
        "isError": false,
    });
    let envelope_bytes = serde_json::to_vec(&envelope)
        .map(|bytes| bytes.len())
        .map_err(|error| error.to_string())?;
    let mut tool_result: rmcp::model::CallToolResult =
        serde_json::from_value(envelope.clone()).map_err(|error| error.to_string())?;
    tool_result.result_type = Some(rmcp::model::ResultType::COMPLETE);
    // Quote and backslash double, control characters escape to six bytes,
    // non-ASCII stays unescaped UTF-8 (two and four bytes per char).
    let id_classes = [
        ("plain", "req-1".to_owned()),
        ("quotes", "\"".repeat(64)),
        ("backslashes", "\\".repeat(64)),
        ("control", "\u{1}".repeat(64)),
        ("non-ascii", "é".repeat(64)),
        ("emoji", "\u{1F600}".repeat(64)),
    ];
    for (class, id_text) in id_classes {
        let id = rmcp::model::RequestId::String(id_text.clone().into());
        let id_bytes = serde_json::to_vec(&id)
            .map(|bytes| bytes.len())
            .map_err(|error| error.to_string())?;
        let budget = super::super::outer_envelope_budget(&id);
        if budget != RESPONSE_BOUND - id_bytes - super::super::OUTER_RESPONSE_OVERHEAD_BYTES {
            return Err(format!(
                "{class} id budget drifted: {budget} != {RESPONSE_BOUND} - {id_bytes} - reserve"
            ));
        }
        // An envelope that fits the id-aware budget must fit the wire cap
        // including the modern resultType field and the frame fields.
        let complete = envelope_bytes + id_bytes + 34 + RESULT_TYPE_FIELD; // 34 = 33 payload + newline
        if complete > RESPONSE_BOUND {
            return Err(format!(
                "{class} id: envelope at budget cannot fit the wire cap ({complete} > {RESPONSE_BOUND})"
            ));
        }
        let message = ServerJsonRpcMessage::response(
            rmcp::model::ServerResult::CallToolResult(tool_result.clone()),
            id,
        );
        let frame = encode_message(&message).map_err(|error| error.to_string())?;
        if frame.len() - 1 != envelope_bytes + id_bytes + 33 + RESULT_TYPE_FIELD {
            return Err(format!(
                "{class} id: real encoder frame drifted from the fixed wrapper arithmetic"
            ));
        }
    }
    Ok(())
}

#[test]
fn exactly_max_response_bytes_passes_and_one_more_falls_back() -> Result<(), String> {
    // The writer backstop admits exactly-MAX frames, matching the semantic
    // layers' `> MAX` typed refusal (#5254 item 5). The overflow case pads
    // the message text, not the id: a giant id falls back to a hard error
    // by design, which is a different contract.
    let message_with_pad = |pad_len: usize| {
        ServerJsonRpcMessage::error(
            ErrorData::internal_error("x".repeat(pad_len), None),
            Some(rmcp::model::RequestId::String("pad".into())),
        )
    };
    let base_len = serde_json::to_vec(&message_with_pad(0))
        .map_err(|error| error.to_string())?
        .len();
    let pad_len = super::super::MAX_RESPONSE_BYTES
        .checked_sub(base_len)
        .ok_or_else(|| "fixture exceeds the response bound".to_string())?;
    let exact = encode_message(&message_with_pad(pad_len)).map_err(|error| error.to_string())?;
    // The newline is appended after the cap check, so the frame is MAX + 1.
    if exact.len() != super::super::MAX_RESPONSE_BYTES + 1 {
        return Err(format!(
            "exactly-MAX frame has wrong length: {}",
            exact.len()
        ));
    }
    let exact_text = String::from_utf8(exact).map_err(|error| error.to_string())?;
    if !exact_text.contains("\"code\":-32603")
        || exact_text.contains("MCP response exceeds the configured byte limit")
    {
        return Err("exactly-MAX frame was replaced by the fallback".into());
    }
    let over = encode_message(&message_with_pad(pad_len + 1)).map_err(|error| error.to_string())?;
    let over_text = String::from_utf8(over).map_err(|error| error.to_string())?;
    if !over_text.contains("MCP response exceeds the configured byte limit") {
        return Err(format!("MAX+1 frame missed the fallback: {over_text}"));
    }
    Ok(())
}

#[tokio::test]
async fn cancelled_partial_write_resumes_without_repeating_bytes() -> Result<(), String> {
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    let message = ServerJsonRpcMessage::error(
        ErrorData::internal_error("partial write witness", None),
        Some(rmcp::model::RequestId::Number(9)),
    );
    let expected = encode_message(&message).map_err(|error| error.to_string())?;
    let (sink, mut source) = tokio::io::duplex(2);
    let mut writer = FrameWriter::new(sink);
    {
        let mut sending = Box::pin(writer.send(&message));
        poll_fn(|context| match sending.as_mut().poll(context) {
            Poll::Pending => Poll::Ready(Ok(())),
            Poll::Ready(_) => Poll::Ready(Err("tiny sink must pause a partial frame".to_string())),
        })
        .await?;
    }
    if writer.cursor == 0 {
        return Err("control never wrote its prefix before cancellation".into());
    }
    let collector = tokio::spawn(async move {
        let mut bytes = Vec::new();
        source
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(bytes)
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), writer.close())
        .await
        .map_err(|_error| "resumed writer timed out".to_string())?
        .map_err(|error| error.to_string())?;
    let actual = collector
        .await
        .map_err(|_error| "output collector failed".to_string())??;
    if actual != expected {
        return Err("cancelled send duplicated or truncated frame bytes".into());
    }
    Ok(())
}
