use super::{
    framing::{FrameRead, FrameReader},
    server::InitializeSessionService,
    writer::FrameWriter,
};
use crate::workspace_status::WorkspaceStatus;
use rmcp::{
    RoleServer, ServiceExt,
    model::{ClientJsonRpcMessage, RequestId, ServerJsonRpcMessage},
    transport::{Transport, async_rw::JsonRpcMessageCodec},
};
use serde_json::json;
use std::{
    io::Error,
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex, Notify},
};
use tokio_util::{bytes::BytesMut, codec::Decoder};

#[derive(Default)]
struct TransportFailure {
    reason: StdMutex<Option<&'static str>>,
    wake: Notify,
}
type Failure = Arc<TransportFailure>;
/// One typed request remains admitted until its actual reply frame is flushed.
#[derive(Default)]
struct Admission {
    pending: StdMutex<Option<RequestId>>,
    wake: Notify,
}
impl Admission {
    fn is_pending(&self) -> Result<bool, Error> {
        self.pending
            .lock()
            .map(|id| id.is_some())
            .map_err(|_error| Error::other("MCP admission unavailable"))
    }
    fn admit(&self, id: RequestId) -> Result<(), Error> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_error| Error::other("MCP admission unavailable"))?;
        if pending.is_some() {
            return Err(Error::other("MCP admission already pending"));
        }
        *pending = Some(id);
        Ok(())
    }
    fn complete(&self, id: Option<RequestId>) -> Result<(), Error> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_error| Error::other("MCP admission unavailable"))?;
        if id.is_some() && *pending == id {
            *pending = None;
            self.wake.notify_one();
        }
        Ok(())
    }
}
fn record_failure(failure: &Failure, reason: &'static str) {
    if let Ok(mut stored) = failure.reason.lock()
        && stored.is_none()
    {
        *stored = Some(reason);
    }
    // A response send task can fail while the SDK is awaiting receive.
    // Notify retains a permit even if that future is between polls.
    failure.wake.notify_one();
}
struct BoundedTransport<R, W> {
    /// Shared across handshake attempts so bytes already buffered from a
    /// pipelined frame survive a retried handshake (#5267).
    reader: Arc<Mutex<FrameReader<R>>>,
    writer: Arc<Mutex<FrameWriter<W>>>,
    failure: Failure,
    admission: Arc<Admission>,
    pending_protocol_error: Option<Vec<u8>>,
    writer_needs_drain: bool,
}

