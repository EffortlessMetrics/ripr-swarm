use super::*;
use rmcp::model::ErrorData;
use serde_json::{Value, json};
use std::sync::Mutex as StdMutex;
use tokio::io::AsyncWriteExt;

/// In-memory sink that keeps the wire bytes readable after `serve` returns,
/// so a test can assert the exact frames the session produced.
#[derive(Clone, Default)]
struct SharedOutput(Arc<StdMutex<Vec<u8>>>);

impl tokio::io::AsyncWrite for SharedOutput {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.0.lock() {
            Ok(mut output) => {
                output.extend_from_slice(buf);
                std::task::Poll::Ready(Ok(buf.len()))
            }
            Err(_poisoned) => {
                std::task::Poll::Ready(Err(std::io::Error::other("shared output unavailable")))
            }
        }
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

fn wire_frames(output: &SharedOutput) -> Result<Vec<Value>, String> {
    let bytes = output
        .0
        .lock()
        .map_err(|_poisoned| "output poisoned".to_string())?;
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .map(|frame| serde_json::from_slice(frame).map_err(|error| error.to_string()))
        .collect()
}

async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut FrameReader<R>,
) -> Result<FrameRead, String> {
    reader.read_frame().await.map_err(|error| error.to_string())
}

#[tokio::test]
async fn empty_or_syntax_invalid_eof_before_initialize_is_normal() -> Result<(), String> {
    for input in [b"".as_slice(), b"{unfinished".as_slice()] {
        serve(
            input,
            Vec::<u8>::new(),
            WorkspaceStatus::resolve_with_root(None).0,
            None,
        )
        .await?;
    }
    Ok(())
}

#[tokio::test]
async fn repeated_preinitialize_violations_recover_with_invalid_request_and_stay_alive()
-> Result<(), String> {
    // #5267: a request sent before `initialize` that lacks the SDK's required
    // `_meta` fields must be answered with its typed Invalid Request error and
    // must never terminate the session — every violation, not only the first.
    // After the violations a normal initialize still completes on the same
    // connection, and stdin EOF then ends the session cleanly.
    let tools_list = |id: u64| format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/list"}}"#);
    let initialize = r#"{"jsonrpc":"2.0","id":9,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"control","version":"1"}}}"#;
    let input = format!("{}\n{}\n{initialize}\n", tools_list(1), tools_list(2));
    let output = SharedOutput::default();
    serve(
        std::io::Cursor::new(input.into_bytes()),
        output.clone(),
        WorkspaceStatus::resolve_with_root(None).0,
        None,
    )
    .await?;
    let frames = wire_frames(&output)?;
    if frames.len() != 3 {
        return Err(format!(
            "two pre-init violations plus one initialize must produce exactly three frames: {frames:?}"
        ));
    }
    for (index, id) in [(0usize, 1u64), (1, 2)] {
        let frame = &frames[index];
        if frame.get("id") != Some(&json!(id)) {
            return Err(format!("violation reply lost its correlated id: {frame}"));
        }
        if frame.pointer("/error/code").and_then(Value::as_i64) != Some(-32602) {
            return Err(format!(
                "pre-init violation must answer Invalid Request: {frame}"
            ));
        }
    }
    let initialized = &frames[2];
    if initialized.get("id") != Some(&json!(9)) || initialized.get("error").is_some() {
        return Err(format!(
            "initialize must still complete after repeated pre-init violations: {initialized}"
        ));
    }
    if initialized
        .pointer("/result/serverInfo/name")
        .and_then(Value::as_str)
        != Some("ripr")
    {
        return Err(format!("initialize result drifted: {initialized}"));
    }
    Ok(())
}

#[tokio::test]
async fn preinitialize_notification_recovers_without_a_reply_frame() -> Result<(), String> {
    // The handshake refuses any pre-initialize message that is not an
    // `initialize` request, notifications included. A notification owes no
    // reply frame, so recovery must consume it silently and answer the next
    // real `initialize` on the same connection (#5267 review).
    let notification = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let initialize = r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"control","version":"1"}}}"#;
    let input = format!("{notification}\n{initialize}\n");
    let output = SharedOutput::default();
    serve(
        std::io::Cursor::new(input.into_bytes()),
        output.clone(),
        WorkspaceStatus::resolve_with_root(None).0,
        None,
    )
    .await?;
    let frames = wire_frames(&output)?;
    if frames.len() != 1 {
        return Err(format!(
            "a pre-init notification must produce no reply frame, leaving only the initialize result: {frames:?}"
        ));
    }
    let frame = &frames[0];
    if frame.get("id") != Some(&json!(7))
        || frame
            .pointer("/result/serverInfo/name")
            .and_then(Value::as_str)
            != Some("ripr")
    {
        return Err(format!(
            "the session must continue into a normal initialize after a refused pre-init notification: {frame}"
        ));
    }
    Ok(())
}

