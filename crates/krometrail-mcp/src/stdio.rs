//! Byte-preserving stdio supervision, independent of the SDK's completion reason.
use std::{
    future::Future as _,
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

// Bound a stalled local response write, not the action producing that response.
// In particular, SDK opening writes cannot rely on another stdin poll to observe EOF.
const WRITE_STALL: Duration = Duration::from_secs(3);

pub(crate) struct State {
    stop: CancellationToken,
    read_failed: AtomicBool,
    write_failed: AtomicBool,
    unflushed: AtomicBool,
}
impl State {
    pub(crate) fn new(stop: CancellationToken) -> Arc<Self> {
        Arc::new(Self {
            stop,
            read_failed: AtomicBool::new(false),
            write_failed: AtomicBool::new(false),
            unflushed: AtomicBool::new(false),
        })
    }
    pub(crate) fn failure(&self) -> Option<&'static str> {
        if self.read_failed.load(Ordering::SeqCst) {
            Some("MCP stdin read failed; request delivery may be incomplete")
        } else if self.write_failed.load(Ordering::SeqCst) || self.unflushed.load(Ordering::SeqCst)
        {
            Some(
                "MCP response transport did not finish shutdown; unread responses were interrupted",
            )
        } else {
            None
        }
    }
    fn write_failure(&self) {
        self.write_failed.store(true, Ordering::SeqCst);
        self.stop.cancel();
    }
}

pub(crate) struct Reader<R> {
    pub(crate) inner: R,
    pub(crate) state: Arc<State>,
}
impl<R: AsyncRead + Unpin> AsyncRead for Reader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let capacity = buf.remaining();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        match &result {
            Poll::Ready(Err(_)) => {
                self.state.read_failed.store(true, Ordering::SeqCst);
                self.state.stop.cancel();
            }
            Poll::Ready(Ok(())) if capacity > 0 && buf.filled().len() == before => {
                self.state.stop.cancel()
            }
            _ => {}
        }
        result
    }
}

pub(crate) struct Writer<W> {
    inner: W,
    state: Arc<State>,
    timer: Option<Pin<Box<tokio::time::Sleep>>>,
}
impl<W> Writer<W> {
    pub(crate) fn new(inner: W, state: Arc<State>) -> Self {
        Self {
            inner,
            state,
            timer: None,
        }
    }
    fn pending<T>(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<T>> {
        let timer = self
            .timer
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(WRITE_STALL)));
        if timer.as_mut().poll(cx).is_ready() {
            self.state.write_failure();
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "MCP output made no observable progress before its write deadline",
            )))
        } else {
            Poll::Pending
        }
    }
    fn failed<T>(&self, error: io::Error) -> Poll<io::Result<T>> {
        self.state.write_failure();
        Poll::Ready(Err(error))
    }
}
impl<W: AsyncWrite + Unpin> AsyncWrite for Writer<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if !bytes.is_empty() {
            self.state.unflushed.store(true, Ordering::SeqCst);
        }
        match Pin::new(&mut self.inner).poll_write(cx, bytes) {
            Poll::Pending => self.pending(cx),
            Poll::Ready(Err(error)) => self.failed(error),
            Poll::Ready(Ok(0)) if !bytes.is_empty() => self.failed(io::ErrorKind::WriteZero.into()),
            result => {
                self.timer = None;
                result
            }
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_flush(cx) {
            Poll::Pending => self.pending(cx),
            Poll::Ready(Err(error)) => self.failed(error),
            Poll::Ready(Ok(())) => {
                self.timer = None;
                self.state.unflushed.store(false, Ordering::SeqCst);
                Poll::Ready(Ok(()))
            }
        }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_shutdown(cx) {
            Poll::Pending => self.pending(cx),
            Poll::Ready(Err(error)) => self.failed(error),
            Poll::Ready(Ok(())) => {
                self.timer = None;
                self.state.unflushed.store(false, Ordering::SeqCst);
                Poll::Ready(Ok(()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt as _;

    #[tokio::test(start_paused = true)]
    async fn stalled_write_is_bounded_without_any_reader_poll_or_shutdown_signal() {
        let stop = CancellationToken::new();
        let state = State::new(stop.clone());
        let (inner, _unread) = tokio::io::duplex(1);
        let mut writer = Writer::new(inner, state.clone());
        let began = tokio::time::Instant::now();
        assert_eq!(
            writer.write_all(b"unread output").await.unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(began.elapsed(), WRITE_STALL);
        assert!(stop.is_cancelled());
        assert!(state.failure().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn action_time_is_not_a_write_deadline_and_flushed_bytes_are_clean() {
        let state = State::new(CancellationToken::new());
        let mut writer = Writer::new(tokio::io::sink(), state.clone());
        tokio::time::advance(Duration::from_secs(120)).await;
        assert!(state.failure().is_none());
        writer.write_all(b"complete response\n").await.unwrap();
        assert!(state.failure().is_some(), "accepted but not yet flushed");
        writer.flush().await.unwrap();
        drop(writer);
        assert!(state.failure().is_none());
    }
}