/// A transport-level protocol error with an explicit null id, pre-encoded
/// through the writer cap. rmcp's `JsonRpcError` omits a missing id, but
/// JSON-RPC requires `"id": null` when the request id is unknown (#5254
/// item 3), so these frames bypass the SDK type.
fn protocol_error_frame(code: i32, message: &str) -> std::io::Result<Vec<u8>> {
    protocol_error_frame_with_id(json!(null), code, message)
}
fn protocol_error_frame_with_id(
    id: serde_json::Value,
    code: i32,
    message: &str,
) -> std::io::Result<Vec<u8>> {
    super::writer::encode(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message},
    }))
}
fn carries_id_member(frame: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(frame)
        .ok()
        .and_then(|value| value.as_object().map(|object| object.contains_key("id")))
        .unwrap_or(false)
}
/// The request id of a typed-shape-invalid frame, when it is still readable.
///
/// JSON-RPC 2.0 says the error id is the request's id when it can be
/// detected, and null only otherwise. A frame that is a JSON object with a
/// `method` and a string or integer `id` names its request even when `params`
/// or `_meta` is the wrong shape, so the client can correlate the rejection.
/// The id is echoed verbatim: any JSON integer is accepted, a superset of the
/// SDK's i64 `RequestId` (a u64 above `i64::MAX` is the client's own id).
/// Notifications, null, boolean, float and structured ids stay null.
fn recoverable_request_id(frame: &[u8]) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_slice(frame).ok()?;
    let object = value.as_object()?;
    object.get("method")?.as_str()?;
    let id = object.get("id")?;
    (id.is_string() || id.is_i64() || id.is_u64()).then(|| id.clone())
}
/// The error frame for a message the SDK could not type: Parse error with a
/// null id when the bytes are not JSON (including JSON nested past the
/// parser's depth limit), Invalid Request carrying the request id when one is
/// still readable.
fn unreadable_frame_error(frame: &[u8]) -> std::io::Result<Vec<u8>> {
    if serde_json::from_slice::<serde_json::Value>(frame).is_err() {
        return protocol_error_frame(-32700, "Parse error");
    }
    protocol_error_frame_with_id(
        recoverable_request_id(frame).unwrap_or(serde_json::Value::Null),
        -32600,
        "Invalid request",
    )
}
/// The request id of an over-cap message, read from its retained prefix.
///
/// Only a top-level `"id"` member that is a string or integer and that
/// finishes inside the prefix counts; members are walked in order and skipped
/// structurally, so an `"id"` nested inside `params` or text inside a string
/// is never mistaken for the request id. A prefix cut before the id (or an
/// id past the prefix) yields `None`, and the error keeps the null id.
fn oversized_request_id(prefix: &[u8]) -> Option<serde_json::Value> {
    use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
    use std::{cell::RefCell, fmt};
    struct Members<'a>(&'a RefCell<Option<serde_json::Value>>);
    impl<'de> Visitor<'de> for Members<'_> {
        type Value = ();
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON object")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            while let Some(key) = map.next_key::<String>()? {
                if key == "id" {
                    let id: serde_json::Value = map.next_value()?;
                    // Accept the id only once the member after it parses: a
                    // number cut by the prefix end would otherwise read as a
                    // shorter, different id.
                    map.next_key::<IgnoredAny>()?;
                    if id.is_string() || id.is_i64() || id.is_u64() {
                        *self.0.borrow_mut() = Some(id);
                    }
                    return Ok(());
                }
                map.next_value::<IgnoredAny>()?;
            }
            Ok(())
        }
    }
    let found = RefCell::new(None);
    let mut deserializer = serde_json::Deserializer::from_slice(prefix);
    // A truncated prefix always errors after the id is captured or lost;
    // the captured value is the only output that matters.
    let _ = deserializer.deserialize_map(Members(&found));
    found.into_inner()
}
impl<R: AsyncRead + Unpin + Send, W: AsyncWrite + Unpin + Send + 'static> Transport<RoleServer>
    for BoundedTransport<R, W>
{
    type Error = Error;
    fn send(
        &mut self,
        item: ServerJsonRpcMessage,
    ) -> impl Future<Output = Result<(), Error>> + Send + 'static {
        let writer = self.writer.clone();
        let failure = self.failure.clone();
        let admission = self.admission.clone();
        async move {
            let result = async {
                let mut writer = writer.lock().await;
                admission.complete(writer.finish_pending().await?)?;
                writer.queue(&item)?;
                // Retained notification and frame identity survive a dropped send.
                admission.wake.notify_one();
                admission.complete(writer.finish_pending().await?)
            }
            .await;
            if let Err(error) = &result {
                record_failure(&failure, super::writer::failure_reason(error));
            }
            result
        }
    }
    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            if self
                .failure
                .reason
                .lock()
                .map_or(true, |reason| reason.is_some())
            {
                return None;
            }
            match self.admission.is_pending() {
                Err(_) => {
                    record_failure(&self.failure, "MCP admission unavailable");
                    return None;
                }
                Ok(true) => {
                    // Never hold the admission mutex while acquiring the writer.
                    let mut writer = tokio::select! {
                        _ = self.failure.wake.notified() => return None,
                        writer = self.writer.lock() => writer,
                    };
                    if writer.has_pending() {
                        let flushed = tokio::select! {
                            _ = self.failure.wake.notified() => return None,
                            flushed = writer.finish_pending() => flushed,
                        };
                        match flushed.and_then(|id| self.admission.complete(id)) {
                            Ok(()) => continue,
                            Err(error) => {
                                record_failure(
                                    &self.failure,
                                    super::writer::failure_reason(&error),
                                );
                                return None;
                            }
                        }
                    }
                    drop(writer);
                    if self.admission.is_pending().ok() != Some(true) {
                        continue;
                    }
                    tokio::select! {
                        _ = self.failure.wake.notified() => return None,
                        _ = self.admission.wake.notified() => continue,
                    }
                }
                Ok(false) => {}
            }
            if self.pending_protocol_error.is_some() || self.writer_needs_drain {
                let mut writer = self.writer.lock().await;
                if let Err(error) = writer
                    .finish_pending()
                    .await
                    .and_then(|id| self.admission.complete(id))
                {
                    record_failure(&self.failure, super::writer::failure_reason(&error));
                    return None;
                }
                self.writer_needs_drain = false;
                if let Some(frame) = self.pending_protocol_error.take() {
                    if let Err(error) = writer.queue_raw(frame) {
                        record_failure(&self.failure, super::writer::failure_reason(&error));
                        return None;
                    }
                    self.writer_needs_drain = true;
                    if let Err(error) = writer
                        .finish_pending()
                        .await
                        .and_then(|id| self.admission.complete(id))
                    {
                        record_failure(&self.failure, super::writer::failure_reason(&error));
                        return None;
                    }
                    self.writer_needs_drain = false;
                }
            }
            let read = tokio::select! {
                _ = self.failure.wake.notified() => return None,
                read = async {
                    let mut reader = self.reader.lock().await;
                    reader.read_frame().await
                } => read,
            };
            let frame = match read {
                Ok(FrameRead::Eof) => return None,
                Ok(FrameRead::Empty) => continue,
                Ok(FrameRead::Oversized(prefix)) => {
                    match protocol_error_frame_with_id(
                        oversized_request_id(&prefix).unwrap_or(serde_json::Value::Null),
                        -32600,
                        "MCP message exceeds the configured byte limit",
                    ) {
                        Ok(frame) => {
                            self.pending_protocol_error = Some(frame);
                        }
                        Err(error) => {
                            record_failure(&self.failure, super::writer::failure_reason(&error));
                            return None;
                        }
                    }
                    continue;
                }
                Ok(FrameRead::Frame(frame)) => frame,
                Err(_) => {
                    record_failure(&self.failure, "MCP input IO failed");
                    return None;
                }
            };
            // The SDK codec owns typed parsing and compatibility. Framing
            // already enforced the product input bound, including at EOF.
            let mut bytes = BytesMut::from(frame.as_slice());
            match JsonRpcMessageCodec::<ClientJsonRpcMessage>::default().decode_eof(&mut bytes) {
                // A frame with an `id` member is a request, never a
                // notification. The SDK's untagged decode falls back to the
                // notification shape (ignoring `id`) when the id is null or
                // not a string/integer, which dropped the request unanswered.
                Ok(Some(ClientJsonRpcMessage::Notification(_))) if carries_id_member(&frame) => {
                    match unreadable_frame_error(&frame) {
                        Ok(frame) => {
                            self.pending_protocol_error = Some(frame);
                        }
                        Err(error) => {
                            record_failure(&self.failure, super::writer::failure_reason(&error));
                            return None;
                        }
                    }
                }
                Ok(Some(message)) => {
                    if let ClientJsonRpcMessage::Request(request) = &message
                        && self.admission.admit(request.id.clone()).is_err()
                    {
                        record_failure(&self.failure, "MCP admission unavailable");
                        return None;
                    }
                    return Some(message);
                }
                Ok(None) => continue,
                // JSON-RPC 2.0: text that is not JSON is a Parse error
                // (-32700) and anything that is JSON but not a request the
                // SDK can type is an Invalid Request (-32600). Both must be
                // answered: a silent drop leaves the client waiting forever
                // on a request it cannot know was rejected. Classifying by an
                // independent JSON parse (not the codec's error category)
                // keeps SDK id and nesting-limit failures from being ignored.
                Err(_) => match unreadable_frame_error(&frame) {
                    Ok(frame) => {
                        self.pending_protocol_error = Some(frame);
                    }
                    Err(error) => {
                        record_failure(&self.failure, super::writer::failure_reason(&error));
                        return None;
                    }
                },
            }
        }
    }
    async fn close(&mut self) -> Result<(), Error> {
        let result = self.writer.lock().await.close().await;
        if let Err(error) = &result {
            record_failure(&self.failure, super::writer::failure_reason(error));
        }
        result
    }
}
pub(super) async fn serve_stdio(explicit_root: Option<PathBuf>) -> Result<(), String> {
    let (status, analysis_root) = WorkspaceStatus::resolve_with_root(explicit_root);
    serve(
        tokio::io::stdin(),
        tokio::io::stdout(),
        status,
        analysis_root,
    )
    .await
}
async fn serve<R, W>(
    reader: R,
    writer: W,
    status: WorkspaceStatus,
    analysis_root: Option<PathBuf>,
) -> Result<(), String>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let failure: Failure = Arc::new(TransportFailure::default());
    let writer = Arc::new(Mutex::new(FrameWriter::new(writer)));
    let reader = Arc::new(Mutex::new(FrameReader::new(reader)));
    let admission = Arc::new(Admission::default());
    // A pre-initialize message the SDK refuses (a request without its
    // required `_meta`, or a stray notification) ends one handshake attempt
    // with the typed error already on the wire. ADR 0022's contract is
    // recovery with Invalid Request, not process death (#5267), so the
    // handshake restarts on the same connection; each attempt consumes at
    // least one frame, so the loop always makes progress toward a real
    // initialize, a valid-meta request, or stdin EOF.
    loop {
        let server = InitializeSessionService::new(status.clone(), analysis_root.clone())
            .map_err(|_error| "MCP status projection failed".to_owned())?;
        let transport = BoundedTransport {
            reader: reader.clone(),
            writer: writer.clone(),
            failure: failure.clone(),
            admission: admission.clone(),
            pending_protocol_error: None,
            writer_needs_drain: false,
        };
        let service = match server.serve(transport).await {
            Ok(service) => service,
            Err(error) => {
                let reason = failure
                    .reason
                    .lock()
                    .map_err(|_error| "MCP transport status unavailable".to_string())?
                    .take();
                if let Some(reason) = reason {
                    return Err(reason.to_string());
                }
                match error {
                    // stdin EOF before initialize is a normal client exit.
                    rmcp::service::ServerInitializeError::ConnectionClosed(_) => return Ok(()),
                    // The violating message is answered; the session lives.
                    rmcp::service::ServerInitializeError::ExpectedInitializeRequest(_) => {
                        continue;
                    }
                    _ => return Err("MCP SDK startup failed".to_string()),
                }
            }
        };
        service
            .waiting()
            .await
            .map_err(|_error| "MCP SDK service failed".to_string())?;
        let error = failure
            .reason
            .lock()
            .map_err(|_error| "MCP transport status unavailable".to_string())?
            .take();
        return match error {
            Some(error) => Err(error.to_string()),
            None => Ok(()),
        };
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transport_backpressure_tests.rs"]
mod backpressure_tests;
