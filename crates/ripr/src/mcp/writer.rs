use rmcp::model::{ErrorData, ServerJsonRpcMessage};
use serde::Serialize;
use std::io::{Error, ErrorKind, Write};
use tokio::io::{AsyncWrite, AsyncWriteExt};

struct CappedBuffer(Vec<u8>);
impl Write for CappedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let len = self
            .0
            .len()
            .checked_add(bytes.len())
            .ok_or_else(output_limit)?;
        if len >= super::MAX_RESPONSE_BYTES {
            return Err(output_limit());
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
enum EncodingFailure {
    OutputLimit,
    Encoding,
}
impl std::fmt::Display for EncodingFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::OutputLimit => "MCP output limit",
            Self::Encoding => "MCP encoding failed",
        })
    }
}
impl std::error::Error for EncodingFailure {}
fn output_limit() -> Error {
    Error::new(ErrorKind::InvalidData, EncodingFailure::OutputLimit)
}
pub(super) fn failure_reason(error: &Error) -> &'static str {
    match error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<EncodingFailure>())
    {
        Some(EncodingFailure::OutputLimit) => "MCP output limit",
        Some(EncodingFailure::Encoding) => "MCP encoding failed",
        None => "MCP output IO failed",
    }
}
fn encode<T: Serialize>(item: &T) -> std::io::Result<Vec<u8>> {
    let mut buffer = CappedBuffer(Vec::new());
    serde_json::to_writer(&mut buffer, item).map_err(|error| {
        if error.io_error_kind() == Some(ErrorKind::InvalidData) {
            output_limit()
        } else {
            Error::new(ErrorKind::InvalidData, EncodingFailure::Encoding)
        }
    })?;
    buffer.0.push(b'\n');
    Ok(buffer.0)
}
pub(super) fn encode_message(item: &ServerJsonRpcMessage) -> std::io::Result<Vec<u8>> {
    match encode(item) {
        Ok(frame) => Ok(frame),
        Err(error) => {
            if !matches!(
                error
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<EncodingFailure>()),
                Some(EncodingFailure::OutputLimit)
            ) {
                return Err(error);
            }
            let id = match item {
                ServerJsonRpcMessage::Response(response) => Some(response.id.clone()),
                ServerJsonRpcMessage::Error(error) => error.id.clone(),
                _ => return Err(output_limit()),
            };
            let fallback = ServerJsonRpcMessage::error(
                ErrorData::internal_error(
                    "MCP response exceeds the configured byte limit",
                    Some(serde_json::json!({"maxResponseBytes": super::MAX_RESPONSE_BYTES})),
                ),
                id,
            );
            // A readable ID that cannot fit is never replaced with an
            // uncorrelated ID. The service terminates with a bounded reason.
            encode(&fallback)
        }
    }
}

/// Cursor and bytes survive cancellation while the mutex guard is dropped.
pub(super) struct FrameWriter<W> {
    writer: W,
    pending: Vec<u8>,
    cursor: usize,
}
impl<W: AsyncWrite + Unpin> FrameWriter<W> {
    pub(super) fn new(writer: W) -> Self {
        Self {
            writer,
            pending: Vec::new(),
            cursor: 0,
        }
    }
    pub(super) async fn finish_pending(&mut self) -> std::io::Result<()> {
        while let Some(bytes) = self
            .pending
            .get(self.cursor..)
            .filter(|bytes| !bytes.is_empty())
        {
            let written = self.writer.write(bytes).await?;
            if written == 0 {
                return Err(Error::new(ErrorKind::WriteZero, "write MCP response"));
            }
            self.cursor = self.cursor.checked_add(written).ok_or_else(output_limit)?;
        }
        self.writer.flush().await?;
        self.pending.clear();
        self.cursor = 0;
        Ok(())
    }
    pub(super) async fn send(&mut self, item: &ServerJsonRpcMessage) -> std::io::Result<()> {
        self.finish_pending().await?;
        self.queue(item)?;
        self.finish_pending().await
    }
    pub(super) fn queue(&mut self, item: &ServerJsonRpcMessage) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            return Err(Error::new(ErrorKind::WouldBlock, "MCP output pending"));
        }
        self.pending = encode_message(item)?;
        Ok(())
    }
    pub(super) async fn close(&mut self) -> std::io::Result<()> {
        self.finish_pending().await?;
        self.writer.shutdown().await
    }
    #[cfg(test)]
    pub(super) fn output(&self) -> &W {
        &self.writer
    }
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
