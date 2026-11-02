//! Session-end signal for in-flight analysis (#11439).
//!
//! tower-lsp-server stops serving as soon as `exit` arrives or stdin ends, but
//! it first waits for every request and notification already in flight. A
//! `didOpen` handler awaits its whole refresh, and `shutdown` itself is
//! cancelled by an `exit` that follows it back to back, so its
//! `refresh_scheduler.stop()` never runs. The analysis then ran to completion
//! before the process could return (about 12 s for a 6,000-line unsaved
//! buffer).
//!
//! The two events that end a session therefore also cancel the analysis
//! directly: the `exit` notification (recorded by `RecordShutdownExit`) and
//! the end of the ingress byte stream (`EndOfInput`, which also covers a read
//! error). Both call the same hook, which is `RefreshScheduler::stop`: it
//! only cancels the active and pending tokens, so the analysis exits at its
//! next checkpoint and its result is dropped fail-closed. It is idempotent
//! and does not replace `shutdown`'s terminal diagnostics clear.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

/// Cancels in-flight analysis because the session is ending.
pub(super) type SessionEndHook = Arc<dyn Fn() + Send + Sync>;

/// Fires the hook once, however many session-end signals arrive.
#[derive(Clone)]
pub(super) struct SessionEnd {
    hook: SessionEndHook,
    fired: Arc<AtomicBool>,
}

impl SessionEnd {
    pub(super) fn new(hook: SessionEndHook) -> Self {
        Self {
            hook,
            fired: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn signal(&self) {
        if !self.fired.swap(true, Ordering::SeqCst) {
            (self.hook)();
        }
    }
}

/// Signals the session end when the wrapped reader reaches end of input or
/// fails. Bytes pass through untouched.
pub(super) struct EndOfInput<R> {
    inner: R,
    end: SessionEnd,
}

impl<R> EndOfInput<R> {
    pub(super) fn new(inner: R, end: SessionEnd) -> Self {
        Self { inner, end }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for EndOfInput<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let polled = Pin::new(&mut self.inner).poll_read(cx, buf);
        match &polled {
            Poll::Ready(Ok(())) if buf.filled().len() == before && buf.remaining() > 0 => {
                self.end.signal();
            }
            Poll::Ready(Err(_)) => self.end.signal(),
            _ => {}
        }
        polled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::AsyncReadExt;

    fn counting_end() -> (SessionEnd, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&count);
        let end = SessionEnd::new(Arc::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        (end, count)
    }

    #[test]
    fn signal_runs_the_hook_exactly_once() {
        let (end, count) = counting_end();
        end.signal();
        end.clone().signal();
        end.signal();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn end_of_input_signals_only_after_the_last_byte() -> Result<(), String> {
        let (end, count) = counting_end();
        let mut reader = EndOfInput::new(&b"abc"[..], end);
        let mut one = [0u8; 2];
        let n = reader.read(&mut one).await.map_err(|e| e.to_string())?;
        assert_eq!(&one[..n], b"ab");
        assert_eq!(count.load(Ordering::SeqCst), 0, "data is not the end");
        let n = reader.read(&mut one).await.map_err(|e| e.to_string())?;
        assert_eq!(&one[..n], b"c");
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "the last bytes are not the end"
        );
        let n = reader.read(&mut one).await.map_err(|e| e.to_string())?;
        assert_eq!(n, 0);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        Ok(())
    }

    struct FailingReader;

    impl AsyncRead for FailingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::other("broken pipe")))
        }
    }

    #[tokio::test]
    async fn a_read_error_also_ends_the_session() {
        let (end, count) = counting_end();
        let mut reader = EndOfInput::new(FailingReader, end);
        let mut byte = [0u8; 1];
        let failure = reader.read(&mut byte).await;
        assert_eq!(failure.map_err(|e| e.kind()), Err(io::ErrorKind::Other));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    /// The `exit` notification reaches the hook through the real layer, even
    /// though no `shutdown` handler ran; any other method does not.
    #[tokio::test]
    async fn the_exit_notification_signals_the_session_end() -> Result<(), String> {
        use super::super::{RecordShutdownExit, ShutdownExitOrder};
        use tower::Service;
        use tower_lsp_server::jsonrpc::{Request, Response};

        struct Ignore;
        impl Service<Request> for Ignore {
            type Response = Option<Response>;
            type Error = std::convert::Infallible;
            type Future = std::future::Ready<Result<Option<Response>, Self::Error>>;
            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }
            fn call(&mut self, _request: Request) -> Self::Future {
                std::future::ready(Ok(None))
            }
        }

        let (end, count) = counting_end();
        let order = ShutdownExitOrder::default().ending_session_with(end);
        let mut layer = RecordShutdownExit::new(Ignore, order);
        for method in ["textDocument/didOpen", "shutdown"] {
            layer
                .call(Request::build(method.to_owned()).finish())
                .await
                .map_err(|e| e.to_string())?;
        }
        assert_eq!(count.load(Ordering::SeqCst), 0);
        layer
            .call(Request::build("exit".to_owned()).finish())
            .await
            .map_err(|e| e.to_string())?;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        Ok(())
    }
}
