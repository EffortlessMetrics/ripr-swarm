//! Repair of raw (non-percent-encoded) non-ASCII characters in the
//! `initialize` request's URIs before tower-lsp-server deserializes them
//! (#10016).
//!
//! `ls_types::Uri` is a strict RFC 3986 parser: a `rootUri` of
//! `file:///tmp/café-repo` (raw UTF-8, no percent-encoding) fails the WHOLE
//! `initialize` params deserialization with an untyped
//! "unexpected character at index N" JSON-RPC error, so the session cannot
//! start even though the LSP spec only says clients SHOULD percent-encode and
//! real editors on non-English directory names send raw UTF-8.
//!
//! This adapter sits on the ingress byte stream (after the transport bounds,
//! so header/message caps still apply) and rewrites exactly one frame: the
//! first message whose `method` is `initialize`. Raw non-ASCII bytes in
//! `params.rootUri` and `params.workspaceFolders[].uri` are UTF-8
//! percent-encoded; ASCII — including an existing `%XX` escape — passes
//! untouched, so already-conformant clients are byte-identical. The repair is
//! a spelling repair only: `lsp/uri.rs` remains the single admission
//! authority for what the decoded path means. Every failure mode
//! (non-UTF-8 body, non-JSON body, missing Content-Length, oversized header)
//! falls through with the original bytes, preserving the pre-existing error
//! behavior exactly.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

/// Header scan cap. The bounded reader upstream enforces the real
/// `max_header_bytes`; this only bounds our accumulator so a hostile header
/// cannot grow memory before that trip fires.
const HEADER_SCAN_CAP: usize = 64 * 1024;

/// Framing terminator between LSP headers and body.
const HEADER_TERMINATOR: &[u8] = b"\r\n\r\n";

/// Incremental scan state for the one frame inspected before pass-through.
#[derive(Default)]
struct FrameScan {
    /// Everything read so far: header bytes, then body bytes once
    /// `content_length` is known.
    buf: Vec<u8>,
    /// Parsed Content-Length, set once the header terminator is seen.
    content_length: Option<usize>,
    /// How far `buf` has already been searched for the header terminator, so
    /// the per-chunk search only inspects the new tail.
    header_scanned: usize,
}

/// Wraps the ingress reader: repairs the first `initialize` frame, then
/// passes every later byte through unmodified.
pub(crate) struct InitializeRepairReader<R> {
    inner: R,
    scan: FrameScan,
    /// Bytes of the (repaired or original) inspected frame not yet consumed.
    pending: Vec<u8>,
    pending_pos: usize,
    /// Set once the first `initialize` frame has been inspected (or the scan
    /// gave up): afterwards `poll_read` delegates to `inner` directly.
    initialize_settled: bool,
}

impl<R: AsyncRead + Unpin> InitializeRepairReader<R> {
    pub(crate) fn new(inner: R) -> Self {
        Self {
            inner,
            scan: FrameScan::default(),
            pending: Vec::new(),
            pending_pos: 0,
            initialize_settled: false,
        }
    }

    /// True when the accumulated buffer ends with the header terminator not
    /// yet consumed into `content_length`. Searches only the unscanned tail.
    fn header_terminator_at(&mut self) -> Option<usize> {
        if self.scan.content_length.is_some() {
            return None;
        }
        let start = self
            .scan
            .header_scanned
            .saturating_sub(HEADER_TERMINATOR.len() - 1);
        let at = self.scan.buf[start..]
            .windows(HEADER_TERMINATOR.len())
            .position(|window| window == HEADER_TERMINATOR)
            .map(|pos| start + pos);
        self.scan.header_scanned = self.scan.buf.len();
        at
    }