#[tokio::test]
async fn fatal_output_limit_wakes_receive_while_input_remains_open() -> Result<(), String> {
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    let (held_input, reader) = tokio::io::duplex(64);
    let mut transport = BoundedTransport {
        reader: Arc::new(Mutex::new(FrameReader::new(reader))),
        writer: Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new()))),
        failure: Arc::new(TransportFailure::default()),
        admission: Arc::new(Admission::default()),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    let sending = transport.send(ServerJsonRpcMessage::error(
        ErrorData::internal_error("bounded refusal", None),
        Some(rmcp::model::RequestId::String(
            "\n".repeat(70 * 1024).into(),
        )),
    ));
    let mut receiving = Box::pin(transport.receive());
    poll_fn(|context| match receiving.as_mut().poll(context) {
        Poll::Pending => Poll::Ready(Ok(())),
        Poll::Ready(_) => Poll::Ready(Err("open input did not suspend receive".to_string())),
    })
    .await?;
    let failure = sending
        .await
        .err()
        .ok_or_else(|| "giant correlated response unexpectedly fit".to_string())?;
    if super::super::writer::failure_reason(&failure) != "MCP output limit" {
        return Err("actual output refusal lost its fixed failure classification".into());
    }
    if tokio::time::timeout(std::time::Duration::from_secs(5), receiving)
        .await
        .map_err(|_error| "fatal send did not wake open-input receive".to_string())?
        .is_some()
    {
        return Err("fatal send did not terminate receive".into());
    }
    drop(held_input);
    Ok(())
}

#[tokio::test]
async fn oversized_frame_error_carries_explicit_null_id() -> Result<(), String> {
    // Transport-level errors answer no request, so JSON-RPC requires
    // `"id": null` — not a missing member (#5254 item 3).
    let mut input = vec![b'x'; super::super::MAX_MESSAGE_BYTES + 16];
    input.push(b'\n');
    let writer = Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new())));
    let mut transport = BoundedTransport {
        reader: Arc::new(Mutex::new(FrameReader::new(input.as_slice()))),
        writer: writer.clone(),
        failure: Arc::new(TransportFailure::default()),
        admission: Arc::new(Admission::default()),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    if transport.receive().await.is_some() {
        return Err("oversized input must not yield a message".to_string());
    }
    let guard = writer.lock().await;
    let frames: Vec<_> = guard
        .output()
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .collect();
    if frames.len() != 1 {
        return Err(format!(
            "expected exactly one protocol-error frame, got {}",
            frames.len()
        ));
    }
    let error: Value = serde_json::from_slice(frames[0]).map_err(|error| error.to_string())?;
    if error.get("id") != Some(&Value::Null) {
        return Err(format!("protocol error lost its null id: {error}"));
    }
    if error.pointer("/error/code").and_then(Value::as_i64) != Some(-32600) {
        return Err(format!("protocol error lost its code: {error}"));
    }
    Ok(())
}

#[tokio::test]
async fn oversized_request_id_is_rejected_with_null_id_and_the_session_continues()
-> Result<(), String> {
    let huge = "i".repeat(super::super::MAX_RESPONSE_BYTES);
    let boundary = "b".repeat(super::super::MAX_REQUEST_ID_BYTES - 2);
    let request =
        |id: &str| format!("{{\"jsonrpc\":\"2.0\",\"id\":\"{id}\",\"method\":\"ping\"}}\n");
    let input = format!("{}{}", request(&huge), request(&boundary));
    let writer = Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new())));
    let mut transport = BoundedTransport {
        reader: Arc::new(Mutex::new(FrameReader::new(input.as_bytes()))),
        writer: writer.clone(),
        failure: Arc::new(TransportFailure::default()),
        admission: Arc::new(Admission::default()),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    let message = transport
        .receive()
        .await
        .ok_or_else(|| "boundary-sized id was not delivered after the oversized one".to_string())?;
    let ClientJsonRpcMessage::Request(delivered) = message else {
        return Err("expected the boundary request".into());
    };
    if delivered.id != rmcp::model::RequestId::String(boundary.clone().into()) {
        return Err("delivered the wrong request".into());
    }
    let guard = writer.lock().await;
    let frames: Vec<_> = guard
        .output()
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .collect();
    if frames.len() != 1 || frames[0].len() > 512 {
        return Err(format!(
            "expected one bounded error frame, got {}",
            frames.len()
        ));
    }
    let error: Value = serde_json::from_slice(frames[0]).map_err(|error| error.to_string())?;
    if error.get("id") != Some(&Value::Null)
        || error.pointer("/error/code").and_then(Value::as_i64) != Some(-32600)
    {
        return Err(format!("oversized id error has wrong id/code: {error}"));
    }
    if transport
        .failure
        .reason
        .lock()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("oversized id must not fail the transport".into());
    }
    Ok(())
}

