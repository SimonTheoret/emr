//! Turns one proxied request and its response into one saved `Exchange`.
//!
//! Each body is wrapped in a `Tap` that owns its own frames. A tap hands its frames over a
//! oneshot only if its body was read to the end, so an error or an early drop leaves the
//! receiver with nothing. One task per exchange joins the two receivers and saves.

use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
    time::Instant,
};

use bytes::Bytes;
use futures::future::try_join;
use http::request;
use http_body::{Body, Frame, SizeHint};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::{
    ir::{ERMRequestParts, ERMResponseParts, FinalRequest, FinalResponse, TimedFrame, UrlIR},
    storage::{Exchange, Id, Recorder},
};

/// The only path to the `Recorder`. Every clone keeps `Drained::wait` pending, and every
/// exchange still capturing holds one, so `Writer::finish` cannot run while a `save` is
/// still possible.
#[derive(Clone)]
pub(super) struct Saver {
    recorder: Arc<dyn Recorder>,
    _alive: mpsc::Sender<Infallible>,
}

/// Resolves once every `Saver` clone is dropped.
pub(super) struct Drained(mpsc::Receiver<Infallible>);

/// One exchange whose request body is being captured. Dropping it saves nothing, which is
/// how the `502` path and a client that leaves before the response head both end.
pub(super) struct Capture {
    saver: Saver,
    id: Id,
    request: ERMRequestParts,
    request_body: oneshot::Receiver<Vec<TimedFrame>>,
}

/// Passes a body through unchanged, including `is_end_stream` and `size_hint`, and records
/// each data frame with its delay.
pub(super) struct Tap<B: Body<Data = Bytes> + Unpin> {
    inner: B,
    /// `None` once the outcome is decided: the frames were sent, or the body failed.
    capturing: Option<Capturing>,
}

struct Capturing {
    frames: Vec<TimedFrame>,
    last: Instant,
    done: oneshot::Sender<Vec<TimedFrame>>,
}

impl Saver {
    pub(super) fn new(recorder: impl Recorder) -> (Self, Drained) {
        let (alive, gone) = mpsc::channel(1);
        let saver = Self {
            recorder: Arc::new(recorder),
            _alive: alive,
        };
        (saver, Drained(gone))
    }

    /// Starts capturing the exchange `id`. Forward the returned body in place of `body`.
    pub(super) fn capture<B: Body<Data = Bytes> + Unpin>(
        &self,
        id: Id,
        request: &request::Parts,
        body: B,
    ) -> (Capture, Tap<B>) {
        let (body, request_body) = Tap::new(body);
        let capture = Capture {
            saver: self.clone(),
            id,
            request: ERMRequestParts::from(request),
            request_body,
        };
        (capture, body)
    }

    /// Takes `&self`, so a task that saves captures the whole `Saver`, `_alive` included.
    /// Naming `saver.recorder` inside an `async move` block captures only that field, and the
    /// task would stop holding off `Drained`.
    fn save(&self, exchange: Exchange) {
        self.recorder.save(exchange);
    }
}

impl Capture {
    /// Taps the upstream response body. The exchange is saved once both bodies are read to
    /// their end, in whichever order they finish.
    pub(super) fn respond(self, upstream: reqwest::Response) -> http::Response<Tap<reqwest::Body>> {
        let url = UrlIR(upstream.url().clone());
        let (head, body) = http::Response::from(upstream).into_parts();
        let response = ERMResponseParts {
            url,
            version: head.version.into(),
            headers: head.headers.clone().into(),
            status: head.status.into(),
            id: Uuid::nil(),
        };
        let (body, response_body) = Tap::new(body);
        let Self {
            saver,
            id,
            request,
            request_body,
        } = self;
        tokio::spawn(async move {
            // Returns early on the first receiver whose tap ended without a complete body.
            let Ok((request_frames, response_frames)) = try_join(request_body, response_body).await
            else {
                return;
            };
            saver.save(Exchange {
                id,
                request: FinalRequest {
                    parts: request,
                    body: request_frames,
                },
                response: FinalResponse {
                    parts: response,
                    body: response_frames,
                },
            });
        });
        http::Response::from_parts(head, body)
    }
}

impl Drained {
    pub(super) async fn wait(mut self) {
        // `recv` returns `None` once every sender is gone, and `Infallible` means it returns
        // nothing else.
        let _ = self.0.recv().await;
    }
}

impl<B: Body<Data = Bytes> + Unpin> Tap<B> {
    fn new(inner: B) -> (Self, oneshot::Receiver<Vec<TimedFrame>>) {
        let (done, frames) = oneshot::channel();
        let tap = Self {
            inner,
            capturing: Some(Capturing {
                frames: Vec::new(),
                last: Instant::now(),
                done,
            }),
        };
        (tap, frames)
    }
}

impl Capturing {
    fn push(&mut self, content: Bytes) {
        let now = Instant::now();
        self.frames.push(TimedFrame {
            content,
            tslf: now - self.last,
        });
        self.last = now;
    }

