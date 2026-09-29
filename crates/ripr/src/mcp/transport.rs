use super::{
    framing::{FrameRead, FrameReader},
    server::McpServer,
    writer::FrameWriter,
};
use crate::workspace_status::WorkspaceStatus;
use rmcp::{
    RoleServer, ServiceExt,
    model::{ClientJsonRpcMessage, ErrorData, ServerJsonRpcMessage},
    transport::{
        Transport,
        async_rw::{JsonRpcMessageCodec, JsonRpcMessageCodecError},
    },
};
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
fn record_failure(failure: &Failure, reason: &'static str) {
    if let Ok(mut stored) = failure.reason.lock() {
        if stored.is_none() {
            *stored = Some(reason);
        }
    }
    // A response send task can fail while the SDK is awaiting receive.
    // Notify retains a permit even if that future is between polls.
    failure.wake.notify_one();
}
struct BoundedTransport<R, W> {
    reader: FrameReader<R>,
    writer: Arc<Mutex<FrameWriter<W>>>,
    failure: Failure,
    pending_protocol_error: Option<ServerJsonRpcMessage>,
    writer_needs_drain: bool,
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
        async move {
            let result = writer.lock().await.send(&item).await;
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
            if self.pending_protocol_error.is_some() || self.writer_needs_drain {
                let mut writer = self.writer.lock().await;
                if let Err(error) = writer.finish_pending().await {
                    record_failure(&self.failure, super::writer::failure_reason(&error));
                    return None;
                }
                self.writer_needs_drain = false;
                if let Some(error) = self.pending_protocol_error.as_ref() {
                    if let Err(error) = writer.queue(error) {
                        record_failure(&self.failure, super::writer::failure_reason(&error));
                        return None;
                    }
                    self.pending_protocol_error = None;
                    self.writer_needs_drain = true;
                    if let Err(error) = writer.finish_pending().await {
                        record_failure(&self.failure, super::writer::failure_reason(&error));
                        return None;
                    }
                    self.writer_needs_drain = false;
                }
            }
            let read = tokio::select! {
                _ = self.failure.wake.notified() => return None,
                read = self.reader.read_frame() => read,
            };
            let frame = match read {
                Ok(FrameRead::Eof) => return None,
                Ok(FrameRead::Empty) => continue,
                Ok(FrameRead::Oversized) => {
                    let error = ServerJsonRpcMessage::error(
                        ErrorData::invalid_request(
                            "MCP message exceeds the configured byte limit",
                            None,
                        ),
                        None,
                    );
                    self.pending_protocol_error = Some(error);
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
                Ok(Some(message)) => return Some(message),
                Ok(None) => continue,
                Err(JsonRpcMessageCodecError::Serde(error))
                    if matches!(
                        error.classify(),
                        serde_json::error::Category::Syntax | serde_json::error::Category::Eof
                    ) =>
                {
                    continue;
                }
                Err(_) => {
                    let error = ServerJsonRpcMessage::error(
                        ErrorData::invalid_request("Invalid request", None),
                        None,
                    );
                    self.pending_protocol_error = Some(error);
                }
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
    serve(
        tokio::io::stdin(),
        tokio::io::stdout(),
        WorkspaceStatus::resolve(explicit_root),
    )
    .await
}
async fn serve<R, W>(reader: R, writer: W, status: WorkspaceStatus) -> Result<(), String>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let failure: Failure = Arc::new(TransportFailure::default());
    let transport = BoundedTransport {
        reader: FrameReader::new(reader),
        writer: Arc::new(Mutex::new(FrameWriter::new(writer))),
        failure: failure.clone(),
        pending_protocol_error: None,
        writer_needs_drain: false,
    };
    let server = McpServer::new(status).map_err(|_| "MCP status projection failed".to_owned())?;
    let service = match server.serve(transport).await {
        Ok(service) => service,
        Err(error) => {
            let reason = failure
                .reason
                .lock()
                .map_err(|_| "MCP transport status unavailable".to_string())?
                .take();
            return match reason {
                Some(reason) => Err(reason.to_string()),
                None if matches!(
                    error,
                    rmcp::service::ServerInitializeError::ConnectionClosed(_)
                ) =>
                {
                    Ok(())
                }
                None => Err("MCP SDK startup failed".to_string()),
            };
        }
    };
    service
        .waiting()
        .await
        .map_err(|_| "MCP SDK service failed".to_string())?;
    let error = failure
        .reason
        .lock()
        .map_err(|_| "MCP transport status unavailable".to_string())?
        .take();
    match error {
        Some(error) => Err(error.to_string()),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transport_backpressure_tests.rs"]
mod backpressure_tests;