#[tokio::test]
async fn oversized_id_on_a_shape_invalid_request_is_not_echoed() -> Result<(), String> {
    let over = "o".repeat(super::super::MAX_REQUEST_ID_BYTES + 1);
    let kept = "k".repeat(64);
    let request = |id: &str| {
        format!("{{\"jsonrpc\":\"2.0\",\"id\":\"{id}\",\"method\":\"tools/list\",\"params\":[]}}\n")
    };
    let input = format!("{}{}", request(&over), request(&kept));
    let writer = Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new())));
    let mut transport = BoundedTransport {
        reader: Arc::new(Mutex::new(FrameReader::new(input.as_bytes()))),
        writer: writer.clone(),
        failure: Arc::new(TransportFailure::default()),
        admission: Arc::new(Admission::default()),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    if transport.receive().await.is_some() {
        return Err("shape-invalid requests must not be delivered".into());
    }
    let guard = writer.lock().await;
    let frames: Vec<Value> = guard
        .output()
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .map(|frame| serde_json::from_slice(frame).map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    if frames.len() != 2
        || frames[0].get("id") != Some(&Value::Null)
        || frames[0].to_string().len() > 512
        || frames[1].get("id") != Some(&Value::String(kept))
    {
        return Err(format!(
            "oversized id was echoed or in-bound id lost: {frames:?}"
        ));
    }
    Ok(())
}

#[tokio::test]
async fn syntax_error_and_typed_shape_error_are_each_answered_then_next_request_recovers()
-> Result<(), String> {
    let input = b"{not json}\ntrue\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"control\",\"version\":\"1\"}}}\n";
    let writer = Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new())));
    let mut transport = BoundedTransport {
        reader: Arc::new(Mutex::new(FrameReader::new(input.as_slice()))),
        writer: writer.clone(),
        failure: Arc::new(TransportFailure::default()),
        admission: Arc::new(Admission::default()),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    let message = transport
        .receive()
        .await
        .ok_or_else(|| "recovery lost the valid request".to_string())?;
    if !matches!(message, ClientJsonRpcMessage::Request(_)) {
        return Err("recovery did not return the request".into());
    }
    let guard = writer.lock().await;
    let frames: Vec<Value> = guard
        .output()
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .map(|frame| serde_json::from_slice(frame).map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    // Transport-level errors carry an explicit null id per JSON-RPC (#5254
    // item 3), not a missing member.
    let [parse, shape] = frames.as_slice() else {
        return Err(format!(
            "expected a parse error then a shape error: {frames:?}"
        ));
    };
    for (frame, code) in [(parse, -32700), (shape, -32600)] {
        if frame.get("id") != Some(&Value::Null)
            || frame.pointer("/error/code").and_then(Value::as_i64) != Some(code)
        {
            return Err(format!("wrong id/code, expected {code}: {frame}"));
        }
    }
    Ok(())
}

/// One inbound frame in, the error frames the transport wrote out.
async fn error_frames_for(input: &[u8]) -> Result<Vec<Value>, String> {
    let writer = Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new())));
    let mut transport = BoundedTransport {
        reader: Arc::new(Mutex::new(FrameReader::new(input))),
        writer: writer.clone(),
        failure: Arc::new(TransportFailure::default()),
        admission: Arc::new(Admission::default()),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    if transport.receive().await.is_some() {
        return Err(format!("{input:?} must not yield a message"));
    }
    let guard = writer.lock().await;
    guard
        .output()
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .map(|frame| serde_json::from_slice(frame).map_err(|error| error.to_string()))
        .collect()
}