    /// Reads the next chunk into the scan buffer. `Ok(false)` means the
    /// stream ended or the header cap tripped — the caller flushes what it
    /// has. A chunk read (not byte-by-byte) keeps a multi-KB `initialize`
    /// payload at a handful of polls instead of thousands.
    fn poll_scan_chunk(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<bool>> {
        if self.scan.content_length.is_none() && self.scan.buf.len() >= HEADER_SCAN_CAP {
            return Poll::Ready(Ok(false));
        }
        let mut chunk = [0u8; 512];
        let mut read_buf = ReadBuf::new(&mut chunk);
        match Pin::new(&mut self.inner).poll_read(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Ready(Ok(())) => {
                if read_buf.filled().is_empty() {
                    return Poll::Ready(Ok(false));
                }
                self.scan.buf.extend_from_slice(read_buf.filled());
                Poll::Ready(Ok(true))
            }
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for InitializeRepairReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        // Inspect one frame. Loop while the scan makes progress; yield on
        // Pending so the waker is registered by the inner read. The pending
        // drain and the pass-through delegation run at the TOP of every
        // iteration: settling a frame or hitting EOF `continue`s here rather
        // than scanning further.
        loop {
            if this.pending_pos < this.pending.len() {
                let available = this.pending.len() - this.pending_pos;
                let n = available.min(buf.remaining());
                buf.put_slice(&this.pending[this.pending_pos..this.pending_pos + n]);
                this.pending_pos += n;
                if this.pending_pos == this.pending.len() {
                    this.pending.clear();
                    this.pending_pos = 0;
                }
                return Poll::Ready(Ok(()));
            }
            if this.initialize_settled {
                return Pin::new(&mut this.inner).poll_read(cx, buf);
            }
            if let Some(content_length) = this.scan.content_length {
                let header_len = this
                    .scan
                    .buf
                    .windows(HEADER_TERMINATOR.len())
                    .position(|window| window == HEADER_TERMINATOR)
                    .map(|at| at + HEADER_TERMINATOR.len())
                    .unwrap_or(0);
                if this.scan.buf.len() >= header_len + content_length {
                    let frame_end = header_len + content_length;
                    // Settle exactly one frame: a single chunk read can carry
                    // the following frames too, and those must pass through
                    // untouched behind the inspected one.
                    let mut frame: Vec<u8> = this.scan.buf.drain(..frame_end).collect();
                    let settled = settle_frame(std::mem::take(&mut frame));
                    this.pending = settled;
                    this.pending.extend_from_slice(&this.scan.buf);
                    this.scan.buf.clear();
                    this.pending_pos = 0;
                    this.initialize_settled = true;
                    continue;
                }
            } else if let Some(at) = this.header_terminator_at() {
                match parse_content_length(&this.scan.buf[..at]) {
                    Some(content_length) => {
                        this.scan.content_length = Some(content_length);
                        // Re-loop: the frame may already be complete in the
                        // buffer (a chunk read carries header and body).
                        continue;
                    }
                    // Unparseable header: forward what we have unchanged.
                    None => {
                        this.pending = std::mem::take(&mut this.scan.buf);
                        this.pending_pos = 0;
                        this.initialize_settled = true;
                        continue;
                    }
                }
            }
            match this.poll_scan_chunk(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Ready(Ok(true)) => continue,
                Poll::Ready(Ok(false)) => {
                    // EOF or header cap mid-frame. A complete frame still
                    // settles (an EOF exactly at the frame boundary reaches
                    // here when the terminator arrived with the last chunk).
                    if this.scan.content_length.is_some() {
                        continue;
                    }
                    // Forward the partial bytes unchanged and switch to
                    // pass-through.
                    this.pending = std::mem::take(&mut this.scan.buf);
                    this.pending_pos = 0;
                    this.initialize_settled = true;
                    continue;
                }
            }
        }
    }
}

/// Processes one complete (or EOF-truncated) frame: repairs it when it is an
/// `initialize` request with raw non-ASCII URIs; otherwise returns the
/// original bytes.
fn settle_frame(frame: Vec<u8>) -> Vec<u8> {
    let Some(header_end) = frame
        .windows(HEADER_TERMINATOR.len())
        .position(|window| window == HEADER_TERMINATOR)
    else {
        return frame;
    };
    let body = &frame[header_end + HEADER_TERMINATOR.len()..];
    let Ok(text) = std::str::from_utf8(body) else {
        return frame;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) else {
        return frame;
    };
    if value.get("method").and_then(serde_json::Value::as_str) != Some("initialize") {
        return frame;
    }
    if !repair_initialize_params(&mut value) {
        return frame;
    }
    let Ok(new_body) = serde_json::to_vec(&value) else {
        return frame;
    };
    build_frame(&frame[..header_end], &new_body)
}

/// Rebuilds a framed message, rewriting or appending Content-Length and
/// keeping every other header line as received.
fn build_frame(original_header: &[u8], body: &[u8]) -> Vec<u8> {
    let header_text = String::from_utf8_lossy(original_header);
    let mut out = String::with_capacity(header_text.len() + 32);
    let mut found = false;
    for line in header_text.split("\r\n") {
        if line.is_empty() {
            continue;
        }
        match line.split_once(':') {
            Some((name, _)) if name.trim().eq_ignore_ascii_case("content-length") => {
                found = true;
                out.push_str("Content-Length: ");
                out.push_str(body.len().to_string().as_str());
                out.push_str("\r\n");
            }
            _ => {
                out.push_str(line);
                out.push_str("\r\n");
            }
        }
    }
    if !found {
        out.push_str("Content-Length: ");
        out.push_str(body.len().to_string().as_str());
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    let mut frame = out.into_bytes();
    frame.extend_from_slice(body);
    frame
}

/// Extracts Content-Length from a header block (which may still carry its
/// trailing terminator).
fn parse_content_length(header: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(header).ok()?;
    for line in text.split("\r\n") {
        // A colon-less header line (or the empty terminator) must not abort
        // the search for a later Content-Length.
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            return value.trim().parse().ok();
        }
    }
    None
}

/// Percent-encodes raw non-ASCII bytes, leaving every ASCII byte — including
/// an existing `%` of a valid escape — untouched.
fn percent_encode_non_ascii(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii() {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0F));
        }
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        10..=15 => char::from(b'A' + (nibble - 10)),
        _ => '0',
    }
}

