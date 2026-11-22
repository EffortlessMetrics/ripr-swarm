use super::{
    framing::{FrameRead, FrameReader},
    server::InitializeSessionService,
    writer::FrameWriter,
};
use crate::workspace_status::{WorkspaceState, WorkspaceStatus};
use rmcp::{
    RoleServer, ServiceExt,
    model::{
        ClientJsonRpcMessage, ClientRequest, CustomRequest, GetExtensions, RequestId,
        ServerJsonRpcMessage,
    },
    transport::{Transport, async_rw::JsonRpcMessageCodec},
};
use serde_json::json;
use std::{
    io::{Error, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
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
/// Requests stay admitted until their actual reply frame is flushed.
///
/// Admission is serial with one exception (#15983). A `ripr_refresh` call
/// admitted while nothing else is pending takes the `refresh` slot, and while
/// it runs the transport keeps reading. A request whose answer does not
/// depend on that attempt's outcome ([`Lane::Observer`], and a repeated
/// refresh, which the session refuses `analysis_in_flight`) is admitted one
/// at a time into the `serial` slot. Any other request is held unanswered,
/// with nothing read behind it, until the refresh reply is flushed, so a
/// pipelined follower still binds the snapshot that refresh commits. At most
/// two requests and two replies exist at once and only one is an analysis.
#[derive(Default)]
struct Admission {
    slots: StdMutex<AdmissionSlots>,
    wake: Notify,
}
#[derive(Default)]
struct AdmissionSlots {
    serial: Option<RequestId>,
    refresh: Option<RequestId>,
}
impl Admission {
    fn slots(&self) -> Result<std::sync::MutexGuard<'_, AdmissionSlots>, Error> {
        self.slots
            .lock()
            .map_err(|_error| Error::other("MCP admission unavailable"))
    }
    /// Input is not read while the serial slot is occupied.
    fn serial_pending(&self) -> Result<bool, Error> {
        self.slots().map(|slots| slots.serial.is_some())
    }
    fn refresh_pending(&self) -> Result<bool, Error> {
        self.slots().map(|slots| slots.refresh.is_some())
    }
    /// Nothing admitted: the point where EOF and an idle signal end the session.
    fn is_idle(&self) -> Result<bool, Error> {
        self.slots()
            .map(|slots| slots.serial.is_none() && slots.refresh.is_none())
    }
    fn admit(&self, id: RequestId, lane: Lane) -> Result<(), Error> {
        let mut slots = self.slots()?;
        if slots.serial.is_some() {
            return Err(Error::other("MCP admission already pending"));
        }
        if lane == Lane::Refresh && slots.refresh.is_none() {
            slots.refresh = Some(id);
        } else {
            slots.serial = Some(id);
        }
        Ok(())
    }
    /// Releases the slot `id` holds, if any: its reply frame was flushed, or
    /// the client cancelled it and the SDK will drop the reply.
    fn complete(&self, id: Option<RequestId>) -> Result<(), Error> {
        let mut slots = self.slots()?;
        if id.is_none() {
            return Ok(());
        }
        let mut released = false;
        if slots.serial == id {
            slots.serial = None;
            released = true;
        }
        if slots.refresh == id {
            slots.refresh = None;
            released = true;
        }
        if released {
            self.wake.notify_one();
        }
        Ok(())
    }
}
/// How a request relates to a running refresh.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lane {
    /// A `ripr_refresh` call: the one request other input is read beside.
    Refresh,
    /// Answerable while a refresh runs: liveness, the catalog and workspace
    /// status, none of which reads the snapshot that refresh will commit.
    Observer,
    /// Everything else waits for the running refresh, in arrival order.
    Ordered,
}
/// Classified from the SDK-typed request, so the lane and the dispatched
/// handler never disagree. A tool call or catalog request whose raw params
/// were preserved as a custom request is refused quickly and stays ordered.
fn request_lane(request: &ClientRequest) -> Lane {
    use super::protocol::{REFRESH_TOOL_NAME, STATUS_RESOURCE_URI, STATUS_TOOL_NAME};
    match request {
        ClientRequest::CallToolRequest(call) if call.params.name == REFRESH_TOOL_NAME => {
            Lane::Refresh
        }
        ClientRequest::CallToolRequest(call) if call.params.name == STATUS_TOOL_NAME => {
            Lane::Observer
        }
        ClientRequest::ReadResourceRequest(read) if read.params.uri == STATUS_RESOURCE_URI => {
            Lane::Observer
        }
        ClientRequest::PingRequest(_)
        | ClientRequest::ListToolsRequest(_)
        | ClientRequest::ListResourcesRequest(_)
        | ClientRequest::ListResourceTemplatesRequest(_) => Lane::Observer,
        _ => Lane::Ordered,
    }
}
/// The request a `notifications/cancelled` frame names.
///
/// The SDK drops the reply of a request it cancelled, so no reply frame would
/// ever release that request's slot. Only reachable while a refresh runs:
/// input is not read while the serial slot is occupied.
fn cancelled_request_id(frame: &[u8]) -> Option<RequestId> {
    let value: serde_json::Value = serde_json::from_slice(frame).ok()?;
    if value.get("method")?.as_str()? != "notifications/cancelled" {
        return None;
    }
    serde_json::from_value(value.pointer("/params/requestId")?.clone()).ok()
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
/// A termination signal ends an idle session (#14102).
///
/// The server entry turns the first SIGINT or SIGTERM into a flag that a
/// running refresh attempt consumes as its cancellation. With no request
/// admitted nothing else reads that flag: the transport would stay blocked on
/// input, so a single `kill` or Ctrl-C could not stop the server, and the
/// armed flag would cancel whichever refresh came next. The transport
/// therefore treats a still-pending signal observed at its idle point (every
/// earlier reply flushed, no request admitted) as a clean end of input.
///
/// The pair is the server interrupt flag and the wake of a receive blocked on
/// input. An attempt that consumed the signal has already cleared the flag,
/// so that signal never ends the session, and the wake permit it left behind
/// is harmless: the flag alone decides. (A plain pair, so targets without a
/// signal source carry no unconstructed type.)
pub(super) type IdleInterrupt = (Arc<AtomicBool>, Arc<Notify>);
/// Consumes the pending signal, as an attempt's terminal exchange does: the
/// stop request ends this session only. A later `mcp::run` in the same
/// process starts with a clear flag, and the repeated-signal native escape
/// is re-armed by the next signal rather than left armed by this one.
fn take_idle_interrupt(interrupt: Option<&IdleInterrupt>) -> bool {
    interrupt.is_some_and(|(pending, _wake)| pending.swap(false, Ordering::SeqCst))
}
/// Resolves when a signal may be pending; never resolves without a source.
async fn idle_interrupt_wake(interrupt: Option<&IdleInterrupt>) {
    match interrupt {
        Some((_pending, wake)) => wake.notified().await,
        None => std::future::pending().await,
    }
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
    /// A request read while a refresh runs whose answer waits for it.
    held: Option<ClientJsonRpcMessage>,
    /// Input reached EOF while a request was still admitted. Shared across
    /// handshake attempts like the reader it describes.
    input_closed: Arc<AtomicBool>,
    idle_interrupt: Option<IdleInterrupt>,
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
    let echoable = id.is_string() || id.is_i64() || id.is_u64();
    // An id over the admission bound is never echoed, even in a rejection:
    // the reply would otherwise carry it past the response cap.
    let within_bound = serde_json::to_vec(id)
        .map(|serialized| serialized.len() <= super::MAX_REQUEST_ID_BYTES)
        .unwrap_or(false);
    (echoable && within_bound).then(|| id.clone())
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
            match self.admission.serial_pending() {
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
                    if self.admission.serial_pending().ok() != Some(true) {
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
            if self.held.is_some() {
                match self.admission.refresh_pending() {
                    Ok(true) => {
                        tokio::select! {
                            _ = self.failure.wake.notified() => return None,
                            _ = self.admission.wake.notified() => continue,
                        }
                    }
                    Ok(false) => {
                        let message = self.held.take()?;
                        if let ClientJsonRpcMessage::Request(request) = &message
                            && self
                                .admission
                                .admit(request.id.clone(), Lane::Ordered)
                                .is_err()
                        {
                            record_failure(&self.failure, "MCP admission unavailable");
                            return None;
                        }
                        return Some(message);
                    }
                    Err(_) => {
                        record_failure(&self.failure, "MCP admission unavailable");
                        return None;
                    }
                }
            }
            let idle = match self.admission.is_idle() {
                Ok(idle) => idle,
                Err(_) => {
                    record_failure(&self.failure, "MCP admission unavailable");
                    return None;
                }
            };
            // The idle point: nothing admitted, every earlier reply flushed.
            // A signal still pending here was not consumed by an attempt, so
            // it asks the server itself to stop (#14102). A running refresh
            // is not idle: the signal is that attempt's cancellation.
            if idle && take_idle_interrupt(self.idle_interrupt.as_ref()) {
                return None;
            }
            if self.input_closed.load(Ordering::SeqCst) {
                // EOF arrived while a refresh was running. Its reply is still
                // owed, exactly as when EOF could not be read before the
                // reply was flushed.
                if idle {
                    return None;
                }
                tokio::select! {
                    _ = self.failure.wake.notified() => return None,
                    _ = self.admission.wake.notified() => continue,
                }
            }
            let read = tokio::select! {
                _ = self.failure.wake.notified() => return None,
                // Re-check the flag from the top: an attempt may have
                // consumed the signal that left this wake behind.
                _ = idle_interrupt_wake(self.idle_interrupt.as_ref()) => continue,
                // The refresh reply was flushed while this read was blocked:
                // return to the top so the idle point is evaluated again.
                _ = self.admission.wake.notified(), if !idle => continue,
                read = async {
                    let mut reader = self.reader.lock().await;
                    reader.read_frame().await
                } => read,
            };
            let frame = match read {
                Ok(FrameRead::Eof) => {
                    self.input_closed.store(true, Ordering::SeqCst);
                    continue;
                }
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
                Ok(Some(mut message)) => {
                    if let ClientJsonRpcMessage::Request(request) = &message
                        && serde_json::to_vec(&request.id)
                            .map(|id| id.len() > super::MAX_REQUEST_ID_BYTES)
                            .unwrap_or(true)
                    {
                        match protocol_error_frame(
                            -32600,
                            "MCP request id exceeds the configured byte limit",
                        ) {
                            Ok(frame) => self.pending_protocol_error = Some(frame),
                            Err(error) => {
                                record_failure(
                                    &self.failure,
                                    super::writer::failure_reason(&error),
                                );
                                return None;
                            }
                        }
                        continue;
                    }
                    preserve_malformed_request_params(&mut message, &frame);
                    let request = match &message {
                        ClientJsonRpcMessage::Request(request) => {
                            Some((request.id.clone(), request_lane(&request.request)))
                        }
                        _ => None,
                    };
                    let admitted = match request {
                        Some((id, lane)) => match self.admission.refresh_pending() {
                            Ok(true) if lane == Lane::Ordered => {
                                self.held = Some(message);
                                continue;
                            }
                            Ok(_) => self.admission.admit(id, lane),
                            Err(error) => Err(error),
                        },
                        None if matches!(message, ClientJsonRpcMessage::Notification(_)) => {
                            self.admission.complete(cancelled_request_id(&frame))
                        }
                        None => Ok(()),
                    };
                    if admitted.is_err() {
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

/// The SDK can erase a wrongly typed catalog cursor or treat a null tool
/// argument object as omission. Keep that raw input available to the existing
/// custom request handler, after ID validation and before normal SDK dispatch.
/// Moving decoded extensions preserves protocol/metadata validation precedence.
fn preserve_malformed_request_params(message: &mut ClientJsonRpcMessage, frame: &[u8]) {
    let ClientJsonRpcMessage::Request(request) = message else {
        return;
    };
    if !matches!(
        &request.request,
        ClientRequest::CallToolRequest(_)
            | ClientRequest::ListToolsRequest(_)
            | ClientRequest::ListPromptsRequest(_)
            | ClientRequest::ListResourcesRequest(_)
            | ClientRequest::ListResourceTemplatesRequest(_)
    ) {
        return;
    }
    let Ok(raw) = serde_json::from_slice::<serde_json::Value>(frame) else {
        return;
    };
    let Some(params) = raw.get("params") else {
        return;
    };
    let malformed = if matches!(&request.request, ClientRequest::CallToolRequest(_)) {
        params
            .get("arguments")
            .is_some_and(|arguments| !arguments.is_object())
    } else {
        params
            .get("cursor")
            .is_some_and(|cursor| !cursor.is_null() && !cursor.is_string())
    };
    if !malformed {
        return;
    }
    let mut custom = CustomRequest::new(request.request.method(), Some(params.clone()));
    custom.extensions = std::mem::take(request.request.extensions_mut());
    request.request = ClientRequest::CustomRequest(custom);
}

pub(super) async fn serve_stdio(explicit_root: Option<PathBuf>) -> Result<(), String> {
    let (status, analysis_root) = WorkspaceStatus::resolve_with_root(explicit_root);
    if let Some(warning) = startup_warning(&status) {
        // Stdout is protocol-only; an operational failure belongs on stderr.
        // Fallible write: `eprintln!` panics when a stderr write fails, which
        // would abort startup before `serve`. A lost warning is acceptable;
        // the typed `workspace_unavailable` replies still carry the failure.
        let _ = writeln!(std::io::stderr(), "{warning}");
    }
    serve_interruptible(
        tokio::io::stdin(),
        tokio::io::stdout(),
        status,
        analysis_root,
        stdio_idle_interrupt()?,
    )
    .await
}
/// Bind the installed server interrupt flag to an async wake (Unix).
///
/// `None` when the entry installed no interrupts (an embedder that owns its
/// own signal policy): the transport then keeps stdin EOF as its only exit.
/// The streams share signal-hook's handler chain with the flag registration,
/// which runs first, so the flag is already set when a wake is observed.
#[cfg(unix)]
fn stdio_idle_interrupt() -> Result<Option<IdleInterrupt>, String> {
    use tokio::signal::unix::{SignalKind, signal};
    let Some(pending) = crate::cli::interrupt::server_interrupt_flag() else {
        return Ok(None);
    };
    let wake = Arc::new(Notify::new());
    for kind in [SignalKind::interrupt(), SignalKind::terminate()] {
        let mut stream = signal(kind).map_err(|error| {
            format!("observe MCP termination signals: {error}; restart `ripr mcp --stdio`, or report this as a ripr bug with the MCP client and ripr version")
        })?;
        let wake = wake.clone();
        let _forwarder = tokio::spawn(async move {
            while stream.recv().await.is_some() {
                wake.notify_one();
            }
        });
    }
    Ok(Some((pending, wake)))
}
#[cfg(not(unix))]
fn stdio_idle_interrupt() -> Result<Option<IdleInterrupt>, String> {
    Ok(None)
}
/// One operator-facing line when the workspace root did not resolve, so the
/// failure is not visible only as `workspace_unavailable` in tool output.
/// Carries no path: the status document never exposes an absolute path. The
/// reason and next step come from the root owner (`RootErrorCode`), the same
/// wording the `workspace_unavailable` tool replies use (#9895).
fn startup_warning(status: &WorkspaceStatus) -> Option<String> {
    if status.workspace_state != WorkspaceState::Unavailable {
        return None;
    }
    let (code, cause, next) = match status.root.error_code {
        Some(code) => (
            code.as_str(),
            code.cause(status.root.source),
            code.next_step(status.root.source),
        ),
        None => (
            "unknown",
            "the root could not be validated".to_string(),
            "pass `--root <repository>`",
        ),
    };
    Some(format!(
        "ripr mcp: warning: workspace root unavailable ({code}): {cause}; {next}. \
         Tools will fail closed with workspace_unavailable until the server is restarted."
    ))
}
/// The session without a signal source: stdin EOF is its only clean exit.
#[cfg(test)]
pub(super) async fn serve<R, W>(
    reader: R,
    writer: W,
    status: WorkspaceStatus,
    analysis_root: Option<PathBuf>,
) -> Result<(), String>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    serve_interruptible(reader, writer, status, analysis_root, None).await
}
pub(super) async fn serve_interruptible<R, W>(
    reader: R,
    writer: W,
    status: WorkspaceStatus,
    analysis_root: Option<PathBuf>,
    idle_interrupt: Option<IdleInterrupt>,
) -> Result<(), String>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let failure: Failure = Arc::new(TransportFailure::default());
    let writer = Arc::new(Mutex::new(FrameWriter::new(writer)));
    let reader = Arc::new(Mutex::new(FrameReader::new(reader)));
    let admission = Arc::new(Admission::default());
    let input_closed = Arc::new(AtomicBool::new(false));
    // A pre-initialize message the SDK refuses (a request without its
    // required `_meta`, or a stray notification) ends one handshake attempt
    // with the typed error already on the wire. ADR 0022's contract is
    // recovery with Invalid Request, not process death (#5267), so the
    // handshake restarts on the same connection; each attempt consumes at
    // least one frame, so the loop always makes progress toward a real
    // initialize, a valid-meta request, or stdin EOF.
    loop {
        let server = InitializeSessionService::new(status.clone(), analysis_root.clone())
            .map_err(|_error| "MCP status projection failed; restart `ripr mcp --stdio`, or report this as a ripr bug with the MCP client and ripr version".to_owned())?;
        let transport = BoundedTransport {
            reader: reader.clone(),
            writer: writer.clone(),
            failure: failure.clone(),
            admission: admission.clone(),
            pending_protocol_error: None,
            writer_needs_drain: false,
            held: None,
            input_closed: input_closed.clone(),
            idle_interrupt: idle_interrupt.clone(),
        };
        let service = match server.serve(transport).await {
            Ok(service) => service,
            Err(error) => {
                let reason = failure
                    .reason
                    .lock()
                    .map_err(|_error| "MCP transport status unavailable; restart `ripr mcp --stdio`, or report this as a ripr bug with the MCP client and ripr version".to_string())?
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
                    _ => return Err("MCP SDK startup failed; restart `ripr mcp --stdio`, or report this as a ripr bug with the MCP client and ripr version".to_string()),
                }
            }
        };
        service
            .waiting()
            .await
            .map_err(|_error| "MCP SDK service failed; restart `ripr mcp --stdio`, or report this as a ripr bug with the MCP client and ripr version".to_string())?;
        let error = failure
            .reason
            .lock()
            .map_err(|_error| "MCP transport status unavailable; restart `ripr mcp --stdio`, or report this as a ripr bug with the MCP client and ripr version".to_string())?
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
#[path = "wire_panic_tests.rs"]
mod wire_panic_tests;

#[cfg(test)]
#[path = "transport_backpressure_tests.rs"]
mod backpressure_tests;