#[tokio::test]
async fn unanswerable_frames_get_a_parse_or_invalid_request_reply_not_silence() -> Result<(), String>
{
    // A client waiting on any of these would hang forever if they were
    // dropped (F5): non-JSON and truncated JSON are Parse errors; a deeply
    // nested request exceeds the parser's depth limit and is also not
    // readable JSON; null and fractional ids are valid JSON that is not a
    // typed request.
    let deep = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/list\",\"params\":{}{}}}\n",
        "[".repeat(5000),
        "]".repeat(5000)
    );
    let cases: [(&[u8], i64); 6] = [
        (b"{not json\n", -32700),
        (b"{\"jsonrpc\":\"2.0\",\"id\":1,\"met\n", -32700),
        (deep.as_bytes(), -32700),
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"tools/list\"}\n",
            -32600,
        ),
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":1.5,\"method\":\"tools/list\"}\n",
            -32600,
        ),
        (
            b"{\"jsonrpc\":\"1.0\",\"id\":\"known\",\"method\":\"tools/list\"}\n",
            -32600,
        ),
    ];
    for (input, code) in cases {
        let frames = error_frames_for(input).await?;
        let [frame] = frames.as_slice() else {
            return Err(format!(
                "expected exactly one reply for {input:.60?}: {frames:?}"
            ));
        };
        let expected_id = if input.starts_with(b"{\"jsonrpc\":\"1.0\"") {
            json!("known")
        } else {
            Value::Null
        };
        if frame.get("id") != Some(&expected_id)
            || frame.pointer("/error/code").and_then(Value::as_i64) != Some(code)
            || frame.get("jsonrpc") != Some(&json!("2.0"))
        {
            return Err(format!("wrong reply for {input:.60?}: {frame}"));
        }
    }
    Ok(())
}

#[tokio::test]
async fn oversized_frame_error_names_a_front_loaded_request_id() -> Result<(), String> {
    let filler = "a".repeat(super::super::MAX_MESSAGE_BYTES);
    let cases: [(String, Value); 6] = [
        (
            format!("{{\"jsonrpc\":\"2.0\",\"id\":42,\"method\":\"m\",\"params\":\"{filler}\"}}\n"),
            json!(42),
        ),
        (
            format!("{{\"id\":\"req-9\",\"params\":\"{filler}\"}}\n"),
            json!("req-9"),
        ),
        // An id after the retained prefix is unreadable: keep null.
        (
            format!("{{\"params\":\"{filler}\",\"id\":42}}\n"),
            Value::Null,
        ),
        // A nested `id` is not the request id.
        (
            format!("{{\"params\":{{\"id\":7}},\"x\":\"{filler}\"}}\n"),
            Value::Null,
        ),
        // An `id` spelled inside a string is not a member.
        (
            format!("{{\"text\":\"\\\"id\\\":7,\",\"x\":\"{filler}\"}}\n"),
            Value::Null,
        ),
        // Non-object frames and unusable id kinds stay null.
        (format!("[\"{filler}\"]\n"), Value::Null),
    ];
    for (input, expected_id) in cases {
        let frames = error_frames_for(input.as_bytes()).await?;
        let [frame] = frames.as_slice() else {
            return Err(format!("expected one reply, got {frames:?}"));
        };
        if frame.get("id") != Some(&expected_id)
            || frame.pointer("/error/code").and_then(Value::as_i64) != Some(-32600)
        {
            return Err(format!(
                "wrong oversized reply, expected id {expected_id}: {frame}"
            ));
        }
    }
    Ok(())
}