    fn finish(self) {
        // A closed receiver means the exchange was already abandoned.
        let _ = self.done.send(self.frames);
    }
}

impl<B: Body<Data = Bytes> + Unpin> Body for Tap<B> {
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, B::Error>>> {
        let tap = self.get_mut();
        let frame = ready!(Pin::new(&mut tap.inner).poll_frame(cx));
        match &frame {
            Some(Ok(frame)) => {
                if let (Some(data), Some(capturing)) = (frame.data_ref(), &mut tap.capturing) {
                    capturing.push(data.clone());
                }
            }
            Some(Err(_)) => tap.capturing = None,
            None => {
                if let Some(capturing) = tap.capturing.take() {
                    capturing.finish();
                }
            }
        }
        Poll::Ready(frame)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl<B: Body<Data = Bytes> + Unpin> Drop for Tap<B> {
    fn drop(&mut self) {
        // hyper drops a body without polling it to `None` once it reports `is_end_stream`, and
        // always for HEAD, 204, and 304 responses. By the `Body` contract such a body has no
        // frames left, so it is complete.
        if self.inner.is_end_stream()
            && let Some(capturing) = self.capturing.take()
        {
            capturing.finish();
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::{io, pin::pin, time::Duration};

    use futures::{StreamExt, poll, stream};
    use tokio::{sync::mpsc::error::TryRecvError, time::timeout};

    use super::*;

    /// Collects every save. Its sender drops with the last `Saver`.
    pub(in crate::record) struct Saves(mpsc::UnboundedSender<Exchange>);

    impl Recorder for Saves {
        fn save(&self, exchange: Exchange) {
            let _ = self.0.send(exchange);
        }
    }

    pub(in crate::record) fn saves() -> (Saves, mpsc::UnboundedReceiver<Exchange>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Saves(sender), receiver)
    }

    /// Every exchange saved, once the last `Saver` is gone.
    pub(in crate::record) fn saved(
        mut receiver: mpsc::UnboundedReceiver<Exchange>,
    ) -> Vec<Exchange> {
        let mut exchanges = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(exchange) => exchanges.push(exchange),
                Err(TryRecvError::Disconnected) => return exchanges,
                Err(TryRecvError::Empty) => panic!("a Saver is still alive"),
            }
        }
    }

    fn upstream(body: &'static str) -> reqwest::Response {
        http::Response::new(reqwest::Body::from(body)).into()
    }

    /// Runs one exchange whose request body a consumer reads until `None`, past any error,
    /// and whose empty response is dropped unread.
    async fn exchange_after_reading(request_body: axum::body::Body) -> Vec<Exchange> {
        let (recorder, receiver) = saves();
        let (saver, drained) = Saver::new(recorder);
        let (parts, ()) = http::Request::new(()).into_parts();
        let (capture, tap) = saver.capture(Id::parse(b"read").unwrap(), &parts, request_body);
        drop(saver);
        let mut frames = axum::body::Body::new(tap).into_data_stream();
        while frames.next().await.is_some() {}
        drop(capture.respond(upstream("")));
        timeout(Duration::from_secs(10), drained.wait())
            .await
            .expect("the exchange finishes");
        saved(receiver)
    }

    #[tokio::test]
    async fn a_body_read_to_its_end_hands_over_its_frames() {
        let body = stream::iter([Ok::<_, io::Error>(Bytes::from_static(b"a"))]);

        let exchanges = exchange_after_reading(axum::body::Body::from_stream(body)).await;

        assert_eq!(exchanges.len(), 1, "the exchange is saved");
        assert_eq!(exchanges[0].request.body[0].content, "a");
    }

    #[tokio::test]
    async fn a_body_that_fails_and_then_ends_hands_over_nothing() {
        let body = stream::iter([
            Ok(Bytes::from_static(b"a")),
            Err(io::Error::other("the client broke")),
        ]);

        let exchanges = exchange_after_reading(axum::body::Body::from_stream(body)).await;

        assert!(exchanges.is_empty(), "nothing is saved");
    }

    #[tokio::test]
    async fn drained_stays_pending_while_an_exchange_holds_a_saver() {
        let (recorder, _receiver) = saves();
        let (saver, drained) = Saver::new(recorder);
        let (parts, ()) = http::Request::new(()).into_parts();
        let (capture, request_tap) = saver.capture(
            Id::parse(b"wait").unwrap(),
            &parts,
            axum::body::Body::empty(),
        );
        let response = capture.respond(upstream("unread"));
        drop(saver);
        let mut wait = pin!(drained.wait());

        assert!(
            poll!(&mut wait).is_pending(),
            "the exchange's join task holds a Saver"
        );
        drop(request_tap);
        assert!(
            poll!(&mut wait).is_pending(),
            "the join task still waits for the response body"
        );
        drop(response);
        timeout(Duration::from_secs(10), wait)
            .await
            .expect("Drained resolves once the last Saver is gone");
    }
}