/// Repairs `params.rootUri` and `params.workspaceFolders[].uri` in place.
/// Returns true when any value changed.
fn repair_initialize_params(value: &mut serde_json::Value) -> bool {
    let mut changed = false;
    let Some(params) = value.get_mut("params") else {
        return false;
    };
    if let Some(root_uri) = params.get_mut("rootUri") {
        changed |= repair_uri_value(root_uri);
    }
    if let Some(folders) = params
        .get_mut("workspaceFolders")
        .and_then(serde_json::Value::as_array_mut)
    {
        for folder in folders {
            if let Some(uri) = folder.get_mut("uri") {
                changed |= repair_uri_value(uri);
            }
        }
    }
    changed
}

/// Replaces a raw non-ASCII URI string with its percent-encoded spelling.
fn repair_uri_value(value: &mut serde_json::Value) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    if text.is_ascii() {
        return false;
    }
    *value = serde_json::Value::String(percent_encode_non_ascii(text));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn frame(body: &str) -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }

    #[test]
    fn percent_encode_encodes_only_non_ascii_bytes() {
        assert_eq!(
            percent_encode_non_ascii("file:///tmp/café-repo"),
            "file:///tmp/caf%C3%A9-repo"
        );
        assert_eq!(
            percent_encode_non_ascii("file:///tmp/caf%C3%A9-repo"),
            "file:///tmp/caf%C3%A9-repo"
        );
        assert_eq!(
            percent_encode_non_ascii("file:///plain/path"),
            "file:///plain/path"
        );
    }

    #[test]
    fn repair_rewrites_raw_non_ascii_root_uri() -> Result<(), String> {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":"file:///tmp/café-repo","capabilities":{}}}"#;
        let input = frame(body);
        let out = settle_frame(input.clone());
        assert_ne!(out, input);
        let text =
            String::from_utf8(out).map_err(|_| "repaired frame must be utf-8".to_string())?;
        assert!(text.contains("caf%C3%A9-repo"), "repaired frame: {text}");
        let length_line = text.lines().next().ok_or("missing header line")?;
        let declared: usize = length_line
            .split(':')
            .nth(1)
            .ok_or("missing length value")?
            .trim()
            .parse()
            .map_err(|_| "length must be numeric".to_string())?;
        let body_start = text.find("\r\n\r\n").ok_or("missing terminator")? + 4;
        assert_eq!(declared, text.len() - body_start);
        Ok(())
    }

    #[test]
    fn repair_rewrites_workspace_folder_uris() -> Result<(), String> {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"workspaceFolders":[{"uri":"file:///c:/用户/proj","name":"proj"}],"capabilities":{}}}"#;
        let out = settle_frame(frame(body));
        let text =
            String::from_utf8(out).map_err(|_| "repaired frame must be utf-8".to_string())?;
        assert!(
            text.contains("%E7%94%A8%E6%88%B7"),
            "repaired frame: {text}"
        );
        Ok(())
    }

    #[test]
    fn already_encoded_and_ascii_uris_pass_through_unchanged() {
        let encoded = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///tmp/caf%C3%A9-repo","capabilities":{}}}"#;
        assert_eq!(settle_frame(frame(encoded)), frame(encoded));
        let plain = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///tmp/plain","capabilities":{}}}"#;
        assert_eq!(settle_frame(frame(plain)), frame(plain));
    }

    #[test]
    fn non_initialize_and_malformed_frames_pass_through_unchanged() {
        let other = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        assert_eq!(settle_frame(frame(other)), frame(other));
        let malformed = "not json at all";
        assert_eq!(settle_frame(frame(malformed)), frame(malformed));
    }

    #[test]
    fn missing_content_length_flushes_original_bytes() {
        let raw = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}".to_vec();
        assert_eq!(settle_frame(raw.clone()), raw);
    }

    #[tokio::test]
    async fn reader_repairs_initialize_then_passes_through_byte_identical() -> Result<(), String> {
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":"file:///tmp/café-repo","capabilities":{}}}"#;
        let next = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        let mut stream = frame(init);
        stream.extend_from_slice(&frame(next));
        let mut reader = InitializeRepairReader::new(stream.as_slice());
        let mut out = Vec::new();
        reader
            .read_to_end(&mut out)
            .await
            .map_err(|err| format!("read failed: {err}"))?;
        let text = String::from_utf8(out).map_err(|_| "output must be utf-8".to_string())?;
        assert!(text.contains("caf%C3%A9-repo"), "repaired: {text}");
        // The frame after initialize is byte-identical, including its own
        // Content-Length line.
        let next_frame =
            String::from_utf8(frame(next)).map_err(|_| "frame must be utf-8".to_string())?;
        assert!(text.contains(&next_frame));
        Ok(())
    }

    #[tokio::test]
    async fn reader_terminates_at_eof_after_the_inspected_frame() -> Result<(), String> {
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///tmp/café-repo","capabilities":{}}}"#;
        let mut stream = frame(init);
        stream.extend_from_slice(&frame(r#"{"jsonrpc":"2.0","method":"exit"}"#));
        let mut reader = InitializeRepairReader::new(stream.as_slice());
        let mut out = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            reader.read_to_end(&mut out),
        )
        .await
        .map_err(|_| "read_to_end must terminate at EOF".to_string())?
        .map_err(|err| format!("read failed: {err}"))?;
        let text = String::from_utf8(out).map_err(|_| "output must be utf-8".to_string())?;
        assert!(text.contains("caf%C3%A9-repo"), "repaired: {text}");
        assert!(
            text.contains("exit"),
            "trailing frame passes through: {text}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn reader_passes_through_when_no_initialize_arrives() -> Result<(), String> {
        let other = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        let stream = frame(other);
        let mut reader = InitializeRepairReader::new(stream.as_slice());
        let mut out = Vec::new();
        reader
            .read_to_end(&mut out)
            .await
            .map_err(|err| format!("read failed: {err}"))?;
        assert_eq!(out, frame(other));
        Ok(())
    }
}