#[test]
fn oversized_request_id_rejects_a_number_cut_by_the_prefix_end() {
    assert_eq!(oversized_request_id(br#"{"id":12"#), None);
    assert_eq!(oversized_request_id(br#"{"id":12,"m":"#), Some(json!(12)));
    assert_eq!(oversized_request_id(br#"{"id":true,"m":1}"#), None);
}

#[tokio::test]
async fn typed_shape_error_keeps_a_recoverable_request_id() -> Result<(), String> {
    // JSON-RPC: the error id is the request id when it can be detected. A
    // request whose `params` is the wrong shape still names its id; frames
    // with no readable request id keep the explicit null.
    let cases: [(&[u8], Value); 9] = [
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/list\",\"params\":[]}\n",
            json!(5),
        ),
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":\"req-a\",\"method\":\"tools/list\",\"params\":[]}\n",
            json!("req-a"),
        ),
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":1.5,\"method\":\"tools/list\",\"params\":[]}\n",
            Value::Null,
        ),
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":{\"a\":1},\"method\":\"tools/list\",\"params\":[]}\n",
            Value::Null,
        ),
        (b"{\"jsonrpc\":\"2.0\",\"id\":7}\n", Value::Null),
        // A notification (method, no id) has no request to correlate.
        (
            b"{\"jsonrpc\":\"2.0\",\"method\":\"tools/list\",\"params\":[]}\n",
            Value::Null,
        ),
        // An explicit null id names no request.
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"tools/list\",\"params\":[]}\n",
            Value::Null,
        ),
        // A boolean is not a JSON-RPC id.
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":true,\"method\":\"tools/list\",\"params\":[]}\n",
            Value::Null,
        ),
        // A u64 above i64::MAX is a valid JSON integer id the client sent but
        // outside the SDK's i64 `RequestId`; the transport echoes the client's
        // own id verbatim rather than leaving the reply unmatchable.
        (
            b"{\"jsonrpc\":\"2.0\",\"id\":18446744073709551615,\"method\":\"tools/list\",\"params\":[]}\n",
            json!(18446744073709551615_u64),
        ),
    ];
    for (input, expected_id) in cases {
        let writer = Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new())));
        let mut transport = BoundedTransport {
            reader: Arc::new(Mutex::new(FrameReader::new(input))),
            writer: writer.clone(),
            failure: Arc::new(TransportFailure::default()),
            admission: Arc::new(Admission::default()),
            pending_protocol_error: None,
            writer_needs_drain: false,
        };
        if transport.receive().await.is_some() {
            return Err("an invalid typed shape must not yield a message".into());
        }
        let guard = writer.lock().await;
        let frames: Vec<_> = guard
            .output()
            .split(|byte| *byte == b'\n')
            .filter(|frame| !frame.is_empty())
            .collect();
        let [frame] = frames.as_slice() else {
            return Err(format!("expected exactly one error frame for {input:?}"));
        };
        let error: Value = serde_json::from_slice(frame).map_err(|error| error.to_string())?;
        if error.get("id") != Some(&expected_id)
            || error.pointer("/error/code").and_then(Value::as_i64) != Some(-32600)
        {
            return Err(format!(
                "wrong id/code for {input:?}: expected id {expected_id}, got {error}"
            ));
        }
    }
    Ok(())
}

#[tokio::test]
async fn typed_shape_error_with_an_oversized_id_uses_null_and_the_session_continues()
-> Result<(), String> {
    let huge = "i".repeat(super::super::MAX_RESPONSE_BYTES);
    let input = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":\"{huge}\",\"method\":\"tools/list\",\"params\":[]}}\n\
         {{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}}\n"
    );
    let writer = Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new())));
    let mut transport = BoundedTransport {
        reader: Arc::new(Mutex::new(FrameReader::new(input.as_bytes()))),
        writer: writer.clone(),
        failure: Arc::new(TransportFailure::default()),
        admission: Arc::new(Admission::default()),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    let message = transport.receive().await.ok_or_else(|| {
        "the request after the oversized-id shape error was not delivered".to_string()
    })?;
    let ClientJsonRpcMessage::Request(delivered) = message else {
        return Err("expected the following ping request".into());
    };
    if delivered.id != rmcp::model::RequestId::Number(7) {
        return Err("delivered the wrong request".into());
    }
    let guard = writer.lock().await;
    let frames: Vec<_> = guard
        .output()
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .collect();
    let [frame] = frames.as_slice() else {
        return Err(format!("expected one error frame, got {}", frames.len()));
    };
    let error: Value = serde_json::from_slice(frame).map_err(|error| error.to_string())?;
    if frame.len() > 512
        || error.get("id") != Some(&Value::Null)
        || error.pointer("/error/code").and_then(Value::as_i64) != Some(-32600)
    {
        return Err(format!(
            "oversized-id shape error must be bounded with a null id: {error}"
        ));
    }
    if transport
        .failure
        .reason
        .lock()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("oversized id on the shape-error path must not fail the transport".into());
    }
    Ok(())
}

#[tokio::test]
async fn framing_delivers_valid_undelimited_eof_and_then_closes() -> Result<(), String> {
    let mut reader = FrameReader::new(b"{\"eof\":true}".as_slice());
    match read_frame(&mut reader).await? {
        FrameRead::Frame(bytes) if bytes == b"{\"eof\":true}" => {}
        _ => return Err("undelimited EOF lost valid frame".into()),
    }
    if !matches!(read_frame(&mut reader).await?, FrameRead::Eof) {
        return Err("EOF did not remain closed".into());
    }
    Ok(())
}

#[tokio::test]
async fn consumed_protocol_error_survives_receive_cancellation_behind_busy_writer()
-> Result<(), String> {
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    let mut input = vec![b'x'; super::super::MAX_MESSAGE_BYTES + 1];
    input.extend_from_slice(b"\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"control\",\"version\":\"1\"}}}\n");
    let writer = Arc::new(Mutex::new(FrameWriter::new(Vec::<u8>::new())));
    let held = writer.lock().await;
    let mut transport = BoundedTransport {
        reader: Arc::new(Mutex::new(FrameReader::new(input.as_slice()))),
        writer: writer.clone(),
        failure: Arc::new(TransportFailure::default()),
        admission: Arc::new(Admission::default()),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    {
        let mut receiving = Box::pin(transport.receive());
        poll_fn(|context| match receiving.as_mut().poll(context) {
            Poll::Pending => Poll::Ready(Ok(())),
            Poll::Ready(_) => Poll::Ready(Err(
                "busy writer must pause the consumed-frame error".to_string()
            )),
        })
        .await?;
    }
    if transport.pending_protocol_error.is_none() {
        return Err("consumed invalid frame lost its pending error".into());
    }
    drop(held);
    let message = tokio::time::timeout(std::time::Duration::from_secs(5), transport.receive())
        .await
        .map_err(|_error| "protocol error resume timed out".to_string())?
        .ok_or_else(|| "transport failed to recover next valid message".to_string())?;
    if !matches!(message, ClientJsonRpcMessage::Request(_)) {
        return Err("next valid request was not recovered".into());
    }
    let guard = writer.lock().await;
    let lines: Vec<_> = guard
        .output()
        .split(|byte| *byte == b'\n')
        .filter(|frame| !frame.is_empty())
        .collect();
    if lines.len() != 1 {
        return Err("consumed-frame error was missing or duplicated".into());
    }
    let frame = lines
        .first()
        .ok_or_else(|| "error frame missing".to_string())?;
    let error: Value = serde_json::from_slice(frame).map_err(|error| error.to_string())?;
    if error.pointer("/error/code").and_then(Value::as_i64) != Some(-32600) {
        return Err("recovered protocol error code differs".into());
    }
    Ok(())
}

#[tokio::test]
async fn framing_accepts_coalesced_lines_and_crlf() -> Result<(), String> {
    let input = b"{\"one\":1}\r\n{\"two\":2}\n";
    let mut reader = FrameReader::new(&input[..]);
    let first = read_frame(&mut reader).await?;
    let second = read_frame(&mut reader).await?;
    let eof = read_frame(&mut reader).await?;

    match first {
        FrameRead::Frame(value) if value.as_slice() == b"{\"one\":1}" => {}
        _ => return Err("first coalesced frame drifted".to_string()),
    }
    match second {
        FrameRead::Frame(value) if value.as_slice() == b"{\"two\":2}" => {}
        _ => return Err("second coalesced frame drifted".to_string()),
    }
    if !matches!(eof, FrameRead::Eof) {
        return Err("framing must end cleanly at EOF".to_string());
    }
    Ok(())
}

#[tokio::test]
async fn framing_reassembles_a_message_split_across_reads() -> Result<(), String> {
    let (mut writer, reader) = tokio::io::duplex(4);
    let writer_task = tokio::spawn(async move {
        writer
            .write_all(b"{\"split\":")
            .await
            .map_err(|error| error.to_string())?;
        writer
            .write_all(b"true}\n")
            .await
            .map_err(|error| error.to_string())
    });
    let mut reader = FrameReader::new(reader);
    let frame = read_frame(&mut reader).await?;
    writer_task
        .await
        .map_err(|error| format!("fragment writer task failed: {error}"))??;
    match frame {
        FrameRead::Frame(value) if value.as_slice() == b"{\"split\":true}" => Ok(()),
        _ => Err("fragmented frame was not reassembled".to_string()),
    }
}

#[tokio::test]
async fn framing_retains_a_partial_message_when_receive_is_cancelled() -> Result<(), String> {
    use std::future::{Future, poll_fn};
    use std::task::Poll;

    // SDK receive runs inside select!, so cancellation can happen after
    // consuming a prefix but before the delimiter arrives. Poll once to
    // that exact Pending boundary; no wall-clock timing or spawned actor
    // decides whether the cancellation actually reached the partial read.
    let (mut writer, reader) = tokio::io::duplex(64);
    writer
        .write_all(b"{\"split\":")
        .await
        .map_err(|error| error.to_string())?;
    let mut reader = FrameReader::new(reader);
    {
        let mut receiving = Box::pin(read_frame(&mut reader));
        poll_fn(|context| match receiving.as_mut().poll(context) {
            Poll::Pending => Poll::Ready(Ok(())),
            Poll::Ready(_) => Poll::Ready(Err(
                "partial-read control unexpectedly completed before its delimiter".to_string(),
            )),
        })
        .await?;
    }
    writer
        .write_all(b"true}\n")
        .await
        .map_err(|error| error.to_string())?;
    let completed =
        tokio::time::timeout(std::time::Duration::from_secs(5), read_frame(&mut reader))
            .await
            .map_err(|_error| {
                "cancelled receive did not complete after its delimiter".to_string()
            })??;
    match completed {
        FrameRead::Frame(value) if value.as_slice() == b"{\"split\":true}" => Ok(()),
        _ => Err("cancelled receive discarded the consumed prefix".to_string()),
    }
}

#[tokio::test]
async fn oversized_frame_is_discarded_without_allocating_past_the_cap() -> Result<(), String> {
    let mut input = vec![b'x'; super::super::MAX_MESSAGE_BYTES + 1];
    input.push(b'\n');
    input.extend_from_slice(b"{}\n");
    let mut reader = FrameReader::new(input.as_slice());
    if !matches!(read_frame(&mut reader).await?, FrameRead::Oversized(_)) {
        return Err("oversized frame must fail closed".to_string());
    }
    match read_frame(&mut reader).await? {
        FrameRead::Frame(value) if value.as_slice() == b"{}" => Ok(()),
        _ => Err("reader did not recover after oversized frame".to_string()),
    }
}

#[tokio::test]
async fn oversized_response_fallback_keeps_the_known_request_id() -> Result<(), String> {
    let response = ServerJsonRpcMessage::error(
        ErrorData::internal_error(
            "large response",
            Some(json!({ "blob": "x".repeat(super::super::MAX_RESPONSE_BYTES + 1) })),
        ),
        Some(rmcp::model::RequestId::Number(7)),
    );
    let mut output =
        super::super::writer::encode_message(&response).map_err(|error| error.to_string())?;
    if output.last() == Some(&b'\n') {
        let _trailing_newline = output.pop();
    }
    let parsed: Value = serde_json::from_slice(&output)
        .map_err(|error| format!("fallback response is not JSON: {error}"))?;
    if parsed.get("id") != Some(&json!(7)) {
        return Err(format!(
            "over-cap fallback dropped the known request id: {parsed}"
        ));
    }
    if parsed.pointer("/error/code").and_then(Value::as_i64) != Some(-32603) {
        return Err("over-cap fallback error code drifted".to_string());
    }
    if parsed
        .pointer("/error/data/maxResponseBytes")
        .and_then(Value::as_u64)
        != Some(super::super::MAX_RESPONSE_BYTES as u64)
    {
        return Err("over-cap fallback omitted the byte limit".to_string());
    }
    Ok(())
}

#[tokio::test]
async fn oversized_response_fallback_preserves_sdk_unknown_id_omission() -> Result<(), String> {
    let response = ServerJsonRpcMessage::error(
        ErrorData::internal_error("x".repeat(super::super::MAX_RESPONSE_BYTES + 1), None),
        None,
    );
    let mut output =
        super::super::writer::encode_message(&response).map_err(|error| error.to_string())?;
    if output.last() == Some(&b'\n') {
        let _trailing_newline = output.pop();
    }
    let parsed: Value = serde_json::from_slice(&output)
        .map_err(|error| format!("fallback response is not JSON: {error}"))?;
    if parsed.get("id").is_some() {
        return Err(format!(
            "over-cap fallback must preserve SDK unknown-id omission: {parsed}"
        ));
    }
    Ok(())
}
